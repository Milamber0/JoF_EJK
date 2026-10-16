//! Voxelised static world for probe global illumination: a fine occupancy bit grid for
//! visibility and a coarse material grid for bounce colour and emission. Built once at
//! map installation from the flattened world draws; never touched per frame.
use glam::Vec3;

/// Preferred fine (visibility) cell size in world units. JKA walls are 8..16 units thick,
/// so eight keeps thin walls opaque to probe rays; huge maps double it to stay in budget.
pub(crate) const FINE: f32 = 8.;
/// Occupancy budget in bits (16 MB); the cell size doubles until the map fits.
const MAX_FINE_BITS: usize = 128 * 1024 * 1024;
/// Coarse (material) cells per fine cell edge; 32-unit material voxels.
pub(crate) const COARSE_FACTOR: u32 = 4;

/// Occupancy bits and per-coarse-cell material ids, both indexed x-fastest.
pub(crate) struct VoxelWorld {
    pub(crate) origin: Vec3,
    /// Fine cell size actually used (`FINE` or a power-of-two multiple).
    pub(crate) fine_size: f32,
    pub(crate) fine: [u32; 3],
    pub(crate) coarse: [u32; 3],
    /// One bit per fine cell; set when any world triangle touches the cell.
    pub(crate) occupancy: Vec<u32>,
    /// Material index + 1 per coarse cell, zero for empty; emissive materials win.
    pub(crate) materials: Vec<u16>,
    pub(crate) triangles: usize,
}

/// Per-material bounce colour (linear) and emitted radiance (linear, q3map units / 1000).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Surface {
    pub(crate) albedo: [f32; 3],
    pub(crate) emission: [f32; 3],
    /// Sky shader: rays that reach it see the sky radiance, not a wall.
    pub(crate) sky: bool,
}

impl VoxelWorld {
    /// Voxelise the world triangles (`world` selects draws) inside `bounds`, padded by one
    /// fine cell so surfaces on the boundary are inside the grid.
    pub(crate) fn build<'a>(
        vertices: &[crate::GpuVertex],
        indices: &[u32],
        draws: impl Iterator<Item = (&'a std::ops::Range<u32>, u16, bool)>,
        bounds: [Vec3; 2],
    ) -> Self {
        Self::build_from(
            &|vertex| Vec3::from_array(vertices[vertex as usize].position),
            indices,
            draws,
            bounds,
        )
    }

    /// [`Self::build`] over positions from anywhere: a world that is not in the shared
    /// vertex buffer (an external scene) is voxelised from its own.
    pub(crate) fn build_from<'a>(
        position: &dyn Fn(u32) -> Vec3,
        indices: &[u32],
        draws: impl Iterator<Item = (&'a std::ops::Range<u32>, u16, bool)>,
        bounds: [Vec3; 2],
    ) -> Self {
        let mut size = FINE;
        let extent = bounds[1] - bounds[0];
        while (extent.x / size) * (extent.y / size) * (extent.z / size) > MAX_FINE_BITS as f32 {
            size *= 2.;
        }
        let origin = (bounds[0] / size).floor() * size - Vec3::splat(size);
        let extent = ((bounds[1] - origin) / size).ceil() + Vec3::splat(1.);
        let fine = [extent.x as u32, extent.y as u32, extent.z as u32]
            .map(|n| n.max(1).div_ceil(COARSE_FACTOR) * COARSE_FACTOR);
        let coarse = fine.map(|n| n / COARSE_FACTOR);
        let mut world = Self {
            origin,
            fine_size: size,
            fine,
            coarse,
            occupancy: vec![
                0;
                (fine[0] as usize * fine[1] as usize * fine[2] as usize).div_ceil(32)
            ],
            materials: vec![0; coarse[0] as usize * coarse[1] as usize * coarse[2] as usize],
            triangles: 0,
        };
        for (range, material, emissive) in draws {
            for triangle in indices[range.start as usize..range.end as usize].chunks_exact(3) {
                let corners = [0, 1, 2].map(|i| position(triangle[i]));
                world.rasterise(corners, material, emissive);
            }
        }
        world
    }

    fn rasterise(&mut self, corners: [Vec3; 3], material: u16, emissive: bool) {
        if corners.iter().any(|c| !c.is_finite()) {
            return;
        }
        self.triangles += 1;
        let size = self.fine_size;
        let lo = corners[0].min(corners[1]).min(corners[2]);
        let hi = corners[0].max(corners[1]).max(corners[2]);
        let cell = |p: Vec3| ((p - self.origin) / size).floor();
        let first = cell(lo).max(Vec3::ZERO);
        let last = cell(hi).min(
            Vec3::new(
                self.fine[0] as f32,
                self.fine[1] as f32,
                self.fine[2] as f32,
            ) - Vec3::ONE,
        );
        if first.cmpgt(last).any() {
            return;
        }
        let half = Vec3::splat(size * 0.5);
        // Plane slab first: a large triangle's bounding box is mostly far from its plane,
        // and this rejects those cells before the thirteen-axis test.
        let normal = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        let slab = half.x * normal.x.abs() + half.y * normal.y.abs() + half.z * normal.z.abs();
        for z in first.z as u32..=last.z as u32 {
            for y in first.y as u32..=last.y as u32 {
                for x in first.x as u32..=last.x as u32 {
                    let center =
                        self.origin + (Vec3::new(x as f32, y as f32, z as f32) + 0.5) * size;
                    if normal.dot(center - corners[0]).abs() > slab {
                        continue;
                    }
                    if !triangle_box_overlap(center, half, corners) {
                        continue;
                    }
                    let index = x as usize
                        + (y as usize + z as usize * self.fine[1] as usize) * self.fine[0] as usize;
                    self.occupancy[index / 32] |= 1 << (index % 32);
                    let c = [x, y, z].map(|v| (v / COARSE_FACTOR) as usize);
                    let slot = &mut self.materials
                        [c[0] + (c[1] + c[2] * self.coarse[1] as usize) * self.coarse[0] as usize];
                    if *slot == 0 || emissive {
                        *slot = material + 1;
                    }
                }
            }
        }
    }

    /// Whether the fine cell containing `point` is occupied; outside the grid is empty.
    pub(crate) fn occupied(&self, point: Vec3) -> bool {
        let c = (point - self.origin) / self.fine_size;
        if c.cmplt(Vec3::ZERO).any() {
            return false;
        }
        let (x, y, z) = (c.x as u32, c.y as u32, c.z as u32);
        if x >= self.fine[0] || y >= self.fine[1] || z >= self.fine[2] {
            return false;
        }
        let index =
            x as usize + (y as usize + z as usize * self.fine[1] as usize) * self.fine[0] as usize;
        self.occupancy[index / 32] & (1 << (index % 32)) != 0
    }

    /// Bytes of the two grids, for the installation log.
    pub(crate) fn bytes(&self) -> usize {
        self.occupancy.len() * 4 + self.materials.len() * 2
    }

    /// Fraction of fine cells that are occupied.
    pub(crate) fn occupied_fraction(&self) -> f64 {
        let set: u64 = self
            .occupancy
            .iter()
            .map(|w| u64::from(w.count_ones()))
            .sum();
        set as f64 / (self.fine[0] as f64 * self.fine[1] as f64 * self.fine[2] as f64)
    }
}

/// Separating-axis triangle/box overlap (Akenine-Möller), exact for conservative voxels.
pub(crate) fn triangle_box_overlap(center: Vec3, half: Vec3, corners: [Vec3; 3]) -> bool {
    let v = corners.map(|c| c - center);
    let edges = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    let axes = [Vec3::X, Vec3::Y, Vec3::Z];
    for edge in edges {
        for axis in axes {
            let a = axis.cross(edge);
            let r = half.x * a.x.abs() + half.y * a.y.abs() + half.z * a.z.abs();
            let p = v.map(|q| q.dot(a));
            let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
            if lo > r || hi < -r {
                return false;
            }
        }
    }
    for axis in 0..3 {
        let (lo, hi) = (
            v[0][axis].min(v[1][axis]).min(v[2][axis]),
            v[0][axis].max(v[1][axis]).max(v[2][axis]),
        );
        if lo > half[axis] || hi < -half[axis] {
            return false;
        }
    }
    let normal = edges[0].cross(edges[1]);
    let r = half.x * normal.x.abs() + half.y * normal.y.abs() + half.z * normal.z.abs();
    normal.dot(v[0]).abs() <= r
}

/// Mean colour of an image in linear light: bounce colour for a material's voxels.
pub(crate) fn mean_linear_color(image: &image::RgbaImage) -> [f32; 3] {
    // A lookup table for the transfer curve and one texel in sixteen: the mean of a
    // texture does not need every texel, and per-texel `powf` over every material's
    // images cost nine seconds of map load (retail ffa3).
    static LINEAR: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    let linear = LINEAR.get_or_init(|| std::array::from_fn(|v| (v as f32 / 255.).powf(2.2)));
    let (width, height) = image.dimensions();
    let step = if width >= 8 && height >= 8 { 4 } else { 1 };
    let mut sum = [0f64; 3];
    let mut weight = 0f64;
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let pixel = image.get_pixel(x, y);
            let alpha = f64::from(pixel[3]) / 255.;
            for c in 0..3 {
                sum[c] += f64::from(linear[pixel[c] as usize]) * alpha;
            }
            weight += alpha;
        }
    }
    if weight <= 0. {
        return [0.5; 3];
    }
    sum.map(|s| (s / weight) as f32)
}

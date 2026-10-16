//! Bounded CPU descriptions for GPU tessellation; no GPU state or gameplay consumer.
use super::*;

/// One cylinder, widened line strip or source polygon, with stable output offsets.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Description {
    /// Kind, output vertex offset, output index offset, accepted vertex count.
    pub(crate) meta: [u32; 4],
    /// Start and radius, or polygon-point offset.
    pub(crate) a: [f32; 4],
    /// End and radius.
    pub(crate) b: [f32; 4],
    /// Widening/ring direction and depth hack.
    pub(crate) right: [f32; 4],
    /// Cylinder second basis and segment count; zero for strips and source points.
    pub(crate) up: [f32; 4],
    /// Atlas rectangle.
    pub(crate) uv: [f32; 4],
    /// Atlas coordinate transform.
    pub(crate) transform: [f32; 4],
    /// Already sampled envelope and shader-layer colour.
    pub(crate) color: [f32; 4],
}

/// Clipped positions and texture coordinates, not reprojected or retraced on the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Point {
    /// World position; the padding lane preserves storage alignment.
    pub(crate) position: [f32; 4],
    /// Clipped local texture coordinates in the first two lanes.
    pub(crate) uv: [f32; 4],
}

/// Load-time allocated descriptions and points, reusing storage across frames.
pub(crate) struct Batch {
    /// Stable draw-order descriptions, not a sorted particle list.
    pub(crate) descriptions: Vec<Description>,
    /// Source points for clipped polygons and explicitly supplied quads.
    pub(crate) points: Vec<Point>,
    /// Number of output vertices reserved this frame.
    pub(crate) vertices: usize,
    /// Number of output indices reserved this frame.
    pub(crate) indices: usize,
}

impl Batch {
    /// Every description emits at least three vertices; capacities cannot grow in a frame.
    pub(crate) fn new() -> Self {
        Self {
            descriptions: Vec::with_capacity(MAX_VERTICES / 3),
            points: Vec::with_capacity(MAX_VERTICES),
            vertices: 0,
            indices: 0,
        }
    }

    /// Clear lengths without freeing or replacing backing storage.
    pub(crate) fn clear(&mut self) {
        self.descriptions.clear();
        self.points.clear();
        self.vertices = 0;
        self.indices = 0;
    }

    /// Reserve a previously capacity-checked primitive's disjoint output ranges.
    pub(crate) fn push(&mut self, mut description: Description, vertices: usize, indices: usize) {
        description.meta[1] = self.vertices as u32;
        description.meta[2] = self.indices as u32;
        description.meta[3] = vertices as u32;
        self.descriptions.push(description);
        self.vertices += vertices;
        self.indices += indices;
    }
}

impl Mesh {
    /// Reserve the CPU cylinder pool's exact accepted prefix, before GPU ring expansion.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn expanded_cylinder(
        &mut self,
        start: Vec3,
        end: Vec3,
        right: Vec3,
        up: Vec3,
        start_radius: f32,
        end_radius: f32,
        segments: usize,
        uv: [f32; 4],
        transform: [f32; 4],
        color: [f32; 4],
        depth: bool,
    ) -> bool {
        let Some(batch) = &mut self.expansion else {
            return false;
        };
        let accepted = segments
            .min((MAX_VERTICES - batch.vertices) / 4)
            .min((MAX_INDICES - batch.indices) / 6);
        self.dropped += segments - accepted;
        if accepted > 0 {
            batch.push(
                Description {
                    meta: [0, 0, 0, 0],
                    a: start.extend(start_radius).to_array(),
                    b: end.extend(end_radius).to_array(),
                    right: right.extend(f32::from(depth)).to_array(),
                    up: up.extend(segments as f32).to_array(),
                    uv,
                    transform,
                    color,
                },
                accepted * 4,
                accepted * 6,
            );
        }
        true
    }
    /// Logical output counts, independent of the tessellation backend.
    pub(crate) fn counts(&self) -> (usize, usize) {
        self.expansion
            .as_ref()
            .map_or((self.vertices.len(), self.indices.len()), |batch| {
                (batch.vertices, batch.indices)
            })
    }

    /// Enable only after every GPU resource has been constructed successfully.
    pub(crate) fn enable_expansion(&mut self) {
        self.expansion = Some(Batch::new());
    }

    /// CPU random centreline remains unchanged; GPU widens its four vertices.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn expanded_line(
        &mut self,
        start: Vec3,
        end: Vec3,
        right: Vec3,
        start_radius: f32,
        end_radius: f32,
        uv: [f32; 4],
        transform: [f32; 4],
        color: [f32; 4],
        depth: bool,
    ) -> bool {
        let Some(batch) = &mut self.expansion else {
            return false;
        };
        if batch.vertices + 4 > MAX_VERTICES || batch.indices + 6 > MAX_INDICES {
            self.dropped += 1;
            return true;
        }
        batch.push(
            Description {
                meta: [1, 0, 0, 0],
                a: start.extend(start_radius).to_array(),
                b: end.extend(end_radius).to_array(),
                right: right.extend(f32::from(depth)).to_array(),
                up: [0.0; 4],
                uv,
                transform,
                color,
            },
            4,
            6,
        );
        true
    }

    /// Record a clipped polygon without rebuilding per-vertex colour or fan indices.
    pub(super) fn expanded_polygon(
        &mut self,
        points: &[crate::decal_marks::DecalVertex],
        uv: [f32; 4],
        transform: [f32; 4],
        color: [f32; 4],
    ) -> bool {
        let Some(batch) = &mut self.expansion else {
            return false;
        };
        let indices = points.len().saturating_sub(2) * 3;
        if points.len() < 3
            || batch.vertices + points.len() > MAX_VERTICES
            || batch.indices + indices > MAX_INDICES
        {
            self.dropped += 1;
            return true;
        }
        let offset = batch.points.len();
        batch.points.extend(points.iter().map(|p| Point {
            position: [p.position[0], p.position[1], p.position[2], 0.0],
            uv: [p.st[0], p.st[1], 0.0, 0.0],
        }));
        batch.push(
            Description {
                meta: [2, 0, 0, 0],
                a: [offset as f32, 0.0, 0.0, 0.0],
                b: [0.0; 4],
                right: [0.0; 4],
                up: [0.0; 4],
                uv,
                transform,
                color,
            },
            points.len(),
            indices,
        );
        true
    }

    /// Expand shared attributes and indices for an explicitly CPU-authored quad.
    pub(super) fn expanded_quad(
        &mut self,
        points: [(Vec3, [f32; 2]); 4],
        uv: [f32; 4],
        transform: [f32; 4],
        color: [f32; 4],
        depth: bool,
    ) -> bool {
        let Some(batch) = &mut self.expansion else {
            return false;
        };
        if batch.vertices + 4 > MAX_VERTICES || batch.indices + 6 > MAX_INDICES {
            self.dropped += 1;
            return true;
        }
        let offset = batch.points.len();
        batch.points.extend(points.map(|(position, uv)| Point {
            position: position.extend(0.0).to_array(),
            uv: [uv[0], uv[1], 0.0, 0.0],
        }));
        batch.push(
            Description {
                meta: [3, 0, 0, 0],
                a: [offset as f32, 0.0, 0.0, 0.0],
                b: [0.0; 4],
                right: [0.0, 0.0, 0.0, f32::from(depth)],
                up: [0.0; 4],
                uv,
                transform,
                color,
            },
            4,
            6,
        );
        true
    }
}

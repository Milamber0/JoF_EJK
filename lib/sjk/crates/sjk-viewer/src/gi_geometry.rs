//! Map-lifetime triangle visibility for lighting. No raster resolution or map-size bias.
use glam::Vec3;

/// A static visibility triangle and its renderer material, independent of file format.
#[derive(Clone, Copy)]
pub(crate) struct Triangle {
    pub(crate) points: [Vec3; 3],
    pub(crate) material: u32,
    pub(crate) sky: bool,
}

/// Linear BVH with four-triangle leaves, packed for the portable GPU tracer.
pub(crate) struct Geometry {
    /// Two vec4 words per node: lower/first-child, upper/leaf-count.
    pub(crate) nodes: Vec<[u32; 4]>,
    /// Three vec4 words per triangle: origin/material, edge/sky, edge/padding.
    pub(crate) triangles: Vec<[u32; 4]>,
}

fn word(p: Vec3, tag: u32) -> [u32; 4] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits(), tag]
}

impl Geometry {
    /// Build a balanced median tree; every shader traversal fits its 32-entry stack.
    pub(crate) fn new(triangles: &[Triangle]) -> Self {
        let mut ordered: Vec<_> = triangles
            .iter()
            .copied()
            .filter(|t| {
                t.points.iter().all(|p| p.is_finite())
                    && (t.points[1] - t.points[0])
                        .cross(t.points[2] - t.points[0])
                        .length_squared()
                        > 1e-8
            })
            .collect();
        let mut nodes = vec![[0; 4]; 2];
        if !ordered.is_empty() {
            build(&mut ordered, 0, 0, &mut nodes);
        }
        let triangles = ordered
            .iter()
            .flat_map(|t| {
                [
                    word(t.points[0], t.material),
                    word(t.points[1] - t.points[0], u32::from(t.sky)),
                    word(t.points[2] - t.points[0], 0),
                ]
            })
            .collect();
        Self { nodes, triangles }
    }
}

impl Geometry {
    /// Whether a solid (non-sky) triangle crosses the segment from `from` to `to`, as
    /// the GPU tracer traverses the tree (`gi_trace_through` with `through_sky`).
    pub(crate) fn blocked(&self, from: Vec3, to: Vec3) -> bool {
        let unpack = |w: [u32; 4]| {
            Vec3::new(
                f32::from_bits(w[0]),
                f32::from_bits(w[1]),
                f32::from_bits(w[2]),
            )
        };
        let delta = to - from;
        let length = delta.length();
        if length < 1e-3 || self.triangles.is_empty() {
            return false;
        }
        let direction = delta / length;
        let mut stack = [0usize; 64];
        let mut count = 1;
        while count > 0 {
            count -= 1;
            let node = stack[count] * 2;
            let (lo, hi) = (self.nodes[node], self.nodes[node + 1]);
            if !segment_box(from, direction, length, unpack(lo), unpack(hi)) {
                continue;
            }
            if hi[3] == 0 {
                if count + 2 > stack.len() {
                    return true;
                }
                stack[count] = lo[3] as usize;
                stack[count + 1] = lo[3] as usize + 1;
                count += 2;
                continue;
            }
            for i in 0..hi[3] as usize {
                let t = (lo[3] as usize + i) * 3;
                let (a, b, c) = (
                    self.triangles[t],
                    self.triangles[t + 1],
                    self.triangles[t + 2],
                );
                if b[3] != 0 {
                    continue;
                }
                let (e1, e2) = (unpack(b), unpack(c));
                let h = direction.cross(e2);
                let determinant = e1.dot(h);
                if determinant.abs() < 1e-8 {
                    continue;
                }
                let s = from - unpack(a);
                let u = s.dot(h) / determinant;
                let q = s.cross(e1);
                let v = direction.dot(q) / determinant;
                let distance = e2.dot(q) / determinant;
                if u >= -1e-6
                    && v >= -1e-6
                    && u + v <= 1.000001
                    && distance > 1e-3
                    && distance < length
                {
                    return true;
                }
            }
        }
        false
    }
}

fn segment_box(origin: Vec3, direction: Vec3, length: f32, lo: Vec3, hi: Vec3) -> bool {
    let (mut near, mut far) = (0f32, length);
    for axis in 0..3 {
        if direction[axis].abs() < 1e-8 {
            if origin[axis] < lo[axis] || origin[axis] > hi[axis] {
                return false;
            }
        } else {
            let a = (lo[axis] - origin[axis]) / direction[axis];
            let b = (hi[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
        }
    }
    near <= far
}

fn build(triangles: &mut [Triangle], offset: usize, node: usize, nodes: &mut Vec<[u32; 4]>) {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for t in triangles.iter() {
        for &p in &t.points {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if triangles.len() <= 4 {
        nodes[node * 2] = word(lo, offset as u32);
        nodes[node * 2 + 1] = word(hi, triangles.len() as u32);
        return;
    }
    let extent = hi - lo;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    let middle = triangles.len() / 2;
    triangles.select_nth_unstable_by(middle, |a, b| {
        let centroid = |t: &Triangle| t.points.iter().map(|p| p[axis]).sum::<f32>();
        centroid(a).total_cmp(&centroid(b))
    });
    let first = nodes.len() / 2;
    nodes.extend([[0; 4]; 4]);
    nodes[node * 2] = word(lo, first as u32);
    nodes[node * 2 + 1] = word(hi, 0);
    let (left, right) = triangles.split_at_mut(middle);
    build(left, offset, first, nodes);
    build(right, offset + middle, first + 1, nodes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wall_blocks_the_segments_crossing_it_and_sky_does_not() {
        let wall = |x: f32, sky: bool| {
            [
                Triangle {
                    points: [
                        Vec3::new(x, -100., -100.),
                        Vec3::new(x, 100., -100.),
                        Vec3::new(x, 100., 100.),
                    ],
                    material: 0,
                    sky,
                },
                Triangle {
                    points: [
                        Vec3::new(x, -100., -100.),
                        Vec3::new(x, 100., 100.),
                        Vec3::new(x, -100., 100.),
                    ],
                    material: 0,
                    sky,
                },
            ]
        };
        let mut triangles: Vec<Triangle> = (0..20)
            .flat_map(|i| wall(1000. + i as f32 * 10., false))
            .collect();
        triangles.extend(wall(50., false));
        triangles.extend(wall(-50., true));
        let geometry = Geometry::new(&triangles);
        assert!(geometry.blocked(Vec3::ZERO, Vec3::new(60., 0., 0.)));
        assert!(!geometry.blocked(Vec3::ZERO, Vec3::new(40., 0., 0.)));
        assert!(
            !geometry.blocked(Vec3::ZERO, Vec3::new(-60., 0., 0.)),
            "sky is no wall"
        );
        assert!(
            !geometry.blocked(Vec3::ZERO, Vec3::new(60., 150., 0.)),
            "around the wall"
        );
    }
}

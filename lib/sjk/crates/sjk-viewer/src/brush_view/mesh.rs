//! The invisible faces of a map, rebuilt from its brushes' planes: q3map2 keeps
//! every brush for collision but makes no drawn surface for a tool texture.

use super::kinds::Kind;
use crate::world_materials::mover_occlusion::reach;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use sjk_bsp::{Bsp, Model};
use std::ops::Range;

/// Vertex shared by faces and edges; layout of `brush_view.wgsl`'s `VertexIn`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct Vertex {
    pub(super) position: [f32; 3],
    /// The face's plane normal, pointing out of its brush.
    pub(super) normal: [f32; 3],
    pub(super) kind: u32,
}

/// A face smaller than this (square units) is a bevel or sliver; it is skipped.
const MIN_AREA: f32 = 0.5;

/// Every invisible face as triangles and outline segments, grouped by kind.
#[derive(Default)]
pub(super) struct Mesh {
    pub(super) faces: Vec<Vertex>,
    pub(super) face_ranges: Vec<Range<u32>>,
    pub(super) edges: Vec<Vertex>,
    pub(super) edge_ranges: Vec<Range<u32>>,
}

impl Mesh {
    /// Build from the world's brushes and those of `trigger_*` entities and of
    /// movers (doors, lifts, buttons, trains…), the movers where the map file
    /// puts them, plus the box a door, lift or button moves through. Other
    /// entities' brushes are left out: the map draws them.
    pub(super) fn build(bsp: &Bsp) -> Self {
        let mut faces: Vec<Vec<Vertex>> = vec![Vec::new(); Kind::ALL.len()];
        let mut edges: Vec<Vec<Vertex>> = vec![Vec::new(); Kind::ALL.len()];
        let models = bsp.render().models();
        let placed = placed_models(bsp, models);
        for (model_index, model) in models.iter().enumerate() {
            let (trigger, origin) = if model_index == 0 {
                (None, [0.0; 3])
            } else if let Some(placed) = &placed[model_index] {
                (Some(placed.kind), placed.origin)
            } else {
                continue;
            };
            if let Some((lower, upper)) = placed.get(model_index).and_then(|p| p.as_ref()?.travel) {
                push_box(&mut faces, &mut edges, lower, upper);
            }
            for brush_index in model.brushes.clone() {
                let Some(brush) = bsp.brushes().get(brush_index) else {
                    continue;
                };
                for side_index in brush.sides.clone() {
                    let Some(side) = bsp.brush_sides().get(side_index) else {
                        continue;
                    };
                    let Some(kind) = trigger.or_else(|| {
                        let shader = bsp.shaders().get(side.shader)?;
                        Kind::of_face(brush.content_flags, shader.surface_flags, shader.name())
                    }) else {
                        continue;
                    };
                    let mut points = bsp.brush_side_polygon(brush_index, side_index);
                    for point in &mut points {
                        for axis in 0..3 {
                            point[axis] += origin[axis];
                        }
                    }
                    if area(&points) < MIN_AREA {
                        continue;
                    }
                    let normal = bsp.planes()[side.plane].normal;
                    let vertex = |position| Vertex {
                        position,
                        normal,
                        kind: kind.index() as u32,
                    };
                    let bucket = &mut faces[kind.index()];
                    for i in 2..points.len() {
                        bucket.extend([points[0], points[i - 1], points[i]].map(vertex));
                    }
                    let bucket = &mut edges[kind.index()];
                    for (i, &point) in points.iter().enumerate() {
                        bucket.extend([point, points[(i + 1) % points.len()]].map(vertex));
                    }
                }
            }
        }
        let (faces, face_ranges) = concatenate(faces);
        let (edges, edge_ranges) = concatenate(edges);
        Self {
            faces,
            face_ranges,
            edges,
            edge_ranges,
        }
    }
}

/// Where an entity's inline model is drawn and what it is.
struct Placed {
    kind: Kind,
    /// The entity's `origin` key: a brush model with an origin brush is stored
    /// relative to it (q3map2 moves it), so its faces are shifted back.
    origin: [f32; 3],
    /// The world box a door, lift or button moves through, when it moves.
    travel: Option<(Vec3, Vec3)>,
}

/// The `trigger_*` entities and movers each inline model belongs to.
fn placed_models(bsp: &Bsp, models: &[Model]) -> Vec<Option<Placed>> {
    let mut placed: Vec<Option<Placed>> = (0..models.len()).map(|_| None).collect();
    let entities = sjk_entity::parse_entity_lump(bsp.entities()).unwrap_or_default();
    for entity in &entities {
        let Some(classname) = entity.classname() else {
            continue;
        };
        let kind = if classname.len() > 8 && classname[..8].eq_ignore_ascii_case("trigger_") {
            Kind::of_trigger(classname)
        } else if let Some(kind) = Kind::of_mover(classname) {
            kind
        } else {
            continue;
        };
        let Some(index) = entity
            .get("model")
            .and_then(|model| model.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok())
            .filter(|&index| index > 0 && index < models.len())
        else {
            continue;
        };
        let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
        let model = &models[index];
        let (lower, upper) = (
            Vec3::from_array(model.minimums) + Vec3::from_array(origin),
            Vec3::from_array(model.maximums) + Vec3::from_array(origin),
        );
        // The box the mover can reach, only when it is more than where it rests.
        let travel = matches!(kind, Kind::Door | Kind::Lift | Kind::Button)
            .then(|| {
                reach(
                    Some(entity),
                    Vec3::from_array(model.minimums),
                    Vec3::from_array(model.maximums),
                )
            })
            .filter(|(low, high)| {
                (*low - lower).abs().max_element() > 1.0
                    || (*high - upper).abs().max_element() > 1.0
            });
        placed[index] = Some(Placed {
            kind,
            origin,
            travel,
        });
    }
    placed
}

/// A box as the `Travel` kind's faces and outline.
fn push_box(faces: &mut [Vec<Vertex>], edges: &mut [Vec<Vertex>], lower: Vec3, upper: Vec3) {
    let kind = Kind::Travel.index() as u32;
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { lower.x } else { upper.x },
            if i & 2 == 0 { lower.y } else { upper.y },
            if i & 4 == 0 { lower.z } else { upper.z },
        )
    };
    // Per axis: the face at the low and at the high end, as its four corners
    // in order around it.
    const SIDES: [([usize; 4], [f32; 3]); 6] = [
        ([0, 2, 6, 4], [-1.0, 0.0, 0.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
        ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
        ([0, 1, 3, 2], [0.0, 0.0, -1.0]),
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
    ];
    for (ring, normal) in SIDES {
        let vertex = |i: usize| Vertex {
            position: corner(ring[i]).to_array(),
            normal,
            kind,
        };
        faces[Kind::Travel.index()].extend([0, 1, 2, 0, 2, 3].map(vertex));
        for i in 0..4 {
            edges[Kind::Travel.index()].extend([vertex(i), vertex((i + 1) % 4)]);
        }
    }
}

fn concatenate(buckets: Vec<Vec<Vertex>>) -> (Vec<Vertex>, Vec<Range<u32>>) {
    let mut all = Vec::with_capacity(buckets.iter().map(Vec::len).sum());
    let mut ranges = Vec::with_capacity(buckets.len());
    for bucket in buckets {
        let start = all.len() as u32;
        all.extend(bucket);
        ranges.push(start..all.len() as u32);
    }
    (all, ranges)
}

fn area(points: &[[f32; 3]]) -> f32 {
    let mut sum = [0.0f32; 3];
    for i in 2..points.len() {
        let a = sub(points[i - 1], points[0]);
        let b = sub(points[i], points[0]);
        let c = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        for axis in 0..3 {
            sum[axis] += c[axis];
        }
    }
    0.5 * (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_of_a_square() {
        let square = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        assert!((area(&square) - 4.0).abs() < 1e-5);
        assert_eq!(area(&square[..2]), 0.0);
    }

    #[test]
    fn buckets_become_ranges_in_kind_order() {
        let vertex = |kind| Vertex {
            position: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            kind,
        };
        let (all, ranges) = concatenate(vec![vec![vertex(0); 3], vec![], vec![vertex(2); 6]]);
        assert_eq!(all.len(), 9);
        assert_eq!(ranges, [0..3, 3..3, 3..9]);
    }
}

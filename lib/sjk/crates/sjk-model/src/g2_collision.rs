//! Ghoul2's ray-against-model test, `G2API_CollisionDetect` (OpenJK
//! `codemp/rd-dedicated/G2_API.cpp` and `G2_misc.cpp`): what a server asks when it wants
//! to know whether a trace touched a posed model, and where.
//!
//! The ray is brought into model space by the inverse of the entity's world matrix; each
//! model of the instance is skinned at the trace LOD; its surfaces are walked from the
//! root, depth first, leaving out any whose flags (or an override) switch it off and
//! stopping below one marked no-descendants; every triangle hit fills the next of sixteen
//! records in the order found, which are then sorted by distance (`qsort` with
//! `QsortDistance`, `G2_API.cpp:1951`) — callers such as a saber's damage take the
//! first. A point trace (radius under 0.1) uses the segment test; any other
//! radius the cylinder test Ghoul2 shares with its gore marks.
//!
//! Format behaviour only: which models an entity has, their poses and the radius are the
//! caller's.

use crate::posed_trace::triangle;
use crate::{Glm, ModelError};

/// `MAX_G2_COLLISIONS`: records a call can fill.
pub const MAX_COLLISIONS: usize = 16;
/// `G2SURFACEFLAG_NODESCENDANTS`: nothing below this surface is traced.
const NO_DESCENDANTS: u32 = 0x100;

/// One model of the instance, posed.
#[derive(Clone, Copy)]
pub struct PosedModel<'a> {
    /// The mesh.
    pub glm: &'a Glm,
    /// Its bones' skinning matrices, as `sjk-model` evaluates them.
    pub bones: &'a [[[f32; 4]; 3]],
    /// Applied to each skinned vertex, completing it to Ghoul2's model space: the body's
    /// quarter-turn root, or for a model bolted to another the parent's raw bolt.
    pub placement: [[f32; 4]; 3],
    /// Surfaces whose flags the game overrode (`G2API_SetSurfaceOnOff`), by index.
    pub overrides: &'a [(usize, u32)],
    /// The entity's `modelScale` as Ghoul2 corrects it (a zero axis is 1), multiplying
    /// every model-space vertex (`R_TransformEachSurface`); `[1.0; 3]` for none.
    pub scale: [f32; 3],
}

/// One `CollisionRecord_t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionRecord {
    /// Which model of the instance.
    pub model: usize,
    /// The surface's index in the model.
    pub surface: usize,
    /// The triangle's index in that surface.
    pub poly: usize,
    /// `G2_FRONTFACE` (1) or `G2_BACKFACE` (0); a radius hit is always front.
    pub front_face: bool,
    /// From the ray's start to the hit, in model space.
    pub distance: f32,
    /// The hit, in the world.
    pub position: [f32; 3],
    /// The triangle's normal, in the world, normalized.
    pub normal: [f32; 3],
}

/// Reused storage for one call's skinned vertices, so a call allocates nothing once warm.
#[derive(Default)]
pub struct CollisionScratch {
    vertices: Vec<[f32; 3]>,
    flags: Vec<u8>,
}

/// `G2API_CollisionDetect`: every record, in the order Ghoul2 fills them, up to
/// [`MAX_COLLISIONS`], appended to `records` (which is cleared first). `axes` is the
/// entity's `AnglesToAxis` (forward, left, up), `origin` where it stands.
#[allow(clippy::too_many_arguments)]
pub fn collision_detect(
    models: &[PosedModel<'_>],
    origin: [f32; 3],
    ray_start: [f32; 3],
    ray_end: [f32; 3],
    lod: usize,
    radius: f32,
    axes: [[f32; 3]; 3],
    scratch: &mut CollisionScratch,
    records: &mut Vec<CollisionRecord>,
) -> Result<(), ModelError> {
    records.clear();
    // `G2_GenerateWorldMatrix`: `Create_Matrix` (the axes as columns) and the origin.
    let world: [[f32; 4]; 3] =
        std::array::from_fn(|row| [axes[0][row], axes[1][row], axes[2][row], origin[row]]);
    let inverse = inverse(&world);
    let start = transform_and_translate(ray_start, &inverse);
    let end = transform_and_translate(ray_end, &inverse);
    let mut trace = Trace {
        start,
        end,
        radius,
        world: &world,
        records,
        full: false,
    };
    for (index, model) in models.iter().enumerate() {
        // `G2_DecideTraceLod`: the one asked for, or the model's last.
        let lod = lod.min(model.glm.lods.len().saturating_sub(1));
        trace.surfaces(model, index, 0, lod, scratch)?;
        if trace.full {
            break;
        }
    }
    sort_by_distance(records);
    Ok(())
}

/// `qsort(collRecMap, n, …, QsortDistance)`. The comparator never calls two records
/// equal (`a < b ? -1 : 1`), so for a tie the order is the sort's own: this is glibc's
/// merge sort (`msort_with_tmp`), which on a tie takes the right half's record first.
fn sort_by_distance(records: &mut [CollisionRecord]) {
    if records.len() <= 1 {
        return;
    }
    let (left_len, right_len) = (records.len() / 2, records.len() - records.len() / 2);
    sort_by_distance(&mut records[..left_len]);
    sort_by_distance(&mut records[left_len..]);
    let mut merged = [records[0]; MAX_COLLISIONS];
    let (mut left, mut right, mut out) = (0, left_len, 0);
    while left < left_len && right < left_len + right_len {
        // `cmp(b1, b2) <= 0` takes the left one; the comparator answers 1 on a tie.
        if records[left].distance < records[right].distance {
            merged[out] = records[left];
            left += 1;
        } else {
            merged[out] = records[right];
            right += 1;
        }
        out += 1;
    }
    for record in records[left..left_len].iter().chain(&records[right..]) {
        merged[out] = *record;
        out += 1;
    }
    records.copy_from_slice(&merged[..records.len()]);
}

struct Trace<'a> {
    start: [f32; 3],
    end: [f32; 3],
    radius: f32,
    world: &'a [[f32; 4]; 3],
    records: &'a mut Vec<CollisionRecord>,
    /// `TS.hitOne`: the records ran out, so nothing more is traced.
    full: bool,
}

impl Trace<'_> {
    /// `G2_TraceSurfaces`.
    fn surfaces(
        &mut self,
        model: &PosedModel<'_>,
        model_index: usize,
        surface: usize,
        lod: usize,
        scratch: &mut CollisionScratch,
    ) -> Result<(), ModelError> {
        if self.full {
            return Ok(());
        }
        let hierarchy = model
            .glm
            .hierarchy
            .get(surface)
            .ok_or_else(|| ModelError::invalid(surface, "surface is outside the hierarchy"))?;
        let flags = model
            .overrides
            .iter()
            .find(|(index, _)| *index == surface)
            .map_or(hierarchy.flags, |(_, flags)| *flags);
        if flags == 0 {
            self.polys(model, model_index, surface, lod, scratch)?;
        }
        if flags & NO_DESCENDANTS != 0 {
            return Ok(());
        }
        for &child in &hierarchy.children {
            if self.full {
                break;
            }
            self.surfaces(model, model_index, child, lod, scratch)?;
        }
        Ok(())
    }

    /// Skin one surface at the LOD (`R_TransformEachSurface`), then test its triangles.
    fn polys(
        &mut self,
        model: &PosedModel<'_>,
        model_index: usize,
        surface: usize,
        lod: usize,
        scratch: &mut CollisionScratch,
    ) -> Result<(), ModelError> {
        let source = model
            .glm
            .lods
            .get(lod)
            .and_then(|lod| lod.surfaces.get(surface))
            .ok_or_else(|| ModelError::invalid(surface, "surface is absent from the LOD"))?;
        scratch.vertices.clear();
        for vertex in &source.vertices {
            let skinned = crate::reskin::vertex(source, vertex, model.bones).position;
            let placed = transform_and_translate(skinned, &model.placement);
            scratch
                .vertices
                .push(std::array::from_fn(|axis| placed[axis] * model.scale[axis]));
        }
        let vertex = |index: u32| {
            scratch
                .vertices
                .get(index as usize)
                .copied()
                .ok_or_else(|| ModelError::invalid(surface, "triangle index is out of range"))
        };
        if self.radius.abs() < 0.1 {
            for (poly, indices) in source.triangles.iter().enumerate() {
                let corners = [
                    vertex(indices[0])?,
                    vertex(indices[1])?,
                    vertex(indices[2])?,
                ];
                let Some(hit) = triangle(self.start, self.end, corners, true, true) else {
                    continue;
                };
                let distance = length(sub(hit.position, self.start));
                if !self.record(
                    model_index,
                    surface,
                    poly,
                    hit.facing > 0.0,
                    distance,
                    hit.position,
                    hit.normal,
                ) {
                    return Ok(());
                }
            }
            return Ok(());
        }
        self.radius_polys(model_index, surface, source.triangles.as_slice(), scratch)
    }

    /// `G2_RadiusTracePolys`: a triangle is hit unless all three corners lie on the same
    /// outside of the cylinder around the ray — beyond either end, or off either side.
    fn radius_polys(
        &mut self,
        model_index: usize,
        surface: usize,
        triangles: &[[u32; 3]],
        scratch: &mut CollisionScratch,
    ) -> Result<(), ModelError> {
        let mut direction = sub(self.end, self.start);
        let mut basis2 = [0.0, 0.0, 1.0];
        let mut basis1 = cross(direction, basis2);
        if dot(basis1, basis1) < 0.1 {
            basis2 = [0.0, 1.0, 0.0];
            basis1 = cross(direction, basis2);
        }
        basis2 = cross(direction, basis1);
        normalize(&mut basis1);
        normalize(&mut basis2);
        // `cos(0)` and `sin(0)`: the gore code's rotation, here none.
        let (c, s) = (1.0_f32, 0.0_f32);
        let t_axis = add(
            scale(basis1, 0.5 * c / self.radius),
            scale(basis2, 0.5 * s / self.radius),
        );
        let s_axis = add(
            scale(basis1, -0.5 * s / self.radius),
            scale(basis2, 0.5 * c / self.radius),
        );
        let length_squared = dot(direction, direction);
        for axis in &mut direction {
            *axis /= length_squared;
        }
        scratch.flags.clear();
        let mut all = 63_u8;
        for vertex in &scratch.vertices {
            let delta = sub(*vertex, self.start);
            let s = dot(delta, s_axis) + 0.5;
            let t = dot(delta, t_axis) + 0.5;
            let u = dot(delta, direction);
            let mut flags = 0_u8;
            if s > 0.0 {
                flags |= 1
            }
            if s < 1.0 {
                flags |= 2
            }
            if t > 0.0 {
                flags |= 4
            }
            if t < 1.0 {
                flags |= 8
            }
            if u > 0.0 {
                flags |= 16
            }
            if u < 1.0 {
                flags |= 32
            }
            let outside = !flags;
            all &= outside;
            scratch.flags.push(outside);
        }
        if all & 63 != 0 {
            return Ok(());
        }
        let ray = sub(self.end, self.start);
        for (poly, indices) in triangles.iter().enumerate() {
            let flag = |index: u32| scratch.flags.get(index as usize).copied().unwrap_or(0);
            if 63 & flag(indices[0]) & flag(indices[1]) & flag(indices[2]) != 0 {
                continue;
            }
            let [a, b, c] =
                [indices[0], indices[1], indices[2]].map(|index| scratch.vertices[index as usize]);
            let normal = cross(sub(b, a), sub(c, a));
            // "Let's work out the impact point on the triangle."
            let third = -(a[0] * (b[1] * c[2] - c[1] * b[2])
                + b[0] * (c[1] * a[2] - a[1] * c[2])
                + c[0] * (a[1] * b[2] - b[1] * a[2]));
            let side = normal[0] * self.start[0]
                + normal[1] * self.start[1]
                + normal[2] * self.start[2]
                + third;
            let side2 = normal[0] * ray[0] + normal[1] * ray[1] + normal[2] * ray[2];
            let distance_along = side / side2;
            let hit = add(self.start, scale(ray, -distance_along));
            let distance = length(sub(hit, self.start));
            if !self.record(model_index, surface, poly, true, distance, hit, normal) {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Fill the next record; `false` once there is no room, which stops the trace.
    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        model: usize,
        surface: usize,
        poly: usize,
        front_face: bool,
        distance: f32,
        hit: [f32; 3],
        normal: [f32; 3],
    ) -> bool {
        if self.records.len() == MAX_COLLISIONS {
            self.full = true;
            return false;
        }
        let mut normal = transform(normal, self.world);
        normalize(&mut normal);
        self.records.push(CollisionRecord {
            model,
            surface,
            poly,
            front_face,
            distance,
            position: transform_and_translate(hit, self.world),
            normal,
        });
        true
    }
}

/// `Inverse_Matrix`: the transposed rotation and the translation brought back through it.
fn inverse(matrix: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let mut out = [[0.0; 4]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = matrix[j][i];
        }
    }
    for i in 0..3 {
        out[i][3] = 0.0;
        for j in 0..3 {
            out[i][3] -= out[i][j] * matrix[j][3];
        }
    }
    out
}

/// `TransformAndTranslatePoint`.
fn transform_and_translate(point: [f32; 3], matrix: &[[f32; 4]; 3]) -> [f32; 3] {
    std::array::from_fn(|i| {
        point[0] * matrix[i][0] + point[1] * matrix[i][1] + point[2] * matrix[i][2] + matrix[i][3]
    })
}

/// `TransformPoint`: the rotation only.
fn transform(point: [f32; 3], matrix: &[[f32; 4]; 3]) -> [f32; 3] {
    std::array::from_fn(|i| {
        point[0] * matrix[i][0] + point[1] * matrix[i][1] + point[2] * matrix[i][2]
    })
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f32; 3], by: f32) -> [f32; 3] {
    [a[0] * by, a[1] * by, a[2] * by]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// `VectorNormalize`.
fn normalize(a: &mut [f32; 3]) {
    let length = length(*a);
    if length != 0.0 {
        let inverse = 1.0 / length;
        for value in a {
            *value *= inverse;
        }
    }
}

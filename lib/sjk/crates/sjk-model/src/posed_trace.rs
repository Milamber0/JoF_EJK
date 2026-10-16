//! Allocation-free segment queries over already-skinned, model-space triangles.
//!
//! This module neither evaluates a pose nor chooses visibility, LOD, or entity policy.
//! Callers supply the same surfaces they render and transform the segment into their space.

use crate::SkinnedSurface;

/// A segment/triangle intersection in the caller's coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriangleHit {
    /// Segment parameter, inclusive of both endpoints.
    pub fraction: f32,
    /// Intersection position.
    pub position: [f32; 3],
    /// Unnormalized winding normal; its length is twice the triangle area.
    pub normal: [f32; 3],
    /// Dot product of segment displacement with the winding normal.
    pub facing: f32,
}

/// Closest intersection, with indices into the supplied posed surface collection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    /// Index in the supplied collection, not a format-specific hierarchy index.
    pub surface: usize,
    /// Triangle index within that surface.
    pub triangle: usize,
    /// Model-space intersection.
    pub hit: TriangleHit,
}

/// Invalid caller geometry; no partial hit is returned for a corrupt surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceError {
    /// A triangle references a vertex outside its surface.
    InvalidIndex,
    /// A position or segment endpoint is non-finite.
    NonFinite,
}

/// Intersect a finite segment with a triangle, including edge and endpoint contacts.
///
/// The arithmetic/order matches codemp `G2_SegmentTriangleTest` (G2_misc.cpp:692-766).
/// Positive facing is rejected when `back_faces` is false, negative when `front_faces`
/// is false. Parallel, coplanar, degenerate and non-finite inputs produce no hit.
pub fn triangle(
    start: [f32; 3],
    end: [f32; 3],
    vertices: [[f32; 3]; 3],
    back_faces: bool,
    front_faces: bool,
) -> Option<TriangleHit> {
    if !start
        .into_iter()
        .chain(end)
        .chain(vertices.into_iter().flatten())
        .all(f32::is_finite)
    {
        return None;
    }
    let [a, b, c] = vertices;
    let normal = cross(sub(b, a), sub(c, a));
    let ray = sub(end, start);
    let facing = dot(ray, normal);
    if !facing.is_finite()
        || facing.abs() < 1e-10
        || (!back_faces && facing > 0.0)
        || (!front_faces && facing < 0.0)
    {
        return None;
    }
    let fraction = dot(sub(a, start), normal) / facing;
    if !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    let position = std::array::from_fn(|i| ray[i] * fraction + start[i]);
    let pa = sub(a, position);
    let pb = sub(b, position);
    let pc = sub(c, position);
    if dot(cross(pa, pb), normal) < 0.0
        || dot(cross(pc, pa), normal) < 0.0
        || dot(cross(pb, pc), normal) < 0.0
    {
        return None;
    }
    Some(TriangleHit {
        fraction,
        position,
        normal,
        facing,
    })
}

/// Find the nearest two-sided triangle in an already-posed surface slice.
///
/// Borrows the render skinning result directly: no pose evaluation, copying, allocation,
/// or locks. Visibility filtering is the caller's responsibility, including skin-hidden
/// surfaces. Equal-distance hits keep source order. This is a surface query, not a solid
/// volume query: there is no `startsolid`, radius, box sweep, or bone attribution.
pub fn surfaces(
    surfaces: &[SkinnedSurface],
    start: [f32; 3],
    end: [f32; 3],
) -> Result<Option<SurfaceHit>, TraceError> {
    if !start.into_iter().chain(end).all(f32::is_finite) {
        return Err(TraceError::NonFinite);
    }
    let mut closest: Option<SurfaceHit> = None;
    for (surface, mesh) in surfaces.iter().enumerate() {
        for (index, indices) in mesh.triangles.iter().enumerate() {
            let mut vertices = [[0.0; 3]; 3];
            for (slot, vertex) in vertices.iter_mut().zip(indices) {
                *slot = mesh
                    .vertices
                    .get(*vertex as usize)
                    .ok_or(TraceError::InvalidIndex)?
                    .position;
                if !slot.iter().all(|v| v.is_finite()) {
                    return Err(TraceError::NonFinite);
                }
            }
            if let Some(hit) = triangle(start, end, vertices, true, true) {
                if closest.is_none_or(|old| hit.fraction < old.hit.fraction) {
                    closest = Some(SurfaceHit {
                        surface,
                        triangle: index,
                        hit,
                    });
                }
            }
        }
    }
    Ok(closest)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
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

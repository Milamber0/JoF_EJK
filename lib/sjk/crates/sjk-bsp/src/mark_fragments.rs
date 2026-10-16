//! Polygon projection onto world geometry ("mark fragments").
//!
//! Mirrors rd-vanilla `tr_marks.cpp`: a polygon plus a projection vector
//! define a prism of clipping planes (`R_MarkFragments`, lines 280-310); the
//! world surfaces whose bounds touch that prism are gathered by walking the
//! BSP (`R_BoxSurfaces_r`, lines 135-189); every candidate triangle is chopped
//! against each plane (`R_ChopPolyBehindPlane`, lines 42-127) and the
//! survivors are appended to fixed caller-owned buffers
//! (`R_AddMarkFragments`, lines 197-248).
//!
//! The crate supplies the geometry side only: which surfaces are eligible is
//! decided by the caller's `accept` predicate (surface flags such as
//! "no marks" are game data), and the caller feeds the triangles because
//! tessellated patch geometry lives with the renderer's scene build.

use super::{Bsp, NodeChild, Plane, Surface, SurfaceKind, dot};

/// Largest polygon the clipper accepts (`MAX_VERTS_ON_POLY`).
pub const MAX_VERTICES_ON_POLY: usize = 64;
const MAX_PLANES: usize = MAX_VERTICES_ON_POLY + 2;
/// Clip epsilon used for every plane (`R_AddMarkFragments`, line 213).
const CLIP_EPSILON: f32 = 0.5;
/// Extra reach behind the polygon for leaf gathering and the far plane
/// (`R_MarkFragments`, lines 290 and 306-309).
const REACH_BEHIND: f32 = 20.0;
const NEAR_PLANE_OFFSET: f32 = 32.0;
/// Faces whose normal makes a sharp angle with the projection are skipped
/// (`R_BoxSurfaces_r`, line 172).
const FACE_FACING_LIMIT: f32 = -0.5;

/// One clipped polygon inside a [`MarkFragments`] buffer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkFragment {
    /// Index of the fragment's first point in [`MarkFragments::points`].
    pub first_point: usize,
    /// Number of consecutive points that form the fragment.
    pub point_count: usize,
}

/// Clipping prism built from the projected polygon.
#[derive(Clone, Debug)]
pub struct MarkProjection {
    normals: [[f32; 3]; MAX_PLANES],
    distances: [f32; MAX_PLANES],
    plane_count: usize,
    direction: [f32; 3],
    minimums: [f32; 3],
    maximums: [f32; 3],
}

impl MarkProjection {
    /// Build the side, near and far planes for `points` swept along
    /// `projection` (`R_MarkFragments`, lines 280-310). More than
    /// [`MAX_VERTICES_ON_POLY`] points are ignored, as in the reference.
    ///
    /// The side planes face inwards only for the reference winding: seen
    /// from the side the projection starts on, the points run clockwise
    /// (`RE_AddDecalToScene` builds `-a1-a2, +a1-a2, +a1+a2, -a1+a2` with
    /// `a1 = dir x a2`).
    pub fn new(points: &[[f32; 3]], projection: [f32; 3]) -> Self {
        let direction = normalize(projection);
        let mut minimums = [f32::MAX; 3];
        let mut maximums = [f32::MIN; 3];
        for point in points {
            for candidate in [
                *point,
                add(*point, projection),
                add(*point, scale(direction, -REACH_BEHIND)),
            ] {
                for axis in 0..3 {
                    minimums[axis] = minimums[axis].min(candidate[axis]);
                    maximums[axis] = maximums[axis].max(candidate[axis]);
                }
            }
        }
        let points = &points[..points.len().min(MAX_VERTICES_ON_POLY)];
        let mut normals = [[0.0; 3]; MAX_PLANES];
        let mut distances = [0.0; MAX_PLANES];
        for (index, point) in points.iter().enumerate() {
            let edge = subtract(points[(index + 1) % points.len()], *point);
            let normal = normalize(cross(edge, scale(projection, -1.0)));
            normals[index] = normal;
            distances[index] = dot(normal, *point);
        }
        let count = points.len();
        let first = points.first().copied().unwrap_or_default();
        normals[count] = direction;
        distances[count] = dot(direction, first) - NEAR_PLANE_OFFSET;
        normals[count + 1] = scale(direction, -1.0);
        distances[count + 1] = dot(normals[count + 1], first) - REACH_BEHIND;
        Self {
            normals,
            distances,
            plane_count: count + 2,
            direction,
            minimums,
            maximums,
        }
    }

    /// Unit projection direction.
    pub fn direction(&self) -> [f32; 3] {
        self.direction
    }

    /// Axis-aligned bounds of the swept polygon, including the reach behind
    /// its start.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        (self.minimums, self.maximums)
    }
}

/// Fixed-capacity fragment output shared across projections.
#[derive(Clone, Debug)]
pub struct MarkFragments {
    points: Vec<[f32; 3]>,
    fragments: Vec<MarkFragment>,
    max_points: usize,
    max_fragments: usize,
}

impl MarkFragments {
    /// Allocate buffers for at most `max_points` points and `max_fragments`
    /// fragments; no later call allocates.
    pub fn with_capacity(max_points: usize, max_fragments: usize) -> Self {
        Self {
            points: Vec::with_capacity(max_points),
            fragments: Vec::with_capacity(max_fragments),
            max_points,
            max_fragments,
        }
    }

    /// Forget every fragment.
    pub fn clear(&mut self) {
        self.points.clear();
        self.fragments.clear();
    }

    /// Points of every fragment, in fragment order.
    pub fn points(&self) -> &[[f32; 3]] {
        &self.points
    }

    /// Fragments produced so far.
    pub fn fragments(&self) -> &[MarkFragment] {
        &self.fragments
    }

    /// Points of one fragment.
    pub fn fragment_points(&self, fragment: MarkFragment) -> &[[f32; 3]] {
        &self.points[fragment.first_point..fragment.first_point + fragment.point_count]
    }

    /// `true` once the fragment buffer is full; callers stop feeding triangles
    /// exactly as `R_MarkFragments` returns early.
    pub fn is_full(&self) -> bool {
        self.fragments.len() >= self.max_fragments
    }

    /// Clip `polygon` by every plane of `projection` and keep the remainder
    /// (`R_AddMarkFragments`). A polygon clipped away, too large or not
    /// fitting the point buffer leaves the buffers untouched.
    pub fn push_polygon(&mut self, polygon: &[[f32; 3]], projection: &MarkProjection) {
        if self.is_full() {
            return;
        }
        let mut buffers = [[[0.0; 3]; MAX_VERTICES_ON_POLY]; 2];
        let mut count = polygon.len().min(MAX_VERTICES_ON_POLY);
        buffers[0][..count].copy_from_slice(&polygon[..count]);
        let mut current = 0;
        for plane in 0..projection.plane_count {
            let (input, output) = split_buffers(&mut buffers, current);
            count = chop_behind_plane(
                &input[..count],
                output,
                projection.normals[plane],
                projection.distances[plane],
            );
            current ^= 1;
            if count == 0 {
                return;
            }
        }
        if self.points.len() + count > self.max_points {
            return;
        }
        self.fragments.push(MarkFragment {
            first_point: self.points.len(),
            point_count: count,
        });
        self.points.extend_from_slice(&buffers[current][..count]);
    }
}

fn split_buffers(
    buffers: &mut [[[f32; 3]; MAX_VERTICES_ON_POLY]; 2],
    current: usize,
) -> (
    &[[f32; 3]; MAX_VERTICES_ON_POLY],
    &mut [[f32; 3]; MAX_VERTICES_ON_POLY],
) {
    let (first, second) = buffers.split_at_mut(1);
    if current == 0 {
        (&first[0], &mut second[0])
    } else {
        (&second[0], &mut first[0])
    }
}

/// `R_ChopPolyBehindPlane`: keep the part of `input` in front of the plane.
/// Returns the output point count (zero when nothing remains).
fn chop_behind_plane(
    input: &[[f32; 3]],
    output: &mut [[f32; 3]; MAX_VERTICES_ON_POLY],
    normal: [f32; 3],
    distance: f32,
) -> usize {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Side {
        Front,
        Back,
        On,
    }
    if input.len() >= MAX_VERTICES_ON_POLY - 2 {
        return 0;
    }
    let mut distances = [0.0f32; MAX_VERTICES_ON_POLY + 1];
    let mut sides = [Side::On; MAX_VERTICES_ON_POLY + 1];
    let mut front = 0;
    let mut back = 0;
    for (index, point) in input.iter().enumerate() {
        let value = dot(*point, normal) - distance;
        distances[index] = value;
        sides[index] = if value > CLIP_EPSILON {
            front += 1;
            Side::Front
        } else if value < -CLIP_EPSILON {
            back += 1;
            Side::Back
        } else {
            Side::On
        };
    }
    sides[input.len()] = sides[0];
    distances[input.len()] = distances[0];
    if front == 0 {
        return 0;
    }
    if back == 0 {
        output[..input.len()].copy_from_slice(input);
        return input.len();
    }
    let mut count = 0;
    for (index, point) in input.iter().enumerate() {
        if sides[index] == Side::On {
            output[count] = *point;
            count += 1;
            continue;
        }
        if sides[index] == Side::Front {
            output[count] = *point;
            count += 1;
        }
        if sides[index + 1] == Side::On || sides[index + 1] == sides[index] {
            continue;
        }
        let next = input[(index + 1) % input.len()];
        let span = distances[index] - distances[index + 1];
        let fraction = if span == 0.0 {
            0.0
        } else {
            distances[index] / span
        };
        for axis in 0..3 {
            output[count][axis] = point[axis] + fraction * (next[axis] - point[axis]);
        }
        count += 1;
    }
    count
}

impl Bsp {
    /// Gather the world surfaces a mark may land on (`R_BoxSurfaces_r`).
    ///
    /// Walks the tree with the projection bounds, drops flares, planar faces
    /// whose plane misses the box or faces the wrong way, and any surface
    /// `accept` rejects. Surfaces reached through several leaves are listed
    /// once. Gathering stops when `list` reaches `capacity` entries.
    pub fn mark_surfaces(
        &self,
        projection: &MarkProjection,
        capacity: usize,
        list: &mut Vec<usize>,
        accept: impl Fn(usize, &Surface) -> bool,
    ) {
        if self.nodes.is_empty() {
            return;
        }
        let mut gather = SurfaceGather {
            bsp: self,
            projection,
            capacity,
            list,
            accept: &accept,
        };
        gather.visit(NodeChild::Node(0));
    }
}

struct SurfaceGather<'a> {
    bsp: &'a Bsp,
    projection: &'a MarkProjection,
    capacity: usize,
    list: &'a mut Vec<usize>,
    accept: &'a dyn Fn(usize, &Surface) -> bool,
}

impl SurfaceGather<'_> {
    /// Recursive tree walk, tail child handled in the loop as the reference.
    fn visit(&mut self, mut child: NodeChild) {
        let (minimums, maximums) = self.projection.bounds();
        loop {
            match child {
                NodeChild::Node(index) => {
                    let node = &self.bsp.nodes[index];
                    let plane = self.bsp.planes[node.plane];
                    child = match box_on_plane_side(minimums, maximums, plane) {
                        BoxSide::Front => node.children[0],
                        BoxSide::Back => node.children[1],
                        BoxSide::Both => {
                            self.visit(node.children[0]);
                            node.children[1]
                        }
                    };
                }
                NodeChild::Leaf(index) => {
                    self.visit_leaf(index);
                    return;
                }
            }
        }
    }

    fn visit_leaf(&mut self, leaf: usize) {
        let Some(leaf_surfaces) = self.bsp.leaf_surface_indices(leaf) else {
            return;
        };
        let surfaces = self.bsp.render.surfaces();
        let vertices = self.bsp.render.vertices();
        let (minimums, maximums) = self.projection.bounds();
        for &surface_index in leaf_surfaces {
            if self.list.len() >= self.capacity {
                return;
            }
            let Some(surface) = surfaces.get(surface_index) else {
                continue;
            };
            if !(self.accept)(surface_index, surface) || self.list.contains(&surface_index) {
                continue;
            }
            let eligible = match surface.kind {
                SurfaceKind::Planar => {
                    let Some(first) = vertices.get(surface.vertices.start) else {
                        continue;
                    };
                    let plane = Plane {
                        normal: surface.lightmap_vectors[2],
                        distance: dot(first.position, surface.lightmap_vectors[2]),
                    };
                    box_on_plane_side(minimums, maximums, plane) == BoxSide::Both
                        && dot(plane.normal, self.projection.direction()) <= FACE_FACING_LIMIT
                }
                SurfaceKind::Patch | SurfaceKind::TriangleSoup => true,
                SurfaceKind::Flare => false,
            };
            if eligible {
                self.list.push(surface_index);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoxSide {
    Front,
    Back,
    Both,
}

/// `BoxOnPlaneSide` for an arbitrary plane: which side(s) of `plane` the box
/// spans.
fn box_on_plane_side(minimums: [f32; 3], maximums: [f32; 3], plane: Plane) -> BoxSide {
    let mut nearest = [0.0; 3];
    let mut farthest = [0.0; 3];
    for axis in 0..3 {
        if plane.normal[axis] < 0.0 {
            nearest[axis] = maximums[axis];
            farthest[axis] = minimums[axis];
        } else {
            nearest[axis] = minimums[axis];
            farthest[axis] = maximums[axis];
        }
    }
    let front = plane.signed_distance(farthest) >= 0.0;
    let back = plane.signed_distance(nearest) < 0.0;
    match (front, back) {
        (true, false) => BoxSide::Front,
        (false, true) => BoxSide::Back,
        _ => BoxSide::Both,
    }
}

fn add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(vector: [f32; 3], factor: f32) -> [f32; 3] {
    vector.map(|value| value * factor)
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let length = dot(vector, vector).sqrt();
    if length <= 0.0 {
        return [0.0; 3];
    }
    scale(vector, 1.0 / length)
}

use super::trace_scratch::Traversal;
use super::{Bsp, NodeChild, Plane, TraceScratch, dot};
use std::error::Error;
use std::fmt;

const SURFACE_CLIP_EPSILON: f32 = 0.125;

#[path = "collision_box.rs"]
mod boxes;
#[path = "collision_brush.rs"]
mod brush;
pub use boxes::trace_box_against_box;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    minimums: [f32; 3],
    maximums: [f32; 3],
}

impl Aabb {
    pub const POINT: Self = Self {
        minimums: [0.0; 3],
        maximums: [0.0; 3],
    };

    pub fn new(minimums: [f32; 3], maximums: [f32; 3]) -> Result<Self, AabbError> {
        for axis in 0..3 {
            if !minimums[axis].is_finite()
                || !maximums[axis].is_finite()
                || minimums[axis] > maximums[axis]
            {
                return Err(AabbError { axis });
            }
        }
        Ok(Self { minimums, maximums })
    }

    pub fn minimums(self) -> [f32; 3] {
        self.minimums
    }

    pub fn maximums(self) -> [f32; 3] {
        self.maximums
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AabbError {
    axis: usize,
}

impl fmt::Display for AabbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid axis-aligned bounds on axis {}",
            self.axis
        )
    }
}

impl Error for AabbError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionTrace {
    pub fraction: f32,
    pub end_position: [f32; 3],
    pub plane: Option<Plane>,
    pub surface_flags: u32,
    pub content_flags: u32,
    pub start_solid: bool,
    pub all_solid: bool,
    /// The shader (an index into [`Bsp::shaders`]) of the brush side or patch whose
    /// surface flags the trace reports; `None` when it hit nothing or started inside.
    pub shader: Option<usize>,
}

impl Bsp {
    /// Sweeps an axis-aligned box through world brushes and curved patches.
    ///
    /// Fractions and the 1/8-unit clipping epsilon match OpenJK's multiplayer
    /// collision behavior. This convenience allocates scratch storage and is
    /// intended for tests and one-shot tools; frame loops must use
    /// [`Self::trace_box_with`].
    pub fn trace_box(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        bounds: Aabb,
        content_mask: u32,
    ) -> CollisionTrace {
        let mut scratch = self.trace_scratch();
        self.trace_box_with(&mut scratch, start, end, bounds, content_mask)
    }

    /// Sweeps an axis-aligned box using caller-owned reusable storage.
    ///
    /// OpenJK increments `clipMap_t::checkcount` for each trace and stamps
    /// brushes as they are visited (`codemp/qcommon/cm_trace.cpp:662-665,
    /// 1228`). This method uses the same generation scheme and reuses its BSP
    /// traversal stack, so a warmed scratch performs no heap allocation.
    pub fn trace_box_with(
        &self,
        scratch: &mut TraceScratch,
        start: [f32; 3],
        end: [f32; 3],
        bounds: Aabb,
        content_mask: u32,
    ) -> CollisionTrace {
        if !start.into_iter().chain(end).all(f32::is_finite) {
            return CollisionTrace::miss(start, end);
        }

        let work = TraceWork::new(start, end, bounds);
        let mut trace = CollisionTrace::miss(start, end);
        scratch.begin_trace(
            self.brushes.len() + self.patches.len(),
            self.nodes.len(),
            Traversal {
                child: NodeChild::Node(0),
                start_fraction: 0.0,
                end_fraction: 1.0,
                start: work.start,
                end: work.end,
            },
        );

        while let Some(segment) = scratch.pop() {
            if trace.fraction <= segment.start_fraction {
                continue;
            }
            let node_index = match segment.child {
                NodeChild::Node(node_index) => node_index,
                NodeChild::Leaf(leaf_index) => {
                    self.trace_leaf(leaf_index, &work, content_mask, scratch, &mut trace);
                    continue;
                }
            };

            let node = &self.nodes[node_index];
            let plane = self.planes[node.plane];
            let distance_start = plane.signed_distance(segment.start);
            let distance_end = plane.signed_distance(segment.end);
            let offset = work.tree_plane_offset(plane);

            if distance_start >= offset + 1.0 && distance_end >= offset + 1.0 {
                scratch.push(segment.with_child(node.children[0]));
                continue;
            }
            if distance_start < -offset - 1.0 && distance_end < -offset - 1.0 {
                scratch.push(segment.with_child(node.children[1]));
                continue;
            }

            let (near_side, near_fraction, far_fraction) =
                split_fractions(distance_start, distance_end, offset);
            let near = segment.split(near_fraction, node.children[near_side], true);
            let far = segment.split(far_fraction, node.children[near_side ^ 1], false);
            // LIFO: visit the near side first, preserving OpenJK's traversal.
            scratch.push(far);
            scratch.push(near);
        }

        trace.end_position = interpolate(start, end, trace.fraction);
        trace
    }

    fn trace_leaf(
        &self,
        leaf_index: usize,
        work: &TraceWork,
        content_mask: u32,
        scratch: &mut TraceScratch,
        trace: &mut CollisionTrace,
    ) {
        let leaf = &self.leaves[leaf_index];
        for leaf_brush_index in leaf.leaf_brushes.clone() {
            let brush_index = self.leaf_brushes[leaf_brush_index];
            if !scratch.mark_brush(brush_index) {
                continue;
            }
            self.trace_brush(brush_index, work, content_mask, trace);
            if trace.fraction == 0.0 {
                return;
            }
        }
        for leaf_surface in leaf.leaf_surfaces.clone() {
            let index = self.leaf_surfaces[leaf_surface];
            let Some(patch) = &self.patches[index] else {
                continue;
            };
            if !scratch.mark_brush(self.brushes.len() + index) {
                continue;
            }
            patch.trace(work, content_mask, trace);
            if trace.fraction == 0.0 {
                return;
            }
        }
    }

    /// Sweep through one inline model in model-local coordinates. Unlike a
    /// world trace this visits a unique brush range, so it needs no scratch
    /// allocation or traversal. The caller owns any entity transform.
    pub fn trace_model_box(
        &self,
        model_index: usize,
        start: [f32; 3],
        end: [f32; 3],
        bounds: Aabb,
        content_mask: u32,
    ) -> CollisionTrace {
        let mut trace = CollisionTrace::miss(start, end);
        let Some(model) = self.render().models().get(model_index) else {
            return trace;
        };
        if !start.into_iter().chain(end).all(f32::is_finite) {
            return trace;
        }
        let work = TraceWork::new(start, end, bounds);
        for brush_index in model.brushes.clone() {
            self.trace_brush(brush_index, &work, content_mask, &mut trace);
            if trace.fraction == 0.0 {
                break;
            }
        }
        for index in model.surfaces.clone() {
            if trace.fraction == 0.0 {
                break;
            }
            if let Some(patch) = &self.patches[index] {
                patch.trace(&work, content_mask, &mut trace);
            }
        }
        trace.end_position = interpolate(start, end, trace.fraction);
        trace
    }

    /// `CM_TransformedBoxTrace` (`cm_trace.cpp:1480-1560`) through inline model
    /// `model_index` standing at `origin`, turned by `axes` where it is rotated: the box is
    /// centred first (the endpoints move by its asymmetry, the extents do not turn — the
    /// reference's own rule), the endpoints are carried into the model's space, and the
    /// struck plane's normal is turned back. The end is the world's, from the fraction;
    /// the plane's distance stays the model's, as the reference leaves it.
    pub fn trace_transformed_model(
        &self,
        model_index: usize,
        origin: [f32; 3],
        axes: Option<[[f32; 3]; 3]>,
        start: [f32; 3],
        end: [f32; 3],
        bounds: Aabb,
        content_mask: u32,
    ) -> CollisionTrace {
        let (mins, maxs) = (bounds.minimums(), bounds.maximums());
        let offset: [f32; 3] = std::array::from_fn(|axis| (mins[axis] + maxs[axis]) * 0.5);
        let centred = Aabb::new(
            std::array::from_fn(|axis| mins[axis] - offset[axis]),
            std::array::from_fn(|axis| maxs[axis] - offset[axis]),
        )
        .expect("centering valid bounds preserves their order");
        let turn = |point: [f32; 3], axes: [[f32; 3]; 3]| {
            axes.map(|axis| axis[0] * point[0] + axis[1] * point[1] + axis[2] * point[2])
        };
        let local = |point: [f32; 3]| {
            let moved = std::array::from_fn(|axis| point[axis] + offset[axis] - origin[axis]);
            axes.map_or(moved, |axes| turn(moved, axes))
        };
        let mut trace =
            self.trace_model_box(model_index, local(start), local(end), centred, content_mask);
        if let (Some(axes), Some(plane)) = (axes, trace.plane.as_mut())
            && trace.fraction != 1.0
        {
            let transpose =
                std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row]));
            plane.normal = turn(plane.normal, transpose);
        }
        trace.end_position = interpolate(start, end, trace.fraction);
        trace
    }

    /// Allocates trace storage sized for this BSP's brushes, surfaces and nodes.
    pub fn trace_scratch(&self) -> TraceScratch {
        TraceScratch::with_capacity(self.brushes.len() + self.patches.len(), self.nodes.len())
    }
}

impl CollisionTrace {
    pub(super) fn miss(start: [f32; 3], end: [f32; 3]) -> Self {
        Self {
            fraction: 1.0,
            end_position: if end.into_iter().all(f32::is_finite) {
                end
            } else {
                start
            },
            plane: None,
            surface_flags: 0,
            content_flags: 0,
            start_solid: false,
            all_solid: false,
            shader: None,
        }
    }
}

pub(super) struct TraceWork {
    pub(super) start: [f32; 3],
    pub(super) end: [f32; 3],
    offsets: [[f32; 3]; 8],
    pub(super) extents: [f32; 3],
    pub(super) is_point: bool,
}

impl TraceWork {
    pub(super) fn new(start: [f32; 3], end: [f32; 3], bounds: Aabb) -> Self {
        let mut centered_start = start;
        let mut centered_end = end;
        let mut size = [[0.0; 3]; 2];
        for axis in 0..3 {
            let offset = (bounds.minimums[axis] + bounds.maximums[axis]) * 0.5;
            size[0][axis] = bounds.minimums[axis] - offset;
            size[1][axis] = bounds.maximums[axis] - offset;
            centered_start[axis] += offset;
            centered_end[axis] += offset;
        }
        let is_point = size[0] == [0.0; 3];
        let mut offsets = [[0.0; 3]; 8];
        for (sign_bits, corner) in offsets.iter_mut().enumerate() {
            for axis in 0..3 {
                corner[axis] = size[usize::from(sign_bits & (1 << axis) != 0)][axis];
            }
        }
        Self {
            start: centered_start,
            end: centered_end,
            offsets,
            extents: size[1],
            is_point,
        }
    }

    /// The work of `CM_TransformedBoxTrace` for an unrotated model standing at `origin`:
    /// the box is made symmetric first, then the move is taken into the model's frame —
    /// in that order, as the reference has it — and `CM_Trace` makes it symmetric again,
    /// which adds a zero.
    pub(super) fn relative(start: [f32; 3], end: [f32; 3], bounds: Aabb, origin: [f32; 3]) -> Self {
        let mut work = Self::new(start, end, bounds);
        for axis in 0..3 {
            let again = (work.offsets[0][axis] + work.offsets[7][axis]) * 0.5;
            work.start[axis] = (work.start[axis] - origin[axis]) + again;
            work.end[axis] = (work.end[axis] - origin[axis]) + again;
        }
        work
    }

    /// `tw.bounds`: the box's extent over the whole move.
    pub(super) fn swept_bounds(&self) -> ([f32; 3], [f32; 3]) {
        let (mut low, mut high) = ([0.0; 3], [0.0; 3]);
        for axis in 0..3 {
            let (first, last) = if self.start[axis] < self.end[axis] {
                (self.start[axis], self.end[axis])
            } else {
                (self.end[axis], self.start[axis])
            };
            low[axis] = first + self.offsets[0][axis];
            high[axis] = last + self.offsets[7][axis];
        }
        (low, high)
    }

    pub(super) fn corner_for_plane(&self, plane: Plane) -> [f32; 3] {
        self.offsets[plane_sign_bits(plane)]
    }

    fn tree_plane_offset(&self, plane: Plane) -> f32 {
        if let Some(axis) = positive_axial_axis(plane.normal) {
            self.extents[axis]
        } else if self.is_point {
            0.0
        } else {
            // Deliberately retained from JKA: non-axial nodes take both paths
            // for boxes rather than trying a tighter projection.
            2_048.0
        }
    }
}

fn split_fractions(distance_start: f32, distance_end: f32, offset: f32) -> (usize, f32, f32) {
    if distance_start < distance_end {
        let inverse = 1.0 / (distance_start - distance_end);
        (
            1,
            (distance_start - offset + SURFACE_CLIP_EPSILON) * inverse,
            (distance_start + offset + SURFACE_CLIP_EPSILON) * inverse,
        )
    } else if distance_start > distance_end {
        let inverse = 1.0 / (distance_start - distance_end);
        (
            0,
            (distance_start + offset + SURFACE_CLIP_EPSILON) * inverse,
            (distance_start - offset - SURFACE_CLIP_EPSILON) * inverse,
        )
    } else {
        (0, 1.0, 0.0)
    }
}

fn positive_axial_axis(normal: [f32; 3]) -> Option<usize> {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .position(|axis| normal == axis)
}

fn plane_sign_bits(plane: Plane) -> usize {
    plane
        .normal
        .into_iter()
        .enumerate()
        .fold(0, |bits, (axis, value)| {
            bits | (usize::from(value < 0.0) << axis)
        })
}

fn interpolate(start: [f32; 3], end: [f32; 3], fraction: f32) -> [f32; 3] {
    [
        start[0] + fraction * (end[0] - start[0]),
        start[1] + fraction * (end[1] - start[1]),
        start[2] + fraction * (end[2] - start[2]),
    ]
}

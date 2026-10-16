use super::NodeChild;

/// Reusable storage for an allocation-free BSP collision trace.
///
/// Brush and patch marks mirror OpenJK's collision-map `checkcount`: each trace advances
/// a generation rather than clearing a boolean array. The storage may be
/// reused with another BSP. A collision-object-count change resets marks and grows the
/// buffers lazily; callers should construct it with [`crate::Bsp::trace_scratch`]
/// when loading a map so that growth never occurs in the frame loop.
#[derive(Debug, Default)]
pub struct TraceScratch {
    brush_marks: Vec<u32>,
    check_count: u32,
    traversal: Vec<Traversal>,
    completed_traces: u64,
}

impl TraceScratch {
    /// Creates empty storage which is sized by its first trace.
    ///
    /// Prefer [`crate::Bsp::trace_scratch`] in production code.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(brushes: usize, nodes: usize) -> Self {
        Self {
            brush_marks: vec![0; brushes],
            check_count: 0,
            traversal: Vec::with_capacity(nodes.saturating_add(1)),
            completed_traces: 0,
        }
    }

    pub(crate) fn begin_trace(&mut self, brushes: usize, nodes: usize, root: Traversal) {
        if self.brush_marks.len() != brushes {
            self.brush_marks.resize(brushes, 0);
            self.brush_marks.fill(0);
            self.check_count = 0;
        }
        let required = nodes.saturating_add(1);
        if self.traversal.capacity() < required {
            self.traversal
                .reserve_exact(required - self.traversal.capacity());
        }
        self.check_count = self.check_count.wrapping_add(1);
        if self.check_count == 0 {
            self.brush_marks.fill(0);
            self.check_count = 1;
        }
        self.traversal.clear();
        self.traversal.push(root);
        self.completed_traces = self.completed_traces.wrapping_add(1);
    }

    pub(crate) fn mark_brush(&mut self, index: usize) -> bool {
        let mark = &mut self.brush_marks[index];
        if *mark == self.check_count {
            return false;
        }
        *mark = self.check_count;
        true
    }

    pub(crate) fn push(&mut self, traversal: Traversal) {
        self.traversal.push(traversal);
    }

    pub(crate) fn pop(&mut self) -> Option<Traversal> {
        self.traversal.pop()
    }

    /// Returns buffer capacities for allocation-regression tests.
    pub fn storage_capacities(&self) -> (usize, usize) {
        (self.brush_marks.capacity(), self.traversal.capacity())
    }

    /// Returns the number of traces begun with this scratch.
    pub fn completed_traces(&self) -> u64 {
        self.completed_traces
    }

    /// Restarts the diagnostic trace counter without changing storage.
    pub fn reset_completed_traces(&mut self) {
        self.completed_traces = 0;
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Traversal {
    pub(crate) child: NodeChild,
    pub(crate) start_fraction: f32,
    pub(crate) end_fraction: f32,
    pub(crate) start: [f32; 3],
    pub(crate) end: [f32; 3],
}

impl Traversal {
    pub(crate) fn with_child(self, child: NodeChild) -> Self {
        Self { child, ..self }
    }

    pub(crate) fn split(self, fraction: f32, child: NodeChild, near: bool) -> Self {
        let fraction = fraction.clamp(0.0, 1.0);
        let middle_fraction =
            self.start_fraction + (self.end_fraction - self.start_fraction) * fraction;
        let middle = interpolate(self.start, self.end, fraction);
        if near {
            Self {
                child,
                start_fraction: self.start_fraction,
                end_fraction: middle_fraction,
                start: self.start,
                end: middle,
            }
        } else {
            Self {
                child,
                start_fraction: middle_fraction,
                end_fraction: self.end_fraction,
                start: middle,
                end: self.end,
            }
        }
    }
}

fn interpolate(start: [f32; 3], end: [f32; 3], fraction: f32) -> [f32; 3] {
    [
        start[0] + fraction * (end[0] - start[0]),
        start[1] + fraction * (end[1] - start[1]),
        start[2] + fraction * (end[2] - start[2]),
    ]
}

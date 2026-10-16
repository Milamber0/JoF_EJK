//! The flood's priority queue (`CPriorityQueue`, `navigator.cpp:2690-2792`): a binary
//! heap ordered by `std::push_heap` and `std::pop_heap` with "greater cost" as the
//! comparison — libstdc++'s algorithms step for step, since which of two equal-cost
//! entries leaves first decides ranks.

/// A queued connection (`CEdge`): the node reached, the first step taken from the root to
/// reach it, and the cost so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Queued {
    pub(crate) first: i32,
    pub(crate) second: i32,
    pub(crate) cost: i32,
}

/// The heap, its storage kept from flood to flood.
#[derive(Clone, Debug, Default)]
pub(crate) struct Heap(Vec<Queued>);

/// `NodeTotalGreater`: `a` orders below `b` in the max-heap when its cost is greater.
fn below(a: &Queued, b: &Queued) -> bool {
    a.cost > b.cost
}

impl Heap {
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// `std::push_heap` after `push_back`.
    pub(crate) fn push(&mut self, value: Queued) {
        self.0.push(value);
        let hole = self.0.len() - 1;
        self.sift_up(hole, 0, value);
    }

    /// `std::pop_heap` and `pop_back`: the front taken out.
    pub(crate) fn pop(&mut self) -> Option<Queued> {
        let front = *self.0.first()?;
        let last = self.0.len() - 1;
        if last > 0 {
            // `__pop_heap`: the last element set into the hole the front leaves.
            let value = self.0[last];
            self.0[last] = front;
            self.adjust(0, last, value);
        }
        self.0.pop();
        Some(front)
    }

    /// `std::__push_heap`: `value` moved up from `hole` while its parent orders below it.
    fn sift_up(&mut self, mut hole: usize, top: usize, value: Queued) {
        let heap = &mut self.0;
        while hole > top {
            let parent = (hole - 1) / 2;
            if !below(&heap[parent], &value) {
                break;
            }
            heap[hole] = heap[parent];
            hole = parent;
        }
        heap[hole] = value;
    }

    /// `std::__adjust_heap` over the first `len` elements: the hole at `hole` moved down
    /// to a leaf along the greater children, then `value` pushed up from there.
    fn adjust(&mut self, mut hole: usize, len: usize, value: Queued) {
        let top = hole;
        let mut child = hole;
        while child < (len.saturating_sub(1)) / 2 {
            child = 2 * (child + 1);
            if below(&self.0[child], &self.0[child - 1]) {
                child -= 1;
            }
            self.0[hole] = self.0[child];
            hole = child;
        }
        if len & 1 == 0 && len >= 2 && child == (len - 2) / 2 {
            child = 2 * (child + 1);
            self.0[hole] = self.0[child - 1];
            hole = child - 1;
        }
        self.sift_up(hole, top, value);
    }
}

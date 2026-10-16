//! Failed edges (`navigator.cpp:1893-2172`): connections an actor found blocked, in a
//! fixed number of slots, found through a lookup by start node.

use crate::NODE_NONE;

/// A failed connection (`failedEdge_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailedEdge {
    /// The two nodes, `start` as the edge was failed from; [`NODE_NONE`] for a free slot.
    pub start: i32,
    pub end: i32,
    /// When it is next checked for being clear again.
    pub check_time: i32,
    /// The entity that found it blocked.
    pub entity: i32,
}

/// The failed edges and their lookup (`failedEdges`, `m_edgeLookupMap`).
///
/// The lookup is the reference's multimap from a start node to a slot: an entry is added
/// whenever a slot is filled and none is ever removed, so a stale entry can name a slot
/// since reused for another edge — and [`FailedEdges::edge_failed`] then answers for it as
/// the reference does. Entries are kept in insertion order within a start node, as
/// `std::multimap` keeps equal keys; an entry equal to one already kept is not repeated
/// (it could never answer differently), so the lookup stays bounded by nodes × slots.
#[derive(Clone, Debug)]
pub struct FailedEdges {
    slots: Vec<FailedEdge>,
    lookup: Vec<(i32, usize)>,
}

impl FailedEdges {
    /// `count` slots, all as memory left them in the reference (zero).
    pub fn new(count: usize) -> Self {
        Self {
            slots: vec![
                FailedEdge {
                    start: 0,
                    end: 0,
                    check_time: 0,
                    entity: 0
                };
                count
            ],
            lookup: Vec::new(),
        }
    }

    /// Every slot.
    pub fn slots(&self) -> &[FailedEdge] {
        &self.slots
    }

    /// Slot `at`, to change its check time or clear it.
    pub fn slot_mut(&mut self, at: usize) -> Option<&mut FailedEdge> {
        self.slots.get_mut(at)
    }

    /// The slots as a file had them, each put in the lookup by its start (`Load`).
    pub fn load(&mut self, slots: &[FailedEdge]) {
        for (at, slot) in slots.iter().enumerate().take(self.slots.len()) {
            self.slots[at] = *slot;
            self.remember(slot.start, at);
        }
    }

    /// The lookup emptied (`m_edgeLookupMap.clear()`), the slots kept.
    pub fn forget_lookup(&mut self) {
        self.lookup.clear();
    }

    fn remember(&mut self, start: i32, at: usize) {
        if !self.lookup.contains(&(start, at)) {
            self.lookup.push((start, at));
        }
    }

    /// `EdgeFailed`: the slot holding the edge between `start` and `end`, looked up by
    /// either end.
    pub fn edge_failed(&self, start: i32, end: i32) -> Option<usize> {
        let by = |key: i32, other: i32| {
            self.lookup
                .iter()
                .find(|(from, at)| *from == key && self.slots[*at].end == other)
                .map(|(_, at)| *at)
        };
        by(start, end).or_else(|| by(end, start))
    }

    /// `AddFailedEdge`'s bookkeeping for a valid edge: the entity kept if the edge has
    /// failed already (`Ok(None)`), else the first free slot filled, checked at
    /// `check_time` (`Ok(Some(slot))`). `Err(())` when every slot is taken.
    pub fn add(
        &mut self,
        entity: i32,
        start: i32,
        end: i32,
        check_time: i32,
    ) -> Result<Option<usize>, ()> {
        if let Some(at) = self.edge_failed(start, end) {
            self.slots[at].entity = entity;
            return Ok(None);
        }
        let Some(at) = self.slots.iter().position(|slot| slot.start == NODE_NONE) else {
            return Err(());
        };
        self.slots[at] = FailedEdge {
            start,
            end,
            check_time,
            entity,
        };
        self.remember(start, at);
        Ok(Some(at))
    }

    /// `ClearAllFailedEdges`' `memset(-1)`: every field of every slot -1, before each is
    /// cleared.
    pub fn fill_none(&mut self) {
        for slot in &mut self.slots {
            *slot = FailedEdge {
                start: NODE_NONE,
                end: NODE_NONE,
                check_time: -1,
                entity: -1,
            };
        }
    }
}

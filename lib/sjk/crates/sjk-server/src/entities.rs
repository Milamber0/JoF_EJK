//! Dense iteration with constant-time generational lookup and removal.

use sjk_runtime::EntityId;
use std::collections::TryReserveError;

struct Slot {
    generation: u32,
    dense: Option<usize>,
    next_free: Option<usize>,
}

pub(crate) struct Entry<T> {
    pub id: EntityId,
    pub state: T,
}

pub(crate) struct Entities<T> {
    slots: Vec<Slot>,
    pub dense: Vec<Entry<T>>,
    free: Option<usize>,
}

impl<T> Entities<T> {
    pub fn new(capacity: usize) -> Result<Self, TryReserveError> {
        let mut slots = Vec::new();
        let mut dense = Vec::new();
        slots.try_reserve_exact(capacity)?;
        dense.try_reserve_exact(capacity)?;
        slots.extend((0..capacity).map(|index| Slot {
            generation: 1,
            dense: None,
            next_free: (index + 1 < capacity).then_some(index + 1),
        }));
        Ok(Self {
            slots,
            dense,
            free: (capacity > 0).then_some(0),
        })
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn insert(&mut self, state: T) -> Result<EntityId, T> {
        let Some(index) = self.free else {
            return Err(state);
        };
        let slot = &mut self.slots[index];
        self.free = slot.next_free.take();
        let id = EntityId::new((u64::from(slot.generation) << 32) | index as u64);
        slot.dense = Some(self.dense.len());
        self.dense.push(Entry { id, state });
        Ok(id)
    }

    fn index(&self, id: EntityId) -> Option<usize> {
        let slot = self.slots.get(id.get() as u32 as usize)?;
        (u64::from(slot.generation) == id.get() >> 32)
            .then_some(slot.dense)
            .flatten()
    }

    pub fn get(&self, id: EntityId) -> Option<&T> {
        self.index(id).map(|index| &self.dense[index].state)
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        let index = self.index(id)?;
        Some(&mut self.dense[index].state)
    }

    pub fn get_pair_mut(&mut self, one: EntityId, two: EntityId) -> Option<(&mut T, &mut T)> {
        let (one, two) = (self.index(one)?, self.index(two)?);
        let [one, two] = self.dense.get_disjoint_mut([one, two]).ok()?;
        Some((&mut one.state, &mut two.state))
    }

    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        let dense = self.index(id)?;
        let removed = self.dense.swap_remove(dense);
        if let Some(moved) = self.dense.get(dense) {
            self.slots[moved.id.get() as u32 as usize].dense = Some(dense);
        }
        let slot = &mut self.slots[id.get() as u32 as usize];
        slot.dense = None;
        // Never wrap a generation: that would make a long-lived stale handle valid.
        // An exhausted slot remains retired until this world is destroyed.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            slot.next_free = self.free;
            self.free = Some(id.get() as u32 as usize);
        }
        Some(removed.state)
    }
}

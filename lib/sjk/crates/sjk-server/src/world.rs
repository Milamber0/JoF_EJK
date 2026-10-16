use crate::{EntityId, WorldId, entities::Entities};

/// A native entity identity qualified by its owning world.
///
/// The pair is opaque to wire adapters. Its components are useful as lookup keys,
/// never as legacy entity numbers. Handles are valid only in their issuing server.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EntityHandle {
    world: WorldId,
    entity: EntityId,
}

impl EntityHandle {
    /// World lifetime in which this entity was created.
    pub fn world(self) -> WorldId {
        self.world
    }
    /// Generation-qualified identity within that world.
    pub fn entity(self) -> EntityId {
        self.entity
    }
}

/// The world's entity budget is exhausted; the caller retains the rejected state.
///
/// Removed slots are reused with a fresh generation. Slots that exhaust their
/// generation counter are permanently retired instead of aliasing old handles.
#[derive(Debug)]
pub struct EntityRejected<T> {
    /// State that was not inserted. No existing entity was changed.
    pub state: T,
}

/// Owned world resources and dense authoritative game state.
///
/// `W` can own collision/assets and game-wide state; `E` is the game adapter's
/// entity state. This owner assumes no map format, gameplay rules or wire schema.
/// The configured entity capacity is preallocated at world creation. Insertion,
/// lookup, removal and iteration allocate no storage themselves; caller payloads
/// and callbacks may allocate independently.
pub struct ServerWorld<W, E> {
    pub(crate) id: WorldId,
    pub(crate) resources: W,
    pub(crate) entities: Entities<E>,
}

impl<W, E> ServerWorld<W, E> {
    /// Identity of this particular world lifetime, retained across removal.
    pub fn id(&self) -> WorldId {
        self.id
    }
    /// Caller-supplied world resources.
    pub fn resources(&self) -> &W {
        &self.resources
    }
    /// Mutable access to authoritative world resources.
    pub fn resources_mut(&mut self) -> &mut W {
        &mut self.resources
    }
    /// Configured storage slots; exhausted generations can reduce usable capacity.
    pub fn entity_capacity(&self) -> usize {
        self.entities.capacity()
    }
    /// Number of live authoritative entities.
    pub fn len(&self) -> usize {
        self.entities.dense.len()
    }
    /// Whether this world has no live entities.
    pub fn is_empty(&self) -> bool {
        self.entities.dense.is_empty()
    }

    /// Insert state and issue a fresh handle, or return the state when full.
    pub fn spawn(&mut self, state: E) -> Result<EntityHandle, EntityRejected<E>> {
        self.entities
            .insert(state)
            .map(|entity| EntityHandle {
                world: self.id,
                entity,
            })
            .map_err(|state| EntityRejected { state })
    }

    /// Look up live state; stale and foreign-world handles return `None`.
    pub fn entity(&self, handle: EntityHandle) -> Option<&E> {
        if handle.world != self.id {
            return None;
        }
        self.entities.get(handle.entity)
    }

    /// Mutate live state; stale and foreign-world handles return `None`.
    pub fn entity_mut(&mut self, handle: EntityHandle) -> Option<&mut E> {
        if handle.world != self.id {
            return None;
        }
        self.entities.get_mut(handle.entity)
    }

    /// Mutate two live states at once; `None` when either handle is stale or foreign, or
    /// both name the same entity.
    pub fn entity_pair_mut(
        &mut self,
        one: EntityHandle,
        two: EntityHandle,
    ) -> Option<(&mut E, &mut E)> {
        if one.world != self.id || two.world != self.id {
            return None;
        }
        self.entities.get_pair_mut(one.entity, two.entity)
    }

    /// Remove state and invalidate its handle before the slot can be reused.
    pub fn despawn(&mut self, handle: EntityHandle) -> Option<E> {
        if handle.world != self.id {
            return None;
        }
        self.entities.remove(handle.entity)
    }

    /// Visit live entities in dense storage order, without scanning vacant slots.
    ///
    /// Removal swaps the last entry into the removed entry's position. This is not
    /// a gameplay scheduling order: compatibility profiles must impose any required
    /// legacy update order separately. Repeated iteration without mutation is stable.
    pub fn entities(&self) -> impl ExactSizeIterator<Item = (EntityHandle, &E)> {
        let world = self.id;
        self.entities.dense.iter().map(move |entry| {
            (
                EntityHandle {
                    world,
                    entity: entry.id,
                },
                &entry.state,
            )
        })
    }

    /// Mutably visit live entities; order follows [`Self::entities`].
    pub fn entities_mut(&mut self) -> impl ExactSizeIterator<Item = (EntityHandle, &mut E)> {
        let world = self.id;
        self.entities.dense.iter_mut().map(move |entry| {
            (
                EntityHandle {
                    world,
                    entity: entry.id,
                },
                &mut entry.state,
            )
        })
    }
}

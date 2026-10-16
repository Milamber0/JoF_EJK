use crate::{ServerWorld, WorldId, entities::Entities};
use std::{
    collections::{BTreeMap, TryReserveError},
    error::Error,
    fmt,
};

/// Owns independent authoritative worlds, with caller-configured resource budgets.
///
/// There is no implicit current world or default legacy capacity. World identities
/// are never reused within this server, including across map removal/replacement.
/// Acquire a world once per simulation step; entity operations then use constant-time
/// lookup or dense iteration. World creation/removal are lifecycle operations.
pub struct Server<W, E> {
    worlds: BTreeMap<WorldId, ServerWorld<W, E>>,
    world_capacity: usize,
    next_world: Option<u64>,
}

impl<W, E> Server<W, E> {
    /// Create an empty owner allowing at most `world_capacity` resident worlds.
    pub fn new(world_capacity: usize) -> Self {
        Self {
            worlds: BTreeMap::new(),
            world_capacity,
            next_world: Some(1),
        }
    }

    /// Create a world, preallocating entity storage before publishing its identity.
    ///
    /// `entity_capacity` is independent of any protocol. Native entity handles use
    /// 32 slot bits and 32 generation bits, so at most `u32::MAX` slots are accepted.
    /// Allocation failure or an invalid budget leaves the server unchanged. Resources
    /// are dropped on failure, just as on removal followed by dropping the world.
    pub fn create_world(
        &mut self,
        resources: W,
        entity_capacity: usize,
    ) -> Result<WorldId, CreateWorldError> {
        if self.worlds.len() == self.world_capacity {
            return Err(CreateWorldError::WorldCapacity);
        }
        if entity_capacity > u32::MAX as usize {
            return Err(CreateWorldError::EntityCapacity);
        }
        let next = self.next_world.ok_or(CreateWorldError::WorldIdentity)?;
        let entities = Entities::new(entity_capacity).map_err(CreateWorldError::Allocation)?;
        let id = WorldId::new(next);
        self.worlds.insert(
            id,
            ServerWorld {
                id,
                resources,
                entities,
            },
        );
        self.next_world = next.checked_add(1);
        Ok(id)
    }

    /// Find an explicitly identified resident world.
    pub fn world(&self, id: WorldId) -> Option<&ServerWorld<W, E>> {
        self.worlds.get(&id)
    }
    /// Mutate an explicitly identified resident world.
    pub fn world_mut(&mut self, id: WorldId) -> Option<&mut ServerWorld<W, E>> {
        self.worlds.get_mut(&id)
    }
    /// Detach a world and transfer all its resources/state to the caller.
    ///
    /// Its identity is never reused or reinserted. Retained owners may still inspect
    /// it independently, but it no longer belongs to this server's resident worlds.
    pub fn remove_world(&mut self, id: WorldId) -> Option<ServerWorld<W, E>> {
        self.worlds.remove(&id)
    }
    /// Iterate resident worlds in creation order without allocating a list.
    pub fn worlds(&self) -> impl ExactSizeIterator<Item = &ServerWorld<W, E>> {
        self.worlds.values()
    }
    /// Maximum simultaneously resident worlds, independent of their entity budgets.
    pub fn world_capacity(&self) -> usize {
        self.world_capacity
    }
}

/// World creation failed without publishing a world or consuming its identity.
#[derive(Debug)]
pub enum CreateWorldError {
    /// The configured resident-world budget is full.
    WorldCapacity,
    /// The requested entity count exceeds the native handle's slot range.
    EntityCapacity,
    /// All native world identities have been consumed; IDs never wrap.
    WorldIdentity,
    /// Preallocating entity storage failed.
    Allocation(TryReserveError),
}

impl fmt::Display for CreateWorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorldCapacity => f.write_str("resident world budget exhausted"),
            Self::EntityCapacity => f.write_str("entity budget exceeds native identity range"),
            Self::WorldIdentity => f.write_str("native world identities exhausted"),
            Self::Allocation(error) => write!(f, "world storage allocation failed: {error}"),
        }
    }
}
impl Error for CreateWorldError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

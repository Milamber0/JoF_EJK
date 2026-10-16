//! Protocol 26's entity numbering, as the legacy adapter projects native identities
//! onto it.
//!
//! A legacy client knows every entity by a 10-bit number: `0..MAX_CLIENTS` are the
//! players, `MAX_CLIENTS..ENTITYNUM_MAX_NORMAL` the game's other entities, and the last
//! two the world and "none". A server's core does not store its entities by these
//! numbers; it hands the adapter an ordinal (the order its entities were allocated in,
//! which for the JKA game profile is `G_Spawn`'s lowest-free-slot order) and the adapter
//! numbers it here — or reports that a legacy client cannot represent it
//! ([`LegacyEntityNumbers::game_entity`] returns `None`).

/// `MAX_CLIENTS`: the first number a game entity other than a player is given.
pub const LEGACY_FIRST_GAME_ENTITY: u16 = 32;
/// `ENTITYNUM_MAX_NORMAL`: no game entity is numbered at or above it
/// (`ENTITYNUM_WORLD` and `ENTITYNUM_NONE` follow).
pub const LEGACY_GAME_ENTITY_CEILING: u16 = 1_022;
/// How many game entities beyond the players a legacy client can be sent at once.
pub const LEGACY_GAME_ENTITIES: usize =
    (LEGACY_GAME_ENTITY_CEILING - LEGACY_FIRST_GAME_ENTITY) as usize;

/// The adapter's numbering of the game's non-player entities.
///
/// The core allocates its entities in an order of its own (an *ordinal*: the JKA profile
/// reuses the lowest free one, as `G_Spawn` does) under a budget of its own; this maps
/// an ordinal to the wire number a legacy client knows it by and back. An ordinal at or
/// beyond [`LEGACY_GAME_ENTITIES`] has no wire number: such an entity is withheld from a
/// legacy client's snapshot, and counted, never aliased onto another number.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyEntityNumbers;

impl LegacyEntityNumbers {
    /// The wire number of the game entity allocated at `ordinal`, or `None` if a legacy
    /// client cannot represent it.
    pub const fn game_entity(ordinal: usize) -> Option<u16> {
        if ordinal < LEGACY_GAME_ENTITIES {
            Some(LEGACY_FIRST_GAME_ENTITY + ordinal as u16)
        } else {
            None
        }
    }

    /// The ordinal a wire number names, if it names a game entity (not a player, the
    /// world or "none").
    pub const fn ordinal(number: u16) -> Option<usize> {
        if number >= LEGACY_FIRST_GAME_ENTITY && number < LEGACY_GAME_ENTITY_CEILING {
            Some((number - LEGACY_FIRST_GAME_ENTITY) as usize)
        } else {
            None
        }
    }
}

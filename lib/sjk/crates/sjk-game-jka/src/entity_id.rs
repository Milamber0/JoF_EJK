//! The native identity of a game entity beyond the players.
//!
//! The game core stores and addresses its entities by [`EntityId`]: the ordinal the
//! entity was allocated at (the order the JKA profile's `G_Spawn` rule gives, which is also
//! the order `G_RunFrame` runs them in) and a generation that moves on every time the
//! ordinal is handed out again. A handle kept after its entity was freed never reaches the
//! entity that took its place.
//!
//! Protocol 26's wire number is not part of the identity: the legacy adapter
//! ([`sjk_protocol::LegacyEntityNumbers`]) projects the ordinal onto it, and an ordinal it
//! cannot represent has none ([`EntityId::legacy_number`]).

use sjk_protocol::{ENTITY_NUMBER_NONE, LegacyEntityNumbers};

/// A game entity's native, generational identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityId {
    ordinal: u32,
    generation: u32,
}

impl EntityId {
    /// The identity of the entity allocated at `ordinal` in its `generation`th use.
    pub const fn new(ordinal: u32, generation: u32) -> Self {
        Self {
            ordinal,
            generation,
        }
    }

    /// Where it was allocated, counted from the first entity beyond the players: its
    /// place in the game's run order.
    pub const fn ordinal(self) -> usize {
        self.ordinal as usize
    }

    /// How many times its ordinal had been handed out before it.
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// The number a legacy client knows it by, through the protocol-26 adapter's
    /// projection; [`ENTITY_NUMBER_NONE`] for an entity beyond what a legacy client can
    /// represent (which the snapshot adapter withholds and counts, never sends).
    ///
    /// This is the boundary crossing of the JKA profile while its traces and wire-state
    /// cross-references still speak wire numbers.
    pub const fn legacy_number(self) -> u16 {
        match LegacyEntityNumbers::game_entity(self.ordinal as usize) {
            Some(number) => number,
            None => ENTITY_NUMBER_NONE,
        }
    }
}

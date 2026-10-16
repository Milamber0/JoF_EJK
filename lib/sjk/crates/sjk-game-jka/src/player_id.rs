//! The native identity of a player.
//!
//! The game core keeps its players by [`PlayerId`]: the player's *ordinal* — its place in
//! the profile's player order, which for the JKA profile is also its gentity number and the
//! order `G_RunFrame` runs the players in — and a generation that moves on every time the
//! place is given to another player. An identity kept after its player left never reaches
//! the player who took the place.
//!
//! Protocol 26's client number is not part of the identity: the legacy adapter
//! ([`sjk_protocol::LegacyClientNumbers`]) projects the ordinal onto it, and an ordinal it
//! cannot represent has none ([`PlayerId::legacy_client`]).

use sjk_protocol::LegacyClientNumbers;

/// A player's native, generational identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlayerId {
    ordinal: u32,
    generation: u32,
}

impl PlayerId {
    /// The identity of the player admitted at `ordinal` in its `generation`th use.
    pub const fn new(ordinal: u32, generation: u32) -> Self {
        Self {
            ordinal,
            generation,
        }
    }

    /// Its place in the profile's player order.
    pub const fn ordinal(self) -> usize {
        self.ordinal as usize
    }

    /// How many players had held its place before it.
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// The client number a legacy client knows it by, through the protocol-26 adapter's
    /// projection; `None` for a player a legacy client cannot represent.
    pub const fn legacy_client(self) -> Option<usize> {
        LegacyClientNumbers::client(self.ordinal as usize)
    }
}

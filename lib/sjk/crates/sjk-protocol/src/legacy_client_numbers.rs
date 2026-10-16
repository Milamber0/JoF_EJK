//! Protocol 26's client numbering, as the legacy adapter projects native player identities
//! onto it.
//!
//! A legacy client knows every player by a client number `0..MAX_CLIENTS`, which is also
//! that player's entity number in every snapshot, its `CS_PLAYERS` configstring's offset,
//! the `clientNum` of its events and its place in the scoreboard. A server's core does not
//! store its players by these numbers: it keeps them by a native identity whose *ordinal*
//! is the player's place in the game profile's player order, and the adapter numbers that
//! ordinal here — or reports that a legacy client cannot represent it
//! ([`LegacyClientNumbers::client`] returns `None`).

/// `MAX_CLIENTS`: how many players a legacy client can tell apart.
pub const LEGACY_MAX_CLIENTS: usize = 32;

/// The adapter's numbering of the game's players.
///
/// The JKA game profile keeps the reference's rule that a player's place is the connection
/// slot it was admitted through (`g_entities[clientNum]`, `SV_DirectConnect`'s and
/// `SV_BotAllocateClient`'s lowest free slot), so for a protocol-26 client the projection
/// is the identity below [`LEGACY_MAX_CLIENTS`]. An ordinal at or beyond it has no client
/// number: such a player is withheld from legacy clients, never aliased onto another.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyClientNumbers;

impl LegacyClientNumbers {
    /// The client number of the player at `ordinal`, or `None` if a legacy client cannot
    /// represent it.
    pub const fn client(ordinal: usize) -> Option<usize> {
        if ordinal < LEGACY_MAX_CLIENTS {
            Some(ordinal)
        } else {
            None
        }
    }

    /// The ordinal a legacy client number names, or `None` for a number no legacy client
    /// can hold.
    pub const fn ordinal(client: usize) -> Option<usize> {
        if client < LEGACY_MAX_CLIENTS {
            Some(client)
        } else {
            None
        }
    }
}

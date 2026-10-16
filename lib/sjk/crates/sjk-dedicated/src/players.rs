//! The game's players, by native identity.
//!
//! Every player — a legacy client's or a bot's — is a peer entity of the core's world
//! (`sjk_server`), which stores it under its own budget. [`PlayerRoster`] is the game
//! profile's order over those peers: each player holds a *place* (the ordinal of its
//! [`PlayerId`]), which is the order the JKA rules run and number players in, and a
//! generation that tells successive holders of one place apart.
//!
//! No protocol-26 client number is stored here. The legacy host boundary
//! (`bridge_host.rs`) turns the adapter's client number into a place through
//! [`sjk_protocol::LegacyClientNumbers`] on the way in and back on the way out.

use sjk_game_jka::player_id::PlayerId;
use sjk_server::EntityHandle;

/// One place of the roster: who holds it, and how many held it before.
#[derive(Clone, Copy, Debug, Default)]
struct Place {
    generation: u32,
    /// The generation counter ran out: the place is never given again, so no identity
    /// is ever issued twice.
    retired: bool,
    holder: Option<EntityHandle>,
}

/// The players in the profile's order, each place naming its peer in the core's world.
///
/// The number of places is the profile's `sv_maxclients`; it is allocated once, so no
/// lookup or iteration allocates.
#[derive(Clone, Debug)]
pub(crate) struct PlayerRoster {
    places: Vec<Place>,
}

impl PlayerRoster {
    /// A roster of `places` empty places.
    pub(crate) fn new(places: usize) -> Self {
        Self {
            places: vec![Place::default(); places],
        }
    }

    /// How many places there are (`sv_maxclients` to the profile's rules).
    pub(crate) fn places(&self) -> usize {
        self.places.len()
    }

    /// Give the place `ordinal` to the peer `holder`; `None` when there is no such place,
    /// it is held, or it is retired.
    pub(crate) fn admit(&mut self, ordinal: usize, holder: EntityHandle) -> Option<PlayerId> {
        let place = self
            .places
            .get_mut(ordinal)
            .filter(|place| place.holder.is_none() && !place.retired)?;
        place.holder = Some(holder);
        Some(PlayerId::new(
            u32::try_from(ordinal).ok()?,
            place.generation,
        ))
    }

    /// Free the place `ordinal`, handing back the peer that held it. The next holder is
    /// another generation.
    pub(crate) fn release(&mut self, ordinal: usize) -> Option<EntityHandle> {
        let place = self.places.get_mut(ordinal)?;
        let holder = place.holder.take()?;
        match place.generation.checked_add(1) {
            Some(next) => place.generation = next,
            None => place.retired = true,
        }
        Some(holder)
    }

    /// The peer holding the place `ordinal` now, whoever that is: the crossing for the
    /// profile code that still addresses players by their place (slice C).
    pub(crate) fn at(&self, ordinal: usize) -> Option<EntityHandle> {
        self.places.get(ordinal)?.holder
    }

    /// Every place's holder, in the profile's order.
    pub(crate) fn holders(&self) -> impl Iterator<Item = Option<EntityHandle>> + '_ {
        self.places.iter().map(|place| place.holder)
    }

    /// Every player, by identity, in the profile's order.
    pub(crate) fn occupied(&self) -> impl Iterator<Item = (PlayerId, EntityHandle)> + '_ {
        self.places
            .iter()
            .enumerate()
            .filter_map(|(ordinal, place)| {
                Some((
                    PlayerId::new(u32::try_from(ordinal).ok()?, place.generation),
                    place.holder?,
                ))
            })
    }
}

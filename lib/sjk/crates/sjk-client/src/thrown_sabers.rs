//! Snapshot-indexed thrown saber ownership; no renderer or predicted state.

use crate::player_identity::saber_names_from_config;
use crate::presentation_equipment::saber_color_from_config;
use sjk_protocol::{EntityState, GameState, Snapshot};

/// Owner-authored presentation of the primary saber detached from the hand.
#[derive(Clone, Copy, Debug)]
pub struct LegacyThrownSaber<'a> {
    /// Legacy owner client number.
    pub owner: u16,
    /// Borrowed primary `.sab` definition name from clientinfo.
    pub name: &'a str,
    /// Primary blade colour, including server-supplied custom RGB.
    pub color: [u8; 3],
    /// False once the owner catches the saber; suppress a lingering entity.
    pub in_flight: bool,
    /// Partial staff holster suppresses every blade after blade zero.
    pub extra_blades: bool,
    /// The saber entity's own `saberInFlight` is clear while its owner still
    /// throws it: the saber is being pulled back, and CG_Player turns it to
    /// face away from its owner without spin (`cg_players.c:10221-10239`).
    pub returning: bool,
}

/// Client owner of an `ET_GENERAL`/`WP_SABER` entity.
///
/// OpenJK `codemp/game/w_saber.c:8679,8683` writes `genericenemyindex =
/// client + 1024` (entity netfield 18) and weapon 3 (`bg_weapons.h:30-35`).
/// Neither `owner` (netfield 40) nor `otherEntityNum` carries this link.
pub fn legacy_thrown_saber_owner(state: &EntityState) -> Option<u16> {
    if state.entity_type() != 0 || state.weapon() != 3 {
        return None;
    }
    let owner = state.raw_field(18)?.checked_sub(1_024)?;
    (owner < 32).then_some(owner as u16)
}

/// One stack-allocated entity table; every thrown-saber/owner lookup is O(1).
pub struct LegacyThrownSabers<'a> {
    states: [Option<&'a EntityState>; 1_024],
    snapshot: &'a Snapshot,
    game: &'a GameState,
}

impl<'a> LegacyThrownSabers<'a> {
    /// Index one presented snapshot without allocating or cloning entities.
    pub fn new(snapshot: &'a Snapshot, game: &'a GameState) -> Self {
        let mut states = [None; 1_024];
        for state in &snapshot.entities {
            if let Some(slot) = states.get_mut(usize::from(state.number())) {
                *slot = Some(state);
            }
        }
        Self {
            states,
            snapshot,
            game,
        }
    }

    /// Resolve the primary hilt and blade policy for one network entity number.
    pub fn get(&self, number: u16) -> Option<LegacyThrownSaber<'a>> {
        let state = (*self.states.get(usize::from(number))?)?;
        let owner = legacy_thrown_saber_owner(state)?;
        let config = self.game.config_string(1_131 + usize::from(owner))?;
        if config.is_empty() {
            return None;
        }
        let (holstered, in_flight) = if owner == self.snapshot.player.client_num() {
            (
                self.snapshot.player.saber_holstered(),
                self.snapshot.player.saber_in_flight(),
            )
        } else {
            self.states[usize::from(owner)].map_or((0, true), |state| {
                (state.saber_holstered(), state.saber_in_flight())
            })
        };
        Some(LegacyThrownSaber {
            owner,
            name: saber_names_from_config(config)[0]?,
            color: saber_color_from_config(config, 0),
            in_flight,
            extra_blades: holstered != 1,
            returning: !state.saber_in_flight(),
        })
    }
}

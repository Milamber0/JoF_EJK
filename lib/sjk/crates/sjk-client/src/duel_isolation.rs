//! Private-duel pass-through in movement prediction on JA+ and jaPRO/TaystJK.
//!
//! Stock codemp keeps duellers solid to everyone. JA+ and jaPRO let the two
//! duellers and everyone else pass through each other, and leave part of that to
//! the client: a JA+ 2.4 server sends a dueller to a client-plugin bystander as a
//! solid player box (with `bolt1`, the duel flag), so the plugin client must skip
//! it in prediction, while a client without the plugin gets the dueller with
//! `solid 0` and bystanders are not sent to duellers at all. jaPRO's server does
//! the same through `BeginHack` (`codemp/game/g_syscalls.c` in EternalJK's tree),
//! and EternalJK's prediction mirrors both mods in `CG_ClipMoveToEntities`
//! (`cgame/cg_predict.c:267-280`).

use crate::CompatProfile;
use sjk_protocol::{EntityState, PlayerState};

const ET_PLAYER: u8 = 1;
const ET_NPC: u8 = 13;

impl CompatProfile {
    /// Whether the server lets private duellers and everyone else pass through
    /// each other (JA+ and jaPRO/TaystJK), see [`duel_passes_through`].
    pub fn isolates_duels(&self) -> bool {
        matches!(self, Self::JaPlus { .. } | Self::TaystJk)
    }
}

/// Whether movement for `local` passes through `entity` because of a private
/// duel, on a server whose profile [isolates duels](CompatProfile::isolates_duels).
///
/// A dueller passes through every player and NPC but its opponent, as jaPRO's
/// server makes them non-solid for it (EternalJK's client skips every non-mover
/// instead; only players and NPCs are made non-solid by the server). Anyone else
/// passes through duelling players.
pub fn duel_passes_through(local: &PlayerState, entity: &EntityState) -> bool {
    if local.duel_in_progress() {
        matches!(entity.entity_type(), ET_PLAYER | ET_NPC) && entity.number() != local.duel_index()
    } else {
        // A player entity's bolt1 is its duelInProgress (BG_PlayerStateToEntityState).
        entity.entity_type() == ET_PLAYER && entity.bolt1()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_protocol::LEGACY_ENTITY_FIELDS;

    fn entity(number: u16, kind: u8, bolt1: bool) -> EntityState {
        let mut state = EntityState::zero(number, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(8, u32::from(kind));
        state.set_raw_field(91, u32::from(bolt1));
        state
    }

    fn player(client: u16, duel: Option<u16>) -> PlayerState {
        let mut state = PlayerState::default();
        state.set_client_num(client);
        if let Some(opponent) = duel {
            state.set_raw_field(119, 1);
            state.set_raw_field(44, u32::from(opponent));
        }
        state
    }

    #[test]
    fn a_dueller_passes_through_everyone_but_the_opponent() {
        let local = player(0, Some(1));
        assert!(!duel_passes_through(&local, &entity(1, ET_PLAYER, true)));
        assert!(duel_passes_through(&local, &entity(2, ET_PLAYER, false)));
        assert!(duel_passes_through(&local, &entity(40, ET_NPC, false)));
        // Movers and other solids keep blocking.
        assert!(!duel_passes_through(&local, &entity(60, 6, false)));
        assert!(!duel_passes_through(&local, &entity(61, 0, false)));
    }

    #[test]
    fn a_bystander_passes_through_duellers_only() {
        let local = player(0, None);
        assert!(duel_passes_through(&local, &entity(1, ET_PLAYER, true)));
        assert!(!duel_passes_through(&local, &entity(2, ET_PLAYER, false)));
        // bolt1 means something else on other entity types.
        assert!(!duel_passes_through(&local, &entity(60, 6, true)));
    }

    #[test]
    fn only_ja_plus_and_japro_isolate_duels() {
        assert!(CompatProfile::JaPlus { version: None }.isolates_duels());
        assert!(CompatProfile::TaystJk.isolates_duels());
        assert!(!CompatProfile::BaseJka.isolates_duels());
        assert!(!CompatProfile::Unknown(String::new()).isolates_duels());
    }
}

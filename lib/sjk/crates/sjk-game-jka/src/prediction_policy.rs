//! When the client stops predicting its own movement and shows the server's instead
//! (`CG_PredictPlayerState`, JoF EternalJK `codemp/cgame/cg_predict.c:1160-1230`).
//!
//! Three situations take the snapshot's state: `cg_noPredict`, a synchronous server and,
//! on a JA+ server, the added side/back kick (the server runs knockdown and get-up rules
//! the client's pmove does not replicate, so every input mispredicts and the correction
//! shakes the camera). Whether the view's angles follow the server too (`grabAngles`)
//! is part of the answer.

use crate::knockdown::in_knockdown;
use crate::legacy_animation_index;
use sjk_protocol::PlayerState;

/// `HANDEXTEND_KNOCKDOWN`.
const HANDEXTEND_KNOCKDOWN: u8 = 8;
/// `ps.forceDodgeAnim` (player-state field 89) in a JA+ added kick's victim.
const KICK_DODGE_ANIMS: [u32; 2] = [4, 5];

/// `GHOST_KNOWN_FLAG` (`q_shared.h`): the JA+ server sets this top bit of
/// `fd.forcePowersKnown` every think while it lets the player through other players.
pub const GHOST_KNOWN_FLAG: u32 = 1 << 31;

/// How the local view is presented instead of predicted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interpolation {
    /// The view takes the server's origin; the mouse keeps control of the angles.
    KeepAngles,
    /// The view takes the server's origin and angles.
    ServerAngles,
}

/// `cg_noPredict`'s values.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoPredict(pub i64);

fn index(name: &str) -> u16 {
    legacy_animation_index(name).map_or(u16::MAX, |index| index as u16)
}

fn is_one_of(animation: u16, names: &[&str]) -> bool {
    names.iter().any(|name| index(name) == animation)
}

/// `CG_InKnockDownState`: from the moment of the knockdown until the get-up has played
/// out, on either channel, including JA+'s kick falls.
fn in_knockdown_state(player: &PlayerState) -> bool {
    const FALLS: [&str; 3] = [
        "BOTH_BACK_FALLING",
        "BOTH_BACK_FALLING_GETUP",
        "BOTH_BACK_FALLING_GETUP_SLOW",
    ];
    let (legs, torso) = (player.leg_animation(), player.torso_animation());
    player.force_hand_extend() == HANDEXTEND_KNOCKDOWN
        || in_knockdown(legs)
        || (in_knockdown(torso) && player.torso_timer() > 0)
        || is_one_of(legs, &FALLS)
        || is_one_of(torso, &FALLS)
}

/// `CG_JAPlusViewLockedState`: JA+ rewrites `delta_angles` every server frame while the
/// player is kicked down, getting up, kissing or hanging from a ledge.
pub fn japlus_view_locked(player: &PlayerState) -> bool {
    let (legs, torso) = (player.leg_animation(), player.torso_animation());
    let backflip_victim = index("BOTH_JUMP_BACKFLIP_ATCKEE");
    if legs == backflip_victim
        || torso == backflip_victim
        || torso == index("BOTH_GETUP1")
        || torso == index("BOTH_NEW_STABEE")
    {
        return true;
    }
    if legs >= index("BOTH_KISSEE") && legs <= index("BOTH_LEDGE_MERCPULL") {
        return true;
    }
    in_knockdown_state(player)
}

/// `CG_InJAPlusSpecialKickState`: the victim of JA+'s added side and back kicks.
pub fn japlus_special_kick(player: &PlayerState) -> bool {
    const KICK_ANIMS: [&str; 5] = [
        "BOTH_BACK_FALLING",
        "BOTH_BACK_FALLING_GETUP",
        "BOTH_BACK_FALLING_GETUP_SLOW",
        "BOTH_JUMP_BACKFLIP_ATCKEE",
        "BOTH_JUMP_BACKFLIP_ATCKEE_FALL",
    ];
    KICK_DODGE_ANIMS.contains(&player.raw_field(89).unwrap_or(0))
        || is_one_of(player.leg_animation(), &KICK_ANIMS)
        || is_one_of(player.torso_animation(), &KICK_ANIMS)
}

/// The mask for a player the server walks through others: `CONTENTS_BODY` and
/// `CONTENTS_PLAYERCLIP` leave the pmove's trace mask (`cg_predict.c:1299-1313`).
pub fn passes_through_players(player_known_powers: u32) -> bool {
    player_known_powers & GHOST_KNOWN_FLAG != 0
}

/// Whether the local view is the server's this frame, and how. `None` predicts.
///
/// `following` is `PMF_FOLLOW`; `synchronous` is `g_synchronousClients`; `japlus` tells
/// whether the server is JA+, asked only when the player's state makes it matter.
pub fn interpolation(
    player: &PlayerState,
    no_predict: NoPredict,
    following: bool,
    synchronous: bool,
    japlus: impl FnOnce() -> bool,
) -> Option<Interpolation> {
    if following || no_predict.0 == 2 {
        return Some(Interpolation::ServerAngles);
    }
    if no_predict.0 != 0 || synchronous {
        // While JA+ holds the view locked it rewrites `delta_angles` each server frame, and
        // re-applying the live mouse on top of a stale lock snaps the view back with every
        // snapshot: take the server's angles for that window.
        return Some(
            if no_predict.0 == 1 && japlus_view_locked(player) && japlus() {
                Interpolation::ServerAngles
            } else {
                Interpolation::KeepAngles
            },
        );
    }
    (japlus_special_kick(player) && japlus()).then_some(Interpolation::ServerAngles)
}

/// `CG_FakeNoclip_f`'s and `CG_PredictPlayerState`'s gate: only a live player on foot.
pub fn fake_noclip_allowed(player: &PlayerState, following: bool) -> bool {
    const PM_DEAD: u8 = 5;
    !(following
        || player.is_spectator()
        || player.movement_type() == PM_DEAD
        || player.movement_type() == crate::PM_INTERMISSION
        || player.vehicle_entity_num() != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player_with(fields: &[(usize, u32)]) -> PlayerState {
        let mut player = PlayerState::default();
        for &(field, value) in fields {
            player.set_raw_field(field, value);
        }
        player
    }

    fn animation(name: &str) -> u32 {
        legacy_animation_index(name).expect("known animation") as u32
    }

    #[test]
    fn ghost_flag_is_the_top_known_power_bit() {
        assert!(passes_through_players(1 << 31));
        assert!(!passes_through_players((1 << 18) - 1));
    }

    #[test]
    fn predicting_is_the_default() {
        let player = PlayerState::default();
        assert_eq!(
            interpolation(&player, NoPredict(0), false, false, || true),
            None
        );
    }

    #[test]
    fn no_predict_two_and_following_take_the_server_angles() {
        let player = PlayerState::default();
        let server = Some(Interpolation::ServerAngles);
        assert_eq!(
            interpolation(&player, NoPredict(2), false, false, || false),
            server
        );
        assert_eq!(
            interpolation(&player, NoPredict(0), true, false, || false),
            server
        );
    }

    #[test]
    fn no_predict_one_and_synchronous_keep_the_mouse() {
        let player = PlayerState::default();
        let keep = Some(Interpolation::KeepAngles);
        assert_eq!(
            interpolation(&player, NoPredict(1), false, false, || true),
            keep
        );
        assert_eq!(
            interpolation(&player, NoPredict(0), false, true, || true),
            keep
        );
    }

    #[test]
    fn japlus_view_lock_takes_the_server_angles_only_for_no_predict_one() {
        let player = player_with(&[(13, animation("BOTH_JUMP_BACKFLIP_ATCKEE"))]);
        assert!(japlus_view_locked(&player));
        assert_eq!(
            interpolation(&player, NoPredict(1), false, false, || true),
            Some(Interpolation::ServerAngles)
        );
        assert_eq!(
            interpolation(&player, NoPredict(1), false, false, || false),
            Some(Interpolation::KeepAngles)
        );
        assert_eq!(
            interpolation(&player, NoPredict(0), false, true, || true),
            Some(Interpolation::KeepAngles)
        );
    }

    #[test]
    fn a_japlus_kick_victim_is_not_predicted() {
        let down = player_with(&[(13, animation("BOTH_BACK_FALLING"))]);
        assert!(japlus_special_kick(&down));
        assert_eq!(
            interpolation(&down, NoPredict(0), false, false, || true),
            Some(Interpolation::ServerAngles)
        );
        assert_eq!(
            interpolation(&down, NoPredict(0), false, false, || false),
            None
        );
        let marked = player_with(&[(89, 4)]);
        assert!(japlus_special_kick(&marked));
        assert!(!japlus_special_kick(&PlayerState::default()));
    }

    #[test]
    fn fake_noclip_needs_a_live_player_on_foot() {
        assert!(fake_noclip_allowed(&PlayerState::default(), false));
        assert!(!fake_noclip_allowed(&PlayerState::default(), true));
        assert!(!fake_noclip_allowed(&player_with(&[(84, 7)]), false));
    }
}

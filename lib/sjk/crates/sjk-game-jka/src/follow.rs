//! Spectators following players: `Cmd_FollowCycle_f`'s choice, `StopFollowing`, and what
//! `SpectatorClientEndFrame` shows a follower (OpenJK `codemp/game/g_cmds.c:935-967,
//! 1365-1497`, `g_active.c:3654-3701`). The game keeps who follows whom in the session
//! ([`crate::client_begin::PlayerSession::spectator_client`]) and runs the commands.

use crate::disruptor::EF_DISINTEGRATION;
use sjk_protocol::PlayerState;

/// `PMF_FOLLOW`, `PMF_SCOREBOARD`.
pub const PMF_FOLLOW: u16 = 0x1000;
pub const PMF_SCOREBOARD: u16 = 0x2000;
/// `PM_SPECTATOR`.
const PM_SPECTATOR: u8 = 4;
/// Player-state netfields `StopFollowing` resets (`msg.cpp` `playerStateFields`).
const PS_EFLAGS: usize = 17;
const PS_WEAPON: usize = 47;
const PS_VIEWANGLES_ROLL: usize = 50;
const PS_VEHICLE: usize = 84;
const PS_EMPLACED: usize = 112;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_ZOOM_MODE: usize = 90;
const PS_ZOOM_LOCKED: usize = 94;
const PS_SABER_MOVE: usize = 34;
const PS_LEGS_ANIM: usize = 13;
const PS_LEGS_TIMER: usize = 21;
const PS_TORSO_ANIM: usize = 15;
const PS_TORSO_TIMER: usize = 20;
const PS_JEDI_MASTER: usize = 114;
const PS_CLOAK_FUEL: usize = 40;
const PS_JETPACK_FUEL: usize = 39;
const PS_BOB_CYCLE: usize = 9;

/// `Cmd_FollowCycle_f`'s walk from `current` by `dir` (1 or -1) through `slots` slots,
/// wrapping once: the first slot `followable` accepts (connected, not a spectator), or
/// `None` to leave it where it was. `current` may be -1 or -2 (`follow1`, `follow2`).
pub fn cycle(
    current: i32,
    dir: i32,
    slots: usize,
    followable: impl Fn(usize) -> bool,
) -> Option<usize> {
    let (mut client, original, mut looped) = (current, current, false);
    let slots = slots as i32;
    loop {
        client += dir;
        if client >= slots {
            if looped {
                return None;
            }
            (client, looped) = (0, true);
        }
        if client < 0 {
            if looped {
                return None;
            }
            (client, looped) = (slots - 1, true);
        }
        if followable(client as usize) {
            return Some(client as usize);
        }
        if client == original {
            return None;
        }
    }
}

/// `StopFollowing`'s state: its own client number again, flying free, no weapon, no
/// animation, no powerups, the fuel full, `PMF_FOLLOW` gone. The session's team and
/// state, the entity's health (100) and `forceHandExtendTime` are the caller's.
pub fn stop_following(state: &mut PlayerState, client: u16) {
    state.set_movement_flags(state.movement_flags() & !PMF_FOLLOW);
    state.set_client_num(client);
    for (index, value) in [
        (PS_WEAPON, 0),
        (PS_VEHICLE, 0),
        (PS_EMPLACED, 0),
        (PS_VIEWANGLES_ROLL, 0),
        (PS_FORCE_HAND_EXTEND, 0),
        (PS_ZOOM_MODE, 0),
        (PS_ZOOM_LOCKED, 0),
        (PS_SABER_MOVE, 0),
        (PS_LEGS_ANIM, 0),
        (PS_LEGS_TIMER, 0),
        (PS_TORSO_ANIM, 0),
        (PS_TORSO_TIMER, 0),
        (PS_JEDI_MASTER, 0),
        (PS_CLOAK_FUEL, 100),
        (PS_JETPACK_FUEL, 100),
        (PS_BOB_CYCLE, 0),
    ] {
        state.set_raw_field(index, value);
    }
    state.stats[0] = 100;
    state.set_movement_type(PM_SPECTATOR);
    let flags = state.raw_field(PS_EFLAGS).unwrap_or(0);
    state.set_raw_field(PS_EFLAGS, flags & !EF_DISINTEGRATION);
    state.powerups = [0; 16];
}

/// `SpectatorClientEndFrame`'s copy: the follower is shown the followed player's own
/// state, flagged as a follower's.
pub fn followed_state(followed: &PlayerState) -> PlayerState {
    let mut state = followed.clone();
    state.set_movement_flags(state.movement_flags() | PMF_FOLLOW);
    state
}

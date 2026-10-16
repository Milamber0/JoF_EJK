//! `G_CheckClientIdle` (OpenJK `codemp/game/g_active.c:1262-1381`): a living player that
//! stands still, presses nothing, keeps its view and is left alone for five seconds
//! plays its stance's idle animation, and does not again until that has played out and
//! up to two more seconds have passed (the game's generator); anything at all — a move,
//! a button, a hurt, a turn of more than ten degrees, a weapon busy or changing — breaks
//! an idle that is playing and starts the five seconds over. `ClientThink_real` runs it
//! after the once-a-second actions.

use crate::player_death::Rng;
use crate::pmove::Predictor;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `BOTH_STAND1` and its idle, `BOTH_STAND2` and its two, `BOTH_STAND3`, `BOTH_STAND4`,
/// `BOTH_STAND5` and theirs, `TORSO_RAISEWEAP1`.
const BOTH_STAND1: u16 = 915;
const BOTH_STAND1IDLE1: u16 = 916;
const BOTH_STAND2: u16 = 917;
const BOTH_STAND2IDLE1: u16 = 918;
const BOTH_STAND2IDLE2: u16 = 919;
const BOTH_STAND3: u16 = 920;
const BOTH_STAND3IDLE1: u16 = 921;
const BOTH_STAND4: u16 = 922;
const BOTH_STAND5: u16 = 923;
const BOTH_STAND5IDLE1: u16 = 924;
const TORSO_RAISEWEAP1: u16 = 1_398;
/// `WEAPON_READY`, `WEAPON_CHARGING`, `WEAPON_CHARGING_ALT`.
const WEAPON_READY: u8 = 0;
const WEAPON_CHARGING: u8 = 4;
const WEAPON_CHARGING_ALT: u8 = 5;
/// `WP_SABER`, `WP_MELEE`.
const WP_SABER: u8 = 3;
const WP_MELEE: u8 = 2;
/// `LS_READY`.
const LS_READY: u32 = 1;
/// The buttons `G_ActionButtonPressed` counts: attack, use holdable, gesture, use, grip,
/// alternate attack, force power, lightning, drain.
const ACTION_BUTTONS: u16 = 1 | 4 | 8 | 32 | 64 | 128 | 256 | 512 | 1_024;

/// A player's idle memory: `idleTime`, `idleHealth`, `idleViewAngles`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Idle {
    pub time: i32,
    pub health: i32,
    pub view_angles: [f32; 3],
}

/// `G_StandingAnim`: the four plain stances (no idle, no cinematic stand).
fn standing(animation: u16) -> bool {
    matches!(
        animation,
        BOTH_STAND1 | BOTH_STAND2 | BOTH_STAND3 | BOTH_STAND4
    )
}

/// The idle animations, which a break ends.
fn is_idle(animation: u16) -> bool {
    matches!(
        animation,
        BOTH_STAND1IDLE1
            | BOTH_STAND2IDLE1
            | BOTH_STAND2IDLE2
            | BOTH_STAND3IDLE1
            | BOTH_STAND5IDLE1
    )
}

/// The check after a think, on the movement's state (the wire state is written from it
/// after): `health` is the entity's, `spectating` the session's team; `command` the
/// think's. Returns whether the animation changed (an idle begun or broken), for the
/// caller to write the wire state again.
pub fn check_idle(
    idle: &mut Idle,
    movement: &mut Predictor,
    health: i32,
    armor: i32,
    spectating: bool,
    command: &UserCommand,
    level_time: i32,
    rng: &mut Rng,
) -> bool {
    let state = movement.state();
    if health <= 0
        || state.health <= 0
        || spectating
        || u32::from(state.movement_flags) & PMF_FOLLOW != 0
    {
        return false;
    }
    let action = command.buttons & ACTION_BUTTONS != 0;
    let moving = command.forward_move != 0 || command.right_move != 0 || command.up_move != 0;
    let view_change: [f32; 3] =
        std::array::from_fn(|axis| state.view_angles[axis] - idle.view_angles[axis]);
    let turned = (view_change[0] * view_change[0]
        + view_change[1] * view_change[1]
        + view_change[2] * view_change[2])
        .sqrt()
        > 10.0;
    let total = health + armor;
    let charging = matches!(state.weapon_state, WEAPON_CHARGING | WEAPON_CHARGING_ALT);
    let busy = state.weapon_state != WEAPON_READY && state.weapon != WP_SABER;
    let changing = state.weapon != command.weapon;
    let still_velocity = state.velocity == [0.0; 3];
    // `ps.saberBlocking` is a blocking mode, which the reference compares with the time
    // all the same: never past it.
    let hands = state.force_hand_extend != 0
        || state.saber_blocked != 0
        || i32::from(state.saber_blocking) >= level_time
        || state.weapon == WP_MELEE;
    let active = !still_velocity
        || action
        || moving
        || !standing(state.legs_anim)
        || total != idle.health
        || turned
        || state.legs_timer > 0
        || state.torso_timer > 0
        || state.weapon_time > 0
        || charging
        || state.zoom_mode != 0
        || busy
        || hands
        || changing;
    if active {
        // Anything but a stance or a timer breaks an idle that is playing.
        let breaks = !still_velocity
            || action
            || moving
            || total != idle.health
            || state.zoom_mode != 0
            || busy
            || (state.weapon_time > 0 && state.weapon == WP_SABER)
            || charging
            || hands
            || changing;
        let mut broke_out = false;
        if breaks {
            broke_out = movement.break_idle(
                is_idle(state.legs_anim),
                is_idle(state.torso_anim),
                LS_READY,
            );
        }
        idle.health = total;
        idle.view_angles = movement.state().view_angles;
        if idle.time < level_time {
            idle.time = level_time;
        }
        if broke_out && charging {
            movement.set_torso_anim_raw(TORSO_RAISEWEAP1);
        }
        return broke_out;
    }
    if level_time - idle.time > 5_000 {
        let mut animation = match state.legs_anim {
            BOTH_STAND1 => Some(BOTH_STAND1IDLE1),
            BOTH_STAND2 => Some(BOTH_STAND2IDLE1),
            BOTH_STAND3 => Some(BOTH_STAND3IDLE1),
            BOTH_STAND5 => Some(BOTH_STAND5IDLE1),
            _ => None,
        };
        if animation == Some(BOTH_STAND2IDLE1) && rng.irand(1, 10) <= 5 {
            animation = Some(BOTH_STAND2IDLE2);
        }
        if let Some(animation) = animation {
            movement.set_animation_parts(
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            idle.time = level_time + movement.state().legs_timer + rng.irand(0, 2_000);
            return true;
        }
    }
    false
}

/// `PMF_FOLLOW`.
const PMF_FOLLOW: u32 = 4_096;

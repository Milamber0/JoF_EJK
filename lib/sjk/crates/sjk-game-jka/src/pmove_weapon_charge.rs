//! MP gun charging and zoom, OpenJK codemp/game/bg_pmove.c (not the SP path).

use crate::pmove::MovementState;
use crate::predicted_events::{PredictedEvent, PredictedEvents};
use sjk_protocol::UserCommand;

/// `weaponstate_t`, codemp/game/bg_public.h:442-443.
pub(crate) const CHARGING: u8 = 4;
pub(crate) const CHARGING_ALT: u8 = 5;
/// `EV_WEAPON_CHARGE`, `EV_WEAPON_CHARGE_ALT`.
const EV_WEAPON_CHARGE: u16 = 108;
const EV_WEAPON_CHARGE_ALT: u16 = 109;
const FIRING: u32 = 1 << 9;
const ATTACK: u16 = 1;
const ALT: u16 = 128;
const ALT_FIRING: u32 = 1 << 10;

/// Append through the existing predictable-event sequence, never an audio side channel.
pub(crate) fn event(state: &mut MovementState, events: &mut PredictedEvents, kind: u16, parm: u16) {
    events.push(PredictedEvent {
        weapon: state.weapon,
        zoom_mode: state.zoom_mode,
        sequence: state.event_sequence,
        event: kind,
        parameter: parm,
        command_time: state.command_time,
        client: state.client_num,
        entity_flags: state.entity_flags,
        origin: state.origin,
    });
    state.event_sequence = state.event_sequence.wrapping_add(1);
}

/// PmoveSingle cancellation/jump suppression (:10444-10464) and :10503-10513 latch.
pub(crate) fn prepare(
    state: &mut MovementState,
    cmd: &mut UserCommand,
    _previous_time: i32,
    events: &mut PredictedEvents,
) -> bool {
    if state.weapon != 6 {
        return false;
    }
    if state.weapon_state == CHARGING_ALT {
        if moving(cmd) {
            state.weapon_state = 0;
            state.weapon_time = 1000;
            event(state, events, 108, 6);
            cmd.up_move = 0;
        }
    } else if state.zoom_mode == 1 && cmd.up_move > 0 {
        cmd.up_move = 0;
    }
    state.zoom_mode == 1 && state.zoom_locked && cmd.buttons & (ATTACK | ALT) == ALT
}

/// PM_AdjustAttackStates zoom edges (:8072-8117); previous commandTime is intentional.
pub(crate) fn adjust_zoom(
    state: &mut MovementState,
    cmd: &UserCommand,
    previous_time: i32,
    events: &mut PredictedEvents,
) {
    if state.weapon != 6 || state.weapon_state != 0 {
        return;
    }
    if state.entity_flags & ALT_FIRING == 0 && cmd.buttons & ALT != 0 {
        if state.zoom_mode == 0 && state.movement_type != 4 {
            state.zoom_mode = 1;
            state.zoom_locked = false;
            state.zoom_fov = 80.0;
            state.zoom_lock_time = cmd.server_time + 50;
            event(state, events, 39, 0);
        } else if state.zoom_mode == 1 && state.zoom_lock_time < cmd.server_time {
            state.zoom_mode = 0;
            state.zoom_time = previous_time;
            state.zoom_locked = false;
            state.weapon_time = 1000;
            event(state, events, 39, 0);
        }
    } else if cmd.buttons & ALT == 0
        && state.zoom_lock_time < cmd.server_time
        && state.zoom_mode != 0
    {
        if state.zoom_mode == 1 && !state.zoom_locked {
            state.zoom_fov =
                (((cmd.server_time + 50 - state.zoom_lock_time) as f32) * 0.035).clamp(1.0, 50.0);
        }
        state.zoom_locked = true;
    }
}

/// Rewrite only the simulation command (:8200-8212), never the transmitted usercmd.
pub(crate) fn convert_attack(state: &mut MovementState, cmd: &mut UserCommand) {
    if state.weapon == 6 && state.zoom_mode == 1 && state.zoom_locked {
        if cmd.buttons & ATTACK != 0 {
            cmd.buttons |= ALT;
            state.entity_flags |= ALT_FIRING;
        } else if cmd.buttons & ALT != 0 {
            cmd.buttons &= !ALT;
            state.entity_flags &= !ALT_FIRING;
        }
    }
}

/// PM_Weapon's scoped movement/cancel gate (:7221-7239).
pub(crate) fn blocks_fire(
    state: &mut MovementState,
    cmd: &UserCommand,
    cancel: bool,
    events: &mut PredictedEvents,
) -> bool {
    if state.weapon != 6 || state.zoom_mode != 1 {
        return false;
    }
    if cancel {
        state.zoom_mode = 0;
        state.zoom_fov = 0.0;
        state.zoom_locked = false;
        state.zoom_lock_time = 0;
        event(state, events, 39, 0);
        return true;
    }
    moving(cmd)
}

fn moving(cmd: &UserCommand) -> bool {
    cmd.forward_move != 0 || cmd.right_move != 0 || cmd.up_move > 0
}

/// `PM_RocketLock(2048, qfalse)` (`bg_pmove.c:5900-5998`): a trace from the muzzle
/// 2048 units along the view through the players (`MASK_PLAYERSOLID`, the player itself
/// passed); a player struck is locked on — at once when nothing was, or once the
/// target's time is past when another was; the lock's time kept, and its target time
/// pushed 500 ms on while the lock holds; struck nothing, the lock goes once its target
/// time is past, and meanwhile its time is remembered and set to -1. Entities that are
/// not players (a rocket, a husk) are no target; cloaking is not ported.
fn rocket_lock(
    state: &mut MovementState,
    cmd: &UserCommand,
    collision: &dyn crate::pmove::MovementCollision,
) {
    const LOCK_DISTANCE: f32 = 2_048.0;
    const MASK_PLAYERSOLID: u32 = 0x1 | 0x100 | 0x10000;
    const MUZZLE: [f32; 3] = [12.0, 8.0, -4.0];
    const ENTITY_NONE: u16 = 1_023;
    let (forward, right) = crate::pmove::flight::flight_axes(state.view_angles);
    let (forward, right) = (forward.to_array(), right.to_array());
    let mut muzzle: [f32; 3] = std::array::from_fn(|axis| {
        state.origin[axis] + MUZZLE[0] * forward[axis] + MUZZLE[1] * right[axis]
    });
    muzzle[2] += state.view_height as f32 + MUZZLE[2];
    let end: [f32; 3] = std::array::from_fn(|axis| muzzle[axis] + forward[axis] * LOCK_DISTANCE);
    let trace = collision.trace(muzzle, [0.0; 3], [0.0; 3], end, MASK_PLAYERSOLID);
    let struck = trace.entity_number;
    if trace.fraction != 1.0 && struck < ENTITY_NONE && struck != state.client_num {
        if struck < 32 {
            if state.rocket_lock_index == ENTITY_NONE {
                state.rocket_lock_index = struck;
                state.rocket_lock_time = cmd.server_time as f32;
            } else if state.rocket_lock_index != struck
                && state.rocket_target_time < cmd.server_time as f32
            {
                state.rocket_lock_index = struck;
                state.rocket_lock_time = cmd.server_time as f32;
            } else if state.rocket_lock_index == struck && state.rocket_lock_time == -1.0 {
                state.rocket_lock_time = state.rocket_last_valid_time;
            }
            if state.rocket_lock_index == struck {
                state.rocket_target_time = (cmd.server_time + 500) as f32;
            }
        } else if state.rocket_target_time < cmd.server_time as f32 {
            state.rocket_lock_index = ENTITY_NONE;
            state.rocket_lock_time = 0.0;
        }
    } else if state.rocket_target_time < cmd.server_time as f32 {
        state.rocket_lock_index = ENTITY_NONE;
        state.rocket_lock_time = 0.0;
    } else {
        if state.rocket_lock_time != -1.0 {
            state.rocket_last_valid_time = state.rocket_lock_time;
        }
        state.rocket_lock_time = -1.0;
    }
}

/// PM_DoChargedWeapons (:6044-6060,6111-6257). True short-circuits PM_Weapon. The
/// pistol's alternate, the disruptor's zoomed primary and the DEMP2's alternate charge
/// as alternate fires (`WEAPON_CHARGING_ALT`); the bowcaster's primary charges as a
/// primary (`WEAPON_CHARGING`, `bg_weapons.c:206-222`: five of ammo every 400 ms for
/// 1700 ms at most).
pub(crate) fn charge(
    state: &mut MovementState,
    cmd: &mut UserCommand,
    events: &mut PredictedEvents,
    collision: Option<&dyn crate::pmove::MovementCollision>,
) -> bool {
    // (interval, subtract, maximum, alternate)
    let (interval, subtract, maximum, alternate) = match state.weapon {
        // bg_weapons.c:121-137,155-171,206-222. Retail MP Bryar charge is free.
        4 => (0, 0, 0, true),
        // The old Bryar's alternate charges and costs (bg_weapons.c: WP_BRYAR_OLD, one
        // every 200 ms for 1500 ms at most).
        16 => (200, 1, 1500, true),
        6 => (200, 3, 1700, true),
        7 => (400, 5, 1700, false),
        9 => (250, 3, 2100, true),
        // The rocket launcher's alternate is not a charge but a lock, held until the
        // button comes up (bg_weapons.c:240-256: two of ammo, no drain).
        11 => (0, 0, 0, true),
        // The thermal detonator charges on either button, the alternate one winning
        // (bg_pmove.c:6097-6108), and its charge is never capped (:6733-6757).
        12 => (0, 0, 0, cmd.buttons & ALT != 0),
        _ => return false,
    };
    let mut charging = if state.weapon == 12 {
        cmd.buttons & (ATTACK | ALT) != 0
    } else if state.weapon == 6 {
        cmd.buttons & ATTACK != 0 && state.zoom_mode == 1 && state.zoom_locked && !moving(cmd)
    } else if state.weapon == 11 {
        let data = crate::weapon_data::legacy_weapon_data(11).unwrap();
        let locking = cmd.buttons & ALT != 0 && state.ammo[data.ammo_index] >= data.alternate_cost;
        if locking && let Some(collision) = collision {
            rocket_lock(state, cmd, collision);
        }
        locking
    } else if alternate {
        cmd.buttons & ALT != 0
    } else {
        cmd.buttons & ATTACK != 0
    };
    if state.weapon == 6 && state.zoom_mode != 1 && state.weapon_state == CHARGING_ALT {
        state.weapon_state = 0;
        charging = false;
    }
    let (charging_state, charge_event) = if alternate {
        (CHARGING_ALT, EV_WEAPON_CHARGE_ALT)
    } else {
        (CHARGING, EV_WEAPON_CHARGE)
    };
    if charging {
        if state.weapon_state != charging_state {
            state.weapon_state = charging_state;
            state.weapon_charge_time = cmd.server_time;
            state.weapon_charge_subtract_time = cmd.server_time + interval;
            event(state, events, charge_event, u16::from(state.weapon));
        }
        let data = crate::weapon_data::legacy_weapon_data(state.weapon).unwrap();
        let cost = if alternate {
            data.alternate_cost
        } else {
            data.primary_cost
        };
        if state.ammo[data.ammo_index] >= subtract + cost {
            if cmd.server_time - state.weapon_charge_time < maximum
                && state.weapon_charge_subtract_time < cmd.server_time
            {
                state.ammo[data.ammo_index] -= subtract;
                state.weapon_charge_subtract_time = cmd.server_time + interval;
            }
            return true;
        }
        // Low ammo takes the stock `goto rest`, including the -1 ammo sentinel.
    }
    if state.weapon_state == CHARGING {
        cmd.buttons |= ATTACK;
        state.entity_flags |= FIRING;
    } else if state.weapon_state == CHARGING_ALT {
        cmd.buttons |= ALT;
        state.entity_flags |= FIRING | ALT_FIRING;
    }
    false
}

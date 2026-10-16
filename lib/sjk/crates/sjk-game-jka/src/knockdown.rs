//! The knockdown, server side: `G_Knockdown` (OpenJK `codemp/game/g_combat.c:4383-4392`)
//! and the concussion rifle's own (`g_weapon.c:3220-3259`), which put a player in
//! `HANDEXTEND_KNOCKDOWN` for the pmove to hold the fall, and `WP_ForcePowersUpdate`'s
//! part (`w_force.c:5045-5127`), run for every playing client each frame, which keeps a
//! knocked-down player's weapon and saber still and, once the knockdown's time is past,
//! gets it up — by a roll on a direction held (`G_SpecialRollGetup`), by the Force on a
//! jump, or on its own after four seconds of lying — and hands the weapon back
//! (`HANDEXTEND_WEAPONREADY`).

use crate::event_entity::EventEntity;
use crate::pmove_hand_extend::{HANDEXTEND_KNOCKDOWN, HANDEXTEND_NONE, HANDEXTEND_WEAPONREADY};
use sjk_protocol::{PlayerState, UserCommand};

/// `BOTH_GETUP_BROLL_B/F/L/R`.
use crate::pmove_hand_extend::{BOTH_KNOCKDOWN1, BOTH_KNOCKDOWN5};
const BOTH_GETUP_BROLL_B: u16 = 1_239;
const BOTH_GETUP_BROLL_F: u16 = 1_240;
const BOTH_GETUP_BROLL_L: u16 = 1_241;
const BOTH_GETUP_BROLL_R: u16 = 1_242;
/// `EV_PREDEFSOUND`, `PDSOUND_FORCEJUMP`; `EV_ENTITY_SOUND`, `CHAN_VOICE`.
const EV_PREDEFSOUND: u32 = 40;
const PDSOUND_FORCEJUMP: u32 = 5;
pub const EV_ENTITY_SOUND: u32 = 79;
const CHAN_VOICE: u32 = 3;
/// The wire fields: `zoomMode`, `zoomTime`, `zoomLocked`, `zoomFov`, `forceHandExtend`,
/// `forceDodgeAnim`, `fd.forcePowerLevel[FP_LEVITATION]`, `eFlags`; the entity's
/// `origin`, `clientNum`, `trickedentindex`.
const PS_ZOOM_MODE: usize = 90;
const PS_ZOOM_TIME: usize = 92;
const PS_ZOOM_LOCKED: usize = 94;
const PS_ZOOM_FOV: usize = 95;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_FORCE_DODGE_ANIM: usize = 89;
const PS_LEVITATION_LEVEL: usize = 52;
const PS_EFLAGS: usize = 17;
const EF_DEAD: u32 = 1;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_CLIENT: usize = 32;
const ES_TRICKED: usize = 58;
/// `saberMove`, `saberBlocked`, `weaponTime`, `weaponstate`: stilled while down.
const PS_SABER_MOVE: usize = 34;
const PS_SABER_BLOCKED: usize = 77;
const PS_WEAPON_TIME: usize = 10;
const PS_WEAPON_STATE: usize = 33;

/// A player's knockdown memory: `forceHandExtendTime`, `quickerGetup`, `otherKiller`
/// with its times — the game's, not the wire's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Knockdown {
    pub hand_extend_time: i32,
    pub quicker_getup: bool,
}

/// `BG_InKnockDownOnly`: the legs in one of the five knockdown animations, lying —
/// a landing then costs the whole fall.
pub fn in_knockdown_only(legs_anim: u16) -> bool {
    (BOTH_KNOCKDOWN1..=BOTH_KNOCKDOWN5).contains(&legs_anim)
}

/// `G_InGetUpAnim`'s animations: the get-ups, plain, Force and rolling.
pub fn in_get_up(animation: u16) -> bool {
    matches!(
        crate::legacy_animation_name(usize::from(animation)),
        Some(
            "BOTH_GETUP1"
                | "BOTH_GETUP2"
                | "BOTH_GETUP3"
                | "BOTH_GETUP4"
                | "BOTH_GETUP5"
                | "BOTH_FORCE_GETUP_F1"
                | "BOTH_FORCE_GETUP_F2"
                | "BOTH_FORCE_GETUP_B1"
                | "BOTH_FORCE_GETUP_B2"
                | "BOTH_FORCE_GETUP_B3"
                | "BOTH_FORCE_GETUP_B4"
                | "BOTH_FORCE_GETUP_B5"
                | "BOTH_GETUP_BROLL_B"
                | "BOTH_GETUP_BROLL_F"
                | "BOTH_GETUP_BROLL_L"
                | "BOTH_GETUP_BROLL_R"
                | "BOTH_GETUP_FROLL_B"
                | "BOTH_GETUP_FROLL_F"
                | "BOTH_GETUP_FROLL_L"
                | "BOTH_GETUP_FROLL_R"
        )
    )
}

/// `BG_InKnockDown`: lying, or getting up.
pub fn in_knockdown(animation: u16) -> bool {
    in_knockdown_only(animation) || in_get_up(animation)
}

/// `BG_KnockDownable`: not on a vehicle, not at an emplaced gun.
pub fn knockdownable(state: &PlayerState) -> bool {
    state.raw_field(84).unwrap_or(0) == 0 && state.raw_field(112).unwrap_or(0) == 0
}

/// `G_Knockdown`: down for 1100 ms, the plain get-up, no quicker one.
pub fn knock_down(state: &mut PlayerState, memory: &mut Knockdown, level_time: i32) {
    if !knockdownable(state) {
        return;
    }
    state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_KNOCKDOWN));
    state.set_raw_field(PS_FORCE_DODGE_ANIM, 0);
    memory.hand_extend_time = level_time + 1_100;
    memory.quicker_getup = false;
}

/// `PM_VehicleImpact`'s knockdown of a player a vehicle rammed (`bg_slidemove.c:522-528`):
/// down for 1100 ms from the ramming command unless it is down already.
pub fn rammed(state: &mut PlayerState, memory: &mut Knockdown, command_time: i32) {
    if state.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) as u8 != HANDEXTEND_KNOCKDOWN {
        state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_KNOCKDOWN));
        memory.hand_extend_time = command_time + 1_100;
        state.set_raw_field(PS_FORCE_DODGE_ANIM, 0);
    }
}

/// What the frame's update did, for the caller to restart the movement from the wire
/// state (`changed`), then set the roll-up's animation on it (`animation`, both halves,
/// overriding and held) and write the wire state from it again, and to raise.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Updated {
    /// A roll-up's animation (`G_SetAnim`, both halves, overriding and held).
    pub animation: Option<u16>,
    /// The wire state changed (a get-up begun, the pose ended, the weapon stilled).
    pub changed: bool,
    /// Down: `ps.saberBlocking` (the blocking mode, not on the wire) is zeroed too.
    pub blocking_cleared: bool,
    /// `G_PreDefSound`, `G_EntitySound`: the temp entities to spawn, in order.
    pub events: Vec<EventEntity>,
    /// `G_EntitySound`'s `*jump1.wav`, to register on the caller's table before its
    /// event is raised (the index goes in the event's parameter).
    pub jump_sound: bool,
}

/// `WP_ForcePowersUpdate`'s knockdown part for a playing client at `level_time`, with
/// its last command (`pers.cmd`). A jump sound the caller registers is patched into the
/// event by [`Updated::jump_sound`].
pub fn force_update(
    state: &mut PlayerState,
    memory: &mut Knockdown,
    health: i32,
    command: &UserCommand,
    level_time: i32,
) -> Updated {
    let mut updated = Updated::default();
    let extend = state.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) as u8;
    if extend == HANDEXTEND_KNOCKDOWN {
        // No scope while down.
        for (field, value) in [
            (PS_ZOOM_FOV, 0),
            (PS_ZOOM_MODE, 0),
            (PS_ZOOM_LOCKED, 0),
            (PS_ZOOM_TIME, 0),
        ] {
            if state.raw_field(field) != Some(value) {
                state.set_raw_field(field, value);
                updated.changed = true;
            }
        }
    }
    if extend == HANDEXTEND_KNOCKDOWN && memory.hand_extend_time >= level_time {
        // Down: the saber's move, blocking mode and block cleared, the weapon at rest
        // and ready.
        let stilled = [
            (PS_SABER_MOVE, 0),
            (PS_SABER_BLOCKED, 0),
            (PS_WEAPON_TIME, 0),
            (PS_WEAPON_STATE, 0),
        ];
        for (field, value) in stilled {
            if state.raw_field(field) != Some(value) {
                state.set_raw_field(field, value);
                updated.changed = true;
            }
        }
        updated.blocking_cleared = true;
        return updated;
    }
    if extend != HANDEXTEND_NONE && memory.hand_extend_time < level_time {
        let dodge = state.raw_field(PS_FORCE_DODGE_ANIM).unwrap_or(0);
        if extend == HANDEXTEND_KNOCKDOWN && dodge == 0 {
            let dead = health < 1 || state.raw_field(PS_EFLAGS).unwrap_or(0) & EF_DEAD != 0;
            if dead {
                state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_NONE));
                updated.changed = true;
            } else if special_roll_getup(state, memory, command, level_time, &mut updated) {
                state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_NONE));
                updated.changed = true;
            } else if level_time - memory.hand_extend_time > 4_000 {
                // Four seconds of lying: up on its own — with the Force on a jump held
                // (level 2 or more), quicker after a push, else the plain get-up.
                if command.up_move != 0 && state.raw_field(PS_LEVITATION_LEVEL).unwrap_or(0) > 1 {
                    updated
                        .events
                        .push(predef_sound(state.origin(), PDSOUND_FORCEJUMP));
                    state.set_raw_field(PS_FORCE_DODGE_ANIM, 2);
                    memory.hand_extend_time = level_time + 500;
                } else if memory.quicker_getup {
                    updated.jump_sound = true;
                    updated.events.push(entity_sound(
                        state.origin(),
                        state.client_num(),
                        CHAN_VOICE,
                    ));
                    state.set_raw_field(PS_FORCE_DODGE_ANIM, 3);
                    memory.hand_extend_time = level_time + 500;
                    let mut velocity = state.velocity();
                    velocity[2] = 300.0;
                    state.set_velocity(velocity);
                } else {
                    state.set_raw_field(PS_FORCE_DODGE_ANIM, 1);
                    memory.hand_extend_time = level_time + 1_000;
                }
                updated.changed = true;
            }
            memory.quicker_getup = false;
        } else if extend == 14 {
            // `HANDEXTEND_POSTTHROWN`: with the grip's throw.
        } else {
            state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_WEAPONREADY));
            updated.changed = true;
        }
    }
    updated
}

/// `G_SpecialRollGetup` (`w_force.c:4912-4967`): a direction held alone rolls the
/// player up that way (`BOTH_GETUP_BROLL_*`, both halves, held) with the jump sound; a
/// jump held instead is the Force get-up 500 ms on, with its sound. Returns whether it
/// rolled.
fn special_roll_getup(
    state: &mut PlayerState,
    memory: &mut Knockdown,
    command: &UserCommand,
    level_time: i32,
    updated: &mut Updated,
) -> bool {
    let roll = if command.right_move > 0 && command.forward_move == 0 {
        Some(BOTH_GETUP_BROLL_R)
    } else if command.right_move < 0 && command.forward_move == 0 {
        Some(BOTH_GETUP_BROLL_L)
    } else if command.right_move == 0 && command.forward_move > 0 {
        Some(BOTH_GETUP_BROLL_F)
    } else if command.right_move == 0 && command.forward_move < 0 {
        Some(BOTH_GETUP_BROLL_B)
    } else {
        None
    };
    if let Some(animation) = roll {
        updated.animation = Some(animation);
        updated.jump_sound = true;
        updated
            .events
            .push(entity_sound(state.origin(), state.client_num(), CHAN_VOICE));
        return true;
    }
    if command.up_move != 0 {
        updated
            .events
            .push(predef_sound(state.origin(), PDSOUND_FORCEJUMP));
        state.set_raw_field(PS_FORCE_DODGE_ANIM, 2);
        memory.hand_extend_time = level_time + 500;
        updated.changed = true;
    }
    false
}

/// `G_PreDefSound`: the sound's temp entity, its origin unsnapped in `s.origin`.
pub fn predef_sound(origin: [f32; 3], sound: u32) -> EventEntity {
    let mut event = EventEntity {
        event: EV_PREDEFSOUND,
        parameter: sound,
        origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    for axis in 0..3 {
        event.extra[axis] = (ES_ORIGIN[axis], origin[axis].to_bits());
    }
    event
}

/// `G_EntitySound`: the sound's temp entity naming the player and the channel; the
/// sound's index is the caller's to put in the parameter.
pub fn entity_sound(origin: [f32; 3], client: u16, channel: u32) -> EventEntity {
    EventEntity {
        event: EV_ENTITY_SOUND,
        parameter: 0,
        origin,
        client: None,
        broadcast: false,
        extra: [
            (ES_CLIENT, u32::from(client)),
            (ES_TRICKED, channel),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ],
    }
}

/// The concussion rifle's alternate on a client it struck (`g_weapon.c:3220-3259`), after
/// the damage: the push straight along the shot (lifted to 0.2 at least), its strength
/// 500 less the distance between the two, 150 at least — and over 200 (within 300 units)
/// the knockdown for 1100 ms, the shooter remembered as the other killer for five
/// seconds. Nothing for the dead, the unknockdownable (`FL_NO_KNOCKBACK`, a non-humanoid,
/// a vehicle, an emplaced gun) or one already down.
pub fn concussion_shove(
    state: &mut PlayerState,
    memory: &mut Knockdown,
    other_killer: &mut crate::damage::OtherKiller,
    health: i32,
    no_knockback: bool,
    forward: [f32; 3],
    shooter: u16,
    shooter_origin: [f32; 3],
    level_time: i32,
) -> bool {
    if health <= 0 {
        return false;
    }
    let mut push = forward;
    if push[2] < 0.2 {
        push[2] = 0.2;
    }
    let down = state.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) as u8 == HANDEXTEND_KNOCKDOWN;
    if no_knockback || down || !knockdownable(state) {
        return false;
    }
    let origin = state.origin();
    let apart: [f32; 3] = std::array::from_fn(|axis| origin[axis] - shooter_origin[axis]);
    let mut strength =
        500.0 - (apart[0] * apart[0] + apart[1] * apart[1] + apart[2] * apart[2]).sqrt();
    if strength < 150.0 {
        strength = 150.0;
    }
    if strength > 200.0 {
        state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_KNOCKDOWN));
        memory.hand_extend_time = level_time + 1_100;
        state.set_raw_field(PS_FORCE_DODGE_ANIM, 0);
    }
    *other_killer = crate::damage::OtherKiller::credit(shooter, level_time);
    let mut velocity = state.velocity();
    velocity[0] += push[0] * strength;
    velocity[1] += push[1] * strength;
    velocity[2] = strength;
    state.set_velocity(velocity);
    true
}

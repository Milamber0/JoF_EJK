//! Which animation a player on foot is in: standing, walking, running, crouching,
//! jumping, falling and landing, and a gun carrier's ready pose.
//!
//! A client is told all of this by its server and only has to agree; a server has to
//! decide it. Ported from OpenJK `codemp/game/bg_pmove.c` for players — the branches
//! for NPC classes (`clientNum >= MAX_CLIENTS`) are not here. A saber carrier standing on
//! a slope takes `PM_AdjustStandAnimForSlope`'s poses ([`crate::pmove_slope`]).

use crate::pmove::MovementState;
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS,
    continue_legs, force_legs, set_animation, start_torso,
};
use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

pub(crate) const BOTH_STAND1: u16 = 915;
const BOTH_STAND2: u16 = 917;
const BOTH_STAND6: u16 = 925;
const BOTH_STAND1TO2: u16 = 927;
const BOTH_STAND2TO1: u16 = 928;
const BOTH_SABERFAST_STANCE: u16 = 850;
const BOTH_SABERSLOW_STANCE: u16 = 851;
const BOTH_SABERDUAL_STANCE: u16 = 852;
const BOTH_SABERSTAFF_STANCE: u16 = 853;
const BOTH_BUTTON_HOLD: u16 = 1_328;
const BOTH_BUTTON_RELEASE: u16 = 1_329;
const BOTH_CROUCH1IDLE: u16 = 1_005;
const BOTH_CROUCH1WALK: u16 = 1_006;
const BOTH_CROUCH1WALKBACK: u16 = 1_007;
const BOTH_GUNSIT1: u16 = 1_014;
const BOTH_WALK1: u16 = 1_102;
const BOTH_WALK2: u16 = 1_103;
const BOTH_WALK_STAFF: u16 = 1_104;
const BOTH_WALKBACK_STAFF: u16 = 1_105;
const BOTH_WALK_DUAL: u16 = 1_106;
const BOTH_WALKBACK_DUAL: u16 = 1_107;
const BOTH_RUN1: u16 = 1_111;
const BOTH_RUN2: u16 = 1_114;
const BOTH_RUN_STAFF: u16 = 1_118;
const BOTH_RUN_DUAL: u16 = 1_120;
const BOTH_WALKBACK1: u16 = 1_134;
const BOTH_WALKBACK2: u16 = 1_135;
const BOTH_RUNBACK1: u16 = 1_136;
const BOTH_RUNBACK2: u16 = 1_137;
const BOTH_JUMP1: u16 = 1_138;
const BOTH_INAIR1: u16 = 1_139;
const BOTH_LAND1: u16 = 1_140;
const BOTH_JUMPBACK1: u16 = 1_142;
const BOTH_LANDBACK1: u16 = 1_144;
const BOTH_JUMPLEFT1: u16 = 1_145;
const BOTH_LANDLEFT1: u16 = 1_147;
const BOTH_JUMPRIGHT1: u16 = 1_148;
const BOTH_LANDRIGHT1: u16 = 1_150;
const BOTH_FORCEJUMP1: u16 = 1_151;
const BOTH_FORCELAND1: u16 = 1_153;
const BOTH_FORCEJUMPBACK1: u16 = 1_154;
const BOTH_FORCELANDBACK1: u16 = 1_156;
const BOTH_FORCEJUMPLEFT1: u16 = 1_157;
const BOTH_FORCELANDLEFT1: u16 = 1_159;
const BOTH_FORCEJUMPRIGHT1: u16 = 1_160;
const BOTH_FORCELANDRIGHT1: u16 = 1_162;
const BOTH_A7_KICK_F_AIR: u16 = 895;
const BOTH_A7_KICK_B_AIR: u16 = 896;
const BOTH_A7_KICK_R_AIR: u16 = 897;
const BOTH_A7_KICK_L_AIR: u16 = 898;
const BOTH_WALL_RUN_RIGHT: u16 = 1_211;
const BOTH_WALL_RUN_LEFT: u16 = 1_214;
const BOTH_SWIM_IDLE1: u16 = 1_310;
const BOTH_SWIMFORWARD: u16 = 1_311;
const BOTH_CHOKE3: u16 = 1_322;
const BOTH_ATTACK4: u16 = 116;
const TORSO_DROPWEAP1: u16 = 1_396;
const TORSO_WEAPONREADY1: u16 = 1_400;
const TORSO_WEAPONREADY2: u16 = 1_401;
const TORSO_WEAPONREADY3: u16 = 1_402;
const TORSO_WEAPONREADY4: u16 = 1_403;
const TORSO_WEAPONREADY10: u16 = 1_404;
const TORSO_WEAPONIDLE3: u16 = 1_406;
/// `LEGS_LEFTUP1` through `LEGS_S5_RUP5`: ten groups of five slope poses.
const SLOPE_POSES: std::ops::RangeInclusive<u16> = 1_422..=1_471;
/// `BOTH_A1_T__B_` through `BOTH_H1_S1_BR` (`PM_InSaberAnim`).
const SABER_ANIMATIONS: std::ops::RangeInclusive<u16> = 126..=689;
/// `BOTH_PAIN1` through `BOTH_PAIN18` (`PM_PainAnim`).
pub(crate) const PAIN_ANIMATIONS: std::ops::RangeInclusive<u16> = 95..=112;

const WP_STUN_BATON: u8 = 1;
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_BRYAR_PISTOL: u8 = 4;
const WP_DISRUPTOR: u8 = 6;
const WP_THERMAL: u8 = 12;
const WP_DET_PACK: u8 = 14;
const WP_BRYAR_OLD: u8 = 16;
const WP_EMPLACED_GUN: u8 = 17;
const WP_TURRET: u8 = 18;
const SS_DUAL: u8 = 6;
const SS_STAFF: u8 = 7;
const PM_FLOAT: u8 = 2;
const PMF_DUCKED: u16 = 1;
const PMF_JUMP_HELD: u16 = 2;
const PMF_ROLLING: u16 = 4;
const PMF_BACKWARDS_JUMP: u16 = 8;
const PMF_BACKWARDS_RUN: u16 = 16;
const BUTTON_WALKING: u16 = 16;
const BUTTON_ATTACKS: u16 = 1 | 128;
const LS_SPINATTACK: u32 = 30;
const FORCE_SPEED_BIT: u32 = 1 << 2;
/// `bg_local.h:30`.
const TIMER_LAND: i32 = 130;

/// `CS_LEGACY_FIXES` bit for `g_fixRunWalkAnims` (`bg_public.h:162-165`).
pub const LEGACY_FIX_RUN_WALK_ANIMS: u32 = 1 << 2;

/// `WeaponReadyAnim` (`bg_misc.c:245-270`).
pub(crate) fn weapon_ready_torso(weapon: u8) -> u16 {
    match weapon {
        0 => TORSO_DROPWEAP1,
        WP_SABER => BOTH_STAND2,
        WP_BRYAR_PISTOL | WP_BRYAR_OLD => TORSO_WEAPONREADY2,
        WP_THERMAL..=WP_DET_PACK => TORSO_WEAPONREADY10,
        WP_EMPLACED_GUN => BOTH_STAND1,
        WP_TURRET => TORSO_WEAPONREADY1,
        _ => TORSO_WEAPONREADY3,
    }
}

/// `WeaponReadyLegsAnim` (`bg_misc.c:271-296`): every weapon stands the same way but
/// the saber.
fn weapon_ready_legs(weapon: u8) -> u16 {
    if weapon == WP_SABER {
        BOTH_STAND2
    } else {
        BOTH_STAND1
    }
}

/// `BG_SabersOff` (`bg_pmove.c:223-238`).
pub(crate) fn sabers_off_state(state: &MovementState) -> bool {
    sabers_off(state)
}

fn sabers_off(state: &MovementState) -> bool {
    state.saber_holstered != 0
        && !(matches!(state.saber_anim_level_base, SS_DUAL | SS_STAFF) && state.saber_holstered < 2)
}

/// `PM_GetSaberStance` (`bg_pmove.c`), without a custom saber's own ready animation:
/// saber definitions do not reach movement yet.
pub(crate) fn saber_stance(state: &MovementState) -> u16 {
    // No saber entity: it was knocked away.
    if state.saber_entity_num == 0 || sabers_off(state) {
        return BOTH_STAND1;
    }
    match state.saber_anim_level {
        SS_DUAL => BOTH_SABERDUAL_STANCE,
        SS_STAFF => BOTH_SABERSTAFF_STANCE,
        // SS_FAST and SS_TAVION.
        1 | 5 => BOTH_SABERFAST_STANCE,
        // SS_STRONG.
        3 => BOTH_SABERSLOW_STANCE,
        _ => BOTH_STAND2,
    }
}

/// `BG_SaberInSpecial` (`bg_panimate.c`): `LS_A_BACK` through `LS_HILT_BASH`.
fn saber_in_special(saber_move: u32) -> bool {
    (11..=61).contains(&saber_move)
}

fn landing(animation: u16) -> bool {
    matches!(
        animation,
        BOTH_LAND1
            | 1_141
            | BOTH_LANDBACK1
            | BOTH_LANDLEFT1
            | BOTH_LANDRIGHT1
            | BOTH_FORCELAND1
            | BOTH_FORCELANDBACK1
            | BOTH_FORCELANDLEFT1
            | BOTH_FORCELANDRIGHT1
    )
}

fn force_landing(animation: u16) -> bool {
    matches!(
        animation,
        BOTH_FORCELAND1 | BOTH_FORCELANDBACK1 | BOTH_FORCELANDLEFT1 | BOTH_FORCELANDRIGHT1
    )
}

/// `PM_LegsSlopeBackTransition` (`bg_pmove.c:5101-5162`): legs in a slope pose step
/// back one pose at a time, every 8 ms, and the player stands still meanwhile.
fn slope_back_transition(state: &mut MovementState, command: &UserCommand, desired: u16) -> u16 {
    let legs = state.legs_anim;
    if !SLOPE_POSES.contains(&legs) || (legs - SLOPE_POSES.start()) % 5 == 0 {
        return desired;
    }
    state.velocity = [0.0; 3];
    if state.slope_recalc_time < command.server_time {
        state.slope_recalc_time = command.server_time + 8;
        legs - 1
    } else {
        legs
    }
}

/// What a mover standing at rest needs for `PM_AdjustStandAnimForSlope`: the bottom of
/// its box (`pm->mins[2]`), its feet on its model, and the world to drop them in.
pub(crate) struct Slope<'a> {
    pub(crate) minimum_z: f32,
    pub(crate) feet: &'a crate::pmove::FootBolts<'a>,
    pub(crate) collision: &'a dyn crate::pmove::MovementCollision,
}

/// The legs half of `PM_Footsteps` (`bg_pmove.c:5179-5668`) except the roll, which
/// the caller tries first while ducked.
pub(crate) fn footsteps(
    state: &mut MovementState,
    command: &UserCommand,
    legacy_fixes: u32,
    lengths: &dyn AnimationLengths,
    npc: Option<&crate::pmove::npc::NpcBody>,
    slope: &Slope<'_>,
) {
    let legs = state.legs_anim;
    // Legs in a saber, stance, landing or pain animation are overridden.
    let flags = if SABER_ANIMATIONS.contains(&legs) && !crate::pmove_roll_anim::spinning_saber(legs)
        || matches!(
            legs,
            BOTH_STAND1
                | BOTH_STAND1TO2
                | BOTH_STAND2TO1
                | BOTH_STAND2
                | BOTH_SABERFAST_STANCE
                | BOTH_SABERSLOW_STANCE
                | BOTH_BUTTON_HOLD
                | BOTH_BUTTON_RELEASE
        )
        || landing(legs)
        || PAIN_ANIMATIONS.contains(&legs)
    {
        SETANIM_FLAG_OVERRIDE
    } else {
        0
    };
    let set_or_continue = |state: &mut MovementState, animation: u16| {
        if state.legs_anim != animation {
            set_animation(state, SETANIM_LEGS, animation, flags, lengths);
        } else {
            continue_legs(state, animation);
        }
    };
    if state.saber_move == LS_SPINATTACK {
        // Both halves of the function only continue the torso's animation.
        continue_legs(state, state.torso_anim);
        return;
    }
    if state.ground_entity_number == ENTITY_NUMBER_NONE {
        if state.water_level > 1 {
            let speed = (state.velocity[0] * state.velocity[0]
                + state.velocity[1] * state.velocity[1])
                .sqrt();
            continue_legs(
                state,
                if speed > 60.0 {
                    BOTH_SWIMFORWARD
                } else {
                    BOTH_SWIM_IDLE1
                },
            );
        }
        return;
    }
    let crouch_walk = if state.movement_flags & PMF_BACKWARDS_RUN != 0 {
        BOTH_CROUCH1WALKBACK
    } else {
        BOTH_CROUCH1WALK
    };
    if command.forward_move == 0 && command.right_move == 0 {
        let speed =
            (state.velocity[0] * state.velocity[0] + state.velocity[1] * state.velocity[1]).sqrt();
        if speed >= 5.0 {
            return;
        }
        if let Some(stance) = crate::pmove::npc::monster_stance(npc) {
            continue_legs(state, stance);
        } else if state.movement_flags & (PMF_DUCKED | PMF_ROLLING) != 0 {
            set_or_continue(state, BOTH_CROUCH1IDLE);
        } else if state.weapon == WP_DISRUPTOR && state.zoom_mode == 1 {
            // "The anim has a valid pose for the legs": a scoped player cannot move.
            continue_legs(state, TORSO_WEAPONREADY4);
        } else {
            // `PM_AdjustStandAnimForSlope` reads the feet here for a saber carrier: on
            // uneven ground it takes a slope pose ([`crate::pmove_slope`]). The read is
            // also what the server's skeleton sees, and the server collects it after the
            // command.
            state.read_foot_bolts |= state.weapon == WP_SABER;
            if state.weapon == WP_SABER {
                let feet = (slope.feet)(state.origin, state.view_angles[1]);
                if feet.is_none() && SLOPE_POSES.contains(&state.legs_anim) {
                    // A client predicting without the model (no feet to read) keeps the
                    // slope pose the server sent while it stands still. The cgame reads
                    // its own model here; stepping back to the stance instead made the
                    // legs flick between the pose and the stance after every snapshot
                    // (Creyon's ledge and idle stutter, 11/10/2026).
                    continue_legs(state, state.legs_anim);
                    return;
                }
                if crate::pmove_slope::adjust_stand_for_slope(
                    state,
                    command,
                    slope.minimum_z,
                    feet,
                    slope.collision,
                ) {
                    return;
                }
            }
            let stand = if state.weapon == WP_SABER {
                saber_stance(state)
            } else {
                weapon_ready_legs(state.weapon)
            };
            let stand = slope_back_transition(state, command, stand);
            continue_legs(state, stand);
        }
        return;
    }
    if state.movement_flags & PMF_DUCKED != 0 {
        set_or_continue(state, crouch_walk);
        return;
    }
    if state.movement_flags & PMF_ROLLING != 0
        && !crate::pmove_roll::in_roll(state)
        && !crate::pmove_roll::in_roll_complete(state)
    {
        set_or_continue(state, crouch_walk);
        return;
    }
    if force_landing(legs) && state.legs_timer > 0 {
        return;
    }
    let walking = command.buttons & BUTTON_WALKING != 0;
    let monster = if walking {
        None
    } else {
        crate::pmove::npc::monster_run(npc, state.movement_flags)
    };
    let desired = monster.unwrap_or_else(|| locomotion(state, walking, legacy_fixes));
    let result = slope_back_transition(state, command, desired);
    if state.legs_anim != desired && result == desired {
        set_animation(state, SETANIM_LEGS, desired, flags, lengths);
    } else {
        continue_legs(state, result);
    }
}

/// The walk or run cycle for this weapon, saber style and direction
/// (`bg_pmove.c:5410-5660`). With `g_fixRunWalkAnims` off, as on retail servers, a gun
/// carrier moves like a saber carrier of the same style.
fn locomotion(state: &MovementState, walking: bool, legacy_fixes: u32) -> u16 {
    let backwards = state.movement_flags & PMF_BACKWARDS_RUN != 0;
    // `[saber off, one blade on, all blades on]` of the styles with two blades.
    let by_blades = |off: u16, one: u16, all: u16, single_on: u16| match state.saber_anim_level {
        SS_STAFF | SS_DUAL if state.saber_holstered > 1 => off,
        SS_STAFF | SS_DUAL if state.saber_holstered == 1 => one,
        SS_STAFF | SS_DUAL => all,
        _ if state.saber_holstered != 0 => off,
        _ => single_on,
    };
    let staff = state.saber_anim_level == SS_STAFF;
    let gun_fixed = legacy_fixes & LEGACY_FIX_RUN_WALK_ANIMS != 0 && state.weapon != WP_SABER;
    match (walking, backwards) {
        (false, true) if gun_fixed => BOTH_RUNBACK1,
        // The reference disabled the staff's and the pair's own backwards runs
        // ("pretty messed up for some reason"); one lit blade does not count as off.
        (false, true) => match state.saber_anim_level {
            SS_STAFF | SS_DUAL if state.saber_holstered > 1 => BOTH_RUNBACK1,
            SS_STAFF | SS_DUAL => BOTH_RUNBACK2,
            _ if state.saber_holstered != 0 => BOTH_RUNBACK1,
            _ => BOTH_RUNBACK2,
        },
        (false, false) if gun_fixed => BOTH_RUN1,
        (false, false)
            if staff
                && state.saber_holstered == 0
                && state.force_powers_active & FORCE_SPEED_BIT != 0 =>
        {
            BOTH_RUN1
        }
        (false, false) => by_blades(
            BOTH_RUN1,
            BOTH_RUN2,
            if staff { BOTH_RUN_STAFF } else { BOTH_RUN_DUAL },
            BOTH_RUN2,
        ),
        (true, true) if gun_fixed => BOTH_WALKBACK1,
        (true, true) => by_blades(
            BOTH_WALKBACK1,
            BOTH_WALKBACK2,
            if staff {
                BOTH_WALKBACK_STAFF
            } else {
                BOTH_WALKBACK_DUAL
            },
            BOTH_WALKBACK2,
        ),
        (true, false) if state.weapon == WP_MELEE || sabers_off(state) || gun_fixed => BOTH_WALK1,
        (true, false) => by_blades(
            BOTH_WALK1,
            BOTH_WALK2,
            if staff {
                BOTH_WALK_STAFF
            } else {
                BOTH_WALK_DUAL
            },
            BOTH_WALK2,
        ),
    }
}

/// The animation half of `PM_GroundTraceMissed` (`bg_pmove.c:4021-4098`), before it
/// clears the ground entity: leaving the ground with no floor within 64 units below
/// puts the legs into a jump or fall. `floor_is_far` runs that trace when needed.
pub(crate) fn left_the_ground(
    state: &mut MovementState,
    command: &UserCommand,
    lengths: &dyn AnimationLengths,
    floor_is_far: impl FnOnce(&MovementState) -> bool,
) {
    if state.movement_type == PM_FLOAT {
        // Being choked; no hold, or the legs float before settling on the ground.
        set_animation(
            state,
            SETANIM_LEGS,
            BOTH_CHOKE3,
            SETANIM_FLAG_OVERRIDE,
            lengths,
        );
    } else if state.movement_type != 1
        && (state.ground_entity_number != ENTITY_NUMBER_NONE || state.legs_anim == BOTH_CHOKE3)
    {
        // Without the trace a player would back-flip down staircases.
        if floor_is_far(state) {
            if state.velocity[2] <= 0.0 && state.movement_flags & PMF_JUMP_HELD == 0 {
                set_animation(state, SETANIM_LEGS, BOTH_INAIR1, 0, lengths);
                state.movement_flags &= !PMF_BACKWARDS_JUMP;
            } else if command.forward_move >= 0 {
                set_animation(
                    state,
                    SETANIM_LEGS,
                    BOTH_JUMP1,
                    SETANIM_FLAG_OVERRIDE,
                    lengths,
                );
                state.movement_flags &= !PMF_BACKWARDS_JUMP;
            } else {
                set_animation(
                    state,
                    SETANIM_LEGS,
                    BOTH_JUMPBACK1,
                    SETANIM_FLAG_OVERRIDE,
                    lengths,
                );
                state.movement_flags |= PMF_BACKWARDS_JUMP;
            }
            state.in_air_animation = true;
        }
    } else if !state.in_air_animation && floor_is_far(state) {
        state.in_air_animation = true;
    }
    if crate::pmove_roll::in_roll_complete(state) {
        // A client only sees a roll restart if the animation changed in between.
        set_animation(
            state,
            SETANIM_BOTH,
            BOTH_INAIR1,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            lengths,
        );
        state.in_air_animation = true;
    }
}

/// Thrown off the ground while standing on it (`bg_pmove.c:4153-4170`).
pub(crate) fn kicked_off(state: &mut MovementState, command: &UserCommand) {
    if command.forward_move >= 0 {
        force_legs(state, BOTH_JUMP1);
        state.movement_flags &= !PMF_BACKWARDS_JUMP;
    } else {
        force_legs(state, BOTH_JUMPBACK1);
        state.movement_flags |= PMF_BACKWARDS_JUMP;
    }
}

/// `PM_JumpForDir` (`bg_pmove.c:1277-1310`): the jump animation by input direction.
pub(crate) fn jump_for_direction(
    state: &mut MovementState,
    command: &UserCommand,
    lengths: &dyn AnimationLengths,
) {
    let animation = if command.forward_move > 0 {
        BOTH_JUMP1
    } else if command.forward_move < 0 {
        BOTH_JUMPBACK1
    } else if command.right_move > 0 {
        BOTH_JUMPRIGHT1
    } else if command.right_move < 0 {
        BOTH_JUMPLEFT1
    } else {
        BOTH_JUMP1
    };
    if animation == BOTH_JUMPBACK1 {
        state.movement_flags |= PMF_BACKWARDS_JUMP;
    } else {
        state.movement_flags &= !PMF_BACKWARDS_JUMP;
    }
    if !crate::pmove_roll_anim::death(state.legs_anim) {
        set_animation(
            state,
            SETANIM_LEGS,
            animation,
            SETANIM_FLAG_OVERRIDE,
            lengths,
        );
    }
}

/// The animation part of `PM_CrashLand` (`bg_pmove.c:3736-3843`), run for every
/// landing whose impact could be computed, before any of its early returns.
pub(crate) fn crash_land(state: &mut MovementState, lengths: &dyn AnimationLengths) {
    let legs = state.legs_anim;
    let hold = SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD;
    let airborne_roll = crate::pmove_roll::in_roll(state);
    if let Some(land) = match legs {
        BOTH_A7_KICK_F_AIR => Some(BOTH_FORCELAND1),
        BOTH_A7_KICK_B_AIR => Some(BOTH_FORCELANDBACK1),
        BOTH_A7_KICK_R_AIR => Some(BOTH_FORCELANDRIGHT1),
        BOTH_A7_KICK_L_AIR => Some(BOTH_FORCELANDLEFT1),
        _ => None,
    } {
        let parts = if state.torso_anim == legs {
            SETANIM_BOTH
        } else {
            SETANIM_LEGS
        };
        set_animation(state, parts, land, hold, lengths);
    } else if let Some(land) = match legs {
        BOTH_FORCEJUMPLEFT1 => Some(BOTH_LANDLEFT1),
        BOTH_FORCEJUMPRIGHT1 => Some(BOTH_LANDRIGHT1),
        BOTH_FORCEJUMPBACK1 => Some(BOTH_LANDBACK1),
        BOTH_FORCEJUMP1 => Some(BOTH_LAND1),
        _ => None,
    } {
        set_animation(state, SETANIM_BOTH, land, hold, lengths);
    } else if !airborne_roll
        && state.in_air_animation
        && state.vehicle_entity_num == 0
        && !saber_in_special(state.saber_move)
    {
        // Only after an in-air animation was entered off the ground.
        force_legs(
            state,
            if state.movement_flags & PMF_BACKWARDS_JUMP != 0 {
                BOTH_LANDBACK1
            } else {
                BOTH_LAND1
            },
        );
    }
    if !matches!(state.weapon, WP_SABER | WP_MELEE) {
        // Back into the ready stance from the landing; the saber has its own.
        start_torso(state, ready_torso_pose(state));
    }
    let legs = state.legs_anim;
    if (!crate::pmove_roll_anim::special_jump(legs)
        || state.legs_timer < 1
        || matches!(legs, BOTH_WALL_RUN_LEFT | BOTH_WALL_RUN_RIGHT))
        && !crate::pmove_roll::in_roll(state)
        && state.in_air_animation
        && (!saber_in_special(state.saber_move) || state.weapon != WP_SABER)
        && !force_landing(legs)
    {
        state.legs_timer = TIMER_LAND;
    }
}

/// The torso pose of a gun at rest: scoped, seated at an emplaced gun, or ready.
fn ready_torso_pose(state: &MovementState) -> u16 {
    if state.weapon == WP_DISRUPTOR && state.zoom_mode == 1 {
        TORSO_WEAPONREADY4
    } else if state.weapon == WP_EMPLACED_GUN {
        BOTH_GUNSIT1
    } else {
        weapon_ready_torso(state.weapon)
    }
}

/// `PM_Weapon`'s idle torso (`bg_pmove.c:7282-7349`): a ready gun returns to its ready
/// pose, melee follows the legs, and the disruptor's scoped pose follows the scope.
pub(crate) fn idle_torso(state: &mut MovementState, command: &UserCommand) {
    if state.vehicle_entity_num != 0 {
        return;
    }
    let ready = weapon_ready_torso(state.weapon);
    if state.weapon_state == 0
        && state.weapon_time <= 0
        && (state.weapon >= WP_BRYAR_PISTOL || state.weapon == WP_STUN_BATON)
        && state.torso_timer <= 0
        && state.torso_anim != ready
        && state.torso_anim != TORSO_WEAPONIDLE3
        && state.weapon != WP_EMPLACED_GUN
    {
        start_torso(state, ready);
    } else if state.weapon == WP_MELEE && state.weapon_time <= 0 && state.force_hand_extend == 0 {
        // The standing poses become the melee stance.
        let pose = if matches!(state.legs_anim, BOTH_STAND1 | BOTH_STAND2) {
            BOTH_STAND6
        } else {
            state.legs_anim
        };
        if command.buttons & BUTTON_ATTACKS == 0 && state.torso_anim != pose {
            start_torso(state, pose);
        }
    }
    let scoped = state.weapon == WP_DISRUPTOR && state.zoom_mode == 1;
    let scoped_pose = matches!(state.torso_anim, TORSO_WEAPONREADY4 | BOTH_ATTACK4);
    if scoped_pose && !scoped {
        start_torso(
            state,
            if state.weapon == WP_EMPLACED_GUN {
                BOTH_GUNSIT1
            } else {
                ready
            },
        );
    } else if !scoped_pose && scoped {
        start_torso(state, TORSO_WEAPONREADY4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmove::{
        ENTITY_NUMBER_WORLD, MovementCollision, MovementConfig, MovementTrace, Predictor,
    };
    use sjk_protocol::PlayerState;

    /// A floor at height 0.
    struct Floor;
    impl MovementCollision for Floor {
        fn trace(
            &self,
            start: [f32; 3],
            mins: [f32; 3],
            _: [f32; 3],
            end: [f32; 3],
            _: u32,
        ) -> MovementTrace {
            if end[2] + mins[2] < 0.0 {
                let fraction = (start[2] + mins[2]) / (start[2] - end[2]);
                MovementTrace {
                    fraction,
                    end_position: [end[0], end[1], -mins[2]],
                    plane_normal: [0.0, 0.0, 1.0],
                    entity_number: ENTITY_NUMBER_WORLD,
                    ..MovementTrace::miss(end)
                }
            } else {
                MovementTrace::miss(end)
            }
        }
    }

    /// A saber carrier standing on the floor in the slope pose `legs`, as a snapshot from
    /// a server that reads the feet on its model sends it.
    fn standing_in(legs: u16) -> Predictor {
        let mut player = PlayerState::default();
        player.stats[0] = 100;
        player.set_origin([0.0, 0.0, 24.0]);
        player.set_gravity(800);
        player.set_speed(250.0);
        player.set_base_speed(250);
        player.set_ground_entity_num(ENTITY_NUMBER_WORLD);
        let mut predictor = Predictor::from_player_state(&player, MovementConfig::default());
        // Every animation a second long: footsteps run only with the model's timings.
        let timing = crate::pmove_anim::AnimationTiming {
            frame_count: 20,
            frame_lerp_ms: 50,
        };
        predictor.set_animation_lengths(std::sync::Arc::new(
            crate::pmove_anim::AnimationLengthTable::new(vec![timing; 1_600]),
        ));
        let state = predictor.state_mut();
        state.weapon = WP_SABER;
        state.legs_anim = legs;
        state.torso_anim = BOTH_STAND1;
        predictor
    }

    #[test]
    fn a_client_without_the_model_keeps_the_servers_slope_pose() {
        // `LEGS_S1_LUP5`: one foot hanging over a ledge. Stepping back to the stance
        // made the legs flick between the two after every snapshot. Every command step
        // (125, 142, 250 and 333 FPS) keeps it.
        for step in [8, 7, 4, 3] {
            let mut predictor = standing_in(1_436);
            for time in (step..=200).step_by(step as usize) {
                predictor.predict_command(
                    UserCommand {
                        server_time: time,
                        ..UserCommand::default()
                    },
                    &Floor,
                );
                assert_eq!(
                    predictor.state().legs_anim,
                    1_436,
                    "at {time} ms, {step} ms steps"
                );
            }
        }
    }
}

//! Roll entry and motion rules shared by local prediction and parity tools.
//!
//! Stock entry follows OpenJK `codemp/game/bg_pmove.c:3567-3650,5316-5360`.
//! Dialect modes follow TaystJK `codemp/game/bg_pmove.c:279-322,8293-8297,
//! 11814-11845,12134-12283` at commit 5802c999.

use glam::Vec3;
use sjk_protocol::{
    GameState, JaPlusCapabilities, JaProCapabilities, ServerDialect, ServerProfile,
    TaystJkCapabilities, UserCommand,
};

use crate::pmove::{MovementCollision, MovementState};
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, set_animation,
};

pub const PMF_ROLLING: u16 = 4;
pub(crate) const PMF_BACKWARDS_RUN: u16 = 16;
const PMF_DUCKED: u16 = 1;
/// `DEFAULT_VIEWHEIGHT`: `DEFAULT_MAXS_2` less four (`bg_public.h:80`).
const DEFAULT_VIEW_HEIGHT: i32 = 36;
const CONTENTS_SOLID: u32 = 1;
const STEP_SIZE: f32 = 18.0;
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const BOTH_A7_SOULCAL: u16 = 910;

pub const BOTH_ROLL_F: u16 = 1_167;
pub const BOTH_ROLL_B: u16 = 1_168;
pub const BOTH_ROLL_L: u16 = 1_169;
pub const BOTH_ROLL_R: u16 = 1_170;

/// Server-selected roll compatibility behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RollRules {
    /// TaystJK `GetFixRoll` result: stock, fix1, fix2, or fix3.
    pub fix_roll: u8,
    /// Either equipped `.sab` definition carries `noRolls 1`.
    pub saber_forbids_rolls: bool,
    /// Legacy gametype used by `BG_HasYsalamiri` for CTY flags.
    pub gametype: u8,
}

impl RollRules {
    /// Derive the highest-precedence FIXROLL mode from a parsed dialect.
    pub fn from_dialect(dialect: &ServerDialect) -> Self {
        let bits = match dialect {
            ServerDialect::JaPlus { capabilities } => [
                capabilities.contains(JaPlusCapabilities::FIX_ROLL_1),
                capabilities.contains(JaPlusCapabilities::FIX_ROLL_2),
                capabilities.contains(JaPlusCapabilities::FIX_ROLL_3),
            ],
            ServerDialect::TaystJk {
                ja_pro_capabilities,
                tayst_capabilities,
            } => [
                ja_pro_capabilities.contains(JaProCapabilities::FIX_ROLL_1)
                    || tayst_capabilities.contains(TaystJkCapabilities::FIX_ROLL_1),
                ja_pro_capabilities.contains(JaProCapabilities::FIX_ROLL_2)
                    || tayst_capabilities.contains(TaystJkCapabilities::FIX_ROLL_2),
                ja_pro_capabilities.contains(JaProCapabilities::FIX_ROLL_3)
                    || tayst_capabilities.contains(TaystJkCapabilities::FIX_ROLL_3),
            ],
            ServerDialect::BaseJka | ServerDialect::Unknown { .. } => [false; 3],
        };
        Self {
            fix_roll: if bits[2] {
                3
            } else if bits[1] {
                2
            } else if bits[0] {
                1
            } else {
                0
            },
            ..Self::default()
        }
    }

    /// Read roll dialect flags and gametype from `CS_SERVERINFO`.
    pub fn from_game_state(game_state: &GameState) -> Self {
        let Some(bytes) = game_state.config_string(0) else {
            return Self::default();
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Self::default();
        };
        let Ok(info) = sjk_protocol::InfoString::parse(text) else {
            return Self::default();
        };
        let mut rules = Self::from_dialect(&ServerProfile::from_server_info(&info).dialect);
        rules.gametype = info
            .get_i32("g_gametype")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(0);
        rules
    }
}
/// Update `PMF_BACKWARDS_RUN` exactly where `PmoveSingle` does
/// (`bg_pmove.c:10648-10653`).
pub(crate) fn update_backwards_flag(state: &mut MovementState, command: &UserCommand) {
    if command.forward_move < 0 {
        state.movement_flags |= PMF_BACKWARDS_RUN;
    } else if command.forward_move > 0 || command.right_move != 0 {
        state.movement_flags &= !PMF_BACKWARDS_RUN;
    }
}
/// Force authored roll input and speed before movement (`bg_pmove.c:10477-10484,
/// 8475-8503`; TaystJK fix3 at `bg_pmove.c:11814-11845,12134-12170`).
pub(crate) fn prepare_command(
    state: &mut MovementState,
    command: &mut UserCommand,
    rules: RollRules,
) {
    if !in_roll(state) {
        return;
    }
    command.up_move = 0;
    match state.legs_anim {
        BOTH_ROLL_F => {
            if rules.fix_roll < 3 || command.forward_move >= 0 {
                command.forward_move = 127;
                command.right_move = 0;
            }
        }
        BOTH_ROLL_B => {
            command.forward_move = -127;
            command.right_move = 0;
        }
        BOTH_ROLL_R => {
            command.forward_move = 0;
            command.right_move = 127;
        }
        BOTH_ROLL_L => {
            command.forward_move = 0;
            command.right_move = -127;
        }
        _ => {}
    }
}
/// The roll inside `PM_Footsteps` (`bg_pmove.c:5316-5360`): a ducked, running player
/// goes into a roll if there is room. Returns whether it did; every other animation
/// of that function is `pmove_locomotion::footsteps`.
pub(crate) fn roll_from_crouch(
    state: &mut MovementState,
    command: &UserCommand,
    collision: &impl MovementCollision,
    lengths: &dyn AnimationLengths,
    rules: RollRules,
) -> bool {
    const LS_SPINATTACK: u32 = 30;
    if state.saber_move == LS_SPINATTACK
        || state.ground_entity_number == sjk_protocol::ENTITY_NUMBER_NONE
        || command.forward_move == 0 && command.right_move == 0
        || state.movement_flags & PMF_DUCKED == 0
    {
        return false;
    }
    let speed_squared = Vec3::from_array(state.velocity).length_squared();
    let threshold = if rules.fix_roll == 1 {
        30_000.0
    } else {
        40_000.0
    };
    let eligible = if rules.fix_roll > 1 {
        crate::pmove_roll_anim::running(state.legs_anim) || can_roll_from_soulcal(state)
    } else {
        crate::pmove_roll_anim::running(state.legs_anim) && speed_squared >= threshold
            || can_roll_from_soulcal(state)
    };
    if !eligible || in_roll(state) {
        return false;
    }
    let Some(animation) = try_roll(state, command, collision, rules) else {
        return false;
    };
    state.legs_timer = 0;
    state.legs_anim = 0;
    set_animation(
        state,
        SETANIM_BOTH,
        animation,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        lengths,
    );
    state.movement_flags &= !PMF_DUCKED;
    state.movement_flags |= PMF_ROLLING;
    state.view_height = DEFAULT_VIEW_HEIGHT;
    true
}
/// `PM_TryRoll` including the stock 64-unit clearance trace.
pub(crate) fn try_roll(
    state: &mut MovementState,
    command: &UserCommand,
    collision: &impl MovementCollision,
    rules: RollRules,
) -> Option<u16> {
    if crate::pmove_roll_anim::blocks_roll(state.saber_move, state.torso_anim, state.legs_anim)
        && !can_roll_from_soulcal(state)
    {
        return None;
    }
    if !matches!(state.weapon, WP_SABER | WP_MELEE)
        || !can_use_levitation(state, command.server_time, rules.gametype)
        || state.weapon == WP_SABER && rules.saber_forbids_rolls
    {
        return None;
    }
    let (forward, right) = yaw_axes(state.view_angles[1]);
    let (animation, displacement) = if command.forward_move != 0 {
        if state.movement_flags & PMF_BACKWARDS_RUN != 0 {
            (BOTH_ROLL_B, forward * -64.0)
        } else {
            (BOTH_ROLL_F, forward * 64.0)
        }
    } else if command.right_move > 0 {
        (BOTH_ROLL_R, right * 64.0)
    } else if command.right_move < 0 {
        (BOTH_ROLL_L, right * -64.0)
    } else {
        return None;
    };
    let end = (Vec3::from_array(state.origin) + displacement).to_array();
    let trace = collision.trace(
        state.origin,
        [-15.0, -15.0, -24.0 + STEP_SIZE],
        [15.0, 15.0, state.crouching_height],
        end,
        CONTENTS_SOLID,
    );
    if trace.fraction < 1.0 {
        return None;
    }
    state.saber_move = 0;
    Some(animation)
}

/// The strict Soul Cal roll window (`bg_panimate.c:1438-1447`).
pub fn can_roll_from_soulcal(state: &MovementState) -> bool {
    state.legs_anim == BOTH_A7_SOULCAL && (251..700).contains(&state.legs_timer)
}

/// Whether an ordinary roll animation is active (`bg_panimate.c:808-830`).
pub fn in_roll(state: &MovementState) -> bool {
    crate::pmove_roll_anim::in_roll(state.legs_anim) && state.legs_timer > 0
}

/// Whether an ordinary roll clip has reached its endpoint (`bg_panimate.c:1421-1435`).
pub fn in_roll_complete(state: &MovementState) -> bool {
    matches!(state.legs_anim, BOTH_ROLL_F..=BOTH_ROLL_R) && state.legs_timer < 1
}

/// Return the rolling hull top, or finish the rolling hull transition.
///
/// This is the rolling portion of `PM_CheckDuck`
/// (`codemp/game/bg_pmove.c:4491-4538`).
pub(crate) fn bounds_height(
    state: &mut MovementState,
    collision: &impl MovementCollision,
    minimums: [f32; 3],
    content_mask: u32,
) -> Option<f32> {
    // A get-up roll forwards or back is a "kicking" animation (`BG_KickingAnim`): it
    // is not the rolling hull, and stands up as any crouch would.
    if in_roll(state) && !crate::pmove_input_freeze::kicking_animation(state.legs_anim) {
        state.movement_flags &= !PMF_DUCKED;
        state.movement_flags |= PMF_ROLLING;
        state.view_height = DEFAULT_VIEW_HEIGHT;
        return Some(state.crouching_height);
    }
    if state.movement_flags & PMF_ROLLING == 0 {
        return None;
    }
    if !crate::pmove_posture::can_stand(state, collision, minimums, content_mask) {
        state.view_height = DEFAULT_VIEW_HEIGHT;
        Some(state.crouching_height)
    } else {
        state.movement_flags &= !PMF_ROLLING;
        // This branch is an `else if` in `PM_CheckDuck`; a crouch command is
        // not reconsidered until the next PmoveSingle (`bg_pmove.c:4500-4528`).
        state.view_height = DEFAULT_VIEW_HEIGHT;
        Some(state.standing_height)
    }
}

pub(crate) fn adjust_speed(state: &mut MovementState, command: &UserCommand, fix_roll: u8) {
    if state.speed <= 50.0 {
        return;
    }
    let timer = state.legs_timer as f32;
    state.speed = if fix_roll > 2 {
        if state.legs_anim == BOTH_ROLL_B {
            timer / 2.5
        } else if timer <= 100.0 && command.forward_move < 0 {
            timer / 20.0
        } else if timer <= 100.0 {
            timer / 5.0
        } else {
            timer / 1.5
        }
    } else if state.legs_anim == BOTH_ROLL_B {
        if timer > 800.0 {
            timer / 2.5
        } else {
            timer / 6.0
        }
    } else if timer > 800.0 {
        timer / 1.5
    } else {
        timer / 5.0
    };
    if fix_roll < 3 {
        state.speed = state.speed.min(600.0);
    }
}

fn can_use_levitation(state: &MovementState, time: i32, gametype: u8) -> bool {
    let cty_flag =
        gametype == 9 && (state.powerup_deadlines[4] > time || state.powerup_deadlines[5] > time);
    let ysalamiri = state.powerup_deadlines[15] > time;
    !cty_flag
        && !ysalamiri
        && !state.force_restricted
        && !state.true_non_jedi
        && state.emplaced_index == 0
        && state.vehicle_entity_num == 0
        && state.saber_lock_frame == 0
        && state.saber_lock_time <= time
        && state.falling_to_death == 0
}

fn yaw_axes(yaw: f32) -> (Vec3, Vec3) {
    let radians = yaw.to_radians();
    let (sine, cosine) = radians.sin_cos();
    // `AngleVectors` returns JKA's negative-local-Y right axis, the same
    // convention used by `PM_WalkMove` (`bg_pmove.c:3624-3627`).
    (Vec3::new(cosine, sine, 0.0), Vec3::new(sine, -cosine, 0.0))
}

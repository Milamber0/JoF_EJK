//! `BG_AdjustClientSpeed`, OpenJK codemp/game/bg_pmove.c:8346-8518.

use crate::pmove::{MovementConfig, MovementState};
use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

const SPEED: u32 = 1 << 2;
const GRIP: u32 = 1 << 6;
const RAGE: u32 = 1 << 8;

/// Apply the ordered on-foot rules, once per PmoveSingle (:10484).
pub(crate) fn adjust(state: &mut MovementState, cmd: &UserCommand, config: MovementConfig) {
    // :8360-8375. Keep an authoritative zero; never seed from modified speed.
    state.speed = state.base_speed;
    if matches!(state.force_hand_extend, 7 | 8 | 13 | 14) {
        state.speed = 0.0;
    }
    // :8379-8387. Roll input has already been forced by BG_CmdForRoll.
    if cmd.forward_move < 0
        && cmd.buttons & 16 == 0
        && state.ground_entity_number != ENTITY_NUMBER_NONE
    {
        state.speed *= 0.75;
    }
    let active = state.force_powers_active;
    if active & GRIP != 0 {
        state.speed *= config.ja_plus.grip_speed_scale();
    }
    // :8389-8400. Speed takes precedence over rage and recovery.
    if active & SPEED != 0 {
        state.speed *= 1.7;
    } else if active & RAGE != 0 {
        state.speed *= 1.3;
    } else if state.force_rage_recovery_time > cmd.server_time {
        state.speed *= 0.75;
    }
    // :8402-8406. The local-only zoom deadline survives acknowledged-command reseeding.
    if state.weapon == 6 && state.zoom_mode == 1 && state.zoom_lock_time < cmd.server_time {
        state.speed *= 0.5;
    }
    // :8408-8414. Unlike the boost branch, rage wins here when both are set.
    if state.force_grip_cripple && state.team != 3 {
        state.speed *= if active & RAGE != 0 {
            0.9
        } else if active & SPEED != 0 {
            0.8
        } else {
            0.2
        };
    }
    saber_penalty(state, cmd);
    // :8475-8503. The roll curve overrides prior factors only above 50.
    if crate::pmove_roll::in_roll(state) {
        crate::pmove_roll::adjust_speed(state, cmd, config.roll_rules.fix_roll);
    }
    // :8505-8517. Each equipped saber multiplies independently, even holstered.
    for scale in config.saber_speed_scales {
        if scale != 1.0 {
            state.speed *= scale;
        }
    }
}

fn saber_penalty(state: &mut MovementState, cmd: &UserCommand) {
    // BG_SaberInAttack, bg_panimate.c:246-309: ordinary and all special attacks.
    let attack = (4..=61).contains(&state.saber_move);
    let style = state.saber_anim_level;
    // bg_pmove.c:8416-8473: mutually exclusive, in this exact order.
    let factor = if attack && cmd.forward_move < 0 {
        match style {
            1 => 0.75,
            2 | 6 | 7 => 0.60,
            3 => 0.45,
            _ => 1.0,
        }
    } else if spinning(state.legs_anim) {
        if style == 3 { 0.3 } else { 0.5 }
    } else if state.weapon == 3 && attack {
        match style {
            2 | 6 | 7 => 0.85,
            3 => 0.55,
            _ => 1.0,
        }
    } else if state.weapon == 3 && style == 3 && (76..=117).contains(&state.saber_move) {
        // PM_SaberInTransition, bg_panimate.c:1622-1629.
        if cmd.forward_move < 0 { 0.4 } else { 0.6 }
    } else {
        1.0
    };
    state.speed *= factor;
}

/// Full BG_SpinningSaberAnim table, bg_panimate.c:498-605 (all seven styles).
fn spinning(animation: u16) -> bool {
    matches!(
        animation,
        136 | 139
            | 140
            | 144
            | 134
            | 135
            | 151
            | 153
            | 154
            | 156
            | 157
            | 158
            | 212
            | 213
            | 217
            | 230
            | 233
            | 234
            | 289
            | 290
            | 294
            | 307
            | 310
            | 311
            | 366
            | 367
            | 371
            | 384
            | 387
            | 388
            | 444
            | 447
            | 448
            | 452
            | 442
            | 443
            | 459
            | 461
            | 462
            | 464
            | 465
            | 466
            | 519
            | 523
            | 524
            | 525
            | 527
            | 528
            | 529
            | 533
            | 535
            | 536
            | 538
            | 539
            | 551
            | 552
            | 555
            | 556
            | 558
            | 520
            | 521
            | 541
            | 542
            | 543
            | 596
            | 597
            | 598
            | 601
            | 602
            | 605
            | 608
            | 613
            | 615
            | 616
            | 618
            | 619
            | 620
            | 629
            | 607
            | 632
            | 732
            | 855
            | 860
            | 1209
            | 1210
            | 1252
            | 1253
            | 856
            | 857
    )
}

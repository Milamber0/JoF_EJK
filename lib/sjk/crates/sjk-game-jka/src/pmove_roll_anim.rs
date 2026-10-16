//! Animation predicates used by the bounded roll predictor.

fn name(animation: u16) -> Option<&'static str> {
    crate::legacy_animation_name(usize::from(animation))
}

/// Exact `PM_TryRoll` animation gate from OpenJK
/// `codemp/game/bg_pmove.c:3573-3574`.
pub(crate) fn blocks_roll(saber_move: u32, torso: u16, legs: u16) -> bool {
    let move_number = saber_move as u16;
    // `BG_SaberInAttack` and `PM_SaberInStart` are contiguous LS_ ranges in
    // `bg_saber.c:593-614,629-638`.
    (4..=10).contains(&move_number)
        || (62..=68).contains(&move_number)
        || special_attack(torso)
        || spinning_saber(legs)
}

/// Exact `PM_RunningAnim` set (`bg_pmove.c:4598-4617`).
pub(crate) fn running(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_RUN1"
                | "BOTH_RUN2"
                | "BOTH_RUN_STAFF"
                | "BOTH_RUN_DUAL"
                | "BOTH_RUNBACK1"
                | "BOTH_RUNBACK2"
                | "BOTH_RUNBACK_STAFF"
                | "BOTH_RUNBACK_DUAL"
                | "BOTH_RUN1START"
                | "BOTH_RUN1STOP"
                | "BOTH_RUNSTRAFE_LEFT1"
                | "BOTH_RUNSTRAFE_RIGHT1"
        )
    )
}

/// Exact `PM_InOnGroundAnim` set (`bg_panimate.c:1369-1418`).
pub(crate) fn on_ground(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_DEAD1"
                | "BOTH_DEAD2"
                | "BOTH_DEAD3"
                | "BOTH_DEAD4"
                | "BOTH_DEAD5"
                | "BOTH_DEADFORWARD1"
                | "BOTH_DEADBACKWARD1"
                | "BOTH_DEADFORWARD2"
                | "BOTH_DEADBACKWARD2"
                | "BOTH_LYINGDEATH1"
                | "BOTH_LYINGDEAD1"
                | "BOTH_SLEEP1"
                | "BOTH_KNOCKDOWN1"
                | "BOTH_KNOCKDOWN2"
                | "BOTH_KNOCKDOWN3"
                | "BOTH_KNOCKDOWN4"
                | "BOTH_KNOCKDOWN5"
                | "BOTH_GETUP1"
                | "BOTH_GETUP2"
                | "BOTH_GETUP3"
                | "BOTH_GETUP4"
                | "BOTH_GETUP5"
                | "BOTH_GETUP_CROUCH_F1"
                | "BOTH_GETUP_CROUCH_B1"
                | "BOTH_FORCE_GETUP_F1"
                | "BOTH_FORCE_GETUP_F2"
                | "BOTH_FORCE_GETUP_B1"
                | "BOTH_FORCE_GETUP_B2"
                | "BOTH_FORCE_GETUP_B3"
                | "BOTH_FORCE_GETUP_B4"
                | "BOTH_FORCE_GETUP_B5"
                | "BOTH_FORCE_GETUP_B6"
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

/// Exact `BG_InRoll` animation set (`bg_panimate.c:808-830`).
pub(crate) fn in_roll(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_GETUP_BROLL_B"
                | "BOTH_GETUP_BROLL_F"
                | "BOTH_GETUP_BROLL_L"
                | "BOTH_GETUP_BROLL_R"
                | "BOTH_GETUP_FROLL_B"
                | "BOTH_GETUP_FROLL_F"
                | "BOTH_GETUP_FROLL_L"
                | "BOTH_GETUP_FROLL_R"
                | "BOTH_ROLL_F"
                | "BOTH_ROLL_B"
                | "BOTH_ROLL_L"
                | "BOTH_ROLL_R"
        )
    )
}

/// Exact `BG_FlippingAnim` set (`bg_panimate.c:452-496`): flips, wall runs, butterflies,
/// arials, cartwheels and the flip attacks.
pub(crate) fn flipping(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_FLIP_F"
                | "BOTH_FLIP_B"
                | "BOTH_FLIP_L"
                | "BOTH_FLIP_R"
                | "BOTH_WALL_RUN_RIGHT_FLIP"
                | "BOTH_WALL_RUN_LEFT_FLIP"
                | "BOTH_WALL_FLIP_RIGHT"
                | "BOTH_WALL_FLIP_LEFT"
                | "BOTH_FLIP_BACK1"
                | "BOTH_FLIP_BACK2"
                | "BOTH_FLIP_BACK3"
                | "BOTH_WALL_FLIP_BACK1"
                | "BOTH_WALL_RUN_RIGHT"
                | "BOTH_WALL_RUN_LEFT"
                | "BOTH_WALL_RUN_RIGHT_STOP"
                | "BOTH_WALL_RUN_LEFT_STOP"
                | "BOTH_BUTTERFLY_LEFT"
                | "BOTH_BUTTERFLY_RIGHT"
                | "BOTH_BUTTERFLY_FL1"
                | "BOTH_BUTTERFLY_FR1"
                | "BOTH_ARIAL_LEFT"
                | "BOTH_ARIAL_RIGHT"
                | "BOTH_ARIAL_F1"
                | "BOTH_CARTWHEEL_LEFT"
                | "BOTH_CARTWHEEL_RIGHT"
                | "BOTH_JUMPFLIPSLASHDOWN1"
                | "BOTH_JUMPFLIPSTABDOWN"
                | "BOTH_JUMPATTACK6"
                | "BOTH_JUMPATTACK7"
                | "BOTH_FORCEWALLRUNFLIP_END"
                | "BOTH_FORCEWALLRUNFLIP_ALT"
                | "BOTH_FLIP_ATTACK7"
                | "BOTH_A7_SOULCAL"
        )
    )
}

/// `BG_InDeathAnim` (`bg_panimate.c:855-940`): dying, lying dead, flopping, dismembered.
pub(crate) fn death(animation: u16) -> bool {
    name(animation).is_some_and(|name| {
        ["BOTH_DEATH", "BOTH_DEAD", "BOTH_LYINGDEA", "BOTH_STUMBLEDEA", "BOTH_FALLDEA", "BOTH_DISMEMBER_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            // `BOTH_DEATH_ROLL` and its kind are getting-up-dead poses the reference leaves out.
            && !name.starts_with("BOTH_DEATH_")
    })
}

/// Exact `BG_InSpecialJump` animation set (`bg_panimate.c:90-140`), with the rebound
/// and back-flip sets it includes.
pub(crate) fn special_jump(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_WALL_RUN_RIGHT"
                | "BOTH_WALL_RUN_RIGHT_STOP"
                | "BOTH_WALL_RUN_RIGHT_FLIP"
                | "BOTH_WALL_RUN_LEFT"
                | "BOTH_WALL_RUN_LEFT_STOP"
                | "BOTH_WALL_RUN_LEFT_FLIP"
                | "BOTH_WALL_FLIP_RIGHT"
                | "BOTH_WALL_FLIP_LEFT"
                | "BOTH_FLIP_BACK1"
                | "BOTH_FLIP_BACK2"
                | "BOTH_FLIP_BACK3"
                | "BOTH_WALL_FLIP_BACK1"
                | "BOTH_BUTTERFLY_LEFT"
                | "BOTH_BUTTERFLY_RIGHT"
                | "BOTH_BUTTERFLY_FL1"
                | "BOTH_BUTTERFLY_FR1"
                | "BOTH_FJSS_TR_BL"
                | "BOTH_FJSS_TL_BR"
                | "BOTH_FORCELEAP2_T__B_"
                | "BOTH_JUMPFLIPSLASHDOWN1"
                | "BOTH_JUMPFLIPSTABDOWN"
                | "BOTH_JUMPATTACK6"
                | "BOTH_JUMPATTACK7"
                | "BOTH_ARIAL_LEFT"
                | "BOTH_ARIAL_RIGHT"
                | "BOTH_ARIAL_F1"
                | "BOTH_CARTWHEEL_LEFT"
                | "BOTH_CARTWHEEL_RIGHT"
                | "BOTH_FORCELONGLEAP_START"
                | "BOTH_FORCELONGLEAP_ATTACK"
                | "BOTH_FORCEWALLRUNFLIP_START"
                | "BOTH_FORCEWALLRUNFLIP_END"
                | "BOTH_FORCEWALLRUNFLIP_ALT"
                | "BOTH_FLIP_ATTACK7"
                | "BOTH_FLIP_HOLD7"
                | "BOTH_FLIP_LAND"
                | "BOTH_A7_SOULCAL"
                | "BOTH_FORCEWALLREBOUND_FORWARD"
                | "BOTH_FORCEWALLREBOUND_LEFT"
                | "BOTH_FORCEWALLREBOUND_BACK"
                | "BOTH_FORCEWALLREBOUND_RIGHT"
                | "BOTH_FORCEWALLHOLD_FORWARD"
                | "BOTH_FORCEWALLHOLD_LEFT"
                | "BOTH_FORCEWALLHOLD_BACK"
                | "BOTH_FORCEWALLHOLD_RIGHT"
                | "BOTH_FORCEWALLRELEASE_FORWARD"
                | "BOTH_FORCEWALLRELEASE_LEFT"
                | "BOTH_FORCEWALLRELEASE_BACK"
                | "BOTH_FORCEWALLRELEASE_RIGHT"
        )
    )
}

fn special_attack(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_A2_STABBACK1"
                | "BOTH_ATTACK_BACK"
                | "BOTH_CROUCHATTACKBACK1"
                | "BOTH_ROLL_STAB"
                | "BOTH_BUTTERFLY_LEFT"
                | "BOTH_BUTTERFLY_RIGHT"
                | "BOTH_BUTTERFLY_FL1"
                | "BOTH_BUTTERFLY_FR1"
                | "BOTH_FJSS_TR_BL"
                | "BOTH_FJSS_TL_BR"
                | "BOTH_LUNGE2_B__T_"
                | "BOTH_FORCELEAP2_T__B_"
                | "BOTH_JUMPFLIPSLASHDOWN1"
                | "BOTH_JUMPFLIPSTABDOWN"
                | "BOTH_JUMPATTACK6"
                | "BOTH_JUMPATTACK7"
                | "BOTH_SPINATTACK6"
                | "BOTH_SPINATTACK7"
                | "BOTH_FORCELONGLEAP_ATTACK"
                | "BOTH_VS_ATR_S"
                | "BOTH_VS_ATL_S"
                | "BOTH_VT_ATR_S"
                | "BOTH_VT_ATL_S"
                | "BOTH_A7_KICK_F"
                | "BOTH_A7_KICK_B"
                | "BOTH_A7_KICK_R"
                | "BOTH_A7_KICK_L"
                | "BOTH_A7_KICK_S"
                | "BOTH_A7_KICK_BF"
                | "BOTH_A7_KICK_RL"
                | "BOTH_A7_KICK_F_AIR"
                | "BOTH_A7_KICK_B_AIR"
                | "BOTH_A7_KICK_R_AIR"
                | "BOTH_A7_KICK_L_AIR"
                | "BOTH_STABDOWN"
                | "BOTH_STABDOWN_STAFF"
                | "BOTH_STABDOWN_DUAL"
                | "BOTH_A6_SABERPROTECT"
                | "BOTH_A7_SOULCAL"
                | "BOTH_A1_SPECIAL"
                | "BOTH_A2_SPECIAL"
                | "BOTH_A3_SPECIAL"
                | "BOTH_FLIP_ATTACK7"
                | "BOTH_PULL_IMPALE_STAB"
                | "BOTH_PULL_IMPALE_SWING"
                | "BOTH_ALORA_SPIN_SLASH"
                | "BOTH_A6_FB"
                | "BOTH_A6_LR"
                | "BOTH_A7_HILT"
        )
    )
}

pub(crate) fn spinning_saber(animation: u16) -> bool {
    matches!(
        name(animation),
        Some(
            "BOTH_T1_BR_BL"
                | "BOTH_T1__R__L"
                | "BOTH_T1__R_BL"
                | "BOTH_T1_TR_BL"
                | "BOTH_T1_BR_TL"
                | "BOTH_T1_BR__L"
                | "BOTH_T1_TL_BR"
                | "BOTH_T1__L_BR"
                | "BOTH_T1__L__R"
                | "BOTH_T1_BL_BR"
                | "BOTH_T1_BL__R"
                | "BOTH_T1_BL_TR"
                | "BOTH_T2_BR__L"
                | "BOTH_T2_BR_BL"
                | "BOTH_T2__R_BL"
                | "BOTH_T2__L_BR"
                | "BOTH_T2_BL_BR"
                | "BOTH_T2_BL__R"
                | "BOTH_T3_BR__L"
                | "BOTH_T3_BR_BL"
                | "BOTH_T3__R_BL"
                | "BOTH_T3__L_BR"
                | "BOTH_T3_BL_BR"
                | "BOTH_T3_BL__R"
                | "BOTH_T4_BR__L"
                | "BOTH_T4_BR_BL"
                | "BOTH_T4__R_BL"
                | "BOTH_T4__L_BR"
                | "BOTH_T4_BL_BR"
                | "BOTH_T4_BL__R"
                | "BOTH_T5_BR_BL"
                | "BOTH_T5__R__L"
                | "BOTH_T5__R_BL"
                | "BOTH_T5_TR_BL"
                | "BOTH_T5_BR_TL"
                | "BOTH_T5_BR__L"
                | "BOTH_T5_TL_BR"
                | "BOTH_T5__L_BR"
                | "BOTH_T5__L__R"
                | "BOTH_T5_BL_BR"
                | "BOTH_T5_BL__R"
                | "BOTH_T5_BL_TR"
                | "BOTH_T6_BR_TL"
                | "BOTH_T6__R_TL"
                | "BOTH_T6__R__L"
                | "BOTH_T6__R_BL"
                | "BOTH_T6_TR_TL"
                | "BOTH_T6_TR__L"
                | "BOTH_T6_TR_BL"
                | "BOTH_T6_T__TL"
                | "BOTH_T6_T__BL"
                | "BOTH_T6_TL_BR"
                | "BOTH_T6__L_BR"
                | "BOTH_T6__L__R"
                | "BOTH_T6_TL__R"
                | "BOTH_T6_TL_TR"
                | "BOTH_T6__L_TR"
                | "BOTH_T6__L_T_"
                | "BOTH_T6_BL_T_"
                | "BOTH_T6_BR__L"
                | "BOTH_T6_BR_BL"
                | "BOTH_T6_BL_BR"
                | "BOTH_T6_BL__R"
                | "BOTH_T6_BL_TR"
                | "BOTH_T7_BR_TL"
                | "BOTH_T7_BR__L"
                | "BOTH_T7_BR_BL"
                | "BOTH_T7__R__L"
                | "BOTH_T7__R_BL"
                | "BOTH_T7_TR__L"
                | "BOTH_T7_T___R"
                | "BOTH_T7_TL_BR"
                | "BOTH_T7__L_BR"
                | "BOTH_T7__L__R"
                | "BOTH_T7_BL_BR"
                | "BOTH_T7_BL__R"
                | "BOTH_T7_BL_TR"
                | "BOTH_T7_TL_TR"
                | "BOTH_T7_T__BR"
                | "BOTH_T7__L_TR"
                | "BOTH_V7_BL_S7"
                | "BOTH_ATTACK_BACK"
                | "BOTH_CROUCHATTACKBACK1"
                | "BOTH_BUTTERFLY_LEFT"
                | "BOTH_BUTTERFLY_RIGHT"
                | "BOTH_FJSS_TR_BL"
                | "BOTH_FJSS_TL_BR"
                | "BOTH_JUMPFLIPSLASHDOWN1"
                | "BOTH_JUMPFLIPSTABDOWN"
        )
    )
}

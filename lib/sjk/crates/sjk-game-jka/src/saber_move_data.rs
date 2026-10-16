//! Canonical BaseJKA multiplayer saber-move metadata.
//!
//! This is the complete `saberMoveData[LS_MOVE_MAX]` table from
//! OpenJK `codemp/game/bg_saber.c:148-357`. Numeric move and animation
//! ordinals follow `bg_public.h:1280-1499` and `anims.h`.

/// Number of protocol-26 saber moves before `LS_MOVE_MAX`.
pub const SABER_MOVE_COUNT: usize = 162;

/// One row of OpenJK's `saberMoveData_t`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaberMove {
    /// Developer-facing move name.
    pub name: &'static str,
    /// `animNumber_t` selected before style-group adjustment.
    pub animation: u16,
    /// Starting `saberQuadrant_t`.
    pub start_quad: u8,
    /// Ending `saberQuadrant_t`.
    pub end_quad: u8,
    /// `SETANIM_FLAG_*` bitset.
    pub animation_flags: u8,
    /// Ghoul2 transition blend duration in milliseconds.
    pub blend_time: u16,
    /// `saberBlocking_t` mode.
    pub blocking: u8,
    /// Move selected when the attack chain idles.
    pub chain_idle: u16,
    /// Move selected when the attack chain continues.
    pub chain_attack: u16,
    /// Trail duration column in milliseconds.
    pub trail_len: u16,
}

/// Complete 162-row BaseJKA move table.
pub const SABER_MOVES: [SaberMove; SABER_MOVE_COUNT] = [
    SaberMove {
        name: "None",
        animation: 915,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 0,
        blend_time: 350,
        blocking: 0,
        chain_idle: 0,
        chain_attack: 0,
        trail_len: 0,
    }, // 0
    SaberMove {
        name: "Ready",
        animation: 917,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 0,
        blend_time: 350,
        blocking: 2,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 0,
    }, // 1
    SaberMove {
        name: "Draw",
        animation: 927,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 350,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 0,
    }, // 2
    SaberMove {
        name: "Putaway",
        animation: 928,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 350,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 0,
    }, // 3
    SaberMove {
        name: "TL2BR Att",
        animation: 129,
        start_quad: 4,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 69,
        chain_attack: 69,
        trail_len: 200,
    }, // 4
    SaberMove {
        name: "L2R Att",
        animation: 127,
        start_quad: 5,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 70,
        chain_attack: 70,
        trail_len: 200,
    }, // 5
    SaberMove {
        name: "BL2TR Att",
        animation: 131,
        start_quad: 6,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 50,
        blocking: 1,
        chain_idle: 71,
        chain_attack: 71,
        trail_len: 200,
    }, // 6
    SaberMove {
        name: "BR2TL Att",
        animation: 130,
        start_quad: 0,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 72,
        chain_attack: 72,
        trail_len: 200,
    }, // 7
    SaberMove {
        name: "R2L Att",
        animation: 128,
        start_quad: 1,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 73,
        chain_attack: 73,
        trail_len: 200,
    }, // 8
    SaberMove {
        name: "TR2BL Att",
        animation: 132,
        start_quad: 2,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 74,
        chain_attack: 74,
        trail_len: 200,
    }, // 9
    SaberMove {
        name: "T2B Att",
        animation: 126,
        start_quad: 3,
        end_quad: 7,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 75,
        chain_attack: 75,
        trail_len: 200,
    }, // 10
    SaberMove {
        name: "Back Stab",
        animation: 854,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 11
    SaberMove {
        name: "Back Att",
        animation: 855,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 12
    SaberMove {
        name: "CR Back Att",
        animation: 860,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 13
    SaberMove {
        name: "RollStab",
        animation: 914,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 14
    SaberMove {
        name: "Lunge Att",
        animation: 859,
        start_quad: 7,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 15
    SaberMove {
        name: "Jump Att",
        animation: 858,
        start_quad: 3,
        end_quad: 7,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 16
    SaberMove {
        name: "Flip Stab",
        animation: 857,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 95,
        trail_len: 200,
    }, // 17
    SaberMove {
        name: "Flip Slash",
        animation: 856,
        start_quad: 5,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 84,
        trail_len: 200,
    }, // 18
    SaberMove {
        name: "DualJump Atk",
        animation: 861,
        start_quad: 1,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 114,
        trail_len: 200,
    }, // 19
    SaberMove {
        name: "DualJumpAtkL_A",
        animation: 1201,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 4,
        trail_len: 200,
    }, // 20
    SaberMove {
        name: "DualJumpAtkR_A",
        animation: 1202,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 9,
        trail_len: 200,
    }, // 21
    SaberMove {
        name: "DualJumpAtkL_A",
        animation: 1203,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 100,
        trail_len: 200,
    }, // 22
    SaberMove {
        name: "DualJumpAtkR_A",
        animation: 1204,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 93,
        trail_len: 200,
    }, // 23
    SaberMove {
        name: "DualJumpAtkLStaff",
        animation: 1259,
        start_quad: 1,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 107,
        trail_len: 200,
    }, // 24
    SaberMove {
        name: "DualJumpAtkRStaff",
        animation: 1258,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 86,
        trail_len: 200,
    }, // 25
    SaberMove {
        name: "ButterflyLeft",
        animation: 1209,
        start_quad: 1,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 107,
        trail_len: 200,
    }, // 26
    SaberMove {
        name: "ButterflyRight",
        animation: 1210,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 86,
        trail_len: 200,
    }, // 27
    SaberMove {
        name: "BkFlip Atk",
        animation: 862,
        start_quad: 7,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 95,
        trail_len: 200,
    }, // 28
    SaberMove {
        name: "DualSpinAtk",
        animation: 863,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 29
    SaberMove {
        name: "StfSpinAtk",
        animation: 864,
        start_quad: 5,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 30
    SaberMove {
        name: "LngLeapAtk",
        animation: 870,
        start_quad: 1,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 31
    SaberMove {
        name: "SwoopAtkR",
        animation: 1049,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 32
    SaberMove {
        name: "SwoopAtkL",
        animation: 1048,
        start_quad: 5,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 33
    SaberMove {
        name: "TauntaunAtkR",
        animation: 1087,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 34
    SaberMove {
        name: "TauntaunAtkL",
        animation: 1086,
        start_quad: 5,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 35
    SaberMove {
        name: "StfKickFwd",
        animation: 887,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 36
    SaberMove {
        name: "StfKickBack",
        animation: 888,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 37
    SaberMove {
        name: "StfKickRight",
        animation: 889,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 38
    SaberMove {
        name: "StfKickLeft",
        animation: 890,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 39
    SaberMove {
        name: "StfKickSpin",
        animation: 891,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 40
    SaberMove {
        name: "StfKickBkFwd",
        animation: 892,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 41
    SaberMove {
        name: "StfKickSplit",
        animation: 894,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 42
    SaberMove {
        name: "StfKickFwdAir",
        animation: 895,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 43
    SaberMove {
        name: "StfKickBackAir",
        animation: 896,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 44
    SaberMove {
        name: "StfKickRightAir",
        animation: 897,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 45
    SaberMove {
        name: "StfKickLeftAir",
        animation: 898,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 46
    SaberMove {
        name: "StabDown",
        animation: 906,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 47
    SaberMove {
        name: "StabDownStf",
        animation: 907,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 48
    SaberMove {
        name: "StabDownDual",
        animation: 908,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 66,
        trail_len: 200,
    }, // 49
    SaberMove {
        name: "dualspinprot",
        animation: 909,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 500,
    }, // 50
    SaberMove {
        name: "StfSoulCal",
        animation: 910,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 500,
    }, // 51
    SaberMove {
        name: "specialfast",
        animation: 911,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 2000,
    }, // 52
    SaberMove {
        name: "specialmed",
        animation: 912,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 2000,
    }, // 53
    SaberMove {
        name: "specialstr",
        animation: 913,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 2000,
    }, // 54
    SaberMove {
        name: "upsidedwnatk",
        animation: 899,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 55
    SaberMove {
        name: "pullatkstab",
        animation: 902,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 56
    SaberMove {
        name: "pullatkswing",
        animation: 903,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 57
    SaberMove {
        name: "AloraSpinAtk",
        animation: 1273,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 58
    SaberMove {
        name: "Dual FB Atk",
        animation: 1264,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 59
    SaberMove {
        name: "Dual LR Atk",
        animation: 1265,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 60
    SaberMove {
        name: "StfHiltBash",
        animation: 1266,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 61
    SaberMove {
        name: "TL2BR St",
        animation: 178,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 4,
        chain_attack: 4,
        trail_len: 200,
    }, // 62
    SaberMove {
        name: "L2R St",
        animation: 176,
        start_quad: 1,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 5,
        chain_attack: 5,
        trail_len: 200,
    }, // 63
    SaberMove {
        name: "BL2TR St",
        animation: 180,
        start_quad: 1,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 6,
        chain_attack: 6,
        trail_len: 200,
    }, // 64
    SaberMove {
        name: "BR2TL St",
        animation: 179,
        start_quad: 1,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 7,
        chain_attack: 7,
        trail_len: 200,
    }, // 65
    SaberMove {
        name: "R2L St",
        animation: 177,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 8,
        chain_attack: 8,
        trail_len: 200,
    }, // 66
    SaberMove {
        name: "TR2BL St",
        animation: 181,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 9,
        chain_attack: 9,
        trail_len: 200,
    }, // 67
    SaberMove {
        name: "T2B St",
        animation: 175,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 1,
        chain_idle: 10,
        chain_attack: 10,
        trail_len: 200,
    }, // 68
    SaberMove {
        name: "TL2BR Ret",
        animation: 186,
        start_quad: 0,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 69
    SaberMove {
        name: "L2R Ret",
        animation: 184,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 70
    SaberMove {
        name: "BL2TR Ret",
        animation: 188,
        start_quad: 2,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 71
    SaberMove {
        name: "BR2TL Ret",
        animation: 185,
        start_quad: 4,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 72
    SaberMove {
        name: "R2L Ret",
        animation: 183,
        start_quad: 5,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 73
    SaberMove {
        name: "TR2BL Ret",
        animation: 187,
        start_quad: 6,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 74
    SaberMove {
        name: "T2B Ret",
        animation: 182,
        start_quad: 7,
        end_quad: 1,
        animation_flags: 2,
        blend_time: 100,
        blocking: 1,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 200,
    }, // 75
    SaberMove {
        name: "BR2R Trans",
        animation: 133,
        start_quad: 0,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 8,
        trail_len: 150,
    }, // 76
    SaberMove {
        name: "BR2TR Trans",
        animation: 160,
        start_quad: 0,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 77
    SaberMove {
        name: "BR2T Trans",
        animation: 161,
        start_quad: 0,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 78
    SaberMove {
        name: "BR2TL Trans",
        animation: 134,
        start_quad: 0,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 79
    SaberMove {
        name: "BR2L Trans",
        animation: 135,
        start_quad: 0,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 5,
        trail_len: 150,
    }, // 80
    SaberMove {
        name: "BR2BL Trans",
        animation: 136,
        start_quad: 0,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 81
    SaberMove {
        name: "R2BR Trans",
        animation: 162,
        start_quad: 1,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 82
    SaberMove {
        name: "R2TR Trans",
        animation: 137,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 83
    SaberMove {
        name: "R2T Trans",
        animation: 163,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 84
    SaberMove {
        name: "R2TL Trans",
        animation: 138,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 85
    SaberMove {
        name: "R2L Trans",
        animation: 139,
        start_quad: 1,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 5,
        trail_len: 150,
    }, // 86
    SaberMove {
        name: "R2BL Trans",
        animation: 140,
        start_quad: 1,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 87
    SaberMove {
        name: "TR2BR Trans",
        animation: 141,
        start_quad: 2,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 88
    SaberMove {
        name: "TR2R Trans",
        animation: 164,
        start_quad: 2,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 8,
        trail_len: 150,
    }, // 89
    SaberMove {
        name: "TR2T Trans",
        animation: 165,
        start_quad: 2,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 90
    SaberMove {
        name: "TR2TL Trans",
        animation: 142,
        start_quad: 2,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 91
    SaberMove {
        name: "TR2L Trans",
        animation: 143,
        start_quad: 2,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 5,
        trail_len: 150,
    }, // 92
    SaberMove {
        name: "TR2BL Trans",
        animation: 144,
        start_quad: 2,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 93
    SaberMove {
        name: "T2BR Trans",
        animation: 145,
        start_quad: 3,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 94
    SaberMove {
        name: "T2R Trans",
        animation: 146,
        start_quad: 3,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 8,
        trail_len: 150,
    }, // 95
    SaberMove {
        name: "T2TR Trans",
        animation: 147,
        start_quad: 3,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 96
    SaberMove {
        name: "T2TL Trans",
        animation: 148,
        start_quad: 3,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 97
    SaberMove {
        name: "T2L Trans",
        animation: 149,
        start_quad: 3,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 5,
        trail_len: 150,
    }, // 98
    SaberMove {
        name: "T2BL Trans",
        animation: 150,
        start_quad: 3,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 99
    SaberMove {
        name: "TL2BR Trans",
        animation: 151,
        start_quad: 4,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 100
    SaberMove {
        name: "TL2R Trans",
        animation: 166,
        start_quad: 4,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 8,
        trail_len: 150,
    }, // 101
    SaberMove {
        name: "TL2TR Trans",
        animation: 167,
        start_quad: 4,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 102
    SaberMove {
        name: "TL2T Trans",
        animation: 168,
        start_quad: 4,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 103
    SaberMove {
        name: "TL2L Trans",
        animation: 169,
        start_quad: 4,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 5,
        trail_len: 150,
    }, // 104
    SaberMove {
        name: "TL2BL Trans",
        animation: 152,
        start_quad: 4,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 105
    SaberMove {
        name: "L2BR Trans",
        animation: 153,
        start_quad: 5,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 106
    SaberMove {
        name: "L2R Trans",
        animation: 154,
        start_quad: 5,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 8,
        trail_len: 150,
    }, // 107
    SaberMove {
        name: "L2TR Trans",
        animation: 170,
        start_quad: 5,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 108
    SaberMove {
        name: "L2T Trans",
        animation: 171,
        start_quad: 5,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 109
    SaberMove {
        name: "L2TL Trans",
        animation: 155,
        start_quad: 5,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 110
    SaberMove {
        name: "L2BL Trans",
        animation: 172,
        start_quad: 5,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 111
    SaberMove {
        name: "BL2BR Trans",
        animation: 156,
        start_quad: 6,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 112
    SaberMove {
        name: "BL2R Trans",
        animation: 157,
        start_quad: 6,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 8,
        trail_len: 150,
    }, // 113
    SaberMove {
        name: "BL2TR Trans",
        animation: 158,
        start_quad: 6,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 114
    SaberMove {
        name: "BL2T Trans",
        animation: 173,
        start_quad: 6,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 115
    SaberMove {
        name: "BL2TL Trans",
        animation: 174,
        start_quad: 6,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 116
    SaberMove {
        name: "BL2L Trans",
        animation: 159,
        start_quad: 6,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 5,
        trail_len: 150,
    }, // 117
    SaberMove {
        name: "Bounce BR",
        animation: 189,
        start_quad: 0,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 77,
        trail_len: 150,
    }, // 118
    SaberMove {
        name: "Bounce R",
        animation: 190,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 86,
        trail_len: 150,
    }, // 119
    SaberMove {
        name: "Bounce TR",
        animation: 191,
        start_quad: 2,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 91,
        trail_len: 150,
    }, // 120
    SaberMove {
        name: "Bounce T",
        animation: 192,
        start_quad: 3,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 99,
        trail_len: 150,
    }, // 121
    SaberMove {
        name: "Bounce TL",
        animation: 193,
        start_quad: 4,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 102,
        trail_len: 150,
    }, // 122
    SaberMove {
        name: "Bounce L",
        animation: 194,
        start_quad: 5,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 107,
        trail_len: 150,
    }, // 123
    SaberMove {
        name: "Bounce BL",
        animation: 195,
        start_quad: 6,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 114,
        trail_len: 150,
    }, // 124
    SaberMove {
        name: "Deflect BR",
        animation: 196,
        start_quad: 0,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 69,
        chain_attack: 77,
        trail_len: 150,
    }, // 125
    SaberMove {
        name: "Deflect R",
        animation: 197,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 70,
        chain_attack: 86,
        trail_len: 150,
    }, // 126
    SaberMove {
        name: "Deflect TR",
        animation: 198,
        start_quad: 2,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 91,
        trail_len: 150,
    }, // 127
    SaberMove {
        name: "Deflect T",
        animation: 192,
        start_quad: 3,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 99,
        trail_len: 150,
    }, // 128
    SaberMove {
        name: "Deflect TL",
        animation: 199,
        start_quad: 4,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 72,
        chain_attack: 102,
        trail_len: 150,
    }, // 129
    SaberMove {
        name: "Deflect L",
        animation: 200,
        start_quad: 5,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 73,
        chain_attack: 107,
        trail_len: 150,
    }, // 130
    SaberMove {
        name: "Deflect BL",
        animation: 201,
        start_quad: 6,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 74,
        chain_attack: 114,
        trail_len: 150,
    }, // 131
    SaberMove {
        name: "Deflect B",
        animation: 202,
        start_quad: 7,
        end_quad: 7,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 71,
        chain_attack: 99,
        trail_len: 150,
    }, // 132
    SaberMove {
        name: "Reflected BR",
        animation: 676,
        start_quad: 0,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 133
    SaberMove {
        name: "Reflected R",
        animation: 677,
        start_quad: 1,
        end_quad: 1,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 134
    SaberMove {
        name: "Reflected TR",
        animation: 678,
        start_quad: 2,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 135
    SaberMove {
        name: "Reflected T",
        animation: 679,
        start_quad: 3,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 136
    SaberMove {
        name: "Reflected TL",
        animation: 680,
        start_quad: 4,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 137
    SaberMove {
        name: "Reflected L",
        animation: 681,
        start_quad: 5,
        end_quad: 5,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 138
    SaberMove {
        name: "Reflected BL",
        animation: 682,
        start_quad: 6,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 139
    SaberMove {
        name: "Reflected B",
        animation: 683,
        start_quad: 7,
        end_quad: 7,
        animation_flags: 11,
        blend_time: 100,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 140
    SaberMove {
        name: "BParry Top",
        animation: 684,
        start_quad: 3,
        end_quad: 7,
        animation_flags: 11,
        blend_time: 50,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 141
    SaberMove {
        name: "BParry UR",
        animation: 685,
        start_quad: 2,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 50,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 142
    SaberMove {
        name: "BParry UL",
        animation: 686,
        start_quad: 4,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 50,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 143
    SaberMove {
        name: "BParry LR",
        animation: 687,
        start_quad: 6,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 50,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 144
    SaberMove {
        name: "BParry Bot",
        animation: 688,
        start_quad: 7,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 50,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 145
    SaberMove {
        name: "BParry LL",
        animation: 689,
        start_quad: 0,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 50,
        blocking: 0,
        chain_idle: 1,
        chain_attack: 1,
        trail_len: 150,
    }, // 146
    SaberMove {
        name: "Knock Top",
        animation: 670,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 71,
        chain_attack: 94,
        trail_len: 150,
    }, // 147
    SaberMove {
        name: "Knock UR",
        animation: 671,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 71,
        chain_attack: 89,
        trail_len: 150,
    }, // 148
    SaberMove {
        name: "Knock UL",
        animation: 672,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 72,
        chain_attack: 104,
        trail_len: 150,
    }, // 149
    SaberMove {
        name: "Knock LR",
        animation: 673,
        start_quad: 1,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 69,
        chain_attack: 116,
        trail_len: 150,
    }, // 150
    SaberMove {
        name: "Knock LL",
        animation: 675,
        start_quad: 1,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 74,
        chain_attack: 77,
        trail_len: 150,
    }, // 151
    SaberMove {
        name: "Parry Top",
        animation: 665,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 150,
    }, // 152
    SaberMove {
        name: "Parry UR",
        animation: 666,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 150,
    }, // 153
    SaberMove {
        name: "Parry UL",
        animation: 667,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 150,
    }, // 154
    SaberMove {
        name: "Parry LR",
        animation: 668,
        start_quad: 1,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 150,
    }, // 155
    SaberMove {
        name: "Parry LL",
        animation: 669,
        start_quad: 1,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 150,
    }, // 156
    SaberMove {
        name: "Reflect Top",
        animation: 665,
        start_quad: 1,
        end_quad: 3,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 71,
        chain_attack: 10,
        trail_len: 300,
    }, // 157
    SaberMove {
        name: "Reflect UR",
        animation: 667,
        start_quad: 1,
        end_quad: 2,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 72,
        chain_attack: 4,
        trail_len: 300,
    }, // 158
    SaberMove {
        name: "Reflect UL",
        animation: 666,
        start_quad: 1,
        end_quad: 4,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 71,
        chain_attack: 9,
        trail_len: 300,
    }, // 159
    SaberMove {
        name: "Reflect LR",
        animation: 669,
        start_quad: 1,
        end_quad: 6,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 74,
        chain_attack: 6,
        trail_len: 300,
    }, // 160
    SaberMove {
        name: "Reflect LL",
        animation: 668,
        start_quad: 1,
        end_quad: 0,
        animation_flags: 11,
        blend_time: 50,
        blocking: 2,
        chain_idle: 69,
        chain_attack: 7,
        trail_len: 300,
    }, // 161
];

/// Look up one move without accepting the `LS_MOVE_MAX` sentinel.
pub fn saber_move(index: u16) -> Option<&'static SaberMove> {
    SABER_MOVES.get(usize::from(index))
}

/// `saberMoveData[movement].animToUse` after `BG_FixSaberMoveData` (`bg_saber.c:351-384`):
/// with the fix (`g_fixSaberMoveData`, on by default and told to clients through
/// `CS_LEGACY_FIXES`) the lower parries, broken parries and knockaways play the poses of
/// their own side instead of the other's.
pub fn move_animation(movement: u16, fixed: bool) -> u16 {
    let unfixed = SABER_MOVES[usize::from(movement)].animation;
    if !fixed {
        return unfixed;
    }
    match movement {
        // "BParry LR", "BParry LL": `BOTH_H1_S1_BR`, `BOTH_H1_S1_BL`.
        144 => 689,
        146 => 687,
        // "Knock LR", "Knock LL": `BOTH_K1_S1_BR`, `BOTH_K1_S1_BL`.
        150 => 675,
        151 => 673,
        // "Parry LR", "Parry LL": `BOTH_P1_S1_BR`, `BOTH_P1_S1_BL`.
        155 => 669,
        156 => 668,
        _ => unfixed,
    }
}

/// Numeric `saberMoveName_t` values used by the compatibility predictor.
pub(crate) mod movement {
    pub const LS_NONE: u16 = 0;
    pub const LS_READY: u16 = 1;
    pub const LS_DRAW: u16 = 2;
    pub const LS_PUTAWAY: u16 = 3;
    pub const LS_A_TL2BR: u16 = 4;
    pub const LS_A_L2R: u16 = 5;
    pub const LS_A_BL2TR: u16 = 6;
    pub const LS_A_BR2TL: u16 = 7;
    pub const LS_A_R2L: u16 = 8;
    pub const LS_A_TR2BL: u16 = 9;
    pub const LS_A_T2B: u16 = 10;
    pub const LS_A_BACKSTAB: u16 = 11;
    pub const LS_A_BACK: u16 = 12;
    pub const LS_A_BACK_CR: u16 = 13;
    pub const LS_ROLL_STAB: u16 = 14;
    pub const LS_A_LUNGE: u16 = 15;
    pub const LS_A_JUMP_T__B_: u16 = 16;
    pub const LS_A_FLIP_STAB: u16 = 17;
    pub const LS_A_FLIP_SLASH: u16 = 18;
    pub const LS_JUMPATTACK_DUAL: u16 = 19;
    pub const LS_JUMPATTACK_ARIAL_LEFT: u16 = 20;
    pub const LS_JUMPATTACK_ARIAL_RIGHT: u16 = 21;
    pub const LS_JUMPATTACK_CART_LEFT: u16 = 22;
    pub const LS_JUMPATTACK_CART_RIGHT: u16 = 23;
    pub const LS_JUMPATTACK_STAFF_LEFT: u16 = 24;
    pub const LS_JUMPATTACK_STAFF_RIGHT: u16 = 25;
    pub const LS_BUTTERFLY_LEFT: u16 = 26;
    pub const LS_BUTTERFLY_RIGHT: u16 = 27;
    pub const LS_A_BACKFLIP_ATK: u16 = 28;
    pub const LS_SPINATTACK_DUAL: u16 = 29;
    pub const LS_SPINATTACK: u16 = 30;
    pub const LS_LEAP_ATTACK: u16 = 31;

    pub const LS_KICK_F: u16 = 36;
    pub const LS_KICK_B: u16 = 37;
    pub const LS_KICK_R: u16 = 38;
    pub const LS_KICK_L: u16 = 39;

    pub const LS_KICK_F_AIR: u16 = 43;
    pub const LS_KICK_B_AIR: u16 = 44;
    pub const LS_KICK_R_AIR: u16 = 45;
    pub const LS_KICK_L_AIR: u16 = 46;
    pub const LS_STABDOWN: u16 = 47;
    pub const LS_STABDOWN_STAFF: u16 = 48;
    pub const LS_STABDOWN_DUAL: u16 = 49;
    pub const LS_DUAL_SPIN_PROTECT: u16 = 50;
    pub const LS_STAFF_SOULCAL: u16 = 51;
    pub const LS_A1_SPECIAL: u16 = 52;
    pub const LS_A2_SPECIAL: u16 = 53;
    pub const LS_A3_SPECIAL: u16 = 54;
    pub const LS_UPSIDE_DOWN_ATTACK: u16 = 55;
    pub const LS_PULL_ATTACK_STAB: u16 = 56;
    pub const LS_PULL_ATTACK_SWING: u16 = 57;

    pub const LS_DUAL_FB: u16 = 59;
    pub const LS_DUAL_LR: u16 = 60;
    pub const LS_HILT_BASH: u16 = 61;
    pub const LS_S_TL2BR: u16 = 62;
    pub const LS_S_L2R: u16 = 63;

    pub const LS_S_R2L: u16 = 66;

    pub const LS_S_T2B: u16 = 68;
    pub const LS_R_TL2BR: u16 = 69;

    pub const LS_R_BL2TR: u16 = 71;
    pub const LS_R_BR2TL: u16 = 72;

    pub const LS_R_T2B: u16 = 75;
    pub const LS_T1_BR__R: u16 = 76;

    pub const LS_T1_BL__L: u16 = 117;

    pub const LS_D1_BR: u16 = 125;

    pub const LS_D1_B_: u16 = 132;
    pub const LS_V1_BR: u16 = 133;

    pub const LS_V1_BL: u16 = 139;
    pub const LS_V1_B_: u16 = 140;
    pub const LS_H1_T_: u16 = 141;
    pub const LS_H1_TR: u16 = 142;
    pub const LS_H1_TL: u16 = 143;
    pub const LS_H1_BR: u16 = 144;

    pub const LS_H1_BL: u16 = 146;
    pub const LS_K1_T_: u16 = 147;

    pub const LS_K1_BL: u16 = 151;
    pub const LS_PARRY_UP: u16 = 152;
    pub const LS_PARRY_UR: u16 = 153;
    pub const LS_PARRY_UL: u16 = 154;
    pub const LS_PARRY_LR: u16 = 155;
    pub const LS_PARRY_LL: u16 = 156;
    pub const LS_REFLECT_UP: u16 = 157;
    pub const LS_REFLECT_UR: u16 = 158;
    pub const LS_REFLECT_UL: u16 = 159;
    pub const LS_REFLECT_LR: u16 = 160;
    pub const LS_REFLECT_LL: u16 = 161;
}

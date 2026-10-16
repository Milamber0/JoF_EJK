//! `g_debugMelee`: the server's switch for melee kicks and the grapple, and for holding
//! a grabbed wall until jump is let go.
//!
//! Stock `codemp` reads the `CS_SERVERINFO` value into `pm->debugMelee`
//! (`cg_servercmds.c:137`, `cg_predict.c:1246`; the game copies `g_debugMelee` at
//! `g_active.c`), and any nonzero value turns on both the kicks in `PM_Weapon`
//! (`bg_pmove.c:7464-7581`) and the hold in `PM_AdjustAngleForWallJump`
//! (`bg_pmove.c:1621-1641`).
//!
//! JA+ splits them: its documentation (`japlus_doc/server_cvars.rtf`, JA+ 2.4) defines
//! 0 as neither, 1 as the melee attacks only and 2 as both, and a grabbed wall no
//! longer turns the player to face it ("Free mouse look when stuck to a wall",
//! `history.rtf`): a JA+ 2.4 server keeps the player's own yaw and pitch while the
//! wall is held (observed approaching a wall 30 degrees off square). EternalJK's JA+
//! client code reads the levels the same way (`codemp/game/bg_pmove.c:1871-1910`,
//! `cgs.serverMod >= SVMOD_JAPLUS`) but frees only the pitch, which the server does not
//! match; its jaPRO server code applies the JA+ levels unconditionally
//! (`#ifdef _GAME if (1)`), so the jaPRO-lineage TaystJK dialect takes the levels too,
//! without the free look. On JA+ an alternate attack standing
//! still is a front kick rather than nothing (EternalJK `bg_saber.c`
//! `PM_KickMoveForConditions`, `cgs.serverMod == SVMOD_JAPLUS`; observed on JA+ 2.4).

use sjk_protocol::{GameState, InfoString, ServerDialect, ServerProfile, UserCommand};

use crate::pmove::{MovementCollision, MovementState};
use crate::pmove_anim::{AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use crate::saber_move_data::movement::{
    LS_HILT_BASH, LS_KICK_B, LS_KICK_B_AIR, LS_KICK_F, LS_KICK_F_AIR, LS_KICK_L, LS_KICK_L_AIR,
    LS_KICK_R, LS_KICK_R_AIR,
};

const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `MASK_SOLID`: `CONTENTS_SOLID | CONTENTS_TERRAIN` (`bg_public.h:1225`).
const MASK_SOLID: u32 = 0x1 | 0x1000;
const ENTITY_NONE: u16 = sjk_protocol::ENTITY_NUMBER_NONE;

/// How the server's `g_debugMelee` reads for prediction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DebugMelee {
    /// `atoi` of `CS_SERVERINFO` `g_debugMelee`; 0 when the server omits it.
    pub level: i32,
    /// JA+ and jaPRO hold a wall only from level 2; stock at any nonzero level.
    pub split_levels: bool,
    /// JA+ leaves the view to the player while a grabbed wall is held.
    pub free_wall_look: bool,
    /// JA+ kicks forward when no move picks a kick.
    pub standing_kick: bool,
}

impl DebugMelee {
    /// The value and its dialect's reading from `CS_SERVERINFO`.
    pub fn from_game_state(game: &GameState) -> Self {
        game.config_string(0)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| InfoString::parse(text).ok())
            .map_or_else(Self::default, |info| Self::from_server_info(&info))
    }

    /// [`Self::from_game_state`] from an already parsed serverinfo string.
    pub fn from_server_info(info: &InfoString) -> Self {
        let level = info.get("g_debugMelee").map_or(0, atoi);
        match ServerProfile::from_server_info(info).dialect {
            ServerDialect::JaPlus { .. } => Self {
                level,
                split_levels: true,
                free_wall_look: true,
                standing_kick: true,
            },
            ServerDialect::TaystJk { .. } => Self {
                level,
                split_levels: true,
                ..Self::default()
            },
            ServerDialect::BaseJka | ServerDialect::Unknown { .. } => Self {
                level,
                ..Self::default()
            },
        }
    }

    /// The melee kicks and the grapple of `PM_Weapon` (`if (pm->debugMelee && ...)`).
    pub fn melee_moves(self) -> bool {
        self.level != 0
    }

    /// Holding a grabbed wall while jump is held (stock `if (pm->debugMelee)`, JA+
    /// `if (pm->debugMelee > 1)`).
    pub fn holds_walls(self) -> bool {
        if self.split_levels {
            self.level > 1
        } else {
            self.level != 0
        }
    }
}

/// `atoi`: leading blanks, an optional sign, then digits; 0 without any.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let magnitude = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0_i64, |value, digit| {
            (value * 10 + i64::from(digit - b'0')).min(i64::from(i32::MAX) + 1)
        });
    let value = if negative { -magnitude } else { magnitude };
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// What the melee attack does under `g_debugMelee` (`bg_pmove.c:7464-7581`).
pub(crate) enum MeleeMove {
    /// Neither button combination of the debug moves: the stock punches.
    Punch,
    /// The command is over for the weapon: `PM_Weapon` returns before firing.
    Done,
}

/// The melee branch of `PM_Weapon` with `g_debugMelee` on, for a player not riding.
///
/// Attack with alternate attack is the grapple: the game tries `TryGrapple`, which a
/// client cannot predict, so the client's `PM_Weapon` returns there (`#else return;`).
/// Alternate attack alone is a kick the way the player moves (`PM_KickMoveForConditions`,
/// `bg_saber.c:2561-2635`; a hilt bash becomes a front kick), in the air only well above
/// the ground; with no kick to do the torso falls in with the legs. Either way the
/// weapon is idle and nothing is fired.
pub(crate) fn melee(
    state: &mut MovementState,
    command: &mut UserCommand,
    rules: DebugMelee,
    lengths: Option<&dyn AnimationLengths>,
    collision: Option<&dyn MovementCollision>,
    bounds: ([f32; 3], [f32; 3]),
    fixed_moves: bool,
) -> MeleeMove {
    if command.buttons & BUTTON_ATTACK != 0 && command.buttons & BUTTON_ALT_ATTACK != 0 {
        return MeleeMove::Done;
    }
    if command.buttons & BUTTON_ALT_ATTACK == 0 {
        return MeleeMove::Punch;
    }
    let Some(lengths) = lengths else {
        // Not predicted without the animation table (`predicts_rider_command`).
        return MeleeMove::Done;
    };
    if !crate::pmove_input_freeze::kicking_animation(state.torso_anim)
        && !crate::pmove_input_freeze::kicking_animation(state.legs_anim)
    {
        let mut kick = kick_for_conditions(command, rules.standing_kick).map(|kick| {
            if kick == LS_HILT_BASH {
                LS_KICK_F
            } else {
                kick
            }
        });
        if let Some(chosen) = kick
            && state.ground_entity_number == ENTITY_NONE
        {
            let ground = ground_distance(state, collision, bounds);
            kick = if (!crate::pmove_roll_anim::flipping(state.legs_anim) || state.legs_timer <= 0)
                && ground > 64.0
                && ground > -state.velocity[2] - 64.0
            {
                match chosen {
                    LS_KICK_F => Some(LS_KICK_F_AIR),
                    LS_KICK_B => Some(LS_KICK_B_AIR),
                    LS_KICK_R => Some(LS_KICK_R_AIR),
                    LS_KICK_L => Some(LS_KICK_L_AIR),
                    _ => None,
                }
            } else {
                None
            };
        }
        if let Some(kick) = kick {
            let animation = crate::saber_move_data::move_animation(kick, fixed_moves);
            crate::pmove_anim::set_animation(
                state,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                lengths,
            );
            if state.legs_anim == animation {
                state.weapon_time = state.legs_timer;
                return MeleeMove::Done;
            }
        }
    }
    if state.torso_anim != state.legs_anim {
        crate::pmove_anim::set_animation(
            state,
            SETANIM_BOTH,
            state.legs_anim,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            lengths,
        );
    }
    state.weapon_time = 0;
    MeleeMove::Done
}

/// `PM_KickMoveForConditions` (`bg_saber.c:2561-2635`): a side kick when strafing, else
/// a front or back kick when moving; the move it reads is spent. Standing still, none —
/// or a front kick where `standing` (JA+).
fn kick_for_conditions(command: &mut UserCommand, standing: bool) -> Option<u16> {
    if command.right_move != 0 {
        let kick = if command.right_move > 0 {
            LS_KICK_R
        } else {
            LS_KICK_L
        };
        command.right_move = 0;
        Some(kick)
    } else if command.forward_move != 0 {
        let kick = if command.forward_move > 0 {
            LS_KICK_F
        } else {
            LS_KICK_B
        };
        command.forward_move = 0;
        Some(kick)
    } else {
        standing.then_some(LS_KICK_F)
    }
}

/// `PM_GroundDistance` (`bg_saber.c:1936-1949`): straight down with the move's box.
fn ground_distance(
    state: &MovementState,
    collision: Option<&dyn MovementCollision>,
    (minimums, maximums): ([f32; 3], [f32; 3]),
) -> f32 {
    let origin = state.origin;
    let down = [origin[0], origin[1], origin[2] - 4_096.0];
    let end = collision.map_or(down, |collision| {
        collision
            .trace(origin, minimums, maximums, down, MASK_SOLID)
            .end_position
    });
    let apart = [origin[0] - end[0], origin[1] - end[1], origin[2] - end[2]];
    (apart[0] * apart[0] + apart[1] * apart[1] + apart[2] * apart[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(text: &str) -> InfoString {
        InfoString::parse(text).unwrap()
    }

    #[test]
    fn stock_levels_turn_on_everything() {
        let stock = DebugMelee::from_server_info(&info("\\g_debugMelee\\1\\gamename\\basejka"));
        assert!(stock.melee_moves() && stock.holds_walls() && !stock.free_wall_look);
        let off = DebugMelee::from_server_info(&info("\\gamename\\basejka"));
        assert!(!off.melee_moves() && !off.holds_walls());
        // `if (pm->debugMelee)`: any nonzero value, a negative one too.
        let negative = DebugMelee::from_server_info(&info("\\g_debugMelee\\-1"));
        assert!(negative.melee_moves() && negative.holds_walls());
    }

    #[test]
    fn ja_plus_holds_walls_from_level_two() {
        let one = DebugMelee::from_server_info(&info(
            "\\g_debugMelee\\1\\gamename\\JA+ Mod v2.4 Build 7\\jp_cinfo\\196819",
        ));
        assert!(one.melee_moves() && !one.holds_walls() && one.free_wall_look);
        assert!(one.standing_kick);
        let two = DebugMelee::from_server_info(&info(
            "\\g_debugMelee\\2\\gamename\\JA+ Mod v2.4 Build 7",
        ));
        assert!(two.melee_moves() && two.holds_walls());
        let japro = DebugMelee::from_server_info(&info("\\g_debugMelee\\1\\gamename\\japro"));
        assert!(japro.melee_moves() && !japro.holds_walls() && !japro.free_wall_look);
        assert!(!japro.standing_kick);
    }

    #[test]
    fn levels_read_like_atoi() {
        assert_eq!(atoi(" 2x"), 2);
        assert_eq!(atoi("-3"), -3);
        assert_eq!(atoi("+1"), 1);
        assert_eq!(atoi("abc"), 0);
        assert_eq!(atoi(""), 0);
    }

    #[test]
    fn kicks_follow_the_move_and_spend_it() {
        let mut command = UserCommand {
            right_move: -127,
            forward_move: 127,
            ..UserCommand::default()
        };
        assert_eq!(kick_for_conditions(&mut command, false), Some(LS_KICK_L));
        assert_eq!((command.right_move, command.forward_move), (0, 127));
        assert_eq!(kick_for_conditions(&mut command, false), Some(LS_KICK_F));
        assert_eq!(command.forward_move, 0);
        assert_eq!(kick_for_conditions(&mut command, false), None);
        // JA+: standing still is a front kick.
        assert_eq!(kick_for_conditions(&mut command, true), Some(LS_KICK_F));
    }
}

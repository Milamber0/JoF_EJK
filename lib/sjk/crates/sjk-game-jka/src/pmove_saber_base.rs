//! `fd.saberAnimLevelBase` for a client's own prediction.
//!
//! The base style is no wire field. A server keeps it (`g_active.c`, `g_cmds.c`), but a
//! client has to work it out for every command it predicts, which `CG_PredictPlayerState`
//! does just before `Pmove` (`cg_predict.c:1335-1347` in EternalJK and JoF EJK, whose
//! `cgame` keeps stock's rule):
//!
//! - the base is the current style;
//! - unless one blade or saber is off (`saberHolstered == 1`): then a client whose first
//!   saber has blades uses the staff base, and otherwise one that holds a second saber the
//!   dual base.
//!
//! Without it the base reads 0, `BG_SabersOff` takes one blade off for all of them and a
//! swing is turned into a draw (`EV_SABER_UNHOLSTER`) that the server never agrees to.

use crate::pmove::MovementState;

/// `SS_DUAL`, `SS_STAFF`.
const SS_DUAL: u8 = 6;
const SS_STAFF: u8 = 7;

/// What a client knows of the sabers it holds: `ci->saber[0].numBlades` and
/// `ci->saber[1].model[0]`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaberHands {
    /// The first saber has blades (`ci->saber[0].numBlades > 0`).
    pub primary_blades: bool,
    /// A second saber is held (`ci->saber[1].model[0]`).
    pub second_saber: bool,
}

impl SaberHands {
    /// The base the client's prediction uses for a player in `level` with `holstered`.
    pub fn base(self, level: u8, holstered: u8) -> u8 {
        if holstered == 1 {
            if self.primary_blades {
                return SS_STAFF;
            }
            if self.second_saber {
                return SS_DUAL;
            }
        }
        level
    }

    /// Sets `state`'s base as `CG_PredictPlayerState` does before a command.
    pub(crate) fn apply(self, state: &mut MovementState) {
        state.saber_anim_level_base = self.base(state.saber_anim_level, state.saber_holstered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmove::{MovementCollision, MovementConfig, MovementTrace, Predictor};
    use sjk_protocol::{PlayerState, UserCommand};

    const SS_FAST: u8 = 1;
    const BUTTON_ATTACK: u16 = 1;
    const EV_SABER_UNHOLSTER: u16 = 33;

    struct Void;

    impl MovementCollision for Void {
        fn trace(
            &self,
            start: [f32; 3],
            _: [f32; 3],
            _: [f32; 3],
            end: [f32; 3],
            _: u32,
        ) -> MovementTrace {
            MovementTrace {
                end_position: end,
                ..MovementTrace::miss(start)
            }
        }
    }

    const BOTH: SaberHands = SaberHands {
        primary_blades: true,
        second_saber: true,
    };

    #[test]
    fn stance_base_follows_the_current_style_unless_a_blade_is_off() {
        for hands in [SaberHands::default(), BOTH] {
            for level in 1..=7 {
                assert_eq!(hands.base(level, 0), level);
                assert_eq!(hands.base(level, 2), level);
            }
        }
    }

    #[test]
    fn stance_base_of_one_blade_off_is_staff_then_dual() {
        assert_eq!(BOTH.base(SS_FAST, 1), SS_STAFF);
        let pair_only = SaberHands {
            primary_blades: false,
            second_saber: true,
        };
        assert_eq!(pair_only.base(SS_FAST, 1), SS_DUAL);
        assert_eq!(SaberHands::default().base(SS_FAST, 1), SS_FAST);
    }

    /// A living saber wielder in the fast style with one blade off, holding attack.
    fn swing(hands: Option<SaberHands>, step: i32) -> (Predictor, Vec<u16>) {
        let mut player = PlayerState::default();
        player.stats[0] = 100;
        player.set_raw_field(37, 250);
        player.set_raw_field(47, 3); // WP_SABER
        player.set_raw_field(23, u32::from(SS_FAST));
        player.set_raw_field(81, 1); // saberHolstered
        let mut predictor = Predictor::from_player_state(
            &player,
            MovementConfig {
                saber_hands: hands,
                ..MovementConfig::default()
            },
        );
        // Saber moves are predicted only where the animation lengths are known.
        predictor.set_animation_lengths(std::sync::Arc::new(crate::AnimationLengthTable::new([])));
        let mut events = Vec::new();
        let mut time = player.command_time();
        for _ in 0..(400 / step) {
            time += step;
            predictor.predict_command(
                UserCommand {
                    server_time: time,
                    buttons: BUTTON_ATTACK,
                    ..UserCommand::default()
                },
                &Void,
            );
            predictor.emit_events(|event| events.push(event.event));
        }
        (predictor, events)
    }

    #[test]
    fn stance_base_keeps_one_blade_off_from_playing_the_saber_on_sound() {
        for step in [8, 7, 4, 3] {
            let (predictor, events) = swing(Some(BOTH), step);
            assert!(
                !events.contains(&EV_SABER_UNHOLSTER),
                "step {step}: {events:?}"
            );
            assert_eq!(predictor.state().saber_holstered, 1, "step {step}");
            assert_eq!(predictor.state().saber_anim_level_base, SS_STAFF);
        }
    }

    #[test]
    fn stance_base_unknown_hands_leave_the_old_behaviour() {
        let (predictor, events) = swing(None, 8);
        assert!(events.contains(&EV_SABER_UNHOLSTER), "{events:?}");
        assert_eq!(predictor.state().saber_holstered, 0);
    }

    #[test]
    fn stance_base_is_not_kept_from_an_earlier_reseed() {
        let (mut predictor, _) = swing(Some(BOTH), 8);
        // The base is worked out again for every command, so a style change shows at once.
        predictor.state_mut().saber_holstered = 0;
        predictor.state_mut().saber_anim_level = SS_DUAL;
        let time = predictor.state().command_time + 8;
        predictor.predict_command(
            UserCommand {
                server_time: time,
                ..UserCommand::default()
            },
            &Void,
        );
        assert_eq!(predictor.state().saber_anim_level_base, SS_DUAL);
    }
}

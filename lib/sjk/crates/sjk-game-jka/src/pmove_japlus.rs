//! JA+ movement and saber rules that a JA+ server runs and its clients predict.
//!
//! JA+ is closed source, so the client side follows EternalJK's reimplementation of the
//! JA+ client plugin (`codemp/game/bg_pmove.c`, `bg_saber.c`, the `SVMOD_JAPLUS` and
//! `JAPLUS_CINFO_*` branches) and, where that and a JA+ server disagree, the server as
//! the replay checks observed it. Some rules follow a `jp_cinfo` bit the server
//! publishes in its serverinfo; others apply on every JA+ server. Stock and other
//! servers, SJK's own included, keep the stock rules ([`JaPlusRules::default`]).
//!
//! Not predicted: the `jp_cinfo` options EternalJK never reads outside its
//! `serverconfig` listing (single-player attacks, the new DFA, model scale, kata,
//! the auto replier, ledge grab, the alternate dimension, macro scan); the Jedi
//! Outcast red DFA (`jp_jk2RedDFA`, off by default), whose jump-hack start sets a saber
//! move from the jump code (`bg_pmove.c:3165-3203`); and the animation stiffening for
//! JA+'s extra GLA animations, which stock animation tables lack.

use sjk_protocol::{GameState, InfoString, JaPlusCapabilities, ServerDialect, ServerProfile};

/// `WP_MELEE`.
const WP_MELEE: u8 = 2;

/// The JA+ rules in force for one session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JaPlusRules {
    /// The server runs JA+ (`cgs.serverMod == SVMOD_JAPLUS`).
    pub enabled: bool,
    /// `JAPLUS_CINFO_FLIPKICK` (`jp_allowFlipKick`): wall flips and runs off players,
    /// and the front flip off a player run at.
    pub flip_kick: bool,
    /// `JAPLUS_CINFO_HEADSLIDE` (`jp_slideOnPlayer`): standing on a player keeps stock's
    /// frictionless slide; without it JA+ applies ground friction there.
    pub head_slide: bool,
    /// `JAPLUS_CINFO_YELLOWDFA` (`jp_improveYellowDFA`): the medium style's flip over
    /// leaps 60 forward instead of 150 and does not spin or lock the view.
    pub yellow_dfa: bool,
}

impl JaPlusRules {
    /// The rules a parsed dialect selects: JA+ ones only for a JA+ server.
    pub fn from_dialect(dialect: &ServerDialect) -> Self {
        let ServerDialect::JaPlus { capabilities } = dialect else {
            return Self::default();
        };
        Self {
            enabled: true,
            flip_kick: capabilities.contains(JaPlusCapabilities::FLIP_KICK),
            head_slide: capabilities.contains(JaPlusCapabilities::HEAD_SLIDE),
            yellow_dfa: capabilities.contains(JaPlusCapabilities::YELLOW_DFA),
        }
    }

    /// Read the dialect from `CS_SERVERINFO`.
    pub fn from_game_state(game_state: &GameState) -> Self {
        game_state
            .config_string(0)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| InfoString::parse(text).ok())
            .map_or_else(Self::default, |info| {
                Self::from_dialect(&ServerProfile::from_server_info(&info).dialect)
            })
    }

    /// Whether standing on a player is frictionless, as stock has it
    /// (`bg_pmove.c:1236-1248`).
    pub(crate) fn slides_on_players(self) -> bool {
        !self.enabled || self.head_slide
    }

    /// The run speed kept while gripping (`bg_pmove.c:10063-10073`): JA+'s
    /// `jp_gripSpeedScale` default 0.8, stock 0.4. The server's own value is not
    /// published, so a changed scale cannot be predicted.
    pub(crate) fn grip_speed_scale(self) -> f32 {
        if self.enabled { 0.8 } else { 0.4 }
    }

    /// Whether an attack pressed with the holdable button cancels both
    /// (`bg_pmove.c:12035-12048`): not for melee on JA+.
    pub(crate) fn cancels_attack_with_holdable(self, weapon: u8) -> bool {
        !(self.enabled && weapon == WP_MELEE)
    }

    /// The medium style's flip-over leap speed (`bg_saber.c:1697-1703`). EternalJK uses
    /// jaPRO's 50 for JA+ too; a JA+ 2.4 server launches it at 60.
    pub(crate) fn flip_over_forward_speed(self) -> f32 {
        if self.yellow_dfa { 60.0 } else { 150.0 }
    }

    /// Whether the flip over turns and locks the view (`bg_pmove.c:12119-12135`).
    pub(crate) fn flip_over_spins(self) -> bool {
        !self.yellow_dfa
    }

    /// Whether a staff's alternate attack standing still is a front kick
    /// (`bg_saber.c:2857-2866`).
    pub(crate) fn standing_front_kick(self) -> bool {
        self.enabled
    }

    /// Whether a full-body taunt leaves the view (and, past meditation, the controls)
    /// free (`bg_pmove.c:12228-12270`).
    pub(crate) fn free_taunts(self) -> bool {
        self.enabled
    }

    /// Whether wall flips and runs, and the front flip, push off players
    /// (`bg_pmove.c:2655-2665, 2919-2990`).
    pub(crate) fn flip_kick(self) -> bool {
        self.enabled && self.flip_kick
    }

    /// Whether a run up a wall may start from the Force jump's flips forward, left and
    /// right as well as from a plain jump. Not in EternalJK (stock and it require
    /// `BOTH_JUMP1`/`BOTH_INAIR1`); observed on a JA+ 2.4 server.
    pub(crate) fn wall_runs_from_force_flips(self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(gamename: &str, cinfo: u32) -> JaPlusRules {
        let info =
            InfoString::parse(&format!("\\gamename\\{gamename}\\jp_cinfo\\{cinfo}")).unwrap();
        JaPlusRules::from_dialect(&ServerProfile::from_server_info(&info).dialect)
    }

    #[test]
    fn only_ja_plus_selects_the_rules() {
        assert_eq!(rules("basejka", u32::MAX), JaPlusRules::default());
        assert_eq!(rules("TaystJK", u32::MAX), JaPlusRules::default());
        let stock = JaPlusRules::default();
        assert!(stock.slides_on_players());
        assert_eq!(stock.grip_speed_scale(), 0.4);
        assert!(stock.cancels_attack_with_holdable(2));
        assert_eq!(stock.flip_over_forward_speed(), 150.0);
        assert!(stock.flip_over_spins());
        assert!(!stock.standing_front_kick() && !stock.free_taunts());
        assert!(!stock.flip_kick());
    }

    #[test]
    fn default_ja_plus_server_bits() {
        // JA+ 2.4 B7's default jp_cinfo: flip kick, fix roll 1, yellow DFA, SP attacks,
        // new DFA, ledge grab, alternate dimension.
        let japlus = rules("JA+ Mod v2.4 B7", 196_819);
        assert!(japlus.enabled && japlus.flip_kick && japlus.yellow_dfa);
        assert!(!japlus.head_slide);
        assert!(!japlus.slides_on_players());
        assert_eq!(japlus.grip_speed_scale(), 0.8);
        assert!(!japlus.cancels_attack_with_holdable(2));
        assert!(japlus.cancels_attack_with_holdable(3));
        assert_eq!(japlus.flip_over_forward_speed(), 60.0);
        assert!(!japlus.flip_over_spins());
        assert!(japlus.standing_front_kick() && japlus.free_taunts() && japlus.flip_kick());
    }

    /// No world: every trace misses.
    struct Open;

    impl crate::pmove::MovementCollision for Open {
        fn trace(
            &self,
            _: [f32; 3],
            _: [f32; 3],
            _: [f32; 3],
            end: [f32; 3],
            _: u32,
        ) -> crate::pmove::MovementTrace {
            crate::pmove::MovementTrace::miss(end)
        }

        fn point_contents(&self, _: [f32; 3]) -> u32 {
            0
        }
    }

    fn animation(name: &str) -> u16 {
        crate::legacy_animation_index(name).unwrap() as u16
    }

    /// Run the head of `PmoveSingle`'s command filter on `state` with a command that
    /// pushes forward and turns 90 degrees.
    fn filtered(
        mut state: crate::pmove::MovementState,
        rules: JaPlusRules,
    ) -> (crate::pmove::MovementState, sjk_protocol::UserCommand) {
        let mut command = sjk_protocol::UserCommand {
            server_time: 1_000,
            forward_move: 127,
            angles: [0, 16_384, 0],
            ..Default::default()
        };
        state.ground_entity_number = 1_022;
        crate::pmove_input_freeze::apply(&mut state, &mut command, 8, &Open, None, rules);
        (state, command)
    }

    fn ja_plus() -> JaPlusRules {
        rules("JA+ Mod v2.4 B7", 196_819)
    }

    #[test]
    fn ja_plus_taunts_leave_the_controls_free() {
        let state = crate::pmove::MovementState {
            legs_anim: animation("BOTH_BOW"),
            torso_anim: animation("BOTH_BOW"),
            legs_timer: 500,
            torso_timer: 500,
            ..Default::default()
        };
        let (stock_state, stock) = filtered(state.clone(), JaPlusRules::default());
        assert_eq!(stock.forward_move, 0);
        assert_ne!(stock_state.delta_angles, state.delta_angles);
        let (japlus_state, japlus) = filtered(state.clone(), ja_plus());
        assert_eq!(japlus.forward_move, 127);
        assert_eq!(japlus_state.delta_angles, state.delta_angles);
    }

    #[test]
    fn ja_plus_meditation_keeps_still_but_looks_around() {
        let state = crate::pmove::MovementState {
            legs_anim: animation("BOTH_MEDITATE"),
            torso_anim: animation("BOTH_MEDITATE"),
            legs_timer: 500,
            torso_timer: 500,
            ..Default::default()
        };
        let (stock_state, stock) = filtered(state.clone(), JaPlusRules::default());
        assert_eq!(stock.forward_move, 0);
        assert_ne!(stock_state.delta_angles, state.delta_angles);
        let (japlus_state, japlus) = filtered(state.clone(), ja_plus());
        assert_eq!(japlus.forward_move, 0);
        assert_eq!(japlus_state.delta_angles, state.delta_angles);
    }

    #[test]
    fn ja_plus_melee_kicks_hold_the_kicker_still() {
        for kick in [
            "BOTH_MELEE_SPINKICK",
            "BOTH_MELEE_BACKKICK",
            "BOTH_JUMP_BACKKICK_SPIN",
            "BOTH_JUMP_BACKFLIP_ATCK",
            "BOTH_FLIP_STAB",
            "BOTH_JUMP_BACKFLIP_ATCK_MISSED",
        ] {
            let state = crate::pmove::MovementState {
                legs_anim: animation(kick),
                torso_anim: animation(kick),
                legs_timer: 600,
                torso_timer: 600,
                ..Default::default()
            };
            let (_, stock) = filtered(state.clone(), JaPlusRules::default());
            assert_eq!(
                stock.forward_move, 127,
                "{kick}: stock does not know the kick"
            );
            let (japlus_state, japlus) = filtered(state.clone(), ja_plus());
            assert_eq!(japlus.forward_move, 0, "{kick}: JA+ holds the kicker still");
            assert_eq!(
                japlus_state.delta_angles, state.delta_angles,
                "{kick}: view free"
            );
        }
    }

    #[test]
    fn ja_plus_kiss_and_backflip_victim_lock_the_view() {
        for (legs, torso) in [
            ("BOTH_KISSEE", "BOTH_KISSEE"),
            ("BOTH_LEDGE_MERCPULL", "BOTH_LEDGE_MERCPULL"),
            ("BOTH_STAND1", "BOTH_GETUP1"),
            ("BOTH_STAND1", "BOTH_NEW_STABEE"),
            ("BOTH_JUMP_BACKFLIP_ATCKEE", "BOTH_STAND1"),
            ("BOTH_STAND1", "BOTH_JUMP_BACKFLIP_ATCKEE"),
        ] {
            let state = crate::pmove::MovementState {
                legs_anim: animation(legs),
                torso_anim: animation(torso),
                ..Default::default()
            };
            let (japlus_state, japlus) = filtered(state.clone(), ja_plus());
            assert_eq!(japlus.forward_move, 0, "{legs}/{torso}");
            assert_ne!(
                japlus_state.delta_angles, state.delta_angles,
                "{legs}/{torso}"
            );
        }
    }

    #[test]
    fn other_servers_leave_ja_plus_animations_alone() {
        let state = crate::pmove::MovementState {
            legs_anim: animation("BOTH_KISSEE"),
            torso_anim: animation("BOTH_KISSEE"),
            ..Default::default()
        };
        let (stock_state, stock) = filtered(state.clone(), JaPlusRules::default());
        assert_eq!(stock.forward_move, 127);
        assert_eq!(stock_state.delta_angles, state.delta_angles);
    }

    #[test]
    fn improved_yellow_dfa_does_not_spin() {
        let state = crate::pmove::MovementState {
            legs_anim: animation("BOTH_JUMPFLIPSLASHDOWN1"),
            torso_anim: animation("BOTH_JUMPFLIPSLASHDOWN1"),
            legs_timer: 1_200,
            torso_timer: 1_200,
            saber_move: u32::from(crate::saber_move_data::movement::LS_A_FLIP_SLASH),
            ..Default::default()
        };
        let (stock, _) = filtered(state.clone(), JaPlusRules::default());
        assert!((stock.view_angles[1] - 240.0 * 0.008).abs() < 1e-4);
        let (japlus, command) = filtered(state.clone(), ja_plus());
        assert_eq!(japlus.view_angles[1], 0.0);
        assert_eq!(japlus.delta_angles, state.delta_angles);
        // Still frozen in place.
        assert_eq!(command.forward_move, 0);
    }

    #[test]
    fn optional_bits_follow_the_server() {
        let japlus = rules("JA+ Mod v2.4 B7", 1 << 5);
        assert!(japlus.head_slide && !japlus.flip_kick && !japlus.yellow_dfa);
        assert!(japlus.slides_on_players());
        assert_eq!(japlus.flip_over_forward_speed(), 150.0);
    }
}

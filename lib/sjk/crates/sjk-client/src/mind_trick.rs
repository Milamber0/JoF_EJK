//! Mind Trick presentation: who is tricked, and the trickster's fade.
//!
//! The server lists the clients a player has tricked in its
//! `fd.forceMindtrickTargetIndex*` bits, copied to the entity's
//! `trickedentindex*` (`w_force.c`, `BG_PlayerStateToEntityState`). `CG_Player`
//! (EternalJK `cg_players.c:10191-10345`, stock code) fades a trickster out for
//! each client it tricked and back in afterwards, and `CG_IsMindTricked`
//! (`:7771-7808`) never counts a client whose Force Sight is active as tricked.

use sjk_protocol::{EntityState, PlayerState};

/// `FP_SEE` in `forcePowersActive`.
const FP_SEE: u32 = 1 << 14;
/// `trickedentindex`, `trickedentindex2`, 3 and 4 in the protocol-26 entity fields.
const ENTITY_TRICK_FIELDS: [usize; 4] = [58, 74, 92, 94];
/// `fd.forceMindtrickTargetIndex` 1-4 in the protocol-26 player fields.
const PLAYER_TRICK_FIELDS: [usize; 4] = [98, 99, 101, 104];

/// The four 16-client trick bitsets of a trickster's entity.
pub fn legacy_entity_trick_targets(state: &EntityState) -> [u32; 4] {
    ENTITY_TRICK_FIELDS.map(|field| state.raw_field(field).unwrap_or(0))
}

/// The four 16-client bitsets of the clients a player state has tricked.
pub fn legacy_player_trick_targets(player: &PlayerState) -> [u32; 4] {
    PLAYER_TRICK_FIELDS.map(|field| player.raw_field(field).unwrap_or(0))
}

/// `CG_IsMindTricked`: whether `client`, whose `forcePowersActive` is
/// `client_force_powers_active`, is in `targets`. Active Force Sight, at any level,
/// sees through every trick (`cg_players.c:7776-7779`).
pub fn legacy_mind_tricked(
    targets: [u32; 4],
    client: u16,
    client_force_powers_active: u32,
) -> bool {
    if client_force_powers_active & FP_SEE != 0 || client > 63 {
        return false;
    }
    targets[usize::from(client / 16)] & (1 << (client % 16)) != 0
}

/// How `CG_Player` draws a possible trickster this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyTrickFade {
    /// `RF_FORCE_ENT_ALPHA` alpha for the body while `fading`, 1-255.
    pub alpha: u8,
    /// Stock `doAlpha`: fading out or back in. Also stops the Force Speed trail.
    pub fading: bool,
    /// Stock `iwantout`: fully faded. The body, its weapon, held sabers and every
    /// effect after the mind-trick cut-off (`cg_players.c:11351-11356`) are skipped.
    pub hidden: bool,
}

impl LegacyTrickFade {
    /// Neither tricked nor fading.
    pub const OPAQUE: Self = Self {
        alpha: 255,
        fading: false,
        hidden: false,
    };
}

/// `centity_t::trickAlpha` and `trickAlphaTime`.
#[derive(Clone, Copy, Debug, Default)]
struct Fade {
    alpha: i32,
    time: i32,
}

/// Per-entity trick fades, allocated once for every entity number.
pub struct LegacyTrickFades {
    entities: Box<[Fade]>,
}

impl Default for LegacyTrickFades {
    fn default() -> Self {
        Self {
            entities: vec![Fade::default(); sjk_protocol::MAX_LEGACY_ENTITIES].into_boxed_slice(),
        }
    }
}

impl LegacyTrickFades {
    /// Run `CG_Player`'s fade step for entity `number` at cgame time `time`. Call it
    /// once per rendered frame for each actor stock passes through `CG_Player`,
    /// tricked or not; a repeated call at the same time changes nothing.
    pub fn advance(&mut self, number: u16, tricked: bool, time: i32) -> LegacyTrickFade {
        let Some(fade) = self.entities.get_mut(usize::from(number)) else {
            return LegacyTrickFade::OPAQUE;
        };
        // "things got out of sync, perhaps a new client is trying to fill in this slot"
        if fade.time == 0 || time.wrapping_sub(fade.time) > 1000 {
            fade.alpha = 255;
            fade.time = time;
        }
        let elapsed = time.wrapping_sub(fade.time);
        let mut result = LegacyTrickFade::OPAQUE;
        if tricked {
            result.fading = true;
            if fade.alpha > 1 {
                // `trickAlpha -= (cg.time - trickAlphaTime) * 0.5`: computed in double and
                // truncated back to int.
                fade.alpha = ((f64::from(fade.alpha) - f64::from(elapsed) * 0.5) as i32).max(0);
            } else {
                fade.alpha = 1;
                result.hidden = true;
            }
            fade.time = time;
        } else if fade.alpha < 255 {
            fade.alpha = fade.alpha.saturating_add(elapsed).min(255);
            fade.time = time;
            result.fading = true;
        } else {
            fade.alpha = 255;
            fade.time = time;
        }
        // "don't cancel it out even if it's < 1"
        result.alpha = fade.alpha.clamp(1, 255) as u8;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRICKSTER: u16 = 5;

    /// Run 8 ms frames from `start` while `tricked`, returning the last fade and time.
    fn run(
        fades: &mut LegacyTrickFades,
        tricked: bool,
        start: i32,
        frames: i32,
    ) -> (LegacyTrickFade, i32) {
        let mut last = LegacyTrickFade::OPAQUE;
        let mut time = start;
        for _ in 0..frames {
            time += 8;
            last = fades.advance(TRICKSTER, tricked, time);
        }
        (last, time)
    }

    #[test]
    fn untricked_entity_stays_opaque() {
        let mut fades = LegacyTrickFades::default();
        let (fade, _) = run(&mut fades, false, 1_000, 20);
        assert_eq!(fade, LegacyTrickFade::OPAQUE);
    }

    /// Fade-out loses 0.5 per ms (4 per 8 ms frame), then the trickster is hidden;
    /// fade-in gains 1 per ms.
    #[test]
    fn fade_out_then_hide_then_fade_in() {
        let mut fades = LegacyTrickFades::default();
        assert_eq!(fades.advance(TRICKSTER, true, 1_000).alpha, 255);
        let first = fades.advance(TRICKSTER, true, 1_008);
        assert_eq!(
            (first.alpha, first.fading, first.hidden),
            (251, true, false)
        );
        // 251 needs 63 more frames to reach zero (251 - 62 * 4 = 3, then 0), then one
        // frame clamps to 1 and hides.
        let (fade, time) = run(&mut fades, true, 1_008, 63);
        assert_eq!(
            (fade.alpha, fade.hidden),
            (1, false),
            "alpha 0 still draws at 1"
        );
        let hidden = fades.advance(TRICKSTER, true, time + 8);
        assert_eq!(
            (hidden.alpha, hidden.fading, hidden.hidden),
            (1, true, true)
        );
        let (still, time) = run(&mut fades, true, time + 8, 50);
        assert!(still.hidden);
        // The trick ends: fade back in from 1, 8 per frame, opaque after 32 frames.
        let back = fades.advance(TRICKSTER, false, time + 8);
        assert_eq!((back.alpha, back.fading, back.hidden), (9, true, false));
        let (fade, time) = run(&mut fades, false, time + 8, 31);
        assert_eq!((fade.alpha, fade.fading), (255, true));
        assert_eq!(
            fades.advance(TRICKSTER, false, time + 8),
            LegacyTrickFade::OPAQUE
        );
    }

    /// Odd frame times truncate as stock's int arithmetic does: 255 - 3.5 = 251.
    #[test]
    fn fade_out_truncates_like_stock() {
        let mut fades = LegacyTrickFades::default();
        fades.advance(TRICKSTER, true, 500);
        assert_eq!(fades.advance(TRICKSTER, true, 507).alpha, 251);
        assert_eq!(fades.advance(TRICKSTER, true, 514).alpha, 247);
    }

    /// An entity unseen for over a second starts again from opaque.
    #[test]
    fn long_absence_resets_to_opaque() {
        let mut fades = LegacyTrickFades::default();
        run(&mut fades, true, 1_000, 200);
        let fade = fades.advance(TRICKSTER, true, 1_000 + 200 * 8 + 1_001);
        assert_eq!((fade.alpha, fade.hidden), (255, false));
    }

    #[test]
    fn repeated_time_changes_nothing() {
        let mut fades = LegacyTrickFades::default();
        fades.advance(TRICKSTER, true, 100);
        let once = fades.advance(TRICKSTER, true, 140);
        assert_eq!(fades.advance(TRICKSTER, true, 140), once);
    }

    /// Active Force Sight, at any level, sees through a trick.
    #[test]
    fn force_sight_sees_through_the_trick() {
        let mut targets = [0; 4];
        targets[1] = 1 << (21 - 16);
        assert!(legacy_mind_tricked(targets, 21, 0));
        assert!(!legacy_mind_tricked(targets, 21, FP_SEE));
        assert!(legacy_mind_tricked(targets, 21, !FP_SEE));
        assert!(!legacy_mind_tricked(targets, 20, 0));
        assert!(!legacy_mind_tricked([u32::MAX; 4], 64, 0));
    }

    #[test]
    fn trick_bits_come_from_both_states() {
        let mut state = EntityState::zero(TRICKSTER, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        state.set_raw_field(92, 1 << 3);
        let targets = legacy_entity_trick_targets(&state);
        assert!(legacy_mind_tricked(targets, 35, 0));
        assert_eq!(targets[2], 1 << 3);
        let mut player = PlayerState::zero();
        player.set_raw_field(104, 1 << 15);
        assert!(legacy_mind_tricked(
            legacy_player_trick_targets(&player),
            63,
            0
        ));
    }
}

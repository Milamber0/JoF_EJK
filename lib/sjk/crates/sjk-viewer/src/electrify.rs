//! The crackle of an electrified player, as `CG_Player` sounds it (OpenJK
//! `codemp/cgame/cg_players.c:11060-11098`, JoF EJK 15300-15340).
//!
//! While `ps.electrifyTime` (a player's `entityState_t::emplacedOwner`) is ahead of the
//! cgame clock the body wears the electric shell (`sjk_client::legacy_force_overlays`).
//! On each frame the shell is drawn (a draw over 0.4) a second draw over 0.9 starts
//! `sound/effects/energy_crackle.wav` on the player, `CHAN_AUTO`. A DEMP2 hit sets the
//! time, and so does a JoF server on `amlogin`, for about four seconds.
//!
//! Stock rolls once per rendered frame, so the crackle grows with the frame rate. SJK
//! rolls on the reference 8 ms cadence instead (`effect_cadence.rs`), the rate of
//! `com_maxfps 125`: about seven crackles a second.

/// `cgs.media.crackleSound`.
pub(crate) const CRACKLE: &str = "sound/effects/energy_crackle.wav";

/// Whether a player electrified until `until` crackles in this reference frame at `now`,
/// given the frame's two draws in `[0, 1]`: the shell's and the sound's.
pub(crate) fn crackles(until: i32, now: i32, draws: [f32; 2]) -> bool {
    until > now && draws[0] > 0.4 && draws[1] > 0.9
}

/// Two draws in `[0, 1]` for player `number` at `now`, the same for the same frame.
pub(crate) fn draws(number: u16, now: i32) -> [f32; 2] {
    let mut state = (now as u32)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(u32::from(number).wrapping_mul(0x85eb_ca6b))
        ^ 0x5bd1_e995;
    state = state.max(1);
    std::array::from_fn(|_| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f32 / u32::MAX as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_electrified_player_crackles_and_only_on_both_draws() {
        assert!(crackles(5_000, 4_000, [0.5, 0.95]));
        assert!(!crackles(4_000, 4_000, [0.5, 0.95]), "its time is up");
        assert!(!crackles(0, 4_000, [1.0, 1.0]), "never electrified");
        assert!(!crackles(5_000, 4_000, [0.4, 0.95]), "no shell this frame");
        assert!(!crackles(5_000, 4_000, [0.5, 0.9]), "a shell but no sound");
    }

    /// A JoF login electrifies for about four seconds: on the 8 ms cadence that is 500
    /// frames, of which stock's chances (0.6 times 0.1) sound about 30.
    #[test]
    fn a_login_crackles_about_thirty_times() {
        let start = 2_212_060;
        let until = start + 3_967;
        let count = (start..until)
            .step_by(8)
            .filter(|&now| crackles(until, now, draws(22, now)))
            .count();
        assert!((15..=50).contains(&count), "{count}");
        assert!(
            (until..until + 4_000)
                .step_by(8)
                .all(|now| !crackles(until, now, draws(22, now)))
        );
    }

    /// Rendering at 125, 250, 500 or 1000 frames a second gives about the same crackles:
    /// the roll waits for the 8 ms cadence, as at `com_maxfps 125`.
    #[test]
    fn the_crackle_rate_does_not_grow_with_the_frame_rate() {
        use std::time::{Duration, Instant};
        let start = 2_212_060;
        let until = start + 3_967;
        let counts = [8, 4, 2, 1].map(|step: u64| {
            let mut cadence = crate::effect_aux::cadence::Cadence::default();
            let clock = Instant::now();
            (0..u64::from((until - start) as u32))
                .step_by(step as usize)
                .filter(|&elapsed| {
                    let now = start + elapsed as i32;
                    cadence.due(clock + Duration::from_millis(elapsed))
                        && crackles(until, now, draws(22, now))
                })
                .count()
        });
        let [reference, ..] = counts;
        for count in counts {
            assert!((15..=50).contains(&count), "{counts:?}");
            assert!(count.abs_diff(reference) <= 15, "{counts:?}");
        }
    }

    #[test]
    fn draws_are_units_and_repeat_for_a_frame() {
        for now in 0..2_000 {
            let [a, b] = draws(7, now);
            assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b));
            assert_eq!(draws(7, now), [a, b]);
        }
    }
}

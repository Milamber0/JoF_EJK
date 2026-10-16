//! JoF EJK's `cl_idrive` (`codemp/client/cl_input.cpp` `CL_KeyMove`): when both
//! keys of a movement pair are down, the one pressed last wins instead of the two
//! cancelling out, so a strafe can reverse without letting go of the first key.
//!
//! `cl_idrive 1` resolves every pair (forward/back, the strafe pairs, jump/crouch);
//! `cl_idrive 2` only jump/crouch. EJK compares the keys' press times and keeps
//! both when they were pressed in the same millisecond.
//!
//! Some servers penalise an instant reversal, a command that goes from one
//! direction to the other with no neutral command between. `cl_idriveDelay`
//! (SJK's own, in milliseconds, 0 by default as in EJK) keeps the pair neutral
//! for that long after the newer key's press, while the older key is still held,
//! before the newer key takes over; and again for that long after the newer key is
//! let go while the older one is still held, so a reversal is never instant either
//! way. Releasing the older key ends the gap at once, as it would without `cl_idrive`.
//! The delay is counted in command time: a frame slower than 8 ms makes several
//! commands, each at its own time (`GameplayInput::set_command_age`).

use crate::console::ViewerConsole;

pub(crate) const CVAR: &str = "cl_idrive";
pub(crate) const DELAY_CVAR: &str = "cl_idriveDelay";
/// The delay's normalised name, which the per-frame read looks up without allocating.
const DELAY_KEY: &str = "cl_idrivedelay";
/// Longest `cl_idriveDelay` honoured, in milliseconds.
pub(crate) const MAX_DELAY_MILLIS: u64 = 1000;

/// One key of a pair as `cl_idrive` sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Key {
    /// Share of the command the key was down, 0 to 1.
    pub(super) fraction: f32,
    /// Time of the press that made the key active.
    pub(super) pressed_at: u64,
    /// Time of the release that made it inactive (0 when it never was released).
    pub(super) released_at: u64,
    /// Still held at the end of the command.
    pub(super) active: bool,
}

/// `cl_idrive` and `cl_idriveDelay`, latched from the console.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Idrive {
    /// The raw `cl_idrive` value: 0 off, 2 jump/crouch only, any other value all pairs.
    pub(crate) mode: i64,
    /// `cl_idriveDelay`, clamped to [`MAX_DELAY_MILLIS`].
    pub(crate) delay_millis: u64,
}

impl Idrive {
    /// Read both cvars as `cl_input.cpp` reads `cl_idrive->integer`.
    pub(crate) fn from_cvars(mode: Option<i64>, delay: Option<i64>) -> Self {
        Self {
            mode: mode.unwrap_or(0),
            delay_millis: u64::try_from(delay.unwrap_or(0))
                .unwrap_or(0)
                .min(MAX_DELAY_MILLIS),
        }
    }

    /// The console's current `cl_idrive` and `cl_idriveDelay` (both registered as
    /// integers). The names are the normalised ones so the per-frame read does not
    /// allocate.
    pub(crate) fn read(console: &ViewerConsole) -> Self {
        Self::from_cvars(console.integer_cvar(CVAR), console.integer_cvar(DELAY_KEY))
    }

    /// EJK resolves jump/crouch for every nonzero value, the other pairs unless it is 2.
    fn applies(self, vertical: bool) -> bool {
        self.mode != 0 && (vertical || self.mode != 2)
    }

    /// The pair's fractions after resolution; `now` is the command's sample time.
    pub(super) fn resolve(
        self,
        vertical: bool,
        positive: Key,
        negative: Key,
        now: u64,
    ) -> (f32, f32) {
        let fractions = (positive.fraction, negative.fraction);
        if !self.applies(vertical) {
            return fractions;
        }
        // The key that won the pair is let go while the other is still held: the pair
        // stays neutral for the delay, so the reversal back is not instant either.
        if self.delay_millis > 0 {
            for (held, gone) in [(positive, negative), (negative, positive)] {
                if held.active
                    && held.fraction > 0.0
                    && !gone.active
                    && gone.fraction == 0.0
                    && gone.pressed_at > held.pressed_at
                    && gone.released_at >= gone.pressed_at
                    && now.saturating_sub(gone.released_at) < self.delay_millis
                {
                    return (0.0, 0.0);
                }
            }
        }
        if positive.fraction == 0.0 || negative.fraction == 0.0 {
            return fractions;
        }
        let (newer, older) = match positive.pressed_at.cmp(&negative.pressed_at) {
            std::cmp::Ordering::Greater => (positive, negative),
            std::cmp::Ordering::Less => (negative, positive),
            std::cmp::Ordering::Equal => return fractions,
        };
        if older.active && now.saturating_sub(newer.pressed_at) < self.delay_millis {
            return (0.0, 0.0);
        }
        if newer.pressed_at == positive.pressed_at {
            (positive.fraction, 0.0)
        } else {
            (0.0, negative.fraction)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(pressed_at: u64) -> Key {
        Key {
            fraction: 1.0,
            pressed_at,
            released_at: 0,
            active: true,
        }
    }

    /// A key pressed at `pressed_at` and let go at `released_at`.
    fn gone(pressed_at: u64, released_at: u64) -> Key {
        Key {
            fraction: 0.0,
            pressed_at,
            released_at,
            active: false,
        }
    }

    fn idrive(mode: i64, delay_millis: u64) -> Idrive {
        Idrive { mode, delay_millis }
    }

    #[test]
    fn off_lets_opposing_keys_cancel() {
        assert_eq!(
            idrive(0, 0).resolve(false, held(10), held(20), 30),
            (1.0, 1.0)
        );
    }

    #[test]
    fn last_pressed_key_wins() {
        let on = idrive(1, 0);
        assert_eq!(on.resolve(false, held(10), held(20), 30), (0.0, 1.0));
        assert_eq!(on.resolve(false, held(20), held(10), 30), (1.0, 0.0));
        // Pressed in the same millisecond: EJK keeps both.
        assert_eq!(on.resolve(false, held(10), held(10), 30), (1.0, 1.0));
    }

    #[test]
    fn mode_two_resolves_only_jump_and_crouch() {
        let vertical = idrive(2, 0);
        assert_eq!(vertical.resolve(true, held(10), held(20), 30), (0.0, 1.0));
        assert_eq!(vertical.resolve(false, held(10), held(20), 30), (1.0, 1.0));
    }

    #[test]
    fn delay_holds_the_pair_neutral_then_switches() {
        let delayed = idrive(1, 50);
        assert_eq!(delayed.resolve(false, held(10), held(100), 120), (0.0, 0.0));
        assert_eq!(delayed.resolve(false, held(10), held(100), 149), (0.0, 0.0));
        assert_eq!(delayed.resolve(false, held(10), held(100), 150), (0.0, 1.0));
    }

    #[test]
    fn releasing_the_older_key_ends_the_delay() {
        let released = Key {
            fraction: 0.5,
            pressed_at: 10,
            released_at: 90,
            active: false,
        };
        assert_eq!(
            idrive(1, 50).resolve(false, released, held(100), 110),
            (0.0, 1.0)
        );
    }

    #[test]
    fn delay_is_clamped() {
        assert_eq!(
            Idrive::from_cvars(Some(1), Some(5000)).delay_millis,
            MAX_DELAY_MILLIS
        );
        assert_eq!(Idrive::from_cvars(Some(1), Some(-5)).delay_millis, 0);
        assert_eq!(Idrive::from_cvars(None, None), Idrive::default());
    }

    #[test]
    fn letting_go_of_the_newer_key_holds_the_pair_neutral_for_the_delay() {
        let delayed = idrive(1, 50);
        // Forward from 10, back from 100 to 120: forward alone comes back 50 ms on.
        assert_eq!(
            delayed.resolve(false, held(10), gone(100, 120), 130),
            (0.0, 0.0)
        );
        assert_eq!(
            delayed.resolve(false, held(10), gone(100, 120), 169),
            (0.0, 0.0)
        );
        assert_eq!(
            delayed.resolve(false, held(10), gone(100, 120), 170),
            (1.0, 0.0)
        );
        // No delay: EJK's behaviour, the older key is back at once.
        assert_eq!(
            idrive(1, 0).resolve(false, held(10), gone(100, 120), 121),
            (1.0, 0.0)
        );
        // A key pressed before the one still held was not the winner.
        assert_eq!(
            delayed.resolve(false, held(200), gone(100, 120), 130),
            (1.0, 0.0)
        );
        // Mode 2 leaves forward/back alone and holds jump/crouch.
        assert_eq!(
            idrive(2, 50).resolve(false, held(10), gone(100, 120), 130),
            (1.0, 0.0)
        );
        assert_eq!(
            idrive(2, 50).resolve(true, held(10), gone(100, 120), 130),
            (0.0, 0.0)
        );
    }

    #[test]
    fn both_cvars_are_read_from_the_console_as_registered() {
        // They are registered as integers (console_cvars.rs), `cl_idriveDelay` with a
        // capital: reading them as floats, or by a name that misses, finds nothing.
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert_eq!(Idrive::read(&console), Idrive::default());
        assert!(console.set_cvar("cl_idrive", "1"));
        assert!(console.set_cvar("cl_idriveDelay", "16"));
        assert_eq!(Idrive::read(&console), idrive(1, 16));
        assert!(console.set_cvar("cl_idrive", "2"));
        assert!(console.set_cvar("cl_idriveDelay", "5000"));
        assert_eq!(Idrive::read(&console), idrive(2, MAX_DELAY_MILLIS));
    }
}

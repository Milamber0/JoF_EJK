//! Whether the server takes the player's inputs at the gameplay FPS
//! (`gameplay_fps.rs`, `sjk_client::command_rate`).
//!
//! Faster than 125 commands a second some servers drop or rewrite commands (JoF's
//! is capped at 125 by name, [`sjk_client::command_rate::command_step`]; others
//! may be unknown), and the game turns unplayable: moves snap back, taps vanish.
//! Every predicted snapshot says whether the server ran one of our commands
//! (`local_prediction::CommandStatus`) and how far our prediction missed. When,
//! over a few seconds of play with commands faster than 125 a second, most
//! snapshots say the server did not run what we sent or that we were corrected,
//! the gameplay FPS card opens to say so and suggest 125. It opens once per step
//! and server, so a player who keeps the value is not asked again.

use crate::local_prediction::{CommandStatus, PredictionSample};
use std::time::{Duration, Instant};

/// How long a window of snapshots is judged over.
const WINDOW: Duration = Duration::from_secs(3);
/// Fewest snapshots a window needs before it is judged (a second of a 20 Hz server
/// at least, so a quiet moment is not enough).
const MIN_SNAPSHOTS: u32 = 30;
/// Share of snapshots, in percent, that must say the inputs did not go through.
const BAD_PERCENT: u32 = 40;
/// A re-prediction this far off (units) says the server moved us otherwise.
const MISS_UNITS: f32 = 8.0;
/// Only steps shorter than this (faster than 125 commands a second) are judged:
/// slower ones are what every server takes.
const JUDGED_BELOW: i32 = sjk_client::command_rate::COMMAND_MILLIS;
/// Windows in a row that must be bad.
const BAD_WINDOWS: u32 = 2;

#[derive(Debug, Default)]
pub(crate) struct InputHealth {
    started: Option<Instant>,
    snapshots: u32,
    bad: u32,
    bad_windows: u32,
    /// The step the window measures; a change starts over.
    step: i32,
    /// The step already reported on this server.
    reported: Option<i32>,
}

impl InputHealth {
    /// A new server: judge afresh.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// One predicted snapshot at `now` with commands `step` ms apart. True once
    /// when the inputs have not been going through: open the card.
    pub(crate) fn observe(&mut self, sample: &PredictionSample, step: i32, now: Instant) -> bool {
        if step >= JUDGED_BELOW || self.reported == Some(step) {
            self.started = None;
            self.bad_windows = 0;
            return false;
        }
        if self.step != step {
            self.step = step;
            self.started = None;
            self.bad_windows = 0;
        }
        let started = *self.started.get_or_insert_with(|| {
            self.snapshots = 0;
            self.bad = 0;
            now
        });
        self.snapshots += 1;
        let ran = sample.status == CommandStatus::Acknowledged;
        if !ran || sample.miss_units > MISS_UNITS {
            self.bad += 1;
        }
        if now.duration_since(started) < WINDOW {
            return false;
        }
        let judged = self.snapshots >= MIN_SNAPSHOTS;
        let bad = judged && self.bad * 100 >= self.snapshots * BAD_PERCENT;
        self.started = None;
        self.bad_windows = if bad { self.bad_windows + 1 } else { 0 };
        if self.bad_windows >= BAD_WINDOWS {
            self.reported = Some(step);
            self.bad_windows = 0;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(status: CommandStatus, miss_units: f32) -> PredictionSample {
        PredictionSample {
            status,
            miss_units,
            ..PredictionSample::default()
        }
    }

    /// Feed `seconds` of 40 snapshots a second, every `bad_every`th one bad.
    fn play(
        health: &mut InputHealth,
        start: Instant,
        seconds: u64,
        bad_every: u32,
        step: i32,
    ) -> bool {
        let mut alerted = false;
        for index in 0..(seconds * 40) as u32 {
            let now = start + Duration::from_millis(u64::from(index) * 25);
            let bad = bad_every > 0 && index % bad_every == 0;
            let sample = if bad {
                sample(CommandStatus::Stalled, 0.0)
            } else {
                sample(CommandStatus::Acknowledged, 0.5)
            };
            alerted |= health.observe(&sample, step, now);
        }
        alerted
    }

    #[test]
    fn inputs_that_do_not_go_through_open_the_card_once() {
        let mut health = InputHealth::default();
        let start = Instant::now();
        // Every other snapshot stalled at 1000 commands a second.
        assert!(play(&mut health, start, 10, 2, 1));
        // Not again for the same step on this server.
        assert!(!play(
            &mut health,
            start + Duration::from_secs(10),
            10,
            2,
            1
        ));
        // A new server judges afresh.
        health.reset();
        assert!(play(&mut health, start + Duration::from_secs(20), 10, 1, 1));
    }

    #[test]
    fn healthy_play_or_125_never_opens_it() {
        let mut health = InputHealth::default();
        let start = Instant::now();
        // A few stalls (packet loss) are normal.
        assert!(!play(&mut health, start, 20, 10, 3));
        // 125 a second is what every server takes: never judged.
        assert!(!play(&mut health, start, 20, 1, 8));
        // Large misses count too.
        let mut health = InputHealth::default();
        let mut alerted = false;
        for index in 0..400u64 {
            let now = start + Duration::from_millis(index * 25);
            alerted |= health.observe(&sample(CommandStatus::Acknowledged, 30.0), 3, now);
        }
        assert!(alerted);
    }
}

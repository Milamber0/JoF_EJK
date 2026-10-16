//! JoF EJK's `flipkick` command (`codemp/cgame/cg_consolecmds.c` `CG_Flipkick_f`,
//! sequence in `cg_view.c` `CG_DoAsync`): one press starts a run of jump taps,
//! jump on one command and off the next, so a JA+ flip kick chains off a
//! player without hammering the jump key.
//!
//! EJK counts cgame frames and sends `+moveup`/`-moveup` once per frame, one user
//! command each. SJK counts user commands instead: each one carries the run's
//! jump state, which is what the server sees. The cvars stay in EJK's frames and
//! are read as time at EJK's 125 fps ([`commands`]), whatever the gameplay FPS
//! (`command_rate`): at the default 125 one frame is one command, and
//! `cg_fkDuration` 50 is 0.4 s at any gameplay FPS. A server forbids the bind with bit 7 of
//! serverinfo `restricts` (`RESTRICT_FLIPKICKBIND`).

/// Milliseconds of one EJK frame at the 125 fps its defaults were tuned for.
const FRAME_MILLIS: u64 = 8;
/// User commands `step` ms apart (`command_rate`) that last as long as `frames`
/// EJK frames, rounded up.
pub(crate) fn commands(frames: u32, step: i32) -> u32 {
    (u64::from(frames) * FRAME_MILLIS)
        .div_ceil(u64::from(step.max(1).unsigned_abs()))
        .min(u64::from(u32::MAX)) as u32
}

/// The default gameplay FPS's step.
#[cfg(test)]
const COMMAND_MILLIS: i32 = sjk_client::command_rate::COMMAND_MILLIS;

/// The run's length and its first press, in user commands
/// (`cg_fkDuration`, `cg_fkFirstJumpDuration`, `cg_fkSecondJumpDelay`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Timing {
    pub(crate) duration: u32,
    /// Commands the first jump stays held.
    pub(crate) first_jump: u32,
    /// Command count before the second jump.
    pub(crate) second_jump_delay: u32,
}

impl Default for Timing {
    /// EJK's defaults (`cg_xcvar.h`): 50, 0, 0 frames.
    fn default() -> Self {
        Self {
            duration: commands(50, sjk_client::command_rate::COMMAND_MILLIS),
            first_jump: 0,
            second_jump_delay: 0,
        }
    }
}

/// The cvars behind [`Timing`], archived with EJK's defaults.
pub(crate) const DURATION_CVAR: &str = "cg_fkDuration";
pub(crate) const FIRST_JUMP_CVAR: &str = "cg_fkFirstJumpDuration";
pub(crate) const SECOND_JUMP_CVAR: &str = "cg_fkSecondJumpDelay";

/// `RESTRICT_FLIPKICKBIND` in serverinfo `restricts`.
pub(crate) const RESTRICT_BIT: i32 = 1 << 7;

/// What the next user command does with jump.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    /// No run: the command keeps the player's own keys.
    Idle,
    /// Jump held (true) or released (false) in this command.
    Jump(bool),
    /// The run ended: jump released and the jump key let go (`-moveup`).
    End,
}

/// One run's progress; `frame` 0 means no run.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FlipKick {
    frame: u32,
    jumps: u32,
    jump: bool,
}

impl FlipKick {
    /// `flipkick`: start (or restart) a run.
    pub(crate) fn start(&mut self) {
        *self = Self {
            frame: 1,
            ..Self::default()
        };
    }

    pub(crate) fn stop(&mut self) {
        *self = Self::default();
    }

    /// Advance one user command, as `CG_DoAsync` advances one frame.
    pub(crate) fn step(&mut self, timing: Timing) -> Step {
        if self.frame == 0 {
            return Step::Idle;
        }
        if self.frame > timing.duration {
            self.stop();
            return Step::End;
        }
        match self.jumps {
            1 => {
                if self.frame > timing.first_jump {
                    self.jump = false;
                    self.jumps += 1;
                }
            }
            2 => {
                if self.frame > timing.second_jump_delay {
                    self.jump = true;
                    self.jumps += 1;
                }
            }
            _ => {
                self.jump = self.frame % 2 == 1;
                self.jumps += 1;
            }
        }
        self.frame += 1;
        Step::Jump(self.jump)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_read_as_time_at_125_fps() {
        assert_eq!(commands(0, COMMAND_MILLIS), 0);
        assert_eq!(
            commands(50, COMMAND_MILLIS),
            50,
            "0.4 s, one command a frame"
        );
        assert_eq!(commands(1, COMMAND_MILLIS), 1);
        assert_eq!(commands(50, 3), 134, "still 0.4 s at a gameplay FPS of 333");
        assert!(
            commands(u32::MAX, COMMAND_MILLIS) > 1_000_000_000,
            "no overflow"
        );
        assert_eq!(Timing::default().duration, 50);
    }

    #[test]
    fn the_default_run_ends_before_a_missed_jump_lands() {
        // A jump stays in the air well over half a second (8 ms per command).
        let steps = run(Timing::default());
        assert!(steps.len() as u64 * (COMMAND_MILLIS as u64) < 500);
    }

    fn run(timing: Timing) -> Vec<Step> {
        let mut kick = FlipKick::default();
        assert_eq!(kick.step(timing), Step::Idle);
        kick.start();
        let mut steps = Vec::new();
        loop {
            let step = kick.step(timing);
            steps.push(step);
            if step == Step::End {
                return steps;
            }
        }
    }

    #[test]
    fn default_run_alternates_then_releases() {
        let steps = run(Timing {
            duration: 6,
            ..Timing::default()
        });
        let expected = [true, false, true, false, true, false].map(Step::Jump);
        assert_eq!(steps[..6], expected);
        assert_eq!(steps[6..], [Step::End]);
    }

    #[test]
    fn first_jump_held_then_second_jump_delayed() {
        let steps = run(Timing {
            duration: 10,
            first_jump: 3,
            second_jump_delay: 6,
        });
        // Held through command 3, released at 4, pressed again at 7.
        let jumps: Vec<bool> = steps
            .iter()
            .filter_map(|step| match step {
                Step::Jump(jump) => Some(*jump),
                _ => None,
            })
            .collect();
        assert_eq!(
            jumps,
            [
                true, true, true, false, false, false, true, false, true, false
            ]
        );
        assert_eq!(steps.last(), Some(&Step::End));
    }

    #[test]
    fn restart_begins_a_new_run() {
        let timing = Timing::default();
        let mut kick = FlipKick::default();
        kick.start();
        kick.step(timing);
        kick.step(timing);
        kick.start();
        assert_eq!(kick.step(timing), Step::Jump(true));
    }
}

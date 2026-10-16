//! When user commands are made and when move packets leave.
//!
//! JoF EJK's `cl_cmdratecap` (`codemp/client/cl_input.cpp`
//! `CL_CreateNewCommands`) makes usercmds on a fixed 8 ms step, 125 a second,
//! however fast the client renders, so the server moves the player in the same
//! steps as a 125 FPS client. Stock makes one per rendered frame, so its frame
//! rate is also its movement: `com_maxfps` 125, 142, 250 and 333 give commands
//! of 8, 7, 4 and 3 ms (`1000 / com_maxfps`, `Com_Frame`), and the movement code's
//! rounding makes jumps, acceleration and strafes differ between them. On a
//! `pmove_fixed` server a stock client above 125 FPS loses input:
//! `ClientThink_real` rounds a command's time up to the `pmove_msec` grid
//! (`g_active.c`) and drops one that lands in a slot already run (`msec < 1`),
//! button presses included.
//!
//! SJK makes commands on a clock of their own, as `cl_cmdratecap` does, and picks
//! the step ([`command_step`]): a `pmove_fixed` server's own step wins; otherwise
//! the gameplay FPS (`gameplay_fps`, [`gameplay_step`]) when the
//! player set one, the frame rate whose movement they want whatever they draw;
//! and on Auto (0, the default) the FPS cap, so a cap of 333 moves like 333 as in
//! the other clients, on a steady grid rather than every rendered frame; never
//! faster than a server known to cap commands takes (JoF's: 125 a second).
//!
//! [`CommandSchedule`] stamps every command on a boundary of the step in server
//! time, one per slot, so no two commands share a slot on the server's grid. A
//! frame slower than a step makes a command for each slot it passed, up to
//! [`CATCH_UP_MILLIS`] of them; after a longer hitch the older slots are skipped
//! rather than sent as a burst.
//!
//! [`PacketPacer`] is `cl_maxpackets` (`CL_ReadyToSendPacket`): commands wait
//! for a packet, which leaves at most every `1000 / cl_maxpackets` ms and
//! carries all of them.

use std::time::{Duration, Instant};

/// Milliseconds between user commands at the default gameplay FPS (125 a second).
pub const COMMAND_MILLIS: i32 = 8;
/// JoF EJK's `cl_cmdratecap` rate, and the fastest JoF's servers take.
pub const DEFAULT_GAMEPLAY_FPS: i64 = 125;
/// `gameplay_fps` 0: the step follows the FPS cap ([`command_step`]).
pub const AUTO_GAMEPLAY_FPS: i64 = 0;
/// The gameplay FPS range: stock's 1 ms floor, and no step longer than 50 ms.
pub const MIN_GAMEPLAY_FPS: i64 = 20;
pub const MAX_GAMEPLAY_FPS: i64 = 1000;
/// The frame rates players lock for their movement, in the order the menus offer them.
pub const GAMEPLAY_FPS_PRESETS: [i64; 4] = [125, 142, 250, 333];
/// Most commands one frame makes to fill the slots it passed.
pub const MAX_CATCH_UP: usize = 16;
/// Slots further back than this are skipped after a slow frame: four commands
/// at 125 a second.
pub const CATCH_UP_MILLIS: i32 = 32;
/// Stock `cl_maxpackets` clamp (`cl_input.cpp` `CL_ReadyToSendPacket`; JoF EJK
/// and EternalJK raise the stock 125 ceiling to 1000).
pub const MIN_PACKETS: i64 = 15;
pub const MAX_PACKETS: i64 = 1000;
/// A stamp this far behind the previous one is a new time line.
const RESTART_MILLIS: i32 = 500;

/// The command step of a gameplay FPS: stock's `1000 / com_maxfps` whole
/// milliseconds, so 142 is 7 ms and 333 is 3 ms, as those frame rates moved.
/// Out-of-range values are clamped to [`MIN_GAMEPLAY_FPS`]..=[`MAX_GAMEPLAY_FPS`].
pub fn gameplay_step(fps: i64) -> i32 {
    (1000 / fps.clamp(MIN_GAMEPLAY_FPS, MAX_GAMEPLAY_FPS)) as i32
}

/// Commands a second at a gameplay FPS: what its step really makes (400 makes 500).
pub fn commands_per_second(fps: i64) -> i64 {
    1000 / i64::from(gameplay_step(fps))
}

/// The step commands are made at. On a `pmove_fixed` server its `pmove_msec`
/// (`pmove_fixed` = `Some(msec)`), since the server moves every player in those
/// steps and drops a second command in one. Otherwise the gameplay FPS's step, or
/// on Auto ([`AUTO_GAMEPLAY_FPS`]) the step of `frame_cap`, the FPS cap (0 is
/// uncapped: 1 ms, the shortest a server takes); never shorter than
/// `server_floor`, the step of a server known to take no faster commands (JoF's
/// JA+ servers take 125 a second, as JoF EJK's `cl_cmdratecap` sends, though they
/// are not `pmove_fixed`).
pub fn command_step(
    gameplay_fps: i64,
    pmove_fixed: Option<i32>,
    frame_cap: u32,
    server_floor: Option<i32>,
) -> i32 {
    let step = match pmove_fixed {
        Some(msec) => return msec.clamp(1, 1000 / MIN_GAMEPLAY_FPS as i32),
        None if gameplay_fps > AUTO_GAMEPLAY_FPS => gameplay_step(gameplay_fps),
        None if frame_cap == 0 => 1,
        None => gameplay_step(i64::from(frame_cap)),
    };
    step.max(server_floor.unwrap_or(1))
}

/// The server times of the commands due this frame, oldest first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DueCommands {
    stamps: [i32; MAX_CATCH_UP],
    len: usize,
}

impl DueCommands {
    pub fn as_slice(&self) -> &[i32] {
        &self.stamps[..self.len]
    }

    fn push(&mut self, stamp: i32) {
        self.stamps[self.len] = stamp;
        self.len += 1;
    }
}

/// The command clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandSchedule {
    /// The stamp of the newest command made on an anchored clock.
    last_stamp: Option<i32>,
    /// While the clock stamps 0, the next command is due at this instant.
    unanchored_due: Option<Instant>,
}

impl CommandSchedule {
    /// Start over: the next call makes one command at once.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Commands due at `now` with commands `step` ms apart ([`command_step`]),
    /// given the server time the clock reads now and whether it is anchored. An
    /// unanchored clock stamps 0 (stock `cl.serverTime` during a map load); its
    /// commands still leave every step of real time, so packets keep
    /// acknowledging the server. A changed step starts its grid at the next
    /// boundary after the last command, never on a slot already sent.
    pub fn due(
        &mut self,
        server_time: i32,
        anchored: bool,
        now: Instant,
        step: i32,
    ) -> DueCommands {
        let step = step.max(1);
        let mut due = DueCommands::default();
        if !anchored {
            self.last_stamp = None;
            if self.unanchored_due.is_none_or(|at| now >= at) {
                let interval = Duration::from_millis(step as u64);
                let next = self.unanchored_due.unwrap_or(now) + interval;
                // Behind by more than a step after a hitch: start again from now.
                self.unanchored_due = Some(if next < now { now + interval } else { next });
                due.push(0);
            }
            return due;
        }
        self.unanchored_due = None;
        let slot = server_time.div_euclid(step);
        let newest = slot * step;
        let catch_up = (CATCH_UP_MILLIS / step).clamp(1, MAX_CATCH_UP as i32);
        let first = match self.last_stamp {
            Some(last) if newest <= last => {
                if last - newest <= RESTART_MILLIS {
                    return due;
                }
                slot
            }
            Some(last) => (last.div_euclid(step) + 1).max(slot - (catch_up - 1)),
            None => slot,
        };
        for slot in first..=slot {
            due.push(slot * step);
        }
        self.last_stamp = Some(newest);
        due
    }
}

/// `cl_maxpackets`: how often a move packet may leave.
#[derive(Clone, Copy, Debug, Default)]
pub struct PacketPacer {
    last_sent: Option<Instant>,
}

impl PacketPacer {
    /// Whether a packet may leave at `now` with `max_packets` a second
    /// (clamped like stock). The interval is whole milliseconds, as stock's
    /// `1000 / cl_maxpackets`.
    pub fn ready(&self, now: Instant, max_packets: i64) -> bool {
        let interval = 1000 / max_packets.clamp(MIN_PACKETS, MAX_PACKETS) as u64;
        self.last_sent.is_none_or(|sent| {
            now.saturating_duration_since(sent) >= Duration::from_millis(interval)
        })
    }

    /// A packet left at `now`.
    pub fn sent(&mut self, now: Instant) {
        self.last_sent = Some(now);
    }

    /// Let the next packet leave at once.
    pub fn reset(&mut self) {
        self.last_sent = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamps(schedule: &mut CommandSchedule, server_time: i32) -> Vec<i32> {
        schedule
            .due(server_time, true, Instant::now(), COMMAND_MILLIS)
            .as_slice()
            .to_vec()
    }

    #[test]
    fn one_command_per_8_ms_slot_on_the_grid() {
        let mut schedule = CommandSchedule::default();
        assert_eq!(stamps(&mut schedule, 1_003), [1_000]);
        // A fast frame in the same slot makes nothing.
        assert_eq!(stamps(&mut schedule, 1_005), [] as [i32; 0]);
        assert_eq!(stamps(&mut schedule, 1_007), [] as [i32; 0]);
        assert_eq!(stamps(&mut schedule, 1_008), [1_008]);
        // Clock drift of a millisecond either way never doubles a slot.
        assert_eq!(stamps(&mut schedule, 1_015), [] as [i32; 0]);
        assert_eq!(stamps(&mut schedule, 1_017), [1_016]);
    }

    #[test]
    fn a_slow_frame_fills_the_slots_it_passed() {
        let mut schedule = CommandSchedule::default();
        stamps(&mut schedule, 2_000);
        // 60 FPS: about two slots a frame.
        assert_eq!(stamps(&mut schedule, 2_017), [2_008, 2_016]);
        // A hitch keeps only the newest slots instead of a burst.
        assert_eq!(stamps(&mut schedule, 2_400), [2_376, 2_384, 2_392, 2_400]);
    }

    #[test]
    fn a_new_time_line_starts_again() {
        let mut schedule = CommandSchedule::default();
        stamps(&mut schedule, 90_000);
        // Slightly behind (a clock correction) waits for the old slot.
        assert_eq!(stamps(&mut schedule, 89_990), [] as [i32; 0]);
        assert_eq!(stamps(&mut schedule, 400), [400]);
        assert_eq!(stamps(&mut schedule, 408), [408]);
    }

    #[test]
    fn an_unanchored_clock_stamps_zero_every_8_ms() {
        let mut schedule = CommandSchedule::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        assert_eq!(
            schedule.due(0, false, at(0), COMMAND_MILLIS).as_slice(),
            [0]
        );
        assert!(
            schedule
                .due(0, false, at(5), COMMAND_MILLIS)
                .as_slice()
                .is_empty()
        );
        assert_eq!(
            schedule.due(0, false, at(8), COMMAND_MILLIS).as_slice(),
            [0]
        );
        // After a long stall, one command, not a burst.
        assert_eq!(
            schedule.due(0, false, at(500), COMMAND_MILLIS).as_slice(),
            [0]
        );
        assert!(
            schedule
                .due(0, false, at(503), COMMAND_MILLIS)
                .as_slice()
                .is_empty()
        );
        // Anchoring starts the grid at once.
        assert_eq!(
            schedule
                .due(12_345, true, at(504), COMMAND_MILLIS)
                .as_slice(),
            [12_344]
        );
    }

    #[test]
    fn gameplay_fps_steps_like_the_stock_frame_rates() {
        let steps: Vec<i32> = GAMEPLAY_FPS_PRESETS.map(gameplay_step).to_vec();
        assert_eq!(steps, [8, 7, 4, 3]);
        assert_eq!(gameplay_step(DEFAULT_GAMEPLAY_FPS), COMMAND_MILLIS);
        // 400 cannot be made in whole milliseconds: 2 ms steps, 500 a second.
        assert_eq!(commands_per_second(400), 500);
        assert_eq!(gameplay_step(0), 50, "clamped to 20");
        assert_eq!(gameplay_step(5_000), 1, "clamped to 1000");
        // A pmove_fixed server's step wins over the player's choice.
        assert_eq!(command_step(333, Some(8), 125, None), 8);
        assert_eq!(command_step(333, None, 125, None), 3);
        // Auto follows the FPS cap: 333 moves like 333, uncapped in 1 ms steps.
        assert_eq!(command_step(AUTO_GAMEPLAY_FPS, None, 333, None), 3);
        assert_eq!(command_step(AUTO_GAMEPLAY_FPS, None, 125, None), 8);
        assert_eq!(command_step(AUTO_GAMEPLAY_FPS, None, 0, None), 1);
        assert_eq!(
            command_step(AUTO_GAMEPLAY_FPS, None, 10, None),
            50,
            "slow caps clamp"
        );
        assert_eq!(command_step(AUTO_GAMEPLAY_FPS, Some(8), 333, None), 8);
        // A JoF server takes 125 a second at most, Auto or not; slower stays slower.
        assert_eq!(command_step(AUTO_GAMEPLAY_FPS, None, 0, Some(8)), 8);
        assert_eq!(command_step(333, None, 125, Some(8)), 8);
        assert_eq!(command_step(60, None, 125, Some(8)), 16);
    }

    #[test]
    fn a_333_grid_makes_a_command_every_3_ms() {
        let mut schedule = CommandSchedule::default();
        let mut at = |time| {
            schedule
                .due(time, true, Instant::now(), gameplay_step(333))
                .as_slice()
                .to_vec()
        };
        assert_eq!(at(3_001), [3_000]);
        assert_eq!(at(3_002), [] as [i32; 0]);
        assert_eq!(at(3_003), [3_003]);
        // A 60 FPS frame fills the five slots it passed.
        assert_eq!(at(3_020), [3_006, 3_009, 3_012, 3_015, 3_018]);
        // A hitch keeps 32 ms of the newest slots.
        assert_eq!(at(3_400).len(), 10);
    }

    #[test]
    fn a_changed_step_never_resends_a_slot() {
        let mut schedule = CommandSchedule::default();
        let step = gameplay_step(333);
        assert_eq!(
            schedule.due(1_002, true, Instant::now(), step).as_slice(),
            [1_002]
        );
        // Now 125: the next 8 ms boundary after 1_002 is 1_008.
        let due = schedule.due(1_004, true, Instant::now(), 8);
        assert!(due.as_slice().is_empty());
        assert_eq!(
            schedule.due(1_009, true, Instant::now(), 8).as_slice(),
            [1_008]
        );
    }

    #[test]
    fn packets_leave_at_most_every_interval() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut pacer = PacketPacer::default();
        assert!(pacer.ready(at(0), 63));
        pacer.sent(at(0));
        // 1000 / 63 = 15 ms.
        assert!(!pacer.ready(at(14), 63));
        assert!(pacer.ready(at(15), 63));
        // Clamped to 15..=1000: 0 means 15 a second, 5000 means 1000.
        assert!(!pacer.ready(at(65), 0));
        assert!(pacer.ready(at(66), 15));
        assert!(pacer.ready(at(1), 5_000));
        pacer.reset();
        assert!(pacer.ready(at(0), 15));
    }
}

//! Ghost replay (`ghost`): record the local player's own movement commands and play
//! them back later, for filming showcases while another player films.
//!
//! Movement is deterministic: the same start and the same commands at the same
//! durations give the same run, whatever the ping, because the server steps each
//! command by its own timestamp. A replay therefore resends the recorded commands
//! (view, keys, buttons, weapon, Force) with fresh stamps. The server owns the
//! position, so drift (a command lost twice, a slightly different start) is corrected
//! through the inputs alone ([`steer`]), and only on the ground, where it works: the
//! movement keys turn to take out a sideways speed and close the offset while the view
//! stays the recorded one, and the recorded clock waits or skips a steady command to fix
//! along-path drift. A replay first walks the player onto the recorded start, or
//! refuses when it is far (teleport there).
//!
//! It refuses to start in a duel or within five seconds of taking damage, and stops on
//! damage, death, spectating, a duel, flying (`freecam`, `fakenoclip`), any movement
//! key or button the player presses, or drift it cannot steer out.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sjk_game_jka::pmove::MovementState;
use sjk_protocol::{PlayerState, UserCommand};

#[path = "ghost_replay/file.rs"]
mod file;
#[path = "ghost_replay/frame.rs"]
mod frame;
#[path = "ghost_replay/steer.rs"]
mod steer;

pub(crate) const COMMAND: &str = "ghost";
/// The experimental switch that makes `ghost` and `mimic` available (Settings >
/// Gameplay); off by default.
pub(crate) const SHOWCASE_CVAR: &str = "cl_experimentalShowcase";
/// Why `command` refuses while the switch is off.
pub(crate) fn showcase_off(command: &str) -> String {
    format!(
        "{command} is experimental and off: turn on Settings > Gameplay > Experimental: \
         showcase tools ({SHOWCASE_CVAR} 1)"
    )
}

pub(crate) const HELP: &str = "Experimental (cl_experimentalShowcase): record and replay your own \
     run: ghost record <name>, ghost stop, \
     ghost play <name>, ghost list, ghost info <name>";

/// No ground entity (`ENTITYNUM_NONE`).
const NO_GROUND: u16 = 1023;
/// A replay walks to the start from this close; farther, the player teleports there.
const APPROACH_RANGE: f32 = 192.0;
/// Close enough to the start to begin, in units, and still enough, in units a second.
const START_TOLERANCE: f32 = 0.5;
const START_STILL: f32 = 1.0;
/// Commands standing on the start before the run begins.
const SETTLE_COMMANDS: u8 = 4;
/// How long the walk to the start may take, in server milliseconds.
const APPROACH_TIMEOUT: i32 = 15_000;
/// Drift beyond which the run is lost: horizontal and vertical units.
const LOST_HORIZONTAL: f32 = 48.0;
const LOST_VERTICAL: f32 = 64.0;
/// The most the recorded clock may wait or run ahead in total, in milliseconds.
const MAX_WARP: i32 = 1_000;
/// Commands between two clock adjustments.
const WARP_SPACING: u32 = 3;
/// A replay waits this long after the player took damage.
const DAMAGE_COOLDOWN: Duration = Duration::from_secs(5);
/// `BUTTON_WALKING` and `BUTTON_TALK`: set by the client itself, not a key the
/// player pressed to take control back.
const PASSIVE_BUTTONS: u16 = 16 | sjk_game_jka::pmove_talk::BUTTON_TALK;

/// The player's pose before a command, read from the committed prediction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Pose {
    pub(crate) origin: [f32; 3],
    pub(crate) velocity: [f32; 3],
    pub(crate) on_ground: bool,
    pub(crate) view_angles: [f32; 3],
    pub(crate) delta_angles: [i32; 3],
    pub(crate) speed: f32,
    pub(crate) weapon: u8,
}

impl Pose {
    pub(crate) fn of(state: &MovementState) -> Self {
        Self {
            origin: state.origin,
            velocity: state.velocity,
            on_ground: state.ground_entity_number != NO_GROUND,
            view_angles: state.view_angles,
            delta_angles: state.delta_angles,
            speed: state.speed,
            weapon: state.weapon,
        }
    }
}

/// Where a run begins.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Start {
    origin: [f32; 3],
    /// View angles as the server holds them (command angles plus delta), 16-bit.
    view: [i32; 3],
    velocity: [f32; 3],
    weapon: u8,
}

/// One recorded command and the state after it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Frame {
    /// Milliseconds since the run's first command.
    time: i32,
    view: [i32; 3],
    forward: i8,
    right: i8,
    up: i8,
    buttons: u16,
    weapon: u8,
    force_selection: u8,
    inventory_selection: u8,
    generic_command: u8,
    origin: [f32; 3],
    velocity: [f32; 3],
    ground: bool,
    /// The state is the server's (a snapshot acknowledged this command), not only the
    /// prediction's.
    confirmed: bool,
}

impl Frame {
    /// The same keys, buttons and selections: holding or skipping such a frame cannot
    /// lose a press.
    fn steady_with(&self, next: &Self) -> bool {
        (self.forward, self.right, self.up, self.buttons, self.weapon)
            == (next.forward, next.right, next.up, next.buttons, next.weapon)
            && self.generic_command == 0
            && next.generic_command == 0
    }
}

/// A recorded run.
#[derive(Clone, Debug, Default)]
struct Run {
    map: String,
    start: Start,
    frames: Vec<Frame>,
}

impl Run {
    fn seconds(&self) -> f32 {
        self.frames
            .last()
            .map_or(0.0, |frame| frame.time as f32 / 1000.0)
    }

    /// The state the run had reached when frame `index` began.
    fn state_before(&self, index: usize) -> ([f32; 3], [f32; 3], bool) {
        match index.checked_sub(1).and_then(|i| self.frames.get(i)) {
            Some(frame) => (frame.origin, frame.velocity, frame.ground),
            None => (self.start.origin, self.start.velocity, true),
        }
    }
}

struct Recording {
    name: String,
    run: Run,
    /// The first command's stamp.
    first_stamp: Option<i32>,
}

struct Playback {
    name: String,
    run: Run,
    first_stamp: Option<i32>,
    /// Recorded milliseconds added to the elapsed time (negative: the clock waited).
    warp: i32,
    /// The frame last sent.
    cursor: usize,
    /// Frames passed over are merged into the next command; this is the last one sent
    /// or merged.
    consumed: Option<usize>,
    commands_since_warp: u32,
    corrections: u32,
    worst_drift: f32,
}

struct Approach {
    playback: Playback,
    first_stamp: Option<i32>,
    settled: u8,
}

#[derive(Default)]
enum Mode {
    #[default]
    Idle,
    Recording(Recording),
    Approaching(Box<Approach>),
    Playing(Box<Playback>),
}

/// The ghost's state, owned by the viewer.
#[derive(Default)]
pub(crate) struct Ghost {
    mode: Mode,
    last_damage_event: Option<u8>,
    damaged_at: Option<Instant>,
    /// The recording ended on its own (death, its length): the next frame saves it.
    pending_save: bool,
    /// Lines for the console, taken by [`Self::take_messages`].
    messages: Vec<String>,
}

/// What a replay does with one command.
pub(crate) struct Drive {
    pub(crate) command: UserCommand,
    /// The view the camera should show (pitch, yaw in degrees), while replaying.
    pub(crate) view: Option<[f32; 2]>,
}

impl Ghost {
    /// A replay (or its walk to the start) drives the commands.
    pub(crate) fn driving(&self) -> bool {
        matches!(self.mode, Mode::Approaching(_) | Mode::Playing(_))
    }

    pub(crate) fn recording(&self) -> bool {
        matches!(self.mode, Mode::Recording(_))
    }

    pub(crate) fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    fn stop_with(&mut self, message: String) {
        self.mode = Mode::Idle;
        self.messages.push(message);
    }

    /// End a replay, or a recording (kept), because the player can no longer drive
    /// their own player: `reason` completes "stopped, ...".
    pub(crate) fn interrupt(&mut self, reason: &str) {
        if self.driving() {
            self.stop_with(format!("^3ghost: stopped, {reason}."));
        } else if self.recording() && !self.pending_save {
            self.messages
                .push(format!("^3ghost: recording ended, {reason}."));
            self.pending_save = true;
        }
    }

    /// Replace `input`, the command built from the player's own keys, while a replay
    /// drives; `pose` is the state before it.
    pub(crate) fn drive(&mut self, input: &UserCommand, pose: &Pose) -> Option<Drive> {
        if !self.driving() {
            return None;
        }
        if took_control(input) {
            self.stop_with("^3ghost: stopped, you took control.".into());
            return None;
        }
        let mode = std::mem::take(&mut self.mode);
        let (mode, drive, message) = match mode {
            Mode::Approaching(mut approach) => {
                let drive = approach_step(&mut approach, input, pose);
                match drive {
                    Step::Continue(drive) => (Mode::Approaching(approach), Some(drive), None),
                    Step::Begin(drive) => {
                        let message = format!(
                            "^2ghost: playing {} ({:.1} s).",
                            approach.playback.name,
                            approach.playback.run.seconds()
                        );
                        (
                            Mode::Playing(Box::new(approach.playback)),
                            Some(drive),
                            Some(message),
                        )
                    }
                    Step::Stop(message) => (Mode::Idle, None, Some(message)),
                }
            }
            Mode::Playing(mut playback) => match play_step(&mut playback, input, pose) {
                Step::Continue(drive) | Step::Begin(drive) => {
                    (Mode::Playing(playback), Some(drive), None)
                }
                Step::Stop(message) => (Mode::Idle, None, Some(message)),
            },
            other => (other, None, None),
        };
        self.mode = mode;
        self.messages.extend(message);
        drive
    }

    /// Keep `command`, just sent, while recording; `pose` was the state before it and
    /// `after` is the predicted state after it.
    pub(crate) fn record(
        &mut self,
        command: &UserCommand,
        pose: &Pose,
        after: Option<&MovementState>,
    ) {
        let Mode::Recording(recording) = &mut self.mode else {
            return;
        };
        if self.pending_save {
            return; // Ended; saved on the next frame.
        }
        let first = *recording.first_stamp.get_or_insert(command.server_time);
        let after = after.map(Pose::of).unwrap_or(*pose);
        if recording.run.frames.is_empty() {
            recording.run.start = Start {
                origin: pose.origin,
                view: server_view(command, pose),
                velocity: pose.velocity,
                weapon: pose.weapon,
            };
        }
        recording.run.frames.push(Frame {
            time: command.server_time.wrapping_sub(first),
            view: server_view(command, pose),
            forward: command.forward_move,
            right: command.right_move,
            up: command.up_move,
            buttons: command.buttons,
            weapon: command.weapon,
            force_selection: command.force_selection,
            inventory_selection: command.inventory_selection,
            generic_command: command.generic_command,
            origin: after.origin,
            velocity: after.velocity,
            ground: after.on_ground,
            confirmed: false,
        });
        if recording.run.frames.len() >= file::MAX_FRAMES {
            self.interrupt("ten minutes recorded");
        }
    }

    /// Watch a live snapshot: confirm recorded states with the server's, and stop a
    /// replay on damage, death, spectating or a duel.
    pub(crate) fn snapshot(&mut self, player: &PlayerState) {
        let damage = player.damage_event();
        if self.last_damage_event.is_some_and(|last| last != damage) {
            self.damaged_at = Some(Instant::now());
            if self.driving() {
                self.stop_with("^3ghost: stopped, you took damage.".into());
            }
        }
        self.last_damage_event = Some(damage);
        let unfit = unfit_reason(player);
        match &mut self.mode {
            Mode::Recording(recording) => {
                if let Some(first) = recording.first_stamp {
                    let time = player.command_time().wrapping_sub(first);
                    if let Ok(index) = recording
                        .run
                        .frames
                        .binary_search_by_key(&time, |frame| frame.time)
                    {
                        let frame = &mut recording.run.frames[index];
                        frame.origin = player.origin();
                        frame.velocity = player.velocity();
                        frame.ground = player.ground_entity_num() != NO_GROUND;
                        frame.confirmed = true;
                    }
                }
                if let Some(reason) = unfit {
                    self.interrupt(reason);
                }
            }
            Mode::Approaching(_) | Mode::Playing(_) => {
                if let Some(reason) = unfit {
                    self.stop_with(format!("^3ghost: stopped, {reason}."));
                }
            }
            Mode::Idle => {}
        }
    }

    /// Leaving the game ends whatever runs; a recording is kept.
    pub(crate) fn leave(&mut self, directory: &Path) {
        if self.recording() {
            self.pending_save = true;
            self.save_pending(directory);
        }
        self.mode = Mode::Idle;
        self.last_damage_event = None;
    }

    /// Save a recording that ended on its own.
    pub(crate) fn save_pending(&mut self, directory: &Path) {
        if !std::mem::take(&mut self.pending_save) {
            return;
        }
        if let Mode::Recording(recording) = std::mem::take(&mut self.mode) {
            let line = match save(&ghost_directory(directory), recording) {
                Ok(line) | Err(line) => line,
            };
            self.messages.push(line);
        }
    }
}

/// A replay step's outcome.
enum Step {
    Continue(Drive),
    /// The walk reached the start: this command already plays the first frame.
    Begin(Drive),
    Stop(String),
}

/// The player pressed a movement key or a button during a replay.
pub(crate) fn took_control(input: &UserCommand) -> bool {
    input.forward_move != 0
        || input.right_move != 0
        || input.up_move != 0
        || input.buttons & !PASSIVE_BUTTONS != 0
        || input.generic_command != 0
}

/// Why a replay cannot run now, if it cannot.
fn unfit_reason(player: &PlayerState) -> Option<&'static str> {
    if player.health() <= 0 {
        Some("you are dead")
    } else if player.is_spectator()
        || !crate::local_prediction::predicts_local_view(player.movement_flags())
    {
        Some("you are spectating")
    } else if player.duel_in_progress() {
        Some("a duel is on")
    } else {
        None
    }
}

/// The view angles the server derives from `command` (`cmd.angles + delta_angles`),
/// 16-bit.
fn server_view(command: &UserCommand, pose: &Pose) -> [i32; 3] {
    std::array::from_fn(|i| command.angles[i].wrapping_add(pose.delta_angles[i]) & 0xFFFF)
}

fn short_to_degrees(short: i32) -> f32 {
    (short & 0xFFFF) as f32 * (360.0 / 65_536.0)
}

fn signed_degrees(short: i32) -> f32 {
    let degrees = short_to_degrees(short);
    if degrees > 180.0 {
        degrees - 360.0
    } else {
        degrees
    }
}

/// A command that sends `view` (server angles) with the player's current delta.
fn command_with_view(input: &UserCommand, view: [i32; 3], pose: &Pose) -> UserCommand {
    UserCommand {
        angles: std::array::from_fn(|i| view[i].wrapping_sub(pose.delta_angles[i]) & 0xFFFF),
        forward_move: 0,
        right_move: 0,
        up_move: 0,
        buttons: input.buttons & PASSIVE_BUTTONS,
        generic_command: 0,
        ..*input
    }
}

fn camera_view(view: [i32; 3]) -> [f32; 2] {
    [signed_degrees(view[0]), signed_degrees(view[1])]
}

fn approach_step(approach: &mut Approach, input: &UserCommand, pose: &Pose) -> Step {
    let start = approach.playback.run.start;
    let first = *approach.first_stamp.get_or_insert(input.server_time);
    if input.server_time.wrapping_sub(first) > APPROACH_TIMEOUT {
        return Step::Stop(
            "^3ghost: could not settle on the start with the recorded weapon, stopped.".into(),
        );
    }
    let offset = [
        start.origin[0] - pose.origin[0],
        start.origin[1] - pose.origin[1],
        start.origin[2] - pose.origin[2],
    ];
    let distance = steer::horizontal(offset);
    if distance > APPROACH_RANGE || offset[2].abs() > 24.0 {
        return Step::Stop(format!(
            "^3ghost: the start is {distance:.0} units away; go there first ({}).",
            start_text(&start)
        ));
    }
    let mut command = UserCommand {
        weapon: start.weapon,
        ..*input
    };
    command.forward_move = 0;
    command.right_move = 0;
    command.up_move = 0;
    command.buttons &= PASSIVE_BUTTONS;
    command.generic_command = 0;
    if !pose.on_ground {
        approach.settled = 0;
        return Step::Continue(Drive {
            command,
            view: None,
        });
    }
    if distance > START_TOLERANCE {
        approach.settled = 0;
        let (forward, right) =
            steer::walk_toward(pose.origin, start.origin, pose.view_angles[1], pose.speed);
        command.forward_move = forward;
        command.right_move = right;
        return Step::Continue(Drive {
            command,
            view: None,
        });
    }
    // On the start: face the recorded view and stand still for a few commands.
    let mut command = command_with_view(&command, start.view, pose);
    command.weapon = start.weapon;
    let still = steer::horizontal(pose.velocity) < START_STILL && pose.weapon == start.weapon;
    approach.settled = if still { approach.settled + 1 } else { 0 };
    if approach.settled <= SETTLE_COMMANDS {
        return Step::Continue(Drive {
            command,
            view: Some(camera_view(start.view)),
        });
    }
    match play_step(&mut approach.playback, input, pose) {
        Step::Continue(drive) | Step::Begin(drive) => Step::Begin(drive),
        stop => stop,
    }
}

fn play_step(playback: &mut Playback, input: &UserCommand, pose: &Pose) -> Step {
    let first = *playback.first_stamp.get_or_insert(input.server_time);
    let elapsed = input.server_time.wrapping_sub(first);
    if elapsed < 0 {
        return Step::Stop("^3ghost: the server clock jumped back, stopped.".into());
    }
    let run = &playback.run;
    let time = elapsed + playback.warp;
    let last = run.frames.len() - 1;
    if time > run.frames[last].time && playback.consumed == Some(last) {
        return Step::Stop(format!(
            "^2ghost: {} played to the end ({} corrections, worst drift {:.1} units).",
            playback.name, playback.corrections, playback.worst_drift
        ));
    }
    while playback.cursor < last && run.frames[playback.cursor + 1].time <= time {
        playback.cursor += 1;
    }
    let mut frame = run.frames[playback.cursor];
    // Presses in frames the clock passed over are not lost: they ride on this command.
    let from = playback.consumed.map_or(0, |consumed| consumed + 1);
    for skipped in run.frames.get(from..playback.cursor).unwrap_or_default() {
        frame.buttons |= skipped.buttons;
        if skipped.generic_command != 0 && frame.generic_command == 0 {
            frame.generic_command = skipped.generic_command;
        }
        if skipped.up > 0 && frame.up <= 0 {
            frame.up = skipped.up;
        }
    }
    playback.consumed = Some(playback.cursor);

    let (origin, velocity, ground) = run.state_before(playback.cursor);
    let error = [
        origin[0] - pose.origin[0],
        origin[1] - pose.origin[1],
        origin[2] - pose.origin[2],
    ];
    let drift = steer::horizontal(error);
    playback.worst_drift = playback.worst_drift.max(drift);
    if drift > LOST_HORIZONTAL || error[2].abs() > LOST_VERTICAL {
        return Step::Stop(format!(
            "^1ghost: lost the run at {:.1} s ({drift:.0} units off), stopped.",
            time as f32 / 1000.0
        ));
    }

    let turn = if pose.on_ground {
        steer::turn_toward_path(error, velocity, pose.velocity)
    } else {
        0.0
    };
    let (forward, right) = steer::turn_moves(frame.forward, frame.right, turn);
    if (forward, right) != (frame.forward, frame.right) {
        playback.corrections += 1;
    }

    // Along-path drift on steady ground: let the recorded clock wait or skip one
    // command.
    playback.commands_since_warp += 1;
    let steady = run
        .frames
        .get(playback.cursor + 1)
        .is_some_and(|next| run.frames[playback.cursor].steady_with(next));
    if pose.on_ground
        && ground
        && steady
        && playback.commands_since_warp >= WARP_SPACING
        && let Some(along) = steer::along_path(error, velocity)
    {
        let step = if along > steer::WARP_UNITS {
            -8 // Behind: the recorded clock waits.
        } else if along < -steer::WARP_UNITS {
            8 // Ahead: it skips a command.
        } else {
            0
        };
        if step != 0 && (playback.warp + step).abs() <= MAX_WARP {
            playback.warp += step;
            playback.commands_since_warp = 0;
            playback.corrections += 1;
        }
    }

    let command = UserCommand {
        server_time: input.server_time,
        angles: std::array::from_fn(|i| frame.view[i].wrapping_sub(pose.delta_angles[i]) & 0xFFFF),
        buttons: frame.buttons | (input.buttons & sjk_game_jka::pmove_talk::BUTTON_TALK),
        weapon: frame.weapon,
        force_selection: frame.force_selection,
        inventory_selection: frame.inventory_selection,
        generic_command: frame.generic_command,
        forward_move: forward,
        right_move: right,
        up_move: frame.up,
    };
    Step::Continue(Drive {
        command,
        view: Some(camera_view(frame.view)),
    })
}

fn start_text(start: &Start) -> String {
    format!(
        "{:.0} {:.0} {:.0}, facing {:.0}",
        start.origin[0],
        start.origin[1],
        start.origin[2],
        short_to_degrees(start.view[1])
    )
}

/// Write `recording` to `<directory>/<name>.ghost`; both outcomes are a console line.
fn save(directory: &Path, recording: Recording) -> Result<String, String> {
    let Recording { name, run, .. } = recording;
    if run.frames.is_empty() {
        return Err("^3ghost: nothing was recorded.".into());
    }
    let path = file::file_name(&name)
        .map(|file| directory.join(file))
        .map_err(|error| format!("^1ghost: {error}"))?;
    std::fs::create_dir_all(directory)
        .and_then(|()| std::fs::write(&path, run.to_text()))
        .map_err(|error| format!("^1ghost: could not save {}: {error}", path.display()))?;
    let confirmed = run.frames.iter().filter(|frame| frame.confirmed).count();
    Ok(format!(
        "^2ghost: saved {name} ({:.1} s, {} commands, {}% confirmed by the server).          Start: {}.",
        run.seconds(),
        run.frames.len(),
        confirmed * 100 / run.frames.len(),
        start_text(&run.start)
    ))
}

/// `<settings>/ghosts`, beside `demos`.
fn ghost_directory(config: &Path) -> PathBuf {
    config.join("ghosts")
}

#[cfg(test)]
#[path = "ghost_replay/tests.rs"]
mod tests;

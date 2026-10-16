//! The `ghost` console command and the viewer's hooks: commands in
//! `live_session.rs`, snapshots in `snapshot_presentation.rs`, leaving in
//! `connection.rs`.

use super::*;

/// Where the player is when a `ghost` command runs.
#[derive(Clone, Copy, Default)]
pub(crate) struct Place<'a> {
    /// The current map (`mp/ffa3`).
    pub(crate) map: Option<&'a str>,
    pub(crate) player: Option<&'a PlayerState>,
    /// The player's committed pose.
    pub(crate) pose: Option<Pose>,
    /// False while flying or spectating locally.
    pub(crate) can_drive: bool,
    /// The mimic is copying someone (`mimic`).
    pub(crate) mimicking: bool,
}

impl Ghost {
    /// `ghost <record|stop|play|list|info>`.
    pub(crate) fn command(
        &mut self,
        args: &[String],
        config: &Path,
        place: Place<'_>,
    ) -> Result<Vec<String>, String> {
        let Place {
            map,
            player,
            pose,
            can_drive,
            mimicking,
        } = place;
        let directory = ghost_directory(config);
        let verb = args.first().map(|arg| arg.to_ascii_lowercase());
        match (verb.as_deref(), args.get(1)) {
            (Some("record"), Some(name)) => {
                file::file_name(name)?;
                let (map, player) = map.zip(player).ok_or("ghost: not in a game")?;
                if self.recording() || self.driving() {
                    return Err("ghost: already running; ghost stop first".into());
                }
                if let Some(reason) = unfit_reason(player) {
                    return Err(format!("ghost: cannot record, {reason}"));
                }
                if !can_drive || pose.is_none() {
                    return Err("ghost: needs your own predicted view (not flying)".into());
                }
                self.mode = Mode::Recording(Recording {
                    name: name.clone(),
                    run: Run {
                        map: map.to_owned(),
                        // A minute of commands; longer runs grow it.
                        frames: Vec::with_capacity(7_500),
                        ..Run::default()
                    },
                    first_stamp: None,
                });
                Ok(vec![format!(
                    "^2ghost: recording {name}^7; ^5ghost stop^7 ends it. Stand still on \
                     the start before you move, for an exact start."
                )])
            }
            (Some("stop"), None) => {
                if self.recording() {
                    self.pending_save = true;
                    self.save_pending(config);
                    return Ok(self.take_messages());
                }
                if self.driving() {
                    self.mode = Mode::Idle;
                    return Ok(vec!["^3ghost: replay stopped.".into()]);
                }
                Err("ghost: nothing to stop".into())
            }
            (Some("play"), Some(name)) => {
                let run = load(&directory, name)?;
                let (map, player) = map.zip(player).ok_or("ghost: not in a game")?;
                if self.recording() || self.driving() {
                    return Err("ghost: already running; ghost stop first".into());
                }
                if mimicking {
                    return Err("ghost: the mimic is running; mimic off first".into());
                }
                if !run.map.eq_ignore_ascii_case(map) {
                    return Err(format!(
                        "ghost: {name} was recorded on {}, this is {map}",
                        run.map
                    ));
                }
                if let Some(reason) = unfit_reason(player) {
                    return Err(format!("ghost: cannot play, {reason}"));
                }
                if self
                    .damaged_at
                    .is_some_and(|at| at.elapsed() < DAMAGE_COOLDOWN)
                {
                    return Err("ghost: you took damage in the last five seconds".into());
                }
                let pose = pose
                    .filter(|_| can_drive)
                    .ok_or("ghost: needs your own predicted view (not flying)")?;
                let distance = steer::horizontal([
                    run.start.origin[0] - pose.origin[0],
                    run.start.origin[1] - pose.origin[1],
                    0.0,
                ]);
                if distance > APPROACH_RANGE {
                    return Err(format!(
                        "ghost: the start is {distance:.0} units away; go or teleport there \
                         first ({})",
                        start_text(&run.start)
                    ));
                }
                let seconds = run.seconds();
                self.mode = Mode::Approaching(Box::new(Approach {
                    playback: Playback {
                        name: name.clone(),
                        run,
                        first_stamp: None,
                        warp: 0,
                        cursor: 0,
                        consumed: None,
                        commands_since_warp: 0,
                        corrections: 0,
                        worst_drift: 0.0,
                    },
                    first_stamp: None,
                    settled: 0,
                }));
                Ok(vec![format!(
                    "^2ghost: walking to the start of {name} ({seconds:.1} s)^7. Hands off; \
                     any key stops it."
                )])
            }
            (Some("list"), None) => list(&directory),
            (Some("info"), Some(name)) => {
                let run = load(&directory, name)?;
                let confirmed = run.frames.iter().filter(|frame| frame.confirmed).count();
                Ok(vec![format!(
                    "{name}: {}, {:.1} s, {} commands, {}% confirmed by the server, start \
                     {}",
                    run.map,
                    run.seconds(),
                    run.frames.len(),
                    confirmed * 100 / run.frames.len(),
                    start_text(&run.start)
                )])
            }
            (None, None) => Ok(vec![
                HELP.to_owned(),
                format!(
                    "ghost: {}; files in {}",
                    match self.mode {
                        Mode::Idle => "idle",
                        Mode::Recording(_) => "recording",
                        Mode::Approaching(_) => "walking to the start",
                        Mode::Playing(_) => "playing",
                    },
                    directory.display()
                ),
            ]),
            _ => Err(format!("usage: {HELP}")),
        }
    }
}

fn load(directory: &Path, name: &str) -> Result<Run, String> {
    let path = directory.join(file::file_name(name)?);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("ghost: cannot read {}: {error}", path.display()))?;
    Run::from_text(&text).map_err(|error| format!("ghost: {}: {error}", path.display()))
}

fn list(directory: &Path) -> Result<Vec<String>, String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Ok(vec![format!("ghost: none yet ({})", directory.display())]);
    };
    let mut lines: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".ghost")?.to_owned();
            Some(match load(directory, &stem) {
                Ok(run) => format!("{stem}: {}, {:.1} s", run.map, run.seconds()),
                Err(error) => format!("{stem}: ^1{error}"),
            })
        })
        .collect();
    lines.sort();
    if lines.is_empty() {
        lines.push(format!("ghost: none yet ({})", directory.display()));
    }
    Ok(lines)
}

impl crate::GpuState {
    /// The `ghost` console command.
    pub(crate) fn ghost_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let directory = self
            .console
            .as_ref()
            .ok_or("Console unavailable")?
            .config_directory()
            .to_owned();
        let session = self.live_session.as_ref();
        let map = session.and_then(|session| current_map(session.game_state()));
        let player = session.map(|session| session.latest_snapshot().player.clone());
        let pose = self.local_prediction.committed_state().map(Pose::of);
        let can_drive =
            !self.local_prediction.fake_noclip() && !self.local_prediction.free_camera();
        let mimicking = self.mimic.active();
        self.ghost.command(
            args,
            &directory,
            Place {
                map: map.as_deref(),
                player: player.as_ref(),
                pose,
                can_drive,
                mimicking,
            },
        )
    }

    /// Whether the experimental showcase tools are switched on.
    pub(crate) fn showcase_enabled(&self) -> bool {
        self.console
            .as_ref()
            .and_then(|console| console.bool_cvar(SHOWCASE_CVAR))
            .unwrap_or(false)
    }

    /// Save a recording that ended on its own and print the ghost's lines; switching
    /// the showcase tools off ends whatever runs (a recording is kept).
    pub(crate) fn ghost_frame(&mut self) {
        if !self.showcase_enabled() {
            self.ghost.interrupt("the showcase tools were switched off");
            self.mimic
                .stop_with("^3mimic: stopped, the showcase tools were switched off.");
        }
        let Some(console) = &mut self.console else {
            return;
        };
        self.ghost.save_pending(console.config_directory());
        for line in self
            .ghost
            .take_messages()
            .into_iter()
            .chain(self.mimic.take_messages())
        {
            console.push_log(line);
        }
    }

    /// Leaving the game or changing map ends the ghost; a recording is saved.
    pub(crate) fn ghost_leave(&mut self) {
        self.mimic.stop_with("^3mimic: stopped, you left the game.");
        if let Some(console) = &self.console {
            let directory = console.config_directory().to_owned();
            self.ghost.leave(&directory);
        }
        self.ghost_frame();
    }
}

/// The map's name in `CS_SERVERINFO`, lower-cased (`mp/ffa3`).
fn current_map(game: &sjk_protocol::GameState) -> Option<String> {
    let info = game
        .config_string(0)
        .map(sjk_client::LegacyClientInfo::new)?;
    info.bytes("mapname")
        .map(|bytes| String::from_utf8_lossy(bytes).to_ascii_lowercase())
}

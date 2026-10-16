//! Deterministic demo playback request parsing and snapshot advancement.

use sjk_client::{DemoPlayback, LegacyWorldAdapter};
use sjk_protocol::{GameState, Snapshot};
use sjk_runtime::{World, WorldId};
use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Camera used while replaying a demo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Camera {
    FirstPerson,
    FirstPersonPitch(i16),
    FollowThirdPerson,
    Spectate(u16),
    /// Third person behind the local player, turned towards entity `n`.
    LookAt(u16),
    /// A fixed offset from entity `n`, framing it whole.
    Orbit(u16),
    /// A fixed camera: origin and JKA yaw/pitch in degrees, as `viewpos` prints them.
    Free {
        origin: [f32; 3],
        yaw: f32,
        pitch: f32,
    },
}

impl Camera {
    pub(crate) fn third_person(self) -> bool {
        matches!(
            self,
            Self::FollowThirdPerson
                | Self::Spectate(_)
                | Self::LookAt(_)
                | Self::Orbit(_)
                | Self::Free { .. }
        )
    }

    /// Cameras that show the recorded player's own view, which that player's zoom
    /// puts in first person; the director's cameras (spectate, look-at, orbit,
    /// free) are not overridden by it.
    pub(crate) fn follows_player_view(self) -> bool {
        matches!(
            self,
            Self::FirstPerson | Self::FirstPersonPitch(_) | Self::FollowThirdPerson
        )
    }

    /// Cameras that leave the local player's position: the local actor must
    /// stay at its entity transform instead of being pinned under the camera.
    pub(crate) fn detached(self) -> bool {
        matches!(self, Self::Spectate(_) | Self::Orbit(_) | Self::Free { .. })
    }
}

/// Parsed command-line options for one demo run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Request {
    pub(crate) game_data: PathBuf,
    pub(crate) demo: PathBuf,

    pub(crate) from_millis: u64,

    pub(crate) fps: u32,

    pub(crate) camera: Camera,
}

/// Parse `--play-demo` and its playback options.
pub(crate) fn request(arguments: &[OsString]) -> Result<Option<Request>, Box<dyn Error>> {
    let Some(play_index) = arguments.iter().position(|value| value == "--play-demo") else {
        return Ok(None);
    };
    let game_data = arguments
        .get(0)
        .filter(|value| !value.to_string_lossy().starts_with('-'))
        .ok_or("demo playback requires GameData as the first argument")?
        .into();
    let demo = arguments
        .get(play_index + 1)
        .ok_or("--play-demo requires a .dm_26 path")?
        .into();
    let mut output = Request {
        game_data,
        demo,

        from_millis: 0,

        fps: 60,

        camera: Camera::FirstPerson,
    };
    let mut index = play_index + 2;
    while index < arguments.len() {
        let option = arguments[index].to_string_lossy();
        match option.as_ref() {
            "--from" => {
                output.from_millis = seconds_value(arguments, &mut index, "--from")?;
            }
            "--fps" => {
                output.fps = number_value(arguments, &mut index, "--fps")?;
                if output.fps == 0 || output.fps > 1_000 {
                    return Err("--fps must be in 1..=1000".into());
                }
            }
            "--follow-third-person" => output.camera = Camera::FollowThirdPerson,
            "--view-pitch" => {
                let pitch = number_value(arguments, &mut index, "--view-pitch")?;
                if !(-89..=89).contains(&pitch) {
                    return Err("--view-pitch must be in -89..=89 JKA degrees".into());
                }
                output.camera = Camera::FirstPersonPitch(pitch);
            }
            "--spectate" => {
                output.camera =
                    Camera::Spectate(number_value(arguments, &mut index, "--spectate")?);
            }
            "--look-at" => {
                output.camera = Camera::LookAt(number_value(arguments, &mut index, "--look-at")?);
            }
            "--orbit" => {
                output.camera = Camera::Orbit(number_value(arguments, &mut index, "--orbit")?);
            }
            // --free-camera x,y,z,yaw,pitch: the camera `viewpos` prints, for rendering an
            // owner's exact view with the owner's config.
            "--free-camera" => {
                let text = path_value(arguments, &mut index, "--free-camera")?;
                let v: Vec<f32> = text
                    .to_string_lossy()
                    .split(',')
                    .filter_map(|n| n.trim().parse().ok())
                    .collect();
                if v.len() != 5 {
                    return Err("--free-camera needs x,y,z,yaw,pitch".into());
                }
                output.camera = Camera::Free {
                    origin: [v[0], v[1], v[2]],
                    yaw: v[3],
                    pitch: v[4],
                };
            }
            unknown => return Err(format!("unknown demo playback option {unknown:?}").into()),
        }
        index += 1;
    }
    Ok(Some(output))
}

fn path_value(
    arguments: &[OsString],
    index: &mut usize,
    option: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    *index += 1;
    arguments
        .get(*index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires a value").into())
}

fn seconds_value(
    arguments: &[OsString],
    index: &mut usize,
    option: &str,
) -> Result<u64, Box<dyn Error>> {
    *index += 1;
    let value = arguments
        .get(*index)
        .ok_or_else(|| format!("{option} requires seconds"))?
        .to_string_lossy()
        .parse::<f64>()?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{option} must be a non-negative number").into());
    }
    Ok((value * 1_000.0).round() as u64)
}

fn number_value<T>(
    arguments: &[OsString],
    index: &mut usize,
    option: &str,
) -> Result<T, Box<dyn Error>>
where
    T: std::str::FromStr,
    T::Err: Error + 'static,
{
    *index += 1;
    Ok(arguments
        .get(*index)
        .ok_or_else(|| format!("{option} requires a value"))?
        .to_string_lossy()
        .parse()?)
}

fn fixed_relative_millis(elapsed: Duration, fps: u32) -> u64 {
    let elapsed = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let fps = u64::from(fps.max(1));
    let frame = elapsed.saturating_mul(fps) / 1_000;
    frame.saturating_mul(1_000) / fps
}

/// Shared-stream playback state used by both windowed and capture modes.
pub(crate) struct Session {
    stream: DemoPlayback<BufReader<File>>,
    world: World,
    adapter: LegacyWorldAdapter,
    first_server_time: i32,
    current_server_time: i64,
    camera: Camera,
    fps: u32,
    ended: bool,
    initial_observation_pending: bool,
}

impl Session {
    /// Apply the viewer's cached `cg_smoothClients` to subsequent snapshot endpoints.
    pub(crate) fn set_smooth_clients(&mut self, enabled: bool) {
        self.adapter.set_smooth_clients(enabled);
    }

    /// Open and prime a demo at its first authoritative snapshot.
    pub(crate) fn open(path: &Path, camera: Camera, fps: u32) -> Result<Self, Box<dyn Error>> {
        let mut stream = DemoPlayback::new(BufReader::new(File::open(path)?))?;
        let first = stream
            .next_snapshot()?
            .ok_or("demo contains a gamestate but no snapshots")?;
        let world_id = WorldId::new(1);
        let mut world = World::new(world_id);
        let mut adapter = LegacyWorldAdapter::new(world_id);
        adapter.apply_snapshot(
            stream.latest_snapshot().expect("first snapshot exists"),
            stream.game_state(),
            &mut world,
        );
        Ok(Self {
            stream,
            world,
            adapter,
            first_server_time: first.server_time,
            current_server_time: i64::from(first.server_time),
            camera,
            fps: fps.max(1),
            ended: false,
            initial_observation_pending: true,
        })
    }

    /// Advance until the stream brackets `relative_millis`, exposing every
    /// accepted snapshot before it enters the shared world adapter.
    ///
    /// `CG_SetInitialSnapshot` checks every initial entity event at
    /// `codemp/cgame/cg_snapshot.c:129-141`; later accepted snapshots enter
    /// `CG_TransitionEntity` at `cg_snapshot.c:188-194`. Callers must therefore
    /// observe every callback, even when one render frame spans multiple
    /// snapshots. The borrowed values avoid cloning or a per-frame drain list.
    pub(crate) fn advance_to(
        &mut self,
        relative_millis: u64,
        mut observe: impl FnMut(&Snapshot, &GameState),
    ) -> Result<(), Box<dyn Error>> {
        self.advance_with_config(relative_millis, |snapshot, game, _| observe(snapshot, game))
    }

    /// Expose pending config changes before each snapshot's sound events.
    pub(crate) fn advance_with_config(
        &mut self,
        relative_millis: u64,
        mut observe: impl FnMut(&Snapshot, &GameState, &sjk_protocol::ConfigStringDirty),
    ) -> Result<(), Box<dyn Error>> {
        if self.initial_observation_pending {
            observe(
                self.stream
                    .latest_snapshot()
                    .expect("primed demo has its initial snapshot"),
                self.stream.game_state(),
                self.stream.config_string_changes(),
            );
            self.initial_observation_pending = false;
        }
        let relative = i32::try_from(relative_millis).unwrap_or(i32::MAX);
        let target = self.first_server_time.saturating_add(relative);
        while self
            .stream
            .latest_snapshot()
            .is_some_and(|snapshot| snapshot.server_time < target)
        {
            let Some(advance) = self.stream.next_snapshot()? else {
                self.ended = true;
                break;
            };
            if advance.map_changed {
                return Err("demo capture across map changes is not supported yet".into());
            }
            let snapshot = self
                .stream
                .latest_snapshot()
                .expect("accepted snapshot exists");
            let game_state = self.stream.game_state();
            observe(snapshot, game_state, self.stream.config_string_changes());
            self.adapter
                .apply_snapshot(snapshot, game_state, &mut self.world);
        }
        self.current_server_time = i64::from(target);
        Ok(())
    }

    /// Consume the shared demo decoder's configstring notifications.
    pub(crate) fn drain_config_string_changes(&mut self, visit: impl FnMut(usize)) {
        self.stream.drain_config_string_changes(visit);
    }

    pub(crate) fn shader_remaps(&self) -> &sjk_client::ShaderRemaps {
        self.stream.shader_remaps()
    }

    pub(crate) fn clear_shader_remaps(&mut self) {
        self.stream.clear_shader_remaps();
    }

    pub(crate) fn game_state(&self) -> &GameState {
        self.stream.game_state()
    }

    pub(crate) fn latest_snapshot(&self) -> &Snapshot {
        self.stream.latest_snapshot().expect("session is primed")
    }

    pub(crate) fn snapshot_at_or_before(&self, time: i32) -> &Snapshot {
        self.stream
            .snapshot_at_or_before(time)
            .expect("session is primed")
    }

    pub(crate) fn world(&self) -> &World {
        &self.world
    }

    pub(crate) fn current_server_time(&self) -> i64 {
        self.current_server_time
    }

    pub(crate) fn first_server_time(&self) -> i32 {
        self.first_server_time
    }

    pub(crate) fn camera(&self) -> Camera {
        self.camera
    }

    /// Quantize a wall-clock duration onto the deterministic playback clock.
    pub(crate) fn paced_relative_millis(&self, elapsed: Duration) -> u64 {
        fixed_relative_millis(elapsed, self.fps)
    }

    pub(crate) fn ended(&self) -> bool {
        self.ended
    }
}

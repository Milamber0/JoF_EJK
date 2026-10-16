//! Background connection and map preparation, separate from the displayed world.
//!
//! The waiting transport is deliberately absent from `GpuState::live_session`:
//! snapshot consumers cannot accidentally draw new-map entities in the old BSP.
//! Only adoption of a verified, completely built destination publishes it again.

#[path = "resident_messages.rs"]
mod messages;
#[path = "resident_scenery.rs"]
mod scenery;
#[path = "resident_walk.rs"]
mod walk;

use crate::GpuState;
use sjk_client::ClientSession;
use std::time::{Duration, Instant};

pub(crate) struct State {
    pub(crate) session: Option<ClientSession>,
    /// This world was built using the server's verified content selection.
    pub(crate) bound: bool,
    pub(crate) map: String,
    walk: Option<walk::Walk>,
    remote_command_due: Instant,
    attached: bool,
    reuse_pending: bool,
    pub(crate) map_change_pending: bool,
    pub(crate) after_sequence: Option<i32>,
    pub(crate) scenery: Option<(sjk_protocol::GameState, sjk_protocol::Snapshot)>,
}

impl State {
    pub(crate) fn new(
        game: Option<&sjk_protocol::GameState>,
        snapshot: Option<&sjk_protocol::Snapshot>,
    ) -> Self {
        let scenery = game.map(|game| {
            (
                game.clone(),
                snapshot.cloned().unwrap_or(sjk_protocol::Snapshot {
                    message_sequence: 0,
                    reliable_acknowledge: 0,
                    server_commands: Vec::new(),
                    server_time: 0,
                    delta_from: None,
                    flags: 0,
                    area_mask: Vec::new(),
                    player: Default::default(),
                    vehicle_player: None,
                    entities: Vec::new(),
                    consumed_bits: 0,
                }),
            )
        });
        Self {
            session: None,
            bound: game.is_some(),
            map: game
                .and_then(|game| game.config_string(0))
                .and_then(|raw| std::str::from_utf8(raw).ok())
                .and_then(|text| sjk_protocol::InfoString::parse(text).ok())
                .and_then(|info| info.get("mapname").map(|name| format!("maps/{name}.bsp")))
                .unwrap_or_default(),
            walk: None,
            remote_command_due: Instant::now(),
            attached: false,
            reuse_pending: false,
            map_change_pending: false,
            after_sequence: None,
            scenery,
        }
    }

    pub(crate) fn snapshot_ready(&self) -> bool {
        if self
            .session
            .as_ref()
            .is_some_and(|session| !crate::live_session::active_snapshot(session.latest_snapshot()))
        {
            return false;
        }
        self.after_sequence.is_none_or(|sequence| {
            self.session
                .as_ref()
                .is_some_and(|s| s.latest_snapshot().message_sequence.wrapping_sub(sequence) > 0)
        })
    }

    pub(crate) fn exploring(&self) -> bool {
        self.walk.is_some()
    }

    pub(crate) fn disconnect(&mut self) {
        if let Some(mut session) = self.session.take() {
            let _ = session.disconnect();
        }
        self.walk = None;
        self.after_sequence = None;
        self.attached = false;
        self.reuse_pending = false;
        self.map_change_pending = false;
    }
}

impl GpuState {
    /// Park the remote transport while the destination loads; gameplay is suspended.
    pub(crate) fn retain_world_for_connection(&mut self) {
        self.end_free_camera();
        self.resident.session = self.live_session.take();
        self.resident.map_change_pending |= self.live_map_installed;
        self.resident.reuse_pending = true;
        self.live_map_installed = false;
        if !self.is_menu_world {
            self.particles.clear();
            self.effect_aux = crate::effect_aux::Runtime::default();
            self.resident.walk = None;
            self.gameplay_input.clear();
            self.local_prediction.stop(36.0);
            self.game_menu = false;
            if let Some(menu) = &mut self.client_menu {
                menu.state_loading("next map");
            }
        }
    }

    /// Park the session as for a map change to the same map, for a graphics reload
    /// ([`crate::graphics_reload`]): the map loads again from the session's gamestate
    /// after the next snapshot, no prepared world stands in for it, and it counts as no
    /// map change.
    pub(crate) fn retain_world_for_reload(&mut self) {
        let sequence = self
            .live_session
            .as_ref()
            .map(|session| session.latest_snapshot().message_sequence);
        self.retain_world_for_connection();
        self.resident.reuse_pending = false;
        self.resident.map_change_pending = false;
        self.resident.after_sequence = sequence;
        self.pending_map_reload = true;
    }

    pub(crate) fn start_exploring(&mut self) {
        if self.resident.exploring() {
            return;
        }
        self.live_world = sjk_runtime::World::new(self.live_world.id());
        self.legacy_world_adapter = None;
        scenery::prepare(self);
        self.resident.walk = Some(walk::Walk::new(self));
        self.local_prediction.stop(36.0);
        self.gameplay_input.view_authority = Default::default();
        self.third_person_camera = crate::camera::State::default();
        self.gameplay_input.clear();
        crate::log::progress(format_args!("resident world: local movement ready"));
    }

    /// Poll the waiting transport independently of the world being rendered.
    /// Neutral commands acknowledge gamestates without sending local exploration input.
    pub(crate) fn poll_resident_connection(&mut self) {
        let Some(session) = &mut self.resident.session else {
            return;
        };
        if session.needs_download() {
            return;
        }
        let mut changed = false;
        let mut after_sequence = self.resident.after_sequence;
        let mut failure = None;
        crate::live_session::drain_snapshots(|| {
            let received = session.receive_snapshot(Duration::ZERO);
            let timed_out = received.as_ref().is_err_and(|e| e.is_timeout());
            if let Err(error) = received
                && !error.is_timeout()
            {
                failure = Some(error.to_string());
            }
            let reactions = super::consume(session.drain_transitions());
            if reactions.reload_world {
                after_sequence = reactions
                    .gamestate_sequence
                    .or(Some(session.latest_snapshot().message_sequence));
            }
            changed |= reactions.reload_world;
            failure = reactions.disconnect_reason.or(failure.take());
            !timed_out && failure.is_none() && !session.needs_download()
        });
        self.resident.after_sequence = after_sequence;
        // Keep server messages flowing while its map is being prepared.
        self.consume_resident_messages();
        if let Some(error) = failure {
            self.session_disconnected(error);
            return;
        }
        if changed {
            self.resident.map_change_pending = true;
            self.pending_map_reload = true;
            self.resident.reuse_pending = true;
            self.world_load_task = None;
            self.world_install_task = None;
            // Even a same-map gamestate can select different PK3s or entity baselines.
            self.portal.aim(None, None);
        }
        if self.resident.reuse_pending && self.resident.snapshot_ready() {
            self.resident.reuse_pending = false;
            if self.attach_prepared_session() {
                self.pending_map_reload = false;
                self.world_load_task = None;
                self.world_install_task = None;
                return;
            }
        }
        let now = Instant::now();
        if now < self.resident.remote_command_due {
            return;
        }
        let command = sjk_protocol::UserCommand::default();
        if let Err(error) = self
            .resident
            .session
            .as_mut()
            .unwrap()
            .send_command(&command)
        {
            self.session_disconnected(error.to_string());
            return;
        }
        self.resident.remote_command_due = now + Duration::from_millis(25);
    }

    /// The join adopts its destination's actual render world; it never starts a
    /// second parallel install.
    pub(crate) fn poll_destination(
        &mut self,
    ) -> Result<Option<GpuState>, Box<dyn std::error::Error>> {
        if let Some(error) = self.portal.session_error() {
            return Err(error.into());
        }
        if !self.portal.ready() || !self.resident.snapshot_ready() {
            return Ok(None);
        }
        // The loading screen stays up until the world can be played: a world
        // built from the joined session's own gamestate, with the session in
        // hand, rather than a preview to explore.
        if self.client_menu.is_some()
            && (self.resident.session.is_none() || !self.portal.for_session())
        {
            return Ok(None);
        }
        self.world_load_map = self.portal.map().unwrap_or_default().to_owned();
        let Some(world) = self.portal.take_world() else {
            return Ok(None);
        };
        self.pending_map_reload = false;
        crate::log::progress(format_args!(
            "destination adopted: {} (session ready={})",
            self.world_load_map, world.resident.bound
        ));
        Ok(Some(world))
    }

    /// Preserve the standalone inspection camera; playable retained worlds use Pmove.
    pub(crate) fn advance_resident_movement(&mut self, delta_seconds: f32) {
        if self.resident.map_change_pending {
            return;
        }
        if let Some(walk) = &mut self.resident.walk {
            self.camera_position = walk.advance(
                &mut self.gameplay_input,
                self.camera_pitch,
                self.camera_yaw,
                &self.bsp,
                &mut self.trace_scratch,
            );
        } else if self.live_session.is_none() && self.demo_session.is_none() && !self.is_menu_world
        {
            use crate::input::GameButton;
            use glam::Vec3;
            let axis = |positive, negative| {
                u8::from(self.gameplay_input.held(positive)) as f32
                    - u8::from(self.gameplay_input.held(negative)) as f32
            };
            let forward = Vec3::new(self.camera_yaw.cos(), self.camera_yaw.sin(), 0.0);
            let right = Vec3::new(-self.camera_yaw.sin(), self.camera_yaw.cos(), 0.0);
            let movement = forward * axis(GameButton::Forward, GameButton::Back)
                + right * axis(GameButton::MoveRight, GameButton::MoveLeft)
                + Vec3::Z * axis(GameButton::Up, GameButton::Down);
            let speed = if self.gameplay_input.held(GameButton::Speed) {
                1800.0
            } else {
                600.0
            };
            self.camera_position += movement.normalize_or_zero() * speed * delta_seconds;
        }
    }
}

impl GpuState {
    /// Start the verified destination at gamestate receipt, before first-snapshot/team waits.
    pub(crate) fn prepare_gate_game(&mut self, game: &sjk_protocol::GameState) {
        if !self.is_menu_world {
            return;
        }
        let Some(map) = game
            .config_string(0)
            .and_then(|raw| std::str::from_utf8(raw).ok())
            .and_then(|text| sjk_protocol::InfoString::parse(text).ok())
            .and_then(|info| info.get("mapname").map(str::to_owned))
        else {
            return;
        };
        self.world_load_map = format!("maps/{map}.bsp");
        self.portal
            .aim_session(&self.world_load_map, &self.game_data, game, None);
    }

    /// A world prepared from this very gamestate needs no second BSP/material install.
    pub(crate) fn attach_prepared_session(&mut self) -> bool {
        if self.is_menu_world || !self.resident.bound {
            return false;
        }
        let Some(session) = &self.resident.session else {
            return false;
        };
        let Some((prepared, _)) = &self.resident.scenery else {
            return false;
        };
        let current = session.game_state();
        if prepared.checksum_feed != current.checksum_feed
            || (1612..1644).any(|slot| prepared.config_string(slot) != current.config_string(slot))
        {
            return false;
        }
        // Server-info also carries changing game metadata. Only the map name
        // identifies geometry; other server metadata can change during a restart.
        let info = |game: &sjk_protocol::GameState| {
            sjk_protocol::InfoString::parse(std::str::from_utf8(game.config_string(0)?).ok()?).ok()
        };
        let (Some(prepared_info), Some(current_info)) = (info(prepared), info(current)) else {
            return false;
        };
        if prepared_info.get("mapname").is_none()
            || prepared_info.get("mapname") != current_info.get("mapname")
        {
            return false;
        }
        let (Ok(prepared_content), Ok(current_content)) = (
            crate::assets::session_content::Selection::from_game(prepared),
            crate::assets::session_content::Selection::from_game(current),
        ) else {
            return false;
        };
        if prepared_content != current_content {
            return false;
        }
        let mut prediction = crate::LocalPrediction::new(
            Some(session.latest_snapshot()),
            // The first loaded model may be another player's, an NPC's or a vehicle's:
            // the new prediction keeps the old one's table, else takes the humanoid set.
            None,
            Some(current),
            self.vfs.as_ref().expect("resident world retains VFS"),
        );
        prediction.keep_animation_lengths(&self.local_prediction);
        self.local_prediction = prediction;
        self.resident.scenery = Some((current.clone(), session.latest_snapshot().clone()));
        self.world_load_map.clone_from(&self.resident.map);
        self.world_load_state = super::LoadStateMachine::new();
        self.world_load_started = None;
        self.completed_map_changes =
            self.completed_map_changes
                .saturating_add(u32::from(std::mem::take(
                    &mut self.resident.map_change_pending,
                )));
        self.gameplay_input.clear();
        self.gameplay_input.view_authority = Default::default();
        self.resident.walk = None;
        self.resident.after_sequence = None;
        self.pending_generic_command = 0;
        self.selected_weapon = None;
        self.legacy_world_adapter = Some(crate::LegacyWorldAdapter::new(self.live_world.id()));
        self.live_session = self.resident.session.take();
        if let Some(remote) = &mut self.live_session {
            remote.take_retired_world();
            remote.invalidate_presentation_config();
        }
        self.live_map_installed = true;
        if let Some(menu) = &mut self.client_menu {
            menu.joined();
        }
        self.reset_live_presentation();
        self.server_clock.reset_connection(Instant::now());
        self.server_clock.activate();
        self.resident.attached = true;
        crate::log::progress(format_args!(
            "resident world: session attached without rebuilding map"
        ));
        true
    }

    pub(crate) fn finish_resident_attach(&mut self, audio: &mut Option<crate::GameAudio>) {
        if std::mem::take(&mut self.resident.attached) {
            self.configure_audio(audio);
            self.present_latest_live_snapshot(audio);
        }
    }
}

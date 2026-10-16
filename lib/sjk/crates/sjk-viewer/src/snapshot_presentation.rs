//! Shared presentation-side observation of newly accepted snapshots.
//!
//! Live networking and deterministic demo playback both enter through this
//! module. This mirrors cgame's event observation rule: every accepted
//! snapshot is observed once, even when one rendered frame advances across
//! several snapshots.

use super::*;

/// Cumulative snapshot-presentation counters used by capture evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Counters {
    pub(crate) snapshots: u64,
    pub(crate) impact_events: u64,
    pub(crate) impact_visual_requests: u64,
    pub(crate) saber_flare_latches: u64,
}

/// Mutable presentation sinks shared by live networking and demo playback.
pub(crate) struct Sinks<'a> {
    pub(crate) audio: &'a mut Option<GameAudio>,
    pub(crate) vfs: &'a VirtualFileSystem,
    pub(crate) impacts: &'a mut impacts::Pool,
    pub(crate) clash_flare: &'a mut LegacySaberClashFlare,
    pub(crate) particles: &'a mut Vec<Particle>,
    pub(crate) auxiliary: &'a mut effect_aux::Runtime,
    pub(crate) effects: &'a mut EffectLibrary,
    pub(crate) damage_feedback: &'a mut damage_feedback::Feedback,
    pub(crate) first_person_view: &'a mut first_person_view::Tracker,
    pub(crate) previous_particle_events: &'a mut HashMap<u16, u16>,
    pub(crate) muzzle_effects: &'a mut sjk_client::LegacyMuzzleEffects,
    pub(crate) force_overlays: &'a mut sjk_client::LegacyForceOverlayTracker,
    /// The nameplates' Force, health and shield estimates.
    pub(crate) nameplate: &'a mut hud::nameplate::State,
    pub(crate) obituaries: &'a mut sjk_client::ObituaryTracker,
    pub(crate) lagometer: &'a mut sjk_client::LagometerSamples,
    pub(crate) console: &'a mut Option<console::ViewerConsole>,
    pub(crate) localization: &'a Localization,
    pub(crate) counters: &'a mut Counters,
    pub(crate) bsp: &'a sjk_bsp::Bsp,
    pub(crate) trace_scratch: &'a mut sjk_bsp::TraceScratch,
}

/// Observe one accepted snapshot through every snapshot-driven presentation
/// subsystem.
pub(crate) fn observe(
    snapshot: &Snapshot,
    game_state: &GameState,
    local: impact_spawn::LocalView,
    predicting_local_view: bool,
    visual_now: Instant,
    sinks: &mut Sinks<'_>,
) {
    if let Some(audio) = sinks.audio.as_mut() {
        audio.observe_snapshot(snapshot, game_state, sinks.vfs);
    }
    let impact = impact_spawn::observe_snapshot(
        sinks.impacts,
        sinks.clash_flare,
        snapshot,
        local,
        impact_spawn::Sinks {
            particles: sinks.particles,
            auxiliary: sinks.auxiliary,
            effects: sinks.effects,
            vfs: sinks.vfs,
            audio: sinks.audio,
        },
        visual_now,
    );
    sinks
        .damage_feedback
        .observe(&snapshot.player, snapshot.server_time);
    sinks
        .first_person_view
        .observe(snapshot, predicting_local_view);
    effect_runtime::spawn_snapshot_particles(
        sinks.particles,
        sinks.auxiliary,
        sinks.previous_particle_events,
        snapshot,
        game_state,
        sinks.vfs,
        sinks.effects,
        sinks.audio,
        visual_now,
        sinks.bsp,
        sinks.trace_scratch,
    );
    sinks.muzzle_effects.observe(snapshot);
    sinks.force_overlays.observe_snapshot(snapshot);
    sinks
        .nameplate
        .observe_snapshot(snapshot, game_state, sinks.bsp, sinks.trace_scratch);
    let obituary_before = sinks.obituaries.decoded();
    sinks.obituaries.observe(snapshot, game_state);
    let obituary_count = sinks.obituaries.decoded() - obituary_before;
    if let Some(audio) = sinks.audio.as_mut() {
        for offset in (0..obituary_count as usize).rev() {
            if let Some(event) = sinks.obituaries.feed().newest(offset) {
                audio.kill_cue(event, snapshot);
            }
        }
    }
    if let Some(console) = sinks.console.as_mut() {
        for offset in (0..obituary_count as usize).rev() {
            if let Some(event) = sinks.obituaries.feed().newest(offset) {
                console.push_log(format_obituary(event, game_state, sinks.localization));
            }
        }
    }
    let ping = snapshot
        .server_time
        .wrapping_sub(snapshot.player.command_time())
        .max(0);
    sinks
        .lagometer
        .add_received(snapshot.message_sequence, ping, snapshot.flags);
    sinks.counters.snapshots += 1;
    sinks.counters.impact_events += impact.events as u64;
    sinks.counters.impact_visual_requests += impact.visual_requests as u64;
    sinks.counters.saber_flare_latches += impact.flare_latches as u64;
}

/// The console line `CG_Obituary` prints (`cg_event.c:124-454`): each name ends
/// with `^7` (`S_COLOR_WHITE`), so a colour left open in a name stops there.
fn format_obituary(
    event: sjk_client::ObituaryEvent,
    game: &GameState,
    localization: &Localization,
) -> String {
    use std::fmt::Write as _;
    // Names come from the configstring's bytes: a name with a Latin-1 letter is
    // not UTF-8, and reading the whole string as UTF-8 printed it as `noname`.
    let target = sjk_client::obituary_name(game, event.target);
    let key = event.attacker_message.unwrap_or(event.message);
    let phrase = localization.strings.get(key).map_or(key, String::as_str);
    let mut line = String::with_capacity(128);
    if event.attacker_message.is_some() {
        let attacker = sjk_client::obituary_name(game, event.attacker);
        let _ = write!(line, "{target}^7 {phrase} {attacker}^7");
    } else {
        let _ = write!(line, "{target}^7 {phrase}");
    }
    line
}

impl GpuState {
    /// A prepared world or local authority cannot contribute samples to a new
    /// remote timeline, even when the numeric timestamps happen to increase.
    pub(crate) fn reset_live_presentation(&mut self) {
        let Some(session) = &self.live_session else {
            return;
        };
        self.live_world = sjk_runtime::World::new(self.live_world.id());
        self.legacy_world_adapter = Some(LegacyWorldAdapter::new(self.live_world.id()));
        if let (Some(adapter), Some(console)) = (&mut self.legacy_world_adapter, &self.console) {
            adapter.set_smooth_clients(console.smooth_clients());
        }
        self.presentation_clock = presentation_clock::SnapshotPresentationClock::new(
            session.latest_snapshot().server_time,
            Instant::now(),
        );
        self.local_actor_state = Default::default();
        for mesh in &mut self.actor_meshes {
            mesh.angle_controller = sjk_client::LegacyPlayerAngleController::new();
        }
    }

    pub(crate) fn update_demo_playback(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        visual_now: Instant,
    ) {
        let Some(mut session) = self.demo_session.take() else {
            return;
        };
        let relative = session.paced_relative_millis(self.ui_epoch.elapsed());
        let relative = self.console.as_ref().map_or(relative, |console| {
            console.demo_time(
                relative,
                (session.current_server_time() - i64::from(session.first_server_time())).max(0)
                    as u64,
            )
        });
        let advance = session.advance_with_config(relative, |snapshot, game_state, changes| {
            if let (Some(audio), Some(vfs)) = (game_audio.as_mut(), &self.vfs) {
                audio.prepare_config_strings(changes, game_state, vfs);
            }
            let local = impact_spawn::LocalView {
                client_num: snapshot.player.client_num(),
                third_person: self.third_person,
                flash_point: self.last_first_person_flash,
            };
            observe(
                snapshot,
                game_state,
                local,
                false,
                visual_now,
                &mut Sinks {
                    audio: game_audio,
                    vfs: self
                        .vfs
                        .as_ref()
                        .expect("demo playback retains its mounted VFS"),
                    impacts: &mut self.impacts,
                    clash_flare: &mut self.saber_clash_flare,
                    particles: &mut self.particles,
                    auxiliary: &mut self.effect_aux,
                    effects: &mut self.effects,
                    damage_feedback: &mut self.damage_feedback,
                    first_person_view: &mut self.first_person_view,
                    previous_particle_events: &mut self.previous_particle_events,
                    muzzle_effects: &mut self.muzzle_effects,
                    force_overlays: &mut self.force_overlays,
                    nameplate: &mut self.hud.nameplate,
                    obituaries: &mut self.obituaries,
                    lagometer: &mut self.lagometer,
                    console: &mut self.console,
                    localization: &self.localization,
                    counters: &mut self.snapshot_observations,
                    bsp: &self.bsp,
                    trace_scratch: &mut self.trace_scratch,
                },
            );
        });
        if let Err(error) = advance {
            eprintln!("demo playback stopped: {error}");
            self.quit_requested = true;
            self.demo_session = Some(session);
            return;
        }
        if session.ended() {
            self.quit_requested = true;
        }
        update_demo_camera(self, &session);
        self.demo_session = Some(session);
    }
}

impl GpuState {
    /// Stand the live presentation on `snapshot`: the shell's team-menu
    /// reflex, entity poses, the presentation and server clocks and the
    /// local view. `observe_events` runs the once-per-snapshot event
    /// observation (`snapshot_presentation::observe`); a snapshot the
    /// previous world already observed passes `false`.
    pub(crate) fn present_live_snapshot(
        &mut self,
        snapshot: &Snapshot,
        observe_events: bool,
        game_audio: &mut Option<GameAudio>,
        visual_now: Instant,
    ) {
        if !self.live_presentation_ready() || !crate::live_session::active_snapshot(snapshot) {
            return;
        }
        self.sync_fake_noclip(&snapshot.player);
        self.ghost.snapshot(&snapshot.player);
        self.mimic_observe(snapshot);
        let Some(session) = self.live_session.as_ref() else {
            return;
        };
        let intermission = sjk_client::IntermissionView::from_player_state(&snapshot.player);
        if self.live_presentation_ready() {
            let mask = if self.local_prediction.fake_noclip() {
                std::borrow::Cow::Owned(vec![0; snapshot.area_mask.len()])
            } else {
                std::borrow::Cow::Borrowed(snapshot.area_mask.as_slice())
            };
            self.world_materials.areas.update(&mask);
        }
        // A server-owned frozen selection scene handles admission itself. Once
        // entered, later freecam elimination must not open the ordinary join UI.
        if snapshot.player.movement_type() == 6 {
            self.auto_opened_team_menu = true;
        }
        if snapshot.player.is_spectator() && intermission.is_none() && !self.auto_opened_team_menu {
            self.game_menu = true;
            self.game_menu_page = GameMenuPage::Team;
            self.game_menu_row = 0;
            self.auto_opened_team_menu = true;
            self.gameplay_input.release_keys();
        }
        let obituaries_before = self.obituaries.decoded();
        if observe_events {
            observe(
                snapshot,
                session.game_state(),
                impact_spawn::LocalView {
                    client_num: snapshot.player.client_num(),
                    third_person: self.third_person,
                    flash_point: self.last_first_person_flash,
                },
                !snapshot.player.is_spectator(),
                visual_now,
                &mut Sinks {
                    audio: game_audio,
                    vfs: self
                        .vfs
                        .as_ref()
                        .expect("live sessions retain their mounted VFS"),
                    impacts: &mut self.impacts,
                    clash_flare: &mut self.saber_clash_flare,
                    particles: &mut self.particles,
                    auxiliary: &mut self.effect_aux,
                    effects: &mut self.effects,
                    damage_feedback: &mut self.damage_feedback,
                    first_person_view: &mut self.first_person_view,
                    previous_particle_events: &mut self.previous_particle_events,
                    muzzle_effects: &mut self.muzzle_effects,
                    force_overlays: &mut self.force_overlays,
                    nameplate: &mut self.hud.nameplate,
                    obituaries: &mut self.obituaries,
                    lagometer: &mut self.lagometer,
                    console: &mut self.console,
                    localization: &self.localization,
                    counters: &mut self.snapshot_observations,
                    bsp: &self.bsp,
                    trace_scratch: &mut self.trace_scratch,
                },
            );
            crate::achievements_frame::observe(
                &mut self.achievement_tracker,
                &self.obituaries,
                session,
                snapshot,
                obituaries_before,
                visual_now,
            );
        }
        if let Some(adapter) = &mut self.legacy_world_adapter {
            // oldServerTime disappears after the server acknowledges admission.
            // Drop samples from that provisional epoch before applying the lower
            // timestamp; otherwise World::upsert correctly rejects them as stale.
            if i64::from(snapshot.server_time) < self.presentation_clock.upper_server_time() {
                self.live_world = sjk_runtime::World::new(self.live_world.id());
                *adapter = LegacyWorldAdapter::new(self.live_world.id());
                if let Some(console) = &self.console {
                    adapter.set_smooth_clients(console.smooth_clients());
                }
                self.local_actor_state = Default::default();
                for mesh in &mut self.actor_meshes {
                    mesh.angle_controller = sjk_client::LegacyPlayerAngleController::new();
                }
            }
            adapter.apply_snapshot(snapshot, session.game_state(), &mut self.live_world);
            if session.is_local() {
                self.presentation_clock
                    .follow_local(snapshot.server_time, Instant::now());
            } else {
                let event = self
                    .presentation_clock
                    .receive_snapshot(snapshot.server_time, Instant::now());
                self.net_timing.adjustment(event);
            }
        }
        if !session.is_local() {
            self.server_clock
                .observe_snapshot(snapshot.server_time, Instant::now());
        }
        if let Some(view) = intermission {
            self.local_prediction.stop(0.0);
            self.camera_position = Vec3::from_array(view.origin);
            self.camera_pitch = -view.angles[0].to_radians();
            self.camera_yaw = view.angles[1].to_radians();
        } else if crate::prediction_preview::interpolated::mode(
            self.console.as_ref(),
            session.game_state(),
            &snapshot.player,
        )
        .is_some()
        {
            // Follow, synchronous, `cg_noPredict` and JA+ kick movement are interpolated
            // each rendered frame.
            let view_height = snapshot.player.view_height() as f32;
            self.local_prediction.stop(view_height);
            self.camera_position =
                Vec3::from_array(snapshot.player.origin()) + Vec3::Z * view_height;
        } else {
            self.local_prediction
                .resolve_ride(snapshot, session.game_state());
            self.camera_position =
                self.local_prediction
                    .apply_snapshot(snapshot, &self.bsp, &mut self.trace_scratch);
            self.net_timing
                .prediction(self.local_prediction.last_sample());
            // Inputs that do not go through at the gameplay FPS open its card.
            let sample = self.local_prediction.last_sample();
            if let Some(console) = &mut self.console
                && console.observe_input_health(&sample, std::time::Instant::now())
            {
                console.open_gameplay_fps_alert();
                self.sync_cursor_policy();
            }
        }
        // After the replay: a predicted local view follows the predicted `delta_angles`.
        super::prediction_preview::observe_view(
            self,
            &snapshot.player,
            intermission.is_none()
                && crate::local_prediction::predicts_local_view(snapshot.player.movement_flags()),
        );
        self.net_timing.player_state(
            snapshot.player.movement_type(),
            snapshot.player.is_spectator(),
        );
    }

    /// A freshly installed world was built from the snapshot its load
    /// started with, seconds before the cut; stand it on the newest snapshot
    /// so its first frame does not show that stale state until the next
    /// snapshot arrives.
    pub(crate) fn present_latest_live_snapshot(&mut self, game_audio: &mut Option<GameAudio>) {
        let Some(snapshot) = self
            .live_session
            .as_ref()
            .map(|session| session.latest_snapshot().clone())
        else {
            return;
        };
        self.present_live_snapshot(&snapshot, false, game_audio, Instant::now());
    }
}

fn update_demo_camera(gpu: &mut GpuState, session: &demo_playback::Session) {
    let time = session.current_server_time();
    let snapshot = session.snapshot_at_or_before(time as i32);
    gpu.world_materials.areas.update(&snapshot.area_mask);
    let player = &snapshot.player;
    let local_client = u64::try_from(session.game_state().client_num).unwrap_or_default();
    let local_id = EntityId::new(local_client + 1);
    let presented = session.world().entity(local_id);
    let mut origin = presented
        .map(|entity| entity.sample(time).translation)
        .unwrap_or_else(|| player.origin());
    origin[2] += player.view_height() as f32;
    let angles = presented
        .and_then(|entity| entity.sample_pose(time))
        .map(|pose| pose.view_angles_degrees)
        .unwrap_or_else(|| player.view_angles());
    gpu.camera_position = Vec3::from_array(origin);
    gpu.detached_camera = session.camera().detached();
    gpu.camera_pitch = -angles[0].to_radians();
    gpu.camera_yaw = angles[1].to_radians();
    if let demo_playback::Camera::FirstPersonPitch(pitch) = session.camera() {
        gpu.camera_pitch = -f32::from(pitch).to_radians();
    }
    gpu.local_prediction
        .set_view_height(player.view_height() as f32);
    if let demo_playback::Camera::Spectate(client) = session.camera()
        && let Some(entity) = session.world().entity(EntityId::new(u64::from(client) + 1))
    {
        let transform = entity.sample(time);
        let rotation = Quat::from_array(transform.rotation);
        gpu.camera_position = Vec3::from_array(transform.translation) + Vec3::Z * 40.0;
        let forward = rotation * Vec3::X;
        gpu.camera_yaw = forward.y.atan2(forward.x);
        gpu.camera_pitch = forward.z.asin();
    }
    if let demo_playback::Camera::Free { origin, yaw, pitch } = session.camera() {
        gpu.camera_position = Vec3::from_array(origin);
        gpu.camera_yaw = yaw.to_radians();
        gpu.camera_pitch = -pitch.to_radians();
        return;
    }
    let (number, offset) = match session.camera() {
        demo_playback::Camera::LookAt(number) => (number, None),
        demo_playback::Camera::Orbit(number) => (number, Some(Vec3::new(-110.0, -80.0, 50.0))),
        _ => return,
    };
    if let Some(entity) = session.world().entity(EntityId::new(u64::from(number) + 1)) {
        let target = Vec3::from_array(entity.sample(time).translation) + Vec3::Z * 40.0;
        if let Some(offset) = offset {
            gpu.camera_position = target + offset;
        }
        let to_target = target - gpu.camera_position;
        gpu.camera_yaw = to_target.y.atan2(to_target.x);
        gpu.camera_pitch = (to_target.z / to_target.length().max(1.0)).asin();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CS_PLAYERS`: the first client's configstring.
    const CS_PLAYERS: usize = 1_131;

    fn kill(target: u16, attacker: u16, key: &'static str) -> sjk_client::ObituaryEvent {
        sjk_client::ObituaryEvent {
            target,
            attacker,
            means_of_death: 3,
            message: "",
            attacker_message: Some(key),
            local_fragged: false,
            local_was_killed: false,
            server_time: 0,
            attacker_force: 0,
        }
    }

    fn players(entries: &[(u16, &[u8])]) -> GameState {
        let mut game = GameState::empty_local(0);
        for (client, info) in entries {
            game.replace_config_string(CS_PLAYERS + usize::from(*client), info.to_vec())
                .unwrap();
        }
        game
    }

    /// Sol's `noname was sabered by {JoF}emiah{I}`: the victim's name has a
    /// Latin-1 letter (`é`, byte 0xE9), so the whole configstring is not UTF-8.
    #[test]
    fn a_latin1_name_is_printed_not_noname() {
        let game = players(&[
            (3, b"n\\R\xe9mi\\t\\0\\model\\kyle/default\\ds\\m"),
            (7, b"n\\{JoF}emiah{I}\\t\\0\\model\\jedi_hm\\ds\\m"),
        ]);
        let mut localization = Localization::default();
        localization
            .strings
            .insert("KILLED_SABER".into(), "was sabered by".into());
        assert_eq!(
            format_obituary(kill(3, 7, "KILLED_SABER"), &game, &localization),
            "Rémi^7 was sabered by {JoF}emiah{I}^7"
        );
    }

    /// `CG_Obituary` ends each name with `^7`, so a colour a name leaves open
    /// does not run into the rest of the line.
    #[test]
    fn each_name_ends_in_white() {
        let game = players(&[(0, b"\\n\\^1Red"), (1, b"\\n\\^4Blue")]);
        let line = format_obituary(kill(0, 1, "KILLED_SABER"), &game, &Localization::default());
        assert_eq!(line, "^1Red^7 KILLED_SABER ^4Blue^7");
        let mut fall = kill(0, 0, "");
        fall.attacker_message = None;
        fall.message = "SUICIDE_FALLDEATH_MALE";
        assert_eq!(
            format_obituary(fall, &game, &Localization::default()),
            "^1Red^7 SUICIDE_FALLDEATH_MALE"
        );
    }

    #[test]
    fn only_a_slot_without_a_name_is_noname() {
        let game = players(&[(1, b"\\n\\Sol\\t\\0")]);
        let line = format_obituary(kill(5, 1, "KILLED_SABER"), &game, &Localization::default());
        assert_eq!(line, "noname^7 KILLED_SABER Sol^7");
    }
}

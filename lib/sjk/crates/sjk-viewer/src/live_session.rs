//! Per-frame live network polling and command submission.

use super::{GameAudio, GpuState, server_commands, session_transition};
use std::time::{Duration, Instant};

#[path = "emplaced_view.rs"]
mod emplaced_view;

/// Consume queued snapshots in order, stopping on timeout/failure or after eight.
pub(crate) fn drain_snapshots(mut receive_and_present: impl FnMut() -> bool) {
    for _ in 0..8 {
        if !receive_and_present() {
            break;
        }
    }
}

impl GpuState {
    /// Only the installed live world shares the session's snapshot time domain.
    pub(crate) fn live_presentation_ready(&self) -> bool {
        if self
            .live_session
            .as_ref()
            .is_some_and(sjk_client::ClientSession::is_local)
        {
            return self.live_map_installed && !self.is_menu_world;
        }
        self.live_session.is_some()
            && self.live_map_installed
            && !self.is_menu_world
            && !self.pending_map_reload
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
    }

    /// Drain queued snapshots, surface lifecycle failures, and submit due input.
    pub(crate) fn update_live_session(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        visual_now: Instant,
        timing: &mut crate::frame_pacing::budget::Timer,
    ) {
        use crate::frame_pacing::budget::Phase;
        timing.mark(Phase::Snapshot);
        drain_snapshots(|| {
            let Some(session) = &mut self.live_session else {
                return false;
            };
            timing.mark(Phase::Receive);
            let received = session.receive_snapshot(Duration::ZERO).cloned();
            timing.mark(Phase::Snapshot);
            // svc_gamestate may have been installed even if the following
            // snapshot times out. Consume its clock reset before sending input.
            let reactions = session_transition::consume(session.drain_transitions());
            if received.is_ok()
                && !reactions.reload_world
                && !self.pending_map_reload
                && self.world_load_task.is_none()
                && self.world_install_task.is_none()
                && let (Some(audio), Some(vfs)) = (game_audio.as_mut(), &self.vfs)
            {
                audio.prepare_config_strings(
                    session.config_string_changes(),
                    session.game_state(),
                    vfs,
                );
            }
            if reactions.reload_world {
                if let Some(audio) = game_audio.as_mut() {
                    audio.begin_map_change();
                }
                // Before presenting: the server reports inflated snapshot
                // times until it sees this client acknowledge the new
                // `serverId` (`sv_snapshot.cpp:190-198`), and a command
                // stamped from that era outranks every later one once
                // `SV_ClientEnterWorld` latches it (`sv_client.cpp:576`).
                // Stock stamps the whole map load 0 (`CL_ClearState`); the
                // world built for the new timeline anchors the clock again.
                self.server_clock.restart();
                self.pending_map_reload = true;
                self.ghost_leave();
                self.retain_world_for_connection();
                self.resident.after_sequence = reactions.gamestate_sequence.or_else(|| {
                    self.resident
                        .session
                        .as_ref()
                        .map(|s| s.latest_snapshot().message_sequence)
                });
            }
            if let Some(reason) = reactions.disconnect_reason {
                self.session_disconnected(reason);
                return false;
            }
            if reactions.reload_world {
                return false;
            }
            let snapshot = match received {
                Ok(snapshot) => snapshot,
                Err(error) if error.is_timeout() => return false,
                Err(error) => {
                    self.session_disconnected(error.to_string());
                    return false;
                }
            };
            if self.live_session.is_none() {
                return false;
            }
            // CG_ProcessSnapshots does not install SNAPFLAG_NOT_ACTIVE frames.
            // These may contain cleared player state while a new map is primed.
            if !active_snapshot(&snapshot) {
                return true;
            }
            self.net_timing.snapshot_received(snapshot.server_time);
            self.present_live_snapshot(&snapshot, true, game_audio, visual_now);
            true
        });
        timing.mark(Phase::Commands);
        self.ghost_frame();
        let talking = self.talk_button();
        // The FPS cap the gameplay FPS's Auto follows (`command_rate`).
        let frame_cap = self.frame_cap();
        // A held push or pull key fills every command no press filled
        // (`input::force_repeat`), while the keys are the game's.
        let force_repeat = if self.text_has_keyboard() {
            None
        } else {
            self.console
                .as_ref()
                .and_then(crate::console::ViewerConsole::force_repeat_command)
        };
        let Some(session) = &mut self.live_session else {
            return;
        };
        if let Some(silence) = self.net_timing.silence_to_report(Instant::now()) {
            // A session with no snapshots reports nothing else, so say what
            // the socket sees: a quiet server, packets arriving and being
            // discarded, or packets going out to nobody.
            let traffic = session.connection_traffic();
            let expected = session.server();
            let rejected = session
                .last_rejected_packet()
                .map(|(source, bytes)| format!("{source} {:?}", String::from_utf8_lossy(bytes)));
            crate::log::progress(format_args!(
                "no snapshots for {:.1} s: sent={} received={} stale={} wrong-source={} \
                 out-of-band={} expected={expected} last-rejected={}",
                silence.as_secs_f64(),
                traffic.sent,
                traffic.received,
                traffic.stale,
                traffic.wrong_source,
                traffic.out_of_band,
                rejected.as_deref().unwrap_or("none"),
            ));
        }
        server_commands::consume(
            session,
            &self.localization,
            self.resident.session.is_none().then_some(&mut self.chat),
            self.console.as_mut(),
            self.legacy_world_adapter.as_mut(),
            &mut self.clientinfo_watch,
        );

        // User commands are made on the gameplay FPS's grid and wait for the next
        // `cl_maxpackets` packet (`command_rate`).
        let now = Instant::now();
        let server_time = self.server_clock.server_time(now);
        let step =
            self.console
                .as_mut()
                .map_or(sjk_client::command_rate::COMMAND_MILLIS, |console| {
                    console.set_frame_cap(frame_cap);
                    console.command_step()
                });
        let due = self
            .command_schedule
            .due(server_time, self.server_clock.anchored(), now, step);
        if let Some(console) = &self.console {
            session.set_packet_dup(console.packet_dup());
        }
        if due.as_slice().is_empty() {
            self.send_due_packet(now);
            return;
        }
        let intermission =
            session.latest_snapshot().player.movement_type() == sjk_client::PM_INTERMISSION;
        if intermission
            && self
                .scoreboard
                .score_request_due(session.latest_snapshot().server_time)
            && let Err(error) = session.send_reliable_command(b"score")
        {
            eprintln!("failed to request intermission scoreboard: {error}");
        }
        let snapshot = session.latest_snapshot();
        let delta_angles = self
            .gameplay_input
            .view_authority
            .command_delta()
            .unwrap_or(snapshot.player.delta_angles());
        self.gameplay_input
            .selection
            .sync(&snapshot.player, server_time);
        // At an emplaced gun, a view past its arc is turned back (`CG_EmplacedView`).
        // The camera's yaw is in radians, the game's in degrees.
        if let Some(yaw) = emplaced_view::forced_yaw(snapshot, self.camera_yaw.to_degrees()) {
            self.camera_yaw = yaw.to_radians();
        }
        let weapon = self
            .selected_weapon
            .unwrap_or_else(|| snapshot.player.weapon());
        let force_power = snapshot.player.selected_force_power();
        let known = self.gameplay_input.selection.known(&snapshot.player);
        let flip_kick = self
            .console
            .as_ref()
            .map(crate::console::ViewerConsole::flip_kick_timing)
            .unwrap_or_default();
        // A frame slower than 8 ms makes a command for each slot it passed; the
        // first carries the frame's presses, the rest the keys still held.
        let last_stamp = due.as_slice().last().copied().unwrap_or_default();
        for &stamp in due.as_slice() {
            self.gameplay_input
                .set_command_age(u64::try_from(last_stamp.saturating_sub(stamp)).unwrap_or(0));
            let mut command = self.gameplay_input.user_command(
                stamp,
                self.camera_pitch,
                self.camera_yaw,
                delta_angles,
                weapon,
                force_power,
                match self.pending_generic_command {
                    0 => force_repeat.unwrap_or(0),
                    pressed => pressed,
                },
            );
            self.gameplay_input.apply_flip_kick(&mut command, flip_kick);
            command.buttons = sjk_game_jka::pmove_talk::command_buttons(command.buttons, talking);
            // JoF EJK's Force wheel: a selected Stasis, Repulse or Dash takes
            // `+useforce`.
            let (buttons, wheel_command) = self
                .gameplay_input
                .force_wheel_buttons(command.buttons, known);
            command.buttons = buttons;
            // A ghost replay sends the recorded command instead (`ghost_replay`).
            if self.local_prediction.fake_noclip() {
                self.ghost.interrupt("you are flying");
                self.mimic.stop_with("^3mimic: stopped, you are flying.");
            }
            let pose = self
                .local_prediction
                .committed_state()
                .map(crate::ghost_replay::Pose::of);
            let replay = pose.and_then(|pose| self.ghost.drive(&command, &pose));
            // Or the mimic copies another player (`mimic`).
            let replay = replay.or_else(|| {
                let state = self.local_prediction.committed_state()?;
                self.mimic.drive(&command, state)
            });
            if let Some(drive) = &replay {
                command = drive.command;
                if let Some([pitch, yaw]) = drive.view {
                    self.camera_pitch = (-pitch).to_radians();
                    self.camera_yaw = yaw.to_radians();
                }
            }
            if !self.local_prediction.free_camera()
                && replay.is_none()
                && let Some(wheel_command) = wheel_command
                && let Err(error) = session.send_reliable_command(wheel_command.as_bytes())
            {
                eprintln!("failed to send {wheel_command}: {error}");
            }
            let sent = self.local_prediction.command_for_server(command);
            session.queue_command(&sent);
            self.gameplay_input.finish_command();
            if !intermission
                && let Some(position) = self.local_prediction.apply_command(
                    command,
                    sent,
                    &self.bsp,
                    &mut self.trace_scratch,
                )
            {
                self.camera_position = position;
            }
            if !intermission && let Some(pose) = pose {
                self.ghost
                    .record(&command, &pose, self.local_prediction.committed_state());
            }
            self.pending_generic_command = 0;
        }
        self.gameplay_input.set_command_age(0);
        self.send_due_packet(now);
    }

    /// Send the queued user commands once `cl_maxpackets` allows a packet.
    fn send_due_packet(&mut self, now: Instant) {
        let Some(session) = &mut self.live_session else {
            return;
        };
        let max_packets = self.console.as_ref().map_or(
            crate::console::DEFAULT_MAX_PACKETS,
            crate::console::ViewerConsole::max_packets,
        );
        if session.queued_commands() == 0 || !self.packet_pacer.ready(now, max_packets) {
            return;
        }
        if let Err(error) = session.send_queued_commands() {
            self.session_disconnected(error.to_string());
            return;
        }
        self.packet_pacer.sent(now);
    }
}

pub(crate) fn active_snapshot(snapshot: &sjk_protocol::Snapshot) -> bool {
    snapshot.flags & 2 == 0
}

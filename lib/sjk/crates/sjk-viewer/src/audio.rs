//! Platform audio boundary: one software mixer feeding one rodio stream.
//!
//! Asset reads and decoding happen on workers. The rodio source owns the mixer
//! and receives bounded SPSC commands, so rendering cannot starve audio and
//! the platform callback never takes a mutex. The only remaining drop-out
//! mode is the callback overrunning its real-time budget; `underruns` counts
//! mix blocks that took longer than the block duration to render.

pub(crate) mod sjk_chat_sound;
pub(crate) mod ui_cues;

use crate::audio_output::{AudioCommand, AudioOutput, LOOPS, SAMPLE_RATE, VOICES};
use crate::console::ViewerConsole;
use sjk_audio::{ChannelId, MixerConfig, PlayRequest, SoundHandle, SourceId};
use sjk_bsp::Bsp;
use sjk_client::{
    LegacyMaintainedActionKind, LegacyMusicAction, LegacySaberView, LegacySoundAdapter,
    legacy_evaluate_trajectory,
};
use sjk_protocol::{EntityState, GameState, Snapshot};
use sjk_runtime::{EntityId, World};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, atomic::Ordering};
use std::time::Instant;

#[path = "audio_assets.rs"]
mod assets;
#[path = "audio_commands.rs"]
mod commands;
#[path = "audio_controls.rs"]
mod controls;
pub(crate) use controls::EMOTE_CHANNEL;
#[path = "audio_dynamic.rs"]
mod dynamic;
#[path = "audio_feedback.rs"]
mod feedback;
#[path = "audio_transitions.rs"]
mod transitions;
pub(crate) use assets::SoundPrefetch;
#[path = "audio_legacy.rs"]
mod legacy;

use legacy::LegacyLoadTask;
#[path = "audio_config_strings.rs"]
mod config_strings;
use config_strings::{DeferredSoundChanges, SoundTableRefresh};

const SOLID_BMODEL: u32 = 0x00ff_ffff; // codemp/qcommon/q_shared.h:1540

/// Process-lifetime audio state. Only the rodio callback owns the mixer.
pub(crate) struct GameAudio {
    output: AudioOutput,
    reported_underruns: u64,
    handles: HashMap<String, SoundHandle>,
    next_handle: u32,
    legacy: Option<LegacySoundAdapter>,
    legacy_load: Option<LegacyLoadTask>,
    sound_table_refresh: Option<SoundTableRefresh>,
    legacy_vfs: Option<Arc<VirtualFileSystem>>,
    map_music: Option<(SoundHandle, SoundHandle)>,
    duel_music: Option<SoundHandle>,
    deferred_snapshots: VecDeque<(Instant, Snapshot, DeferredSoundChanges)>,
    music_value: Option<Vec<u8>>,
    dynamic: Option<dynamic::Level>,
    allow_dynamic: bool,
    duel_enabled: bool,
    chat_beeps: [bool; 2],
    deferred_starts: u64,
    maximum_deferred_start_ms: f64,
    /// `cg_footsteps`: whether the footstep event family is voiced.
    footsteps: bool,
    voice_policy: feedback::VoicePolicy,
    kill_sounds: i64,
    transitions: transitions::Transitions,
    /// `SJK_TRACE_AUDIO=1`: when the next level line is due.
    trace_due: Option<Instant>,
    /// Players muted on this PC, whose sounds are not played (`audio_mute.rs`).
    mute: crate::audio_mute::AudioMute,
    /// The blade skins whose sounds the gamestate's tables get (`saber_skins.rs`).
    blade_skins: Arc<crate::saber_skins::LoadedSkins>,
    /// The generation of the blade skins registered in `legacy`, once it is ready.
    saber_sets_generation: Option<u64>,
}

impl GameAudio {
    /// Feed immediate local movement through the snapshot sound resolver and mixer.
    pub(crate) fn observe_predicted_event(
        &mut self,
        event: sjk_client::predicted_events::PredictedEvent,
        snapshot: &Snapshot,
    ) {
        let Some(adapter) = &mut self.legacy else {
            return;
        };
        adapter.observe_predicted_event(event, snapshot);
        for decision in adapter.decisions() {
            if let Some(handle) = decision.handle
                && controls::allows_event(decision.event, self.footsteps, self.chat_beeps)
                && self
                    .voice_policy
                    .allows(*decision, snapshot.player.client_num())
            {
                self.output
                    .send(AudioCommand::Play(handle, decision.request));
            }
        }
    }
    /// Which blade skin's sound set each client slot wears (`saber_skins.rs`); cheap,
    /// called every frame. Until the gamestate's tables are ready there is nothing to set.
    pub(crate) fn set_saber_sound_overrides(
        &mut self,
        clients: &[Option<u8>; sjk_client::SABER_SOUND_CLIENTS],
    ) {
        if let Some(adapter) = &mut self.legacy {
            adapter.set_saber_sound_overrides(clients);
        }
    }

    /// Create the one process-lifetime output stream and its decode worker.
    pub(crate) fn start() -> Option<Self> {
        let output = AudioOutput::start(MixerConfig {
            voices: VOICES,
            loops: LOOPS,
            sample_rate: SAMPLE_RATE,
            loop_attenuation: sjk_client::legacy_sound_attenuation(0),
            doppler: sjk_client::legacy_doppler_config(true),
            protected_channel: Some(EMOTE_CHANNEL),
        })?;
        Some(Self::with_output(output))
    }

    /// Assemble audio state around a real or test-owned null output.
    pub(super) fn with_output(output: AudioOutput) -> Self {
        let mut audio = Self {
            output,
            reported_underruns: 0,
            handles: HashMap::with_capacity(2_048),
            next_handle: 0,
            legacy: None,
            legacy_load: None,
            sound_table_refresh: None,
            legacy_vfs: None,
            blade_skins: Arc::default(),
            saber_sets_generation: None,
            map_music: None,
            duel_music: None,
            deferred_snapshots: VecDeque::with_capacity(16),
            music_value: None,
            dynamic: None,
            allow_dynamic: true,
            duel_enabled: true,
            chat_beeps: [true; 2],
            deferred_starts: 0,
            maximum_deferred_start_ms: 0.0,
            footsteps: true,
            voice_policy: feedback::VoicePolicy::default(),
            kill_sounds: 2,
            transitions: transitions::Transitions::default(),
            trace_due: std::env::var_os("SJK_TRACE_AUDIO").map(|_| Instant::now()),
            mute: crate::audio_mute::AudioMute::default(),
        };
        // `s_volume`/`s_musicvolume` defaults from `S_Init` in
        // codemp/client/snd_dma.cpp:462-466. The console overwrites these on
        // the first frame, but the initial ring fill must use them too.
        audio.set_gains(0.5, 0.25);
        audio
    }

    /// Update listener axes (`S_Respatialize`, snd_dma.cpp:2503) once per frame.
    pub(crate) fn listener(
        &mut self,
        origin: Vec3,
        forward: Vec3,
        right: Vec3,
        up: Vec3,
        _velocity: Vec3,
    ) {
        self.output.send(AudioCommand::Listener {
            origin: origin.to_array(),
            forward: forward.to_array(),
            right: right.to_array(),
            up: up.to_array(),
            // S_Respatialize supplies no listener velocity. Feeding predicted
            // motion into the generic mixer's relative Doppler pitch shifts
            // stationary hum/ambient loops (up to the legacy 50x clamp).
            velocity: [0.0; 3],
        });
    }

    /// Play newly latched `CG_EntityEvent` sounds.
    pub(crate) fn observe_snapshot(
        &mut self,
        snapshot: &Snapshot,
        game_state: &GameState,
        vfs: &VirtualFileSystem,
    ) {
        // Establish the new snapshot listener before any queued starts are consumed.
        self.transitions.race = crate::chat::combat::race(game_state, &snapshot.player);
        let angles = snapshot.player.view_angles();
        let (sp, cp) = angles[0].to_radians().sin_cos();
        let (sy, cy) = angles[1].to_radians().sin_cos();
        self.listener(
            glam::Vec3::from_array(snapshot.player.origin()),
            glam::Vec3::new(cp * cy, cp * sy, -sp),
            // Quake's `AngleVectors` right axis (+Y is the listener's left).
            glam::Vec3::new(sy, -cy, 0.0),
            glam::Vec3::Z,
            glam::Vec3::ZERO,
        );
        self.poll_legacy_load();
        self.output
            .send(AudioCommand::ListenerSource(Some(SourceId(u32::from(
                snapshot.player.client_num(),
            )))));
        for entity in &snapshot.entities {
            // `CG_SetEntitySoundPosition`: a door's sounds follow its middle.
            let origin = self.legacy.as_ref().map_or_else(
                || entity.trajectory_base(),
                |adapter| adapter.sound_origin(entity),
            );
            self.output.send(AudioCommand::SourcePosition(
                SourceId(u32::from(entity.number())),
                origin,
            ));
        }
        if self.legacy.is_none() {
            if self.deferred_snapshots.len() == 16 {
                self.deferred_snapshots.pop_front();
            }
            let changes = self
                .sound_table_refresh
                .as_ref()
                .and_then(SoundTableRefresh::pending_snapshot);
            self.deferred_snapshots
                .push_back((Instant::now(), snapshot.clone(), changes));
            return;
        }
        let mut music_action = None;
        if let Some(adapter) = &mut self.legacy {
            let output = &mut self.output;
            let handles = &mut self.handles;
            let next_handle = &mut self.next_handle;
            adapter.refresh_clients(game_state, vfs, |path, bytes| {
                let key = path.to_ascii_lowercase();
                if let Some(handle) = handles.get(&key) {
                    return Some(*handle);
                }
                let extension = path.rsplit('.').next().unwrap_or("wav");
                let handle = SoundHandle(*next_handle);
                *next_handle = next_handle.wrapping_add(1);
                output.decode(handle, bytes, extension);
                handles.insert(key, handle);
                Some(handle)
            });
            adapter.set_local_health_pain(self.transitions.old_pain);
            adapter.observe_snapshot(snapshot);
            self.mute.observe(&snapshot.entities);
            let footsteps = self.footsteps;
            for decision in adapter.decisions() {
                if let Some(handle) = self.transitions.handle(decision, snapshot.server_time)
                    && controls::allows_event(decision.event, footsteps, self.chat_beeps)
                    && self
                        .voice_policy
                        .allows(*decision, snapshot.player.client_num())
                    && !self.mute.silences(decision.request.source, decision.cause)
                {
                    self.output
                        .send(AudioCommand::Play(handle, decision.request));
                }
            }
            for action in adapter.maintained_actions() {
                match action.kind {
                    LegacyMaintainedActionKind::Start => {}
                    LegacyMaintainedActionKind::Stop => {
                        self.output.send(AudioCommand::StopLoops(action.source));
                    }
                    LegacyMaintainedActionKind::Mute => {
                        if let Some(channel) = action.channel {
                            self.output
                                .send(AudioCommand::StopChannel(action.source, channel));
                        }
                        self.output.send(AudioCommand::StopLoops(action.source));
                    }
                }
            }
            music_action = adapter.music_actions().last().copied();
        }
        self.transition_cues(snapshot, true);
        match music_action {
            Some(LegacyMusicAction::StartPrivateDuel) if self.duel_enabled => {
                if let Some(handle) = self.duel_music {
                    self.output.start_music(handle, handle);
                }
            }
            Some(LegacyMusicAction::RestoreMap) => {
                if let Some((intro, repeating)) = self.map_music {
                    self.output.start_music(intro, repeating);
                }
            }
            _ => {}
        }
    }

    fn play_deferred_snapshots(&mut self) {
        while let Some((queued_at, snapshot, changes)) = self.deferred_snapshots.pop_front() {
            if let Some((mut changes, game)) = changes {
                if let Some(vfs) = self.legacy_vfs.clone() {
                    changes.drain(|index| self.apply_sound_slot(index, &game, &vfs));
                }
            }
            let Some(adapter) = &mut self.legacy else {
                return;
            };
            adapter.set_local_health_pain(self.transitions.old_pain);
            adapter.observe_snapshot(&snapshot);
            self.mute.observe(&snapshot.entities);
            let latency = queued_at.elapsed().as_secs_f64() * 1_000.0;
            // Advance all latches, but never compress loading history into one audio block.
            if !self.deferred_snapshots.is_empty() || latency > 300.0 {
                self.transitions.observe(&snapshot);
                continue;
            }
            for decision in adapter.decisions() {
                if let Some(handle) = self.transitions.handle(decision, snapshot.server_time)
                    && controls::allows_event(decision.event, self.footsteps, self.chat_beeps)
                    && self
                        .voice_policy
                        .allows(*decision, snapshot.player.client_num())
                    && !self.mute.silences(decision.request.source, decision.cause)
                {
                    self.output
                        .send(AudioCommand::Play(handle, decision.request));
                    self.deferred_starts = self.deferred_starts.saturating_add(1);
                    self.maximum_deferred_start_ms = self.maximum_deferred_start_ms.max(latency);
                }
            }
            self.transition_cues(&snapshot, true);
        }
        if self.deferred_starts != 0 {
            crate::log::progress(format_args!(
                "deferred audio starts: count={} max-latency={:.1}ms",
                self.deferred_starts, self.maximum_deferred_start_ms
            ));
        }
    }

    pub(crate) fn deferred_start_stats(&self) -> (u64, f64) {
        (self.deferred_starts, self.maximum_deferred_start_ms)
    }

    /// Play cgame's saber ignition/retraction for weapon switches, once per rendered
    /// frame as `CG_AddPacketEntities` runs: `predicted` is the live predicted state
    /// (`cg.predictedPlayerState`); without it the snapshot's playerstate is the view.
    pub(crate) fn observe_saber_switches(
        &mut self,
        snapshot: &Snapshot,
        predicted: Option<&sjk_client::pmove::MovementState>,
    ) {
        let Some(adapter) = &mut self.legacy else {
            return;
        };
        let view = predicted.map_or_else(
            || LegacySaberView::from_player_state(&snapshot.player),
            LegacySaberView::from_movement_state,
        );
        adapter.observe_saber_switches(snapshot, view);
        for decision in adapter.decisions() {
            if let Some(handle) = decision.handle
                && controls::allows_event(decision.event, self.footsteps, self.chat_beeps)
            {
                self.output
                    .send(AudioCommand::Play(handle, decision.request));
            }
        }
    }

    /// The client slots muted on this PC (bits), whose sounds are no longer played;
    /// called every frame, it costs a comparison while they stay the same.
    pub(crate) fn set_muted_clients(&mut self, clients: u32) {
        self.mute.set_clients(clients);
    }

    /// Whether a sound from `source`, caused by `cause`, belongs to a muted player.
    pub(crate) fn silenced(&self, source: SourceId, cause: Option<u16>) -> bool {
        self.mute.silences(source, cause)
    }

    /// Rebuild all codemp loops at rendered cadence from interpolated origins.
    ///
    /// The set is bracketed by `ClearLoops`/`CommitLoops` because the audio
    /// thread may drain the queue while it is still being filled; without the
    /// commit, loops not yet re-sent would drop out for a mix block.
    pub(crate) fn update_frame_loops(
        &mut self,
        snapshot: &Snapshot,
        presented_time: i32,
        listener_origin: [f32; 3],
        world: &World,
        bsp: &Bsp,
    ) {
        self.poll_legacy_load();
        self.output.send(AudioCommand::ClearLoops);
        self.send_frame_loops(snapshot, presented_time, listener_origin, world, bsp);
        self.output.send(AudioCommand::CommitLoops);
    }

    fn send_frame_loops(
        &mut self,
        snapshot: &Snapshot,
        presented_time: i32,
        listener_origin: [f32; 3],
        world: &World,
        bsp: &Bsp,
    ) {
        let Some(adapter) = &mut self.legacy else {
            return;
        };
        adapter.observe_loops(snapshot, presented_time, listener_origin, |state| {
            presented_entity_origin(world, bsp, state, presented_time)
        });
        // A muted player's loops (saber hum, Force, a thrown saber's hum) are left out.
        let mute = &self.mute;
        for decision in adapter.loop_decisions() {
            if let Some(handle) = decision.handle
                && !mute.silences(decision.request.source, None)
            {
                self.output.send(AudioCommand::SetLoop(
                    handle,
                    decision.request,
                    decision.velocity,
                ));
            }
        }
        for shot in adapter.ambient_shots() {
            if let Some(handle) = shot.handle
                && !mute.silences(shot.request.source, None)
            {
                self.output.send(AudioCommand::Play(handle, shot.request));
            }
        }
        adapter.observe_maintained_loops(snapshot, |state| {
            presented_entity_origin(world, bsp, state, presented_time)
        });
        for decision in adapter.maintained_loop_decisions() {
            if let Some(handle) = decision.handle
                && !mute.silences(decision.request.source, None)
            {
                self.output.send(AudioCommand::SetLoop(
                    handle,
                    decision.request,
                    decision.velocity,
                ));
            }
        }
    }
}

const TRACE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

fn presented_entity_origin(
    world: &World,
    bsp: &Bsp,
    state: &EntityState,
    presented_time: i32,
) -> [f32; 3] {
    let id = EntityId::new(u64::from(state.number()) + 1);
    let mut origin = world.entity(id).map_or_else(
        || {
            legacy_evaluate_trajectory(
                state.trajectory_base(),
                state.trajectory_delta(),
                state.trajectory_type(),
                state.trajectory_time(),
                state.trajectory_duration(),
                presented_time,
            )
        },
        |entity| entity.sample(i64::from(presented_time)).translation,
    );
    if state.solid() == SOLID_BMODEL
        && let Ok(index) = usize::try_from(state.model_index())
        && let Some(model) = bsp.render().inline_model(index)
    {
        for axis in 0..3 {
            origin[axis] += (model.minimums[axis] + model.maximums[axis]) * 0.5;
        }
    }
    origin
}

use glam::Vec3;

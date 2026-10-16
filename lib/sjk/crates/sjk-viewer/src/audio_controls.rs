//! Listener-facing playback controls and output diagnostics.
use super::*;

impl crate::GpuState {
    /// Update the process-lifetime listener and gains before completing this render frame.
    pub(crate) fn finish_audio_frame(
        &self,
        audio: &mut Option<GameAudio>,
        forward: glam::Vec3,
        right: glam::Vec3,
    ) {
        if let Some(audio) = audio {
            audio.listener(
                self.camera_position,
                forward,
                right,
                glam::Vec3::Z,
                glam::Vec3::ZERO,
            );
            audio.sync_gains(self.console.as_ref());
            if self.live_session.is_none() {
                audio.idle_frame();
            }
            audio.end_frame();
        }
    }
}

/// The channel emote sounds play on: JKA's `CHAN_MUSIC`, which no entity sound of the
/// game uses, so an emote's music never cuts a voice or a swing. The mixer protects
/// it: a full voice table gives up every other sound before an emote's.
pub(crate) const EMOTE_CHANNEL: ChannelId = ChannelId(13);

/// Apply independent notification and footstep switches without muting combat.
pub(super) fn allows_event(
    event: sjk_client::LegacySoundEvent,
    footsteps: bool,
    chat: [bool; 2],
) -> bool {
    match event {
        sjk_client::LegacySoundEvent::ChatBeep => chat[0],
        sjk_client::LegacySoundEvent::TeamChatBeep => chat[1],
        _ => footsteps || !event.is_footstep(),
    }
}

impl GameAudio {
    /// Apply the live archived `s_volume` and `s_musicvolume` values.
    pub(crate) fn set_gains(&mut self, effects: f32, music: f32) {
        self.output.send(AudioCommand::Gains(effects, music));
    }

    /// Pull the two codemp volume cvars through the existing shell registry.
    pub(crate) fn sync_gains(&mut self, console: Option<&ViewerConsole>) {
        self.transitions.hits = console
            .and_then(|c| c.integer_cvar("cg_hitsounds"))
            .unwrap_or(0);
        self.transitions.duel = console
            .and_then(|c| c.integer_cvar("cg_duelsounds"))
            .unwrap_or(1);
        self.transitions.old_pain = console
            .and_then(|c| c.bool_cvar("cg_oldpainsounds"))
            .unwrap_or(false);
        self.voice_policy.sample(console);
        self.kill_sounds = console
            .and_then(|c| c.integer_cvar("cg_killsounds"))
            .unwrap_or(2);
        if let Some(adapter) = &mut self.legacy {
            adapter.set_chat_sound_mode(
                console
                    .and_then(|c| c.integer_cvar("cg_chatsounds"))
                    .unwrap_or(1),
                console
                    .and_then(|c| c.integer_cvar("cg_cleanchatbox"))
                    .unwrap_or(0)
                    != 0,
            );
        }
        self.chat_beeps = ["cg_chatBeep", "cg_teamChatBeep"].map(|name| {
            crate::cgame_options::chat_sounds(console)
                && console.and_then(|c| c.bool_cvar(name)).unwrap_or(true)
        });
        self.duel_enabled = console
            .and_then(|c| c.bool_cvar("cg_duelMusic"))
            .unwrap_or(true);
        self.output.send(AudioCommand::Separation(
            console
                .and_then(|c| c.float_cvar("s_separation"))
                .unwrap_or(0.5) as f32,
        ));
        self.output.show = console.and_then(|c| c.bool_cvar("s_show")).unwrap_or(false);
        self.allow_dynamic = console
            .and_then(|c| c.bool_cvar("s_allowDynamicMusic"))
            .unwrap_or(true);
        let effects = console
            .and_then(|console| console.float_cvar("s_volume"))
            .unwrap_or(0.5) as f32;
        let music = console
            .and_then(|console| console.float_cvar("s_musicVolume"))
            .unwrap_or(0.25) as f32;
        if console.is_some_and(ViewerConsole::window_muted) {
            self.set_gains(0.0, 0.0);
        } else {
            self.set_gains(effects, music);
        }
        self.footsteps = console
            .and_then(|console| console.bool_cvar("cg_footsteps"))
            .unwrap_or(true);
        self.output.send(AudioCommand::Doppler(
            console
                .and_then(|console| console.bool_cvar("s_doppler"))
                .unwrap_or(true),
        ));
    }

    /// Keep background music fed while no live snapshot is producing loops.
    pub(crate) fn idle_frame(&mut self) {
        self.poll_legacy_load();
        self.output.send(AudioCommand::ClearLoops);
        self.output.send(AudioCommand::CommitLoops);
    }

    /// Lookup-only EFX sound playback; `vfs` remains in the signature so old
    /// callers cannot accidentally bypass map asset ownership.
    pub(crate) fn play(
        &mut self,
        _vfs: &VirtualFileSystem,
        path: &str,
        volume: f32,
        origin: [f32; 3],
        source: SourceId,
    ) {
        self.play_on_channel(path, volume, origin, source, ChannelId(0));
    }

    /// Positional playback on a legacy channel. A non-zero channel replaces
    /// the sound `source` is still playing on it, like `S_PickChannel`.
    pub(crate) fn play_on_channel(
        &mut self,
        path: &str,
        volume: f32,
        origin: [f32; 3],
        source: SourceId,
        channel: ChannelId,
    ) {
        let Some(handle) = self.find_handle(path) else {
            return;
        };
        self.output.send(AudioCommand::Play(
            handle,
            PlayRequest {
                origin: Some(origin),
                source,
                channel,
                volume,
                attenuation: sjk_client::legacy_sound_attenuation(channel.0),
            },
        ));
    }

    /// Play a preloaded model animation cue with cgame channel/spatial semantics.
    pub(crate) fn play_animation(
        &mut self,
        path: &str,
        channel: u8,
        footstep: bool,
        origin: [f32; 3],
        entity: u64,
        local: bool,
    ) {
        if footstep && !self.footsteps {
            return;
        }
        // A muted player's footsteps, swings and other animation cues.
        let source = SourceId(entity.saturating_sub(1) as u32);
        if self.mute.silences(source, None) {
            return;
        }
        let Some(handle) = self.find_handle(path) else {
            return;
        };
        let relative = local || channel == 12;
        self.output.send(AudioCommand::Play(
            handle,
            PlayRequest {
                origin: (!relative).then_some(origin),
                source,
                channel: ChannelId(u32::from(if channel == 4 || channel == 12 {
                    3
                } else {
                    channel
                })),
                volume: 1.0,
                attenuation: if relative {
                    sjk_audio::Attenuation::None
                } else {
                    sjk_client::legacy_sound_attenuation(u32::from(channel))
                },
            },
        ));
    }

    /// Play an emote's sound on player `slot` from `offset` in (a synced emote's
    /// music joining), at the very sample player `align`'s voice plays it when they
    /// lead a synced emote, `volume` times as loud as recorded, on the channel
    /// [`EMOTE_CHANNEL`] so [`Self::stop_emote`] can cut it; a muted player's stays
    /// silent. Another player's is placed at `origin` and follows them (the snapshot
    /// moves their source); the player's own (`origin` `None`: their entity is not in
    /// the snapshot to move it) plays in both ears.
    pub(crate) fn play_emote(
        &mut self,
        path: &str,
        slot: u8,
        origin: Option<[f32; 3]>,
        volume: f32,
        offset: std::time::Duration,
        align: Option<u8>,
    ) {
        let source = SourceId(u32::from(slot));
        if self.mute.silences(source, None) {
            return;
        }
        if offset.is_zero() && align.is_none() {
            match origin {
                Some(origin) => self.play_on_channel(path, volume, origin, source, EMOTE_CHANNEL),
                None => self.play_local(path, volume, source, EMOTE_CHANNEL),
            }
            return;
        }
        let Some(handle) = self.find_handle(path) else {
            return;
        };
        let samples = (offset.as_secs_f64() * f64::from(crate::audio_output::SAMPLE_RATE)) as usize;
        self.output.send(AudioCommand::PlayFrom(
            handle,
            PlayRequest {
                origin,
                source,
                channel: EMOTE_CHANNEL,
                volume,
                attenuation: match origin {
                    Some(_) => sjk_client::legacy_sound_attenuation(EMOTE_CHANNEL.0),
                    None => sjk_audio::Attenuation::None,
                },
            },
            samples,
            align.map(|slot| SourceId(u32::from(slot))),
        ));
    }

    /// Stop the emote sound player `slot` plays.
    pub(crate) fn stop_emote(&mut self, slot: u8) {
        self.output.send(AudioCommand::StopChannel(
            SourceId(u32::from(slot)),
            EMOTE_CHANNEL,
        ));
    }

    /// Play a listener-relative sound (interface cues) at full stereo.
    pub(crate) fn play_local(
        &mut self,
        path: &str,
        volume: f32,
        source: SourceId,
        channel: ChannelId,
    ) {
        let Some(handle) = self.find_handle(path) else {
            return;
        };
        self.output.send(AudioCommand::Play(
            handle,
            PlayRequest {
                origin: None,
                source,
                channel,
                volume,
                attenuation: sjk_audio::Attenuation::None,
            },
        ));
    }

    /// Per-frame tail: play the interface cues the menus posted, then
    /// report output health.
    pub(crate) fn end_frame(&mut self) {
        ui_cues::play_pending(self);
        self.report_underruns();
    }

    pub(crate) fn report_underruns(&mut self) {
        let count = self.underrun_count();
        if count != self.reported_underruns {
            crate::log::progress(format_args!("audio output underruns: {count}"));
            self.reported_underruns = count;
        }
        self.trace_level();
    }

    pub(crate) fn underrun_count(&self) -> u64 {
        self.output.stats.underruns.load(Ordering::Relaxed)
    }

    /// Every few seconds, what the mixer actually rendered: a peak of zero
    /// with music started means the mixer is silent; a peak above zero with
    /// nothing audible means the sound leaves the process and is lost after.
    fn trace_level(&mut self) {
        let Some(due) = self.trace_due else { return };
        let now = Instant::now();
        if now < due {
            return;
        }
        self.trace_due = Some(now + TRACE_INTERVAL);
        let stats = &self.output.stats;
        crate::log::progress(format_args!(
            "audio trace: peak={:.3} blocks={} music starts={} rejected={} \
             decode failures={} handle mismatches={} split loop frames={} sounds={}",
            stats.take_peak(),
            stats.rendered_blocks.load(Ordering::Relaxed),
            stats.music_starts.load(Ordering::Relaxed),
            stats.music_rejections.load(Ordering::Relaxed),
            stats.decode_failures.load(Ordering::Relaxed),
            stats.handle_mismatches.load(Ordering::Relaxed),
            stats.split_loop_frames.load(Ordering::Relaxed),
            self.handles.len(),
        ));
    }
}

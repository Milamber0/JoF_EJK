//! The per-frame hook of emotes (`emotes.rs`): the emotes the SJK hub relayed for
//! this server start on the players they name, when the game shows the name their
//! claim was made under in that slot; emotes end when they are over or their player
//! moves. Costs two uncontended locks a frame when nothing comes.

use super::*;
use sjk_runtime::EntityId;

impl GpuState {
    /// Start the emotes received, end those that are over.
    pub(crate) fn update_emotes(&mut self) {
        let received = player_identity::take_emotes();
        if let Some(vfs) = self.vfs.as_deref() {
            {
                let mut state = emotes::lock();
                if state.catalogue.is_none() {
                    state.catalogue = Some(emotes::load(vfs));
                }
            }
            crate::emote_icons::service(vfs, &self.ui_shapes, &self.queue);
        }
        let Some(session) = self.live_session.as_ref() else {
            let mut state = emotes::lock();
            match self.demo_session.as_ref() {
                // A demo keeps the emotes started on it (a local one, a world shot's)
                // until they end or their player moves; none arrive from the hub.
                Some(demo) => {
                    let world = demo.world();
                    state.active.update(Instant::now(), |slot: u8| {
                        world
                            .entity(EntityId::new(u64::from(slot) + 1))
                            .map(|entity| entity.current().transform.translation)
                    });
                }
                None => state.active = emotes::ActiveEmotes::default(),
            }
            return;
        };
        let world = &self.live_world;
        let position = |slot: u8| {
            world
                .entity(EntityId::new(u64::from(slot) + 1))
                .map(|entity| entity.current().transform.translation)
        };
        let now = Instant::now();
        let own = u8::try_from(session.latest_snapshot().player.client_num()).ok();
        let mut state = emotes::lock();
        for emote in received {
            // The hub's id is shown in the console as it came when no catalogue entry
            // names it: only an id that could name an emote may.
            if !emotes::valid_id(&emote.emote) {
                continue;
            }
            // The hub's echo of the player's own emote, already started by `sjkemote`.
            if own == Some(emote.slot)
                && state
                    .active
                    .get(emote.slot)
                    .is_some_and(|active| active.emote == emote.emote)
            {
                continue;
            }
            let shown = player_identity::shown_name(session.game_state(), usize::from(emote.slot));
            if !emotes::trusted(&emote, shown.as_deref()) {
                continue;
            }
            if emote.emote == emotes::STOP {
                state.active.stop(emote.slot);
                continue;
            }
            if emote.emote == emotes::SYNC {
                continue;
            }
            let def = state
                .catalogue
                .as_ref()
                .and_then(|catalogue| catalogue.iter().find(|def| def.id == emote.emote))
                .cloned();
            let origin = position(emote.slot).unwrap_or_default();
            // In step with the leader as this client saw them start; a leader not
            // playing it here (gone, or its own echo still on the way) starts it anew.
            let synced = emote.sync.is_some_and(|leader| {
                state
                    .active
                    .sync(emote.slot, leader, &emote.emote, def.as_ref(), origin, now)
            });
            if !synced {
                state
                    .active
                    .start(emote.slot, &emote.emote, def.as_ref(), origin, now);
            }
            // Until the animations exist, the console says it.
            if let Some(console) = &mut self.console {
                let name = def
                    .as_ref()
                    .map_or(emote.emote.as_str(), |def| def.name.as_str());
                console.push_log(format!("* {}^7: {name}", shown.unwrap_or_default()));
            }
        }
        state.active.update(now, position);
    }

    /// Play and stop the emotes' sounds (`emotes::Cue`) on their players; the
    /// catalogue's sounds are loaded the first time it is seen.
    pub(crate) fn play_emote_sounds(&mut self, audio: &mut Option<crate::GameAudio>) {
        let mut state = emotes::lock();
        let cues = state.active.take_cues();
        let Some(audio) = audio.as_mut() else {
            return;
        };
        if !state.sounds_loaded
            && let (Some(catalogue), Some(vfs)) = (state.catalogue.as_ref(), self.vfs.as_deref())
        {
            audio.register_effect_paths(
                vfs,
                catalogue
                    .iter()
                    .flat_map(|def| [def.sound.as_str(), def.sound_loop.as_str()])
                    .filter(|sound| !sound.is_empty()),
            );
            state.sounds_loaded = true;
        }
        drop(state);
        let world = self
            .demo_session
            .as_ref()
            .map_or(&self.live_world, crate::demo_playback::Session::world);
        let own = match (&self.live_session, &self.demo_session) {
            (Some(session), _) => Some(session.latest_snapshot().player.client_num()),
            (None, Some(demo)) => Some(demo.latest_snapshot().player.client_num()),
            (None, None) => None,
        }
        .and_then(|own| u8::try_from(own).ok());
        let volume = self
            .console
            .as_ref()
            .and_then(|console| console.float_cvar(emotes::VOLUME_CVAR))
            .unwrap_or(emotes::DEFAULT_VOLUME)
            .clamp(0.0, 4.0) as f32;
        for cue in cues {
            match cue {
                emotes::Cue::Start {
                    slot,
                    sound,
                    offset,
                    align,
                } => {
                    let origin = (Some(slot) != own).then(|| {
                        world
                            .entity(EntityId::new(u64::from(slot) + 1))
                            .map(|entity| entity.current().transform.translation)
                            .unwrap_or_default()
                    });
                    audio.play_emote(&sound, slot, origin, volume, offset, align);
                }
                emotes::Cue::Stop { slot } => audio.stop_emote(slot),
            }
        }
    }
}

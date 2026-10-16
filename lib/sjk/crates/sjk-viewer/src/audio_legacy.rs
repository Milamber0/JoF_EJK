//! Asynchronous construction of legacy gamestate sound tables.

use super::*;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct PreparedLegacyAudio {
    pub(super) adapter: LegacySoundAdapter,
    /// The generation of the blade skins whose sound sets the adapter holds.
    skins_generation: u64,
    pub(super) sounds: Vec<(String, SoundHandle, Box<[u8]>)>,
    pub(super) elapsed: Duration,
    cue_handles: [Option<SoundHandle>; 9],
}

pub(super) struct LegacyLoadTask {
    receiver: Receiver<Result<PreparedLegacyAudio, String>>,
}

/// `cgs.inlineModelMidpoints` (`CG_RegisterGraphics`): the middle of each brush
/// model's bounds by model number; the world, model 0, has none.
fn inline_model_midpoints(bsp: &Bsp) -> Box<[[f32; 3]]> {
    let render = bsp.render();
    (0..render.models().len())
        .map(|index| {
            render.inline_model(index).map_or([0.0; 3], |model| {
                std::array::from_fn(|axis| (model.minimums[axis] + model.maximums[axis]) * 0.5)
            })
        })
        .collect()
}

impl LegacyLoadTask {
    pub(super) fn start(
        vfs: Arc<VirtualFileSystem>,
        game_state: GameState,
        skins: Arc<crate::saber_skins::LoadedSkins>,
        inline_model_midpoints: Box<[[f32; 3]]>,
    ) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("sjk-legacy-audio-load".into())
            .spawn(move || {
                let started = Instant::now();
                let result = (|| {
                    let mut sounds = Vec::with_capacity(512);
                    let mut next_handle = 0;
                    let mut register = |path: &str, bytes: &[u8]| {
                        let handle = SoundHandle(next_handle);
                        next_handle = next_handle.wrapping_add(1);
                        sounds.push((path.to_owned(), handle, bytes.into()));
                        Some(handle)
                    };
                    let mut adapter = LegacySoundAdapter::new(&game_state, &vfs, &mut register);
                    adapter.set_inline_model_midpoints(inline_model_midpoints);
                    // The skins' sounds are in their packs, below the game data.
                    let saber_vfs = vfs.with_lower(skins.packs()).map_err(|e| e.to_string())?;
                    adapter.register_saber_sound_sets(
                        &skins.sound_sets(),
                        &saber_vfs,
                        &mut register,
                    );
                    for path in feedback::KILL_SOUNDS {
                        if let Ok(Some(asset)) = vfs.read(path) {
                            let handle = SoundHandle(next_handle);
                            next_handle = next_handle.wrapping_add(1);
                            sounds.push((path.to_owned(), handle, asset.bytes.into()));
                        } else {
                            crate::log::progress(format_args!("optional kill cue missing: {path}"));
                        }
                    }
                    let cue_handles = transitions::CUES.map(|requested| {
                        let Some((path, asset)) = assets::resolve_sound(&vfs, requested) else {
                            crate::log::progress(format_args!(
                                "optional feedback missing: {requested}"
                            ));
                            return None;
                        };
                        let handle = SoundHandle(next_handle);
                        next_handle = next_handle.wrapping_add(1);
                        sounds.push((path, handle, asset.bytes.into()));
                        Some(handle)
                    });
                    Ok(PreparedLegacyAudio {
                        adapter,
                        skins_generation: skins.generation(),
                        sounds,
                        cue_handles,
                        elapsed: started.elapsed(),
                    })
                })();
                let _ = sender.send(result);
            })
            .expect("legacy audio loader thread creation failed");
        Self { receiver }
    }

    pub(super) fn poll(&self) -> Option<Result<PreparedLegacyAudio, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("legacy audio loader stopped unexpectedly".into()))
            }
        }
    }
}

impl GameAudio {
    /// Stop old-map audio before new snapshots can reach the old configstring resolver.
    pub(crate) fn begin_map_change(&mut self) {
        self.transitions.reset();
        self.output.send(AudioCommand::StopEffects);
        self.output.stop_music();
        self.legacy = None;
        self.saber_sets_generation = None;
        self.legacy_load = None;
        self.sound_table_refresh = None;
        self.deferred_snapshots.clear();
        self.map_music = None;
        self.music_value = None;
        self.dynamic = None;
    }

    /// Rebuild legacy tables after a mid-session gamestate/map change. `bsp` is
    /// the gamestate's map, whose brush models place door and mover sounds.
    pub(crate) fn install_gamestate(
        &mut self,
        game_state: &GameState,
        vfs: Arc<VirtualFileSystem>,
        bsp: &Bsp,
    ) {
        self.begin_map_change();
        self.sound_table_refresh = Some(SoundTableRefresh::new(game_state));
        self.legacy_vfs = Some(Arc::clone(&vfs));
        self.deferred_snapshots.clear();
        self.legacy_load = Some(LegacyLoadTask::start(
            vfs,
            game_state.clone(),
            Arc::clone(&self.blade_skins),
            inline_model_midpoints(bsp),
        ));
    }

    /// Integrate worker handles into the current bank before replaying queued events.
    pub(super) fn poll_legacy_load(&mut self) {
        let Some(result) = self.legacy_load.as_ref().and_then(LegacyLoadTask::poll) else {
            return;
        };
        self.legacy_load = None;
        match result {
            Ok(mut prepared) => {
                let elapsed = prepared.elapsed;
                let mut remap = Vec::with_capacity(prepared.sounds.len());
                for (path, _, bytes) in prepared.sounds {
                    let key = path.to_ascii_lowercase();
                    let extension = path.rsplit('.').next().unwrap_or("wav");
                    let handle = if let Some(handle) = self.handles.get(&key) {
                        *handle
                    } else {
                        let handle = SoundHandle(self.next_handle);
                        self.next_handle = self.next_handle.wrapping_add(1);
                        self.output.decode(handle, &bytes, extension);
                        self.handles.insert(key, handle);
                        handle
                    };
                    remap.push(handle);
                }
                prepared
                    .adapter
                    .remap_sound_handles(|handle| remap[handle.0 as usize]);
                self.transitions.handles =
                    prepared.cue_handles.map(|h| h.map(|h| remap[h.0 as usize]));
                self.legacy = Some(prepared.adapter);
                self.saber_sets_generation = Some(prepared.skins_generation);
                // Skins loaded while the tables were built: their sounds now.
                self.register_blade_skin_sounds();
                crate::log::progress(format_args!(
                    "legacy audio tables ready in {:.1}ms",
                    elapsed.as_secs_f64() * 1_000.0
                ));
                self.play_deferred_snapshots();
                self.apply_pending_sound_tables();
            }
            Err(error) => {
                crate::log::progress(format_args!("legacy audio load failed: {error}"));
            }
        }
    }
}

impl GameAudio {
    /// Take the blade skins the renderer took (`saber_skins.rs`): when they are new,
    /// their sounds are decoded afresh and registered at once with the gamestate's tables
    /// (a pack that came mid-session); otherwise nothing (two compares a frame).
    pub(crate) fn follow_blade_skins(&mut self, skins: &Arc<crate::saber_skins::LoadedSkins>) {
        if self.blade_skins.generation() != skins.generation() {
            self.blade_skins = Arc::clone(skins);
            // A new pack may change a sound at the same path: decode them again.
            for set in self.blade_skins.sound_sets() {
                for path in [set.on, set.off, set.hum].into_iter().chain(set.swings) {
                    self.handles.remove(&path.to_ascii_lowercase());
                }
            }
        }
        if self.legacy.is_some() && self.saber_sets_generation != Some(skins.generation()) {
            self.register_blade_skin_sounds();
        }
    }

    /// Register the current blade skins' sound sets with the gamestate's tables, decoding
    /// any sound not decoded yet; reads the packs below the game data.
    fn register_blade_skin_sounds(&mut self) {
        let skins = Arc::clone(&self.blade_skins);
        if self.saber_sets_generation == Some(skins.generation()) {
            return;
        }
        let (Some(adapter), Some(vfs)) = (self.legacy.as_mut(), self.legacy_vfs.as_ref()) else {
            return;
        };
        let vfs = match vfs.with_lower(skins.packs()) {
            Ok(vfs) => vfs,
            Err(error) => {
                crate::log::progress(format_args!("blade skin sounds: {error}"));
                return;
            }
        };
        let (handles, next_handle, output) =
            (&mut self.handles, &mut self.next_handle, &self.output);
        adapter.register_saber_sound_sets(&skins.sound_sets(), &vfs, |path, bytes| {
            let key = path.to_ascii_lowercase();
            if let Some(handle) = handles.get(&key) {
                return Some(*handle);
            }
            let handle = SoundHandle(*next_handle);
            *next_handle = next_handle.wrapping_add(1);
            output.decode(handle, bytes, path.rsplit('.').next().unwrap_or("wav"));
            handles.insert(key, handle);
            Some(handle)
        });
        self.saber_sets_generation = Some(skins.generation());
    }
}

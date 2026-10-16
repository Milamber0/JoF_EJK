//! VFS-backed sound registration and map music selection.

use super::*;
use crate::{GpuState, demo_playback};
use sjk_entity::parse_entity_lump;
use sjk_vfs::Asset;

/// Parsed `CS_MUSIC`/worldspawn music pair consumed by `CG_StartMusic`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MusicSpec {
    pub(crate) intro: String,
    pub(crate) repeating: String,
}

/// Read the full `music "intro loop"` value without discarding its loop token.
pub(crate) fn world_music(bsp: &Bsp) -> Option<MusicSpec> {
    let value = parse_entity_lump(bsp.entities())
        .ok()?
        .into_iter()
        .find(|entity| entity.classname() == Some("worldspawn"))?
        .get("music")?
        .to_owned();
    parse_music(&value)
}

/// Read the server-authoritative `CS_MUSIC` value (`bg_public.h:92`).
pub(crate) fn game_music(game_state: &GameState) -> Option<MusicSpec> {
    parse_music(std::str::from_utf8(game_state.config_string(2)?).ok()?)
}

pub(super) fn parse_music(value: &str) -> Option<MusicSpec> {
    let mut tokens = value.split_ascii_whitespace();
    let intro = tokens.next()?.to_owned();
    if intro.is_empty() {
        return None;
    }
    let repeating = tokens.next().unwrap_or(&intro).to_owned();
    Some(MusicSpec { intro, repeating })
}

pub(super) fn default_music_extension(path: &str) -> String {
    if path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.contains('.'))
    {
        path.to_owned()
    } else {
        format!("{path}.mp3")
    }
}

/// Look `path` up in the VFS, falling back between `.wav` and `.mp3`; returns
/// the path that answered with its bytes.
pub(super) fn resolve_sound(vfs: &VirtualFileSystem, path: &str) -> Option<(String, Asset)> {
    if let Some(asset) = vfs.read(path).ok().flatten() {
        return Some((path.to_owned(), asset));
    }
    let candidates = if let Some(base) = path.strip_suffix(".wav") {
        [format!("{base}.mp3"), String::new()]
    } else if let Some(base) = path.strip_suffix(".mp3") {
        [format!("{base}.wav"), String::new()]
    } else {
        [format!("{path}.wav"), format!("{path}.mp3")]
    };
    candidates
        .into_iter()
        .filter(|candidate| !candidate.is_empty())
        .find_map(|candidate| {
            let asset = vfs.read(&candidate).ok().flatten()?;
            Some((candidate, asset))
        })
}

/// Sound files read ahead on the world-install thread so the cut into a
/// joined map does not pay their pk3 reads on the render thread: the map
/// music alone stalled the frame that swaps worlds by ~90 ms.
#[derive(Default)]
pub(crate) struct SoundPrefetch {
    /// `(requested path, resolved path, bytes)` in lower case.
    assets: Vec<(String, String, Asset)>,
}

impl SoundPrefetch {
    /// Read authored animation samples on the appearance loader, never the frame path.
    pub(crate) fn paths<'a>(vfs: &VirtualFileSystem, paths: impl Iterator<Item = &'a str>) -> Self {
        let mut seen = std::collections::HashSet::new();
        Self {
            assets: paths
                .filter(|path| seen.insert(*path))
                .filter_map(|requested| {
                    let (resolved, asset) = resolve_sound(vfs, requested)?;
                    Some((requested.to_owned(), resolved, asset))
                })
                .collect(),
        }
    }

    /// Read the map's music (server `CS_MUSIC`, else worldspawn) and the duel
    /// music that `configure_audio` will ask for.
    pub(crate) fn read(vfs: &VirtualFileSystem, game_state: Option<&GameState>, bsp: &Bsp) -> Self {
        let music = game_state.and_then(game_music).or_else(|| world_music(bsp));
        let mut paths = vec![DUEL_MUSIC.to_owned()];
        paths.extend(
            sjk_client::animation_events::footsteps::PATHS
                .iter()
                .flatten()
                .flatten()
                .map(|path| (*path).to_owned()),
        );
        if let Ok(definitions) = sjk_client::legacy_saber_definitions(vfs) {
            for definition in definitions.into_values() {
                paths.extend(definition.sound_spin);
                paths.extend(definition.sound_swing.into_iter().flatten());
            }
        }
        paths.extend(crate::effect_aux::saber_contacts::SOUNDS.map(str::to_owned));
        paths.extend(crate::jetpack::SOUNDS.map(str::to_owned));
        paths.push(crate::electrify::CRACKLE.to_owned());
        paths.extend(
            [
                crate::hud::rocket_lock::TICK_SOUND,
                crate::hud::rocket_lock::LOCK_SOUND,
            ]
            .map(str::to_owned),
        );
        if let Some(music) = music {
            paths.push(default_music_extension(&music.intro));
            paths.push(default_music_extension(&music.repeating));
        }
        let assets = paths
            .iter()
            .map(|path| path.to_ascii_lowercase())
            .filter_map(|requested| {
                let (resolved, asset) = resolve_sound(vfs, &requested)?;
                Some((requested, resolved, asset))
            })
            .collect();
        Self { assets }
    }
}

const DUEL_MUSIC: &str = "music/mp/duel.mp3";

impl GameAudio {
    /// Queue decoding once when an appearance first joins the active audio output.
    pub(crate) fn absorb_animation_prefetch(&mut self, prefetch: &SoundPrefetch) {
        for (requested, resolved, asset) in &prefetch.assets {
            if !self.handles.contains_key(requested) {
                self.register_resolved(requested.clone(), resolved.clone(), asset);
            }
        }
    }

    /// Register sounds read ahead of time; later requests for the same paths
    /// hit the handle cache instead of the VFS.
    pub(crate) fn absorb_prefetch(&mut self, prefetch: SoundPrefetch) {
        for (requested, resolved, asset) in prefetch.assets {
            self.register_resolved(requested, resolved, &asset);
        }
    }

    pub(crate) fn register_effect_paths<'a>(
        &mut self,
        vfs: &VirtualFileSystem,
        paths: impl IntoIterator<Item = &'a str>,
    ) {
        for path in paths {
            let _ = self.register_vfs_async(vfs, path);
        }
    }

    pub(crate) fn register_duel_music(&mut self, vfs: &VirtualFileSystem) {
        self.duel_music = self.register_vfs_async(vfs, DUEL_MUSIC);
    }

    pub(crate) fn start_music(&mut self, vfs: &VirtualFileSystem, music: &MusicSpec) {
        self.output.stop_music();
        self.map_music = None;
        self.dynamic = None;
        let path = default_music_extension(&music.intro);
        if !path.contains('/') || resolve_sound(vfs, &path).is_none() {
            if let Ok(Some(asset)) = vfs.read("ext_data/dms.dat") {
                let result = std::str::from_utf8(&asset.bytes)
                    .map_err(|e| e.to_string())
                    .and_then(dynamic::Catalogue::parse)
                    .and_then(|catalogue| catalogue.level(&music.intro));
                match result {
                    Ok(level) => {
                        for track in level.tracks[..2].iter().flatten() {
                            if resolve_sound(vfs, track).is_none() {
                                crate::log::progress(format_args!(
                                    "Missing dynamic track: {track}"
                                ));
                                return;
                            }
                        }
                        let path = level.tracks[0].as_ref().unwrap().clone();
                        self.start_file_music(
                            vfs,
                            &MusicSpec {
                                intro: path.clone(),
                                repeating: path,
                            },
                        );
                        if self.allow_dynamic {
                            self.dynamic = Some(level);
                        }
                        return;
                    }
                    Err(error) => crate::log::progress(format_args!("Dynamic music: {error}")),
                }
            }
        }
        self.start_file_music(vfs, music);
    }

    pub(super) fn start_file_music(&mut self, vfs: &VirtualFileSystem, music: &MusicSpec) {
        let intro_path = default_music_extension(&music.intro);
        let loop_path = default_music_extension(&music.repeating);
        let intro = self.register_vfs_async(vfs, &intro_path);
        let repeating = self.register_vfs_async(vfs, &loop_path).or(intro);
        if let (Some(intro), Some(repeating)) = (intro, repeating) {
            self.output.start_music(intro, repeating);
            self.map_music = Some((intro, repeating));
        }
    }

    pub(super) fn register_vfs_async(
        &mut self,
        vfs: &VirtualFileSystem,
        path: &str,
    ) -> Option<SoundHandle> {
        let requested = path.to_ascii_lowercase();
        if let Some(handle) = self.handles.get(&requested) {
            return Some(*handle);
        }
        let (resolved, asset) = resolve_sound(vfs, &requested)?;
        Some(self.register_resolved(requested, resolved, &asset))
    }

    /// Decode `asset` once under its resolved path; the requested path and
    /// the extension-less name become aliases of the same handle.
    fn register_resolved(
        &mut self,
        requested: String,
        resolved: String,
        asset: &Asset,
    ) -> SoundHandle {
        let handle = match self.handles.get(&resolved) {
            Some(handle) => *handle,
            None => {
                let extension = resolved.rsplit('.').next().unwrap_or("wav");
                let handle = SoundHandle(self.next_handle);
                self.next_handle = self.next_handle.wrapping_add(1);
                self.output.decode(handle, &asset.bytes, extension);
                self.handles.insert(resolved.clone(), handle);
                if let Some(without) = resolved
                    .strip_suffix(".wav")
                    .or_else(|| resolved.strip_suffix(".mp3"))
                {
                    self.handles.entry(without.to_owned()).or_insert(handle);
                }
                handle
            }
        };
        self.handles.entry(requested).or_insert(handle);
        handle
    }

    pub(super) fn find_handle(&self, path: &str) -> Option<SoundHandle> {
        if path.bytes().any(|byte| byte.is_ascii_uppercase()) {
            self.handles.get(&path.to_ascii_lowercase()).copied()
        } else {
            self.handles.get(path).copied()
        }
    }
}

impl GpuState {
    pub(crate) fn configure_audio(&mut self, game_audio: &mut Option<GameAudio>) {
        let started = Instant::now();
        let Some(audio) = game_audio else {
            return;
        };
        let Some(vfs) = self.vfs.as_ref() else {
            return;
        };
        audio.sync_gains(self.console.as_ref());
        audio.absorb_prefetch(std::mem::take(&mut self.sound_prefetch));
        let game_state = self
            .live_session
            .as_ref()
            .map(sjk_client::ClientSession::game_state)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::game_state)
            });
        if let Some(game_state) = game_state {
            audio.install_gamestate(game_state, Arc::clone(vfs), &self.bsp);
        }
        audio.music_value =
            game_state.map(|game| game.config_string(2).unwrap_or_default().to_vec());
        let music = game_state
            .and_then(game_music)
            .or_else(|| world_music(&self.bsp));
        if let Some(music) = music.as_ref() {
            audio.start_music(vfs, music);
        }
        let music_done = Instant::now();
        audio.register_duel_music(vfs);
        audio.register_effect_paths(vfs, ["sound/weapons/disruptor/zoomloop.wav"]);
        let mut effect_sound_paths = Vec::new();
        self.effects.append_sound_paths(&mut effect_sound_paths);
        audio.register_effect_paths(vfs, effect_sound_paths.iter().map(String::as_str));
        audio.register_effect_paths(
            vfs,
            crate::audio::ui_cues::CUE_SOUNDS.iter().map(|cue| cue.1),
        );
        audio.register_sjk_chat_sound(vfs);
        let effects_done = Instant::now();
        crate::log::progress(format_args!(
            "sound profile: music-read={:.1}ms effect-reads={:.1}ms table-dispatch={:.1}ms",
            music_done.duration_since(started).as_secs_f64() * 1_000.0,
            effects_done.duration_since(music_done).as_secs_f64() * 1_000.0,
            Instant::now().duration_since(effects_done).as_secs_f64() * 1_000.0,
        ));
    }
}

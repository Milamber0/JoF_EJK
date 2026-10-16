//! Codemp ambient-set playback: the world's global set and entity-local sets.
//!
//! `CG_AS_Register` (`codemp/cgame/cg_main.c:493-529`) precaches the names in
//! `CS_AMBIENT_SET+1..` (until the first empty slot) plus a non-`default`
//! `CS_GLOBAL_AMBIENT_SET`; `AS_ParseSets` instantiates only those names.
//! Every rendered frame `CG_DrawActiveFrame` adds one local set per non-mover
//! entity carrying `soundSetIndex` (`cg_ents.c:3326-3334`, `S_AddLocalSet`)
//! and finally the global set at the view origin (`cg_view.c:2694-2698`,
//! `S_UpdateAmbientSet`). Crossfade, volume and one-shot scheduling follow
//! `codemp/client/snd_ambient.cpp:907-1141`.
//!
//! Deviation, documented in `KNOWN_ISSUES.md`: codemp schedules the global
//! set from wall-clock `cls.realtime`; this adapter uses the presented server
//! time so replay ledgers stay deterministic, and it seeds Raven's `irand`
//! generator with a fixed value instead of the process clock.

use crate::ambient_sets::{AmbientSet, AmbientSets};
use crate::loop_sounds::{LegacyLoopDecision, LegacyLoopKind};
use crate::sound_events::{RegisteredLegacySound, normal_attenuation};
use sjk_audio::{Attenuation, ChannelId, PlayRequest, SoundHandle, SourceId};
use sjk_protocol::GameState;

/// `codemp/game/bg_public.h:121,125`.
pub const CS_GLOBAL_AMBIENT_SET: usize = 32;
const CS_AMBIENT_SET: usize = 37;
const MAX_AMBIENT_SETS: usize = 256; // codemp/game/bg_public.h:123
const MAX_SET_VOLUME: i32 = 255; // snd_ambient.cpp:29
const CROSS_DELAY_MILLIS: i32 = 1_000; // snd_ambient.cpp:37
const MAX_WAVES_PER_GROUP: usize = 8; // snd_ambient.h:32
const CHAN_AMBIENT: u32 = 7; // codemp/qcommon/q_shared.h:862
const ENTITYNUM_WORLD: u16 = 1_022; // MAX_GENTITIES - 2
const MAX_SHOTS_PER_FRAME: usize = 32;

/// One ambient one-shot sub-wave started this frame (`S_StartAmbientSound`).
#[derive(Clone, Copy, Debug)]
pub struct LegacyAmbientShot {
    /// Global-set or local-set origin of the request.
    pub kind: LegacyLoopKind,
    /// Adapter-local registered-sound index.
    pub sound: Option<u16>,
    /// Engine sound-bank handle when the asset decoded successfully.
    pub handle: Option<SoundHandle>,
    /// Fixed-origin `CHAN_AMBIENT` request scaled by the set's volume byte.
    pub request: PlayRequest,
}

#[derive(Clone, Copy, Debug)]
struct RuntimeSet {
    time_between_waves: [i32; 2],
    volume_range: [i32; 2],
    radius: i32,
    looped: Option<u16>,
    sub_waves: [Option<u16>; MAX_WAVES_PER_GROUP],
    sub_wave_count: u8,
    /// `ambientSet_t::masterVolume`, shared by global crossfade and local use.
    master_volume: i32,
    fade_time: i32,
}

impl RuntimeSet {
    fn new(set: &AmbientSet, intern: &mut impl FnMut(&str) -> u16) -> Self {
        let mut sub_waves = [None; MAX_WAVES_PER_GROUP];
        let mut sub_wave_count = 0;
        for (slot, path) in sub_waves.iter_mut().zip(&set.sub_waves) {
            *slot = Some(intern(path));
            sub_wave_count += 1;
        }
        Self {
            time_between_waves: set.time_between_waves,
            volume_range: set.volume_range,
            radius: set.radius,
            looped: set.looped_wave.as_deref().map(&mut *intern),
            sub_waves,
            sub_wave_count,
            master_volume: MAX_SET_VOLUME,
            fade_time: 0,
        }
    }
}

/// Raven's `irand` (`shared/qcommon/q_math.c:243-260`): a 32-bit LCG whose
/// upper 15 bits select an inclusive integer range.
#[derive(Clone, Copy, Debug)]
pub struct LegacyRandom(u32);

impl LegacyRandom {
    /// Fixed seed so replayed ledgers are bit-identical run to run.
    pub const fn seeded(seed: u32) -> Self {
        Self(seed)
    }

    /// Inclusive `Q_irand(min, max)`.
    pub fn irand(&mut self, min: i32, max: i32) -> i32 {
        self.0 = self.0.wrapping_mul(214_013).wrapping_add(2_531_011);
        let result = (self.0 >> 17) as i32;
        ((result * (max + 1 - min)) >> 15) + min
    }
}

/// Per-map ambient-set state owned by the loop adapter.
pub(crate) struct LegacyAmbientWorld {
    sets: Vec<RuntimeSet>,
    /// Lower-cased precached names parallel to `sets`.
    names: Vec<String>,
    /// `CS_AMBIENT_SET + index` → precached set, for entity local sets.
    cs_sets: Box<[Option<usize>]>,
    global_name: Vec<u8>,
    global_set: Option<usize>,
    /// `currentSet` / `oldSet` crossfade pair and their one-shot timers.
    current: Option<usize>,
    old: Option<usize>,
    current_shot_time: i32,
    old_shot_time: i32,
    shots: Vec<LegacyAmbientShot>,
    random: LegacyRandom,
    global_frames: usize,
    local_frames: usize,
}

impl LegacyAmbientWorld {
    pub(crate) fn new(
        game_state: &GameState,
        catalog: &AmbientSets,
        intern: &mut impl FnMut(&str) -> u16,
    ) -> Self {
        let mut sets = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut cs_sets = vec![None; MAX_AMBIENT_SETS].into_boxed_slice();
        // `AS_AddPrecacheEntry` + `AS_ParseSet`: one runtime set per distinct
        // precached name that the catalog declares.
        let mut precache = |name: &[u8]| -> Option<usize> {
            let key = std::str::from_utf8(name).ok()?.to_ascii_lowercase();
            if let Some(index) = names.iter().position(|known| *known == key) {
                return Some(index);
            }
            let set = catalog.get(&key)?;
            sets.push(RuntimeSet::new(set, intern));
            names.push(key);
            Some(sets.len() - 1)
        };
        for index in 1..MAX_AMBIENT_SETS {
            let Some(name) = game_state.config_string(CS_AMBIENT_SET + index) else {
                break;
            };
            if name.is_empty() {
                break;
            }
            cs_sets[index] = precache(name);
        }
        let global_name = game_state
            .config_string(CS_GLOBAL_AMBIENT_SET)
            .unwrap_or_default()
            .to_vec();
        let global_set = if global_name.eq_ignore_ascii_case(b"default") {
            None
        } else {
            precache(&global_name)
        };
        Self {
            sets,
            names,
            cs_sets,
            global_name,
            global_set,
            current: None,
            old: None,
            current_shot_time: 0,
            old_shot_time: 0,
            shots: Vec::with_capacity(MAX_SHOTS_PER_FRAME),
            random: LegacyRandom::seeded(0x4a4b_5241),
            global_frames: 0,
            local_frames: 0,
        }
    }

    /// Follow a runtime `CS_GLOBAL_AMBIENT_SET` change (`g_trigger.c:61`).
    /// Registration on dirty indices adds names before snapshot observation;
    /// this lookup retains the existing crossfade state until the next frame.
    pub(crate) fn select_global(&mut self, name: Option<&[u8]>) {
        let name = name.unwrap_or_default();
        if name == self.global_name.as_slice() {
            return;
        }
        self.global_name.clear();
        self.global_name.extend_from_slice(name);
        self.global_set = self
            .names
            .iter()
            .position(|known| known.as_bytes().eq_ignore_ascii_case(name));
    }

    pub(crate) fn begin_frame(&mut self) {
        self.shots.clear();
        self.global_frames = 0;
        self.local_frames = 0;
    }

    pub(crate) fn shots(&self) -> &[LegacyAmbientShot] {
        &self.shots
    }

    pub(crate) const fn global_frames(&self) -> usize {
        self.global_frames
    }

    pub(crate) const fn local_frames(&self) -> usize {
        self.local_frames
    }

    /// `S_AddLocalSet` for one presented entity: the looped wave attenuated by
    /// the set radius on top of normal spatialization, plus a one-shot whose
    /// timer codemp resets to `cg.time` every call (`cg_ents.c:3332`).
    pub(crate) fn local_set(
        &mut self,
        sound_set_index: u8,
        entity: u16,
        origin: [f32; 3],
        listener: [f32; 3],
        server_time: i32,
        presented_time: i32,
        sounds: &[RegisteredLegacySound],
    ) -> Option<LegacyLoopDecision> {
        let index = self.cs_sets[usize::from(sound_set_index)]?;
        self.local_frames += 1;
        let set = self.sets[index];
        let distance = length(sub(origin, listener));
        let half_radius = set.radius as f32 * 0.5;
        let scale = if distance < half_radius {
            1.0
        } else {
            (set.radius as f32 - distance) / half_radius
        };
        let volume = if !(0.0..=1.0).contains(&scale) {
            0
        } else {
            (set.master_volume as f32 * scale) as u8
        };
        let looped = set.looped.map(|sound| LegacyLoopDecision {
            kind: LegacyLoopKind::AmbientLocal,
            sound: Some(sound),
            handle: handle_of(sounds, sound),
            configured_index: None,
            soundset_index: Some(sound_set_index),
            request: PlayRequest {
                origin: Some(origin),
                source: SourceId(u32::from(entity)),
                channel: ChannelId(0),
                volume: f32::from(volume) / MAX_SET_VOLUME as f32,
                attenuation: normal_attenuation(0),
            },
            velocity: [0.0; 3],
            doppler_scale: 1.0,
        });
        let wait = self
            .random
            .irand(set.time_between_waves[0], set.time_between_waves[1]);
        if server_time - presented_time < wait * 1_000 {
            return looped;
        }
        let volume_scale = f32::from(volume) / MAX_SET_VOLUME as f32;
        let volume = self.random.irand(
            (volume_scale * set.volume_range[0] as f32) as i32,
            (volume_scale * set.volume_range[1] as f32) as i32,
        ) as u8;
        self.push_shot(
            index,
            LegacyLoopKind::AmbientLocal,
            entity,
            origin,
            volume,
            sounds,
        );
        looped
    }

    /// `S_UpdateAmbientSet` at the view origin: crossfade bookkeeping, then the
    /// current and fading-out sets each emit their loop and timed one-shot.
    pub(crate) fn update_global(
        &mut self,
        listener: [f32; 3],
        time: i32,
        sounds: &[RegisteredLegacySound],
        mut emit: impl FnMut(LegacyLoopDecision),
    ) {
        let Some(id) = self.global_set else { return };
        self.global_frames += 1;
        if self.current != Some(id) {
            self.old = self.current;
            self.current = Some(id);
            if let Some(old) = self.old {
                self.sets[old].master_volume = MAX_SET_VOLUME;
                self.sets[old].fade_time = time;
            }
            self.sets[id].master_volume = 0;
            self.sets[id].fade_time = time;
        }
        self.update_set_volumes(time);
        let mut shot_time = self.current_shot_time;
        self.play_global(id, listener, time, &mut shot_time, sounds, &mut emit);
        self.current_shot_time = shot_time;
        if let Some(old) = self.old {
            let mut shot_time = self.old_shot_time;
            self.play_global(old, listener, time, &mut shot_time, sounds, &mut emit);
            self.old_shot_time = shot_time;
        }
    }

    /// `AS_UpdateSetVolumes` (`snd_ambient.cpp:907-952`).
    fn update_set_volumes(&mut self, time: i32) {
        let Some(current) = self.current else { return };
        let set = &mut self.sets[current];
        if set.master_volume < MAX_SET_VOLUME {
            let scale = (time - set.fade_time) as f32 / CROSS_DELAY_MILLIS as f32;
            set.master_volume = (scale * MAX_SET_VOLUME as f32) as i32;
        }
        set.master_volume = set.master_volume.min(MAX_SET_VOLUME);
        let Some(old) = self.old else { return };
        let set = &mut self.sets[old];
        if set.master_volume > 0 {
            let scale = (time - set.fade_time) as f32 / CROSS_DELAY_MILLIS as f32;
            set.master_volume = MAX_SET_VOLUME - (scale * MAX_SET_VOLUME as f32) as i32;
        }
        if set.master_volume <= 0 {
            set.master_volume = 0;
            self.old = None;
        }
    }

    /// `AS_PlayAmbientSet` (`snd_ambient.cpp:1050-1083`).
    fn play_global(
        &mut self,
        index: usize,
        listener: [f32; 3],
        time: i32,
        last_time: &mut i32,
        sounds: &[RegisteredLegacySound],
        emit: &mut impl FnMut(LegacyLoopDecision),
    ) {
        let set = self.sets[index];
        let master = set.master_volume;
        if let Some(sound) = set.looped {
            emit(LegacyLoopDecision {
                kind: LegacyLoopKind::AmbientGlobal,
                sound: Some(sound),
                handle: handle_of(sounds, sound),
                configured_index: None,
                soundset_index: None,
                request: PlayRequest {
                    origin: None,
                    source: SourceId(u32::from(ENTITYNUM_WORLD)),
                    channel: ChannelId(0),
                    volume: master as f32 / MAX_SET_VOLUME as f32,
                    attenuation: Attenuation::None,
                },
                velocity: [0.0; 3],
                doppler_scale: 1.0,
            });
        }
        let wait = self
            .random
            .irand(set.time_between_waves[0], set.time_between_waves[1]);
        if time - *last_time < wait * 1_000 {
            return;
        }
        *last_time = time;
        let volume_scale = master as f32 / MAX_SET_VOLUME as f32;
        let volume = self.random.irand(
            (volume_scale * set.volume_range[0] as f32) as i32,
            (volume_scale * set.volume_range[1] as f32) as i32,
        ) as u8;
        let volume = volume.min(master.clamp(0, 255) as u8);
        self.push_shot(
            index,
            LegacyLoopKind::AmbientGlobal,
            0,
            listener,
            volume,
            sounds,
        );
    }

    /// `S_StartAmbientSound` for a random sub-wave (`snd_ambient.cpp:1080`).
    fn push_shot(
        &mut self,
        index: usize,
        kind: LegacyLoopKind,
        entity: u16,
        origin: [f32; 3],
        volume: u8,
        sounds: &[RegisteredLegacySound],
    ) {
        let set = self.sets[index];
        if set.sub_wave_count == 0 || self.shots.len() >= MAX_SHOTS_PER_FRAME {
            return;
        }
        let pick = self.random.irand(0, i32::from(set.sub_wave_count) - 1);
        let sound = set.sub_waves[pick as usize];
        self.shots.push(LegacyAmbientShot {
            kind,
            sound,
            handle: sound.and_then(|sound| handle_of(sounds, sound)),
            request: PlayRequest {
                origin: Some(origin),
                source: SourceId(u32::from(entity)),
                channel: ChannelId(CHAN_AMBIENT),
                volume: f32::from(volume) / MAX_SET_VOLUME as f32,
                attenuation: normal_attenuation(CHAN_AMBIENT),
            },
        });
    }
}

fn handle_of(sounds: &[RegisteredLegacySound], index: u16) -> Option<SoundHandle> {
    sounds
        .get(usize::from(index))
        .and_then(|sound| sound.handle)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[path = "ambient_config_strings.rs"]
mod config_strings;

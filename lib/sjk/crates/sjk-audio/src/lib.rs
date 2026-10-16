//! Engine-generic decoded-sound storage, spatialization, and stereo mixing.
//!
//! Asset registration is deliberately separated from playback: decoding and
//! resampling may allocate, while [`Mixer::play`], [`Mixer::set_loop`], and
//! [`Mixer::render`] operate entirely inside fixed-capacity storage.

use std::error::Error;
use std::fmt;
use std::io::Cursor;

mod id3;

/// Opaque index of decoded PCM in a [`SoundBank`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SoundHandle(pub u32);

/// Stable identity of the entity or subsystem producing a voice.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceId(pub u32);

/// Logical channel used to replace an earlier voice from the same source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChannelId(pub u32);

/// Distance behavior supplied by the compatibility or game adapter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Attenuation {
    /// Listener-relative or global sound with equal full-volume channels.
    None,
    /// Full volume through `full_volume_distance`, then linear falloff.
    Linear {
        full_volume_distance: f32,
        falloff_per_unit: f32,
    },
}

/// One allocation-free voice start request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayRequest {
    pub origin: Option<[f32; 3]>,
    pub source: SourceId,
    pub channel: ChannelId,
    pub volume: f32,
    pub attenuation: Attenuation,
}

/// Mixer capacity and output format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixerConfig {
    pub voices: usize,
    /// Independent per-frame loop capacity.
    pub loops: usize,
    pub sample_rate: u32,
    /// Attenuation applied to per-frame looping sources; game adapters supply
    /// their own falloff parameters (the engine ships no game-specific preset).
    pub loop_attenuation: Attenuation,
    /// Caller-supplied loop pitch policy. Compatibility adapters own all
    /// game-specific values; the mixer only evaluates this generic ratio.
    pub doppler: DopplerConfig,
    /// A channel whose voices are stolen last when the table is full: long
    /// sounds (an emote's music) are always the oldest, so the plain
    /// oldest-first steal would cut them first. Its voices playing the same
    /// sound at the same sample (synced emotes, [`Mixer::play_aligned`]) are
    /// mixed once, each ear as loud as the loudest of them, not their sum.
    pub protected_channel: Option<ChannelId>,
}

/// Whether `other` plays the very sample `voice` plays, on its channel: a synced
/// group mixed as one.
fn same_sample(other: &Voice, voice: &Voice) -> bool {
    other.active
        && other.handle == voice.handle
        && other.cursor == voice.cursor
        && other.request.channel == voice.request.channel
}

/// Allocation-free distance-ratio Doppler policy for looping voices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DopplerConfig {
    /// Runtime gate (the legacy adapter maps this from its archived cvar).
    pub enabled: bool,
    /// Divisor applied to the squared-distance ratio.
    pub distance_ratio_divisor: f32,
    /// Upper pitch/resampling clamp.
    pub maximum_scale: f32,
}

impl DopplerConfig {
    /// A neutral engine-default policy; games opt in with explicit values.
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            distance_ratio_divisor: 1.0,
            maximum_scale: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
struct Sound {
    samples: Box<[f32]>,
}

/// Decoded mono PCM ready to install into a mixer-rate [`SoundBank`].
///
/// Decoding and resampling may allocate. Moving this value into a bank does
/// not, provided the bank was created with sufficient reserved capacity.
#[derive(Clone, Debug)]
pub struct DecodedSound {
    samples: Box<[f32]>,
}

impl DecodedSound {
    /// One silent sample used when an asynchronous asset decoder rejects data.
    pub fn silence() -> Self {
        Self {
            samples: Box::new([0.0]),
        }
    }

    /// Mono PCM already at the mixer's rate, such as a sound made from another one.
    pub fn from_samples(samples: Vec<f32>) -> Self {
        Self {
            samples: samples.into_boxed_slice(),
        }
    }

    /// The decoded mono samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

/// Decode WAV or MP3 bytes and resample them before the audio thread sees them.
pub fn decode_encoded(
    bytes: &[u8],
    extension: &str,
    target_sample_rate: u32,
) -> Result<DecodedSound, DecodeError> {
    let (samples, source_rate) = if extension.eq_ignore_ascii_case("mp3") {
        decode_mp3(bytes)?
    } else {
        decode_wav(bytes)?
    };
    Ok(DecodedSound {
        samples: resample(&samples, source_rate, target_sample_rate).into_boxed_slice(),
    })
}

/// Decoded mono PCM, resampled to one mixer rate at registration time.
#[derive(Default)]
pub struct SoundBank {
    sample_rate: u32,
    sounds: Vec<Sound>,
}

impl SoundBank {
    /// Construct an empty bank targeting `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            sounds: Vec::with_capacity(2_048),
        }
    }

    /// Register mono PCM. Resampling occurs now, never from `play`/`render`.
    pub fn register_pcm(&mut self, samples: &[f32], sample_rate: u32) -> SoundHandle {
        let samples = resample(samples, sample_rate, self.sample_rate).into_boxed_slice();
        let handle = SoundHandle(u32::try_from(self.sounds.len()).unwrap_or(u32::MAX));
        self.sounds.push(Sound { samples });
        handle
    }

    /// Decode PCM WAV or MP3 bytes once and register their mono mixdown.
    pub fn register_encoded(
        &mut self,
        bytes: &[u8],
        extension: &str,
    ) -> Result<SoundHandle, DecodeError> {
        let decoded = decode_encoded(bytes, extension, self.sample_rate)?;
        Ok(self.register_decoded(decoded))
    }

    /// Install already-decoded mixer-rate PCM without decoding in the mixer.
    pub fn register_decoded(&mut self, decoded: DecodedSound) -> SoundHandle {
        let handle = SoundHandle(u32::try_from(self.sounds.len()).unwrap_or(u32::MAX));
        self.sounds.push(Sound {
            samples: decoded.samples,
        });
        handle
    }

    fn get(&self, handle: SoundHandle) -> Option<&Sound> {
        self.sounds.get(handle.0 as usize)
    }

    /// Number of registered decoded sounds.
    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    /// Whether no sounds have been registered.
    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    /// Mixer-rate mono sample count for a registered sound.
    pub fn sample_count(&self, handle: SoundHandle) -> Option<usize> {
        self.get(handle).map(|sound| sound.samples.len())
    }

    /// Target sample rate shared by every registered sound.
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Decode or format error produced only during sound registration.
#[derive(Debug)]
pub struct DecodeError(String);

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for DecodeError {}

/// Listener data used by spatial models.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub origin: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    /// World-space velocity supplied independently from the view axes.
    pub velocity: [f32; 3],
}

/// Pluggable conversion from a source position to stereo gains.
pub trait SpatialModel: Send {
    /// Update separation for spatial models that expose that parameter.
    fn set_separation(&mut self, _separation: f32) {}
    fn gains(&self, listener: &Listener, origin: [f32; 3], attenuation: Attenuation) -> [f32; 2];
}

/// Quake/JKA's linear attenuation and stereo-separation calculation.
///
/// The default parameters reproduce `snd_dma.cpp:1345-1422`: full-volume
/// distance 256, `SOUND_ATTENUATE` 0.0008, and the requested symmetric 0.5
/// separation form. Voice-channel distance offsets remain adapter-selected by
/// supplying a different [`Attenuation::Linear`] full-volume distance.
#[derive(Clone, Copy, Debug)]
pub struct LegacyQuakeSpatial {
    separation: f32,
}

impl LegacyQuakeSpatial {
    pub fn new(separation: f32) -> Self {
        Self { separation }
    }
}

impl Default for LegacyQuakeSpatial {
    fn default() -> Self {
        Self::new(0.5)
    }
}

impl SpatialModel for LegacyQuakeSpatial {
    fn set_separation(&mut self, separation: f32) {
        self.separation = separation;
    }
    fn gains(&self, listener: &Listener, origin: [f32; 3], attenuation: Attenuation) -> [f32; 2] {
        if attenuation == Attenuation::None {
            return [1.0; 2];
        }
        let delta = sub(origin, listener.origin);
        let distance = length(delta);
        let direction = if distance > f32::EPSILON {
            scale(delta, 1.0 / distance)
        } else {
            [0.0; 3]
        };
        let dot = dot(listener.right, direction).clamp(-1.0, 1.0);
        let right = (self.separation + (1.0 - self.separation) * dot).max(0.0);
        let left = (self.separation - (1.0 - self.separation) * dot).max(0.0);
        let Attenuation::Linear {
            full_volume_distance,
            falloff_per_unit,
        } = attenuation
        else {
            unreachable!()
        };
        let normalized = ((distance - full_volume_distance).max(0.0) * falloff_per_unit).max(0.0);
        let distance_gain = (1.0 - normalized).max(0.0);
        [left * distance_gain, right * distance_gain]
    }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    handle: SoundHandle,
    request: PlayRequest,
    cursor: usize,
    started: u64,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            active: false,
            handle: SoundHandle(0),
            request: PlayRequest {
                origin: None,
                source: SourceId(0),
                channel: ChannelId(0),
                volume: 0.0,
                attenuation: Attenuation::None,
            },
            cursor: 0,
            started: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LoopVoice {
    active: bool,
    handle: SoundHandle,
    request: PlayRequest,
    velocity: [f32; 3],
    generation: u64,
    cursor: f64,
}

impl Default for LoopVoice {
    fn default() -> Self {
        Self {
            active: false,
            handle: SoundHandle(0),
            request: PlayRequest {
                origin: None,
                source: SourceId(0),
                channel: ChannelId(0),
                volume: 0.0,
                attenuation: Attenuation::None,
            },
            velocity: [0.0; 3],
            generation: 0,
            cursor: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BackgroundTrack {
    intro: SoundHandle,
    repeating: SoundHandle,
    cursor: usize,
    in_intro: bool,
    volume: f32,
}

/// Read-only position of the engine-generic intro/repeating music sequencer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackgroundTrackState {
    /// True until the intro's final sample has been consumed.
    pub in_intro: bool,
    /// Mono sample cursor within the currently selected track.
    pub cursor: usize,
}

/// Fixed-capacity software voice mixer.
pub struct Mixer {
    bank: SoundBank,
    voices: Box<[Voice]>,
    loops: Box<[LoopVoice]>,
    listener: Listener,
    spatial: Box<dyn SpatialModel>,
    serial: u64,
    loop_generation: u64,
    /// True between [`Mixer::clear_loops`] and [`Mixer::commit_loops`].
    loop_frame_open: bool,
    loop_attenuation: Attenuation,
    listener_source: Option<SourceId>,
    rendered_frames: u64,
    master_gain: f32,
    music_gain: f32,
    background: Option<BackgroundTrack>,
    doppler: DopplerConfig,
    protected_channel: Option<ChannelId>,
}

impl Mixer {
    /// Adjust the existing spatializer without replacing or allocating it.
    pub fn set_separation(&mut self, separation: f32) {
        self.spatial.set_separation(separation);
    }
    /// Construct with the legacy-compatible spatial model.
    pub fn new(config: MixerConfig) -> Self {
        Self::with_spatial_and_loop_attenuation(
            config,
            Box::new(LegacyQuakeSpatial::default()),
            config.loop_attenuation,
        )
    }

    /// Construct with a caller-selected spatializer.
    pub fn with_spatial(config: MixerConfig, spatial: Box<dyn SpatialModel>) -> Self {
        Self::with_spatial_and_loop_attenuation(config, spatial, Attenuation::None)
    }

    /// Construct with caller-owned spatial and loop attenuation policies.
    pub fn with_spatial_and_loop_attenuation(
        config: MixerConfig,
        spatial: Box<dyn SpatialModel>,
        loop_attenuation: Attenuation,
    ) -> Self {
        Self {
            bank: SoundBank::new(config.sample_rate),
            voices: vec![Voice::default(); config.voices].into_boxed_slice(),
            loops: vec![LoopVoice::default(); config.loops].into_boxed_slice(),
            listener: Listener {
                origin: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                right: [0.0, -1.0, 0.0],
                up: [0.0, 0.0, 1.0],
                velocity: [0.0; 3],
            },
            spatial,
            serial: 0,
            loop_generation: 0,
            loop_frame_open: false,
            loop_attenuation,
            listener_source: None,
            rendered_frames: 0,
            master_gain: 1.0,
            music_gain: 1.0,
            background: None,
            doppler: config.doppler,
            protected_channel: config.protected_channel,
        }
    }

    pub fn sound_bank(&self) -> &SoundBank {
        &self.bank
    }

    pub fn sound_bank_mut(&mut self) -> &mut SoundBank {
        &mut self.bank
    }

    /// Update the listener exactly once per rendered frame.
    pub fn listener(&mut self, origin: [f32; 3], forward: [f32; 3], right: [f32; 3], up: [f32; 3]) {
        self.listener = Listener {
            origin,
            forward,
            right,
            up,
            velocity: self.listener.velocity,
        };
    }

    /// Set listener velocity for relative-velocity Doppler evaluation.
    pub fn set_listener_velocity(&mut self, velocity: [f32; 3]) {
        self.listener.velocity = velocity;
    }

    /// Toggle Doppler without rebuilding the fixed voice storage.
    pub fn set_doppler_enabled(&mut self, enabled: bool) {
        self.doppler.enabled = enabled;
    }

    /// Identify the listener entity for `S_PickChannel` steal protection.
    pub fn set_listener_source(&mut self, source: Option<SourceId>) {
        self.listener_source = source;
    }

    /// Move every currently spatial voice owned by `source` without allocation.
    /// This is the engine-generic counterpart to `S_UpdateEntityPosition`.
    pub fn update_source_position(&mut self, source: SourceId, origin: [f32; 3]) {
        for voice in &mut self.voices {
            if voice.active && voice.request.source == source && voice.request.origin.is_some() {
                voice.request.origin = Some(origin);
            }
        }
        for loop_voice in &mut self.loops {
            if loop_voice.active
                && loop_voice.request.source == source
                && loop_voice.request.origin.is_some()
            {
                loop_voice.request.origin = Some(origin);
            }
        }
    }

    /// Start a voice without decoding or heap allocation.
    ///
    /// Channel zero follows `CHAN_AUTO`: it never stomps by source/channel.
    /// Non-zero channels replace the same source+channel first. Otherwise the
    /// first free slot wins, then the oldest non-loop voice, matching
    /// `S_PickChannel` (`snd_dma.cpp:1088-1150`).
    pub fn play(&mut self, handle: SoundHandle, request: PlayRequest) -> bool {
        self.play_from(handle, request, 0)
    }

    /// [`Self::play`] from `offset` mono samples into the sound (at the mixer's
    /// rate), as if it had started that long ago; an offset past its end plays
    /// nothing.
    pub fn play_from(&mut self, handle: SoundHandle, request: PlayRequest, offset: usize) -> bool {
        if self
            .bank
            .get(handle)
            .is_none_or(|sound| offset > 0 && offset >= sound.samples.len())
            || self.voices.is_empty()
        {
            return false;
        }
        let replacement = (request.channel != ChannelId(0))
            .then(|| {
                self.voices.iter().position(|voice| {
                    voice.active
                        && voice.request.source == request.source
                        && voice.request.channel == request.channel
                })
            })
            .flatten();
        let free = self.voices.iter().position(|voice| !voice.active);
        let Some(index) = replacement
            .or(free)
            .or_else(|| self.steal_index(request.source))
        else {
            return false;
        };
        self.serial = self.serial.wrapping_add(1);
        self.voices[index] = Voice {
            active: true,
            handle,
            request,
            cursor: offset,
            started: self.serial,
        };
        true
    }

    /// [`Self::play_from`], but at the very sample `align`'s voice on the same
    /// channel plays the same sound, when it plays it within `window` samples of
    /// `offset`: the two then sound as one (see
    /// [`MixerConfig::protected_channel`]). Otherwise from `offset`.
    pub fn play_aligned(
        &mut self,
        handle: SoundHandle,
        request: PlayRequest,
        offset: usize,
        align: SourceId,
        window: usize,
    ) -> bool {
        let aligned = self
            .voices
            .iter()
            .find(|voice| {
                voice.active
                    && voice.handle == handle
                    && voice.request.source == align
                    && voice.request.channel == request.channel
                    && voice.cursor.abs_diff(offset) <= window
            })
            .map_or(offset, |voice| voice.cursor);
        self.play_from(handle, request, aligned)
    }

    /// Apply the legacy channel-picker mute to a source/channel pair.
    ///
    /// Compatibility adapters may normalize related logical channels before
    /// calling this method. Like `S_MuteSound` → `S_PickChannel`, an existing
    /// source/channel is selected first; otherwise a free slot is harmlessly
    /// cleared, or a full table sacrifices its oldest eligible voice. The
    /// operation only touches the fixed voice array and never allocates.
    pub fn stop_source_channel(&mut self, source: SourceId, channel: ChannelId) -> bool {
        let matching = self.voices.iter().position(|voice| {
            voice.active && voice.request.source == source && voice.request.channel == channel
        });
        let free = self.voices.iter().position(|voice| !voice.active);
        let Some(index) = matching.or(free).or_else(|| self.steal_index(source)) else {
            return false;
        };
        let killed = self.voices[index].active;
        self.voices[index].active = false;
        killed
    }

    /// The voice a full table gives up for `source`: the oldest one, never the
    /// listener's for another source's sound, and one on the protected channel
    /// only when every other candidate is protected too.
    fn steal_index(&self, source: SourceId) -> Option<usize> {
        self.voices
            .iter()
            .enumerate()
            .filter(|(_, voice)| {
                !(Some(voice.request.source) == self.listener_source
                    && Some(source) != self.listener_source)
            })
            .min_by_key(|(_, voice)| {
                (
                    Some(voice.request.channel) == self.protected_channel,
                    voice.started,
                )
            })
            .map(|(index, _)| index)
    }

    /// Stop every per-frame loop currently mixed for `source` immediately.
    ///
    /// A later [`Self::set_loop`] may re-add it; game-side persistent lifetime
    /// remains the caller's responsibility, just as Quake cgame owns its
    /// event-maintained loop table independently of the sound backend.
    pub fn stop_source_loops(&mut self, source: SourceId) -> usize {
        let mut stopped = 0;
        for loop_voice in &mut self.loops {
            if loop_voice.active && loop_voice.request.source == source {
                loop_voice.active = false;
                stopped += 1;
            }
        }
        stopped
    }

    /// Stop all voices, loops and background music, retaining decoded assets.
    pub fn stop_all(&mut self) {
        self.stop_effects();
        self.background = None;
    }

    /// Stop transient voices and loops without racing independently queued music changes.
    pub fn stop_effects(&mut self) {
        self.voices.fill(Voice::default());
        self.loops.fill(LoopVoice::default());
    }

    /// Begin rebuilding the current frame's loop set.
    ///
    /// The previous frame's loops keep sounding until [`Self::commit_loops`]
    /// closes the rebuild, so a render that lands between this call and the
    /// frame's last [`Self::set_loop`] cannot cut them for a block. Loops not
    /// re-added by the commit stop at the next render.
    pub fn clear_loops(&mut self) {
        self.loop_generation = self.loop_generation.wrapping_add(1);
        self.loop_frame_open = true;
    }

    /// Finish the loop set begun by [`Self::clear_loops`].
    pub fn commit_loops(&mut self) {
        self.loop_frame_open = false;
    }

    /// Whether a loop set has been cleared but not yet committed.
    pub const fn loop_frame_open(&self) -> bool {
        self.loop_frame_open
    }

    /// Re-add or start a looping sound for this frame without allocation.
    pub fn set_loop(
        &mut self,
        source: SourceId,
        handle: SoundHandle,
        origin: [f32; 3],
        velocity: [f32; 3],
    ) -> bool {
        self.set_loop_request(
            handle,
            PlayRequest {
                origin: Some(origin),
                source,
                channel: ChannelId(0),
                volume: 1.0,
                attenuation: self.loop_attenuation,
            },
            velocity,
        )
    }

    /// Re-add a loop with an explicit generic voice request and source velocity.
    pub fn set_loop_request(
        &mut self,
        handle: SoundHandle,
        request: PlayRequest,
        velocity: [f32; 3],
    ) -> bool {
        if self.bank.get(handle).is_none() {
            return false;
        }
        let (generation, open) = (self.loop_generation, self.loop_frame_open);
        if let Some(loop_voice) = self.loops.iter_mut().find(|loop_voice| {
            loop_voice.request.source == request.source && loop_voice.handle == handle
        }) {
            let continuously_readded = loop_voice.active
                && (loop_voice.generation == generation
                    || loop_voice.generation.wrapping_add(1) == generation);
            if !continuously_readded {
                loop_voice.cursor = self.rendered_frames as f64;
            }
            loop_voice.active = true;
            loop_voice.request = request;
            loop_voice.velocity = velocity;
            loop_voice.generation = self.loop_generation;
            return true;
        }
        // Prefer a slot nobody is mixing; a previous-frame loop that has not
        // been re-added yet is only taken when the table is otherwise full.
        let Some(index) = self
            .loops
            .iter()
            .position(|loop_voice| !loop_audible(loop_voice, generation, open))
            .or_else(|| {
                self.loops
                    .iter()
                    .position(|loop_voice| loop_voice.generation != generation)
            })
        else {
            return false;
        };
        self.loops[index] = LoopVoice {
            active: true,
            handle,
            request,
            velocity,
            generation: self.loop_generation,
            cursor: self.rendered_frames as f64,
        };
        true
    }

    /// Set the effects/voice/loop master gain without touching source voices.
    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = gain.clamp(0.0, 1.0);
    }

    /// Set the background-music gain independently of the master gain.
    pub fn set_music_gain(&mut self, gain: f32) {
        self.music_gain = gain.clamp(0.0, 1.0);
    }

    /// Start one intro sound followed sample-exactly by a repeating sound.
    ///
    /// Supplying the same handle for both implements a conventional single
    /// looping track. Decoding remains an explicit registration-time action.
    pub fn start_background_track(
        &mut self,
        intro: SoundHandle,
        repeating: SoundHandle,
        volume: f32,
    ) -> bool {
        if self.bank.sample_count(intro).is_none_or(|count| count == 0)
            || self
                .bank
                .sample_count(repeating)
                .is_none_or(|count| count == 0)
        {
            return false;
        }
        self.background = Some(BackgroundTrack {
            intro,
            repeating,
            cursor: 0,
            in_intro: true,
            volume: volume.clamp(0.0, 1.0),
        });
        true
    }

    /// Stop the current background track without affecting sound voices.
    pub fn stop_background_track(&mut self) {
        self.background = None;
    }

    /// Current intro/repeating cursor, primarily for diagnostics and tests.
    pub fn background_track_state(&self) -> Option<BackgroundTrackState> {
        self.background.map(|track| BackgroundTrackState {
            in_intro: track.in_intro,
            cursor: track.cursor,
        })
    }

    /// Mix interleaved stereo samples without allocating.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        for index in 0..self.voices.len() {
            let voice = self.voices[index];
            if !voice.active {
                continue;
            }
            let Some(sound) = self.bank.get(voice.handle) else {
                self.voices[index].active = false;
                continue;
            };
            let merged = Some(voice.request.channel) == self.protected_channel;
            // Mixed already with an earlier voice of its group this block.
            if merged
                && self.voices[..index]
                    .iter()
                    .any(|other| same_sample(other, &voice))
            {
                continue;
            }
            let gains_of = |voice: &Voice| {
                let gains = voice.request.origin.map_or([1.0; 2], |origin| {
                    self.spatial
                        .gains(&self.listener, origin, voice.request.attenuation)
                });
                gains.map(|gain| gain * voice.request.volume)
            };
            let mut gains = gains_of(&voice);
            if merged {
                for other in &self.voices[index + 1..] {
                    if same_sample(other, &voice) {
                        let more = gains_of(other);
                        gains = [gains[0].max(more[0]), gains[1].max(more[1])];
                    }
                }
            }
            let mut cursor = voice.cursor;
            let mut active = true;
            for frame in out.chunks_exact_mut(2) {
                if cursor >= sound.samples.len() {
                    active = false;
                    break;
                }
                let sample = sound.samples[cursor] * self.master_gain;
                frame[0] += sample * gains[0];
                frame[1] += sample * gains[1];
                cursor += 1;
            }
            for other in index..self.voices.len() {
                if other == index || (merged && same_sample(&self.voices[other], &voice)) {
                    self.voices[other].cursor = cursor;
                    self.voices[other].active = active;
                }
            }
        }
        self.render_loops(out);
        self.render_background(out);
        self.rendered_frames = self.rendered_frames.wrapping_add((out.len() / 2) as u64);
        for sample in out {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }

    fn render_loops(&mut self, out: &mut [f32]) {
        let (generation, open) = (self.loop_generation, self.loop_frame_open);
        for index in 0..self.loops.len() {
            if !loop_audible(&self.loops[index], generation, open) {
                self.loops[index].active = false;
                continue;
            }
            let handle = self.loops[index].handle;
            if self.loops[..index]
                .iter()
                .any(|earlier| loop_audible(earlier, generation, open) && earlier.handle == handle)
            {
                continue;
            }
            let mut gains = [0.0; 2];
            for loop_voice in &self.loops[index..] {
                if !loop_audible(loop_voice, generation, open) || loop_voice.handle != handle {
                    continue;
                }
                let spatial = loop_voice.request.origin.map_or([1.0; 2], |origin| {
                    self.spatial
                        .gains(&self.listener, origin, loop_voice.request.attenuation)
                });
                gains[0] += spatial[0] * loop_voice.request.volume;
                gains[1] += spatial[1] * loop_voice.request.volume;
            }
            gains[0] = gains[0].min(1.0) * self.master_gain;
            gains[1] = gains[1].min(1.0) * self.master_gain;
            let Some(sound) = self.bank.get(handle) else {
                continue;
            };
            if sound.samples.is_empty() {
                continue;
            }
            let pitch = doppler_scale(
                self.doppler,
                &self.listener,
                self.loops[index].request.origin,
                self.loops[index].velocity,
            );
            let mut cursor = self.loops[index].cursor;
            for frame in out.chunks_exact_mut(2) {
                let base = cursor.floor() as usize;
                let fraction = (cursor - base as f64) as f32;
                let low = base % sound.samples.len();
                let high = (low + 1) % sound.samples.len();
                let sample =
                    sound.samples[low] + (sound.samples[high] - sound.samples[low]) * fraction;
                frame[0] += sample * gains[0];
                frame[1] += sample * gains[1];
                cursor += f64::from(pitch);
            }
            for loop_voice in &mut self.loops[index..] {
                if loop_audible(loop_voice, generation, open) && loop_voice.handle == handle {
                    loop_voice.cursor = cursor;
                }
            }
        }
    }

    fn render_background(&mut self, out: &mut [f32]) {
        let Some(track) = &mut self.background else {
            return;
        };
        for frame in out.chunks_exact_mut(2) {
            let handle = if track.in_intro {
                track.intro
            } else {
                track.repeating
            };
            let Some(sound) = self.bank.get(handle) else {
                return;
            };
            if track.cursor >= sound.samples.len() {
                if track.in_intro {
                    track.in_intro = false;
                    track.cursor = 0;
                } else {
                    track.cursor = 0;
                }
            }
            let handle = if track.in_intro {
                track.intro
            } else {
                track.repeating
            };
            let Some(sound) = self.bank.get(handle) else {
                return;
            };
            if sound.samples.is_empty() {
                return;
            }
            let sample = sound.samples[track.cursor] * track.volume * self.music_gain;
            frame[0] += sample;
            frame[1] += sample;
            track.cursor += 1;
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }

    /// Number of per-frame loops currently retained by the fixed loop table.
    pub fn active_loop_count(&self) -> usize {
        self.loops
            .iter()
            .filter(|loop_voice| loop_voice.active)
            .count()
    }
}

/// Whether a loop belongs to the set being mixed: re-added this frame, or, while
/// a rebuild is still open, carried over from the previous frame.
fn loop_audible(loop_voice: &LoopVoice, generation: u64, open: bool) -> bool {
    loop_voice.active
        && (loop_voice.generation == generation
            || (open && loop_voice.generation.wrapping_add(1) == generation))
}

fn doppler_scale(
    config: DopplerConfig,
    listener: &Listener,
    origin: Option<[f32; 3]>,
    source_velocity: [f32; 3],
) -> f32 {
    if !config.enabled || config.distance_ratio_divisor <= 0.0 {
        return 1.0;
    }
    let Some(origin) = origin else { return 1.0 };
    let relative_velocity = sub(source_velocity, listener.velocity);
    if dot(relative_velocity, relative_velocity) <= f32::EPSILON {
        return 1.0;
    }
    let distance = sub(origin, listener.origin);
    let before = dot(distance, distance);
    if before <= f32::EPSILON {
        return config.maximum_scale.max(1.0);
    }
    let after = sub(add(origin, relative_velocity), listener.origin);
    let scale = dot(after, after) / (before * config.distance_ratio_divisor);
    if scale <= 1.0 {
        1.0
    } else {
        scale.min(config.maximum_scale.max(1.0))
    }
}

fn decode_wav(bytes: &[u8]) -> Result<(Vec<f32>, u32), DecodeError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|error| DecodeError(format!("WAV: {error}")))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let raw = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| DecodeError(format!("WAV samples: {error}")))?,
        hound::SampleFormat::Int => {
            let scale = (1_u64 << spec.bits_per_sample.saturating_sub(1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| DecodeError(format!("WAV samples: {error}")))?
        }
    };
    Ok((mono(&raw, channels), spec.sample_rate))
}

fn decode_mp3(bytes: &[u8]) -> Result<(Vec<f32>, u32), DecodeError> {
    let bytes = id3::without_id3v1(bytes);
    let mut decoder = minimp3_fixed::Decoder::new(Cursor::new(bytes));
    let mut output = Vec::new();
    let mut rate = None;
    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                rate.get_or_insert(frame.sample_rate as u32);
                let samples = frame
                    .data
                    .iter()
                    .map(|sample| f32::from(*sample) / 32768.0)
                    .collect::<Vec<_>>();
                output.extend(mono(&samples, frame.channels));
            }
            Err(minimp3_fixed::Error::Eof) => break,
            Err(error) => return Err(DecodeError(format!("MP3: {error:?}"))),
        }
    }
    Ok((
        output,
        rate.ok_or_else(|| DecodeError("MP3 contained no frames".into()))?,
    ))
}

fn mono(samples: &[f32], channels: usize) -> Vec<f32> {
    samples
        .chunks(channels.max(1))
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let length = (samples.len() as u64 * u64::from(target_rate) / u64::from(source_rate)) as usize;
    (0..length)
        .map(|index| {
            let source = index as f64 * f64::from(source_rate) / f64::from(target_rate);
            let low = source.floor() as usize;
            let high = (low + 1).min(samples.len() - 1);
            let fraction = (source - low as f64) as f32;
            samples[low] + (samples[high] - samples[low]) * fraction
        })
        .collect()
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}
fn add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] + right[axis])
}
fn scale(value: [f32; 3], amount: f32) -> [f32; 3] {
    value.map(|component| component * amount)
}
fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}
fn length(value: [f32; 3]) -> f32 {
    dot(value, value).sqrt()
}

#[cfg(test)]
mod loop_frame_tests;
#[cfg(test)]
mod spatial_tests;
#[cfg(test)]
mod sync_mix_tests;
#[cfg(test)]
mod voice_steal_tests;

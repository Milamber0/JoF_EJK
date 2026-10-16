//! Allocation-free rodio callback and bounded command/decode queues.

use ringbuf::{HeapCons, HeapProd, HeapRb, traits::*};
use rodio::{OutputStream, Source};
use sjk_audio::{
    ChannelId, DecodedSound, Mixer, MixerConfig, PlayRequest, SoundHandle, SourceId, decode_encoded,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[path = "audio_feed.rs"]
mod feed;
#[path = "audio_pending.rs"]
mod pending;

pub(super) const SAMPLE_RATE: u32 = 44_100;
/// Four times retail's 32 channels: a busy saber fight filled 32, and every
/// voice costs one pass over a 256-sample block.
pub(super) const VOICES: usize = 128;
pub(super) const LOOPS: usize = 32;
const MIX_SAMPLES: usize = 256;
const MIX_BLOCK_BUDGET: Duration =
    Duration::from_micros((MIX_SAMPLES as u64 / 2) * 1_000_000 / SAMPLE_RATE as u64);
const COMMANDS: usize = 8_192;
const DECODE_JOBS: usize = 8_192;

/// Counters the audio thread publishes for the render thread's diagnostics;
/// the audio thread only ever touches atomics.
#[derive(Default)]
pub(super) struct OutputStats {
    /// Mix blocks that took longer than their playback duration to render.
    pub(super) underruns: AtomicU64,
    /// Bits of the loudest sample rendered since the last `take_peak`.
    peak_bits: AtomicU32,
    pub(super) rendered_blocks: AtomicU64,
    pub(super) music_starts: AtomicU64,
    /// Music starts the mixer refused (a handle without decoded samples).
    pub(super) music_rejections: AtomicU64,
    /// Sounds registered as silence because their bytes did not decode.
    pub(super) decode_failures: AtomicU64,
    /// Registrations whose bank index differed from the handle handed out.
    pub(super) handle_mismatches: AtomicU64,
    /// Mix blocks rendered while the render thread was still sending a
    /// frame's loops; each one used to cut the unsent loops for a block.
    pub(super) split_loop_frames: AtomicU64,
}

impl OutputStats {
    /// Loudest sample since the previous call.
    pub(super) fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak_bits.swap(0, Ordering::Relaxed))
    }

    fn observe(&self, block: &[f32]) {
        let peak = block
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        // Non-negative floats order like their bit patterns.
        self.peak_bits.fetch_max(peak.to_bits(), Ordering::Relaxed);
        self.rendered_blocks.fetch_add(1, Ordering::Relaxed);
    }
}

struct MixerSource {
    mixer: Mixer,
    commands: HeapCons<AudioCommand>,
    registrations: HeapCons<DecodedAction>,
    scratch: [f32; MIX_SAMPLES],
    cursor: usize,
    running: bool,
    stats: Arc<OutputStats>,
    pending: pending::PendingStarts,
}

impl Iterator for MixerSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.running {
            return None;
        }
        if self.cursor == self.scratch.len() {
            while let Some(action) = self.registrations.try_pop() {
                match action {
                    DecodedAction::Register(expected, decoded) => {
                        // Registration must run in release too: the bank
                        // index is the handle the render thread already
                        // handed out, so it has to be assigned in order.
                        let registered = self.mixer.sound_bank_mut().register_decoded(decoded);
                        if registered != expected {
                            self.stats.handle_mismatches.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    DecodedAction::StopMusic => self.mixer.stop_background_track(),
                    DecodedAction::StartMusic(intro, repeating) => {
                        let counter = if self.mixer.start_background_track(intro, repeating, 1.0) {
                            &self.stats.music_starts
                        } else {
                            &self.stats.music_rejections
                        };
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            while let Some(command) = self.commands.try_pop() {
                if let AudioCommand::Play(handle, request) = command {
                    self.pending.play(&mut self.mixer, handle, request);
                    continue;
                }
                if let AudioCommand::StopChannel(source, channel)
                | AudioCommand::PlayFrom(
                    _,
                    PlayRequest {
                        source, channel, ..
                    },
                    ..,
                ) = &command
                {
                    self.pending.cancel(*source, *channel);
                }
                if matches!(command, AudioCommand::StopAll | AudioCommand::StopEffects) {
                    self.pending.clear();
                }
                self.running = apply_command(&mut self.mixer, command);
                if !self.running {
                    return None;
                }
            }
            self.pending.flush(&mut self.mixer);
            if self.mixer.loop_frame_open() {
                self.stats.split_loop_frames.fetch_add(1, Ordering::Relaxed);
            }
            let started = Instant::now();
            self.mixer.render(&mut self.scratch);
            if started.elapsed() > MIX_BLOCK_BUDGET {
                self.stats.underruns.fetch_add(1, Ordering::Relaxed);
            }
            self.stats.observe(&self.scratch);
            self.cursor = 0;
        }
        let sample = self.scratch[self.cursor];
        self.cursor += 1;
        Some(sample)
    }
}

impl Source for MixerSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub(super) enum AudioCommand {
    Play(SoundHandle, PlayRequest),
    /// Play a loaded sound from this many mono samples in, or at the sample the
    /// given source's voice plays it near there ([`sjk_audio::Mixer::play_aligned`]);
    /// one still decoding is not started.
    PlayFrom(SoundHandle, PlayRequest, usize, Option<SourceId>),
    Listener {
        origin: [f32; 3],
        forward: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        velocity: [f32; 3],
    },
    ListenerSource(Option<SourceId>),
    SourcePosition(SourceId, [f32; 3]),
    StopChannel(SourceId, ChannelId),
    StopLoops(SourceId),
    /// Start a frame's loop set; the previous set plays on until `CommitLoops`.
    ClearLoops,
    /// End the loop set begun by `ClearLoops`.
    CommitLoops,
    StopAll,
    StopEffects,
    SetLoop(SoundHandle, PlayRequest, [f32; 3]),
    Gains(f32, f32),
    Doppler(bool),
    Separation(f32),
    Shutdown,
}

enum DecodedAction {
    Register(SoundHandle, DecodedSound),
    StartMusic(SoundHandle, SoundHandle),
    StopMusic,
}

pub(super) struct AudioOutput {
    _stream: Option<OutputStream>,
    pub(super) description: String,
    commands: HeapProd<AudioCommand>,
    /// Whether the callback still drains `commands`; a stopped stream is not waited on.
    feed: feed::Feed,
    decode_jobs: Option<SyncSender<DecodeJob>>,
    decoder: Option<JoinHandle<()>>,
    /// Set as the output is dropped: the decoder stops waiting for queue room.
    closing: Arc<AtomicBool>,
    pub(super) stats: Arc<OutputStats>,
    pub(super) show: bool,
    pub(super) started: std::collections::VecDeque<SoundHandle>,
    pub(super) cache: std::cell::RefCell<Vec<(SoundHandle, usize, String)>>,
}

enum DecodeJob {
    Sound {
        handle: SoundHandle,
        bytes: Box<[u8]>,
        extension: Box<str>,
    },
    /// A sound made from a decoded one: `make` turns its mixer-rate samples into the
    /// new sound's (`sjk_chat_sound.rs`).
    Variant {
        handle: SoundHandle,
        bytes: Box<[u8]>,
        extension: Box<str>,
        make: fn(&[f32], u32) -> Vec<f32>,
    },
    StartMusic(SoundHandle, SoundHandle),
    StopMusic,
}

impl AudioOutput {
    /// Open the default output device; a failure is logged, not swallowed,
    /// so a silent game names its cause.
    pub(super) fn start(config: MixerConfig) -> Option<Self> {
        let (stream, output) = match OutputStream::try_default() {
            Ok(stream) => stream,
            Err(error) => {
                crate::log::progress(format_args!("audio output unavailable: {error}"));
                return None;
            }
        };
        let description = describe_default_device();
        crate::log::progress(format_args!("audio output: {description}"));
        let (mut audio, source) = Self::assemble(config, description)?;
        if let Err(error) = output.play_raw(source) {
            crate::log::progress(format_args!("audio mixer could not start: {error}"));
            return None;
        }
        audio._stream = Some(stream);
        Some(audio)
    }

    /// The queues, the decode worker and the source the device's callback pulls.
    fn assemble(config: MixerConfig, description: String) -> Option<(Self, MixerSource)> {
        let (command_producer, command_consumer) = HeapRb::new(COMMANDS).split();
        let (registration_producer, registration_consumer) = HeapRb::new(COMMANDS).split();
        let (decode_sender, decode_receiver) = sync_channel::<DecodeJob>(DECODE_JOBS);
        let stats = Arc::new(OutputStats::default());
        let closing = Arc::new(AtomicBool::new(false));
        let decoder = {
            let stats = Arc::clone(&stats);
            let closing = Arc::clone(&closing);
            thread::Builder::new()
                .name("sjk-audio-decode".into())
                .spawn(move || decode(&decode_receiver, registration_producer, &stats, &closing))
                .ok()?
        };
        let source = MixerSource {
            mixer: Mixer::new(config),
            commands: command_consumer,
            registrations: registration_consumer,
            scratch: [0.0; MIX_SAMPLES],
            cursor: MIX_SAMPLES,
            running: true,
            stats: Arc::clone(&stats),
            pending: pending::PendingStarts::new(),
        };
        let audio = Self {
            _stream: None,
            description,
            commands: command_producer,
            feed: feed::Feed::default(),
            decode_jobs: Some(decode_sender),
            decoder: Some(decoder),
            closing,
            stats,
            show: false,
            started: std::collections::VecDeque::with_capacity(64),
            cache: std::cell::RefCell::new(Vec::new()),
        };
        Some((audio, source))
    }

    pub(super) fn send(&mut self, command: AudioCommand) {
        if self.show
            && let AudioCommand::Play(handle, _) = &command
        {
            if self.started.len() == 64 {
                self.started.pop_front();
            }
            self.started.push_back(*handle);
        }
        // Never wait for good on a callback that stopped (its device went away): the
        // game would stop answering. Every frame sends its state again.
        let pushed = self.feed.push(&mut self.commands, command, feed::PATIENCE);
        if pushed == (feed::Pushed::Dropped { stopped: true }) {
            crate::log::progress(format_args!(
                "audio output stopped taking sound (its device went away?); sounds are \
                 dropped until it resumes or snd_restart opens the output again"
            ));
        }
    }

    pub(super) fn decode(&self, handle: SoundHandle, bytes: &[u8], extension: &str) {
        self.cache
            .borrow_mut()
            .push((handle, bytes.len(), extension.to_owned()));
        let Some(sender) = &self.decode_jobs else {
            return;
        };
        let job = DecodeJob::Sound {
            handle,
            bytes: bytes.into(),
            extension: extension.into(),
        };
        if sender.try_send(job).is_err() {
            crate::log::progress(format_args!(
                "audio decode queue is full; dropping {handle:?}"
            ));
        }
    }

    /// Decode `bytes` on the worker, as [`Self::decode`] does, and register under `handle`
    /// what `make` makes of the samples (at [`SAMPLE_RATE`]) instead of the samples.
    pub(super) fn decode_variant(
        &self,
        handle: SoundHandle,
        bytes: &[u8],
        extension: &str,
        make: fn(&[f32], u32) -> Vec<f32>,
    ) {
        self.cache
            .borrow_mut()
            .push((handle, bytes.len(), extension.to_owned()));
        let Some(sender) = &self.decode_jobs else {
            return;
        };
        let job = DecodeJob::Variant {
            handle,
            bytes: bytes.into(),
            extension: extension.into(),
            make,
        };
        if sender.try_send(job).is_err() {
            crate::log::progress(format_args!(
                "audio decode queue is full; dropping {handle:?}"
            ));
        }
    }

    pub(super) fn stop_music(&self) {
        if let Some(sender) = &self.decode_jobs {
            let _ = sender.try_send(DecodeJob::StopMusic);
        }
    }

    pub(super) fn start_music(&self, intro: SoundHandle, repeating: SoundHandle) {
        let Some(sender) = &self.decode_jobs else {
            return;
        };
        if sender
            .try_send(DecodeJob::StartMusic(intro, repeating))
            .is_err()
        {
            crate::log::progress(format_args!("audio decode queue is full; music deferred"));
        }
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        self.decode_jobs.take();
        self.closing.store(true, Ordering::Release);
        if let Some(decoder) = self.decoder.take() {
            let _ = decoder.join();
        }
        let _ = self.commands.try_push(AudioCommand::Shutdown);
    }
}

/// The decode worker: decode each job and queue its registration for the callback, in
/// order, until the jobs end or the output closes.
fn decode(
    jobs: &Receiver<DecodeJob>,
    mut registrations: HeapProd<DecodedAction>,
    stats: &OutputStats,
    closing: &AtomicBool,
) {
    while let Ok(job) = jobs.recv() {
        if closing.load(Ordering::Acquire) {
            return;
        }
        let action = match job {
            DecodeJob::Sound {
                handle,
                bytes,
                extension,
            } => DecodedAction::Register(
                handle,
                decode_encoded(&bytes, &extension, SAMPLE_RATE).unwrap_or_else(|_| {
                    stats.decode_failures.fetch_add(1, Ordering::Relaxed);
                    DecodedSound::silence()
                }),
            ),
            DecodeJob::Variant {
                handle,
                bytes,
                extension,
                make,
            } => DecodedAction::Register(
                handle,
                match decode_encoded(&bytes, &extension, SAMPLE_RATE) {
                    Ok(decoded) => DecodedSound::from_samples(make(decoded.samples(), SAMPLE_RATE)),
                    Err(_) => {
                        stats.decode_failures.fetch_add(1, Ordering::Relaxed);
                        DecodedSound::silence()
                    }
                },
            ),
            DecodeJob::StopMusic => DecodedAction::StopMusic,
            DecodeJob::StartMusic(intro, repeating) => DecodedAction::StartMusic(intro, repeating),
        };
        if !feed::push_until_closed(&mut registrations, action, closing, feed::PATIENCE) {
            return;
        }
    }
}

/// Name and format of the device rodio just opened, for the startup log.
fn describe_default_device() -> String {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let Some(device) = rodio::cpal::default_host().default_output_device() else {
        return "no default output device".into();
    };
    let name = device.name().unwrap_or_else(|_| "unnamed device".into());
    match device.default_output_config() {
        Ok(config) => format!(
            "device={name} {}Hz {}ch {:?}",
            config.sample_rate().0,
            config.channels(),
            config.sample_format()
        ),
        Err(error) => format!("device={name} (format unknown: {error})"),
    }
}

fn apply_command(mixer: &mut Mixer, command: AudioCommand) -> bool {
    match command {
        AudioCommand::Play(handle, request) => {
            mixer.play(handle, request);
        }
        AudioCommand::PlayFrom(handle, request, offset, align) => match align {
            // Half a second either way: a leader's voice further off is another
            // round of its loop, about to end or just begun.
            Some(align) => {
                mixer.play_aligned(handle, request, offset, align, SAMPLE_RATE as usize / 2);
            }
            None => {
                mixer.play_from(handle, request, offset);
            }
        },
        AudioCommand::Listener {
            origin,
            forward,
            right,
            up,
            velocity,
        } => {
            mixer.listener(origin, forward, right, up);
            mixer.set_listener_velocity(velocity);
        }
        AudioCommand::ListenerSource(source) => mixer.set_listener_source(source),
        AudioCommand::SourcePosition(source, origin) => {
            mixer.update_source_position(source, origin);
        }
        AudioCommand::StopChannel(source, channel) => {
            mixer.stop_source_channel(source, channel);
        }
        AudioCommand::StopLoops(source) => {
            mixer.stop_source_loops(source);
        }
        AudioCommand::ClearLoops => {
            mixer.clear_loops();
        }
        AudioCommand::CommitLoops => mixer.commit_loops(),
        AudioCommand::StopAll => mixer.stop_all(),
        AudioCommand::StopEffects => mixer.stop_effects(),
        AudioCommand::SetLoop(handle, request, velocity) => {
            mixer.set_loop_request(handle, request, velocity);
        }
        AudioCommand::Gains(effects, music) => {
            mixer.set_master_gain(effects);
            mixer.set_music_gain(music);
        }
        AudioCommand::Doppler(enabled) => {
            mixer.set_doppler_enabled(enabled);
        }
        AudioCommand::Separation(value) => mixer.set_separation(value),
        AudioCommand::Shutdown => return false,
    }
    true
}

#[cfg(test)]
mod stopped_stream_tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// Long enough for the work below; the old endless waits never finished it.
    const LIMIT: Duration = Duration::from_secs(20);

    /// An output whose source no device pulls: the callback of a stream that ended.
    fn detached() -> (AudioOutput, MixerSource) {
        AudioOutput::assemble(
            MixerConfig {
                voices: VOICES,
                loops: LOOPS,
                sample_rate: SAMPLE_RATE,
                loop_attenuation: sjk_client::legacy_sound_attenuation(0),
                doppler: sjk_client::legacy_doppler_config(true),
                protected_channel: Some(crate::audio::EMOTE_CHANNEL),
            },
            "detached".into(),
        )
        .expect("decode worker starts")
    }

    #[test]
    fn a_stopped_callback_does_not_hold_the_render_thread() {
        let (done, finished) = channel();
        std::thread::spawn(move || {
            let (mut output, mut source) = detached();
            // Several seconds of a live game's commands (a listener and gains each
            // frame, a position for each entity of each snapshot), far past the queue.
            for _ in 0..COMMANDS * 4 {
                output.send(AudioCommand::Gains(0.0, 0.0));
            }
            let stalled = output.feed.stalled();
            // A callback that drains the queue again (one mix block) ends the stall.
            let _ = source.next();
            output.send(AudioCommand::Gains(0.5, 0.25));
            let _ = done.send((stalled, output.feed.stalled()));
        });
        assert_eq!(finished.recv_timeout(LIMIT), Ok((true, false)));
    }

    #[test]
    fn dropping_the_output_does_not_wait_on_a_stopped_callback() {
        let (done, finished) = channel();
        std::thread::spawn(move || {
            let (output, source) = detached();
            let decoded = |count: u64| {
                let started = Instant::now();
                while output.stats.decode_failures.load(Ordering::Relaxed) < count {
                    assert!(started.elapsed() < LIMIT, "decoder stalled at {count}");
                    thread::sleep(Duration::from_millis(1));
                }
            };
            // Fill the registration queue nothing drains, then give the decoder one
            // more sound, which it holds waiting for room (`snd_restart`, quitting).
            for index in 0..COMMANDS {
                output.decode(SoundHandle(index as u32), b"none", "wav");
            }
            decoded(COMMANDS as u64);
            output.decode(SoundHandle(COMMANDS as u32), b"none", "wav");
            decoded(COMMANDS as u64 + 1);
            drop(output);
            drop(source);
            let _ = done.send(());
        });
        assert_eq!(finished.recv_timeout(LIMIT), Ok(()));
    }
}

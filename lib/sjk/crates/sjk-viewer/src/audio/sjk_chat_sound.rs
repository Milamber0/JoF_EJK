//! The SJK chat's sound (`docs/hub-chat.md`, "A sound for a new message"): a variant of
//! JKA's chat sound, `sound/player/talk`, made from the player's own file when the
//! audio first meets the game data. Nothing of JKA's audio ships with SJK: the file is
//! read through the game's file system, decoded as every sound is, and [`variant`]
//! makes the new sound from its samples on the decode worker; the result lives in
//! memory only, under [`HANDLE_KEY`].
//!
//! The variant is the talk sound a little higher and quicker, cut short, faded and
//! given a short echo, so it is recognisably the game's chat sound and still tells an
//! SJK message from a game server's.

use super::GameAudio;
use sjk_audio::{ChannelId, SourceId};
use sjk_vfs::VirtualFileSystem;
use std::sync::atomic::{AtomicBool, Ordering};

/// The game's chat sound the variant is made from (`.wav`, else `.mp3`).
pub(crate) const TALK: &str = "sound/player/talk.wav";

/// The made sound's name among the handles; no file has it (no game path starts `*`).
pub(crate) const HANDLE_KEY: &str = "*sjk/chat";

/// How much faster (and so higher) the talk sound plays.
const PITCH: f64 = 1.122;
/// The made sound keeps this much of the quickened talk sound, in seconds.
const KEEP: f64 = 0.60;
/// Its last part fades out linearly: from this time, in seconds ...
const FADE_START: f64 = 0.45;
/// ... over this long, to silence at [`KEEP`].
const FADE_LENGTH: f64 = 0.15;
/// The echo: the sound itself at this gain ...
const DRY: f32 = 0.8;
/// ... and two delayed copies, `(delay in seconds, gain)`, read from the faded sound
/// (not fed back), the sound growing by the longest delay.
const TAPS: [(f64, f32); 2] = [(0.070, 0.3), (0.140, 0.15)];
/// The echo's sum is scaled by this.
const WET: f32 = 0.6;
/// The last gain, bringing the variant to the talk sound's own loudness.
const LEVEL: f32 = 1.5494;

/// Make the SJK chat sound from the talk sound's mono `talk` samples at `rate` Hz: play
/// them [`PITCH`] times faster (pitch and speed rise together, [`quicken`]), keep the
/// first [`KEEP`] seconds and fade their last [`FADE_LENGTH`] out ([`fade`]), add the two
/// echo [`TAPS`] ([`echo`]) and bring it to the original's level, clamped to `[-1, 1]`.
/// At 44.1 kHz a long enough talk sound gives 0.74 seconds.
pub(crate) fn variant(talk: &[f32], rate: u32) -> Vec<f32> {
    let mut sound = quicken(talk, seconds(KEEP, rate));
    fade(&mut sound, rate);
    let mut sound = echo(&sound, rate);
    for sample in &mut sound {
        *sample = (*sample * LEVEL).clamp(-1.0, 1.0);
    }
    sound
}

/// `time` seconds in samples at `rate` Hz.
fn seconds(time: f64, rate: u32) -> usize {
    (time * f64::from(rate)).round() as usize
}

/// `talk` played [`PITCH`] times faster: sample n is the talk sound at n * PITCH, so
/// the sound is that much shorter and higher; at most `keep` samples of it.
fn quicken(talk: &[f32], keep: usize) -> Vec<f32> {
    let length = ((talk.len() as f64) / PITCH).ceil() as usize;
    (0..length.min(keep))
        .map(|n| read_at(talk, n as f64 * PITCH))
        .collect()
}

/// Fade `sound` out linearly from [`FADE_START`] to silence [`FADE_LENGTH`] later.
fn fade(sound: &mut [f32], rate: u32) {
    let start = seconds(FADE_START, rate);
    let length = seconds(FADE_LENGTH, rate).max(1) as f32;
    for (n, sample) in sound.iter_mut().enumerate().skip(start) {
        *sample *= (1.0 - (n - start) as f32 / length).max(0.0);
    }
}

/// `sound` at [`DRY`] plus its copies [`TAPS`] later, all scaled by [`WET`]; the copies
/// read `sound` itself, never the echo, and the result is longer by the longest delay.
fn echo(sound: &[f32], rate: u32) -> Vec<f32> {
    let taps = TAPS.map(|(delay, gain)| (seconds(delay, rate), gain));
    let longest = taps.iter().map(|(delay, _)| *delay).max().unwrap_or(0);
    let at = |n: usize| sound.get(n).copied().unwrap_or(0.0);
    (0..sound.len() + longest)
        .map(|n| {
            let mut sum = DRY * at(n);
            for (delay, gain) in taps {
                if let Some(earlier) = n.checked_sub(delay) {
                    sum += gain * at(earlier);
                }
            }
            sum * WET
        })
        .collect()
}

/// Zero crossings of the interpolation kernel on each side of a sample, counted at the
/// quickened sound's rate.
const KERNEL_ZEROS: f64 = 16.0;
/// The kernel's cut-off, under the quickened sound's Nyquist frequency, so what would
/// fold back above it is filtered out.
const CUTOFF: f64 = 0.97;
/// The kernel's Kaiser window shape.
const KAISER_BETA: f64 = 9.0;

/// `talk` read at the fractional sample `at`, band-limited for reading it [`PITCH`] times
/// faster: a windowed sinc (Kaiser) with its cut-off at [`CUTOFF`] of the quickened
/// sound's Nyquist frequency. A straight line between two samples would dull the
/// sound's highs.
fn read_at(talk: &[f32], at: f64) -> f32 {
    let cutoff = CUTOFF / PITCH;
    let reach = KERNEL_ZEROS / cutoff;
    let first = (at - reach).ceil().max(0.0) as usize;
    let last = ((at + reach).floor().max(0.0) as usize).min(talk.len().saturating_sub(1));
    let window_norm = bessel_i0(KAISER_BETA);
    let mut sum = 0.0;
    for (index, sample) in talk.iter().enumerate().take(last + 1).skip(first) {
        let offset = index as f64 - at;
        let ratio = offset / reach;
        if ratio.abs() > 1.0 {
            continue;
        }
        let window = bessel_i0(KAISER_BETA * (1.0 - ratio * ratio).sqrt()) / window_norm;
        let x = std::f64::consts::PI * cutoff * offset;
        let sinc = if x.abs() < 1e-9 { 1.0 } else { x.sin() / x };
        sum += f64::from(*sample) * cutoff * sinc * window;
    }
    sum as f32
}

/// The modified Bessel function of the first kind, order zero (the Kaiser window's).
fn bessel_i0(x: f64) -> f64 {
    let quarter = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..64 {
        term *= quarter / f64::from(k * k);
        sum += term;
        if term < sum * 1e-12 {
            break;
        }
    }
    sum
}

/// Said once a run: the player's game data has no talk sound to make the variant from.
static MISSING_TOLD: AtomicBool = AtomicBool::new(false);

impl GameAudio {
    /// Make the SJK chat sound from the game data's talk sound, once: later calls find
    /// it among the handles. Without the file there is no sound (said once in the log).
    pub(crate) fn register_sjk_chat_sound(&mut self, vfs: &VirtualFileSystem) {
        if self.handles.contains_key(HANDLE_KEY) {
            return;
        }
        let Some((resolved, asset)) = super::assets::resolve_sound(vfs, TALK) else {
            if !MISSING_TOLD.swap(true, Ordering::Relaxed) {
                crate::log::progress(format_args!(
                    "SJK chat sound: the game data has no {TALK} (or .mp3); SJK chat \
                     messages play no sound"
                ));
            }
            return;
        };
        let handle = sjk_audio::SoundHandle(self.next_handle);
        self.next_handle = self.next_handle.wrapping_add(1);
        let extension = resolved.rsplit('.').next().unwrap_or("wav");
        self.output
            .decode_variant(handle, &asset.bytes, extension, variant);
        self.handles.insert(HANDLE_KEY.to_owned(), handle);
    }

    /// Play the SJK chat sound as the game's chat beep plays: beside the listener, at
    /// full volume under `s_volume`, on its own interface channel (`channel`).
    pub(super) fn play_sjk_chat(&mut self, source: SourceId, channel: ChannelId) {
        self.play_local(HANDLE_KEY, 1.0, source, channel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A sine of `hz` lasting `time` seconds, at amplitude `level`.
    fn sine(hz: f64, time: f64, level: f32) -> Vec<f32> {
        (0..seconds(time, RATE))
            .map(|n| level * (std::f64::consts::TAU * hz * n as f64 / f64::from(RATE)).sin() as f32)
            .collect()
    }

    /// Where a sound first rises through zero after `from`, in fractional samples.
    fn rising(sound: &[f32], from: usize) -> f64 {
        let n = (from..sound.len() - 1)
            .find(|&n| sound[n] <= 0.0 && sound[n + 1] > 0.0)
            .unwrap();
        n as f64 + f64::from(sound[n] / (sound[n] - sound[n + 1]))
    }

    #[test]
    fn a_long_talk_sound_makes_0_74_seconds() {
        let made = variant(&sine(300.0, 1.5, 0.3), RATE);
        // 0.60 s kept and 140 ms of echo after it.
        assert_eq!(made.len(), 26_460 + 6_174);
        assert!((made.len() as f64 / f64::from(RATE) - 0.74).abs() < 1e-3);
    }

    #[test]
    fn a_short_talk_sound_keeps_its_quickened_length_and_the_echo() {
        let made = variant(&sine(300.0, 0.2, 0.3), RATE);
        let quick = (8_820.0_f64 / PITCH).ceil() as usize;
        assert_eq!(made.len(), quick + 6_174);
        assert_eq!(variant(&[], RATE), vec![0.0; 6_174]);
    }

    #[test]
    fn the_talk_sound_plays_1_122_times_faster_and_higher() {
        // 400 Hz read faster is 448.8 Hz: a period of 44100 / 448.8 samples.
        let quick = quicken(&sine(400.0, 1.0, 0.5), usize::MAX);
        assert_eq!(quick.len(), (44_100.0_f64 / PITCH).ceil() as usize);
        let first = rising(&quick, 1_000);
        let tenth = (0..10).fold(first, |at, _| rising(&quick, at as usize + 2));
        let period = (tenth - first) / 10.0;
        assert!(
            (period - 44_100.0 / (400.0 * PITCH)).abs() < 0.01,
            "{period}"
        );
        // Band-limited, not dulled: a high tone keeps its level.
        let bright = quicken(&sine(8_000.0, 0.5, 0.5), usize::MAX);
        let peak = bright[2_000..18_000]
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
        // And it keeps at most what it is asked to.
        assert_eq!(quicken(&sine(400.0, 1.0, 0.5), 100).len(), 100);
    }

    #[test]
    fn the_fade_reaches_silence_at_0_60_seconds_before_the_echo() {
        let mut sound = vec![1.0; 26_460];
        fade(&mut sound, RATE);
        assert_eq!(sound[19_844], 1.0, "full until 0.45 s");
        assert_eq!(sound[19_845], 1.0);
        assert!((sound[19_845 + 3_307] - 0.5).abs() < 1e-3, "half way");
        assert!(sound.windows(2).skip(19_845).all(|pair| pair[1] < pair[0]));
        // Its last sample, just before 0.60 s, is one step from silence.
        assert!((sound[26_459] - 1.0 / 6_615.0).abs() < 1e-6);
        let mut longer = vec![1.0; 30_000];
        fade(&mut longer, RATE);
        assert!(longer[26_460..].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn an_impulse_echoes_at_70_and_140_ms() {
        let mut impulse = vec![0.0; 1_000];
        impulse[0] = 1.0;
        let echoed = echo(&impulse, RATE);
        assert_eq!(echoed.len(), 1_000 + 6_174);
        assert_eq!(echoed[0], DRY * WET);
        assert_eq!(echoed[3_087], 0.3 * WET, "70 ms");
        assert_eq!(echoed[6_174], 0.15 * WET, "140 ms");
        let elsewhere = echoed
            .iter()
            .enumerate()
            .filter(|(n, _)| ![0, 3_087, 6_174].contains(n))
            .all(|(_, sample)| *sample == 0.0);
        assert!(elsewhere, "no feedback, no other tap");
    }

    #[test]
    fn the_level_is_scaled_and_clamped() {
        let talk = sine(300.0, 1.0, 0.2);
        let made = variant(&talk, RATE);
        let mut expected = quicken(&talk, 26_460);
        fade(&mut expected, RATE);
        let expected = echo(&expected, RATE);
        for n in [500, 4_000, 20_000, 30_000] {
            assert!((made[n] - expected[n] * 1.5494).abs() < 1e-6, "{n}");
        }
        // Loud enough to pass 1 once the echoes add up (0.8 + 0.3 + 0.15 of a steady
        // full-scale sound, times 0.6 and 1.5494, is 1.16): clamped.
        let loud = variant(&vec![1.0; 44_100], RATE);
        assert!(loud.iter().all(|sample| (-1.0..=1.0).contains(sample)));
        assert!(loud.contains(&1.0));
    }
}

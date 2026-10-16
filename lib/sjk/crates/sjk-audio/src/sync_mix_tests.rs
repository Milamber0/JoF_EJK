//! Synced emotes' music: copies aligned to the leader's sample, mixed as one.

use super::*;

const RATE: u32 = 44_100;
const EMOTE: ChannelId = ChannelId(13);

fn emote_mixer() -> Mixer {
    Mixer::new(MixerConfig {
        voices: 8,
        loops: 0,
        sample_rate: RATE,
        loop_attenuation: Attenuation::None,
        doppler: DopplerConfig::disabled(),
        protected_channel: Some(EMOTE),
    })
}

fn request(source: u32, channel: ChannelId) -> PlayRequest {
    PlayRequest {
        origin: None,
        source: SourceId(source),
        channel,
        volume: 1.0,
        attenuation: Attenuation::None,
    }
}

fn block(mixer: &mut Mixer) -> Vec<f32> {
    let mut out = vec![0.0; 256];
    mixer.render(&mut out);
    out
}

fn cursor(mixer: &Mixer, source: u32) -> Option<usize> {
    mixer
        .voices
        .iter()
        .find(|voice| voice.active && voice.request.source == SourceId(source))
        .map(|voice| voice.cursor)
}

#[test]
fn a_follower_joins_at_the_leaders_sample_and_the_pair_sounds_as_one() {
    let mut mixer = emote_mixer();
    // A ramp, so a misaligned copy would show.
    let ramp: Vec<f32> = (0..20_000).map(|i| (i % 1_000) as f32 / 4_000.0).collect();
    let song = mixer.sound_bank_mut().register_pcm(&ramp, RATE);
    mixer.play(song, request(1, EMOTE));
    let _ = block(&mut mixer);
    let leader_at = cursor(&mixer, 1).unwrap();
    // The clock says 140 samples; the leader really is at 128: the copy takes 128.
    assert!(mixer.play_aligned(song, request(2, EMOTE), 140, SourceId(1), 22_050));
    assert_eq!(cursor(&mixer, 2), Some(leader_at));
    let together = block(&mut mixer);
    // Mixed once: as loud as the leader alone would be, not twice.
    let mut solo = emote_mixer();
    let song = solo.sound_bank_mut().register_pcm(&ramp, RATE);
    solo.play(song, request(1, EMOTE));
    let _ = block(&mut solo);
    assert_eq!(together, block(&mut solo));
    assert_eq!(
        cursor(&mixer, 1),
        cursor(&mixer, 2),
        "they move on together"
    );
}

#[test]
fn a_leader_too_far_off_is_not_copied_and_other_channels_still_add_up() {
    let mut mixer = emote_mixer();
    let song = mixer.sound_bank_mut().register_pcm(&[0.25; 20_000], RATE);
    mixer.play(song, request(1, EMOTE));
    let _ = block(&mut mixer);
    // Another round of a loop: the clock's offset wins.
    assert!(mixer.play_aligned(song, request(2, EMOTE), 15_000, SourceId(1), 1_000));
    assert_eq!(cursor(&mixer, 2), Some(15_000));
    // The same sound at the same sample on another channel is not merged.
    let mut plain = emote_mixer();
    let tone = plain.sound_bank_mut().register_pcm(&[0.25; 20_000], RATE);
    plain.play(tone, request(1, ChannelId(1)));
    plain.play(tone, request(2, ChannelId(1)));
    assert!((block(&mut plain)[0] - 0.5).abs() < 1e-6);
}

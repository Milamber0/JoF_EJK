//! Which voice a full table gives up for a new sound.

use super::*;

const RATE: u32 = 44_100;
/// The channel these tests protect (an emote's music in the client).
const PROTECTED: ChannelId = ChannelId(13);

fn mixer() -> Mixer {
    Mixer::new(MixerConfig {
        voices: 4,
        loops: 0,
        sample_rate: RATE,
        loop_attenuation: Attenuation::None,
        doppler: DopplerConfig::disabled(),
        protected_channel: Some(PROTECTED),
    })
}

fn play(mixer: &mut Mixer, sound: SoundHandle, source: u32, channel: u32) {
    assert!(mixer.play(
        sound,
        PlayRequest {
            origin: None,
            source: SourceId(source),
            channel: ChannelId(channel),
            volume: 1.0,
            attenuation: Attenuation::None,
        },
    ));
}

fn plays(mixer: &Mixer, source: u32) -> bool {
    mixer
        .voices
        .iter()
        .any(|voice| voice.active && voice.request.source == SourceId(source))
}

#[test]
fn a_full_table_keeps_the_protected_voice_even_when_it_is_oldest() {
    let mut mixer = mixer();
    let sound = mixer.sound_bank_mut().register_pcm(&[0.25; 4_096], RATE);
    play(&mut mixer, sound, 1, PROTECTED.0);
    for source in 2..=5 {
        play(&mut mixer, sound, source, 0);
    }
    assert_eq!(mixer.active_voice_count(), 4);
    // The emote, oldest of all, still plays; the oldest plain sound went.
    assert!(plays(&mixer, 1));
    assert!(!plays(&mixer, 2));
    assert!((3..=5).all(|source| plays(&mixer, source)));
}

#[test]
fn protected_voices_are_stolen_when_nothing_else_is_left() {
    let mut mixer = mixer();
    let sound = mixer.sound_bank_mut().register_pcm(&[0.25; 4_096], RATE);
    for source in 1..=4 {
        play(&mut mixer, sound, source, PROTECTED.0);
    }
    play(&mut mixer, sound, 5, 0);
    assert_eq!(mixer.active_voice_count(), 4);
    // The oldest protected voice made room.
    assert!(!plays(&mixer, 1));
    assert!((2..=5).all(|source| plays(&mixer, source)));
}

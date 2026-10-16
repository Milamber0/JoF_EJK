//! Loop sets rebuilt by a producer thread while the mixer keeps rendering.

use super::*;

const RATE: u32 = 44_100;
/// Interleaved stereo samples in one rendered block.
const BLOCK: usize = 256;

fn mixer(loops: usize) -> Mixer {
    Mixer::new(MixerConfig {
        voices: 4,
        loops,
        sample_rate: RATE,
        loop_attenuation: Attenuation::None,
        doppler: DopplerConfig::disabled(),
        protected_channel: None,
    })
}

fn tone(mixer: &mut Mixer) -> SoundHandle {
    mixer.sound_bank_mut().register_pcm(&[0.25; 64], RATE)
}

fn loudest(mixer: &mut Mixer) -> f32 {
    let mut out = [0.0; BLOCK];
    mixer.render(&mut out);
    out.iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
}

#[test]
fn loops_keep_sounding_while_a_frame_is_rebuilt() {
    let mut mixer = mixer(4);
    let hum = tone(&mut mixer);
    mixer.clear_loops();
    mixer.set_loop(SourceId(1), hum, [0.0; 3], [0.0; 3]);
    mixer.commit_loops();
    assert!(loudest(&mut mixer) > 0.0);

    // The audio thread renders after the next frame's clear but before its
    // loops arrive: the hum must not drop out for that block.
    mixer.clear_loops();
    assert!(loudest(&mut mixer) > 0.0);
    mixer.set_loop(SourceId(1), hum, [0.0; 3], [0.0; 3]);
    mixer.commit_loops();
    assert!(loudest(&mut mixer) > 0.0);
    assert_eq!(mixer.active_loop_count(), 1);
}

#[test]
fn loops_missing_from_a_committed_frame_stop() {
    let mut mixer = mixer(4);
    let hum = tone(&mut mixer);
    mixer.clear_loops();
    mixer.set_loop(SourceId(1), hum, [0.0; 3], [0.0; 3]);
    mixer.commit_loops();
    loudest(&mut mixer);

    mixer.clear_loops();
    mixer.commit_loops();
    assert_eq!(loudest(&mut mixer), 0.0);
    assert_eq!(mixer.active_loop_count(), 0);
}

#[test]
fn new_loops_do_not_take_slots_still_awaiting_their_frame() {
    let mut mixer = mixer(3);
    let first = tone(&mut mixer);
    let second = tone(&mut mixer);
    let third = tone(&mut mixer);
    mixer.clear_loops();
    mixer.set_loop(SourceId(1), first, [0.0; 3], [0.0; 3]);
    mixer.set_loop(SourceId(2), second, [0.0; 3], [0.0; 3]);
    mixer.commit_loops();
    loudest(&mut mixer);

    mixer.clear_loops();
    mixer.set_loop(SourceId(3), third, [0.0; 3], [0.0; 3]);
    loudest(&mut mixer);
    assert_eq!(mixer.active_loop_count(), 3);
    mixer.set_loop(SourceId(1), first, [0.0; 3], [0.0; 3]);
    mixer.set_loop(SourceId(2), second, [0.0; 3], [0.0; 3]);
    mixer.commit_loops();
    loudest(&mut mixer);
    assert_eq!(mixer.active_loop_count(), 3);
}

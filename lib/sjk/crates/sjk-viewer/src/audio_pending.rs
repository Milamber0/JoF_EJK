//! Bounded first-use starts waiting for the asynchronous decoder. A handle is
//! reserved before its PCM reaches the callback; dropping Play at that point
//! silently lost jump/weapon sounds. Expire old events instead of replaying a
//! loading backlog. All storage is reserved before the callback starts.
use super::*;
use std::collections::VecDeque;

pub(super) struct PendingStarts {
    starts: VecDeque<(SoundHandle, PlayRequest, u64)>,
    block: u64,
}

impl PendingStarts {
    pub fn new() -> Self {
        Self {
            starts: VecDeque::with_capacity(VOICES),
            block: 0,
        }
    }

    pub fn play(&mut self, mixer: &mut Mixer, handle: SoundHandle, request: PlayRequest) {
        if request.channel != ChannelId(0) {
            self.cancel(request.source, request.channel);
        }
        if mixer.sound_bank().sample_count(handle).is_some() {
            mixer.play(handle, request);
        } else {
            if self.starts.len() == VOICES {
                self.starts.pop_front();
            }
            self.starts.push_back((handle, request, self.block));
        }
    }

    pub fn cancel(&mut self, source: SourceId, channel: ChannelId) {
        self.starts
            .retain(|(_, r, _)| r.source != source || r.channel != channel);
    }

    pub fn clear(&mut self) {
        self.starts.clear();
    }

    pub fn flush(&mut self, mixer: &mut Mixer) {
        // Event validity window (codemp EVENT_VALID_MSEC): 300 ms.
        let maximum_age = u64::from(SAMPLE_RATE) * 300 / 1000 / (MIX_SAMPLES as u64 / 2);
        self.starts.retain(|(handle, request, queued)| {
            if self.block.saturating_sub(*queued) > maximum_age {
                return false;
            }
            if mixer.sound_bank().sample_count(*handle).is_some() {
                mixer.play(*handle, *request);
                false
            } else {
                true
            }
        });
        self.block = self.block.wrapping_add(1);
    }
}

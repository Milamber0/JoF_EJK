//! Viewer-side combat voice policy; the mixer remains game independent.
use super::*;

/// Registered on the existing asset worker, not during event dispatch.
pub(super) const KILL_SOUNDS: [&str; 2] = ["sound/frag/frag.wav", "sound/frag/middy.wav"];

/// TaystJK cg_event.c:270-285: local kills, with a restricted midair weapon set.
pub(super) fn kill_sound(
    mode: i64,
    local_fragged: bool,
    airborne: bool,
    means: u8,
) -> Option<&'static str> {
    if mode == 0 || !local_fragged {
        return None;
    }
    let midair = mode > 1 && airborne && matches!(means, 3 | 11 | 13 | 19 | 29);
    Some(KILL_SOUNDS[usize::from(midair)])
}

impl GameAudio {
    /// Consume the shared obituary tracker's newly accepted event, never re-scan snapshots.
    pub(crate) fn kill_cue(&mut self, event: sjk_client::ObituaryEvent, snapshot: &Snapshot) {
        let airborne = snapshot
            .entities
            .iter()
            .find(|entity| entity.number() == event.target)
            .is_some_and(|entity| entity.ground_entity_num() == 1023);
        if let Some(path) = kill_sound(
            self.kill_sounds,
            event.local_fragged,
            airborne,
            event.means_of_death,
        ) {
            self.play_local(
                path,
                1.0,
                SourceId(u32::from(snapshot.player.client_num())),
                ChannelId(8),
            );
        }
    }
}

/// Retained jump and roll voice switches (TaystJK cg_event.c:1812-1829,1859-1874).
#[derive(Clone, Copy)]
pub(super) struct VoicePolicy {
    jump: i64,
    roll: i64,
}

impl Default for VoicePolicy {
    fn default() -> Self {
        Self { jump: 0, roll: 1 }
    }
}

impl VoicePolicy {
    /// Sample options outside the event loop using allocation-free lowercase lookup.
    pub(super) fn sample(&mut self, console: Option<&ViewerConsole>) {
        self.jump = console
            .and_then(|c| c.integer_cvar("cg_jumpsounds"))
            .unwrap_or(0);
        self.roll = console
            .and_then(|c| c.integer_cvar("cg_rollsounds"))
            .unwrap_or(1);
    }

    /// Filter voice requests only: a roll's body impact always remains audible.
    pub(super) fn allows(self, event: sjk_client::LegacySoundDecision, local: u16) -> bool {
        let mode = match event.event {
            sjk_client::LegacySoundEvent::Jump => self.jump,
            sjk_client::LegacySoundEvent::Roll if event.request.channel == ChannelId(3) => {
                self.roll
            }
            _ => return true,
        };
        match mode {
            1 => true,
            2 => event.request.source != SourceId(u32::from(local)),
            3 => event.request.source == SourceId(u32::from(local)),
            _ => false,
        }
    }
}

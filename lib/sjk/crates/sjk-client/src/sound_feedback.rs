//! Access to the existing custom-player pain bank for a caller-owned transition policy.
use super::*;

impl LegacySoundAdapter {
    /// Use health transitions instead of local EV_PAIN; remote pain remains event-driven.
    pub fn set_local_health_pain(&mut self, enabled: bool) {
        self.local_health_pain = enabled;
    }

    /// Resolve CG_PainEvent using the existing shared 500ms deadline and custom sound bank.
    /// TaystJK codemp/cgame/cg_event.c:700-724. Does not re-observe an entity event.
    pub fn local_health_pain(&mut self, snapshot: &Snapshot) -> Option<LegacySoundDecision> {
        let client = usize::from(snapshot.player.client_num());
        if client >= self.custom.len() || self.pain_deadline[client] > snapshot.server_time {
            return None;
        }
        self.pain_deadline[client] = snapshot.server_time.saturating_add(500);
        let band = match snapshot.player.health() {
            ..25 => 0,
            25..50 => 1,
            50..75 => 2,
            _ => 3,
        };
        let sound = self.custom[client].pain[band];
        let handle = sound
            .and_then(|index| self.sounds.get(index as usize))?
            .handle;
        Some(LegacySoundDecision {
            event: LegacySoundEvent::Pain,
            sound,
            handle,
            additional: false,
            cause: None,
            request: PlayRequest {
                origin: None,
                source: SourceId(client as u32),
                channel: ChannelId(CHAN_VOICE),
                volume: 1.0,
                attenuation: Attenuation::None,
            },
        })
    }
}

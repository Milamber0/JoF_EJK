//! Snapshot transitions for local feedback, not a second event or mixer system.
use super::*;

/// TaystJK cg_main.c:878-882 and cg_event.c:2509-2512; alternate formats use VFS fallback.
pub(super) const CUES: [&str; 9] = [
    "sound/effects/hitsound.wav",
    "sound/effects/hitsound2.wav",
    "sound/effects/hitsound3.wav",
    "sound/effects/hitsound4.wav",
    "sound/effects/hitsoundteam.wav",
    "sound/weapons/saber/saberhit.wav",
    "sound/weapons/saber/saberhit1.wav",
    "sound/weapons/saber/saberhit2.wav",
    "sound/weapons/saber/saberhit3.wav",
];

#[derive(Clone, Copy)]
struct Previous {
    client: u16,
    team: u32,
    hits: i32,
    health: i32,
    time: i32,
}

/// Shared, fixed-size previous-playerstate cache. Options are sampled outside traversal.
pub(super) struct Transitions {
    previous: Option<Previous>,
    /// Sampled hit confirmation/saber-impact mode.
    pub(super) hits: i64,
    /// Select health transitions rather than local pain events.
    pub(super) old_pain: bool,
    /// Independent private-duel text/audio mode.
    pub(super) duel: i64,
    /// Current jaPRO race restriction.
    pub(super) race: bool,
    /// Resolved on the asset worker completion, never during snapshot observation.
    pub(super) handles: [Option<SoundHandle>; 9],
}

impl Default for Transitions {
    fn default() -> Self {
        Self {
            previous: None,
            hits: 0,
            old_pain: false,
            duel: 1,
            race: false,
            handles: [None; 9],
        }
    }
}

impl Transitions {
    /// Forget the previous map without losing the sampled user options.
    pub(super) fn reset(&mut self) {
        self.previous = None;
        self.handles.fill(None);
    }
    /// Compare signed PERS_HITS and health after excluding initial/team/follow/time resets.
    pub(super) fn observe(&mut self, snapshot: &Snapshot) -> (Option<usize>, bool) {
        let player = &snapshot.player;
        let now = Previous {
            client: player.client_num(),
            team: player.persistent[3],
            hits: player.persistent[1] as i32,
            health: player.health(),
            time: snapshot.server_time,
        };
        let previous = self.previous.replace(now);
        let Some(old) = previous else {
            return (None, false);
        };
        if old.client != now.client || old.team != now.team || now.time <= old.time {
            return (None, false);
        }
        let hit = if now.hits > old.hits && (1..=4).contains(&self.hits) {
            Some(self.hits as usize - 1)
        } else if now.hits < old.hits && self.hits != 0 {
            Some(4)
        } else {
            None
        };
        (
            hit,
            self.old_pain && now.health > 0 && old.health.saturating_sub(now.health) > 3,
        )
    }

    /// Select legacy saber variation without changing event origin, source or channel.
    pub(super) fn handle(
        &self,
        decision: &sjk_client::LegacySoundDecision,
        time: i32,
    ) -> Option<SoundHandle> {
        if decision.event == sjk_client::LegacySoundEvent::PrivateDuel
            && (self.duel == 0 || self.duel == 3 || self.race)
        {
            return None;
        }
        if decision.event != sjk_client::LegacySoundEvent::SaberHit {
            return decision.handle;
        }
        match self.hits {
            5 => self.handles[5],
            6 => {
                // Deterministic presentation variation; no simulation RNG is consumed.
                let seed = (time as u32).wrapping_mul(1664525)
                    ^ decision.request.source.0.wrapping_mul(1013904223);
                self.handles[5 + ((seed ^ (seed >> 16)) & 3) as usize]
            }
            _ => decision.handle,
        }
    }
}

impl GameAudio {
    /// Advance latches even for loading history; only the current snapshot may start cues.
    pub(super) fn transition_cues(&mut self, snapshot: &Snapshot, play: bool) {
        let (hit, pain) = self.transitions.observe(snapshot);
        if !play {
            return;
        }
        if let Some(handle) = hit.and_then(|index| self.transitions.handles[index]) {
            self.output.send(AudioCommand::Play(
                handle,
                PlayRequest {
                    origin: None,
                    source: SourceId(u32::from(snapshot.player.client_num())),
                    channel: ChannelId(8),
                    volume: 1.0,
                    attenuation: sjk_audio::Attenuation::None,
                },
            ));
        }
        if pain
            && let Some(adapter) = &mut self.legacy
            && let Some(decision) = adapter.local_health_pain(snapshot)
            && let Some(handle) = decision.handle
        {
            self.output
                .send(AudioCommand::Play(handle, decision.request));
        }
    }
}

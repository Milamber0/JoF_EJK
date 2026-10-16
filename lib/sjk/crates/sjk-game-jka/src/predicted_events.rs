//! Bounded predictable-event transport and CG_CheckPlayerstateEvents reconciliation.
//! codemp/cgame/cg_playerstate.c:224-289; game/bg_misc.c:2511-2540.

/// One client-predicted event, identified by the playerstate event sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PredictedEvent {
    /// Weapon at the event moment, before a later authoritative snapshot arrives.
    pub weapon: u8,
    /// Zoom mode at the event moment, selecting the correct start/end sound.
    pub zoom_mode: u8,
    pub sequence: u16,
    pub event: u16,
    pub parameter: u16,
    pub command_time: i32,
    pub client: u16,
    pub entity_flags: u32,
    pub origin: [f32; 3],
}

/// Fixed storage; neither Pmove nor snapshot replay allocates for events.
#[derive(Clone, Debug, PartialEq)]
pub struct PredictedEvents {
    entries: [PredictedEvent; 64],
    len: usize,
    start: usize,
}

impl Default for PredictedEvents {
    fn default() -> Self {
        Self {
            entries: [PredictedEvent::default(); 64],
            len: 0,
            start: 0,
        }
    }
}

impl PredictedEvents {
    /// Retain the newest bounded transition window, like cgame's event history.
    pub fn push(&mut self, event: PredictedEvent) {
        if self.len == self.entries.len() {
            self.start = (self.start + 1) & 63;
            self.len -= 1;
        }
        self.entries[(self.start + self.len) & 63] = event;
        self.len += 1;
    }
    pub fn clear(&mut self) {
        self.len = 0;
        self.start = 0;
    }
    pub fn iter(&self) -> impl Iterator<Item = PredictedEvent> + '_ {
        (0..self.len).map(|index| self.entries[(self.start + index) & 63])
    }
}

/// Shared sound/view policy: play a predicted event once, but accept corrections.
#[derive(Clone, Debug, PartialEq)]
pub struct PredictedEventLedger {
    identity: Option<(u16, u32)>,
    entries: [Option<(u16, u16)>; 64],
}

impl Default for PredictedEventLedger {
    fn default() -> Self {
        Self {
            identity: None,
            entries: [None; 64],
        }
    }
}

impl PredictedEventLedger {
    /// Check a local entity's copy of an already-delivered playerstate event.
    pub fn contains(&self, sequence: u16, event: u16, _parameter: u16) -> bool {
        self.entries[usize::from(sequence) & 63] == Some((sequence, event))
    }
    /// Clear history on client changes or teleport epochs, not on acknowledgements.
    pub fn identity(&mut self, client: u16, flags: u32) {
        let identity = (client, flags & crate::EF_TELEPORT_BIT);
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.entries.fill(None);
        }
    }
    /// The same sequence/type is a replay or acknowledgement, not a new sound.
    /// Sequence is modulo 65536, matching the existing protocol-26 field.
    pub fn accept(&mut self, sequence: u16, event: u16, _parameter: u16) -> bool {
        // CG_CheckChangedPredictableEvents compares event type, not parameter:
        // recomputed fall strength must not restart a landing already heard.
        let entry = (sequence, event);
        let slot = &mut self.entries[usize::from(sequence) & 63];
        if *slot == Some(entry) {
            return false;
        }
        *slot = Some(entry);
        true
    }
}

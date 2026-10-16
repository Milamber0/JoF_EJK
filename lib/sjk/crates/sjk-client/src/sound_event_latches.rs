//! Event lifetimes from codemp CG_CheckEvents/CG_ResetEntity and
//! CG_CheckPlayerstateEvents. Reused temporary entity slots are not permanent
//! sound IDs, and an event clearing to zero must re-arm ordinary entities.
use super::*;

impl LegacySoundAdapter {
    /// Resolve predicted movement immediately using the existing registered sound tables.
    pub fn observe_predicted_event(
        &mut self,
        event: crate::predicted_events::PredictedEvent,
        snapshot: &Snapshot,
    ) {
        self.decisions.clear();
        self.predicted_events
            .identity(event.client, event.entity_flags);
        if !self
            .predicted_events
            .accept(event.sequence, event.event, event.parameter)
        {
            return;
        }
        let mut subject = EventSubject::player(snapshot);
        subject.weapon = event.weapon;
        subject.predicted_zoom = Some(event.zoom_mode);
        subject.origin = event.origin;
        subject.fixed_origin = event.origin;
        self.resolve(
            event.event,
            sound_parameter(event.event, event.parameter),
            subject,
            snapshot,
            event.command_time,
        );
    }
    pub(super) fn observe_entity_events(&mut self, snapshot: &Snapshot) {
        let previous_epoch = self.epoch;
        self.epoch = self.epoch.wrapping_add(1).max(1);
        for entity in &snapshot.entities {
            let number = usize::from(entity.number());
            if number >= MAX_ENTITIES {
                continue;
            }
            // CG_AddPacketEntities (codemp/cgame/cg_ents.c) does not re-add
            // the view client's ordinary snapshot entity. Its events come
            // from prediction/CG_CheckPlayerstateEvents instead, including
            // when prediction has not emitted the sound yet (or in demos).
            // Event-only entities retain their separate server lifetime.
            if entity.number() == snapshot.player.client_num() && entity.entity_type() <= ET_EVENTS
            {
                continue;
            }
            if self.seen_entity[number] != previous_epoch
                && self.last_entity_time[number] < snapshot.server_time.saturating_sub(300)
            {
                self.previous_entity_event[number] = 0;
            }
            self.seen_entity[number] = self.epoch;
            self.last_entity_time[number] = snapshot.server_time;
            let mut subject = EventSubject::entity(entity);
            let event = if entity.entity_type() > ET_EVENTS {
                if self.previous_entity_event[number] != 0 {
                    continue;
                }
                self.previous_entity_event[number] = 1;
                if entity.e_flags() & (1 << 5) != 0 {
                    // EF_PLAYER_EVENT
                    subject.number = entity.other_entity_num();
                    subject.client_num = subject.number;
                }
                u16::from(entity.entity_type() - ET_EVENTS)
            } else {
                let raw = entity.event();
                let previous = self.previous_entity_event[number];
                self.previous_entity_event[number] = raw;
                if previous == raw || raw & 0xff == 0 {
                    continue;
                }
                raw & 0xff
            };
            // BG_PlayerStateToEntityState includes a two-bit sequence tag.
            // If our own entity repeats a predicted ring event, it is not a
            // second sound. Remote entity latches remain completely separate.
            if subject.number == snapshot.player.client_num()
                && (snapshot.player.event_sequence() - 2..snapshot.player.event_sequence()).any(
                    |sequence| {
                        let slot = (sequence & 1) as usize;
                        snapshot.player.event(slot).unwrap_or(0) & 0xff == event
                            && ((sequence as u16 & 3) << 8) == entity.event() & 0x300
                            && self.predicted_events.contains(
                                sequence as u16,
                                event,
                                snapshot.player.event_parameter(slot).unwrap_or(0),
                            )
                    },
                )
            {
                continue;
            }
            self.resolve(
                event,
                u16::from(entity.event_parameter()),
                subject,
                snapshot,
                snapshot.server_time,
            );
        }
    }

    pub(super) fn observe_player_events(&mut self, snapshot: &Snapshot) {
        self.predicted_events
            .identity(snapshot.player.client_num(), snapshot.player.entity_flags());
        // External events precede the predictable ring in CG_CheckPlayerstateEvents.
        let external = snapshot.player.external_event();
        if external != 0 && external != self.player_external_event {
            self.resolve(
                external & 0xff,
                u16::from(snapshot.player.external_event_parameter()),
                EventSubject::player(snapshot),
                snapshot,
                snapshot.server_time,
            );
        }
        self.player_external_event = external;
        let sequence = snapshot.player.event_sequence();
        let events = [0, 1].map(|i| snapshot.player.event(i).unwrap_or(0));
        let previous = self.player_event_sequence;
        // Only eventSequence is truncated to 16 bits on the wire. Preserve
        // adjacent ring events at rollover without replaying older slots.
        let previous = if previous > 65533 && sequence < 2 {
            previous - 65536
        } else {
            previous
        };
        for i in sequence - 2..sequence {
            let slot = (i & 1) as usize;
            if i >= previous || (i > previous - 2 && events[slot] != self.player_events[slot]) {
                let parameter = snapshot.player.event_parameter(slot).unwrap_or(0);
                if !self
                    .predicted_events
                    .accept(i as u16, events[slot] & 0xff, parameter)
                {
                    continue;
                }
                self.resolve(
                    events[slot] & 0xff,
                    sound_parameter(events[slot] & 0xff, parameter),
                    EventSubject::player(snapshot),
                    snapshot,
                    snapshot.server_time,
                );
            }
        }
        self.player_events = events;
        self.player_event_sequence = sequence;
    }
}

// Fall and roll consumers compare strength against small thresholds. Preserve
// that ordering for a predicted (or even-slot) strength above 255.
fn sound_parameter(event: u16, parameter: u16) -> u16 {
    if matches!(event, 11 | 12) {
        parameter.min(255)
    } else {
        parameter
    }
}

//! Event-maintained actor-overlay timers.

use crate::LegacyTeamPowerEffect;
use sjk_protocol::{EntityState, Snapshot};

#[path = "shield_hit.rs"]
mod shield_hit;
pub use shield_hit::LegacyShieldHit;

const ENTITY_LIMIT: usize = 1_024;
const CLIENT_LIMIT: u16 = 64;
const ET_EVENTS: u8 = 18;
const EVENT_MASK: u16 = 0xff;
const EV_PREDEFSOUND: u16 = 40;
const EV_TEAM_POWER: u16 = 41;
const EV_FORCE_DRAINED: u16 = 96;
const PDSOUND_ABSORBHIT: u8 = 3;

/// Fixed per-entity state for event-maintained team-power shells.
///
/// This mirrors the updates in OpenJK `codemp/cgame/cg_event.c:2529-2537`,
/// `2559-2580`, and `3361-3367`. Storage is fixed so snapshot observation
/// never allocates.
pub struct LegacyForceOverlayTracker {
    effects: [LegacyTeamPowerEffect; ENTITY_LIMIT],
    signatures: [u16; ENTITY_LIMIT],
    active: [bool; ENTITY_LIMIT],
    shields: [LegacyShieldHit; ENTITY_LIMIT],
}

impl Default for LegacyForceOverlayTracker {
    fn default() -> Self {
        Self {
            effects: [LegacyTeamPowerEffect::default(); ENTITY_LIMIT],
            signatures: [0; ENTITY_LIMIT],
            active: [false; ENTITY_LIMIT],
            shields: [LegacyShieldHit::default(); ENTITY_LIMIT],
        }
    }
}

impl LegacyForceOverlayTracker {
    /// Direction and expiry captured by EV_SHIELD_HIT for this entity.
    pub fn shield(&self, entity: u16) -> LegacyShieldHit {
        self.shields
            .get(usize::from(entity))
            .copied()
            .unwrap_or_default()
    }
    /// Observe event toggles once, matching `CG_EntityEvent` timer updates.
    pub fn observe_snapshot(&mut self, snapshot: &Snapshot) {
        let mut present = [false; ENTITY_LIMIT];
        for entity in &snapshot.entities {
            let index = usize::from(entity.number());
            if index >= ENTITY_LIMIT {
                continue;
            }
            present[index] = true;
            let signature = if entity.entity_type() >= ET_EVENTS {
                u16::from(entity.entity_type())
            } else {
                entity.event()
            };
            if signature == 0 || (self.active[index] && self.signatures[index] == signature) {
                continue;
            }
            self.active[index] = true;
            self.signatures[index] = signature;
            let event = if entity.entity_type() >= ET_EVENTS {
                u16::from(entity.entity_type() - ET_EVENTS)
            } else {
                signature & EVENT_MASK
            };
            self.observe_event(entity, event, snapshot.server_time);
        }
        for index in 0..ENTITY_LIMIT {
            if self.active[index] && !present[index] {
                self.active[index] = false;
            }
        }
    }

    /// Current event-maintained shell state for one client number.
    pub fn effect(&self, client: u16) -> LegacyTeamPowerEffect {
        self.effects
            .get(usize::from(client))
            .copied()
            .unwrap_or_default()
    }

    fn observe_event(&mut self, entity: &EntityState, event: u16, now: i32) {
        match event {
            110 => {
                if let Some(shield) = self.shields.get_mut(usize::from(entity.other_entity_num())) {
                    // time2 is protocol-26 netfield 61; no codec change.
                    shield.hit(
                        now,
                        entity.integer_field(61).unwrap_or(0),
                        crate::legacy_byte_to_direction(entity.event_parameter()),
                    );
                }
            }
            EV_PREDEFSOUND if entity.event_parameter() == PDSOUND_ABSORBHIT => {
                self.set(entity.tracked_entity_num(), now, 3);
            }
            EV_TEAM_POWER => {
                let kind = u8::from(entity.event_parameter() == 1);
                for client in 0..CLIENT_LIMIT {
                    if entity.client_bitflag(client) {
                        self.set(client, now, kind);
                    }
                }
            }
            EV_FORCE_DRAINED => self.set(entity.owner(), now, 2),
            _ => {}
        }
    }

    fn set(&mut self, client: u16, now: i32, kind: u8) {
        if let Some(effect) = self.effects.get_mut(usize::from(client)) {
            *effect = LegacyTeamPowerEffect {
                until: now.saturating_add(1_000),
                kind,
            };
        }
    }
}

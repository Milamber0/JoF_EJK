//! Reusable snapshot projection for an in-process client; no encoding or transport.
use super::{EntityList, LegacySnapshotFrame, MAX_AREA_BYTES};
use crate::LegacyGameHost;
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState, Snapshot};

/// Storage for projecting the normal game host into a local client's snapshot.
/// Shares the server's projection callback, entity ordering and visibility policy.
pub struct LocalSnapshotBuffer {
    ring: Vec<EntityState>,
    spare: Vec<EntityState>,
    vehicle: PlayerState,
}
impl Default for LocalSnapshotBuffer {
    fn default() -> Self {
        Self::new()
    }
}
impl LocalSnapshotBuffer {
    /// Allocate the legacy projection budget once, outside gameplay frames.
    pub fn new() -> Self {
        let entries = || {
            (0..super::LEGACY_SNAPSHOT_ENTITIES)
                .map(|_| EntityState::zero(0, &LEGACY_ENTITY_FIELDS))
                .collect()
        };
        Self {
            ring: entries(),
            spare: entries(),
            vehicle: PlayerState::zero(),
        }
    }

    /// Fill the client-owned snapshot using its existing entity allocations.
    /// The caller supplies monotonically advancing local time and message sequence.
    pub fn capture(&mut self, host: &impl LegacyGameHost, client: usize, snapshot: &mut Snapshot) {
        let mut list = EntityList {
            area: [0; MAX_AREA_BYTES],
            area_bytes: 0,
            first_entity: 0,
            entities: 0,
        };
        let mut next = 0;
        host.build_snapshot(
            client,
            &mut LegacySnapshotFrame {
                player: &mut snapshot.player,
                vehicle: &mut self.vehicle,
                list: &mut list,
                ring: &mut self.ring,
                next_entity: &mut next,
            },
        );
        while snapshot.entities.len() > list.entities {
            self.spare.push(snapshot.entities.pop().unwrap());
        }
        while snapshot.entities.len() < list.entities {
            snapshot
                .entities
                .push(self.spare.pop().expect("local snapshot budget"));
        }
        for (out, source) in snapshot.entities.iter_mut().zip(&self.ring) {
            out.copy_from(source);
        }
        snapshot.area_mask.clear();
        snapshot
            .area_mask
            .extend_from_slice(&list.area[..list.area_bytes]);
        if snapshot.player.vehicle_entity_num() != 0 {
            snapshot
                .vehicle_player
                .get_or_insert_with(PlayerState::zero)
                .copy_from(&self.vehicle);
        } else {
            snapshot.vehicle_player = None;
        }
    }
}

//! Snapshot history and delta-base selection: `SV_WriteSnapshotToClient`.
use sjk_protocol::{
    EntityState, LEGACY_ENTITY_FIELDS, MessageWriter, PlayerState, SnapshotHeader, SnapshotPlayer,
    SnapshotWriteError, SnapshotWriter,
};

/// `PACKET_BACKUP`: messages a client may still ask to delta from.
const FRAMES: usize = 32;
/// `MAX_SNAPSHOT_ENTITIES`: entities one snapshot can list.
pub const LEGACY_SNAPSHOT_ENTITIES: usize = 256;
const MAX_AREA_BYTES: usize = 32;
/// `SNAPFLAG_RATE_DELAYED` and `SNAPFLAG_NOT_ACTIVE`.
const RATE_DELAYED: u8 = 1;
const NOT_ACTIVE: u8 = 2;

#[derive(Clone)]
struct Frame {
    player: PlayerState,
    /// `frame->vps`: sent only while `player` rides a vehicle.
    vehicle: PlayerState,
    list: EntityList,
}
impl Frame {
    fn sent(&self) -> SnapshotPlayer<'_> {
        SnapshotPlayer {
            player: &self.player,
            vehicle: Some(&self.vehicle),
        }
    }
}

/// Where a frame's entities and area bits are; separate from the player state so
/// the game can be lent both at once.
#[derive(Clone)]
struct EntityList {
    area: [u8; MAX_AREA_BYTES],
    area_bytes: usize,
    /// Position of the first entity in the ring, counted without wrapping.
    first_entity: u64,
    entities: usize,
}

/// What one client was sent in its last 32 messages, in storage allocated once.
///
/// The reference shares one entity ring among all clients; here every wire client
/// number has its own, so one client's crowded view cannot age another's delta
/// bases. The rule for a base gone stale is the reference's.
pub(super) struct SnapshotHistory {
    frames: Vec<Frame>,
    ring: Vec<EntityState>,
    next_entity: u64,
    writer: SnapshotWriter,
}

/// One client's view of the world for one message, filled in by the game.
///
/// This is where a native world is projected onto what protocol 26 can show:
/// which entities this client sees, under which wire entity numbers. Entities must
/// be pushed in strictly increasing number order.
pub struct LegacySnapshotFrame<'a> {
    /// The client's player state. It holds what this history slot held 32
    /// messages ago: overwrite all of it.
    pub player: &'a mut PlayerState,
    /// The ridden vehicle's player state (`frame->vps`), sent only while `player`'s
    /// `m_iVehicleNum` is set. Like the reference's `SV_BuildClientSnapshot`
    /// (`sv_snapshot.cpp:579-591`) nothing clears it: it holds what this slot held
    /// 32 messages ago until the game overwrites it, which it must do whenever
    /// the player rides a vehicle it can find.
    pub vehicle: &'a mut PlayerState,
    list: &'a mut EntityList,
    ring: &'a mut [EntityState],
    next_entity: &'a mut u64,
}

/// An entity the frame could not take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacySnapshotRefusal {
    /// The snapshot already lists [`LEGACY_SNAPSHOT_ENTITIES`] entities.
    Full,
    /// Its number is not above the previous entity's.
    Order,
}

impl LegacySnapshotFrame<'_> {
    /// List an entity. A refusal leaves the frame as it was: the game decides what
    /// a legacy client does without, it is never dropped behind its back.
    pub fn push(&mut self, entity: &EntityState) -> Result<(), LegacySnapshotRefusal> {
        if self.list.entities == LEGACY_SNAPSHOT_ENTITIES.min(self.ring.len()) {
            return Err(LegacySnapshotRefusal::Full);
        }
        let ring = self.ring.len() as u64;
        if self.list.entities > 0
            && self.ring[((*self.next_entity - 1) % ring) as usize].number() >= entity.number()
        {
            return Err(LegacySnapshotRefusal::Order);
        }
        self.ring[(*self.next_entity % ring) as usize].copy_from(entity);
        *self.next_entity += 1;
        self.list.entities += 1;
        Ok(())
    }

    /// Portal-area visibility bits; bytes beyond 32 are ignored.
    pub fn set_area_mask(&mut self, mask: &[u8]) {
        let length = mask.len().min(MAX_AREA_BYTES);
        self.list.area[..length].copy_from_slice(&mask[..length]);
        self.list.area_bytes = length;
    }
}

/// Everything about one snapshot that does not come from the game.
#[derive(Clone, Copy, Debug)]
pub(super) struct SnapshotRequest {
    /// `netchan.outgoingSequence` of the message being built.
    pub sequence: i32,
    /// The message the client asked to delta from; zero or less asks for none.
    pub delta_message: i32,
    /// `sv.time`, already offset for an unacknowledged map change.
    pub server_time: i32,
    /// `svs.snapFlagServerBit`.
    pub server_bit: u8,
    pub rate_delayed: bool,
    /// Only active clients are sent deltas; the others are flagged not active.
    pub active: bool,
    /// A server demo forbids a delta (`sv_snapshot.cpp:151-161`): it waits for a whole
    /// frame, or the requested base is older than the oldest frame it holds.
    pub demo_forbids_delta: bool,
}

impl SnapshotHistory {
    /// `entity_ring` states are shared by the 32 frames; the reference allows an
    /// average of 64 per frame.
    pub fn new(entity_ring: usize) -> Self {
        let list = EntityList {
            area: [0; MAX_AREA_BYTES],
            area_bytes: 0,
            first_entity: 0,
            entities: 0,
        };
        let frame = Frame {
            player: PlayerState::zero(),
            vehicle: PlayerState::zero(),
            list,
        };
        Self {
            frames: vec![frame; FRAMES],
            ring: vec![EntityState::zero(0, &LEGACY_ENTITY_FIELDS); entity_ring.max(1)],
            // As the reference starts: no frame of a fresh history is a valid base.
            next_entity: entity_ring.max(1) as u64,
            writer: SnapshotWriter::default(),
        }
    }

    /// Forget everything sent; no frame of a fresh history is a valid base.
    pub fn reset(&mut self) {
        self.next_entity = self.ring.len() as u64;
        for frame in &mut self.frames {
            (frame.list.first_entity, frame.list.entities) = (0, 0);
        }
    }

    /// Record this message's frame and append its snapshot to `message`.
    ///
    /// Without `fill` the frame is left as `SV_BuildClientSnapshot` leaves a
    /// zombie's (`sv_snapshot.cpp:560-569`): no entities, cleared area bits, and the
    /// player state this history slot held 32 messages ago. `fill` must therefore
    /// overwrite the player state completely. Returns whether the snapshot was a delta.
    pub fn write<'b>(
        &mut self,
        message: &mut MessageWriter,
        request: SnapshotRequest,
        fill: Option<impl FnOnce(&mut LegacySnapshotFrame<'_>)>,
        baseline: impl Fn(u16) -> Option<&'b EntityState>,
    ) -> Result<bool, SnapshotWriteError> {
        let index = request.sequence as usize & (FRAMES - 1);
        let Frame {
            player,
            vehicle,
            list,
        } = &mut self.frames[index];
        list.area = [0; MAX_AREA_BYTES];
        (list.entities, list.first_entity) = (0, self.next_entity);
        if let Some(fill) = fill {
            fill(&mut LegacySnapshotFrame {
                player,
                vehicle,
                list,
                ring: &mut self.ring,
                next_entity: &mut self.next_entity,
            });
        }
        let ring = self.ring.len() as u64;
        let distance = request.sequence.wrapping_sub(request.delta_message);
        let base = (request.delta_message > 0
            && request.active
            && distance < FRAMES as i32 - 3
            && !request.demo_forbids_delta)
            .then(|| &self.frames[request.delta_message as usize & (FRAMES - 1)])
            // Its entities were overwritten once the ring moved a full turn past them.
            .filter(|base| base.list.first_entity > self.next_entity - ring);
        let states = &self.ring;
        let list = |frame: &Frame| {
            (frame.list.first_entity..frame.list.first_entity + frame.list.entities as u64)
                .map(move |position| &states[(position % ring) as usize])
        };
        let frame = &self.frames[index];
        let flags = request.server_bit
            | if request.rate_delayed {
                RATE_DELAYED
            } else {
                0
            }
            | if request.active { 0 } else { NOT_ACTIVE };
        let delta = base.is_some();
        self.writer.write_frame(
            message,
            &SnapshotHeader {
                server_time: request.server_time,
                distance: if base.is_some() { distance as u8 } else { 0 },
                flags,
                area_mask: &frame.list.area[..frame.list.area_bytes],
            },
            base.map(|base| (base.sent(), list(base))),
            frame.sent(),
            list(frame),
            |number| baseline(number),
        )?;
        Ok(delta)
    }
}

#[path = "local_snapshot.rs"]
mod local;
pub use local::LocalSnapshotBuffer;

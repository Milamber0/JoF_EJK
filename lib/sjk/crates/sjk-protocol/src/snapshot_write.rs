//! Outbound snapshots for the protocol-26 server compatibility adapter.
use super::Snapshot;
use crate::{
    ENTITY_NUMBER_NONE, EntityState, EntityWriteError, GameState, LEGACY_ENTITY_FIELDS,
    LEGACY_ENTITY_NUMBER_BITS, MessageError, MessageWriter, PlayerState, ServiceCommand,
    write_delta_entity,
};
use std::{error::Error, fmt};
#[path = "player_state_write.rs"]
mod player;

/// `state` as a client decodes it once it has crossed the wire: every field cut to the
/// bits and the sign protocol 26 gives it (`MSG_WriteDeltaPlayerstate` against nothing,
/// read back). A legacy client restarts its prediction from exactly this, so a server
/// that keeps more in a field than the wire carries disagrees with its own clients.
/// `None` if the state does not fit a message, which no single state does.
pub fn player_state_as_received(state: &PlayerState) -> Option<PlayerState> {
    let mut message = MessageWriter::new(16_384);
    player::write(&mut message, None, state).ok()?;
    let bytes = message.finish().ok()?;
    super::read_player_state(&mut crate::MessageReader::new(&bytes), &PlayerState::zero()).ok()
}

/// Reusable snapshot serializer; keeps an empty baseline outside the send loop.
///
/// Follows `codemp/server/sv_snapshot.cpp` (`SV_WriteSnapshotToClient` and
/// `SV_EmitPacketEntities`) and `codemp/qcommon/msg.cpp`, including a riding
/// player's pilot schema and the vehicle's own player state.
/// No sockets, game simulation, or snapshot-history policy live in this adapter.
pub struct SnapshotWriter {
    zero_entity: EntityState,
}
impl Default for SnapshotWriter {
    fn default() -> Self {
        Self {
            zero_entity: EntityState::zero(0, &LEGACY_ENTITY_FIELDS),
        }
    }
}
impl SnapshotWriter {
    /// Append one server message, including reliable acknowledgement/commands and EOF.
    ///
    /// `snapshot.delta_from` selects the exact supplied base. A base snapshot ignores
    /// `previous`. Entity lists must be strictly increasing and exclude the sentinel.
    /// Validation happens before writing; a message-capacity error can leave a partial
    /// message, which the caller must discard. This method allocates no scratch state;
    /// the caller owns the compressed message's capacity and lifetime.
    pub fn write(
        &self,
        message: &mut MessageWriter,
        snapshot: &Snapshot,
        game: &GameState,
        previous: Option<&Snapshot>,
    ) -> Result<(), SnapshotWriteError> {
        let (previous, distance) = validate(snapshot, previous)?;
        if snapshot.entities.iter().any(|entity| {
            game.baseline(usize::from(entity.number()))
                .is_some_and(|baseline| baseline.field_count() != LEGACY_ENTITY_FIELDS.len())
        }) {
            return Err(SnapshotWriteError::EntityOrderOrSchema);
        }
        message.write_i32(snapshot.reliable_acknowledge)?;
        for command in &snapshot.server_commands {
            message.write_u8(ServiceCommand::ServerCommand as u8)?;
            message.write_i32(command.sequence)?;
            message.write_c_string(&command.command)?;
        }
        self.write_frame(
            message,
            &SnapshotHeader {
                server_time: snapshot.server_time,
                distance,
                flags: snapshot.flags,
                area_mask: &snapshot.area_mask,
            },
            previous.map(|state| (SnapshotPlayer::of(state), state.entities.iter())),
            SnapshotPlayer::of(snapshot),
            snapshot.entities.iter(),
            |number| game.baseline(usize::from(number)),
        )?;
        message.write_u8(ServiceCommand::End as u8)?;
        Ok(())
    }

    /// Append `svc_snapshot` through the entity terminator to a message in progress.
    ///
    /// The part of `SV_WriteSnapshotToClient` after the delta base is chosen. A
    /// server writes what precedes it (acknowledged client command, pending reliable
    /// commands) and the final `svc_EOF`, and keeps its frame history wherever it
    /// likes: entity lists arrive as iterators, so a ring that wraps needs no copy.
    /// Both lists must be strictly increasing, below the sentinel and on the stock
    /// schema; that and the area mask are checked before anything is written.
    ///
    /// While `player` rides (`m_iVehicleNum` set) its vehicle's state follows its own,
    /// delta-coded against the base's vehicle only when the base rode too, as
    /// `sv_snapshot.cpp:227-259` does; a riding player without a vehicle state,
    /// or riding after a riding base without one, is [`SnapshotWriteError::VehicleState`].
    pub fn write_frame<'a, I>(
        &self,
        message: &mut MessageWriter,
        header: &SnapshotHeader<'_>,
        previous: Option<(SnapshotPlayer<'_>, I)>,
        player: SnapshotPlayer<'_>,
        entities: I,
        baseline: impl Fn(u16) -> Option<&'a EntityState>,
    ) -> Result<(), SnapshotWriteError>
    where
        I: Iterator<Item = &'a EntityState> + Clone,
    {
        if header.area_mask.len() > super::MAX_AREA_MASK {
            return Err(SnapshotWriteError::AreaMask);
        }
        let old_player = previous.as_ref().map(|(player, _)| *player);
        // `frame->vps`, and `oldframe->vps` when the old frame rode too; else from nothing.
        let vehicle = if rides(player.player) {
            let old = match old_player {
                Some(old) if rides(old.player) => {
                    Some(old.vehicle.ok_or(SnapshotWriteError::VehicleState)?)
                }
                _ => None,
            };
            Some((old, player.vehicle.ok_or(SnapshotWriteError::VehicleState)?))
        } else {
            None
        };
        let old = previous.map(|(_, entities)| entities);
        for list in std::iter::once(entities.clone()).chain(old.clone()) {
            let mut last = None;
            for entity in list {
                if entity.number() >= ENTITY_NUMBER_NONE
                    || entity.field_count() != LEGACY_ENTITY_FIELDS.len()
                    || last.is_some_and(|last| last >= entity.number())
                    || baseline(entity.number())
                        .is_some_and(|base| base.field_count() != LEGACY_ENTITY_FIELDS.len())
                {
                    return Err(SnapshotWriteError::EntityOrderOrSchema);
                }
                last = Some(entity.number());
            }
        }
        message.write_u8(ServiceCommand::Snapshot as u8)?;
        message.write_i32(header.server_time)?;
        message.write_u8(header.distance)?;
        message.write_u8(header.flags)?;
        message.write_u8(header.area_mask.len() as u8)?;
        for &byte in header.area_mask {
            message.write_u8(byte)?;
        }
        player::write(message, old_player.map(|old| old.player), player.player)?;
        if let Some((old, vehicle)) = vehicle {
            player::write_vehicle(message, old, vehicle)?;
        }
        let (mut old, mut new) = (old.into_iter().flatten().peekable(), entities.peekable());
        while old.peek().is_some() || new.peek().is_some() {
            let old_number = old
                .peek()
                .map_or(ENTITY_NUMBER_NONE, |entity| entity.number());
            let new_number = new
                .peek()
                .map_or(ENTITY_NUMBER_NONE, |entity| entity.number());
            match new_number.cmp(&old_number) {
                std::cmp::Ordering::Equal => {
                    write_delta_entity(
                        message,
                        old.next().unwrap(),
                        new.next(),
                        false,
                        &LEGACY_ENTITY_FIELDS,
                    )?;
                }
                std::cmp::Ordering::Less => {
                    let entity = new.next().unwrap();
                    let base = baseline(new_number).unwrap_or(&self.zero_entity);
                    write_delta_entity(message, base, Some(entity), true, &LEGACY_ENTITY_FIELDS)?;
                }
                std::cmp::Ordering::Greater => {
                    write_delta_entity(
                        message,
                        old.next().unwrap(),
                        None,
                        true,
                        &LEGACY_ENTITY_FIELDS,
                    )?;
                }
            }
        }
        message.write_bits(u32::from(ENTITY_NUMBER_NONE), LEGACY_ENTITY_NUMBER_BITS)?;
        Ok(())
    }
}

/// A client's player state and, while it rides, its vehicle's (`frame->ps` and
/// `frame->vps`). The vehicle's is read only while `player` rides.
#[derive(Clone, Copy)]
pub struct SnapshotPlayer<'a> {
    /// The client's own player state.
    pub player: &'a PlayerState,
    /// The ridden vehicle's player state: the entity `m_iVehicleNum` names.
    pub vehicle: Option<&'a PlayerState>,
}
impl<'a> SnapshotPlayer<'a> {
    fn of(snapshot: &'a Snapshot) -> Self {
        Self {
            player: &snapshot.player,
            vehicle: snapshot.vehicle_player.as_ref(),
        }
    }
}
impl<'a> From<&'a PlayerState> for SnapshotPlayer<'a> {
    /// A player on foot.
    fn from(player: &'a PlayerState) -> Self {
        Self {
            player,
            vehicle: None,
        }
    }
}

/// `m_iVehicleNum` is set: the snapshot carries a vehicle player state.
fn rides(state: &PlayerState) -> bool {
    state.fields[84] != 0
}

/// The fixed fields that open a snapshot.
pub struct SnapshotHeader<'a> {
    /// `sv.time`, plus the old map's offset while a client has not acknowledged a map change.
    pub server_time: i32,
    /// Messages between this snapshot and its delta base; zero for a full snapshot.
    /// A byte on the wire, so a server passes its difference truncated as the reference does.
    pub distance: u8,
    /// `SNAPFLAG_*` bits.
    pub flags: u8,
    /// Portal-area visibility, at most 32 bytes.
    pub area_mask: &'a [u8],
}

fn validate<'a>(
    snapshot: &Snapshot,
    previous: Option<&'a Snapshot>,
) -> Result<(Option<&'a Snapshot>, u8), SnapshotWriteError> {
    let (previous, distance) = if let Some(sequence) = snapshot.delta_from {
        let base = previous
            .filter(|base| base.message_sequence == sequence)
            .ok_or(SnapshotWriteError::DeltaBase)?;
        let distance = snapshot
            .message_sequence
            .checked_sub(sequence)
            .and_then(|distance| u8::try_from(distance).ok())
            .filter(|&distance| distance != 0)
            .ok_or(SnapshotWriteError::DeltaBase)?;
        (Some(base), distance)
    } else {
        (None, 0)
    };
    // A rider needs its vehicle's state, and so does a riding base it is delta-coded
    // against; a vehicle state beside a player on foot is never sent.
    if rides(&snapshot.player)
        && (snapshot.vehicle_player.is_none()
            || previous.is_some_and(|base| rides(&base.player) && base.vehicle_player.is_none()))
    {
        return Err(SnapshotWriteError::VehicleState);
    }
    for state in std::iter::once(snapshot).chain(previous) {
        if state.entities.iter().any(|entity| {
            entity.number() >= ENTITY_NUMBER_NONE
                || entity.field_count() != LEGACY_ENTITY_FIELDS.len()
        }) || state
            .entities
            .windows(2)
            .any(|pair| pair[0].number() >= pair[1].number())
        {
            return Err(SnapshotWriteError::EntityOrderOrSchema);
        }
    }
    if snapshot.area_mask.len() > super::MAX_AREA_MASK {
        return Err(SnapshotWriteError::AreaMask);
    }
    if snapshot
        .server_commands
        .iter()
        .any(|command| command.command.len() >= 1024 || command.command.contains(&0))
    {
        return Err(SnapshotWriteError::ServerCommand);
    }
    Ok((previous, distance))
}

/// An invalid outbound snapshot or a compressed-message serialization failure.
#[derive(Debug)]
pub enum SnapshotWriteError {
    /// Missing, mismatched, future, or unrepresentably distant delta base.
    DeltaBase,
    /// A riding player state (`m_iVehicleNum` set) came without its vehicle's player
    /// state, or its riding delta base did.
    VehicleState,
    /// Entity lists must be ordered, unique, below the sentinel, and use the stock schema.
    EntityOrderOrSchema,
    /// Area visibility exceeds the protocol's mask capacity.
    AreaMask,
    /// A reliable command contains NUL or exceeds the stock string capacity.
    ServerCommand,
    /// Compressed-message writing failed.
    Message(MessageError),
    /// An entity delta could not be written.
    Entity(EntityWriteError),
}
impl fmt::Display for SnapshotWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeltaBase => f.write_str("invalid snapshot delta base"),
            Self::VehicleState => {
                f.write_str("riding player state without its vehicle's player state")
            }
            Self::EntityOrderOrSchema => f.write_str("invalid snapshot entity order or schema"),
            Self::AreaMask => f.write_str("snapshot area mask exceeds protocol capacity"),
            Self::ServerCommand => f.write_str("invalid reliable server command"),
            Self::Message(error) => error.fmt(f),
            Self::Entity(error) => error.fmt(f),
        }
    }
}
impl Error for SnapshotWriteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Message(e) => Some(e),
            Self::Entity(e) => Some(e),
            _ => None,
        }
    }
}
impl From<MessageError> for SnapshotWriteError {
    fn from(error: MessageError) -> Self {
        Self::Message(error)
    }
}
impl From<EntityWriteError> for SnapshotWriteError {
    fn from(error: EntityWriteError) -> Self {
        Self::Entity(error)
    }
}

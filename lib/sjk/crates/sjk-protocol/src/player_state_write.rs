//! Protocol-26 player deltas, matching codemp `MSG_WriteDeltaPlayerstate` as built with
//! `_OPTIMIZED_VEHICLE_NETWORKING` (the stock build): the ordinary schema, the pilot
//! schema for a rider hidden inside its vehicle, and the vehicle's own schema.
use super::super::{
    FLOAT_INT_BIAS, FLOAT_INT_BITS, MAX_WEAPONS, PILOT_FIELD_SPECS, PLAYER_FIELD_WIDTHS,
    PlayerState, STAT_WEAPONS, Slot, VEHICLE_FIELD_SPECS,
};
use crate::{MessageError, MessageWriter};

/// `EF_NODRAW` (`codemp/game/bg_public.h:647`).
const EF_NODRAW: u32 = 1 << 8;
/// `m_iVehicleNum` and `eFlags` in the ordinary schema.
const VEHICLE_NUM: usize = 84;
const ENTITY_FLAGS: usize = 17;

/// A client's own player state (`isVehiclePS == qfalse`). One bit picks the schema:
/// the pilot fields while the player rides *inside* a vehicle (`m_iVehicleNum` set and
/// `EF_NODRAW`), the ordinary fields otherwise (`msg.cpp:2123-2137`).
pub(super) fn write(
    message: &mut MessageWriter,
    from: Option<&PlayerState>,
    to: &PlayerState,
) -> Result<(), MessageError> {
    let inside = to.fields[VEHICLE_NUM] != 0 && to.fields[ENTITY_FLAGS] & EF_NODRAW != 0;
    message.write_bits(u32::from(inside), 1)?;
    write_fields(message, from, to, inside.then_some(PILOT_FIELD_SPECS))
}

/// The ridden vehicle's player state (`isVehiclePS == qtrue`): no schema bit, and
/// the vehicle-only fields ([`crate::VehicleNetFields`]) among its own.
pub(super) fn write_vehicle(
    message: &mut MessageWriter,
    from: Option<&PlayerState>,
    to: &PlayerState,
) -> Result<(), MessageError> {
    write_fields(message, from, to, Some(VEHICLE_FIELD_SPECS))
}

/// The field section and the four arrays. `schema` maps each wire field to its
/// storage slot and width; `None` is the ordinary schema itself.
fn write_fields(
    message: &mut MessageWriter,
    from: Option<&PlayerState>,
    to: &PlayerState,
    schema: Option<&[(Slot, i8)]>,
) -> Result<(), MessageError> {
    let field = |i: usize| {
        schema.map_or_else(
            || (Slot::Field(i), PLAYER_FIELD_WIDTHS[i]),
            |fields| fields[i],
        )
    };
    let count = schema.map_or(PLAYER_FIELD_WIDTHS.len(), <[_]>::len);
    let before = |slot: Slot| from.map_or(0, |state| state.slot(slot));
    let changed = |i: usize| {
        let slot = field(i).0;
        before(slot) != to.slot(slot)
    };
    let last = (0..count).rfind(|&i| changed(i)).map_or(0, |i| i + 1);
    message.write_u8(last as u8)?;
    for i in 0..last {
        let (slot, width) = field(i);
        if !changed(i) {
            message.write_bits(0, 1)?;
            continue;
        }
        message.write_bits(1, 1)?;
        let value = to.slot(slot);
        if width != 0 {
            message.write_bits(value, width.unsigned_abs())?;
        } else {
            write_float(message, value)?;
        }
    }
    let zero = [0; 16];
    let previous = from.map_or([&zero; 4], |state| {
        [
            &state.stats,
            &state.persistent,
            &state.ammo,
            &state.powerups,
        ]
    });
    let current = [&to.stats, &to.persistent, &to.ammo, &to.powerups];
    let masks: [u32; 4] = std::array::from_fn(|array| {
        (0..16)
            .filter(|&i| previous[array][i] != current[array][i])
            .fold(0, |mask, i| mask | (1 << i))
    });
    let any = masks.iter().any(|&mask| mask != 0);
    message.write_bits(u32::from(any), 1)?;
    if !any {
        return Ok(());
    }
    for (array, &mask) in masks.iter().enumerate() {
        message.write_bits(u32::from(mask != 0), 1)?;
        if mask == 0 {
            continue;
        }
        message.write_bits(mask, 16)?;
        for i in 0..16 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let width = if array == 3 {
                32
            } else if array == 0 && i == STAT_WEAPONS {
                MAX_WEAPONS
            } else {
                16
            };
            message.write_bits(current[array][i], width)?;
        }
    }
    Ok(())
}

fn write_float(message: &mut MessageWriter, bits: u32) -> Result<(), MessageError> {
    let value = f32::from_bits(bits);
    let integral = value as i32;
    if (-FLOAT_INT_BIAS..FLOAT_INT_BIAS).contains(&integral) && integral as f32 == value {
        message.write_bits(0, 1)?;
        message.write_bits((integral + FLOAT_INT_BIAS) as u32, FLOAT_INT_BITS)
    } else {
        message.write_bits(1, 1)?;
        message.write_bits(bits, 32)
    }
}

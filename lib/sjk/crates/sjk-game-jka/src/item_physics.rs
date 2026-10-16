//! `G_RunItem` and `G_BounceItem` (OpenJK `codemp/game/g_items.c:3143-3272`) for a world
//! object that is not a missile but has `physicsObject` set — the Jedi Master's saber while
//! held, a holocron — in the two halves its caller runs its think between.

use crate::pmove::{MovementCollision, MovementTrace};
use crate::weapon_fire::Missile;

/// The entity's wire fields `G_RunItem` reads and writes.
const ES_POS_TIME: usize = 0;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_DURATION: usize = 20;
const ES_GROUND: usize = 22;
const ES_POS_TYPE: usize = 23;
/// `TR_STATIONARY`, `TR_GRAVITY`, `CONTENTS_NODROP`.
const TR_STATIONARY: u32 = 0;
const TR_GRAVITY: u32 = 6;
const CONTENTS_NODROP: u32 = 0x800;

/// What the first half left: the object at rest (only its think runs), or moved along
/// its trajectory with the trace that moved it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ItemMove {
    Resting,
    Moved(MovementTrace),
}

/// What the second half did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ItemBounce {
    /// The move struck nothing.
    Clear,
    /// It came to rest in a no-drop volume, where the reference frees it.
    NoDrop,
    /// It came to rest on a floor, a unit above it (`G_SetOrigin`, snapped).
    Stopped,
    /// It struck a wall and drops from it; a holocron's touch follows with the trace.
    Bounced(MovementTrace),
}

/// `G_RunItem` up to its think: off the ground it falls again; at rest it does nothing
/// more; else it is traced from where it is to where its trajectory has it now.
pub(crate) fn item_move(
    missile: &mut Missile,
    level_time: i32,
    world: &dyn MovementCollision,
) -> ItemMove {
    let get = |missile: &Missile, index: usize| missile.state.raw_field(index).unwrap_or(0);
    if get(missile, ES_GROUND) == u32::from(sjk_protocol::ENTITY_NUMBER_NONE)
        && get(missile, ES_POS_TYPE) != TR_GRAVITY
    {
        missile.state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
    }
    if get(missile, ES_POS_TYPE) == TR_STATIONARY {
        return ItemMove::Resting;
    }
    let read = |fields: [usize; 3]| -> [f32; 3] {
        std::array::from_fn(|axis| f32::from_bits(get(missile, fields[axis])))
    };
    let origin = crate::trajectory::legacy_evaluate_trajectory(
        read(ES_POS_BASE),
        read(ES_POS_DELTA),
        get(missile, ES_POS_TYPE) as u8,
        get(missile, ES_POS_TIME) as i32,
        get(missile, ES_POS_DURATION) as i32,
        level_time,
    );
    let mut trace = world.trace(
        missile.current,
        missile.bounds.0,
        missile.bounds.1,
        origin,
        missile.clip_mask,
    );
    missile.current = trace.end_position;
    if trace.start_solid {
        trace.fraction = 0.0;
    }
    ItemMove::Moved(trace)
}

/// `G_RunItem` after its think, for a move that struck something: `G_BounceItem` with no
/// bounce left in it (`physicsBounce` is never set) — at rest a unit above a floor, or
/// dropping from a wall from a unit off it.
pub(crate) fn item_bounce(
    missile: &mut Missile,
    trace: &MovementTrace,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
) -> ItemBounce {
    if trace.fraction == 1.0 {
        return ItemBounce::Clear;
    }
    if world.point_contents(missile.current) & CONTENTS_NODROP != 0 {
        return ItemBounce::NoDrop;
    }
    let set_vector = |missile: &mut Missile, fields: [usize; 3], value: [f32; 3]| {
        for axis in 0..3 {
            missile
                .state
                .set_raw_field(fields[axis], value[axis].to_bits());
        }
    };
    // The velocity at the moment of the hit, reflected, then scaled by `physicsBounce` —
    // zero, which keeps each component's sign.
    let get = |index: usize| missile.state.raw_field(index).unwrap_or(0);
    let delta: [f32; 3] = std::array::from_fn(|axis| f32::from_bits(get(ES_POS_DELTA[axis])));
    let hit_time = previous_time + ((level_time - previous_time) as f32 * trace.fraction) as i32;
    let velocity = crate::trajectory::legacy_evaluate_trajectory_delta(
        delta,
        get(ES_POS_TYPE) as u8,
        get(ES_POS_TIME) as i32,
        get(ES_POS_DURATION) as i32,
        hit_time,
    );
    let normal = trace.plane_normal;
    let dot = velocity.iter().zip(normal).map(|(a, b)| a * b).sum::<f32>();
    let reflected: [f32; 3] =
        std::array::from_fn(|axis| (velocity[axis] + -2.0 * dot * normal[axis]) * 0.0);
    set_vector(missile, ES_POS_DELTA, reflected);
    if normal[2] > 0.0 && reflected[2] < 40.0 {
        let mut end = trace.end_position;
        end[2] += 1.0;
        let end = crate::weapon_fire::snap_vector(end);
        // `G_SetOrigin`.
        set_vector(missile, ES_POS_BASE, end);
        set_vector(missile, ES_POS_DELTA, [0.0; 3]);
        for (index, value) in [
            (ES_POS_TYPE, TR_STATIONARY),
            (ES_POS_TIME, 0),
            (ES_POS_DURATION, 0),
            (ES_GROUND, u32::from(trace.entity_number)),
        ] {
            missile.state.set_raw_field(index, value);
        }
        missile.current = end;
        return ItemBounce::Stopped;
    }
    let current: [f32; 3] = std::array::from_fn(|axis| missile.current[axis] + normal[axis]);
    missile.current = current;
    set_vector(missile, ES_POS_BASE, current);
    missile.state.set_raw_field(ES_POS_TIME, level_time as u32);
    ItemBounce::Bounced(*trace)
}

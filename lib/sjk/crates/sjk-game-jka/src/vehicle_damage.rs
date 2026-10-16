//! What `G_Damage` does differently to a vehicle NPC (`codemp/game/g_combat.c:4425-5160`):
//! the DEMP2's longer jolt on a speeder or a walker and none on a fighter, no hit location,
//! the attacker remembered for 25 seconds, and — once the shields have taken their share —
//! the rest off the vehicle's armour, the shield display updated, and the vehicle marked
//! dead when the armour is gone. [`crate::npc_damage`] calls these at those points.
//!
//! A vehicle hit in the air is knocked about ([`knocked`]). Not here: a fighter's side
//! struck by a Ghoul2 trace (`g2LastSurfaceHit`, `G_ShipSurfaceForSurfName`), which this
//! server's traces do not report; the side is guessed from the blow's point, as the
//! reference guesses it without one.

use crate::damage::OtherKiller;
use crate::npc_spawn::{NpcActor, es};
use crate::vehicle_fields::kind;

/// `STAT_ARMOR`; `ps.eFlags`; `EF_DEAD`.
const STAT_ARMOR: usize = 5;
const PS_EFLAGS: usize = 17;
const EF_DEAD: u32 = 1 << 1;
/// How long a vehicle remembers its attacker (`g_combat.c:4706-4710`, `4739-4744`).
const REMEMBERED: i32 = 25_000;

/// The DEMP2's jolt on a vehicle (`g_combat.c:4441-4455`): `Some(until)` from the draw
/// `irand(3000, 4000)` for a speeder or a walker, `None` for a fighter (no jolt at all);
/// anything else — an animal, a vehicle of no definition — `Some` of the ordinary one.
pub fn electrify(
    npc: &NpcActor,
    level_time: i32,
    irand: &mut dyn FnMut(i32, i32) -> i32,
) -> Option<i32> {
    match npc.vehicle.as_deref().map(|vehicle| vehicle.kind()) {
        Some(kind::SPEEDER | kind::WALKER) => Some(level_time + irand(3_000, 4_000)),
        Some(kind::FIGHTER) => None,
        _ => Some(level_time + irand(300, 800)),
    }
}

/// `G_LocationBasedDamageModifier`'s "no location-based damage on vehicles"
/// (`g_combat.c:4301-4304`).
pub fn located(npc: &NpcActor) -> bool {
    npc.vehicle.is_none()
}

/// The attacker a vehicle remembers (`otherKiller`), for 25 seconds rather than a player's
/// five, whether or not the blow pushed it; `None` for an NPC that is no vehicle.
pub fn remembered(npc: &NpcActor, attacker: u16, level_time: i32) -> Option<OtherKiller> {
    npc.vehicle.as_ref()?;
    Some(OtherKiller {
        number: attacker,
        time: level_time + REMEMBERED,
        debounce_time: level_time + REMEMBERED,
    })
}

/// After the armour's share (`g_combat.c:4959-5051`): the attacker the vehicle's enemy
/// (the world where `G_Damage` was given none, as it fills one in), its shields the armour
/// stat and shown, `take` off its own armour, and dead (`EF_DEAD`) with none left.
pub fn armor_taken(npc: &mut NpcActor, take: i32, attacker: u16) {
    let Some(vehicle) = npc.vehicle.as_deref_mut() else {
        return;
    };
    npc.mind.enemy = Some(attacker);
    vehicle.shields = npc.player.stats[STAT_ARMOR] as i32;
    crate::vehicle_update::update_shields(vehicle, &mut npc.player);
    vehicle.armor -= take;
    if vehicle.armor <= 0 {
        let flags = npc.state.raw_field(es::EFLAGS).unwrap_or(0);
        npc.state.set_raw_field(es::EFLAGS, flags | EF_DEAD);
        let flags = npc.player.raw_field(PS_EFLAGS).unwrap_or(0);
        npc.player.set_raw_field(PS_EFLAGS, flags | EF_DEAD);
        vehicle.armor = 0;
    }
}

/// The knock a blow gives a vehicle up in the air (`g_combat.c:5052-5145`): not an animal's,
/// not its own, with a point that is not where it stands, and nothing within its landing
/// height — pitched or rolled by up to ten degrees by the side the point is on.
pub fn knocked(npc: &mut NpcActor, damage: i32, attacker: u16, point: Option<[f32; 3]>) {
    use crate::vehicle_surfaces::{BACK, FRONT, LEFT, RIGHT};
    let (number, origin, current) = (npc.number, npc.player.origin(), npc.current_origin);
    let Some(vehicle) = npc.vehicle.as_deref_mut() else {
        return;
    };
    let Some(point) = point else { return };
    if vehicle.kind() == kind::ANIMAL
        || attacker == number
        || point == origin
        || vehicle.land_trace.fraction < 1.0
    {
        return;
    }
    let strength = (damage as f32 / 200.0 * 10.0).min(10.0);
    let mut toward: [f32; 3] = std::array::from_fn(|axis| point[axis] - current[axis]);
    crate::player_angle_math::normalize(&mut toward);
    let up = crate::pmove::flight::angles_to_axis(vehicle.orientation)[2];
    let (forward, right) = crate::pmove::flight::flight_axes(vehicle.orientation);
    let dot = |a: [f32; 3]| a[0] * toward[0] + a[1] * toward[1] + a[2] * toward[2];
    let across = dot(right.to_array());
    let side = if across > 0.4 {
        RIGHT
    } else if across < -0.4 {
        LEFT
    } else if dot(forward.to_array()) > 0.0 {
        FRONT
    } else {
        BACK
    };
    let above = dot(up) > 0.0;
    let orientation = &mut vehicle.orientation;
    match (side, above) {
        (FRONT, true) | (BACK, false) => orientation[0] += strength,
        (FRONT, false) | (BACK, true) => orientation[0] -= strength,
        (RIGHT, true) | (LEFT, false) => orientation[2] -= strength,
        _ => orientation[2] += strength,
    }
}

//! The space-ship triggers (`codemp/game/g_trigger.c:1449-1760`): space (`trigger_space`:
//! no gravity, and a player outside a closed ship suffocates), the ship boundary
//! (`trigger_shipboundary`: a fighter that flies into it is turned back toward a point for
//! a while, one with no pilot or with pieces missing is blown up) and the hyperspace lane
//! (`trigger_hyperspace`: a ship that enters is faced along the lane, thrown to hyperspace
//! speed and, three quarters of the way through, put out at the same place around another
//! point, facing that point's way).
//!
//! What they are and what their touches decide is here; the roster owns them
//! ([`crate::npc_roster::NpcRoster::ship_triggers`]), touches them for its vehicles
//! (`G_TouchTriggers` after an NPC's move) and runs the boundaries' thinks; the players'
//! side (a player in space, a pilot carried through hyperspace) is the caller's.

use sjk_entity::Entity;

/// `INITIAL_SUFFOCATION_DELAY` (`g_trigger.c:1459`): half a second of air.
pub const INITIAL_SUFFOCATION_DELAY: i32 = 500;
/// `ENTITYNUM_NONE`.
pub const ENTITYNUM_NONE: u16 = 1_023;
/// `EF2_HYPERSPACE`.
const EF2_HYPERSPACE: u32 = 1 << 5;
/// The hyperspace end's sound (`SP_trigger_hyperspace` registers it).
pub const HYPERSPACE_END_SOUND: &[u8] = b"sound/vehicles/common/hyperend.wav";

/// A place a trigger names (`target`, `target2`): an entity of the map with that
/// `targetname`, its number, where it stands and how it faces.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub number: u16,
    pub name: String,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

/// What a space-ship trigger does.
#[derive(Clone, Debug, PartialEq)]
pub enum ShipTriggerKind {
    /// `trigger_space`.
    Space,
    /// `trigger_shipboundary`: the point it turns ships toward, how long (`traveltime`,
    /// `genericValue1`; the turn lasts twice it), until when its think looks for fighters
    /// in it (`genericValue7`), and its next think.
    Boundary {
        target: String,
        travel_time: i32,
        detailed_until: i32,
        next_think: i32,
    },
    /// `trigger_hyperspace`: the point ships leave from (`target`) and the one they come
    /// out around (`target2`).
    Hyperspace { from: String, to: String },
}

/// A space-ship trigger: its entity number, its brush's box (`r.mins`, `r.maxs`; a
/// brush model sits at the world's origin) and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct ShipTrigger {
    pub number: u16,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub kind: ShipTriggerKind,
}

impl ShipTrigger {
    /// `r.absmin`, `r.absmax`: the box linked a unit wider (`SV_LinkEntity`).
    pub fn absolute(&self) -> ([f32; 3], [f32; 3]) {
        (
            self.mins.map(|value| value - 1.0),
            self.maxs.map(|value| value + 1.0),
        )
    }

    /// `G_PointInBounds(point, absmin, absmax)`.
    pub fn holds(&self, point: [f32; 3]) -> bool {
        let (low, high) = self.absolute();
        (0..3).all(|axis| point[axis] >= low[axis] && point[axis] <= high[axis])
    }

    /// `trap->EntityContact(mins, maxs, trigger)` against its box: a toucher's box
    /// (`origin + r.mins` to `origin + r.maxs`) that overlaps it.
    pub fn contacts(&self, low: [f32; 3], high: [f32; 3]) -> bool {
        (0..3).all(|axis| low[axis] < self.maxs[axis] && high[axis] > self.mins[axis])
    }

    /// `trap->EntitiesInBox` of `G_TouchTriggers` (the toucher's origin 40 by 40 by 52
    /// about) meeting its linked box.
    pub fn near(&self, origin: [f32; 3]) -> bool {
        const RANGE: [f32; 3] = [40.0, 40.0, 52.0];
        let (low, high) = self.absolute();
        (0..3).all(|axis| {
            origin[axis] - RANGE[axis] <= high[axis] && origin[axis] + RANGE[axis] >= low[axis]
        })
    }
}

/// The level's space-ship triggers and the points they name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShipTriggers {
    pub triggers: Vec<ShipTrigger>,
    pub points: Vec<Point>,
}

/// The classname of a space-ship trigger.
pub fn is_ship_trigger(classname: &str) -> bool {
    [
        "trigger_space",
        "trigger_shipboundary",
        "trigger_hyperspace",
    ]
    .iter()
    .any(|known| known.eq_ignore_ascii_case(classname))
}

/// The names the map's space-ship triggers point at (`target`, `target2`).
pub fn named_points(entities: &[Entity]) -> Vec<String> {
    let mut names = Vec::new();
    for entity in entities
        .iter()
        .filter(|entity| entity.classname().is_some_and(is_ship_trigger))
    {
        for key in ["target", "target2"] {
            if let Some(name) = entity.get(key).filter(|name| !name.is_empty())
                && !names.iter().any(|known: &String| known == name)
            {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// `SP_trigger_space`, `SP_trigger_shipboundary`, `SP_trigger_hyperspace` (`InitTrigger`:
/// the brush's box, `CONTENTS_TRIGGER`): the trigger `entity` as entity `number`, its
/// brush `bounds`; `None` for another classname, or a brush without the keys its spawn
/// function demands (the reference's `ERR_DROP`, which this server reports and skips).
pub fn spawn(
    entity: &Entity,
    number: u16,
    bounds: ([f32; 3], [f32; 3]),
    level_time: i32,
) -> Result<ShipTrigger, String> {
    let classname = entity.classname().unwrap_or_default();
    let target = entity
        .get("target")
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let kind = if classname.eq_ignore_ascii_case("trigger_space") {
        ShipTriggerKind::Space
    } else if classname.eq_ignore_ascii_case("trigger_shipboundary") {
        let target = target.ok_or("trigger_shipboundary without a target.")?;
        let travel_time = entity
            .get("traveltime")
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
        if travel_time == 0 {
            return Err("trigger_shipboundary without traveltime.".to_owned());
        }
        ShipTriggerKind::Boundary {
            target,
            travel_time,
            detailed_until: 0,
            next_think: level_time + 500,
        }
    } else if classname.eq_ignore_ascii_case("trigger_hyperspace") {
        let from = target.ok_or("trigger_hyperspace without a target.")?;
        let to = entity
            .get("target2")
            .filter(|name| !name.is_empty())
            .ok_or("trigger_hyperspace without a target2.")?
            .to_owned();
        ShipTriggerKind::Hyperspace { from, to }
    } else {
        return Err(format!("{classname} is no space-ship trigger"));
    };
    Ok(ShipTrigger {
        number,
        mins: bounds.0,
        maxs: bounds.1,
        kind,
    })
}

/// A place a trigger names, as the map's entity of that name stands (`G_SetOrigin` of
/// its `origin`, its `angles` or `angle`).
pub fn point(entity: &Entity, number: u16, name: &str) -> Point {
    let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
    let angles = entity.vector("angles").ok().flatten().unwrap_or_else(|| {
        let yaw = entity
            .get("angle")
            .and_then(|text| text.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        [0.0, yaw, 0.0]
    });
    Point {
        number,
        name: name.to_owned(),
        origin,
        angles,
    }
}

impl ShipTriggers {
    /// `G_Find(NULL, FOFS(targetname), name)`: the first point of that name.
    pub fn find(&self, name: &str) -> Option<&Point> {
        self.points.iter().find(|point| point.name == name)
    }
}

/// A vehicle as the space-ship triggers read and change it.
pub struct Ship<'a> {
    /// Its entity number.
    pub number: u16,
    /// Its `ps.origin`.
    pub origin: [f32; 3],
    /// `r.mins`, `r.maxs`.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `ps.m_iVehicleNum`: someone flies it.
    pub piloted: bool,
    /// `ps.eFlags2`, whose `EF2_HYPERSPACE` the hyperspace clears once it has jumped.
    pub flags2: &'a mut u32,
    /// Its vehicle: its hyperspace and turnaround, its surfaces, its space.
    pub vehicle: &'a mut crate::vehicle::Vehicle,
}

/// What a ship's touch of a space-ship trigger asks of the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShipTouched {
    /// Nothing more.
    Nothing,
    /// `G_Damage(ship, ship, ship, NULL, origin, 99999, DAMAGE_NO_PROTECTION,
    /// MOD_SUICIDE)`: flown into a boundary or a lane with no pilot or pieces missing.
    Destroyed,
    /// The boundary's point linked, for the clients to know where the ship turns
    /// (`trap->LinkEntity(ent)`).
    Turned { point: u16 },
    /// The jump: the ship (and its pilot) put at `origin` facing `angles`
    /// (`TeleportPlayer`), and the hyperspace's end sound on the ship.
    Jumped { origin: [f32; 3], angles: [f32; 3] },
}

/// A client's `inSpaceIndex` names a space trigger (neither 0 nor `ENTITYNUM_NONE`, which a
/// rider hidden in its ship has).
pub fn in_space(index: u16) -> bool {
    index != 0 && index != ENTITYNUM_NONE
}

/// `space_touch` (`g_trigger.c:1461-1496`) for any client: `hidden_in_ship` a player
/// inside a ship that hides it (protected from space); otherwise a client whose origin is
/// in the trigger is in space from now, its air half a second from the first touch.
/// `index` and `suffocation` are its `inSpaceIndex` and `inSpaceSuffocation`.
pub fn space_touch(
    trigger: &ShipTrigger,
    origin: [f32; 3],
    hidden_in_ship: bool,
    index: &mut u16,
    suffocation: &mut i32,
    level_time: i32,
) {
    if hidden_in_ship {
        *suffocation = 0;
        *index = ENTITYNUM_NONE;
        return;
    }
    if !trigger.holds(origin) {
        return;
    }
    if *index == 0 || *index == ENTITYNUM_NONE {
        *suffocation = level_time + INITIAL_SUFFOCATION_DELAY;
    }
    *index = trigger.number;
}

/// `shipboundary_touch` (`g_trigger.c:1506-1542`): a ship not jumping is turned toward
/// the boundary's point for twice its travel time — or destroyed with no pilot or with
/// pieces missing — and the boundary looks for fighters in it for two seconds more.
pub fn boundary_touch(
    trigger: &mut ShipTrigger,
    points: &[Point],
    ship: &mut Ship<'_>,
    level_time: i32,
) -> ShipTouched {
    let ShipTriggerKind::Boundary {
        target,
        travel_time,
        detailed_until,
        ..
    } = &mut trigger.kind
    else {
        return ShipTouched::Nothing;
    };
    let vehicle = &mut *ship.vehicle;
    if vehicle.hyperspace_time != 0
        && level_time - vehicle.hyperspace_time < crate::vehicle_fighter::HYPERSPACE_TIME
    {
        return ShipTouched::Nothing;
    }
    let Some(point) = points.iter().find(|point| point.name == *target) else {
        return ShipTouched::Nothing;
    };
    if !ship.piloted || vehicle.removed_surfaces != 0 {
        return ShipTouched::Destroyed;
    }
    vehicle.turnaround_index = point.number;
    vehicle.turnaround_time = level_time + *travel_time * 2;
    *detailed_until = level_time + 2_000;
    ShipTouched::Turned {
        point: point.number,
    }
}

/// `shipboundary_think` (`g_trigger.c:1545-1579`) is due: every 100 ms, and only while
/// the boundary was touched in the last two seconds. `true` where it looks at the fighters
/// in it now.
pub fn boundary_think_due(trigger: &mut ShipTrigger, level_time: i32) -> bool {
    let ShipTriggerKind::Boundary {
        detailed_until,
        next_think,
        ..
    } = &mut trigger.kind
    else {
        return false;
    };
    if *next_think > level_time {
        return false;
    }
    *next_think = level_time + 100;
    *detailed_until >= level_time
}

/// `hyperspace_touch` (`g_trigger.c:1596-1690`): a ship entering the lane starts its jump
/// (its hyperspace time and the lane's heading) — or is destroyed with no pilot or with
/// pieces missing; a ship already jumping and facing the heading is, three quarters of the
/// way through, put out at the same place around the far point, facing its heading.
pub fn hyperspace_touch(
    trigger: &ShipTrigger,
    points: &[Point],
    ship: &mut Ship<'_>,
    level_time: i32,
) -> ShipTouched {
    let ShipTriggerKind::Hyperspace { from, to } = &trigger.kind else {
        return ShipTouched::Nothing;
    };
    let vehicle = &mut *ship.vehicle;
    let jumping = vehicle.hyperspace_time != 0
        && level_time - vehicle.hyperspace_time < crate::vehicle_fighter::HYPERSPACE_TIME;
    if jumping {
        if *ship.flags2 & EF2_HYPERSPACE == 0 {
            return ShipTouched::Nothing;
        }
        let fraction = (level_time - vehicle.hyperspace_time) as f32
            / crate::vehicle_fighter::HYPERSPACE_TIME as f32;
        if fraction < crate::vehicle_fighter::HYPERSPACE_TELEPORT_FRAC {
            return ShipTouched::Nothing;
        }
        // Once only.
        *ship.flags2 &= !EF2_HYPERSPACE;
        let (Some(from), Some(to)) = (
            points.iter().find(|point| point.name == *from),
            points.iter().find(|point| point.name == *to),
        ) else {
            return ShipTouched::Nothing;
        };
        let away: [f32; 3] = std::array::from_fn(|axis| ship.origin[axis] - from.origin[axis]);
        let axes = |angles: [f32; 3]| {
            let (forward, right) = crate::pmove::flight::flight_axes(angles);
            let up = crate::pmove::flight::angles_to_axis(angles)[2];
            [forward.to_array(), right.to_array(), up]
        };
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let [forward, right, up] = axes(from.angles);
        let offsets = [dot(forward, away), dot(right, away), dot(up, away)];
        let [forward, right, up] = axes(to.angles);
        let mut origin = to.origin;
        for (direction, offset) in [forward, right, up].into_iter().zip(offsets) {
            origin = std::array::from_fn(|axis| origin[axis] + direction[axis] * offset);
        }
        vehicle.hyperspace_angles = to.angles;
        return ShipTouched::Jumped {
            origin,
            angles: to.angles,
        };
    }
    let Some(from) = points.iter().find(|point| point.name == *from) else {
        return ShipTouched::Nothing;
    };
    if !ship.piloted || vehicle.removed_surfaces != 0 {
        return ShipTouched::Destroyed;
    }
    vehicle.hyperspace_angles = from.angles;
    vehicle.hyperspace_time = level_time;
    ShipTouched::Nothing
}

/// `G_RunFrame`'s space for a player (`g_main.c:3173-3203`): out of the trigger (or with
/// the trigger gone) it is out of space; in it past its air, it suffocates now — the caller
/// deals `Q_irand(50, 70)` and chokes it — and next in 100 to 200 ms. `true` where it
/// suffocates this frame.
pub fn space_frame(
    triggers: &ShipTriggers,
    origin: [f32; 3],
    index: &mut u16,
    suffocation: i32,
    level_time: i32,
) -> bool {
    if *index == 0 || *index == ENTITYNUM_NONE {
        return false;
    }
    let inside = triggers
        .triggers
        .iter()
        .find(|trigger| trigger.number == *index)
        .is_some_and(|trigger| trigger.holds(origin));
    if !inside {
        *index = 0;
        return false;
    }
    suffocation < level_time
}

/// `G_RunFrame`'s suffocation (`g_main.c:3186-3201`), once [`space_frame`] says it is due:
/// the damage dealt (`Q_irand(50, 70)`, `DAMAGE_NO_ARMOR`, `MOD_SUICIDE`, by the space
/// trigger), drawn before the blow.
pub fn suffocation_damage(irand: &mut impl FnMut(i32, i32) -> i32) -> i32 {
    irand(50, 70)
}

/// After the blow, for one it left alive: the choking sound (`*chokeN.wav`, heard on
/// `CHAN_VOICE`) and the hand at the throat until two seconds on (`HANDEXTEND_CHOKE`).
pub fn choke_sound(irand: &mut impl FnMut(i32, i32) -> i32) -> String {
    format!("*choke{}.wav", irand(1, 3))
}

/// `HANDEXTEND_CHOKE` and how long a suffocating player holds its throat.
pub const HANDEXTEND_CHOKE: u8 = 5;
pub const CHOKE_TIME: i32 = 2_000;

/// When the next breath is missed: 100 to 200 ms on (whether or not the blow landed).
pub fn next_suffocation(irand: &mut impl FnMut(i32, i32) -> i32, level_time: i32) -> i32 {
    level_time + irand(100, 200)
}

//! A fighter coming apart (`codemp/game/g_vehicles.c:1959-2433`): which side of it a knock
//! struck (`G_FlyVehicleImpactDir`), the damage each side has taken (`locationDamage`)
//! against its definition's `health_front` .. `health_left`, the side's damage shown to the
//! clients (`G_VehicleSetDamageLocFlags`, `G_SetVehDamageFlags`: `brokenLimbs`), a side torn
//! off (`G_FlyVehicleDestroySurface`: its surfaces hidden, a blast, the pilot's scream, the
//! electrical shader while it spirals), and a wingless fighter's box
//! (`G_VehicleDamageBoxSizing`).
//!
//! These are the game's, run between moves on the roster's NPC, as `PM_VehicleImpact` and
//! `Update` ask for them ([`crate::vehicle_update::VehicleRequest`]).

use crate::damage::{Attacker, DamageRequest};
use crate::npc_damage::NpcBlow;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;

/// `SHIPSURF_FRONT` .. `SHIPSURF_LEFT`.
pub const FRONT: usize = 0;
pub const BACK: usize = 1;
pub const RIGHT: usize = 2;
pub const LEFT: usize = 3;
/// `SHIPSURF_BROKEN_A` .. `G`.
const BROKEN_A: i32 = 1 << 0;
const BROKEN_B: i32 = 1 << 1;
const BROKEN_C: i32 = 1 << 2;
const BROKEN_D: i32 = 1 << 3;
const BROKEN_E: i32 = 1 << 4;
const BROKEN_F: i32 = 1 << 5;
const BROKEN_G: i32 = 1 << 6;
const ALL_WINGS: i32 = BROKEN_C | BROKEN_D | BROKEN_E | BROKEN_F;
/// `ps.brokenLimbs`, `s.brokenLimbs`; `ps.electrifyTime`.
const PS_BROKEN_LIMBS: usize = 93;
const ES_BROKEN_LIMBS: usize = 79;
const PS_ELECTRIFY_TIME: usize = 73;
/// `CHAN_VOICE`.
const CHAN_VOICE: u32 = 3;
/// `FL_UNDYING`.
const FL_UNDYING: u32 = 0x10_0000;

/// The side's health on the definition (`health_front` ..), `None` for no side.
fn death_point(info: &crate::vehicle_fields::VehicleInfo, side: usize) -> Option<i32> {
    match side {
        FRONT => Some(info.health_front),
        BACK => Some(info.health_back),
        RIGHT => Some(info.health_right),
        LEFT => Some(info.health_left),
        _ => None,
    }
}

/// `G_FlyVehicleDestroySurface`'s surfaces and bits for a side.
fn torn(side: usize) -> (&'static [&'static str], i32) {
    match side {
        FRONT => (&["nose"], BROKEN_G),
        BACK => (
            &["r_wing2", "l_wing2", "r_gear", "l_gear"],
            BROKEN_A | BROKEN_B | BROKEN_D | BROKEN_F,
        ),
        RIGHT => (
            &["r_wing1", "r_wing2", "r_gear"],
            BROKEN_B | BROKEN_E | BROKEN_F,
        ),
        _ => (
            &["l_wing1", "l_wing2", "l_gear"],
            BROKEN_A | BROKEN_C | BROKEN_D,
        ),
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_FlyVehicleImpactDir` (`g_vehicles.c:2017-2102`): the side a knock on the plane
    /// `normal` struck — a wing whose sweep 256 ahead is blocked (with the nose clear), else
    /// by the plane's yaw against the view; `origin` and `view` the parent's at the bump.
    fn impact_side(
        &mut self,
        me: usize,
        normal: [f32; 3],
        origin: [f32; 3],
        view: [f32; 3],
    ) -> usize {
        let npc = &self.actors[me];
        let (number, mask) = (npc.number, npc.clip_mask);
        let removed = npc
            .vehicle
            .as_deref()
            .map_or(0, |vehicle| vehicle.removed_surfaces);
        let (forward, right) = crate::pmove::flight::flight_axes(view);
        let (forward, right) = (forward.to_array(), right.to_array());
        let (low, high) = ([-24.0; 3], [24.0; 3]);
        let along = |from: [f32; 3], scale: f32, direction: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|axis| from[axis] + direction[axis] * scale)
        };
        let Self { host, bodies, .. } = self;
        let mut blocked = |start: [f32; 3]| {
            let found = host.trace(
                start,
                low,
                high,
                along(start, 256.0, forward),
                number,
                mask,
                bodies,
            );
            found.start_solid || found.all_solid || found.fraction != 1.0
        };
        if !blocked(origin) {
            if removed & (BROKEN_E | BROKEN_F) != BROKEN_E | BROKEN_F
                && blocked(along(origin, 128.0, right))
            {
                return RIGHT;
            }
            if removed & (BROKEN_C | BROKEN_D) != BROKEN_C | BROKEN_D
                && blocked(along(origin, -128.0, right))
            {
                return LEFT;
            }
        }
        let relative = crate::player_angle_math::angle_subtract(
            crate::saber_lock::vector_yaw(normal),
            view[1],
        );
        if !(-130.0..=130.0).contains(&relative) {
            FRONT
        } else if relative > 0.0 {
            RIGHT
        } else if relative < 0.0 {
            LEFT
        } else {
            BACK
        }
    }

    /// `G_FlyVehicleSurfaceDestruction` (`g_vehicles.c:2364-2433`): the struck side's
    /// damage gains seven times the knock — all of its health where `force` — and at its
    /// health the side comes off; a second side struck in the same knock too.
    pub(crate) fn surface_destruction(
        &mut self,
        me: usize,
        (normal, origin, view): ([f32; 3], [f32; 3], [f32; 3]),
        magnitude: i32,
        force: bool,
        rider_at: Option<(u16, [f32; 3])>,
    ) {
        let mut side = self.impact_side(me, normal, origin, view);
        let mut again = true;
        loop {
            let Some(vehicle) = self.actors[me].vehicle.as_deref_mut() else {
                return;
            };
            vehicle.surface_damage[side] += magnitude * 7;
            let point = death_point(&vehicle.info, side).unwrap_or(-1);
            if point != -1 {
                if force && vehicle.surface_damage[side] < point {
                    vehicle.surface_damage[side] = point;
                }
                if vehicle.surface_damage[side] >= point {
                    if self.destroy_surface(me, side, origin, rider_at) {
                        self.set_damage_location_flags(me, side);
                    }
                } else {
                    self.set_damage_location_flags(me, side);
                }
            }
            if !again {
                return;
            }
            let second = self.impact_side(me, normal, origin, view);
            if second == side {
                return;
            }
            again = false;
            side = second;
        }
    }

    /// `G_FlyVehicleDestroySurface` (`g_vehicles.c:2276-2362`): the side's surfaces
    /// hidden, the pilot's scream the first time, its bits removed, a blast of 100 around
    /// it (`ps.origin` at the bump) that spares it, and the electrical shader for ten seconds.
    fn destroy_surface(
        &mut self,
        me: usize,
        side: usize,
        origin: [f32; 3],
        rider_at: Option<(u16, [f32; 3])>,
    ) -> bool {
        let level_time = self.level_time;
        let (names, bits) = torn(side);
        for name in names.iter().rev() {
            crate::npc_begin::set_surface(&mut self.actors[me], name, false, &mut *self.host);
        }
        let npc = &mut self.actors[me];
        let number = npc.number;
        let Some(vehicle) = npc.vehicle.as_deref_mut() else {
            return false;
        };
        let first = vehicle.removed_surfaces == 0;
        let pilot = vehicle.pilot;
        vehicle.removed_surfaces |= bits;
        if first && let Some(pilot) = pilot {
            // "make the pilot scream to his death".
            let sound = self.host.sound_index(b"*falling1.wav");
            let at = rider_at
                .filter(|(number, _)| *number == pilot)
                .map_or(origin, |(_, at)| at);
            let mut event = crate::knockdown::entity_sound(at, pilot, CHAN_VOICE);
            event.parameter = u32::from(sound);
            self.host.raise(event);
        }
        self.vehicle_blast(
            number,
            origin,
            100,
            500.0,
            Some(number),
            Some(number),
            rider_at,
        );
        self.actors[me]
            .player
            .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 10_000) as u32);
        true
    }

    /// `G_VehicleSetDamageLocFlags` (`g_vehicles.c:2215-2274`): the side's damage against
    /// its light and heavy marks (a quarter of and the malfunction armour's share of its
    /// health, else 14 and 66 per cent), shown in `brokenLimbs`.
    fn set_damage_location_flags(&mut self, me: usize, side: usize) {
        let Some(vehicle) = self.actors[me].vehicle.as_deref() else {
            return;
        };
        let info = &vehicle.info;
        let Some(point) = death_point(info, side) else {
            return;
        };
        let (light, heavy) = if info.malfunction_armor_level != 0 && info.armor != 0 {
            let share = (info.malfunction_armor_level as f32 / info.armor as f32).min(0.99);
            (
                (f64::from(point as f32 * share * 0.25)).ceil() as i32,
                (f64::from(point as f32 * share)).ceil() as i32,
            )
        } else {
            (
                (f64::from(point as f32 * 0.14)).ceil() as i32,
                (f64::from(point as f32 * 0.66)).ceil() as i32,
            )
        };
        let taken = vehicle.surface_damage[side];
        let level = if taken >= point {
            3
        } else if taken <= light {
            1
        } else if taken <= heavy {
            2
        } else {
            return;
        };
        self.set_damage_flags(me, side, level);
    }

    /// `G_SetVehDamageFlags` (`g_vehicles.c:2135-2213`): the side's light and heavy bits in
    /// `brokenLimbs` for `level` (0 none, 1 light, 2 heavy, 3 gone), copied to the entity;
    /// the back gone blows up the droid unit, the back heavily damaged makes it mortal.
    fn set_damage_flags(&mut self, me: usize, side: usize, level: i32) {
        let npc = &mut self.actors[me];
        let (light, heavy) = (1u32 << side, 1u32 << (4 + side));
        let mut limbs = npc.player.raw_field(PS_BROKEN_LIMBS).unwrap_or(0);
        limbs = match level {
            3 => limbs | heavy | light,
            2 => (limbs | heavy) & !light,
            1 => (limbs | light) & !heavy,
            _ => limbs & !(heavy | light),
        };
        npc.player.set_raw_field(PS_BROKEN_LIMBS, limbs);
        npc.state.set_raw_field(ES_BROKEN_LIMBS, limbs);
        if side != BACK || level < 2 {
            return;
        }
        let enemy = npc.mind.enemy;
        let Some(droid) = npc
            .vehicle
            .as_deref()
            .and_then(|vehicle| vehicle.droid_unit)
        else {
            return;
        };
        let Some(at) = self.actors.iter().position(|other| other.number == droid) else {
            return;
        };
        let undying = self.actors[at].flags & FL_UNDYING != 0;
        if level == 3 && (undying || self.actors[at].health > 0) {
            // "boom": mortal, then blown up by whoever the ship last fought.
            self.actors[at].flags &= !FL_UNDYING;
            let attacker = enemy.map(|number| self.attacker_of(number));
            let request = DamageRequest {
                level_time: self.level_time,
                attacker,
                direction: None,
                point: None,
                damage: 99_999,
                flags: 0,
                means: crate::means_of_death::MOD_UNKNOWN,
            };
            self.damage(
                at,
                NpcBlow {
                    request,
                    spared_by_master: false,
                    surface: None,
                },
            );
        } else if level == 2 && undying {
            self.actors[at].flags &= !FL_UNDYING;
        }
    }

    /// `G_VehicleDamageBoxSizing` (`g_vehicles.c:1959-2015`): a fighter with all its wings
    /// gone shrinks to a box around its body, 256 along and 32 across — if that is clear;
    /// otherwise it dies (9999, no protection, `MOD_SUICIDE`, by itself).
    pub(crate) fn damage_box_sizing(&mut self, me: usize) {
        let npc = &self.actors[me];
        let Some(vehicle) = npc.vehicle.as_deref() else {
            return;
        };
        if vehicle.removed_surfaces & ALL_WINGS != ALL_WINGS {
            return;
        }
        let axis = crate::pmove::flight::angles_to_axis(vehicle.orientation);
        let (forward, right, up) = (axis[0], axis[1].map(|value| -value), axis[2]);
        let along = |from: [f32; 3], scale: f32, direction: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|at| from[at] + direction[at] * scale)
        };
        // The reference's arithmetic as it is: the back is built from the nose, so the
        // box runs from the body's right front to 32 above it.
        let nose = along(
            along(along([0.0; 3], 256.0, forward), 32.0, right),
            32.0,
            up,
        );
        let back = along(nose, -32.0, up);
        let (number, mask, origin) = (npc.number, npc.clip_mask, npc.player.origin());
        let Self { host, bodies, .. } = self;
        let found = host.trace(origin, back, nose, origin, number, mask, bodies);
        if !found.all_solid && !found.start_solid && found.fraction == 1.0 {
            let npc = &mut self.actors[me];
            npc.mins = back;
            npc.maxs = nose;
        } else {
            let npc = &self.actors[me];
            let attacker = Attacker {
                npc: true,
                client: number,
                max_health: npc.max_health,
                team: npc.session_team,
                saber_knockback: [0.0; 4],
            };
            let request = DamageRequest {
                level_time: self.level_time,
                attacker: Some(attacker),
                direction: None,
                point: Some(origin),
                damage: 9_999,
                flags: crate::damage::DAMAGE_NO_PROTECTION,
                means: crate::means_of_death::MOD_SUICIDE,
            };
            self.damage(
                me,
                NpcBlow {
                    request,
                    spared_by_master: false,
                    surface: None,
                },
            );
        }
    }

    /// The attacker `G_Damage` is handed for entity `number` from the roster's side: an NPC
    /// as it knows it, a player as the host's bodies show it (at the stock handicap).
    pub(crate) fn attacker_of(&self, number: u16) -> Attacker {
        match self.actors.iter().find(|npc| npc.number == number) {
            Some(npc) => Attacker {
                npc: true,
                client: number,
                max_health: npc.max_health,
                team: npc.session_team,
                saber_knockback: [0.0; 4],
            },
            None => {
                let team = self
                    .host
                    .players()
                    .iter()
                    .find(|body| body.number == number)
                    .map_or(0, |body| body.session_team);
                Attacker {
                    npc: false,
                    client: number,
                    max_health: self.host.player_max_health(number),
                    team,
                    saber_knockback: [0.0; 4],
                }
            }
        }
    }
}

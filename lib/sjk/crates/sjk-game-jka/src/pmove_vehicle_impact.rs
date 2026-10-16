//! `PM_VehicleImpact` (OpenJK `codemp/game/bg_slidemove.c:69-572`): a vehicle's move that
//! bumps into something.
//!
//! Each bump of a vehicle's slide move (`bg_slidemove.c:731-738`) is judged where it happens,
//! with the velocity and origin of that moment: what the judgement does to the move itself
//! — a speeder's speed cut to a frame's worth, a fighter pushed off what it struck and left
//! turning away (`m_vFullAngleVelocity`), the knock's debounce, the vehicle crashing — is
//! done at once, since the rest of the slide reads it. What it does to the world (the
//! vehicle's own damage and its surfaces coming off, what it rammed, another fighter it
//! turned, the impact effect) is kept ([`JudgedImpact`]) and handed to the game once the
//! move is over ([`VehicleGame::impact`]). What differs between the server and a client's
//! prediction (`_GAME`/`_CGAME`) is the config's `authoritative` flag and the game's end.
//!
//! What the judgement reads of the entity struck is the caller's ([`ImpactBody`], through
//! [`VehicleGame::impact_bodies`]): an entity it does not list is judged as an entity that
//! is nothing in particular (no brush, no body, no vehicle).

use super::vehicle::VehicleGame;
use super::*;
use crate::vehicle_fields::kind;

/// How many bumps one move keeps: three slide moves of four bumps each at most.
pub(super) const MOST_IMPACTS: usize = 12;
/// `MAX_IMPACT_TURN_ANGLE`.
const MAX_IMPACT_TURN_ANGLE: f32 = 45.0;

/// What a vehicle's impact reads of the entity it struck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactBody {
    /// Its entity number.
    pub number: u16,
    /// What it is.
    pub class: ImpactClass,
    /// `r.currentOrigin` (the server's) or `s.origin` (a client's).
    pub origin: [f32; 3],
    /// `client->ps.speed` (or `s.speed` without a client, and on a client).
    pub speed: f32,
    /// `r.ownerNum` (`s.owner` on a client): a missile's shooter.
    pub owner: u16,
    /// `takedamage` (and `inuse`).
    pub takes_damage: bool,
}

/// What an entity a vehicle struck is, as `PM_VehicleImpact` tells them apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImpactClass {
    /// A brush entity (`SOLID_BMODEL`, `r.bmodel`); `rotating_impact` for a turning
    /// `func_rotating` with `IMPACT` (16), which takes a fighter's surfaces off.
    Brush { rotating_impact: bool },
    /// `ET_TERRAIN`; `spares_vehicles` its spawnflag 1.
    Terrain { spares_vehicles: bool },
    /// A player (`ET_PLAYER`).
    Player,
    /// An NPC that is no vehicle (`ET_NPC`).
    Npc,
    /// A vehicle NPC (`CLASS_VEHICLE`) of `kind` ([`kind`]); `landed_or_suspended` a
    /// fighter that `FighterIsLanded` or hangs `SUSPENDED` (never turned by a knock);
    /// `mass`, `orientation` and `velocity` for the knock the server gives it back.
    Vehicle {
        kind: i32,
        landed_or_suspended: bool,
        mass: i32,
        orientation: [f32; 3],
        velocity: [f32; 3],
    },
    /// A missile (`ET_MISSILE`).
    Missile,
    /// Anything else.
    Other,
}

impl ImpactBody {
    /// The world (`ENTITYNUM_WORLD`): a brush.
    pub const WORLD: Self = Self {
        number: ENTITY_NUMBER_WORLD,
        class: ImpactClass::Brush {
            rotating_impact: false,
        },
        origin: [0.0; 3],
        speed: 0.0,
        owner: ENTITY_NUMBER_NONE,
        takes_damage: false,
    };

    /// An entity the caller did not describe.
    fn unknown(number: u16) -> Self {
        Self {
            number,
            class: ImpactClass::Other,
            origin: [0.0; 3],
            speed: 0.0,
            owner: ENTITY_NUMBER_NONE,
            takes_damage: false,
        }
    }

    /// `s.solid == SOLID_BMODEL` (`r.bmodel`), which a vehicle bounces off.
    fn brush(&self) -> bool {
        matches!(self.class, ImpactClass::Brush { .. })
    }

    /// `s.NPC_class == CLASS_VEHICLE`.
    fn vehicle(&self) -> bool {
        matches!(self.class, ImpactClass::Vehicle { .. })
    }
}

/// One bump of a vehicle's move: what it struck, the plane, and the vehicle's velocity
/// and origin at that moment (`pm->ps->velocity`, `pm->ps->origin`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleImpact {
    /// `trace->entityNum`.
    pub entity: u16,
    /// `trace->plane.normal`.
    pub normal: [f32; 3],
    pub velocity: [f32; 3],
    pub origin: [f32; 3],
}

impl VehicleImpact {
    /// `magnitude`: the vehicle's speed times its `mass`, over 50.
    pub fn magnitude(&self, mass: i32) -> f32 {
        Vec3::from_array(self.velocity).length() * mass as f32 / 50.0
    }
}

/// A bump judged, for the game's end ([`VehicleGame::impact`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JudgedImpact {
    /// The bump.
    pub impact: VehicleImpact,
    /// What was struck, as the caller described it.
    pub body: ImpactBody,
    /// What the game is to do about it.
    pub outcome: ImpactOutcome,
}

impl Default for JudgedImpact {
    fn default() -> Self {
        Self {
            impact: VehicleImpact::default(),
            body: ImpactBody::WORLD,
            outcome: ImpactOutcome::Scrape,
        }
    }
}

/// What a judged bump leaves for the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImpactOutcome {
    /// A fighter with pieces missing that struck a vehicle, or a brush head on: dead
    /// (`G_Damage(self, killer, killer, NULL, origin, 999999, DAMAGE_NO_ARMOR,
    /// MOD_FALLING)`, the killer its last attacker while it is remembered, else none).
    Wrecked,
    /// The knock (`bg_slidemove.c:440-552`): the effect, the vehicle's own damage —
    /// `damage` the magnitude over its toughness (a fighter's multiplied), `None` where
    /// terrain spares it — its surfaces (`force` a rotating mover's), what it struck
    /// damaged (`ram`, the magnitude over the toughness), and another fighter knocked back
    /// (`turned`).
    Knock {
        damage: Option<f32>,
        force: bool,
        ram: Option<f32>,
        turned: Option<TurnedFighter>,
    },
    /// A client's end (`_CGAME`): the effect, crashing.
    Scrape,
}

/// Another fighter a fighter struck, knocked back by the server (`bg_slidemove.c:357-431`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnedFighter {
    /// The push added to its velocity.
    pub push: [f32; 3],
    /// Its `m_vFullAngleVelocity`'s pitch and roll, where they change.
    pub pitch: Option<f32>,
    pub roll: Option<f32>,
}

/// A float made an `int` as the reference's x86 build makes it (`cvttss2si`): truncated,
/// and the "integer indefinite" `INT_MIN` for anything out of range — as a definition
/// without `toughness` (0 by `BG_VehicleSetDefaults`' `memset`) divides a knock's
/// magnitude by zero.
pub fn x86_int(value: f32) -> i32 {
    if value.is_finite() && (-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        value as i32
    } else {
        i32::MIN
    }
}

/// `VectorNormalize2`: `v` scaled to unit length, zero for zero.
fn normalized(v: [f32; 3]) -> [f32; 3] {
    let mut out = v;
    crate::player_angle_math::normalize(&mut out);
    out
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The turn a knock gives (`bg_slidemove.c:305-343`, `384-426`): the pitch and roll the
/// full angle velocity takes toward `away` from `orientation`, `strength` strong, over
/// `divider`, by the frame's time.
fn turn_away(
    away: [f32; 3],
    orientation: [f32; 3],
    strength: f32,
    divider: f32,
    modifier: f32,
) -> (Option<f32>, Option<f32>) {
    let angles = crate::player_angle_math::vector_angles(away);
    let delta: [f32; 3] = std::array::from_fn(|axis| {
        crate::player_angle_math::angle_subtract(angles[axis], orientation[axis])
    });
    let pitch = (away[2] != 0.0).then(|| {
        let turn = (strength * delta[0]).clamp(-MAX_IMPACT_TURN_ANGLE, MAX_IMPACT_TURN_ANGLE);
        crate::player_angle_math::normalized_angle(orientation[0] + turn / divider * modifier)
    });
    let roll = (away[0] != 0.0 || away[1] != 0.0).then(|| {
        let turn = (strength * delta[1]).clamp(-MAX_IMPACT_TURN_ANGLE, MAX_IMPACT_TURN_ANGLE);
        crate::player_angle_math::normalized_angle(orientation[2] - turn / divider * modifier)
    });
    (pitch, roll)
}

impl Predictor {
    /// What the entity numbered `number` is, as the caller described it.
    fn impact_body(&self, number: u16) -> ImpactBody {
        if number == ENTITY_NUMBER_WORLD {
            return ImpactBody::WORLD;
        }
        self.impact_bodies
            .iter()
            .find(|body| body.number == number)
            .copied()
            .unwrap_or_else(|| ImpactBody::unknown(number))
    }

    /// `PM_VehicleImpact` for a bump of the slide move, where this is a vehicle's move:
    /// judged now (the slide's `velocity` pushed where a fighter bounces), kept for the
    /// game's end.
    pub(super) fn record_vehicle_impact(
        &mut self,
        trace: &MovementTrace,
        velocity: &mut Vec3,
        seconds: f32,
    ) {
        if !self.vehicle_move() || usize::from(self.impact_count) >= MOST_IMPACTS {
            return;
        }
        let impact = VehicleImpact {
            entity: trace.entity_number,
            normal: trace.plane_normal,
            velocity: velocity.to_array(),
            origin: self.state.origin,
        };
        let body = self.impact_body(trace.entity_number);
        let server = self.config.authoritative;
        let command_time = self.state.command_time;
        let mut v = velocity.to_array();
        let Some(outcome) =
            self.judge_impact(&impact, &body, &mut v, server, command_time, seconds)
        else {
            return;
        };
        *velocity = Vec3::from_array(v);
        self.impacts[usize::from(self.impact_count)] = JudgedImpact {
            impact,
            body,
            outcome,
        };
        self.impact_count += 1;
    }

    /// The judgement (`bg_slidemove.c:69-572`), for the move's own state; `None` where the
    /// bump leaves the game nothing to do.
    fn judge_impact(
        &mut self,
        impact: &VehicleImpact,
        body: &ImpactBody,
        velocity: &mut [f32; 3],
        server: bool,
        command_time: i32,
        seconds: f32,
    ) -> Option<ImpactOutcome> {
        let speed = self.state.speed;
        let Some(vehicle) = self.vehicle.as_deref_mut() else {
            return None;
        };
        let info = std::sync::Arc::clone(&vehicle.info);
        let fighter = info.kind == kind::FIGHTER;
        let magnitude = impact.magnitude(info.mass);
        let mut force = false;
        if server {
            // A missile of the pilot's own is passed through.
            if matches!(body.class, ImpactClass::Missile)
                && vehicle.pilot.is_some_and(|pilot| body.owner == pilot)
            {
                return None;
            }
            if vehicle.removed_surfaces != 0 {
                // "spiralling to our deaths, explode on any solid impact".
                if body.vehicle() {
                    return Some(ImpactOutcome::Wrecked);
                }
                if impact.normal != [0.0; 3]
                    && (body.number == ENTITY_NUMBER_WORLD || body.brush())
                    && dot(normalized(*velocity), impact.normal) <= -0.7
                {
                    return Some(ImpactOutcome::Wrecked);
                }
            }
        }
        let [x, y, z] = *velocity;
        if server
            && matches!(
                body.class,
                ImpactClass::Brush {
                    rotating_impact: true
                }
            )
        {
            // A turning `func_rotating` that destroys what touches it.
            force = true;
        } else if x.abs() + y.abs() < 100.0 && z > -100.0 {
            // "we're landing, we're cool" — but a fighter always smacks players.
            if !(server && fighter && matches!(body.class, ImpactClass::Player | ImpactClass::Npc))
            {
                return None;
            }
        }
        if !(matches!(info.kind, kind::SPEEDER | kind::FIGHTER) && (magnitude >= 100.0 || force)) {
            return None;
        }
        if vehicle.hit_debounce >= command_time && !force {
            return None;
        }
        let mut turned = None;
        if vehicle.removed_surfaces == 0 && !force {
            let half_speed = speed * 0.5;
            let mut bounce = [0.0f32; 3];
            let (mut turn, mut turn_other) = (false, false);
            if (body.number == ENTITY_NUMBER_WORLD || body.brush()) && impact.normal != [0.0; 3] {
                if info.kind == kind::SPEEDER {
                    self.state.speed *= seconds;
                    bounce = impact.normal;
                } else if impact.normal[2] >= crate::vehicle_fighter::MIN_LANDING_SLOPE
                    && vehicle.land_trace.fraction < 1.0
                    && speed <= crate::vehicle_fighter::MIN_LANDING_SPEED
                {
                    // "could land here, don't bounce off, in fact, return altogether!"
                    return None;
                } else {
                    turn = fighter;
                    bounce = impact.normal;
                }
            } else if fighter
                && matches!(
                    body.class,
                    ImpactClass::Vehicle {
                        kind: kind::FIGHTER,
                        ..
                    }
                )
            {
                // Two fighters: turned away from each other.
                turn = true;
                turn_other = true;
                bounce = normalized(std::array::from_fn(|axis| {
                    self.state.origin[axis] - body.origin[axis]
                }));
            }
            if turn {
                let vehicle = self
                    .vehicle
                    .as_deref_mut()
                    .expect("the vehicle of the move");
                let mass = info.mass as f32;
                let mut push = if !turn_other {
                    bounce.map(|axis| axis * (speed * 0.25 / mass))
                } else if server {
                    let push = bounce.map(|axis| axis * ((speed + body.speed) * 0.5));
                    push.map(|axis| axis * (half_speed / mass))
                        .map(|axis| axis * 0.1)
                } else {
                    // The client's build scales the bounce itself and leaves the push zero.
                    bounce = bounce.map(|axis| axis * ((speed + body.speed) * 0.5));
                    [0.0; 3]
                };
                let mut bounce_dot = dot(normalized(*velocity), bounce) * -1.0;
                if bounce_dot < 0.1 {
                    bounce_dot = 0.1;
                }
                push = push.map(|axis| axis * bounce_dot);
                *velocity = std::array::from_fn(|axis| velocity[axis] + push[axis]);
                let mut divider = mass / 400.0;
                if turn_other {
                    divider *= 4.0;
                }
                if divider < 0.5 {
                    divider = 0.5;
                }
                let strength = (magnitude / 2_000.0).clamp(0.1, 2.0);
                let modifier = vehicle.time_modifier;
                let (pitch, roll) =
                    turn_away(bounce, vehicle.orientation, strength, divider, modifier);
                if let Some(pitch) = pitch {
                    vehicle.full_angle_velocity[0] = pitch;
                }
                if let Some(roll) = roll {
                    vehicle.full_angle_velocity[2] = roll;
                }
                if server
                    && turn_other
                    && let ImpactClass::Vehicle {
                        landed_or_suspended: false,
                        mass: other_mass,
                        orientation,
                        velocity: other_velocity,
                        ..
                    } = body.class
                {
                    // "turn the guy we hit away from us, too".
                    let other_speed = body.speed;
                    let back = bounce.map(|axis| -axis);
                    let push = back
                        .map(|axis| axis * ((speed + other_speed) * 0.5))
                        .map(|axis| axis * (other_speed * 0.5 / other_mass as f32));
                    let mut other_dot = dot(normalized(other_velocity), back) * -1.0;
                    if other_dot < 0.1 {
                        other_dot = 0.1;
                    }
                    let push = push.map(|axis| axis * other_dot);
                    let mut other_divider = other_mass as f32 / 400.0 * 4.0;
                    if other_divider < 0.5 {
                        other_divider = 0.5;
                    }
                    let (pitch, roll) =
                        turn_away(back, orientation, strength, other_divider, modifier);
                    turned = Some(TurnedFighter { push, pitch, roll });
                }
            }
        }
        let vehicle = self
            .vehicle
            .as_deref_mut()
            .expect("the vehicle of the move");
        if !server {
            // "don't hit your own missiles!"
            if body.owner == self.state.client_num {
                return None;
            }
            vehicle.hit_debounce = command_time + 200;
            vehicle.flags |= crate::vehicle::flags::CRASHING;
            return Some(ImpactOutcome::Scrape);
        }
        vehicle.hit_debounce = command_time + 200;
        let mut knock = magnitude / (info.toughness * 50.0);
        let damage = if matches!(
            body.class,
            ImpactClass::Terrain {
                spares_vehicles: true
            }
        ) && !fighter
        {
            None
        } else {
            if fighter {
                // "increase the damage...": by its pitch, less for ramming what takes damage
                // (half again for another vehicle).
                let mut multiplier = (vehicle.orientation[0] * 0.1).max(1.0);
                if body.takes_damage {
                    multiplier = if body.vehicle() { 1.5 } else { 0.5 };
                }
                knock *= multiplier;
            }
            vehicle.last_impact_damage = x86_int(knock);
            vehicle.flags |= crate::vehicle::flags::CRASHING;
            Some(knock)
        };
        // What it struck is damaged where it takes damage, which the game knows.
        let ram = (body.number < ENTITY_NUMBER_WORLD).then_some(knock);
        Some(ImpactOutcome::Knock {
            damage,
            force,
            ram,
            turned,
        })
    }

    /// The bumps the move judged, handed to the game in order.
    pub(super) fn vehicle_impacts<'d>(
        &mut self,
        command: &UserCommand,
        game: Option<&mut (dyn VehicleGame + 'd)>,
    ) {
        let count = usize::from(std::mem::take(&mut self.impact_count));
        let Some(game) = game else { return };
        let Self {
            vehicle,
            state,
            impacts,
            ..
        } = self;
        let Some(vehicle) = vehicle.as_deref_mut() else {
            return;
        };
        for judged in &impacts[..count] {
            game.impact(vehicle, state, judged, command.server_time);
        }
    }
}

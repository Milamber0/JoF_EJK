//! The game's think for a vehicle inside its move (`codemp/game/g_vehicles.c`'s `Update`,
//! with `SpeederNPC.c`'s and `AnimalNPC.c`'s wrappers, `Animate` and `UpdateRider`): the
//! ammunition and shields recharged, the owner kept in step, dying ([`crate::vehicle_death`]),
//! the no-pilot death's timer, the riders checked and the command stored, the weapon links,
//! the steering and speed ([`crate::vehicle_move`]) with the parent's and the pilot's view
//! set to the vehicle's orientation, the shift sounds' draws, the move's direction, and
//! the animations of the animal and its rider ([`crate::vehicle_riders`]).
//!
//! [`VehicleThink`] is what `PmoveSingle` calls ([`crate::pmove::vehicle::VehicleGame`]);
//! what it reaches beyond the vehicle and its move — the parent's player state and entity,
//! the pilot ([`Rider`]), the level's time and generator — it is handed. What the think
//! asks of the world that the move cannot do in its middle (a death, an explosion, a
//! sound registered) it leaves in [`VehicleThink::requests`], for the caller to do once the
//! move is over.
//!
//! A fighter's `Update` opens with `BG_FighterUpdate` (its riders ghosted, its gravity,
//! the landing trace) and animates its wings and gears ([`crate::vehicle_fighter`],
//! [`crate::vehicle_fighter_orient`]).
//!
//! The turrets think in the game's hands, in their turn ([`VehicleRequest::Turrets`],
//! [`crate::vehicle_turrets`]). Not here yet: passengers other than the pilot.

use crate::pmove::vehicle::VehicleGame;
use crate::pmove::{MovementCollision, MovementState};
use crate::pmove_anim::{
    AnimationLengths, SETANIM_FLAG_HOLD, SETANIM_FLAG_HOLDLESS, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS,
};
use crate::vehicle::{Vehicle, flags};
use crate::vehicle_board::Parent;
use crate::vehicle_fields::{VEHICLE_WEAPONS, kind};
use crate::vehicle_move::{Steering, process_move_commands, process_orient_commands};
use crate::vehicle_rider::Rider;
use sjk_protocol::{EntityState, PlayerState, UserCommand};

/// `STAT_ARMOR`; `ps.activeForcePass`, `ps.electrifyTime`; `s.owner`.
const STAT_ARMOR: usize = 5;
const PS_ACTIVE_FORCE_PASS: usize = 72;
const PS_ELECTRIFY_TIME: usize = 73;
const ES_OWNER: usize = 40;
/// `BUTTON_TALK`, `BUTTON_USE_HOLDABLE`, `BUTTON_WALKING`.
const BUTTON_TALK: u16 = 2;
const BUTTON_USE_HOLDABLE: u16 = 4;
const BUTTON_WALKING: u16 = 16;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;
/// `ROLL`.
const ROLL: usize = 2;
/// The animal's own animations (`anims.h`).
const BOTH_VT_MOUNT_L: u16 = 1_056;
const BOTH_VT_MOUNT_R: u16 = 1_057;
const BOTH_VT_MOUNT_B: u16 = 1_058;
const BOTH_VT_WALK_FWD: u16 = 1_062;
const BOTH_VT_WALK_REV: u16 = 1_063;
const BOTH_VT_RUN_FWD: u16 = 1_066;
const BOTH_VT_BUCK: u16 = 1_076;
const BOTH_VT_TURBO: u16 = 1_078;
const BOTH_VT_IDLE1: u16 = 1_082;

/// What a vehicle's think asks of the world once its move is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehicleRequest {
    /// `G_Damage(parent, parent, parent, ..., 99999, DAMAGE_NO_PROTECTION, MOD_SUICIDE)`:
    /// left with no pilot too long, or too far from its last one.
    DieUnpiloted,
    /// `G_Damage(rider, ...)`: a rider killed with the vehicle — `killRiderOnDeath`
    /// (10000, `MOD_SUICIDE`, no attacker), or one still aboard as it blows up (999,
    /// `DAMAGE_NO_PROTECTION`, the vehicle the attacker).
    KillRider {
        rider: u16,
        damage: i32,
        by_vehicle: bool,
    },
    /// `StartDeathDelay` of a flammable vehicle: its loop sound, the fire's.
    FireLoop,
    /// `AttachRiders`' droid unit (`g_vehicles.c:1876-1905`) at the end of a slice of the
    /// move: on its bolt of the parent at `origin` (`r.currentOrigin`) facing `yaw`
    /// ([`crate::vehicle_droid`]).
    AttachDroid { origin: [f32; 3], yaw: f32 },
    /// `EjectAll`'s end: `G_EjectDroidUnit`, killing the droid where the vehicle kills its
    /// riders ([`crate::vehicle_droid`]).
    EjectDroid { kill: bool },
    /// `DeathUpdate`'s explosion: its effect where the vehicle is (where it has one) and the
    /// scorch mark where the trace down found the ground, then `G_RadiusDamage` from
    /// `blast` — and the vehicle freed a frame on.
    Explode {
        at: [f32; 3],
        effect: bool,
        mark: Option<[f32; 3]>,
        blast: Option<[f32; 3]>,
    },
    /// A walker's weight on what it stands on (`g_active.c:3035-3046`, before its move):
    /// `G_Damage(under, vehicle, vehicle, down, under's origin, 100, 0, MOD_CRUSH)` where it
    /// has health and takes damage.
    Crush { under: u16 },
    /// `PM_VehicleImpact`'s knock on the vehicle itself: `G_Damage(vehicle, NULL, NULL,
    /// NULL, origin, damage, DAMAGE_NO_ARMOR, MOD_FALLING)` where it was at the bump.
    Crash { at: [f32; 3], damage: i32 },
    /// `PM_VehicleImpact`'s knock on what it struck (`bg_slidemove.c:506-552`): `magnitude`
    /// already over the vehicle's toughness, its velocity and origin at the bump;
    /// `humanoid_scale` what a player's or an NPC's damage is multiplied by (2000 from a
    /// fighter, 40 from anything else).
    Ram {
        target: u16,
        magnitude: f32,
        velocity: [f32; 3],
        at: [f32; 3],
        command_time: i32,
        humanoid_scale: f32,
    },
    /// `G_FlyVehicleSurfaceDestruction(parent, trace, magnitude, force)`: a fighter's
    /// knock taking its surfaces off: the bump's plane `normal`, the parent where it was
    /// and its view (`ps.origin`, `ps.viewangles` in the middle of the move), and its pilot
    /// where it was linked before the move carried it along (`rider_at`).
    Surfaces {
        normal: [f32; 3],
        at: [f32; 3],
        view: [f32; 3],
        magnitude: i32,
        force: bool,
        rider_at: Option<(u16, [f32; 3])>,
    },
    /// The server's knock back on another fighter a fighter struck
    /// ([`crate::pmove::vehicle_impact::TurnedFighter`]).
    TurnFighter {
        target: u16,
        turned: crate::pmove::vehicle_impact::TurnedFighter,
    },
    /// `G_CheapWeaponFire`'s `FireWeapon` (`g_active.c:907`, `4502-4505`): the volley of
    /// [`crate::vehicle_weapons::fire`], from the vehicle's `ps.origin` and view as they
    /// stood at the trigger.
    Fire {
        alternate: bool,
        origin: [f32; 3],
        view_angles: [f32; 3],
    },
    /// `VEH_TurretThink` for every turret (`g_vehicles.c:1502-1507`), from the vehicle's
    /// `ps.origin`, view and `m_vOrientation` as they stood then.
    Turrets {
        origin: [f32; 3],
        view_angles: [f32; 3],
        orientation: [f32; 3],
    },
    /// `Update`'s recharge of the turrets' ammunition (`g_vehicles.c:1225-1239`) at the
    /// command's `serverTime`: asked for with the turrets' thinks, which alone read and
    /// spend it, so that each slice of the move sees what the last one's turrets left.
    TurretRecharge { server_time: i32 },
    /// `G_EntitySound(parent, CHAN_AUTO, sound)`: a fighter's take-off, turbo or landing
    /// sound, where the parent was linked (`r.currentOrigin`).
    EntitySound { sound: i32, at: [f32; 3] },
    /// A speeder's turbo (`SpeederNPC.c:136-162`): `G_PlayEffectID(iTurboStartFX)` at each
    /// of its exhausts, the parent's yaw and `ps.origin` as they were.
    TurboStart { origin: [f32; 3], yaw: f32 },
    /// `G_VehicleDamageBoxSizing`: a fighter with its surfaces coming off, resized (or
    /// killed where the smaller box does not fit), before its wear.
    BoxSizing,
    /// `G_Damage(parent, killer, killer, ..., at, damage, flags, means)` on the vehicle
    /// credited to its last attacker while it is remembered (`otherKiller`), else to
    /// `fallback` (the vehicle itself, or none): a fighter that touched ground with pieces
    /// missing (`FighterDamageRoutine`), one wrecked by an impact while spiralling
    /// (`PM_VehicleImpact`).
    Wrecked {
        at: [f32; 3],
        damage: i32,
        flags: u32,
        means: u32,
        fallback_self: bool,
    },
}

/// The game's side of a vehicle's move: its `Update`, `Animate` and `UpdateRider`, and
/// `AttachRiders` ([`VehicleGame`]).
pub struct VehicleThink<'a, 'r> {
    /// `level.time`.
    pub level_time: i32,
    /// The game's generator (`Q_irand`).
    pub rng: &'a mut crate::player_death::Rng,
    /// The parent's entity number.
    pub number: u16,
    /// The parent's player state for what the move does not carry: its armour stat, shield
    /// display, loop sound and `m_iVehicleNum`.
    pub player: &'a mut PlayerState,
    /// The parent's entity: its owner and loop sound.
    pub entity: &'a mut EntityState,
    /// The parent's `client->pers.cmd`, which the think overwrites with the vehicle's.
    pub parent_command: &'a mut UserCommand,
    /// The parent's `health` and `spawnflags`, `r.currentAngles[YAW]`, `r.currentOrigin`,
    /// `r.mins`, `r.maxs`, `clipmask`.
    pub parent_health: i32,
    pub parent_spawnflags: i32,
    pub parent_yaw: f32,
    pub parent_origin: [f32; 3],
    pub parent_mins: [f32; 3],
    pub parent_maxs: [f32; 3],
    pub parent_clip_mask: u32,
    /// The parent's `r.contents`, which a passenger getting off meets.
    pub parent_contents: u32,
    /// The parent's animations, timed by its own skeleton.
    pub lengths: Option<std::sync::Arc<dyn AnimationLengths>>,
    /// The pilot, borrowed for the move, if one rides.
    pub rider: Option<&'a mut Rider<'r>>,
    /// Its passengers, borrowed for the move (`m_ppPassengers`, each by its number).
    pub passengers: &'a mut [Rider<'r>],
    /// Where the last pilot stands, for `NO_PILOT_DIE`'s distance, if it is still in the game.
    pub last_pilot_origin: Option<[f32; 3]>,
    /// Where the driver tag is on the vehicle's model, from its origin along its yaw
    /// (forward, left, up); zero where the host has no model for it.
    pub driver_offset: [f32; 3],
    /// `s.angles` as `SetClientViewAngle` left them on the parent.
    pub entity_angles: Option<[f32; 3]>,
    /// `g_gravity`, which an empty fighter without a gravity of its own falls by.
    pub gravity: f32,
    /// `client->inSpaceIndex` of the parent: in a `trigger_space`.
    pub in_space: bool,
    /// `bg_fighterAltControl`: a player pilot flies a fighter unrestrained.
    pub fighter_alt_control: bool,
    /// What the vehicle's move may bump into ([`crate::pmove::vehicle_impact::ImpactBody`]).
    pub impact_bodies: &'a [crate::pmove::vehicle_impact::ImpactBody],
    /// Where the pilot was linked as the move began (`AttachRiders` carries it on at the
    /// move's end): where a blast in the middle of the move finds it.
    pub rider_origin: Option<[f32; 3]>,
    /// What the move could not do in its middle.
    pub requests: Vec<VehicleRequest>,
}

impl VehicleThink<'_, '_> {
    /// The boxes a passenger's way off meets beside the world: the vehicle's where it is
    /// linked, and each rider's with contents (a hidden one has none) — the pilot's and the
    /// passengers' — as the move began. At most the pilot and ten passengers.
    fn rider_boxes(&self) -> [Option<crate::entity_clip::BoxObstacle>; 12] {
        let mut boxes = [None; 12];
        boxes[0] = (self.parent_contents != 0).then_some(crate::entity_clip::BoxObstacle {
            entity: self.number,
            origin: self.parent_origin,
            bounds: (self.parent_mins, self.parent_maxs),
            contents: self.parent_contents,
            model: None,
        });
        let riders = self
            .rider
            .as_deref()
            .into_iter()
            .chain(self.passengers.iter());
        for (slot, rider) in boxes.iter_mut().skip(1).zip(riders) {
            *slot = (rider.body.contents != 0).then_some(crate::entity_clip::BoxObstacle {
                entity: rider.number,
                origin: rider.movement.origin,
                bounds: (
                    [-15.0, -15.0, crate::vehicle_board::DEFAULT_MINS_2],
                    rider.maxs,
                ),
                contents: rider.body.contents,
                model: None,
            });
        }
        boxes
    }
}

impl VehicleGame for VehicleThink<'_, '_> {
    fn update(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        command: &UserCommand,
        move_dir: &mut [f32; 3],
        _move_box: ([f32; 3], [f32; 3]),
        collision: &dyn MovementCollision,
    ) {
        let piloted = state.vehicle_entity_num != 0;
        if piloted
            && let Some(rider) = self
                .rider
                .as_deref_mut()
                .filter(|rider| rider.vehicle() != 0)
        {
            crate::vehicle_riders::vehicle_view_angles(vehicle, rider, self.fighter_alt_control);
        }
        if vehicle.kind() == kind::FIGHTER {
            self.fighter_update(vehicle, state, collision);
        }
        // A speeder's and an animal's `Update` are the shared one; a speeder then runs its
        // `DeathUpdate` while dying (`SpeederNPC.c:59-72`).
        if self.shared_update(vehicle, state, command, move_dir, collision)
            && vehicle.kind() == kind::SPEEDER
            && vehicle.die_time != 0
        {
            self.death_update(vehicle, state, collision);
        }
        self.animate(vehicle, state);
        if piloted {
            let own = vehicle.ucmd;
            let level_time = self.level_time;
            let mut trace =
                |start, mins, maxs, end, mask| collision.trace(start, mins, maxs, end, mask);
            if let Some(rider) = self
                .rider
                .as_deref_mut()
                .filter(|rider| vehicle.pilot == Some(rider.number))
            {
                let mut parent = parent_view(
                    vehicle,
                    state,
                    &mut *self.player,
                    &mut *self.entity,
                    &mut *self.parent_command,
                    self.number,
                    self.parent_health,
                    self.parent_yaw,
                    self.parent_origin,
                    self.parent_maxs,
                    self.parent_clip_mask,
                );
                crate::vehicle_riders::update_rider(
                    &mut parent,
                    rider,
                    &own,
                    level_time,
                    &mut trace,
                );
            }
            // "update the passengers" (`bg_pmove.c:10953-10967`), each on its own command,
            // the seats read again after every one (a passenger off moves the rest up).
            // A passenger's way off meets what the move passes: the vehicle, which it does not
            // own, and the other riders (`VEH_TryEject` clears only its own owner).
            let riders = self.rider_boxes();
            let mut seat = 0;
            while seat < vehicle.passenger_count.max(0) as usize {
                let number = vehicle.passengers.get(seat).copied().flatten();
                if let Some(passenger) = self
                    .passengers
                    .iter_mut()
                    .find(|passenger| Some(passenger.number) == number && passenger.connected)
                {
                    let command = passenger.command;
                    let me = passenger.number;
                    let mut trace = |start, mins, maxs, end, mask| {
                        let movement = crate::entity_clip::Move {
                            ends: (start, end),
                            bounds: (mins, maxs),
                            content_mask: mask,
                        };
                        let others = riders
                            .iter()
                            .flatten()
                            .copied()
                            .filter(|body| body.entity != me);
                        crate::entity_clip::clip_move_to_entities(
                            collision.trace(start, mins, maxs, end, mask),
                            movement,
                            others,
                            |body| crate::box_sweep::sweep_box(body, start, mins, maxs, end),
                        )
                    };
                    let mut parent = parent_view(
                        vehicle,
                        state,
                        &mut *self.player,
                        &mut *self.entity,
                        &mut *self.parent_command,
                        self.number,
                        self.parent_health,
                        self.parent_yaw,
                        self.parent_origin,
                        self.parent_maxs,
                        self.parent_clip_mask,
                    );
                    crate::vehicle_riders::update_rider(
                        &mut parent,
                        passenger,
                        &command,
                        level_time,
                        &mut trace,
                    );
                }
                seat += 1;
            }
        }
    }

    fn fire(&mut self, vehicle: &mut Vehicle, state: &mut MovementState, alternate: bool) {
        let pilot = self.rider.as_deref().filter(|rider| {
            state.vehicle_entity_num != 0 && rider.number == state.vehicle_entity_num - 1
        });
        if !alternate
            && vehicle.kind() == kind::SPEEDER
            && pilot.is_some_and(|rider| !crate::vehicle_weapons::pilot_may_fire(rider.movement))
        {
            return;
        }
        self.requests.push(VehicleRequest::Fire {
            alternate,
            origin: state.origin,
            view_angles: state.view_angles,
        });
        // `dangerTime`, `invulnerableTimer`: the spawn protection is over.
        state.entity_flags &= !crate::vehicle_weapons::EF_INVULNERABLE;
    }

    fn wake(&mut self) {
        let _ = self.rng.irand(0, 100);
    }

    fn attach_riders(&mut self, vehicle: &mut Vehicle, state: &MovementState) {
        // Its droid unit, in its turn among what the move asked for (a later slice's death
        // blast finds it where this slice put it).
        if vehicle.droid_unit.is_some() {
            self.requests.push(VehicleRequest::AttachDroid {
                origin: self.parent_origin,
                yaw: state.view_angles[1],
            });
        }
        // The passengers on the driver tag too (`g_vehicles.c:1847-1873`).
        for seat in vehicle
            .passengers
            .iter()
            .take(vehicle.passenger_count.max(0) as usize)
            .flatten()
        {
            if let Some(passenger) = self
                .passengers
                .iter_mut()
                .find(|passenger| passenger.number == *seat)
            {
                let origin = crate::vehicle_riders::driver_origin(
                    state.origin,
                    state.view_angles[1],
                    self.driver_offset,
                );
                passenger.movement.origin = origin;
                passenger.set_origin(origin);
            }
        }
        let Some(rider) = self.rider.as_deref_mut() else {
            return;
        };
        crate::vehicle_riders::attach_riders(
            vehicle,
            state.origin,
            state.view_angles[1],
            self.driver_offset,
            rider,
        );
    }

    /// What the move's vehicle may bump into, as the caller described it.
    fn impact_bodies(&mut self, bodies: &mut Vec<crate::pmove::vehicle_impact::ImpactBody>) {
        bodies.clear();
        bodies.extend_from_slice(self.impact_bodies);
    }

    /// `PM_VehicleImpact`'s `_GAME` end (`bg_slidemove.c:69-137`, `446-552`): a wreck
    /// killed; a knock damaging the vehicle by five times its magnitude (the impact effect
    /// is registered by clients only, `VF_EFFECT_CLIENT`, so the server's is none), taking
    /// a fighter's surfaces off, ramming what it struck, turning another fighter.
    fn impact(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        judged: &crate::pmove::vehicle_impact::JudgedImpact,
        command_time: i32,
    ) {
        use crate::pmove::vehicle_impact::{ImpactOutcome, x86_int};
        let impact = &judged.impact;
        match judged.outcome {
            ImpactOutcome::Scrape => {}
            ImpactOutcome::Wrecked => {
                let (flags, means) = (
                    crate::damage::DAMAGE_NO_ARMOR,
                    crate::means_of_death::MOD_FALLING,
                );
                self.requests.push(VehicleRequest::Wrecked {
                    at: impact.origin,
                    damage: 999_999,
                    flags,
                    means,
                    fallback_self: false,
                });
            }
            ImpactOutcome::Knock {
                damage,
                force,
                ram,
                turned,
            } => {
                if let Some(damage) = damage {
                    self.requests.push(VehicleRequest::Crash {
                        at: impact.origin,
                        damage: x86_int(damage * 5.0),
                    });
                    if vehicle.info.surf_destruction != 0 {
                        let rider_at = vehicle.pilot.zip(self.rider_origin);
                        self.requests.push(VehicleRequest::Surfaces {
                            normal: impact.normal,
                            at: impact.origin,
                            view: state.view_angles,
                            magnitude: x86_int(damage),
                            force,
                            rider_at,
                        });
                    }
                }
                if let Some(turned) = turned {
                    self.requests.push(VehicleRequest::TurnFighter {
                        target: impact.entity,
                        turned,
                    });
                }
                if let Some(magnitude) = ram
                    && impact.entity < crate::pmove::ENTITY_NUMBER_WORLD
                {
                    let humanoid_scale = if vehicle.kind() == kind::FIGHTER {
                        2_000.0
                    } else {
                        40.0
                    };
                    self.requests.push(VehicleRequest::Ram {
                        target: impact.entity,
                        magnitude,
                        velocity: impact.velocity,
                        at: impact.origin,
                        command_time,
                        humanoid_scale,
                    });
                }
            }
        }
    }
}

/// The vehicle's parent as the boarding functions reach it, from the move's parts (no
/// spawnflags: only a boarding clears `SUSPENDED`, never a move).
#[allow(clippy::too_many_arguments)]
pub(crate) fn parent_view<'p>(
    vehicle: &'p mut Vehicle,
    state: &'p mut MovementState,
    player: &'p mut PlayerState,
    entity: &'p mut EntityState,
    command: &'p mut UserCommand,
    number: u16,
    health: i32,
    yaw: f32,
    origin: [f32; 3],
    maxs: [f32; 3],
    clip_mask: u32,
) -> Parent<'p> {
    Parent {
        number,
        vehicle,
        state,
        player,
        entity,
        command,
        health,
        spawnflags: None,
        yaw,
        origin,
        maxs,
        clip_mask,
    }
}

impl VehicleThink<'_, '_> {
    /// `BG_FighterUpdate` on the server (`FighterNPC.c:47-116`, `120-137`): the riders
    /// ghosted, then the gravity and the landing trace (the parent's linked box, the trace
    /// the move's own).
    fn fighter_update(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        collision: &dyn MovementCollision,
    ) {
        if let Some(rider) = self
            .rider
            .as_deref_mut()
            .filter(|rider| vehicle.pilot == Some(rider.number))
        {
            crate::vehicle_board::ghost(rider);
        }
        let mut trace =
            |start, mins, maxs, end, mask| collision.trace(start, mins, maxs, end, mask);
        crate::vehicle_fighter::update(
            vehicle,
            state,
            self.gravity,
            self.parent_mins,
            self.parent_maxs,
            &mut trace,
        );
    }

    /// What a fighter's steering reads of the parent and the game on the server.
    fn fighter_steering(
        &self,
        vehicle: &Vehicle,
        state: &MovementState,
    ) -> crate::vehicle_fighter::FighterSteering {
        const PS_EFLAGS: usize = 17;
        const PS_EFLAGS2: usize = 103;
        const EF_DEAD: u32 = 1 << 1;
        let pilot = self
            .rider
            .as_deref()
            .filter(|rider| vehicle.pilot == Some(rider.number));
        let pilot_player = pilot.is_some_and(|rider| rider.number < 32);
        crate::vehicle_fighter::FighterSteering {
            server: true,
            in_space: self.in_space,
            suspended: self.parent_spawnflags & 2 != 0,
            parent_number: self.number,
            hyperspace: self.player.raw_field(PS_EFLAGS2).unwrap_or(0)
                & crate::vehicle_riders::EF2_HYPERSPACE
                != 0,
            dead: self.player.raw_field(PS_EFLAGS).unwrap_or(0) & EF_DEAD != 0,
            unrestrained: self.fighter_alt_control
                && pilot_player
                && vehicle.kind() == kind::FIGHTER
                && pilot.is_some_and(|rider| rider.vehicle() != 0),
            rider_roll: pilot.map_or(state.view_angles[ROLL], |rider| {
                rider.movement.view_angles[ROLL]
            }),
            pilot_player,
        }
    }

    /// What the vehicle's `ProcessMoveCommands` and `ProcessOrientCommands` asked the game
    /// to do, as requests.
    fn cues(
        &mut self,
        vehicle: &Vehicle,
        state: &MovementState,
        moved: crate::vehicle_move::MoveCues,
        oriented: crate::vehicle_fighter_orient::OrientCues,
    ) {
        let info = &vehicle.info;
        if oriented.landed_broken {
            let means = crate::means_of_death::MOD_SUICIDE;
            self.requests.push(VehicleRequest::Wrecked {
                at: state.origin,
                damage: 99_999,
                flags: crate::damage::DAMAGE_NO_ARMOR,
                means,
                fallback_self: true,
            });
        }
        if moved.take_off {
            self.requests.push(VehicleRequest::EntitySound {
                sound: info.sound_take_off,
                at: self.parent_origin,
            });
        }
        if moved.turbo {
            self.requests.push(VehicleRequest::EntitySound {
                sound: info.sound_turbo,
                at: self.parent_origin,
            });
        }
        if moved.turbo_start {
            self.requests.push(VehicleRequest::TurboStart {
                origin: state.origin,
                yaw: state.view_angles[1],
            });
        }
    }

    /// `Update` (`g_vehicles.c:1183-1631`). Whether the vehicle is alive and running.
    fn shared_update(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        command: &UserCommand,
        move_dir: &mut [f32; 3],
        collision: &dyn MovementCollision,
    ) -> bool {
        let info = std::sync::Arc::clone(&vehicle.info);
        let time = self.level_time;
        for slot in 0..VEHICLE_WEAPONS {
            let weapon = &info.weapons[slot];
            let status = &mut vehicle.weapon_status[slot];
            if weapon.id > 0
                && weapon.ammo_recharge_ms != 0
                && status.ammo < weapon.ammo_max
                && command.server_time - status.last_ammo_inc >= weapon.ammo_recharge_ms
            {
                status.last_ammo_inc = command.server_time;
                status.ammo += 1;
                state.ammo[slot] = status.ammo;
            }
        }
        // The turrets' recharge, in its turn among the turrets' thinks
        // ([`VehicleRequest::TurretRecharge`]).
        if info
            .turrets
            .iter()
            .any(|turret| turret.weapon > 0 && turret.ammo_recharge_ms != 0)
        {
            self.requests.push(VehicleRequest::TurretRecharge {
                server_time: command.server_time,
            });
        }
        // The shields recharge (`lastShieldInc` is never moved on: every frame).
        let armor = self.player.stats[STAT_ARMOR] as i32;
        if info.shield_recharge_ms != 0
            && armor > 0
            && armor < info.shields
            && command.server_time - vehicle.last_shield_inc >= info.shield_recharge_ms
        {
            let armor = (armor + 1).min(info.shields);
            self.player.stats[STAT_ARMOR] = armor as u32;
            vehicle.shields = armor;
            update_shields(vehicle, self.player);
        }
        // `s.owner` kept to `r.ownerNum`: the pilot.
        let owner = vehicle.pilot.unwrap_or(ENTITYNUM_NONE);
        self.entity.set_raw_field(ES_OWNER, u32::from(owner));
        vehicle.ps_boarding = vehicle.boarding != 0;
        if vehicle.die_time != 0 {
            // Dying: turned and moved on, then `DeathUpdate` (`g_vehicles.c:1272-1310`).
            vehicle.prev_orientation = vehicle.orientation;
            let steering = self.steering(vehicle, state, command);
            let oriented = process_orient_commands(vehicle, state, &steering);
            self.set_view_angle(vehicle.orientation, state);
            if let Some(rider) = self
                .rider
                .as_deref_mut()
                .filter(|rider| vehicle.pilot == Some(rider.number))
            {
                rider.set_view_angle(vehicle.orientation);
            }
            let moved = process_move_commands(vehicle, state, &steering, move_dir);
            self.cues(vehicle, state, moved, oriented);
            self.set_move_dir(vehicle, move_dir);
            self.death_update(vehicle, state, collision);
            return false;
        }
        if self.parent_health <= 0 {
            // `StartDeathDelay` — at once for a fighter that crashed hard ("explode
            // instantly in inferno-y death") — then `DeathUpdate`.
            let delay = if vehicle.kind() == kind::FIGHTER && vehicle.last_impact_damage > 500 {
                -1
            } else {
                0
            };
            self.start_death_delay(vehicle, delay);
            self.death_update(vehicle, state, collision);
            return false;
        }
        if self.parent_spawnflags & 1 != 0 {
            if vehicle.pilot.is_some() || !vehicle.has_had_pilot {
                if let Some(pilot) = vehicle.pilot
                    && !vehicle.has_had_pilot
                {
                    vehicle.has_had_pilot = true;
                    vehicle.pilot_last_index = i32::from(pilot);
                }
                vehicle.pilot_time = time + vehicle.no_pilot_delay;
            } else if vehicle.pilot_time != 0 {
                match self.last_pilot_origin {
                    None => self.requests.push(VehicleRequest::DieUnpiloted),
                    Some(at) => {
                        let away: [f32; 3] =
                            std::array::from_fn(|axis| state.origin[axis] - at[axis]);
                        let distance =
                            (away[0] * away[0] + away[1] * away[1] + away[2] * away[2]).sqrt();
                        if distance < vehicle.no_pilot_distance {
                            vehicle.pilot_time = time + vehicle.no_pilot_delay;
                        } else if vehicle.pilot_time < time {
                            self.requests.push(VehicleRequest::DieUnpiloted);
                        }
                    }
                }
            }
        }
        // A pilot dead or gone while it boards is thrown off (`g_vehicles.c:1368-1380`).
        if vehicle.boarding != 0 && self.pilot_invalid(vehicle) {
            self.eject_pilot(vehicle, state, collision);
            return false;
        }
        if vehicle.boarding != 0 {
            if !vehicle.was_boarding {
                vehicle.boarding_velocity = state.velocity;
                vehicle.was_boarding = true;
            }
            if vehicle.boarding > -1 && vehicle.boarding <= time {
                vehicle.was_boarding = false;
                vehicle.boarding = 0;
            } else {
                return self.steer(vehicle, state, command, move_dir, true);
            }
        }
        if self.parent_health <= 0 {
            return false;
        }
        // "See if any of the riders are dead and if so kick em off."
        if vehicle.pilot.is_some() && self.pilot_invalid(vehicle) {
            self.eject_pilot(vehicle, state, collision);
        }
        *self.parent_command = vehicle.ucmd;
        vehicle.ucmd.buttons &= !BUTTON_TALK;
        let mut link_held = false;
        for slot in 0..VEHICLE_WEAPONS {
            if info.weapons[slot].linkable == 2 {
                vehicle.weapon_status[slot].linked = true;
            } else if vehicle.ucmd.buttons & BUTTON_USE_HOLDABLE != 0 {
                if !vehicle.link_weapon_toggle_held && info.weapons[slot].linkable == 1 {
                    vehicle.weapon_status[slot].linked = !vehicle.weapon_status[slot].linked;
                }
                link_held = true;
            }
        }
        vehicle.link_weapon_toggle_held = link_held;
        vehicle.ps_weapons_linked = vehicle.weapon_status.iter().any(|status| status.linked);
        // `VEH_TurretThink` for each turret with ammunition, from the vehicle as it stands
        // now ([`crate::vehicle_turrets`]).
        if info.turrets.iter().any(|turret| turret.ammo_max != 0) {
            self.requests.push(VehicleRequest::Turrets {
                origin: state.origin,
                view_angles: state.view_angles,
                orientation: vehicle.orientation,
            });
        }
        self.steer(vehicle, state, command, move_dir, false)
    }

    /// Whether the pilot is dead or out of the game (`!inuse || !client || health <= 0 ||
    /// connected != CON_CONNECTED`).
    fn pilot_invalid(&self, vehicle: &Vehicle) -> bool {
        let Some(pilot) = vehicle.pilot else {
            return false;
        };
        match self.rider.as_deref() {
            Some(rider) if rider.number == pilot => !rider.connected || rider.health <= 0,
            // A pilot the move was not handed is none the host has any more.
            _ => true,
        }
    }

    /// `Eject(pVeh, pVeh->m_pPilot, qtrue)` from inside the move.
    fn eject_pilot(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        collision: &dyn MovementCollision,
    ) {
        let level_time = self.level_time;
        let mut trace =
            |start, mins, maxs, end, mask| collision.trace(start, mins, maxs, end, mask);
        let mut parent = parent_view(
            vehicle,
            state,
            &mut *self.player,
            &mut *self.entity,
            &mut *self.parent_command,
            self.number,
            self.parent_health,
            self.parent_yaw,
            self.parent_origin,
            self.parent_maxs,
            self.parent_clip_mask,
        );
        match self
            .rider
            .as_deref_mut()
            .filter(|rider| parent.vehicle.pilot == Some(rider.number))
        {
            Some(rider) if rider.connected => {
                crate::vehicle_board::eject(&mut parent, rider, true, level_time, &mut trace);
            }
            _ => {
                let pilot = parent.vehicle.pilot.unwrap_or(ENTITYNUM_NONE);
                crate::vehicle_board::eject_gone(&mut parent, pilot, level_time);
            }
        }
    }

    /// What the steering reads: the pilot's view and weapon, or the vehicle's own view.
    fn steering(
        &self,
        vehicle: &Vehicle,
        state: &MovementState,
        command: &UserCommand,
    ) -> Steering {
        let pilot = self
            .rider
            .as_deref()
            .filter(|rider| vehicle.pilot == Some(rider.number));
        Steering {
            time: self.level_time,
            command_time: command.server_time,
            rider_yaw: pilot.map_or(state.view_angles[1], |rider| rider.movement.view_angles[1]),
            rider_pitch: pilot.map_or(state.view_angles[0], |rider| rider.movement.view_angles[0]),
            rider_player: pilot.is_some(),
            electrify_time: self.player.raw_field(PS_ELECTRIFY_TIME).unwrap_or(0) as i32,
            pilot_weapon: pilot.map(|rider| {
                (
                    rider.movement.weapon,
                    crate::pmove_locomotion::sabers_off_state(rider.movement),
                )
            }),
            fighter: if vehicle.kind() == kind::FIGHTER {
                self.fighter_steering(vehicle, state)
            } else {
                Default::default()
            },
        }
    }

    /// `Update` from `maintainSelfDuringBoarding` on (`g_vehicles.c:1509-1630`): the
    /// orientation and the speed, the parent's and the pilot's view, the shift sounds'
    /// draws and the move's direction.
    fn steer(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        command: &UserCommand,
        move_dir: &mut [f32; 3],
        boarding: bool,
    ) -> bool {
        let time = self.level_time;
        if boarding && vehicle.pilot.is_some() {
            if let Some(rider) = self
                .rider
                .as_deref_mut()
                .filter(|rider| vehicle.pilot == Some(rider.number))
            {
                rider.movement.view_angles = vehicle.orientation;
            }
            vehicle.ucmd.buttons = 0;
            vehicle.ucmd.forward_move = 0;
            vehicle.ucmd.right_move = 0;
            vehicle.ucmd.up_move = 0;
        }
        vehicle.prev_orientation = vehicle.orientation;
        let steering = self.steering(vehicle, state, command);
        let oriented = process_orient_commands(vehicle, state, &steering);
        self.set_view_angle(vehicle.orientation, state);
        if !steering.fighter.unrestrained
            && let Some(rider) = self
                .rider
                .as_deref_mut()
                .filter(|rider| vehicle.pilot == Some(rider.number))
        {
            let view = rider.movement.view_angles;
            rider.set_view_angle([view[0], view[1], vehicle.orientation[ROLL]]);
        }
        let previous_speed = state.speed as i32;
        let moved = process_move_commands(vehicle, state, &steering, move_dir);
        self.cues(vehicle, state, moved, oriented);
        let next_speed = state.speed as i32;
        let half_max_speed = (vehicle.info.speed_max * 0.5) as i32;
        if vehicle.turbo_time < time
            && vehicle.sound_debounce_timer < time
            && ((next_speed > previous_speed
                && next_speed > half_max_speed
                && previous_speed < half_max_speed)
                || (next_speed > half_max_speed && self.rng.irand(0, 1_000) == 0))
        {
            let shift = self.rng.irand(1, 4);
            if vehicle.info.sound_shifts[(shift - 1) as usize] != 0 {
                vehicle.sound_debounce_timer = time + self.rng.irand(1_000, 4_000);
            }
        }
        self.set_move_dir(vehicle, move_dir);
        if vehicle.info.surf_destruction != 0 {
            if vehicle.removed_surfaces != 0 {
                // "damage him constantly if any chunks are currently taken off": three
                // seconds at most to its death.
                self.requests.push(VehicleRequest::BoxSizing);
                let most = self.player.stats[crate::npc_begin::STAT_MAX_HEALTH] as i32 as f32;
                let damage =
                    crate::pmove::vehicle_impact::x86_int(most * vehicle.time_modifier / 180.0);
                let flags = crate::damage::DAMAGE_NO_SELF_PROTECTION
                    | crate::damage::DAMAGE_NO_HIT_LOC
                    | crate::damage::DAMAGE_NO_PROTECTION
                    | crate::damage::DAMAGE_NO_ARMOR;
                self.requests.push(VehicleRequest::Wrecked {
                    at: state.origin,
                    damage,
                    flags,
                    means: crate::means_of_death::MOD_SUICIDE,
                    fallback_self: true,
                });
            }
            vehicle.ps_surfaces = vehicle.removed_surfaces;
        }
        vehicle.ps_boarding = vehicle.boarding != 0;
        true
    }

    /// The move's direction: a fighter's whole orientation, anyone else's yaw.
    fn set_move_dir(&self, vehicle: &Vehicle, move_dir: &mut [f32; 3]) {
        let yaw = if vehicle.kind() == kind::FIGHTER {
            vehicle.orientation
        } else {
            [0.0, vehicle.orientation[1], 0.0]
        };
        *move_dir = crate::pmove::flight::flight_axes(yaw).0.to_array();
    }

    /// `SetClientViewAngle(parent, angles)`: the delta angles against the stored command
    /// (`pers.cmd`), `s.angles` and the view.
    fn set_view_angle(&mut self, angles: [f32; 3], state: &mut MovementState) {
        for axis in 0..3 {
            let short = crate::npc_think::angle_to_short(angles[axis]);
            state.delta_angles[axis] = short.wrapping_sub(self.parent_command.angles[axis]);
        }
        state.view_angles = angles;
        self.entity_angles = Some(angles);
    }

    /// `Animate` (`g_vehicles.c:147-159`): the pilot's animation (`AnimateRiders`), then the
    /// vehicle's own — an animal's (`AnimalNPC.c:353-480`); a speeder has none.
    fn animate(&mut self, vehicle: &mut Vehicle, state: &mut MovementState) {
        let level_time = self.level_time;
        if vehicle.pilot.is_some()
            && let Some(rider) = self
                .rider
                .as_deref_mut()
                .filter(|rider| vehicle.pilot == Some(rider.number))
        {
            match vehicle.kind() {
                kind::SPEEDER => {
                    crate::vehicle_riders::animate_speeder_riders(vehicle, rider, level_time)
                }
                kind::ANIMAL => crate::vehicle_riders::animate_animal_riders(
                    vehicle,
                    state.speed,
                    rider,
                    level_time,
                ),
                _ => {}
            }
        }
        if vehicle.kind() == kind::WALKER {
            if let Some(lengths) = self.lengths.clone() {
                crate::vehicle_walker::animate(
                    vehicle,
                    state,
                    self.parent_health,
                    lengths.as_ref(),
                );
            }
            return;
        }
        if vehicle.kind() == kind::FIGHTER {
            let fighter = self.fighter_steering(vehicle, state);
            if let Some(lengths) = self.lengths.clone()
                && crate::vehicle_fighter_orient::animate(
                    vehicle,
                    state,
                    &fighter,
                    level_time,
                    lengths.as_ref(),
                )
            {
                self.requests.push(VehicleRequest::EntitySound {
                    sound: vehicle.info.sound_land,
                    at: self.parent_origin,
                });
            }
            return;
        }
        if vehicle.kind() != kind::ANIMAL || self.parent_health <= 0 {
            return;
        }
        let Some(lengths) = self.lengths.clone() else {
            return;
        };
        let set = |state: &mut MovementState, animation: u16, flags: u8| {
            crate::pmove_anim::set_animation(
                state,
                SETANIM_LEGS,
                animation,
                flags,
                lengths.as_ref(),
            )
        };
        let time = self.level_time;
        if state.legs_anim == BOTH_VT_BUCK {
            if state.legs_timer <= 0 {
                vehicle.flags &= !flags::BUCKING;
            } else {
                return;
            }
        } else if vehicle.flags & flags::BUCKING != 0 {
            set(
                state,
                BOTH_VT_BUCK,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
            return;
        }
        if vehicle.boarding != 0 {
            if vehicle.boarding < 0 {
                let animation = match vehicle.boarding {
                    -1 => BOTH_VT_MOUNT_L,
                    -2 => BOTH_VT_MOUNT_R,
                    -3 => BOTH_VT_MOUNT_B,
                    _ => 1_081,
                };
                let length = lengths.length_ms(animation).unwrap_or(0);
                vehicle.boarding = time + (length as f32 * 0.7) as i32;
                set(state, animation, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD);
                if let Some(rider) = self
                    .rider
                    .as_deref_mut()
                    .filter(|rider| vehicle.pilot == Some(rider.number))
                {
                    crate::vehicle_riders::animal_mount(rider, animation);
                }
                return;
            } else if vehicle.boarding <= time {
                vehicle.boarding = 0;
            }
        }
        let fraction = state.speed / vehicle.info.speed_max;
        let (animation, flags) = if fraction < -0.01 {
            (BOTH_VT_WALK_REV, 0)
        } else {
            vehicle.flags &= !flags::CRASHING;
            let turbo = fraction > 0.0 && time < vehicle.turbo_time;
            let walking =
                fraction > 0.0 && (vehicle.ucmd.buttons & BUTTON_WALKING != 0 || fraction <= 0.275);
            if turbo {
                (BOTH_VT_TURBO, SETANIM_FLAG_OVERRIDE)
            } else if walking {
                (
                    BOTH_VT_WALK_FWD,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLDLESS,
                )
            } else if fraction > 0.275 {
                (
                    BOTH_VT_RUN_FWD,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLDLESS,
                )
            } else {
                (BOTH_VT_IDLE1, SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLDLESS)
            }
        };
        set(state, animation, flags);
    }
}

/// `G_VehUpdateShields` (`g_vehicles.c:2435-2447`): the shield's tenths on the parent's
/// `activeForcePass`, for a vehicle with shields.
pub fn update_shields(vehicle: &Vehicle, player: &mut PlayerState) {
    if vehicle.info.shields <= 0 {
        return;
    }
    let tenths =
        (f64::from(vehicle.shields as f32 / vehicle.info.shields as f32 * 10.0)).floor() as i32;
    player.set_raw_field(PS_ACTIVE_FORCE_PASS, tenths as u32);
}

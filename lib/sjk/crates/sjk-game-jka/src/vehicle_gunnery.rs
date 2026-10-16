//! A vehicle's volley in the level ([`crate::vehicle_weapons`] on the roster's world): the
//! trigger read off the vehicle NPC as its move left it, the bolts, traces and projectiles
//! answered by the host, and what the volley did to the vehicle's player state and its
//! pilot.

use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove::MovementTrace;
use crate::vehicle_drive::VehicleOutcome;
use crate::vehicle_fields::VEHICLE_TURRETS;
use crate::vehicle_turrets::{Gunner, TurretParent, TurretTarget, TurretWorld};
use crate::vehicle_weapons::{EV_NOAMMO, GunneryWorld, MuzzleBolt, Trigger};

/// `ps.electrifyTime`; `ps.rocketLockIndex`, `rocketLockTime`, `rocketTargetTime`.
const PS_ELECTRIFY_TIME: usize = 73;
const PS_ROCKET_LOCK: [usize; 3] = [24, 79, 71];
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;
/// `FL_NOTARGET`.
const FL_NOTARGET: u32 = 0x20;

/// The host as the guns of vehicle `number` reach it.
struct Gunnery<'w, H: NpcHost> {
    host: &'w mut H,
    number: u16,
    riders: &'w [u16],
    bodies: &'w [crate::entity_clip::BoxObstacle],
    velocities: &'w [(u16, [f32; 3])],
}

impl<H: NpcHost> GunneryWorld for Gunnery<'_, H> {
    fn muzzle(
        &mut self,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
        level_time: i32,
    ) -> MuzzleBolt {
        self.host
            .vehicle_bolt(self.number, tag, angles, origin, level_time)
    }

    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace {
        self.host
            .trace_past_riders(start, mins, maxs, end, pass, self.riders, mask, self.bodies)
    }

    fn vehicle_velocity(&self, number: u16) -> Option<[f32; 3]> {
        self.velocities
            .iter()
            .find(|(known, _)| *known == number)
            .map(|(_, velocity)| *velocity)
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `FireWeapon` for vehicle `me` (`g_weapon.c:4502-4505`): its volley fired from where
    /// its move stood at the trigger, the projectiles launched in order, the flash, the
    /// ammunition shown, the rocket lock cleared, and a dry weapon told to a player pilot.
    pub(crate) fn vehicle_fire(
        &mut self,
        me: usize,
        alternate: bool,
        origin: [f32; 3],
        view_angles: [f32; 3],
    ) {
        let velocities: Vec<(u16, [f32; 3])> = self
            .actors
            .iter()
            .filter(|npc| npc.vehicle.is_some())
            .map(|npc| (npc.number, npc.player.velocity()))
            .collect();
        let Some(trigger) = self.trigger(me, alternate, origin, view_angles) else {
            return;
        };
        let (pilot, pilot_is_player) = (trigger.pilot, trigger.pilot_is_player);
        let riders: Vec<u16> = pilot.into_iter().collect();
        let mut fired = Vec::new();
        let Self {
            actors,
            host,
            bodies,
            vehicle_weapons,
            ..
        } = self;
        let npc = &mut actors[me];
        let Some(vehicle) = npc.vehicle.as_deref_mut() else {
            return;
        };
        let mut world = Gunnery {
            host: &mut **host,
            number: trigger.number,
            riders: &riders,
            bodies,
            velocities: &velocities,
        };
        let volley = crate::vehicle_weapons::fire(
            vehicle,
            vehicle_weapons,
            &trigger,
            &mut world,
            &mut fired,
        );
        if let Some(slot) = volley.spent {
            npc.player.ammo[slot] = vehicle.weapon_status[slot].ammo as u32;
        }
        if volley.clear_lock {
            let [index, time, target] = PS_ROCKET_LOCK;
            npc.player.set_raw_field(index, u32::from(ENTITYNUM_NONE));
            npc.player.set_raw_field(time, 0);
            npc.player.set_raw_field(target, 0);
        }
        for missile in fired {
            let _ = self.host.launch(missile);
        }
        if let Some(flash) = volley.flash {
            self.host.raise(flash);
        }
        if let (Some(pilot), Some(slot), true) = (pilot, volley.no_ammo, pilot_is_player) {
            self.vehicle_outcomes.push(VehicleOutcome::PilotEvent {
                pilot,
                event: EV_NOAMMO,
                parameter: slot,
            });
        }
    }

    /// Vehicle `me` as its guns read it: from `origin` facing `view_angles`, its pilot, where
    /// it was linked.
    fn trigger(
        &mut self,
        me: usize,
        alternate: bool,
        origin: [f32; 3],
        view_angles: [f32; 3],
    ) -> Option<Trigger> {
        let cull_distance = self.host.cull_distance();
        let npc = &self.actors[me];
        let pilot = npc.vehicle.as_deref()?.pilot;
        let pilot_is_player =
            pilot.is_some_and(|pilot| !self.actors.iter().any(|npc| npc.number == pilot));
        Some(Trigger {
            level_time: self.level_time,
            number: npc.number,
            pilot,
            pilot_is_player,
            origin,
            view_angles,
            electrify_time: npc.player.raw_field(PS_ELECTRIFY_TIME).unwrap_or(0) as i32,
            current_origin: npc.current_origin,
            mins: npc.mins,
            maxs: npc.maxs,
            cull_distance,
            alternate,
        })
    }

    /// `VEH_TurretThink` for each of vehicle `me`'s turrets ([`crate::vehicle_turrets`]),
    /// from where it stood (and how it was turned) in its `Update`: what the level offers as targets, the gunners
    /// aboard, the projectiles launched and the flashes raised.
    pub(crate) fn vehicle_turrets(
        &mut self,
        me: usize,
        origin: [f32; 3],
        view_angles: [f32; 3],
        orientation: [f32; 3],
    ) {
        let Some(trigger) = self.trigger(me, false, origin, view_angles) else {
            return;
        };
        let targets = self.turret_targets(me);
        let velocities: Vec<(u16, [f32; 3])> = self
            .actors
            .iter()
            .filter(|npc| npc.vehicle.is_some())
            .map(|npc| (npc.number, npc.player.velocity()))
            .collect();
        let npc = &self.actors[me];
        let parent = TurretParent {
            session_team: npc.session_team,
            enemy: npc.mind.enemy,
            level_time: self.level_time,
            orientation,
            targets: &targets,
        };
        let gunners: [Option<Gunner>; VEHICLE_TURRETS] =
            std::array::from_fn(|turret| self.gunner(me, turret));
        let mut fired = Vec::new();
        let mut flashes = Vec::new();
        let Self {
            actors,
            host,
            bodies,
            vehicle_weapons,
            ..
        } = self;
        let npc = &mut actors[me];
        let Some(vehicle) = npc.vehicle.as_deref_mut() else {
            return;
        };
        let gunnery = Gunnery {
            host: &mut **host,
            number: trigger.number,
            riders: &[],
            bodies,
            velocities: &velocities,
        };
        let mut world = Turretry {
            gunnery,
            state: &mut npc.state,
            level_time: trigger.level_time,
        };
        for (turret, gunner) in gunners.into_iter().enumerate() {
            let volley = crate::vehicle_turrets::think(
                vehicle,
                vehicle_weapons,
                &trigger,
                turret,
                &parent,
                gunner,
                &mut world,
                &mut fired,
            );
            // Each projectile flies from its own turn; the flash goes up with it.
            for missile in fired.drain(..) {
                flashes.push(Err(missile));
            }
            flashes.extend(volley.flash.map(Ok));
        }
        for launched in flashes {
            match launched {
                Err(missile) => drop(self.host.launch(missile)),
                Ok(flash) => self.host.raise(flash),
            }
        }
    }

    /// `Update`'s turret recharge for vehicle `me` at `server_time`: one round a turret
    /// below its ammunition gets once its recharge time has passed, shown in
    /// `ps.ammo[MAX_VEHICLE_WEAPONS + n]`.
    pub(crate) fn recharge_turrets(&mut self, me: usize, server_time: i32) {
        let npc = &mut self.actors[me];
        let Some(vehicle) = npc.vehicle.as_deref_mut() else {
            return;
        };
        for (slot, turret) in vehicle.info.turrets.iter().enumerate() {
            let status = &mut vehicle.turret_status[slot];
            if turret.weapon > 0
                && turret.ammo_recharge_ms != 0
                && status.ammo < turret.ammo_max
                && server_time - status.last_ammo_inc >= turret.ammo_recharge_ms
            {
                status.last_ammo_inc = server_time;
                status.ammo += 1;
                npc.player.ammo[crate::vehicle_fields::VEHICLE_WEAPONS + slot] = status.ammo as u32;
            }
        }
    }

    /// The passenger who works vehicle `me`'s turret `turret`: aboard in its seat
    /// (`turretNPassengerNum`), a living client — an NPC's view and command, a borrowed
    /// passenger's, or a player's as the host has it.
    fn gunner(&mut self, me: usize, turret: usize) -> Option<Gunner> {
        let vehicle = self.actors[me].vehicle.as_deref()?;
        let seat = vehicle.info.turrets[turret].passenger_num;
        if seat == 0 || vehicle.passenger_count < seat {
            return None;
        }
        let number = vehicle
            .passengers
            .get(seat as usize - 1)
            .copied()
            .flatten()?;
        // A player borrowed with the vehicle is its own states, not the host's.
        if let Some(rider) = self.passengers.iter().find(|rider| rider.number == number) {
            return (rider.health > 0).then(|| Gunner {
                view_angles: rider.movement.view_angles,
                buttons: rider.command.buttons,
            });
        }
        match self.actors.iter().find(|npc| npc.number == number) {
            Some(npc) => (npc.health > 0).then(|| Gunner {
                view_angles: npc.player.view_angles(),
                buttons: npc.mind.command.buttons,
            }),
            None => self.host.gunner(number),
        }
    }

    /// Everything vehicle `me`'s turrets may aim at, in entity order: the players, the NPCs
    /// (a rider of this vehicle owned by it), and what else the host lists.
    fn turret_targets(&mut self, me: usize) -> Vec<TurretTarget> {
        let number = self.actors[me].number;
        let riders: Vec<u16> = self.actors[me]
            .vehicle
            .as_deref()
            .map(|vehicle| {
                vehicle
                    .pilot
                    .into_iter()
                    .chain(vehicle.passengers.iter().copied().flatten())
                    .collect()
            })
            .unwrap_or_default();
        let owner_of = |rider: u16, own: u16| if riders.contains(&rider) { number } else { own };
        let mut targets: Vec<TurretTarget> = self
            .host
            .players()
            .iter()
            .map(|body| TurretTarget {
                number: body.number,
                client: true,
                takes_damage: true,
                health: body.health,
                no_target: body.flags & FL_NOTARGET != 0,
                shootable_thing: false,
                session_team: body.session_team,
                temp_spectate_until: if body.spectating { i32::MAX } else { 0 },
                team_no_damage: 0,
                owner: owner_of(body.number, ENTITYNUM_NONE),
                origin: body.origin,
                bounds: (
                    std::array::from_fn(|axis| body.origin[axis] + body.mins[axis] - 1.0),
                    std::array::from_fn(|axis| body.origin[axis] + body.maxs[axis] + 1.0),
                ),
                velocity: body.velocity,
            })
            .collect();
        targets.extend(self.actors.iter().map(|npc| TurretTarget {
            number: npc.number,
            client: true,
            takes_damage: npc.takes_damage,
            health: npc.health,
            no_target: npc.flags & FL_NOTARGET != 0,
            shootable_thing: false,
            session_team: npc.session_team,
            temp_spectate_until: 0,
            team_no_damage: 0,
            owner: owner_of(npc.number, crate::vehicle_board::owner_of(npc)),
            origin: npc.current_origin,
            bounds: npc.link,
            velocity: npc.player.velocity(),
        }));
        self.host.turret_targets(&mut targets);
        targets.sort_by_key(|target| target.number);
        targets
    }
}

/// The world as a vehicle's turrets reach it: its guns', and its entity whose bones they
/// turn.
struct Turretry<'w, H: NpcHost> {
    gunnery: Gunnery<'w, H>,
    state: &'w mut sjk_protocol::EntityState,
    level_time: i32,
}

impl<H: NpcHost> GunneryWorld for Turretry<'_, H> {
    fn muzzle(
        &mut self,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
        level_time: i32,
    ) -> MuzzleBolt {
        self.gunnery.muzzle(tag, angles, origin, level_time)
    }

    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace {
        self.gunnery.trace(start, mins, maxs, end, pass, mask)
    }

    fn vehicle_velocity(&self, number: u16) -> Option<[f32; 3]> {
        self.gunnery.vehicle_velocity(number)
    }
}

impl<H: NpcHost> TurretWorld for Turretry<'_, H> {
    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.gunnery.host.in_pvs(from, to)
    }

    fn set_bone_angles(&mut self, bone: &[u8], angles: [f32; 3]) {
        let number = self.gunnery.number;
        crate::npc_droid::set_bone_angles(
            self.state,
            &mut *self.gunnery.host,
            number,
            bone,
            angles,
            self.level_time,
        );
    }
}

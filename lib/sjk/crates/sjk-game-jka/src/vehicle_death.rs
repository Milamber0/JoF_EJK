//! A vehicle dying inside its move (`codemp/game/g_vehicles.c`): `StartDeathDelay` — the
//! fuse its definition's `explosionDelay` sets — and `DeathUpdate` once the fuse is out: its
//! riders thrown off the top (`EjectAll`) and killed if it kills them, then a speeder's
//! explosion; an animal's (`AnimalNPC.c:42-93`) only throws its riders off.
//!
//! What the explosion does to the world — its effect and scorch mark, `G_RadiusDamage`, the
//! vehicle freed a frame on — is asked of the caller ([`VehicleRequest::Explode`]), as is a
//! rider's death; the traces that place them run here, through the vehicle's move.

use crate::pmove::{MovementCollision, MovementState};
use crate::vehicle::Vehicle;
use crate::vehicle_fields::kind;
use crate::vehicle_update::{VehicleRequest, VehicleThink, parent_view};

/// `CONTENTS_SOLID`.
const CONTENTS_SOLID: u32 = 1;

impl VehicleThink<'_, '_> {
    /// `StartDeathDelay` (`g_vehicles.c:966-983`): the fuse lit (`delay`, or the
    /// definition's), and a flammable vehicle's fire loop.
    pub(crate) fn start_death_delay(&mut self, vehicle: &mut Vehicle, delay: i32) {
        vehicle.die_time = self.level_time
            + if delay != 0 {
                delay
            } else {
                vehicle.info.explosion_delay
            };
        if vehicle.info.flammable {
            self.requests.push(VehicleRequest::FireLoop);
        }
    }

    /// `DeathUpdate` (`g_vehicles.c:986-1063`, `AnimalNPC.c:42-93`) once the fuse is out.
    pub(crate) fn death_update(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        collision: &dyn MovementCollision,
    ) {
        if self.level_time < vehicle.die_time {
            return;
        }
        let animal = vehicle.kind() == kind::ANIMAL;
        if vehicle.inhabited() {
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
            let rider = self.rider.as_deref_mut();
            let mut killed = Vec::new();
            crate::vehicle_board::eject_all(
                &mut parent,
                rider,
                self.passengers,
                level_time,
                &mut trace,
                &mut killed,
            );
            for rider in killed {
                self.requests.push(VehicleRequest::KillRider {
                    rider,
                    damage: 10_000,
                    by_vehicle: false,
                });
            }
            if vehicle.droid_unit.is_some() {
                self.requests.push(VehicleRequest::EjectDroid {
                    kill: vehicle.info.kill_rider_on_death,
                });
            }
            if animal {
                return;
            }
            // "if we've still got people in us, just kill the bastards".
            if vehicle.inhabited() {
                for rider in vehicle.pilot.into_iter().chain(
                    vehicle
                        .passengers
                        .iter()
                        .take(vehicle.info.max_passengers.max(0) as usize)
                        .flatten()
                        .copied(),
                ) {
                    self.requests.push(VehicleRequest::KillRider {
                        rider,
                        damage: 999,
                        by_vehicle: true,
                    });
                }
            }
        }
        if animal || vehicle.inhabited() {
            return;
        }
        let origin = self.parent_origin;
        let mark = (vehicle.info.explode_fx != 0).then(|| {
            let mut bottom = origin;
            bottom[2] -= 80.0;
            let found = collision.trace(origin, [0.0; 3], [0.0; 3], bottom, CONTENTS_SOLID);
            (found.fraction < 1.0).then_some(found.end_position)
        });
        let blast = (vehicle.info.explosion_radius > 0.0 && vehicle.info.explosion_damage > 0)
            .then(|| {
                let mut mins = self.parent_mins;
                // "to keep it off the ground a *little*".
                mins[2] = -4.0;
                let mut bottom = origin;
                bottom[2] += self.parent_mins[2] - 32.0;
                collision
                    .trace(origin, mins, self.parent_maxs, bottom, CONTENTS_SOLID)
                    .end_position
            });
        let effect = vehicle.info.explode_fx != 0;
        self.requests.push(VehicleRequest::Explode {
            at: origin,
            effect,
            mark: mark.flatten(),
            blast,
        });
    }
}

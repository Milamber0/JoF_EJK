//! A vehicle's blast inside its move (`G_RadiusDamage` from `DeathUpdate`'s explosion and
//! from a fighter's surface coming off, `g_vehicles.c:1031-1044`, `2345-2352`), on the
//! roster's NPCs at once — as the reference deals it, in the middle of the vehicle's think:
//! a later slice of the same move finds them hurt, pushed and in pain, and whatever the
//! move then does to them (a droid unit set back on its bolt, standing) comes after.
//!
//! The rest of the blast — the players and the host's own things — is the caller's
//! ([`VehicleOutcome::Blast`]), once the vehicle's think is over.

use crate::damage::{DamageRequest, SplashTarget};
use crate::npc_damage::NpcBlow;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::pmove::{MovementCollision, MovementTrace};
use crate::vehicle_drive::VehicleOutcome;

/// `MOD_SUICIDE`: what a vehicle's blast kills by.
const MOD_SUICIDE: u32 = crate::means_of_death::MOD_SUICIDE;

/// The host's world for `CanDamage`'s lines (`MASK_SOLID`, no bodies).
struct World<'x, H: NpcHost> {
    host: std::cell::RefCell<&'x mut H>,
}

impl<H: NpcHost> MovementCollision for World<'_, H> {
    fn trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        self.host.borrow_mut().trace(
            start,
            mins,
            maxs,
            end,
            crate::npc_spawn::ENTITYNUM_NONE,
            mask,
            &[],
        )
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_RadiusDamage(at, attacker, damage, radius, spared, NULL, MOD_SUICIDE)` of vehicle
    /// `vehicle`'s blast: the NPCs struck now, in entity order — "say my pilot did it" for
    /// a vehicle attacker; `rider_at` one of them where the blast finds it, where the move
    /// has since carried it on — then the players' part asked of the caller.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn vehicle_blast(
        &mut self,
        vehicle: u16,
        at: [f32; 3],
        damage: i32,
        radius: f32,
        attacker: Option<u16>,
        spared: Option<u16>,
        rider_at: Option<(u16, [f32; 3])>,
    ) {
        let level_time = self.level_time;
        let credited = attacker.map(|number| {
            self.actor_at(number)
                .and_then(|at| self.actors[at].vehicle.as_deref())
                .and_then(|vehicle| vehicle.pilot)
                .unwrap_or(number)
        });
        let blamed = credited.map(|number| self.attacker_of(number));
        let mut targets: Vec<SplashTarget> = self
            .order
            .iter()
            .map(|&at| &self.actors[at])
            .filter(|npc| npc.begun())
            .map(|npc| SplashTarget {
                number: npc.number,
                bounds: npc.link,
                origin: npc.current_origin,
                takes_damage: npc.takes_damage,
            })
            .collect();
        if let Some((rider, then)) = rider_at
            && let Some(target) = targets.iter_mut().find(|target| target.number == rider)
        {
            let shift: [f32; 3] = std::array::from_fn(|axis| then[axis] - target.origin[axis]);
            target.origin = then;
            target.bounds = (
                std::array::from_fn(|axis| target.bounds.0[axis] + shift[axis]),
                std::array::from_fn(|axis| target.bounds.1[axis] + shift[axis]),
            );
        }
        let mut blows: Vec<(u16, DamageRequest)> = Vec::new();
        {
            let world = World {
                host: std::cell::RefCell::new(&mut *self.host),
            };
            crate::damage::radius_damage(
                at,
                blamed,
                damage as f32,
                radius,
                spared,
                MOD_SUICIDE,
                level_time,
                &targets,
                &world,
                &mut |number, request| {
                    blows.push((number, request));
                    false
                },
            );
        }
        for (number, request) in blows {
            if let Some(target) = self.actor_at(number) {
                self.damage(
                    target,
                    NpcBlow {
                        request,
                        spared_by_master: false,
                        surface: None,
                    },
                );
            }
        }
        self.vehicle_outcomes.push(VehicleOutcome::Blast {
            vehicle,
            at,
            damage,
            radius,
            attacker,
            spared,
            rider_at,
        });
    }
}

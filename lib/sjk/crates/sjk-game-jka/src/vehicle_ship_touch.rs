//! The roster's side of the space-ship triggers ([`crate::vehicle_triggers`]): they are
//! placed as the map spawns (with the points they name), touched by its vehicles after
//! each move (`G_TouchTriggers`, `g_active.c:3374-3377`), and the boundaries think in their
//! turn of the frame (`shipboundary_think`); a ship's jump puts it out around the far point
//! (`TeleportPlayer`, `g_misc.c:197-256`). The players' side — a player in space, a pilot
//! carried through the jump — is asked of the caller ([`crate::vehicle_drive::VehicleOutcome`],
//! [`crate::npc_roster::NpcRoster::player_space_touch`]).

use crate::damage::{Attacker, DamageRequest};
use crate::event_entity::EventEntity;
use crate::npc_damage::NpcBlow;
use crate::npc_roster::NpcRoster;
use crate::npc_spawn::{NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::vehicle_drive::VehicleOutcome;
use crate::vehicle_triggers::{ShipTouched, ShipTriggerKind, ShipTriggers};
use sjk_entity::Entity;

/// `ps.eFlags2`; `ps.pm_time`; `ps.eFlags`.
const PS_EFLAGS2: usize = 103;
const PS_EFLAGS: usize = 17;
/// `PMF_TIME_KNOCKBACK`; `EF_TELEPORT_BIT`.
const PMF_TIME_KNOCKBACK: u16 = 64;
const EF_TELEPORT_BIT: u32 = 1 << 3;
/// `CHAN_LOCAL`, which the jump's end is heard on.
const CHAN_LOCAL: u32 = 1;
/// `MOD_TELEFRAG`, `MOD_SUICIDE`; `DAMAGE_NO_PROTECTION`.
const MOD_TELEFRAG: u32 = crate::means_of_death::MOD_TELEFRAG;
const MOD_SUICIDE: u32 = crate::means_of_death::MOD_SUICIDE;
const DAMAGE_NO_PROTECTION: u32 = crate::damage::DAMAGE_NO_PROTECTION;

impl NpcRoster {
    /// A map entity that is a space-ship trigger, or a point one names, spawned in its turn
    /// of the map (`G_SpawnEntitiesFromString`): its entity, its brush's box. `names` are the
    /// points the map's triggers name ([`crate::vehicle_triggers::named_points`]). Whether it
    /// was one.
    pub(crate) fn place_ship_entity(
        &mut self,
        entity: &Entity,
        names: &[String],
        level_time: i32,
        host: &mut impl NpcHost,
    ) -> bool {
        let Some(classname) = entity.classname() else {
            return false;
        };
        if crate::vehicle_triggers::is_ship_trigger(classname) {
            let Some(number) = host.spawn_hidden() else {
                return true;
            };
            let model = entity.get("model").unwrap_or_default();
            let Some(bounds) = host.brush_bounds(model) else {
                host.print(&format!("{classname} {number}: no brush model {model}\n"));
                return true;
            };
            if classname.eq_ignore_ascii_case("trigger_hyperspace") {
                host.sound_index(crate::vehicle_triggers::HYPERSPACE_END_SOUND);
            }
            match crate::vehicle_triggers::spawn(entity, number, bounds, level_time) {
                Ok(trigger) => self.ship_triggers.triggers.push(trigger),
                Err(error) => host.print(&format!("^1ERROR: {error}\n")),
            }
            return true;
        }
        let named = entity
            .get("targetname")
            .filter(|name| names.iter().any(|known| known == name));
        if let Some(name) = named
            && crate::spawn_table::support(entity) == Some(crate::spawn_table::Support::Point)
        {
            let Some(number) = host.spawn_hidden() else {
                return true;
            };
            self.ship_triggers
                .points
                .push(crate::vehicle_triggers::point(entity, number, name));
            return true;
        }
        false
    }

    /// `G_TouchTriggers` of the space-ship triggers for player `origin` with its box
    /// (`r.mins`, `r.maxs`): `space_touch` of each it is in (`hidden_in_ship` inside a ship
    /// that hides its riders); `index` and `suffocation` its `inSpaceIndex` and
    /// `inSpaceSuffocation`. Boundaries and lanes take only ships.
    pub fn player_space_touch(
        &self,
        origin: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        hidden_in_ship: bool,
        index: &mut u16,
        suffocation: &mut i32,
        level_time: i32,
    ) {
        let (low, high) = (
            std::array::from_fn(|axis| origin[axis] + mins[axis]),
            std::array::from_fn(|axis| origin[axis] + maxs[axis]),
        );
        for trigger in &self.ship_triggers.triggers {
            if matches!(trigger.kind, ShipTriggerKind::Space)
                && trigger.near(origin)
                && trigger.contacts(low, high)
            {
                crate::vehicle_triggers::space_touch(
                    trigger,
                    origin,
                    hidden_in_ship,
                    index,
                    suffocation,
                    level_time,
                );
            }
        }
    }

    /// `G_RunFrame`'s space for a player ([`crate::vehicle_triggers::space_frame`]).
    pub fn player_space_frame(
        &self,
        origin: [f32; 3],
        index: &mut u16,
        suffocation: i32,
        level_time: i32,
    ) -> bool {
        crate::vehicle_triggers::space_frame(
            &self.ship_triggers,
            origin,
            index,
            suffocation,
            level_time,
        )
    }

    /// What a rider's move reads of vehicle `number` a boundary turns back
    /// (`PM_VehForcedTurning`): `None` where it has no turnaround point, or the point is not
    /// in the level.
    pub fn turnaround(&self, number: u16) -> Option<crate::pmove::riding::Turnaround> {
        let npc = self.actors.iter().find(|npc| npc.number == number)?;
        let vehicle = npc.vehicle.as_deref()?;
        if vehicle.turnaround_index == 0 {
            return None;
        }
        let point = self
            .ship_triggers
            .points
            .iter()
            .find(|point| point.number == vehicle.turnaround_index)?;
        Some(crate::pmove::riding::Turnaround {
            until: vehicle.turnaround_time,
            target: point.origin,
            vehicle_origin: npc.player.origin(),
        })
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_TouchTriggers` for the NPC at `me`, the space-ship triggers: the living (a
    /// vehicle's touches only), where its box meets them.
    pub(crate) fn touch_ship_triggers(&mut self, me: usize) {
        let npc = &self.actors[me];
        if npc.player.health() <= 0 || self.ship_triggers.triggers.is_empty() {
            return;
        }
        let origin = npc.player.origin();
        let (low, high): ([f32; 3], [f32; 3]) = (
            std::array::from_fn(|axis| origin[axis] + npc.mins[axis]),
            std::array::from_fn(|axis| origin[axis] + npc.maxs[axis]),
        );
        let mut order: Vec<u16> = self
            .ship_triggers
            .triggers
            .iter()
            .filter(|trigger| trigger.near(origin) && trigger.contacts(low, high))
            .map(|trigger| trigger.number)
            .collect();
        order.sort_unstable();
        for number in order {
            self.ship_touch(me, number);
        }
    }

    /// One space-ship trigger's touch of the NPC at `me`.
    fn ship_touch(&mut self, me: usize, number: u16) {
        let level_time = self.level_time;
        let Some(at) = self
            .ship_triggers
            .triggers
            .iter()
            .position(|trigger| trigger.number == number)
        else {
            return;
        };
        let npc = &mut self.actors[me];
        let origin = npc.player.origin();
        let (mins, maxs) = (npc.mins, npc.maxs);
        let piloted = npc.player.vehicle_entity_num() != 0;
        let mut flags2 = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0);
        let Some(vehicle) = npc.vehicle.as_deref_mut() else {
            // An NPC in space: its `inSpaceIndex`, which only a vehicle keeps here.
            return;
        };
        let triggers: &mut ShipTriggers = self.ship_triggers;
        let (trigger, points) = (&mut triggers.triggers[at], &triggers.points);
        let touched = match trigger.kind {
            ShipTriggerKind::Space => {
                let mut suffocation = 0;
                crate::vehicle_triggers::space_touch(
                    trigger,
                    origin,
                    false,
                    &mut vehicle.in_space_index,
                    &mut suffocation,
                    level_time,
                );
                ShipTouched::Nothing
            }
            ShipTriggerKind::Boundary { .. } => {
                let mut ship = crate::vehicle_triggers::Ship {
                    number: npc.number,
                    origin,
                    mins,
                    maxs,
                    piloted,
                    flags2: &mut flags2,
                    vehicle,
                };
                crate::vehicle_triggers::boundary_touch(trigger, points, &mut ship, level_time)
            }
            ShipTriggerKind::Hyperspace { .. } => {
                let mut ship = crate::vehicle_triggers::Ship {
                    number: npc.number,
                    origin,
                    mins,
                    maxs,
                    piloted,
                    flags2: &mut flags2,
                    vehicle,
                };
                crate::vehicle_triggers::hyperspace_touch(trigger, points, &mut ship, level_time)
            }
        };
        npc.player.set_raw_field(PS_EFLAGS2, flags2);
        self.ship_touched(me, touched);
    }

    /// What a ship's touch asked for.
    fn ship_touched(&mut self, me: usize, touched: ShipTouched) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let number = npc.number;
        match touched {
            ShipTouched::Nothing => {}
            ShipTouched::Destroyed => {
                let attacker = Attacker {
                    npc: true,
                    client: number,
                    max_health: npc.max_health,
                    team: npc.session_team,
                    saber_knockback: [0.0; 4],
                };
                let blow = DamageRequest {
                    level_time,
                    attacker: Some(attacker),
                    direction: None,
                    point: Some(npc.player.origin()),
                    damage: 99_999,
                    flags: DAMAGE_NO_PROTECTION,
                    means: MOD_SUICIDE,
                };
                self.damage(
                    me,
                    NpcBlow {
                        request: blow,
                        spared_by_master: false,
                        surface: None,
                    },
                );
            }
            ShipTouched::Turned { point } => self.host.show(point),
            ShipTouched::Jumped { origin, angles } => {
                self.teleport_npc(me, origin, angles);
                let pilot = self.actors[me]
                    .vehicle
                    .as_deref()
                    .and_then(|vehicle| vehicle.pilot);
                let sound = self
                    .host
                    .sound_index(crate::vehicle_triggers::HYPERSPACE_END_SOUND);
                self.vehicle_outcomes.push(VehicleOutcome::Jumped {
                    vehicle: number,
                    pilot,
                    origin,
                    angles,
                    sound,
                });
            }
        }
    }

    /// `TeleportPlayer` for an NPC (`g_misc.c:197-256`): its flashes where it was and where
    /// it goes, put a unit above the place facing `angles`, spat out at 400 along them and
    /// held 160 ms, its teleport bit flipped, whatever stands there killed (`G_KillBox`,
    /// sparing its owner), and its entity made from its state at once.
    pub(crate) fn teleport_npc(&mut self, me: usize, origin: [f32; 3], angles: [f32; 3]) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let number = npc.number;
        let owner = crate::vehicle_board::owner_of(npc);
        self.host
            .raise(EventEntity::teleport_out(npc.player.origin(), number));
        self.host.raise(EventEntity::teleport_in(origin, number));
        let npc = &mut self.actors[me];
        npc.player
            .set_origin([origin[0], origin[1], origin[2] + 1.0]);
        let forward = crate::pmove::flight::flight_axes(angles).0.to_array();
        npc.player.set_velocity(forward.map(|axis| axis * 400.0));
        npc.player.set_movement_time(160);
        npc.player
            .set_movement_flags(npc.player.movement_flags() | PMF_TIME_KNOCKBACK);
        crate::triggers::face(&mut npc.player, angles, npc.mind.command.angles);
        for axis in 0..3 {
            npc.state
                .set_raw_field(es::ANGLES[axis], angles[axis].to_bits());
        }
        let flags = npc.player.raw_field(PS_EFLAGS).unwrap_or(0);
        npc.player.set_raw_field(PS_EFLAGS, flags ^ EF_TELEPORT_BIT);
        // `G_KillBox`: every client in its box but itself and its owner.
        let arrived = npc.player.origin();
        let (low, high): ([f32; 3], [f32; 3]) = (
            std::array::from_fn(|axis| arrived[axis] + npc.mins[axis]),
            std::array::from_fn(|axis| arrived[axis] + npc.maxs[axis]),
        );
        self.host.telefrag_players_but(low, high, number, owner);
        let victims: Vec<usize> = (0..self.actors.len())
            .filter(|&at| at != me && self.actors[at].number != owner && self.actors[at].health > 0)
            .filter(|&at| {
                let (bottom, top) = self.actors[at].link;
                (0..3).all(|axis| bottom[axis] <= high[axis] && top[axis] >= low[axis])
            })
            .collect();
        for at in victims {
            let npc = &self.actors[me];
            let attacker = Attacker {
                npc: true,
                client: number,
                max_health: npc.max_health,
                team: npc.session_team,
                saber_knockback: [0.0; 4],
            };
            let blow = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: None,
                point: None,
                damage: 100_000,
                flags: DAMAGE_NO_PROTECTION,
                means: MOD_TELEFRAG,
            };
            self.damage(
                at,
                NpcBlow {
                    request: blow,
                    spared_by_master: false,
                    surface: None,
                },
            );
        }
        // `BG_PlayerStateToEntityState(ps, s, qtrue)`, typed an NPC, linked where it is.
        let npc = &mut self.actors[me];
        crate::player_entity::player_entity_state(
            &npc.player,
            &mut npc.mind.shown_events,
            crate::player_entity::PlayerEntityMotion::Interpolated,
            true,
            &mut npc.state,
        );
        npc.state.set_raw_field(es::TYPE, crate::npc_spawn::ET_NPC);
        npc.current_origin = npc.player.origin();
        npc.movement = npc.movement.reseeded(&npc.player);
        npc.relink();
    }

    /// `shipboundary_think` for boundary `number` (`g_trigger.c:1545-1579`): while it was
    /// touched in the last two seconds, every piloted fighter whose box meets it is touched.
    pub(crate) fn boundary_think(&mut self, number: u16) {
        let level_time = self.level_time;
        let Some(at) = self
            .ship_triggers
            .triggers
            .iter()
            .position(|trigger| trigger.number == number)
        else {
            return;
        };
        if !crate::vehicle_triggers::boundary_think_due(
            &mut self.ship_triggers.triggers[at],
            level_time,
        ) {
            return;
        }
        let (low, high) = self.ship_triggers.triggers[at].absolute();
        let mut fighters: Vec<(u16, usize)> = self
            .actors
            .iter()
            .enumerate()
            .filter(|(_, npc)| {
                npc.player.vehicle_entity_num() != 0
                    && npc.vehicle.as_deref().is_some_and(|vehicle| {
                        vehicle.kind() == crate::vehicle_fields::kind::FIGHTER
                    })
            })
            .filter(|(_, npc)| {
                let (bottom, top) = npc.link;
                (0..3).all(|axis| bottom[axis] <= high[axis] && top[axis] >= low[axis])
            })
            .map(|(me, npc)| (npc.number, me))
            .collect();
        fighters.sort_unstable();
        for (_, me) in fighters {
            self.ship_touch(me, number);
        }
    }
}

/// The hyperspace end's sound on a jumped ship (`G_Sound(other, CHAN_LOCAL, ...)`): where
/// the ship now is.
pub fn jump_sound(at: [f32; 3], sound: u16) -> EventEntity {
    crate::weapon_fire::sound_event(at, CHAN_LOCAL, sound)
}

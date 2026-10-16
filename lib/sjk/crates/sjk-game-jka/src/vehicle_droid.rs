//! A vehicle's droid unit (OpenJK `codemp/game`): the astromech an X-wing carries behind
//! its cockpit. `NPC_Begin` spawns it for a vehicle whose model has a `*droidunit` bolt
//! (`NPC_spawn.c:1201-1266`) — the type its spawner names (`model2`), else its definition's
//! `droidNPC`, `random` and `default` choosing R2 or R5 by `Q_irand` — owned by the vehicle,
//! riding in it and undying. `AttachRiders` keeps it on its bolt after every move of the
//! vehicle (`g_vehicles.c:1876-1905`); while it rides it thinks nothing but its chatter
//! (`G_DroidSounds`, `NPC.c:1725-1754`, `1857-1860`). `G_EjectDroidUnit`
//! (`g_vehicles.c:579-602`) lets it go, killing it where the vehicle kills its riders.
//!
//! The droid's entity is an ordinary NPC of the roster; this module only links it to its
//! vehicle ([`crate::vehicle::Vehicle::droid_unit`]). Its ownership (`r.ownerNum`) is its
//! `s.owner`, which `NPC_Begin` sets with it and which the vehicle's own traces pass.

use crate::npc_roster::{CommandPlace, Fired, NpcFiles, NpcRoster};
use crate::npc_spawn::{ENTITYNUM_NONE, NpcActor, NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::vehicle_drive::VehicleOutcome;

/// `ps.m_iVehicleNum`, `s.m_iVehicleNum`.
const PS_VEHICLE: usize = 84;
const ES_VEHICLE: usize = 93;
/// `ps.torsoTimer`, `ps.legsTimer`; `BOTH_STAND2`.
const PS_TORSO_TIMER: usize = 20;
const PS_LEGS_TIMER: usize = 21;
const BOTH_STAND2: u16 = 917;
/// `FL_UNDYING`.
const FL_UNDYING: u32 = 0x10_0000;
/// `CLASS_R2D2`, `CLASS_R5D2`, `CLASS_PROBE`, `CLASS_MOUSE`, `CLASS_GONK`, and each one's
/// chatter: its sound's name before and after the number, and the numbers drawn from.
const CHATTER: [(i32, &str, &str, i32); 5] = [
    (34, "sound/chars/r2d2/misc/r2d2talk0", ".wav", 3),
    (35, "sound/chars/r5d2/misc/r5talk", ".wav", 4),
    (32, "sound/chars/probe/misc/probetalk", ".wav", 3),
    (29, "sound/chars/mouse/misc/mousego", ".wav", 3),
    (11, "sound/chars/gonk/misc/gonktalk", ".wav", 2),
];
/// `MASK_SOLID`; `EV_MUTE_SOUND`, `EV_PLAY_EFFECT_ID`.
const MASK_SOLID: u32 = 1;
const EV_MUTE_SOUND: u32 = 74;
const EV_PLAY_EFFECT_ID: u32 = 69;
/// `ps.eFlags2`; `EF2_SHIP_DEATH`; the `head` surface's bit in `s.surfacesOff`
/// (`bgToggleableSurfaces`).
const PS_EFLAGS2: usize = 103;
const EF2_SHIP_DEATH: u32 = 1 << 7;
const HEAD_SURFACE: u32 = 1 << 27;
/// `CHAN_AUTO`, `CHAN_VOICE`.
const CHAN_AUTO: u32 = 0;
const CHAN_VOICE: u32 = 3;

impl NpcRoster {
    /// The end of `NPC_Begin` for the vehicle at `at` (`NPC_spawn.c:1201-1266`): its droid
    /// unit spawned where `npc spawn` would put one for it (`NPC_SpawnType`), then set in
    /// the vehicle — its `m_iVehicleNum`, `s.owner` and `r.ownerNum` the vehicle's, at the
    /// vehicle's origin and angles, undying.
    pub(crate) fn spawn_droid_unit(
        &mut self,
        at: usize,
        level_time: i32,
        files: NpcFiles<'_>,
        host: &mut impl NpcHost,
        fired: &mut Fired,
    ) {
        let npc = &self.actors[at];
        let Some(vehicle) = npc
            .vehicle
            .as_deref()
            .filter(|vehicle| vehicle.droid_unit_tag != -1)
        else {
            return;
        };
        let named = vehicle.droid_npc.clone().filter(|name| !name.is_empty());
        let Some(mut npc_type) = named.or_else(|| {
            vehicle
                .info
                .droid_npc
                .clone()
                .filter(|name| !name.is_empty())
        }) else {
            return;
        };
        if npc_type.eq_ignore_ascii_case(b"random") || npc_type.eq_ignore_ascii_case(b"default") {
            npc_type = if host.irand(0, 1) != 0 {
                b"r2d2".to_vec()
            } else {
                b"r5d2".to_vec()
            };
        }
        let (number, origin, view) = (npc.number, npc.current_origin, npc.player.view_angles());
        let angles =
            es::APOS_BASE.map(|index| f32::from_bits(npc.state.raw_field(index).unwrap_or(0)));
        // `NPC_SpawnType`'s place: 64 ahead along the vehicle's view, through the world
        // (`MASK_SOLID`, passing entity 0).
        let (place, yaw) = crate::npc_spawn::command_place(origin, view, &mut |start, end| {
            host.trace(start, [0.0; 3], [0.0; 3], end, 0, MASK_SOLID, &[])
        });
        let before = self.actors.len();
        fired.append(&mut self.spawn_command(
            &npc_type,
            b"",
            false,
            CommandPlace { origin: place, yaw },
            level_time,
            files,
            host,
        ));
        let Some(droid) = self.actors.get_mut(before) else {
            return;
        };
        // `NPC_SpawnType(ent, type, NULL, qfalse)`: no name.
        droid.targetname = None;
        droid.player.set_raw_field(PS_VEHICLE, u32::from(number));
        droid.state.set_raw_field(ES_VEHICLE, u32::from(number));
        droid.state.set_raw_field(es::OWNER, u32::from(number));
        droid.player.set_origin(origin);
        for axis in 0..3 {
            droid
                .state
                .set_raw_field(es::ORIGIN[axis], origin[axis].to_bits());
        }
        crate::npc_spawn::set_origin(droid, origin);
        set_angles(droid, angles);
        droid.desired_yaw = angles[1];
        droid.mind.desired_pitch = angles[0];
        droid.flags |= FL_UNDYING;
        let droid_number = droid.number;
        host.publish(droid_number, &droid.state, droid.bounds(), droid.contents);
        if let Some(vehicle) = self.actors[at].vehicle.as_deref_mut() {
            vehicle.droid_unit = Some(droid_number);
        }
    }
}

/// `G_SetAngles` (`g_utils.c:1903-1908`): `r.currentAngles`, `s.angles` and
/// `s.apos.trBase`.
fn set_angles(npc: &mut NpcActor, angles: [f32; 3]) {
    for axis in 0..3 {
        npc.state
            .set_raw_field(es::ANGLES[axis], angles[axis].to_bits());
        npc.state
            .set_raw_field(es::APOS_BASE[axis], angles[axis].to_bits());
    }
    npc.mind.current_angles = angles;
}

/// The vehicle an NPC rides in (`s.m_iVehicleNum` set, not a vehicle itself), which is its
/// `r.ownerNum` (`s.owner`): a droid unit's.
pub(crate) fn riding_owner(npc: &NpcActor) -> Option<u16> {
    let vehicle = npc.state.raw_field(ES_VEHICLE).unwrap_or(0);
    if npc.vehicle.is_some() || vehicle == 0 || vehicle == u32::from(ENTITYNUM_NONE) {
        return None;
    }
    npc.state
        .raw_field(es::OWNER)
        .map(|owner| owner as u16)
        .filter(|owner| *owner != ENTITYNUM_NONE)
}

/// Whether the NPC's traces pass `other` because `other` is its own (`r.ownerNum`, which
/// for an NPC in a vehicle is its `s.owner`): a vehicle's droid unit, to the vehicle.
pub(crate) fn owned_by(other: &NpcActor, number: u16) -> bool {
    riding_owner(other) == Some(number)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `AttachRiders`' droid (`g_vehicles.c:1876-1905`), at the end of a slice of the move
    /// of the vehicle at `me`: the droid set on the `*droidunit` bolt of the vehicle's
    /// model posed at `origin` (its `r.currentOrigin`, which the move does not change)
    /// facing `yaw` (its view's), looking down the bolt's negative y, linked there.
    pub(crate) fn attach_droid(&mut self, me: usize, origin: [f32; 3], yaw: f32) {
        let npc = &self.actors[me];
        let Some((droid, tag)) = npc
            .vehicle
            .as_deref()
            .and_then(|vehicle| Some((vehicle.droid_unit?, vehicle.droid_unit_tag)))
            .filter(|(_, tag)| *tag != -1)
        else {
            return;
        };
        let number = npc.number;
        let Some(at) = self.actor_at(droid) else {
            return;
        };
        let bolt = self
            .host
            .vehicle_tag(number, tag, [0.0, yaw, 0.0], origin, self.level_time);
        let (pitch, yaw) = crate::damage::vector_to_angles(bolt.forward);
        let view = [pitch, yaw, 0.0];
        let droid = &mut self.actors[at];
        droid.player.set_origin(bolt.origin);
        crate::npc_spawn::set_origin(droid, bolt.origin);
        for axis in 0..3 {
            droid
                .state
                .set_raw_field(es::APOS_BASE[axis], view[axis].to_bits());
        }
        droid.mind.current_angles = view;
        // `SetClientViewAngle`: the delta angles against its last command.
        let command = droid.mind.command.angles;
        let delta: [i32; 3] = std::array::from_fn(|axis| {
            crate::npc_think::angle_to_short(view[axis]).wrapping_sub(command[axis])
        });
        droid.player.set_delta_angles(delta);
        for axis in 0..3 {
            droid
                .state
                .set_raw_field(es::ANGLES[axis], view[axis].to_bits());
        }
        droid.player.set_view_angles(view);
        let (number, state, bounds, contents) = (
            droid.number,
            droid.state.clone(),
            droid.bounds(),
            droid.contents,
        );
        self.host.publish(number, &state, bounds, contents);
        // Sat still: `BOTH_STAND2` held, both timers at half a second.
        use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
        self.set_animation(
            at,
            SETANIM_BOTH,
            BOTH_STAND2,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        let droid = &mut self.actors[at];
        droid.player.set_raw_field(PS_TORSO_TIMER, 500);
        droid.player.set_raw_field(PS_LEGS_TIMER, 500);
    }

    /// `G_DroidSounds` (`NPC.c:1725-1754`) for the NPC at `me`, a droid riding a vehicle:
    /// now and then (one think in 21, its `patrolNoise` timer out) a line of its chatter,
    /// and two to four seconds before the next.
    pub(crate) fn droid_sounds(&mut self, me: usize) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.done("patrolNoise", level_time)
            || self.host.irand(0, 20) != 0
        {
            return;
        }
        let class = self.actors[me].definition.client_class;
        if let Some((_, before, after, count)) = CHATTER.iter().find(|(known, ..)| *known == class)
        {
            let line = self.host.irand(1, *count);
            let name = format!("{before}{line}{after}");
            let sound = self.host.sound_index(name.as_bytes());
            let npc = &self.actors[me];
            let mut event =
                crate::knockdown::entity_sound(npc.current_origin, npc.number, CHAN_AUTO);
            event.parameter = u32::from(sound);
            self.host.raise(event);
        }
        let wait = self.host.irand(2_000, 4_000);
        self.actors[me]
            .mind
            .timers
            .set("patrolNoise", level_time, wait);
    }

    /// `G_EjectDroidUnit` (`g_vehicles.c:579-602`) for the vehicle at `me`: its droid let
    /// go — nobody's, riding nothing (`ENTITYNUM_NONE`), mortal — and, where the vehicle
    /// kills its riders, silenced and killed (10000, `MOD_SUICIDE`, by nobody).
    pub(crate) fn eject_droid(&mut self, me: usize, kill: bool) {
        let Some(droid) = self.actors[me]
            .vehicle
            .as_deref_mut()
            .and_then(|vehicle| vehicle.droid_unit.take())
        else {
            return;
        };
        let Some(at) = self.actor_at(droid) else {
            return;
        };
        let npc = &mut self.actors[at];
        let none = u32::from(ENTITYNUM_NONE);
        npc.state.set_raw_field(ES_VEHICLE, none);
        npc.state.set_raw_field(es::OWNER, none);
        npc.flags &= !FL_UNDYING;
        npc.player.set_raw_field(PS_VEHICLE, none);
        if kill {
            let origin =
                es::ORIGIN.map(|index| f32::from_bits(npc.state.raw_field(index).unwrap_or(0)));
            let mute = crate::event_entity::EventEntity {
                event: EV_MUTE_SOUND,
                parameter: 0,
                origin: [0.0; 3],
                client: None,
                broadcast: true,
                extra: [(0, 0); 12],
            }
            .muting(droid, CHAN_VOICE);
            self.host.raise(mute);
            let request = crate::damage::DamageRequest {
                level_time: self.level_time,
                attacker: None,
                direction: None,
                point: Some(origin),
                damage: 10_000,
                flags: 0,
                means: crate::means_of_death::MOD_SUICIDE,
            };
            self.damage(
                at,
                crate::npc_damage::NpcBlow {
                    request,
                    spared_by_master: false,
                    surface: None,
                },
            );
        }
    }

    /// `player_die`'s first rule for a vehicle with no death delay and someone aboard
    /// (`g_combat.c:2127-2233`): everyone aboard killed "in the name of the attacker" — the
    /// murderer the one it last died of while that counts (a vehicle's pilot for a vehicle),
    /// else its attacker (a piloted vehicle's pilot's last attacker), else its own pilot,
    /// else itself. A pilot inside it (`hideRider`) is killed by the caller
    /// ([`VehicleOutcome::KillRider`], 99999, `DAMAGE_NO_PROTECTION`, `MOD_BLASTER`), and its
    /// droid unit here, made mortal first.
    ///
    /// A player pilot's own last attacker is the caller's, not the roster's: a piloted
    /// vehicle that kills one is taken for no murderer (the vehicle itself). Passengers
    /// are not aboard this server's vehicles.
    pub(crate) fn kill_everyone_aboard(&mut self, me: usize, attacker: u16) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let Some(vehicle) = npc.vehicle.as_deref() else {
            return;
        };
        if vehicle.info.explosion_delay != 0
            || (vehicle.pilot.is_none()
                && vehicle.passenger_count <= 0
                && vehicle.droid_unit.is_none())
        {
            return;
        }
        let (number, pilot, droid, hidden) = (
            npc.number,
            vehicle.pilot,
            vehicle.droid_unit,
            vehicle.info.hide_rider,
        );
        let other = npc.mind.fight.other_killer;
        let piloted = |world: &Self, number: u16| {
            world
                .actor_at(number)
                .and_then(|at| world.actors[at].vehicle.as_deref())
                .and_then(|vehicle| vehicle.pilot)
        };
        let murderer = if other.time >= level_time {
            self.body(other.number)
                .map(|killer| piloted(self, killer.number).unwrap_or(killer.number))
        } else if attacker != number && self.body(attacker).is_some() {
            if piloted(self, attacker).is_some() {
                None
            } else {
                Some(attacker)
            }
        } else {
            pilot.filter(|pilot| self.body(*pilot).is_some())
        };
        let murderer = murderer.unwrap_or(number);
        if hidden && let Some(pilot) = pilot {
            self.vehicle_outcomes.push(VehicleOutcome::KillRider {
                rider: pilot,
                damage: 99_999,
                flags: crate::damage::DAMAGE_NO_PROTECTION,
                by: Some(murderer),
                means: crate::means_of_death::MOD_BLASTER,
            });
        }
        let Some(at) = droid.and_then(|droid| self.actor_at(droid)) else {
            return;
        };
        self.actors[at].flags &= !FL_UNDYING;
        let attacker = self.body(murderer).map(|_| self.attacker_of(murderer));
        let point = self.actors[at].player.origin();
        let request = crate::damage::DamageRequest {
            level_time,
            attacker,
            direction: None,
            point: Some(point),
            damage: 99_999,
            flags: crate::damage::DAMAGE_NO_PROTECTION,
            means: crate::means_of_death::MOD_BLASTER,
        };
        self.damage(
            at,
            crate::npc_damage::NpcBlow {
                request,
                spared_by_master: false,
                surface: None,
            },
        );
    }

    /// `player_die` for an NPC riding a vehicle (`g_combat.c:2256-2298`): it gets off
    /// (`Eject`: a droid unit let go); dying in a fighter it stays where its ship is, in
    /// "die in ship" mode (`EF2_SHIP_DEATH`); a droid that still has its head throws it.
    pub(crate) fn leave_ridden_vehicle(&mut self, me: usize) {
        let npc = &self.actors[me];
        let riding = npc.player.raw_field(PS_VEHICLE).unwrap_or(0) as u16;
        if npc.vehicle.is_some() || riding == 0 {
            return;
        }
        let number = npc.number;
        if let Some(vehicle_at) = self
            .actor_at(riding)
            .filter(|at| self.actors[*at].vehicle.is_some())
        {
            if self.actors[vehicle_at]
                .vehicle
                .as_deref()
                .is_some_and(|vehicle| vehicle.droid_unit == Some(number))
            {
                self.eject_droid(vehicle_at, false);
            }
            let vehicle = &self.actors[vehicle_at];
            if vehicle
                .vehicle
                .as_deref()
                .is_some_and(|vehicle| vehicle.kind() == crate::vehicle_fields::kind::FIGHTER)
            {
                let origin = vehicle.player.origin();
                let npc = &mut self.actors[me];
                let flags2 = npc.player.raw_field(PS_EFLAGS2).unwrap_or(0);
                npc.player
                    .set_raw_field(PS_EFLAGS2, flags2 | EF2_SHIP_DEATH);
                crate::npc_spawn::set_origin(npc, origin);
                npc.player.set_origin(origin);
            }
        }
        let npc = &self.actors[me];
        let head = match npc.definition.client_class {
            34 => b"chunks/r2d2head_veh".as_slice(),
            35 => b"chunks/r5d2head_veh".as_slice(),
            _ => return,
        };
        if npc.state.raw_field(es::SURFACES_OFF).unwrap_or(0) & HEAD_SURFACE != 0 {
            return;
        }
        let (_, _, up) = crate::npc_sniper::angle_vectors(npc.mind.current_angles);
        let at = npc.current_origin;
        let effect = self.host.effect_index(head);
        let mut event = crate::event_entity::EventEntity {
            event: EV_PLAY_EFFECT_ID,
            parameter: u32::from(effect),
            origin: at,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for axis in 0..3 {
            event.extra[axis] = (es::ORIGIN[axis], at[axis].to_bits());
        }
        for axis in 0..3 {
            event.extra[3 + axis] = (es::ANGLES[axis], up[axis].to_bits());
        }
        self.host.raise(event);
    }
}

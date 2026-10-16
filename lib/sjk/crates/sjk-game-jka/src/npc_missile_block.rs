//! What comes at an NPC, and its saber meeting it: `WP_SaberStartMissileBlockCheck` for an
//! NPC (`codemp/game/w_saber.c:5456-5846`) — its look target, and the missile half: every
//! missile or thrown saber within 256 units heading its way, the nearest with a clear path
//! taken for `Jedi_SaberBlockGo`, a thermal detonator or an exploding missile pushed away or
//! jumped from — and `G_MissileImpact`'s saber block by an NPC (`g_missile.c:498-640`),
//! through the players' own [`crate::saber_block`].
//!
//! The missiles and the players' thrown sabers are the host's ([`NpcHost::incoming`]);
//! an NPC's thrown saber is its own record.

use crate::crt_rand::CrtRand;
use crate::npc_jedi_evasion::EvasionType;
use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::saber_block::{Defender, SaberBlock, block_missile, block_on_saber};
use crate::weapon_fire::Missile;

/// `SABER_REFLECT_MISSILE_CONE`, and the check's reach.
const REFLECT_CONE: f32 = 0.2;
const RADIUS: f32 = 256.0;
/// `WP_SABER`, `WP_THERMAL`; `TR_STATIONARY`, `TR_INTERPOLATE`.
const WP_SABER: i32 = 3;
const WP_THERMAL: i32 = 12;
const TR_STATIONARY: u8 = 0;
const TR_INTERPOLATE: u8 = 1;
/// `FP_PUSH`, `FP_GRIP`, `FP_LIGHTNING`, `FP_DRAIN`, `FP_SABER_DEFENSE`.
const FP_PUSH: usize = 3;
const FP_GRIP: usize = 6;
const FP_LIGHTNING: usize = 7;
const FP_DRAIN: usize = 13;
const FP_SABER_DEFENSE: usize = 16;
/// `CLASS_BOBAFETT`; `EF2_FLYING`; `MOD_ROCKET_HOMING`.
const CLASS_BOBAFETT: i32 = 5;
const EF2_FLYING: u32 = 1 << 4;
const MOD_ROCKET_HOMING: u32 = 21;
/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;
/// Wire fields: `s.pos.trDelta`, `s.pos.trType`, `s.weapon`.
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_TYPE: usize = 23;
const ES_WEAPON: usize = 14;

/// An entity in an NPC's box as `WP_SaberStartMissileBlockCheck` reads it: a missile
/// (`ET_MISSILE` or stuck to a wall, `EF_MISSILE_STICK`) or a client's saber entity while
/// it flies (`ps.saberInFlight`, `ps.saberEntityNum`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IncomingEntity {
    /// Its entity number, and `r.ownerNum`.
    pub number: u16,
    pub owner: u16,
    /// `r.currentOrigin`, `r.mins`, `r.maxs`: its linked box is these grown by a unit.
    pub origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `s.pos.trDelta`, `s.pos.trType`, `s.weapon`.
    pub delta: [f32; 3],
    pub trajectory: u8,
    pub weapon: i32,
    /// `splashDamage`, `splashRadius`: an exploding missile.
    pub splash_damage: i32,
    pub splash_radius: f32,
    /// `nextthink`, `count`: a thermal detonator's fuse and its arming.
    pub next_think: i32,
    pub count: i32,
    /// `clipmask`, `methodOfDeath`.
    pub clip_mask: u32,
    pub method_of_death: u32,
}

impl IncomingEntity {
    /// A missile in flight as the check reads it (`r.currentOrigin`, its box, its
    /// trajectory and weapon from its wire state, its splash and fuse): its linked box is
    /// what [`Self::meets`] tests.
    pub fn of_missile(number: u16, missile: &Missile) -> Self {
        let field = |index: usize| missile.state.raw_field(index).unwrap_or(0);
        Self {
            number,
            owner: missile.owner,
            origin: missile.current,
            mins: missile.bounds.0,
            maxs: missile.bounds.1,
            delta: ES_POS_DELTA.map(|index| f32::from_bits(field(index))),
            trajectory: field(ES_POS_TYPE) as u8,
            weapon: field(ES_WEAPON) as i32,
            splash_damage: missile.splash_damage,
            splash_radius: missile.splash_radius,
            next_think: missile
                .thermal
                .map_or(missile.free_at, |thermal| thermal.fuse),
            count: i32::from(missile.thermal.is_some()),
            clip_mask: missile.clip_mask,
            method_of_death: missile.method_of_death,
        }
    }

    /// Whether its linked box (its box at its place, grown by a unit as `SV_LinkEntity`
    /// grows every one) meets the box from `mins` to `maxs` (`trap->EntitiesInBox`).
    pub fn meets(&self, mins: [f32; 3], maxs: [f32; 3]) -> bool {
        (0..3).all(|axis| {
            self.origin[axis] + self.mins[axis] - 1.0 <= maxs[axis]
                && self.origin[axis] + self.maxs[axis] + 1.0 >= mins[axis]
        })
    }
}

/// `VectorNormalize`: the vector made a unit one, its length returned.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        *vector = vector.map(|axis| axis * inverse);
    }
    length
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `WP_SaberStartMissileBlockCheck` for the NPC at `me` (`w_saber.c:5472-5846`): the look
    /// target cleared (unless a monster holds it); then — its weapon at rest, its saber one
    /// that blocks, alive and standing — the nearest missile or thrown saber coming at it
    /// found (while its hands are free for the saber), the look target set from where it
    /// looks, and the missile met: `Jedi_SaberBlockGo` lighting the saber, or a hovering
    /// Boba Fett's dodge.
    pub(crate) fn missile_block_check(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let held = npc
            .player
            .raw_field(crate::npc_saber::PS_EFLAGS2)
            .unwrap_or(0)
            & crate::npc_saber::EF2_HELD_BY_MONSTER
            != 0;
        if !held {
            npc.player
                .set_raw_field(crate::npc_saber::PS_HAS_LOOK_TARGET, 0);
        }
        let full = self.full_routine(me);
        let npc = &self.actors[me];
        let state = &npc.player;
        let blocks =
            npc.definition.sabers[0].flags & crate::npc_saber::SFL_NOT_ACTIVE_BLOCKING == 0;
        if state.weapon_time() > 0
            || !blocks
            || npc.health <= 0
            || crate::pmove_hand_extend::in_knockdown(state.leg_animation(), state.legs_timer())
        {
            return;
        }
        let full = full && npc.force.debounce[FP_SABER_DEFENSE] <= level_time;
        let incoming = if full {
            self.nearest_incoming(me)
        } else {
            None
        };
        let npc = &mut self.actors[me];
        let target = npc.mind.look_target;
        if npc.humanoid && target < crate::npc_saber::ENTITYNUM_WORLD && !held {
            npc.player
                .set_raw_field(crate::npc_saber::PS_HAS_LOOK_TARGET, 1);
            npc.player
                .set_raw_field(crate::npc_saber::PS_LOOK_TARGET, u32::from(target));
        }
        if let Some(incoming) = incoming {
            self.meet_incoming(me, &incoming);
        }
    }

    /// `doFullRoutine` (`w_saber.c:5484-5507`): a saber carrier (or Boba Fett) with its saber in
    /// hand, not zapping, draining, shoving or gripping.
    fn full_routine(&self, me: usize) -> bool {
        let npc = &self.actors[me];
        let state = &npc.player;
        let busy = (1 << FP_LIGHTNING) | (1 << FP_DRAIN) | (1 << FP_PUSH) | (1 << FP_GRIP);
        (i32::from(state.weapon()) == WP_SABER || npc.definition.client_class == CLASS_BOBAFETT)
            && !state.saber_in_flight()
            && state.force_powers_active() & busy == 0
    }

    /// The entity loop (`w_saber.c:5545-5775`) for an NPC: what it can do nothing about is
    /// passed over; a thermal detonator or an exploding missile is jumped from or pushed
    /// away; of the rest coming at it — a missile from in front, a thrown saber from
    /// anywhere — the nearest with a clear path is the one met, and its shooter taken for an
    /// enemy by an NPC without one.
    fn nearest_incoming(&mut self, me: usize) -> Option<IncomingEntity> {
        let npc = &self.actors[me];
        let (number, origin) = (npc.number, npc.current_origin);
        let mins = origin.map(|axis| axis - RADIUS);
        let maxs = origin.map(|axis| axis + RADIUS);
        let mut listed = std::mem::take(&mut self.level.incoming);
        listed.clear();
        self.host.incoming(mins, maxs, &mut listed);
        self.npc_sabers_out(mins, maxs, &mut listed);
        listed.sort_by_key(|entity| entity.number);
        let (forward, _) =
            crate::pmove::flight::flight_axes([0.0, npc.player.view_angles()[1], 0.0]);
        let forward = forward.to_array();
        let mut closest = RADIUS;
        let mut incoming = None;
        for ent in listed
            .iter()
            .filter(|ent| ent.number != number && ent.owner != number)
        {
            let mut dir: [f32; 3] = std::array::from_fn(|axis| ent.origin[axis] - origin[axis]);
            let dist = normalize(&mut dir);
            if ent.weapon == WP_THERMAL {
                self.thermal_near(me, ent, dir, forward, dist);
                continue;
            }
            if ent.splash_damage != 0 && ent.splash_radius != 0.0 {
                self.explosive_near(me, ent, dir, forward, dist);
                continue;
            }
            if ent.weapon != WP_SABER && dot(dir, forward) < REFLECT_CONE {
                continue;
            }
            let mut heading = ent.delta;
            normalize(&mut heading);
            if dot(dir, heading) > 0.0 || dist >= closest {
                continue;
            }
            if self.path_blocked(me, ent) {
                continue;
            }
            self.take_shooter_for_enemy(me, ent.owner);
            closest = dist;
            incoming = Some(*ent);
        }
        self.level.incoming = listed;
        incoming
    }

    /// The NPCs' sabers out of their hands — thrown or knocked down — whose linked boxes
    /// meet `mins`..`maxs`, as `EntitiesInBox` meets them beside the missiles and the
    /// players' sabers (the host's).
    fn npc_sabers_out(&self, mins: [f32; 3], maxs: [f32; 3], out: &mut Vec<IncomingEntity>) {
        const ES_TYPE: usize = 8;
        const ET_MISSILE: u32 = 3;
        for npc in self.actors.iter() {
            let Some(number) = npc.saber_entity else {
                continue;
            };
            let Some(state) = self.host.entity_state(number) else {
                continue;
            };
            let knocked = state.raw_field(ES_TYPE).unwrap_or(0) == ET_MISSILE;
            let flying = npc.player.saber_in_flight() && npc.player.saber_entity_num() == number;
            if !(knocked || flying) {
                continue;
            }
            let read = |index: usize| state.raw_field(index).unwrap_or(0);
            let flight = &npc.saber.flight;
            let entity = IncomingEntity {
                number,
                owner: npc.number,
                origin: flight.current,
                mins: flight.mins,
                maxs: flight.maxs,
                delta: ES_POS_DELTA.map(|index| f32::from_bits(read(index))),
                trajectory: read(ES_POS_TYPE) as u8,
                weapon: read(ES_WEAPON) as i32,
                next_think: flight.next_think,
                clip_mask: flight.clip_mask,
                method_of_death: crate::means_of_death::MOD_SABER,
                ..IncomingEntity::default()
            };
            if entity.meets(mins, maxs) {
                out.push(entity);
            }
        }
    }

    /// A thermal detonator within its blast (`w_saber.c:5603-5623`): Force-jumped from when it
    /// is about to go off near an NPC on the ground and cannot be pushed; else pushed away
    /// (not by Boba Fett).
    fn thermal_near(
        &mut self,
        me: usize,
        ent: &IncomingEntity,
        dir: [f32; 3],
        forward: [f32; 3],
        dist: f32,
    ) {
        if dist >= ent.splash_radius {
            return;
        }
        let on_ground = self.actors[me].player.ground_entity_num() != ENTITYNUM_NONE;
        let jump = ent.next_think < self.level_time + 600
            && ent.count != 0
            && on_ground
            && (ent.trajectory == TR_STATIONARY
                || ent.trajectory == TR_INTERPOLATE
                || dot(dir, forward) < REFLECT_CONE
                || !self.force_power_usable(me, FP_PUSH));
        if jump {
            self.actors[me].force.jump_charge = 480.0;
        } else if self.actors[me].definition.client_class != CLASS_BOBAFETT {
            self.force_throw(me, false);
        }
    }

    /// An exploding missile (`w_saber.c:5625-5686`): Force-jumped from within its blast by an
    /// NPC on the ground that it is behind or that cannot push; else pushed away (not by
    /// Boba Fett).
    fn explosive_near(
        &mut self,
        me: usize,
        ent: &IncomingEntity,
        dir: [f32; 3],
        forward: [f32; 3],
        dist: f32,
    ) {
        let on_ground = self.actors[me].player.ground_entity_num() != ENTITYNUM_NONE;
        if dist < ent.splash_radius
            && on_ground
            && (dot(dir, forward) < REFLECT_CONE || !self.force_power_usable(me, FP_PUSH))
        {
            self.actors[me].force.jump_charge = 480.0;
        } else if self.actors[me].definition.client_class != CLASS_BOBAFETT {
            self.force_throw(me, false);
        }
    }

    /// Whether `ent` cannot reach the NPC (`w_saber.c:5737-5751`): its box traced to the top of
    /// the NPC's, and then — failing that — 256 units along its own way, each stopped by
    /// anything but the NPC and its saber. The trace passes the missile and whatever shares
    /// its owner (`SV_ClipMoveToEntities`: the shooter's saber).
    fn path_blocked(&mut self, me: usize, ent: &IncomingEntity) -> bool {
        let npc = &self.actors[me];
        let (number, saber) = (npc.number, npc.player.saber_entity_num());
        let mut to_top = npc.current_origin;
        to_top[2] = npc.current_origin[2] + npc.maxs[2] + 1.0 - 4.0;
        let mut heading = ent.delta;
        normalize(&mut heading);
        let along: [f32; 3] = std::array::from_fn(|axis| ent.origin[axis] + RADIUS * heading[axis]);
        let blocked = |trace: &crate::pmove::MovementTrace| {
            trace.all_solid
                || trace.start_solid
                || (trace.fraction < 1.0
                    && trace.entity_number != number
                    && trace.entity_number != saber)
        };
        let first = self.missile_trace(ent, to_top);
        blocked(&first) && blocked(&self.missile_trace(ent, along))
    }

    /// `trap->Trace` of `ent`'s box from where it is to `end` with its clip mask: through the
    /// map, the players, every NPC and every lit saber entity but those of `ent`'s owner.
    fn missile_trace(
        &mut self,
        ent: &IncomingEntity,
        end: [f32; 3],
    ) -> crate::pmove::MovementTrace {
        let Self {
            actors,
            bodies,
            host,
            ..
        } = self;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|npc| npc.contents != 0)
                .map(NpcActor::body),
        );
        for npc in actors
            .iter()
            .filter(|npc| npc.number != ent.owner && npc.saber.entity_solid())
        {
            let Some(saber) = npc.saber_entity else {
                continue;
            };
            let entity = &npc.saber.entity;
            bodies.push(crate::entity_clip::BoxObstacle {
                entity: saber,
                origin: entity.origin,
                bounds: (entity.mins, entity.maxs),
                contents: crate::npc_saber::CONTENTS_LIGHTSABER,
                model: None,
            });
        }
        let start = bodies.len();
        host.player_saber_boxes(bodies);
        let mut at = start;
        while at < bodies.len() {
            if host
                .player_saber(bodies[at].entity)
                .is_some_and(|owner| owner.number == ent.owner)
            {
                bodies.remove(at);
            } else {
                at += 1;
            }
        }
        host.trace(
            ent.origin,
            ent.mins,
            ent.maxs,
            end,
            ent.number,
            ent.clip_mask,
            bodies,
        )
    }

    /// `G_SetEnemy(self, owner)` for an NPC without an enemy (`w_saber.c:5752-5762`): a shooter
    /// alive (or a body at zero) of another team, or no client at all.
    fn take_shooter_for_enemy(&mut self, me: usize, owner: u16) {
        if self.actors[me].mind.enemy.is_some() || owner == ENTITYNUM_NONE {
            return;
        }
        let own_team = self.actors[me].player_team;
        let takes = match self.body(owner) {
            Some(shooter) => shooter.health >= 0 && shooter.player_team != own_team,
            None => self
                .host
                .damageable_health(owner)
                .is_none_or(|health| health >= 0),
        };
        if takes {
            self.set_enemy(me, owner);
        }
    }

    /// The missile met (`w_saber.c:5810-5836`): an ambusher drops; a hovering Boba Fett, not
    /// before a homing rocket, strafes or changes height now and then; any other blocks or
    /// dodges it (`Jedi_SaberBlockGo` with its last command), lighting its saber to do so.
    fn meet_incoming(&mut self, me: usize, incoming: &IncomingEntity) {
        if self.jedi_waiting_ambush(me) {
            self.jedi_ambush(me);
        }
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let boba = npc.definition.client_class == CLASS_BOBAFETT;
        let flying = npc
            .player
            .raw_field(crate::npc_saber::PS_EFLAGS2)
            .unwrap_or(0)
            & EF2_FLYING
            != 0;
        if boba && flying && incoming.method_of_death != MOD_ROCKET_HOMING {
            if self.host.irand(0, 1) == 0 {
                self.actors[me].mind.stand_time = 0;
                let until = level_time + self.host.irand(1_000, 2_000);
                self.actors[me].force.debounce[FP_SABER_DEFENSE] = until;
            }
            if self.host.irand(0, 1) == 0 {
                let duration = self.host.irand(1_000, 3_000);
                self.actors[me]
                    .mind
                    .timers
                    .set("heightChange", level_time, duration);
                let until = level_time + self.host.irand(1_000, 2_000);
                self.actors[me].force.debounce[FP_SABER_DEFENSE] = until;
            }
            return;
        }
        let mut command = self.actors[me].mind.last_command;
        let evasion =
            self.jedi_saber_block_go(me, &mut command, None, None, Some(incoming.number), 0.0);
        self.actors[me].mind.last_command = command;
        if evasion != EvasionType::None && !boba {
            self.activate_saber(me);
        }
    }
}

/// `G_MissileImpact`'s saber block by the NPC `npc` (`g_missile.c:498-640`): struck itself,
/// its saber may block (`WP_SaberCanBlock`, `saberBlockTime`); struck on its saber entity
/// (`on_saber`), the blade turns the missile aside whatever it is doing. `shooter_origin` is
/// where the missile's owner stands (the reflection aims back at it); `rand` is the C
/// library's generator. A block marks the saber's deed (`SEF_DEFLECTED`) for the Jedi AI.
pub fn npc_missile_defence(
    npc: &mut NpcActor,
    missile: &mut Missile,
    normal: [f32; 3],
    on_saber: bool,
    shooter_origin: [f32; 3],
    level_time: i32,
    rand: &mut CrtRand,
) -> Option<SaberBlock> {
    let saber_blocking = npc.movement.state().saber_blocking;
    let command = npc.mind.command;
    let defense = npc
        .force_levels
        .get(FP_SABER_DEFENSE)
        .copied()
        .unwrap_or(0)
        .clamp(0, 255) as u8;
    let mut defender = Defender {
        client: npc.number,
        state: &mut npc.player,
        saber_blocking,
        buttons: command.buttons,
        forward_move: command.forward_move,
        defense,
        block_time: &mut npc.saber.block_time,
    };
    let block = if on_saber {
        block_on_saber(
            &mut defender,
            missile,
            normal,
            shooter_origin,
            level_time,
            rand,
        )
    } else {
        block_missile(
            &mut defender,
            missile,
            normal,
            shooter_origin,
            level_time,
            rand,
        )
    };
    if block.is_some() {
        npc.saber.event_flags |= crate::saber_clash::sef::DEFLECTED;
    }
    block
}

//! `G_Damage` on an NPC (`codemp/game/g_combat.c:4425-5560`, the branches a target with a
//! client and `s.eType == ET_NPC` takes): the DEMP2's electrocution, the attacker's
//! handicap (a player's only), where the blow landed (by the NPC's own facing), the
//! rancor's cap on what hurts it, the knockback and its movement lock, `OnSameTeam`'s NPC
//! rules and an NPC attacker's allied team, the attacker's hit counter, the armour
//! (`CheckArmor`), a droid's weakness to the DEMP2, the health and its bar
//! (`G_ScaleNetHealth`), and then the death (`player_die`, [`crate::npc_death`]) or the
//! pain (`ent->pain`, [`crate::npc_pain`]).
//!
//! Force rage halves the blow and protection pays part of it from the pool. Not here, and
//! said so: siege's round and class rules and the battle suit. A blade's surface is the
//! caller's ([`NpcBlow::surface`]); a missile's is the host's model
//! (`d_projectileGhoul2Collision`); a cut corpse is [`crate::npc_dismember_check`]'s.
//!
//! Held to `tools/game-oracle/npccombat.c` (`game-npccombat.txt`).

use crate::damage::{
    Attacker, DAMAGE_HALF_ABSORB, DAMAGE_NO_ARMOR, DAMAGE_NO_HIT_LOC, DAMAGE_NO_KNOCKBACK,
    DAMAGE_NO_PROTECTION, DAMAGE_SABER_KNOCKBACK1, DAMAGE_SABER_KNOCKBACK1_B2,
    DAMAGE_SABER_KNOCKBACK2, DAMAGE_SABER_KNOCKBACK2_B2, DamageRequest, HitLocation, OtherKiller,
    hit_location, location_modified,
};
use crate::event_entity::EventEntity;
use crate::means_of_death::{MOD_DEMP2, MOD_DEMP2_ALT, MOD_SABER};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;

/// `g_knockback`'s default.
const KNOCKBACK: f32 = 1_000.0;
/// `ARMOR_PROTECTION`.
const ARMOR_PROTECTION: f64 = 0.5;
/// `EV_SHIELD_HIT`.
const EV_SHIELD_HIT: u32 = 110;
/// `PMF_TIME_KNOCKBACK`.
const PMF_TIME_KNOCKBACK: u16 = 0x40;
/// `STAT_HEALTH`, `STAT_ARMOR`, `PERS_ATTACKER`.
const STAT_HEALTH: usize = 0;
const STAT_ARMOR: usize = 5;
const PERS_ATTACKER: usize = 6;
/// `ps.electrifyTime`, `ps.fd.forcePowersActive`.
const PS_ELECTRIFY_TIME: usize = 73;
const PS_FORCE_ACTIVE: usize = 82;
/// `1 << FP_RAGE` in `fd.forcePowersActive` (`FP_RAGE` is 8; step 523 tested bit 6,
/// `FP_GRIP`'s).
const FP_RAGE: u32 = 1 << crate::force_powers::FP_RAGE;
/// `FP_PROTECT`'s bit; `fd.forcePower`; `PW_FORCE_BOON`; `PDSOUND_PROTECTHIT`.
const FP_PROTECT: u32 = 1 << crate::force_powers::FP_PROTECT;
const PS_FORCE_POWER: usize = 18;
const PW_FORCE_BOON: usize = 14;
const PDSOUND_PROTECTHIT: u32 = 1;
/// `FL_*` of an NPC that `G_Damage` reads.
const FL_NO_KNOCKBACK: u32 = 0x800;
/// `DAMAGE_NO_DISMEMBER`; `EF_DEAD` in `s.eFlags` (its wire field).
const DAMAGE_NO_DISMEMBER: u32 = 0x8000;
const EF_DEAD: u32 = 1 << 1;
const ES_EFLAGS: usize = 19;
const FL_UNDYING: u32 = 0x10_0000;
const FL_DMG_BY_SABER_ONLY: u32 = 0x100_0000;
const FL_DMG_BY_HEAVY_WEAP_ONLY: u32 = 0x200_0000;
/// `ENTITYNUM_WORLD`.
const ENTITY_WORLD: u16 = 1_022;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: u16 = 32;
/// `GT_TEAM`, `GT_SIEGE`.
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
/// `TEAM_FREE`.
const TEAM_FREE: i32 = 0;
/// `CLASS_RANCOR`, and the droids the DEMP2 hurts more.
const CLASS_RANCOR: i32 = 54;
const DEMP2_TWICE: [i32; 6] = [33, 41, 34, 35, 29, 11];
const DEMP2_FIVE_TIMES: [i32; 6] = [32, 16, 23, 24, 42, 1];

/// A blow on an NPC as the caller knows it: what `G_Damage` is given, and what it reads of
/// the game beyond the NPCs.
#[derive(Clone, Copy, Debug)]
pub struct NpcBlow {
    /// What `G_Damage` is given: the attacker, the direction and point, the damage, the
    /// flags and the means.
    pub request: DamageRequest,
    /// `G_Damage`'s Jedi Master rule holds between the NPC and the attacker (the push lands,
    /// nothing more): [`crate::jedi_master::spares`].
    pub spared_by_master: bool,
    /// Where a blade struck the NPC's posed model this frame, by the surface it named
    /// (`G_LocationBasedDamageModifier`'s Ghoul2 half, `g_combat.c:4306-4317`:
    /// [`crate::saber_damage::location_from_surface`]); `None` places the blow by the NPC's
    /// box (`G_GetHitLocation`).
    pub surface: Option<HitLocation>,
}

/// What a blow on an NPC came to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NpcDamaged {
    /// The health taken, after the armour.
    pub take: i32,
    /// What the attacker's persistants gain (`PERS_HITS`, `PERS_ATTACKEE_ARMOR`), for an
    /// attacker with a client; the caller applies it to a player, the roster to an NPC.
    pub attacker_hits: i32,
    pub attackee_armor: Option<u32>,
    /// The blow killed (`player_die` ran; a corpse's again returns at once).
    pub died: bool,
    /// The blow got as far as the health (not spared by a team or the master).
    pub landed: bool,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_Damage(npc, inflictor, attacker, dir, point, damage, dflags, mod)` for the NPC at
    /// `me`.
    pub fn damage(&mut self, me: usize, blow: NpcBlow) -> NpcDamaged {
        let request = blow.request;
        let level_time = self.level_time;
        let mut damaged = NpcDamaged::default();
        let attacker = request.attacker;
        let attacker_number = attacker.map_or(ENTITY_WORLD, |attacker| attacker.client);
        let by_another = attacker_number != self.actors[me].number;
        let attacker_body = attacker.and_then(|attacker| self.body(attacker.client));
        let gametype = self.host.gametype();
        // The DEMP2's electrocution, the first thing it does to a client.
        if request.means == MOD_DEMP2
            && (self.actors[me]
                .player
                .raw_field(PS_ELECTRIFY_TIME)
                .unwrap_or(0) as i32)
                < level_time
        {
            // A vehicle's own jolt ([`crate::vehicle_damage::electrify`]).
            let (actors, host) = (&mut *self.actors, &mut *self.host);
            if let Some(until) =
                crate::vehicle_damage::electrify(&actors[me], level_time, &mut |low, high| {
                    host.irand(low, high)
                })
            {
                actors[me]
                    .player
                    .set_raw_field(PS_ELECTRIFY_TIME, until as u32);
            }
        }
        let npc = &mut self.actors[me];
        if !npc.takes_damage
            || (npc.flags & FL_DMG_BY_SABER_ONLY != 0 && request.means != MOD_SABER)
        {
            return damaged;
        }
        if npc.flags & FL_DMG_BY_HEAVY_WEAP_ONLY != 0 && !heavy(request.means) {
            // A class with heavy melee could punch such a thing; no NPC has the flag.
            return damaged;
        }
        let mut damage = request.damage;
        let mut flags = request.flags;
        let rage = flags & DAMAGE_NO_PROTECTION == 0
            && npc.player.raw_field(PS_FORCE_ACTIVE).unwrap_or(0) & FP_RAGE != 0;
        if rage {
            damage = (f64::from(damage) * 0.5) as i32;
        }
        // A player's handicap (`attacker->s.eType == ET_PLAYER`), outside siege.
        if let Some(attacker) = attacker
            && by_another
            && !attacker.npc
            && attacker.client < MAX_CLIENTS
            && gametype != GT_SIEGE
        {
            damage = damage * attacker.max_health / 100;
        }
        // Where it landed, for a blow by a client or an NPC (`g_combat.c:4602-4610`).
        let placed = attacker_body.is_some()
            && flags & DAMAGE_NO_HIT_LOC == 0
            && !(request.means == MOD_SABER && damage <= 1)
            && crate::vehicle_damage::located(npc);
        if placed && let Some(point) = request.point {
            // A blade's surface is the caller's; with `d_projectileGhoul2Collision` on, any
            // blow finds the surface the model was struck on this frame (a missile's), else
            // the NPC's box places it.
            let surface = match blow.surface {
                Some(surface) => Some(surface),
                None if self.host.projectile_ghoul2_collision() => {
                    let (actors, host) = (&*self.actors, &mut *self.host);
                    host.npc_surface_location(&actors[me], flags, point, level_time)
                }
                None => None,
            };
            let npc = &mut self.actors[me];
            let location = surface
                .unwrap_or_else(|| hit_location(npc.mind.current_angles[1], npc.link, point));
            damage = location_modified(damage, location);
        }
        let npc = &mut self.actors[me];
        // The rancor shrugs off anything under ten, and takes ten of anything more.
        if npc.definition.client_class == CLASS_RANCOR
            && attacker_body.is_none_or(|body| body.class != CLASS_RANCOR)
        {
            damage = if damage < 10 { 0 } else { 10 };
        }
        let direction = match request.direction {
            Some(direction) => {
                let length = direction.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
                if length != 0.0 {
                    direction.map(|axis| axis * (1.0 / length))
                } else {
                    direction
                }
            }
            None => {
                flags |= DAMAGE_NO_KNOCKBACK;
                [0.0; 3]
            }
        };
        let mut knockback = damage.min(200);
        if npc.flags & FL_NO_KNOCKBACK != 0 || flags & DAMAGE_NO_KNOCKBACK != 0 {
            knockback = 0;
        }
        if knockback != 0 {
            push(
                npc,
                attacker,
                attacker_body.is_some() && by_another,
                direction,
                knockback,
                flags,
                request.means,
                level_time,
            );
        }
        // A vehicle remembers who shot it for longer, pushed or not.
        if let Some(attacker) =
            attacker.filter(|_| by_another && (knockback == 0 || attacker_body.is_some()))
            && let Some(remembered) =
                crate::vehicle_damage::remembered(npc, attacker.client, level_time)
        {
            npc.mind.fight.other_killer = remembered;
        }
        // `OnSameTeam` and an allied NPC's blow (`g_combat.c:4787-4845`), with
        // `g_friendlyFire` 0; and the Jedi Master's rule.
        if flags & DAMAGE_NO_PROTECTION == 0
            && by_another
            && (self.same_team(me, attacker_body.as_ref(), gametype) || blow.spared_by_master)
        {
            return damaged;
        }
        let npc = &mut self.actors[me];
        // The attacker's hit counter: a client hurting a living NPC.
        if attacker_body.is_some() && by_another && npc.health > 0 {
            damaged.attacker_hits = if self.same_team(me, attacker_body.as_ref(), gametype) {
                -1
            } else {
                1
            };
            let npc = &self.actors[me];
            damaged.attackee_armor =
                Some(((npc.health as u32) << 8) | npc.player.stats[STAT_ARMOR]);
        }
        let npc = &mut self.actors[me];
        // Half of it hurting itself (siege: half as much again), unless the blow says not.
        if !by_another && flags & crate::damage::DAMAGE_NO_SELF_PROTECTION == 0 {
            damage = (f64::from(damage) * if gametype == GT_SIEGE { 1.5 } else { 0.5 }) as i32;
        }
        let damage = damage.max(1);
        let mut take = damage;
        // `CheckArmor`.
        let armor = npc.player.stats[STAT_ARMOR] as i32;
        let mut save = if flags & DAMAGE_HALF_ABSORB != 0 {
            (f64::from(damage) * ARMOR_PROTECTION).ceil() as i32
        } else {
            damage
        };
        save = save.min(armor);
        if flags & DAMAGE_NO_ARMOR != 0 || damage == 0 {
            save = 0;
        }
        if save > 0 {
            npc.player.stats[STAT_ARMOR] = (armor - save) as u32;
        }
        take -= save;
        let attacker_number = attacker.map_or(ENTITY_WORLD, |attacker| attacker.client);
        crate::vehicle_damage::armor_taken(npc, take, attacker_number);
        crate::vehicle_damage::knocked(npc, damage, attacker_number, request.point);
        // The DEMP2: twice on the small droids, five times on the big ones, a third on
        // anything else (one at least).
        if matches!(request.means, MOD_DEMP2 | MOD_DEMP2_ALT) {
            let class = npc.definition.client_class;
            if DEMP2_TWICE.contains(&class) {
                take *= 2;
            } else if DEMP2_FIVE_TIMES.contains(&class) {
                take *= 5;
            } else if take > 0 {
                take = (take / 3).max(1);
            }
        }
        // The frame's totals, which no frame's end reads for an NPC.
        npc.player.persistent[PERS_ATTACKER] = u32::from(attacker_number);
        npc.mind.fight.last_hurt = Some((attacker_number, request.means));
        // Force protection pays part of it from the pool (`g_combat.c:5245-5310`).
        let pool = npc.player.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32;
        if flags & DAMAGE_NO_PROTECTION == 0
            && take != 0
            && npc.player.raw_field(PS_FORCE_ACTIVE).unwrap_or(0) & FP_PROTECT != 0
            && pool != 0
        {
            if npc.force.sound_debounce < level_time {
                let origin = npc.player.origin();
                npc.force.sound_debounce = level_time + 400;
                self.host
                    .raise(crate::knockdown::predef_sound(origin, PDSOUND_PROTECTHIT));
            }
            let npc = &mut self.actors[me];
            let boon = npc.player.powerups[PW_FORCE_BOON] != 0;
            let (pool, left) = crate::damage::protect_share(
                pool,
                npc.force.levels[crate::force_powers::FP_PROTECT],
                boon,
                take,
            );
            npc.player.set_raw_field(PS_FORCE_POWER, pool as u32);
            take = left;
        }
        let npc = &mut self.actors[me];
        damaged.landed = true;
        if save > 0 {
            // The shield shell, facing the hit.
            let mut flash = EventEntity {
                event: EV_SHIELD_HIT,
                parameter: u32::from(sjk_protocol::legacy_direction_to_byte(direction)),
                origin: npc.current_origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            flash.extra[0] = (59, u32::from(npc.number));
            flash.extra[1] = (61, save as u32);
            self.host.raise(flash);
        }
        damaged.take = take;
        if take != 0 {
            damaged.died = self.take_health(me, take, rage && attacker_body.is_some(), request);
        }
        damaged
    }

    /// The health taken (`g_combat.c:5362-5555`): rage's division and floor, `FL_UNDYING`,
    /// the stat, the bar; then `player_die`, or the pain. Returns whether it died.
    fn take_health(&mut self, me: usize, take: i32, raging: bool, request: DamageRequest) -> bool {
        let npc = &mut self.actors[me];
        let mut take = take;
        if raging {
            let level = npc
                .force_levels
                .get(crate::force_powers::FP_RAGE)
                .copied()
                .unwrap_or(0);
            take /= level + 1;
        }
        npc.health -= take;
        if npc.flags & FL_UNDYING != 0 && npc.health < 1 {
            npc.health = 1;
        }
        npc.player.stats[STAT_HEALTH] = npc.health as u32;
        if raging {
            npc.health = npc.health.max(1);
            npc.player.stats[STAT_HEALTH] = (npc.player.stats[STAT_HEALTH] as i32).max(1) as u32;
        }
        if npc.bar_max_health != 0 {
            crate::npc_begin::scale_net_health(npc);
        }
        let attacker = request.attacker.map(|attacker| attacker.client);
        // `gPainHitLoc` and `locationDamage` ([`crate::npc_machine_parts`]).
        self.note_pain_part(me, take);
        let npc = &mut self.actors[me];
        if npc.health <= 0 {
            npc.flags |= FL_NO_KNOCKBACK;
            npc.mind.fight.death_point = request.point.unwrap_or(npc.player.origin());
            npc.health = npc.health.max(-999);
            // "An NPC that's already dead. Maybe we can cut some more limbs off!"
            // (`g_combat.c:5502-5510`).
            let killer = attacker.unwrap_or(ENTITY_WORLD);
            if npc.state.raw_field(ES_EFLAGS).unwrap_or(0) & EF_DEAD != 0
                && take > 2
                && request.flags & DAMAGE_NO_DISMEMBER == 0
                && self.cuts_limbs(request.means, killer)
            {
                let npc = &self.actors[me];
                let check = crate::npc_dismember_check::DismemberCheck {
                    victim: npc.number,
                    enemy: killer,
                    point: npc.mind.fight.death_point,
                    damage: take,
                    death_anim: npc.player.torso_animation(),
                    post_death: true,
                    avoid: self.level.avoid_dismember,
                };
                self.check_for_dismemberment(check);
            }
            let npc = &mut self.actors[me];
            npc.mind.enemy = Some(killer);
            self.die(me, attacker.unwrap_or(ENTITY_WORLD), take, request.means);
            return true;
        }
        // No pain for a saber's idle touch; pain from where it landed, else where the NPC is.
        if request.means != MOD_SABER || take > 1 {
            let point = request.point.unwrap_or(npc.current_origin);
            self.pain(me, attacker, take, request.means, point);
        }
        false
    }

    /// `OnSameTeam(npc, attacker)` and an allied NPC's blow (`g_team.c:206-288`,
    /// `g_combat.c:4787-4830`) in a team game: two NPCs of the same session team (neither
    /// free); never an NPC and a player; an NPC attacker allied with the NPC's team.
    fn same_team(
        &self,
        me: usize,
        attacker: Option<&crate::npc_senses::Body>,
        gametype: i32,
    ) -> bool {
        let Some(attacker) = attacker else {
            return false;
        };
        if gametype < GT_TEAM || !attacker.npc {
            return false;
        }
        let npc = &self.actors[me];
        let (mine, theirs) = (npc.session_team, attacker.session_team);
        let allied = self.actor_at(attacker.number).is_some_and(|at| {
            self.actors[at].allied_team != 0 && self.actors[at].allied_team == mine
        });
        (mine == theirs && !(mine == TEAM_FREE && theirs == TEAM_FREE)) || allied
    }
}

/// The knockback's push (`g_combat.c:4630-4738`): the velocity along the blow — a saber's
/// scaled by `g_saberDmgVelocityScale`, 0, unless the blow asks for its sabers' own
/// scales — the attacker (a client) remembered as the one who pushed, and the movement
/// locked while the push lasts.
#[allow(clippy::too_many_arguments)]
fn push(
    npc: &mut crate::npc_spawn::NpcActor,
    attacker: Option<Attacker>,
    credited: bool,
    direction: [f32; 3],
    knockback: i32,
    flags: u32,
    means: u32,
    level_time: i32,
) {
    let saber = means == MOD_SABER;
    let pushed_by_saber = saber && flags & (DAMAGE_SABER_KNOCKBACK1 | DAMAGE_SABER_KNOCKBACK2) != 0;
    let scale = if saber {
        let mut saber_scale = 0.0;
        if pushed_by_saber {
            saber_scale = 1.0;
            let scales = attacker.map_or([0.0; 4], |attacker| attacker.saber_knockback);
            for (bit, factor) in [
                DAMAGE_SABER_KNOCKBACK1,
                DAMAGE_SABER_KNOCKBACK1_B2,
                DAMAGE_SABER_KNOCKBACK2,
                DAMAGE_SABER_KNOCKBACK2_B2,
            ]
            .into_iter()
            .zip(scales)
            {
                if flags & bit != 0 {
                    saber_scale *= factor;
                }
            }
        }
        (KNOCKBACK * knockback as f32 / 200.0) * saber_scale
    } else {
        KNOCKBACK * knockback as f32 / 200.0
    };
    let velocity = npc.player.velocity();
    npc.player.set_velocity(std::array::from_fn(|axis| {
        velocity[axis] + direction[axis] * scale
    }));
    if credited && let Some(attacker) = attacker {
        npc.mind.fight.other_killer = OtherKiller::credit(attacker.client, level_time);
    }
    if npc.player.movement_time() == 0 && (!saber || pushed_by_saber) {
        let lock = (knockback * 2).clamp(50, 200);
        npc.player.set_movement_time(lock as i16);
        npc.player
            .set_movement_flags(npc.player.movement_flags() | PMF_TIME_KNOCKBACK);
    }
}

/// The means `FL_DMG_BY_HEAVY_WEAP_ONLY` lets through (`g_combat.c:4484-4510`).
fn heavy(means: u32) -> bool {
    use crate::means_of_death::*;
    matches!(
        means,
        MOD_REPEATER_ALT
            | MOD_ROCKET
            | MOD_FLECHETTE_ALT_SPLASH
            | MOD_ROCKET_HOMING
            | MOD_THERMAL
            | MOD_THERMAL_SPLASH
            | MOD_TRIP_MINE_SPLASH
            | MOD_TIMED_MINE_SPLASH
            | MOD_DET_PACK_SPLASH
            | MOD_VEHICLE
            | MOD_CONC
            | MOD_CONC_ALT
            | MOD_SABER
            | MOD_TURBLAST
            | MOD_SUICIDE
            | MOD_FALLING
            | MOD_CRUSH
            | MOD_TELEFRAG
            | MOD_TRIGGER_HURT
    )
}

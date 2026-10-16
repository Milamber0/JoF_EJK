//! `G_Damage` for a player (OpenJK `codemp/game/g_combat.c:4425-5560`), as far as a
//! free-for-all's shots reach it: the attacker's handicap, the hit's location and what
//! it does to the damage (`G_LocationBasedDamageModifier`, `G_GetHitLocation`,
//! `:65-285`, `:4277-4360`), the knockback and its movement lock, the spawn's
//! protection, the attacker's hit counter, the armour's share (`CheckArmor`,
//! `:2926-2975`) and its shield flash, the health taken — and, at the end of the
//! frame, what the hurt player is told of it (`P_DamageFeedback`, `g_active.c:53-135`).
//! A death is reported to the caller, who runs `player_die` (`player_death`).
//!
//! Not here: vehicles, NPCs, siege, duels, the Jedi Master, Force rage and protect,
//! the battle suit, DEMP2 shocks, dismemberment and arm breakage, movers and breakable
//! brushes, god mode, `g_friendlyFire` 1 (a teammate's blow always stops, as at the
//! default).

use sjk_protocol::{PlayerState, legacy_direction_to_byte};

use crate::event_entity::EventEntity;

/// `hitLocation_t` (`bg_public.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitLocation {
    None,
    FootRight,
    FootLeft,
    LegRight,
    LegLeft,
    Waist,
    BackRight,
    BackLeft,
    Back,
    ChestRight,
    ChestLeft,
    Chest,
    ArmRight,
    ArmLeft,
    HandRight,
    HandLeft,
    Head,
}

/// `DAMAGE_*` flags (`g_local.h:1165-1185`).
pub const DAMAGE_NO_ARMOR: u32 = 0x2;
pub const DAMAGE_NO_KNOCKBACK: u32 = 0x4;
/// `DAMAGE_SABER_KNOCKBACK1`, `2`, `1_B2`, `2_B2`: a saber blow pushed by its saber's
/// `knockbackScale` (first saber, second saber; `_B2`: the second blade style's).
pub const DAMAGE_SABER_KNOCKBACK1: u32 = 0x1_0000;
pub const DAMAGE_SABER_KNOCKBACK2: u32 = 0x2_0000;
pub const DAMAGE_SABER_KNOCKBACK1_B2: u32 = 0x4_0000;
pub const DAMAGE_SABER_KNOCKBACK2_B2: u32 = 0x8_0000;
pub const DAMAGE_NO_PROTECTION: u32 = 0x8;
pub const DAMAGE_HALF_ABSORB: u32 = 0x400;
pub const DAMAGE_NO_HIT_LOC: u32 = 0x2000;
pub const DAMAGE_NO_SELF_PROTECTION: u32 = 0x4000;
/// `g_knockback`'s default.
const KNOCKBACK: f32 = 1_000.0;
/// `ARMOR_PROTECTION`.
const ARMOR_PROTECTION: f64 = 0.5;
/// `EV_PAIN`, `EV_SHIELD_HIT`.
const EV_PAIN: u32 = 89;
const EV_SHIELD_HIT: u32 = 110;
const EF_INVULNERABLE: u32 = 1 << 27;
const PMF_TIME_KNOCKBACK: u16 = 0x40;
const PM_DEAD: u8 = 5;
const STAT_HEALTH: usize = 0;
const STAT_ARMOR: usize = 5;
const PERS_ATTACKER: usize = 6;
/// `MAX_CLIENTS`: an entity numbered at least this is no player.
const MAX_CLIENTS: u16 = 32;
const PS_EFLAGS: usize = 17;
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
const PS_DAMAGE_YAW: usize = 57;
const PS_DAMAGE_COUNT: usize = 58;
const PS_DAMAGE_EVENT: usize = 70;
const PS_DAMAGE_TYPE: usize = 78;
const PS_DAMAGE_PITCH: usize = 83;
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;
/// `ENTITYNUM_WORLD`.
const ENTITY_WORLD: u16 = 1_022;

/// `G_GetHitLocation` (`g_combat.c:65-285`): where on the target the point is, from the
/// target's centre, in fifths along its up, forward and right — a player's forward being
/// `r.currentAngles`' yaw, which nothing sets for a player in the reference, so it is
/// `0`. `bounds` is the linked box, grown by a unit each way (`r.absmin`, `r.absmax`).
pub fn hit_location(yaw: f32, bounds: ([f32; 3], [f32; 3]), point: [f32; 3]) -> HitLocation {
    if point == [0.0; 3] {
        return HitLocation::None;
    }
    let (forward, right) = crate::pmove::flight::flight_axes([0.0, yaw, 0.0]);
    let (forward, right) = (forward.to_array(), right.to_array());
    let up = [0.0, 0.0, 1.0];
    let centre: [f32; 3] = std::array::from_fn(|axis| (bounds.0[axis] + bounds.1[axis]) * 0.5);
    let mut direction: [f32; 3] = std::array::from_fn(|axis| point[axis] - centre[axis]);
    let length = direction.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
    if length != 0.0 {
        direction = direction.map(|axis| axis * (1.0 / length));
    }
    let dot = |vector: [f32; 3]| {
        vector
            .iter()
            .zip(direction)
            .map(|(a, b)| a * b)
            .sum::<f32>()
    };
    let (udot, fdot, rdot) = (dot(up), dot(forward), dot(right));
    let vertical = if udot > 0.800 {
        4
    } else if udot > 0.400 {
        3
    } else if udot > -0.333 {
        2
    } else if udot > -0.666 {
        1
    } else {
        0
    };
    let fifth = |value: f32| {
        if value > 0.666 {
            4
        } else if value > 0.333 {
            3
        } else if value > -0.333 {
            2
        } else if value > -0.666 {
            1
        } else {
            0
        }
    };
    let code = vertical * 25 + fifth(fdot) * 5 + fifth(rdot);
    let side = |right_side: HitLocation, left_side: HitLocation| {
        if rdot > 0.0 { right_side } else { left_side }
    };
    if code <= 10 {
        side(HitLocation::FootRight, HitLocation::FootLeft)
    } else if code <= 50 {
        side(HitLocation::LegRight, HitLocation::LegLeft)
    } else if matches!(code, 56 | 60 | 61 | 65 | 66 | 70) {
        side(HitLocation::HandRight, HitLocation::HandLeft)
    } else if matches!(code, 83 | 87 | 88 | 92 | 93 | 97) {
        side(HitLocation::ArmRight, HitLocation::ArmLeft)
    } else if matches!(code, 107..=109 | 112..=114 | 117..=119) {
        HitLocation::Head
    } else if udot < 0.3 {
        HitLocation::Waist
    } else if fdot < 0.0 {
        if rdot > 0.4 {
            HitLocation::BackRight
        } else if rdot < -0.4 {
            HitLocation::BackLeft
        } else {
            HitLocation::Back
        }
    } else if rdot > 0.3 {
        HitLocation::ChestRight
    } else if rdot < -0.3 {
        HitLocation::ChestLeft
    } else {
        // `else if (fdot < 0)` in a branch where it is not: nothing is returned for a
        // chest hit square on, and the function falls through to `HL_NONE`.
        HitLocation::None
    }
}

/// `G_LocationBasedDamageModifier`'s table (`g_combat.c:4324-4356`): the damage scaled by
/// where it landed, a float product truncated.
pub fn location_modified(damage: i32, location: HitLocation) -> i32 {
    let factor = match location {
        HitLocation::FootRight | HitLocation::FootLeft => 0.5,
        HitLocation::LegRight | HitLocation::LegLeft => 0.7,
        HitLocation::ArmRight | HitLocation::ArmLeft => 0.85,
        HitLocation::HandRight | HitLocation::HandLeft => 0.6,
        HitLocation::Head => 1.3,
        _ => return damage,
    };
    (f64::from(damage) * factor) as i32
}

/// The hurt player's bookkeeping between the hit and the frame's end (`gclient_t`'s
/// `damage_*`, `pain_debounce_time`, `lasthurt_*`; `gentity_t`'s `FL_NO_KNOCKBACK` and
/// `pos1`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Wounds {
    /// `damage_armor`, `damage_blood`, `damage_knockback`: this frame's totals.
    pub armor: i32,
    pub blood: i32,
    pub knockback: i32,
    /// `damage_from`, `damage_fromWorld`: where the last hit came from.
    pub from: [f32; 3],
    pub from_world: bool,
    /// `pain_debounce_time`: no pain sound before it.
    pub pain_debounce_time: i32,
    /// `lasthurt_client`, `lasthurt_mod`.
    pub last_hurt: Option<(u16, u32)>,
    /// `FL_NO_KNOCKBACK`: a corpse is not pushed around.
    pub no_knockback: bool,
    /// `pos1`: the point of the killing blow, for the death animation.
    pub death_point: [f32; 3],
    /// `ps.otherKiller`, `otherKillerTime`, `otherKillerDebounceTime`: who last pushed the
    /// player, credited with a fall that follows.
    pub other_killer: OtherKiller,
}

/// `ps.otherKiller`, `otherKillerTime`, `otherKillerDebounceTime`: who last pushed, shoved,
/// held or knocked the player down, credited with a fall to death that follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OtherKiller {
    pub number: u16,
    pub time: i32,
    pub debounce_time: i32,
}

impl OtherKiller {
    /// `number` credited for five seconds, not to be forgotten for a tenth of one.
    pub fn credit(number: u16, level_time: i32) -> Self {
        Self {
            number,
            time: level_time + 5_000,
            debounce_time: level_time + 100,
        }
    }

    /// `ClientEndFrame`'s upkeep (`g_active.c:2789-2803`): forgotten once the player is
    /// back on the ground past the debounce (`ENTITYNUM_NONE`), kept while it is in the air.
    pub fn end_frame(&mut self, grounded: bool, level_time: i32) {
        if self.time > level_time && grounded && self.debounce_time < level_time {
            (self.time, self.number) = (0, ENTITYNUM_NONE);
        } else if self.time > level_time && !grounded && self.debounce_time < level_time + 100 {
            self.debounce_time = level_time + 100;
        }
    }

    /// Whom a fall to death now is put down to (`g_active.c:2764-2775`), if anyone.
    pub fn credited(&self, level_time: i32) -> Option<u16> {
        (self.time > level_time && self.number != ENTITYNUM_NONE).then_some(self.number)
    }
}

/// `ENTITYNUM_NONE`.
const ENTITYNUM_NONE: u16 = 1_023;

/// What `G_Damage` is given, besides the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageRequest {
    /// `level.time`.
    pub level_time: i32,
    /// The attacker, `None` for the world; with what `G_Damage` reads of it.
    pub attacker: Option<Attacker>,
    /// The damage's direction, `None` for none (no knockback).
    pub direction: Option<[f32; 3]>,
    /// Where it landed, `None` for nowhere (no location).
    pub point: Option<[f32; 3]>,
    pub damage: i32,
    pub flags: u32,
    /// `meansOfDeath`.
    pub means: u32,
}

/// The attacker as `G_Damage` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attacker {
    /// An NPC: a client (`attacker->client`) that is no player (`s.eType == ET_NPC`).
    pub npc: bool,
    /// Its wire client number — an NPC's entity number.
    pub client: u16,
    /// `ps.stats[STAT_MAX_HEALTH]`: the handicap.
    pub max_health: i32,
    /// `sess.sessionTeam`, for `OnSameTeam`.
    pub team: i32,
    /// Its sabers' `knockbackScale` and `knockbackScale2`, first saber then second, which a
    /// saber blow's `DAMAGE_SABER_KNOCKBACK*` flags ask for.
    pub saber_knockback: [f32; 4],
}

/// The target as `G_Damage` reads and changes it.
pub struct Target<'a> {
    /// Its wire client number.
    pub client: u16,
    pub state: &'a mut PlayerState,
    /// `gentity_t::health`.
    pub health: &'a mut i32,
    /// `r.currentOrigin` and the linked box (`r.mins`, `r.maxs`).
    pub origin: [f32; 3],
    pub bounds: ([f32; 3], [f32; 3]),
    /// `invulnerableTimer`.
    pub invulnerable_until: &'a mut i32,
    /// `sess.sessionTeam`.
    pub team: i32,
    pub wounds: &'a mut Wounds,
    /// Its Force no client is sent, whose protection and rage guard against the damage;
    /// `None` for a target with none.
    pub force: Option<&'a mut crate::force_powers::ForcePowers>,
    /// `G_Damage`'s Jedi Master rule holds between this target and the attacker
    /// ([`crate::jedi_master::spares`]): the blow's push lands, nothing more.
    pub spared_by_master: bool,
}

/// What `G_Damage` came to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Damaged {
    /// The health taken, after the armour.
    pub take: i32,
    /// The armour's share, shown as a shield flash (`EV_SHIELD_HIT`) when any.
    pub absorbed: i32,
    /// The shield flash.
    pub events: Vec<EventEntity>,
    /// What the attacker's persistants gain: `PERS_HITS` (a hit on a foe, minus one on
    /// a teammate) and `PERS_ATTACKEE_ARMOR`.
    pub attacker_hits: i32,
    pub attackee_armor: Option<u32>,
    /// The blow got as far as the frame's damage totals (not spared by a teammate, the
    /// spawn's protection or god mode): where `Team_CheckHurtCarrier` looks.
    pub landed: bool,
    /// The target died: the caller runs `player_die` with this damage and point.
    pub died: Option<Death>,
}

/// A killing blow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Death {
    pub take: i32,
    pub point: [f32; 3],
    pub means: u32,
    pub attacker: Option<u16>,
}

/// `G_Damage` for a player target that takes damage (a corpse does too: its health goes
/// further down and its `die` returns at once). Returns `None` where the reference
/// returns before anything: nothing here yet does.
pub fn damage(
    target: &mut Target<'_>,
    request: DamageRequest,
    rng: &mut crate::player_death::Rng,
) -> Damaged {
    damage_at(target, request, None, rng)
}

pub use crate::means_of_death::MOD_SABER;

/// `MOD_DEMP2` on a client (`g_combat.c:4439-4455`): the electrocution effect, 300 to 800
/// ms of it by the generator, when none is running — the first thing `G_Damage` does, even
/// to a duellist the duel then shields (`crate::duel::damage_refused`).
pub fn shock(target: &mut Target<'_>, request: &DamageRequest, rng: &mut crate::player_death::Rng) {
    if request.means == MOD_DEMP2
        && (target.state.raw_field(PS_ELECTRIFY_TIME).unwrap_or(0) as i32) < request.level_time
    {
        target.state.set_raw_field(
            PS_ELECTRIFY_TIME,
            (request.level_time + rng.irand(300, 800)) as u32,
        );
    }
}

/// [`damage`], with the hit's location decided by the caller when it can be: a saber hit
/// whose Ghoul2 collision named the surface struck this frame is placed by that surface
/// (`G_GetHitLocFromSurfName`, `g_combat.c:4306-4316`), and `Some(HitLocation::None)`
/// then means a surface no location answers to — no scaling, and no geometric guess.
///
/// A saber's blow (`MOD_SABER`) also differs in two places. Idle touches of 1 are not
/// placed at all (`g_combat.c:4291`). Its push is scaled by `g_saberDmgVelocityScale`,
/// 0 by default, so it moves nobody — but it still names the attacker as the one who
/// last pushed, and sets no movement lock (`g_combat.c:4659-4740`).
pub fn damage_at(
    target: &mut Target<'_>,
    request: DamageRequest,
    surface_location: Option<HitLocation>,
    rng: &mut crate::player_death::Rng,
) -> Damaged {
    let saber = request.means == MOD_SABER;
    let mut damaged = Damaged::default();
    let mut damage = request.damage;
    let mut flags = request.flags;
    let level_time = request.level_time;
    shock(target, &request, rng);
    let attacker_number = request
        .attacker
        .map_or(ENTITY_WORLD, |attacker| attacker.client);
    let by_another = request
        .attacker
        .is_some_and(|attacker| attacker.client != target.client);
    // `attacker->client`: an entity that is no client — a hurt brush, a mover — has none
    // of a player's bookkeeping, only its number on the one it hurt.
    let by_a_player = request
        .attacker
        .is_some_and(|attacker| attacker.client < MAX_CLIENTS);
    // `attacker->client` of a player or an NPC: a turret or a brush that hurts a player
    // places no blow by location, credits no push and is not held off by the spawn's
    // protection (`g_combat.c:4604-4611`, `4705`, `4854-4865`).
    let by_a_client = by_a_player || request.attacker.is_some_and(|attacker| attacker.npc);
    let rage = flags & DAMAGE_NO_PROTECTION == 0
        && target.state.raw_field(PS_FORCE_ACTIVE).unwrap_or(0) & (1 << FP_RAGE) != 0;
    // Rage halves every blow first (`g_combat.c:4563-4569`).
    if rage {
        damage = (f64::from(damage) * 0.5) as i32;
    }
    // The attacker's handicap, unless it hurts itself.
    if let Some(attacker) = request.attacker
        && attacker.client != target.client
        && by_a_player
    {
        damage = damage * attacker.max_health / 100;
    }
    // The hit's location, for a player hit by a player.
    if flags & DAMAGE_NO_HIT_LOC == 0
        && by_a_client
        && !(saber && damage <= 1)
        && let Some(point) = request.point
    {
        let location = surface_location.unwrap_or_else(|| hit_location(0.0, target.bounds, point));
        damage = location_modified(damage, location);
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
    if target.wounds.no_knockback || flags & DAMAGE_NO_KNOCKBACK != 0 {
        knockback = 0;
    }
    // The momentum, even where the damage will not be taken.
    if knockback != 0 {
        // A saber's is scaled by `g_saberDmgVelocityScale`, 0: no push at all — unless the
        // blow asks for its saber's `knockbackScale` (`g_combat.c:4659-4700`; the second
        // blade style's scales count only beside a first one's flag).
        let pushed_by_saber =
            saber && flags & (DAMAGE_SABER_KNOCKBACK1 | DAMAGE_SABER_KNOCKBACK2) != 0;
        let scale = if saber {
            let mut saber_scale = 0.0;
            if pushed_by_saber {
                saber_scale = 1.0;
                let scales = request
                    .attacker
                    .map_or([0.0; 4], |attacker| attacker.saber_knockback);
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
        let velocity = target.state.velocity();
        target.state.set_velocity(std::array::from_fn(|axis| {
            velocity[axis] + direction[axis] * scale
        }));
        if by_another && by_a_client {
            target.wounds.other_killer = OtherKiller::credit(attacker_number, level_time);
        }
        // The other client cannot cancel the push out at once — unless it was a saber's,
        // which pushes nobody.
        if target.state.movement_time() == 0 && (!saber || pushed_by_saber) {
            let lock = (knockback * 2).clamp(50, 200);
            target.state.set_movement_time(lock as i16);
            target
                .state
                .set_movement_flags(target.state.movement_flags() | PMF_TIME_KNOCKBACK);
        }
    }
    // `g_friendlyFire` 0: a teammate's blow goes no further — the push has landed
    // (`OnSameTeam`: the same team in a team game; everyone is `TEAM_FREE` outside one).
    // A thing that is no client and carries a team — an emplaced gun its gunner's
    // (`g_combat.c:4806-4817`) — spares that team the same way.
    let teammate = request.attacker.is_some_and(|attacker| {
        (by_a_player || !by_a_client) && attacker.team != 0 && attacker.team == target.team
    });
    if flags & DAMAGE_NO_PROTECTION == 0 && by_another && (teammate || target.spared_by_master) {
        return damaged;
    }
    // Complete protection: the spawn's, while it lasts.
    if flags & DAMAGE_NO_PROTECTION == 0
        && by_another
        && by_a_client
        && target.state.raw_field(PS_EFLAGS).unwrap_or(0) & EF_INVULNERABLE != 0
    {
        if *target.invulnerable_until <= level_time {
            let eflags = target.state.raw_field(PS_EFLAGS).unwrap_or(0);
            target
                .state
                .set_raw_field(PS_EFLAGS, eflags & !EF_INVULNERABLE);
        } else {
            return damaged;
        }
    }
    // The attacker's hit counter, for a living target: a player's or an NPC's
    // (`attacker->client`, `g_combat.c:4895-4905`), which is never on a player's team.
    if let Some(attacker) = request.attacker
        && attacker.client != target.client
        && *target.health > 0
        && (by_a_player || attacker.npc)
    {
        damaged.attacker_hits = if by_a_player && attacker.team == target.team && attacker.team != 0
        {
            -1
        } else {
            1
        };
        damaged.attackee_armor =
            Some(((*target.health as u32) << 8) | target.state.stats[STAT_ARMOR]);
    }
    // Half damage hurting oneself.
    if request
        .attacker
        .is_some_and(|attacker| attacker.client == target.client)
    {
        damage = (f64::from(damage) * 0.5) as i32;
    }
    let damage = damage.max(1);
    let mut take = damage;
    // `CheckArmor`: the armour takes the whole of it — or half, for a weapon that says so
    // — up to what it has; none of what says the armour does not protect.
    let armor = target.state.stats[STAT_ARMOR] as i32;
    let mut save = if flags & DAMAGE_HALF_ABSORB != 0 {
        (f64::from(damage) * ARMOR_PROTECTION).ceil() as i32
    } else {
        damage
    };
    if save >= armor {
        save = armor;
    }
    if flags & DAMAGE_NO_ARMOR != 0 {
        save = 0;
    }
    if save > 0 {
        target.state.stats[STAT_ARMOR] = (armor - save) as u32;
        damaged.absorbed = save;
    }
    take -= save;
    // `MOD_DEMP2` on a player (`g_combat.c:5150-5195`): a third of what is left, one at
    // least (droids take more, fighters none; a jetpack goes off — none of them here yet).
    if matches!(request.means, MOD_DEMP2 | MOD_DEMP2_ALT) && take > 0 {
        take = (take / 3).max(1);
    }
    // The frame's totals, for the feedback at its end.
    target.state.persistent[PERS_ATTACKER] = u32::from(attacker_number);
    damaged.landed = true;
    target.wounds.armor += save;
    target.wounds.blood += take;
    target.wounds.knockback += knockback;
    if request.direction.is_some() {
        target.wounds.from = direction;
        target.wounds.from_world = false;
    } else {
        target.wounds.from = target.origin;
        target.wounds.from_world = true;
    }
    target.wounds.last_hurt = Some((attacker_number, request.means));
    if flags & DAMAGE_NO_PROTECTION == 0 && take > 0 {
        take = protected(target, take, level_time, &mut damaged.events);
    }
    if damaged.absorbed > 0 {
        // The shield shell on the player, facing the hit, for as much as it absorbed.
        let mut flash = EventEntity {
            event: EV_SHIELD_HIT,
            parameter: u32::from(legacy_direction_to_byte(direction)),
            origin: target.origin,
            client: None,
            broadcast: false,
            extra: [
                (59, u32::from(target.client)),
                (61, damaged.absorbed as u32),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
                (0, 0),
            ],
        };
        flash.client = None;
        damaged.events.push(flash);
    }
    if take > 0 {
        // `g_combat.c:5363-5378`: a DEMP2 hit taken shocks a player whose weapon is idle
        // — two seconds of weapon time and of electrification, a charge let go.
        if matches!(request.means, MOD_DEMP2 | MOD_DEMP2_ALT)
            && target.state.raw_field(PS_WEAPON_TIME).unwrap_or(0) as i32 <= 0
        {
            target.state.set_raw_field(PS_WEAPON_TIME, 2_000);
            target
                .state
                .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 2_000) as u32);
            if matches!(target.state.raw_field(PS_WEAPON_STATE), Some(4 | 5)) {
                target.state.set_raw_field(PS_WEAPON_STATE, 0);
            }
        }
        // Rage divides what is left by its level and one, and while it lasts a player's
        // blow leaves at least one health (`g_combat.c:5381-5419`).
        // `inflictor->client || attacker->client`: a player's blow or an NPC's.
        let raging = rage
            && request
                .attacker
                .is_some_and(|attacker| by_a_player || attacker.npc);
        if raging {
            let level = target
                .force
                .as_ref()
                .map_or(0, |force| i32::from(force.levels[FP_RAGE]));
            take /= level + 1;
        }
        *target.health -= take;
        // The stat is written from the health as it stands (`g_combat.c:5398-5400`);
        // the floor below is the entity's alone, so a hit that takes thousands leaves
        // the stat at what it really took.
        target.state.stats[STAT_HEALTH] = *target.health as u32;
        if raging {
            *target.health = (*target.health).max(1);
            target.state.stats[STAT_HEALTH] =
                (target.state.stats[STAT_HEALTH] as i32).max(1) as u32;
        }
        if *target.health <= 0 {
            target.wounds.no_knockback = true;
            target.wounds.death_point = request.point.unwrap_or(target.state.origin());
            *target.health = (*target.health).max(-999);
            damaged.died = Some(Death {
                take,
                point: target.wounds.death_point,
                means: request.means,
                attacker: request.attacker.map(|attacker| attacker.client),
            });
        }
    }
    damaged.take = take;
    damaged
}

/// Force protection (`g_combat.c:5245-5310`): while the pool lasts, part of the blow is
/// paid from it — by level, a point of Force for a point of damage and 40% of the health
/// kept, half a point and 60%, or a quarter and 80%, on up to 100, 200 or 400 of it (the
/// boon halves the Force's share) — with the hit's sound at most every 400 ms. Returns
/// what is left to take.
fn protected(
    target: &mut Target<'_>,
    take: i32,
    level_time: i32,
    events: &mut Vec<EventEntity>,
) -> i32 {
    use crate::force_powers::FP_PROTECT;
    if target.state.raw_field(PS_FORCE_ACTIVE).unwrap_or(0) & (1 << FP_PROTECT) == 0 {
        return take;
    }
    let pool = target.state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32;
    let Some(force) = target.force.as_mut() else {
        return take;
    };
    if pool == 0 {
        return take;
    }
    if force.sound_debounce < level_time {
        events.push(crate::knockdown::predef_sound(
            target.state.origin(),
            PDSOUND_PROTECTHIT,
        ));
        force.sound_debounce = level_time + 400;
    }
    let (pool, take) = protect_share(
        pool,
        force.levels[FP_PROTECT],
        target.state.powerups[PW_FORCE_BOON] != 0,
        take,
    );
    target.state.set_raw_field(PS_FORCE_POWER, pool as u32);
    take
}

/// Force protection's arithmetic (`g_combat.c:5256-5308`) for a blow of `take` on a pool of
/// `pool` (not 0) at protect level `level`, `boon` halving the Force's share: the pool left
/// and what is left to take.
pub(crate) fn protect_share(pool: i32, level: u8, boon: bool, take: i32) -> (i32, i32) {
    let (force_share, health_share, most): (f32, f32, i32) = match level {
        1 => (1.0, 0.40, 100),
        2 => (0.5, 0.60, 200),
        3 => (0.25, 0.80, 400),
        _ => (0.0, 0.0, take),
    };
    let most = take.min(most);
    let spent = if boon {
        most as f32 * force_share / 2.0
    } else {
        most as f32 * force_share
    };
    // `forcePower -= maxtake*famt`: an int less a float, truncated.
    let pool = (pool as f32 - spent) as i32;
    let mut saved = (most as f32 * health_share) as i32 + (take - most);
    let pool = if pool < 0 {
        saved += pool;
        0
    } else {
        pool
    };
    (
        pool,
        if saved != 0 {
            (take - saved).max(0)
        } else {
            take
        },
    )
}

/// `fd.forcePower`, `fd.forcePowersActive`; `FP_RAGE`; `PW_FORCE_BOON`;
/// `PDSOUND_PROTECTHIT`.
const PS_FORCE_POWER: usize = 18;
const PS_FORCE_ACTIVE: usize = 82;
const FP_RAGE: usize = crate::force_powers::FP_RAGE;
const PW_FORCE_BOON: usize = 14;
const PDSOUND_PROTECTHIT: u32 = 1;

/// `MOD_FALLING`.
pub const MOD_FALLING: u32 = 38;
/// `MOD_DEMP2`, `MOD_DEMP2_ALT`.
const MOD_DEMP2: u32 = 15;
const MOD_DEMP2_ALT: u32 = 16;
/// `weaponTime`, `weaponstate`, `electrifyTime`.
const PS_WEAPON_TIME: usize = 10;
const PS_WEAPON_STATE: usize = 33;
const PS_ELECTRIFY_TIME: usize = 73;
/// `DF_NO_FALLING`: the `dmflags` bit that turns falling damage off.
pub const DF_NO_FALLING: u32 = 8;

/// `ClientEvents`' `EV_FALL`/`EV_ROLL` (`g_active.c:949-1010`): what a landing of `delta`
/// costs — nothing up to 44, else `delta * 0.16` truncated; a player landing while
/// knocked down (`BG_InKnockDownOnly` on its legs, `knocked_down`) suffers the whole
/// delta above 14. The caller then damages the player from the world with
/// `DAMAGE_NO_ARMOR` and `MOD_FALLING`, after setting its `pain_debounce_time` 200 ms on
/// (no ordinary pain sound), and plays the splat where the fall killed.
pub fn fall_damage(delta: i32, dmflags: u32, knocked_down: bool) -> Option<i32> {
    if dmflags & DF_NO_FALLING != 0 {
        return None;
    }
    if knocked_down {
        return (delta > 14).then_some(delta);
    }
    if delta <= 44 {
        return None;
    }
    Some((f64::from(delta) * 0.16) as i32)
}

/// `P_DamageFeedback` (`g_active.c:53-135`), at the end of every frame for a living
/// player: the damage indicator's direction and amount, and the pain sound
/// (`EV_PAIN` as the player's external event, `G_AddEvent`) at most every 700 ms, for
/// ten points or more — none for a player whose entity is dead (`s.eFlags & EF_DEAD`,
/// which the conversion sets from the health: a disintegrated player floats in
/// `PM_NOCLIP`, dead all the same). `health` is the entity's. Returns whether the pain
/// event was raised: the player's entity must then be told
/// (`PlayerEntity::event_raised`).
pub fn damage_feedback(
    state: &mut PlayerState,
    wounds: &mut Wounds,
    health: i32,
    level_time: i32,
    pain_time: &mut i32,
    pain_direction: &mut bool,
) -> bool {
    if state.movement_type() == PM_DEAD {
        return false;
    }
    let count = (wounds.blood + wounds.armor).min(255);
    if count == 0 {
        return false;
    }
    let mut pain = false;
    if wounds.from_world {
        state.set_raw_field(PS_DAMAGE_PITCH, 255);
        state.set_raw_field(PS_DAMAGE_YAW, 255);
        wounds.from_world = false;
    } else {
        let (pitch, yaw) = vector_to_angles(wounds.from);
        // `angles[PITCH]/360.0 * 256` in double, truncated, then capped below at zero.
        let pitch = (f64::from(pitch) / 360.0 * 256.0) as i32;
        let yaw = (f64::from(yaw) / 360.0 * 256.0) as i32;
        state.set_raw_field(PS_DAMAGE_PITCH, pitch.max(0) as u32);
        state.set_raw_field(PS_DAMAGE_YAW, yaw.max(0) as u32);
    }
    let dead = health <= 0;
    if level_time > wounds.pain_debounce_time && !dead {
        // Not more than two pain sounds a second, none for a nick.
        if level_time - *pain_time < 500 || count < 10 {
            return false;
        }
        pain = true;
        // `P_SetTwitchInfo`.
        *pain_time = level_time;
        *pain_direction = !*pain_direction;
        wounds.pain_debounce_time = level_time + 700;
        // `G_AddEvent` on a player: its external event, with the stepped bits.
        let bits = (state.raw_field(PS_EXTERNAL_EVENT).unwrap_or(0) & EVENT_BITS)
            .wrapping_add(EVENT_BIT1)
            & EVENT_BITS;
        state.set_raw_field(PS_EXTERNAL_EVENT, EV_PAIN | bits);
        state.set_raw_field(PS_EXTERNAL_EVENT_PARM, health as u32);
        state.set_raw_field(
            PS_DAMAGE_EVENT,
            state
                .raw_field(PS_DAMAGE_EVENT)
                .unwrap_or(0)
                .wrapping_add(1)
                & 0xff,
        );
        let kind = if wounds.armor != 0 && wounds.blood == 0 {
            1
        } else if wounds.armor != 0 {
            2
        } else {
            0
        };
        state.set_raw_field(PS_DAMAGE_TYPE, kind);
    }
    state.set_raw_field(PS_DAMAGE_COUNT, count as u32);
    (wounds.blood, wounds.armor, wounds.knockback) = (0, 0, 0);
    pain
}

/// `vectoangles` (`q_math.c:616-653`): pitch and yaw, in degrees, of a direction.
pub(crate) fn vector_to_angles(value: [f32; 3]) -> (f32, f32) {
    let (pitch, yaw);
    if value[1] == 0.0 && value[0] == 0.0 {
        yaw = 0.0;
        pitch = if value[2] > 0.0 { 90.0 } else { 270.0 };
    } else {
        // `atan2f(...) * 180 / M_PI`: a float times an int, divided by a double.
        let degrees = |radians: f32| ((f64::from(radians * 180.0)) / std::f64::consts::PI) as f32;
        let mut y = if value[0] != 0.0 {
            degrees(value[1].atan2(value[0]))
        } else if value[1] > 0.0 {
            90.0
        } else {
            270.0
        };
        if y < 0.0 {
            y += 360.0;
        }
        yaw = y;
        let forward = (value[0] * value[0] + value[1] * value[1]).sqrt();
        let mut p = degrees(value[2].atan2(forward));
        if p < 0.0 {
            p += 360.0;
        }
        pitch = p;
    }
    (-pitch, yaw)
}

/// `DAMAGE_RADIUS`: damage was indirect, from a nearby explosion.
pub const DAMAGE_RADIUS: u32 = 0x1;
/// `MASK_SOLID`, which `CanDamage` traces with: the world alone.
const MASK_SOLID: u32 = 0x1;

/// Something an explosion may reach, as `G_RadiusDamage` sees it.
#[derive(Clone, Copy, Debug)]
pub struct SplashTarget {
    pub number: u16,
    /// `r.absmin`, `r.absmax`: the linked box grown by a unit.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `takedamage`.
    pub takes_damage: bool,
}

/// `CanDamage` (`g_combat.c:5563-5611`): a clear line through the world from `origin` to
/// the target's middle, or to any of four points fifteen units off it.
pub fn can_damage(
    target: &SplashTarget,
    origin: [f32; 3],
    world: &dyn crate::pmove::MovementCollision,
) -> bool {
    let midpoint: [f32; 3] =
        std::array::from_fn(|axis| (target.bounds.0[axis] + target.bounds.1[axis]) * 0.5);
    let trace = world.trace(origin, [0.0; 3], [0.0; 3], midpoint, MASK_SOLID);
    if trace.fraction == 1.0 || trace.entity_number == target.number {
        return true;
    }
    [(15.0, 15.0), (15.0, -15.0), (-15.0, 15.0), (-15.0, -15.0)]
        .into_iter()
        .any(|(x, y)| {
            let destination = [midpoint[0] + x, midpoint[1] + y, midpoint[2]];
            world
                .trace(origin, [0.0; 3], [0.0; 3], destination, MASK_SOLID)
                .fraction
                == 1.0
        })
}

/// `G_RadiusDamage` (`g_combat.c:5613-5730`): everything within `radius` of `origin`
/// (`EntitiesInBox`, then the distance from the box) but `ignore` takes `damage` scaled by
/// the distance, truncated, as `DAMAGE_RADIUS` from `attacker` with the direction from
/// the origin to it lifted 24 units — where `CanDamage` finds a line. `hurt` deals it
/// and says whether the hit counts (`LogAccuracyHit`); returns whether any did.
pub fn radius_damage(
    origin: [f32; 3],
    attacker: Option<Attacker>,
    damage: f32,
    radius: f32,
    ignore: Option<u16>,
    means: u32,
    level_time: i32,
    targets: &[SplashTarget],
    world: &dyn crate::pmove::MovementCollision,
    hurt: &mut dyn FnMut(u16, DamageRequest) -> bool,
) -> bool {
    let radius = radius.max(1.0);
    let mut hit_client = false;
    for target in targets {
        if Some(target.number) == ignore || !target.takes_damage {
            continue;
        }
        let inside = (0..3).all(|axis| {
            target.bounds.0[axis] <= origin[axis] + radius
                && target.bounds.1[axis] >= origin[axis] - radius
        });
        if !inside {
            continue;
        }
        let v: [f32; 3] = std::array::from_fn(|axis| {
            if origin[axis] < target.bounds.0[axis] {
                target.bounds.0[axis] - origin[axis]
            } else if origin[axis] > target.bounds.1[axis] {
                origin[axis] - target.bounds.1[axis]
            } else {
                0.0
            }
        });
        let distance = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if distance >= radius {
            continue;
        }
        let points = damage * (1.0 - distance / radius);
        if !can_damage(target, origin, world) {
            continue;
        }
        let mut direction: [f32; 3] =
            std::array::from_fn(|axis| target.origin[axis] - origin[axis]);
        direction[2] += 24.0;
        let request = DamageRequest {
            level_time,
            attacker,
            direction: Some(direction),
            point: Some(origin),
            damage: points as i32,
            flags: DAMAGE_RADIUS,
            means,
        };
        hit_client |= hurt(target.number, request);
    }
    hit_client
}

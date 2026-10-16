//! A lit blade cutting what it sweeps through: `WP_SaberPositionUpdate`'s damage half
//! (`codemp/game/w_saber.c:8786-9075`) with the defaults a retail server runs —
//! `d_saberSPStyleDamage 1`, `d_saberInterpolate 0`, `d_saberGhoul2Collision 1`.
//!
//! Each frame, each blade is swept from where it was last frame to where it is now
//! ([`SaberHits::sweep`], `G_SPSaberDamageTraceLerped`, `w_saber.c:5252`): a trace
//! along the base, then traces every 8 units up the blade, the swing cut into chunks of
//! at most 33° so an arc is not flattened. Each trace ([`check`], `CheckSaberDamage`,
//! `w_saber.c:3835`) is the engine's box trace, then — for a player it meets — Ghoul2
//! collision against the posed mesh (`G_G2TraceCollide`, `w_saber.c:2315`), which
//! decides whether the blade touched the body at all. What it did is gathered per victim
//! (`WP_SaberDamageAdd`); every blade's hits raise their effect ([`SaberHits::hit_effects`],
//! `WP_SaberDoHit`), and once all blades are swept the totals are dealt
//! ([`SaberHits::blows`], `WP_SaberApplyDamage`).
//!
//! Not here yet, each with its reference: blades meeting blades (the `CONTENTS_LIGHTSABER`
//! clash branch of `CheckSaberDamage`, `WP_SabersIntersect`, blocks, parries, locks), a
//! thrown saber's damage, saber definitions' flags (`SFL_BOUNCE_ON_WALLS`, transition
//! damage, knockback scales, per-saber damage scales, `SFL2_NO_IDLE_EFFECT`), duels, and
//! Ghoul2 collision against entities that are not players.

use crate::damage::{DAMAGE_NO_HIT_LOC, HitLocation};
use crate::event_entity::EventEntity;
use crate::pmove::{ENTITY_NUMBER_WORLD, MovementTrace};
use crate::saber_clash::{Blow, Fighter, clash, sabers_intersect};
use crate::saber_rules;

/// `EV_SABER_HIT`, `EV_SABER_BLOCK`, `EV_SABER_CLASHFLARE`.
pub const EV_SABER_HIT: u32 = 30;
pub const EV_SABER_BLOCK: u32 = 31;
const EV_SABER_CLASHFLARE: u32 = 32;
/// `MASK_PLAYERSOLID | CONTENTS_LIGHTSABER | MASK_SHOT`: what a blade's traces meet.
pub const SABER_TRACE_MASK: u32 = 0x1 | 0x10 | 0x100 | 0x200 | 0x1000 | 0x4_0000;
/// `DAMAGE_NO_DISMEMBER`.
pub const DAMAGE_NO_DISMEMBER: u32 = 0x8000;
/// `MAX_SABER_VICTIMS`: at most one fewer than this are hit a frame.
const MAX_SABER_VICTIMS: usize = 16;
const SABER_EXTRAPOLATE_DIST: f32 = 16.0;
const MAX_SABER_SWING_INC: f32 = 0.33;
/// `SABER_NONATTACK_DAMAGE`: an idle touch.
const SABER_NONATTACK_DAMAGE: i32 = 1;
/// `g_saberDmgDelay_Idle`, `g_saberWallDamageScale`.
const IDLE_DELAY: i32 = 350;
const WALL_DAMAGE_SCALE: f32 = 0.4;
/// `LS_READY`, and the three back attacks `G_SaberInBackAttack` names.
const LS_READY: u32 = 1;
/// `BOTH_A1_SPECIAL` .. `BOTH_A3_SPECIAL`: the single-style katas, one level stronger.
const KATAS: [u16; 3] = [911, 912, 913];
/// `GT_DUEL`, `GT_POWERDUEL`, `GT_SIEGE`: the slower games, where sabers are not doubled.
const SLOWER_GAMES: [i32; 3] = [3, 4, 7];
/// `g_saberDmgDelay_Wound`: how long after a clash no blow lands.
const ATTACK_WOUND_DELAY: i32 = 0;
/// `BOTH_SPINATTACK6`, `BOTH_SPINATTACK7`: "too easy to do, lower damage".
const SPIN_ATTACKS: [u16; 2] = [863, 864];
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_WEAPON: usize = 14;
const ES_LEGS_ANIM: usize = 16;
const ES_OTHER_ENTITY_NUM: usize = 59;
const ES_OTHER_ENTITY_NUM2: usize = 39;

/// The player swinging, as `CheckSaberDamage` reads it: its [`Fighter`] (whose move and
/// block a clash may change) and what the damage rules read besides. `idle_wound` and
/// `attack_wound` are written back.
#[derive(Clone, Copy, Debug, Default)]
pub struct Swinger {
    /// The swinger as a clash sees it; its blade is the one being swept.
    pub fighter: Fighter,
    /// `level.time`, `level.gametype`.
    pub level_time: i32,
    /// The game type: not a duel, power duel or siege, "sabers do more damage".
    pub gametype: i32,
    /// `ps.legsAnim`.
    pub legs: u16,
    /// `ps.weaponTime`: a swing is traced with a box, anything else with a point.
    pub weapon_time: i32,
    /// `ps.saberAttackWound`, `ps.saberIdleWound`: nothing is hurt before these.
    pub attack_wound: i32,
    /// Set when a touch lands.
    pub idle_wound: i32,
    /// `ps.m_iVehicleNum != 0`: riding, any saber move past the ready one cuts.
    pub riding: bool,
    /// `saber[n].type` of the saber being swept.
    pub saber_type: i32,
    /// The swinger's saber in the air, as the damage reads it.
    pub thrown: ThrownSwing,
    /// Which saber and blade are swept (`rSaberNum`, `rBladeNum`).
    pub saber: usize,
    pub blade: usize,
    /// `ps.isJediMaster`, a siege class with `CFL_MORESABERDMG`: twice the damage each.
    pub jedi_master: bool,
    /// The siege class flag.
    pub more_saber_damage: bool,
    /// Whom the swinger fights (`self->enemy`): a client struck is its enemy
    /// (`SEF_HITENEMY`) or not (`SEF_HITOBJECT`).
    pub enemy: Option<u16>,
}

/// A swing while the swinger's saber is in the air (`ps.saberInFlight`): the thrown first
/// saber cuts whatever its move, and any blade's damage is the throw's — 2.5 a throw
/// level going out, 1 coming back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThrownSwing {
    /// `ps.saberInFlight`.
    pub in_flight: bool,
    /// The saber swept is the first (`rSaberNum == 0`), the one thrown.
    pub first_saber: bool,
    /// The saber entity's `s.saberInFlight`: still going out, spinning.
    pub going_out: bool,
    /// `fd.forcePowerLevel[FP_SABERTHROW]`.
    pub level: u8,
}

/// `saberFlags` and `saberFlags2` bits the damage reads.
const SFL_BOUNCE_ON_WALLS: u32 = 1 << 8;
const SFL2_NO_DISMEMBERMENT: u32 = 1 << 4;
const SFL2_NO_IDLE_EFFECT: u32 = 1 << 5;
const SFL2_TRANSITION_DAMAGE: [u32; 2] = [1 << 8, 1 << 17];
/// `LS_A_JUMP_T__B_`: the one special that bounces off walls too.
const LS_A_JUMP_T_B: u32 = 16;
/// `ENTITYNUM_NONE`.
const ENTITY_NUMBER_NONE: u32 = 1_023;

/// What the swing may do to the entity a trace met.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaberVictim {
    /// A client (a player; an NPC once there are any).
    pub client: bool,
    /// `takedamage`.
    pub takes_damage: bool,
    /// Its health, and whether it was disintegrated (`EF_DISINTEGRATION`): a corpse that
    /// was not can be cut.
    pub health: i32,
    /// `EF_DISINTEGRATION`.
    pub disintegrated: bool,
    /// On the swinger's team with `g_friendlySaber` off: idle touches spare it.
    pub spared_by_idle: bool,
    /// In a duel with somebody else, or the swinger in a duel with somebody else.
    pub duel_elsewhere: bool,
    /// `BG_InKnockDownOnGround`: the only players a stab down cuts.
    pub knocked_down_on_ground: bool,
    /// `client->playerTeam`: an NPC's blow on a client of its own is a fifth
    /// (`w_saber.c:4662-4667`).
    pub player_team: i32,
}

/// Ghoul2 collision's answer for the entity a trace met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ghoul2Answer {
    /// It has no Ghoul2 instance: the box is all there is.
    NoModel,
    /// The mesh was missed: the trace passes on, having hit nothing.
    Miss,
    /// The mesh was struck here, facing this way.
    Hit {
        position: [f32; 3],
        normal: [f32; 3],
    },
}

/// The world a blade sweeps through.
pub trait SaberTargets: crate::saber_clash::ClashWorld {
    /// `trap->Trace`, passing through the swinger and what it owns.
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace;
    /// `G_G2TraceCollide`'s `G2API_CollisionDetect` on `number`, from `start` to `end`
    /// with a box of `radius`. A hit also stamps the struck player's last surface.
    fn collide(&mut self, number: u16, start: [f32; 3], end: [f32; 3], radius: f32)
    -> Ghoul2Answer;
    /// What entity `number` is, when it is in use and can be hurt.
    fn victim(&self, number: u16) -> Option<SaberVictim>;
    /// The owner of saber entity `number` as a clash reads it, when that is what `number`
    /// is.
    fn saber_owner(&self, number: u16) -> Option<Fighter>;
    /// A clash's changes to the other saber's owner: its move and its block.
    fn set_saber_owner(&mut self, owner: &Fighter);
    /// A blade bouncing off a wall (`SFL_BOUNCE_ON_WALLS`), after the swinger's move and
    /// block have changed: the swinger's animation set where it asks, the bounce sound,
    /// the hit event, and the splash.
    fn wall_bounce(&mut self, bounce: &WallBounce);
}

/// A blade bouncing off a wall (`w_saber.c:4453-4523`), for the game to carry out in
/// this order.
#[derive(Clone, Copy, Debug)]
pub struct WallBounce {
    /// `G_SetAnim(self, SETANIM_BOTH, anim, OVERRIDE|HOLD)`, where the swinger's torso and
    /// legs play the same animation.
    pub animation: Option<u16>,
    /// `WP_SaberBounceSound`: the saber's own sound, or `saberblock<n>.wav`.
    pub sound: BounceSound,
    /// The `EV_SABER_HIT` naming nobody where the blade struck.
    pub event: EventEntity,
    /// `WP_SaberRadiusDamage` from where it struck: radius, damage, knockback.
    pub point: [f32; 3],
    pub splash: (f32, i32, f32),
}

/// The sound a wall bounce makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BounceSound {
    /// A sound index the saber's definition registered.
    Registered(u16),
    /// `sound/weapons/saber/saberblock<n>.wav`, 1 to 9.
    Block(i32),
}

/// A blade's last damage-traced position (`blade.trail`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trail {
    /// Its base and tip.
    pub base: [f32; 3],
    /// The tip.
    pub tip: [f32; 3],
    /// When; a trail older than 100 ms is not swept from.
    pub last_time: i32,
}

/// One victim's gathered damage.
#[derive(Clone, Copy, Debug, Default)]
struct Victim {
    number: u16,
    total: i32,
    direction: [f32; 3],
    spot: [f32; 3],
    dismember: bool,
    /// `saberKnockbackFlags`: the `DAMAGE_SABER_KNOCKBACK*` bits the blades asked for.
    knockback: u32,
    effect_done: bool,
}

/// What one sweep of a blade decided, beyond the victims: `saberHitWall`,
/// `saberHitSaber`, `saberHitFraction`.
#[derive(Clone, Copy, Debug)]
struct SweepState {
    hit_wall: bool,
    hit_saber: bool,
    fraction: f32,
}

impl Default for SweepState {
    fn default() -> Self {
        Self {
            hit_wall: false,
            hit_saber: false,
            fraction: 1.0,
        }
    }
}

/// A frame's saber damage for one player (`WP_SaberClearDamage`'s arrays): no
/// allocation, at most fifteen victims.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaberHits {
    victims: [Victim; MAX_SABER_VICTIMS],
    count: usize,
    /// `saberDoClashEffect`, `saberClashPos`, `saberClashNorm`: where the frame's blades
    /// last met another saber. Kept for the whole frame, as the reference keeps it.
    clash: Option<([f32; 3], [f32; 3])>,
    /// A blade met another saber at the very start of a trace (`saberHitFraction` 0):
    /// the rest of the sweep collapses onto the old blade, and its rays, near zero
    /// length, are extrapolated along the rounding noise of the angle round trip.
    pub collapsed: bool,
}

/// A blow for `G_Damage`: `WP_SaberApplyDamage`'s call, `MOD_SABER`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaberBlow {
    /// Who is struck.
    pub victim: u16,
    /// The damage, the direction and the spot.
    pub damage: i32,
    /// The first trace's direction.
    pub direction: [f32; 3],
    /// Where the first trace struck.
    pub spot: [f32; 3],
    /// `DAMAGE_NO_DISMEMBER` unless a trace allowed it.
    pub flags: u32,
}

impl SaberHits {
    /// `WP_SaberClearDamage`.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `G_SPSaberDamageTraceLerped` for the swinger's blade (`swinger.fighter.blade`, read
    /// this frame), from `trail`. Returns the base and tip the trail keeps: the blade's,
    /// or, where it met another saber, where the sweep was cut short — the reference's
    /// sweep writes that back into the caller's blade points.
    pub fn sweep(
        &mut self,
        swinger: &mut Swinger,
        trail: &Trail,
        targets: &mut dyn SaberTargets,
    ) -> ([f32; 3], [f32; 3]) {
        let blade = swinger.fighter.blade;
        let length = blade.length;
        let mut base = blade.point;
        let mut tip = add_scaled(base, length, blade.direction);
        let (base_old, tip_old) = if swinger.level_time - trail.last_time > 100 {
            (base, tip)
        } else {
            (trail.base, trail.tip)
        };
        let (mp1, mp2) = (base_old, base);
        let mut md1 = sub(tip_old, base_old);
        normalize(&mut md1);
        let mut md2 = sub(tip, base);
        normalize(&mut md2);
        let mut state = SweepState::default();
        if vector_compare2(base_old, base) && vector_compare2(tip_old, tip) {
            check(self, swinger, &mut state, base, tip, false, targets);
            return (base, tip);
        }
        // The trace along the base first.
        check(self, swinger, &mut state, base_old, base, true, targets);
        if state.fraction < 1.0 {
            // "if hit a saber, shorten rest of traces to match"
            let (ma1, ma2) = (vector_angles(md1), vector_angles(md2));
            let angles: [f32; 3] =
                std::array::from_fn(|axis| lerp_angle(ma1[axis], ma2[axis], state.fraction));
            md2 = crate::pmove::flight::angles_to_axis(angles)[0];
            base = add_scaled(mp1, state.fraction, sub(mp2, mp1));
            tip = add_scaled(base, length, md2);
        }
        let mut extrapolate = true;
        // "If the angle diff in the blade is high, need to do it in chunks of 33 to avoid
        // flattening of the arc."
        let saber_move = swinger.fighter.saber_move;
        let torso = swinger.fighter.torso;
        let mut fraction = if saber_rules::in_attack(saber_move)
            || saber_rules::special_attack(torso)
            || saber_rules::spinning(torso)
            || saber_rules::special_jump(torso)
        {
            dot(md1, md2)
        } else {
            1.0
        };
        let increment;
        if f64::from(fraction).abs() < f64::from(1.0 - MAX_SABER_SWING_INC) {
            increment = 1.0 / ((1.0 - fraction) / MAX_SABER_SWING_INC);
            fraction = increment;
        } else {
            fraction = 1.0;
            increment = 0.0;
        }
        let (ma1, ma2) = (vector_angles(md1), vector_angles(md2));
        let (mut direction2, mut base2) = (md1, base_old);
        loop {
            let (direction1, base1) = (direction2, base2);
            if fraction >= 1.0 {
                (direction2, base2) = (md2, base);
            } else {
                let angles: [f32; 3] =
                    std::array::from_fn(|axis| lerp_angle(ma1[axis], ma2[axis], fraction));
                direction2 = crate::pmove::flight::angles_to_axis(angles)[0];
                base2 = add_scaled(base_old, fraction, sub(base, base_old));
            }
            // Up the blade, 8 units at a time.
            let mut step = 8.0_f32;
            while step <= length {
                let old = add_scaled(base1, step, direction1);
                let new = add_scaled(base2, step, direction2);
                // Never set back: every trace after the one that reaches the tip is not
                // extrapolated, even in the next chunk.
                if step + 8.0 >= length {
                    extrapolate = false;
                }
                check(self, swinger, &mut state, old, new, extrapolate, targets);
                if state.fraction < 1.0 {
                    base = add_scaled(mp1, state.fraction, sub(mp2, mp1));
                    tip = add_scaled(base, length, direction2);
                    let (ma1, ma2) = (vector_angles(direction1), vector_angles(direction2));
                    let angles: [f32; 3] = std::array::from_fn(|axis| {
                        lerp_angle(ma1[axis], ma2[axis], state.fraction)
                    });
                    direction2 = crate::pmove::flight::angles_to_axis(angles)[0];
                    state.hit_saber = true;
                }
                if state.hit_wall {
                    break;
                }
                step += 8.0;
            }
            if state.hit_wall || state.hit_saber || fraction >= 1.0 {
                break;
            }
            fraction = (fraction + increment).min(1.0);
        }
        (base, tip)
    }

    /// `WP_SaberDoClash` after a blade's sweep: while the frame's blades have met another
    /// saber, an `EV_SABER_BLOCK` where they met.
    pub fn clash_effect(&self, swinger: u16, saber: u32, blade: u32) -> Option<EventEntity> {
        let (position, normal) = self.clash?;
        let mut event = EventEntity {
            event: EV_SABER_BLOCK,
            parameter: 1,
            origin: position,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        let fields = [
            (ES_OTHER_ENTITY_NUM2, u32::from(swinger)),
            (ES_WEAPON, saber),
            (ES_LEGS_ANIM, blade),
            (ES_ORIGIN[0], position[0].to_bits()),
            (ES_ORIGIN[1], position[1].to_bits()),
            (ES_ORIGIN[2], position[2].to_bits()),
            (ES_ANGLES[0], normal[0].to_bits()),
            (ES_ANGLES[1], normal[1].to_bits()),
            (ES_ANGLES[2], normal[2].to_bits()),
        ];
        event.extra[..fields.len()].copy_from_slice(&fields);
        Some(event)
    }

    /// `WP_SaberDoHit` after a blade's sweep: an `EV_SABER_HIT` for each victim not yet
    /// shown, sized by what it has taken so far. `is_player` tells a client, an NPC or a
    /// body from anything else, which only flares — unless the blade raises no flare
    /// (`no_flare`, [`no_clash_flare`]).
    pub fn hit_effects(
        &mut self,
        swinger: u16,
        saber: u32,
        blade: u32,
        no_flare: bool,
        is_player: &dyn Fn(u16) -> bool,
        raise: &mut dyn FnMut(EventEntity),
    ) {
        for victim in &mut self.victims[..self.count] {
            if victim.effect_done {
                continue;
            }
            victim.effect_done = true;
            let mut angles = victim.direction.map(|value| -value);
            if angles == [0.0; 3] {
                angles[1] = 1.0;
            }
            // The hit's temp entity comes first, the flare (a droid's or a wall's) after it.
            let bleeds = is_player(victim.number);
            let parameter = if bleeds {
                match victim.total {
                    ..5 => 3,
                    5..20 => 2,
                    _ => 1,
                }
            } else {
                0
            };
            let mut event = EventEntity {
                event: EV_SABER_HIT,
                parameter,
                origin: victim.spot,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            let fields = [
                (ES_OTHER_ENTITY_NUM, u32::from(victim.number)),
                (ES_OTHER_ENTITY_NUM2, u32::from(swinger)),
                (ES_WEAPON, saber),
                (ES_LEGS_ANIM, blade),
                (ES_ORIGIN[0], victim.spot[0].to_bits()),
                (ES_ORIGIN[1], victim.spot[1].to_bits()),
                (ES_ORIGIN[2], victim.spot[2].to_bits()),
                (ES_ANGLES[0], angles[0].to_bits()),
                (ES_ANGLES[1], angles[1].to_bits()),
                (ES_ANGLES[2], angles[2].to_bits()),
            ];
            event.extra[..fields.len()].copy_from_slice(&fields);
            raise(event);
            if !bleeds && !no_flare && victim.total > SABER_NONATTACK_DAMAGE {
                raise(flare(victim.spot));
            }
        }
    }

    /// `WP_SaberApplyDamage`: every victim's total, in the order they were first hit. What
    /// is not a client takes `g_saberWallDamageScale` of it.
    pub fn blows<'a>(
        &'a self,
        is_client: &'a dyn Fn(u16) -> bool,
    ) -> impl Iterator<Item = SaberBlow> + 'a {
        self.victims[..self.count]
            .iter()
            .map(move |victim| SaberBlow {
                victim: victim.number,
                damage: if is_client(victim.number) {
                    victim.total
                } else {
                    (victim.total as f32 * WALL_DAMAGE_SCALE) as i32
                },
                direction: victim.direction,
                spot: victim.spot,
                flags: if victim.dismember {
                    victim.knockback
                } else {
                    DAMAGE_NO_DISMEMBER | victim.knockback
                },
            })
    }

    /// `WP_SaberDamageAdd`.
    fn add(
        &mut self,
        number: u16,
        direction: [f32; 3],
        spot: [f32; 3],
        damage: i32,
        dismember: bool,
        knockback: u32,
    ) {
        if number >= ENTITY_NUMBER_WORLD || damage == 0 {
            return;
        }
        let index = match self.victims[..self.count]
            .iter()
            .position(|victim| victim.number == number)
        {
            Some(index) => index,
            // "can't add another victim at this time"
            None if self.count + 1 >= MAX_SABER_VICTIMS => return,
            None => {
                self.victims[self.count] = Victim {
                    number,
                    ..Victim::default()
                };
                self.count += 1;
                self.count - 1
            }
        };
        let victim = &mut self.victims[index];
        victim.total += damage;
        if victim.direction == [0.0; 3] {
            victim.direction = direction;
        }
        if victim.spot == [0.0; 3] {
            victim.spot = spot;
        }
        victim.dismember |= dismember;
        victim.knockback |= knockback;
    }
}

/// `CheckSaberDamage`'s SP-style body path for one trace from `start` to `end`.
fn check(
    hits: &mut SaberHits,
    swinger: &mut Swinger,
    state: &mut SweepState,
    start: [f32; 3],
    end: [f32; 3],
    extrapolate: bool,
    targets: &mut dyn SaberTargets,
) {
    // A swing is traced with the SP damage box; anything else with a point.
    let half = if swinger.weapon_time <= 0 { 0.0 } else { 2.0 };
    let (mins, maxs) = ([-half; 3], [half; 3]);
    let traced_end = if extrapolate {
        let mut direction = sub(end, start);
        normalize(&mut direction);
        add_scaled(start, SABER_EXTRAPOLATE_DIST, direction)
    } else {
        end
    };
    let mut trace = targets.trace(start, mins, maxs, traced_end, SABER_TRACE_MASK);
    if trace.entity_number < ENTITY_NUMBER_WORLD {
        match targets.collide(trace.entity_number, start, traced_end, half) {
            Ghoul2Answer::NoModel => {}
            Ghoul2Answer::Miss => {
                trace.fraction = 1.0;
                trace.entity_number = sjk_protocol::ENTITY_NUMBER_NONE;
                trace.start_solid = false;
                trace.all_solid = false;
            }
            Ghoul2Answer::Hit { position, normal } => {
                trace.end_position = position;
                trace.plane_normal = normal;
            }
        }
    }
    let time = swinger.level_time;
    let fighter = swinger.fighter;
    let mut idle = false;
    let mut strength = 0;
    let mut damage;
    let thrown = swinger.thrown;
    // The swept saber's definition, and which blade style the swept blade is.
    let sabers = fighter.saber_set();
    let combat = sabers.sabers[swinger.saber.min(1)].combat;
    let style = combat.style(swinger.blade);
    let transition = combat.flags2 & SFL2_TRANSITION_DAMAGE[style] != 0
        && saber_rules::in_transition_any(fighter.saber_move);
    if swinger.attack_wound < time
        && (saber_rules::attacking(
            fighter.saber_move,
            fighter.weapon_state,
            fighter.saber_blocked,
        ) || saber_rules::super_break_win(fighter.torso)
            || (thrown.in_flight && thrown.first_saber)
            || transition
            || (swinger.riding && fighter.saber_move > LS_READY))
    {
        let mut full = if thrown.in_flight {
            // `w_saber.c:4068-4081`: "does less damage on the way back".
            strength = i32::from(thrown.going_out);
            if thrown.going_out {
                2.5 * f32::from(thrown.level)
            } else {
                1.0
            }
        } else {
            strength = saber_rules::power_level(
                fighter.torso,
                fighter.torso_timer,
                fighter.torso_length - fighter.torso_timer,
                swinger.saber_type,
                0,
                false,
            );
            if SPIN_ATTACKS.contains(&fighter.torso) {
                2.5
            } else {
                2.5 * strength as f32
            }
        };
        if !SLOWER_GAMES.contains(&swinger.gametype) {
            full *= 2.0;
        }
        damage = 0;
        if full != 0.0 {
            // "the longer the trace, the more damage it does" — measured on the blade's
            // own points, not the extrapolated end.
            let length = length(sub(end, start));
            let share = if trace.fraction >= 1.0 {
                full * length * 0.1 * 0.33
            } else {
                full * length * (1.0 - trace.fraction) * 0.1 * 0.33
            };
            damage = f64::from(share).ceil() as i32;
        }
        // "parry/block/break-parry bonus for single-style kata moves"
        if KATAS.contains(&fighter.torso) {
            strength += 1;
        }
    } else if swinger.attack_wound < time && swinger.idle_wound < time {
        // "no idle damage or effects": the first saber's flag, whichever blade.
        if sabers.sabers[0].combat.flags2 & SFL2_NO_IDLE_EFFECT != 0 {
            return;
        }
        damage = if saber_rules::in_return(fighter.saber_move) {
            SABER_NONATTACK_DAMAGE
        } else {
            0
        };
        idle = true;
    } else {
        return;
    }
    let unblockable = saber_rules::in_special(fighter.saber_move);
    if unblockable {
        // The SP style adds nothing for specials.
        swinger.fighter.saber_blocked = 0;
    }
    if damage == 0 {
        return;
    }
    if damage > SABER_NONATTACK_DAMAGE {
        // `g_saberDamageScale` 1; the saber's own scale for this blade's style.
        let scale = combat.damage_scale[style];
        if scale != 1.0 {
            damage = (damage as f32 * scale).ceil() as i32;
        }
        if fighter.broken_arm {
            damage = (f64::from(damage) * 0.3) as i32;
            damage = damage.max(SABER_NONATTACK_DAMAGE + 1);
        }
        if swinger.jedi_master {
            damage *= 2;
        }
        if swinger.more_saber_damage {
            damage *= 2;
        }
    }
    let mut direction = sub(end, start);
    normalize(&mut direction);
    if trace.entity_number == ENTITY_NUMBER_WORLD {
        // "register this as a wall hit for jedi AI"
        swinger.fighter.event_flags |= crate::saber_clash::sef::HIT_WALL;
        state.hit_wall = true;
    }
    // A saber that bounces off walls, in a plain attack or death from above: a broken
    // parry, the bounce, and the splash — and no damage.
    let saber_move = swinger.fighter.saber_move;
    if state.hit_wall
        && combat.flags & SFL_BOUNCE_ON_WALLS != 0
        && (saber_rules::in_attack_pure(saber_move) || saber_move == LS_A_JUMP_T_B)
    {
        swinger.fighter.saber_move = saber_rules::broken_parry_for_attack(saber_move);
        swinger.fighter.saber_blocked = crate::saber_clash::BLOCKED_PARRY_BROKEN;
        let animation = (fighter.torso == swinger.legs).then(|| {
            crate::saber_move_data::move_animation(swinger.fighter.saber_move as u16, true)
        });
        let index = targets.rng().irand(1, 9);
        let (bounce, block) = (combat.bounce_sounds[style], combat.block_sounds[style]);
        let sound = if bounce[0] != 0 {
            BounceSound::Registered(bounce[targets.rng().irand(0, 2) as usize])
        } else if block[0] != 0 {
            BounceSound::Registered(block[targets.rng().irand(0, 2) as usize])
        } else {
            BounceSound::Block(index)
        };
        let mut angles = trace.plane_normal;
        if angles == [0.0; 3] {
            angles[1] = 1.0;
        }
        let point = trace.end_position;
        let mut event = EventEntity {
            event: EV_SABER_HIT,
            parameter: 0,
            origin: point,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        let fields = [
            (ES_OTHER_ENTITY_NUM, ENTITY_NUMBER_NONE),
            (ES_OTHER_ENTITY_NUM2, u32::from(fighter.number)),
            (ES_WEAPON, swinger.saber as u32),
            (ES_LEGS_ANIM, swinger.blade as u32),
            (ES_ORIGIN[0], point[0].to_bits()),
            (ES_ORIGIN[1], point[1].to_bits()),
            (ES_ORIGIN[2], point[2].to_bits()),
            (ES_ANGLES[0], angles[0].to_bits()),
            (ES_ANGLES[1], angles[1].to_bits()),
            (ES_ANGLES[2], angles[2].to_bits()),
        ];
        event.extra[..fields.len()].copy_from_slice(&fields);
        let splash = (
            combat.splash_radius[style],
            combat.splash_damage[style],
            combat.splash_knockback[style],
        );
        targets.wall_bounce(&WallBounce {
            animation,
            sound,
            event,
            point,
            splash,
        });
        return;
    }
    let number = trace.entity_number;
    if !(trace.fraction != 1.0 || trace.start_solid) {
        return;
    }
    let victim = targets.victim(number).filter(|victim| {
        victim.takes_damage
            && (victim.health > 0 || !victim.disintegrated)
            && number != fighter.number
    });
    if let Some(victim) = victim {
        if (idle && victim.client && victim.spared_by_idle)
            || (victim.client && victim.duel_elsewhere)
        {
            return;
        }
        if saber_rules::stab_down(fighter.torso) && victim.client && !victim.knocked_down_on_ground
        {
            return;
        }
        swinger.idle_wound = time + IDLE_DELAY;
        // SP style: no fake block — a blow is stopped only by the saber it meets.
        if victim.client && damage > SABER_NONATTACK_DAMAGE {
            damage = (f64::from(damage) * 1.5) as i32;
        }
        // "Since he's an NPC, we'll be forgiving and cut the damage down."
        if fighter.npc && victim.client && fighter.player_team == victim.player_team {
            damage = (damage as f32 * 0.2) as i32;
        }
        // Dismemberment unless the saber forbids it (its first style's flag, for either);
        // its knockback scale, if any, asked for by the first style's number.
        let dismember = combat.flags2 & SFL2_NO_DISMEMBERMENT == 0;
        let knockback = if combat.knockback_scale[0] > 0.0 {
            match (swinger.saber < 1, style) {
                (true, 0) => crate::damage::DAMAGE_SABER_KNOCKBACK1,
                (false, 0) => crate::damage::DAMAGE_SABER_KNOCKBACK2,
                (true, _) => crate::damage::DAMAGE_SABER_KNOCKBACK1_B2,
                (false, _) => crate::damage::DAMAGE_SABER_KNOCKBACK2_B2,
            }
        } else {
            0
        };
        hits.add(
            number,
            direction,
            trace.end_position,
            damage,
            dismember,
            knockback,
        );
        // "Let jedi AI know if it hit an enemy"
        if victim.client {
            swinger.fighter.event_flags |= if swinger.enemy == Some(number) {
                crate::saber_clash::sef::HIT_ENEMY
            } else {
                crate::saber_clash::sef::HIT_OBJECT
            };
        }
    } else if let Some(mut other) = targets.saber_owner(number) {
        // The clash branch: blades that really crossed, and the other saber in hand (a
        // thrown one is not tested closer).
        if !other.in_flight
            && (swinger.fighter.sabers_off
                || other.sabers_off
                || !sabers_intersect(&swinger.fighter.blade, &other.saber_set(), false))
        {
            return;
        }
        let thrown = swinger.thrown.in_flight && swinger.thrown.first_saber;
        let blow = Blow {
            damage,
            strength,
            unblockable,
            point: trace.end_position,
            fraction: trace.fraction,
            level_time: time,
            gametype: swinger.gametype,
            thrown,
        };
        let Some(outcome) = clash(&mut swinger.fighter, &mut other, &blow, targets) else {
            return;
        };
        // The debug lock returns before any clash bookkeeping (`w_saber.c:4792`).
        if outcome.locked && !outcome.met {
            return;
        }
        swinger.idle_wound = time + IDLE_DELAY;
        hits.clash = Some((trace.end_position, trace.plane_normal));
        state.hit_saber = true;
        state.fraction = trace.fraction;
        hits.collapsed |= trace.fraction == 0.0;
        if outcome.wound {
            swinger.attack_wound = time + ATTACK_WOUND_DELAY;
        }
        targets.set_saber_owner(&other);
    }
}

/// `SFL2_NO_CLASH_FLARE`, `SFL2_NO_CLASH_FLARE2` (`saberFlags2`).
pub const SFL2_NO_CLASH_FLARE: u32 = 1 << 3;
const SFL2_NO_CLASH_FLARE2: u32 = 1 << 12;

/// Whether blade `blade` of `saber` raises no clash flare where it strikes what does not
/// bleed (`w_saber.c:3656-3663`): `SFL2_NO_CLASH_FLARE`, or `SFL2_NO_CLASH_FLARE2` for a
/// blade of its second style (`WP_SaberBladeUseSecondBladeStyle`).
pub fn no_clash_flare(saber: &crate::saber_definition::SaberDefinition, blade: usize) -> bool {
    let second = saber.blade_style2_start > 0 && blade as i32 >= saber.blade_style2_start;
    saber.flags2
        & if second {
            SFL2_NO_CLASH_FLARE2
        } else {
            SFL2_NO_CLASH_FLARE
        }
        != 0
}

/// `G_TempEntity(spot, EV_SABER_CLASHFLARE)` with the exact origin.
fn flare(spot: [f32; 3]) -> EventEntity {
    let mut event = EventEntity {
        event: EV_SABER_CLASHFLARE,
        parameter: 0,
        origin: spot,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    for axis in 0..3 {
        event.extra[axis] = (ES_ORIGIN[axis], spot[axis].to_bits());
    }
    event
}

/// `G_GetHitLocFromSurfName` (`g_combat.c:3689`) for a humanoid player, as
/// `G_LocationBasedDamageModifier` calls it (`MOD_UNKNOWN`, so the torso never answers
/// the head): the body part a surface belongs to, narrowed to the knee, hand or foot
/// when the point is near that bolt. `bolt` reads a bolt of the struck player's model in
/// the world (see [`crate::server_skeleton::ServerSkeleton::bolt_point`]): the knees,
/// hands and feet with the entity's own angles, which a player never sets, and
/// `thoracic` (the torso point) facing the view. A surface no rule names is
/// [`HitLocation::None`]: no scaling at all.
pub fn location_from_surface(
    surface: &str,
    point: [f32; 3],
    bolt: &mut dyn FnMut(&str, bool) -> Option<[f32; 3]>,
) -> HitLocation {
    let near = |bolt: Option<[f32; 3]>, reach: f32| {
        bolt.is_some_and(|at| distance_squared(point, at) < reach)
    };
    let starts = |prefix: &str| {
        surface.len() >= prefix.len() && surface.as_bytes()[..prefix.len()] == *prefix.as_bytes()
    };
    if starts("hips") {
        if near(bolt("*hips_l_knee", false), 100.0) {
            HitLocation::LegLeft
        } else if near(bolt("*hips_r_knee", false), 100.0) {
            HitLocation::LegRight
        } else {
            HitLocation::Waist
        }
    } else if starts("torso") {
        // `renderInfo.torsoAngles` is never set on a server: the torso's axes are the
        // world's, forward +x, right -y, up +z.
        let Some(torso) = bolt("thoracic", true) else {
            return HitLocation::Chest;
        };
        let impact = sub(point, torso);
        let (front, right, up) = (impact[0], -impact[1], impact[2]);
        if up < -10.0 {
            HitLocation::Waist
        } else if right > 4.0 {
            HitLocation::ArmRight
        } else if right < -4.0 {
            HitLocation::ArmLeft
        } else if right > 2.0 {
            if front > 0.0 {
                HitLocation::ChestRight
            } else {
                HitLocation::BackRight
            }
        } else if right < -2.0 {
            if front > 0.0 {
                HitLocation::ChestLeft
            } else {
                HitLocation::BackLeft
            }
        } else if front > 0.0 {
            HitLocation::Chest
        } else {
            HitLocation::Back
        }
    } else if starts("head") {
        HitLocation::Head
    } else if starts("r_arm") {
        if near(bolt("*r_hand", false), 256.0) {
            HitLocation::HandRight
        } else {
            HitLocation::ArmRight
        }
    } else if starts("l_arm") {
        if near(bolt("*l_hand", false), 256.0) {
            HitLocation::HandLeft
        } else {
            HitLocation::ArmLeft
        }
    } else if starts("r_leg") {
        if near(bolt("*r_leg_foot", false), 100.0) {
            HitLocation::FootRight
        } else {
            HitLocation::LegRight
        }
    } else if starts("l_leg") {
        if near(bolt("*l_leg_foot", false), 100.0) {
            HitLocation::FootLeft
        } else {
            HitLocation::LegLeft
        }
    } else if starts("r_hand") || starts("w_") {
        HitLocation::HandRight
    } else if starts("l_hand") {
        HitLocation::HandLeft
    } else {
        HitLocation::None
    }
}

/// Whether `G_Damage` places a saber blow by the surface Ghoul2 named: the victim's last
/// surface was struck this frame, and the blow is placed at all.
pub fn placed_by_surface(flags: u32, last_surface_time: i32, level_time: i32) -> bool {
    flags & DAMAGE_NO_HIT_LOC == 0 && last_surface_time == level_time
}

/// `VectorCompare2` (`q_math.c:1228`), with its typo: the second and third components'
/// lower bounds add the tolerance instead of subtracting it, so two equal vectors
/// compare unequal and a blade that did not move is swept all the same.
fn vector_compare2(left: [f32; 3], right: [f32; 3]) -> bool {
    const EPSILON: f32 = 0.0001;
    !(left[0] > right[0] + EPSILON
        || left[0] < right[0] - EPSILON
        || left[1] > right[1] + EPSILON
        || left[1] < right[1] + EPSILON
        || left[2] > right[2] + EPSILON
        || left[2] < right[2] + EPSILON)
}

/// `LerpAngle`.
fn lerp_angle(from: f32, mut to: f32, fraction: f32) -> f32 {
    if to - from > 180.0 {
        to -= 360.0;
    }
    if to - from < -180.0 {
        to += 360.0;
    }
    from + fraction * (to - from)
}

fn vector_angles(value: [f32; 3]) -> [f32; 3] {
    crate::player_angle_math::vector_angles(value)
}

fn normalize(vector: &mut [f32; 3]) {
    crate::player_angle_math::normalize(vector);
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn add_scaled(base: [f32; 3], scale: f32, direction: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| base[axis] + scale * direction[axis])
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn length(vector: [f32; 3]) -> f32 {
    dot(vector, vector).sqrt()
}

fn distance_squared(left: [f32; 3], right: [f32; 3]) -> f32 {
    let difference = sub(left, right);
    dot(difference, difference)
}

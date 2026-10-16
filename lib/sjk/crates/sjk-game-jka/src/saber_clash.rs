//! Blades meeting blades: `CheckSaberDamage`'s `CONTENTS_LIGHTSABER` branch
//! (`codemp/game/w_saber.c:4744-5247`) and what it calls.
//!
//! Every player with a lit saber has a saber entity: a box around its blade
//! (`SetSaberBoxSize`, [`saber_box`]) that other blades' damage traces can meet. A trace
//! that does is checked against the blades themselves (`WP_SabersIntersect`,
//! [`sabers_intersect`]). If they meet, the two sabers clash. The frame's clash effect is
//! raised, the rest of the sweep is cut short, and the two players' saber moves are
//! decided ([`clash`]): a lock, a deflection, a knockaway, a broken parry, a bounce, or a
//! parry.
//!
//! The rules draw from the game's generator (`Q_irand`) in the reference's order, and
//! try a saber lock through the world they run in ([`ClashWorld::check_lock`]).
//! A saber knocked out of the hand or out of the air is the world's to knock
//! ([`ClashWorld::knock_out`], [`ClashWorld::smash`], [`crate::saber_drop`]).

use crate::player_death::Rng;
use crate::saber_rules::{
    self, attacking, in_bounce, in_broken_parry, in_deflect, in_parry, in_special,
};

/// `saberBlockedType_t`.
pub const BLOCKED_NONE: u32 = 0;
pub const BLOCKED_BOUNCE_MOVE: u32 = 1;
pub const BLOCKED_PARRY_BROKEN: u32 = 2;
pub const BLOCKED_ATK_BOUNCE: u32 = 3;
const BLOCKED_UPPER_RIGHT: u32 = 4;
const BLOCKED_UPPER_LEFT: u32 = 5;
const BLOCKED_LOWER_RIGHT: u32 = 6;
const BLOCKED_LOWER_LEFT: u32 = 7;
const BLOCKED_TOP: u32 = 8;
const BLOCKED_UPPER_RIGHT_PROJ: u32 = 9;
const BLOCKED_UPPER_LEFT_PROJ: u32 = 10;
const BLOCKED_LOWER_RIGHT_PROJ: u32 = 11;
const BLOCKED_LOWER_LEFT_PROJ: u32 = 12;
const BLOCKED_TOP_PROJ: u32 = 13;
/// Saber moves this module names.
const LS_NONE: u32 = 0;
const LS_READY: u32 = 1;
const LS_PARRY_UP: u32 = 152;
const LS_PARRY_UR: u32 = 153;
const LS_PARRY_UL: u32 = 154;
const LS_PARRY_LR: u32 = 155;
const LS_PARRY_LL: u32 = 156;
const LS_REFLECT_UP: u32 = 157;
const LS_REFLECT_UR: u32 = 158;
const LS_REFLECT_UL: u32 = 159;
const LS_REFLECT_LR: u32 = 160;
const LS_REFLECT_LL: u32 = 161;
const LS_K1_T_: u32 = 147;
const LS_K1_TR: u32 = 148;
const LS_K1_TL: u32 = 149;
const LS_K1_BR: u32 = 150;
const LS_K1_BL: u32 = 151;
const LS_H1_T_: u32 = 141;
const LS_H1_B_: u32 = 145;
/// `BOTH_A1_SPECIAL`, `BOTH_A2_SPECIAL`, `BOTH_A3_SPECIAL`: the single-style katas,
/// which parry one level better.
const KATAS: [u16; 3] = [911, 912, 913];
/// `saberQuadrant_t`: `Q_BR` .. `Q_B`.
const Q_BR: i32 = 0;
const Q_R: i32 = 1;
const Q_TR: i32 = 2;
const Q_TL: i32 = 4;
const Q_L: i32 = 5;
const Q_BL: i32 = 6;
const Q_B: i32 = 7;
/// `SS_FAST`, `SS_MEDIUM`, `SS_STRONG`, `SS_DUAL`, `SS_STAFF`.
const SS_FAST: i32 = 1;
const SS_MEDIUM: i32 = 2;
const SS_STRONG: i32 = 3;
const SS_DUAL: i32 = 6;
const SS_STAFF: i32 = 7;
/// `GT_DUEL`, `GT_POWERDUEL`, `GT_SIEGE`.
pub const GT_DUEL: i32 = 3;
pub const GT_POWERDUEL: i32 = 4;
pub const GT_SIEGE: i32 = 7;
/// `SABER_BOX_SIZE`, the box of a saber whose blade has not been read lately.
const SABER_BOX_SIZE: f32 = 16.0;
const SABER_EXTRAPOLATE_DIST: f32 = 16.0;
/// `g_saberLockFactor`.
const LOCK_FACTOR: i32 = 2;
/// `SABER_NONATTACK_DAMAGE`.
const SABER_NONATTACK_DAMAGE: i32 = 1;

/// Where a player's blade was last read (`lastSaberBase_Always`, `lastSaberStorageTime`)
/// and the reading before it (`olderSaberBase`, `olderIsValid`): how fast it swings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SaberStorage {
    /// When the blade was last read; 0 before the first.
    pub last_time: i32,
    /// Whether the reading before it is under 200 ms older.
    pub older_valid: bool,
    /// The blade's base then.
    pub last_base: [f32; 3],
    /// And the reading before.
    pub older_base: [f32; 3],
}

impl SaberStorage {
    /// `WP_SaberPositionUpdate`'s store of a freshly read blade (`w_saber.c:8578-8591`).
    pub fn store(&mut self, base: [f32; 3], level_time: i32) {
        if self.last_time != 0 && level_time - self.last_time < 200 {
            self.older_base = self.last_base;
            self.older_valid = true;
        } else {
            self.older_valid = false;
        }
        self.last_base = base;
        self.last_time = level_time;
    }
}

/// A blade as the damage loop keeps it (`bladeInfo_t`): now and at the frame before
/// (`muzzlePoint`/`muzzleDir`, `muzzlePointOld`/`muzzleDirOld`), its length, and when it
/// was last read (`storageTime`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BladeHistory {
    /// Where the blade leaves the hilt, and which way it points.
    pub point: [f32; 3],
    /// Its direction.
    pub direction: [f32; 3],
    /// Both, the frame before.
    pub point_old: [f32; 3],
    /// The direction the frame before.
    pub direction_old: [f32; 3],
    /// `lengthMax`.
    pub length: f32,
    /// `storageTime`.
    pub storage_time: i32,
}

impl BladeHistory {
    /// The damage loop's update for a freshly read blade (`w_saber.c:8836-8893`).
    pub fn read(&mut self, point: [f32; 3], direction: [f32; 3], level_time: i32) {
        self.point_old = self.point;
        self.direction_old = self.direction;
        self.point = point;
        self.direction = direction;
        self.storage_time = level_time;
    }
}

/// `MAX_BLADES`.
pub const MAX_BLADES: usize = 8;
/// `SFL2_ALWAYS_BLOCK`, `SFL2_ALWAYS_BLOCK2`: blades that block even in a broken parry.
const SFL2_ALWAYS_BLOCK: u32 = 1 << 6;
const SFL2_ALWAYS_BLOCK2: u32 = 1 << 15;

/// One saber's blades as the blade rules read them (`saberInfo_t`'s blades and what
/// decides which of them count).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SaberBlades {
    /// Each blade now and the frame before, its length and when it was read.
    pub blades: [BladeHistory; MAX_BLADES],
    /// `numBlades`.
    pub count: usize,
    /// `model[0]`: a hilt in this hand.
    pub held: bool,
    /// `type != SABER_NONE`: a removed saber keeps a type, a hand never set has none.
    pub typed: bool,
    /// What the blade rules read of its definition.
    pub combat: SaberCombat,
}

/// What of a saber's definition the blade rules read: its flags, where its second blade
/// style starts, its type, and its combat numbers — pairs hold the first style's, then
/// the second's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaberCombat {
    /// `saberFlags`, `saberFlags2`.
    pub flags: u32,
    pub flags2: u32,
    /// `bladeStyle2Start`: blades from here on use the second style.
    pub style2_start: usize,
    /// `saberType_t`.
    pub saber_type: i32,
    /// `parryBonus`, `breakParryBonus`, `disarmBonus`, `lockBonus`.
    pub parry_bonus: i32,
    pub break_parry_bonus: i32,
    pub disarm_bonus: i32,
    pub lock_bonus: i32,
    /// `damageScale`, `knockbackScale` (each style's).
    pub damage_scale: [f32; 2],
    pub knockback_scale: [f32; 2],
    /// `splashRadius`, `splashDamage`, `splashKnockback` (each style's).
    pub splash_radius: [f32; 2],
    pub splash_damage: [i32; 2],
    pub splash_knockback: [f32; 2],
    /// `bounceSound`, `blockSound` (each style's three).
    pub bounce_sounds: [[u16; 3]; 2],
    pub block_sounds: [[u16; 3]; 2],
}

impl Default for SaberCombat {
    /// `WP_SaberSetDefaults`' numbers.
    fn default() -> Self {
        Self {
            flags: 0,
            flags2: 0,
            style2_start: 0,
            saber_type: 0,
            parry_bonus: 0,
            break_parry_bonus: 0,
            disarm_bonus: 0,
            lock_bonus: 0,
            damage_scale: [1.0; 2],
            knockback_scale: [0.0; 2],
            splash_radius: [0.0; 2],
            splash_damage: [0; 2],
            splash_knockback: [0.0; 2],
            bounce_sounds: [[0; 3]; 2],
            block_sounds: [[0; 3]; 2],
        }
    }
}

impl SaberCombat {
    /// A definition's.
    pub fn of(saber: &crate::saber_definition::SaberDefinition) -> Self {
        Self {
            flags: saber.flags,
            flags2: saber.flags2,
            style2_start: saber.blade_style2_start.max(0) as usize,
            saber_type: saber.saber_type,
            parry_bonus: saber.parry_bonus,
            break_parry_bonus: saber.break_parry_bonus[0],
            disarm_bonus: saber.disarm_bonus[0],
            lock_bonus: saber.lock_bonus,
            damage_scale: saber.damage_scale,
            knockback_scale: saber.knockback_scale,
            splash_radius: saber.splash_radius,
            splash_damage: saber.splash_damage,
            splash_knockback: saber.splash_knockback,
            bounce_sounds: saber.bounce_sounds,
            block_sounds: saber.block_sounds,
        }
    }

    /// `WP_SaberBladeUseSecondBladeStyle`: 1 for a blade of the second style, else 0.
    pub fn style(&self, blade: usize) -> usize {
        usize::from(self.style2_start > 0 && blade >= self.style2_start)
    }
}

/// A fighter's two sabers' blades (`client->saber`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SaberSet {
    pub sabers: [SaberBlades; 2],
}

impl SaberSet {
    /// One saber of one blade.
    pub fn single(blade: BladeHistory) -> Self {
        let mut set = Self::default();
        set.sabers[0].blades[0] = blade;
        set.sabers[0].count = 1;
        set.sabers[0].held = true;
        set.sabers[0].typed = true;
        set.sabers[0].combat.saber_type = saber_rules::SABER_SINGLE;
        set
    }

    /// Whether a second saber is held (`saber[1].model[0]`).
    pub fn pair(&self) -> bool {
        self.sabers[1].held
    }
}

/// One side of a clash, as the rules read and change it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fighter {
    /// Its entity number.
    pub number: u16,
    /// `ps.saberMove`, `ps.saberBlocked`: what the clash changes.
    pub saber_move: u32,
    /// The block the saber is raised to.
    pub saber_blocked: u32,
    /// `ps.torsoAnim`, `ps.torsoTimer`, and the animation's `BG_AnimLength`.
    pub torso: u16,
    /// What is left of it.
    pub torso_timer: i32,
    /// Its whole length.
    pub torso_length: i32,
    /// `ps.weaponstate`.
    pub weapon_state: u8,
    /// `ps.fd.saberAnimLevel`.
    pub style: i32,
    /// Either arm broken.
    pub broken_arm: bool,
    /// `FP_SABER_OFFENSE` and `FP_SABER_DEFENSE`.
    pub offense: i32,
    /// The defence.
    pub defense: i32,
    /// How its blade has been read.
    pub storage: SaberStorage,
    /// The blade being swung, or for the other side its first saber's first blade.
    pub blade: BladeHistory,
    /// All its sabers' blades; empty for a fighter that carries just [`Self::blade`].
    pub sabers: SaberSet,
    /// `ps.saberHolstered`: 0 all lit, 1 the second saber or blades off, 2 all off.
    pub holstered: u8,
    /// `BG_SabersOff`, `ps.saberInFlight`, `ps.saberEntityNum != 0`, `ps.saberLockTime`.
    pub sabers_off: bool,
    /// Thrown.
    pub in_flight: bool,
    /// Its saber is in its hand or in the air, not knocked away.
    pub has_saber: bool,
    /// Until when it is in a saber lock.
    pub lock_time: i32,
    /// `sess.sessionTeam`: team games spare teammates' sabers.
    pub team: i32,
    /// `ps.duelIndex` while `ps.duelInProgress`.
    pub duel: Option<u16>,
    /// `G_ClientIdleInWorld`: no movement and no button in its last command.
    pub idle_in_world: bool,
    /// `ps.origin`, `ps.viewheight`, the view's yaw: where its saber is raised to block.
    pub origin: [f32; 3],
    /// Its eyes' height.
    pub view_height: f32,
    /// Which way it looks.
    pub view_yaw: f32,
    /// On its own against two in a power duel (`DUELTEAM_LONE`).
    pub lone_duelist: bool,
    /// An NPC (`s.eType == ET_NPC`): its blade and the blade of anyone on its
    /// `playerTeam` pass through each other (`w_saber.c:4773-4778`).
    pub npc: bool,
    /// `client->playerTeam` (`npcteam_t`): a player's is `NPCTEAM_PLAYER` outside siege.
    pub player_team: i32,
    /// `ps.saberEventFlags` (`SEF_*`): what its saber did lately, which the Jedi AI reads.
    pub event_flags: u32,
}

/// `saberEventFlags` (`w_saber.h:27-37`): what a saber did, for the Jedi AI.
pub mod sef {
    /// `SEF_HITENEMY`: its blade struck its enemy.
    pub const HIT_ENEMY: u32 = 0x1;
    /// `SEF_HITOBJECT`: it struck another client.
    pub const HIT_OBJECT: u32 = 0x2;
    /// `SEF_HITWALL`: it struck the world.
    pub const HIT_WALL: u32 = 0x4;
    /// `SEF_PARRIED`: it parried a swing.
    pub const PARRIED: u32 = 0x8;
    /// `SEF_DEFLECTED`: it deflected a missile or a saber.
    pub const DEFLECTED: u32 = 0x10;
    /// `SEF_BLOCKED`: its swing was parried.
    pub const BLOCKED: u32 = 0x20;
    /// `SEF_INWATER`: its blade is under water.
    pub const IN_WATER: u32 = 0x80;
    /// `SEF_LOCK_WON`: it won a saber lock.
    pub const LOCK_WON: u32 = 0x100;
}

/// `G_SaberAttackPower` (`w_saber.c:137`): how strong a saber is in a clash. The style's
/// level, twice over plus one when attacking, plus one for every `toleranceAmt` units
/// the blade moved since the reading before, 30% with a broken arm, between 1 and 16;
/// doubled for a lone power duellist, tripled for an attacker in siege.
pub fn attack_power(fighter: &Fighter, attacking: bool, level_time: i32, gametype: i32) -> i32 {
    let mut level = match fighter.style {
        SS_DUAL | SS_STAFF => 2,
        style => style,
    };
    if attacking {
        level = level * 2 + 1;
        let storage = &fighter.storage;
        if storage.last_time >= level_time - 50 && storage.older_valid {
            let tolerance = match fighter.style {
                SS_STRONG => 8,
                SS_MEDIUM => 16,
                SS_FAST => 24,
                _ => 16,
            };
            let swing = sub(storage.last_base, storage.older_base);
            let mut distance = length(swing) as i32;
            while distance > 0 {
                level += 1;
                distance -= tolerance;
            }
        }
    }
    if fighter.broken_arm {
        level = (f64::from(level) * 0.3) as i32;
    }
    level = level.clamp(1, 16);
    if gametype == GT_POWERDUEL && fighter.lone_duelist {
        level * 2
    } else if attacking && gametype == GT_SIEGE {
        level * 3
    } else {
        level
    }
}

/// `G_ClientIdleInWorld` (`w_saber.c:2295`) for a player's last command: no movement,
/// and none of attack, alternate attack, gesture, grip, Force power, lightning or drain.
pub fn idle_in_world(buttons: u16, forward: i8, right: i8, up: i8) -> bool {
    const ACTIVE: u16 = 1 | 8 | 64 | 128 | 512 | 1024 | 2048;
    forward == 0 && right == 0 && up == 0 && buttons & ACTIVE == 0
}

/// `SaberAttacking` for a fighter.
pub fn fighter_attacking(fighter: &Fighter) -> bool {
    attacking(
        fighter.saber_move,
        fighter.weapon_state,
        fighter.saber_blocked,
    )
}

impl Fighter {
    /// Its sabers, or the one saber of [`Self::blade`] for a fighter given none.
    pub fn saber_set(&self) -> SaberSet {
        if self.sabers.sabers.iter().any(|saber| saber.typed) {
            self.sabers
        } else {
            SaberSet::single(self.blade)
        }
    }

    /// `disarmChance` against this fighter's opponent (`w_saber.c:6840-6849`): 1, its first
    /// saber's `disarmBonus`, and — with `g_fixSaberDisarmBonus` — a fully lit second's.
    pub fn disarm_chance(&self) -> i32 {
        let [first, second] = &self.sabers.sabers;
        1 + first.combat.disarm_bonus
            + if second.held && self.holstered == 0 {
                second.combat.disarm_bonus
            } else {
                0
            }
    }
}

/// `WP_SabersIntersect` (`w_saber.c:2830`): `one` against every blade of the other's
/// sabers that has a type and a length.
pub fn sabers_intersect(one: &BladeHistory, other: &SaberSet, check_dir: bool) -> bool {
    other
        .sabers
        .iter()
        .filter(|saber| saber.typed)
        .any(|saber| {
            saber.blades[..saber.count.min(MAX_BLADES)]
                .iter()
                .filter(|blade| blade.length > 0.0)
                .any(|blade| blades_intersect(one, blade, check_dir))
        })
}

/// `WP_SabersIntersect`'s test for one blade of each: the triangles each blade swept
/// since the frame before, each point pushed 16 units further, tested with the game's
/// [`crate::tri_tri::tri_tri_intersect`]. `check_dir` also refuses blades swinging the
/// same way or held nearly parallel.
pub fn blades_intersect(one: &BladeHistory, other: &BladeHistory, check_dir: bool) -> bool {
    let (base1, tip1, base_next1, tip_next1) = swept(one);
    let (base2, tip2, base_next2, tip_next2) = swept(other);
    if check_dir {
        let mut direction1 = sub(tip_next1, tip1);
        let mut direction2 = sub(tip_next2, tip2);
        normalize(&mut direction1);
        normalize(&mut direction2);
        if dot(direction1, direction2) > 0.6 {
            return false;
        }
        let facing = dot(one.direction, other.direction);
        if !(-0.9..=0.9).contains(&facing) {
            return false;
        }
    }
    use crate::tri_tri::tri_tri_intersect;
    tri_tri_intersect([base1, tip1, base_next1], [base2, tip2, base_next2])
        || tri_tri_intersect([base1, tip1, base_next1], [base2, tip2, tip_next2])
        || tri_tri_intersect([base1, tip1, tip_next1], [base2, tip2, base_next2])
        || tri_tri_intersect([base1, tip1, tip_next1], [base2, tip2, tip_next2])
}

/// A blade's swept points: the old base, the old tip `length + 16` out, and both now,
/// each moved 16 further along its own motion.
fn swept(blade: &BladeHistory) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    let base = blade.point_old;
    let mut motion = sub(blade.point, blade.point_old);
    normalize(&mut motion);
    let base_next = add_scaled(blade.point, SABER_EXTRAPOLATE_DIST, motion);
    let reach = blade.length + SABER_EXTRAPOLATE_DIST;
    let tip = add_scaled(base, reach, blade.direction_old);
    let tip_moved = add_scaled(base_next, reach, blade.direction);
    let mut tip_motion = sub(tip_moved, tip);
    normalize(&mut tip_motion);
    (
        base,
        tip,
        base_next,
        add_scaled(tip_moved, SABER_EXTRAPOLATE_DIST, tip_motion),
    )
}

/// `SetSaberBoxSize` (`w_saber.c:391`): the box around every lit blade of the owner's
/// sabers as last read (each base and tip), relative to the saber entity at `origin`.
/// No box at all while the sabers are off, or in a broken parry or losing a super break
/// (the blows pass through) but for blades that always block; the default cube when the
/// blade was not read lately. The blades are as the damage loop last stored them — the
/// frame before, when called where the reference calls it.
pub fn saber_box(
    owner: &Fighter,
    super_break_lose: bool,
    origin: [f32; 3],
    level_time: i32,
) -> ([f32; 3], [f32; 3]) {
    let set = owner.saber_set();
    let pair = set.pair();
    // Blades that block all the same; the reference leaves the others unset.
    let mut always = [[false; MAX_BLADES]; 2];
    let mut force_block = false;
    if in_broken_parry(owner.saber_move) || super_break_lose {
        for (hand, saber) in set.sabers.iter().enumerate() {
            if hand > 0 && !pair {
                continue;
            }
            let count = saber.count.min(MAX_BLADES);
            if saber.combat.flags2 & SFL2_ALWAYS_BLOCK != 0 {
                always[hand][..count].fill(true);
                force_block |= count > 0;
            }
            if saber.combat.style2_start > 0 {
                for blade in saber.combat.style2_start..count {
                    always[hand][blade] = saber.combat.flags2 & SFL2_ALWAYS_BLOCK2 != 0;
                    force_block |= always[hand][blade];
                }
            }
        }
        if !force_block {
            return ([0.0; 3], [0.0; 3]);
        }
    }
    if level_time - owner.storage.last_time > 200
        || level_time - set.sabers[0].blades[0].storage_time > 100
    {
        return ([-SABER_BOX_SIZE; 3], [SABER_BOX_SIZE; 3]);
    }
    let off = if pair || set.sabers[0].count > 1 {
        owner.holstered > 1
    } else {
        owner.holstered != 0
    };
    if off {
        return ([0.0; 3], [0.0; 3]);
    }
    let (mut mins, mut maxs) = (origin, origin);
    for (hand, saber) in set.sabers.iter().enumerate() {
        if !saber.held {
            break;
        }
        if pair && owner.holstered == 1 && hand == 1 {
            break;
        }
        for (blade, history) in saber.blades[..saber.count.min(MAX_BLADES)]
            .iter()
            .enumerate()
        {
            if blade > 0 && !pair && saber.count > 1 && owner.holstered == 1 {
                break;
            }
            if force_block && !always[hand][blade] {
                continue;
            }
            let tip = add_scaled(history.point, history.length, history.direction);
            for axis in 0..3 {
                mins[axis] = mins[axis].min(history.point[axis]).min(tip[axis]);
                maxs[axis] = maxs[axis].max(history.point[axis]).max(tip[axis]);
            }
        }
    }
    (
        std::array::from_fn(|axis| mins[axis] - origin[axis]),
        std::array::from_fn(|axis| maxs[axis] - origin[axis]),
    )
}

/// What a clash decided beyond the two fighters' moves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clash {
    /// The blow returns a hit. A debug lock returns before setting the idle wound.
    pub hit: bool,
    /// The sabers met (`saberHitSaber`): the rest of the sweep is cut to `fraction`, and
    /// the clash effect is raised at the trace's end with its normal.
    pub met: bool,
    /// The two locked (`WP_SabersCheckLock`): no more clash rules ran. A debug lock
    /// also returns before the ordinary branch clears blocking and sets `met`.
    pub locked: bool,
    /// The rules ran to their end, which sets the swinger's `saberAttackWound`: no more
    /// blows land this frame.
    pub wound: bool,
}

/// What the clash rules read of the blow itself.
#[derive(Clone, Copy, Debug)]
pub struct Blow {
    /// The damage the trace would have done (after the multipliers).
    pub damage: i32,
    /// `attackStr`: the swing's `G_PowerLevelForSaberAnim`, one more for a kata.
    pub strength: i32,
    /// A special: nothing blocks it.
    pub unblockable: bool,
    /// Where the trace met the other saber.
    pub point: [f32; 3],
    /// The trace's fraction (`saberHitFraction`).
    pub fraction: f32,
    /// `level.time`, `level.gametype`.
    pub level_time: i32,
    /// The game type.
    pub gametype: i32,
    /// The blade swept is the swinger's thrown first saber (`saberInFlight`,
    /// `rSaberNum == 0`).
    pub thrown: bool,
}

/// What a clash reaches besides the two fighters.
pub trait ClashWorld {
    /// `g_debugSaberLocks`: bypass the ordinary clash rules with a random lock.
    fn debug_saber_locks(&self) -> bool {
        false
    }
    /// The game's generator (`Q_irand`), which the clash rules draw from.
    fn rng(&mut self) -> &mut Rng;
    /// `WP_SabersCheckLock(me, other)` on the two players themselves: whether they
    /// locked. A lock changes their states, not the clash's copies.
    fn check_lock(&mut self, me: u16, other: u16) -> bool;
    /// `saberKnockOutOfHand(owner's saber, owner, velocity)`: whether it flew.
    fn knock_out(&mut self, owner: u16, velocity: [f32; 3]) -> bool;
    /// `saberCheckKnockdown_Smashed(owner's thrown saber, owner, striker, damage)`:
    /// whether the striker knocked it out of the air.
    fn smash(&mut self, owner: u16, striker: u16, damage: i32) -> bool;
}

/// `CheckSaberDamage`'s clash branch after `WP_SabersIntersect` said yes, from the team
/// and duel checks to the final `saberAttackWound` (`w_saber.c:4766-5247`), for a player
/// whose saber is in hand. `me` swung; `other` owns the saber that was met.
pub fn clash(
    me: &mut Fighter,
    other: &mut Fighter,
    blow: &Blow,
    world: &mut dyn ClashWorld,
) -> Option<Clash> {
    // `OnSameTeam` with `g_friendlySaber 0`, then the duels.
    if me.team != 0 && me.team == other.team {
        return None;
    }
    // "don't hit your teammate's sabers if you are an NPC" (`w_saber.c:4773-4778`).
    if (me.npc || other.npc) && me.player_team == other.player_team && blow.gametype != GT_SIEGE {
        return None;
    }
    if other.duel.is_some_and(|with| with != me.number)
        || me.duel.is_some_and(|with| with != other.number)
    {
        return None;
    }
    // `w_saber.c:4792-4796`: before wounds, effects, thrown-saber checks and the
    // lock probability draw. The lock changes the real fighters through the world.
    if world.debug_saber_locks() {
        world.check_lock(me.number, other.number);
        return Some(Clash {
            hit: true,
            locked: true,
            ..Clash::default()
        });
    }
    let mut result = Clash {
        hit: true,
        met: true,
        ..Clash::default()
    };
    // `saberCheckKnockdown_Smashed`: a thrown saber struck hard enough, or by a blade in a
    // defence move, is knocked out of the air, and the blow is no hit.
    if other.in_flight && world.smash(other.number, me.number, blow.damage) {
        result.hit = false;
        return Some(result);
    }
    // "is this my thrown saber?": smashed by the blade it met.
    if blow.thrown && world.smash(me.number, other.number, blow.damage) {
        result.hit = false;
        return Some(result);
    }
    // A saber in the air, the other's or this thrown one, meets no clash rules: the
    // effect alone (`w_saber.c:4831-4842`).
    if other.in_flight || blow.thrown {
        result.hit = false;
        return Some(result);
    }
    let level = blow.level_time;
    let my_level = attack_power(me, fighter_attacking(me), level, blow.gametype);
    let other_level = attack_power(other, fighter_attacking(other), level, blow.gametype);
    let damage = blow.damage;
    let unblockable = blow.unblockable;
    let mut other_unblockable = false;
    if damage > SABER_NONATTACK_DAMAGE
        && !unblockable
        && world.rng().irand(1, 20) <= LOCK_FACTOR
        && !other.idle_in_world
        && world.check_lock(me.number, other.number)
    {
        // Locked (`w_saber.c:4854-4860`): the rules end here, the blow a hit.
        me.saber_blocked = BLOCKED_NONE;
        other.saber_blocked = BLOCKED_NONE;
        result.locked = true;
        return Some(result);
    }
    if in_special(other.saber_move) {
        other_unblockable = true;
        other.saber_blocked = BLOCKED_NONE;
    }
    let (mut did_offense, mut did_defense, mut try_deflect_again) = (false, false, false);
    if damage > SABER_NONATTACK_DAMAGE
        && my_level < 3
        && !in_bounce(other.saber_move)
        && !in_parry(me.saber_move)
        && !in_broken_parry(me.saber_move)
        && !in_special(me.saber_move)
        && !in_bounce(me.saber_move)
        && !in_deflect(me.saber_move)
        && !saber_rules::in_reflect(me.saber_move)
        && !unblockable
    {
        // "for now, just always try a deflect"
        if deflection_angle(me, other, level, blow.gametype, world.rng()) {
            me.saber_blocked = BLOCKED_BOUNCE_MOVE;
            did_offense = true;
        } else {
            try_deflect_again = true;
        }
    }
    // A knockaway: the other turns the blow aside and I go into a broken parry. The
    // draws happen as C's `&&` and `||` reach them, left to right.
    let first = (my_level < 3
        && if try_deflect_again {
            world.rng().irand(1, 10) <= 3
        } else {
            world.rng().irand(1, 10) <= 7
        })
        || (world.rng().irand(1, 10) <= 1 && other_level >= 3);
    let knocks = first
        && !in_bounce(me.saber_move)
        && !in_broken_parry(other.saber_move)
        && !in_special(other.saber_move)
        && !in_bounce(other.saber_move)
        && !in_deflect(other.saber_move)
        && !saber_rules::in_reflect(other.saber_move)
        && (other_level > 2 || (other.defense >= 3 && world.rng().irand(0, other_level) != 0))
        && !unblockable
        && !other_unblockable
        && damage > SABER_NONATTACK_DAMAGE
        && !did_offense;
    if knocks {
        if me.has_saber {
            broken_parry_knockdown(me, other, level, blow.gametype, world);
        }
        if !in_parry(other.saber_move) {
            other.saber_blocked = block_quadrant(other, blow.point);
            other.saber_move = saber_rules::knockaway_for_parry(other.saber_blocked);
        } else {
            other.saber_move = knockaway_for_parry_move(other.saber_move);
        }
        other.saber_blocked = BLOCKED_BOUNCE_MOVE;
        me.saber_move = saber_rules::broken_parry_for_attack(me.saber_move);
        me.saber_blocked = BLOCKED_BOUNCE_MOVE;
        did_defense = true;
    } else if (my_level > 2 || unblockable)
        && (other.defense < my_level
            || (other.defense == my_level
                && (f64::from(world.rng().irand(1, 10)) >= f64::from(other_level) * 1.5
                    || unblockable)))
        && in_parry(other.saber_move)
        && !in_broken_parry(other.saber_move)
        && !in_parry(me.saber_move)
        && !in_broken_parry(me.saber_move)
        && !in_bounce(me.saber_move)
        && damage > SABER_NONATTACK_DAMAGE
        && !did_offense
        && !other_unblockable
    {
        // I slam down on their parry with a move as strong as their defence: broken.
        if other.has_saber {
            broken_parry_knockdown(other, me, level, blow.gametype, world);
        }
        other.saber_move = broken_parry_for_parry(other.saber_move, world.rng());
        other.saber_blocked = BLOCKED_PARRY_BROKEN;
        did_defense = true;
    } else if my_level > 2
        && other_level >= 3
        && in_parry(other.saber_move)
        && !in_broken_parry(other.saber_move)
        && !in_parry(me.saber_move)
        && !in_broken_parry(me.saber_move)
        && !in_bounce(me.saber_move)
        && !in_deflect(me.saber_move)
        && !saber_rules::in_reflect(me.saber_move)
        && damage > SABER_NONATTACK_DAMAGE
        && !did_offense
        && !unblockable
    {
        // I bounce off their strong parry.
        if !try_deflect_again && !deflection_angle(me, other, level, blow.gametype, world.rng()) {
            try_deflect_again = true;
        }
        did_offense = true;
    } else if fighter_attacking(other)
        && damage > SABER_NONATTACK_DAMAGE
        && !in_special(other.saber_move)
        && !did_offense
        && !other_unblockable
    {
        // Both attacking: who wins the bounce.
        if !in_bounce(me.saber_move)
            && !in_bounce(other.saber_move)
            && !in_deflect(me.saber_move)
            && !in_deflect(other.saber_move)
            && !saber_rules::in_reflect(me.saber_move)
            && !saber_rules::in_reflect(other.saber_move)
        {
            let elapsed = other.torso_length - other.torso_timer;
            let (mine, theirs) = (me.saber_set(), other.saber_set());
            let mut defence = saber_rules::power_level(
                other.torso,
                other.torso_timer,
                elapsed,
                theirs.sabers[0].combat.saber_type,
                0,
                true,
            );
            if KATAS.contains(&other.torso) {
                defence += 1;
            }
            // `parryBonus` and `breakParryBonus`, drawn even at zero; a lit second saber's
            // too.
            defence += world.rng().irand(0, theirs.sabers[0].combat.parry_bonus);
            if theirs.pair() && other.holstered == 0 {
                defence += world.rng().irand(0, theirs.sabers[1].combat.parry_bonus);
            }
            let mut bonus = world
                .rng()
                .irand(0, mine.sabers[0].combat.break_parry_bonus);
            if mine.pair() && me.holstered == 0 {
                bonus += world
                    .rng()
                    .irand(0, mine.sabers[1].combat.break_parry_bonus);
            }
            let advantage = (blow.strength + bonus + me.offense) - (defence + other.offense);
            if advantage > 1 {
                other.saber_move = saber_rules::broken_parry_for_attack(other.saber_move);
                other.saber_blocked = BLOCKED_BOUNCE_MOVE;
            } else if advantage > 0 {
                other.saber_blocked = BLOCKED_ATK_BOUNCE;
            } else {
                // `attackAdv < 1`: every remaining case lands here.
                me.saber_move = saber_rules::broken_parry_for_attack(me.saber_move);
                me.saber_blocked = BLOCKED_BOUNCE_MOVE;
            }
            did_offense = true;
        }
    }
    let other_free = !in_parry(other.saber_move)
        && !in_broken_parry(other.saber_move)
        && !in_special(other.saber_move)
        && !in_bounce(other.saber_move)
        && !in_deflect(other.saber_move)
        && !saber_rules::in_reflect(other.saber_move);
    if !did_defense && damage <= SABER_NONATTACK_DAMAGE && !other_unblockable {
        // A touch: the other simply raises its saber to where it was touched.
        if other_free {
            other.saber_blocked = block_quadrant(other, blow.point);
            other.event_flags |= sef::PARRIED;
        }
    } else if !did_defense && damage > SABER_NONATTACK_DAMAGE && !other_unblockable && other_free {
        let mut crush = unblockable;
        if !fighter_attacking(other) {
            let idle_strength = match other.style {
                SS_DUAL | SS_STAFF => SS_MEDIUM,
                style => style,
            };
            other.saber_blocked = block_quadrant(other, blow.point);
            other.event_flags |= sef::PARRIED;
            me.event_flags |= sef::BLOCKED;
            if blow.strength + me.offense > idle_strength + other.defense {
                crush = true;
            } else {
                try_deflect_again = true;
            }
        } else if my_level > other_level
            || (my_level == other_level && world.rng().irand(1, 10) <= 2)
        {
            other.saber_blocked = block_quadrant(other, blow.point);
            crush = true;
            if other.has_saber {
                broken_parry_knockdown(other, me, level, blow.gametype, world);
            }
        } else if my_level == other_level {
            let free = |fighter: &Fighter| {
                !in_parry(fighter.saber_move)
                    && !in_broken_parry(fighter.saber_move)
                    && !in_special(fighter.saber_move)
                    && !in_bounce(fighter.saber_move)
                    && !in_deflect(fighter.saber_move)
                    && !saber_rules::in_reflect(fighter.saber_move)
                    && !unblockable
            };
            if !did_offense && free(me) {
                me.saber_blocked = BLOCKED_ATK_BOUNCE;
                did_offense = true;
            }
            if free(other) {
                other.saber_blocked = BLOCKED_ATK_BOUNCE;
            }
            me.event_flags |= sef::DEFLECTED;
            other.event_flags |= sef::DEFLECTED;
        } else if level - other.storage.last_time < 500 && !unblockable {
            // They are stronger: my attack breaks on theirs.
            me.saber_move = saber_rules::broken_parry_for_attack(me.saber_move);
            me.saber_blocked = BLOCKED_PARRY_BROKEN;
            if me.has_saber {
                broken_parry_knockdown(me, other, level, blow.gametype, world);
            }
            other.event_flags &= !sef::BLOCKED;
            did_offense = true;
        }
        let parry = parry_for_block(other.saber_blocked);
        if crush && in_parry(parry) {
            other.saber_move = broken_parry_for_parry(parry, world.rng());
            other.saber_blocked = BLOCKED_PARRY_BROKEN;
            other.event_flags &= !sef::PARRIED;
            me.event_flags &= !sef::BLOCKED;
        } else if in_parry(parry) && !did_offense && try_deflect_again {
            let before = other.saber_move;
            other.saber_move = parry;
            deflection_angle(me, other, level, blow.gametype, world.rng());
            other.saber_move = before;
        }
    }
    result.wound = true;
    Some(result)
}

/// `saberCheckKnockdown_BrokenParry` (`w_saber.c:6777-6860`): `owner`'s saber knocked out
/// of its hand by `other`'s stronger stance, along the swing's momentum — drawn as the
/// reference draws it, and knocked out through the world where it says so.
fn broken_parry_knockdown(
    owner: &Fighter,
    other: &Fighter,
    level_time: i32,
    gametype: i32,
    world: &mut dyn ClashWorld,
) {
    if let Some(velocity) = broken_parry_disarm(owner, other, level_time, gametype, world.rng()) {
        let _ = world.knock_out(owner.number, velocity);
    }
}

/// The draws and the momentum of `saberCheckKnockdown_BrokenParry`: the velocity `owner`'s
/// saber is to be knocked out of its hand with, where it is.
pub fn broken_parry_disarm(
    owner: &Fighter,
    other: &Fighter,
    level_time: i32,
    gametype: i32,
    rng: &mut Rng,
) -> Option<[f32; 3]> {
    // `SABERINVALID`.
    if !owner.has_saber || owner.lock_time > level_time - 100 {
        return None;
    }
    let mine = attack_power(owner, false, level_time, gametype);
    let theirs = attack_power(other, false, level_time, gametype);
    if !other.storage.older_valid || level_time - other.storage.last_time >= 200 {
        return None;
    }
    let knock =
        (theirs > mine + 1 && rng.irand(1, 10) <= 7) || (theirs > mine && rng.irand(1, 10) <= 3);
    if !knock {
        return None;
    }
    let velocity =
        crate::saber_drop::disarm_velocity(&owner.storage, &other.storage, false, level_time)?;
    (rng.irand(0, other.disarm_chance()) != 0).then_some(velocity)
}

/// `WP_GetSaberDeflectionAngle` (`w_saber.c:1931`): the attacker's move after its blade
/// met the defender's — straight back (`PM_SaberBounceForAttack`) when the defender met
/// it square, otherwise deflected to a quadrant halfway to the defender's. `true` for a
/// deflection. Both players' blades must have been read in the last half second.
fn deflection_angle(
    attacker: &mut Fighter,
    defender: &Fighter,
    level_time: i32,
    gametype: i32,
    rng: &mut Rng,
) -> bool {
    if level_time - attacker.storage.last_time > 500
        || level_time - defender.storage.last_time > 500
    {
        return false;
    }
    let attacker_level = attack_power(attacker, fighter_attacking(attacker), level_time, gametype);
    let defender_level = attack_power(defender, fighter_attacking(defender), level_time, gametype);
    let (start, end) = saber_rules::move_quads(attacker.saber_move);
    let (start, end) = (i32::from(start), i32::from(end));
    let mut defender_quad = i32::from(saber_rules::move_quads(defender.saber_move).1);
    let mut difference = (defender_quad as f32 - start as f32).abs() as i32;
    if defender.saber_move == LS_READY {
        return false;
    }
    // Mirrored: they face each other.
    defender_quad = match defender_quad {
        Q_BR => Q_BL,
        Q_R => Q_L,
        Q_TR => Q_TL,
        Q_TL => Q_TR,
        Q_L => Q_R,
        Q_BL => Q_BR,
        quad => quad,
    };
    if difference > 4 {
        difference = 4 - (difference - 4);
    }
    let square = difference == 0 || (difference == 1 && rng.irand(0, 1) != 0);
    if square
        && (defender_level == attacker_level || rng.irand(0, defender_level - attacker_level) >= 0)
    {
        attacker.saber_move = saber_rules::bounce_for_attack(attacker.saber_move);
        attacker.saber_blocked = BLOCKED_ATK_BOUNCE;
        return false;
    }
    let mut difference = defender_quad - end;
    if difference > 4 {
        difference = 4 - (difference - 4);
    } else if difference < -4 {
        difference = -4 + (difference + 4);
    }
    let mut quad = end + (difference as f32 / 2.0).ceil() as i32;
    if quad < Q_BR {
        quad += Q_B;
    }
    if quad == start {
        if rng.irand(0, 1) != 0 {
            quad -= 1
        } else {
            quad += 1
        }
        if quad < Q_BR {
            quad = Q_B;
        } else if quad > Q_B {
            quad = Q_BR;
        }
    }
    if quad == defender_quad {
        attacker.saber_move = saber_rules::bounce_for_attack(attacker.saber_move);
        attacker.saber_blocked = BLOCKED_ATK_BOUNCE;
        return false;
    }
    attacker.saber_move = saber_rules::deflection_for_quad(quad as usize);
    attacker.saber_blocked = BLOCKED_BOUNCE_MOVE;
    true
}

/// `BG_BrokenParryForParry`, whose top parry is knocked up or down at random.
fn broken_parry_for_parry(parry: u32, rng: &mut Rng) -> u32 {
    if parry == LS_PARRY_UP {
        return if rng.irand(0, 1) != 0 {
            LS_H1_B_
        } else {
            LS_H1_T_
        };
    }
    saber_rules::broken_parry_for_parry(parry)
}

/// `G_KnockawayForParry` (`w_saber.c:2203`): a parry turned into the knockaway on its
/// side; anything else the upper right one.
fn knockaway_for_parry_move(parry: u32) -> u32 {
    match parry {
        LS_PARRY_UP => LS_K1_T_,
        LS_PARRY_UL => LS_K1_TL,
        LS_PARRY_LR => LS_K1_BR,
        LS_PARRY_LL => LS_K1_BL,
        _ => LS_K1_TR,
    }
}

/// `G_GetParryForBlock` (`w_saber.c:1886`).
fn parry_for_block(block: u32) -> u32 {
    match block {
        BLOCKED_UPPER_RIGHT => LS_PARRY_UR,
        BLOCKED_UPPER_RIGHT_PROJ => LS_REFLECT_UR,
        BLOCKED_UPPER_LEFT => LS_PARRY_UL,
        BLOCKED_UPPER_LEFT_PROJ => LS_REFLECT_UL,
        BLOCKED_LOWER_RIGHT => LS_PARRY_LR,
        BLOCKED_LOWER_RIGHT_PROJ => LS_REFLECT_LR,
        BLOCKED_LOWER_LEFT => LS_PARRY_LL,
        BLOCKED_LOWER_LEFT_PROJ => LS_REFLECT_LL,
        BLOCKED_TOP => LS_PARRY_UP,
        BLOCKED_TOP_PROJ => LS_REFLECT_UP,
        _ => LS_NONE,
    }
}

/// `WP_SaberBlockNonRandom` for a blow at `point` (`w_saber.c:9133`).
fn block_quadrant(fighter: &Fighter, point: [f32; 3]) -> u32 {
    u32::from(crate::saber_block::block_quadrant_at(
        fighter.origin,
        fighter.view_height,
        fighter.view_yaw,
        point,
        false,
    ))
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

/// `VectorNormalize`, returning the length.
pub(crate) fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = length(*vector);
    if length != 0.0 {
        let inverse = 1.0 / length;
        for value in vector.iter_mut() {
            *value *= inverse;
        }
    }
    length
}

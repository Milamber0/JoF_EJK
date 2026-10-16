//! Saber locks, the game's half: whether two blades that met lock (`WP_SabersCheckLock`,
//! `codemp/game/w_saber.c:1459-1885`), the lock itself (`WP_SabersCheckLock2`: the two
//! poses, the frames they start at, ten seconds, the two drawn together and facing), and
//! the lock's part of every think (`ClientThink_real`, `g_active.c:2917-3001`: facing the
//! other, the attack presses counted). The lock's movement half — the frames pushed, the
//! break — is `pmove_saber_lock`'s, and the animation tables it shares are here.

use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use crate::pmove_anim::{AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::PlayerState;

/// The old single-saber locks and their breaks.
pub const BOTH_BF2BREAK: u16 = 837;
pub const BOTH_BF2LOCK: u16 = 838;
pub const BOTH_BF1BREAK: u16 = 840;
pub const BOTH_BF1LOCK: u16 = 841;
pub const BOTH_CWCIRCLEBREAK: u16 = 846;
pub const BOTH_CCWCIRCLEBREAK: u16 = 847;
pub const BOTH_CWCIRCLELOCK: u16 = 848;
pub const BOTH_CCWCIRCLELOCK: u16 = 849;
/// The first of the new locks (`BOTH_LK_S_DL_S_B_1_L`); each pairing of styles holds ten
/// in a row (`_S_`/`_T_` side and top, each `B_1_L`, `B_1_W`, `L_1`, `SB_1_L`, `SB_1_W`).
const BOTH_LK_S_DL_S_B_1_L: u16 = 740;
const BOTH_LK_S_ST_S_B_1_L: u16 = 750;
const BOTH_LK_S_S_S_B_1_L: u16 = 760;
const BOTH_LK_DL_DL_S_B_1_L: u16 = 770;
const BOTH_LK_DL_ST_S_B_1_L: u16 = 780;
const BOTH_LK_DL_S_S_B_1_L: u16 = 790;
const BOTH_LK_ST_DL_S_B_1_L: u16 = 800;
const BOTH_LK_ST_ST_S_B_1_L: u16 = 810;
const BOTH_LK_ST_S_S_B_1_L: u16 = 820;
/// The locks of the same styles the other began, `BOTH_LK_S_S_S_L_2` on.
pub const BOTH_LK_S_S_S_L_2: u16 = 830;
pub const BOTH_LK_S_S_T_L_2: u16 = 831;
pub const BOTH_LK_DL_DL_S_L_2: u16 = 832;
pub const BOTH_LK_DL_DL_T_L_2: u16 = 833;
pub const BOTH_LK_ST_ST_S_L_2: u16 = 834;
pub const BOTH_LK_ST_ST_T_L_2: u16 = 835;
/// A pairing's lock proper (`L_1`) within its ten.
const LOCK_OFFSET: u16 = 2;
const TOP_OFFSET: u16 = 5;

/// `saberStyle_t`.
const SS_FAST: i32 = 1;
const SS_TAVION: i32 = 5;
const SS_DUAL: i32 = 6;
const SS_STAFF: i32 = 7;
/// `GT_DUEL`, `GT_POWERDUEL`.
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
/// `BLK_WIDE`: a saber held out to block anything.
const BLK_WIDE: u8 = 2;
/// `PMF_DUCKED`.
const PMF_DUCKED: u32 = 1;
/// `ENTITYNUM_NONE`.
const ENTITY_NONE: u32 = 1_023;
/// `FP_RAGE`, `FP_SABER_OFFENSE`.
const FP_RAGE: usize = 8;
/// `g_saberLockRandomNess`: what the game adds to a press, drawn up to this.
pub const LOCK_RANDOMNESS: i32 = 2;
/// `MASK_PLAYERSOLID`: what the two are drawn together through.
pub const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;

/// Wire fields of `playerState_t`.
const PS_VELOCITY: [usize; 3] = [6, 7, 8];
const PS_WEAPON_TIME: usize = 10;
const PS_EFLAGS_PM_FLAGS: usize = 38;
const PS_TORSO_ANIM: usize = 15;
const PS_LEGS_ANIM: usize = 13;
const PS_LEGS_TIMER: usize = 21;
const PS_GROUND: usize = 16;
const PS_SABER_ENTITY: usize = 31;
const PS_SABER_IN_FLIGHT: usize = 88;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_LOCK_TIME: usize = 107;
const PS_SABER_LOCK_FRAME: usize = 108;
const PS_SABER_LOCK_ENEMY: usize = 110;
const PS_DUEL_INDEX: usize = 44;
const PS_DUEL_IN_PROGRESS: usize = 119;
const PS_SABER_LOCK_ADVANCE: usize = 120;
const PS_FORCE_POWERS_ACTIVE: usize = 82;
const PS_WEAPON: usize = 47;
const PS_WEAPON_STATE: usize = 33;

fn field(state: &PlayerState, index: usize) -> u32 {
    state.raw_field(index).unwrap_or(0)
}

/// Which way a lock's two poses run (`sabersLockMode_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Top,
    DiagonalTopRight,
    DiagonalTopLeft,
    DiagonalBottomRight,
    DiagonalBottomLeft,
    Right,
    Left,
}

/// `G_SaberLockAnim` (`w_saber.c:1093-1197`): the pose of a lock, break or superbreak for
/// the side using `mine` against `theirs`, on `top` or the side, won or lost.
pub fn lock_animation(mine: i32, theirs: i32, top: bool, kind: LockKind, won: bool) -> u16 {
    let single = |style: i32| (SS_FAST..=SS_TAVION).contains(&style);
    if kind == LockKind::Lock && (mine == theirs || single(mine) && single(theirs)) && !won {
        // The loser of a lock between equals takes the defender's own stance.
        return match (theirs, top) {
            (SS_DUAL, true) => BOTH_LK_DL_DL_T_L_2,
            (SS_DUAL, false) => BOTH_LK_DL_DL_S_L_2,
            (SS_STAFF, true) => BOTH_LK_ST_ST_T_L_2,
            (SS_STAFF, false) => BOTH_LK_ST_ST_S_L_2,
            (_, true) => BOTH_LK_S_S_T_L_2,
            (_, false) => BOTH_LK_S_S_S_L_2,
        };
    }
    let base = match (mine, theirs) {
        (SS_DUAL, SS_DUAL) => BOTH_LK_DL_DL_S_B_1_L,
        (SS_DUAL, SS_STAFF) => BOTH_LK_DL_ST_S_B_1_L,
        (SS_DUAL, _) => BOTH_LK_DL_S_S_B_1_L,
        (SS_STAFF, SS_DUAL) => BOTH_LK_ST_DL_S_B_1_L,
        (SS_STAFF, SS_STAFF) => BOTH_LK_ST_ST_S_B_1_L,
        (SS_STAFF, _) => BOTH_LK_ST_S_S_B_1_L,
        (_, SS_DUAL) => BOTH_LK_S_DL_S_B_1_L,
        (_, SS_STAFF) => BOTH_LK_S_ST_S_B_1_L,
        _ => BOTH_LK_S_S_S_B_1_L,
    };
    let top = if top { TOP_OFFSET } else { 0 };
    match kind {
        LockKind::Lock => base + top + LOCK_OFFSET,
        LockKind::Break => base + top + u16::from(won),
        LockKind::SuperBreak => base + top + 3 + u16::from(won),
    }
}

/// A lock, its break, or a break that overpowers (`SABERLOCK_LOCK`, `_BREAK`,
/// `_SUPERBREAK`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKind {
    Lock,
    Break,
    SuperBreak,
}

/// `BG_CheckIncrementLockAnim` (`bg_saber.c:1283-1340`): whether the side in the new lock
/// pose `animation` wins by moving its frame up (`true`) or down, as the winning (`won`)
/// or losing side.
pub fn increments(animation: u16, won: bool) -> bool {
    let lock_1 = |base: u16, top: bool| base + if top { TOP_OFFSET } else { 0 } + LOCK_OFFSET;
    let up = [
        lock_1(BOTH_LK_DL_DL_S_B_1_L, false),
        BOTH_LK_DL_DL_S_L_2,
        lock_1(BOTH_LK_DL_DL_S_B_1_L, true),
        BOTH_LK_DL_DL_T_L_2,
        lock_1(BOTH_LK_DL_S_S_B_1_L, false),
        lock_1(BOTH_LK_DL_S_S_B_1_L, true),
        lock_1(BOTH_LK_DL_ST_S_B_1_L, false),
        lock_1(BOTH_LK_DL_ST_S_B_1_L, true),
        lock_1(BOTH_LK_S_S_S_B_1_L, false),
        BOTH_LK_S_S_T_L_2,
        lock_1(BOTH_LK_ST_S_S_B_1_L, false),
        lock_1(BOTH_LK_ST_S_S_B_1_L, true),
        lock_1(BOTH_LK_ST_ST_S_B_1_L, true),
        BOTH_LK_ST_ST_T_L_2,
    ];
    let down = [
        lock_1(BOTH_LK_S_DL_S_B_1_L, false),
        lock_1(BOTH_LK_S_DL_S_B_1_L, true),
        BOTH_LK_S_S_S_L_2,
        lock_1(BOTH_LK_S_S_S_B_1_L, true),
        lock_1(BOTH_LK_S_ST_S_B_1_L, false),
        lock_1(BOTH_LK_S_ST_S_B_1_L, true),
        lock_1(BOTH_LK_ST_DL_S_B_1_L, false),
        lock_1(BOTH_LK_ST_DL_S_B_1_L, true),
        lock_1(BOTH_LK_ST_ST_S_B_1_L, false),
        BOTH_LK_ST_ST_S_L_2,
    ];
    if up.contains(&animation) {
        won
    } else if down.contains(&animation) {
        !won
    } else {
        false
    }
}

/// `BG_InSaberLockOld`: the four single-saber lock poses.
pub fn in_lock_old(animation: u16) -> bool {
    matches!(
        animation,
        BOTH_BF2LOCK | BOTH_BF1LOCK | BOTH_CWCIRCLELOCK | BOTH_CCWCIRCLELOCK
    )
}

/// `BG_InSaberLock`: any lock pose, old or new.
pub fn in_lock(animation: u16) -> bool {
    let bases = [
        BOTH_LK_S_DL_S_B_1_L,
        BOTH_LK_S_ST_S_B_1_L,
        BOTH_LK_S_S_S_B_1_L,
        BOTH_LK_DL_DL_S_B_1_L,
        BOTH_LK_DL_ST_S_B_1_L,
        BOTH_LK_DL_S_S_B_1_L,
        BOTH_LK_ST_DL_S_B_1_L,
        BOTH_LK_ST_ST_S_B_1_L,
        BOTH_LK_ST_S_S_B_1_L,
    ];
    bases
        .iter()
        .any(|base| animation == base + LOCK_OFFSET || animation == base + TOP_OFFSET + LOCK_OFFSET)
        || (BOTH_LK_S_S_S_L_2..=BOTH_LK_ST_ST_T_L_2).contains(&animation)
        || in_lock_old(animation)
}

/// The swings each lock is taken from, one per style (`BOTH_A1_…` to `BOTH_A7_…`, each
/// style 77 animations on from the last).
fn swinging(animation: u16, first: u16) -> bool {
    (0..7).any(|style| animation == first + 77 * style)
}
const A1_T__B_: u16 = 126;
const A1__L__R: u16 = 127;
const A1__R__L: u16 = 128;
const A1_TL_BR: u16 = 129;
const A1_BR_TL: u16 = 130;
const A1_BL_TR: u16 = 131;
const A1_TR_BL: u16 = 132;
/// The parries a lock may also take (`BOTH_P1_S1_TR`, `_TL`, `_BL`, `_BR`).
const P1_S1_TR: u16 = 666;
const P1_S1_TL: u16 = 667;
const P1_S1_BL: u16 = 668;
const P1_S1_BR: u16 = 669;

/// A player as a lock reads and changes it.
pub struct LockFighter<'a> {
    /// Set when the lock's positioning trace permits `G_SetOrigin` and `LinkEntity`,
    /// including an unchanged origin. Initially false.
    pub origin_linked: bool,
    pub number: u16,
    pub state: &'a mut PlayerState,
    /// `r.mins`, `r.maxs`: the box it is linked with.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `ps.saberBlocking`, which the movement keeps.
    pub saber_blocking: u8,
    /// `pers.cmd.angles`: its last command's, which `SetClientViewAngle` turns it from.
    pub command_angles: [i32; 3],
    /// `ps.saberLockHits`, which the game keeps beside the wire.
    pub hits: &'a mut i32,
    /// The sabers' `animSpeedScale`, which the lock's poses play at.
    pub animation_scales: [f32; 2],
    /// Its sabers lock ([`crate::player_sabers::PlayerSabers::lockable`]).
    pub lockable: bool,
    /// An NPC (`ET_NPC`) locks with anyone not of its `playerTeam` (`client->playerTeam`,
    /// which a player has too), duelling or not ([`crate::npc_saber_lock`]).
    pub npc: bool,
    pub player_team: i32,
    /// `clipmask`: what it is drawn to the lock's distance through ([`MASK_PLAYERSOLID`]
    /// for a player).
    pub clip_mask: u32,
}

/// What a lock's traces go through: the map and every player but the two locking, with
/// `other` — the one not moving, where it stands now — added.
pub trait LockWorld {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        skip: u16,
        mask: u32,
        other: crate::entity_clip::BoxObstacle,
    ) -> MovementTrace;
}

/// `WP_SabersCheckLock` (`w_saber.c:1459-1885`) for two players whose blades met, both
/// with sabers (`g_debugSaberLocks` is [`forced_lock`]): whether they lock, and the
/// lock begun if so. `locking` is `g_saberLocking`.
pub fn check_lock<'a>(
    one: &mut LockFighter<'a>,
    two: &mut LockFighter<'a>,
    gametype: i32,
    locking: bool,
    rng: &mut Rng,
    lengths: &dyn AnimationLengths,
    world: &mut dyn LockWorld,
    level_time: i32,
) -> bool {
    if gametype == GT_POWERDUEL || !locking {
        return false;
    }
    // An NPC never locks with its own `playerTeam`, and locks outside duels (`w_saber.c:1488-1515`).
    let npc = one.npc || two.npc;
    if npc && one.player_team == two.player_team {
        return false;
    }
    let (a, b) = (&*one.state, &*two.state);
    if field(a, PS_SABER_ENTITY) == 0
        || field(b, PS_SABER_ENTITY) == 0
        || field(a, PS_SABER_IN_FLIGHT) != 0
        || field(b, PS_SABER_IN_FLIGHT) != 0
    {
        return false;
    }
    // Only two duelling each other lock, outside the duel games.
    let with_each_other = field(a, PS_DUEL_IN_PROGRESS) != 0
        && field(b, PS_DUEL_IN_PROGRESS) != 0
        && field(a, PS_DUEL_INDEX) == u32::from(two.number)
        && field(b, PS_DUEL_INDEX) == u32::from(one.number);
    if !npc && !with_each_other && gametype != GT_DUEL && gametype != GT_POWERDUEL {
        return false;
    }
    let (origin_a, origin_b) = (a.origin(), b.origin());
    if (origin_a[2] - origin_b[2]).abs() > 16.0 {
        return false;
    }
    if field(a, PS_GROUND) == ENTITY_NONE || field(b, PS_GROUND) == ENTITY_NONE {
        return false;
    }
    let distance = (0..3)
        .map(|axis| (origin_a[axis] - origin_b[axis]).powi(2))
        .sum::<f32>();
    if !(64.0..=6400.0).contains(&distance) {
        return false;
    }
    for state in [a, b] {
        let legs = field(state, PS_LEGS_ANIM) as u16;
        let rolling =
            crate::pmove_roll_anim::in_roll(legs) && field(state, PS_LEGS_TIMER) as i32 > 0;
        if crate::pmove_roll_anim::special_jump(legs) || rolling {
            return false;
        }
        if field(state, PS_FORCE_HAND_EXTEND) != 0
            || field(state, PS_EFLAGS_PM_FLAGS) & PMF_DUCKED != 0
        {
            return false;
        }
    }
    if !one.lockable || !two.lockable {
        return false;
    }
    if !crate::saber_block::in_front(origin_a, origin_b, b.view_angles(), 0.4)
        || !crate::saber_block::in_front(origin_b, origin_a, a.view_angles(), 0.4)
    {
        return false;
    }
    let (torso_a, torso_b) = (
        field(a, PS_TORSO_ANIM) as u16,
        field(b, PS_TORSO_ANIM) as u16,
    );
    // Top to bottom: the one swinging down attacks.
    if swinging(torso_a, A1_T__B_) {
        return begin(
            one,
            two,
            Some(LockMode::Top),
            rng,
            lengths,
            world,
            level_time,
        );
    }
    if swinging(torso_b, A1_T__B_) {
        return begin(
            two,
            one,
            Some(LockMode::Top),
            rng,
            lengths,
            world,
            level_time,
        );
    }
    // A player with its saber out wide and its weapon ready blocks anything.
    let blocking = |fighter: &LockFighter| {
        fighter.number < 32
            && fighter.saber_blocking == BLK_WIDE
            && field(fighter.state, PS_WEAPON_TIME) as i32 <= 0
    };
    let (blocking_a, blocking_b) = (blocking(one), blocking(two));
    // Each pairing, the first player's swing before the second's: the swing, the mode
    // against a block, then the answers and the modes they give.
    type Answer = (&'static [u16], &'static [u16], LockMode);
    let pairings: [(u16, LockMode, [Answer; 2]); 4] = [
        (
            A1_TR_BL,
            LockMode::DiagonalTopRight,
            [
                (&[A1_TR_BL], &[P1_S1_TL], LockMode::DiagonalTopRight),
                (&[A1_BR_TL], &[P1_S1_BL], LockMode::DiagonalBottomLeft),
            ],
        ),
        (
            A1_TL_BR,
            LockMode::DiagonalTopLeft,
            [
                (&[A1_TL_BR], &[P1_S1_TR], LockMode::DiagonalTopLeft),
                (&[A1_BL_TR], &[P1_S1_BR], LockMode::DiagonalBottomRight),
            ],
        ),
        (
            A1__L__R,
            LockMode::Left,
            [
                (&[A1_TL_BR], &[P1_S1_TR, P1_S1_BL], LockMode::Left),
                (&[], &[], LockMode::Left),
            ],
        ),
        (
            A1__R__L,
            LockMode::Right,
            [
                (&[A1_TR_BL], &[P1_S1_TL, P1_S1_BR], LockMode::Right),
                (&[], &[], LockMode::Right),
            ],
        ),
    ];
    for (swing, against_block, answers) in pairings {
        for first in [true, false] {
            let (torso, other_torso, other_blocks) = if first {
                (torso_a, torso_b, blocking_b)
            } else {
                (torso_b, torso_a, blocking_a)
            };
            if !swinging(torso, swing) {
                continue;
            }
            let (attacker, defender): (&mut LockFighter<'a>, &mut LockFighter<'a>) =
                if first { (one, two) } else { (two, one) };
            if other_blocks {
                return begin(
                    attacker,
                    defender,
                    Some(against_block),
                    rng,
                    lengths,
                    world,
                    level_time,
                );
            }
            for (swings, parries, mode) in answers {
                if swings.iter().any(|&answer| swinging(other_torso, answer))
                    || parries.contains(&other_torso)
                {
                    return begin(
                        attacker,
                        defender,
                        Some(mode),
                        rng,
                        lengths,
                        world,
                        level_time,
                    );
                }
            }
            return false;
        }
    }
    // Anything else locks now and then, any way (`LOCK_RANDOM`).
    rng.irand(0, 10) == 0 && begin(one, two, None, rng, lengths, world, level_time)
}

/// `WP_SabersCheckLock` with `g_debugSaberLocks` set (`w_saber.c:1466-1470`): any two whose
/// blades met lock at once, in a lock drawn at random (`LOCK_RANDOM`), whatever else would
/// refuse them.
pub fn forced_lock<'a>(
    one: &mut LockFighter<'a>,
    two: &mut LockFighter<'a>,
    rng: &mut Rng,
    lengths: &dyn AnimationLengths,
    world: &mut dyn LockWorld,
    level_time: i32,
) -> bool {
    let _ = begin(one, two, None, rng, lengths, world, level_time);
    true
}

/// Which lock `Q_irand(LOCK_FIRST, LOCK_RANDOM - 1)` picks, in the enum's order.
const MODES: [LockMode; 7] = [
    LockMode::Top,
    LockMode::DiagonalTopRight,
    LockMode::DiagonalTopLeft,
    LockMode::DiagonalBottomRight,
    LockMode::DiagonalBottomLeft,
    LockMode::Right,
    LockMode::Left,
];

/// `LOCK_IDEAL_DIST_TOP`, `LOCK_IDEAL_DIST_CIRCLE`, `LOCK_IDEAL_DIST_JKA`: how far apart the
/// two lock.
const IDEAL_TOP: f32 = 32.0;
const IDEAL_CIRCLE: f32 = 48.0;
const IDEAL_NEW: f32 = 46.0;

/// `WP_SabersCheckLock2` (`w_saber.c:1201-1457`): the lock begun, `attacker` in the pose
/// that starts ahead (`mode` drawn here when `None`, `LOCK_RANDOM`). Both take their
/// poses held, the frames they start at, ten seconds, one to three seconds before either
/// may push, stillness, each other's number; they are turned to face each other and drawn
/// to the lock's distance where nothing is in the way.
fn begin<'a>(
    attacker: &mut LockFighter<'a>,
    defender: &mut LockFighter<'a>,
    mode: Option<LockMode>,
    rng: &mut Rng,
    lengths: &dyn AnimationLengths,
    world: &mut dyn LockWorld,
    level_time: i32,
) -> bool {
    let mode = mode.unwrap_or_else(|| MODES[rng.irand(0, 6) as usize]);
    let (mine, theirs) = (
        field(attacker.state, PS_SABER_ANIM_LEVEL) as i32,
        field(defender.state, PS_SABER_ANIM_LEVEL) as i32,
    );
    let single = |style: i32| (SS_FAST..=SS_TAVION).contains(&style);
    let (att_anim, def_anim, att_start, def_start, ideal) = if single(mine) && single(theirs) {
        match mode {
            LockMode::Top => (BOTH_BF2LOCK, BOTH_BF1LOCK, 0.5, 0.5, IDEAL_TOP),
            LockMode::DiagonalTopRight => (
                BOTH_CCWCIRCLELOCK,
                BOTH_CWCIRCLELOCK,
                0.5,
                0.5,
                IDEAL_CIRCLE,
            ),
            LockMode::DiagonalTopLeft => (
                BOTH_CWCIRCLELOCK,
                BOTH_CCWCIRCLELOCK,
                0.5,
                0.5,
                IDEAL_CIRCLE,
            ),
            LockMode::DiagonalBottomRight => (
                BOTH_CWCIRCLELOCK,
                BOTH_CCWCIRCLELOCK,
                0.85,
                0.85,
                IDEAL_CIRCLE,
            ),
            LockMode::DiagonalBottomLeft => (
                BOTH_CCWCIRCLELOCK,
                BOTH_CWCIRCLELOCK,
                0.85,
                0.85,
                IDEAL_CIRCLE,
            ),
            LockMode::Right => (
                BOTH_CCWCIRCLELOCK,
                BOTH_CWCIRCLELOCK,
                0.75,
                0.75,
                IDEAL_CIRCLE,
            ),
            LockMode::Left => (
                BOTH_CWCIRCLELOCK,
                BOTH_CCWCIRCLELOCK,
                0.75,
                0.75,
                IDEAL_CIRCLE,
            ),
        }
    } else {
        // The new locks: the attacker's side winning or losing by the mode; the bottom and
        // side ones start near the end each side pushes towards.
        let top = mode == LockMode::Top;
        let attacker_wins = matches!(
            mode,
            LockMode::Top
                | LockMode::DiagonalTopRight
                | LockMode::DiagonalBottomRight
                | LockMode::Left
        );
        let att = lock_animation(mine, theirs, top, LockKind::Lock, attacker_wins);
        let def = lock_animation(theirs, mine, top, LockKind::Lock, !attacker_wins);
        let (far, near) = match mode {
            LockMode::DiagonalBottomRight | LockMode::DiagonalBottomLeft => (0.85, 0.15),
            LockMode::Right | LockMode::Left => (0.75, 0.25),
            _ => (0.5, 0.5),
        };
        let att_start = if increments(att, true) { far } else { near };
        let def_start = if increments(def, false) { far } else { near };
        (att, def, att_start, def_start, IDEAL_NEW)
    };
    for (fighter, animation, start) in [
        (&mut *attacker, att_anim, att_start),
        (&mut *defender, def_anim, def_start),
    ] {
        crate::pmove_anim::animate(
            fighter.state,
            SETANIM_BOTH,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            lengths,
            fighter.animation_scales,
        );
        // `firstFrame + numFrames * start`, in floats, truncated.
        let first = lengths.first_frame(animation).unwrap_or(0) as f32;
        let frames = lengths
            .timing(animation)
            .map_or(0, |timing| timing.frame_count) as f32;
        fighter
            .state
            .set_raw_field(PS_SABER_LOCK_FRAME, (first + frames * start) as i32 as u32);
    }
    let delay = rng.irand(1_000, 3_000);
    let (attacker_number, defender_number) = (attacker.number, defender.number);
    for (fighter, other) in [
        (&mut *attacker, defender_number),
        (&mut *defender, attacker_number),
    ] {
        *fighter.hits = 0;
        fighter.state.set_raw_field(PS_SABER_LOCK_ADVANCE, 0);
        for index in PS_VELOCITY {
            fighter.state.set_raw_field(index, 0);
        }
        fighter
            .state
            .set_raw_field(PS_SABER_LOCK_TIME, (level_time + 10_000) as u32);
        fighter
            .state
            .set_raw_field(PS_SABER_LOCK_ENEMY, u32::from(other));
        fighter.state.set_raw_field(PS_WEAPON_TIME, delay as u32);
    }
    // Face to face: the attacker turned to the defender, the defender back.
    let (att_origin, def_origin) = (attacker.state.origin(), defender.state.origin());
    let mut towards = [
        def_origin[0] - att_origin[0],
        def_origin[1] - att_origin[1],
        def_origin[2] - att_origin[2],
    ];
    let mut att_angles = attacker.state.view_angles();
    att_angles[1] = vector_yaw(towards);
    crate::triggers::face(attacker.state, att_angles, attacker.command_angles);
    let def_angles = [
        -att_angles[0],
        angle_normalize_180(att_angles[1] + 180.0),
        0.0,
    ];
    crate::triggers::face(defender.state, def_angles, defender.command_angles);
    // Drawn together: the attacker half the way, the defender the rest.
    let error = normalize(&mut towards) - ideal;
    let goal = [
        att_origin[0] + towards[0] * (error * 0.5),
        att_origin[1] + towards[1] * (error * 0.5),
        att_origin[2] + towards[2] * (error * 0.5),
    ];
    let other = obstacle(defender);
    let trace = world.trace(
        att_origin,
        attacker.bounds.0,
        attacker.bounds.1,
        goal,
        attacker.number,
        attacker.clip_mask,
        other,
    );
    if !trace.start_solid && !trace.all_solid {
        attacker.state.set_origin(trace.end_position);
        attacker.origin_linked = true;
    }
    let att_origin = attacker.state.origin();
    let mut back = [
        att_origin[0] - def_origin[0],
        att_origin[1] - def_origin[1],
        att_origin[2] - def_origin[2],
    ];
    let error = normalize(&mut back) - ideal;
    let goal = [
        def_origin[0] + back[0] * error,
        def_origin[1] + back[1] * error,
        def_origin[2] + back[2] * error,
    ];
    let other = obstacle(attacker);
    let trace = world.trace(
        def_origin,
        defender.bounds.0,
        defender.bounds.1,
        goal,
        defender.number,
        defender.clip_mask,
        other,
    );
    if !trace.start_solid && !trace.all_solid {
        defender.state.set_origin(trace.end_position);
        defender.origin_linked = true;
    }
    true
}

/// A fighter as the other's traces meet it: a living body where it stands.
fn obstacle(fighter: &LockFighter) -> crate::entity_clip::BoxObstacle {
    crate::entity_clip::BoxObstacle {
        entity: fighter.number,
        origin: fighter.state.origin(),
        bounds: fighter.bounds,
        contents: 0x100,
        model: None,
    }
}

/// `VectorNormalize`: the vector made a unit one in place, its length returned.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = f64::from(vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2])
        .sqrt() as f32;
    if length != 0.0 {
        let inverse = 1.0 / length;
        for value in vector.iter_mut() {
            *value *= inverse;
        }
    }
    length
}

/// `vectoyaw` (`bg_misc.c:1688-1707`): a direction's yaw, in doubles, as the game has it.
pub(crate) fn vector_yaw(vector: [f32; 3]) -> f32 {
    if vector[1] == 0.0 && vector[0] == 0.0 {
        return 0.0;
    }
    let mut yaw = if vector[0] != 0.0 {
        (f64::from(vector[1]).atan2(f64::from(vector[0])) * 180.0 / std::f64::consts::PI) as f32
    } else if vector[1] > 0.0 {
        90.0
    } else {
        270.0
    };
    if yaw < 0.0 {
        yaw += 360.0;
    }
    yaw
}

/// `AngleNormalize180`, through `AngleNormalize360`'s sixteen bits.
fn angle_normalize_180(angle: f32) -> f32 {
    let angle =
        (360.0_f32 / 65_536.0) * (((angle * (65_536.0_f32 / 360.0)) as i32 & 65_535) as f32);
    if angle > 180.0 { angle - 360.0 } else { angle }
}

/// What the game keeps of a lock beside the wire (`ps.saberLockHits`,
/// `saberLockHitCheckTime`, `saberLockHitIncrementTime`), and the buttons of the last two
/// commands (`client->buttons`, `oldbuttons`), which a press is told by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockMemory {
    pub hits: i32,
    pub hit_check_time: i32,
    pub hit_increment_time: i32,
    pub buttons: u16,
    pub old_buttons: u16,
}

impl LockMemory {
    /// `ClientThink_real`'s swap at its end (`g_active.c:3395-3396`).
    pub fn latch(&mut self, buttons: u16) {
        self.old_buttons = self.buttons;
        self.buttons = buttons;
    }
}

/// `BUTTON_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
/// `saberStyle_t`'s medium and the two strong ones.
const SS_MEDIUM: i32 = 2;
const SS_STRONG: i32 = 3;
const SS_DESANN: i32 = 4;

/// The lock's part of `ClientThink_real` before the move (`g_active.c:2917-3001`) for a
/// player whose lock opponent stands at `opponent` (`None` for one gone): turned to face
/// it; once a frame an attack pressed since the last command adds to its weight — more
/// for a stronger style, rage's level instead while raging, a point less now and then
/// while recovering from it, and a little at random — and weight left over pushes the
/// lock this move (`saberLockAdvance`). Outside a lock the lock's frame is cleared.
/// `rage` is the rage level, `rage_recovery_time` `fd.forceRageRecoveryTime`, `bonus` the
/// sabers' `lockBonus` ([`crate::player_sabers::PlayerSabers::lock_bonus`]).
#[allow(clippy::too_many_arguments)]
pub fn think(
    state: &mut PlayerState,
    memory: &mut LockMemory,
    opponent: Option<[f32; 3]>,
    command_angles: [i32; 3],
    rage: u8,
    rage_recovery_time: i32,
    bonus: i32,
    level_time: i32,
    rng: &mut Rng,
) {
    if field(state, PS_SABER_LOCK_TIME) as i32 <= level_time {
        state.set_raw_field(PS_SABER_LOCK_FRAME, 0);
        return;
    }
    if let Some(opponent) = opponent {
        let origin = state.origin();
        let (pitch, yaw) = crate::damage::vector_to_angles([
            opponent[0] - origin[0],
            opponent[1] - origin[1],
            opponent[2] - origin[2],
        ]);
        crate::triggers::face(state, [pitch, yaw, 0.0], command_angles);
    }
    if memory.hit_check_time >= level_time {
        return;
    }
    memory.hit_check_time = level_time;
    if memory.buttons & BUTTON_ATTACK != 0
        && memory.old_buttons & BUTTON_ATTACK == 0
        && memory.hit_increment_time < level_time
    {
        memory.hit_increment_time = level_time;
        let mut hits = if field(state, PS_FORCE_POWERS_ACTIVE) & (1 << FP_RAGE) != 0 {
            1 + i32::from(rage)
        } else {
            match field(state, PS_SABER_ANIM_LEVEL) as i32 {
                SS_FAST => 1,
                SS_MEDIUM | SS_TAVION | SS_DUAL | SS_STAFF => 2,
                SS_STRONG | SS_DESANN => 3,
                _ => 0,
            }
        };
        if rage_recovery_time > level_time && rng.irand(0, 1) != 0 {
            hits -= 1;
        }
        memory.hits += hits + bonus;
        if LOCK_RANDOMNESS != 0 {
            memory.hits += rng.irand(0, LOCK_RANDOMNESS);
            memory.hits = memory.hits.max(0);
        }
    }
    if memory.hits > 0 {
        if field(state, PS_SABER_LOCK_ADVANCE) == 0 {
            memory.hits -= 1;
        }
        state.set_raw_field(PS_SABER_LOCK_ADVANCE, 1);
    }
}

/// `EV_SABER_BLOCK`, and the entity fields its origin and direction go in.
const EV_SABER_BLOCK: u32 = 31;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ANGLES_1: usize = 9;
/// `ps.saberBlocked`.
const PS_SABER_BLOCKED: usize = 77;

/// `WP_SaberPositionUpdate` in a lock (`w_saber.c:8747-8781`), for a player whose saber
/// entity stands at `saber_origin`: the blades' block effect every 400 to 600 ms
/// (`saberIdleWound`, the game's generator), no block raised, and nothing more of the
/// saber's update — no damage traces — this frame. Returns whether the player is locked
/// (the update stops here) and the effect to raise.
pub fn lock_blocks(
    state: &mut PlayerState,
    health: i32,
    idle_wound: &mut i32,
    saber_origin: [f32; 3],
    level_time: i32,
    rng: &mut Rng,
) -> (bool, Option<crate::event_entity::EventEntity>) {
    // The update stops before the lock for a player without its saber out, raising or
    // lowering it, or dead (`returnAfterUpdate`, `w_saber.c:8427-8436`).
    let weapon_state = field(state, PS_WEAPON_STATE);
    if field(state, PS_WEAPON) != 3 || matches!(weapon_state, 1 | 2) || health < 1 {
        return (false, None);
    }
    if field(state, PS_SABER_LOCK_TIME) as i32 <= level_time || field(state, PS_SABER_ENTITY) == 0 {
        return (false, None);
    }
    let mut effect = None;
    if *idle_wound < level_time {
        let mut event = crate::event_entity::EventEntity {
            event: EV_SABER_BLOCK,
            parameter: 1,
            origin: saber_origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for (slot, (index, value)) in ES_ORIGIN.into_iter().zip(saber_origin).enumerate() {
            event.extra[slot] = (index, value.to_bits());
        }
        event.extra[3] = (ES_ANGLES_1, 1.0f32.to_bits());
        effect = Some(event);
        *idle_wound = level_time + rng.irand(400, 600);
    }
    state.set_raw_field(PS_SABER_BLOCKED, 0);
    (true, effect)
}

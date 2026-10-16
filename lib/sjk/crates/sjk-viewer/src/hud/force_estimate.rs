//! Estimated Force points of other players.
//!
//! The server sends a client's Force pool only to that client, so another
//! player's pool is rebuilt from what their entity state shows, with the
//! server's own rules (`WP_ForcePowerStart`, `WP_ForcePowerRun`,
//! `WP_ForcePowersUpdate`, see `sjk-game-jka`'s `force_powers.rs`):
//!
//! - the pool starts full at spawn and refills a point every
//!   `g_forceRegenTime` (200 ms unless the server says otherwise), six times as
//!   fast with the boon, but not while any power but drain is active or a saber
//!   is thrown;
//! - a power that switches on in `forcePowersActive` costs its `forcePowerNeeded`
//!   entry (a grip a flat 30); protect and absorb take another point every 300 and
//!   600 ms, grip every 100 ms and lightning every 50 ms while on;
//! - a force jump costs nothing to start but, while it lasts, every 300 ms (200 at
//!   level 1) the faster it rises the more: 20, 16, 12, 8, 6 or 4 over its level
//!   (`BG_ForcePowerDrain`'s levitation case, the rise sent as `pos.trDelta`);
//! - a push or pull (seen as its torso animation starting) and a saber throw cost
//!   their price; a disarm also sends the saber flying, but its own entity says it was
//!   not thrown, and costs nothing;
//! - saber specials cost a flat price when their move starts (`PM_SaberAttackForMovement`,
//!   `PM_CheckKata`): a kata 50, a cartwheel or butterfly 10, a lunge, a spin, a flip
//!   attack, a death from above, a stab down or a roll stab 25; a wall run-up, a flip
//!   off a wall and a jump off a wall grab 6 or so;
//! - heal and the team powers, which never show as active, cost their price when
//!   their sound or `EV_TEAM_POWER` is seen; team energize gives what it says;
//! - drain costs its shots and takes from its victims, who refill nothing for 800 ms
//!   after each; absorb gives a point back a drain or lightning shot
//!   ([`super::force_streams`]) and more against a push or pull;
//! - protect pays Force for the blows it softens ([`super::vitals_estimate`]);
//! - a special refused for want of Force (`EV_NOAMMO` 0) proves the pool under 50;
//!   becoming the Jedi Master fills it.
//!
//! The players' power levels are not sent either, so the pool is kept as a range
//! ([`Range`]): the low bound pays every power at its dearest level, the high bound
//! at its cheapest, and the guess at level 3, the level most players run. A power
//! can only start when the pool holds its price, so each one seen raises the low
//! bound to that price. While the pace has not been measured the bounds refill a
//! little slower and faster than the guess. Saber blocks in some mods and pickups are
//! not seen. The pool refills while a player idles, so the range closes within about
//! twenty seconds, and a respawn resets it to full.
use super::estimate::{History, Range};
use super::force_streams::Effect;
use super::vitals_estimate::ForceFacts;
use sjk_game_jka::force_powers::{
    FORCE_POWER_MAX, FORCE_POWER_NEEDED, FP_ABSORB, FP_DRAIN, FP_GRIP, FP_HEAL, FP_LEVITATION,
    FP_LIGHTNING, FP_PROTECT, FP_PULL, FP_PUSH, FP_SABER_THROW, NUM_FORCE_POWERS,
};

/// A full pool.
const MAX_POOL: f32 = FORCE_POWER_MAX as f32;
/// The level the guess assumes every power is at (see the module notes).
const ASSUMED_LEVEL: usize = 3;
/// `g_forceRegenTime`'s default.
pub(super) const DEFAULT_REGEN_MILLIS: f32 = 200.0;
/// A player unseen this long (in server milliseconds) is taken to have idled.
const GAP_MILLIS: i32 = 1_500;
/// A force jump's charge by how fast it rises (`BG_ForcePowerDrain`): over each
/// speed, the points before the level divides them; and how often it is charged at
/// level 1 and above.
const JUMP_BANDS: [(f32, f32); 6] = [
    (250.0, 20.0),
    (200.0, 16.0),
    (150.0, 12.0),
    (100.0, 8.0),
    (50.0, 6.0),
    (0.0, 4.0),
];
const JUMP_MILLIS: (i32, i32) = (200, 300);
/// A grip's start (`GRIP_DRAIN_AMOUNT`).
const GRIP_START: f32 = 30.0;
/// Saber moves with a price when they start (`saberMoveName_t`), whatever the level.
const SABER_MOVE_COSTS: [(u16, f32); 20] = [
    // Katas (`SABER_ALT_ATTACK_POWER`).
    (52, 50.0), // LS_A1_SPECIAL
    (53, 50.0), // LS_A2_SPECIAL
    (54, 50.0), // LS_A3_SPECIAL
    (50, 50.0), // LS_DUAL_SPIN_PROTECT
    (51, 50.0), // LS_STAFF_SOULCAL
    // Cartwheels and butterflies (`SABER_ALT_ATTACK_POWER_LR`).
    (20, 10.0), // LS_JUMPATTACK_ARIAL_LEFT
    (21, 10.0), // LS_JUMPATTACK_ARIAL_RIGHT
    (26, 10.0), // LS_BUTTERFLY_LEFT
    (27, 10.0), // LS_BUTTERFLY_RIGHT
    // The rest (`SABER_ALT_ATTACK_POWER_FB`).
    (19, 25.0), // LS_JUMPATTACK_DUAL
    (25, 25.0), // LS_JUMPATTACK_STAFF_RIGHT
    (18, 25.0), // LS_A_FLIP_SLASH
    (17, 25.0), // LS_A_FLIP_STAB
    (16, 25.0), // LS_A_JUMP_T__B_
    (15, 25.0), // LS_A_LUNGE
    (30, 25.0), // LS_SPINATTACK
    (29, 25.0), // LS_SPINATTACK_DUAL
    (47, 25.0), // LS_STABDOWN
    (48, 25.0), // LS_STABDOWN_STAFF
    (49, 25.0), // LS_STABDOWN_DUAL
];
const LS_ROLL_STAB: u16 = 14;
/// A wall run-up's flip and a jump off a wall grab, at level 3 (`420 / 3`, `336 / 3`).
const WALL_MOVE: f32 = 6.0;
/// What an absorb gives back against a push or pull (its price of 20, a third, times
/// the absorb level 1 to 3): the least and the guess.
const ABSORBED_THROW: (f32, f32) = (6.0, 18.0);
/// A special refused for want of Force: the pool was under the dearest's 50.
const REFUSED_CEILING: f32 = 49.0;
/// While drain stays on the pool held 25 before the frame's shot of 5
/// (`WP_ForcePowerRun` stops it below 25).
const DRAINING_FLOOR: f32 = 20.0;
/// How much slower and faster than the guess the bounds refill while the pace is
/// only assumed (not measured from the local pool).
const SLOW_PACE: f32 = 1.5;
const FAST_PACE: f32 = 0.75;
/// Clients tracked.
const CLIENTS: usize = 32;

/// What one snapshot shows of a player's Force use.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Observation {
    /// Server time of the snapshot, in milliseconds.
    pub(super) time: i32,
    /// `entityState_t::forcePowersActive`.
    pub(super) active: u32,
    /// Torso animation number.
    pub(super) torso_animation: u16,
    /// `entityState_t::saberInFlight`.
    pub(super) saber_in_flight: bool,
    /// A saber special move is under way (`BG_SaberInSpecial`), which also stops the refill.
    pub(super) saber_special: bool,
    /// How many times faster than normal the pool refills (boon, Jedi Master).
    pub(super) regen_multiplier: f32,
    /// The player is dead.
    pub(super) dead: bool,
    /// What drain, lightning and grip shots did to the pool since the last observation.
    pub(super) stream: Effect,
    /// How fast the player rises (`pos.trDelta[2]`), for the force jump's charge.
    pub(super) rising: f32,
    /// Legs animation number.
    pub(super) legs_animation: u16,
    /// `saberMove` while a saber is held.
    pub(super) saber_move: Option<u16>,
    /// The saber flies because it was knocked away, not thrown.
    pub(super) disarmed: bool,
    /// `isJediMaster`.
    pub(super) jedi_master: bool,
    /// What the snapshot's events showed (heals, team powers, protect, absorb).
    pub(super) facts: ForceFacts,
    /// The absorb took a push or pull (its hit sound while one is thrown nearby).
    pub(super) absorbed_throw: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Track {
    pool: Range,
    previous: Observation,
    seen: bool,
    /// Until when each bound's refill is held back (low, guess, high).
    held: [i32; 3],
    /// When a force jump's next charge is due at level 1 and above.
    jump_due: [Option<i32>; 2],
}

/// Animation numbers of the instant powers and the moves with a price.
#[derive(Clone, Copy, Debug)]
pub(super) struct Animations {
    push: Option<u16>,
    pull: Option<u16>,
    /// `BOTH_FORCEWALLRUNFLIP_START`: a level-3 wall run-up's flip.
    run_up: Option<u16>,
    /// `BOTH_WALL_FLIP_BACK1`: a level-2 flip back off a wall (or a JA+ flip kick).
    flip_back: Option<u16>,
    /// `BOTH_FORCEWALLREBOUND_*` and `BOTH_FORCEWALLHOLD_*`: hanging on a wall.
    wall_grab: [Option<u16>; 8],
    /// `BOTH_FORCEJUMP1` and `BOTH_FORCEWALLRELEASE_*`: leaving a wall grab.
    wall_leave: [Option<u16>; 5],
}

impl Animations {
    pub(super) fn lookup() -> Self {
        let index = |name| sjk_game_jka::legacy_animation_index(name).map(|n| n as u16);
        Self {
            push: index("BOTH_FORCEPUSH"),
            pull: index("BOTH_FORCEPULL"),
            run_up: index("BOTH_FORCEWALLRUNFLIP_START"),
            flip_back: index("BOTH_WALL_FLIP_BACK1"),
            wall_grab: [
                "BOTH_FORCEWALLREBOUND_FORWARD",
                "BOTH_FORCEWALLREBOUND_LEFT",
                "BOTH_FORCEWALLREBOUND_BACK",
                "BOTH_FORCEWALLREBOUND_RIGHT",
                "BOTH_FORCEWALLHOLD_FORWARD",
                "BOTH_FORCEWALLHOLD_LEFT",
                "BOTH_FORCEWALLHOLD_BACK",
                "BOTH_FORCEWALLHOLD_RIGHT",
            ]
            .map(index),
            wall_leave: [
                "BOTH_FORCEJUMP1",
                "BOTH_FORCEWALLRELEASE_FORWARD",
                "BOTH_FORCEWALLRELEASE_LEFT",
                "BOTH_FORCEWALLRELEASE_BACK",
                "BOTH_FORCEWALLRELEASE_RIGHT",
            ]
            .map(index),
        }
    }

    /// Whether `torso` is a push or a pull.
    pub(super) fn throwing(&self, torso: u16) -> bool {
        Some(torso) == self.push || Some(torso) == self.pull
    }
}

/// Measures the server's real refill pace from the local player's own pool, which
/// the server does send. `g_forceRegenTime` is not part of the stock info string
/// and mods (JA+) change the pace, so the estimate trusts this over any cvar.
///
/// While the local player idles (no power but drain, no thrown saber, no boon,
/// not Jedi Master) every point gained over a snapshot interval is a point of
/// regeneration; the total milliseconds over the total points is the pace.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Calibration {
    seen: bool,
    last_time: i32,
    last_pool: i32,
    last_idle: bool,
    millis: f32,
    points: f32,
}

/// Points of idle regeneration needed before the measured pace is used: one
/// unseen point either end then costs at most 5%.
const CALIBRATION_POINTS: f32 = 20.0;
/// Past this many points the totals are halved, so the pace follows changes.
const CALIBRATION_WINDOW: f32 = 400.0;

impl Calibration {
    /// Take one snapshot of the local player: its pool (`fd.forcePower`), and
    /// whether it was regenerating normally (see the type's notes).
    pub(super) fn observe(&mut self, time: i32, pool: i32, idle: bool) {
        if !self.seen {
            *self = Self {
                seen: true,
                last_time: time,
                last_pool: pool,
                last_idle: idle,
                ..*self
            };
            return;
        }
        if time <= self.last_time {
            return;
        }
        let elapsed = time - self.last_time;
        let gained = pool - self.last_pool;
        // Only a whole idle interval below a full pool says anything.
        if self.last_idle && idle && elapsed <= GAP_MILLIS && gained >= 0 && pool < 100 {
            self.millis += elapsed as f32;
            self.points += gained as f32;
            if self.points > CALIBRATION_WINDOW {
                self.millis *= 0.5;
                self.points *= 0.5;
            }
        }
        self.last_time = time;
        self.last_pool = pool;
        self.last_idle = idle;
    }

    /// Measured milliseconds per point, once enough has been seen.
    pub(super) fn millis_per_point(&self) -> Option<f32> {
        (self.points >= CALIBRATION_POINTS).then(|| self.millis / self.points)
    }
}

/// Fixed-capacity estimator for every client slot.
pub(super) struct Estimator {
    tracks: [Track; CLIENTS],
    regen_millis: f32,
    /// The pace was measured from the local pool, so the bounds refill with the guess.
    measured: bool,
    animations: Animations,
    /// Each player's pool over the last snapshots, for their shared vitals.
    histories: Box<[History; CLIENTS]>,
}

impl Default for Estimator {
    fn default() -> Self {
        Self {
            tracks: [Track::default(); CLIENTS],
            regen_millis: DEFAULT_REGEN_MILLIS,
            measured: false,
            animations: Animations::lookup(),
            histories: Box::new([History::default(); CLIENTS]),
        }
    }
}

impl Estimator {
    /// Use a refill pace in milliseconds per point: `measured` from the local pool,
    /// or else the server's `g_forceRegenTime`; anything unusable keeps the default.
    pub(super) fn set_regen_millis(&mut self, millis: Option<f32>, measured: bool) {
        let millis = millis.filter(|m| m.is_finite() && *m >= 1.0);
        self.measured = measured && millis.is_some();
        self.regen_millis = millis.unwrap_or(DEFAULT_REGEN_MILLIS);
    }

    /// Whether `torso` is a push or a pull.
    pub(super) fn throwing(&self, torso: u16) -> bool {
        self.animations.throwing(torso)
    }

    /// Milliseconds per point in use.
    pub(super) fn regen_millis(&self) -> f32 {
        self.regen_millis
    }

    /// Estimated shares of a full pool for `slot`, if it has been seen alive.
    pub(super) fn ratio(&self, slot: u16) -> Option<Range> {
        let track = self.tracks.get(usize::from(slot))?;
        (track.seen && !track.previous.dead).then(|| track.pool.share(MAX_POOL))
    }

    /// Correct `slot`'s pool with what its player shares (`PROTOCOL.md`, "Vitals"): their
    /// own client's Force at server time `time`, as points of a full pool of 100.
    pub(super) fn anchor(&mut self, slot: u16, time: i32, points: f32) {
        let slot = usize::from(slot);
        let Some(track) = self.tracks.get_mut(slot) else {
            return;
        };
        if track.seen
            && !track.previous.dead
            && self.histories[slot].anchor(time, points, &mut track.pool)
        {
            track.pool = track.pool.clamp(0.0, MAX_POOL);
        }
    }

    /// Points regained at `multiplier` times the pace over each bound's `elapsed`
    /// milliseconds: the low bound's, the guess's and the high bound's.
    fn regained(&self, elapsed: [f32; 3], multiplier: f32) -> (f32, f32, f32) {
        let points = |elapsed: f32, millis: f32| elapsed / millis * multiplier.max(1.0);
        let (slow, fast) = if self.measured {
            (1.0, 1.0)
        } else {
            (SLOW_PACE, FAST_PACE)
        };
        (
            points(elapsed[0], self.regen_millis * slow),
            points(elapsed[1], self.regen_millis),
            points(elapsed[2], self.regen_millis * fast),
        )
    }

    /// Take one snapshot's view of `slot`. Observations must come in server-time order;
    /// repeating a time changes nothing.
    pub(super) fn observe(&mut self, slot: u16, now: Observation) {
        let animations = self.animations;
        let Some(mut track) = self.tracks.get(usize::from(slot)).copied() else {
            return;
        };
        if now.dead {
            track.previous = now;
            track.seen = false;
            self.tracks[usize::from(slot)] = track;
            self.histories[usize::from(slot)].clear();
            return;
        }
        if !track.seen {
            self.histories[usize::from(slot)].clear();
            self.histories[usize::from(slot)].push(now.time, Range::exact(MAX_POOL));
            self.tracks[usize::from(slot)] = Track {
                pool: Range::exact(MAX_POOL),
                previous: now,
                seen: true,
                held: [i32::MIN; 3],
                jump_due: [None; 2],
            };
            return;
        }
        let before = track.previous;
        let elapsed = now.time.wrapping_sub(before.time);
        if elapsed <= 0 {
            return;
        }
        let elapsed_f = elapsed as f32;
        if elapsed > GAP_MILLIS {
            // Unseen: the guess idled, but nothing says the low bound did not spend it.
            let (_, best, high) = self.regained([elapsed_f; 3], now.regen_multiplier);
            track.pool = track.pool.add(0.0, best, high);
        } else {
            let cost = running_cost(before.active, elapsed_f);
            track.pool = track.pool.add(-cost, -cost, -cost);
            if regenerates(&before) {
                // A drain shot holds the refill back until its time is up.
                let open = track
                    .held
                    .map(|until| now.time.saturating_sub(before.time.max(until)).max(0) as f32);
                let (low, best, high) = self.regained(open, before.regen_multiplier);
                track.pool = track.pool.add(low, best, high);
            }
            // The pool tops out before what began in this interval is paid.
            track.pool = track.pool.at_most(MAX_POOL);
            track.pool = pay_starts(track.pool, &before, &now, &animations);
        }
        track.pool = jump_charges(track.pool, &mut track.jump_due, &now);
        track.pool = pay_moves(track.pool, &before, &now, &animations);
        track.pool = pay_events(track.pool, &now);
        let [low, best, high] = now.stream.force;
        track.pool = track.pool.add(-low, -best, -high);
        for (held, until) in track.held.iter_mut().zip(now.stream.hold) {
            *held = (*held).max(until);
        }
        if now.active & (1 << FP_DRAIN) != 0 {
            track.pool = track.pool.at_least(DRAINING_FLOOR);
        }
        if now.facts.refused {
            track.pool = track.pool.at_most(REFUSED_CEILING);
        }
        if now.jedi_master && !before.jedi_master {
            // `JediMasterUpdate`: the new master's pool is full.
            track.pool = Range::exact(MAX_POOL);
        }
        track.pool = track.pool.clamp(0.0, MAX_POOL);
        track.previous = now;
        self.tracks[usize::from(slot)] = track;
        self.histories[usize::from(slot)].push(now.time, track.pool);
    }
}

/// Whether the server lets the pool refill while `state` holds
/// (`WP_ForcePowersUpdate`): no power but drain on, no saber thrown.
fn regenerates(state: &Observation) -> bool {
    state.active & !(1 << FP_DRAIN) == 0 && !state.saber_in_flight && !state.saber_special
}

/// Points taken over `elapsed` milliseconds by powers that stay on.
fn running_cost(active: u32, elapsed: f32) -> f32 {
    let on = |power: usize| active & (1 << power) != 0;
    let mut cost = 0.0;
    if on(FP_PROTECT) {
        cost += elapsed / 300.0;
    }
    if on(FP_ABSORB) {
        cost += elapsed / 600.0;
    }
    if on(FP_GRIP) {
        cost += elapsed / 100.0;
    }
    if on(FP_LIGHTNING) {
        cost += elapsed / 50.0;
    }
    cost
}

/// `power`'s price over the levels a player may have it at: the cheapest, level
/// 3's (the guess) and the dearest.
fn prices(power: usize) -> (f32, f32, f32) {
    let levels = FORCE_POWER_NEEDED[1..=3]
        .iter()
        .map(|row| row[power] as f32);
    let cheapest = levels.clone().fold(f32::INFINITY, f32::min);
    let dearest = levels.fold(0.0, f32::max);
    (
        cheapest,
        FORCE_POWER_NEEDED[ASSUMED_LEVEL][power] as f32,
        dearest,
    )
}

/// `pool` once what began between two observations is paid. Each power that
/// starts first proves the pool held what `WP_ForcePowerUsable` asks for it
/// (drain and lightning start from 25 whatever their price).
fn pay_starts(
    mut pool: Range,
    before: &Observation,
    now: &Observation,
    animations: &Animations,
) -> Range {
    let pay = |pool: Range, power: usize| {
        let (cheapest, guess, dearest) = prices(power);
        let needed = if matches!(power, FP_DRAIN | FP_LIGHTNING) {
            cheapest.max(25.0)
        } else {
            cheapest
        };
        let pool = pool.at_least(needed);
        match power {
            // Charged while it rises ([`jump_charges`]).
            FP_LEVITATION => pool,
            // `ForceGrip`: a flat price, then the running cost.
            FP_GRIP => pool.add(-GRIP_START, -GRIP_START, -GRIP_START),
            // Paid by its shots.
            FP_DRAIN => pool,
            _ => pool.add(-dearest, -guess, -cheapest),
        }
    };
    let started = now.active & !before.active;
    for power in 0..NUM_FORCE_POWERS {
        // Push and pull are paid by their animation below.
        if started & (1 << power) != 0 && !matches!(power, FP_PUSH | FP_PULL) {
            pool = pay(pool, power);
        }
    }
    if now.torso_animation != before.torso_animation {
        if Some(now.torso_animation) == animations.push {
            pool = pay(pool, FP_PUSH);
        } else if Some(now.torso_animation) == animations.pull {
            pool = pay(pool, FP_PULL);
        }
    }
    if now.saber_in_flight && !before.saber_in_flight && !now.disarmed {
        pool = pay(pool, FP_SABER_THROW);
    }
    pool
}

/// A force jump's charges due by `now` (`PM_CheckJump`'s upkeep): one when it starts,
/// then every 200 ms at level 1 (the low bound's) and 300 ms above, each the points of
/// the band it rises in over the level (1 for the low bound; 3 for the guess and the
/// high bound, with the integer division the server does).
fn jump_charges(pool: Range, due: &mut [Option<i32>; 2], now: &Observation) -> Range {
    if now.active & (1 << FP_LEVITATION) == 0 {
        *due = [None; 2];
        return pool;
    }
    let band = JUMP_BANDS
        .iter()
        .find(|(above, _)| now.rising > *above)
        .map_or(0.0, |(_, points)| *points);
    let mut charges = [0.0_f32; 2];
    for (index, millis) in [JUMP_MILLIS.0, JUMP_MILLIS.1].into_iter().enumerate() {
        let next = due[index].get_or_insert(now.time);
        while *next <= now.time {
            charges[index] += 1.0;
            *next += millis;
        }
    }
    let dearest = band * charges[0];
    let cheapest = (band / 3.0).floor() * charges[1];
    pool.add(-dearest, -cheapest, -cheapest)
}

/// The flat prices of saber specials and wall moves that start between two
/// observations. A special only starts with its price in the pool
/// (`BG_EnoughForcePowerForMove`).
fn pay_moves(
    mut pool: Range,
    before: &Observation,
    now: &Observation,
    animations: &Animations,
) -> Range {
    if let Some(saber_move) = now.saber_move
        && now.saber_move != before.saber_move
    {
        let price = SABER_MOVE_COSTS
            .iter()
            .find(|(number, _)| *number == saber_move)
            .map(|(_, price)| *price)
            .or((saber_move == LS_ROLL_STAB).then_some(25.0));
        if let Some(price) = price {
            pool = pool.at_least(price).add(-price, -price, -price);
        }
    }
    if now.legs_animation != before.legs_animation {
        let legs = Some(now.legs_animation);
        let was = Some(before.legs_animation);
        let left_wall =
            animations.wall_grab.contains(&was) && animations.wall_leave.contains(&legs);
        if legs == animations.run_up || left_wall {
            pool = pool.add(-WALL_MOVE, -WALL_MOVE, -WALL_MOVE);
        } else if legs == animations.flip_back {
            // Level 2's flip: the rise's band over 2.
            let band = JUMP_BANDS
                .iter()
                .find(|(above, _)| now.rising > *above)
                .map_or(0.0, |(_, points)| *points);
            let price = (band / 2.0).floor();
            pool = pool.add(-price, -price, -price);
        }
    }
    pool
}

/// What the snapshot's events show: heals and team powers paid at their price, a team
/// energize's gift, protect's share of the blows (halved with the boon) and what an
/// absorb gave back against a push or pull.
fn pay_events(mut pool: Range, now: &Observation) -> Range {
    let facts = now.facts;
    let paid = |pool: Range, power: usize| {
        let (cheapest, guess, dearest) = prices(power);
        pool.at_least(cheapest).add(-dearest, -guess, -cheapest)
    };
    for _ in 0..facts.heals {
        pool = paid(pool, FP_HEAL);
    }
    if let Some(power) = facts.team_cast {
        pool = paid(pool, power);
    }
    if facts.gift > 0.0 {
        pool = pool
            .add(facts.gift, facts.gift, facts.gift)
            .at_most(MAX_POOL);
    }
    let boon = if now.regen_multiplier >= 6.0 {
        0.5
    } else {
        1.0
    };
    let [low, best, high] = facts.protect.map(|points| points * boon);
    pool = pool.add(-low, -best, -high);
    if now.absorbed_throw {
        let (least, guess) = ABSORBED_THROW;
        pool = pool.add(least, guess, guess).at_most(MAX_POOL);
    }
    pool
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_game_jka::force_powers::{FP_RAGE, FP_SPEED};

    fn at(time: i32) -> Observation {
        Observation {
            time,
            regen_multiplier: 1.0,
            ..Observation::default()
        }
    }

    fn with(mut observation: Observation, power: usize) -> Observation {
        observation.active |= 1 << power;
        observation
    }

    fn drained(mut observation: Observation, force: [f32; 3], hold: i32) -> Observation {
        observation.stream = Effect {
            force,
            hold: [hold; 3],
            ..Effect::default()
        };
        observation
    }

    fn percent(estimator: &Estimator) -> f32 {
        estimator.ratio(3).unwrap().best * 100.0
    }

    fn bounds(estimator: &Estimator) -> (f32, f32) {
        let range = estimator.ratio(3).unwrap();
        ((range.low * 100.0).round(), (range.high * 100.0).round())
    }

    #[test]
    fn a_player_is_full_when_first_seen() {
        let mut estimator = Estimator::default();
        assert_eq!(estimator.ratio(3), None);
        estimator.observe(3, at(1_000));
        assert_eq!(percent(&estimator), 100.0);
    }

    #[test]
    fn a_power_costs_its_price_when_it_switches_on_and_refills_afterwards() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(1_000));
        estimator.observe(3, with(at(1_050), FP_RAGE));
        assert!((percent(&estimator) - 50.0).abs() < 0.01, "rage costs 50");
        estimator.observe(3, at(1_100));
        // 10 s idle at one point per 200 ms is 50 points: full again.
        for step in 1..=10 {
            estimator.observe(3, at(1_100 + step * 1_000));
        }
        assert!((percent(&estimator) - 100.0).abs() < 0.01);
    }

    #[test]
    fn no_refill_while_a_power_is_on() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_SPEED));
        let after_start = percent(&estimator);
        estimator.observe(3, with(at(1_050), FP_SPEED));
        assert_eq!(percent(&estimator), after_start);
    }

    #[test]
    fn protect_takes_a_point_every_300_ms_while_on() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_PROTECT));
        let start = percent(&estimator);
        assert!((start - 90.0).abs() < 0.01, "protect costs 10 at level 3");
        for step in 1..=30 {
            estimator.observe(3, with(at(50 + step * 100), FP_PROTECT));
        }
        assert!((percent(&estimator) - (start - 10.0)).abs() < 0.01);
    }

    #[test]
    fn a_force_jump_is_charged_by_how_fast_it_rises() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        let rising = |time: i32, speed: f32| {
            let mut jump = with(at(time), FP_LEVITATION);
            jump.rising = speed;
            jump
        };
        // Rising over 250: a charge of 20 at level 1, 20 / 3 = 6 at level 3.
        estimator.observe(3, rising(50, 300.0));
        assert_eq!(bounds(&estimator), (80.0, 94.0));
        assert!((percent(&estimator) - 94.0).abs() < 0.01);
        // 300 ms later, slower (over 100): level 3 pays 8 / 3 = 2 at 350, level 1 the
        // whole 8 at 250 (its next is due at 450).
        estimator.observe(3, rising(350, 120.0));
        assert_eq!(bounds(&estimator), (72.0, 92.0));
        // Landed: no more charges.
        estimator.observe(3, at(400));
        assert_eq!(bounds(&estimator), (72.0, 92.0));
    }

    #[test]
    fn saber_specials_and_wall_moves_have_a_price() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        let mut kata = at(50);
        kata.saber_move = Some(52);
        kata.saber_special = true;
        estimator.observe(3, kata);
        assert_eq!(bounds(&estimator), (50.0, 50.0));
        // The same move seen again costs nothing more.
        kata.time = 100;
        estimator.observe(3, kata);
        assert_eq!(bounds(&estimator), (50.0, 50.0));
        // A cartwheel proves 10 and costs it.
        let mut cartwheel = kata;
        cartwheel.time = 150;
        cartwheel.saber_move = Some(20);
        estimator.observe(3, cartwheel);
        assert_eq!(bounds(&estimator), (40.0, 40.0));
        // A jump off a wall grab: 6.
        let animations = Animations::lookup();
        let mut grab = at(200);
        grab.legs_animation = animations.wall_grab[0].expect("a wall rebound");
        estimator.observe(3, grab);
        let mut leave = at(250);
        leave.legs_animation = animations.wall_leave[0].expect("a force jump");
        estimator.observe(3, leave);
        let (low, high) = bounds(&estimator);
        assert!((34.0..=35.0).contains(&low) && (34.0..=35.0).contains(&high));
    }

    #[test]
    fn heals_team_powers_protect_and_refusals() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        // A heal: 65 at level 1, 50 at level 3.
        let mut heal = at(50);
        heal.facts.heals = 1;
        estimator.observe(3, heal);
        assert_eq!(bounds(&estimator), (35.0, 50.0));
        // An energize from a teammate: exactly 33 more.
        let mut gift = at(100);
        gift.facts.gift = 33.0;
        estimator.observe(3, gift);
        assert_eq!(bounds(&estimator), (68.0, 83.0));
        // Protect paid for a blow.
        let mut protect = at(150);
        protect.facts.protect = [10.0, 2.5, 2.5];
        estimator.observe(3, protect);
        assert_eq!(bounds(&estimator), (58.0, 81.0));
        // A special refused: under 50.
        let mut refused = at(200);
        refused.facts.refused = true;
        estimator.observe(3, refused);
        assert_eq!(bounds(&estimator), (49.0, 49.0));
        // Becoming the Jedi Master fills it.
        let mut master = at(250);
        master.jedi_master = true;
        estimator.observe(3, master);
        assert_eq!(bounds(&estimator), (100.0, 100.0));
    }

    #[test]
    fn a_disarm_costs_nothing_and_a_grip_thirty() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        let mut knocked = at(50);
        knocked.saber_in_flight = true;
        knocked.disarmed = true;
        estimator.observe(3, knocked);
        assert_eq!(bounds(&estimator), (100.0, 100.0));
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_GRIP));
        assert_eq!(bounds(&estimator), (70.0, 70.0));
    }

    #[test]
    fn a_push_costs_its_price_when_its_animation_starts() {
        let mut estimator = Estimator::default();
        let push = estimator.animations.push.expect("BOTH_FORCEPUSH exists");
        estimator.observe(3, at(0));
        let mut pushing = at(50);
        pushing.torso_animation = push;
        estimator.observe(3, pushing);
        assert!((percent(&estimator) - 80.0).abs() < 0.01, "push costs 20");
        let mut still = at(100);
        still.torso_animation = push;
        estimator.observe(3, still);
        assert!((percent(&estimator) - 80.0).abs() < 0.5, "once per start");
    }

    #[test]
    fn a_thrown_saber_costs_and_blocks_refill() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        let mut thrown = at(50);
        thrown.saber_in_flight = true;
        estimator.observe(3, thrown);
        let after = percent(&estimator);
        assert!((after - 80.0).abs() < 0.01, "throw costs 20");
        thrown.time = 1_050;
        estimator.observe(3, thrown);
        assert_eq!(percent(&estimator), after, "no refill with a saber out");
    }

    #[test]
    fn a_saber_special_move_blocks_the_refill() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_RAGE));
        let mut special = at(100);
        special.saber_special = true;
        estimator.observe(3, special);
        let held = percent(&estimator);
        for step in 1..=20 {
            special.time = 100 + step * 100;
            estimator.observe(3, special);
        }
        assert_eq!(percent(&estimator), held);
    }

    #[test]
    fn dying_and_respawning_resets_the_pool() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_RAGE));
        let mut dead = at(100);
        dead.dead = true;
        estimator.observe(3, dead);
        assert_eq!(estimator.ratio(3), None);
        estimator.observe(3, at(5_000));
        assert_eq!(percent(&estimator), 100.0);
    }

    #[test]
    fn a_long_gap_counts_as_idle_time() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_RAGE));
        estimator.observe(3, at(60_000));
        assert_eq!(percent(&estimator), 100.0);
    }

    #[test]
    fn the_boon_and_the_servers_regen_time_speed_the_refill() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(100.0), false);
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_RAGE));
        estimator.observe(3, at(100));
        estimator.observe(3, at(1_100));
        // 1 s at one point per 100 ms: 10 points back.
        assert!((percent(&estimator) - 60.0).abs() < 0.01);
        estimator.set_regen_millis(Some(f32::NAN), true);
        assert_eq!(estimator.regen_millis, DEFAULT_REGEN_MILLIS);
        assert!(!estimator.measured);
    }

    #[test]
    fn unknown_levels_spread_the_bounds() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_PROTECT));
        // Protect costs 50, 25 or 10 by level; the guess is level 3's.
        assert!((percent(&estimator) - 90.0).abs() < 0.01);
        assert_eq!(bounds(&estimator), (50.0, 90.0));
    }

    #[test]
    fn a_power_starting_proves_the_pool_held_its_price() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        // Rage (50) then protect (50 at worst) could have left nothing.
        estimator.observe(3, with(at(50), FP_RAGE));
        estimator.observe(3, at(100));
        estimator.observe(3, with(at(150), FP_PROTECT));
        estimator.observe(3, at(200));
        assert_eq!(bounds(&estimator).0, 0.0);
        // Drain needs 25 to start, whatever its price: the low bound rises to it.
        estimator.observe(3, with(at(250), FP_DRAIN));
        let (low, high) = bounds(&estimator);
        assert_eq!(low, 25.0);
        assert!(high >= 39.0, "{high}");
    }

    #[test]
    fn the_bounds_meet_again_once_the_pool_is_full() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        estimator.observe(3, with(at(50), FP_PROTECT));
        estimator.observe(3, at(100));
        // An assumed pace refills the bounds at different speeds, but all reach 100.
        for step in 1..=60 {
            estimator.observe(3, at(100 + step * 1_000));
        }
        assert_eq!(bounds(&estimator), (100.0, 100.0));
    }

    #[test]
    fn drain_shots_take_from_the_pool_and_hold_its_refill() {
        let mut estimator = Estimator::default();
        estimator.set_regen_millis(Some(200.0), true);
        estimator.observe(3, at(0));
        estimator.observe(3, drained(at(50), [4.0, 4.0, 2.0], 850));
        assert!((percent(&estimator) - 96.0).abs() < 0.01);
        assert_eq!(bounds(&estimator), (96.0, 98.0));
        // Nothing comes back until 800 ms after the shot, then a point every 200 ms.
        estimator.observe(3, at(850));
        assert!((percent(&estimator) - 96.0).abs() < 0.01);
        estimator.observe(3, at(1_250));
        assert!((percent(&estimator) - 98.0).abs() < 0.01);
    }

    #[test]
    fn a_drainer_keeps_what_its_drain_needs() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        // More shots counted than it could pay: the drain would have stopped under 25.
        estimator.observe(3, drained(with(at(500), FP_DRAIN), [120.0; 3], 1_000));
        assert_eq!(bounds(&estimator), (20.0, 20.0));
        // Once it stops, nothing says it kept anything.
        estimator.observe(3, drained(at(550), [5.0; 3], 1_050));
        assert_eq!(bounds(&estimator), (15.0, 15.0));
    }

    #[test]
    fn the_pace_is_measured_from_the_local_pool() {
        let mut calibration = Calibration::default();
        assert_eq!(calibration.millis_per_point(), None);
        // A point every 150 ms, snapshots every 50 ms, from a pool of 10.
        let mut pool = 10;
        for step in 0..=120 {
            let time = step * 50;
            pool = 10 + time / 150;
            calibration.observe(time, pool, true);
        }
        assert!(pool < 100);
        let pace = calibration.millis_per_point().expect("enough points");
        assert!((pace - 150.0).abs() < 8.0, "{pace}");
    }

    #[test]
    fn spending_and_other_regeneration_are_not_counted() {
        let mut calibration = Calibration::default();
        // Not idle (a power on): the gain is not regeneration.
        for step in 0..200 {
            calibration.observe(step * 50, 10 + step / 2, false);
        }
        assert_eq!(calibration.millis_per_point(), None);
        // A pool already full cannot show a pace.
        let mut full = Calibration::default();
        for step in 0..200 {
            full.observe(step * 50, 100, true);
        }
        assert_eq!(full.millis_per_point(), None);
        // A long gap in snapshots is skipped.
        let mut gap = Calibration::default();
        gap.observe(0, 10, true);
        gap.observe(60_000, 90, true);
        assert_eq!(gap.millis_per_point(), None);
    }

    #[test]
    fn the_pool_never_leaves_its_range() {
        let mut estimator = Estimator::default();
        estimator.observe(3, at(0));
        for step in 1..20 {
            estimator.observe(3, with(at(step * 100), FP_LIGHTNING));
        }
        assert!(percent(&estimator) >= 0.0);
        assert_eq!(Estimator::default().ratio(40), None);
    }
}

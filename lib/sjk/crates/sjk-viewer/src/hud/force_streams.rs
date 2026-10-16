//! Force powers that act on other players shot by shot (drain, lightning and grip),
//! rebuilt from where everybody stands and looks, for the nameplates' estimates.
//!
//! Drain and lightning shoot every 50 ms while held (`WP_ForcePowerRun`'s
//! `FORCE_DEBOUNCE_TIME`; `ForceShootDrain`, `ForceShootLightning`, ported in
//! `sjk-game-jka`'s `force_dark.rs`). At level 3 a shot reaches every enemy whose box is
//! within reach of the caster's origin (512 units for drain, 300 for lightning), in
//! front of it (the middle of the box at most 60 degrees off the view) and in clear
//! sight; at levels 1 and 2 only the first player on a 2048-unit line along the view.
//!
//! - **Drain** takes 2, 3 or 4 Force from each victim by level, heals the drainer by as
//!   much (below its maximum) and holds back the victim's refill for 800 ms. It costs
//!   the drainer 5 a shot (at levels 1 and 2 only when the line found a player) and
//!   holds back its own refill for 500 ms.
//! - **Lightning** does 1 or 2 damage a shot, twice that two-handed (no weapon) at
//!   level 3; the armour takes it first. Its Force cost is the running cost
//!   ([`super::force_estimate`]). Each shot renews the victim's electrification to 800
//!   ms ahead when less than 400 ms is left, which every client is sent: a renewal
//!   proves a shot landed.
//! - An **absorb** that is up turns drain and lightning down by its level (what the
//!   levels' difference leaves) and gives the victim a point of Force a shot.
//! - **Grip** holds the first player on a 256-unit line from the gripper's eyes when
//!   it starts and does 2 damage past the armour then and every second after.
//!
//! Only `EV_FORCE_DRAINED`, at most every 400 ms a victim, names a drain victim, so the
//! shots are rebuilt with the world's walls in the way (not movers, nor other players
//! in the arc's line of sight). Other players' levels are not sent: the guess is level
//! 3's arc, a value's low bound takes the dearest level that reaches it and its high
//! bound the cheapest. The local player's own levels are its Force profile's.
//!
//! The local player's own drain is measured rather than rebuilt ([`Own`]): the server
//! sends it its pool, which each shot takes 5 from, and its health, which each shot
//! raises by what it took (below the maximum). So its victims lose what it gained,
//! whatever the server's rules, and the shots' pace and strength it shows are learnt
//! for everyone else's drain: servers built on the original game (JA+) shoot every
//! server frame rather than every 50 ms.
use glam::Vec3;
use sjk_game_jka::force_powers::{FP_ABSORB, FP_DRAIN, FP_GRIP, FP_LIGHTNING};

const CLIENTS: usize = 32;
/// A caster unseen this long starts its shots afresh.
const GAP_MILLIS: i32 = 1_500;
/// The arc's cone: the cosine of 60 degrees.
const ARC_COSINE: f32 = 0.5;
/// Drain: points a shot takes at levels 1 to 3 (index 0 unused), what a shot costs.
const TAKEN: [f32; 4] = [0.0, 2.0, 3.0, 4.0];
const SHOT_COST: f32 = 5.0;
/// How long a drain shot holds back the victim's and the drainer's refill.
const VICTIM_HOLD: i32 = 800;
const DRAINER_HOLD: i32 = 500;
/// Lightning's damage a shot: the least, the mean and the most.
const BOLT: (f32, f32, f32) = (1.0, 1.5, 2.0);
/// Grip: its reach, its damage and how often it is done.
const GRIP_REACH: f32 = 256.0;
const GRIP_DAMAGE: f32 = 2.0;
const GRIP_MILLIS: i32 = 1_000;
/// `WP_MELEE`: no weapon, both hands free for lightning.
const WP_MELEE: u8 = 2;
/// `CONTENTS_SOLID | CONTENTS_TERRAIN`: the world part of `MASK_SHOT`.
const WORLD_SHOT: u32 = 0x1 | 0x1000;
/// A player with no packed box (`SV_LinkEntity`) stands.
const STANDING: ([f32; 3], [f32; 3]) = ([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0]);

/// Shots and points seen before the learnt pace and strength are used.
const LEARNT_SHOTS: f32 = 8.0;

/// Bounds of a value, in [`super::estimate::Range`] order.
const LOW: usize = 0;
const BEST: usize = 1;
const HIGH: usize = 2;

/// A stream power's reach.
#[derive(Clone, Copy)]
struct Stream {
    power: usize,
    interval: i32,
    arc: f32,
}

const DRAIN: Stream = Stream {
    power: FP_DRAIN,
    interval: 50,
    arc: 512.0,
};
const LIGHTNING: Stream = Stream {
    power: FP_LIGHTNING,
    interval: 50,
    arc: 300.0,
};
const LINE_REACH: f32 = 2_048.0;

/// What one snapshot's shots did to a player, per bound of each estimate (the low
/// bound, the guess, the high bound): a loss is what the bound loses, so the low bound
/// loses the most, and a gain what it gains, so the low bound gains the least.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Effect {
    /// Force lost; negative when an absorb gave more back.
    pub(super) force: [f32; 3],
    /// Until when (server milliseconds) each bound's Force refill is held back.
    pub(super) hold: [i32; 3],
    /// Health gained by draining (only below the maximum).
    pub(super) heal: [f32; 3],
    /// Damage the armour takes first (lightning).
    pub(super) damage: [f32; 3],
    /// Damage past the armour (grip).
    pub(super) piercing: [f32; 3],
    /// A lightning shot surely landed (its electrification was renewed).
    pub(super) struck: bool,
}

impl Effect {
    fn lose(&mut self, bound: usize, points: f32, hold_until: i32) {
        self.force[bound] += points;
        self.hold[bound] = self.hold[bound].max(hold_until);
    }
}

/// One player as a shot sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Body {
    pub(super) number: u16,
    pub(super) origin: [f32; 3],
    /// View angles in degrees (pitch, yaw, roll).
    pub(super) view: [f32; 3],
    /// Box corners about the origin.
    pub(super) mins: [f32; 3],
    pub(super) maxs: [f32; 3],
    /// `forcePowersActive`.
    pub(super) active: u32,
    pub(super) weapon: u8,
    pub(super) team: i32,
    pub(super) duelling: bool,
    /// `electrifyTime` (`entityState_t::emplacedOwner`).
    pub(super) electrified: i32,
    /// The drain and lightning levels when known (the local player's), else `None`.
    pub(super) drain_level: Option<u8>,
    pub(super) lightning_level: Option<u8>,
    /// Its values are estimated: an alive player other than the local one.
    pub(super) estimated: bool,
}

impl Body {
    /// Box corners from the packed `entityState_t::solid` (`SV_LinkEntity`: bits 0-7
    /// the half width, 8-15 the depth below the origin, 16-23 the top plus 32).
    pub(super) fn unpack_box(solid: u32) -> ([f32; 3], [f32; 3]) {
        let (side, below, above) = (solid & 255, (solid >> 8) & 255, (solid >> 16) & 255);
        if solid == 0 || side == 0 || above == 0 {
            return STANDING;
        }
        let (side, below, above) = (side as f32, below as f32, above as f32 - 32.0);
        ([-side, -side, -below], [side, side, above])
    }

    /// The standing box, for a player whose box is not sent (the local player).
    pub(super) fn standing_box() -> ([f32; 3], [f32; 3]) {
        STANDING
    }

    fn uses(&self, power: usize) -> bool {
        self.active & (1 << power) != 0
    }

    fn level(&self, stream: Stream) -> Option<u8> {
        match stream.power {
            FP_DRAIN => self.drain_level,
            _ => self.lightning_level,
        }
        .map(|level| level.clamp(1, 3))
    }

    /// `r.absmin`, `r.absmax`: the box in the world, an epsilon wider.
    fn bounds(&self) -> (Vec3, Vec3) {
        let origin = Vec3::from_array(self.origin);
        (
            origin + Vec3::from_array(self.mins) - Vec3::ONE,
            origin + Vec3::from_array(self.maxs) + Vec3::ONE,
        )
    }

    /// The eyes: the view height is the box's top less 4 (36 standing, 12 crouched).
    fn eyes(&self) -> Vec3 {
        Vec3::from_array(self.origin) + Vec3::Z * (self.maxs[2] - 4.0)
    }

    fn forward(&self) -> Vec3 {
        let (pitch, yaw) = (self.view[0].to_radians(), self.view[1].to_radians());
        Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
        )
    }
}

/// The rules of the match that decide who can hit whom.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Rules {
    /// A team game: teammates are spared (`g_friendlyFire` off).
    pub(super) teams: bool,
}

/// The local player's pool and health, which the server sends it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Own {
    pub(super) number: u16,
    pub(super) pool: i32,
    pub(super) health: i32,
    pub(super) max_health: i32,
}

/// What the local player's pool and health say its drain did since the last snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Measured {
    /// Shots paid for (5 each).
    shots: f32,
    /// What they took, from the heal: `None` at the maximum, where nothing heals.
    taken: Option<f32>,
    elapsed: i32,
    /// The drain was on at the last snapshot too.
    continued: bool,
}

/// How the victims of a local drain were found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Found {
    /// In the shot's reach.
    Reach,
    /// Named by `EV_FORCE_DRAINED`.
    Event,
    /// None in reach: the player most in front.
    Front,
}

/// One drain by the local player from start to end, for the log.
#[derive(Clone, Copy, Debug)]
pub(super) struct Run {
    pub(super) millis: i32,
    pub(super) shots: f32,
    /// What it healed, when the health was under its maximum throughout.
    pub(super) healed: Option<f32>,
    /// What each player lost to it.
    pub(super) taken: [f32; CLIENTS],
    /// How often the victims were found each way: reach, event, front.
    pub(super) found: [u16; 3],
    /// The pace in use at its end, milliseconds a shot.
    pub(super) pace: f32,
}

/// Each caster's shots in progress.
#[derive(Clone, Copy, Debug, Default)]
struct Casting {
    /// When the next drain, lightning and grip shot are due, while each lasts.
    drain: Option<i32>,
    lightning: Option<i32>,
    grip: Option<i32>,
    /// Whom the grip holds.
    gripped: Option<u16>,
}

/// Follows every caster's shots and gives each player its [`Effect`] per snapshot.
pub(super) struct Tracker {
    casting: [Casting; CLIENTS],
    /// Each player's electrification when last seen.
    electrified: [i32; CLIENTS],
    effects: [Effect; CLIENTS],
    last_time: i32,
    /// The local player at the last snapshot, and whether it drained.
    own_before: Option<(i32, Own, bool)>,
    /// Learnt from the local player's drains: milliseconds and shots while draining,
    /// and points taken from a lone victim over the shots that took them, at a level.
    learnt_pace: (f32, f32),
    learnt_take: (f32, f32, u8),
    run: Option<Run>,
    finished: Option<Run>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            casting: [Casting::default(); CLIENTS],
            electrified: [0; CLIENTS],
            effects: [Effect::default(); CLIENTS],
            last_time: i32::MIN,
            own_before: None,
            learnt_pace: (0.0, 0.0),
            learnt_take: (0.0, 0.0, 3),
            run: None,
            finished: None,
        }
    }
}

impl Tracker {
    /// The local player's last finished drain, once.
    pub(super) fn take_finished(&mut self) -> Option<Run> {
        self.finished.take()
    }

    /// Milliseconds between drain and lightning shots: learnt from the local player's
    /// drains, else `FORCE_DEBOUNCE_TIME`'s 50.
    pub(super) fn pace(&self) -> f32 {
        let (millis, shots) = self.learnt_pace;
        if shots >= LEARNT_SHOTS {
            (millis / shots).clamp(10.0, 100.0)
        } else {
            DRAIN.interval as f32
        }
    }

    /// What a drain shot at `level` takes: as learnt from the local player's drains
    /// (scaled from its level), else the stock 2, 3 or 4.
    fn taken(&self, level: u8) -> f32 {
        let stock = TAKEN[usize::from(level.clamp(1, 3))];
        let (points, shots, learnt) = self.learnt_take;
        if shots >= LEARNT_SHOTS {
            let scale = points / shots / TAKEN[usize::from(learnt.clamp(1, 3))];
            stock * scale.clamp(0.25, 4.0)
        } else {
            stock
        }
    }

    /// What the local player's pool and health say about its drain since the last
    /// snapshot, and the run it belongs to.
    fn measure(&mut self, time: i32, own: Own, draining: bool) -> Option<Measured> {
        let before = self.own_before.replace((time, own, draining));
        if !draining {
            if let Some(mut run) = self.run.take() {
                run.pace = self.pace();
                self.finished = Some(run);
            }
            return None;
        }
        let (then, was, was_draining) = before?;
        if time - then > GAP_MILLIS || was.number != own.number {
            return None;
        }
        let spent = (was.pool - own.pool).max(0) as f32;
        let healed = own.health - was.health;
        let measured = Measured {
            shots: (spent / SHOT_COST).round(),
            taken: (was.health < was.max_health && healed >= 0).then_some(healed as f32),
            elapsed: time - then,
            continued: was_draining,
        };
        let run = self.run.get_or_insert(Run {
            millis: 0,
            shots: 0.0,
            healed: Some(0.0),
            taken: [0.0; CLIENTS],
            found: [0; 3],
            pace: 0.0,
        });
        run.millis += measured.elapsed;
        run.shots += measured.shots;
        run.healed = run
            .healed
            .zip(measured.taken)
            .map(|(sum, taken)| sum + taken);
        if measured.continued {
            self.learnt_pace.0 += measured.elapsed as f32;
            self.learnt_pace.1 += measured.shots;
        }
        Some(measured)
    }

    /// The local player's drain: the shots its pool paid for take what its health
    /// gained, from those in reach, else those the event names, else the player most in
    /// front.
    #[allow(clippy::too_many_arguments)]
    fn shoot_own(
        &mut self,
        caster: &Body,
        measured: Measured,
        bodies: &[Body],
        rules: Rules,
        named: u32,
        time: i32,
        sight: &mut impl FnMut([f32; 3], [f32; 3]) -> f32,
    ) -> u32 {
        if measured.shots <= 0.0 {
            return 0;
        }
        let level = caster.level(DRAIN).unwrap_or(3);
        let centre = Vec3::from_array(caster.origin);
        let forward = caster.forward();
        let line = first_on_line(caster, bodies, centre, forward, LINE_REACH, sight);
        let candidates = || {
            bodies.iter().filter(|victim| {
                victim.estimated
                    && victim.number != caster.number
                    && usize::from(victim.number) < CLIENTS
                    && reachable(caster, victim, rules)
            })
        };
        let mut victims = 0_u32;
        for victim in candidates() {
            let reached = if level == 3 {
                in_arc(victim, centre, forward, DRAIN.arc, sight)
            } else {
                line == Some(victim.number)
            };
            if reached {
                victims |= 1 << victim.number;
            }
        }
        let mut found = Found::Reach;
        if victims == 0 {
            found = Found::Event;
            victims = candidates().fold(0, |bits, victim| bits | (named & 1 << victim.number));
        }
        if victims == 0 {
            found = Found::Front;
            let front = candidates()
                .map(|victim| {
                    let (low, high) = victim.bounds();
                    let toward = ((low + high) * 0.5 - centre).normalize_or_zero();
                    (victim.number, toward.dot(forward))
                })
                .filter(|(_, facing)| *facing >= ARC_COSINE)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((number, _)) = front {
                victims = 1 << number;
            }
        }
        if victims == 0 {
            return 0;
        }
        let total = measured
            .taken
            .unwrap_or_else(|| measured.shots * self.taken(level));
        let each = total / victims.count_ones() as f32;
        if let Some(taken) = measured.taken
            && victims.count_ones() == 1
        {
            let (points, shots, _) = self.learnt_take;
            self.learnt_take = (points + taken, shots + measured.shots, level);
        }
        let run = self.run.as_mut();
        let mut taken = [0.0; CLIENTS];
        for (slot, taken) in taken.iter_mut().enumerate() {
            if victims & (1 << slot) != 0 {
                *taken = each;
                let effect = &mut self.effects[slot];
                for bound in [LOW, BEST, HIGH] {
                    effect.lose(bound, each, time + VICTIM_HOLD);
                }
            }
        }
        if let Some(run) = run {
            for (sum, taken) in run.taken.iter_mut().zip(taken) {
                *sum += taken;
            }
            run.found[found as usize] = run.found[found as usize].saturating_add(1);
        }
        victims
    }

    /// What the last observed snapshot's shots did to `slot`.
    pub(super) fn effect(&self, slot: u16) -> Effect {
        self.effects
            .get(usize::from(slot))
            .copied()
            .unwrap_or_default()
    }

    /// Take one snapshot at server `time`: every living player in it (`bodies`), the
    /// victims `EV_FORCE_DRAINED` named in it (bits by client), and `sight`, the
    /// fraction of a straight line between two points the world leaves clear.
    pub(super) fn observe(
        &mut self,
        time: i32,
        bodies: &[Body],
        rules: Rules,
        named: u32,
        own: Option<Own>,
        mut sight: impl FnMut([f32; 3], [f32; 3]) -> f32,
    ) {
        if time < self.last_time {
            // A new map or a restart.
            *self = Self::default();
        }
        if time == self.last_time {
            return;
        }
        let fresh = self.last_time == i32::MIN || time - self.last_time > GAP_MILLIS;
        self.last_time = time;
        self.effects = [Effect::default(); CLIENTS];
        // A caster out of view starts afresh when it comes back.
        let present = bodies
            .iter()
            .filter(|body| usize::from(body.number) < CLIENTS)
            .fold(0_u32, |bits, body| bits | 1 << body.number);
        for (slot, casting) in self.casting.iter_mut().enumerate() {
            if present & (1 << slot) == 0 || fresh {
                *casting = Casting::default();
            }
        }
        // The local player's drain, measured.
        let measured = own.and_then(|own| {
            let draining = bodies
                .iter()
                .any(|body| body.number == own.number && body.uses(FP_DRAIN));
            self.measure(time, own, draining)
        });
        let pace = self.pace().round() as i32;
        // The dearest and cheapest drain shot any drainer in sight can fire, for the
        // victims the event names.
        let mut floor: Option<[f32; 3]> = None;
        let mut lightning = false;
        // Victims the local player's measured drain has already accounted for.
        let mut measured_victims = 0_u32;
        for caster in bodies {
            let slot = usize::from(caster.number);
            if slot >= CLIENTS {
                continue;
            }
            let mut casting = self.casting[slot];
            let mut drain_shots = shots(&mut casting.drain, caster.uses(FP_DRAIN), pace, time);
            let bolts = shots(
                &mut casting.lightning,
                caster.uses(FP_LIGHTNING),
                pace,
                time,
            );
            let grips = self.grip(&mut casting, caster, bodies, rules, time, &mut sight);
            self.casting[slot] = casting;
            if let (Some(measured), Some(own)) = (measured, own)
                && own.number == caster.number
            {
                measured_victims =
                    self.shoot_own(caster, measured, bodies, rules, named, time, &mut sight);
                drain_shots = 0;
            }
            if caster.uses(FP_DRAIN) {
                let (levels, guess) = levels(caster.level(DRAIN));
                let price = |level: u8| self.taken(level);
                let shot = [
                    levels.clone().map(price).fold(0.0, f32::max),
                    price(guess),
                    levels.map(price).fold(f32::INFINITY, f32::min),
                ];
                floor = Some(match floor {
                    Some(f) => [
                        f[LOW].max(shot[LOW]),
                        f[BEST].max(shot[BEST]),
                        f[HIGH].min(shot[HIGH]),
                    ],
                    None => shot,
                });
            }
            lightning |= caster.uses(FP_LIGHTNING);
            if drain_shots > 0 {
                self.shoot(caster, DRAIN, bodies, rules, drain_shots, time, &mut sight);
            }
            if bolts > 0 {
                self.shoot(caster, LIGHTNING, bodies, rules, bolts, time, &mut sight);
            }
            if let (Some(victim), true) = (casting.gripped, grips > 0) {
                let effect = &mut self.effects[usize::from(victim)];
                for bound in [LOW, BEST, HIGH] {
                    effect.piercing[bound] += GRIP_DAMAGE * grips as f32;
                }
            }
        }
        let floor = floor.unwrap_or([TAKEN[3], TAKEN[3], TAKEN[1]]);
        for slot in 0..CLIENTS {
            if named & !measured_victims & (1 << slot) == 0 {
                continue;
            }
            let effect = &mut self.effects[slot];
            for bound in [LOW, BEST, HIGH] {
                effect.force[bound] = effect.force[bound].max(floor[bound]);
                effect.hold[bound] = effect.hold[bound].max(time + VICTIM_HOLD);
            }
        }
        // A renewed electrification while somebody casts lightning: a bolt landed.
        for body in bodies {
            let slot = usize::from(body.number);
            if slot >= CLIENTS {
                continue;
            }
            let renewed = body.electrified != self.electrified[slot] && body.electrified > time;
            self.electrified[slot] = body.electrified;
            if renewed && lightning && body.estimated {
                let effect = &mut self.effects[slot];
                effect.struck = true;
                if !body.uses(FP_ABSORB) {
                    let (least, _, _) = BOLT;
                    effect.damage[HIGH] = effect.damage[HIGH].max(least);
                    effect.damage[BEST] = effect.damage[BEST].max(least);
                    effect.damage[LOW] = effect.damage[LOW].max(least);
                }
            }
        }
    }

    /// The grip `caster` holds, if any: its victim (found on the line from the eyes
    /// when the grip starts, kept while it lasts) and how many blows are due.
    fn grip(
        &self,
        casting: &mut Casting,
        caster: &Body,
        bodies: &[Body],
        rules: Rules,
        time: i32,
        sight: &mut impl FnMut([f32; 3], [f32; 3]) -> f32,
    ) -> u32 {
        if !caster.uses(FP_GRIP) {
            casting.grip = None;
            casting.gripped = None;
            return 0;
        }
        let held = casting.gripped.and_then(|number| {
            bodies
                .iter()
                .find(|body| body.number == number && body.estimated)
        });
        if held.is_none() {
            casting.gripped = first_on_line(
                caster,
                bodies,
                caster.eyes(),
                caster.forward(),
                GRIP_REACH,
                sight,
            )
            .filter(|number| {
                bodies.iter().any(|body| {
                    body.number == *number && body.estimated && reachable(caster, body, rules)
                })
            });
            casting.grip = None;
        }
        if casting.gripped.is_none() {
            return 0;
        }
        shots(&mut casting.grip, true, GRIP_MILLIS, time)
    }

    /// `shots` shots of `caster`'s `stream` at the levels it may have it at.
    #[allow(clippy::too_many_arguments)]
    fn shoot(
        &mut self,
        caster: &Body,
        stream: Stream,
        bodies: &[Body],
        rules: Rules,
        shots: u32,
        time: i32,
        sight: &mut impl FnMut([f32; 3], [f32; 3]) -> f32,
    ) {
        let shots = shots as f32;
        let centre = Vec3::from_array(caster.origin);
        let forward = caster.forward();
        let line = first_on_line(caster, bodies, centre, forward, LINE_REACH, sight);
        let (levels, guess) = levels(caster.level(stream));
        let drain = stream.power == FP_DRAIN;
        let slot = usize::from(caster.number);
        if drain {
            // What the drainer pays at each bound: the dearest level, the guess, the
            // cheapest.
            let pays = |level: u8| level == 3 || line.is_some();
            let paid = [
                levels.clone().any(pays),
                pays(guess),
                levels.clone().all(pays),
            ];
            for (bound, paid) in paid.into_iter().enumerate() {
                if paid {
                    self.effects[slot].lose(bound, SHOT_COST * shots, time + DRAINER_HOLD);
                }
            }
        }
        let two_handed = caster.weapon == WP_MELEE;
        let learnt = [self.taken(1), self.taken(1), self.taken(2), self.taken(3)];
        let taken = |level: u8| learnt[usize::from(level.clamp(1, 3))];
        let mut heal = [0.0; 3];
        for victim in bodies {
            if !victim.estimated
                || victim.number == caster.number
                || usize::from(victim.number) >= CLIENTS
                || !reachable(caster, victim, rules)
            {
                continue;
            }
            let in_arc = in_arc(victim, centre, forward, stream.arc, sight);
            let on_line = line == Some(victim.number);
            let reaches = |level: u8| if level == 3 { in_arc } else { on_line };
            let absorbing = victim.uses(FP_ABSORB);
            // Per shot at `level`: what it does, at most, as guessed and at least, by the
            // victim's absorb level (1 to 3, the guess 3).
            let take = |level: u8| -> Option<[f32; 3]> {
                if !reaches(level) {
                    return None;
                }
                Some(if drain {
                    if absorbing {
                        // `WP_AbsorbConversion`: the levels' difference is taken, a
                        // point given back.
                        let most = f32::from(level.saturating_sub(1).min(2)) - 1.0;
                        let guess = f32::from(level.saturating_sub(3)) - 1.0;
                        [most, guess, -1.0]
                    } else {
                        [taken(level); 3]
                    }
                } else {
                    let double = if two_handed && level == 3 { 2.0 } else { 1.0 };
                    let (least, mean, most) = BOLT;
                    if absorbing {
                        // One point at most gets through.
                        let most = if level > 1 { double } else { 0.0 };
                        [most, 0.0, 0.0]
                    } else {
                        [most * double, mean * double, least * double]
                    }
                })
            };
            // The low bound takes the dearest level that reaches, the guess its level,
            // the high bound the cheapest (nothing when one misses).
            let dearest = levels
                .clone()
                .filter_map(take)
                .map(|per| per[LOW])
                .reduce(f32::max);
            let guessed = take(guess).map(|per| per[BEST]);
            let cheapest = levels
                .clone()
                .map(take)
                .try_fold(f32::INFINITY, |least, per| {
                    per.map(|per| least.min(per[HIGH]))
                });
            let per_bound = [dearest, guessed, cheapest];
            let effect = &mut self.effects[usize::from(victim.number)];
            for (bound, per_shot) in per_bound.into_iter().enumerate() {
                let Some(per_shot) = per_shot else {
                    continue;
                };
                if drain {
                    effect.lose(bound, per_shot * shots, time + VICTIM_HOLD);
                    // The drainer gains what was taken: its low bound the least.
                    heal[HIGH - bound] += per_shot.max(0.0) * shots;
                } else {
                    effect.damage[bound] += per_shot * shots;
                    if absorbing {
                        effect.force[bound] -= shots;
                    }
                }
            }
        }
        if drain {
            let effect = &mut self.effects[slot];
            for (bound, gained) in heal.into_iter().enumerate() {
                effect.heal[bound] += gained;
            }
        }
    }
}

/// Shots `interval` milliseconds apart due by `time` while `on`, starting with one when
/// it starts.
fn shots(next: &mut Option<i32>, on: bool, interval: i32, time: i32) -> u32 {
    if !on {
        *next = None;
        return 0;
    }
    match *next {
        Some(mut due) => {
            let mut count = 0;
            while due <= time {
                count += 1;
                due += interval;
            }
            *next = Some(due);
            count
        }
        None => {
            *next = Some(time + interval);
            1
        }
    }
}

/// The levels a caster may have a power at, and the guess.
fn levels(known: Option<u8>) -> (std::ops::RangeInclusive<u8>, u8) {
    match known {
        Some(level) => (level..=level, level),
        None => (1..=3, 3),
    }
}

/// `ForcePowerUsableOn` and `OnSameTeam`: no teammates in team games, and a duellist
/// only reaches its opponent (both must be duelling).
fn reachable(caster: &Body, victim: &Body, rules: Rules) -> bool {
    let teammates = rules.teams && matches!(caster.team, 1 | 2) && caster.team == victim.team;
    !teammates && caster.duelling == victim.duelling
}

/// Level 3's arc: the box within `radius`, its middle in the cone, in clear sight.
fn in_arc(
    victim: &Body,
    centre: Vec3,
    forward: Vec3,
    radius: f32,
    sight: &mut impl FnMut([f32; 3], [f32; 3]) -> f32,
) -> bool {
    let (absmin, absmax) = victim.bounds();
    let nearest = (absmin - centre).max(Vec3::ZERO) + (centre - absmax).max(Vec3::ZERO);
    if nearest.length() >= radius {
        return false;
    }
    let middle = (absmin + absmax) * 0.5;
    if (middle - centre).normalize_or_zero().dot(forward) < ARC_COSINE {
        return false;
    }
    sight(centre.to_array(), middle.to_array()) >= 1.0
}

/// The first player a line from `start` along `forward` meets within `reach`, before
/// the world stops it.
fn first_on_line(
    caster: &Body,
    bodies: &[Body],
    start: Vec3,
    forward: Vec3,
    reach: f32,
    sight: &mut impl FnMut([f32; 3], [f32; 3]) -> f32,
) -> Option<u16> {
    let (number, entry) = bodies
        .iter()
        .filter(|body| body.number != caster.number)
        .filter_map(|body| {
            let origin = Vec3::from_array(body.origin);
            let (low, high) = (
                origin + Vec3::from_array(body.mins),
                origin + Vec3::from_array(body.maxs),
            );
            ray_box(start, forward, low, high).map(|entry| (body.number, entry))
        })
        .filter(|(_, entry)| *entry <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let reach = start + forward * entry;
    (sight(start.to_array(), reach.to_array()) >= 1.0).then_some(number)
}

/// Where a ray from `start` along `direction` enters the box, as a distance; `None`
/// when it misses or starts inside (a trace that starts solid finds nothing).
fn ray_box(start: Vec3, direction: Vec3, low: Vec3, high: Vec3) -> Option<f32> {
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        let (s, d) = (start[axis], direction[axis]);
        if d.abs() < 1e-6 {
            if s < low[axis] || s > high[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = ((low[axis] - s) / d, (high[axis] - s) / d);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
    }
    (near > 0.0 && near <= far).then_some(near)
}

/// `sight` against `bsp`'s world: the clear fraction of the line.
pub(super) fn world_sight(
    bsp: &sjk_bsp::Bsp,
    scratch: &mut sjk_bsp::TraceScratch,
) -> impl FnMut([f32; 3], [f32; 3]) -> f32 {
    let point = sjk_bsp::Aabb::new([0.0; 3], [0.0; 3]).expect("an empty box is valid");
    move |start, end| {
        let trace = bsp.trace_box_with(scratch, start, end, point, WORLD_SHOT);
        if trace.start_solid || trace.all_solid {
            0.0
        } else {
            trace.fraction
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: u16 = 0;
    const CASTER: u16 = 1;
    const VICTIM: u16 = 2;

    fn body(number: u16, origin: [f32; 3], yaw: f32) -> Body {
        let (mins, maxs) = STANDING;
        Body {
            number,
            origin,
            view: [0.0, yaw, 0.0],
            mins,
            maxs,
            estimated: number != LOCAL,
            ..Body::default()
        }
    }

    fn draining(mut body: Body, level: Option<u8>) -> Body {
        body.active |= 1 << FP_DRAIN;
        body.drain_level = level;
        body
    }

    fn casting_lightning(mut body: Body, level: Option<u8>) -> Body {
        body.active |= 1 << FP_LIGHTNING;
        body.lightning_level = level;
        body
    }

    fn clear(_: [f32; 3], _: [f32; 3]) -> f32 {
        1.0
    }

    fn observe(tracker: &mut Tracker, time: i32, bodies: &[Body]) {
        tracker.observe(time, bodies, Rules::default(), 0, None, clear);
    }

    #[test]
    fn a_level_3_drain_takes_four_a_shot_from_everyone_in_the_arc() {
        let mut tracker = Tracker::default();
        let drainer = draining(body(CASTER, [0.0; 3], 0.0), None);
        // In front, 200 units off; another behind; a third far beyond the arc.
        let front = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        let behind = body(3, [-200.0, 0.0, 0.0], 0.0);
        let far = body(4, [900.0, 0.0, 0.0], 0.0);
        observe(&mut tracker, 1_000, &[drainer, front, behind, far]);
        let hit = tracker.effect(VICTIM);
        // The guess and the low bound take level 3's 4; the high bound may be level 2's
        // line, which this victim stands on, or level 1's 2.
        assert_eq!(hit.force, [4.0, 4.0, 2.0]);
        assert_eq!(hit.hold, [1_800; 3]);
        assert_eq!(tracker.effect(3), Effect::default());
        assert_eq!(
            tracker.effect(4),
            Effect::default(),
            "the line stops at the first"
        );
        // The drainer pays 5 a shot whatever its level (its line found somebody) and
        // heals by what it took: at least 2, as guessed 4.
        let drainer_effect = tracker.effect(CASTER);
        assert_eq!(drainer_effect.force, [5.0; 3]);
        assert_eq!(drainer_effect.hold, [1_500; 3]);
        assert_eq!(drainer_effect.heal, [2.0, 4.0, 4.0]);
        // Alone on the line beyond the arc, only levels 1 and 2 reach: the guess (3)
        // takes nothing, the low bound level 2's 3 and the high bound nothing.
        let mut tracker = Tracker::default();
        observe(&mut tracker, 1_000, &[drainer, far]);
        assert_eq!(tracker.effect(4).force, [3.0, 0.0, 0.0]);
        assert_eq!(tracker.effect(4).hold, [1_800, 0, 0]);
    }

    #[test]
    fn a_shot_every_50_ms_while_the_drain_lasts() {
        let mut tracker = Tracker::default();
        let drainer = draining(body(CASTER, [0.0; 3], 0.0), Some(3));
        let victim = body(VICTIM, [100.0, 50.0, 0.0], 0.0);
        let mut taken = 0.0;
        // Snapshots every 25 ms for half a second: a shot at the start, then every 50.
        for step in 0..=20 {
            observe(&mut tracker, 1_000 + step * 25, &[drainer, victim]);
            taken += tracker.effect(VICTIM).force[BEST];
        }
        assert_eq!(taken, 11.0 * 4.0);
        // It stops when the drain does.
        observe(&mut tracker, 1_525, &[body(CASTER, [0.0; 3], 0.0), victim]);
        assert_eq!(tracker.effect(VICTIM), Effect::default());
    }

    #[test]
    fn the_local_players_known_level_is_exact() {
        let mut tracker = Tracker::default();
        let local = draining(body(LOCAL, [0.0; 3], 90.0), Some(2));
        // Level 2 drains the line only: the victim straight ahead, not the one aside.
        let ahead = body(VICTIM, [0.0, 300.0, 0.0], 0.0);
        let aside = body(3, [100.0, 200.0, 0.0], 0.0);
        observe(&mut tracker, 2_000, &[local, ahead, aside]);
        assert_eq!(tracker.effect(VICTIM).force, [3.0; 3]);
        assert_eq!(tracker.effect(3), Effect::default());
    }

    #[test]
    fn walls_teammates_and_duels_stop_the_drain() {
        let drainer = draining(body(CASTER, [0.0; 3], 0.0), Some(3));
        let victim = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        let mut tracker = Tracker::default();
        tracker.observe(
            1_000,
            &[drainer, victim],
            Rules::default(),
            0,
            None,
            |_, _| 0.5,
        );
        assert_eq!(tracker.effect(VICTIM), Effect::default(), "a wall between");
        let teams = Rules { teams: true };
        let (mut red, mut also_red) = (drainer, victim);
        red.team = 1;
        also_red.team = 1;
        let mut tracker = Tracker::default();
        tracker.observe(1_000, &[red, also_red], teams, 0, None, clear);
        assert_eq!(tracker.effect(VICTIM), Effect::default(), "a teammate");
        let mut duellist = victim;
        duellist.duelling = true;
        let mut tracker = Tracker::default();
        observe(&mut tracker, 1_000, &[drainer, duellist]);
        assert_eq!(tracker.effect(VICTIM), Effect::default(), "in a duel");
    }

    #[test]
    fn an_absorb_gives_a_point_back() {
        let mut tracker = Tracker::default();
        let drainer = draining(body(CASTER, [0.0; 3], 0.0), Some(3));
        let mut victim = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        victim.active |= 1 << FP_ABSORB;
        observe(&mut tracker, 1_000, &[drainer, victim]);
        // Absorb 1 leaves 2 of level 3's drain (net 1); absorb 3 leaves none (net -1).
        assert_eq!(tracker.effect(VICTIM).force, [1.0, -1.0, -1.0]);
    }

    #[test]
    fn a_named_victim_lost_at_least_a_shot() {
        let mut tracker = Tracker::default();
        // The drainer looks away (or is out of view): the event alone tells.
        let drainer = draining(body(CASTER, [0.0; 3], 180.0), None);
        let victim = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        tracker.observe(
            1_000,
            &[drainer, victim],
            Rules::default(),
            1 << VICTIM,
            None,
            clear,
        );
        assert_eq!(tracker.effect(VICTIM).force, [4.0, 4.0, 2.0]);
        assert_eq!(tracker.effect(VICTIM).hold, [1_800; 3]);
        let mut tracker = Tracker::default();
        tracker.observe(1_000, &[victim], Rules::default(), 1 << VICTIM, None, clear);
        assert_eq!(tracker.effect(VICTIM).force, [4.0, 4.0, 2.0]);
    }

    #[test]
    fn lightning_strikes_the_arc_twice_as_hard_two_handed() {
        let mut tracker = Tracker::default();
        let caster = casting_lightning(body(CASTER, [0.0; 3], 0.0), None);
        let near = body(VICTIM, [150.0, 0.0, 0.0], 0.0);
        // Beyond lightning's 300 but within drain's 512: the line only.
        let far = body(3, [400.0, 60.0, 0.0], 0.0);
        observe(&mut tracker, 1_000, &[caster, near, far]);
        // Level 3 (guess) or 2 hits it on the line, so the high bound takes a bolt's least.
        assert_eq!(tracker.effect(VICTIM).damage, [2.0, 1.5, 1.0]);
        assert_eq!(tracker.effect(3), Effect::default());
        // No Force is taken, and the caster pays through its running cost, not here.
        assert_eq!(tracker.effect(VICTIM).force, [0.0; 3]);
        assert_eq!(tracker.effect(CASTER), Effect::default());
        let mut bare = caster;
        bare.weapon = WP_MELEE;
        bare.lightning_level = Some(3);
        let mut tracker = Tracker::default();
        observe(&mut tracker, 1_000, &[bare, near]);
        assert_eq!(tracker.effect(VICTIM).damage, [4.0, 3.0, 2.0]);
        // Absorbed: at most a point through, and a point of Force back a shot.
        let mut absorbing = near;
        absorbing.active |= 1 << FP_ABSORB;
        let mut tracker = Tracker::default();
        observe(&mut tracker, 1_000, &[caster, absorbing]);
        assert_eq!(tracker.effect(VICTIM).damage, [1.0, 0.0, 0.0]);
        assert_eq!(tracker.effect(VICTIM).force, [-1.0; 3]);
    }

    #[test]
    fn a_renewed_electrification_proves_a_bolt() {
        let mut tracker = Tracker::default();
        // The caster looks away: only the electrification tells.
        let caster = casting_lightning(body(CASTER, [0.0; 3], 180.0), None);
        let mut victim = body(VICTIM, [150.0, 0.0, 0.0], 0.0);
        observe(&mut tracker, 1_000, &[caster, victim]);
        assert!(!tracker.effect(VICTIM).struck);
        victim.electrified = 1_850;
        observe(&mut tracker, 1_050, &[caster, victim]);
        let struck = tracker.effect(VICTIM);
        assert!(struck.struck);
        assert_eq!(struck.damage, [1.0; 3]);
        // Unchanged next time: nothing new.
        observe(&mut tracker, 1_100, &[caster, victim]);
        assert!(!tracker.effect(VICTIM).struck);
        // Electrified with nobody casting lightning (a DEMP2): not a bolt.
        let mut tracker = Tracker::default();
        let idle = body(CASTER, [0.0; 3], 0.0);
        observe(
            &mut tracker,
            1_000,
            &[idle, body(VICTIM, [150.0, 0.0, 0.0], 0.0)],
        );
        observe(&mut tracker, 1_050, &[idle, victim]);
        assert!(!tracker.effect(VICTIM).struck);
    }

    #[test]
    fn a_grip_holds_who_was_in_front_and_hurts_two_a_second() {
        let mut tracker = Tracker::default();
        let mut gripper = body(CASTER, [0.0; 3], 0.0);
        gripper.active |= 1 << FP_GRIP;
        let held = body(VICTIM, [100.0, 0.0, 0.0], 0.0);
        let aside = body(3, [100.0, 80.0, 0.0], 0.0);
        let mut taken = 0.0;
        for step in 0..=25 {
            observe(&mut tracker, 1_000 + step * 100, &[gripper, held, aside]);
            taken += tracker.effect(VICTIM).piercing[BEST];
            assert_eq!(tracker.effect(3), Effect::default());
        }
        // Blows at 1.0, 2.0 and 3.0 s.
        assert_eq!(taken, 6.0);
        // Turning away keeps the hold (the server lets go, and the power goes off).
        gripper.view[1] = 90.0;
        observe(&mut tracker, 4_000, &[gripper, held, aside]);
        assert_eq!(tracker.effect(VICTIM).piercing, [2.0; 3]);
    }

    #[test]
    fn the_packed_box_unpacks() {
        let standing = (72 << 16) | (24 << 8) | 15;
        assert_eq!(
            Body::unpack_box(standing),
            ([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0])
        );
        let crouching = (48 << 16) | (24 << 8) | 15;
        assert_eq!(Body::unpack_box(crouching).1[2], 16.0);
        assert_eq!(Body::unpack_box(0), STANDING);
    }

    #[test]
    fn the_line_meets_the_nearest_box() {
        let start = Vec3::ZERO;
        let x = Vec3::X;
        let near = ray_box(
            start,
            x,
            Vec3::new(100.0, -15.0, -24.0),
            Vec3::new(130.0, 15.0, 40.0),
        );
        assert_eq!(near, Some(100.0));
        assert_eq!(
            ray_box(
                start,
                x,
                Vec3::new(100.0, 20.0, -24.0),
                Vec3::new(130.0, 50.0, 40.0)
            ),
            None
        );
        assert_eq!(
            ray_box(
                start,
                x,
                Vec3::new(-10.0, -10.0, -10.0),
                Vec3::new(10.0, 10.0, 10.0)
            ),
            None,
            "starts inside"
        );
    }

    fn own(pool: i32, health: i32) -> Option<Own> {
        Some(Own {
            number: LOCAL,
            pool,
            health,
            max_health: 100,
        })
    }

    #[test]
    fn your_drain_takes_what_your_health_gained() {
        let mut tracker = Tracker::default();
        let idle = body(LOCAL, [0.0; 3], 0.0);
        let local = draining(idle, Some(3));
        let victim = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        let rules = Rules::default();
        tracker.observe(1_000, &[idle, victim], rules, 0, own(100, 50), clear);
        // Two shots paid (10) between snapshots healed 12: the victim lost 12.
        tracker.observe(1_025, &[local, victim], rules, 0, own(90, 62), clear);
        assert_eq!(tracker.effect(VICTIM).force, [12.0; 3]);
        assert_eq!(tracker.effect(VICTIM).hold, [1_825; 3]);
        // Nothing paid: no shot landed in between.
        tracker.observe(1_050, &[local, victim], rules, 0, own(90, 62), clear);
        assert_eq!(tracker.effect(VICTIM), Effect::default());
        // Healed up to the maximum: what the heal shows.
        tracker.observe(1_075, &[local, victim], rules, 0, own(80, 100), clear);
        assert_eq!(tracker.effect(VICTIM).force, [38.0; 3]);
        // At the maximum nothing heals: the shots paid take the stock 4 each.
        tracker.observe(1_100, &[local, victim], rules, 0, own(70, 100), clear);
        assert_eq!(tracker.effect(VICTIM).force, [8.0; 3]);
        // The drain ends: its run is told once.
        tracker.observe(1_125, &[idle, victim], rules, 0, own(70, 100), clear);
        let run = tracker.take_finished().expect("a run");
        assert_eq!(run.shots, 6.0);
        assert_eq!(run.taken[usize::from(VICTIM)], 12.0 + 38.0 + 8.0);
        assert_eq!(run.healed, None, "it reached the maximum");
        assert_eq!(run.found, [3, 0, 0]);
        assert!(tracker.take_finished().is_none());
    }

    #[test]
    fn a_victim_out_of_reach_is_found_by_the_event_or_in_front() {
        let rules = Rules::default();
        let idle = body(LOCAL, [0.0; 3], 0.0);
        let local = draining(idle, Some(3));
        // Behind a wall the rebuilt shot misses; the event names the victim.
        let victim = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        let wall = |_: [f32; 3], _: [f32; 3]| 0.5;
        let mut tracker = Tracker::default();
        tracker.observe(1_000, &[idle, victim], rules, 0, own(100, 50), wall);
        tracker.observe(
            1_050,
            &[local, victim],
            rules,
            1 << VICTIM,
            own(95, 54),
            wall,
        );
        assert_eq!(tracker.effect(VICTIM).force[BEST], 4.0);
        // No event either: the one most in front.
        let mut tracker = Tracker::default();
        tracker.observe(1_000, &[idle, victim], rules, 0, own(100, 50), wall);
        tracker.observe(1_050, &[local, victim], rules, 0, own(95, 54), wall);
        assert_eq!(tracker.effect(VICTIM).force[BEST], 4.0);
        tracker.observe(1_075, &[idle, victim], rules, 0, own(95, 54), wall);
        assert_eq!(tracker.take_finished().expect("a run").found, [0, 0, 1]);
    }

    #[test]
    fn your_drains_teach_the_pace_and_strength_for_everyone() {
        let rules = Rules::default();
        let idle = body(LOCAL, [0.0; 3], 0.0);
        let local = draining(idle, Some(3));
        let victim = body(VICTIM, [200.0, 0.0, 0.0], 0.0);
        let mut tracker = Tracker::default();
        assert_eq!(tracker.pace(), 50.0);
        // A server shooting every 25 ms (a frame), 6 a shot.
        let (mut pool, mut health) = (100, 10);
        tracker.observe(1_000, &[idle, victim], rules, 0, own(pool, health), clear);
        for step in 1..=10 {
            pool -= 5;
            health += 6;
            tracker.observe(
                1_000 + step * 25,
                &[local, victim],
                rules,
                0,
                own(pool, health),
                clear,
            );
        }
        assert_eq!(tracker.pace(), 25.0);
        assert_eq!(tracker.taken(3), 6.0);
        assert_eq!(tracker.taken(1), 3.0);
        // Another drainer now shoots at that pace and strength.
        let other = draining(body(CASTER, [0.0, 400.0, 0.0], 0.0), Some(3));
        let near = body(3, [200.0, 400.0, 0.0], 0.0);
        let mut taken = 0.0;
        for step in 0..4 {
            tracker.observe(
                2_000 + step * 25,
                &[idle, other, near],
                rules,
                0,
                own(100, 100),
                clear,
            );
            taken += tracker.effect(3).force[BEST];
        }
        assert_eq!(taken, 4.0 * 6.0);
    }
}

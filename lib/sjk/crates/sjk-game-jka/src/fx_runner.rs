//! A map's effect runners (`fx_runner`, `g_misc.c:2292-2549`): a named effect file played
//! at a place — continuously, toggled on and off by name, or once per use — with a
//! soundset loop while it runs and, for spawnflag 4, a little radius damage at every
//! think (ffa2's torches burn whoever stands in them).
//!
//! - **Spawn** ([`FxRunner::spawn`], `SP_fx_runner`): the keys read, the default aim up
//!   (`-90 0 0`), the entity linked with a ±32 box and linked up 400 ms on.
//! - **Link** ([`FxRunner::link`], `fx_runner_link`): the aim at the `target`'s origin,
//!   the start 200 ms on unless it starts off or runs once, the soundset loop.
//! - **Think** ([`FxRunner::think`], `fx_runner_think`): the continuous state, the next
//!   think `delay` + `random` on, the damage and the `target2` it asks for.
//! - **Use** ([`FxRunner::use_runner`], `fx_runner_use`): the one-shot counter clients
//!   replay the effect by, or the toggle with its start and stop sounds.
//!
//! The runner keeps names — its effect file and its soundset — not indices: the legacy
//! projection ([`FxRunner::project`]) is handed the indices the server registered them at
//! (`G_EffectIndex`, `G_SoundSetIndex`). Held to `tools/game-oracle/fxrunner.c`
//! (`game-fxrunner.txt`).

use crate::player_death::Rng;
use sjk_entity::Entity;
use sjk_protocol::EntityState;

/// `ET_FX`.
pub const ET_FX: u32 = 17;
/// `FX_STATE_OFF`, `FX_STATE_ONE_SHOT`, `FX_STATE_ONE_SHOT_LIMIT`, `FX_STATE_CONTINUOUS`
/// (`bg_public.h:1236-1239`): what `s.modelindex2` tells a client to do.
pub const FX_STATE_OFF: u32 = 0;
const FX_STATE_ONE_SHOT: u32 = 1;
const FX_STATE_ONE_SHOT_LIMIT: u32 = 10;
pub const FX_STATE_CONTINUOUS: u32 = 20;
/// `BMS_START`, `BMS_MID`, `BMS_END` (`g_mover.c:55-57`): a soundset's start, loop and
/// stop sounds.
pub const BMS_START: u32 = 0;
pub const BMS_MID: u32 = 1;
pub const BMS_END: u32 = 2;
/// `FX_ENT_RADIUS`: the box a runner is linked by.
pub const FX_ENT_RADIUS: f32 = 32.0;
/// Spawnflags: `STARTOFF`, `ONESHOT`, `DAMAGE`.
const STARTOFF: u32 = 1;
const ONESHOT: u32 = 2;
const DAMAGE: u32 = 4;
/// `MOD_UNKNOWN`: what the runner's burn kills with.
pub const MOD_UNKNOWN: u32 = 0;

/// The wire fields a runner's projection writes (`msg.cpp`'s entity fields).
mod es {
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const APOS_BASE: [usize; 3] = [5, 3, 33];
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const ANGLES: [usize; 3] = [25, 9, 24];
    pub const TYPE: usize = 8;
    pub const SPEED: usize = 31;
    pub const MODEL: usize = 46;
    pub const MODEL2: usize = 41;
    pub const LOOP_SOUND: usize = 55;
    pub const TIME: usize = 65;
    pub const LOOP_IS_SOUNDSET: usize = 70;
    pub const SOUND_SET: usize = 78;
}

/// What the runner will do at its next think (`think`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxThink {
    /// `fx_runner_link`.
    Link,
    /// `fx_runner_think`.
    Run,
}

/// An effect runner of the map.
#[derive(Clone, Debug, PartialEq)]
pub struct FxRunner {
    /// `fxFile`: the effect a client plays, by name.
    pub effect: String,
    /// `soundSet`: the soundset whose loop runs while it plays; empty for none.
    pub sound_set: String,
    /// `s.origin` (and the stationary trajectory's base, `G_SetOrigin`).
    pub origin: [f32; 3],
    /// `s.angles`: where the effect points.
    pub angles: [f32; 3],
    /// `s.apos.trBase`: the angles `G_SetAngles` set at the link; zero before it.
    pub apos: [f32; 3],
    /// `delay` (ms between thinks) and `random` (ms of spread added to it).
    pub delay: i32,
    pub random: f32,
    /// `splashDamage`, `splashRadius`: the burn of spawnflag 4.
    pub splash_damage: i32,
    pub splash_radius: i32,
    pub spawnflags: u32,
    /// `target` (the aim), `target2` (fired at every think), `targetname` (its name).
    pub target: String,
    pub target2: String,
    pub targetname: String,
    /// `think` and `nextthink` (-1: never, until used).
    pub think: FxThink,
    pub next_think: i32,
    /// `s.modelindex2`: off, a one-shot's counter, or continuous.
    pub state: u32,
    /// `s.loopSound` = `BMS_MID` with `loopIsSoundset`: the soundset's loop plays.
    pub looping: bool,
    /// `use`: whether a name reaches it (set by the link, for a runner with a name).
    pub usable: bool,
}

/// What a think asks of the world beyond the runner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thought {
    /// `G_RadiusDamage(origin, self, damage, radius, self, self, MOD_UNKNOWN)`: the burn.
    pub damage: Option<Splash>,
    /// `G_UseTargets2(self, self, target2)`, fired this many times (a one-shot's use
    /// fires it from the think and again itself).
    pub fired: u8,
}

/// A runner's radius damage: where, how much at the middle, how far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Splash {
    pub origin: [f32; 3],
    pub damage: i32,
    pub radius: i32,
}

/// What a use did: its think, if one ran, and the `EV_BMODEL_SOUND` it raised on
/// itself (`BMS_START` or `BMS_END`), if any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Used {
    pub thought: Thought,
    pub sound: Option<u32>,
}

/// Why a map's `fx_runner` does not spawn.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// `"ERROR: fx_runner %s at %s has no fxFile specified"`: freed at once.
    NoEffectFile {
        targetname: String,
        origin: [f32; 3],
    },
}

impl FxRunner {
    /// `SP_fx_runner` for a map entity spawned at `level_time`: the keys and their
    /// defaults (`delay` 200, `random` 0, `splashRadius` 16, `splashDamage` 5), the aim up
    /// unless the map gave angles, and the link 400 ms on.
    pub fn spawn(entity: &Entity, level_time: i32) -> Result<Self, Refused> {
        let text = |key: &str| entity.get(key).unwrap_or_default().to_owned();
        let int = |key: &str, default: i32| {
            entity
                .get(key)
                .map_or(default, |value| crate::userinfo::atoi(value.as_bytes()))
        };
        let origin = vector(entity, "origin");
        if text("fxFile").is_empty() {
            return Err(Refused::NoEffectFile {
                targetname: text("targetname"),
                origin,
            });
        }
        let mut angles = spawn_angles(entity);
        if angles == [0.0; 3] {
            angles = [-90.0, 0.0, 0.0];
        }
        Ok(Self {
            effect: text("fxFile"),
            sound_set: text("soundSet"),
            origin,
            angles,
            apos: [0.0; 3],
            delay: int("delay", 200),
            random: entity
                .get("random")
                .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes())),
            splash_damage: int("splashDamage", 5),
            splash_radius: int("splashRadius", 16),
            spawnflags: int("spawnflags", 0) as u32,
            target: text("target"),
            target2: text("target2"),
            targetname: text("targetname"),
            think: FxThink::Link,
            next_think: level_time + 400,
            state: FX_STATE_OFF,
            looping: false,
            usable: false,
        })
    }

    /// `fx_runner_link` at `level_time`, `aim` being the origin of the first standing
    /// entity called `target` (`G_Find`), if there is one. Returns whether a target was
    /// named and not found (the reference warns and keeps the aim up).
    pub fn link(&mut self, aim: Option<[f32; 3]>, level_time: i32) -> bool {
        let mut missing = false;
        if !self.target.is_empty() {
            match aim {
                Some(at) => {
                    let mut direction = std::array::from_fn(|axis| at[axis] - self.origin[axis]);
                    crate::player_angle_math::normalize(&mut direction);
                    self.angles = crate::player_angle_math::vector_angles(direction);
                }
                None => missing = true,
            }
        }
        // `G_SetAngles`.
        self.apos = self.angles;
        if self.spawnflags & (STARTOFF | ONESHOT) != 0 {
            self.next_think = -1;
        } else {
            if !self.sound_set.is_empty() {
                self.looping = true;
            }
            self.think = FxThink::Run;
            self.next_think = level_time + 200;
        }
        self.usable = !self.targetname.is_empty();
        missing
    }

    /// `fx_runner_think` at `level_time`: continuous from now, the next think `delay`
    /// plus up to `random` on (`Q_flrand` drawn every time), the burn for spawnflag 4,
    /// `target2`, and the soundset loop for a runner that is not a one-shot.
    pub fn think(&mut self, level_time: i32, rng: &mut Rng) -> Thought {
        self.state = FX_STATE_CONTINUOUS;
        // `level.time + ent->delay + Q_flrand(0.0f, 1.0f) * ent->random`: summed in float.
        self.next_think =
            ((level_time + self.delay) as f32 + rng.flrand(0.0, 1.0) * self.random) as i32;
        let damage = (self.spawnflags & DAMAGE != 0).then_some(Splash {
            origin: self.origin,
            damage: self.splash_damage,
            radius: self.splash_radius,
        });
        let fired = u8::from(!self.target2.is_empty());
        if self.spawnflags & ONESHOT == 0 && !self.looping && !self.sound_set.is_empty() {
            self.looping = true;
        }
        Thought { damage, fired }
    }

    /// Whether the runner thinks at `level_time` (`G_RunThink`: a `nextthink` above zero
    /// that has come).
    pub fn due(&self, level_time: i32) -> bool {
        self.next_think > 0 && self.next_think <= level_time
    }

    /// `G_RunThink` for the runner at `level_time`: the link, or a think.
    pub fn run(
        &mut self,
        level_time: i32,
        rng: &mut Rng,
        aim: impl FnOnce(&str) -> Option<[f32; 3]>,
    ) -> (Thought, bool) {
        self.next_think = 0;
        match self.think {
            FxThink::Link => {
                let at = if self.target.is_empty() {
                    None
                } else {
                    aim(&self.target)
                };
                (Thought::default(), self.link(at, level_time))
            }
            FxThink::Run => (self.think(level_time, rng), false),
        }
    }

    /// `fx_runner_use` at `level_time`. A one-shot plays once more — its counter moves on,
    /// wrapping past `FX_STATE_ONE_SHOT_LIMIT` to `FX_STATE_ONE_SHOT` — and never thinks
    /// on its own; any other runner toggles, thinking at once when turned on.
    pub fn use_runner(&mut self, level_time: i32, rng: &mut Rng) -> Used {
        let with_set = !self.sound_set.is_empty();
        if self.spawnflags & ONESHOT != 0 {
            let counter = self.state + 1;
            let mut thought = self.think(level_time, rng);
            self.next_think = -1;
            self.state = if counter > FX_STATE_ONE_SHOT_LIMIT {
                FX_STATE_ONE_SHOT
            } else {
                counter
            };
            thought.fired += u8::from(!self.target2.is_empty());
            return Used {
                thought,
                sound: with_set.then_some(BMS_START),
            };
        }
        self.think = FxThink::Run;
        if self.next_think == -1 {
            let thought = self.think(level_time, rng);
            if with_set {
                self.looping = true;
            }
            Used {
                thought,
                sound: with_set.then_some(BMS_START),
            }
        } else {
            self.next_think = -1;
            self.state = FX_STATE_OFF;
            if with_set {
                self.looping = false;
            }
            Used {
                thought: Thought::default(),
                sound: with_set.then_some(BMS_END),
            }
        }
    }

    /// The runner as protocol 26 carries it, `effect` and `sound_set` being the indices
    /// its names were registered at: `ET_FX`, the effect, the state, `delay` as `speed`
    /// and `random` as `time`, the stationary place and aim, and the loop. The event
    /// fields are the caller's (`G_AddEvent`'s sequence).
    pub fn project(&self, state: &mut EntityState, effect: u16, sound_set: u16) {
        state.set_raw_field(es::TYPE, ET_FX);
        state.set_raw_field(es::MODEL, u32::from(effect));
        state.set_raw_field(es::MODEL2, self.state);
        state.set_raw_field(es::SPEED, (self.delay as f32).to_bits());
        state.set_raw_field(es::TIME, self.random as i32 as u32);
        for axis in 0..3 {
            state.set_raw_field(es::ORIGIN[axis], self.origin[axis].to_bits());
            state.set_raw_field(es::POS_BASE[axis], self.origin[axis].to_bits());
            state.set_raw_field(es::ANGLES[axis], self.angles[axis].to_bits());
            state.set_raw_field(es::APOS_BASE[axis], self.apos[axis].to_bits());
        }
        state.set_raw_field(es::LOOP_SOUND, if self.looping { BMS_MID } else { 0 });
        state.set_raw_field(es::LOOP_IS_SOUNDSET, u32::from(self.looping));
        state.set_raw_field(es::SOUND_SET, u32::from(sound_set));
    }

    /// `r.mins`, `r.maxs`.
    pub fn bounds() -> ([f32; 3], [f32; 3]) {
        ([-FX_ENT_RADIUS; 3], [FX_ENT_RADIUS; 3])
    }
}

/// A spawn vector key (`sscanf "%f %f %f"`), zero when absent or unreadable.
pub fn vector(entity: &Entity, key: &str) -> [f32; 3] {
    entity.vector(key).ok().flatten().unwrap_or([0.0; 3])
}

/// `s.angles` as `G_ParseField` leaves them: `angles` (`F_VECTOR`) or `angle`
/// (`F_ANGLEHACK`: a yaw), whichever the lump gives last.
pub fn spawn_angles(entity: &Entity) -> [f32; 3] {
    let last =
        entity.fields().iter().rev().find(|(key, _)| {
            key.eq_ignore_ascii_case("angles") || key.eq_ignore_ascii_case("angle")
        });
    match last {
        Some((key, _)) if key.eq_ignore_ascii_case("angles") => vector(entity, "angles"),
        Some((_, value)) => [0.0, crate::text_parse::atof(value.as_bytes()), 0.0],
        None => [0.0; 3],
    }
}

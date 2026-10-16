//! The movers that are not doors (OpenJK `codemp/game/g_mover.c`): the train that rides
//! its `path_corner`s (`func_train`, ffa5's lift), the bobbing and swinging brushes
//! (`func_bobbing`, duel1; `func_pendulum`), the spinning one (`func_rotating`) and the brush
//! that just stands there (`func_static`).
//!
//! | Reference | Here |
//! | --- | --- |
//! | `InitMover`, `InitMoverTrData`, `SetMoverState` | [`PathMover::spawn`], [`PathMover::set_state`] |
//! | `SP_func_train`, `SP_path_corner`, `Think_SetupTrainTargets`, `Reached_Train`, `Think_BeginMoving` | [`link_corners`], [`PathMover::setup_train`], [`PathMover::reached_train`], [`PathMover::think`] |
//! | `SP_func_bobbing`, `SP_func_pendulum`, `SP_func_rotating`, `SP_func_static`, `func_static_use` | [`PathMover::spawn`], [`PathMover::use_static`] |
//! | `G_MoverTeam`'s move and `G_MoverPush`/`G_TryPushingEntity` (rotation included) | [`PathMover::travel`], [`push`] |
//! | `BG_EvaluateTrajectory` (the server's own arithmetic) | [`Trajectory::evaluate`] |
//!
//! A mover is one of the few entities a client draws and predicts nothing of: the server
//! owns where it is, so what [`PathMover::project`] writes is what a player sees.

use sjk_entity::Entity;
use sjk_protocol::EntityState;

/// The wire fields a mover's projection writes (`msg.cpp`'s entity fields).
mod es {
    /// One trajectory's fields.
    pub struct Fields {
        pub kind: usize,
        pub time: usize,
        pub duration: usize,
        pub base: [usize; 3],
        pub delta: [usize; 3],
    }
    pub const POS: Fields = Fields {
        kind: 23,
        time: 0,
        duration: 20,
        base: [2, 1, 4],
        delta: [6, 7, 10],
    };
    pub const APOS: Fields = Fields {
        kind: 15,
        time: 34,
        duration: 89,
        base: [5, 3, 33],
        delta: [48, 44, 49],
    };
    pub const TYPE: usize = 8;
    pub const EFLAGS: usize = 19;
    pub const SPEED: usize = 31;
    pub const MODEL2: usize = 41;
    pub const LOOP_SOUND: usize = 55;
    pub const CONSTANT_LIGHT: usize = 64;
    pub const LOOP_IS_SOUNDSET: usize = 70;
    pub const MODEL_SCALE: usize = 76;
    pub const SOUND_SET: usize = 78;
    pub const FRAME: usize = 83;
    pub const EFLAGS2: usize = 96;
}

/// `trType_t`.
pub const TR_STATIONARY: u32 = 0;
pub const TR_LINEAR: u32 = 2;
pub const TR_LINEAR_STOP: u32 = 3;
pub const TR_NONLINEAR_STOP: u32 = 4;
pub const TR_SINE: u32 = 5;
/// `ET_MOVER`.
pub const ET_MOVER: u32 = 6;
/// `EF_SHADER_ANIM` and `EF_RADAROBJECT`, `EF2_HYPERSPACE`.
pub const EF_SHADER_ANIM: u32 = 1 << 4;
pub const EF_RADAROBJECT: u32 = 1 << 26;
pub const EF2_HYPERSPACE: u32 = 1 << 3;
/// `BMS_START`, `BMS_MID`, `BMS_END`: a mover's soundset sounds.
pub const BMS_START: u32 = 0;
pub const BMS_MID: u32 = 1;
pub const BMS_END: u32 = 2;
/// `FRAMETIME`: trains start on the level's second frame.
const FRAMETIME: i32 = 100;
/// `TRAIN_START_ON`, `TRAIN_BLOCK_STOPS`, and the `CRUSH_THROUGH` flag `G_MoverPush`
/// reads on any pusher (32).
const TRAIN_START_ON: u32 = 1;
const TRAIN_BLOCK_STOPS: u32 = 4;
pub const CRUSH_THROUGH: u32 = 32;
/// `func_rotating`'s IMPACT (16): kills whatever it touches while turning.
pub const ROTATING_IMPACT: u32 = 16;
/// `MOVER_INACTIVE` (128) and `MOVER_PLAYER_USE` (64), which `InitMover` reads.
const MOVER_PLAYER_USE: u32 = 64;
const MOVER_INACTIVE: u32 = 128;

/// `trajectory_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trajectory {
    pub kind: u32,
    pub time: i32,
    pub duration: i32,
    pub base: [f32; 3],
    pub delta: [f32; 3],
}

impl Trajectory {
    /// `BG_EvaluateTrajectory` (`bg_misc.c:2227-2284`) with the game module's own
    /// arithmetic: the double constants and the float rounding where the C code has them.
    pub fn evaluate(&self, at_time: i32) -> [f32; 3] {
        let ma = |scale: f32| -> [f32; 3] {
            std::array::from_fn(|axis| self.base[axis] + scale * self.delta[axis])
        };
        match self.kind {
            TR_LINEAR => ma((f64::from(at_time - self.time) * 0.001) as f32),
            TR_SINE => {
                let delta_time = (at_time - self.time) as f32 / self.duration as f32;
                ma((f64::from(delta_time) * std::f64::consts::PI * 2.0).sin() as f32)
            }
            TR_LINEAR_STOP => {
                let at_time = at_time.min(self.time + self.duration);
                let delta_time = (f64::from(at_time - self.time) * 0.001) as f32;
                ma(delta_time.max(0.0))
            }
            TR_NONLINEAR_STOP => {
                let at_time = at_time.min(self.time + self.duration);
                let elapsed = at_time - self.time;
                let delta_time = if elapsed > self.duration || elapsed <= 0 {
                    0.0
                } else {
                    // `DEG2RAD` is a float product (`PI_DIV_180` is a float constant); the
                    // cosine is `cos`'s, in double.
                    let degrees = 90.0_f32 - (90.0_f32 * elapsed as f32) / self.duration as f32;
                    let cosine = f64::from(degrees * 0.017_453_292_f32).cos() as f32;
                    self.duration as f32 * 0.001_f32 * cosine
                };
                ma(delta_time)
            }
            _ => self.base,
        }
    }
}

/// A `path_corner`: where a train goes next, how fast, and how long it waits there.
#[derive(Clone, Debug, PartialEq)]
pub struct Corner {
    pub targetname: String,
    pub target: String,
    pub origin: [f32; 3],
    /// `speed` (0: the train's own), `wait` in seconds.
    pub speed: f32,
    pub wait: f32,
    /// `nextTrain`: the corner after this one, as `Think_SetupTrainTargets` links them.
    pub next: Option<usize>,
}

impl Corner {
    /// `SP_path_corner`: refused (`G_FreeEntity`) without a name.
    pub fn spawn(entity: &Entity) -> Option<Self> {
        let targetname = entity
            .get("targetname")
            .filter(|name| !name.is_empty())?
            .to_owned();
        let float = |key: &str| {
            entity
                .get(key)
                .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()))
        };
        Some(Self {
            targetname,
            target: entity.get("target").unwrap_or_default().to_owned(),
            origin: crate::fx_runner::vector(entity, "origin"),
            speed: float("speed"),
            wait: float("wait"),
            next: None,
        })
    }
}

/// `Think_SetupTrainTargets`' loop from corner `first` (`G_Find` over the standing
/// entities called a corner's `target`, the first `path_corner` among them — `corners` is
/// in entity order). A corner whose target names no corner ends the path; so does coming
/// back round to `first`.
pub fn link_corners(corners: &mut [Corner], first: usize) {
    let mut path = first;
    let start = first;
    loop {
        if corners[path].target.is_empty() {
            break;
        }
        let target = corners[path].target.clone();
        let Some(next) = corners
            .iter()
            .position(|corner| corner.targetname.eq_ignore_ascii_case(&target))
        else {
            break;
        };
        corners[path].next = Some(next);
        path = next;
        if path == start {
            break;
        }
    }
}

/// Which non-door mover it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Train,
    Bobbing,
    Pendulum,
    Rotating,
    Static,
}

/// What a train waits to do (`ent->think`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Think {
    None,
    /// `Think_SetupTrainTargets`, a frame after the level's spawn.
    SetupTrain,
    /// `Think_BeginMoving`, once a corner's wait is over.
    BeginMoving,
}

/// A mover of this module, as its spawn left it and its runs keep it.
#[derive(Clone, Debug, PartialEq)]
pub struct PathMover {
    pub kind: Kind,
    /// The inline model it is made of, and that model's bounds.
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    /// `model2`, which the caller registers (`G_ModelIndex`) for `s.modelindex2`; a `.glm`
    /// is not supported in MP and is left out.
    pub model2: String,
    /// `s.constantLight` from the `light` and `color` keys.
    pub constant_light: u32,
    pub spawnflags: u32,
    pub targetname: String,
    pub target: String,
    /// `speed` (a train's units a second), `dmg`.
    pub speed: f32,
    pub damage: i32,
    /// `alt_fire` (the `linear` key): a train's legs are linear rather than eased.
    pub linear: bool,
    /// `s.origin`: the map's `origin` key, where a named train that is not started stands.
    pub spawn_origin: [f32; 3],
    /// `pos1`, `pos2` and `moverState` (a train's current leg).
    pub pos1: [f32; 3],
    pub pos2: [f32; 3],
    pub moving: bool,
    /// `s.pos` and `s.apos`: what clients lerp.
    pub pos: Trajectory,
    pub apos: Trajectory,
    /// `r.currentOrigin`, `r.currentAngles`.
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// A train's next corner (`nextTrain`).
    pub next_corner: Option<usize>,
    pub next_think: i32,
    pub think: Think,
    /// `s.eFlags`, `s.eFlags2`, `s.frame`, `s.iModelScale`, `s.speed` (a radar object's
    /// range), `SVF_BROADCAST`.
    pub eflags: u32,
    pub eflags2: u32,
    pub frame: u32,
    pub model_scale: u32,
    pub radar_range: f32,
    pub broadcast: bool,
    /// `FL_INACTIVE`, `SVF_PLAYER_USABLE`.
    pub inactive: bool,
    pub player_usable: bool,
    /// `soundSet`: its start, travel and stop sounds; `looping` is `s.loopSound = BMS_MID`.
    pub sound_set: String,
    pub looping: bool,
    /// `EV_PLAYDOORSOUND`s raised and not yet sent.
    pub sounds: Vec<u32>,
}

/// Why a mover is refused at spawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// `func_train without a target` (`G_FreeEntity`).
    TrainWithoutTarget,
    /// No inline model (`"model" "*N"`).
    NoModel,
}

impl PathMover {
    /// The spawn function for `entity` (`bounds` its inline model's, as `SetBrushModel`
    /// gives them), or `None` when it is none of this module's. `gravity` is `g_gravity`,
    /// which a pendulum's period is reckoned from.
    pub fn spawn(
        entity: &Entity,
        bounds: ([f32; 3], [f32; 3]),
        gravity: f32,
        level_time: i32,
    ) -> Option<Result<Self, Refused>> {
        let kind = match entity.classname()?.to_ascii_lowercase().as_str() {
            "func_train" => Kind::Train,
            "func_bobbing" => Kind::Bobbing,
            "func_pendulum" => Kind::Pendulum,
            "func_rotating" => Kind::Rotating,
            "func_static" => Kind::Static,
            _ => return None,
        };
        let Some(model) = entity
            .get("model")
            .and_then(|model| model.strip_prefix('*'))
            .and_then(|number| number.parse().ok())
        else {
            return Some(Err(Refused::NoModel));
        };
        let text = |key: &str| entity.get(key).unwrap_or_default().to_owned();
        let int = |key: &str, default: i32| {
            entity
                .get(key)
                .map_or(default, |value| crate::userinfo::atoi(value.as_bytes()))
        };
        let float = |key: &str, default: f32| {
            entity
                .get(key)
                .map_or(default, |value| crate::text_parse::atof(value.as_bytes()))
        };
        let origin = crate::fx_runner::vector(entity, "origin");
        let spawnflags = int("spawnflags", 0) as u32;
        let model2 = text("model2");
        let mut mover = Self {
            kind,
            model,
            bounds,
            model2: if model2.contains(".glm") {
                String::new()
            } else {
                model2
            },
            constant_light: constant_light(entity),
            spawnflags,
            targetname: text("targetname"),
            target: text("target"),
            speed: float("speed", 0.0),
            damage: int("dmg", 0),
            linear: int("linear", 0) != 0,
            spawn_origin: origin,
            pos1: [0.0; 3],
            pos2: [0.0; 3],
            moving: false,
            pos: Trajectory::default(),
            apos: Trajectory::default(),
            origin: [0.0; 3],
            angles: [0.0; 3],
            next_corner: None,
            next_think: 0,
            think: Think::None,
            eflags: 0,
            eflags2: 0,
            frame: 0,
            model_scale: 0,
            radar_range: 0.0,
            broadcast: false,
            inactive: spawnflags & MOVER_INACTIVE != 0,
            player_usable: spawnflags & MOVER_PLAYER_USE != 0,
            sound_set: text("soundSet"),
            looping: false,
            sounds: Vec::new(),
        };
        let spawn_angles = crate::fx_runner::spawn_angles(entity);
        let scale = || int("model2scale", 0).clamp(0, 1023) as u32;
        match kind {
            Kind::Train => {
                // `VectorClear(self->s.angles)`; no damage when blocking stops it, else 2.
                mover.damage = if spawnflags & TRAIN_BLOCK_STOPS != 0 {
                    0
                } else if mover.damage == 0 {
                    2
                } else {
                    mover.damage
                };
                if mover.speed == 0.0 {
                    mover.speed = 100.0;
                }
                if mover.target.is_empty() {
                    return Some(Err(Refused::TrainWithoutTarget));
                }
                mover.init_mover();
                mover.next_think = level_time + FRAMETIME;
                mover.think = Think::SetupTrain;
            }
            Kind::Static => {
                (mover.pos1, mover.pos2) = (origin, origin);
                mover.init_mover();
                // `G_SetOrigin`, `G_SetAngles`.
                mover.set_origin(origin);
                mover.angles = spawn_angles;
                mover.apos = Trajectory {
                    base: spawn_angles,
                    ..Trajectory::default()
                };
                mover.broadcast = spawnflags & 2048 != 0;
                if spawnflags & 4 != 0 {
                    mover.eflags |= EF_SHADER_ANIM;
                }
                mover.model_scale = scale();
                if int("hyperspace", 0) != 0 {
                    mover.broadcast = true;
                    mover.eflags2 |= EF2_HYPERSPACE;
                }
            }
            Kind::Rotating => {
                // A rotating brush with health is a breakable one (`SP_func_breakable`):
                // left to the breakables, which do not turn it.
                mover.init_mover();
                mover.pos.base = origin;
                mover.origin = origin;
                mover.angles = mover.apos.base;
                mover.model_scale = scale();
                match entity.vector("spinangles").ok().flatten() {
                    Some(spin) => {
                        mover.speed =
                            (spin[0] * spin[0] + spin[1] * spin[1] + spin[2] * spin[2]).sqrt();
                        mover.apos.delta = spin;
                    }
                    None => {
                        if mover.speed == 0.0 {
                            mover.speed = 100.0;
                        }
                        let axis = if spawnflags & 4 != 0 {
                            2
                        } else if spawnflags & 8 != 0 {
                            0
                        } else {
                            1
                        };
                        mover.apos.delta[axis] = mover.speed;
                    }
                }
                mover.apos.kind = TR_LINEAR;
                if mover.damage == 0 {
                    mover.damage = if spawnflags & ROTATING_IMPACT != 0 {
                        10_000
                    } else {
                        2
                    };
                }
                if spawnflags & 2 != 0 {
                    // RADAR: `Distance(r.absmin, r.absmax) * 0.5`.
                    let (low, high) = mover.absolute_bounds();
                    let span: f32 = (0..3)
                        .map(|axis| (high[axis] - low[axis]) * (high[axis] - low[axis]))
                        .sum();
                    mover.radar_range = span.sqrt() * 0.5;
                    mover.eflags |= EF_RADAROBJECT;
                }
            }
            Kind::Bobbing => {
                let speed = float("speed", 4.0);
                let height = float("height", 32.0);
                mover.damage = int("dmg", 2);
                let phase = float("phase", 0.0);
                mover.speed = speed;
                mover.init_mover();
                mover.pos.base = origin;
                mover.origin = origin;
                mover.pos.duration = (speed * 1000.0) as i32;
                mover.pos.time = (mover.pos.duration as f32 * phase) as i32;
                mover.pos.kind = TR_SINE;
                mover.pos.delta = [0.0; 3];
                let axis = if spawnflags & 1 != 0 {
                    0
                } else if spawnflags & 2 != 0 {
                    1
                } else {
                    2
                };
                mover.pos.delta[axis] = height;
            }
            Kind::Pendulum => {
                let speed = float("speed", 30.0);
                mover.damage = int("dmg", 2);
                let phase = float("phase", 0.0);
                let length = bounds.0[2].abs().max(8.0);
                let frequency = (1.0 / (std::f64::consts::PI * 2.0)
                    * f64::from(gravity / (3.0 * length)).sqrt())
                    as f32;
                mover.pos.duration = (1000.0 / frequency) as i32;
                mover.init_mover();
                mover.pos.base = origin;
                mover.origin = origin;
                mover.apos.base = spawn_angles;
                mover.apos.duration = (1000.0 / frequency) as i32;
                mover.apos.time = (mover.apos.duration as f32 * phase) as i32;
                mover.apos.kind = TR_SINE;
                mover.apos.delta[2] = speed;
                // `r.currentAngles` is never set: the first frame turns it from zero.
                mover.angles = [0.0; 3];
            }
        }
        Some(Ok(mover))
    }

    /// `InitMover` with `InitMoverTrData`: at rest at `pos1`, a leg to `pos2` reckoned from
    /// the speed.
    fn init_mover(&mut self) {
        self.moving = false;
        self.origin = self.pos1;
        self.pos.kind = TR_STATIONARY;
        self.pos.base = self.pos1;
        let travel: [f32; 3] = std::array::from_fn(|axis| self.pos2[axis] - self.pos1[axis]);
        let distance =
            (travel[0] * travel[0] + travel[1] * travel[1] + travel[2] * travel[2]).sqrt();
        if self.speed == 0.0 {
            self.speed = 100.0;
        }
        self.pos.delta = travel.map(|value| value * self.speed);
        self.pos.duration = (distance * 1000.0 / self.speed) as i32;
        if self.pos.duration <= 0 {
            self.pos.duration = 1;
        }
    }

    /// `G_SetOrigin`: at rest at `origin`.
    fn set_origin(&mut self, origin: [f32; 3]) {
        self.pos = Trajectory {
            kind: TR_STATIONARY,
            time: 0,
            duration: 0,
            base: origin,
            delta: [0.0; 3],
        };
        self.origin = origin;
    }

    /// `r.absmin`, `r.absmax` as `SV_LinkEntity` gives a brush model at its origin.
    pub fn absolute_bounds(&self) -> ([f32; 3], [f32; 3]) {
        (
            std::array::from_fn(|axis| self.origin[axis] + self.bounds.0[axis] - 1.0),
            std::array::from_fn(|axis| self.origin[axis] + self.bounds.1[axis] + 1.0),
        )
    }

    /// `SetMoverState` for a train's leg: `true` from `pos1` to `pos2` over `s.pos.trDuration`,
    /// eased unless `linear`; `false` at rest at `pos1`.
    fn set_state(&mut self, moving: bool, time: i32, level_time: i32) {
        self.moving = moving;
        self.pos.time = time;
        if self.pos.duration <= 0 {
            self.pos.duration = 1;
        }
        if moving {
            self.pos.base = self.pos1;
            let scale = (1000.0_f64 / f64::from(self.pos.duration)) as f32;
            self.pos.delta =
                std::array::from_fn(|axis| (self.pos2[axis] - self.pos1[axis]) * scale);
            self.pos.kind = if self.linear {
                TR_LINEAR_STOP
            } else {
                TR_NONLINEAR_STOP
            };
        } else {
            self.pos.base = self.pos1;
            self.pos.kind = TR_STATIONARY;
        }
        self.origin = self.pos.evaluate(level_time);
    }

    /// `G_PlayDoorSound`.
    fn play_sound(&mut self, sound: u32) {
        if !self.sound_set.is_empty() {
            self.sounds.push(sound);
        }
    }

    /// `G_PlayDoorLoopSound`.
    fn play_loop(&mut self) {
        if !self.sound_set.is_empty() {
            self.looping = true;
        }
    }

    /// `Think_SetupTrainTargets` once the corners are linked ([`link_corners`]): the first
    /// corner is the first standing entity called its `target` (`first`, `None` when there
    /// is none — the reference prints so and the train stays); an unnamed or START_ON train
    /// sets off (`Reached_Train`), another stays where it is. Returns the name its first
    /// corner fires, if any.
    pub fn setup_train(
        &mut self,
        corners: &[Corner],
        first: Option<usize>,
        level_time: i32,
    ) -> Option<String> {
        self.next_corner = first;
        first?;
        if self.targetname.is_empty() || self.spawnflags & TRAIN_START_ON != 0 {
            return self.reached_train(corners, level_time);
        }
        // `G_SetOrigin(ent, ent->s.origin)`: the map's own origin key.
        self.set_origin(self.spawn_origin);
        None
    }

    /// `Reached_Train`: the next leg set from the corner reached to the one after it, at
    /// the corner's speed (or the train's), waiting there first for the corner's `wait`.
    /// Returns the corner's `target`, which is fired (`G_UseTargets(next, NULL)`).
    pub fn reached_train(&mut self, corners: &[Corner], level_time: i32) -> Option<String> {
        let next = self.next_corner?;
        let after = corners[next].next?;
        let fired = (!corners[next].target.is_empty()).then(|| corners[next].target.clone());
        self.next_corner = Some(after);
        self.pos1 = corners[next].origin;
        self.pos2 = corners[after].origin;
        let speed = if corners[next].speed != 0.0 {
            corners[next].speed
        } else {
            self.speed
        }
        .max(1.0);
        let travel: [f32; 3] = std::array::from_fn(|axis| self.pos2[axis] - self.pos1[axis]);
        let length = (travel[0] * travel[0] + travel[1] * travel[1] + travel[2] * travel[2]).sqrt();
        self.pos.duration = (length * 1000.0 / speed) as i32;
        self.set_state(true, level_time, level_time);
        self.play_sound(BMS_END);
        if corners[next].wait != 0.0 {
            self.looping = false;
            self.next_think = (level_time as f32 + corners[next].wait * 1000.0) as i32;
            self.think = Think::BeginMoving;
            self.pos.kind = TR_STATIONARY;
        } else {
            self.play_loop();
        }
        fired
    }

    /// `G_RunThink` for a train: the corners linked a frame after the spawn, or a corner's
    /// wait over. `first` is the train's first corner, for the setup.
    pub fn think(
        &mut self,
        corners: &[Corner],
        first: Option<usize>,
        level_time: i32,
    ) -> Option<String> {
        let think = std::mem::replace(&mut self.think, Think::None);
        self.next_think = 0;
        match think {
            Think::None => None,
            Think::SetupTrain => self.setup_train(corners, first, level_time),
            Think::BeginMoving => {
                self.play_sound(BMS_START);
                self.play_loop();
                self.pos.time = level_time;
                self.pos.kind = TR_LINEAR_STOP;
                None
            }
        }
    }

    /// Whether `G_RunThink` runs its think at `level_time`.
    pub fn due(&self, level_time: i32) -> bool {
        self.next_think > 0 && self.next_think <= level_time
    }

    /// Whether `G_RunMover` moves it at all this frame (either trajectory not stationary).
    pub fn travels(&self) -> bool {
        self.pos.kind != TR_STATIONARY || self.apos.kind != TR_STATIONARY
    }

    /// `G_MoverTeam`'s first half for a mover of its own: how far it moves and turns from
    /// where it is to where its trajectories put it at `level_time` (`move`, `amove`).
    pub fn travel(&self, level_time: i32) -> ([f32; 3], [f32; 3]) {
        let origin = self.pos.evaluate(level_time);
        let angles = self.apos.evaluate(level_time);
        (
            std::array::from_fn(|axis| origin[axis] - self.origin[axis]),
            std::array::from_fn(|axis| angles[axis] - self.angles[axis]),
        )
    }

    /// `G_MoverTeam` after the push of `shift` and `turn` ([`Self::travel`]): moved
    /// (`currentOrigin += move`, as `G_MoverPush` adds it — not the trajectory's point, which
    /// can differ in the last bit), or held back where it was (`trTime` pushed on by the
    /// frame, as the reference does when blocked). Returns whether it reached its leg's end
    /// (`reached`): the caller then runs [`Self::reached_train`] for a train.
    pub fn moved(
        &mut self,
        shift: [f32; 3],
        turn: [f32; 3],
        pushed: bool,
        level_time: i32,
        previous_time: i32,
    ) -> bool {
        if !pushed {
            self.pos.time += level_time - previous_time;
            self.apos.time += level_time - previous_time;
            self.origin = self.pos.evaluate(level_time);
            self.angles = self.apos.evaluate(level_time);
            return false;
        }
        if shift != [0.0; 3] || turn != [0.0; 3] {
            self.origin = std::array::from_fn(|axis| self.origin[axis] + shift[axis]);
            self.angles = std::array::from_fn(|axis| self.angles[axis] + turn[axis]);
        }
        matches!(self.pos.kind, TR_LINEAR_STOP | TR_NONLINEAR_STOP)
            && level_time >= self.pos.time + self.pos.duration
    }

    /// `func_static_use`: its shader frame toggled (SWITCH_SHADER); its targets are fired
    /// by the caller.
    pub fn use_static(&mut self) {
        if self.spawnflags & 4 != 0 {
            self.frame = u32::from(self.frame == 0);
        }
    }

    /// The mover's wire state, what every client draws it by.
    /// `model2` and `sound_set` are the registered indices of [`Self::model2`] and
    /// [`Self::sound_set`].
    pub fn project(&self, state: &mut EntityState, model2: u16, sound_set: u16) {
        state.set_raw_field(es::TYPE, ET_MOVER);
        state.set_raw_field(es::SOUND_SET, u32::from(sound_set));
        crate::triggers::set_brush_model(state, self.model);
        state.set_raw_field(es::MODEL2, u32::from(model2));
        state.set_raw_field(es::CONSTANT_LIGHT, self.constant_light);
        for (trajectory, fields) in [(&self.pos, es::POS), (&self.apos, es::APOS)] {
            state.set_raw_field(fields.kind, trajectory.kind);
            state.set_raw_field(fields.time, trajectory.time as u32);
            state.set_raw_field(fields.duration, trajectory.duration as u32);
            for axis in 0..3 {
                state.set_raw_field(fields.base[axis], trajectory.base[axis].to_bits());
                state.set_raw_field(fields.delta[axis], trajectory.delta[axis].to_bits());
            }
        }
        state.set_raw_field(es::EFLAGS, self.eflags);
        state.set_raw_field(es::EFLAGS2, self.eflags2);
        state.set_raw_field(es::FRAME, self.frame);
        state.set_raw_field(es::MODEL_SCALE, self.model_scale);
        state.set_raw_field(es::SPEED, self.radar_range.to_bits());
        state.set_raw_field(es::LOOP_SOUND, if self.looping { BMS_MID } else { 0 });
        state.set_raw_field(es::LOOP_IS_SOUNDSET, u32::from(self.looping));
    }
}

/// `InitMover`'s `s.constantLight` (`g_mover.c:986-1009`): when the `light` or `color` key
/// is set, its colour in bytes and its radius over four, each at most 255.
fn constant_light(entity: &Entity) -> u32 {
    let light_set = entity.get("light").is_some();
    let color_set = entity.get("color").is_some();
    if !light_set && !color_set {
        return 0;
    }
    let light = entity
        .get("light")
        .map_or(100.0, |value| crate::text_parse::atof(value.as_bytes()));
    let color = if color_set {
        crate::fx_runner::vector(entity, "color")
    } else {
        [1.0; 3]
    };
    let byte = |value: f32| ((value * 255.0) as i32).min(255) as u32 & 0xff;
    let intensity = ((light / 4.0) as i32).min(255) as u32 & 0xff;
    byte(color[0]) | byte(color[1]) << 8 | byte(color[2]) << 16 | intensity << 24
}

/// One entity a mover may push: its number, where it stands, its box, and what it stands
/// on (`s.groundEntityNum`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pushee {
    pub number: u16,
    pub origin: [f32; 3],
    pub bounds: ([f32; 3], [f32; 3]),
    pub ground: u16,
    /// `health < 1`: a dead player is crushed rather than blocking.
    pub dead: bool,
}

/// What `G_MoverPush` did to one entity it moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pushed {
    pub number: u16,
    pub to: [f32; 3],
    /// `delta_angles[YAW] += ANGLE2SHORT(amove[YAW])`: the view turned with the mover.
    pub yaw: i32,
    /// Whether it stays on the mover (`groundEntityNum` kept); otherwise it is in the air.
    pub on_mover: bool,
}

/// The outcome of `G_MoverPush`.
#[derive(Clone, Debug, PartialEq)]
pub struct Push {
    /// Moved where it could be.
    pub pushed: Vec<Pushed>,
    /// Hurt on the way (`G_Damage(check, pusher, pusher, NULL, NULL, damage, flags, MOD_CRUSH)`):
    /// crushed through, run over dead, struck by a spinning IMPACT brush or a bobbing one.
    pub crushed: Vec<(u16, i32, u32)>,
    /// Who blocked it (`*obstacle`): the move is undone and nobody is moved.
    pub blocked: Option<u16>,
}

/// `DAMAGE_NO_KNOCKBACK`, which a spinning IMPACT brush deals with.
const DAMAGE_NO_KNOCKBACK: u32 = 0x4;

/// `G_MoverPush` with `G_TryPushingEntity` (`g_mover.c:178-420`): the entities the mover's
/// new place takes (or that stand on it) moved by `move` and turned about its new origin by
/// `amove`. `inside` answers whether an entity's box where it stands is inside the mover at
/// its new place; `free` whether a box may stand somewhere at all (`G_TestEntityPosition`,
/// the mover at its new place included).
pub fn push(
    mover: &PathMover,
    number: u16,
    shift: [f32; 3],
    turn: [f32; 3],
    candidates: &[Pushee],
    inside: &mut dyn FnMut(&Pushee) -> bool,
    free: &mut dyn FnMut(u16, [f32; 3], ([f32; 3], [f32; 3])) -> bool,
) -> Push {
    let mut outcome = Push {
        pushed: Vec::new(),
        crushed: Vec::new(),
        blocked: None,
    };
    let new_origin: [f32; 3] = std::array::from_fn(|axis| mover.origin[axis] + shift[axis]);
    // The box the move takes: a turning mover's radius about its origin, else its own box.
    let (low, high) = if mover.angles != [0.0; 3] || turn != [0.0; 3] {
        let radius = radius_from_bounds(mover.bounds);
        (
            new_origin.map(|value| value - radius),
            new_origin.map(|value| value + radius),
        )
    } else {
        let (low, high) = mover.absolute_bounds();
        (
            std::array::from_fn(|axis| low[axis] + shift[axis]),
            std::array::from_fn(|axis| high[axis] + shift[axis]),
        )
    };
    let matrix = transposed_rotation(turn);
    let spinning_impact = mover.kind == Kind::Rotating
        && mover.apos.kind != TR_STATIONARY
        && mover.spawnflags & ROTATING_IMPACT != 0;
    let bobbing = mover.pos.kind == TR_SINE || mover.apos.kind == TR_SINE;
    for check in candidates {
        let on_mover = check.ground == number;
        // `G_TestEntityPosition` tests a client's box with its top at least 1 (a corpse's is -8).
        let mut bounds = check.bounds;
        bounds.1[2] = bounds.1[2].max(1.0);
        if !on_mover {
            // `r.absmin`, `r.absmax`: a linked box has a unit of slack all round.
            let their_low: [f32; 3] =
                std::array::from_fn(|axis| check.origin[axis] + check.bounds.0[axis] - 1.0);
            let their_high: [f32; 3] =
                std::array::from_fn(|axis| check.origin[axis] + check.bounds.1[axis] + 1.0);
            if (0..3).any(|axis| their_low[axis] >= high[axis] || their_high[axis] <= low[axis])
                || !inside(&Pushee { bounds, ..*check })
            {
                continue;
            }
        }
        // `G_TryPushingEntity`.
        if spinning_impact {
            outcome
                .crushed
                .push((check.number, mover.damage, DAMAGE_NO_KNOCKBACK));
            continue;
        }
        let relative: [f32; 3] = std::array::from_fn(|axis| check.origin[axis] - new_origin[axis]);
        let rotated: [f32; 3] = std::array::from_fn(|row| {
            matrix[row][0] * relative[0]
                + matrix[row][1] * relative[1]
                + matrix[row][2] * relative[2]
        });
        let to: [f32; 3] = std::array::from_fn(|axis| {
            check.origin[axis] + shift[axis] + (rotated[axis] - relative[axis])
        });
        if free(check.number, to, bounds) {
            outcome.pushed.push(Pushed {
                number: check.number,
                to,
                yaw: angle_to_short(turn[1]),
                on_mover,
            });
            continue;
        }
        // "if it is ok to leave in the old position, do it".
        if free(check.number, check.origin, bounds) {
            continue;
        }
        if mover.damage != 0 && mover.spawnflags & CRUSH_THROUGH != 0 {
            outcome.crushed.push((check.number, mover.damage, 0));
            continue;
        }
        if check.dead {
            outcome.crushed.push((check.number, 999, 0));
            continue;
        }
        if bobbing {
            outcome.crushed.push((check.number, 99_999, 0));
            continue;
        }
        outcome.pushed.clear();
        outcome.blocked = Some(check.number);
        return outcome;
    }
    outcome
}

/// `RadiusFromBounds`.
pub fn radius_from_bounds(bounds: ([f32; 3], [f32; 3])) -> f32 {
    let corner: [f32; 3] =
        std::array::from_fn(|axis| bounds.0[axis].abs().max(bounds.1[axis].abs()));
    (corner[0] * corner[0] + corner[1] * corner[1] + corner[2] * corner[2]).sqrt()
}

/// `G_CreateRotationMatrix(amove, transpose)` then `G_TransposeMatrix(transpose, matrix)`.
fn transposed_rotation(turn: [f32; 3]) -> [[f32; 3]; 3] {
    let axes = crate::pmove::flight::angles_to_axis(turn);
    std::array::from_fn(|row| std::array::from_fn(|column| axes[column][row]))
}

/// `ANGLE2SHORT`: `((int)((x)*65536/360) & 65535)`.
fn angle_to_short(degrees: f32) -> i32 {
    ((degrees * 65_536.0 / 360.0) as i32) & 65_535
}

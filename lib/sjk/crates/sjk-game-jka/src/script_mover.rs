//! Entities a script moves (`g_ICARUScb.c`'s `Q3_Lerp2Pos`, `Q3_Lerp2Angles`,
//! `Q3_Lerp2Origin` and their callbacks, over `g_mover.c`'s `InitMoverTrData`,
//! `SetMoverState` and `G_RunMover`): a `func_static` from the moment it spawns, and any
//! other entity a script sets moving — the reference turns whatever it is into an
//! `ET_MOVER` on the spot, which is how a map switches a trigger on and off by moving it
//! out of the way.
//!
//! A scripted mover is a binary mover whose two places the script picks each time: the
//! one it stands at and the one it is sent to. It travels there as a door does
//! (`TR_NONLINEAR_STOP`, or `TR_LINEAR_STOP` with the map's `linear` key) and, when it
//! arrives, completes the script's `move` task. A `rotate` turns it over its own angular
//! trajectory; the entity's think stops the turn and completes that task.

use crate::movers::{MoverState, TR_LINEAR_STOP, TR_NONLINEAR_STOP, TR_STATIONARY};
use crate::player_angle_math::{angle_subtract, normalized_angle};

/// `PI_DIV_180`, the float the reference's `DEG2RAD` multiplies by.
const PI_DIV_180: f32 = 0.017_453_292_519_943_295;

/// A `trajectory_t`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trajectory {
    /// `trType`.
    pub kind: u32,
    /// `trTime`.
    pub time: i32,
    /// `trDuration`.
    pub duration: i32,
    /// `trBase`.
    pub base: [f32; 3],
    /// `trDelta`.
    pub delta: [f32; 3],
}

impl Trajectory {
    /// `BG_EvaluateTrajectory` (`bg_misc.c:2227-2284`), exactly for the kinds a scripted
    /// mover takes: still, and the two that stop at the end of their duration.
    pub fn evaluate(&self, at_time: i32) -> [f32; 3] {
        let scale = |factor: f32| -> [f32; 3] {
            std::array::from_fn(|axis| self.base[axis] + self.delta[axis] * factor)
        };
        match self.kind {
            TR_LINEAR_STOP => {
                let at_time = at_time.min(self.time.wrapping_add(self.duration));
                let mut delta_time = (f64::from(at_time.wrapping_sub(self.time)) * 0.001) as f32;
                if delta_time < 0.0 {
                    delta_time = 0.0;
                }
                scale(delta_time)
            }
            TR_NONLINEAR_STOP => {
                let at_time = at_time.min(self.time.wrapping_add(self.duration));
                let elapsed = at_time.wrapping_sub(self.time);
                // "new slow-down at end"
                let delta_time = if elapsed > self.duration || elapsed <= 0 {
                    0.0
                } else {
                    let degrees = 90.0_f32 - (90.0_f32 * elapsed as f32) / self.duration as f32;
                    self.duration as f32
                        * 0.001_f32
                        * (f64::from(degrees * PI_DIV_180).cos() as f32)
                };
                scale(delta_time)
            }
            TR_STATIONARY => self.base,
            kind => crate::trajectory::legacy_evaluate_trajectory(
                self.base,
                self.delta,
                kind as u8,
                self.time,
                self.duration,
                at_time,
            ),
        }
    }
}

/// What a scripted mover does when it arrives (`ent->reached`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reached {
    /// Nothing: a `func_static` at rest (`SP_func_static` clears it).
    #[default]
    Nothing,
    /// `moverCallback`: the move task is complete and it rests at the end.
    Move,
    /// `moveAndRotateCallback`: the turn stops too, then as [`Reached::Move`].
    MoveAndRotate,
}

/// A task slot a callback completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completed {
    /// `TID_ANGLE_FACE`.
    Angles,
    /// `TID_MOVE_NAV`.
    Travel,
}

/// What a script-set move leaves waiting on the entity (`ICARUS_TaskIDSet`), in the
/// reference's order: the angles first, then the travel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Waits {
    /// `TID_ANGLE_FACE` waits (a move with angles).
    pub angles: bool,
    /// `TID_MOVE_NAV` waits.
    pub travel: bool,
}

/// An entity's motion as a script sees it: where it is, the trajectories a client lerps
/// it along, and the two places of its binary travel.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptMover {
    /// `s.eType == ET_MOVER`: `G_RunMover` runs it. A `rotate` alone does not make an
    /// entity a mover.
    pub is_mover: bool,
    /// `pos1` and `pos2`.
    pub pos1: [f32; 3],
    pub pos2: [f32; 3],
    /// `moverState`.
    pub state: MoverState,
    /// `s.pos` and `s.apos`.
    pub pos: Trajectory,
    pub apos: Trajectory,
    /// `r.currentOrigin` and `r.currentAngles`.
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// `speed` (`InitMoverTrData` makes a zero a hundred).
    pub speed: f32,
    /// `alt_fire`: the map's `linear` key, a linear travel instead of an eased one.
    pub linear: bool,
    /// `damage`: what it does to whoever blocks it.
    pub damage: i32,
    /// `reached`.
    pub reached: Reached,
    /// `blocked == Blocked_Mover`: a script set it moving with damage to deal.
    pub crushes: bool,
    /// The soundset its start and stop sounds come from (empty for none).
    pub sound_set: String,
}

impl Default for ScriptMover {
    fn default() -> Self {
        Self {
            is_mover: false,
            pos1: [0.0; 3],
            pos2: [0.0; 3],
            state: MoverState::Pos1,
            pos: Trajectory::default(),
            apos: Trajectory::default(),
            origin: [0.0; 3],
            angles: [0.0; 3],
            speed: 0.0,
            linear: false,
            damage: 0,
            reached: Reached::Nothing,
            crushes: false,
            sound_set: String::new(),
        }
    }
}

impl ScriptMover {
    /// An entity standing where the map put it, not a mover (`G_SetOrigin`,
    /// `G_SetAngles` at spawn).
    pub fn standing(origin: [f32; 3], angles: [f32; 3]) -> Self {
        let mut motion = Self {
            origin,
            angles,
            ..Self::default()
        };
        motion.pos.base = origin;
        motion.apos.base = angles;
        motion
    }

    /// `SP_func_static`'s mover resting at `origin` (both places there): `InitMover`
    /// and `InitMoverTrData`, then placed still, with `reached` cleared.
    pub fn resting(
        origin: [f32; 3],
        angles: [f32; 3],
        speed: f32,
        linear: bool,
        damage: i32,
        sound_set: &str,
    ) -> Self {
        let mut mover = Self {
            is_mover: true,
            pos1: origin,
            pos2: origin,
            speed,
            linear,
            damage,
            sound_set: sound_set.to_owned(),
            ..Self::standing(origin, angles)
        };
        mover.init_trajectory();
        // `G_SetOrigin` and `G_SetAngles` after `InitMover`: both trajectories still.
        mover.set_origin(origin);
        mover.apos = Trajectory {
            kind: TR_STATIONARY,
            time: 0,
            duration: 0,
            base: angles,
            delta: [0.0; 3],
        };
        mover
    }

    /// `G_SetOrigin`: still at `origin`.
    pub fn set_origin(&mut self, origin: [f32; 3]) {
        self.pos = Trajectory {
            kind: TR_STATIONARY,
            time: 0,
            duration: 0,
            base: origin,
            delta: [0.0; 3],
        };
        self.origin = origin;
    }

    /// `InitMoverTrData`: still at `pos1`, the travel's delta scaled by the speed and its
    /// duration from its length.
    pub fn init_trajectory(&mut self) {
        self.pos.kind = TR_STATIONARY;
        self.pos.base = self.pos1;
        let travel: [f32; 3] = std::array::from_fn(|axis| self.pos2[axis] - self.pos1[axis]);
        let distance =
            (travel[0] * travel[0] + travel[1] * travel[1] + travel[2] * travel[2]).sqrt();
        if self.speed == 0.0 {
            self.speed = 100.0;
        }
        self.pos.delta = std::array::from_fn(|axis| travel[axis] * self.speed);
        self.pos.duration = (distance * 1_000.0 / self.speed) as i32;
        if self.pos.duration <= 0 {
            self.pos.duration = 1;
        }
    }

    /// `SetMoverState`: at rest at either place, or on its way between them; the
    /// current origin is where the travel puts it at `level_time`.
    pub fn set_state(&mut self, state: MoverState, time: i32, level_time: i32) {
        self.state = state;
        self.pos.time = time;
        if self.pos.duration <= 0 {
            self.pos.duration = 1;
        }
        // `f = 1000.0 / trDuration`: a double quotient, stored as a float.
        let travel = |from: [f32; 3], to: [f32; 3], duration: i32| -> [f32; 3] {
            let factor = (1_000.0_f64 / f64::from(duration)) as f32;
            std::array::from_fn(|axis| (to[axis] - from[axis]) * factor)
        };
        let moving = if self.linear {
            TR_LINEAR_STOP
        } else {
            TR_NONLINEAR_STOP
        };
        match state {
            MoverState::Pos1 => {
                self.pos.base = self.pos1;
                self.pos.kind = TR_STATIONARY;
            }
            MoverState::Pos2 => {
                self.pos.base = self.pos2;
                self.pos.kind = TR_STATIONARY;
            }
            MoverState::OneToTwo => {
                self.pos.base = self.pos1;
                self.pos.delta = travel(self.pos1, self.pos2, self.pos.duration);
                self.pos.kind = moving;
            }
            MoverState::TwoToOne => {
                self.pos.base = self.pos2;
                self.pos.delta = travel(self.pos2, self.pos1, self.pos.duration);
                self.pos.kind = moving;
            }
        }
        self.origin = self.pos.evaluate(level_time);
    }

    /// The two places for a travel to `origin`: from where it stands, onwards in the
    /// direction it last went. Returns the state it travels in.
    fn aim(&mut self, origin: [f32; 3]) -> MoverState {
        if matches!(self.state, MoverState::Pos1 | MoverState::TwoToOne) {
            self.pos1 = self.origin;
            self.pos2 = origin;
            MoverState::OneToTwo
        } else {
            self.pos2 = self.origin;
            self.pos1 = origin;
            MoverState::TwoToOne
        }
    }

    /// `Q3_Lerp2Pos` (`g_ICARUScb.c:799-901`): to `origin` in `duration` milliseconds (a
    /// zero is one), turning to `angles` on the way if given. The travel sounds start.
    pub fn lerp_to_position(
        &mut self,
        origin: [f32; 3],
        angles: Option<[f32; 3]>,
        duration: f32,
        level_time: i32,
    ) -> Waits {
        let duration = if duration == 0.0 { 1.0 } else { duration };
        self.is_mover = true;
        let state = self.aim(origin);
        self.init_trajectory();
        self.pos.duration = duration as i32;
        // `MatchTeam`: a scripted mover is a team of one.
        self.set_state(state, level_time, level_time);
        let mut waits = Waits {
            angles: false,
            travel: true,
        };
        if let Some(angles) = angles {
            for axis in 0..3 {
                let turn = normalized_angle(angles[axis] - self.angles[axis]);
                self.apos.delta[axis] = turn / (duration * 0.001_f32);
            }
            self.apos.base = self.angles;
            self.apos.kind = if self.linear {
                TR_LINEAR_STOP
            } else {
                TR_NONLINEAR_STOP
            };
            self.apos.duration = duration as i32;
            self.apos.time = level_time;
            self.reached = Reached::MoveAndRotate;
            waits.angles = true;
        } else {
            self.reached = Reached::Move;
        }
        if self.damage != 0 {
            self.crushes = true;
        }
        waits
    }

    /// `Q3_Lerp2Origin` (`:2072-2133`): the same travel without angles.
    pub fn lerp_to_origin(&mut self, origin: [f32; 3], duration: f32, level_time: i32) -> Waits {
        self.is_mover = true;
        let state = self.aim(origin);
        self.init_trajectory();
        self.pos.duration = duration as i32;
        self.set_state(state, level_time, level_time);
        self.reached = Reached::Move;
        if self.damage != 0 {
            self.crushes = true;
        }
        Waits {
            angles: false,
            travel: true,
        }
    }

    /// `Q3_Lerp2Angles` (`:910-957`): turns to `angles` in `duration` milliseconds.
    /// Returns when the entity's think (`anglerCallback`, which calls
    /// [`ScriptMover::stop_turning`]) is to stop it.
    pub fn lerp_to_angles(&mut self, angles: [f32; 3], duration: f32, level_time: i32) -> i32 {
        self.apos.duration = if duration > 0.0 { duration as i32 } else { 1 };
        for axis in 0..3 {
            let turn = angle_subtract(angles[axis], self.angles[axis]);
            self.apos.delta[axis] = turn / (self.apos.duration as f32 * 0.001_f32);
        }
        self.apos.base = self.angles;
        self.apos.kind = if self.linear {
            TR_LINEAR_STOP
        } else {
            TR_NONLINEAR_STOP
        };
        self.apos.time = level_time;
        // `level.time + duration`: a float sum, truncated as it is stored.
        (level_time as f32 + duration) as i32
    }

    /// Whether `G_RunMover` moves it this frame: a mover travelling or turning.
    pub fn in_motion(&self) -> bool {
        self.is_mover && (self.pos.kind != TR_STATIONARY || self.apos.kind != TR_STATIONARY)
    }

    /// Where the trajectories put it at `level_time`: the origin and the angles.
    pub fn place_at(&self, level_time: i32) -> ([f32; 3], [f32; 3]) {
        (
            self.pos.evaluate(level_time),
            self.apos.evaluate(level_time),
        )
    }

    /// `G_MoverTeam` when the move was blocked: the trajectories slide on by the frame
    /// so the mover stays where it was (`trTime += level.time - level.previousTime`).
    pub fn hold(&mut self, level_time: i32, previous_time: i32) {
        let frame = level_time.wrapping_sub(previous_time);
        self.pos.time = self.pos.time.wrapping_add(frame);
        self.apos.time = self.apos.time.wrapping_add(frame);
        self.origin = self.pos.evaluate(level_time);
        self.angles = self.apos.evaluate(level_time);
    }

    /// The rest of `G_MoverTeam` once the caller has pushed what was in the way and
    /// committed the new place (`moved_ok`; false when the move was blocked): at or past
    /// the travel's end, `reached` runs. Returns the task slots it completes.
    pub fn arrive_if_due(&mut self, moved_ok: bool, level_time: i32) -> Vec<Completed> {
        let mut completed = Vec::new();
        if moved_ok
            && matches!(self.pos.kind, TR_LINEAR_STOP | TR_NONLINEAR_STOP)
            && level_time >= self.pos.time.wrapping_add(self.pos.duration)
        {
            match self.reached {
                Reached::Nothing => {}
                Reached::Move => self.arrive(&mut completed, level_time),
                Reached::MoveAndRotate => {
                    completed.push(self.stop_turning(level_time));
                    self.arrive(&mut completed, level_time);
                }
            }
        }
        completed
    }

    /// `anglerCallback`'s motion: the turn is over; the angles rest where the whole turn
    /// takes them, and `reached` is cleared whatever it was (the caller clears the
    /// entity's think if it was this one). Returns the slot it completes.
    pub fn stop_turning(&mut self, level_time: i32) -> Completed {
        let scale = self.apos.duration as f32 * 0.001_f32;
        self.angles =
            std::array::from_fn(|axis| self.apos.base[axis] + scale * self.apos.delta[axis]);
        self.apos.base = self.angles;
        self.apos.delta = [0.0; 3];
        self.apos.duration = 1;
        self.apos.kind = TR_STATIONARY;
        self.apos.time = level_time;
        self.reached = Reached::Nothing;
        Completed::Angles
    }

    /// `moverCallback`'s motion: the move task is complete and it rests at the end it
    /// reached (the caller stops the travel sound and plays the stop sound).
    fn arrive(&mut self, completed: &mut Vec<Completed>, level_time: i32) {
        completed.push(Completed::Travel);
        match self.state {
            MoverState::OneToTwo => self.set_state(MoverState::Pos2, level_time, level_time),
            MoverState::TwoToOne => self.set_state(MoverState::Pos1, level_time, level_time),
            _ => {}
        }
        self.crushes = false;
    }
}

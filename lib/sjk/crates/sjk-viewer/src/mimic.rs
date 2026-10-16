//! Mimic (`mimic`): copy another player's moves live, facing them, for showcases.
//!
//! The client never sees another player's inputs, only what they lead to: each
//! snapshot carries their position, velocity, view angles, ground, bounding box
//! (crouched or not), saber move, stance, holster and weapon. The mimic keeps those
//! [`Sample`]s, plays them back after the chosen delay and rebuilds commands that make
//! the local player do the same:
//!
//! - **View:** the leader's yaw turn since the start, mirrored (or copied), and pitch.
//! - **Movement:** the leader's motion since the start, turned into the mimic's own
//!   frame (facing each other: the leader stepping forward makes the mimic step
//!   forward too) and mirrored left for right; followed through the movement keys by
//!   matching velocity, then closing the offset ([`follow_keys`]).
//! - **Jumps and crouches:** a takeoff is a jump held while the leader rises; a
//!   crouched bounding box is a crouch.
//! - **Saber:** the move the leader is in names the controls that ask for it
//!   ([`saber::plan`]); a different stance cycles the stance, a different holster
//!   state toggles the saber.
//!
//! Reactions (parries, knockdowns, locks) are the fight's, not inputs, and are not
//! copied. With no delay the copy runs as soon as the leader is seen, which is after
//! their ping, the snapshot interval and this client's ping.

use std::collections::VecDeque;

use sjk_game_jka::pmove::MovementState;
use sjk_protocol::{EntityState, PlayerState, UserCommand};

use crate::ghost_replay::Drive;

#[path = "mimic/frame.rs"]
mod frame;
#[path = "mimic/saber.rs"]
mod saber;

pub(crate) const COMMAND: &str = "mimic";
pub(crate) const HELP: &str = "Experimental (cl_experimentalShowcase): copy a player's moves \
     while facing them: mimic <player> \
     [delay seconds] [mirror|copy] [walk], mimic off";

/// No ground entity (`ENTITYNUM_NONE`).
const NO_GROUND: u16 = 1023;
/// `WP_SABER`.
const WP_SABER: u8 = 3;
/// `BUTTON_WALKING`.
const BUTTON_WALKING: u16 = 16;
/// The largest key that keeps walking: Pmove clears `BUTTON_WALKING` past it, "to
/// avoid proxy no-footsteps cheats" (`bg_pmove.c:10490-10494`).
const WALK_KEY: i8 = 64;
/// `GENCMD_SABER_SWITCH` and `GENCMD_SABER_ATTACK_CYCLE`.
const GENCMD_SABER_SWITCH: u8 = 1;
const GENCMD_SABER_ATTACK_CYCLE: u8 = 26;
/// The longest delay, in milliseconds.
pub(crate) const MAX_DELAY: i32 = 10_000;
/// Samples kept at most: ten seconds of 125 snapshots a second and the second behind.
const MAX_SAMPLES: usize = 1_500;
/// How far past the newest sample the leader is extrapolated, in milliseconds.
const EXTRAPOLATE: i32 = 100;
/// Out of sight this long, the mimic stops.
const LOST_SIGHT: i32 = 3_000;
/// Samples older than the playback time by this much are dropped.
const KEEP_BEHIND: i32 = 1_000;
/// A takeoff: leaving the ground faster than this upward, in units a second.
const TAKEOFF_SPEED: f32 = 100.0;
/// Following: a unit of offset asks for this many units a second back, and the aim
/// adds this share of the gap between the wanted and the current velocity.
const FOLLOW_GAIN: f32 = 4.0;
const SPEED_GAIN: f32 = 0.5;
/// Offset left alone, in units (Pmove's velocity rounding turns tiny corrections into a
/// lasting drift; see `ghost_replay`).
const DEADBAND: f32 = 2.0;
/// The least push that moves at all against ground friction (`pm_stopspeed`).
const MIN_PUSH: f32 = 60.0;
/// Below this ground speed the leader walks (the walking animation).
const WALK_SPEED: f32 = 140.0;
/// How far ahead the leader's velocity is read, in milliseconds: about the time ground
/// acceleration takes to turn a run.
const LEAD: i32 = 80;
/// An attack this close to its end, in milliseconds, is choosing the next move.
const DECIDE_MS: i32 = 24;
/// The least horizontal acceleration, in units a second squared, read as the leader
/// steering in the air (velocities arrive rounded to whole units).
const AIR_STEER: f32 = 60.0;
/// A backward key smaller than this share (percent) of the sideways one is dropped.
const BACKWARD_SHARE: i32 = 35;
/// A crouched leader faster than this is rolling.
const ROLL_SPEED: f32 = 200.0;
/// Commands between two stance or holster presses, and stance presses before giving
/// up on a stance this player lacks.
const PRESS_SPACING: i32 = 400;
const STANCE_TRIES: u8 = 8;

/// What a snapshot shows of the leader.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Sample {
    /// The snapshot's server time.
    pub(crate) time: i32,
    pub(crate) origin: [f32; 3],
    pub(crate) velocity: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub(crate) angles: [f32; 3],
    pub(crate) on_ground: bool,
    pub(crate) crouched: bool,
    pub(crate) saber_move: u32,
    pub(crate) style: u8,
    pub(crate) holstered: bool,
    pub(crate) weapon: u8,
}

impl Sample {
    /// A player's entity in a snapshot: `TR_INTERPOLATE` keeps the origin in `trBase`
    /// and the velocity in `trDelta`; `apos.trBase` holds the view angles; the linked
    /// box's top (`solid` bits 16-23, `maxs[2] + 32`) tells a crouch (16) from standing
    /// (40); `fireflag` holds the saber stance.
    pub(crate) fn of_entity(time: i32, entity: &EntityState) -> Self {
        let solid = entity.solid();
        let top = ((solid >> 16) & 0xFF) as i32 - 32;
        Self {
            time,
            origin: entity.trajectory_base(),
            velocity: entity.trajectory_delta(),
            angles: entity.angles(),
            on_ground: entity.ground_entity_num() != NO_GROUND,
            crouched: solid != 0 && solid != 0x00FF_FFFF && top < 30,
            saber_move: entity.saber_move(),
            style: entity.fire_flag(),
            holstered: entity.saber_holstered() != 0,
            weapon: entity.weapon(),
        }
    }
}

/// Where both stood, and faced, when the copy began.
#[derive(Clone, Copy, Debug)]
struct Anchors {
    leader: [f32; 3],
    leader_yaw: f32,
    own: [f32; 3],
    own_yaw: f32,
}

struct Following {
    slot: u16,
    name: String,
    delay: i32,
    mirror: bool,
    /// `walk`: walk always (keys within [`WALK_KEY`]), whatever the leader's speed.
    always_walk: bool,
    samples: VecDeque<Sample>,
    anchors: Option<Anchors>,
    /// Command stamp minus sample time at the start: maps commands onto the leader's
    /// timeline.
    offset: Option<i32>,
    /// The leader's ground at the previous command, for takeoffs.
    leader_was_on_ground: bool,
    jumping: bool,
    last_press: Press,
    last_up: i8,
    stance_tries: u8,
    stance_wanted: u8,
    last_stance_press: i32,
    last_holster_press: i32,
}

/// The leader on the playback timeline: at the playback time, a little ahead (for
/// velocity), as this client's attack ends (for the next move), and how their velocity
/// turns there.
struct Seen {
    now: Sample,
    ahead: Sample,
    soon: Sample,
    steering: [f32; 3],
}

/// The controls of one command, before the view is added.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Press {
    forward: i8,
    right: i8,
    up: i8,
    buttons: u16,
}

/// The mimic's state, owned by the viewer.
#[derive(Default)]
pub(crate) struct Mimic {
    following: Option<Box<Following>>,
    messages: Vec<String>,
}

impl Mimic {
    pub(crate) fn active(&self) -> bool {
        self.following.is_some()
    }

    /// The followed slot.
    pub(crate) fn leader(&self) -> Option<u16> {
        self.following.as_ref().map(|following| following.slot)
    }

    pub(crate) fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    /// Begin copying `slot`.
    pub(crate) fn start(&mut self, slot: u16, name: String, delay: i32, mirror: bool, walk: bool) {
        self.following = Some(Box::new(Following {
            slot,
            name,
            delay: delay.clamp(0, MAX_DELAY),
            mirror,
            always_walk: walk,
            samples: VecDeque::with_capacity(512),
            anchors: None,
            offset: None,
            leader_was_on_ground: true,
            jumping: false,
            last_press: Press::default(),
            last_up: 0,
            stance_tries: 0,
            stance_wanted: 0,
            last_stance_press: i32::MIN / 2,
            last_holster_press: i32::MIN / 2,
        }));
    }

    pub(crate) fn stop_with(&mut self, message: impl Into<String>) {
        if self.following.take().is_some() {
            self.messages.push(message.into());
        }
    }

    /// Keep what a live snapshot shows of the leader (`entity`, `None` when out of
    /// sight) and stop when the local player can no longer copy.
    pub(crate) fn observe(
        &mut self,
        time: i32,
        entity: Option<&EntityState>,
        player: &PlayerState,
    ) {
        if self.following.is_none() {
            return;
        }
        if player.health() <= 0 {
            return self.stop_with("^3mimic: stopped, you are dead.");
        }
        if player.is_spectator()
            || !crate::local_prediction::predicts_local_view(player.movement_flags())
        {
            return self.stop_with("^3mimic: stopped, you are spectating.");
        }
        if player.duel_in_progress() {
            return self.stop_with("^3mimic: stopped, a duel is on.");
        }
        if let Some(entity) = entity {
            self.push(Sample::of_entity(time, entity));
        }
    }

    /// Keep one sample (tests feed them directly).
    pub(crate) fn push(&mut self, sample: Sample) {
        let Some(following) = &mut self.following else {
            return;
        };
        if following
            .samples
            .back()
            .is_some_and(|last| sample.time <= last.time)
        {
            return;
        }
        if following.samples.len() == MAX_SAMPLES {
            following.samples.pop_front();
        }
        following.samples.push_back(sample);
    }

    /// Replace `input` while copying; `state` is the local player's state before it.
    pub(crate) fn drive(&mut self, input: &UserCommand, state: &MovementState) -> Option<Drive> {
        let following = self.following.as_mut()?;
        if crate::ghost_replay::took_control(input) {
            self.stop_with("^3mimic: stopped, you took control.");
            return None;
        }
        let stamp = input.server_time;
        let Some(newest) = following.samples.back().copied() else {
            return Some(still(input, following.always_walk));
        };
        let offset = *following.offset.get_or_insert(stamp - newest.time);
        let anchors = *following.anchors.get_or_insert(Anchors {
            leader: newest.origin,
            leader_yaw: newest.angles[1],
            own: state.origin,
            own_yaw: state.view_angles[1],
        });
        let time = stamp - offset - following.delay;
        if time - newest.time > LOST_SIGHT {
            let name = following.name.clone();
            self.stop_with(format!("^3mimic: stopped, lost sight of {name}^3."));
            return None;
        }
        while following.samples.len() > 2 && following.samples[1].time < time - KEEP_BEHIND {
            following.samples.pop_front();
        }
        let Some(leader) = sample_at(&following.samples, time) else {
            // The delay has not passed since the copy began.
            return Some(still(input, following.always_walk));
        };
        // Velocity a little ahead, where the delay leaves samples there: the keys then
        // turn when the leader's did rather than an acceleration later.
        let ahead = sample_at(&following.samples, (time + LEAD).min(newest.time.max(time)))
            .unwrap_or(leader);
        // The move the leader is in as this client's attack ends: the next one is
        // chosen then, a few commands from now.
        let steering = acceleration_at(&following.samples, time);
        let soon = sample_at(
            &following.samples,
            (time + DECIDE_MS + 8).min(newest.time.max(time)),
        )
        .unwrap_or(leader);
        let seen = Seen {
            now: leader,
            ahead,
            soon,
            steering,
        };
        Some(following.command(input, state, &seen, &anchors, time))
    }
}

impl Following {
    fn command(
        &mut self,
        input: &UserCommand,
        state: &MovementState,
        seen: &Seen,
        anchors: &Anchors,
        time: i32,
    ) -> Drive {
        let Seen {
            now: leader,
            ahead,
            soon,
            steering,
        } = seen;
        let steering = *steering;
        let side = if self.mirror { -1.0 } else { 1.0 };
        // The leader's motion in their starting frame, laid into the mimic's.
        let moved = sub(leader.origin, anchors.leader);
        let target = add(
            anchors.own,
            to_frame(from_frame(moved, anchors.leader_yaw, side), anchors.own_yaw),
        );
        let target_velocity = to_frame(
            from_frame(ahead.velocity, anchors.leader_yaw, side),
            anchors.own_yaw,
        );
        let yaw = anchors.own_yaw + side * angle_delta(leader.angles[1], anchors.leader_yaw);
        let pitch = leader.angles[0];

        let on_ground = state.ground_entity_number != NO_GROUND;
        let leader_speed = leader.velocity[0].hypot(leader.velocity[1]);
        let own_speed = state.velocity[0].hypot(state.velocity[1]);
        // A crouch waits for the mimic to slow down (crouching while running rolls,
        // `PM_Footsteps`), so the offset is not chased meanwhile.
        let crouch_waits =
            leader.crouched && on_ground && own_speed >= WALK_SPEED && leader_speed <= ROLL_SPEED;
        let (forward, right) = if on_ground {
            follow_keys(
                target,
                target_velocity,
                state,
                yaw,
                on_ground,
                !crouch_waits,
            )
        } else {
            // In the air, chasing the offset only bends the flight (air control adds
            // speed along the keys): steer the way the leader's velocity turns instead.
            let turn = to_frame(
                from_frame(steering, anchors.leader_yaw, side),
                anchors.own_yaw,
            );
            air_keys(turn, yaw)
        };
        let mut press = Press {
            forward,
            right,
            up: 0,
            buttons: 0,
        };
        if self.always_walk || leader.on_ground && leader_speed > 5.0 && leader_speed < WALK_SPEED {
            press.buttons |= BUTTON_WALKING;
        }

        // A takeoff starts a jump, held while the leader rises.
        let takeoff =
            self.leader_was_on_ground && !leader.on_ground && leader.velocity[2] > TAKEOFF_SPEED;
        if takeoff {
            self.jumping = true;
        }
        if leader.on_ground || leader.velocity[2] <= 0.0 {
            self.jumping = false;
        }
        self.leader_was_on_ground = leader.on_ground;
        if self.jumping {
            press.up = 127;
        } else if leader.crouched && !crouch_waits {
            press.up = -127;
        }

        // A directional attack's keys matter only where a move is chosen: starting from
        // the ready or a return, and as an attack ends (`PM_WeaponLightsaber`), where
        // the leader a few commands ahead says what comes next. In between the keys
        // follow.
        let own = state.saber_move;
        let ending = saber::is_attack(own) && state.weapon_time <= DECIDE_MS;
        let deciding = !saber::in_chain(own) || ending;
        let plan = if ending {
            saber::plan(soon.saber_move, self.mirror)
        } else {
            saber::plan(leader.saber_move, self.mirror)
        };
        match plan {
            saber::Plan::Release => {}
            saber::Plan::Keep => {
                press.buttons |=
                    self.last_press.buttons & (saber::BUTTON_ATTACK | saber::BUTTON_ALT_ATTACK);
            }
            saber::Plan::Hold(hold) => {
                press.buttons |= hold.buttons;
                if deciding || hold.whole_move {
                    press.forward = hold.forward;
                    press.right = hold.right;
                    if let Some(up) = hold.up {
                        press.up = up;
                    }
                }
            }
        }
        // Walking keeps every key within half (a saber attack is chosen by the keys'
        // signs alone), or Pmove would clear the walk and run.
        if press.buttons & BUTTON_WALKING != 0 {
            (press.forward, press.right) = cap_keys(press.forward, press.right, WALK_KEY);
        }
        // Pmove jumps once per press (`PMF_JUMP_HELD`): a takeoff while jump is still
        // held from the last one lets go for a command first.
        if takeoff && self.last_up > 0 {
            press.up = 0;
            self.leader_was_on_ground = true; // Take off on the next command.
        }
        self.last_up = press.up;
        self.last_press = press;

        let generic_command = self.saber_upkeep(leader, state, time);
        let view = [pitch, yaw];
        Drive {
            command: UserCommand {
                server_time: input.server_time,
                angles: [
                    angle_short(pitch).wrapping_sub(state.delta_angles[0]) & 0xFFFF,
                    angle_short(yaw).wrapping_sub(state.delta_angles[1]) & 0xFFFF,
                    0_i32.wrapping_sub(state.delta_angles[2]) & 0xFFFF,
                ],
                buttons: press.buttons | (input.buttons & sjk_game_jka::pmove_talk::BUTTON_TALK),
                weapon: if leader.weapon != 0 {
                    leader.weapon
                } else {
                    input.weapon
                },
                generic_command,
                forward_move: press.forward,
                right_move: press.right,
                up_move: press.up,
                ..*input
            },
            view: Some(view),
        }
    }

    /// Match the leader's stance and holster with one generic command at a time.
    fn saber_upkeep(&mut self, leader: &Sample, state: &MovementState, time: i32) -> u8 {
        if leader.weapon != WP_SABER || state.weapon != WP_SABER {
            return 0;
        }
        let holstered = state.saber_holstered != 0;
        if leader.holstered != holstered {
            if time - self.last_holster_press >= PRESS_SPACING {
                self.last_holster_press = time;
                return GENCMD_SABER_SWITCH;
            }
            return 0;
        }
        if leader.style != self.stance_wanted {
            self.stance_wanted = leader.style;
            self.stance_tries = 0;
        }
        if leader.style != 0
            && leader.style != state.saber_anim_level
            && self.stance_tries < STANCE_TRIES
            && time - self.last_stance_press >= PRESS_SPACING
        {
            self.last_stance_press = time;
            self.stance_tries += 1;
            return GENCMD_SABER_ATTACK_CYCLE;
        }
        0
    }
}

/// Stand still, keeping the view the player has (and the walk when walking always).
fn still(input: &UserCommand, walk: bool) -> Drive {
    Drive {
        command: UserCommand {
            forward_move: 0,
            right_move: 0,
            up_move: 0,
            buttons: input.buttons & sjk_game_jka::pmove_talk::BUTTON_TALK
                | if walk { BUTTON_WALKING } else { 0 },
            generic_command: 0,
            ..*input
        },
        view: None,
    }
}

/// The leader at `time`: interpolated between the samples around it, extrapolated a
/// little past the newest; discrete fields from the sample at or before it. `None`
/// before the first sample.
fn sample_at(samples: &VecDeque<Sample>, time: i32) -> Option<Sample> {
    let after = samples.partition_point(|sample| sample.time <= time);
    let before = *samples.get(after.checked_sub(1)?)?;
    let Some(next) = samples.get(after) else {
        let ahead = (time - before.time).min(EXTRAPOLATE) as f32 / 1000.0;
        return Some(Sample {
            origin: std::array::from_fn(|i| before.origin[i] + before.velocity[i] * ahead),
            ..before
        });
    };
    let span = (next.time - before.time).max(1) as f32;
    let t = (time - before.time) as f32 / span;
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Some(Sample {
        origin: std::array::from_fn(|i| lerp(before.origin[i], next.origin[i])),
        velocity: std::array::from_fn(|i| lerp(before.velocity[i], next.velocity[i])),
        angles: std::array::from_fn(|i| {
            angle_delta(
                before.angles[i] + angle_delta(next.angles[i], before.angles[i]) * t,
                0.0,
            )
        }),
        ..before
    })
}

/// Movement keys that follow `target` moving at `velocity`, from `state`, in a view
/// facing `yaw`: aim for the target velocity plus a return toward the target, and on
/// the ground add part of the gap to the current velocity so a sideways speed is
/// taken out. The keys' largest component sets the speed (`PM_CmdScale`).
fn follow_keys(
    target: [f32; 3],
    velocity: [f32; 3],
    state: &MovementState,
    yaw: f32,
    on_ground: bool,
    chase: bool,
) -> (i8, i8) {
    let offset = [target[0] - state.origin[0], target[1] - state.origin[1]];
    let distance = offset[0].hypot(offset[1]);
    let gain = if chase && distance > DEADBAND {
        FOLLOW_GAIN
    } else {
        0.0
    };
    let wanted = [
        velocity[0] + gain * offset[0],
        velocity[1] + gain * offset[1],
    ];
    let wish = if on_ground {
        [
            wanted[0] + SPEED_GAIN * (wanted[0] - state.velocity[0]),
            wanted[1] + SPEED_GAIN * (wanted[1] - state.velocity[1]),
        ]
    } else {
        wanted
    };
    let size = wish[0].hypot(wish[1]);
    let speed = state.speed.max(1.0);
    if size < 15.0 && (distance <= DEADBAND || !chase) {
        return (0, 0);
    }
    // A crouch or a slow stance can make the full speed itself less than the push.
    let push = size.max(MIN_PUSH).min(speed) / speed * 127.0;
    keys_toward(wish, yaw, push)
}

/// Keys pushing toward the world direction `wish` at `push` (the largest key), in a
/// view facing `yaw`. A slight backward component is dropped: any backward key slows
/// the player (`BG_AdjustClientSpeed`: 0.75, and 0.45-0.75 in an attack), which a
/// sideways run with a tiny correction would otherwise pay.
fn keys_toward(wish: [f32; 2], yaw: f32, push: f32) -> (i8, i8) {
    let angle = wish[1].atan2(wish[0]) - yaw.to_radians();
    let (sin, cos) = angle.sin_cos();
    let scale = push / cos.abs().max(sin.abs()).max(1e-6);
    let key = |value: f32| value.round().clamp(-127.0, 127.0) as i8;
    let (forward, right) = (key(cos * scale), key(-sin * scale));
    if forward < 0 && i32::from(forward).abs() * 100 < i32::from(right).abs() * BACKWARD_SHARE {
        (0, right)
    } else {
        (forward, right)
    }
}

/// The leader's horizontal acceleration at `time`, in units a second squared, from
/// the samples around it (gravity left out); zero without two samples.
fn acceleration_at(samples: &VecDeque<Sample>, time: i32) -> [f32; 3] {
    let after = samples.partition_point(|sample| sample.time <= time).max(1);
    let (Some(before), Some(next)) = (
        samples.get(after - 1),
        samples.get(after).or_else(|| samples.get(after - 1)),
    ) else {
        return [0.0; 3];
    };
    let (before, next) = if next.time > before.time {
        (before, next)
    } else {
        match after.checked_sub(2).and_then(|index| samples.get(index)) {
            Some(earlier) => (earlier, before),
            None => return [0.0; 3],
        }
    };
    let seconds = (next.time - before.time) as f32 / 1000.0;
    [
        (next.velocity[0] - before.velocity[0]) / seconds,
        (next.velocity[1] - before.velocity[1]) / seconds,
        0.0,
    ]
}

/// Air keys toward `turn`, the leader's horizontal acceleration: air control adds
/// speed along the keys (`PM_AirMove`), so its direction is theirs; too small to tell
/// from the velocity's rounding, no keys.
fn air_keys(turn: [f32; 3], yaw: f32) -> (i8, i8) {
    if turn[0].hypot(turn[1]) < AIR_STEER {
        return (0, 0);
    }
    keys_toward([turn[0], turn[1]], yaw, 127.0)
}

/// `forward` and `right` scaled down together so the larger is at most `limit`, keeping
/// their direction and signs.
fn cap_keys(forward: i8, right: i8, limit: i8) -> (i8, i8) {
    let largest = forward.unsigned_abs().max(right.unsigned_abs());
    if largest <= limit.unsigned_abs() {
        return (forward, right);
    }
    let scale = f32::from(limit) / f32::from(largest);
    let shrink = |key: i8| {
        let scaled = (f32::from(key) * scale).round() as i8;
        if scaled == 0 && key != 0 {
            key.signum()
        } else {
            scaled
        }
    };
    (shrink(forward), shrink(right))
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// A world vector as (forward, left, up) in a frame facing `yaw`, `side` -1 mirroring
/// left for right.
fn from_frame(v: [f32; 3], yaw: f32, side: f32) -> [f32; 3] {
    let (sin, cos) = yaw.to_radians().sin_cos();
    [
        v[0] * cos + v[1] * sin,
        side * (-v[0] * sin + v[1] * cos),
        v[2],
    ]
}

/// (forward, left, up) in a frame facing `yaw` back to world axes.
fn to_frame(v: [f32; 3], yaw: f32) -> [f32; 3] {
    let (sin, cos) = yaw.to_radians().sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos, v[2]]
}

/// `a - b` in degrees, in (-180, 180].
fn angle_delta(a: f32, b: f32) -> f32 {
    (a - b + 540.0).rem_euclid(360.0) - 180.0
}

fn angle_short(degrees: f32) -> i32 {
    ((degrees * (65_536.0 / 360.0)).round() as i32) & 0xFFFF
}

#[cfg(test)]
#[path = "mimic/tests.rs"]
mod tests;

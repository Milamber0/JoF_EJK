//! Keeping a replay on its recorded path through the inputs alone: the server owns the
//! position, so drift is corrected by turning the movement keys' direction (the view
//! stays the recorded one) and by letting the recorded clock wait or run ahead on the
//! ground.

/// How fast lateral drift is steered out: a unit of error asks for this many units a
/// second toward the path, up to [`MAX_RETURN`]. Gentle on purpose: nothing steers in
/// the air, so what matters when a jump leaves the ground is the sideways speed, not
/// the offset; a constant offset of a few units lands as safely as none.
const GAIN: f32 = 1.0;
const MAX_RETURN: f32 = 20.0;
/// How much of the gap between the wanted and the current sideways speed the aim adds,
/// so that a sideways speed already there (a shove) is taken back out first.
const SPEED_GAIN: f32 = 1.0;
/// The most the movement direction turns, in degrees.
const MAX_TURN: f32 = 20.0;
/// Lateral drift and lateral speed left alone, in units and units a second. Pmove
/// rounds the velocity to whole units after every command (`SnapVector`), and friction
/// cannot shrink a small sideways speed past that rounding, so a tiny correction leaves
/// a sideways speed that lasts the whole run: an accurate start must replay untouched.
const DEADBAND: f32 = 2.0;
const SPEED_DEADBAND: f32 = 1.5;
/// Along-path drift on the ground beyond which the recorded clock waits (behind)
/// or skips a command (ahead), in units.
pub(super) const WARP_UNITS: f32 = 2.0;
/// Recorded ground speed below which along-path drift is not measured.
const WARP_MIN_SPEED: f32 = 60.0;

/// Horizontal length of `v`.
pub(super) fn horizontal(v: [f32; 3]) -> f32 {
    v[0].hypot(v[1])
}

/// Movement keys turned by `turn` degrees (positive is to the left, as yaw grows)
/// within the view, keeping how fast they move: `PM_CmdScale` scales the wish speed by
/// the largest key, so the largest key keeps its size.
pub(super) fn turn_moves(forward: i8, right: i8, turn: f32) -> (i8, i8) {
    let size = forward.unsigned_abs().max(right.unsigned_abs());
    if size == 0 || turn == 0.0 {
        return (forward, right);
    }
    // World wish = forward * F + right * R with R = (sin yaw, -cos yaw): its angle in
    // the view is atan2(-right, forward).
    let angle = (-f32::from(right)).atan2(f32::from(forward)) + turn.to_radians();
    let (sin, cos) = angle.sin_cos();
    let scale = f32::from(size) / cos.abs().max(sin.abs());
    let clamp = |value: f32| value.round().clamp(-127.0, 127.0) as i8;
    (clamp(cos * scale), clamp(-sin * scale))
}

/// How far to turn the movement keys on the ground, in degrees (positive to the
/// left), to bring the player back to the path: `error` is the recorded position minus
/// the current one, `recorded` and `current` the two velocities. Only the ground is
/// steered: in the air (`pm_airaccelerate` 1) a turn small enough to keep the speed
/// adds less than the velocity rounding takes back.
pub(super) fn turn_toward_path(error: [f32; 3], recorded: [f32; 3], current: [f32; 3]) -> f32 {
    let speed = horizontal(recorded);
    if speed < 1.0 {
        return 0.0;
    }
    let left = [-recorded[1] / speed, recorded[0] / speed];
    let drift = error[0] * left[0] + error[1] * left[1];
    let sideways = (current[0] - recorded[0]) * left[0] + (current[1] - recorded[1]) * left[1];
    if drift.abs() < DEADBAND && sideways.abs() < SPEED_DEADBAND {
        return 0.0;
    }
    let wanted = (GAIN * drift).clamp(-MAX_RETURN, MAX_RETURN);
    let aim = wanted + SPEED_GAIN * (wanted - sideways);
    aim.atan2(speed).to_degrees().clamp(-MAX_TURN, MAX_TURN)
}

/// Along-path drift in units: positive when the recorded position is ahead of the
/// player, `None` when the recorded move is too slow to measure it.
pub(super) fn along_path(error: [f32; 3], recorded: [f32; 3]) -> Option<f32> {
    let speed = horizontal(recorded);
    (speed >= WARP_MIN_SPEED).then(|| (error[0] * recorded[0] + error[1] * recorded[1]) / speed)
}

/// Movement keys that walk toward `target` from `origin`, relative to the view yaw
/// `yaw` (degrees), slowing as they arrive: `ground_speed` is the player's full speed.
pub(super) fn walk_toward(
    origin: [f32; 3],
    target: [f32; 3],
    yaw: f32,
    ground_speed: f32,
) -> (i8, i8) {
    let delta = [target[0] - origin[0], target[1] - origin[1]];
    let distance = delta[0].hypot(delta[1]);
    if distance < 1e-3 || ground_speed <= 0.0 {
        return (0, 0);
    }
    // Ground friction stops anything slower than `pm_stopspeed` within a command, so
    // below a wish speed of about 60 each command starts from rest and barely moves:
    // the walk keeps that much push to the end. Farther, six times the distance brakes
    // in time (friction sheds about a sixth of the speed per unit travelled).
    let wanted_speed = (distance * 6.0).clamp(60.0, 200.0);
    let size = (wanted_speed / ground_speed * 127.0).clamp(1.0, 127.0);
    let angle = delta[1].atan2(delta[0]) - yaw.to_radians();
    let (sin, cos) = angle.sin_cos();
    let clamp = |value: f32| value.round().clamp(-127.0, 127.0) as i8;
    (clamp(cos * size), clamp(-sin * size))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world direction (degrees) the keys push toward at view yaw 0.
    fn wish_angle(forward: i8, right: i8) -> f32 {
        (-f32::from(right)).atan2(f32::from(forward)).to_degrees()
    }

    #[test]
    fn turning_keeps_the_largest_key_and_turns_by_the_angle() {
        for (forward, right) in [(127, 0), (127, 127), (0, -127), (46, 0), (-127, 64)] {
            for turn in [-20.0_f32, -5.0, 3.0, 20.0] {
                let (f, r) = turn_moves(forward, right, turn);
                let size = forward.unsigned_abs().max(right.unsigned_abs());
                assert_eq!(f.unsigned_abs().max(r.unsigned_abs()), size);
                let got = wish_angle(f, r) - wish_angle(forward, right);
                let got = (got + 540.0).rem_euclid(360.0) - 180.0;
                assert!((got - turn).abs() < 1.0, "{forward},{right} {turn}: {got}");
            }
        }
        assert_eq!(turn_moves(0, 0, 10.0), (0, 0));
        assert_eq!(turn_moves(127, 0, 0.0), (127, 0));
    }

    #[test]
    fn a_path_to_the_left_turns_left_and_the_turn_is_bounded() {
        let run = [250.0, 0.0, 0.0];
        // Running along +x; the path is 3 units to the left (+y).
        let turn = turn_toward_path([0.0, 3.0, 0.0], run, run);
        assert!(turn > 0.0 && turn < MAX_TURN, "{turn}");
        // Far off and still moving away: the turn is capped.
        assert_eq!(
            turn_toward_path([0.0, -500.0, 0.0], run, [250.0, 200.0, 0.0]),
            -MAX_TURN
        );
        // Along-path error alone does not turn; standing still does not either.
        assert_eq!(turn_toward_path([5.0, 0.0, 0.0], run, run), 0.0);
        assert_eq!(turn_toward_path([0.0, 5.0, 0.0], [0.0; 3], [0.0; 3]), 0.0);
    }

    #[test]
    fn small_drift_is_left_alone_but_a_sideways_speed_is_taken_out() {
        let run = [250.0, 0.0, 0.0];
        assert_eq!(turn_toward_path([0.0, 1.5, 0.0], run, run), 0.0);
        // On the line, but drifting right at 3 units a second: steer left.
        assert!(turn_toward_path([0.0, 0.0, 0.0], run, [250.0, -3.0, 0.0]) > 0.0);
        // The path 3 units to the right and the player already heading there at 40
        // units a second, far faster than the 3 a second the drift asks for: the keys
        // turn left to slow the return instead of overshooting the line.
        assert!(turn_toward_path([0.0, -3.0, 0.0], run, [250.0, -40.0, 0.0]) > 0.0);
    }

    #[test]
    fn along_path_measures_who_is_ahead() {
        assert_eq!(along_path([3.0, 1.0, 0.0], [250.0, 0.0, 0.0]), Some(3.0));
        assert_eq!(along_path([-3.0, 1.0, 0.0], [250.0, 0.0, 0.0]), Some(-3.0));
        assert_eq!(along_path([3.0, 0.0, 0.0], [10.0, 0.0, 0.0]), None);
    }

    #[test]
    fn walking_heads_for_the_target_and_slows_near_it() {
        // Target straight to the right of a player looking along +x.
        let (f, r) = walk_toward([0.0; 3], [0.0, -100.0, 0.0], 0.0, 250.0);
        assert_eq!(f, 0);
        assert!(r > 90, "{r}");
        let (f, r) = walk_toward([0.0; 3], [0.5, 0.0, 0.0], 0.0, 250.0);
        assert_eq!((f, r), (30, 0), "enough push to move at all");
        assert_eq!(walk_toward([1.0; 3], [1.0; 3], 0.0, 250.0), (0, 0));
        // Looking along +y, a target along +x is to the right.
        let (f, r) = walk_toward([0.0; 3], [100.0, 0.0, 0.0], 90.0, 250.0);
        assert_eq!(f, 0);
        assert!(r > 0);
    }
}

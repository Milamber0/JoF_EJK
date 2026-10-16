//! `BG_EvaluateTrajectory` and its companions, shared by movement and presentation.

/// Evaluate a protocol-26 trajectory exactly as `BG_EvaluateTrajectory` in
/// `codemp/game/bg_misc.c:2227-2281` does.
///
/// Snapshot world presentation and cgame-style missile presentation share
/// this adapter-level implementation so the legacy arithmetic cannot drift.
pub fn legacy_evaluate_trajectory(
    base: [f32; 3],
    delta: [f32; 3],
    kind: u8,
    start_time: i32,
    duration: i32,
    at_time: i32,
) -> [f32; 3] {
    let elapsed_millis = at_time.wrapping_sub(start_time);
    let factor = match kind {
        // TR_STATIONARY and TR_INTERPOLATE are represented directly by their
        // snapshot bases. World sampling interpolates consecutive snapshots.
        0 | 1 => return base,
        2 => seconds(elapsed_millis),
        3 => elapsed_millis.clamp(0, duration.max(0)) as f32 / 1_000.0,
        4 => {
            if duration <= 0 || elapsed_millis <= 0 {
                0.0
            } else {
                let elapsed = elapsed_millis.min(duration) as f32;
                duration as f32 / 1_000.0
                    * (elapsed / duration as f32 * std::f32::consts::FRAC_PI_2).sin()
            }
        }
        5 => {
            if duration == 0 {
                0.0
            } else {
                (elapsed_millis as f32 / duration as f32 * std::f32::consts::TAU).sin()
            }
        }
        6 => seconds(elapsed_millis),
        _ => return base,
    };
    let mut result = std::array::from_fn(|axis| base[axis] + factor * delta[axis]);
    if kind == 6 {
        // `0.5 * DEFAULT_GRAVITY * deltaTime * deltaTime`: a double product, rounded
        // once as it is taken off the float.
        result[2] =
            (f64::from(result[2]) - 0.5 * 800.0 * f64::from(factor) * f64::from(factor)) as f32;
    }
    result
}

/// `deltaTime = ( atTime - tr->trTime ) * 0.001`: the double product rounded to a float.
fn seconds(millis: i32) -> f32 {
    (f64::from(millis) * 0.001) as f32
}

/// Evaluate an angular protocol-26 trajectory as codemp does for `apos` in
/// `CG_CalcEntityLerpPositions` (`codemp/cgame/cg_ents.c:3128-3131`).
///
/// `trajectory_t` uses the same arithmetic for position and Euler-angle
/// trajectories; keeping this named adapter beside the position evaluator
/// makes mover call sites explicit without duplicating that arithmetic.
pub fn legacy_evaluate_trajectory_angles(
    base: [f32; 3],
    delta: [f32; 3],
    kind: u8,
    start_time: i32,
    duration: i32,
    at_time: i32,
) -> [f32; 3] {
    legacy_evaluate_trajectory(base, delta, kind, start_time, duration, at_time)
}

/// Evaluate protocol-26 trajectory velocity exactly as
/// `BG_EvaluateTrajectoryDelta` (`codemp/game/bg_misc.c:2288-2338`).
pub fn legacy_evaluate_trajectory_delta(
    delta: [f32; 3],
    kind: u8,
    start_time: i32,
    duration: i32,
    at_time: i32,
) -> [f32; 3] {
    let elapsed_millis = at_time.wrapping_sub(start_time);
    match kind {
        0 | 1 => [0.0; 3],
        2 => delta,
        3 => {
            if at_time > start_time.wrapping_add(duration) {
                [0.0; 3]
            } else {
                delta
            }
        }
        4 => {
            if duration <= 0 || elapsed_millis > duration || elapsed_millis <= 0 {
                [0.0; 3]
            } else {
                let factor = duration as f32
                    * 0.001
                    * (elapsed_millis as f32 / duration as f32 * std::f32::consts::FRAC_PI_2).sin();
                delta.map(|component| component * factor)
            }
        }
        5 => {
            if duration == 0 {
                [0.0; 3]
            } else {
                let phase =
                    (elapsed_millis as f32 / duration as f32 * std::f32::consts::TAU).cos() * 0.5;
                delta.map(|component| component * phase)
            }
        }
        6 => {
            // `DEFAULT_GRAVITY * deltaTime`: an integer times the float seconds.
            let mut result = delta;
            result[2] -= 800.0 * seconds(elapsed_millis);
            result
        }
        _ => [0.0; 3],
    }
}

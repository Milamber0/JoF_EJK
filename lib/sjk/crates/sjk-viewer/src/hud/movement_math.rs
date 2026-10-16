//! TaystJK hud_strafehelper.c geometry, independent of drawing and player-state ownership.

/// `CGAZ_Opt` (:1314-1333), including the reference's 45-degree offset and guards.
pub fn optimal(vf: f32, acceleration: f32, wishspeed: f32) -> f32 {
    if vf == 0.0 {
        return 0.0;
    }
    let angle = ((wishspeed - acceleration) / vf).clamp(-1.0, 1.0).acos();
    let angle = angle as f64 * (180.0 / std::f64::consts::PI) - 45.0;
    let angle = angle as f32;
    if !(0.0..=360.0).contains(&angle) {
        0.0
    } else {
        angle
    }
}

/// Reference command-direction offsets (`DF_GetLine`, :1088-1186), front optimum lines.
pub fn direction(delta: f32, key: usize) -> f32 {
    match key {
        0 => 45.0 + delta,
        1 => delta,
        2 => -45.0 + delta,
        3 => -90.0 + delta,
        4 => 225.0 + delta,
        5 => 90.0 - delta,
        6 => 45.0 - delta,
        _ => -delta,
    }
}

/// JoF EJK's JKA-physics lines (`CG_StrafeHelper`, JoF `cg_draw.c`): the
/// `cg_strafeHelper` bit, yaw offset from the velocity, the movement direction that
/// lights the line (numbered as in [`direction`]) and the inactive RGB tint. JoF draws
/// the centre line only for other physics, so it is not here.
pub fn jof_lines(delta: f32) -> [(i64, f32, usize, [f32; 3]); 8] {
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
    const CYAN: [f32; 3] = [0.5, 1.0, 1.0];
    const PINK: [f32; 3] = [1.0, 0.5, 0.5];
    [
        (1 << 6, delta, 1, WHITE),
        (1 << 7, -delta, 7, WHITE),
        (1 << 8, -(45.0 - delta), 2, CYAN),
        (1 << 9, 45.0 - delta, 6, CYAN),
        (1 << 10, 225.0 - delta, 2, CYAN),
        (1 << 10, 135.0 + delta, 6, CYAN),
        (1 << 5, 45.0 + delta, 0, PINK),
        (1 << 5, -45.0 - delta, 0, PINK),
    ]
}

/// First-person projection of a horizontal ray, preserving the reference depth cutoff.
pub fn project(angle: f32, pitch: f32, distance: f32, fov: f32) -> Option<f32> {
    let angle = angle.to_radians();
    let depth = distance * angle.cos() * pitch.to_radians().cos();
    if depth <= 0.001 {
        return None;
    }
    Some(320.0 - distance * angle.sin() / depth * 320.0 / (fov * 0.5).to_radians().tan())
}

/// `CG_WorldCoordToScreenCoord` (cg_draw.c:8307-8332) against the actual rendered axes.
pub fn project_basis(
    angle: f32,
    distance: f32,
    forward: [f32; 3],
    left: [f32; 3],
    fov: f32,
) -> Option<f32> {
    let ray = [
        angle.to_radians().cos() * distance,
        angle.to_radians().sin() * distance,
    ];
    let depth = ray[0] * forward[0] + ray[1] * forward[1];
    if depth <= 0.001 {
        return None;
    }
    let lateral = ray[0] * left[0] + ray[1] * left[1];
    Some(320.0 - lateral * 320.0 / (depth * (fov * 0.5).to_radians().tan()))
}

/// Fixed-capacity snap-angle boundaries (`DF_UpdateSnapHudSettings`, :2503-2521).
pub fn zones(speed: f32, fps: f32, out: &mut [f32; 128]) -> usize {
    if !speed.is_finite() || speed <= 0.0 || !fps.is_finite() || fps <= 0.0 {
        return 0;
    }
    let speed = speed / fps;
    let mut step = (speed + 0.5).floor() - 0.5;
    let mut count = 0;
    while step > 0.0 && count < 126 {
        out[count] = (step / speed).acos().to_degrees();
        out[count + 1] = (step / speed).asin().to_degrees();
        count += 2;
        step -= 1.0;
    }
    // Stock sortzones truncates the difference to int. Stable merging retains those ties,
    // with bounded stack scratch and O(n log n) work even for unusually large speed overrides.
    let mut scratch = [0.0; 128];
    let mut width = 1;
    while width < count {
        for start in (0..count).step_by(width * 2) {
            let middle = (start + width).min(count);
            let end = (middle + width).min(count);
            let (mut left, mut right) = (start, middle);
            for value in &mut scratch[start..end] {
                if left < middle && (right == end || (out[left] - out[right]) as i32 <= 0) {
                    *value = out[left];
                    left += 1;
                } else {
                    *value = out[right];
                    right += 1;
                }
            }
        }
        out[..count].copy_from_slice(&scratch[..count]);
        width *= 2;
    }
    if count > 0 {
        out[count] = out[0] + 90.0;
    }
    count
}

/// `DF_FillAngleYaw` (:2487-2495) in virtual 640-wide coordinates.
pub fn snap_span(start: f32, end: f32, yaw: f32, fov: f32) -> [f32; 2] {
    let scale = (fov * 0.5).to_radians().tan();
    let a = (yaw + start).to_radians().tan();
    let b = (yaw + end).to_radians().tan();
    [
        320.0 + a / scale * 320.0,
        (640.0 * (b - a) / (scale * 2.0)).abs() + 1.0,
    ]
}

//! Angle math from OpenJK shared/qcommon/q_math.c and codemp/game/bg_pmove.c, shared by
//! the server's `BG_G2PlayerAngles` ([`crate::g2_player_angles`]) and the client's.

/// bg_pmove.c:9147,9043; q_math.c:499-501: AngleMod truncates to an unsigned short, even
/// between snapshots.
pub fn angle_mod(value: f32) -> f32 {
    (360.0 / 65_536.0) * (((value * (65_536.0 / 360.0)) as i32 & 65_535) as f32)
}

/// bg_pmove.c:8982,9436; q_math.c:511-528: AngleNormalize180 also quantizes, preserving
/// positive 180.
pub fn normalized_angle(value: f32) -> f32 {
    let angle = angle_mod(value);
    if angle > 180.0 { angle - 360.0 } else { angle }
}

/// bg_pmove.c:9014,9439; q_math.c:479-489: AngleSubtract uses fmod, without short quantization.
pub fn angle_subtract(left: f32, right: f32) -> f32 {
    let mut angle = (left - right) % 360.0;
    if angle > 180.0 {
        angle -= 360.0;
    }
    if angle < -180.0 {
        angle += 360.0;
    }
    angle
}

/// bg_pmove.c:9195; q_math.c:1158-1172: VectorNormalize uses all three components and exact zero.
pub fn normalize(vector: &mut [f32; 3]) {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        for value in vector {
            *value *= inverse;
        }
    }
}

/// bg_pmove.c:9254,8970; q_math.c:616-653: vectoangles includes vertical/zero-vector conventions.
pub fn vector_angles(value: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = value;
    let (mut yaw, mut pitch);
    if x == 0.0 && y == 0.0 {
        yaw = 0.0;
        pitch = if z > 0.0 { 90.0 } else { 270.0 };
    } else {
        yaw = if x != 0.0 {
            radians_to_degrees(y.atan2(x))
        } else if y > 0.0 {
            90.0
        } else {
            270.0
        };
        if yaw < 0.0 {
            yaw += 360.0;
        }
        pitch = radians_to_degrees(z.atan2((x * x + y * y).sqrt()));
        if pitch < 0.0 {
            pitch += 360.0;
        }
    }
    [-pitch, yaw, 0.0]
}

fn radians_to_degrees(value: f32) -> f32 {
    // C multiplies in float, then promotes for division by the double M_PI.
    (f64::from(value * 180.0) / std::f64::consts::PI) as f32
}

/// bg_pmove.c:9006-9061: tolerance start, .5/1/2 rate, overshoot stop and clamp-1.
pub fn swing_angles(
    destination: f32,
    swing_tolerance: f32,
    clamp_tolerance: f32,
    speed: f32,
    angle: &mut f32,
    swinging: &mut bool,
    frame_millis: f32,
) {
    if !*swinging && angle_subtract(*angle, destination).abs() > swing_tolerance {
        *swinging = true;
    }
    if !*swinging {
        return;
    }
    let swing = angle_subtract(destination, *angle);
    let scale = if swing.abs() < swing_tolerance * 0.5 {
        0.5
    } else if swing.abs() < swing_tolerance {
        1.0
    } else {
        2.0
    };
    let mut movement = frame_millis * scale * speed;
    if swing < 0.0 {
        movement = -movement;
    }
    if (swing >= 0.0 && movement >= swing) || (swing < 0.0 && movement <= swing) {
        movement = swing;
        *swinging = false;
    }
    *angle = angle_mod(*angle + movement);
    let remaining = angle_subtract(destination, *angle);
    if remaining > clamp_tolerance {
        *angle = angle_mod(destination - (clamp_tolerance - 1.0));
    } else if remaining < -clamp_tolerance {
        *angle = angle_mod(destination + (clamp_tolerance - 1.0));
    }
}

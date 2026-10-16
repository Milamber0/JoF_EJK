//! The thrown saber's client-side tilt, `codemp/cgame/cg_players.c:10057-10255`.
//!
//! The server throws the saber with the hand's blade direction written into its
//! angles (`w_saber.c:8576,8606,8630`), so its pitch is a direction component near
//! zero and the hilt would fly upright. `saberFirstThrown` leaves `startang[0] = 90`
//! commented out (`w_saber.c:8632-8634`): the owner's client lays the saber down
//! instead, stepping the pitch towards 90 at half a degree per millisecond in the
//! owner's integer `bolt3`, timed by `bolt2`. The spin (yaw) and roll stay the
//! server's. While the owner pulls it back the saber faces away from the owner,
//! pitched a further 90 degrees and without spin (`:10221-10239`).

use glam::{EulerRot, Quat, Vec3};

/// One owner's `centity_t::bolt3` (pitch in whole degrees, 0 = unset) and
/// `bolt2` (the last `cg.time` it stepped at, 0 = unset).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ThrowTilt {
    pitch: i32,
    stepped_at: i64,
}

impl ThrowTilt {
    /// The owner holds the saber again (`cg_players.c:10448-10449`).
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Step the tilt to `time` and return the pitch the flying hilt is drawn at.
    ///
    /// `server_pitch` is the saber entity's `apos.trBase[PITCH]`; it seeds the
    /// tilt whenever `bolt3` is zero, truncated to an integer as the C does
    /// (`:10082-10087`). Each step adds `(cg.time - bolt2) * 0.5` in double
    /// precision and truncates, clamped at 90 (`:10194-10216`).
    pub(crate) fn advance(&mut self, server_pitch: f32, time: i64) -> i32 {
        if self.pitch == 0 {
            self.pitch = server_pitch as i32;
            if self.pitch == 0 {
                self.pitch = 1;
            }
            self.stepped_at = 0;
        }
        if self.stepped_at == 0 {
            self.stepped_at = time;
        }
        let step = (time - self.stepped_at) as f64 * 0.5;
        if self.pitch < 90 {
            self.pitch = ((f64::from(self.pitch) + step) as i32).min(90);
        } else if self.pitch > 90 {
            self.pitch = ((f64::from(self.pitch) - step) as i32).max(90);
        }
        self.stepped_at = time;
        self.pitch
    }
}

/// The flying hilt's rotation and the blades' (roll zeroed, `:10254-10255`).
///
/// `presented` is the saber entity's evaluated rotation (server spin included).
/// In flight its pitch is replaced by the tilt; while returning, the angles are
/// `vectoangles(saber - owner)` with 90 added to the pitch and no spin.
pub(crate) fn flight_rotations(
    presented: Quat,
    tilt_pitch: i32,
    returning_from: Option<(Vec3, Vec3)>,
) -> (Quat, Quat) {
    let (yaw, pitch, roll) = match returning_from {
        Some((saber, owner)) => {
            let [pitch, yaw, _] = vector_to_angles((saber - owner).normalize_or_zero());
            (yaw.to_radians(), (pitch + 90.0).to_radians(), 0.0)
        }
        None => {
            let (yaw, _, roll) = presented.to_euler(EulerRot::ZYX);
            (yaw, (tilt_pitch as f32).to_radians(), roll)
        }
    };
    (
        Quat::from_euler(EulerRot::ZYX, yaw, pitch, roll),
        Quat::from_euler(EulerRot::ZYX, yaw, pitch, 0.0),
    )
}

/// `vectoangles` (`q_math.c`): pitch positive looking down, in degrees.
fn vector_to_angles(value: Vec3) -> [f32; 3] {
    if value.x == 0.0 && value.y == 0.0 {
        return [if value.z > 0.0 { -90.0 } else { -270.0 }, 0.0, 0.0];
    }
    let mut yaw = value.y.atan2(value.x).to_degrees();
    if yaw < 0.0 {
        yaw += 360.0;
    }
    let forward = value.x.hypot(value.y);
    let mut pitch = value.z.atan2(forward).to_degrees();
    if pitch < 0.0 {
        pitch += 360.0;
    }
    [-pitch, yaw, 0.0]
}

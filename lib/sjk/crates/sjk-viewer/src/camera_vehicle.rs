//! Vehicle-authored camera fields from `codemp/cgame/cg_view.c`.

/// Cached beside the vehicle model when its `.veh` definition is read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Profile {
    pub(crate) override_camera: bool,
    pub(crate) range: f32,
    pub(crate) vertical: f32,
    pub(crate) horizontal: f32,
    pub(crate) pitch: f32,
    pub(crate) pitch_dependent: bool,
    pub(crate) animal: bool,
}

impl Profile {
    /// Cache camera settings from the shared multiplayer vehicle parser.
    pub(crate) fn from_vehicle(info: &sjk_game_jka::vehicle_fields::VehicleInfo) -> Self {
        Self {
            override_camera: info.camera_override,
            range: info.camera_range,
            vertical: info.camera_vert_offset,
            horizontal: info.camera_horz_offset,
            pitch: info.camera_pitch_offset,
            pitch_dependent: info.camera_pitch_dependant_vert_offset,
            animal: info.kind == sjk_game_jka::vehicle_fields::kind::ANIMAL,
        }
    }

    /// Override [angle, pitch offset, range, height] and horizontal offset. Pitch
    /// arguments retain stock's down-positive convention; strafing is hackingTime.
    pub(crate) fn apply(
        self,
        framing: &mut [f32; 4],
        horizontal: &mut f32,
        snapshot_pitch: f32,
        predicted_pitch: f32,
        strafe_time: i32,
    ) {
        if self.override_camera {
            framing[2] = self.range + (strafe_time as f32 / 2000.0).abs() * 100.0;
            *horizontal = self.horizontal + strafe_time as f32 / 2000.0 * -80.0;
            if self.pitch_dependent {
                framing[1] = if snapshot_pitch == 0.0 {
                    0.0
                } else {
                    predicted_pitch * -0.75
                };
                framing[3] = if snapshot_pitch > 0.0 {
                    (130.0 - predicted_pitch * 10.0).max(-170.0)
                } else if snapshot_pitch < 0.0 {
                    (130.0 - predicted_pitch * 5.0).min(130.0)
                } else {
                    30.0
                };
            } else {
                framing[1] = self.pitch;
                framing[3] = self.vertical;
            }
        } else if self.animal {
            framing[3] = 0.0;
        }
    }
}

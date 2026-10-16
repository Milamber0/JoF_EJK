//! BaseJKA humanoid root and spine-angle policy.
//!
//! Mirrors `BG_G2PlayerAngles` (codemp/game/bg_pmove.c:9091-9463).

use sjk_runtime::PoseState;

use sjk_game_jka::player_angle_math as math;
#[path = "player_angle_spine.rs"]
mod spine;
use math::{angle_mod, normalize, normalized_angle, swing_angles, vector_angles};

/// Result of one stateful BaseJKA player-angle update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyPlayerAngleSample {
    /// Whether ordinary humanoid bone angles are active; vehicle/forced/dead
    /// paths explicitly clear all five overrides in codemp.
    pub bone_angles_active: bool,
    /// Absolute yaw used by the player refEntity/actor root.
    pub legs_yaw_degrees: f32,
    /// Forced-path root roll; ordinary root pitch/roll are zero (9119-9128,9319-9328).
    pub root_roll_degrees: f32,
    /// Lower-lumbar angle command in Quake pitch/yaw/roll order.
    pub lower_lumbar: [f32; 3],
    /// Upper-lumbar angle command in Quake pitch/yaw/roll order.
    pub upper_lumbar: [f32; 3],
    /// Thoracic angle command after default look-angle contribution.
    pub thoracic: [f32; 3],
    /// Cervical angle command after the codemp head clamp.
    pub cervical: [f32; 3],
    /// Cranium angle command after the codemp head clamp.
    pub cranium: [f32; 3],
}

impl LegacyPlayerAngleSample {
    /// Five lower-to-upper humanoid angle commands in Ghoul2 submission order.
    pub fn commands(self) -> [[f32; 3]; 5] {
        [
            self.lower_lumbar,
            self.upper_lumbar,
            self.thoracic,
            self.cervical,
            self.cranium,
        ]
    }
}

/// Model/world inputs supplied by the cgame adapter alongside the generic pose.
#[derive(Clone, Copy, Debug, Default)]
pub struct LegacyPlayerAngleInputs {
    /// Interpolated actor origin for the float add/subtract at bg_pmove.c:9239-9250.
    pub origin: [f32; 3],
    /// Caller frame duration (cg.frametime), independent of an actor's visibility gaps.
    pub frame_millis: Option<i64>,
    /// Motion bolt angles, present only when the cached ci tracks permit correction.
    pub motion_angles: Option<[f32; 3]>,
    /// Direction to the interpolated look target (cg_players.c:4279-4289).
    pub look_target_angles: Option<[f32; 3]>,
}

impl LegacyPlayerAngleInputs {
    /// Resolve cg_players.c:4279-4289 look direction using interpolated world positions.
    pub fn from_world(
        pose: PoseState,
        origin: [f32; 3],
        world: &sjk_runtime::World,
        time: i64,
    ) -> Self {
        let look_target_angles =
            pose.angle
                .look_target
                .and_then(|id| world.entity(id))
                .map(|entity| {
                    let target = entity.sample(time).translation;
                    vector_angles(std::array::from_fn(|axis| target[axis] - origin[axis]))
                });
        Self {
            origin,
            look_target_angles,
            motion_angles: None,
            frame_millis: None,
        }
    }
}

// bg_pmove.c:8964-8982: the raw (NoRecNoRot) bolt uses -Y forward, -X right.
// Row normalization belongs to G2_API.cpp:2059-2063, before extracting columns.
pub(super) fn motion_angles(mut matrix: [[f32; 4]; 3]) -> [f32; 3] {
    for row in &mut matrix {
        let mut basis = [row[0], row[1], row[2]];
        normalize(&mut basis);
        row[..3].copy_from_slice(&basis);
    }
    let mut angles = vector_angles([-matrix[0][1], -matrix[1][1], -matrix[2][1]]);
    angles[2] = -vector_angles([-matrix[0][0], -matrix[1][0], -matrix[2][0]])[0];
    angles
}

/// Fixed per-actor swing and head state; no per-frame allocation or shared static scratch.
#[derive(Clone, Debug, Default)]
pub struct LegacyPlayerAngleController {
    legs_yaw: f32,
    legs_yawing: bool,
    torso_pitch: f32,
    torso_pitching: bool,
    initialized: bool,
    last_time_millis: i64,
    look: spine::LookState,
    last_commands: [[f32; 3]; 5],
    root_roll: f32,
}

impl LegacyPlayerAngleController {
    /// Existing torso swing pitch used by force beams (`cg_players.c:9479`).
    pub fn torso_pitch_degrees(&self) -> f32 {
        self.torso_pitch
    }

    /// Construct empty per-actor state without heap storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe every render frame, including frames where this actor is absent.
    /// cg_players.c:4299 passes cg.frametime, not time since this actor was last drawn.
    pub fn begin_frame(&mut self, time: i64) -> i64 {
        let elapsed = if self.initialized {
            (time - self.last_time_millis).max(0)
        } else {
            0
        };
        self.last_time_millis = time;
        elapsed
    }

    /// Root roll retained for forced-angle paths; ordinary roots have zero roll.
    pub fn root_roll_degrees(&self) -> f32 {
        self.root_roll
    }

    /// Advance the complete ordinary MP angle policy with cgame model/world inputs.
    pub fn evaluate_with_inputs(
        &mut self,
        pose: PoseState,
        time: i64,
        inputs: LegacyPlayerAngleInputs,
    ) -> LegacyPlayerAngleSample {
        let view = pose.view_angles_degrees;
        let elapsed = inputs
            .frame_millis
            .unwrap_or_else(|| {
                if self.initialized {
                    (time - self.last_time_millis).max(0)
                } else {
                    0
                }
            })
            .max(0);
        if !self.initialized {
            // CG_ResetPlayerEntity, cg_players.c:11225-11235: use raw lerp yaw/pitch.
            self.legs_yaw = view[1];
            self.torso_pitch = view[0];
            self.initialized = true;
        }
        self.last_time_millis = time;
        self.root_roll = if pose.lock_root_angles { view[2] } else { 0.0 };
        let root_yaw;
        let active;
        if pose.lock_root_angles {
            // bg_pmove.c:9119-9138: return without modifying *lYawAngle or swing flags.
            root_yaw = view[1];
            active = pose.angle.preserve_overrides;
            if !active {
                self.last_commands = [[0.0; 3]; 5];
            }
        } else {
            active = true;
            if pose.angle.center_swing {
                // bg_pmove.c:9153-9163. tYawAngle follows head instantly (9176-9179).
                self.legs_yawing = true;
                self.torso_pitching = true;
            }
            let pitch_dest = if view[0] > 180.0 {
                view[0] - 360.0
            } else {
                view[0]
            } * 0.75;
            swing_angles(
                pitch_dest,
                15.0,
                30.0,
                0.1,
                &mut self.torso_pitch,
                &mut self.torso_pitching,
                elapsed as f32,
            );
            let destination = legs_destination(pose, inputs.origin);
            swing_angles(
                destination,
                0.0,
                90.0,
                0.65,
                &mut self.legs_yaw,
                &mut self.legs_yawing,
                elapsed as f32,
            );
            // bg_pmove.c:9330-9335: IK root exception does not change swing history.
            root_yaw = if pose.angle.hold_view_yaw {
                view[1]
            } else {
                self.legs_yaw
            };
            let pitch = view[0];
            let spine_view = [pitch * 0.5, normalized_angle(view[1] - root_yaw), 0.0];
            let motion = inputs
                .motion_angles
                .filter(|_| pose.angle.correct_animation_motion);
            let look = self.look.update(view, inputs.look_target_angles, time);
            self.last_commands = spine::commands(spine_view, motion, look);
        }
        let [lower_lumbar, upper_lumbar, thoracic, cervical, cranium] = self.last_commands;
        LegacyPlayerAngleSample {
            bone_angles_active: active,
            legs_yaw_degrees: root_yaw,
            root_roll_degrees: self.root_roll,
            lower_lumbar,
            upper_lumbar,
            thoracic,
            cervical,
            cranium,
        }
    }
}

fn legs_destination(pose: PoseState, origin: [f32; 3]) -> f32 {
    let mut destination = angle_mod(pose.view_angles_degrees[1]);
    let mut velocity = pose.velocity;
    // bg_pmove.c:9183-9195,9243-9248: only grounded unsuppressed normalized XY faces.
    if !pose.grounded || pose.suppress_velocity_facing {
        return destination;
    }
    normalize(&mut velocity);
    let direction = [
        origin[0] - (origin[0] + velocity[0]),
        origin[1] - (origin[1] + velocity[1]),
        0.0,
    ];
    if direction == [0.0; 3] {
        return destination;
    }
    let velocity_yaw = vector_angles(direction)[1];
    // bg_pmove.c:9256-9276: keep the 360-vs-zero distinction and positive tie direction.
    let (negative, positive) = if velocity_yaw <= destination {
        (
            destination - velocity_yaw,
            (360.0 - destination) + velocity_yaw,
        )
    } else {
        (
            destination + (360.0 - velocity_yaw),
            velocity_yaw - destination,
        )
    };
    let subtract = negative >= positive;
    let mut difference = if subtract { positive } else { negative };
    if difference > 90.0 {
        difference = 180.0 - difference;
    }
    difference = difference.min(60.0);
    if matches!(pose.movement_direction, 3 | 5) {
        difference = -difference;
    }
    if subtract {
        destination -= difference;
    } else {
        destination += difference;
    }
    destination
}

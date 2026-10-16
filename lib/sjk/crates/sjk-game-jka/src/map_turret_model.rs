//! A `misc_turretG2`'s Ghoul2 instance on the server (`turretG2_set_models`): the cannon
//! (`turret_canon.glm`) or the turbolaser on its own skeleton, the pitch bone the game
//! turns (`G2Tur_SetBoneAngles`: `BONE_ANGLES_POSTMULT`, +Y up, -Z right, -X forward,
//! blended over 100 ms), and the muzzle bolts read where the turret points
//! (`G2API_GetBoltMatrix` with its yaw, its origin and its scale). The turret's aim is the
//! game's ([`crate::map_turret_g2`]); this is only where its shots leave the model.
//!
//! [`TurretModel`] is a model's files, shared by every turret wearing it; [`TurretPose`]
//! is one turret's pitch.

use crate::ghoul2_bolt::{model_bolt, world_bolt};
use sjk_model::{
    BoneAngleCommand, BoneAngleMode, BoneAxis, BoneOverridePose, Gla, Glm, ModelError,
};

/// `G2Tur_SetBoneAngles`' blend time; the dedicated server's evaluator honours the angles
/// only until it has passed (as it does `NPC_SetBoneAngles`', [`crate::vehicle_skeleton`]).
pub const TURRET_BONE_BLEND: i32 = 100;

/// A turret model's files: its mesh and the skeleton it names.
pub struct TurretModel {
    glm: Glm,
    gla: Gla,
}

impl TurretModel {
    /// Build from the bytes of the `.glm` and of the `.gla` it names
    /// ([`crate::vehicle_skeleton::VehicleModels::skeleton_name`]).
    pub fn new(glm: &[u8], gla: &[u8]) -> Result<Self, ModelError> {
        Ok(Self {
            glm: Glm::parse(glm)?,
            gla: Gla::parse(gla)?,
        })
    }

    /// Whether the model has a bolt of this name (a surface `*name`, else a bone).
    pub fn has_bolt(&self, name: &str) -> bool {
        if name.starts_with('*') {
            return self
                .glm
                .hierarchy
                .iter()
                .any(|surface| surface.name.eq_ignore_ascii_case(name));
        }
        self.gla
            .bones
            .iter()
            .any(|bone| bone.name.eq_ignore_ascii_case(name))
    }
}

/// One turret's pose: the bones the game turned, each with when.
pub struct TurretPose {
    pose: BoneOverridePose,
    angles: Vec<(usize, BoneAngleCommand, i32)>,
}

impl TurretPose {
    /// A turret's pose as its spawn leaves it: nothing turned.
    pub fn new(model: &TurretModel) -> Self {
        Self {
            pose: BoneOverridePose::new(model.gla.bones.len()),
            angles: Vec::new(),
        }
    }

    /// `G2API_SetBoneAngles(ghoul2, 0, bone, angles, BONE_ANGLES_POSTMULT, POSITIVE_Y,
    /// NEGATIVE_Z, NEGATIVE_X, NULL, 100, time)`: nothing for a bone the model lacks.
    pub fn set_bone_angles(
        &mut self,
        model: &TurretModel,
        bone: &str,
        angles: [f32; 3],
        time: i32,
    ) {
        let Some(index) = model
            .gla
            .bones
            .iter()
            .position(|known| known.name.eq_ignore_ascii_case(bone))
        else {
            return;
        };
        let command = BoneAngleCommand {
            angles_degrees: angles,
            mode: BoneAngleMode::PostMultiply,
            up: BoneAxis::PositiveY,
            left: BoneAxis::NegativeZ,
            forward: BoneAxis::NegativeX,
        };
        self.angles.retain(|(known, ..)| *known != index);
        self.angles.push((index, command, time));
    }

    /// `G2API_GetBoltMatrix` of bolt `name` posed at the Ghoul2 `clock`, the model at
    /// `origin` turned by `angles` and scaled by `scale`: the world matrix (the multiplayer
    /// column swap applied, as `BG_GiveMeVectorFromMatrix` reads it). `None` for a bolt the
    /// model lacks — the reference then answers the model's origin.
    pub fn bolt_matrix(
        &mut self,
        model: &TurretModel,
        name: &str,
        angles: [f32; 3],
        origin: [f32; 3],
        scale: [f32; 3],
        clock: i32,
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        for &(bone, command, set_at) in &self.angles {
            if set_at + TURRET_BONE_BLEND < clock {
                self.pose.clear_bone_angles(bone);
            } else {
                self.pose.set_bone_angles(&model.gla, bone, command)?;
            }
        }
        self.pose.evaluate(&model.gla, i64::from(clock))?;
        let Some(raw) = model_bolt(&model.glm, &model.gla, name, self.pose.matrices())? else {
            return Ok(None);
        };
        Ok(Some(world_bolt(raw, angles, origin, scale)))
    }
}

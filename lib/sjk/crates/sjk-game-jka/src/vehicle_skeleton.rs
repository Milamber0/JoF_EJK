//! A vehicle's Ghoul2 instance on the server, as its bolts are read.
//!
//! `NPC_Spawn_Do` gives a vehicle its own model (`models/players/<model>/model.glm`) on its
//! own skeleton, and every frame, at the vehicle's turn, `WP_SaberPositionUpdate` poses it:
//! `G_UpdateClientAnims` sets the root's animation from the legs' — the root alone for a
//! vehicle (`g_client.c:2854-2905`), none at all for an animation its `animation.cfg` does
//! not list. The vehicle code reads its tags from that pose:
//!
//! - `*driver`, which a pilot sits on (`AttachRidersGeneric`, `G2API_GetBoltMatrix`);
//! - the muzzles, which its guns fire from (`WP_CalcVehMuzzle`,
//!   `G2API_GetBoltMatrix_NoRecNoRot`: the same bolt without the "90 degree offset" the
//!   plain read swaps in — [`VehicleSkeleton::bolt`]'s `swapped`).
//!
//! A turret's bones are turned by the game (`NPC_SetBoneAngles`, [`VehicleSkeleton::set_bone_angles`]),
//! and the muzzles on them follow. A walker's head (`BG_G2ATSTAngles`) is the same call.
//!
//! [`VehicleModels`] is a model's files, shared by every vehicle of that model;
//! [`VehicleSkeleton`] is one vehicle's pose.

use crate::server_skeleton::{
    GHOUL2_ROOT, Sequence, command, multiply, normalize_rows, world_matrix,
};
use crate::vehicle_weapons::MuzzleBolt;
use sjk_model::{
    AnimationConfig, BoneAngleCommand, BoneAngleMode, BoneAxis, BoneOverridePose, Gla, Glm,
    ModelError,
};

/// The bone bolts' handles begin here; below it a handle is a surface's.
const BONE_BOLT: i32 = 0x4000_0000;

/// A vehicle model's files: its mesh, its skeleton and its animations.
pub struct VehicleModels {
    glm: Glm,
    gla: Gla,
    sequences: Vec<Sequence>,
    root: usize,
}

impl VehicleModels {
    /// Build from the files' bytes: `model.glm`, the skeleton it names, and the
    /// `animation.cfg` beside that skeleton.
    pub fn new(glm: &[u8], gla: &[u8], animation_cfg: &[u8]) -> Result<Self, ModelError> {
        let gla = Gla::parse(gla)?;
        let config = AnimationConfig::parse(animation_cfg)?;
        let root = gla
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case("model_root"))
            .unwrap_or(0);
        Ok(Self {
            glm: Glm::parse(glm)?,
            sequences: Sequence::table(&config),
            gla,
            root,
        })
    }

    /// The skeleton `model.glm` names (`G2API_GetGLAName`, the header's `animName`),
    /// without its extension; `None` for a file too short to hold one.
    pub fn skeleton_name(glm: &[u8]) -> Option<String> {
        let name = glm.get(72..136)?;
        let name = &name[..name.iter().position(|&byte| byte == 0).unwrap_or(64)];
        Some(String::from_utf8_lossy(name).into_owned())
    }

    /// `G2API_AddBolt(ghoul2, 0, name)` (`G2_Add_Bolt`): a handle to the surface of that
    /// name, else the bone; `-1` where the model has neither.
    pub fn bolt(&self, name: &str) -> i32 {
        if let Some(surface) = self
            .glm
            .hierarchy
            .iter()
            .position(|surface| surface.name.eq_ignore_ascii_case(name))
        {
            return surface as i32;
        }
        match self
            .gla
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(name))
        {
            Some(bone) => BONE_BOLT + bone as i32,
            None => -1,
        }
    }

    fn sequence(&self, animation: u16) -> Sequence {
        self.sequences
            .get(usize::from(animation))
            .copied()
            .unwrap_or_default()
    }
}

/// One vehicle's skeleton.
pub struct VehicleSkeleton {
    pose: BoneOverridePose,
    /// `legsAnimExecute`, `legsLastFlip`: what was last installed on the root (a zeroed
    /// client's animation 0 to begin with).
    installed: (u16, bool),
    /// When the matrices were built, and whether an animation changed since.
    built_at: Option<i32>,
    changed: bool,
    /// The bones the game turned, each with its command, when it was set and its blend
    /// time (`NPC_SetBoneAngles`' 100 ms, which the dedicated server's evaluator honours
    /// only until it has passed; `BG_G2ATSTAngles`' none, which holds).
    angles: Vec<(usize, BoneAngleCommand, i32, i32)>,
}

/// `NPC_SetBoneAngles`' blend time.
pub const NPC_BONE_BLEND: i32 = 100;

impl VehicleSkeleton {
    /// A vehicle's skeleton as its spawn leaves it: nothing installed.
    pub fn new(models: &VehicleModels) -> Self {
        Self {
            pose: BoneOverridePose::new(models.gla.bones.len()),
            installed: (0, false),
            built_at: None,
            changed: true,
            angles: Vec::new(),
        }
    }

    /// `G_UpdateClientAnims` for a vehicle at `level_time`: its legs' animation on the root,
    /// where it changed (or started again) and the model lists it.
    pub fn update_animation(
        &mut self,
        models: &VehicleModels,
        legs: u16,
        flip: bool,
        speed_scale: f32,
        level_time: i32,
    ) -> Result<(), ModelError> {
        let sequence = models.sequence(legs);
        if sequence.is_empty() || self.installed == (legs, flip) {
            return Ok(());
        }
        self.pose.set_bone_animation(
            &models.gla,
            models.root,
            command(sequence, legs, speed_scale, i64::from(level_time)),
        )?;
        self.installed = (legs, flip);
        self.changed = true;
        Ok(())
    }

    /// `G2API_GetBoltMatrix` of bolt `tag` ([`VehicleModels::bolt`]) with the model at
    /// `origin` facing `angles`, posed at the Ghoul2 clock: the bolt's origin and its
    /// `NEGATIVE_Y`. `swapped` is the plain read's "90 degree offset" (columns 0 and 1
    /// turned), which `_NoRecNoRot` leaves out. `None` for a tag the model lacks — the
    /// reference then answers the model's origin.
    pub fn bolt(
        &mut self,
        models: &VehicleModels,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
        clock: i32,
        swapped: bool,
    ) -> Result<Option<MuzzleBolt>, ModelError> {
        if tag < 0 {
            return Ok(None);
        }
        if self.changed || self.built_at != Some(clock) {
            for &(bone, command, set_at, blend) in &self.angles {
                if blend != 0 && set_at + blend < clock {
                    self.pose.clear_bone_angles(bone);
                } else {
                    self.pose.set_bone_angles(&models.gla, bone, command)?;
                }
            }
            self.pose.evaluate(&models.gla, i64::from(clock))?;
            self.built_at = Some(clock);
            self.changed = false;
        }
        let matrices = self.pose.matrices();
        let raw = if tag >= BONE_BOLT {
            let bone = (tag - BONE_BOLT) as usize;
            matrices
                .get(bone)
                .map(|matrix| multiply(*matrix, models.gla.bones[bone].base_pose))
        } else {
            let Some(surface) = models.glm.hierarchy.get(tag as usize) else {
                return Ok(None);
            };
            models.glm.surface_bolt_matrix(&surface.name, 0, matrices)?
        };
        let Some(raw) = raw else { return Ok(None) };
        let world = multiply(
            world_matrix(angles, origin),
            normalize_rows(multiply(GHOUL2_ROOT, raw)),
        );
        // `NEGATIVE_Y`: the second column negated — the swap puts the first there.
        let column = if swapped { 0 } else { 1 };
        Ok(Some(MuzzleBolt {
            origin: [world[0][3], world[1][3], world[2][3]],
            forward: std::array::from_fn(|row| -world[row][column]),
        }))
    }

    /// `G2API_SetBoneAngles(ghoul2, 0, bone, angles, BONE_ANGLES_POSTMULT, POSITIVE_X,
    /// NEGATIVE_Y, NEGATIVE_Z, NULL, blend, time)` by name — `NPC_SetBoneAngles`' with its
    /// [`NPC_BONE_BLEND`], `BG_G2ATSTAngles`' on `thoracic` with none: nothing for a bone the
    /// model lacks.
    pub fn set_bone_angles(
        &mut self,
        models: &VehicleModels,
        bone: &str,
        angles: [f32; 3],
        blend: i32,
        time: i32,
    ) {
        let Some(index) = models
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
            up: BoneAxis::PositiveX,
            left: BoneAxis::NegativeY,
            forward: BoneAxis::NegativeZ,
        };
        self.angles.retain(|(known, ..)| *known != index);
        self.angles.push((index, command, time, blend));
        self.changed = true;
    }

    /// The bolt's place in the model's own frame, facing nowhere: what a rider on `*driver`
    /// is carried by as the vehicle turns (`AttachRidersGeneric` reads it along the
    /// vehicle's yaw alone, [`crate::vehicle_riders::driver_origin`]).
    pub fn offset(
        &mut self,
        models: &VehicleModels,
        tag: i32,
        clock: i32,
    ) -> Result<Option<[f32; 3]>, ModelError> {
        Ok(self
            .bolt(models, tag, [0.0; 3], [0.0; 3], clock, false)?
            .map(|bolt| bolt.origin))
    }
}

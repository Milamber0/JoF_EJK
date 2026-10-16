//! The vehicles' server-side models on this server ([`sjk_game_jka::vehicle_skeleton`]):
//! each vehicle model's files read from the level's filesystem once, each vehicle's
//! skeleton posed at its turn every frame (`WP_SaberPositionUpdate`'s
//! `G_UpdateClientAnims`), and the tags its code reads from it — the muzzles its guns fire
//! from, the `*driver` its pilot sits on, the bolts a spawn looks up (`G2API_AddBolt`).
//!
//! Kept by the vehicle's entity number, forgotten with it; the lists grow with the level's
//! vehicles and have no fixed size.

use super::host::ServerHost;
use sjk_game_jka::npc_spawn::NpcActor;
use sjk_game_jka::vehicle_skeleton::{VehicleModels, VehicleSkeleton};
use sjk_game_jka::vehicle_weapons::MuzzleBolt;
use std::sync::Arc;

/// `ps.legsFlip`, `ps.fd.forcePowersActive`.
const PS_LEGS_FLIP: usize = 69;
const PS_FORCE_POWERS_ACTIVE: usize = 82;
/// `FP_RAGE`'s bit: a vehicle's animations would play at twice the speed.
const FP_RAGE: u32 = 1 << 8;

/// The vehicle models the level has read (`None` where one would not load), and each
/// vehicle's skeleton.
#[derive(Default)]
pub(super) struct VehicleBodies {
    models: Vec<(Vec<u8>, Option<Arc<VehicleModels>>)>,
    skeletons: Vec<(u16, Arc<VehicleModels>, VehicleSkeleton)>,
}

impl VehicleBodies {
    /// Forgets vehicle `number`'s skeleton: the entity was freed.
    pub(super) fn forget(&mut self, number: u16) {
        self.skeletons.retain(|(known, ..)| *known != number);
    }
}

impl ServerHost<'_> {
    /// `models/players/<model>/model.glm` with its skeleton and animations, read once.
    fn vehicle_models(&mut self, model: &[u8]) -> Option<Arc<VehicleModels>> {
        let bodies = &mut self.bodies.vehicles;
        if let Some((_, known)) = bodies
            .models
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(model))
        {
            return known.clone();
        }
        let loaded = self.map.and_then(|map| {
            let read = |path: &str| map.files.read(path).ok().flatten().map(|asset| asset.bytes);
            let glm = read(&format!(
                "models/players/{}/model.glm",
                String::from_utf8_lossy(model)
            ))?;
            let skeleton = VehicleModels::skeleton_name(&glm)?;
            let directory = skeleton
                .rsplit_once('/')
                .map_or(skeleton.as_str(), |(directory, _)| directory);
            let (gla, config) = (
                read(&format!("{skeleton}.gla"))?,
                read(&format!("{directory}/animation.cfg"))?,
            );
            VehicleModels::new(&glm, &gla, &config)
                .map_err(|error| {
                    eprintln!("vehicle model {}: {error}", String::from_utf8_lossy(model))
                })
                .ok()
                .map(Arc::new)
        });
        bodies.models.push((model.to_vec(), loaded.clone()));
        loaded
    }

    /// `G2API_AddBolt` on a vehicle model: the tag's handle, -1 where it has none (or the
    /// model will not load).
    pub(super) fn vehicle_model_bolt(&mut self, model: &[u8], name: &str) -> i32 {
        self.vehicle_models(model)
            .map_or(-1, |models| models.bolt(name))
    }

    /// `WP_SaberPositionUpdate` for a vehicle: its skeleton built the first time, then its
    /// legs' animation set on the root (`G_UpdateClientAnims`).
    pub(super) fn pose_vehicle(&mut self, npc: &NpcActor, level_time: i32) {
        if !self
            .bodies
            .vehicles
            .skeletons
            .iter()
            .any(|(known, ..)| *known == npc.number)
        {
            // The definition's model (`swoop`), not the NPC's (`$swoop_mp`).
            let Some(model) = npc
                .vehicle
                .as_deref()
                .and_then(|vehicle| vehicle.info.model.clone())
            else {
                return;
            };
            let Some(models) = self.vehicle_models(&model) else {
                return;
            };
            let skeleton = VehicleSkeleton::new(&models);
            self.bodies
                .vehicles
                .skeletons
                .push((npc.number, models, skeleton));
        }
        let Some((_, models, skeleton)) = self
            .bodies
            .vehicles
            .skeletons
            .iter_mut()
            .find(|(known, ..)| *known == npc.number)
        else {
            return;
        };
        let flip = npc.player.raw_field(PS_LEGS_FLIP).unwrap_or(0) != 0;
        let scale = if npc.player.raw_field(PS_FORCE_POWERS_ACTIVE).unwrap_or(0) & FP_RAGE != 0 {
            2.0
        } else {
            1.0
        };
        if let Err(error) =
            skeleton.update_animation(models, npc.player.leg_animation(), flip, scale, level_time)
        {
            eprintln!("vehicle {}'s skeleton: {error}", npc.number);
        }
        // `G_G2PlayerAngles` for a walker (`w_saber.c:1020-1032`): its head pitched with its
        // view (`BG_G2ATSTAngles`), the guns on it with it.
        if npc
            .vehicle
            .as_deref()
            .is_some_and(|vehicle| vehicle.kind() == sjk_game_jka::vehicle_fields::kind::WALKER)
        {
            skeleton.set_bone_angles(
                models,
                "thoracic",
                [npc.player.view_angles()[0], 0.0, 0.0],
                0,
                level_time,
            );
        }
    }

    /// `G2API_GetBoltMatrix_NoRecNoRot` on vehicle `number`: its tag at the Ghoul2 clock,
    /// or the model's origin where it has no skeleton yet.
    pub(super) fn vehicle_bolt_at(
        &mut self,
        number: u16,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> MuzzleBolt {
        let clock = self.ghoul2_time;
        let found = self
            .bodies
            .vehicles
            .skeletons
            .iter_mut()
            .find(|(known, ..)| *known == number);
        let bolt = found.and_then(|(_, models, skeleton)| {
            skeleton
                .bolt(models, tag, angles, origin, clock, false)
                .map_err(|error| eprintln!("vehicle {number}'s bolt: {error}"))
                .ok()
                .flatten()
        });
        bolt.unwrap_or_else(|| MuzzleBolt::unposed(origin))
    }

    /// `G2API_GetBoltMatrix` on vehicle `number` (the plain read, with its swap): its tag at
    /// the Ghoul2 clock, or the model's origin where it has no skeleton yet.
    pub(super) fn vehicle_tag_at(
        &mut self,
        number: u16,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> MuzzleBolt {
        let clock = self.ghoul2_time;
        let found = self
            .bodies
            .vehicles
            .skeletons
            .iter_mut()
            .find(|(known, ..)| *known == number);
        let bolt = found.and_then(|(_, models, skeleton)| {
            skeleton
                .bolt(models, tag, angles, origin, clock, true)
                .map_err(|error| eprintln!("vehicle {number}'s tag: {error}"))
                .ok()
                .flatten()
        });
        bolt.unwrap_or_else(|| MuzzleBolt::unposed(origin))
    }

    /// `NPC_SetBoneAngles` on vehicle `number`'s model (a turret's bones, a walker's head):
    /// whether it is a vehicle this server poses.
    pub(super) fn vehicle_set_bone_angles(
        &mut self,
        number: u16,
        bone: &str,
        angles: [f32; 3],
        level_time: i32,
    ) -> bool {
        let Some((_, models, skeleton)) = self
            .bodies
            .vehicles
            .skeletons
            .iter_mut()
            .find(|(known, ..)| *known == number)
        else {
            return false;
        };
        skeleton.set_bone_angles(
            models,
            bone,
            angles,
            sjk_game_jka::vehicle_skeleton::NPC_BONE_BLEND,
            level_time,
        );
        true
    }

    /// Where `*driver` is on vehicle `number`'s model, in the model's own frame: zero where
    /// it has none (the pilot then sits at the vehicle's origin).
    pub(super) fn vehicle_driver_offset(&mut self, number: u16) -> [f32; 3] {
        let clock = self.ghoul2_time;
        let Some((_, models, skeleton)) = self
            .bodies
            .vehicles
            .skeletons
            .iter_mut()
            .find(|(known, ..)| *known == number)
        else {
            return [0.0; 3];
        };
        let tag = models.bolt("*driver");
        skeleton
            .offset(models, tag, clock)
            .ok()
            .flatten()
            .unwrap_or([0.0; 3])
    }
}

//! Codemp projectile selection and transform presentation.
//!
//! `collect` mirrors `CG_Missile` in `codemp/cgame/cg_ents.c:2430-2665`:
//! ET_MISSILE is sampled with the shared `BG_EvaluateTrajectory` adapter,
//! model axis X follows `pos.trDelta`, and the model rolls by the same
//! presented-time factors. Visual entries come from `CG_RegisterWeapon` in
//! `codemp/cgame/cg_weaponinit.c:206-596`; effect-only shots retain retail
//! effect shaders instead of acquiring placeholder art.

use glam::{Quat, Vec3};
use sjk_client::legacy_evaluate_trajectory;
use sjk_protocol::{EntityState, Snapshot};

pub(crate) const ET_MISSILE: u8 = 3; // codemp/game/bg_public.h:1244-1249
const EF_ALT_FIRING: u32 = 1 << 10; // codemp/game/bg_public.h:649
const EF_MISSILE_STICK: u32 = 1 << 22; // codemp/game/bg_public.h:666
const TR_STATIONARY: u8 = 0; // codemp/qcommon/q_shared.h:1542-1545
const TR_INTERPOLATE: u8 = 1; // codemp/qcommon/q_shared.h:1542-1545
const EF_NODRAW: u32 = 1 << 8; // codemp/game/bg_public.h:645
/// `WP_SABER` (`codemp/game/bg_weapons.h`).
const WP_SABER: u8 = 3;
/// `CG_Missile` falls back to `DEFAULT_SABER_MODEL` (`bg_public.h`).
const DEFAULT_SABER_MODEL: &str = "models/weapons2/saber_1/saber_1.glm";

/// Original visual selected by one weapon firing mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Visual {
    /// A retail MD3/GLM model whose local +X axis follows travel.
    Model(&'static str),
    /// Authored model at a registered vehicle weapon index.
    VehicleModel(usize),
    /// A representative component of the retail repeating projectile EFX.
    Effect(EffectVisual),
    /// A saber knocked away or dropped (`saberKnockDown` makes it an `ET_MISSILE` with
    /// `WP_SABER`): `CG_Missile` draws its hilt, the `CS_MODELS` entry `modelindex` names
    /// (`DEFAULT_SABER_MODEL` without one), with no blade.
    Saber(i16),
    /// Intentionally not drawn by codemp (the flying tripmine case).
    None,
}

/// Allocation-free rendering data extracted from a retail EFX component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EffectVisual {
    pub(crate) shader: &'static str,
    pub(crate) size: f32,
    pub(crate) length: f32,
    pub(crate) color: [f32; 3],
}

/// One ET_MISSILE at the current cgame presentation time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Presented {
    pub(crate) origin: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) visual: Visual,
}

/// Append all decoded ET_MISSILEs without allocating when `output` has the
/// documented 1024-entity protocol capacity.
pub(crate) fn collect(snapshot: &Snapshot, at_time: i32, output: &mut Vec<Presented>) {
    output.clear();
    for state in snapshot
        .entities
        .iter()
        .filter(|state| state.entity_type() == ET_MISSILE)
    {
        output.push(present(state, at_time));
    }
}

fn present(state: &EntityState, at_time: i32) -> Presented {
    let alternate = state.e_flags() & EF_ALT_FIRING != 0;
    let origin = legacy_evaluate_trajectory(
        state.trajectory_base(),
        state.trajectory_delta(),
        state.trajectory_type(),
        state.trajectory_time(),
        state.trajectory_duration(),
        at_time,
    );
    let direction = Vec3::from_array(state.trajectory_delta()).normalize_or(Vec3::Z);
    let rotation = if state.angular_trajectory_type() == TR_INTERPOLATE {
        angles_to_rotation(state.angular_trajectory_base())
    } else {
        let roll_degrees = if state.trajectory_type() != TR_STATIONARY {
            at_time as f32
                * if state.e_flags() & EF_MISSILE_STICK != 0 {
                    0.5
                } else {
                    0.25
                }
        } else if state.e_flags() & EF_MISSILE_STICK != 0 {
            state.trajectory_time() as f32 * 0.5
        } else {
            state.raw_field(65).unwrap_or(0) as i32 as f32
        };
        Quat::from_rotation_arc(Vec3::X, direction)
            * Quat::from_rotation_x(roll_degrees.to_radians())
    };
    Presented {
        origin,
        rotation: rotation.to_array(),
        visual: if state.weapon() == WP_SABER {
            if state.e_flags() & EF_NODRAW != 0 {
                Visual::None
            } else {
                Visual::Saber(state.model_index())
            }
        } else if state.other_entity_num2() != 0 {
            if state.e_flags() & (1 << 11) != 0 {
                Visual::VehicleModel(usize::from(state.other_entity_num2()))
            } else {
                Visual::None
            }
        } else {
            visual(state.weapon(), alternate)
        },
    }
}

fn angles_to_rotation(angles: [f32; 3]) -> Quat {
    Quat::from_rotation_z(angles[1].to_radians())
        * Quat::from_rotation_y(angles[0].to_radians())
        * Quat::from_rotation_x(angles[2].to_radians())
}

/// BaseJKA `weaponData` projectile model/trail selection. Numeric weapon IDs
/// are the enum order in `codemp/game/bg_weapons.h:30-48`; assets are assigned
/// in `codemp/cgame/cg_weaponinit.c:206-596`.
pub(crate) fn visual(weapon: u8, alternate: bool) -> Visual {
    let effect = |shader, size, length, color| {
        Visual::Effect(EffectVisual {
            shader,
            size,
            length,
            color,
        })
    };
    match (weapon, alternate) {
        (4 | 16, _) => effect("gfx/effects/bryar_blob", 2.0, 88.0, [1.0, 0.72, 0.12]),
        (5, _) => effect("gfx/effects/blaster_blob", 1.5, 88.0, [1.0, 0.16, 0.12]),
        (7, _) => effect("gfx/effects/greenshot", 1.5, 88.0, [0.2, 1.0, 0.2]),
        (8, false) => effect("gfx/misc/spark", 2.0, 0.0, [1.0, 1.0, 1.0]),
        (8, true) => effect("gfx/effects/whiteflare", 28.0, 0.0, [0.3, 0.3, 1.0]),
        (9, false) => effect("gfx/misc/lightningflash", 17.0, 0.0, [0.45, 0.2, 1.0]),
        // DEMP2 alt has no trail/model in cg_weaponinit.c:413-419.
        (9, true) => Visual::None,
        (10, false) => Visual::Model("models/weapons2/golan_arms/projectileMain.md3"),
        (10, true) => Visual::Model("models/weapons2/golan_arms/projectile.md3"),
        (11, _) => Visual::Model("models/weapons2/merr_sonn/projectile.md3"),
        (12, _) => Visual::Model("models/weapons2/thermal/thermal_proj.md3"),
        // cg_weaponinit.c:541/552 deliberately registers no flying mine model.
        (13, _) => Visual::None,
        (14, _) => Visual::Model("models/weapons2/detpack/det_pack.md3"),
        _ => Visual::None,
    }
}

pub(crate) fn model_paths() -> impl Iterator<Item = &'static str> {
    const MODELS: [&str; 5] = [
        "models/weapons2/golan_arms/projectileMain.md3",
        "models/weapons2/golan_arms/projectile.md3",
        "models/weapons2/merr_sonn/projectile.md3",
        "models/weapons2/thermal/thermal_proj.md3",
        "models/weapons2/detpack/det_pack.md3",
    ];
    MODELS.into_iter()
}

impl Visual {}

/// Draw only authored rigid projectiles; their EFX are submitted separately.
pub(crate) fn append_models(
    projectiles: &[Presented],
    effects: &sjk_client::LegacyMissileEffects,
    game_state: Option<&sjk_protocol::GameState>,
    meshes: &[crate::object_meshes::StaticModelMesh],
    groups: &mut [Vec<crate::ActorInstance>],
) {
    for missile in projectiles {
        let saber;
        let model = match missile.visual {
            Visual::Model(model) => Some(model),
            Visual::Saber(index) => {
                saber = game_state
                    .and_then(|game| sjk_client::legacy_model_appearance(game, index))
                    .map(|appearance| appearance.model);
                Some(saber.as_deref().unwrap_or(DEFAULT_SABER_MODEL))
            }
            Visual::VehicleModel(index) => effects
                .vehicle_weapon(index)
                .and_then(|weapon| weapon.model.as_deref()),
            _ => None,
        };
        if let Some(model) = model
            && let Some(mesh) = meshes
                .iter()
                .position(|mesh| mesh.appearance.model.eq_ignore_ascii_case(model))
        {
            groups[mesh].push(crate::ActorInstance::new(
                missile.origin,
                missile.rotation,
                [1.0; 3],
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_meshes::StaticModelMesh;
    use sjk_protocol::{GameState, LEGACY_ENTITY_FIELDS};
    use sjk_runtime::Appearance;

    // entityState_t netfields (`codemp/qcommon/msg.cpp`).
    const TYPE: usize = 8;
    const WEAPON: usize = 14;
    const EFLAGS: usize = 19;
    const MODEL_INDEX: usize = 46;
    const CS_MODELS: usize = 298;
    const HILT: &str = "models/weapons2/saber_2/saber_2.glm";

    /// A saber `saberKnockDown` left on the ground: a missile with the saber's model.
    fn dropped_saber(model_index: i16) -> EntityState {
        let mut state = EntityState::zero(100, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(TYPE, u32::from(ET_MISSILE));
        state.set_raw_field(WEAPON, u32::from(WP_SABER));
        state.set_raw_field(MODEL_INDEX, model_index as u16 as u32);
        state
    }

    fn mesh(model: &str) -> StaticModelMesh {
        StaticModelMesh {
            appearance: Appearance {
                model: model.to_owned(),
                variant: String::new(),
            },
            draws: Vec::new(),
            center: [0.0; 3],
            flash_bolt: None,
        }
    }

    /// The instances each of `meshes` gets for `states`.
    fn drawn(states: &[EntityState], meshes: &[StaticModelMesh]) -> Vec<usize> {
        let mut game = GameState::empty_local(0);
        game.replace_config_string(CS_MODELS + 7, HILT.as_bytes().to_vec())
            .unwrap();
        let presented: Vec<_> = states.iter().map(|state| present(state, 0)).collect();
        let mut groups = vec![Vec::new(); meshes.len()];
        append_models(
            &presented,
            &sjk_client::LegacyMissileEffects::empty(),
            Some(&game),
            meshes,
            &mut groups,
        );
        groups.iter().map(Vec::len).collect()
    }

    #[test]
    fn a_dropped_saber_draws_its_hilt() {
        let meshes = [mesh(HILT), mesh(DEFAULT_SABER_MODEL)];
        assert_eq!(present(&dropped_saber(7), 0).visual, Visual::Saber(7));
        assert_eq!(drawn(&[dropped_saber(7)], &meshes), [1, 0]);
        // Without a model of its own, `CG_Missile`'s default saber.
        assert_eq!(drawn(&[dropped_saber(0)], &meshes), [0, 1]);
    }

    #[test]
    fn a_hidden_dropped_saber_draws_nothing() {
        let mut hidden = dropped_saber(7);
        hidden.set_raw_field(EFLAGS, EF_NODRAW);
        assert_eq!(present(&hidden, 0).visual, Visual::None);
        assert_eq!(drawn(&[hidden], &[mesh(HILT)]), [0]);
    }
}

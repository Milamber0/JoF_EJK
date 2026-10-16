//! `CG_DrawPlayerSphere` (`codemp/cgame/cg_players.c`, JoF EJK 8842-8886): the
//! spawn-protection, Ysalamiri, Boon and Enlightenment spheres around a player.
//!
//! Each sphere is `models/weaphits/testboom.md3` with a custom shader, 9 units above
//! the player's origin, scaled uniformly and turned to face the view: its forward axis
//! points back at the view origin, rolled half a turn. The second, refraction pass
//! (`effects/refract_2` with `RF_DISTORTION`) needs `cg_renderToTextureFX` and is not
//! drawn. The shaders set their own colour, so the zeroed `shaderRGBA` is left white.

use super::*;

/// Submit the spheres of one living player.
pub(super) fn submit(sinks: &mut Sinks<'_>, origin: Vec3, e_flags: u32, powerups: u32) {
    let Some(mesh) = sinks.shield_mesh else {
        return;
    };
    let center = origin + Vec3::Z * 9.0;
    let Some(rotation) = facing(center, sinks.view_origin) else {
        return;
    };
    for sphere in sjk_client::legacy_player_spheres(e_flags, powerups, sinks.gametype)
        .into_iter()
        .flatten()
    {
        if sinks.overrides.len() == sinks.overrides.capacity() {
            break;
        }
        let Some(material) = sinks.material_overrides.force(sphere.shader) else {
            continue;
        };
        let mut instance =
            ActorInstance::new(center.to_array(), rotation.to_array(), [sphere.scale; 3]);
        instance.view_flags = sinks.entity_view_flags;
        sinks.overrides.push(entity_materials::OverrideInstance {
            mesh: entity_materials::OverrideMesh::Object(mesh),
            material: Some(material),
            instance,
            no_depth: false,
            forced_alpha: false,
        });
    }
}

/// `vectoangles(center - vieworg)` with pitch and roll each turned by 180 degrees,
/// through `AnglesToAxis`: forward back at the view, left level, up completing the
/// frame. `None` when the view sits on the centre.
fn facing(center: Vec3, view: Vec3) -> Option<Quat> {
    let toward = center - view;
    if toward.length() <= 0.1 {
        return None;
    }
    let toward = toward.normalize();
    let level = Vec3::new(toward.x, toward.y, 0.0);
    // `vectoangles` gives yaw 0 to a straight up or down direction.
    let (cos_yaw, sin_yaw) = if level.length_squared() > 1e-12 {
        let level = level.normalize();
        (level.x, level.y)
    } else {
        (1.0, 0.0)
    };
    let forward = -toward;
    let left = Vec3::new(sin_yaw, -cos_yaw, 0.0);
    let up = forward.cross(left);
    Some(Quat::from_mat3(&glam::Mat3::from_cols(forward, left, up)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_faces_the_view() {
        let center = Vec3::new(100.0, 50.0, 30.0);
        let view = Vec3::new(-20.0, 10.0, 80.0);
        let rotation = facing(center, view).unwrap();
        let forward = rotation * Vec3::X;
        assert!(forward.abs_diff_eq((view - center).normalize(), 1e-5));
        // The roll leaves the left axis level.
        assert!((rotation * Vec3::Y).z.abs() < 1e-6);
    }

    #[test]
    fn view_from_straight_above() {
        let rotation = facing(Vec3::ZERO, Vec3::new(0.0, 0.0, 64.0)).unwrap();
        assert!((rotation * Vec3::X).abs_diff_eq(Vec3::Z, 1e-5));
        assert!(facing(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.05)).is_none());
    }
}

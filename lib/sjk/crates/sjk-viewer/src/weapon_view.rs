//! Legacy in-hand weapon selection and world transform policy.
//!
//! World weapons are Ghoul2 children of the player's `*r_hand` bolt in
//! OpenJK codemp. The renderer-facing +90 degree conversion is isolated here:
//! humanoid GLM bind geometry faces -Y, while JKA entity yaw is +X-forward.

use glam::Quat;

pub(crate) fn actor_world_rotation(rotation: [f32; 4]) -> Quat {
    Quat::from_array(rotation) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
}

/// BaseJKA `weaponData[].weaponMdl` world models.
pub(crate) fn held_model(weapon: u8) -> Option<&'static str> {
    match weapon {
        1 => Some("models/weapons2/stun_baton/baton_w.glm"),
        4 => Some("models/weapons2/blaster_pistol/blaster_pistol_w.glm"),
        16 => Some("models/weapons2/briar_pistol/briar_pistol_w.glm"),
        5 => Some("models/weapons2/blaster_r/blaster_w.glm"),
        6 => Some("models/weapons2/disruptor/disruptor_w.glm"),
        7 => Some("models/weapons2/bowcaster/bowcaster_w.glm"),
        8 => Some("models/weapons2/heavy_repeater/heavy_repeater_w.glm"),
        9 => Some("models/weapons2/demp2/demp2_w.glm"),
        10 => Some("models/weapons2/golan_arms/golan_arms_w.glm"),
        11 => Some("models/weapons2/merr_sonn/merr_sonn_w.glm"),
        12 => Some("models/weapons2/thermal/thermal_w.glm"),
        13 => Some("models/weapons2/laser_trap/laser_trap_w.glm"),
        14 => Some("models/weapons2/detpack/det_pack_w.glm"),
        15 => Some("models/weapons2/concussion/c_rifle_w.glm"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::held_model;

    #[test]
    fn multiplayer_pistols_keep_their_distinct_item_models() {
        // codemp/game/bg_misc.c: weapon_blaster_pistol and weapon_bryar_pistol.
        assert_eq!(
            held_model(4),
            Some("models/weapons2/blaster_pistol/blaster_pistol_w.glm")
        );
        assert_eq!(
            held_model(16),
            Some("models/weapons2/briar_pistol/briar_pistol_w.glm")
        );
    }
}

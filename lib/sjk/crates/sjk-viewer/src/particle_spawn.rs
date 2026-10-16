//! Deterministic spawn-time sampling for Raven particle templates.

use glam::{Quat, Vec3};

/// Implements `FX_EVEN_DISTRIBUTION` from
/// `codemp/client/FxScheduler.cpp:890-906`.
pub(crate) fn delay_millis(
    component: &sjk_effect::Component,
    particle_index: usize,
    count: usize,
    random_unit: f32,
) -> f32 {
    if component.spawn_flags.even_distribution && count > 0 {
        let spread = (component.delay.maximum - component.delay.minimum).abs();
        particle_index as f32 * spread / count as f32
    } else {
        component.delay.sample(random_unit)
    }
}

/// Samples RGB endpoints with Raven's shared interpolation fraction when
/// `FX_RGB_COMPONENT_INTERP` is authored (`FxScheduler.cpp:759-774`).
pub(crate) fn rgb_envelopes(
    component: &sjk_effect::Component,
    units: [f32; 2],
    seed: u32,
) -> [crate::effect_envelope::Envelope; 3] {
    if component.spawn_flags.rgb_component_interpolation {
        let parameter = component.rgb_parameter.sample(units[1]);
        std::array::from_fn(|axis| {
            crate::effect_envelope::Envelope::from_values(
                component.rgb_start[axis].sample(units[0]),
                component.rgb_end[axis].sample(units[0]),
                parameter,
                component.rgb_flags,
            )
        })
    } else {
        std::array::from_fn(|axis| {
            crate::effect_envelope::Envelope::from_ranges(
                component.rgb_start[axis],
                component.rgb_end[axis],
                component.rgb_parameter,
                component.rgb_flags,
                seed.wrapping_add(13 + axis as u32 * 3),
            )
        })
    }
}

/// Samples the mounted-data sphere/cylinder placement flags using the branches
/// in `codemp/client/FxScheduler.cpp:1242-1316`.
pub(crate) fn placement(
    component: &sjk_effect::Component,
    base_origin: Vec3,
    effect_rotation: Quat,
    units: [f32; 6],
) -> (Vec3, Quat) {
    let mut rotation = effect_rotation;
    if component.spawn_flags.random_rotation_around_forward {
        rotation *= Quat::from_axis_angle(Vec3::X, units[3] * std::f32::consts::TAU);
    }
    let authored = Vec3::from_array(component.origin.sample([units[0], units[1], units[2]]));
    let offset = if component.spawn_flags.cheap_origin || component.flags.relative {
        authored
    } else {
        rotation * authored
    };
    let mut origin = if component.flags.relative {
        offset
    } else {
        base_origin + offset
    };
    let radial = if component.spawn_flags.origin_on_sphere {
        let azimuth = units[3] * std::f32::consts::TAU;
        let polar = units[4] * std::f32::consts::PI;
        let radius = component.radius.sample(units[5]);
        let height = component.height.sample(units[2]);
        Vec3::new(
            azimuth.sin() * radius * polar.sin(),
            azimuth.cos() * radius * polar.sin(),
            polar.cos() * height,
        )
    } else if component.spawn_flags.origin_on_cylinder {
        let forward = rotation * Vec3::X;
        let side = rotation * Vec3::Y;
        let radius = component.radius.sample(units[5]);
        let height = component.height.sample(units[2]);
        let point = side * radius + forward * ((units[4] - 0.5) * height);
        Quat::from_axis_angle(forward, units[3] * std::f32::consts::TAU) * point
    } else {
        Vec3::ZERO
    };
    origin += radial;
    if component.spawn_flags.axis_from_sphere && radial.length_squared() > f32::EPSILON {
        rotation = crate::combat_effects::rotation_from_direction(radial.normalize().to_array());
    }
    (origin, rotation)
}

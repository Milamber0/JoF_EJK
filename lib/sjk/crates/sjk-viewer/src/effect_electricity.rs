//! Raven `RT_ELECTRICITY` deterministic bolt tessellation.
//!
//! `RB_SurfaceElectricity` and `DoBoltSeg` use 20-unit major segments and
//! `LIGHTNING_RECURSION_LEVEL == 1`, producing three line quads per major
//! segment (`codemp/rd-vanilla/tr_surface.cpp:868-1079`).
//! `RF_FORKED` is retained in primitive state, but this exact rd-vanilla
//! revision never initializes its file-static `f_count`, so its branch at
//! lines 1009-1028 is unreachable and is likewise not emitted here.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn append(
    mesh: &mut crate::effect_geometry::Mesh,
    start: Vec3,
    end: Vec3,
    radius: f32,
    chaos: f32,
    tapered: bool,
    _branched: bool,
    camera: Vec3,
    seed: u32,
    uv: [f32; 4],
    uv_transform: [f32; 4],
    color: [f32; 4],
    depth_hack: bool,
) {
    let mut state = seed as i32;
    append_bolt(
        mesh,
        start,
        end,
        radius,
        chaos,
        tapered,
        false,
        camera,
        &mut state,
        uv,
        uv_transform,
        color,
        depth_hack,
    );
}

#[allow(clippy::too_many_arguments)]
fn append_bolt(
    mesh: &mut crate::effect_geometry::Mesh,
    start: Vec3,
    end: Vec3,
    radius: f32,
    chaos: f32,
    tapered: bool,
    _branched: bool,
    camera: Vec3,
    state: &mut i32,
    uv: [f32; 4],
    uv_transform: [f32; 4],
    color: [f32; 4],
    depth_hack: bool,
) {
    let delta = end - start;
    let distance = delta.length();
    if distance < 20.0 {
        return;
    }
    let forward = delta / distance;
    let (side, up) = normal_vectors(forward);
    let view_right = (start - camera).cross(end - camera).normalize_or(side);
    let mut previous = start;
    let mut offset = Vec3::splat(10.0);
    let steps = (distance / 20.0).floor() as usize;
    let mut old_fraction = 0.0;
    for step in 1..=steps {
        let fraction = if step == steps {
            1.0
        } else {
            step as f32 * 20.0 / distance
        };
        offset += forward * (q_crandom(state) * 3.0);
        offset += side * (q_crandom(state) * 7.0 * chaos);
        offset += up * (q_crandom(state) * 7.0 * chaos);
        let current = (start + offset).lerp(end, fraction);
        let old_radius = tapered_radius(radius, old_fraction, tapered);
        let new_radius = tapered_radius(radius, fraction, tapered);
        shaped_line(
            mesh,
            current,
            previous,
            view_right,
            new_radius,
            old_radius,
            state,
            uv,
            uv_transform,
            color,
            depth_hack,
        );
        previous = current;
        old_fraction = fraction;
    }
}

fn tapered_radius(radius: f32, fraction: f32, tapered: bool) -> f32 {
    if tapered {
        radius * (1.0 - fraction * fraction)
    } else {
        radius
    }
}

#[allow(clippy::too_many_arguments)]
fn shaped_line(
    mesh: &mut crate::effect_geometry::Mesh,
    start: Vec3,
    end: Vec3,
    right: Vec3,
    start_radius: f32,
    end_radius: f32,
    state: &mut i32,
    uv: [f32; 4],
    uv_transform: [f32; 4],
    color: [f32; 4],
    depth_hack: bool,
) {
    let delta = end - start;
    let (side, up) = normal_vectors(delta.normalize_or(Vec3::X));
    let scale = delta.length() * 0.7;
    let first_fraction = 0.66 + q_crandom(state) * 0.1;
    let first = end.lerp(start, first_fraction)
        + side * scale * (0.07 + q_crandom(state) * 0.025)
        + up * scale * (0.07 + q_crandom(state) * 0.025);
    let second_fraction = 0.33 + q_crandom(state) * 0.1;
    let second = end.lerp(start, second_fraction)
        + side * scale * (-0.07 + q_crandom(state) * 0.02)
        + up * scale * (-0.07 + q_crandom(state) * 0.02);
    let radius1 = start_radius * 0.666 + end_radius * 0.333;
    let radius2 = start_radius * 0.333 + end_radius * 0.666;
    line(
        mesh,
        start,
        first,
        right,
        start_radius,
        radius1,
        uv,
        uv_transform,
        color,
        depth_hack,
    );
    line(
        mesh,
        second,
        first,
        right,
        radius2,
        radius1,
        uv,
        uv_transform,
        color,
        depth_hack,
    );
    line(
        mesh,
        second,
        end,
        right,
        radius2,
        end_radius,
        uv,
        uv_transform,
        color,
        depth_hack,
    );
}

#[allow(clippy::too_many_arguments)]
fn line(
    mesh: &mut crate::effect_geometry::Mesh,
    start: Vec3,
    end: Vec3,
    right: Vec3,
    start_radius: f32,
    end_radius: f32,
    uv: [f32; 4],
    uv_transform: [f32; 4],
    color: [f32; 4],
    depth_hack: bool,
) {
    if mesh.expanded_line(
        start,
        end,
        right,
        start_radius,
        end_radius,
        uv,
        uv_transform,
        color,
        depth_hack,
    ) {
        return;
    }
    mesh.line_quad(
        [
            (start + right * start_radius, [0.0, 0.0]),
            (start - right * start_radius, [1.0, 0.0]),
            (end + right * end_radius, [0.0, 1.0]),
            (end - right * end_radius, [1.0, 1.0]),
        ],
        uv,
        uv_transform,
        color,
        depth_hack,
    );
}

fn normal_vectors(forward: Vec3) -> (Vec3, Vec3) {
    let reference = if forward.z.abs() > 0.9 {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let right = forward.cross(reference).normalize_or(Vec3::Y);
    (right, right.cross(forward).normalize_or(Vec3::Z))
}

fn q_random(seed: &mut i32) -> f32 {
    *seed = seed.wrapping_mul(69_069).wrapping_add(1);
    (*seed as u32 & 0xffff) as f32 / 65_536.0
}

fn q_crandom(seed: &mut i32) -> f32 {
    2.0 * (q_random(seed) - 0.5)
}

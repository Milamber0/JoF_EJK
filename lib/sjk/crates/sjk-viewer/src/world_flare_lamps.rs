//! BSP flares have coincident vertices, not area triangles. Recover their source
//! footprint from the same authored radius used by the display shader, at map load.
use super::*;
use glam::Vec3;

pub(super) fn extract(
    materials: &[PendingMaterial],
    vertices: &[([f32; 3], [f32; 3], [f32; 2])],
    indices: &[u32],
) -> Vec<crate::lamp_lights::Lamp> {
    let mut lamps = Vec::new();
    for material in materials.iter().filter(|m| m.flare) {
        let radiance = Vec3::from_array(material.surface.emission);
        let luminance = radiance.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        if !luminance.is_finite() || luminance <= 1e-6 {
            continue;
        }
        let radius = material
            .stages
            .first()
            .map_or(30., |s| s.gpu.wave_functions[2])
            .clamp(5., 128.);
        let power = luminance * 4. * radius * radius * crate::lamp_lights::POWER_SCALE;
        for draw in &material.static_draws {
            // Each original flare is exactly one six-index quad, even after draw merging.
            for quad in
                indices[draw.indices.start as usize..draw.indices.end as usize].chunks_exact(6)
            {
                let corner = vertices[quad[0] as usize];
                let position = Vec3::from_array(corner.0) + Vec3::from_array(corner.1) * 3.;
                lamps.push(crate::lamp_lights::Lamp {
                    position,
                    normal: Vec3::ZERO,
                    color: (radiance / luminance).to_array(),
                    power,
                    radius: (power / 0.004).sqrt().clamp(32., 1024.),
                    axis_u: Vec3::ZERO,
                    axis_v: Vec3::ZERO,
                });
            }
        }
    }
    crate::log::progress(format_args!("BSP flare lamps: {}", lamps.len()));
    lamps
}

//! Short, outward-moving hot fragments that clear the blade glow before fading.
use glam::Vec3;
use std::time::Instant;

pub(super) fn spawn_sparks(
    particles: &mut Vec<crate::Particle>,
    effects: &mut crate::EffectLibrary,
    point: Vec3,
    normal: Vec3,
    now: Instant,
    seed: u32,
) {
    for i in 0..4 {
        let phase =
            (seed.wrapping_mul(1664525).wrapping_add(i * 1013904223) & 65535) as f32 * 0.000095875;
        let tangent = normal.any_orthonormal_vector();
        let direction =
            normal * 0.6 + (tangent * phase.cos() + normal.cross(tangent) * phase.sin()) * 0.8;
        let previous = particles.len();
        crate::impact_spawn::spawn_line(
            particles,
            effects,
            crate::impacts::Line {
                start: (point + normal * 0.5).to_array(),
                end: (point + normal * 0.5 + direction * 8.).to_array(),
                size: [0.7, 0.15],
                alpha: [1., 0.],
                color: [1.8, 1.15, 0.4],
                lifetime_millis: 280 + (seed.wrapping_add(i * 53) % 140),
                shader: "gfx/misc/spark",
            },
            now,
            seed.wrapping_add(i),
        );
        if particles.len() > previous {
            particles[previous].start_length = 8.;
            particles[previous].end_length = 0.5;
            particles[previous].motion = crate::particle_motion::Motion::new(
                point + normal * 0.5,
                direction * (100. + (seed.wrapping_add(i * 31) % 60) as f32),
                Vec3::new(0., 0., -160.),
                0.,
                0.,
                now,
            );
        }
    }
}

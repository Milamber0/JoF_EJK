//! CG_ForcePushBlur/CG_ForceGripEffect plus CG_AddPuff's RGB fade and growth.
use super::*;

/// Emit a stock opposing-velocity puff pair without growing the particle pool.
pub(crate) fn pair(
    particles: &mut Vec<Particle>,
    effects: &mut EffectLibrary,
    origin: Vec3,
    view_left: Vec3,
    grip: bool,
    time: i32,
    now: Instant,
) {
    let red = (200.0 + ((time as f32 * 0.004).sin() * 0.08 + 0.1) * 255.0).min(255.0);
    for side in 0..2 {
        // Puffs are effect particles: they leave the billboard reserve alone.
        if !crate::particle_room::effect_fits(particles.len()) {
            break;
        }
        let color = if !grip {
            [24.0, 32.0, 40.0]
        } else if side == 0 {
            [red, 0.0, 0.0]
        } else {
            [255.0; 3]
        };
        let shader = if grip && side == 1 {
            "gfx/effects/sabers/red_glow"
        } else {
            "gfx/effects/forcePush"
        };
        let linear = sjk_effect::CurveFlags {
            linear: true,
            ..Default::default()
        };
        let envelope =
            |start, end| crate::effect_envelope::Envelope::from_values(start, end, 0.0, linear);
        particles.push(Particle {
            motion: crate::particle_motion::Motion::new(
                origin,
                view_left * if side == 0 { 55.0 } else { -55.0 },
                Vec3::ZERO,
                side as f32 * 180.0,
                0.0,
                now,
            ),

            spawned_at: now,
            delay: Duration::ZERO,
            lifetime: Duration::from_millis(120),
            size: envelope(8.0, 10.0),
            start_length: 1.0,
            end_length: 1.0,
            streak: None,
            trace_streak: false,
            normal: None,
            alpha: envelope(1.0, 1.0),
            use_alpha: false,
            set_shader_time: false,
            rgb: color.map(|value| envelope(value / 255.0, 0.0)),
            seed: time as u32,
            shader: effects.shader(shader),
            physics: crate::particle_physics::State::new(
                effects.code_primitive_definition(),
                0,
                0,
                0.0,
                sjk_effect::PrimitiveFlags::default(),
            ),
            shape: crate::particle_types::PrimitiveShape::Billboard,
        });
    }
}

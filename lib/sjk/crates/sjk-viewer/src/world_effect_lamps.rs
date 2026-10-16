//! Map-lifetime lighting for untriggered looping additive particle effects.
//! Source selection uses EFX/material data, not map, effect or texture names.
use super::*;
use crate::effect_envelope::Envelope;
use crate::lamp_lights::Lamp;
use glam::Vec3;
use sjk_effect::{ComponentKind, Curve};

/// Only permanently running placements qualify. OpenJK codemp/game/g_misc.c
/// SP_fx_runner uses bits 1/2 for STARTOFF/ONESHOT; targetname permits toggling.
fn steady(entity: &sjk_entity::Entity) -> bool {
    entity.classname() == Some("fx_runner")
        && entity.get("targetname").is_none()
        && entity
            .get("spawnflags")
            .unwrap_or("0")
            .parse::<u32>()
            .is_ok_and(|f| f & 3 == 0)
}

pub(super) fn extract(
    bsp: &Bsp,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
) -> Result<Vec<Lamp>, Box<dyn Error>> {
    let entities = sjk_entity::parse_entity_lump(bsp.entities())?;
    extract_placements(&entities, vfs, shaders)
}

fn extract_placements(
    entities: &[sjk_entity::Entity],
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
) -> Result<Vec<Lamp>, Box<dyn Error>> {
    let mut cache = ImageCache::new();
    let mut spectra = std::collections::HashMap::new();
    let mut lamps = Vec::new();
    for entity in entities.iter().filter(|e| steady(e)) {
        let Some(name) = entity.get("fxFile") else {
            continue;
        };
        let Some(position) = entity.get("origin").and_then(vector) else {
            continue;
        };
        let Ok(effect) = sjk_effect::load_effect(vfs, name) else {
            continue;
        };
        // Authored lights remain owned by the effect runtime. Nested graphs may have
        // their own lights or conditional emitters: do not invent persistent ones.
        if effect.components.iter().any(|c| {
            c.kind == ComponentKind::Light || !c.effects.is_empty() || !c.emit_effects.is_empty()
        }) {
            continue;
        }
        let period = number(entity.get("delay"), 200.) + number(entity.get("random"), 0.) * 0.5;
        let mut energy = Vec3::ZERO;
        for c in &effect.components {
            if !matches!(
                c.kind,
                ComponentKind::Particle | ComponentKind::OrientedParticle
            ) || c.flags.use_alpha
                || c.shaders.is_empty()
            {
                continue;
            }
            let mut radiance = Vec3::ZERO;
            for name in &c.shaders {
                if !spectra.contains_key(name) {
                    spectra.insert(name.clone(), spectrum(vfs, shaders, name, &mut cache)?);
                }
                radiance += spectra[name];
            }
            radiance /= c.shaders.len() as f32;
            let life = c.life.sample(0.5).clamp(0., 10000.);
            let count = c.count.sample(0.5).clamp(0., 64.);
            let size = envelope(c.size);
            let alpha = envelope(c.alpha);
            let rgb: [Envelope; 3] = std::array::from_fn(|i| {
                Envelope::from_values(
                    c.rgb_start[i].sample(0.5),
                    c.rgb_end[i].sample(0.5),
                    c.rgb_parameter.sample(0.5),
                    c.rgb_flags,
                )
            });
            // Integrate area, visible RGB and alpha over a representative lifetime.
            // Particle cull ranges deliberately do not affect permanent source power.
            for i in 0..32 {
                let t = (i as f32 + 0.5) * life / 32.;
                let radius = size.sample(t, life, 0).clamp(0., 128.);
                let color = Vec3::from_array(rgb.map(|e| e.sample(t, life, 0).clamp(0., 1.)));
                energy += radiance
                    * color
                    * (4.
                        * radius
                        * radius
                        * alpha.sample(t, life, 0).clamp(0., 1.)
                        * count
                        * life
                        / period.max(16.)
                        / 32.);
            }
        }
        let power = energy.dot(Vec3::new(0.2126, 0.7152, 0.0722)) * crate::lamp_lights::POWER_SCALE;
        if !power.is_finite() || power <= 0.01 {
            continue;
        }
        lamps.push(Lamp {
            position,
            normal: Vec3::ZERO,
            color: (energy * (crate::lamp_lights::POWER_SCALE / power)).to_array(),
            power,
            radius: (power / 0.004).sqrt().clamp(32., 1024.),
            axis_u: Vec3::ZERO,
            axis_v: Vec3::ZERO,
        });
    }
    crate::log::progress(format_args!("Steady effect lamps: {}", lamps.len()));
    Ok(lamps)
}

fn spectrum(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    name: &str,
    cache: &mut ImageCache,
) -> Result<Vec3, Box<dyn Error>> {
    let Some(definition) = shaders.get(name) else {
        return Ok(Vec3::ZERO);
    };
    let mut emission = [0.; 3];
    for source in &definition.stages {
        let mut stage = source.clone();
        // Particle vertex/entity colour is supplied by the EFX envelope above.
        if stage
            .rgb_generator
            .as_deref()
            .is_some_and(|s| matches!(s, "vertex" | "entity" | "exactVertex"))
        {
            stage.rgb_generator = Some("identity".into());
        }
        let (images, resolved, key) = load_stage_images(vfs, shaders, &stage, name, cache)?;
        super::super::emission::accumulate(
            &mut emission,
            &stage,
            &images,
            resolved || key.contains("$white;"),
            false,
            false,
        );
    }
    Ok(Vec3::from_array(emission))
}
fn envelope(c: Curve) -> Envelope {
    Envelope::from_values(
        c.start.sample(0.5),
        c.end.sample(0.5),
        c.parameter.sample(0.5),
        c.flags,
    )
}
fn number(text: Option<&str>, fallback: f32) -> f32 {
    text.and_then(|s| s.parse::<f32>().ok())
        .filter(|n| n.is_finite() && *n >= 0.)
        .unwrap_or(fallback)
}
fn vector(text: &str) -> Option<Vec3> {
    let mut words = text.split_whitespace();
    let p = Vec3::new(
        words.next()?.parse().ok()?,
        words.next()?.parse().ok()?,
        words.next()?.parse().ok()?,
    );
    p.is_finite().then_some(p)
}

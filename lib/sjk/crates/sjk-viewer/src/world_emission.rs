//! Static local-light input from the material's actual emissive texture stages.
//! Retail shader scripts often retain the glowing panel but no compiler light power;
//! ignoring these stages leaves a visible light fixture with no effect on its room.
#[path = "emission_texture.rs"]
mod texture;
use image::RgbaImage;
use sjk_shader::{ShaderStage, StageBlend, TextureGenerator};
use std::sync::Arc;
pub(crate) use texture::Texture;

/// White emission is four units of linear radiance. This is a renderer exposure
/// convention, not a recovered q3map intensity; the texture mask sets emitting area.
const RADIANCE: f32 = 4.;
/// Explicit glow on a fixed fixture is a stronger source than an unmarked additive
/// decoration. This is an artistic fallback, never a recovered compiler intensity.
const FIXTURE_RADIANCE: f32 = 16.;

/// Accumulate the time-average emission of one resolved, additive texture stage.
/// Reflection/specular and entity-dependent stages are view-dependent, not lamps.
pub(super) fn accumulate(
    sum: &mut [f32; 3],
    stage: &ShaderStage,
    images: &[Arc<RgbaImage>],
    resolved: bool,
    self_lit: bool,
    infer_fixture: bool,
) {
    // Follow later covers too: the FFA5 landing pad paints opaque metal back over
    // its effects, so only the uncovered portion can contribute to a lamp.
    match stage.blend {
        StageBlend::Replace => {
            *sum = [0.; 3];
            if let Some(radiance) = self_lit_radiance(stage, images, resolved, self_lit) {
                *sum = radiance;
            }
            return;
        }
        StageBlend::Alpha if sum.iter().any(|c| *c > 0.) => {
            let transmission = cover_transmission(stage, images, resolved);
            for c in sum {
                *c *= transmission;
            }
            return;
        }
        _ => {}
    }
    if !resolved
        || images.is_empty()
        || stage.texture_generator != TextureGenerator::Base
        || stage.blend != StageBlend::Add
    {
        return;
    }
    let Some(gain) = stage_gain(stage) else {
        return;
    };
    for image in images {
        let mean = additive_mean(image);
        for c in 0..3 {
            sum[c] +=
                mean[c] * gain[c] * stage_radiance(stage, infer_fixture) / images.len() as f32;
        }
    }
}

/// An unlit fixture needs luminous texels; dark housing must not dilute their test.
const SELF_LIT_FLOOR: f32 = 0.2;
/// Radiance of white self-lit paint. Such a fixture stands in for a point light the map
/// compiler baked and stripped, so it gets what a modest declared light surface gets
/// (`q3map_surfacelight 1000` is 1000/60), not the 4 of a decorative glow.
pub(super) const SELF_LIT_RADIANCE: f32 = 16.;

/// A material no light reaches: a script without a lightmap stage and without vertex,
/// diffuse or entity colours. Its opaque paint shows at full brightness in any light,
/// which is how mappers build fixtures whose light came from point-light entities the
/// map compiler baked and then stripped. With real-time lighting that bake is gone, so
/// the fixture itself must be the source, or the room it lit goes dark.
///
/// Only where nothing else says what the material emits: a declared surface light or an
/// additive glow stage has authority, and the paint under it stays paint. A full
/// opaque replacement hides earlier stages, which cannot light its visible paint.
///
/// A visible `alphaGen lightingSpecular` stage rules it out: a shine computed from the
/// light reaching the surface means the author meant a lit surface, not a light. Such
/// paint shows fullbright when an opaque stage covers the lightmap stage by mistake, as
/// on `JoFTemple`'s statues, whose dense meshes made 44,065 of the map's 58,124 lamps.
pub(super) fn self_lit(definition: Option<&sjk_shader::ShaderDefinition>) -> bool {
    definition.is_some_and(|definition| {
        !definition.stages.is_empty()
            && !(definition.surface_light.is_finite() && definition.surface_light > 0.)
            && definition.stages[definition
                .stages
                .iter()
                .rposition(|stage| {
                    stage.blend == StageBlend::Replace && stage.alpha_function.is_none()
                })
                .unwrap_or(0)..]
                .iter()
                .all(|stage| {
                    stage.texture_generator != TextureGenerator::Lightmap
                        && stage_gain(stage).is_some()
                        && !stage.glow
                        && stage.blend != StageBlend::Add
                        && !stage
                            .alpha_generator
                            .as_deref()
                            .is_some_and(|alpha| alpha.eq_ignore_ascii_case("lightingspecular"))
                })
    })
}

/// Radiance of a self-lit material's opaque paint, when it is bright enough to be a fixture.
pub(super) fn self_lit_radiance(
    stage: &ShaderStage,
    images: &[Arc<RgbaImage>],
    resolved: bool,
    self_lit: bool,
) -> Option<[f32; 3]> {
    if !self_lit
        || !resolved
        || images.is_empty()
        || stage.alpha_function.is_some()
        || stage.texture_generator != TextureGenerator::Base
    {
        return None;
    }
    let gain = stage_gain(stage)?;
    let mut radiance = [0.; 3];
    for image in images {
        let mean = additive_mean(image);
        for c in 0..3 {
            radiance[c] += mean[c] * gain[c] * SELF_LIT_RADIANCE / images.len() as f32;
        }
    }
    let luminance =
        (0.2126 * radiance[0] + 0.7152 * radiance[1] + 0.0722 * radiance[2]) / SELF_LIT_RADIANCE;
    let luminous = luminance >= SELF_LIT_FLOOR
        || images.iter().any(|image| {
            image.pixels().any(|pixel| {
                let linear = std::array::from_fn::<_, 3, _>(|c| {
                    (f32::from(pixel[c]) / 255.).powf(2.2) * gain[c]
                });
                0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2] >= SELF_LIT_FLOOR
            })
        });
    // Eligibility uses luminous texels; energy still uses the whole-area mean.
    luminous.then_some(radiance)
}

/// Shared gain for mean energy and spatial masks. Declared lights and effect
/// sprites keep their existing interpretation; only eligible fixed fixtures opt in.
fn stage_radiance(stage: &ShaderStage, infer_fixture: bool) -> f32 {
    if infer_fixture && stage.glow {
        FIXTURE_RADIANCE
    } else {
        RADIANCE
    }
}

fn stage_gain(stage: &ShaderStage) -> Option<[f32; 3]> {
    let rgb = stage
        .rgb_generator
        .as_deref()
        .unwrap_or("identity")
        .to_ascii_lowercase();
    Some(match rgb.as_str() {
        "identity" | "identitylighting" => [1.; 3],
        "const" | "constant" => stage.rgb_constant.unwrap_or([1.; 3]),
        "wave" => {
            let Some(wave) = &stage.rgb_wave else {
                return None;
            };
            [wave_mean(wave); 3]
        }
        _ => return None,
    })
}

/// Area-average transmission through an ordinary textured alpha cover. Unknown
/// vertex/entity alpha cannot be recovered at material load, so treat it as opaque.
fn cover_transmission(stage: &ShaderStage, images: &[Arc<RgbaImage>], resolved: bool) -> f32 {
    if !resolved || images.is_empty() {
        return 0.;
    }
    let alpha = match stage.alpha_generator.as_deref().unwrap_or("identity") {
        "identity" => 1.,
        "const" | "constant" => stage.alpha_constant.unwrap_or(1.).clamp(0., 1.),
        _ => return 0.,
    };
    images
        .iter()
        .map(|image| {
            let count = u64::from(image.width()) * u64::from(image.height());
            let opaque: u64 = image.pixels().map(|p| u64::from(p[3])).sum();
            1. - alpha * (opaque as f64 / (count.max(1) as f64 * 255.)) as f32
        })
        .sum::<f32>()
        / images.len() as f32
}

/// Average the clamped generator, so a lamp pulsing through zero still emits light.
/// The display stage keeps its animation; static lighting uses its cycle average.
fn wave_mean(wave: &sjk_shader::WaveForm) -> f32 {
    if wave.frequency == 0. {
        return crate::world_stage::evaluate_wave(wave, 0.).clamp(0., 1.);
    }
    let mut cycle = wave.clone();
    cycle.phase = 0.;
    cycle.frequency = 1.;
    cycle.function.make_ascii_lowercase();
    (0..64)
        .map(|i| {
            let t = (i as f32 + 0.5) / 64.;
            let value = if cycle.function == "noise" {
                cycle.base + cycle.amplitude * (2. * t - 1.)
            } else {
                crate::world_stage::evaluate_wave(&cycle, t)
            };
            value.clamp(0., 1.)
        })
        .sum::<f32>()
        / 64.
}

/// GL_ONE/GL_ONE uses RGB even when alpha is zero. Include black texels in the
/// average: a small strip in a large image must not emit like a full white panel.
fn additive_mean(image: &RgbaImage) -> [f32; 3] {
    static LINEAR: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    let linear = LINEAR.get_or_init(|| std::array::from_fn(|v| (v as f32 / 255.).powf(2.2)));
    let mut sum = [0f64; 3];
    let mut count = 0;
    // Scan the mask once at load time: strided sampling can miss a thin lamp strip.
    for pixel in image.pixels() {
        for c in 0..3 {
            sum[c] += f64::from(linear[pixel[c] as usize]);
        }
        count += 1;
    }
    sum.map(|c| (c / count.max(1) as f64) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(script: &str) -> sjk_shader::ShaderDefinition {
        sjk_shader::parse_shader_script(script.as_bytes(), "shaders/t.shader")
            .unwrap()
            .remove(0)
    }

    #[test]
    fn fullbright_paint_is_a_self_lit_fixture() {
        let lamp = definition("textures/t/lamp\n{\n{\nmap textures/t/lamp\n}\n}\n");
        assert!(self_lit(Some(&lamp)));
    }

    #[test]
    fn paint_with_a_lit_shine_is_not_a_light() {
        // JoFTemple's statues: the base covers the lightmap stage, and the shine
        // over it follows the light reaching the surface.
        let statue = definition(
            "models/t/statue\n{\n{\nmap $lightmap\nrgbGen identity\n}\n\
             {\nmap models/t/statue_base\n}\n\
             {\nmap models/t/statue_spec\nblendFunc GL_SRC_ALPHA GL_ONE\n\
             alphaGen lightingSpecular\n}\n}\n",
        );
        assert!(!self_lit(Some(&statue)));
    }
}

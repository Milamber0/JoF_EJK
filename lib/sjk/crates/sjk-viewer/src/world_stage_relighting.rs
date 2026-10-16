//! Recognize diffuse covers in legacy stacks that draw emission between two base layers.
use sjk_shader::{ShaderStage, StageBlend, TextureGenerator};

/// Stage metadata bit: sample live world light on an otherwise unlit diffuse cover.
/// This changes only day-mode composition; authored generators and alpha stay intact.
pub(crate) const DIFFUSE_COVER: u32 = 4;

/// A repeated base texture with alpha blending covers the preceding effect layers.
/// Its solid paint needs the same live light as the earlier diffuse layer. Additive,
/// glowing, animated-colour and unrelated textures must retain authored emission.
pub(crate) fn diffuse_cover(stage: &ShaderStage, stages: &[ShaderStage], lightmap: i32) -> bool {
    if stage.blend != StageBlend::Alpha || stage.glow || !identity_rgb(stage) {
        return false;
    }
    let lightmapped = lightmap >= 0
        && stages
            .iter()
            .any(|s| s.texture_generator == TextureGenerator::Lightmap);
    stages.iter().take_while(|s| *s != stage).any(|base| {
        let vertex_lit = lightmap == -3
            && base.rgb_generator.as_deref().is_some_and(|g| {
                g.eq_ignore_ascii_case("vertex") || g.eq_ignore_ascii_case("exactVertex")
            });
        (vertex_lit || (lightmapped && identity_rgb(base)))
            && matches!(base.blend, StageBlend::Replace | StageBlend::Filter)
            && !base.glow
            && !base.images.is_empty()
            && base.images.len() == stage.images.len()
            && base
                .images
                .iter()
                .zip(&stage.images)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
            && base.texture_generator == TextureGenerator::Base
            && stage.texture_generator == base.texture_generator
            && stage.texture_modifications == base.texture_modifications
            && stage.animation_frequency == base.animation_frequency
            && stage.one_shot == base.one_shot
            && stage.clamp == base.clamp
    })
}

fn identity_rgb(stage: &ShaderStage) -> bool {
    stage.rgb_constant.is_none()
        && stage.rgb_wave.is_none()
        && stage.rgb_generator.as_deref().is_none_or(|g| {
            g.eq_ignore_ascii_case("identity") || g.eq_ignore_ascii_case("identityLighting")
        })
}

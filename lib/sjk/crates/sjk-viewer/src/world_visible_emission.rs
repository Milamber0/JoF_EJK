//! Visible lamp radiance is attached to emissive texture bundles, never diffuse paint.
use crate::world_stage::{CompiledStage, GpuStage};
use sjk_shader::{ShaderDefinition, ShaderStage, StageBlend, TextureGenerator};

/// Use the renderer's four-unit white-emission convention on declared fixtures.
/// Legacy rendering keeps its original texture values and blend operation.
pub(super) fn configure(
    gpu: &mut GpuStage,
    stage: &CompiledStage,
    definition: Option<&ShaderDefinition>,
    fixed_opaque: bool,
) {
    if !fixed_opaque
        || !definition.is_some_and(|d| {
            d.surface_light.is_finite()
                && d.surface_light > 0.
                && d.sky.is_none()
                && d.deforms.is_empty()
        })
    {
        return;
    }
    gpu.emission[0] = if luminous(&stage.primary) { 4. } else { 0. };
    gpu.emission[1] = if stage.secondary.as_ref().is_some_and(luminous) {
        4.
    } else {
        0.
    };
}

fn luminous(stage: &ShaderStage) -> bool {
    if stage.texture_generator != TextureGenerator::Base {
        return false;
    }
    let authored_colour = stage.rgb_generator.as_deref().is_none_or(|g| {
        ["identity", "identitylighting", "const", "constant", "wave"]
            .iter()
            .any(|name| g.eq_ignore_ascii_case(name))
    });
    authored_colour
        && (stage.blend == StageBlend::Add
            || matches!(&stage.blend,
        StageBlend::Custom { source, destination } if
            source.eq_ignore_ascii_case("gl_dst_color") &&
            destination.eq_ignore_ascii_case("gl_one")))
}

/// Destination-modulated glow needs an additive pipeline in live lighting: its
/// emission must survive even when the diffuse receiver is completely dark.
pub(super) fn live_key(
    mut key: crate::world_stage::PipelineKey,
    gpu: &GpuStage,
) -> crate::world_stage::PipelineKey {
    // Specialize only luminous bundles: ordinary materials compile out the gain
    // entirely, keeping their previous register pressure and instruction path.
    if gpu.emission[0] > 0. || gpu.emission[1] > 0. {
        key.geometry |= 4;
    }
    if gpu.emission[0] > 0.
        && key.source == wgpu::BlendFactor::Dst
        && key.destination == wgpu::BlendFactor::One
    {
        crate::world_stage::PipelineKey {
            source: wgpu::BlendFactor::One,
            ..key
        }
    } else {
        key
    }
}

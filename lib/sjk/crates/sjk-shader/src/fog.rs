//! Shared material sort resolution and rd-vanilla volume-pass selection.
use crate::{ShaderDefinition, ShaderStage, StageBlend};

/// Depth test for `RB_FogPass` (rd-vanilla/tr_shade.cpp:1100-1128).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FogPass {
    /// No volume pass.
    #[default]
    None,
    /// Draw over the material's previously written depth.
    Equal,
    /// Fog sheets without a preceding depth-writing colour stage.
    LessEqual,
}

impl ShaderDefinition {
    /// Explicit sort or the shared `FinishShader` stage default. A `polygonOffset` shader
    /// without a sort is a decal (`SS_DECAL`, rd-vanilla/tr_shader.cpp `FinishShader`):
    /// drawn after the opaque surfaces it lies on.
    pub fn resolved_sort(&self) -> f32 {
        self.sort.unwrap_or_else(|| {
            if self.polygon_offset {
                4.0
            } else {
                infer_sort(&self.stages)
            }
        })
    }

    /// `GeneratePermanentShader`, rd-vanilla/tr_shader.cpp:2687-2693.
    /// `noglfog` disables this pass, including on opaque surfaces.
    pub fn fog_pass(&self) -> FogPass {
        if self.no_gl_fog || self.sky.is_some() {
            FogPass::None
        } else if self.resolved_sort() <= 5.0 {
            FogPass::Equal
        } else if self.fog_contents {
            FogPass::LessEqual
        } else {
            FogPass::None
        }
    }
}

/// Shared numeric sort inference for stage-bearing materials.
/// Port of rd-vanilla/tr_shader.cpp:3033-3150 (`FinishShader`).
pub fn infer_sort(stages: &[ShaderStage]) -> f32 {
    if stages
        .first()
        .is_none_or(|stage| stage.blend == StageBlend::Replace)
    {
        return 3.0;
    }
    for stage in stages {
        if stage.blend != StageBlend::Replace {
            if stage.depth_write {
                return 5.0;
            }
            return if stage.blend == StageBlend::Add {
                15.0
            } else {
                14.0
            };
        }
    }
    3.0
}

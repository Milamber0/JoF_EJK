//! rd-vanilla's `RF_FORCE_ENT_ALPHA` pipeline state for entity stages.
//!
//! A refEntity carrying `RF_FORCE_ENT_ALPHA` keeps its own shader, but every
//! stage is drawn with `GL_State(GLS_SRCBLEND_SRC_ALPHA |
//! GLS_DSTBLEND_ONE_MINUS_SRC_ALPHA)` after `ForceAlpha` replaced the vertex
//! alpha with `shaderRGBA[3]` (`codemp/rd-vanilla/tr_shade.cpp:1745-1757`).
//! Those state bits replace the stage's own: alpha blending, a `GL_LEQUAL`
//! depth test, no depth write (no `RF_ALPHA_DEPTH`) and no alpha test, which
//! [`FORCED_ALPHA`] removes from the stage program. Without that, a GE128 cut-out
//! whose forced alpha is below one half would discard every fragment. Culling and
//! `polygonOffset` are applied outside `GL_State` and stay the shader's, as do
//! the other specialization bits. The alpha override itself is the stage
//! program's `entity_control.y`.

use crate::world_stage::{FORCED_ALPHA, PipelineKey};

/// Pipeline state a stage with `key` uses when its entity forces alpha.
/// `alpha_tested` stages also drop their alpha test; other stages keep sharing
/// pipelines with ordinary alpha-blended stages.
pub(crate) fn key(key: PipelineKey, alpha_tested: bool) -> PipelineKey {
    PipelineKey {
        geometry: if alpha_tested {
            key.geometry | FORCED_ALPHA
        } else {
            key.geometry
        },
        source: wgpu::BlendFactor::SrcAlpha,
        destination: wgpu::BlendFactor::OneMinusSrcAlpha,
        depth_write: false,
        depth: wgpu::CompareFunction::LessEqual,
        cull: key.cull,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_stage_becomes_alpha_blended_without_depth_write() {
        let opaque = PipelineKey {
            geometry: 1 | 8,
            source: wgpu::BlendFactor::One,
            destination: wgpu::BlendFactor::Zero,
            depth_write: true,
            depth: wgpu::CompareFunction::LessEqual,
            cull: Some(wgpu::Face::Front),
        };
        let forced = key(opaque, false);
        assert_eq!(forced.source, wgpu::BlendFactor::SrcAlpha);
        assert_eq!(forced.destination, wgpu::BlendFactor::OneMinusSrcAlpha);
        assert!(!forced.depth_write);
        assert_eq!(forced.depth, wgpu::CompareFunction::LessEqual);
        // Deforms, polygonOffset and program selection are not GL_State bits.
        assert_eq!(forced.geometry, 1 | 8);
        assert_eq!(key(opaque, true).geometry, 1 | 8 | FORCED_ALPHA);
        assert_eq!(forced.cull, Some(wgpu::Face::Front));
    }

    #[test]
    fn equal_and_disabled_depth_tests_return_to_lequal() {
        for depth in [wgpu::CompareFunction::Equal, wgpu::CompareFunction::Always] {
            let stage = PipelineKey {
                geometry: 4,
                source: wgpu::BlendFactor::One,
                destination: wgpu::BlendFactor::One,
                depth_write: false,
                depth,
                cull: None,
            };
            let forced = key(stage, true);
            assert_eq!(forced.depth, wgpu::CompareFunction::LessEqual);
            assert_eq!(forced.geometry, 4 | FORCED_ALPHA);
            assert_eq!(forced.cull, None);
        }
    }
}

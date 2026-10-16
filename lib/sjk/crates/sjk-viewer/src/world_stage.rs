//! CPU compiler for generic Quake 3 world shader stages.
//!
//! Sort inference mirrors `FinishShader` in rd-vanilla `tr_shader.cpp:3033-3150`.
//! The fixed GPU representation follows `MAX_SHADER_STAGES == 8` and
//! `TR_MAX_TEXMODS == 4` from `tr_local.h:198,304`.

#[path = "world_stage_collapse.rs"]
mod collapse;
pub(crate) use collapse::collapse_multitexture;

#[path = "world_stage_relighting.rs"]
pub(crate) mod relighting;

use bytemuck::{Pod, Zeroable};
use sjk_shader::{
    DepthFunction, ShaderCull, ShaderDefinition, ShaderStage, StageBlend, TextureGenerator,
    TextureModification, WaveForm,
};

pub(crate) const MAX_SHADER_STAGES: usize = 8;
const MAX_TEXMODS: usize = 4;
pub(crate) const SORT_OPAQUE: f32 = 3.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuStage {
    pub(crate) animation: [f32; 4],
    pub(crate) generators: [f32; 4],
    pub(crate) rgb_wave: [f32; 4],
    pub(crate) alpha_wave: [f32; 4],
    pub(crate) constant_color: [f32; 4],
    pub(crate) wave_functions: [f32; 4],
    pub(crate) tcmod_a: [[f32; 4]; MAX_TEXMODS],
    pub(crate) tcmod_b: [[f32; 4]; MAX_TEXMODS],
    pub(crate) secondary_animation: [f32; 4],
    pub(crate) secondary_control: [f32; 4],
    pub(crate) secondary_tcmod_a: [[f32; 4]; MAX_TEXMODS],
    pub(crate) secondary_tcmod_b: [[f32; 4]; MAX_TEXMODS],
    pub(crate) deform_a: [[f32; 4]; 3],
    pub(crate) deform_b: [[f32; 4]; 3],
    pub(crate) deform_c: [[f32; 4]; 3],
    pub(crate) sprites: [[f32; 4]; 5],
    /// X/Y: live emission gain for primary/secondary texture bundles (zero means
    /// diffuse); Z shader time offset; W material gloss. Never a diffuse illumination term.
    pub(crate) emission: [f32; 4],
}

#[path = "surface_deform.rs"]
mod deform;
#[path = "geometry_uniform.rs"]
pub(crate) mod geometry_uniform;
pub(crate) use deform::compile as compile_deforms;

/// One hardware pass, optionally containing rd-vanilla's collapsed second bundle.
#[derive(Clone, Debug)]
pub(crate) struct CompiledStage {
    pub(crate) primary: ShaderStage,
    pub(crate) secondary: Option<ShaderStage>,
    pub(crate) combine: CollapseOperator,
    pub(crate) output_blend: StageBlend,
    pub(crate) output_depth_write: bool,
    pub(crate) output_depth_function: DepthFunction,
    /// rd-vanilla `shaderStage_t::glow` of the hardware pass: drawn again into the
    /// dynamic glow target ([`crate::frame_target::aa::glow`]). A collapsed pass keeps
    /// stage 0's flag (`CollapseMultitexture` moves only the texture bundles), so a
    /// glowing second stage merged under a non-glowing first one does not glow, as in
    /// stock. `primary.glow` keeps the source stage's own flag for emission inference.
    pub(crate) glow: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CollapseOperator {
    None,
    Modulate,
    Add,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PipelineKey {
    /// Specialization bits: 1 geometry deforms, 2 surface sprites, 4 live emission,
    /// 8 `polygonOffset` (a depth bias, no shader work), 16 forced entity alpha
    /// (no alpha test), 32 material maps (`material_maps::PIPELINE_BIT`). Each bit is
    /// its own: a shared one makes two kinds of stage share a pipeline layout.
    /// Zero strips the extra work out of ordinary material pipelines.
    pub(crate) geometry: u8,
    pub(crate) source: wgpu::BlendFactor,
    pub(crate) destination: wgpu::BlendFactor,
    pub(crate) depth_write: bool,
    pub(crate) depth: wgpu::CompareFunction,
    pub(crate) cull: Option<wgpu::Face>,
}

pub(crate) fn material_stages(
    definition: Option<&ShaderDefinition>,
    lightmap: i32,
) -> (Vec<ShaderStage>, f32, ShaderCull) {
    if let Some(definition) = definition {
        let sort = definition.resolved_sort();
        return (
            definition
                .stages
                .iter()
                .filter(|stage| stage.surface_sprites.is_none())
                .cloned()
                .collect(),
            sort,
            definition.cull,
        );
    }
    let diffuse = implicit_stage(TextureGenerator::Base, StageBlend::Filter, lightmap >= 0);
    if lightmap >= 0 {
        (
            vec![
                implicit_stage(TextureGenerator::Lightmap, StageBlend::Replace, true),
                diffuse,
            ],
            SORT_OPAQUE,
            ShaderCull::Front,
        )
    } else {
        let mut stage = implicit_stage(TextureGenerator::Base, StageBlend::Replace, true);
        // R_FindShader's default shading commands (`tr_shader.cpp:3483-3495`):
        // an unscripted texture on a model (`LIGHTMAP_NONE`, -1) is lit by
        // `CGEN_LIGHTING_DIFFUSE`; pre-lit world surfaces keep vertex colour.
        if lightmap == LIGHTMAP_NONE {
            stage.rgb_generator = Some("lightingDiffuse".into());
        } else if lightmap == LIGHTMAP_BY_VERTEX {
            stage.rgb_generator = Some("exactVertex".into());
            // Stock EXACT_VERTEX copies RGBA and AGEN_SKIP leaves that alpha.
            // Our RGB/alpha generators are separate, so retain vertex alpha explicitly.
            stage.alpha_generator = Some("vertex".into());
        }
        (vec![stage], SORT_OPAQUE, ShaderCull::Front)
    }
}

/// rd-vanilla `LIGHTMAP_NONE` (`tr_local.h:437`): models registered without a
/// lightmap, as opposed to `LIGHTMAP_BY_VERTEX` (-3) pre-lit world surfaces.
pub(crate) const LIGHTMAP_NONE: i32 = -1;
/// BSP meshes whose compiled lighting is stored in their vertex colours.
pub(crate) const LIGHTMAP_BY_VERTEX: i32 = -3;

fn implicit_stage(
    generator: TextureGenerator,
    blend: StageBlend,
    depth_write: bool,
) -> ShaderStage {
    ShaderStage {
        surface_sprites: None,
        portal_range: None,
        images: if generator == TextureGenerator::Lightmap {
            vec!["$lightmap".into()]
        } else {
            Vec::new()
        },
        video: None,
        animation_frequency: None,
        one_shot: false,
        clamp: false,
        blend,
        glow: false,
        alpha_function: None,
        rgb_generator: Some("identity".into()),
        alpha_generator: Some("identity".into()),
        resolved_colour: sjk_shader::StageColour::IDENTITY,
        rgb_wave: None,
        alpha_wave: None,
        texture_modifications: Vec::new(),
        rgb_constant: None,
        alpha_constant: None,
        texture_generator: generator,
        depth_write,
        depth_function: DepthFunction::LessEqual,
        material: Default::default(),
    }
}

pub(crate) fn compile_stage(stage: &ShaderStage) -> GpuStage {
    let mut result = GpuStage::zeroed();
    result.animation = [
        stage.animation_frequency.unwrap_or(0.0),
        stage.images.len().max(1) as f32,
        f32::from(stage.one_shot),
        match stage.texture_generator {
            TextureGenerator::Base => 0.0,
            TextureGenerator::Lightmap => 1.0,
            TextureGenerator::Environment => 2.0,
        },
    ];
    result.generators = [
        generator_code(stage.rgb_generator.as_deref(), stage.rgb_constant.is_some()),
        generator_code(
            stage.alpha_generator.as_deref(),
            stage.alpha_constant.is_some(),
        ),
        alpha_function_code(stage.alpha_function.as_deref()),
        stage.texture_modifications.len().min(MAX_TEXMODS) as f32,
    ];
    result.rgb_wave = wave_parameters(stage.rgb_wave.as_ref());
    result.alpha_wave = wave_parameters(stage.alpha_wave.as_ref());
    let rgb = stage.rgb_constant.unwrap_or([1.0; 3]);
    result.constant_color = [rgb[0], rgb[1], rgb[2], stage.alpha_constant.unwrap_or(1.0)];
    result.wave_functions = [
        stage
            .rgb_wave
            .as_ref()
            .map_or(0.0, |wave| wave_code(&wave.function)),
        stage
            .alpha_wave
            .as_ref()
            .map_or(0.0, |wave| wave_code(&wave.function)),
        stage.portal_range.unwrap_or(256.0),
        0.0,
    ];
    for (index, modification) in stage
        .texture_modifications
        .iter()
        .take(MAX_TEXMODS)
        .enumerate()
    {
        let (a, b) = compile_tcmod(modification);
        result.tcmod_a[index] = a;
        result.tcmod_b[index] = b;
    }
    result
}

pub(crate) fn compile_hardware_stage(stage: &CompiledStage) -> GpuStage {
    let mut result = compile_stage(&stage.primary);
    if let Some(secondary) = &stage.secondary {
        result.secondary_animation = animation_parameters(secondary);
        result.secondary_control = [
            secondary.texture_modifications.len().min(MAX_TEXMODS) as f32,
            match stage.combine {
                CollapseOperator::None => 0.0,
                CollapseOperator::Modulate => 1.0,
                CollapseOperator::Add => 2.0,
            },
            1.0,
            0.0,
        ];
        for (index, modification) in secondary
            .texture_modifications
            .iter()
            .take(MAX_TEXMODS)
            .enumerate()
        {
            let (a, b) = compile_tcmod(modification);
            result.secondary_tcmod_a[index] = a;
            result.secondary_tcmod_b[index] = b;
        }
    }
    result
}

fn animation_parameters(stage: &ShaderStage) -> [f32; 4] {
    [
        stage.animation_frequency.unwrap_or(0.0),
        stage.images.len().max(1) as f32,
        f32::from(stage.one_shot),
        match stage.texture_generator {
            TextureGenerator::Base => 0.0,
            TextureGenerator::Lightmap => 1.0,
            TextureGenerator::Environment => 2.0,
        },
    ]
}

fn generator_code(generator: Option<&str>, constant: bool) -> f32 {
    if constant {
        return 6.0;
    }
    match generator
        .unwrap_or("identity")
        .to_ascii_lowercase()
        .as_str()
    {
        "identitylighting" => 1.0,
        "vertex" => 2.0,
        "exactvertex" => 3.0,
        "lightingdiffuse" => 4.0,
        "wave" => 5.0,
        "entity" => 7.0,
        "oneminusentity" => 8.0,
        "lightingdiffuseentity" => 9.0,
        "lightingspecular" => 10.0,
        "portal" => 11.0,
        _ => 0.0,
    }
}

fn alpha_function_code(function: Option<&str>) -> f32 {
    match function.unwrap_or("").to_ascii_lowercase().as_str() {
        "gt0" => 1.0,
        "lt128" => 2.0,
        "ge128" => 3.0,
        // Grass and other surface sprites: without it the whole sprite quad blended and
        // wrote depth, a pane of glass that hid what stood behind it.
        "ge192" => 4.0,
        _ => 0.0,
    }
}

fn wave_parameters(wave: Option<&WaveForm>) -> [f32; 4] {
    wave.map_or([0.0; 4], |wave| {
        [wave.base, wave.amplitude, wave.phase, wave.frequency]
    })
}

fn wave_code(function: &str) -> f32 {
    match function.to_ascii_lowercase().as_str() {
        "square" => 1.0,
        "triangle" => 2.0,
        "sawtooth" => 3.0,
        "inversesawtooth" => 4.0,
        "noise" => 5.0,
        "rand" => 6.0,
        _ => 0.0,
    }
}

fn compile_tcmod(modification: &TextureModification) -> ([f32; 4], [f32; 4]) {
    let argument = |index| modification.arguments.get(index).copied().unwrap_or(0.0);
    match modification.kind.as_str() {
        "scroll" => ([1.0, argument(0), argument(1), 0.0], [0.0; 4]),
        "scale" => ([2.0, argument(0), argument(1), 0.0], [0.0; 4]),
        "rotate" => ([3.0, argument(0), 0.0, 0.0], [0.0; 4]),
        "turb" => (
            [4.0, argument(0), argument(1), argument(2)],
            [argument(3), 0.0, 0.0, 0.0],
        ),
        "stretch" => {
            let wave = modification.wave.as_ref();
            (
                [
                    5.0,
                    wave.map_or(0.0, |wave| wave_code(&wave.function)),
                    wave.map_or(0.0, |wave| wave.base),
                    wave.map_or(0.0, |wave| wave.amplitude),
                ],
                [
                    wave.map_or(0.0, |wave| wave.phase),
                    wave.map_or(0.0, |wave| wave.frequency),
                    0.0,
                    0.0,
                ],
            )
        }
        "transform" => (
            [6.0, argument(0), argument(1), argument(2)],
            [argument(3), argument(4), argument(5), 0.0],
        ),
        // `TMOD_ENTITY_TRANSLATE` adds `backEnd.currentEntity->e.shaderTexCoord`
        // in rd-vanilla `tr_shade_calc.cpp:1058-1061`; the values arrive in
        // the generic per-instance record rather than the material table.
        "entitytranslate" => ([7.0, 0.0, 0.0, 0.0], [0.0; 4]),
        _ => ([0.0; 4], [0.0; 4]),
    }
}

/// [`PipelineKey::geometry`] bit of a `polygonOffset` material.
pub(crate) const POLYGON_OFFSET: u8 = 8;
/// [`PipelineKey::geometry`] bit of an `RF_FORCE_ENT_ALPHA` pipeline: its fixed
/// `GL_State` carries no alpha-test bits, so the stage program skips the alpha test.
pub(crate) const FORCED_ALPHA: u8 = 16;
/// `qglPolygonOffset(r_offsetFactor, r_offsetUnits)` with the -1 / -2 defaults
/// (`tr_shade.cpp:1811`, `tr_init.cpp:1666-1667`): markings laid onto a surface win the
/// depth test against it instead of fighting with it.
pub(crate) const POLYGON_OFFSET_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: -2,
    slope_scale: -1.0,
    clamp: 0.0,
};

pub(crate) fn pipeline_key(stage: &ShaderStage, cull: ShaderCull) -> PipelineKey {
    let (source, destination) = blend_factors(&stage.blend);
    PipelineKey {
        geometry: if stage.surface_sprites.is_some() {
            2
        } else {
            0
        },
        source,
        destination,
        depth_write: stage.depth_write,
        depth: match stage.depth_function {
            DepthFunction::LessEqual => wgpu::CompareFunction::LessEqual,
            DepthFunction::Equal => wgpu::CompareFunction::Equal,
            DepthFunction::Disabled => wgpu::CompareFunction::Always,
        },
        cull: cull_face(cull),
    }
}

/// Map a Q3 cull type onto wgpu with the default counter-clockwise front face.
/// Q3 geometry is wound clockwise, so `GL_Cull` (`codemp/rd-vanilla/tr_backend.cpp:130`)
/// implements `CT_FRONT_SIDED` as `glCullFace(GL_FRONT)` against GL's default
/// `GL_CCW` front face; the wgpu equivalent is culling `Face::Front`.
pub(crate) fn cull_face(cull: ShaderCull) -> Option<wgpu::Face> {
    match cull {
        ShaderCull::TwoSided => None,
        ShaderCull::Front => Some(wgpu::Face::Front),
        ShaderCull::Back => Some(wgpu::Face::Back),
    }
}

pub(crate) fn hardware_pipeline_key(stage: &CompiledStage, cull: ShaderCull) -> PipelineKey {
    let mut primary = stage.primary.clone();
    primary.blend = stage.output_blend.clone();
    primary.depth_write = stage.output_depth_write;
    primary.depth_function = stage.output_depth_function;
    pipeline_key(&primary, cull)
}

fn blend_factors(blend: &StageBlend) -> (wgpu::BlendFactor, wgpu::BlendFactor) {
    match blend {
        StageBlend::Replace => (wgpu::BlendFactor::One, wgpu::BlendFactor::Zero),
        StageBlend::Add => (wgpu::BlendFactor::One, wgpu::BlendFactor::One),
        StageBlend::Filter => (wgpu::BlendFactor::Dst, wgpu::BlendFactor::Zero),
        StageBlend::Alpha => (
            wgpu::BlendFactor::SrcAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        ),
        StageBlend::Custom {
            source,
            destination,
        } => (blend_factor(source), blend_factor(destination)),
    }
}

fn blend_factor(value: &str) -> wgpu::BlendFactor {
    match value.to_ascii_lowercase().as_str() {
        "gl_zero" => wgpu::BlendFactor::Zero,
        "gl_src_color" => wgpu::BlendFactor::Src,
        "gl_one_minus_src_color" => wgpu::BlendFactor::OneMinusSrc,
        "gl_src_alpha" => wgpu::BlendFactor::SrcAlpha,
        "gl_one_minus_src_alpha" => wgpu::BlendFactor::OneMinusSrcAlpha,
        "gl_dst_color" => wgpu::BlendFactor::Dst,
        "gl_one_minus_dst_color" => wgpu::BlendFactor::OneMinusDst,
        "gl_dst_alpha" => wgpu::BlendFactor::DstAlpha,
        "gl_one_minus_dst_alpha" => wgpu::BlendFactor::OneMinusDstAlpha,
        "gl_src_alpha_saturate" => wgpu::BlendFactor::SrcAlphaSaturated,
        _ => wgpu::BlendFactor::One,
    }
}

/// CPU-side rd-vanilla table evaluation used by fixtures and evidence JSON.
pub(crate) fn evaluate_wave(wave: &WaveForm, time: f32) -> f32 {
    let phase = wave.phase + time * wave.frequency;
    let index = ((phase * 1024.0) as i32 & 1023) as usize;
    let table = match wave.function.as_str() {
        "square" => {
            if index < 512 {
                1.0
            } else {
                -1.0
            }
        }
        "triangle" => {
            let quarter = index / 256;
            let fraction = (index & 255) as f32 / 256.0;
            match quarter {
                0 => fraction,
                1 => 1.0 - fraction,
                2 => -fraction,
                _ => -1.0 + fraction,
            }
        }
        "sawtooth" => index as f32 / 1024.0,
        "inversesawtooth" => 1.0 - index as f32 / 1024.0,
        _ => ((index as f32 * std::f32::consts::TAU) / 1023.0).sin(),
    };
    wave.base + table * wave.amplitude
}

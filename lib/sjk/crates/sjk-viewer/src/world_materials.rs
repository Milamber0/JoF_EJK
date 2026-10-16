//! Map-lifetime GPU resources and draw lists for Q3 world shader stages.
//!
//! Static BSP, inline movers, skinned Ghoul2 actors, and rigid MD3/GLM models
//! share this runtime. Stage compilation lives exclusively in `world_stage`;
//! `model_materials` is only a cgame override-name adapter.

#[path = "depth_prime.rs"]
mod depth_prime;
#[path = "world_material_draw.rs"]
mod draw;
#[path = "dynamic_light_shadows.rs"]
pub(crate) mod dynamic_light_shadows;
#[path = "gi_probe_domain.rs"]
mod gi_probe_domain;
#[path = "indirect_draws.rs"]
mod indirect_draws;
#[path = "lamp_cache.rs"]
pub(crate) mod lamp_cache;
#[path = "lamp_geometry.rs"]
pub(crate) mod lamp_geometry;
#[path = "lighting_environment.rs"]
mod lighting_environment;
#[path = "world_lighting_mode.rs"]
pub(crate) mod lighting_mode;
#[path = "mover_occlusion.rs"]
pub(crate) mod mover_occlusion;

#[cfg(test)]
#[path = "world_decal_order_tests.rs"]
mod decal_order_tests;
#[cfg(test)]
#[path = "world_shader_fxc_tests.rs"]
mod fxc_tests;
#[path = "sun_shadows.rs"]
pub(crate) mod shadows;
#[path = "stage_table.rs"]
pub(crate) mod stage_table;

#[path = "world_card_outline.rs"]
mod card_outline;
#[path = "world_flares.rs"]
pub(crate) mod flares;
#[path = "fog_draws.rs"]
mod fog_draws;
#[path = "fog_gpu.rs"]
mod fog_gpu;
#[path = "world_forced_alpha.rs"]
mod forced_alpha;
#[path = "world_material_forge.rs"]
mod forge;
#[path = "world_glow.rs"]
mod glow;
#[path = "world_material_gpu.rs"]
mod gpu;
#[path = "material_maps.rs"]
pub(crate) mod material_maps;
#[path = "model_light_grid.rs"]
mod model_grid;
#[path = "world_pipeline_jobs.rs"]
pub(crate) mod pipeline_jobs;
#[path = "world_ssao.rs"]
pub(crate) mod ssao;
#[path = "world_texture_prepare.rs"]
mod texture_prepare;
#[path = "world_videos.rs"]
pub(crate) mod videos;

#[path = "world_map_remaps.rs"]
pub(crate) mod map_remaps;
#[path = "world_shader_remaps.rs"]
mod remaps;
use fog_draws::FogDraw;
pub(crate) use fog_draws::FrameDraw;
use fog_gpu::FogGpu;
pub(crate) use remaps::remap_target;
use sjk_shader::{FogPass, ShaderCull};

use super::{DrawBatch, ViewerMaterial, create_rgba8_texture};
#[path = "world_area_visibility.rs"]
pub(crate) mod areas;
use crate::dynamic_lights::{GpuPointLightBlock, PointLightList};
use crate::world_stage::{
    CompiledStage, GpuStage, MAX_SHADER_STAGES, PipelineKey, SORT_OPAQUE, collapse_multitexture,
    compile_hardware_stage, hardware_pipeline_key, material_stages,
};
pub(crate) use forge::{DetachedDraw, DetachedMaterials, PreparedTexture, warm_entity_materials};
use forge::{Forge, build_passes, compile_material};
use gpu::{
    create_entity_pipeline, create_pipeline, create_sampler, create_stage_layout, load_stage_images,
};
pub(crate) use gpu::{upload_array, upload_lightmaps};
use image::RgbaImage;
use sjk_bsp::{Bsp, Visibility};
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;
use std::num::NonZeroU64;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;

#[path = "gi_runtime.rs"]
pub(crate) mod gi;
#[path = "gi_probes.rs"]
pub(crate) mod gi_probes;
#[path = "model_sun.rs"]
mod model_sun;

macro_rules! stage_shader {
    ($sun:expr, $flares:expr) => {
        concat!(
            include_str!("vertex_transform.wgsl"),
            include_str!("surface_orientation.wgsl"),
            include_str!("gpu_skinning.wgsl"),
            include_str!("geometry_stage.wgsl"),
            include_str!("stage_runtime.wgsl"),
            include_str!("point_lights.wgsl"),
            include_str!("point_light_octa.wgsl"),
            include_str!("world_lighting_mode.wgsl"),
            include_str!("light_grid_sample.wgsl"),
            include_str!("model_light_grid.wgsl"),
            include_str!("surface_deform.wgsl"),
            include_str!("surface_tables.wgsl"),
            include_str!("surface_noise.wgsl"),
            include_str!("surface_sprites.wgsl"),
            $sun,
            $flares,
        )
    };
}
const STAGE_SHADER: &str = stage_shader!(
    include_str!("model_sun_off.wgsl"),
    include_str!("surface_flares.wgsl")
);
/// The shared real-time sources (sun visibility, probes, lamps and the light sum) with
/// the receiver group at `group`.
pub(crate) fn realtime_sources(group: u32) -> (String, String, String) {
    let visibility = include_str!("sun_visibility.wgsl").to_string();
    let probes = format!(
        "{}",
        gi_probes::sample_source(group, model_sun::PROBE_BASE)
            .replace(
                &format!("@group({group}) @binding(12) var probe_far_map: texture_depth_2d;"),
                ""
            )
            .replace("probe_far_map", "far_map")
    );
    let realtime = format!(
        "{}{}",
        crate::lamp_lights::source(group, model_sun::LAMP_BASE, true),
        concat!(
            include_str!("ambient_fill.wgsl"),
            include_str!("lamp_response.wgsl"),
            include_str!("sun_realtime.wgsl")
        )
        .replace("@group(3)", &format!("@group({group})"))
    );

    (visibility, probes, realtime)
}

/// The light pass's entity program: the stage program's vertex path (skinning, deforms)
/// with the light pass fragments, the receiver group at group 3 behind the stage and
/// geometry groups. Entities are lit into the buffer like the world and the stage
/// pipelines only upsample it, so no per-key program carries the cascades and probes.
pub(crate) fn entity_light_shader() -> String {
    let (visibility, probes, realtime) = realtime_sources(3);
    stage_shader!(include_str!("sun_visibility.wgsl"), "")
        .to_string()
        .replace(
            include_str!("sun_visibility.wgsl"),
            &format!(
                "{}{}{}{}{}{}{}{}",
                include_str!("sun_shadow_uniform.wgsl"),
                visibility,
                probes,
                realtime,
                include_str!("light_pass.wgsl").replace("@group(1)", "@group(3)"),
                include_str!("light_pass_entity.wgsl"),
                include_str!("model_sun_off.wgsl"),
                include_str!("sun_contact.wgsl")
            ),
        )
}

/// The world program of the real-time mode: the stage shader reading the half-resolution
/// light buffer instead of evaluating the cascades and probes per pixel.
pub(crate) fn world_sun_shader() -> &'static str {
    stage_shader!(
        concat!(
            include_str!("sun_shadow_uniform.wgsl"),
            include_str!("sun_highlights.wgsl"),
            include_str!("sun_realtime_buffer.wgsl")
        ),
        ""
    )
}

impl Runtime {
    /// The material maps of world material `source` (an index into the flattened
    /// scene's materials), for a floor mirror drawn over its surfaces.
    pub(crate) fn floor_maps(&self, source: usize) -> Option<&material_maps::FloorMaps> {
        let index = *self.source_to_runtime.get(source)?;
        self.materials
            .get(index)?
            .stages
            .iter()
            .find_map(|stage| stage.floor_maps.as_ref())
    }

    /// The map's reflection probes, when material maps placed any.
    pub(crate) fn reflection_probes(&self) -> Option<&material_maps::reflections::gpu::Probes> {
        self.forge.material_maps.as_ref()?.reflections.as_ref()
    }
}

#[path = "world_draw_ranges.rs"]
pub(crate) mod draw_ranges;
#[path = "view_culling.rs"]
mod view_culling;

struct StagePass {
    /// What this stage gives the map's stage table; entity-only stages carry none.
    table: Option<stage_table::Source>,
    shadow_caster: bool,
    /// Opaque with an alpha test on its first texture (leaves, grates): the light pass draws
    /// it through the alpha-testing entity pipeline, so its holes stay holes.
    light_cutout: bool,
    ao_receiver: bool,
    bind_group: wgpu::BindGroup,
    /// The colour passes' group of a material-mapped stage (`material_maps`): the stage
    /// entries plus its maps. Other passes keep binding `bind_group`.
    material_group: Option<wgpu::BindGroup>,
    /// The maps of a material-mapped stage again, for the floor mirrors' finish.
    floor_maps: Option<material_maps::FloorMaps>,
    geometry_group: wgpu::BindGroup,
    pipeline: usize,
    live_pipeline: usize,
    /// `pipeline` and `live_pipeline` under `RF_FORCE_ENT_ALPHA` ([`forced_alpha`]).
    forced_alpha_pipelines: [usize; 2],
    /// Drawn again into the dynamic glow target ([`glow`]).
    glow: bool,
    /// Drawn there only for its emission map (`r_emissiveGlow` can leave it out).
    emission_glow: bool,
}

impl StagePass {
    /// The group the stage's own pipelines (`pipeline`, `live_pipeline`) draw with.
    fn color_group(&self) -> &wgpu::BindGroup {
        self.material_group.as_ref().unwrap_or(&self.bind_group)
    }
}

#[derive(Clone)]
struct StaticDraw {
    view_cache: view_culling::Cache,
    indices: Range<u32>,
    clusters: Vec<usize>,
    /// World AABB from the flattened scene.
    bounds: [glam::Vec3; 2],
}

#[derive(Clone)]
struct MoverDraw {
    mesh: usize,
    indices: Range<u32>,
}

struct Material {
    view_bounded: bool,
    flare: bool,
    fog_pass: FogPass,
    fog_pipeline: Option<usize>,
    fog_draws: Vec<FogDraw>,
    blended: bool,
    /// Cull index (front, back, none) of an opaque material the half-resolution light pass
    /// draws; `None` surfaces upsample the nearest buffered depth instead.
    light_buffered: Option<u8>,
    /// The light pass draws the static surfaces with the first stage's alpha test. Without
    /// it a cut-out surface borrowed the light of whatever stood behind it and read as
    /// tinted glass: bright before a sunlit wall, dark before a shadow.
    light_cutout: bool,
    sort: f32,
    /// rd-vanilla `shader_t::hasGlow`: at least one stage glows.
    has_glow: bool,
    stages: Vec<StagePass>,
    static_draws: Vec<StaticDraw>,
    camera_ranges: draw_ranges::Cache,
    static_draws_by_cluster: Vec<Vec<StaticDraw>>,
    mover_draws: Vec<MoverDraw>,
}

/// Immutable map-lifetime stage tables, textures, pipelines, and draw lists.
/// The draw methods only traverse prebuilt lists and perform no allocation.
pub(crate) struct Runtime {
    pub(crate) view_culling: view_culling::State,
    shadow_bounds: [glam::Vec3; 2],
    shadow_hulls: Option<shadows::hulls::Hulls>,
    /// Every static sun caster as joined index runs, built on first use per map.
    caster_runs: std::cell::OnceCell<Vec<Range<u32>>>,
    depth_prime: std::cell::OnceCell<[wgpu::RenderPipeline; 3]>,
    /// BSP-sealed void/solid cells on the fixed probe lattice; load-time only.
    probe_domain: gi_probe_domain::Domain,
    environment_policy: lighting_environment::Policy,
    /// Optional probe-GI foundation, installed only for the real-time lighting mode.
    pub(crate) gi: Option<gi::Runtime>,
    /// Bounce colour and emission per flattened (source) material index.
    surfaces_by_source: Vec<crate::gi_voxels::Surface>,
    /// Local lights from the map's emissive faces (real-time mode).
    lamps: crate::lamp_lights::LampSet,
    /// The movers' opaque casters (`mover_occlusion.rs`): door tiles for the lamps near
    /// them and, with or without, the far sun cascade's refreshes.
    pub(crate) mover_occluders: Vec<mover_occlusion::Occluder>,
    /// One bind group for every opaque static world stage, where the device allows.
    stage_table: Option<stage_table::Table>,

    /// Per-view indirect draw lists of the material-free geometry passes.
    indirect: Option<indirect_draws::Lists>,
    /// Materials the current PVS cluster can show; see `draw_ranges::Active`.
    active: std::cell::RefCell<draw_ranges::Active>,
    /// Which world surfaces the static lamp cache serves; `None`: evaluate lamps directly.
    lamp_cache_pages: Option<lamp_cache::Pages>,
    /// The baked cache, created with the first real-time lighting setup of this map.
    lamp_cache: Option<lamp_cache::Cache>,
    shadows: Option<shadows::Runtime>,
    ssao: ssao::AmbientOcclusion,
    /// Accepted snapshot visibility, independent of entity and collision culling.
    pub(crate) areas: areas::Areas,
    flares: flares::Runtime,
    pub(crate) fog_mode: crate::fog_volumes::Mode,
    pub(crate) fogged_entities: usize,
    fog: FogGpu,
    opaque_fog: Vec<FogDraw>,
    opaque_fog_by_cluster: Vec<Vec<FogDraw>>,
    sky: crate::sky_stage::Runtime,
    materials: Vec<Material>,
    source_to_runtime: Vec<usize>,
    /// Immutable sort/pipeline keys indexed by source, also extended for late materials.
    source_order: Vec<(f32, usize)>,
    /// One depth-tested pipeline per key for world statics (bound to the forge's identity
    /// instance), movers and entities alike, plus the entities' no-depth variant. Slots
    /// fill at load for the keys the scene uses and on first draw otherwise (every
    /// distinct program costs the driver about half a second on a cold cache).
    entity_pipelines: Vec<std::cell::OnceCell<wgpu::RenderPipeline>>,
    entity_no_depth_pipelines: Vec<std::cell::OnceCell<wgpu::RenderPipeline>>,
    opaque_order: Vec<PassRef>,
    /// The decals drawn before the opaque world's fog ([`underlay`]).
    decal_order: Vec<PassRef>,
    blended_order: Vec<PassRef>,
    /// The glowing passes of `opaque_order`, `decal_order` then `blended_order`, for the
    /// glow pass.
    glow_order: Vec<PassRef>,
    /// Glow-target variants of the pipeline keys, compiled on first glow draw.
    glow_pipelines: glow::Pipelines,
    /// The player card outline's mask pipeline, compiled on its first draw.
    card_outline_mask: std::cell::OnceCell<wgpu::RenderPipeline>,
    /// Bumped when the pipeline slots empty for a new program, so pipelines a worker
    /// compiled against the old one are dropped ([`pipeline_jobs`]).
    pipeline_generation: u32,
    dynamic_light_buffer: wgpu::Buffer,
    lighting_mode: std::cell::Cell<u32>,
    forge: Forge,
    remaps: remaps::State,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PassRef {
    material: usize,
    stage: usize,
}

type ImageCache = std::collections::HashMap<String, Arc<RgbaImage>>;

struct PendingStage {
    allow_ssao: bool,
    /// The hardware pass glows ([`CompiledStage::glow`]).
    glow: bool,
    gpu: GpuStage,
    primary_pixels: Vec<Arc<RgbaImage>>,
    primary_key: String,
    primary_clamp: bool,
    secondary_pixels: Option<Vec<Arc<RgbaImage>>>,
    secondary_key: Option<String>,
    secondary_clamp: bool,
    lightmap: wgpu::TextureView,
    key: PipelineKey,
    /// Decoded material maps; the key then carries `material_maps::PIPELINE_BIT`.
    maps: Option<material_maps::StageMaps>,
}

struct PendingMaterial {
    view_bounded: bool,
    flare: bool,
    fog_pass: FogPass,
    fog_pipeline: Option<usize>,
    fog_draws: Vec<FogDraw>,
    source_index: usize,
    /// Bounce colour and emission for global illumination.
    surface: crate::gi_voxels::Surface,
    emission_texture: emission::Texture,
    /// The emission comes from an emission map: its lamps are capped separately.
    mapped_emission: bool,
    sort: f32,
    stages: Vec<PendingStage>,
    static_draws: Vec<StaticDraw>,
    mover_draws: Vec<MoverDraw>,
}

#[path = "world_material_build.rs"]
mod build;
#[path = "world_emission.rs"]
pub(crate) mod emission;
#[path = "world_visible_emission.rs"]
mod visible_emission;
pub(crate) use build::create_filtered_runtime;

#[path = "texture_filtering.rs"]
pub(crate) mod filtering;

fn finish_runtime(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    mut forge: Forge,
    sky: crate::sky_stage::Runtime,
    fog: FogGpu,
    pending: Vec<PendingMaterial>,
    resolved: usize,
) -> Result<(Runtime, usize), Box<dyn Error>> {
    texture_prepare::upload(device, queue, &mut forge, &pending)?;
    let dynamic_light_buffer = forge.point_lights.clone();
    let source_count = pending
        .iter()
        .map(|material| material.source_index)
        .max()
        .map_or(0, |maximum| maximum + 1);
    let mut source_to_runtime = vec![usize::MAX; source_count];
    let mut surfaces_by_source = vec![crate::gi_voxels::Surface::default(); source_count];
    let mut runtime_materials = Vec::with_capacity(pending.len());
    let mut opaque_fog = Vec::new();
    for mut material in pending {
        surfaces_by_source[material.source_index] = material.surface;
        // Sort-5 depth-writing blends must draw colour before their EQUAL fog.
        // Keep those fog draws in the material order; batch ordinary opaque fog.
        if material.sort <= SORT_OPAQUE && material.fog_pass == FogPass::Equal {
            opaque_fog.append(&mut material.fog_draws);
        }
        source_to_runtime[material.source_index] = runtime_materials.len();
        let passes = build_passes(
            device,
            queue,
            &mut forge,
            &dynamic_light_buffer,
            material.stages,
        )?;
        runtime_materials.push(Material {
            view_bounded: material.view_bounded,
            flare: material.flare,
            fog_pass: material.fog_pass,
            fog_pipeline: material.fog_pipeline,
            fog_draws: material.fog_draws,
            blended: material.sort > SORT_OPAQUE,
            light_buffered: (material.sort <= SORT_OPAQUE
                && !material.flare
                && passes.first().is_some_and(|stage| stage.shadow_caster))
            .then(|| match forge.pipeline_keys[passes[0].pipeline].cull {
                Some(wgpu::Face::Front) => 0,
                Some(wgpu::Face::Back) => 1,
                None => 2,
            }),
            light_cutout: material.sort <= SORT_OPAQUE
                && !material.flare
                && passes.first().is_some_and(|stage| stage.light_cutout),
            sort: material.sort,
            has_glow: glow::has_glow(passes.iter().map(|stage| stage.glow)),
            stages: passes,
            static_draws: material.static_draws,
            camera_ranges: Default::default(),
            static_draws_by_cluster: Vec::new(),
            mover_draws: material.mover_draws,
        });
    }
    let source_order = source_to_runtime
        .iter()
        .map(|&index| {
            runtime_materials
                .get(index)
                .map_or((SORT_OPAQUE, 0), |material| {
                    (
                        material.sort,
                        material.stages.first().map_or(0, |stage| stage.pipeline),
                    )
                })
        })
        .collect();
    let (opaque_order, decal_order, blended_order, glow_order) =
        draw_orders(&runtime_materials, &forge.pipeline_keys);
    let keys = forge.pipeline_keys.clone();
    let fog_surfaces = opaque_fog.len();
    fog_draws::compact(&mut opaque_fog);
    crate::log::progress(format_args!(
        "fog plan: {fog_surfaces} opaque ranges -> {} draws before PVS",
        opaque_fog.len(),
    ));
    let mut runtime = Runtime {
        view_culling: Default::default(),
        gi: None,
        surfaces_by_source,
        lamps: crate::lamp_lights::LampSet::default(),
        mover_occluders: Vec::new(),
        active: Default::default(),
        indirect: None,
        stage_table: None,

        lamp_cache_pages: None,
        lamp_cache: None,
        shadow_bounds: [glam::Vec3::ZERO; 2],
        shadow_hulls: None,
        caster_runs: std::cell::OnceCell::new(),
        depth_prime: std::cell::OnceCell::new(),
        probe_domain: gi_probe_domain::Domain::default(),
        environment_policy: lighting_environment::Policy::default(),
        ssao: ssao::AmbientOcclusion::default(),
        shadows: None,
        areas: areas::Areas::default(),
        flares: flares::Runtime::default(),
        fog_mode: crate::fog_volumes::Mode::default(),
        fogged_entities: 0,
        fog,
        opaque_fog,
        opaque_fog_by_cluster: Vec::new(),
        sky,
        materials: runtime_materials,
        source_to_runtime,
        source_order,
        entity_pipelines: Vec::with_capacity(keys.len()),
        entity_no_depth_pipelines: Vec::with_capacity(keys.len()),
        opaque_order,
        decal_order,
        blended_order,
        glow_order,
        glow_pipelines: glow::Pipelines::default(),
        card_outline_mask: std::cell::OnceCell::new(),
        pipeline_generation: 0,
        dynamic_light_buffer,
        lighting_mode: std::cell::Cell::new(0),
        forge,
        remaps: Default::default(),
    };
    // One thread per key was measured twice as no faster on RADV (cold cache, 88
    // pipelines: 98 s sequential, 106 s parallel): the driver serialises. Sequential,
    // and only the variants each side of the scene uses; the rest compile on first draw.
    let pipelines_started = Instant::now();
    let mut world_keys = vec![false; keys.len()];
    let mut model_keys = vec![false; keys.len()];
    // Forced-alpha (Force Speed trail) variants of model stages: depth-tested only.
    let mut forced_keys = vec![false; keys.len()];
    for material in &runtime.materials {
        let world = !material.static_draws.is_empty() || !material.mover_draws.is_empty();
        for stage in &material.stages {
            for index in [stage.pipeline, stage.live_pipeline] {
                if world {
                    world_keys[index] = true;
                } else {
                    model_keys[index] = true;
                }
            }
            if !world {
                for index in stage.forced_alpha_pipelines {
                    forced_keys[index] = true;
                }
            }
        }
    }
    for index in 0..keys.len() {
        runtime.push_pipeline_slots();
        if world_keys[index] || model_keys[index] || forced_keys[index] {
            runtime.entity_pipeline(index, true);
        }
        if model_keys[index] {
            runtime.entity_pipeline(index, false);
        }
    }
    runtime.warm_glow(0..runtime.materials.len());
    let both = world_keys
        .iter()
        .zip(&model_keys)
        .filter(|(w, m)| **w && **m)
        .count();
    crate::log::progress(format_args!(
        "compiled {} pipeline keys in {:.0} ms \
        (cumulative per variant, depth-tested, no-depth: {:?} ms; keys used by \
        world materials {}, by model materials {}, by both {}, with the glow \
        variants; the rest on first draw)",
        keys.len(),
        pipelines_started.elapsed().as_secs_f64() * 1e3,
        forge::compile_profile(),
        world_keys.iter().filter(|w| **w).count(),
        model_keys.iter().filter(|m| **m).count(),
        both
    ));
    runtime.flares = flares::Runtime::new(device, &runtime.forge, &runtime.materials);
    runtime.ssao = ssao::AmbientOcclusion::new(
        device,
        &runtime.forge.camera_layout,
        runtime.forge.format,
        &runtime.materials,
        &runtime.forge.pipeline_keys,
    );
    Ok((runtime, resolved))
}

/// Which world surfaces a draw order holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Sort opaque and below.
    Opaque,
    /// [`underlay`] decals, drawn over the opaque world before its fog pass.
    Decals,
    /// Every other blended surface, after the fog pass.
    Blended,
}

/// rd-vanilla's `SS_DECAL`: the sort of a `polygonOffset` shader without one of its own.
const SORT_DECAL: f32 = 4.0;

/// A decal that only colours the surface it lies on: sort `decal`, no stage writing depth.
/// It is drawn before the opaque world's fog pass, whose `Equal` test on the wall's depth
/// then fogs the wall and the decal together, so the decal fades into fog with its wall
/// (`textures/decals/mp_imperial_symbol` on `mp/ctf4` showed through the fog at full
/// strength, Sol's note of 07/10/2026). Its own fog pass is left out: with the decal's
/// depth bias and no depth of its own, its `Equal` test meets nothing, in rd-vanilla too.
fn underlay(material: &Material, keys: &[PipelineKey]) -> bool {
    !material.flare
        && decal_underlay(
            material.blended,
            material.sort,
            material
                .stages
                .iter()
                .map(|stage| keys.get(stage.pipeline).is_none_or(|key| key.depth_write)),
        )
}

/// [`underlay`] on a material's blending, sort and whether each stage writes depth.
fn decal_underlay(blended: bool, sort: f32, depth_writes: impl IntoIterator<Item = bool>) -> bool {
    blended && sort == SORT_DECAL && !depth_writes.into_iter().any(|writes| writes)
}

/// The opaque, decal and blended draw orders and the glow order over them.
fn draw_orders(
    materials: &[Material],
    keys: &[PipelineKey],
) -> (Vec<PassRef>, Vec<PassRef>, Vec<PassRef>, Vec<PassRef>) {
    let opaque = build_draw_order(materials, keys, Phase::Opaque);
    let decals = build_draw_order(materials, keys, Phase::Decals);
    let blended = build_draw_order(materials, keys, Phase::Blended);
    let glow = glow::order(materials, &opaque, &[decals.as_slice(), &blended].concat());
    (opaque, decals, blended, glow)
}

fn build_draw_order(materials: &[Material], keys: &[PipelineKey], phase: Phase) -> Vec<PassRef> {
    let blended = phase != Phase::Opaque;
    let mut order = materials
        .iter()
        .enumerate()
        .filter(|(_, material)| {
            !material.flare
                && material.blended == blended
                && (phase == Phase::Opaque || (phase == Phase::Decals) == underlay(material, keys))
        })
        .flat_map(|(material, entry)| {
            let fog = !entry.fog_draws.is_empty() && phase != Phase::Decals;
            (0..entry.stages.len() + usize::from(fog)).map(move |stage| PassRef { material, stage })
        })
        .collect::<Vec<_>>();
    // A material's stages blend over one another: stage k must be drawn before stage k+1.
    // Sorting by pipeline first drew them in pipeline-index order, which only happens to be
    // stage order for the common replace-then-filter stack. Stage-major order keeps every
    // material's stages in sequence and still batches each level by pipeline; the order
    // between different opaque materials does not matter. Blended materials overlap what
    // is behind them, so each finishes all its stages before the next one starts.
    order.sort_by(|left, right| {
        let pipeline = |pass: &PassRef| {
            materials[pass.material]
                .stages
                .get(pass.stage)
                .map_or(usize::MAX, |stage| stage.pipeline)
        };
        let by_sort = materials[left.material]
            .sort
            .total_cmp(&materials[right.material].sort);
        if blended {
            by_sort
                .then(left.material.cmp(&right.material))
                .then(left.stage.cmp(&right.stage))
        } else {
            by_sort
                .then(left.stage.cmp(&right.stage))
                .then(pipeline(left).cmp(&pipeline(right)))
                .then(left.material.cmp(&right.material))
        }
    });
    order
}

/// Identical-PVS neighbours share one submission, including all their colour stages.
/// This offsets fog submission cost without changing which triangles are visible.
fn coalesce_static_draws(draws: &mut Vec<StaticDraw>) {
    for draw in draws.iter_mut() {
        draw.clusters.sort_unstable();
    }
    draws.dedup_by(|next, previous| {
        if previous.indices.end == next.indices.start && previous.clusters == next.clusters {
            previous.indices.end = next.indices.end;
            previous.bounds = [
                previous.bounds[0].min(next.bounds[0]),
                previous.bounds[1].max(next.bounds[1]),
            ];
            true
        } else {
            false
        }
    });
}

pub(crate) fn visible(
    clusters: &[usize],
    source: Option<usize>,
    visibility: Option<&Visibility>,
) -> bool {
    let (Some(source), Some(visibility)) = (source, visibility) else {
        return true;
    };
    clusters.is_empty()
        || clusters
            .iter()
            .any(|&cluster| visibility.is_cluster_visible(source, cluster))
}

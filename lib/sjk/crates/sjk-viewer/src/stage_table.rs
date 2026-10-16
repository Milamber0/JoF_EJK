//! One bind group for every static world material stage.
//!
//! A Quake 3 material is a list of stages, each with its own textures and parameters, and
//! each stage used to be one bind group: a frame's colour passes switched groups thousands
//! of times, the most expensive thing they did on the CPU (`wgpu_cost_model_tests.rs`).
//! The table puts every stage's parameters into one storage buffer and every stage image
//! into binding arrays. A draw names its stage through the instance index, so all visible
//! surfaces that share a pipeline go out as one `multi_draw_indexed_indirect`.
//!
//! It serves opaque stages without vertex deforms or surface sprites, of the static world
//! and of entities (models loaded mid-match join it). Movers, blended, depth-less and
//! deforming draws, and any device without binding arrays, keep the per-stage bind groups.
//!
//! Static world surfaces name their stage by instance index, entity draws by an
//! immediate. Bind-group switches in the colour passes fall from about 1,770 to 190 per
//! frame. The table costs the GPU 0.06-0.08 ms at render scale 2 (dynamically indexed
//! binding arrays); `SJK_STAGE_TABLE=0` turns it off for same-binary comparisons.
use super::*;
use crate::world_stage::GpuStage;

/// Device features the table needs; requested when the adapter offers all of them.
pub(crate) const FEATURES: wgpu::Features = wgpu::Features::TEXTURE_BINDING_ARRAY
    .union(wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING)
    .union(wgpu::Features::INDIRECT_FIRST_INSTANCE)
    // Player models load mid-match: the arrays grow inside a fixed-size layout, and an
    // entity draw names its stage record with an immediate.
    .union(wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY)
    .union(wgpu::Features::IMMEDIATES);

/// Array sizes of the fixed layout; a map or session beyond them keeps per-stage groups
/// for the stages that no longer fit.
const MAX_IMAGES: usize = 4096;
const MAX_LIGHTMAPS: usize = 512;

/// What a stage contributes to the table, kept from material build until the map's
/// table is assembled.
#[derive(Clone)]
pub(super) struct Source {
    pub(super) gpu: GpuStage,
    pub(super) primary: wgpu::TextureView,
    pub(super) secondary: wgpu::TextureView,
    pub(super) lightmap: wgpu::TextureView,
    /// Clamp (instead of repeat) for the primary and the secondary image.
    pub(super) clamp: [bool; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Record {
    stage: GpuStage,
    /// Primary image, secondary image, lightmap, sampler bits (1 primary, 2 secondary clamp).
    images: [u32; 4],
}

pub(super) struct Table {
    /// Present from the first stage on; `build` returns no table without one.
    group: Option<wgpu::BindGroup>,
    layout: wgpu::BindGroupLayout,
    /// Per runtime material, per stage: the stage's record, or `None` off the table.
    records: Vec<Vec<Option<u32>>>,
    table: Vec<Record>,
    images: Vec<wgpu::TextureView>,
    lightmaps: Vec<wgpu::TextureView>,
    /// Table pipelines per pipeline key (static world, then entity), compiled against the
    /// current program at map load for the map's own stages ([`Table::prewarm`]) and on
    /// first use for any other.
    pipelines: [Vec<std::cell::OnceCell<wgpu::RenderPipeline>>; 2],
    program: std::cell::OnceCell<(wgpu::PipelineLayout, wgpu::ShaderModule)>,
}

/// Stage fields every fragment reads.
const HOT_FIELDS: [&str; 4] = ["generators", "animation", "secondary_control", "emission"];

/// The stage program with its per-stage group replaced by the table: `stage.field` reads
/// the draw's record in place, so every other line of the program is unchanged. The
/// patterns span line breaks, so a CRLF checkout's source is normalised first.
pub(super) fn program_source(source: &str) -> String {
    let mut source = crate::wgsl_source::lf(source).into_owned();
    for (from, to) in [
        (
            "@group(1) @binding(0) var stage_images: texture_2d_array<f32>;",
            "struct StageRecord { stage: Stage, images: vec4<u32> };\n\
             @group(1) @binding(0) var<storage, read> stage_records: array<StageRecord>;",
        ),
        (
            "@group(1) @binding(1) var stage_sampler: sampler;",
            "@group(1) @binding(1) var table_images: binding_array<texture_2d_array<f32>>;",
        ),
        (
            "@group(1) @binding(2) var secondary_images: texture_2d_array<f32>;",
            "@group(1) @binding(2) var table_lightmaps: binding_array<texture_2d<f32>>;",
        ),
        (
            "@group(1) @binding(3) var secondary_sampler: sampler;",
            "@group(1) @binding(3) var table_samplers: binding_array<sampler>;",
        ),
        (
            "@group(1) @binding(4) var lightmap_image: texture_2d<f32>;",
            "var<private> table_index: vec4<u32>;",
        ),
        (
            "@group(1) @binding(5) var<uniform> stage: Stage;",
            "var<private> table_record: u32;",
        ),
        // A group with binding arrays may not hold a uniform buffer, and reading the block
        // as storage cost 0.1 ms of GPU time: it lives in the receiver group instead.
        (
            "@group(1) @binding(6) var<uniform> point_lights: PointLightBlock;",
            "@group(3) @binding(40) var<uniform> point_lights: PointLightBlock;",
        ),
        (
            "textureSample(stage_images, stage_sampler, uv, frame)",
            "textureSample(table_images[table_index.x], table_samplers[table_index.w & 1u], uv, frame)",
        ),
        (
            "textureSample(lightmap_image, stage_sampler, uv)",
            "textureSample(table_lightmaps[table_index.z], table_samplers[table_index.w & 1u], uv)",
        ),
        (
            "textureSample(secondary_images, secondary_sampler,",
            "textureSample(table_images[table_index.y], table_samplers[(table_index.w >> 1u) & 1u],",
        ),
        (
            "textureSample(lightmap_image, secondary_sampler, input.secondary_uv)",
            "textureSample(table_lightmaps[table_index.z], table_samplers[(table_index.w >> 1u) & 1u], input.secondary_uv)",
        ),
    ] {
        assert_eq!(
            source.matches(from).count(),
            1,
            "stage program changed: {from}"
        );
        source = source.replace(from, to);
    }
    // Every `stage.field` reads that field of the draw's record in place: copying the
    // whole 624-byte stage per fragment cost 0.1 ms of GPU time at render scale 2.
    let mut fields = String::with_capacity(source.len() + 4096);
    let mut rest = source.as_str();
    while let Some(at) = rest.find("stage.") {
        let word_start = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !(before.is_ascii_alphanumeric() || before == '_'));
        fields.push_str(&rest[..at]);
        fields.push_str(if word_start {
            "stage_records[table_record].stage."
        } else {
            "stage."
        });
        rest = &rest[at + "stage.".len()..];
    }
    fields.push_str(rest);
    let mut source = fields;
    // Fragments read their hot stage fields from flat varyings the vertex stage filled:
    // a storage read with a per-draw index costs every fragment what a per-stage uniform
    // gave for free. Rarely used fields (waves, constant colours) stay storage reads.
    for field in HOT_FIELDS {
        source = source.replace(
            &format!("stage_records[table_record].stage.{field}"),
            &format!("table_{field}"),
        );
    }
    // Four locations remain: generators and the two fragment flags of `secondary_control`
    // share one, which leaves one for the image indices.
    let varyings = "    @location(12) @interpolate(flat) table_packed: vec4<f32>,\n    \
        @location(13) @interpolate(flat) table_animation_out: vec4<f32>,\n    \
        @location(14) @interpolate(flat) table_emission_out: vec4<f32>,\n    \
        @location(15) @interpolate(flat) table_images: vec4<u32>,\n";
    let privates: String = HOT_FIELDS
        .iter()
        .map(|field| format!("var<private> table_{field}: vec4<f32>;\n"))
        .collect();
    // Pack the record and signed animation override into one location so the
    // table retains its sixteen-location interface on existing devices.
    let tail = "    @location(11) @interpolate(flat) animation_index: i32,\n};";
    assert_eq!(
        source.matches(tail).count(),
        1,
        "stage program changed: VertexOutput"
    );
    source = source.replace(
        tail,
        &format!(
            "    @location(11) @interpolate(flat) \
        record_animation: vec2<i32>,\n{varyings}}};\n{privates}"
        ),
    );
    source = source
        .replace("output.animation_index", "output.record_animation.y")
        .replace("input.animation_index", "input.record_animation.y");

    source.push_str(&crate::wgsl_source::lf(include_str!("stage_table.wgsl")));
    // Enable directives precede every declaration.
    format!("enable wgpu_binding_array;\n{source}")
}

impl Table {
    /// Repack changed material records while retaining compiled pipeline variants.
    pub(super) fn rebuild(&mut self, device: &wgpu::Device, forge: &Forge, materials: &[Material]) {
        self.records.clear();
        self.table.clear();
        self.images.clear();
        self.lightmaps.clear();
        self.group = None;
        self.append(device, forge, materials);
    }

    /// `None`: the device lacks binding arrays, the table is switched off, or the map has
    /// no stage it could serve.
    pub(super) fn build(
        device: &wgpu::Device,
        forge: &Forge,
        materials: &[Material],
    ) -> Option<Self> {
        if !device.features().contains(FEATURES)
            || std::env::var_os("SJK_STAGE_TABLE").is_some_and(|value| value == "0")
        {
            return None;
        }
        let limit = device.limits().max_binding_array_elements_per_shader_stage as usize;
        if limit < MAX_IMAGES + MAX_LIGHTMAPS {
            return None;
        }
        let count = |n: usize| std::num::NonZeroU32::new(n as u32);
        let buffer_entry = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture_entry = |binding, view_dimension, n| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension,
                multisampled: false,
            },
            count: count(n),
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK stage table"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                texture_entry(1, wgpu::TextureViewDimension::D2Array, MAX_IMAGES),
                texture_entry(2, wgpu::TextureViewDimension::D2, MAX_LIGHTMAPS),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: count(2),
                },
                buffer_entry(7, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let mut table = Self {
            group: None,
            layout,
            records: Vec::new(),
            table: Vec::new(),
            images: Vec::new(),
            lightmaps: Vec::new(),
            pipelines: Default::default(),
            program: Default::default(),
        };
        table.append(device, forge, materials);
        crate::log::progress(format_args!(
            "Stage table: {} stages, {} images, {} lightmaps in one bind group",
            table.table.len(),
            table.images.len(),
            table.lightmaps.len()
        ));
        table.group.is_some().then_some(table)
    }

    /// Take the stages of `materials[self.records.len()..]` (a map's materials, later a
    /// loaded model's) and rebuild the group. Opaque stages without vertex deforms or
    /// surface sprites qualify: static world surfaces draw them by instance index,
    /// entities by immediate.
    pub(super) fn append(&mut self, device: &wgpu::Device, forge: &Forge, materials: &[Material]) {
        let slot = |list: &mut Vec<wgpu::TextureView>, view: &wgpu::TextureView, limit: usize| {
            list.iter()
                .position(|known| known == view)
                .or_else(|| {
                    (list.len() < limit).then(|| {
                        list.push(view.clone());
                        list.len() - 1
                    })
                })
                .map(|index| index as u32)
        };
        let before = self.table.len();
        for material in &materials[self.records.len()..] {
            let records = material
                .stages
                .iter()
                .map(|stage| {
                    // Material-mapped stages bind their own group (`material_maps`).
                    let source = stage.table.as_ref().filter(|_| {
                        !material.blended
                            && forge.pipeline_keys[stage.pipeline].geometry
                                & (3 | super::material_maps::PIPELINE_BIT)
                                == 0
                    })?;
                    let images = [
                        slot(&mut self.images, &source.primary, MAX_IMAGES)?,
                        slot(&mut self.images, &source.secondary, MAX_IMAGES)?,
                        slot(&mut self.lightmaps, &source.lightmap, MAX_LIGHTMAPS)?,
                        u32::from(source.clamp[0]) | u32::from(source.clamp[1]) << 1,
                    ];
                    self.table.push(Record {
                        stage: source.gpu,
                        images,
                    });
                    Some(self.table.len() as u32 - 1)
                })
                .collect();
            self.records.push(records);
        }
        for list in &mut self.pipelines {
            list.resize_with(forge.pipeline_keys.len(), Default::default);
        }
        if self.table.len() == before {
            return;
        }
        self.group = Some(Self::bind(
            device,
            forge,
            &self.layout,
            &self.table,
            &self.images,
            &self.lightmaps,
        ));
    }

    fn bind(
        device: &wgpu::Device,
        forge: &Forge,
        layout: &wgpu::BindGroupLayout,
        table: &[Record],
        images: &[wgpu::TextureView],
        lightmaps: &[wgpu::TextureView],
    ) -> wgpu::BindGroup {
        use wgpu::util::DeviceExt;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK stage table"),
            contents: bytemuck::cast_slice(table),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let image_views: Vec<&wgpu::TextureView> = images.iter().collect();
        let lightmap_views: Vec<&wgpu::TextureView> = lightmaps.iter().collect();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK stage table"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureViewArray(&image_views),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureViewArray(&lightmap_views),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::SamplerArray(&[&forge.repeat, &forge.clamp]),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: forge.model_grid.as_entire_binding(),
                },
            ],
        })
    }

    pub(super) fn group(&self) -> &wgpu::BindGroup {
        self.group
            .as_ref()
            .expect("a built stage table has a group")
    }

    /// The record of `stage` of runtime material `material`, when the table serves it.
    pub(super) fn record(&self, material: usize, stage: usize) -> Option<u32> {
        *self.records.get(material)?.get(stage)?
    }

    /// Forget compiled pipelines: the stage program was replaced (real-time lighting).
    pub(super) fn reset_program(&mut self) {
        self.program = Default::default();
        for list in &mut self.pipelines {
            for pipeline in list.iter_mut() {
                *pipeline = Default::default();
            }
        }
    }

    /// The table pipeline of key `index`, compiled on first use.
    pub(super) fn pipeline(
        &self,
        forge: &Forge,
        index: usize,
        entity: bool,
    ) -> Option<&wgpu::RenderPipeline> {
        // The point-light block sits in the receiver group: the real-time program only.
        forge.model_sun.as_ref()?;
        let slot = self.pipelines[usize::from(entity)].get(index)?;
        Some(slot.get_or_init(|| {
            let (layout, shader) = self.program.get_or_init(|| {
                let device = &forge.device;
                let quads = crate::shared_geometry::quads::layout(device);
                let sun = forge
                    .model_sun
                    .as_ref()
                    .map(|_| super::model_sun::receiver_layout(device));
                let mut groups = vec![Some(&forge.camera_layout), Some(&self.layout), Some(&quads)];
                if let Some(sun) = &sun {
                    groups.push(Some(sun));
                }
                let source = if forge.model_sun.is_some() {
                    world_sun_shader()
                } else {
                    STAGE_SHADER
                };
                (
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("SJK stage table program"),
                        bind_group_layouts: &groups,
                        immediate_size: 4,
                    }),
                    device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("SJK stage table program"),
                        source: wgpu::ShaderSource::Wgsl(program_source(source).into()),
                    }),
                )
            });
            super::gpu::create_pipeline_for_vertex(
                &forge.device,
                layout,
                shader,
                forge.format,
                forge.pipeline_keys[index],
                entity,
                if entity {
                    "table_entity_vertex_main"
                } else {
                    "table_vertex_main"
                },
                if entity {
                    "table_entity_fragment_main"
                } else {
                    "table_fragment_main"
                },
                "SJK stage table pipeline",
                true,
            )
        }))
    }

    /// Compile now, at map installation, the table pipelines this map's materials draw
    /// with ([`warm_keys`]), so the first frames after the map goes live do not compile
    /// them one by one on the render thread. Nothing without the real-time program (the
    /// table then draws nothing). Returns how many pipelines exist afterwards.
    pub(super) fn prewarm(&self, forge: &Forge, materials: &[Material]) -> usize {
        if forge.model_sun.is_none() {
            return 0;
        }
        let stages = materials.iter().enumerate().flat_map(|(index, material)| {
            let drawn = Drawn::of(
                material.flare,
                !material.static_draws.is_empty(),
                !material.mover_draws.is_empty(),
            );
            material
                .stages
                .iter()
                .enumerate()
                .map(move |(stage_index, stage)| {
                    (
                        drawn,
                        self.record(index, stage_index).is_some(),
                        [stage.pipeline, stage.live_pipeline],
                    )
                })
        });
        let warm = warm_keys(forge.pipeline_keys.len(), stages);
        let mut compiled = 0;
        for (list, entity) in warm.iter().zip([false, true]) {
            for (index, _) in list.iter().enumerate().filter(|(_, wanted)| **wanted) {
                compiled += usize::from(self.pipeline(forge, index, entity).is_some());
            }
        }
        compiled
    }
}

/// How a material's tabled stages are drawn (`draw_order` and `draw_entity_stages` in
/// `world_material_draw.rs`): static world surfaces by instance index, materials without
/// world draws (models) as entities. Mover draws and flares never use the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Drawn {
    Statics,
    Entities,
    Neither,
}

impl Drawn {
    /// The same split as the load-time entity pipeline warm-up (`world_materials.rs`):
    /// a material with static or mover draws is the world's, any other a model's.
    pub(super) fn of(flare: bool, statics: bool, movers: bool) -> Self {
        match (flare, statics, movers) {
            (true, _, _) => Self::Neither,
            (false, true, _) => Self::Statics,
            (false, false, false) => Self::Entities,
            (false, false, true) => Self::Neither,
        }
    }
}

/// Which table pipelines a map draws with, `[static world, entity]` per pipeline key:
/// both lighting variants (scene and live emission) of every stage the table serves, as
/// the main view and the secondary views pick either. `stages` yields each stage's
/// [`Drawn`], whether the table holds it, and its two keys.
pub(super) fn warm_keys(
    key_count: usize,
    stages: impl IntoIterator<Item = (Drawn, bool, [usize; 2])>,
) -> [Vec<bool>; 2] {
    let mut warm = [vec![false; key_count], vec![false; key_count]];
    for (drawn, tabled, keys) in stages {
        let list = match drawn {
            _ if !tabled => continue,
            Drawn::Statics => &mut warm[0],
            Drawn::Entities => &mut warm[1],
            Drawn::Neither => continue,
        };
        for key in keys {
            if let Some(slot) = list.get_mut(key) {
                *slot = true;
            }
        }
    }
    warm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgsl_source::{crlf, lf};

    #[test]
    fn statics_and_models_warm_their_own_variant_of_both_keys() {
        let warm = warm_keys(
            6,
            [
                (Drawn::Statics, true, [0, 1]),
                (Drawn::Entities, true, [2, 2]),
                // Off the table (blended, deformed, material-mapped): entity pipelines.
                (Drawn::Statics, false, [3, 3]),
                (Drawn::Entities, false, [4, 4]),
                // Movers and flares never draw through the table.
                (Drawn::Neither, true, [5, 5]),
            ],
        );
        assert_eq!(warm[0], [true, true, false, false, false, false]);
        assert_eq!(warm[1], [false, false, true, false, false, false]);
    }

    #[test]
    fn shared_keys_compile_once_and_unknown_keys_are_ignored() {
        let warm = warm_keys(
            2,
            [
                (Drawn::Statics, true, [1, 1]),
                (Drawn::Statics, true, [1, 7]),
            ],
        );
        assert_eq!(warm[0], [false, true]);
        assert_eq!(warm[1], [false, false]);
        let none = warm_keys(0, [(Drawn::Entities, true, [0, 0])]);
        assert!(none.iter().all(Vec::is_empty));
    }

    #[test]
    fn materials_split_like_the_entity_warm_up() {
        assert_eq!(Drawn::of(false, true, false), Drawn::Statics);
        assert_eq!(Drawn::of(false, true, true), Drawn::Statics);
        assert_eq!(Drawn::of(false, false, false), Drawn::Entities);
        assert_eq!(Drawn::of(false, false, true), Drawn::Neither);
        assert_eq!(Drawn::of(true, true, false), Drawn::Neither);
        assert_eq!(Drawn::of(true, false, false), Drawn::Neither);
    }

    /// The embedded programs patch whatever line endings the checkout gave them, and a
    /// CRLF copy yields the same program as an LF one (#67).
    #[test]
    fn program_source_patches_lf_and_crlf_programs() {
        for source in [STAGE_SHADER, world_sun_shader()] {
            let embedded = program_source(source);
            let unix = program_source(&lf(source));
            let windows = program_source(&crlf(source));
            assert_eq!(embedded, unix);
            assert_eq!(windows, unix);
            assert!(!unix.contains('\r'));
            assert!(unix.contains("record_animation: vec2<i32>,"));
        }
    }
}

//! The part of the material runtime that outlives map load: everything
//! needed to compile one more Q3 material later — the stage bind-group
//! layout, the pipeline layout and shader, the samplers, the fallback
//! lightmap, the texture cache and the pipeline key table. World materials
//! and detached materials (the menu's player stage) go through the same
//! two steps: [`compile_material`] resolves stages and decodes images,
//! [`build_passes`] uploads textures and builds one bind group per stage.

use super::*;

#[path = "world_material_compile.rs"]
mod compile;
pub(super) use compile::compile_material;

/// Map-lifetime GPU state shared by every compiled material.
pub(super) struct Forge {
    /// Optional main-view sun program; authored and secondary views retain old lighting.
    pub(super) model_sun: Option<super::model_sun::Runtime>,
    /// Camera layout reused by optional main-view ambient correction pipelines.
    pub(super) camera_layout: wgpu::BindGroupLayout,
    /// Shared immutable spatial light grid for every model material in this map.
    pub(super) model_grid: wgpu::Buffer,
    pub(super) filtering: super::filtering::Policy,
    pub(super) geometry: wgpu::BindGroup,
    pub(super) flare_layout: wgpu::PipelineLayout,
    pub(super) empty_geometry_stage: wgpu::BindGroup,
    pub(super) stage_layout: wgpu::BindGroupLayout,
    pub(super) pipeline_layout: wgpu::PipelineLayout,
    pub(super) shader: wgpu::ShaderModule,
    /// The device, for pipelines compiled on first draw.
    pub(super) device: wgpu::Device,
    /// One identity actor instance: static world draws bind it so the instanced vertex
    /// path serves them unchanged.
    pub(super) identity_instance: wgpu::Buffer,
    pub(super) format: wgpu::TextureFormat,
    pub(super) repeat: wgpu::Sampler,
    pub(super) clamp: wgpu::Sampler,
    pub(super) fallback_lightmap: wgpu::TextureView,
    /// One pipeline per key in each of the runtime's four pipeline lists.
    /// The frame's point-light block, shared by every stage group and the receiver group.
    pub(super) point_lights: wgpu::Buffer,
    /// The queue, for per-view uploads made while a pass is being recorded.
    pub(super) queue: crate::frame_queue::FrameQueue,
    pub(super) pipeline_keys: Vec<PipelineKey>,
    pub(super) texture_cache: std::collections::HashMap<String, wgpu::TextureView>,
    /// The `videoMap` stages' textures and their players.
    pub(super) videos: super::videos::Videos,
    /// Material-map layout, textures, frames and program; only for maps that found any.
    pub(super) material_maps: Option<super::material_maps::gpu::Gpu>,
}

impl Forge {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let stage_layout = create_stage_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK Q3 world stage runtime"),
            source: wgpu::ShaderSource::Wgsl(STAGE_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK Q3 world stage pipeline layout"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
            ],
            immediate_size: 0,
        });
        let fallback_lightmap = create_rgba8_texture(
            device,
            queue,
            "SJK unlit world lightmap",
            1,
            1,
            &[128, 128, 128, 255],
            true,
        );
        let flare_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK depth-tested flare stages"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&super::flares::depth_layout(device)),
            ],
            immediate_size: 0,
        });
        let identity_instance = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("SJK identity world instance"),
                contents: bytemuck::bytes_of(&crate::ActorInstance::world_identity()),
                usage: wgpu::BufferUsages::VERTEX,
            },
        );
        Self {
            model_sun: None,
            camera_layout: camera_layout.clone(),
            model_grid: super::model_grid::empty(device),
            filtering: super::filtering::Policy::default(),
            geometry: crate::shared_geometry::quads::empty_binding(device),
            flare_layout,
            empty_geometry_stage: crate::world_stage::geometry_uniform::empty(device),
            stage_layout,
            pipeline_layout,
            identity_instance,
            device: device.clone(),
            queue: queue.clone(),
            // The CPU's block, then the shadow tiles only the GPU writes
            // (`dynamic_light_shadows.rs`); created zeroed, so with no lights and no tiles.
            point_lights: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("SJK fixed world point lights"),
                size: crate::dynamic_lights::BLOCK_BYTES,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            shader,
            format,
            repeat: create_sampler(device, false),
            clamp: create_sampler(device, true),
            fallback_lightmap,
            pipeline_keys: Vec::new(),
            texture_cache: std::collections::HashMap::with_capacity(512),
            videos: super::videos::Videos::default(),
            material_maps: None,
        }
    }

    /// The program a pipeline of `key` compiles against: the material program for a
    /// material-mapped stage, the current stage program otherwise.
    pub(super) fn program_for(
        &self,
        key: PipelineKey,
    ) -> (&wgpu::PipelineLayout, &wgpu::ShaderModule) {
        match &self.material_maps {
            Some(maps) if key.geometry & super::material_maps::PIPELINE_BIT != 0 => {
                maps.program(self)
            }
            _ => self.program(),
        }
    }

    /// Whether the real-time light pass also writes the light's direction (and the lamp
    /// cache bakes it): some stage of the map has normal or specular maps to move it to.
    pub(super) fn directed_light(&self) -> bool {
        self.material_maps
            .as_ref()
            .is_some_and(|maps| maps.directed)
    }

    /// Index of `key` in the pipeline lists, registering it when new.
    pub(super) fn pipeline_index(&mut self, key: PipelineKey) -> (usize, bool) {
        match self.pipeline_keys.iter().position(|known| *known == key) {
            Some(index) => (index, false),
            None => {
                self.pipeline_keys.push(key);
                (self.pipeline_keys.len() - 1, true)
            }
        }
    }

    pub(super) fn texture(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        key: &str,
        pixels: &[Arc<RgbaImage>],
    ) -> Result<wgpu::TextureView, Box<dyn Error>> {
        if let Some(texture) = self.texture_cache.get(key) {
            return Ok(texture.clone());
        }
        // A video's frames change: one updatable layer without mips.
        if key.starts_with(super::videos::KEY_PREFIX)
            && let Some(first) = pixels.first()
        {
            let (texture, view) = super::videos::upload(device, queue, first);
            self.videos.register(key, texture);
            self.texture_cache.insert(key.to_owned(), view.clone());
            return Ok(view);
        }
        let texture = if self.filtering.mipmapped() {
            super::filtering::mips::upload(device, queue, pixels)?
        } else {
            upload_array(device, queue, pixels)?
        };
        self.texture_cache.insert(key.to_owned(), texture.clone());
        Ok(texture)
    }
}

/// Stages of one material resolved and decoded but not yet uploaded, plus
/// the material's sort value and the number of resolved image lookups.
pub(super) struct CompiledMaterial {
    pub(super) sort: f32,
    /// Light supplied by textured emissive stages, independent of the diffuse paint.
    pub(super) emission: [f32; 3],
    pub(super) emission_texture: super::emission::Texture,
    /// The emission comes from an emission map (`material_maps::lights`).
    pub(super) mapped_emission: bool,
    pub(super) stages: Vec<PendingStage>,
    pub(super) resolved: usize,
}

/// Upload a compiled material's textures and stage table and build one
/// bind group per stage. Pipeline indices refer to the forge's key table;
/// the caller creates pipelines for any key this registered as new.
pub(super) fn build_passes(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    forge: &mut Forge,
    dynamic_lights: &wgpu::Buffer,
    stages: Vec<PendingStage>,
) -> Result<Vec<StagePass>, Box<dyn Error>> {
    let entry_size = std::mem::size_of::<GpuStage>();
    let alignment = usize::try_from(device.limits().min_uniform_buffer_offset_alignment)?;
    let stride = entry_size.div_ceil(alignment) * alignment;
    let mut table_bytes = vec![0_u8; stride * MAX_SHADER_STAGES];
    for (index, stage) in stages.iter().enumerate() {
        let start = index * stride;
        table_bytes[start..start + entry_size].copy_from_slice(bytemuck::bytes_of(&stage.gpu));
    }
    let stage_table = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK immutable Q3 material stage table"),
        contents: &table_bytes,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut passes = Vec::with_capacity(stages.len());
    for (stage_index, stage) in stages.into_iter().enumerate() {
        let primary = forge.texture(device, queue, &stage.primary_key, &stage.primary_pixels)?;
        let secondary = match (&stage.secondary_pixels, &stage.secondary_key) {
            (Some(pixels), Some(key)) => forge.texture(device, queue, key, pixels)?,
            _ => primary.clone(),
        };
        let sampler = |clamp: bool| if clamp { &forge.clamp } else { &forge.repeat };
        let entries = [
            wgpu::BindGroupEntry {
                binding: 7,
                resource: forge.model_grid.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&primary),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler(stage.primary_clamp)),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&secondary),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler(stage.secondary_clamp)),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&stage.lightmap),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &stage_table,
                    offset: u64::try_from(stage_index * stride)?,
                    size: NonZeroU64::new(u64::try_from(entry_size)?),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: dynamic_lights.as_entire_binding(),
            },
        ];
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK Q3 world stage bind group"),
            layout: &forge.stage_layout,
            entries: &entries,
        });
        let (material_group, floor_maps) = match (&stage.maps, &mut forge.material_maps) {
            (Some(maps), Some(gpu)) => {
                let sampler = if maps.clamp {
                    &forge.clamp
                } else {
                    &forge.repeat
                };
                let group = gpu.bind(device, queue, &entries, maps, sampler)?;
                (Some(group), Some(gpu.floor_maps(device, queue, maps)))
            }
            _ => (None, None),
        };
        let (pipeline, _) = forge.pipeline_index(stage.key);
        let live_key = super::visible_emission::live_key(stage.key, &stage.gpu);
        let (live_pipeline, _) = forge.pipeline_index(live_key);
        let alpha_tested = stage.gpu.generators[2] != 0.0;
        let forced_alpha_pipelines = [stage.key, live_key].map(|key| {
            forge
                .pipeline_index(super::forced_alpha::key(key, alpha_tested))
                .0
        });
        let emission_glow = stage.maps.as_ref().is_some_and(|maps| maps.emission_glow());
        passes.push(StagePass {
            glow: stage.glow || emission_glow,
            emission_glow,
            table: Some(super::stage_table::Source {
                gpu: stage.gpu,
                primary: primary.clone(),
                secondary: secondary.clone(),
                lightmap: stage.lightmap.clone(),
                clamp: [stage.primary_clamp, stage.secondary_clamp],
            }),
            ao_receiver: stage.allow_ssao && super::ssao::eligible(stage.key, &stage.gpu),
            shadow_caster: super::material_maps::without_maps(stage.key.geometry) == 0
                && stage.gpu.generators[2] == 0.0
                && stage.key.depth_write
                && stage.key.source == wgpu::BlendFactor::One
                && stage.key.destination == wgpu::BlendFactor::Zero,
            // Cut-out paint, or surface sprites (grass): alpha-tested, depth-writing.
            light_cutout: stage.gpu.generators[2] != 0.0
                && stage.key.depth_write
                && (stage.key.geometry & 2 != 0
                    || (stage.key.geometry & 3 == 0
                        && stage.key.source == wgpu::BlendFactor::One
                        && stage.key.destination == wgpu::BlendFactor::Zero)),
            geometry_group: crate::world_stage::geometry_uniform::bind(
                device,
                &stage_table,
                (stage_index * stride) as u64,
            ),
            bind_group,
            material_group,
            floor_maps,
            pipeline,
            live_pipeline,
            forced_alpha_pipelines,
        });
    }
    Ok(passes)
}

/// Materials compiled after map load for geometry that is not part of the
/// world's draw lists (the menu's player stage). Indexed by the position in
/// the slice handed to [`Runtime::compile_detached`].
pub(crate) struct DetachedMaterials {
    materials: Vec<DetachedMaterial>,
}

struct DetachedMaterial {
    blended: bool,
    stages: Vec<StagePass>,
}

/// One index range of a detached mesh and the detached material it uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetachedDraw {
    pub(crate) indices: Range<u32>,
    pub(crate) material: usize,
}

impl Runtime {
    /// Compile `materials` against this runtime's shared stage state. New
    /// pipeline variants are appended to the runtime's pipeline lists, so
    /// existing world materials keep their indices.
    pub(crate) fn compile_detached(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        materials: &[ViewerMaterial],
    ) -> Result<DetachedMaterials, Box<dyn Error>> {
        let mut image_cache = ImageCache::with_capacity(64);
        let mut compiled = Vec::with_capacity(materials.len());
        let known = self.forge.pipeline_keys.len();
        for key in materials {
            let lightmap = self.forge.fallback_lightmap.clone();
            let material = compile_material(
                vfs,
                shaders,
                key,
                &lightmap,
                true,
                Default::default(),
                &mut image_cache,
                false,
            )?;
            let stages = build_passes(
                device,
                queue,
                &mut self.forge,
                &self.dynamic_light_buffer,
                material.stages,
            )?;
            compiled.push(DetachedMaterial {
                blended: material.sort > SORT_OPAQUE,
                stages,
            });
        }
        for key in self.forge.pipeline_keys[known..].to_vec() {
            self.push_pipelines(device, key);
        }
        Ok(DetachedMaterials {
            materials: compiled,
        })
    }

    /// The lightmap entity materials compile against, for [`warm_entity_materials`].
    pub(crate) fn entity_lightmap(&self) -> wgpu::TextureView {
        self.forge.fallback_lightmap.clone()
    }

    /// Compile `materials` as entity materials of this runtime (no world
    /// surfaces of their own) and return the source index of the first one:
    /// draws that use them refer to `base + position in materials`. Existing
    /// materials keep their indices. Their new pipelines compile now.
    pub(crate) fn append_entity_materials(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        materials: &[ViewerMaterial],
    ) -> Result<usize, Box<dyn Error>> {
        let (base, jobs) =
            self.append_entity_materials_deferred(device, queue, vfs, shaders, materials)?;
        self.install_pipelines(jobs.compile());
        Ok(base)
    }

    /// [`Self::append_entity_materials`] without compiling their new pipelines: the
    /// returned jobs do, on a worker ([`pipeline_jobs`]), before anything draws with them.
    pub(crate) fn append_entity_materials_deferred(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        materials: &[ViewerMaterial],
    ) -> Result<(usize, pipeline_jobs::Jobs), Box<dyn Error>> {
        let base = self.source_to_runtime.len();
        let mut image_cache = ImageCache::with_capacity(64);
        let known = self.forge.pipeline_keys.len();
        for key in materials {
            let lightmap = self.forge.fallback_lightmap.clone();
            let material = compile_material(
                vfs,
                shaders,
                key,
                &lightmap,
                true,
                Default::default(),
                &mut image_cache,
                false,
            )?;
            let stages = build_passes(
                device,
                queue,
                &mut self.forge,
                &self.dynamic_light_buffer,
                material.stages,
            )?;
            self.remaps.applied = None;
            self.remaps.sources.push(remaps::Source {
                key: key.clone(),
                name: sjk_client::shader_name(&key.shader),
                lightmap: lightmap.clone(),
                fog: Vec::new(),
                applied: None,
            });
            self.source_to_runtime.push(self.materials.len());
            self.source_order.push((
                material.sort,
                stages.first().map_or(0, |stage| stage.pipeline),
            ));
            self.materials.push(Material {
                view_bounded: false,
                flare: false,
                // Late (cgame custom) materials have no world draws to buffer.
                light_buffered: None,
                light_cutout: false,
                fog_pass: shaders
                    .get(&key.shader)
                    .map_or(FogPass::Equal, |d| d.fog_pass()),
                fog_pipeline: self.fog.slot(
                    self.forge.format,
                    shaders
                        .get(&key.shader)
                        .map_or(FogPass::Equal, |d| d.fog_pass()),
                    shaders
                        .get(&key.shader)
                        .map_or(ShaderCull::Front, |d| d.cull),
                    u8::from(
                        shaders
                            .get(&key.shader)
                            .is_some_and(|d| !d.deforms.is_empty()),
                    ) | super::fog_gpu::polygon_offset(shaders.get(&key.shader)),
                ),
                fog_draws: Vec::new(),
                blended: material.sort > SORT_OPAQUE,
                sort: material.sort,
                has_glow: super::glow::has_glow(stages.iter().map(|stage| stage.glow)),
                stages,
                static_draws: Vec::new(),
                camera_ranges: Default::default(),
                static_draws_by_cluster: Vec::new(),
                mover_draws: Vec::new(),
            });
        }
        for _ in known..self.forge.pipeline_keys.len() {
            self.push_pipeline_slots();
        }
        // A loaded model's stages join the map's stage table.
        if let Some(table) = &mut self.stage_table {
            table.append(device, &self.forge, &self.materials);
        }
        // With the model, not on its first (glowing, fogged) frame.
        let first_material = self.materials.len() - materials.len();
        let jobs = self.pipeline_jobs(
            known..self.forge.pipeline_keys.len(),
            first_material..self.materials.len(),
        );
        Ok((base, jobs))
    }

    /// Append one empty slot per list for the next key.
    pub(super) fn push_pipeline_slots(&mut self) {
        self.entity_pipelines.push(std::cell::OnceCell::new());
        self.entity_no_depth_pipelines
            .push(std::cell::OnceCell::new());
        self.glow_pipelines.push();
    }

    /// Create every variant of `key` now (late cgame materials are few).
    pub(super) fn push_pipelines(&mut self, _device: &wgpu::Device, _key: PipelineKey) {
        let index = self.entity_pipelines.len();
        self.push_pipeline_slots();
        self.entity_pipeline(index, true);
        self.entity_pipeline(index, false);
    }

    /// The world pipeline of key `index`: the depth-tested entity pipeline (statics bind
    /// the identity instance), so a key costs one driver compile for both.
    pub(super) fn world_pipeline(&self, index: usize) -> &wgpu::RenderPipeline {
        self.entity_pipeline(index, true)
    }

    /// The entity pipeline of key `index`, depth-tested or not, compiled on first use.
    pub(super) fn entity_pipeline(&self, index: usize, depth: bool) -> &wgpu::RenderPipeline {
        let slot = if depth {
            &self.entity_pipelines[index]
        } else {
            &self.entity_no_depth_pipelines[index]
        };
        slot.get_or_init(|| self.make_entity_pipeline(index, depth)())
    }

    /// What compiles the entity pipeline of key `index`, on any thread.
    fn make_entity_pipeline(
        &self,
        index: usize,
        depth: bool,
    ) -> impl FnOnce() -> wgpu::RenderPipeline + Send + 'static {
        let key = self.forge.pipeline_keys[index];
        let (layout, shader) = self.forge.program_for(key);
        let (device, layout, shader) = (self.forge.device.clone(), layout.clone(), shader.clone());
        let format = self.forge.format;
        move || {
            timed(usize::from(!depth), || {
                create_entity_pipeline(&device, &layout, &shader, format, key, depth)
            })
        }
    }

    /// A job for the entity pipeline of key `index` when it is not compiled yet.
    pub(super) fn entity_job(&self, index: usize, depth: bool, jobs: &mut pipeline_jobs::Jobs) {
        let slot = if depth {
            &self.entity_pipelines[index]
        } else {
            &self.entity_no_depth_pipelines[index]
        };
        if slot.get().is_none() {
            jobs.push(
                pipeline_jobs::Slot::Entity { index, depth },
                self.make_entity_pipeline(index, depth),
            );
        }
    }

    /// Draw detached geometry with the entity vertex path (buffer 0 vertices,
    /// buffer 1 one actor instance), opaque or blended materials only.
    pub(crate) fn draw_detached<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        geometry: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        detached: &'pass DetachedMaterials,
        draws: &'pass [DetachedDraw],
        blended: bool,
    ) {
        if draws.is_empty() {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(2, geometry, &[]);
        if let Some(sun) = &self.forge.model_sun {
            pass.set_bind_group(3, sun.binding(false), &[]);
        }
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut last_pipeline = None;
        for draw in draws {
            let Some(material) = detached.materials.get(draw.material) else {
                continue;
            };
            if material.blended != blended {
                continue;
            }
            for stage in &material.stages {
                if last_pipeline != Some(stage.pipeline) {
                    pass.set_pipeline(self.entity_pipeline(stage.pipeline, true));
                    last_pipeline = Some(stage.pipeline);
                }
                pass.set_bind_group(1, stage.color_group(), &[]);
                pass.draw_indexed(draw.indices.clone(), 0, 0..1);
            }
        }
    }
}

/// Time one pipeline creation into the per-variant profile. The driver reuses a compiled
/// program across pipelines that share both stages, so the no-depth variant is nearly
/// free after the depth-tested one, and world and entity draws share one vertex entry
/// (statics bind the identity instance) so a key never compiles twice.
fn timed(index: usize, make: impl FnOnce() -> wgpu::RenderPipeline) -> wgpu::RenderPipeline {
    let started = Instant::now();
    let pipeline = make();
    COMPILE_MS[index].fetch_add(
        started.elapsed().as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    pipeline
}

/// Cumulative driver time per variant (depth-tested, no-depth), for the load-profile
/// line: which program's compile a map load waits on.
static COMPILE_MS: [std::sync::atomic::AtomicU64; 2] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// Cumulative compile milliseconds per variant since start, in list order.
pub(super) fn compile_profile() -> [u64; 2] {
    COMPILE_MS
        .each_ref()
        .map(|v| v.load(std::sync::atomic::Ordering::Relaxed))
}

/// Compile `materials` as [`Runtime::append_entity_materials`] does, CPU only, and keep
/// nothing: their images land decoded in the process cache
/// ([`crate::decoded_image_cache`]), so the append that follows on the render thread
/// only uploads. Run on a worker; a failure is left for the append to report.
///
/// With `mipmapped`, the textures' mip chains are built too and returned, for
/// [`Runtime::upload_prepared_textures`]: resizing a model's textures down to one texel
/// on the render thread cost 50 to 180 ms for some player models.
pub(crate) fn warm_entity_materials(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    lightmap: &wgpu::TextureView,
    materials: &[ViewerMaterial],
    mipmapped: bool,
) -> Vec<PreparedTexture> {
    let mut image_cache = ImageCache::with_capacity(64);
    let mut prepared: Vec<PreparedTexture> = Vec::new();
    for key in materials {
        let Ok(material) = compile_material(
            vfs,
            shaders,
            key,
            lightmap,
            true,
            Default::default(),
            &mut image_cache,
            false,
        ) else {
            continue;
        };
        if !mipmapped {
            continue;
        }
        for stage in &material.stages {
            let textures = std::iter::once((&stage.primary_key, &stage.primary_pixels)).chain(
                stage
                    .secondary_key
                    .as_ref()
                    .zip(stage.secondary_pixels.as_ref()),
            );
            for (key, pixels) in textures {
                if key.starts_with(super::videos::KEY_PREFIX)
                    || prepared.iter().any(|texture| texture.key == *key)
                {
                    continue;
                }
                let (width, height) = super::filtering::mips::extent(pixels);
                prepared.push(PreparedTexture {
                    key: key.clone(),
                    width,
                    height,
                    chains: super::filtering::mips::cache::prepare(pixels, width, height),
                });
            }
        }
    }
    prepared
}

/// One texture's mip chains built on a worker ([`warm_entity_materials`]).
pub(crate) struct PreparedTexture {
    key: String,
    width: u32,
    height: u32,
    chains: Arc<Vec<Vec<RgbaImage>>>,
}

impl Runtime {
    /// Upload textures a worker prepared, under their material keys, so the materials
    /// appended next find them uploaded and only copy them to the GPU.
    pub(crate) fn upload_prepared_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        textures: Vec<PreparedTexture>,
    ) {
        if !self.forge.filtering.mipmapped() {
            return;
        }
        for texture in textures {
            if self.forge.texture_cache.contains_key(&texture.key) {
                continue;
            }
            // A failure leaves the texture to the material's own upload.
            if let Ok(view) = super::filtering::mips::upload_chains(
                device,
                queue,
                texture.width,
                texture.height,
                &texture.chains,
            ) {
                self.forge.texture_cache.insert(texture.key, view);
            }
        }
    }

    /// Whether this runtime's textures carry mip chains (`r_textureMode`).
    pub(crate) fn mipmapped(&self) -> bool {
        self.forge.filtering.mipmapped()
    }
}

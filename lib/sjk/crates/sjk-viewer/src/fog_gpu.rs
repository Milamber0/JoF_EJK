//! Map-lifetime volume uniforms, selectors and fog pipelines.
use super::*;
use crate::fog_volumes::{FOG_SLOTS, Mode, Table};
use sjk_shader::{FogPass, ShaderCull, ShaderDefinition};
use std::cell::OnceCell;

/// The fog key's depth-bias bit for a `polygonOffset` shader. rd-vanilla runs
/// `RB_FogPass` before `RB_StageIteratorGeneric` disables `GL_POLYGON_OFFSET_FILL`, so
/// a decal's fog is offset like its stages; unbiased, its `Equal` test fights with the
/// surface under it pixel by pixel.
pub(super) fn polygon_offset(definition: Option<&ShaderDefinition>) -> u8 {
    if definition.is_some_and(|d| d.polygon_offset) {
        crate::world_stage::POLYGON_OFFSET
    } else {
        0
    }
}

/// GPU resources allocated once, with selectors aligned for dynamic uniform offsets.
pub(super) struct FogGpu {
    pub(super) table: Table,
    group: wgpu::BindGroup,
    stride: u32,
    device: wgpu::Device,
    layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    /// The colour target's format, from the first key.
    format: Option<wgpu::TextureFormat>,
    keys: Vec<PipelineKey>,
    /// Per key, the world/mover and the entity variant: compiled with the key at map
    /// load, on a worker for a model loaded later ([`super::pipeline_jobs`]), or on
    /// first draw.
    mover: Vec<OnceCell<wgpu::RenderPipeline>>,
    entity: Vec<OnceCell<wgpu::RenderPipeline>>,
}

impl FogGpu {
    /// Upload the immutable fog table and aligned selector block once.
    pub(super) fn new(device: &wgpu::Device, camera: &wgpu::BindGroupLayout, table: Table) -> Self {
        let entries = [uniform_entry(0, false), uniform_entry(1, true)];
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK fog volume layout"),
            entries: &entries,
        });
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK map fog table"),
            contents: bytemuck::cast_slice(&table.entries),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let stride = device.limits().min_uniform_buffer_offset_alignment.max(16);
        let mut selectors = vec![0_u8; stride as usize * FOG_SLOTS * 2];
        for index in 0..FOG_SLOTS * 2 {
            let offset = index * stride as usize;
            let fog = (index % FOG_SLOTS) as u32;
            let global_exp2 = (index / FOG_SLOTS) as u32;
            selectors[offset..offset + 4].copy_from_slice(&fog.to_le_bytes());
            selectors[offset + 4..offset + 8].copy_from_slice(&global_exp2.to_le_bytes());
        }
        let selector = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK immutable fog selectors"),
            contents: &selectors,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK fog volumes"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &selector,
                        offset: 0,
                        size: NonZeroU64::new(16),
                    }),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK fog pipeline layout"),
            bind_group_layouts: &[
                Some(camera),
                Some(&bind_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&crate::world_stage::geometry_uniform::layout(device)),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK RB_FogPass"),
            source: wgpu::ShaderSource::Wgsl(crate::fog_volumes::SHADER.into()),
        });
        Self {
            table,
            group,
            stride,
            device: device.clone(),
            layout,
            shader,
            format: None,
            keys: Vec::new(),
            mover: Vec::new(),
            entity: Vec::new(),
        }
    }

    pub(super) fn pipeline_with_geometry(
        &mut self,
        _device: &wgpu::Device,
        format: wgpu::TextureFormat,
        fog_pass: FogPass,
        cull: ShaderCull,
        geometry: u8,
    ) -> Option<usize> {
        let index = self.slot(format, fog_pass, cull, geometry)?;
        self.mover(index);
        self.entity(index);
        Some(index)
    }

    /// The key index of a fog pass, registered without compiling when new.
    pub(super) fn slot(
        &mut self,
        format: wgpu::TextureFormat,
        fog_pass: FogPass,
        cull: ShaderCull,
        geometry: u8,
    ) -> Option<usize> {
        if fog_pass == FogPass::None {
            return None;
        }
        self.format = Some(format);
        // RB_FogPass, tr_shade.cpp:1120-1125: alpha blend, no depth write.
        let key = PipelineKey {
            geometry,
            source: wgpu::BlendFactor::SrcAlpha,
            destination: wgpu::BlendFactor::OneMinusSrcAlpha,
            depth_write: false,
            depth: if fog_pass == FogPass::Equal {
                wgpu::CompareFunction::Equal
            } else {
                wgpu::CompareFunction::LessEqual
            },
            cull: crate::world_stage::cull_face(cull),
        };
        if let Some(index) = self.keys.iter().position(|known| *known == key) {
            return Some(index);
        }
        let index = self.keys.len();
        self.keys.push(key);
        self.mover.push(OnceCell::new());
        self.entity.push(OnceCell::new());
        Some(index)
    }

    /// The world/mover variant of key `index`. Static world fog draws bind the identity
    /// instance and share the mover program: one driver compile per key instead of two.
    fn mover(&self, index: usize) -> &wgpu::RenderPipeline {
        self.mover[index].get_or_init(|| self.make(index, false)())
    }

    fn entity(&self, index: usize) -> &wgpu::RenderPipeline {
        self.entity[index].get_or_init(|| self.make(index, true)())
    }

    /// What compiles the `entity` or world/mover variant of key `index`, on any thread.
    fn make(
        &self,
        index: usize,
        entity: bool,
    ) -> impl FnOnce() -> wgpu::RenderPipeline + Send + 'static {
        let device = self.device.clone();
        let layout = self.layout.clone();
        let shader = self.shader.clone();
        let format = self.format.expect("a fog key carries its format");
        let key = self.keys[index];
        move || {
            if entity {
                create_entity_pipeline(&device, &layout, &shader, format, key, true)
            } else {
                create_pipeline(&device, &layout, &shader, format, key, true)
            }
        }
    }

    /// Jobs for the variants of key `index` not compiled yet.
    pub(super) fn jobs(&self, index: usize, jobs: &mut super::pipeline_jobs::Jobs) {
        use super::pipeline_jobs::Slot;
        for entity in [false, true] {
            let cell = if entity {
                &self.entity[index]
            } else {
                &self.mover[index]
            };
            if cell.get().is_none() {
                jobs.push(Slot::Fog { index, entity }, self.make(index, entity));
            }
        }
    }

    /// Put a pipeline a worker compiled in its slot; a slot filled meanwhile keeps its own.
    pub(super) fn install(&self, index: usize, entity: bool, pipeline: wgpu::RenderPipeline) {
        let cell = if entity {
            &self.entity[index]
        } else {
            &self.mover[index]
        };
        let _ = cell.set(pipeline);
    }

    /// Select a table index without modifying or allocating a GPU buffer.
    pub(super) fn bind<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        fog: u32,
        mode: Mode,
    ) {
        let bank = u32::from(mode == Mode::GlobalExp2) * FOG_SLOTS as u32;
        pass.set_bind_group(1, &self.group, &[(fog + bank) * self.stride]);
    }

    /// Bind the world, mover or entity vertex variant for the selected material.
    pub(super) fn set_pipeline<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        index: usize,
        kind: usize,
    ) {
        pass.set_pipeline(match kind {
            0 | 1 => self.mover(index),
            _ => self.entity(index),
        });
    }
}

fn uniform_entry(binding: u32, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

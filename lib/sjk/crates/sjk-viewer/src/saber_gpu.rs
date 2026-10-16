//! Production WGPU resources for saber blades and motion trails.

use crate::saber::{self, Instance};
use crate::saber_skins::{LoadedSkins, MAX_SKINS, SkinUniform};
use crate::saber_trail::SegmentPool;
use crate::{DepthTarget, texture_layout_entry};
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;
use std::ops::Range;

#[path = "saber_glow_integral.rs"]
pub(crate) mod glow_integral;

pub(crate) struct Runtime {
    pipeline: wgpu::RenderPipeline,
    /// The glow capsules alone, for the dynamic glow image: the retail `*_glow` blade
    /// shaders carry `glow`, the `*_line` cores do not (`sabers.shader`).
    glow_pipeline: wgpu::RenderPipeline,
    materials: Vec<wgpu::BindGroup>,
    /// What a blade-skin slot's pair is made with when a skin loads.
    texture_layout: wgpu::BindGroupLayout,
    samplers: saber::Samplers,
    /// The loaded blade skins' parameters (`saber.wgsl` `skins`), written only when
    /// skins load.
    skins_buffer: wgpu::Buffer,
    skins: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    ranges: [Range<u32>; crate::saber_rgb::MATERIAL_COUNT],
    trails: crate::saber_trail_gpu::Runtime,
    /// The classic model preview's blades, kept apart from the world's.
    preview_buffer: wgpu::Buffer,
    preview_ranges: [Range<u32>; crate::saber_rgb::MATERIAL_COUNT],
    /// Skins' heat haze (`saber.wgsl` `fragment_haze`): drawn first into the effect layer,
    /// replacing its pixels with the scene before effects, bent.
    haze_pipeline: wgpu::RenderPipeline,
    haze_layout: wgpu::BindGroupLayout,
    haze_sampler: wgpu::Sampler,
    /// The effect layer's scene image bound for the haze, made again only when the layer
    /// is rebuilt (a resize).
    haze_source: std::cell::RefCell<Option<(wgpu::TextureView, wgpu::BindGroup)>>,
    /// Whether a loaded skin has haze at all.
    any_haze: bool,
    /// Skins' black holes: their lens (`saber.wgsl` `fragment_lens`), drawn with the haze
    /// and blended over it, and their horizons (`fragment_shade`), blended black over
    /// the blades' glow after it.
    lens_pipeline: wgpu::RenderPipeline,
    shade_pipeline: wgpu::RenderPipeline,
    /// Whether a loaded skin has black holes at all.
    any_holes: bool,
    /// Skins' solid parts (`saber.wgsl` `fragment_solid`: ink and chrome), blended over the
    /// blades just drawn, and whether a loaded skin has any.
    solid_pipeline: wgpu::RenderPipeline,
    any_solids: bool,
}

/// Blade instances the model preview can draw: two sabers of eight blades, each its glow,
/// core and echo and a skin's glints.
pub(crate) const PREVIEW_BLADES: usize =
    2 * 8 * (3 + crate::blade_skin_file::extras::MAX_GLINTS as usize);

impl Runtime {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Result<Self, Box<dyn Error>> {
        let source = include_str!("saber.wgsl");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK saber shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK saber texture layout"),
            entries: &[
                texture_layout_entry(0),
                texture_layout_entry(1),
                texture_layout_entry(3),
                sampler_entry(2),
                sampler_entry(4),
            ],
        });
        let samplers = saber::Samplers::new(device);
        let materials =
            saber::create_materials(device, queue, vfs, shaders, &texture_layout, &samplers)?;
        let skins_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK saber skins layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let skins_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK saber skins"),
            size: std::mem::size_of::<[SkinUniform; MAX_SKINS]>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let skins = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK saber skins"),
            layout: &skins_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: skins_buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK saber pipeline layout"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&texture_layout),
                Some(&skins_layout),
            ],
            immediate_size: 0,
        });
        let haze_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK saber haze scene layout"),
            entries: &[texture_layout_entry(0), sampler_entry(1)],
        });
        let haze_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK saber haze pipeline layout"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&texture_layout),
                Some(&skins_layout),
                Some(&haze_layout),
            ],
            immediate_size: 0,
        });
        let create = |fragment| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK saber pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(Instance::layout())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ADDITIVE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = create("fragment_main");
        let glow_pipeline = create("fragment_glow");
        // The haze replaces what it covers (the layer still holds the scene there, as no
        // effect has drawn yet), depth-tested so a wall before the blade stays sharp; a
        // black hole's lens and horizon blend over what is there.
        let special = |label, layout, vertex, fragment, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(Instance::layout())],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let haze_pipeline = special(
            "SJK saber haze pipeline",
            &haze_pipeline_layout,
            "vertex_haze",
            "fragment_haze",
            None,
        );
        let lens_pipeline = special(
            "SJK saber black hole lens pipeline",
            &haze_pipeline_layout,
            "vertex_hole",
            "fragment_lens",
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let shade_pipeline = special(
            "SJK saber black hole horizon pipeline",
            &layout,
            "vertex_hole",
            "fragment_shade",
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let solid_pipeline = special(
            "SJK saber solid pipeline",
            &layout,
            "vertex_solid",
            "fragment_solid",
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let haze_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK saber haze sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK saber instances"),
            size: (saber::MAX_BLADE_INSTANCES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let preview_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK model preview saber instances"),
            size: (PREVIEW_BLADES * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            glow_pipeline,
            materials,
            texture_layout,
            samplers,
            skins_buffer,
            skins,
            instance_buffer,
            preview_buffer,
            preview_ranges: std::array::from_fn(|_| 0..0),
            ranges: std::array::from_fn(|_| 0..0),
            haze_pipeline,
            haze_layout,
            haze_sampler,
            haze_source: std::cell::RefCell::new(None),
            any_haze: false,
            lens_pipeline,
            shade_pipeline,
            any_holes: false,
            solid_pipeline,
            any_solids: false,
            trails: crate::saber_trail_gpu::Runtime::new(
                device,
                queue,
                vfs,
                shaders,
                camera_layout,
                format,
            )?,
        })
    }

    /// Take the loaded blade skins: their parameters into the uniform array and their
    /// glow/core pairs into their material slots. Only when skins load, never per frame.
    pub(crate) fn upload_skins(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        skins: &LoadedSkins,
    ) {
        queue.write_buffer(
            &self.skins_buffer,
            0,
            bytemuck::cast_slice(&skins.uniforms()),
        );
        self.any_haze = skins.iter().any(|(_, skin)| skin.def.haze.is_some());
        self.any_holes = skins.iter().any(|(_, skin)| skin.def.holes.is_some());
        self.any_solids = skins.iter().any(|(_, skin)| skin.def.solids().any());
        for (index, skin) in skins.iter() {
            self.materials[crate::saber_rgb::SKIN_MATERIAL as usize + usize::from(index)] =
                saber::material(
                    device,
                    queue,
                    &self.texture_layout,
                    &self.samplers,
                    &skin.glow,
                    &skin.core,
                );
        }
    }

    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        blades: &mut [Instance],
        trails: &mut SegmentPool,
        now: i64,
    ) {
        self.ranges = saber::material_ranges(blades);
        if !blades.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(blades));
        }
        self.trails.prepare(queue, trails, now);
    }

    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
    ) {
        self.trails.draw(pass, camera);
        self.draw_blades(pass, camera, &self.pipeline);
        self.draw_horizons(pass, camera, &self.instance_buffer, &self.ranges);
    }

    /// Whether this frame's blades may bend the scene with heat haze or a black hole's
    /// lens: a loaded skin has one and a skinned blade is drawn (the vertex stage folds
    /// away every other one).
    pub(crate) fn has_haze(&self) -> bool {
        (self.any_haze || self.any_holes) && skinned(&self.ranges)
    }

    /// The black holes' horizons and the skins' solid parts (ink, chrome) over the blades
    /// just drawn from `buffer`.
    fn draw_horizons<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        buffer: &'pass wgpu::Buffer,
        ranges: &[Range<u32>],
    ) {
        if self.any_holes && skinned(ranges) {
            draw_instances(
                pass,
                camera,
                &self.skins,
                &self.shade_pipeline,
                &self.materials,
                buffer,
                ranges,
            );
        }
        if self.any_solids && skinned(ranges) {
            draw_instances(
                pass,
                camera,
                &self.skins,
                &self.solid_pipeline,
                &self.materials,
                buffer,
                ranges,
            );
        }
    }

    /// Draw the skins' heat haze into the effect layer whose scene image is `scene`
    /// (`effect_layer.rs` `original`), before any other effect: each pixel it covers is
    /// the scene there, bent.
    pub(crate) fn draw_haze(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        camera: &wgpu::BindGroup,
        scene: &wgpu::TextureView,
    ) {
        if !self.has_haze() {
            return;
        }
        let bind = {
            let mut source = self.haze_source.borrow_mut();
            match source.as_ref() {
                Some((view, bind)) if view == scene => bind.clone(),
                _ => {
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("SJK saber haze scene"),
                        layout: &self.haze_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(scene),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.haze_sampler),
                            },
                        ],
                    });
                    *source = Some((scene.clone(), bind.clone()));
                    bind
                }
            }
        };
        // The haze first, then the lenses over it: each in its own run, as the instances
        // of a material are in no set order.
        let pipelines = [
            (self.any_haze, &self.haze_pipeline),
            (self.any_holes, &self.lens_pipeline),
        ];
        for (_, pipeline) in pipelines.into_iter().filter(|(any, _)| *any) {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, camera, &[]);
            pass.set_bind_group(2, &self.skins, &[]);
            pass.set_bind_group(3, &bind, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            let skins = crate::saber_rgb::SKIN_MATERIAL as usize;
            for (material, instances) in self
                .materials
                .iter()
                .zip(self.ranges.iter().cloned())
                .skip(skins)
            {
                if instances.is_empty() {
                    continue;
                }
                pass.set_bind_group(1, material, &[]);
                pass.draw(0..6, instances);
            }
        }
    }

    /// Upload the model preview's blades (at most [`PREVIEW_BLADES`]).
    pub(crate) fn prepare_preview(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        blades: &mut [Instance],
    ) {
        let count = blades.len().min(PREVIEW_BLADES);
        let blades = &mut blades[..count];
        self.preview_ranges = saber::material_ranges(blades);
        if !blades.is_empty() {
            queue.write_buffer(&self.preview_buffer, 0, bytemuck::cast_slice(blades));
        }
    }

    /// Draw the model preview's blades with the preview's `camera`.
    pub(crate) fn draw_preview<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
    ) {
        draw_instances(
            pass,
            camera,
            &self.skins,
            &self.pipeline,
            &self.materials,
            &self.preview_buffer,
            &self.preview_ranges,
        );
        self.draw_horizons(pass, camera, &self.preview_buffer, &self.preview_ranges);
    }

    /// Draw into the dynamic glow image: the trails (`saberBlur`/`swordTrail` glow)
    /// unless `blades_only` (`r_DynamicGlow 2`, stock's `RT_SABER_GLOW` test), then the
    /// blades' glow capsules without their cores.
    pub(crate) fn draw_glow<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        blades_only: bool,
    ) {
        if !blades_only {
            self.trails.draw(pass, camera);
        }
        self.draw_blades(pass, camera, &self.glow_pipeline);
        // A hole in front of the blade hides its glow there too.
        self.draw_horizons(pass, camera, &self.instance_buffer, &self.ranges);
    }

    /// Whether the glow image gets any saber this frame.
    pub(crate) fn has_glow(&self, blades_only: bool) -> bool {
        (!blades_only && self.trails.has_draws()) || self.ranges.iter().any(|r| !r.is_empty())
    }

    fn draw_blades<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        pipeline: &'pass wgpu::RenderPipeline,
    ) {
        draw_instances(
            pass,
            camera,
            &self.skins,
            pipeline,
            &self.materials,
            &self.instance_buffer,
            &self.ranges,
        );
    }

    /// Add this frame's trail quads to the effect layer's screen bounds.
    pub(crate) fn bound_trails(
        &self,
        bounds: &mut crate::frame_target::aa::effects::bounds::Bounds,
    ) {
        for position in self.trails.positions() {
            bounds.sphere(glam::Vec3::from_array(position), 0.0);
        }
    }

    /// Whether this frame has any blade or trail to draw.
    pub(crate) fn has_draws(&self) -> bool {
        self.trails.has_draws() || self.ranges.iter().any(|range| !range.is_empty())
    }
}

/// Whether `ranges` holds a skinned blade.
fn skinned(ranges: &[Range<u32>]) -> bool {
    ranges[crate::saber_rgb::SKIN_MATERIAL as usize..]
        .iter()
        .any(|range| !range.is_empty())
}

/// Draw the blade instances of `buffer`, a run per material.
fn draw_instances<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    camera: &'pass wgpu::BindGroup,
    skins: &'pass wgpu::BindGroup,
    pipeline: &'pass wgpu::RenderPipeline,
    materials: &'pass [wgpu::BindGroup],
    buffer: &'pass wgpu::Buffer,
    ranges: &[Range<u32>],
) {
    if ranges.iter().all(Range::is_empty) {
        return;
    }
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, camera, &[]);
    pass.set_bind_group(2, skins, &[]);
    pass.set_vertex_buffer(0, buffer.slice(..));
    for (material, instances) in materials.iter().zip(ranges.iter().cloned()) {
        if instances.is_empty() {
            continue;
        }
        pass.set_bind_group(1, material, &[]);
        pass.draw(0..6, instances);
    }
}

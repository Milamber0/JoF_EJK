//! rd-vanilla's blended-effect framebuffer: an 8-bit, display-encoded layer.
//!
//! Stock draws every blended shader stage straight into an 8-bit gamma-space colour
//! buffer (`rd-vanilla/tr_backend.cpp` `GL_State` blend factors on a UNORM target), so
//! `GL_ONE GL_ONE` adds display values and clamps each channel to 1, and
//! `GL_DST_COLOR GL_SRC_COLOR` is `2·src·dst` on display values. SJK's scene is linear
//! (an sRGB swapchain or an RGBA16F HDR target), so effects (particles, effect geometry,
//! decals, sabers and trails) are blended here instead:
//!
//! 1. [`Layer::encode`] writes the scene's display values into two `Rgba8Unorm` images,
//!    `blended` and `original`.
//! 2. The effect pipelines, built for [`FORMAT`], blend into `blended` with the scene's
//!    depth attached read-only.
//! 3. The main view's final resolve adds `blended − original` to its display value, so the
//!    world, its bloom and its anti-aliasing never see the effects; a secondary view
//!    writes `blended` back, decoded, wherever it differs from `original`.
//!
//! Every resource is sized at construction or resize; a frame only records passes.

use std::cell::RefCell;

#[path = "effect_bounds.rs"]
pub(crate) mod bounds;

/// Colour format of the effect pipelines: display values, clamped per channel.
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Retained bind groups per scene target read by [`Layer::encode`]. The main, sky,
/// portal and floor targets are the only readers, so four entries never thrash.
const SOURCES: usize = 4;

/// The layer images and the passes that fill and drain them.
pub(crate) struct Layer {
    size: [u32; 2],
    blended: wgpu::TextureView,
    original: wgpu::TextureView,
    encode: wgpu::RenderPipeline,
    write_back: wgpu::RenderPipeline,
    write_back_bind: wgpu::BindGroup,
    source_layout: wgpu::BindGroupLayout,
    /// The scene exposure (`post_exposure.rs`), bound with every source.
    exposure: wgpu::Buffer,
    /// Scene views already bound for encoding, replaced only when a target is rebuilt.
    sources: RefCell<[Option<(wgpu::TextureView, wgpu::BindGroup)>; SOURCES]>,
    next_source: std::cell::Cell<usize>,
    /// The main camera, for [`crate::effect_bounds`]; set with the camera uniform.
    view_projection: std::cell::Cell<[[f32; 4]; 4]>,
    /// Texture-space rectangle `[u0, v0, u1, v1]` the final resolve merges.
    region: wgpu::Buffer,
    region_value: std::cell::Cell<[f32; 4]>,
}

/// How the layer turns a scene target's colour into display values.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Encoding {
    /// Scene attachment format; every scene target of a context shares it.
    pub(crate) scene: wgpu::TextureFormat,
}

impl Encoding {
    fn mode(self) -> f64 {
        if matches!(
            self.scene,
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rg11b10Ufloat
        ) {
            2.0
        } else if self.scene.is_srgb() {
            1.0
        } else {
            0.0
        }
    }
}

impl Layer {
    /// Allocate both images and the three pipelines; called at startup and resize only.
    /// `exposure` is the scene exposure state the final resolve reads.
    pub(crate) fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        encoding: Encoding,
        exposure: &wgpu::Buffer,
    ) -> Self {
        let texture = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0].max(1),
                        height: size[1].max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let blended = texture("SJK legacy effect layer");
        let original = texture("SJK legacy effect layer, scene before effects");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK legacy effect layer"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("effect_layer.wgsl"),
                    include_str!("post_hdr.wgsl")
                )
                .into(),
            ),
        });
        let entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            count: None,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                view_dimension: wgpu::TextureViewDimension::D2,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
            },
        };
        let exposure_entry = wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::FRAGMENT,
            count: None,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        };
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK effect layer source"),
            entries: &[entry(0), exposure_entry],
        });
        let drain_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK effect layer write-back"),
            entries: &[entry(0), entry(2), exposure_entry],
        });
        let constants = [("ENCODING", encoding.mode())];
        let pipeline = |layout: &wgpu::BindGroupLayout,
                        entry_point,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets,
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let encode = pipeline(
            &source_layout,
            "fs_encode",
            &[Some(FORMAT.into()), Some(FORMAT.into())],
        );
        let write_back = pipeline(
            &drain_layout,
            "fs_write_back",
            &[Some(wgpu::ColorTargetState {
                format: encoding.scene,
                blend: None,
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        );
        let write_back_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK effect layer write-back"),
            layout: &drain_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&blended),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&original),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: exposure.as_entire_binding(),
                },
            ],
        });
        let region = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK effect layer merge region"),
            size: 16,
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            size,
            blended,
            original,
            encode,
            write_back,
            write_back_bind,
            source_layout,
            exposure: exposure.clone(),
            sources: RefCell::new(Default::default()),
            next_source: Default::default(),
            view_projection: std::cell::Cell::new(glam::Mat4::IDENTITY.to_cols_array_2d()),
            region,
            region_value: std::cell::Cell::new([0.0; 4]),
        }
    }

    /// Dimensions of both images; they always equal the scene targets'.
    pub(crate) const fn size(&self) -> [u32; 2] {
        self.size
    }

    /// Effects composited over the scene, in display values.
    pub(crate) const fn blended(&self) -> &wgpu::TextureView {
        &self.blended
    }

    /// The scene's display values before any effect.
    pub(crate) const fn original(&self) -> &wgpu::TextureView {
        &self.original
    }

    /// Uniform `[u0, v0, u1, v1]` outside which the final resolve skips the merge.
    pub(crate) const fn region(&self) -> &wgpu::Buffer {
        &self.region
    }

    /// Remember the main camera for this frame's effect bounds.
    pub(crate) fn set_view(&self, view_projection: [[f32; 4]; 4]) {
        self.view_projection.set(view_projection);
    }

    /// The main camera of the frame being drawn.
    pub(crate) fn view(&self) -> glam::Mat4 {
        glam::Mat4::from_cols_array_2d(&self.view_projection.get())
    }

    /// Limit the final merge to `region` (pixels of the layer; `None` is all of it). Writes
    /// 16 bytes only when the rectangle changes. Interior edges are inset by a texel, so a
    /// filtered resolve (render scale) never reaches the older texels outside it; the
    /// effect bounds carry a wider margin than that.
    pub(crate) fn set_merge_region(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        region: Option<[u32; 4]>,
    ) {
        let [w, h] = self.size.map(|n| n.max(1) as f32);
        let value = region.map_or([0.0, 0.0, 1.0, 1.0], |[x, y, width, height]| {
            // Only interior edges: at the target's border clamp-to-edge stays inside.
            let [right, bottom] = [x + width, y + height];
            let inset = [
                x + u32::from(x > 0),
                y + u32::from(y > 0),
                right - u32::from(right < self.size[0] && right > x),
                bottom - u32::from(bottom < self.size[1] && bottom > y),
            ];
            [
                inset[0] as f32 / w,
                inset[1] as f32 / h,
                inset[2] as f32 / w,
                inset[3] as f32 / h,
            ]
        });
        if self.region_value.replace(value) != value {
            queue.write_buffer(&self.region, 0, bytemuck::cast_slice(&value));
        }
    }

    /// Copy `scene`'s display values into both images, within `region` (x, y, width,
    /// height in pixels) when given. `scene` must be the size of the layer.
    pub(crate) fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        region: Option<[u32; 4]>,
    ) {
        let bind = self.source(device, scene);
        // Pixels outside `region` keep older contents: neither the merge nor the write-back
        // reads them.
        let mut pass = self.pass(encoder, wgpu::LoadOp::Load, "SJK effect layer encode");
        if let Some([x, y, w, h]) = region {
            pass.set_scissor_rect(x, y, w, h);
        }
        pass.set_pipeline(&self.encode);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Begin blending effects into the layer against the view's depth, read-only.
    pub(crate) fn begin_effects<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK legacy blended effects"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.blended,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Replace a secondary view's pixels that the effects changed with their scene value,
    /// within `region` when given.
    pub(crate) fn write_back(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        region: Option<[u32; 4]>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SJK effect layer write-back"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some([x, y, w, h]) = region {
            pass.set_scissor_rect(x, y, w, h);
        }
        pass.set_pipeline(&self.write_back);
        pass.set_bind_group(0, &self.write_back_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    fn pass<'a>(
        &self,
        encoder: &'a mut wgpu::CommandEncoder,
        load: wgpu::LoadOp<wgpu::Color>,
        label: &'static str,
    ) -> wgpu::RenderPass<'a> {
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[attachment(&self.blended), attachment(&self.original)],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// The retained binding for `scene`, created the first time a target is seen.
    fn source(&self, device: &wgpu::Device, scene: &wgpu::TextureView) -> wgpu::BindGroup {
        let mut sources = self.sources.borrow_mut();
        if let Some((_, bind)) = sources.iter().flatten().find(|(view, _)| view == scene) {
            return bind.clone();
        }
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK effect layer source"),
            layout: &self.source_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.exposure.as_entire_binding(),
                },
            ],
        });
        let slot = self.next_source.get();
        self.next_source.set((slot + 1) % SOURCES);
        sources[slot] = Some((scene.clone(), bind.clone()));
        bind
    }
}

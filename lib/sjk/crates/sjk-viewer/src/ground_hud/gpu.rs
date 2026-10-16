//! GPU side of the ground HUD: one pipeline, one uniform, one instanced
//! four-vertex strip per quad (digits and the stance dot), drawn in the
//! depth-reading overlay pass after the world is complete. Digits sample the
//! client's own UI font atlas through the text bind group.

use super::layout::{Kind, Layout, MAX_QUADS, Quad};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// Layout shared with `ground_hud.wgsl`'s `GpuQuad`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuQuad {
    rect: [f32; 4],
    uv: [f32; 4],
    bounds: [f32; 4],
    outline: [f32; 4],
    color: [f32; 4],
}

/// Layout shared with `ground_hud.wgsl`'s `Ground`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    forward: [f32; 4],
    anchor: [f32; 4],
    viewport: [f32; 4],
    style: [f32; 4],
    depth: [f32; 4],
    quads: [GpuQuad; MAX_QUADS],
}

/// Opacity and depth tuning. One place to tune.
pub(super) mod look {
    /// Occluders within this horizontal distance of the player's axis are the
    /// player's own body.
    pub(crate) const BODY_RADIUS: f32 = 22.0;
    /// Opacity of a part behind the player's own body.
    pub(crate) const BEHIND_BODY: f32 = 0.78;
    /// Lift above the feet plane, against the floor's own depth.
    pub(crate) const LIFT: f32 = 0.6;
    /// Opacity of a part something stands in front of.
    pub(crate) const GHOST: f32 = 0.3;
    /// Opacity of the whole HUD.
    pub(crate) const OPACITY: f32 = 0.95;
    /// View-depth band over which a covered part fades to the ghost: floor
    /// bumps and slopes within the start never dim it.
    pub(crate) const OCCLUSION: [f32; 2] = [14.0, 30.0];
    /// Opacity of the dark outline around digits.
    pub(crate) const OUTLINE: f32 = 0.6;
    /// Opacity of the dark contact halo around the dot.
    pub(crate) const HALO: f32 = 0.45;
}

/// Resolved view of the frame, retained from the camera upload.
#[derive(Clone, Copy)]
pub(super) struct View {
    pub(super) view_projection: Mat4,
    pub(super) camera: Vec3,
    pub(super) forward: Vec3,
}

impl Default for View {
    fn default() -> Self {
        Self {
            view_projection: Mat4::IDENTITY,
            camera: Vec3::ZERO,
            forward: Vec3::X,
        }
    }
}

/// One frame's placement.
#[derive(Clone, Copy)]
pub(super) struct Placement {
    /// Feet: predicted origin plus `mins` z.
    pub(super) feet: Vec3,
    /// Player yaw in radians.
    pub(super) yaw: f32,
    /// Colour target size.
    pub(super) viewport: [f32; 2],
}

pub(super) struct Renderer {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Colours are converted to linear light for this target (sRGB or the float HDR scene).
    linear: bool,
    quads: u32,
}

impl Renderer {
    /// `text_layout` is the UI text bind-group layout (atlas texture, filtering sampler).
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        text_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK ground HUD shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../ground_hud.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK ground HUD uniform"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK ground HUD uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK ground HUD uniform"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let depth_layout = crate::world_materials::flares::depth_layout(device);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK ground HUD"),
            bind_group_layouts: &[Some(&layout), Some(&depth_layout), Some(text_layout)],
            immediate_size: 0,
        });
        // Premultiplied colour over the scene; the target's alpha is left alone.
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK ground HUD"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            // The pass holds scene depth read-only; the shader samples it
            // itself to ghost, rather than cut, what stands in front.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            buffer,
            bind_group,
            linear: format.is_srgb() || format == wgpu::TextureFormat::Rgba16Float,
            quads: 0,
        }
    }

    /// Upload one frame's uniform: the view, the placement and every quad.
    pub(super) fn upload(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        view: View,
        placement: Placement,
        layout: &Layout,
    ) {
        let [width, height] = placement.viewport;
        let feet = placement.feet + Vec3::Z * look::LIFT;
        let mut uniform = Uniform {
            view_projection: view.view_projection.to_cols_array_2d(),
            inverse_view_projection: view.view_projection.inverse().to_cols_array_2d(),
            camera: view.camera.extend(look::BODY_RADIUS).to_array(),
            forward: view.forward.extend(look::BEHIND_BODY).to_array(),
            anchor: [feet.x, feet.y, feet.z, placement.yaw],
            viewport: [width, height, 1.0 / width.max(1.0), 1.0 / height.max(1.0)],
            style: [look::GHOST, look::OPACITY, look::OUTLINE, look::HALO],
            depth: [look::OCCLUSION[0], look::OCCLUSION[1], 0.0, 0.0],
            quads: [GpuQuad::zeroed(); MAX_QUADS],
        };
        for (slot, quad) in uniform.quads.iter_mut().zip(layout.quads()) {
            *slot = self.gpu_quad(quad);
        }
        self.quads = layout.quads().len() as u32;
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn gpu_quad(&self, quad: &Quad) -> GpuQuad {
        let rgb = if self.linear {
            quad.color.map(srgb_to_linear)
        } else {
            quad.color
        };
        let kind = match quad.kind {
            Kind::Glyph => 0.0,
            Kind::Dot => 1.0,
        };
        GpuQuad {
            rect: quad.rect,
            uv: quad.uv,
            bounds: quad.bounds,
            outline: [quad.outline[0], quad.outline[1], 0.0, 0.0],
            color: [rgb[0], rgb[1], rgb[2], kind],
        }
    }

    /// Draw into a pass whose depth attachment is the scene depth, read-only;
    /// `text` is the UI font atlas bind group the layout was built from.
    pub(super) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        depth: &'a wgpu::BindGroup,
        text: &'a wgpu::BindGroup,
    ) {
        if self.quads == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, depth, &[]);
        pass.set_bind_group(2, text, &[]);
        pass.draw(0..4, 0..self.quads);
    }
}

/// sRGB transfer function, display value to linear light.
pub(super) fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

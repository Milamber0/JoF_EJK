//! Floating dust motes around the camera (`r_dustMotes`, on by default in SJK),
//! presentation only.
//!
//! Every mote is generated in `dust_motes.wgsl` from its
//! instance index and the frame time, so the effect owns no vertex or instance
//! buffer: one pipeline and a 16-byte uniform are created per world, and a
//! frame draws `6 × count` vertices. Motes are fixed in world space inside a
//! cube that wraps around the camera, fade out with distance and near the eye,
//! depth-test against the scene without writing depth, and appear only where
//! the current godray volume contains local beam contrast. No volume means no dust.
//!
//! To limit visual obstruction, motes are one to two
//! world units across, their opacity is capped at [`PEAK_ALPHA`] and no mote is
//! drawn within 16 units of the eye.
//!
//! Per-map tuning is not implemented. It would be a multiplier applied to
//! [`Settings::intensity`] in [`Frame::new`]'s caller, resolved once at map load
//! (for example from a worldspawn key or a map-name table).

#[path = "dust_beams.rs"]
pub(crate) mod beams;
use bytemuck::{Pod, Zeroable};
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Console variable: 0 disables the effect, 1 is full density and opacity.
pub(crate) const CVAR: &str = "r_dustMotes";
/// Motes drawn at intensity 1; lower intensities draw a prefix of the same set,
/// so changing the value adds or removes motes without moving the others.
pub(crate) const MAX_MOTES: u32 = 2048;
/// Opacity of a mote's centre at intensity 1, before distance fades.
pub(crate) const PEAK_ALPHA: f32 = 0.35;
/// Opacity kept at the lowest nonzero intensity, so sparse motes stay visible.
const ALPHA_FLOOR: f32 = 0.4;

/// SJK's default intensity: full (Sol's choice). Dust still needs a godray volume.
const DEFAULT_INTENSITY: f32 = 1.0;

/// Live intensity shared by the console and every installed world.
#[derive(Clone)]
pub(crate) struct Settings(Arc<AtomicU32>);

impl Default for Settings {
    fn default() -> Self {
        Self(Arc::new(AtomicU32::new(DEFAULT_INTENSITY.to_bits())))
    }
}

impl Settings {
    /// Register the archived cvar (default 1, full) and follow its changes.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        cvars.register(CvarDefinition::new(
            CVAR,
            f64::from(DEFAULT_INTENSITY),
            CvarFlags::ARCHIVE,
            "Dust in godrays, 0 off to 1; requires r_volumetrics, applies immediately",
        ))?;
        Self::from_registered(cvars)
    }

    /// Seed from the registered value, including an archived one, then subscribe.
    fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        if let Some(cvar) = cvars.get(CVAR) {
            settings.set(&cvar.value);
        }
        let changed = settings.clone();
        cvars.on_change(CVAR, move |change| changed.set(&change.current))?;
        Ok(settings)
    }

    fn set(&self, value: &CvarValue) {
        let value = match value {
            CvarValue::Float(value) => *value,
            CvarValue::Integer(value) => *value as f64,
            _ => return,
        };
        if value.is_finite() {
            let value = value.clamp(0.0, 1.0) as f32;
            self.0.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// Clamped intensity in 0..=1; one relaxed read per frame.
    pub(crate) fn intensity(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Peak mote opacity and uniform-buffer padding. Beam colour comes from the GPU volume.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct DustUniform {
    opacity: [f32; 4],
}

/// What one frame draws, derived without allocation from the intensity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Frame {
    count: u32,
    uniform: DustUniform,
}

impl Frame {
    /// Scale count and peak opacity; beam lighting is sampled at each mote on the GPU.
    pub(crate) fn new(intensity: f32) -> Self {
        let intensity = if intensity.is_finite() {
            intensity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let count = (intensity * MAX_MOTES as f32).round() as u32;
        if count == 0 {
            return Self::default();
        }
        let alpha = PEAK_ALPHA * (ALPHA_FLOOR + (1.0 - ALPHA_FLOOR) * intensity);
        Self {
            count,
            uniform: DustUniform {
                opacity: [alpha, 0.0, 0.0, 0.0],
            },
        }
    }

    /// Number of mote instances to draw; zero skips the effect.
    pub(crate) fn count(&self) -> u32 {
        self.count
    }
}

/// World-lifetime pipeline and uniform; nothing is created per frame.
pub(crate) struct Runtime {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    frame: Frame,
}

impl Runtime {
    /// Build the pipeline for the scene attachment format and depth target.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK dust layout"),
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
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK dust uniform"),
            contents: bytemuck::bytes_of(&DustUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK dust bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK dust pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout), Some(&beams::layout(device))],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK dust shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("volumetric_coordinates.wgsl"),
                    include_str!("dust_beams.wgsl"),
                    include_str!("dust_motes.wgsl"),
                )
                .into(),
            ),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK dust pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            buffer,
            bind_group,
            frame: Frame::default(),
        }
    }

    /// Upload the uniform only when it changed; a disabled effect writes nothing.
    fn prepare(&mut self, queue: &crate::frame_queue::FrameQueue, frame: Frame) {
        if frame.count() != 0 && frame.uniform != self.frame.uniform {
            queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&frame.uniform));
        }
        if frame.count() != 0 {
            self.frame = frame;
        } else {
            self.frame.count = 0;
        }
    }

    /// Draw after the current frame's godray volume has been computed.
    fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        beams: &'pass wgpu::BindGroup,
    ) {
        if self.frame.count() == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_bind_group(2, beams, &[]);
        pass.draw(0..6, 0..self.frame.count());
    }
}

impl crate::GpuState {
    /// Prepare intensity only; the camera's room lighting must not light distant dust.
    pub(crate) fn prepare_dust_motes(&mut self) {
        let frame = Frame::new(self.context.dust_motes.intensity());
        self.dust_motes.prepare(&self.queue, frame);
    }

    /// Depth-tested motes consume this frame's local scattering after the godray pass.
    pub(crate) fn draw_dust_motes(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        if self.dust_motes.frame.count() == 0 {
            return;
        }
        let Some(beams) = self.world_materials.dust_beams() else {
            return;
        };
        let mut pass = crate::main_scene_pass::scene_pass(
            encoder,
            target,
            &self.depth.view,
            wgpu::LoadOp::Load,
            wgpu::LoadOp::Load,
        );
        self.dust_motes
            .draw(&mut pass, &self.camera_bind_group, beams);
    }
}

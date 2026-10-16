//! Reusable render targets and screen-space portal compositing.
use super::*;
use wgpu::util::DeviceExt;

pub(super) struct Target {
    pub size: [u32; 2],
    pub color: wgpu::TextureView,
    pub depth: crate::DepthTarget,
    pub camera: wgpu::Buffer,
    pub camera_bind: wgpu::BindGroup,
    pub instances: wgpu::Buffer,
    pub normal_sample: wgpu::BindGroup,
    pub mirror_sample: wgpu::BindGroup,
}

impl Target {
    pub fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        samples: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SJK map portal view"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color = texture.create_view(&Default::default());
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK portal camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK portal surface transform"),
            size: std::mem::size_of::<ActorInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sample = |mirror: f32| {
            let control = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[mirror, 0., 0., 0.]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("SJK portal composite sample"),
                layout: samples,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&color),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: control.as_entire_binding(),
                    },
                ],
            })
        };
        let normal_sample = sample(0.);
        let mirror_sample = sample(1.);
        Self {
            size,
            color,
            depth: crate::DepthTarget::new(device, size[0], size[1]),
            camera,
            camera_bind,
            instances,
            normal_sample,
            mirror_sample,
        }
    }
}

pub(super) fn sample_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

/// The composite of a portal's view (`portal_main`) or of the sky portal's on the sky
/// faces (`sky_portal_main`, behind them as the sky stage's faces are).
pub(super) fn pipeline(
    device: &wgpu::Device,
    camera: &wgpu::BindGroupLayout,
    samples: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    vertex_entry: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("SJK portal composite"),
        source: wgpu::ShaderSource::Wgsl(
            concat!(
                include_str!("vertex_transform.wgsl"),
                include_str!("scene_view.wgsl")
            )
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(camera), Some(samples)],
        immediate_size: 0,
    });
    let buffers = [Some(GpuVertex::layout()), Some(ActorInstance::layout())];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SJK map view composite"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(vertex_entry),
            compilation_options: Default::default(),
            buffers: &buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("sample_view"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DepthTarget::FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

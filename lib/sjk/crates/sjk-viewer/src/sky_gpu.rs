//! Pipeline construction for the sky renderer.

use super::{GpuVertex, SkyVertex};

pub(super) fn texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK Q3 sky-box texture layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub(super) fn box_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    radiance: f32,
) -> wgpu::RenderPipeline {
    pipeline(
        device,
        layout,
        shader,
        "box_vertex",
        "box_fragment",
        format,
        wgpu::ColorWrites::ALL,
        None,
        SkyVertex::layout(),
        1,
        radiance,
    )
}

/// Sky face pipeline for the scene pass.
#[derive(Clone)]
pub(super) struct FacePipelines {
    pub(super) single: wgpu::RenderPipeline,
}

/// Sky faces with the box's colour: the mask's geometry and culling, colour written.
pub(super) fn face_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    radiance: f32,
) -> FacePipelines {
    let build = |samples| {
        pipeline(
            device,
            layout,
            shader,
            "face_vertex",
            "face_fragment",
            format,
            wgpu::ColorWrites::ALL,
            Some(wgpu::Face::Front),
            GpuVertex::layout(),
            samples,
            radiance,
        )
    };
    FacePipelines { single: build(1) }
}

pub(super) fn mask_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    pipeline(
        device,
        layout,
        shader,
        "mask_vertex",
        "mask_fragment",
        format,
        wgpu::ColorWrites::empty(),
        Some(wgpu::Face::Front),
        GpuVertex::layout(),
        1,
        1.,
    )
}

#[allow(clippy::too_many_arguments)]
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    vertex_entry: &str,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
    write_mask: wgpu::ColorWrites,
    cull_mode: Option<wgpu::Face>,
    vertex_layout: wgpu::VertexBufferLayout<'static>,
    samples: u32,
    radiance: f32,
) -> wgpu::RenderPipeline {
    let targets = [Some(wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("SJK Q3 sky pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_layout)],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("sky_radiance", f64::from(radiance))],
                ..Default::default()
            },
            targets: &targets,
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DepthTarget::FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

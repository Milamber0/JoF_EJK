//! Volumetric clouds over every map with open sky (`r_clouds`, SJK only).
//!
//! The clouds are a layer [`BASE`] units above the camera and [`THICKNESS`] deep,
//! made of the weather noise volume ([`super::noise`]) and drifting with the wind. They
//! are drawn on the visible sky faces right after the sky (`clouds.wgsl`): each pixel
//! marches its view ray through the layer, with light from the map's sun (the day and
//! night clock's when it runs) and from the sky, and is blended over the sky in the
//! scene's light units. Rain and snow raise the cover and darken it. The layer moves
//! with the camera's height, so it stays overhead; across, it is fixed in the world, so
//! walking passes under it. Maps whose weather is space dust have none.

use bytemuck::{Pod, Zeroable};

/// Height of the layer's base above the camera, and its depth.
pub(crate) const BASE: f32 = 6000.0;
pub(crate) const THICKNESS: f32 = 3200.0;
/// Fair-weather cover, and how much a storm adds.
const FAIR_COVER: f32 = 0.42;
const STORM_COVER: f32 = 0.5;
/// Drift with no wind, units per second.
const CALM_DRIFT: [f32; 2] = [180.0, 60.0];
/// The noise tile across, where the drift wraps without a jump.
const DRIFT_WRAP: f64 = 36000.0;

/// The cloud uniform (`clouds.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct GpuClouds {
    pub(crate) sun: [f32; 4],
    pub(crate) sun_color: [f32; 4],
    pub(crate) ambient: [f32; 4],
    pub(crate) flow: [f32; 4],
    pub(crate) shape: [f32; 4],
}

/// The sky's light for the clouds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkyLight {
    /// Unit direction towards the sun.
    pub(crate) direction: [f32; 3],
    /// The sun's colour, 0..1.
    pub(crate) color: [f32; 3],
    /// 1 at the map's own sun, less towards night, 0 with the sun down.
    pub(crate) strength: f32,
    /// What one unit of the sky image is in the scene's light units.
    pub(crate) radiance: f32,
}

impl SkyLight {
    /// A map without an authored sun: a high, slightly warm light.
    pub(crate) const DEFAULT: Self = Self {
        direction: [0.37, 0.25, 0.89],
        color: [1.0, 0.96, 0.9],
        strength: 1.0,
        radiance: 1.0,
    };
}

/// The weather the clouds reflect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Sky {
    /// 0 fair to 1 a storm.
    pub(crate) storm: f32,
    /// Global wind, units per second.
    pub(crate) wind: [f32; 3],
}

/// The uniform for one frame. `flow` is the drift so far.
pub(crate) fn uniform(
    light: SkyLight,
    sky: Sky,
    flow: [f64; 2],
    steps: u32,
    ready: bool,
) -> GpuClouds {
    let storm = sky.storm.clamp(0.0, 1.0);
    let strength = light.strength.clamp(0.0, 1.5);
    let radiance = light.radiance.max(0.0);
    // Overcast light is grey and dimmer; night keeps a little moonlit sky.
    let sun = light.color.map(|channel| channel * 1.25 * radiance);
    let sky_light =
        [0.46, 0.52, 0.6].map(|channel| channel * radiance * (0.08 + 0.92 * strength.min(1.0)));
    GpuClouds {
        sun: [
            light.direction[0],
            light.direction[1],
            light.direction[2],
            strength * (1.0 - 0.7 * storm),
        ],
        sun_color: [
            sun[0],
            sun[1],
            sun[2],
            (FAIR_COVER + STORM_COVER * storm).min(0.95),
        ],
        ambient: [sky_light[0], sky_light[1], sky_light[2], storm],
        flow: [
            flow[0] as f32,
            flow[1] as f32,
            0.0,
            f32::from(u8::from(ready)),
        ],
        shape: [BASE, THICKNESS, steps as f32, 0.0],
    }
}

/// Advance the drift by `seconds` of `wind` (half its speed: the layer is high and
/// the wind there is not the ground's).
pub(crate) fn drift(flow: &mut [f64; 2], wind: [f32; 3], seconds: f32) {
    for axis in 0..2 {
        let speed = f64::from(CALM_DRIFT[axis] + 0.5 * wind[axis]);
        flow[axis] = (flow[axis] + speed * f64::from(seconds)).rem_euclid(DRIFT_WRAP);
    }
}

/// The cloud pipeline and its binding.
pub(crate) struct Gpu {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl Gpu {
    pub(crate) fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        noise: &super::noise::Texture,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK clouds layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK clouds uniform"),
            size: std::mem::size_of::<GpuClouds>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK clouds bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&noise.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&noise.sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK clouds pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK clouds shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("clouds.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SJK clouds pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[Some(crate::GpuVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // The sky faces' own culling (`sky_gpu.rs`).
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
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
            uniform,
            bind_group,
        }
    }

    pub(crate) fn write(&self, queue: &crate::frame_queue::FrameQueue, clouds: &GpuClouds) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(clouds));
    }

    /// Bind for the sky faces the caller then draws.
    pub(crate) fn bind<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storms_cover_and_darken_the_sky_and_night_dims_it() {
        let fair = uniform(SkyLight::DEFAULT, Sky::default(), [0.0; 2], 32, true);
        let storm = uniform(
            SkyLight::DEFAULT,
            Sky {
                storm: 1.0,
                wind: [0.0; 3],
            },
            [0.0; 2],
            32,
            true,
        );
        assert!(storm.sun_color[3] > fair.sun_color[3]);
        assert!(storm.sun[3] < fair.sun[3]);
        assert_eq!(storm.ambient[3], 1.0);
        let night = uniform(
            SkyLight {
                strength: 0.0,
                ..SkyLight::DEFAULT
            },
            Sky::default(),
            [0.0; 2],
            32,
            false,
        );
        assert_eq!(night.sun[3], 0.0);
        assert!(night.ambient[0] < fair.ambient[0] * 0.2);
        assert_eq!((fair.flow[3], night.flow[3]), (1.0, 0.0));
    }

    #[test]
    fn the_drift_follows_the_wind_and_wraps_at_the_noise_tile() {
        let mut flow = [0.0; 2];
        drift(&mut flow, [-5000.0, 0.0, 0.0], 1.0);
        assert!((flow[0] - (DRIFT_WRAP + f64::from(CALM_DRIFT[0]) - 2500.0)).abs() < 1e-6);
        for _ in 0..10_000 {
            drift(&mut flow, [3000.0, 3000.0, 0.0], 0.25);
        }
        assert!(flow.iter().all(|value| (0.0..DRIFT_WRAP).contains(value)));
    }

    #[test]
    fn the_shader_translates_for_vulkan_and_dx12() {
        use wgpu::naga;
        let source = include_str!("clouds.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{error:?}"));
        for (stage, entry) in [
            (naga::ShaderStage::Vertex, "vertex_main"),
            (naga::ShaderStage::Fragment, "fragment_main"),
        ] {
            naga::back::spv::write_vec(
                &module,
                &info,
                &naga::back::spv::Options::default(),
                Some(&naga::back::spv::PipelineOptions {
                    shader_stage: stage,
                    entry_point: entry.to_owned(),
                }),
            )
            .unwrap_or_else(|error| panic!("{entry} SPIR-V: {error:?}"));
            naga::back::hlsl::Writer::new(
                String::new(),
                &naga::back::hlsl::Options::default(),
                &naga::back::hlsl::PipelineOptions {
                    entry_point: Some((stage, entry.to_owned())),
                },
            )
            .write(&module, &info, None)
            .unwrap_or_else(|error| panic!("{entry} HLSL: {error:?}"));
        }
        assert_eq!(std::mem::size_of::<GpuClouds>(), 5 * 16);
    }
}

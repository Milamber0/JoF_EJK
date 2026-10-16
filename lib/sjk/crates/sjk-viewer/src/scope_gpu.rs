//! Fixed-capacity stock scope quads; textures are resolved once at world construction.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const MASK_IMAGE: &str = "gfx/2d/cropcircle2";

const PATHS: [&str; 5] = [
    "gfx/misc/scanline",
    // The mask stage's own image, looked up as an image (not through the
    // `gfx/2d/cropCircle2` shader, whose other stage drew a full-screen white
    // picture) in the engine's `.jpg`, `.png`, `.tga` order, so JoF's HD scope
    // (`JoF_HDWeaponScopeTrue.pk3`, `cropcircle2.png`) wins as in EternalJK.
    MASK_IMAGE,
    "gfx/2d/cropCircle",
    "gfx/2d/insertTick",
    "gfx/2d/crop_charge",
];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// Map-owned stock textures and a bounded batch of 24 scope quads.
pub(crate) struct Mask {
    pipeline: [wgpu::RenderPipeline; 3],
    textures: Vec<wgpu::BindGroup>,
    buffer: wgpu::Buffer,
    vertices: [Vertex; 144],
    materials: [usize; 24],
    count: usize,
}

impl Mask {
    /// Load retail shader images without requiring scope assets in custom-only installations.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        format: wgpu::TextureFormat,
        vfs: &sjk_vfs::VirtualFileSystem,
        shaders: &sjk_shader::ShaderCatalog,
    ) -> Option<Self> {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope textures"),
            entries: &[
                crate::render_helpers::texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut textures = Vec::with_capacity(5);
        for name in PATHS {
            let loaded = if name == MASK_IMAGE {
                shaders
                    .resolve_stage_image(vfs, name)
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
                    .and_then(|path| path.ok_or_else(|| format!("missing {name}").into()))
                    .and_then(|path| {
                        vfs.read(path.as_str())
                            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
                            .and_then(|asset| asset.ok_or_else(|| format!("missing {path}").into()))
                            .and_then(|asset| {
                                crate::decode_image(&asset.bytes, path.as_str())
                                    .map(|image| image.into_rgba8())
                                    .map_err(Into::into)
                            })
                    })
            } else if name.ends_with(".tga") {
                vfs.read(name)
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
                    .and_then(|asset| asset.ok_or_else(|| format!("missing {name}").into()))
                    .and_then(|asset| {
                        crate::decode_image(&asset.bytes, name)
                            .map(|image| image.into_rgba8())
                            .map_err(Into::into)
                    })
            } else {
                crate::shader_image::load_shader_image(vfs, shaders, name)
            };
            let image = match loaded {
                Ok(image) => image,
                Err(error) => {
                    eprintln!("scope unavailable: {error}");
                    return None;
                }
            };
            let texture = device.create_texture_with_data(
                queue.raw(),
                &wgpu::TextureDescriptor {
                    label: Some(name),
                    size: wgpu::Extent3d {
                        width: image.width(),
                        height: image.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: crate::ui_target::TEXTURE_FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                image.as_raw(),
            );
            textures.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(name),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            }));
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scope"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scope.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scope"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let filter = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Dst,
            dst_factor: wgpu::BlendFactor::Src,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = [
            wgpu::BlendState::ALPHA_BLENDING,
            wgpu::BlendState {
                color: add,
                alpha: wgpu::BlendComponent::OVER,
            },
            wgpu::BlendState {
                color: filter,
                alpha: wgpu::BlendComponent::OVER,
            },
        ]
        .map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scope"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &ATTRIBUTES,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scope quads"),
            size: (144 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(Self {
            pipeline,
            textures,
            buffer,
            vertices: [Vertex::zeroed(); 144],
            materials: [0; 24],
            count: 0,
        })
    }

    /// Rebuild the original 640x480 mask, rotating FOV insert, ammo ticks and charge readout.
    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        visible: bool,
        style: i64,
        fov: f32,
        time: i32,
        ammo: f32,
        charge: Option<f32>,
    ) {
        self.count = 0;
        if !visible {
            return;
        }
        let [mask, insert] = crate::cgame_options::scope_layers(style);
        if mask {
            self.quad(0, [320.0, 240.0, 640.0, 480.0], 0.0, [1.0; 4], 1.0);
            self.quad(1, [320.0, 240.0, 640.0, 480.0], 0.0, [1.0; 4], 1.0);
        }
        let level = ((50.0 - fov) / 50.0).clamp(0.0, 1.0) * 103.0;
        let alpha = if level >= 99.0 {
            0.7 + (time as f32 * 0.01).sin() * 0.3
        } else {
            1.0
        };
        if insert {
            self.quad(
                2,
                [320.0, 240.0, 640.0, 480.0],
                -level,
                [1.0, 1.0, 1.0, alpha],
                1.0,
            );
        }
        let ammo = ammo.clamp(0.0, 1.0);
        let color = if ammo < 0.15 && time & 512 != 0 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [
                ((1.0 - ammo) * 2.0).min(1.0),
                (ammo * 1.5).min(1.0),
                0.0,
                1.0,
            ]
        };
        let mut angle = 18.5;
        while angle <= 18.5 + ammo * 58.0 {
            let radians: f32 = (angle + 90.0) / 57.296;
            self.quad(
                3,
                [
                    320.0 + radians.sin() * 190.0,
                    240.0 + radians.cos() * 190.0,
                    12.0,
                    24.0,
                ],
                90.0 - angle,
                color,
                1.0,
            );
            angle += 3.0;
        }
        if let Some(charge) = charge {
            let charge = charge.clamp(0.0, 1.0);
            self.quad(
                4,
                [257.0 + 67.0 * charge, 452.0, 134.0 * charge, 34.0],
                0.0,
                [1.0; 4],
                charge,
            );
        }
        queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::cast_slice(&self.vertices[..self.count * 6]),
        );
    }

    fn quad(&mut self, material: usize, rect: [f32; 4], degrees: f32, color: [f32; 4], umax: f32) {
        if self.count == 24 {
            return;
        }
        let (sin, cos) = degrees.to_radians().sin_cos();
        for (index, [u, v]) in [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ]
        .into_iter()
        .enumerate()
        {
            let x = (u - 0.5) * rect[2];
            let y = (v - 0.5) * rect[3];
            self.vertices[self.count * 6 + index] = Vertex {
                position: [
                    (rect[0] + x * cos - y * sin) / 320.0 - 1.0,
                    1.0 - (rect[1] + x * sin + y * cos) / 240.0,
                ],
                uv: [u * umax, v * if material == 0 { 10.5 } else { 1.0 }],
                color,
            };
        }
        self.materials[self.count] = material;
        self.count += 1;
    }

    /// Draw after scene geometry and before text with no per-frame allocation.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for index in 0..self.count {
            let pipeline = match self.materials[index] {
                0 => 2,
                3 => 1,
                _ => 0,
            };
            pass.set_pipeline(&self.pipeline[pipeline]);
            pass.set_bind_group(0, &self.textures[self.materials[index]], &[]);
            pass.draw(index as u32 * 6..index as u32 * 6 + 6, 0..1);
        }
    }
}

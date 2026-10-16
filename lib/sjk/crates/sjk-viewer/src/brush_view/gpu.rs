//! GPU side of the invisible-brush view: the label tiles as a texture array,
//! one face and one edge pipeline, and the map's mesh in two vertex buffers.
//! Drawn in the overlay pass, which holds scene depth read-only.

use super::kinds::{Kind, KindSet};
use super::mesh::{Mesh, Vertex};
use super::tiles;
use bytemuck::{Pod, Zeroable};
use std::ops::Range;
use wgpu::util::DeviceExt;

/// Layout of `brush_view.wgsl`'s `Look`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Look {
    colours: [[f32; 4]; Kind::ALL.len()],
    opacity: [f32; 4],
}

/// Opacity of a face's fill, frame and label, and of its outline. One place to tune.
const OPACITY: [f32; 4] = [0.22, 0.55, 0.75, 0.85];

pub(super) struct Renderer {
    pub(super) format: wgpu::TextureFormat,
    faces: Pipeline,
    edges: Pipeline,
    bind_group: wgpu::BindGroup,
    /// Kinds this map has faces of.
    pub(super) present: KindSet,
}

struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    buffer: Option<wgpu::Buffer>,
    ranges: Vec<Range<u32>>,
}

impl Renderer {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        format: wgpu::TextureFormat,
        camera_layout: &wgpu::BindGroupLayout,
        anisotropy: u16,
        mesh: Mesh,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK invisible brushes shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../brush_view.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SJK invisible brushes"),
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
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
        let linear = format.is_srgb() || format == wgpu::TextureFormat::Rgba16Float;
        let look = Look {
            colours: Kind::ALL.map(|kind| {
                let [r, g, b] = kind.colour();
                if linear {
                    let linear = crate::ui_target::srgb_to_linear;
                    [linear(r), linear(g), linear(b), 1.0]
                } else {
                    [r, g, b, 1.0]
                }
            }),
            opacity: OPACITY,
        };
        let look = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SJK invisible brushes look"),
            contents: bytemuck::bytes_of(&look),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let tiles = tile_texture(device, queue);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK invisible brushes tiles"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: anisotropy.clamp(1, 8),
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SJK invisible brushes"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: look.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tiles.create_view(
                        &wgpu::TextureViewDescriptor {
                            dimension: Some(wgpu::TextureViewDimension::D2Array),
                            ..Default::default()
                        },
                    )),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK invisible brushes"),
            bind_group_layouts: &[Some(camera_layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point, topology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("SJK invisible brushes"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Float32x3,
                            2 => Uint32,
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    // Seen from inside a trigger, its far faces show too.
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DepthTarget::FORMAT,
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
        let present = Kind::ALL
            .into_iter()
            .filter(|kind| !mesh.face_ranges[kind.index()].is_empty())
            .fold(KindSet::EMPTY, |set, kind| set.with(kind));
        let buffer = |label, vertices: &[Vertex]| {
            (!vertices.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        };
        Self {
            format,
            faces: Pipeline {
                pipeline: pipeline("face_main", wgpu::PrimitiveTopology::TriangleList),
                buffer: buffer("SJK invisible brush faces", &mesh.faces),
                ranges: mesh.face_ranges,
            },
            edges: Pipeline {
                pipeline: pipeline("edge_main", wgpu::PrimitiveTopology::LineList),
                buffer: buffer("SJK invisible brush edges", &mesh.edges),
                ranges: mesh.edge_ranges,
            },
            bind_group,
            present,
        }
    }

    /// Draw the faces, then the outlines, of the kinds in `shown`.
    pub(super) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        shown: KindSet,
    ) {
        for part in [&self.faces, &self.edges] {
            let Some(buffer) = &part.buffer else {
                continue;
            };
            pass.set_pipeline(&part.pipeline);
            pass.set_bind_group(0, camera, &[]);
            pass.set_bind_group(1, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for kind in Kind::ALL {
                let range = &part.ranges[kind.index()];
                if shown.contains(kind) && !range.is_empty() {
                    pass.draw(range.clone(), 0..1);
                }
            }
        }
    }
}

/// The tiles of [`tiles::build`] with their mip chains, one layer per kind.
fn tile_texture(device: &wgpu::Device, queue: &crate::frame_queue::FrameQueue) -> wgpu::Texture {
    let layers = tiles::build();
    let chains: Vec<_> = layers
        .iter()
        .map(crate::gpu_texture::box_mip_chain)
        .collect();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK invisible brushes tiles"),
        size: wgpu::Extent3d {
            width: tiles::SIZE,
            height: tiles::SIZE,
            depth_or_array_layers: layers.len() as u32,
        },
        mip_level_count: chains[0].len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, chain) in chains.iter().enumerate() {
        for (level, image) in chain.iter().enumerate() {
            let (width, height) = image.dimensions();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                image.as_raw(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    texture
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_has_a_colour_per_kind() {
        let program = include_str!("../brush_view.wgsl");
        assert!(program.contains(&format!("const KINDS: u32 = {}u;", Kind::ALL.len())));
        // `Look` is one colour per kind and the opacities, all vec4.
        assert_eq!(std::mem::size_of::<Look>(), (Kind::ALL.len() + 1) * 16);
        assert_eq!(std::mem::size_of::<Vertex>(), 28);
    }
}

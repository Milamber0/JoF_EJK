//! The engine's model views without a world (`RDF_NOWORLDMODEL`): the menus' player
//! model, sabers and cosmetics. Each view's models are posed by the engine (its
//! Ghoul2 bones, its MD3 frames), skinned here on the CPU, drawn into a target of
//! the view's size with a camera from its `refdef_t`, and handed to the 2D layer,
//! which draws the target where the view was asked for in the run of pictures.
//!
//! The models are lit as rd-vanilla lights an entity when there is no world
//! (`R_SetupEntityLighting`: 150 ambient plus the 32 minimum, 150 directed from the
//! default sun direction), from each surface's shader image.

use super::assets;
use super::entities::{Entity, KIND_GHOUL2, KIND_MODEL, Scene};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use sjk_model::{Glm, Md3, Skin};
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::rc::Rc;

/// `sjkHostPreview_t`: one `RDF_NOWORLDMODEL` view and the entities it draws.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreviewView {
    pub(crate) origin: [f32; 3],
    pub(crate) axis: [[f32; 3]; 3],
    pub(crate) fov_x: f32,
    pub(crate) fov_y: f32,
    /// The view's rectangle in window pixels.
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) first_entity: i32,
    pub(crate) num_entities: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_projection: [[f32; 4]; 4],
    light_direction: [f32; 4],
    ambient: [f32; 4],
    directed: [f32; 4],
}

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const NEAR: f32 = 1.0;
const FAR: f32 = 8192.0;
/// `G2SURFACEFLAG_OFF` and `G2SURFACEFLAG_NODESCENDANTS`.
const SURFACE_OFF: u32 = 0x2;
const SURFACE_NO_DESCENDANTS: u32 = 0x100;

enum Model {
    Ghoul2(Glm),
    Md3(Md3),
}

/// A view's target, kept while views of its size keep coming.
struct Target {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind_group: Rc<wgpu::BindGroup>,
}

/// One draw: a run of the vertex buffer with one shader image.
struct Draw {
    texture: Rc<wgpu::BindGroup>,
    vertices: std::ops::Range<u32>,
}

pub(crate) struct Previews {
    device: wgpu::Device,
    pipeline: wgpu::RenderPipeline,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    /// What the 2D layer samples a finished view with.
    picture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: Rc<wgpu::BindGroup>,
    models: HashMap<i32, Option<Rc<Model>>>,
    textures: HashMap<String, Rc<wgpu::BindGroup>>,
    targets: Vec<Target>,
}

impl Previews {
    /// `picture_layout` is the 2D layer's texture-and-sampler layout.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        picture_layout: wgpu::BindGroupLayout,
    ) -> Self {
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("host model views"),
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
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("host model view images"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("host model views"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("host model views"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("host model views"),
            layout: Some(&layout),
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
                    format: COLOR,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("host model view images"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white_image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]));
        let white_view =
            crate::gpu_texture::upload_display_image(device, queue, "host model view white", &white_image);
        let white = Rc::new(texture_bind(device, &texture_layout, &sampler, &white_view));
        Self {
            device: device.clone(),
            pipeline,
            uniform_layout,
            texture_layout,
            picture_layout,
            sampler,
            white,
            models: HashMap::new(),
            textures: HashMap::new(),
            targets: Vec::new(),
        }
    }

    /// Draw every view; the result is each view's picture for the 2D layer, in order.
    pub(crate) fn render(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        scene: &Scene,
    ) -> Vec<Option<Rc<wgpu::BindGroup>>> {
        let mut out = Vec::with_capacity(scene.previews.len());
        if scene.previews.is_empty() {
            return out;
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("host model views"),
            });
        // Keep each view's buffers alive until the submission.
        let mut keep: Vec<(wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup)> = Vec::new();
        for (index, view) in scene.previews.iter().enumerate() {
            let size = [view.width.max(1) as u32, view.height.max(1) as u32];
            if size[0] > 4096 || size[1] > 4096 {
                out.push(None);
                continue;
            }
            self.target(index, size);
            let (vertices, draws) = self.geometry(queue, vfs, shaders, scene, view);
            let uniform = Uniform {
                view_projection: view_projection(view, size).to_cols_array_2d(),
                light_direction: Vec3::from_array(
                    crate::actor_instance::EntityLight::FALLBACK.direction,
                )
                .normalize()
                .extend(0.0)
                .to_array(),
                ambient: [182.0 / 255.0, 182.0 / 255.0, 182.0 / 255.0, 1.0],
                directed: [150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0, 1.0],
            };
            let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("host model view camera"),
                size: std::mem::size_of::<Uniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&uniform));
            let uniform_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("host model view camera"),
                layout: &self.uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });
            let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("host model view vertices"),
                size: (vertices.len().max(1) * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if !vertices.is_empty() {
                queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            }
            let target = &self.targets[index];
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("host model view"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.color,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &uniform_bind, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                for draw in &draws {
                    pass.set_bind_group(1, draw.texture.as_ref(), &[]);
                    pass.draw(draw.vertices.clone(), 0..1);
                }
            }
            out.push(Some(target.bind_group.clone()));
            keep.push((uniform_buffer, vertex_buffer, uniform_bind));
        }
        queue.submit(Some(encoder.finish()));
        drop(keep);
        out
    }

    fn target(&mut self, index: usize, size: [u32; 2]) {
        if self.targets.get(index).is_some_and(|target| target.size == size) {
            return;
        }
        let texture = |format, usage, label| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let color = texture(
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            "host model view",
        );
        let depth = texture(DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT, "host model view depth");
        let bind_group = Rc::new(texture_bind(
            &self.device,
            &self.picture_layout,
            &self.sampler,
            &color,
        ));
        let target = Target {
            size,
            color,
            depth,
            bind_group,
        };
        if index < self.targets.len() {
            self.targets[index] = target;
        } else {
            self.targets.push(target);
        }
    }

    /// The view's triangles in world space, grouped by image.
    fn geometry(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        scene: &Scene,
        view: &PreviewView,
    ) -> (Vec<Vertex>, Vec<Draw>) {
        let mut vertices = Vec::new();
        let mut draws: Vec<Draw> = Vec::new();
        let first = usize::try_from(view.first_entity).unwrap_or(0);
        let count = usize::try_from(view.num_entities).unwrap_or(0);
        let Some(entities) = scene.entities.get(first..first + count) else {
            return (vertices, draws);
        };
        for entity in entities {
            if entity.kind != KIND_GHOUL2 && entity.kind != KIND_MODEL {
                continue;
            }
            let Some(model) = self.model(vfs, entity.model) else {
                continue;
            };
            let skin = (entity.skin > 0)
                .then(|| assets::skin_lines(entity.skin))
                .flatten()
                .map(|lines| Skin::parse(lines.as_bytes()));
            let custom = (entity.shader != 0)
                .then(|| assets::shader_name(entity.shader))
                .flatten();
            let mut push = |shader: &str, triangles: Vec<Vertex>, this: &mut Self| {
                if triangles.is_empty() || shader.eq_ignore_ascii_case("*off") {
                    return;
                }
                let texture = this.texture(queue, vfs, shaders, shader);
                let start = vertices.len() as u32;
                vertices.extend(triangles);
                let end = vertices.len() as u32;
                match draws.last_mut() {
                    Some(last) if Rc::ptr_eq(&last.texture, &texture) && last.vertices.end == start => {
                        last.vertices.end = end;
                    }
                    _ => draws.push(Draw {
                        texture,
                        vertices: start..end,
                    }),
                }
            };
            match model.as_ref() {
                Model::Ghoul2(glm) => {
                    let hidden = hidden_surfaces(glm, scene, entity);
                    let bone = |index: usize| -> Option<Mat4> {
                        let first = usize::try_from(entity.first_bone).ok()?;
                        let rows = scene.bones.get(first + index)?;
                        Some(Mat4::from_cols(
                            Vec4::new(rows[0][0], rows[1][0], rows[2][0], 0.0),
                            Vec4::new(rows[0][1], rows[1][1], rows[2][1], 0.0),
                            Vec4::new(rows[0][2], rows[1][2], rows[2][2], 0.0),
                            Vec4::new(rows[0][3], rows[1][3], rows[2][3], 1.0),
                        ))
                    };
                    let place = placement(entity, false);
                    let Some(lod) = glm.lods.first() else {
                        continue;
                    };
                    for surface in &lod.surfaces {
                        let Some(hierarchy) = glm.hierarchy.get(surface.hierarchy_index) else {
                            continue;
                        };
                        if hidden.get(surface.hierarchy_index).copied().unwrap_or(false) {
                            continue;
                        }
                        let shader = custom.clone().unwrap_or_else(|| {
                            skin.as_ref()
                                .and_then(|skin| skin.shader(&hierarchy.name))
                                .map_or_else(|| hierarchy.shader.clone(), str::to_owned)
                        });
                        let skinned: Vec<(Vec3, Vec3)> = surface
                            .vertices
                            .iter()
                            .map(|vertex| {
                                let mut position = Vec3::ZERO;
                                let mut normal = Vec3::ZERO;
                                for weight in &vertex.weights {
                                    let Some(matrix) = surface
                                        .bone_references
                                        .get(weight.bone_reference)
                                        .and_then(|&index| bone(index))
                                    else {
                                        continue;
                                    };
                                    position += weight.weight
                                        * matrix.transform_point3(Vec3::from_array(vertex.position));
                                    normal += weight.weight
                                        * matrix.transform_vector3(Vec3::from_array(vertex.normal));
                                }
                                (
                                    place.transform_point3(position),
                                    place.transform_vector3(normal).normalize_or_zero(),
                                )
                            })
                            .collect();
                        let triangles = surface
                            .triangles
                            .iter()
                            .flat_map(|triangle| triangle.iter())
                            .filter_map(|&index| {
                                let (position, normal) = *skinned.get(index as usize)?;
                                Some(Vertex {
                                    position: position.to_array(),
                                    normal: normal.to_array(),
                                    uv: surface.vertices[index as usize].texture_coordinates,
                                })
                            })
                            .collect();
                        push(&shader, triangles, self);
                    }
                }
                Model::Md3(md3) => {
                    let place = placement(entity, true);
                    let frames = md3.frames.len().max(1);
                    let frame = usize::try_from(entity.frame).unwrap_or(0) % frames;
                    let old = usize::try_from(entity.oldframe).unwrap_or(0) % frames;
                    let back = entity.backlerp.clamp(0.0, 1.0);
                    for surface in &md3.surfaces {
                        let shader = custom.clone().unwrap_or_else(|| {
                            skin.as_ref()
                                .and_then(|skin| skin.shader(&surface.name))
                                .map(str::to_owned)
                                .or_else(|| surface.shaders.first().cloned())
                                .unwrap_or_default()
                        });
                        let (Some(now), Some(before)) =
                            (surface.frames.get(frame), surface.frames.get(old))
                        else {
                            continue;
                        };
                        let triangles = surface
                            .triangles
                            .iter()
                            .flat_map(|triangle| triangle.iter())
                            .filter_map(|&index| {
                                let index = index as usize;
                                let (a, b) = (now.get(index)?, before.get(index)?);
                                let position = Vec3::from_array(a.position)
                                    .lerp(Vec3::from_array(b.position), back);
                                let normal = Vec3::from_array(a.normal)
                                    .lerp(Vec3::from_array(b.normal), back);
                                Some(Vertex {
                                    position: place.transform_point3(position).to_array(),
                                    normal: place.transform_vector3(normal).normalize_or_zero().to_array(),
                                    uv: *surface.texture_coordinates.get(index)?,
                                })
                            })
                            .collect();
                        push(&shader, triangles, self);
                    }
                }
            }
        }
        (vertices, draws)
    }

    fn model(&mut self, vfs: &VirtualFileSystem, handle: i32) -> Option<Rc<Model>> {
        self.models
            .entry(handle)
            .or_insert_with(|| {
                let name = assets::model_name(handle)?;
                let bytes = vfs.read(&name).ok().flatten()?.bytes;
                let lower = name.to_ascii_lowercase();
                if lower.ends_with(".glm") {
                    Glm::parse(&bytes).ok().map(|glm| Rc::new(Model::Ghoul2(glm)))
                } else if lower.ends_with(".md3") {
                    Md3::parse(&bytes).ok().map(|md3| Rc::new(Model::Md3(md3)))
                } else {
                    None
                }
            })
            .clone()
    }

    /// A shader's picture: its first stage's image, or the image of its name.
    fn texture(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        shader: &str,
    ) -> Rc<wgpu::BindGroup> {
        let key = shader.to_ascii_lowercase();
        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }
        let texture = crate::shader_image::load_shader_image(vfs, shaders, shader)
            .ok()
            .map(|rgba| {
                let view = crate::gpu_texture::upload_display_image(&self.device, queue, shader, &rgba);
                Rc::new(texture_bind(&self.device, &self.texture_layout, &self.sampler, &view))
            })
            .unwrap_or_else(|| self.white.clone());
        self.textures.insert(key, texture.clone());
        texture
    }
}

/// The entity's model-to-world transform: its axis (scaled for an MD3 by
/// `modelScale`; Ghoul2 has the scale in its bones) and origin.
fn placement(entity: &Entity, scaled: bool) -> Mat4 {
    let scale = if scaled { Vec3::from_array(entity.scale) } else { Vec3::ONE };
    Mat4::from_cols(
        (Vec3::from_array(entity.axis[0]) * scale.x).extend(0.0),
        (Vec3::from_array(entity.axis[1]) * scale.y).extend(0.0),
        (Vec3::from_array(entity.axis[2]) * scale.z).extend(0.0),
        Vec3::from_array(entity.origin).extend(1.0),
    )
}

/// The GLM surfaces not drawn: off in the model or by the engine, or below a
/// surface that hides its descendants.
fn hidden_surfaces(glm: &Glm, scene: &Scene, entity: &Entity) -> Vec<bool> {
    let mut flags: Vec<u32> = glm.hierarchy.iter().map(|surface| surface.flags).collect();
    let first = usize::try_from(entity.first_surface).unwrap_or(0);
    let pairs = usize::try_from(entity.num_surfaces).unwrap_or(0);
    for pair in scene.ints.get(first..first + pairs * 2).unwrap_or(&[]).chunks_exact(2) {
        if let Some(slot) = usize::try_from(pair[0]).ok().and_then(|index| flags.get_mut(index)) {
            *slot = pair[1] as u32;
        }
    }
    (0..glm.hierarchy.len())
        .map(|index| {
            if flags[index] & (SURFACE_OFF | sjk_model::GLM_SURFACE_BOLT) != 0 {
                return true;
            }
            let mut parent = glm.hierarchy[index].parent;
            while let Some(up) = parent {
                if flags.get(up).is_some_and(|flag| flag & SURFACE_NO_DESCENDANTS != 0) {
                    return true;
                }
                parent = glm.hierarchy.get(up).and_then(|surface| surface.parent);
            }
            false
        })
        .collect()
}

/// Quake's camera for the view: x right (`-axis[1]`), y up, depth along
/// `axis[0]`, the field of view from `fov_x` and `fov_y`, depth 0..1 from near to far.
fn view_projection(view: &PreviewView, _size: [u32; 2]) -> Mat4 {
    let origin = Vec3::from_array(view.origin);
    let forward = Vec3::from_array(view.axis[0]);
    let right = -Vec3::from_array(view.axis[1]);
    let up = Vec3::from_array(view.axis[2]);
    let tan_x = (view.fov_x.clamp(1.0, 179.0).to_radians() * 0.5).tan();
    let tan_y = (view.fov_y.clamp(1.0, 179.0).to_radians() * 0.5).tan();
    let a = FAR / (FAR - NEAR);
    let b = -NEAR * FAR / (FAR - NEAR);
    let rows = [
        (right / tan_x).extend(-right.dot(origin) / tan_x),
        (up / tan_y).extend(-up.dot(origin) / tan_y),
        (forward * a).extend(-forward.dot(origin) * a + b),
        forward.extend(-forward.dot(origin)),
    ];
    Mat4::from_cols(rows[0], rows[1], rows[2], rows[3]).transpose()
}

fn texture_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("host model view image"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

const SHADER: &str = r"
struct Camera {
    view_projection: mat4x4<f32>,
    light_direction: vec4<f32>,
    ambient: vec4<f32>,
    directed: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> Out {
    var out: Out;
    out.position = camera.view_projection * vec4(position, 1.0);
    out.normal = normal;
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(input: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(image, image_sampler, input.uv);
    if texel.a < 0.5 {
        discard;
    }
    let facing = max(dot(normalize(input.normal), camera.light_direction.xyz), 0.0);
    let light = min(camera.ambient.rgb + camera.directed.rgb * facing, vec3(1.0));
    return vec4(texel.rgb * light, 1.0);
}
";

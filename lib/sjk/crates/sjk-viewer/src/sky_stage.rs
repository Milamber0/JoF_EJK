//! Production wgpu path for Quake 3 outer sky boxes and sky depth masks.
//!
//! The full-box simplification replaces `RB_ClipSkyPolygons` subdivision for
//! normal, non-mirror views. The box is camera-relative and forced to maximum
//! depth, then visible BSP sky polygons write their true depth. This preserves
//! `RB_StageIteratorSky`'s no-parallax and no-bleed behavior without rebuilding
//! box geometry per frame (`codemp/rd-vanilla/tr_sky.cpp:792-849`).

use super::{DrawBatch, GpuVertex, ViewerMaterial};
use bytemuck::{Pod, Zeroable};
use image::RgbaImage;
use sjk_bsp::Visibility;
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;
use std::ops::Range;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[path = "day_sky.rs"]
mod day;
#[cfg(test)]
#[path = "sky_face_depth_tests.rs"]
mod face_depth_tests;
#[path = "sky_gpu.rs"]
mod gpu;
#[path = "sky_remaps.rs"]
mod remaps;

const FACE_SUFFIXES: [&str; 6] = ["rt", "lf", "bk", "ft", "up", "dn"];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SkyVertex {
    position: [f32; 3],
    uv: [f32; 2],
    layer: f32,
}

impl SkyVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[derive(Clone)]
struct SkyDraw {
    indices: Range<u32>,
    clusters: Vec<usize>,
}

struct SkyMaterial {
    name: String,
    remapped: Option<String>,
    bind_group: Option<wgpu::BindGroup>,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
    draws: Vec<SkyDraw>,
    missing_faces: Vec<String>,
}

/// Immutable map-lifetime sky resources. `draw` allocates no heap storage.
pub(crate) struct Runtime {
    /// First authored sun on a sky material used by this map; no invented fallback direction.
    pub(crate) sun: Option<sjk_shader::SunParms>,
    materials: Vec<SkyMaterial>,
    box_pipeline: wgpu::RenderPipeline,
    authored_box_pipeline: wgpu::RenderPipeline,
    mask_pipeline: wgpu::RenderPipeline,
    /// The box's colour on the sky faces themselves; see `draw_faces_shaded`.
    /// What one unit of the sky's image is in the scene's light units; the day program's
    /// pipelines are built with it.
    pub(crate) radiance: f32,
    face_pipeline: gpu::FacePipelines,
    authored_face_pipeline: gpu::FacePipelines,
    day_clock: Option<(wgpu::Buffer, wgpu::BindGroup)>,
}

impl Runtime {
    /// Build sky textures, fixed box geometry, and the BSP depth-mask list.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        camera_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        materials: &[ViewerMaterial],
        draws: &[DrawBatch],
    ) -> Result<Self, Box<dyn Error>> {
        let texture_layout = gpu::texture_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SJK Q3 sky stage"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky_stage.wgsl").into()),
        });
        let box_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK Q3 sky-box pipeline layout"),
            bind_group_layouts: &[Some(camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let mask_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SJK Q3 sky-mask pipeline layout"),
            bind_group_layouts: &[Some(camera_layout)],
            immediate_size: 0,
        });
        let box_pipeline = gpu::box_pipeline(device, &box_layout, &shader, color_format, 1.);
        let mask_pipeline = gpu::mask_pipeline(device, &mask_layout, &shader, color_format);
        let face_pipeline = gpu::face_pipeline(device, &box_layout, &shader, color_format, 1.);
        let mut sky_materials = Vec::new();
        let mut sun = None;
        for (material_index, material) in materials.iter().enumerate() {
            let Some(definition) = shaders
                .get(&material.shader)
                .filter(|definition| definition.sky.is_some())
            else {
                continue;
            };
            let sky = definition.sky.as_ref().expect("filtered sky definition");
            let sky_draws: Vec<SkyDraw> = draws
                .iter()
                .filter(|draw| draw.world_surface && draw.material == material_index)
                .map(|draw| SkyDraw {
                    indices: draw.indices.clone(),
                    clusters: draw.clusters.clone(),
                })
                .collect();
            if sun.is_none() && !sky_draws.is_empty() {
                sun = definition.sun;
            }
            let (bind_group, vertex_buffer, vertex_count, missing_faces) =
                sky.outer_box.as_deref().map_or_else(
                    || Ok((None, None, 0, Vec::new())),
                    |prefix| load_box(device, queue, &texture_layout, vfs, shaders, prefix),
                )?;
            sky_materials.push(SkyMaterial {
                name: material.shader.clone(),
                remapped: None,
                bind_group,
                vertex_buffer,
                vertex_count,
                draws: sky_draws,
                missing_faces,
            });
        }
        for material in &sky_materials {
            println!(
                "loaded sky {}: {} BSP draw ranges, {} box vertices",
                material.name,
                material.draws.len(),
                material.vertex_count
            );
            for face in &material.missing_faces {
                eprintln!(
                    "sky {} missing outer-box face {face}; face skipped",
                    material.name
                );
            }
        }
        Ok(Self {
            sun,
            materials: sky_materials,
            authored_box_pipeline: box_pipeline.clone(),
            box_pipeline,
            mask_pipeline,
            authored_face_pipeline: face_pipeline.clone(),
            face_pipeline,
            radiance: 1.,
            day_clock: None,
        })
    }

    /// Draw visible boxes at maximum depth, then write actual sky polygon depth.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        world_vertices: &'pass wgpu::Buffer,
        world_indices: &'pass wgpu::Buffer,
        source_cluster: Option<usize>,
        visibility: Option<&Visibility>,
        areas: &crate::world_materials::areas::Areas,
    ) {
        if self.materials.is_empty() {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_pipeline(&self.box_pipeline);
        if let Some((_, binding)) = &self.day_clock {
            pass.set_bind_group(2, binding, &[]);
        }
        for material in &self.materials {
            if !material
                .draws
                .iter()
                .any(|d| areas.visible(&d.clusters, source_cluster, visibility))
            {
                continue;
            }
            let (Some(bind_group), Some(vertices)) =
                (&material.bind_group, &material.vertex_buffer)
            else {
                continue;
            };
            pass.set_bind_group(1, bind_group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.draw(0..material.vertex_count, 0..1);
        }
        self.draw_mask(
            pass,
            camera,
            world_vertices,
            world_indices,
            source_cluster,
            visibility,
            areas,
        );
    }

    /// The main view's sky: each visible sky face shaded with its material's box, looked
    /// up by view direction, depth-tested and depth-writing, after the opaque world.
    /// The box drawn first shaded the whole frame (eleven million pixels at render scale
    /// 2) for the little sky a room shows; here hidden sky is rejected before shading,
    /// and whatever lies behind a sky face is still covered by it.
    pub(crate) fn draw_faces_shaded<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        world_vertices: &'pass wgpu::Buffer,
        world_indices: &'pass wgpu::Buffer,
        source_cluster: Option<usize>,
        visibility: Option<&Visibility>,
        areas: &crate::world_materials::areas::Areas,
    ) {
        if self.materials.is_empty() {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_pipeline(&self.face_pipeline.single);
        if let Some((_, binding)) = &self.day_clock {
            pass.set_bind_group(2, binding, &[]);
        }
        pass.set_vertex_buffer(0, world_vertices.slice(..));
        pass.set_index_buffer(world_indices.slice(..), wgpu::IndexFormat::Uint32);
        for material in &self.materials {
            let Some(bind_group) = &material.bind_group else {
                continue;
            };
            let mut bound = false;
            for draw in &material.draws {
                if !areas.visible(&draw.clusters, source_cluster, visibility) {
                    continue;
                }
                if !std::mem::replace(&mut bound, true) {
                    pass.set_bind_group(1, bind_group, &[]);
                }
                pass.draw_indexed(draw.indices.clone(), 0, 0..1);
            }
        }
    }

    /// The visible sky faces with the caller's pipeline and groups: a sky portal's colour
    /// is composited on them, depth-tested, instead of over the whole frame.
    pub(crate) fn draw_faces<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_vertices: &'pass wgpu::Buffer,
        world_indices: &'pass wgpu::Buffer,
        identity_instance: &'pass wgpu::Buffer,
        source_cluster: Option<usize>,
        visibility: Option<&Visibility>,
        areas: &crate::world_materials::areas::Areas,
    ) {
        pass.set_vertex_buffer(0, world_vertices.slice(..));
        pass.set_vertex_buffer(1, identity_instance.slice(..));
        pass.set_index_buffer(world_indices.slice(..), wgpu::IndexFormat::Uint32);
        for material in &self.materials {
            for draw in &material.draws {
                if areas.visible(&draw.clusters, source_cluster, visibility) {
                    pass.draw_indexed(draw.indices.clone(), 0, 0..1);
                }
            }
        }
    }

    /// Retain sky depth occlusion when a map sky portal supplies the colour.
    pub(crate) fn draw_mask<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        world_vertices: &'pass wgpu::Buffer,
        world_indices: &'pass wgpu::Buffer,
        source_cluster: Option<usize>,
        visibility: Option<&Visibility>,
        areas: &crate::world_materials::areas::Areas,
    ) {
        pass.set_bind_group(0, camera, &[]);
        pass.set_pipeline(&self.mask_pipeline);
        pass.set_vertex_buffer(0, world_vertices.slice(..));
        pass.set_index_buffer(world_indices.slice(..), wgpu::IndexFormat::Uint32);
        for material in &self.materials {
            for draw in &material.draws {
                if areas.visible(&draw.clusters, source_cluster, visibility) {
                    pass.draw_indexed(draw.indices.clone(), 0, 0..1);
                }
            }
        }
    }
}

fn load_box(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    prefix: &str,
) -> Result<LoadedBox, Box<dyn Error>> {
    let mut images = Vec::with_capacity(6);
    let mut present = [false; 6];
    let mut missing = Vec::new();
    for (axis, suffix) in FACE_SUFFIXES.iter().enumerate() {
        let name = format!("{prefix}_{suffix}");
        if let Some(path) = shaders.resolve_stage_image(vfs, &name)? {
            if let Some(image) =
                crate::decoded_image_cache::cached_decoded_image(vfs, path.as_str())?
            {
                images.push(image);
                present[axis] = true;
            } else {
                images.push(missing_face());
                missing.push(name);
            }
        } else {
            images.push(missing_face());
            missing.push(name);
        }
    }
    let texture = super::world_materials::upload_array(device, queue, &images)?;
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("SJK Q3 sky clamp sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK Q3 sky-box bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let vertices = box_vertices(present);
    let vertex_count = u32::try_from(vertices.len())?;
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK Q3 sky-box vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    Ok((Some(bind_group), Some(vertex_buffer), vertex_count, missing))
}

fn missing_face() -> Arc<RgbaImage> {
    Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([0; 4])))
}

type LoadedBox = (
    Option<wgpu::BindGroup>,
    Option<wgpu::Buffer>,
    u32,
    Vec<String>,
);

fn box_vertices(present: [bool; 6]) -> Vec<SkyVertex> {
    let mut vertices = Vec::with_capacity(36);
    let corners = [
        (-1.0, -1.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ];
    for (axis, available) in present.into_iter().enumerate() {
        if !available {
            continue;
        }
        vertices.extend(corners.into_iter().map(|(s, t)| SkyVertex {
            position: make_sky_vec(s, t, axis),
            uv: [(s + 1.0) * 0.5, 1.0 - (t + 1.0) * 0.5],
            layer: axis as f32,
        }));
    }
    vertices
}

fn make_sky_vec(s: f32, t: f32, axis: usize) -> [f32; 3] {
    const ST_TO_VEC: [[i8; 3]; 6] = [
        [3, -1, 2],
        [-3, 1, 2],
        [1, 3, 2],
        [-1, -3, 2],
        [-2, -1, 3],
        [2, -1, -3],
    ];
    let base = [s * 1024.0, t * 1024.0, 1024.0];
    std::array::from_fn(|component| {
        let mapping = ST_TO_VEC[axis][component];
        let value = base[usize::from(mapping.unsigned_abs()) - 1];
        if mapping < 0 { -value } else { value }
    })
}

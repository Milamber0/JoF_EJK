//! GPU resource helpers for the map-lifetime Q3 world-material runtime.

use crate::world_stage::PipelineKey;
use crate::{ActorInstance, GpuVertex, create_rgba8_texture};
use image::{RgbaImage, imageops};
use sjk_bsp::Bsp;
use sjk_shader::{ShaderCatalog, ShaderStage, TextureGenerator};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use crate::decoded_image_cache::cached_decoded_image;

pub(super) fn create_stage_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK Q3 world stage layout"),
        entries: &stage_layout_entries(),
    })
}

/// The per-stage group's entries; the material-map layout extends them.
pub(super) fn stage_layout_entries() -> [wgpu::BindGroupLayoutEntry; 8] {
    [
        wgpu::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        array_texture_layout_entry(0),
        sampler_layout_entry(1),
        array_texture_layout_entry(2),
        sampler_layout_entry(3),
        crate::texture_layout_entry(4),
        wgpu::BindGroupLayoutEntry {
            binding: 5,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 6,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]
}

fn array_texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub(super) fn create_sampler(device: &wgpu::Device, clamp: bool) -> wgpu::Sampler {
    let address = if clamp {
        wgpu::AddressMode::ClampToEdge
    } else {
        wgpu::AddressMode::Repeat
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("SJK Q3 stage sampler"),
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

pub(super) fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    key: PipelineKey,
    instanced: bool,
) -> wgpu::RenderPipeline {
    create_pipeline_for_vertex(
        device,
        layout,
        shader,
        format,
        key,
        instanced,
        if instanced {
            "instanced_vertex_main"
        } else {
            "vertex_main"
        },
        "fragment_main",
        if instanced {
            "SJK Q3 inline-model stage pipeline"
        } else {
            "SJK Q3 static-world stage pipeline"
        },
        true,
    )
}

pub(super) fn create_entity_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    key: PipelineKey,
    depth_test: bool,
) -> wgpu::RenderPipeline {
    create_pipeline_for_vertex(
        device,
        layout,
        shader,
        format,
        key,
        true,
        "entity_vertex_main",
        "fragment_main",
        "SJK Q3 skinned/rigid entity stage pipeline",
        depth_test,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn create_pipeline_for_vertex(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    key: PipelineKey,
    instanced: bool,
    vertex_entry: &str,
    fragment_entry: &str,
    label: &str,
    depth_test: bool,
) -> wgpu::RenderPipeline {
    let blend = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: key.source,
            dst_factor: key.destination,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: key.source,
            dst_factor: key.destination,
            operation: wgpu::BlendOperation::Add,
        },
    };
    let static_buffers = [Some(GpuVertex::layout())];
    let instanced_buffers = [Some(GpuVertex::layout()), Some(ActorInstance::layout())];
    let buffers = if instanced {
        &instanced_buffers[..]
    } else {
        &static_buffers[..]
    };
    // Bit 8 is a depth bias only; the vertex shader sees the first three bits and the
    // fragment shader bits 4 and 16.
    let specialization = [
        ("geometry_deforms", f64::from(key.geometry & 1 != 0)),
        ("geometry_sprites", f64::from(key.geometry & 2 != 0)),
    ];
    let fragment_specialization = [
        ("visible_emission", f64::from(key.geometry & 4 != 0)),
        (
            "forced_entity_alpha",
            f64::from(key.geometry & crate::world_stage::FORCED_ALPHA != 0),
        ),
    ];
    // Ordinary pipelines pass no constants; `forced_entity_alpha` only ever comes with
    // `visible_emission` in the list, so a stage-program module always declares both.
    let fragment_constants = if key.geometry & crate::world_stage::FORCED_ALPHA != 0 {
        0..2
    } else if key.geometry & 4 != 0 {
        0..1
    } else {
        0..0
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex_entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: if key.geometry & 7 == 0 {
                    &[]
                } else {
                    &specialization
                },
                ..Default::default()
            },
            buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &fragment_specialization[fragment_constants],
                ..Default::default()
            },
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: key.cull,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DepthTarget::FORMAT,
            depth_write_enabled: Some(depth_test && key.depth_write),
            depth_compare: Some(if depth_test {
                key.depth
            } else {
                wgpu::CompareFunction::Always
            }),
            stencil: Default::default(),
            bias: if key.geometry & crate::world_stage::POLYGON_OFFSET != 0 {
                crate::world_stage::POLYGON_OFFSET_BIAS
            } else {
                Default::default()
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn load_stage_images(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    stage: &ShaderStage,
    implicit_name: &str,
    cache: &mut HashMap<String, Arc<RgbaImage>>,
) -> Result<(Vec<Arc<RgbaImage>>, bool, String), Box<dyn Error>> {
    if stage.texture_generator == TextureGenerator::Lightmap
        || stage
            .images
            .iter()
            .any(|image| image.eq_ignore_ascii_case("$lightmap"))
    {
        return Ok((
            vec![Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])))],
            false,
            "$lightmap-placeholder".into(),
        ));
    }
    // `videoMap`: the video's first picture now, the rest as it plays (`world_videos.rs`).
    if let Some(video) = &stage.video {
        let path = super::videos::path(video);
        if let Some(image) = super::videos::first_frame(vfs, &path) {
            return Ok((vec![image], true, super::videos::key(&path)));
        }
        crate::log::progress(format_args!("videoMap {path}: not found"));
        return Ok((vec![missing_image()], false, format!("$missing:{path};")));
    }
    let names: Vec<&str> = if stage.images.is_empty() {
        vec![implicit_name]
    } else {
        stage.images.iter().map(String::as_str).collect()
    };
    let mut images = Vec::with_capacity(names.len());
    let mut any = false;
    let mut identity = String::new();
    for name in names {
        if name.eq_ignore_ascii_case("$whiteimage") || name.eq_ignore_ascii_case("*white") {
            images.push(Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]))));
            identity.push_str("$white;");
            continue;
        }
        let path = if stage.images.is_empty() {
            shaders.resolve_image(vfs, name)?
        } else {
            shaders.resolve_stage_image(vfs, name)?
        };
        if let Some(path) = path {
            let key = path.as_str().to_ascii_lowercase();
            identity.push_str(&key);
            identity.push(';');
            if let Some(image) = cache.get(&key) {
                images.push(Arc::clone(image));
                any = true;
                continue;
            }
            if let Some(image) = cached_decoded_image(vfs, path.as_str())? {
                cache.insert(key, Arc::clone(&image));
                images.push(image);
                any = true;
                continue;
            }
        }
        identity.push_str("$missing:");
        identity.push_str(name);
        identity.push(';');
        images.push(missing_image());
    }
    Ok((images, any, identity))
}

/// The magenta checker a stage shows for an image it cannot find.
fn missing_image() -> Arc<RgbaImage> {
    Arc::new(
        RgbaImage::from_raw(
            2,
            2,
            vec![
                255, 0, 255, 255, 32, 32, 32, 255, 32, 32, 32, 255, 255, 0, 255, 255,
            ],
        )
        .expect("fixture dimensions"),
    )
}

pub(crate) fn upload_array(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    images: &[Arc<RgbaImage>],
) -> Result<wgpu::TextureView, Box<dyn Error>> {
    let width = images.iter().map(|image| image.width()).max().unwrap_or(1);
    let height = images.iter().map(|image| image.height()).max().unwrap_or(1);
    let layers = u32::try_from(images.len().max(1))?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK Q3 stage animation frames"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, image) in images.iter().enumerate() {
        let resized;
        let pixels = if image.width() == width && image.height() == height {
            image.as_raw()
        } else {
            resized = imageops::resize(
                image.as_ref(),
                width,
                height,
                imageops::FilterType::Lanczos3,
            );
            resized.as_raw()
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: u32::try_from(layer)?,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
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
    Ok(texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("SJK Q3 stage animation array view"),
        format: None,
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        usage: None,
        aspect: wgpu::TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    }))
}

pub(crate) fn upload_lightmaps(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    bsp: &Bsp,
) -> Result<HashMap<i32, wgpu::TextureView>, Box<dyn Error>> {
    let mut result = HashMap::new();
    for index in 0..bsp.render().lightmap_count() {
        let pixels = bsp
            .render()
            .lightmap(index)
            .ok_or("invalid parsed lightmap")?;
        let mut rgba = Vec::with_capacity(128 * 128 * 4);
        for rgb in pixels.chunks_exact(3) {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        result.insert(
            i32::try_from(index)?,
            create_rgba8_texture(
                device,
                queue,
                "SJK Q3 stage lightmap",
                128,
                128,
                &rgba,
                true,
            ),
        );
    }
    Ok(result)
}

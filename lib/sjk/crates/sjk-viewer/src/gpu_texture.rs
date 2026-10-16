//! Shared image decoding and immutable RGBA texture upload helpers.

/// Decode a supported renderer image using its extension when available.
pub(crate) fn decode_image(
    bytes: &[u8],
    path: &str,
) -> Result<image::DynamicImage, image::ImageError> {
    let format = path.rsplit_once('.').and_then(|(_, extension)| {
        match extension.to_ascii_lowercase().as_str() {
            "tga" => Some(image::ImageFormat::Tga),
            "jpg" | "jpeg" => Some(image::ImageFormat::Jpeg),
            "png" => Some(image::ImageFormat::Png),
            _ => None,
        }
    });
    format.map_or_else(
        || image::load_from_memory(bytes),
        |format| image::load_from_memory_with_format(bytes, format),
    )
}

/// Upload one two-dimensional RGBA image and return its default view.
pub(crate) fn create_rgba8_texture(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    label: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
    srgb: bool,
) -> wgpu::TextureView {
    let format = if srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
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
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Upload an RGBA image with a CPU-built mip chain of `levels` levels so
/// minified sampling (small UI text from a large glyph atlas) stays smooth.
pub(crate) fn create_rgba8_texture_mipmapped(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    label: &str,
    image: &image::RgbaImage,
    srgb: bool,
    levels: u32,
) -> wgpu::TextureView {
    let format = if srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    let (width, height) = image.dimensions();
    let levels = levels.clamp(1, width.min(height).max(1).ilog2() + 1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level_image;
    let mut level_pixels: &image::RgbaImage = image;
    for level in 0..levels {
        if level > 0 {
            let (w, h) = level_pixels.dimensions();
            let next = image::imageops::resize(
                level_pixels,
                (w / 2).max(1),
                (h / 2).max(1),
                image::imageops::FilterType::Triangle,
            );
            level_image = next;
            level_pixels = &level_image;
        }
        let (w, h) = level_pixels.dimensions();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            level_pixels.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Resolve, decode, and upload one blended-effect shader image during map installation.
/// The texels stay the display values they are stored as: rd-vanilla samples them without
/// any decode and blends them in display space (`effect_layer.rs`).
pub(crate) fn load_shader_texture(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    vfs: &sjk_vfs::VirtualFileSystem,
    shaders: &sjk_shader::ShaderCatalog,
    shader: &str,
) -> Result<wgpu::TextureView, Box<dyn std::error::Error>> {
    let rgba = crate::shader_image::load_shader_image(vfs, shaders, shader)?;
    Ok(upload_display_image(device, queue, shader, &rgba))
}

/// Box-filtered mip chain of an RGBA image down to 1×1, as rd-vulkan's `R_MipMap` builds it
/// with its default `r_simpleMipMaps 1` (`vk_image_process.cpp:262`): each texel is the
/// truncated mean of its 2×2 parents (2×1 once one side has reached 1).
pub(crate) fn box_mip_chain(image: &image::RgbaImage) -> Vec<image::RgbaImage> {
    let mut chain = vec![image.clone()];
    loop {
        let parent = chain.last().expect("chain starts with level 0");
        let (width, height) = parent.dimensions();
        if width == 1 && height == 1 {
            return chain;
        }
        let (next_width, next_height) = ((width / 2).max(1), (height / 2).max(1));
        let next = image::RgbaImage::from_fn(next_width, next_height, |x, y| {
            let (x0, y0) = ((2 * x).min(width - 1), (2 * y).min(height - 1));
            let (x1, y1) = ((2 * x + 1).min(width - 1), (2 * y + 1).min(height - 1));
            let quad = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
            // One side already 1: R_MipMap averages the two neighbours along the other.
            let texels: &[(u32, u32)] = match (width, height) {
                (1, _) => &[quad[0], quad[2]],
                (_, 1) => &quad[..2],
                _ => &quad,
            };
            let shift = texels.len().trailing_zeros();
            image::Rgba(std::array::from_fn(|channel| {
                let sum: u32 = texels
                    .iter()
                    .map(|&(tx, ty)| u32::from(parent.get_pixel(tx, ty)[channel]))
                    .sum();
                (sum >> shift) as u8
            }))
        });
        chain.push(next);
    }
}

/// Upload a decoded effect image as stored display values with rd-vulkan's box mip chain
/// (`box_mip_chain`), for effects that stock samples with `r_textureMode`'s mipmaps.
pub(crate) fn upload_display_image_mipmapped(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    label: &str,
    rgba: &image::RgbaImage,
) -> wgpu::TextureView {
    upload_levels(device, queue, label, &box_mip_chain(rgba))
}

/// Upload an `Rgba8Unorm` texture whose mip levels were made by the caller; `chain[0]`
/// is the full-size image and each next one half its size.
pub(crate) fn upload_levels(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    label: &str,
    chain: &[image::RgbaImage],
) -> wgpu::TextureView {
    let rgba = &chain[0];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: rgba.width(),
            height: rgba.height(),
            depth_or_array_layers: 1,
        },
        mip_level_count: chain.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, pixels) in chain.iter().enumerate() {
        let (width, height) = pixels.dimensions();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels.as_raw(),
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
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Upload a decoded effect image as stored display values (no sRGB decode).
pub(crate) fn upload_display_image(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    label: &str,
    rgba: &image::RgbaImage,
) -> wgpu::TextureView {
    create_rgba8_texture(
        device,
        queue,
        label,
        rgba.width(),
        rgba.height(),
        rgba.as_raw(),
        false,
    )
}

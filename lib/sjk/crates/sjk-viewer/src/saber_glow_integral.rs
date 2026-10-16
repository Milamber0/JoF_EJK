//! Column integral of a saber glow image, for the smooth glow capsule in `saber.wgsl`.
//!
//! Stock's glow is a chain of overlapping sprites of this image (RB_SurfaceSaberGlow,
//! rd-vanilla tr_surface.cpp:470-490). Integrated along the blade, their sum at a point
//! needs the mean of an image column over a range of rows, which this texture answers
//! with two samples: row `j` holds the mean of rows `0..j` of each column (row 0 is zero),
//! so the mean over rows `[a, b)` is `(P(b) - P(a)) · H / (b - a)`.

use wgpu::util::DeviceExt;

/// Prefix means as RGBA16F texels: `width × (height + 1)`, row-major.
pub(crate) fn texels(image: &image::RgbaImage) -> Vec<u16> {
    let (width, height) = image.dimensions();
    let mut sums = vec![[0.0_f32; 3]; width as usize];
    let mut texels = Vec::with_capacity((width * (height + 1) * 4) as usize);
    let scale = 1.0 / (255.0 * height.max(1) as f32);
    for row in 0..=height {
        for (x, sum) in sums.iter_mut().enumerate() {
            texels.extend(sum.map(|c| half::f16::from_f32(c).to_bits()));
            texels.push(half::f16::from_f32(1.0).to_bits());
            if row < height {
                let pixel = image.get_pixel(x as u32, row);
                for (channel, value) in sum.iter_mut().enumerate() {
                    *value += f32::from(pixel[channel]) * scale;
                }
            }
        }
    }
    texels
}

/// Upload [`texels`] once per material at map installation.
pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    image: &image::RgbaImage,
) -> wgpu::TextureView {
    let (width, height) = image.dimensions();
    device
        .create_texture_with_data(
            queue.raw(),
            &wgpu::TextureDescriptor {
                label: Some("SJK saber glow column integral"),
                size: wgpu::Extent3d {
                    width,
                    height: height + 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&texels(image)),
        )
        .create_view(&Default::default())
}

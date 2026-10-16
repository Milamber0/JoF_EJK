//! The map preview's own texture: the current levelshot at the resolution it
//! ships in, with its mip chain, so a full-screen loading backdrop or a large
//! preview is sampled from the source pixels rather than magnified from a
//! small atlas slot, and a small preview is minified without aliasing.

use crate::menu::levelshot::LevelshotImage;
use sjk_ui::TextureId;

/// `TexturedQuad` texture naming the map preview.
pub(crate) const LEVELSHOT_TEXTURE: TextureId = TextureId(u32::MAX - 1);

/// Texture, view and bind group of the current levelshot. They are rebuilt
/// only when a levelshot of another size arrives (a map change), never per
/// frame.
pub(super) struct LevelshotTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    size: [u32; 2],
    levels: u32,
}

impl LevelshotTexture {
    /// A transparent 1x1 placeholder until the first levelshot is uploaded.
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SJK levelshot sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let (texture, bind_group) = create(device, layout, &sampler, [1, 1], 1);
        Self {
            texture,
            bind_group,
            sampler,
            size: [1, 1],
            levels: 1,
        }
    }

    pub(super) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// Upload `image` with every mip level, resizing the texture to it first
    /// when its size or level count differs from the current one.
    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        image: &LevelshotImage,
    ) {
        let levels = image.levels.len() as u32;
        if levels == 0 || image.size.contains(&0) {
            return;
        }
        if image.size != self.size || levels != self.levels {
            (self.texture, self.bind_group) =
                create(device, layout, &self.sampler, image.size, levels);
            self.size = image.size;
            self.levels = levels;
        }
        for (level, pixels) in image.levels.iter().enumerate() {
            let [width, height] = level_size(image.size, level as u32);
            if pixels.len() != (width * height * 4) as usize {
                return;
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: level as u32,
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
        }
    }
}

/// Size of mip `level` of a `size` texture.
pub(crate) fn level_size(size: [u32; 2], level: u32) -> [u32; 2] {
    [(size[0] >> level).max(1), (size[1] >> level).max(1)]
}

fn create(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    size: [u32; 2],
    levels: u32,
) -> (wgpu::Texture, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK levelshot"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::ui_target::TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK levelshot bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (texture, bind_group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_sizes_halve_and_stop_at_one() {
        assert_eq!(level_size([2048, 1024], 0), [2048, 1024]);
        assert_eq!(level_size([2048, 1024], 1), [1024, 512]);
        assert_eq!(level_size([2048, 1024], 10), [2, 1]);
        assert_eq!(level_size([2048, 1024], 11), [1, 1]);
    }

    #[test]
    fn decoded_levels_match_the_texture_levels() {
        let image = LevelshotImage::from_rgba(image::RgbaImage::new(2048, 1024));
        for (level, pixels) in image.levels.iter().enumerate() {
            let [width, height] = level_size(image.size, level as u32);
            assert_eq!(pixels.len(), (width * height * 4) as usize, "level {level}");
        }
    }
}

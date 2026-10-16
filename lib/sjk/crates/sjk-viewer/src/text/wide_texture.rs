//! The GPU copies of the wide-glyph atlas ([`super::wide`]) and the emoji atlas
//! ([`super::emoji`]), and the text bind groups that carry them.
//!
//! Every text bind group holds its font's own atlas at binding 0, the sampler at 1,
//! the wide atlas at 2 and the emoji atlas at 3; `text.wgsl` samples the wide atlas for
//! vertices whose `u` is [`super::wide::U_OFFSET`] or more and the emoji atlas for
//! those at [`super::emoji::U_OFFSET`] or more.

use super::wide::Cell;
use super::{emoji, wide};
use crate::frame_queue::FrameQueue;

/// The wide atlas (R8 coverage) and the emoji atlas (RGBA display values) on the GPU,
/// with the code-page atlases' mip levels.
pub(crate) struct WideTexture {
    wide: wgpu::Texture,
    wide_view: wgpu::TextureView,
    emoji: wgpu::Texture,
    emoji_view: wgpu::TextureView,
    /// Reused list of cells to copy this frame.
    cells: Vec<Cell>,
}

fn atlas_texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: super::ATLAS_MIP_LEVELS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

impl WideTexture {
    /// Empty atlases. The CPU atlases start over with them, so every wide glyph and
    /// emoji on screen is drawn into these textures over the next frames.
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let wide = atlas_texture(
            device,
            "SJK wide glyph atlas",
            [wide::ATLAS_WIDTH, wide::ATLAS_HEIGHT],
            wgpu::TextureFormat::R8Unorm,
        );
        // Display values, as every picture the 2D layer samples (`ui_target`).
        let emoji = atlas_texture(
            device,
            "SJK emoji atlas",
            [emoji::ATLAS_WIDTH, emoji::ATLAS_HEIGHT],
            crate::ui_target::TEXTURE_FORMAT,
        );
        wide::start_over();
        emoji::start_over();
        Self {
            wide_view: wide.create_view(&wgpu::TextureViewDescriptor::default()),
            wide,
            emoji_view: emoji.create_view(&wgpu::TextureViewDescriptor::default()),
            emoji,
            cells: Vec::new(),
        }
    }

    /// Rasterize the glyphs and decode the emoji this frame asked for (within
    /// [`wide::FRAME_BUDGET`] for both) and copy every new cell, with its mip levels,
    /// into the textures ahead of the next submission. Nothing is copied or allocated on
    /// a frame that asked for nothing new.
    pub(crate) fn update(&mut self, queue: &FrameQueue) {
        let deadline = std::time::Instant::now() + wide::FRAME_BUDGET;
        wide::rasterize_pending(wide::FRAME_BUDGET);
        emoji::rasterize_pending(deadline);
        wide::take_dirty(&mut self.cells);
        if !self.cells.is_empty() {
            let cells = &self.cells;
            wide::mirror(|pixels| {
                upload(queue, &self.wide, cells, 1, |cell, level| {
                    wide::cell_level(pixels, cell, level)
                });
            });
            self.cells.clear();
        }
        emoji::take_dirty(&mut self.cells);
        if !self.cells.is_empty() {
            let cells = &self.cells;
            emoji::mirror(|pixels| {
                upload(queue, &self.emoji, cells, 4, |cell, level| {
                    emoji::cell_level(pixels, cell, level)
                });
            });
            self.cells.clear();
        }
    }
}

/// Copy `cells` of `texture` (`texel` bytes a texel) at every mip level, each level's
/// texels from `level_of`.
fn upload(
    queue: &FrameQueue,
    texture: &wgpu::Texture,
    cells: &[Cell],
    texel: u32,
    level_of: impl Fn(Cell, u32) -> Vec<u8>,
) {
    for &cell in cells {
        for level in 0..super::ATLAS_MIP_LEVELS {
            let data = level_of(cell, level);
            let (width, height) = (cell.width >> level, cell.height >> level);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: level,
                    origin: wgpu::Origin3d {
                        x: cell.x >> level,
                        y: cell.y >> level,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * texel),
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

/// A text bind group: `atlas` (the font's own), the text sampler, and the wide and
/// emoji atlases of `shared`.
pub(crate) fn bind_group(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::BindGroupLayout,
    atlas: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    shared: &WideTexture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(atlas),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&shared.wide_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&shared.emoji_view),
            },
        ],
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_text_program_validates_and_shares_the_offsets() {
        let source = include_str!("../text.wgsl");
        crate::wgsl_source::validate(source);
        for declared in [
            format!("const WIDE_U_OFFSET: f32 = {:.1};", super::wide::U_OFFSET),
            format!("const EMOJI_U_OFFSET: f32 = {:.1};", super::emoji::U_OFFSET),
        ] {
            assert!(source.contains(&declared), "{declared}");
        }
    }

    /// The game chat in other alphabets and emoji as they arrive: Windows-1251 from an
    /// EternalJK player, UTF-8 from SJK players (Russian, Japanese, Chinese, Korean,
    /// emoji with skin tones, a family and flags), and a draft typed in them, in Inter
    /// and in the game fonts (SJK Chat), at 1080p and at 4K.
    #[test]
    #[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
    fn chat_in_russian_and_japanese() {
        for (size, game_font, suffix) in [
            ([1920, 1080], "0", "1080p"),
            ([3840, 2160], "0", "4k"),
            ([1920, 1080], "1", "1080p-game-font"),
        ] {
            crate::world_shot::on_big_stack(move || {
                let Some((mut gpu, _profile)) = crate::world_shot::open(
                    "maps/mp/duel6.bsp",
                    size,
                    None,
                    &[("cg_chatbox", "60000"), (crate::game_font::CVAR, game_font)],
                ) else {
                    return;
                };
                gpu.chat.for_shot = true;
                let lines: [&[u8]; 7] = [
                    // An EternalJK player with `se_language russian`: Windows-1251.
                    b"^1Vasya^7: \xef\xf0\xe8\xe2\xe5\xf2, \xea\xf2\xee \xed\xe0 \xe4\xf3\xfd\xeb\xfc?",
                    "^5Sol^7: Да, иду! Ёлки-палки, ПРИВЕТ всем".as_bytes(),
                    "^3Kenji^7: こんにちは、日本語のチャットです。漢字もカタカナも OK".as_bytes(),
                    "^2Wei^7: 你好，我们这里说中文。繁體字也可以".as_bytes(),
                    "^4Minji^7: 안녕하세요! 한국어 채팅 ㅋㅋㅋ".as_bytes(),
                    "^6Emoji^7: gg 😀🔥👍🏽 ❤\u{FE0F} 👨\u{200D}👩\u{200D}👧 🇫🇷🇯🇵 1\u{FE0F}\u{20E3}".as_bytes(),
                    "^2Mixed^7: café ½ €5 © ™ привет 日本 한국".as_bytes(),
                ];
                for line in lines {
                    gpu.chat.receive(
                        sjk_client::ServerEventKind::Chat,
                        sjk_client::decode_legacy(line).into_owned(),
                        None,
                        std::time::Instant::now(),
                    );
                }
                gpu.chat.draft_for_shot("пишу 日本語 中文 한국어 🎉");
                let path =
                    crate::world_shot::shoot(&mut gpu, 10, &format!("chat-unicode-{suffix}"));
                println!("{}", path.display());
            });
        }
    }
}

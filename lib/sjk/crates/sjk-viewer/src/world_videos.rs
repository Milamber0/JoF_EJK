//! `videoMap` stages: a RoQ video ([`crate::cinematic_roq`]) playing on a surface, as
//! rd-vanilla's `CIN_PlayCinematic(..., CIN_loop | CIN_silent | CIN_shader)` and
//! `R_UploadCinematic` play one: looping, without sound, at the video's own rate.
//!
//! Load gives the stage the video's first frame as its image under a key of its own
//! ([`key`]), so a material compiles like any other. The forge then makes that key's
//! texture one updatable layer and keeps it here ([`Videos::register`]); every frame
//! [`Videos::update`] decodes what the clock asks for and uploads the newest picture.
//! A missing or unreadable video keeps the missing-image checker, as a missing image
//! does.

use crate::cinematic_roq::Roq;
use image::RgbaImage;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Texture keys of video stages start with this; nothing else does.
pub(crate) const KEY_PREFIX: &str = "$video:";
/// Most frames one update decodes; a longer gap (a hitch, a loading pause) skips ahead.
const MAX_STEPS: u32 = 6;

/// The file a `videoMap` argument names (`CIN_PlayCinematic`): a bare name lives in
/// `video/`, and a name without an extension is a `.roq`.
pub(crate) fn path(argument: &str) -> String {
    let argument = argument.replace('\\', "/");
    let mut path = if argument.contains('/') {
        argument
    } else {
        format!("video/{argument}")
    };
    let file = path.rsplit('/').next().unwrap_or_default();
    if !file.contains('.') {
        path.push_str(".roq");
    }
    path.to_ascii_lowercase()
}

/// The texture key of a video stage's image.
pub(crate) fn key(path: &str) -> String {
    format!("{KEY_PREFIX}{path}")
}

/// The video's first picture, for the stage's image at load; `None` when the file is
/// missing or not a RoQ video.
pub(crate) fn first_frame(vfs: &VirtualFileSystem, path: &str) -> Option<Arc<RgbaImage>> {
    let asset = vfs.read(path).ok().flatten()?;
    let mut video = match Roq::open(asset.bytes.into()) {
        Ok(video) => video,
        Err(error) => {
            crate::log::progress(format_args!("videoMap {path}: {error}"));
            return None;
        }
    };
    if !video.advance() {
        return None;
    }
    let mut pixels = vec![0; (video.width() * video.height() * 4) as usize];
    video.write_rgba(&mut pixels);
    RgbaImage::from_raw(video.width(), video.height(), pixels).map(Arc::new)
}

/// The single-layer texture a video stage samples: its first picture, updatable.
pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    image: &RgbaImage,
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: image.width().max(1),
        height: image.height().max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SJK videoMap frame"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write(queue, &texture, image.as_raw(), size);
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("SJK videoMap frame view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (texture, view)
}

fn write(
    queue: &crate::frame_queue::FrameQueue,
    texture: &wgpu::Texture,
    pixels: &[u8],
    size: wgpu::Extent3d,
) {
    queue.write_texture(
        texture.as_image_copy(),
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.width * 4),
            rows_per_image: Some(size.height),
        },
        size,
    );
}

/// One playing video and the texture it updates.
struct Video {
    path: String,
    texture: wgpu::Texture,
    /// Opened on the first update; `None` after a failure, which stops trying.
    player: Option<Roq>,
    opened: bool,
    started: Instant,
    pixels: Vec<u8>,
}

/// The map's playing videos, one per distinct file.
#[derive(Default)]
pub(crate) struct Videos {
    videos: Vec<Video>,
}

impl Videos {
    /// Keep `texture` (made by [`upload`]) updated with the video of `key`.
    pub(crate) fn register(&mut self, key: &str, texture: wgpu::Texture) {
        let Some(path) = key.strip_prefix(KEY_PREFIX) else {
            return;
        };
        if self.videos.iter().any(|video| video.path == path) {
            return;
        }
        self.videos.push(Video {
            path: path.to_owned(),
            texture,
            player: None,
            opened: false,
            started: Instant::now(),
            pixels: Vec::new(),
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.videos.is_empty()
    }

    /// Decode each video up to `now` and upload its newest picture. No allocation once
    /// a video is open.
    pub(crate) fn update(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        now: Instant,
    ) {
        for video in &mut self.videos {
            if !video.opened {
                video.opened = true;
                video.player = vfs
                    .read(&video.path)
                    .ok()
                    .flatten()
                    .and_then(|asset| Roq::open(asset.bytes.into()).ok());
                video.started = now;
                if let Some(player) = &video.player {
                    video.pixels = vec![0; (player.width() * player.height() * 4) as usize];
                }
            }
            let Some(player) = video.player.as_mut() else {
                continue;
            };
            let elapsed = now.saturating_duration_since(video.started).as_secs_f64();
            let wanted = (elapsed * f64::from(player.fps())) as u64 + 1;
            let mut steps = 0;
            while player.frames() < wanted && steps < MAX_STEPS {
                if !player.advance() {
                    break;
                }
                steps += 1;
            }
            if player.frames() < wanted {
                // Too far behind: carry on from here rather than racing to catch up.
                let shown = player.frames().saturating_sub(1) as f64 / f64::from(player.fps());
                video.started = now
                    .checked_sub(Duration::from_secs_f64(shown.max(0.0)))
                    .unwrap_or(now);
            }
            if steps == 0 {
                continue;
            }
            player.write_rgba(&mut video.pixels);
            let size = video.texture.size();
            if size.width == player.width() && size.height == player.height() {
                write(queue, &video.texture, &video.pixels, size);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_cin_play_cinematic() {
        assert_eq!(path("ja04"), "video/ja04.roq");
        assert_eq!(path("video/ja11"), "video/ja11.roq");
        assert_eq!(path("movies/mk_arcade.roq"), "movies/mk_arcade.roq");
        assert_eq!(
            path("textures\\Faru\\HoloNews1"),
            "textures/faru/holonews1.roq"
        );
        assert_eq!(key("video/ja04.roq"), "$video:video/ja04.roq");
    }
}

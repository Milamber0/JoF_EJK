//! GPU side of the classic menu artwork ([`crate::menu::art`]): one texture
//! and bind group per retail image, outside the shared icon atlas.
//!
//! The pieces are full-screen backgrounds and frames (up to 1024 square in
//! retail, more in HD packs); the atlas is sized for 128-pixel icons, and its
//! cells are spoken for. A texture per
//! piece keeps the atlas untouched and lets HD replacements keep their
//! resolution. The shape renderer switches bind groups between draw runs
//! only where a textured quad names a different source, so the draw order
//! of every layer is preserved.
//!
//! Animated pieces ([`ArtPiece::dynamic`]) keep their texture writable: a
//! flickering glow is recomposed on the CPU at most once per frame and only
//! on frames that draw the piece ([`ArtTextures::animate`]). Pieces whose
//! texture scrolls are sampled with wrapping ([`ArtPiece::wraps`]).

use crate::menu::art::{ArtPiece, ArtSet, Decoded, motion};
use crate::menu::emblem::EmblemLayer;
use image::RgbaImage;

/// How an animated piece rewrites its texture.
enum Motion {
    /// A glow recomposed with the static noise ([`motion::compose_flicker`]).
    Flicker {
        base: &'static RgbaImage,
        noise: &'static RgbaImage,
        speeds: [f32; 4],
        pixels: Vec<u8>,
    },
}

impl Motion {
    /// The picture to upload.
    fn pixels(&self) -> &[u8] {
        match self {
            Self::Flicker { pixels, .. } => pixels,
        }
    }

    /// Step to menu time `now`; true when the picture changed.
    fn step(&mut self, now: f64) -> bool {
        match self {
            Self::Flicker {
                base,
                noise,
                speeds,
                pixels,
            } => {
                motion::compose_flicker(base, noise, *speeds, now, pixels);
                true
            }
        }
    }
}

/// A writable piece texture and how it animates.
struct Animated {
    piece: ArtPiece,
    texture: wgpu::Texture,
    size: [u32; 2],
    motion: Motion,
}

/// Per-piece bind groups of the classic menu artwork.
pub(super) struct ArtTextures {
    groups: [Option<wgpu::BindGroup>; ArtPiece::COUNT],
    ready: ArtSet,
    installed: bool,
    animated: Vec<Animated>,
    /// Pieces already animated this frame.
    stepped: ArtSet,
}

impl ArtTextures {
    pub(super) fn new() -> Self {
        Self {
            groups: std::array::from_fn(|_| None),
            ready: ArtSet::default(),
            installed: false,
            animated: Vec::new(),
            stepped: ArtSet::default(),
        }
    }

    /// Whether [`Self::install`] has run (whatever it found).
    pub(super) fn installed(&self) -> bool {
        self.installed
    }

    /// Pieces that can be drawn.
    pub(super) fn ready(&self) -> ArtSet {
        self.ready
    }

    /// The bind group sampling `piece`, if it was installed.
    pub(super) fn group(&self, piece: ArtPiece) -> Option<&wgpu::BindGroup> {
        self.groups[piece.index()].as_ref()
    }

    /// Start a frame: every animated piece may step once again.
    pub(super) fn begin_frame(&mut self) {
        self.stepped = ArtSet::default();
    }

    /// Step `piece`'s animation to menu time `now` and upload its new
    /// picture, once per frame; static pieces are left alone.
    pub(super) fn animate(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        piece: ArtPiece,
        now: f64,
    ) {
        if !piece.dynamic() || self.stepped.has(piece) {
            return;
        }
        self.stepped = self.stepped.with(piece);
        let Some(animated) = self.animated.iter_mut().find(|item| item.piece == piece) else {
            return;
        };
        if animated.motion.step(now) {
            write(
                queue,
                &animated.texture,
                animated.size,
                animated.motion.pixels(),
            );
        }
    }

    /// Upload every decoded piece into its own texture.
    pub(super) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        decoded: &'static Decoded,
    ) {
        self.installed = true;
        let sampler = |address_mode, label| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: address_mode,
                address_mode_v: address_mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let clamped = sampler(
            wgpu::AddressMode::ClampToEdge,
            "SJK classic menu art sampler",
        );
        let wrapping = sampler(
            wgpu::AddressMode::Repeat,
            "SJK classic menu art wrapping sampler",
        );
        let now = motion::seconds();
        for piece in ArtPiece::ALL {
            let animated = animated_piece(piece, decoded, now);
            let (pixels, size): (&[u8], [u32; 2]) = match &animated {
                Some((motion, size)) => (motion.pixels(), *size),
                None => {
                    let Some(image) = decoded.image(piece) else {
                        continue;
                    };
                    (image.as_raw(), [image.width(), image.height()])
                }
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("SJK classic menu art"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::ui_target::TEXTURE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            write(queue, &texture, size, pixels);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = if piece.wraps() { &wrapping } else { &clamped };
            self.groups[piece.index()] =
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("SJK classic menu art bind group"),
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
                }));
            if let Some((motion, size)) = animated {
                self.animated.push(Animated {
                    piece,
                    texture,
                    size,
                    motion,
                });
            }
            self.ready = self.ready.with(piece);
        }
    }
}

/// The motion of `piece`, with its first picture made, if it animates and
/// game data has what it needs.
fn animated_piece(
    piece: ArtPiece,
    decoded: &'static Decoded,
    now: f64,
) -> Option<(Motion, [u32; 2])> {
    let speeds = motion::flicker(piece)?;
    let (base, noise) = decoded.flicker_base(piece)?;
    let mut pixels = vec![0_u8; (base.width() * base.height() * 4) as usize];
    motion::compose_flicker(base, noise, speeds, now, &mut pixels);
    Some((
        Motion::Flicker {
            base,
            noise,
            speeds,
            pixels,
        },
        [base.width(), base.height()],
    ))
}

/// Replace the whole of `texture` (`size`, RGBA) with `pixels`.
fn write(
    queue: &crate::frame_queue::FrameQueue,
    texture: &wgpu::Texture,
    size: [u32; 2],
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 4),
            rows_per_image: Some(size[1]),
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
}

/// Texture source of one run of shape vertices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Source {
    /// The shared icon atlas (also bound for untextured shapes).
    Atlas,
    /// One classic menu art piece.
    Art(ArtPiece),
    /// One layer of SJK's menu emblem.
    Emblem(EmblemLayer),
    /// One medal's whole picture.
    Medal(crate::medals::Medal),
    /// One card face's picture, by its position among the faces.
    Card(u16),
    /// One SJK chat GIF's texture, by its slot.
    Gif(u8),
    /// The map preview's own texture.
    Levelshot,
    /// The classic profile's model preview.
    Preview,
    /// The settings' HUD picker preview.
    HudPreview,
}

impl Source {
    /// Whether the run draws with the additive pipeline: the emblem's glow
    /// and ray layers. Everything else is alpha blended.
    pub(super) fn additive(self) -> bool {
        matches!(self, Self::Emblem(layer) if layer.additive())
    }
}

/// A run of consecutive vertices drawn with one bind group.
#[derive(Clone, Copy, Debug)]
pub(super) struct Run {
    pub(super) start: u32,
    pub(super) source: Source,
}

/// Most bind-group switches one frame may make, across every layer; art quads
/// past it are dropped rather than growing the run list on the frame path.
/// The classic Force page alone switches about three times per power (holocron
/// from the icon atlas, then the retail star art), which ran past the old 48
/// and dropped the last powers' pictures.
pub(crate) const MAX_RUNS: usize = 512;

/// Begin a new run for `source` at vertex `start` unless the current run
/// already samples it. Untextured shapes never call this: they draw the same
/// under any bind group. Returns false when the run list is full.
pub(super) fn switch(runs: &mut Vec<Run>, start: usize, source: Source) -> bool {
    if let Some(run) = runs.last_mut() {
        if run.source == source {
            return true;
        }
        // An empty run can simply change source.
        if run.start as usize == start {
            run.source = source;
            return true;
        }
    }
    if runs.len() >= MAX_RUNS {
        return false;
    }
    runs.push(Run {
        start: start as u32,
        source,
    });
    true
}

/// An untextured shape (a fill, a ring, an arc) at vertex `start`. Shapes draw
/// the same under any alpha-blended bind group, so they stay in the current run,
/// but not in the emblem light's additive one: a gold button drawn after the
/// emblem in the same list (the in-game menu's card) added itself onto the
/// scene and showed it through. Back to the alpha-blended atlas.
pub(super) fn untextured(runs: &mut Vec<Run>, start: usize) {
    if runs.last().is_some_and(|run| run.source.additive()) {
        let _ = switch(runs, start, Source::Atlas);
    }
}

/// Begin the next layer (draw list) at vertex `start`. Its untextured shapes
/// stay in the current run, so a layer that ended on the emblem's light
/// would draw the next one's additively: a pop-up card's dark glass and scrim
/// over the SJK UI's main page added nothing. Back to the alpha-blended atlas.
pub(super) fn begin_layer(runs: &mut Vec<Run>, start: usize) {
    if runs.last().is_some_and(|run| run.source.additive()) {
        let _ = switch(runs, start, Source::Atlas);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_never_inherits_the_light_blend() {
        let mut runs = vec![Run {
            start: 0,
            source: Source::Atlas,
        }];
        assert!(switch(&mut runs, 6, Source::Emblem(EmblemLayer::Lights)));
        begin_layer(&mut runs, 12);
        assert_eq!(
            runs.last().map(|run| (run.start, run.source)),
            Some((12, Source::Atlas))
        );
        // An alpha-blended run carries on.
        assert!(switch(&mut runs, 18, Source::Art(ArtPiece::Background)));
        begin_layer(&mut runs, 24);
        assert_eq!(runs.len(), 4);
        assert_eq!(runs[3].source, Source::Art(ArtPiece::Background));
    }

    #[test]
    fn a_shape_after_the_light_is_alpha_blended() {
        let mut runs = vec![Run {
            start: 0,
            source: Source::Atlas,
        }];
        assert!(switch(&mut runs, 6, Source::Emblem(EmblemLayer::Lights)));
        untextured(&mut runs, 12);
        assert_eq!(
            runs.last().map(|run| (run.start, run.source)),
            Some((12, Source::Atlas))
        );
        // Under an alpha-blended picture a shape stays in its run.
        assert!(switch(&mut runs, 18, Source::Art(ArtPiece::Background)));
        untextured(&mut runs, 24);
        assert_eq!(runs.len(), 4);
    }

    #[test]
    fn runs_switch_only_on_a_new_source() {
        let mut runs = Vec::with_capacity(MAX_RUNS);
        runs.push(Run {
            start: 0,
            source: Source::Atlas,
        });
        // A source change before any vertex reuses the empty run.
        assert!(switch(&mut runs, 0, Source::Art(ArtPiece::Background)));
        assert_eq!(runs.len(), 1);
        assert!(switch(&mut runs, 6, Source::Art(ArtPiece::Background)));
        assert_eq!(runs.len(), 1);
        assert!(switch(&mut runs, 12, Source::Atlas));
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].start, 12);
        while runs.len() < MAX_RUNS {
            // Alternate against the current run so every call starts one.
            let source = if runs.last().expect("run").source == Source::Atlas {
                Source::Art(ArtPiece::Ring)
            } else {
                Source::Atlas
            };
            let start = 100 + runs.len() * 6;
            assert!(switch(&mut runs, start, source));
        }
        let last = runs.last().expect("run").source;
        let other = if last == Source::Atlas {
            Source::Art(ArtPiece::Logo)
        } else {
            Source::Atlas
        };
        assert!(!switch(&mut runs, 10_000, other));
        assert!(switch(&mut runs, 10_000, last));
    }

    #[test]
    fn only_the_emblem_glows_draw_additively() {
        assert!(!Source::Atlas.additive());
        assert!(!Source::Levelshot.additive());
        assert!(!Source::Art(ArtPiece::ButtonBack).additive());
        assert!(!Source::Emblem(EmblemLayer::Base).additive());
        assert!(Source::Emblem(EmblemLayer::Core).additive());
        assert!(Source::Emblem(EmblemLayer::Lights).additive());
        // Each layer is its own run, so the pipeline can change between them.
        let mut runs = Vec::with_capacity(MAX_RUNS);
        runs.push(Run {
            start: 0,
            source: Source::Atlas,
        });
        for (index, layer) in EmblemLayer::ALL.into_iter().enumerate() {
            assert!(switch(&mut runs, 6 + index * 6, Source::Emblem(layer)));
        }
        assert_eq!(runs.len(), 1 + EmblemLayer::ALL.len());
    }
}

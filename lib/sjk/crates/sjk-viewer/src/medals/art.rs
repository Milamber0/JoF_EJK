//! The medals' whole pictures for the Identity page and the new medal pop-up: decoded
//! with their mip chains on a worker thread the first time the renderer is asked to
//! draw one ([`request`]), then uploaded once (`ui_renderer/medal_art.rs`). Until then
//! a screen draws its text without the picture. The small medallions are atlas icons
//! uploaded at start instead (`ui_renderer.rs`).

use super::Medal;
use crate::menu::emblem::MipChain;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// The decoded pictures, by [`Medal::index`].
pub(crate) struct Decoded {
    pictures: [Option<MipChain>; Medal::COUNT],
}

impl Decoded {
    /// The picture of `medal`, if it decoded.
    pub(crate) fn picture(&self, medal: Medal) -> Option<&MipChain> {
        self.pictures[medal.index()].as_ref()
    }
}

static DECODED: OnceLock<Decoded> = OnceLock::new();
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Start decoding the pictures on a worker thread, once per process; later calls do
/// nothing.
pub(crate) fn request() {
    if REQUESTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("sjk-medal-art".into())
        .spawn(|| {
            let _ = DECODED.set(decode_all());
        });
    if let Err(error) = spawned {
        crate::log::progress(format_args!(
            "warning: medal pictures not loaded, drawing without them: {error}"
        ));
    }
}

/// Whether a screen has asked for the pictures yet.
pub(crate) fn requested() -> bool {
    REQUESTED.load(Ordering::Acquire)
}

/// The decoded pictures, once the worker has finished.
pub(crate) fn decoded() -> Option<&'static Decoded> {
    DECODED.get()
}

fn decode_all() -> Decoded {
    Decoded {
        pictures: Medal::ALL.map(|medal| match decode(medal) {
            Ok(chain) => Some(chain),
            Err(error) => {
                crate::log::progress(format_args!("warning: medal {}: {error}", medal.id()));
                None
            }
        }),
    }
}

fn decode(medal: Medal) -> Result<MipChain, String> {
    let image = image::load_from_memory_with_format(medal.art_png(), image::ImageFormat::Png)
        .map_err(|error| error.to_string())?
        .into_rgba8();
    let (width, height) = image.dimensions();
    if width != height || width == 0 {
        return Err(format!("{width}x{height} is not square"));
    }
    Ok(MipChain::new(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_decodes_with_its_mip_chain() {
        for medal in Medal::ALL {
            let chain = decode(medal).expect("the medal decodes");
            assert_eq!(chain.size, 512);
            assert_eq!(chain.levels.len(), 10, "512 down to 1");
        }
    }
}

//! The window's icon: SJK's emblem in the title bar, taskbar and task
//! switcher.
//!
//! `sjk.exe` also carries the full icon set as a Windows resource (`build.rs`,
//! `assets/branding/sjk.ico`), which Explorer shows. A window does not take
//! that on its own, so the client sets it at creation: on Windows the small
//! icon (title bar) and the taskbar's, elsewhere the one icon X11 desktops
//! show. Wayland has no window icon in winit; a desktop entry's icon is used
//! there.
//!
//! Both pictures are the medallion of `assets/branding` (the ring, its core
//! and the blade), which stays readable at 16 to 48 pixels where the whole
//! starburst does not.

use winit::window::{Icon, WindowAttributes};

/// 32-pixel icon, for the title bar (shown at 16 to 24 pixels).
const SMALL: &[u8] = include_bytes!("../../../assets/branding/icon-32.png");
/// 64-pixel icon, for the taskbar and task switcher (32 to 48 pixels).
const LARGE: &[u8] = include_bytes!("../../../assets/branding/icon-64.png");

/// `attributes` with SJK's icons set.
pub(crate) fn with_icons(attributes: WindowAttributes) -> WindowAttributes {
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowAttributesExtWindows as _;
        attributes
            .with_window_icon(icon(SMALL))
            .with_taskbar_icon(icon(LARGE))
    }
    #[cfg(not(target_os = "windows"))]
    {
        attributes.with_window_icon(icon(LARGE))
    }
}

/// The icon in `png`, or none (logged) if it does not decode.
fn icon(png: &[u8]) -> Option<Icon> {
    let result = decode(png).and_then(|(rgba, width, height)| {
        Icon::from_rgba(rgba, width, height).map_err(|error| error.to_string())
    });
    match result {
        Ok(icon) => Some(icon),
        Err(error) => {
            crate::log::progress(format_args!("warning: window icon not set: {error}"));
            None
        }
    }
}

/// RGBA pixels, width and height of `png`.
fn decode(png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok((image.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_icons_decode_at_their_sizes() {
        for (png, size) in [(SMALL, 32), (LARGE, 64)] {
            let (rgba, width, height) = decode(png).expect("icon decodes");
            assert_eq!((width, height), (size, size));
            assert_eq!(rgba.len(), (size * size * 4) as usize);
            // Cut out: transparent corners, an opaque centre.
            assert_eq!(rgba[3], 0);
            let centre = ((size / 2 * size + size / 2) * 4 + 3) as usize;
            assert_eq!(rgba[centre], 255);
            assert!(icon(png).is_some());
        }
    }
}

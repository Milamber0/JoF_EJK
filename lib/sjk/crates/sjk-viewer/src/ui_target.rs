//! The 2D layer's colour model: display values, blended in display space.
//!
//! Retail Jedi Academy drew its menus, HUD and text into a framebuffer that held
//! display (gamma-encoded) values: a colour such as `^1` or a menu's `forecolor`
//! was the value shown on screen, and alpha mixed those values directly. The
//! world, by contrast, is rendered and filtered in linear light into sRGB or float
//! targets. To keep both, every 2D pass (text, retained UI shapes, the shader HUD,
//! the menu-file HUD and the scope) draws through a non-sRGB (UNORM) view of
//! whatever texture holds the finished image, so its vertex colours are written
//! and blended exactly as authored:
//!
//! - the swapchain image, through a view in [`format`] that the surface is
//!   configured to allow ([`surface_view_formats`]);
//! - the display-ramp intermediate (`r_gamma` other than 1, or a surface that
//!   cannot be viewed as UNORM), through the UNORM alias its display pass already
//!   samples; that pass then applies the ramp to the 2D layer too, as retail's
//!   hardware gamma did.
//!
//! The UI never lands in the float (`r_hdr`) scene target: the resolve encodes
//! HDR before the 2D layer is drawn. Pictures the 2D layer samples (icons, menu
//! art, map previews, menu-file HUD art, scope art) are stored as
//! [`TEXTURE_FORMAT`] so their texels are display values too. Font atlases
//! contribute only alpha, which no format decodes.

/// Colour-target format of every 2D pipeline: the display format without the
/// hardware sRGB encode.
pub(crate) fn format(display: wgpu::TextureFormat) -> wgpu::TextureFormat {
    display.remove_srgb_suffix()
}

/// Format of the pictures the 2D layer samples: their bytes are display
/// values and are used without decoding.
pub(crate) const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Whether the 2D layer can draw straight into the swapchain image: it already
/// stores display values, or the surface accepts a UNORM view of its sRGB
/// format (`DownlevelFlags::SURFACE_VIEW_FORMATS`). Otherwise the frame keeps the
/// display-ramp intermediate, whose UNORM alias always exists.
pub(crate) fn direct(display: wgpu::TextureFormat, surface_view_formats: bool) -> bool {
    !display.is_srgb() || surface_view_formats
}

/// Extra view formats the surface is configured with so [`surface_view`]
/// can make the 2D view of each swapchain image.
pub(crate) fn surface_view_formats(
    display: wgpu::TextureFormat,
    direct: bool,
) -> Vec<wgpu::TextureFormat> {
    if direct && display.is_srgb() {
        vec![format(display)]
    } else {
        Vec::new()
    }
}

/// The 2D view of this frame's swapchain image, when it differs from the
/// default view; one view object per frame, as the default view is.
pub(crate) fn surface_view(
    texture: &wgpu::Texture,
    display: wgpu::TextureFormat,
    direct: bool,
) -> Option<wgpu::TextureView> {
    (direct && display.is_srgb()).then(|| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("SJK 2D layer view"),
            format: Some(format(display)),
            ..Default::default()
        })
    })
}

/// Linear-light value of one sRGB-encoded (display) channel.
pub(crate) fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB-encoded (display) value of one linear-light channel.
#[cfg(test)]
pub(crate) fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::TextureFormat as F;

    #[test]
    fn two_d_pipelines_drop_only_the_srgb_encode() {
        assert_eq!(format(F::Bgra8UnormSrgb), F::Bgra8Unorm);
        assert_eq!(format(F::Rgba8UnormSrgb), F::Rgba8Unorm);
        assert_eq!(format(F::Bgra8Unorm), F::Bgra8Unorm);
        assert_eq!(format(F::Rgb10a2Unorm), F::Rgb10a2Unorm);
        assert!(!TEXTURE_FORMAT.is_srgb());
    }

    #[test]
    fn srgb_surfaces_alias_only_when_the_adapter_allows_it() {
        assert!(direct(F::Bgra8UnormSrgb, true));
        assert!(!direct(F::Bgra8UnormSrgb, false));
        // A UNORM swapchain already holds display values.
        assert!(direct(F::Bgra8Unorm, false));
        assert_eq!(
            surface_view_formats(F::Bgra8UnormSrgb, true),
            vec![F::Bgra8Unorm]
        );
        assert!(surface_view_formats(F::Bgra8UnormSrgb, false).is_empty());
        assert!(surface_view_formats(F::Bgra8Unorm, true).is_empty());
    }

    #[test]
    fn transfer_functions_round_trip_and_hit_reference_points() {
        for step in 0..=255 {
            let value = step as f32 / 255.0;
            assert!((linear_to_srgb(srgb_to_linear(value)) - value).abs() < 1e-5);
        }
        // #bcbcbc is relative luminance 0.5; linear 0.5 shows as 188/255.
        assert!((srgb_to_linear(188.0 / 255.0) - 0.5029).abs() < 1e-3);
        assert!((linear_to_srgb(0.5) - 0.7354).abs() < 1e-3);
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
    }
}

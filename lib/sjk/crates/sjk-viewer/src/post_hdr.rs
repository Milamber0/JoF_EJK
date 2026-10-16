//! Startup scene precision and a highlight-only, RGB-ratio-preserving display shoulder.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};

/// Scene precision, sampled at startup. Exposure is live: `r_hdrExposure` is the base
/// that eye adaptation ([`super::exposure`]) adjusts, unless `r_autoExposure` is 0.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    /// Zero preserves the display format; one selects RGBA16F.
    pub(crate) mode: u32,
    /// Startup linear scene multiplier, shown until the first frame reads the live one.
    pub(crate) exposure: f32,
}

// Neutral policy for the display-only resolve; scene defaults are sampled below.
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: 0,
            exposure: 1.0,
        }
    }
}

impl Settings {
    /// Read the registered/archive-loaded values once, before creating scene resources.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>) -> Self {
        Self {
            mode: console
                .and_then(|c| c.integer_cvar("r_sceneHdr"))
                .unwrap_or(1)
                .clamp(0, 1) as u32,
            exposure: exposure(
                console
                    .and_then(|c| c.float_cvar("r_hdrExposure"))
                    .unwrap_or(1.0),
            ),
        }
    }

    /// Scene-only format; the swapchain, HUD and captures retain their display format.
    pub(crate) fn format(self, display: wgpu::TextureFormat) -> wgpu::TextureFormat {
        match self.mode {
            1 => wgpu::TextureFormat::Rgba16Float,
            _ => display,
        }
    }
}

fn exposure(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.25, 4.0) as f32
    } else {
        1.0
    }
}

/// Own names: HDR is neither stock gamma nor the existing optional filmic LDR grade.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    cvars.register(CvarDefinition::new(
        "r_sceneHdr",
        1_i64,
        CvarFlags::ARCHIVE,
        "Scene HDR: 0 off, 1 RGBA16F; applies after a graphics reload (vid_restart)",
    ))?;
    cvars.register(CvarDefinition::new(
        "r_hdrExposure",
        1.0,
        CvarFlags::ARCHIVE,
        "HDR exposure 0.25..4, the base r_autoExposure adjusts; applies immediately",
    ))?;
    cvars.on_change("r_sceneHdr", |_| {
        crate::log::progress(format_args!(
            "HDR setting changed: {}",
            crate::graphics_reload::APPLY
        ));
        crate::graphics_reload::notice();
    })
}

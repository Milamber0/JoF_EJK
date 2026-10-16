//! Startup policy for optional actor/world sun shadows and main-view participating media.
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};

/// Quality selection is sampled from the registered values, never callback-only state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    /// Optional map-relative presentation clock.
    pub(crate) day: super::day::Settings,
    /// No map or pass is allocated when disabled.
    pub(crate) enabled: bool,
    /// Include opaque world casters with a view-volume fit.
    pub(crate) world: bool,
    /// Axial depth of the sharp view cascade in world units; the map-wide far cascade
    /// covers everything beyond it in the world-caster mode.
    pub(crate) distance: f32,
    /// Axial depth of the close cascade, the finest texels for nearby shadows and shafts.
    pub(crate) near: f32,
    /// Square depth-map extent, clamped again against device limits.
    pub(crate) resolution: u32,
    /// Number of PCF samples, separate from the sixteen blocker-search samples.
    pub(crate) taps: u32,
    /// Zero disables media; 1..3 select fixed froxel grids and imply world shadows.
    pub(crate) volumetrics: u32,
    /// Keep the static casters of the view cascades between frames (`r_liveLighting` 1 and
    /// below): fits that do not turn with the camera, a little coarser, drawn rarely.
    pub(crate) held: bool,
}
impl Settings {
    /// Read the startup registry, including archived configuration values.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>) -> Self {
        let day = super::day::Settings::sample(console);
        let volumetrics = console
            .and_then(|c| c.integer_cvar("r_volumetrics"))
            .unwrap_or(3)
            .clamp(0, 3) as u32;
        let world = day.enabled
            || volumetrics > 0
            || console
                .and_then(|c| c.integer_cvar("r_worldSunShadows"))
                .unwrap_or(1)
                != 0;
        Self {
            enabled: world
                || console
                    .and_then(|c| c.integer_cvar("r_actorSunShadows"))
                    .unwrap_or(1)
                    != 0,
            world,
            volumetrics,
            day,
            held: super::day::realtime_tier(console) <= 1,
            distance: console
                .and_then(|c| c.integer_cvar("r_sunShadowDistance"))
                .unwrap_or(1024)
                .clamp(128, 4096) as f32,
            near: console
                .and_then(|c| c.integer_cvar("r_sunShadowNear"))
                .unwrap_or(256)
                .clamp(64, 1024) as f32,
            resolution: console
                .and_then(|c| c.integer_cvar("r_sunShadowResolution"))
                .unwrap_or(2048)
                .clamp(512, 4096) as u32,
            taps: console
                .and_then(|c| c.integer_cvar("r_sunShadowTaps"))
                .unwrap_or(16)
                .clamp(4, 32) as u32,
        }
    }
}
/// Do not alias rend2's whole-world cascaded sun pipeline with different semantics.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    super::day::register(cvars)?;
    for (name, value, description) in [
        (
            "r_actorSunShadows",
            1,
            "Opaque actor sun shadows on diffuse world; vid_restart applies it",
        ),
        (
            "r_worldSunShadows",
            1,
            "World and actor sun shadows; vid_restart applies it",
        ),
        (
            "r_sunShadowDistance",
            1024,
            "Sharp sun-shadow cascade depth 128..4096, map-wide beyond; vid_restart applies it",
        ),
        (
            "r_sunShadowNear",
            256,
            "Close sun-shadow cascade depth 64..1024, finest texels; vid_restart applies it",
        ),
        (
            "r_sunShadowResolution",
            2048,
            "Sun shadow map width 512..4096; vid_restart applies it",
        ),
        (
            "r_sunShadowTaps",
            16,
            "Sun shadow base PCF taps 4..32 (up to 4x on soft edges); vid_restart applies it",
        ),
        (
            "r_volumetrics",
            3,
            "Froxel sun media 0..3; implies world shadows; vid_restart applies it",
        ),
    ] {
        cvars.register(CvarDefinition::new(
            name,
            value as i64,
            CvarFlags::ARCHIVE,
            description,
        ))?;
        cvars.on_change(name, |_| {
            crate::log::progress(format_args!(
                "Sun-shadow setting changed: {}",
                crate::graphics_reload::APPLY
            ));
            crate::graphics_reload::notice();
        })?;
    }
    Ok(())
}

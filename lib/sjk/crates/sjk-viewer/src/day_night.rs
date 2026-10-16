//! Map-relative presentation clock. No wall clock, simulation state, or accumulated integration.
use glam::Vec3;
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};
#[path = "day_live.rs"]
pub(crate) mod live;

/// Startup policy; hours are local to this stylised map-relative solar orbit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Settings {
    /// Preserve every authored path when disabled.
    pub(crate) enabled: bool,
    /// Fixed hour, or the hour at presentation time zero when cycling.
    pub(crate) hour: f32,
    /// Real minutes per simulated day; zero holds the selected hour.
    pub(crate) minutes: f32,
}

/// Frame-local sunlight and ambient fill, evaluated without allocation.
pub(in crate::world_materials) struct Frame {
    pub(in crate::world_materials) sun: sjk_shader::SunParms,
    pub(in crate::world_materials) ambient: [f32; 3],
}

/// The unit horizontal direction the sun rises along (east of the day model).
pub(super) fn azimuth(sun: &sjk_shader::SunParms) -> [f32; 2] {
    Vec3::new(sun.direction[0], sun.direction[1], 0.)
        .try_normalize()
        .unwrap_or(Vec3::X)
        .truncate()
        .to_array()
}

impl Settings {
    /// Seed from archived values at installation, not from change callbacks alone.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>) -> Self {
        let number = |name, default| {
            console
                .and_then(|c| c.float_cvar(name))
                .filter(|v| v.is_finite() && (*v as f32).is_finite())
                .unwrap_or(default) as f32
        };
        Self {
            enabled: console
                .and_then(|c| c.integer_cvar("r_dayNight"))
                .unwrap_or(1)
                != 0,
            hour: number("r_dayHour", 12.).rem_euclid(24.),
            minutes: number("r_dayMinutes", 0.).clamp(0., 1440.),
        }
    }

    /// Absolute shader time avoids frame-rate-dependent integration or drift.
    pub(super) fn hour_at(self, seconds: f32) -> f32 {
        let seconds = if seconds.is_finite() { seconds } else { 0. };
        (self.hour
            + if self.minutes >= 1. {
                seconds * 0.4 / self.minutes
            } else {
                0.
            })
        .rem_euclid(24.)
    }

    /// Sunrise is 06:00 along the authored horizontal sun axis, sunset is opposite at 18:00.
    /// The sun climbs the vertical arc through the authored direction (so the authored hour
    /// shows the authored shadows exactly), then keeps the authored elevation — never the
    /// zenith, which lights every floor flat — while its azimuth swings across the sky to
    /// the evening arc down. Elevation is held within 35°..75°.
    pub(in crate::world_materials) fn frame(
        self,
        authored: sjk_shader::SunParms,
        seconds: f32,
    ) -> Frame {
        if !self.enabled {
            return Frame {
                sun: authored,
                ambient: [0.; 3],
            };
        }
        let angle = (self.hour_at(seconds) - 6.) * std::f32::consts::PI / 12.;
        let direction = Vec3::from_array(authored.direction);
        let horizontal = Vec3::new(direction.x, direction.y, 0.)
            .try_normalize()
            .unwrap_or(Vec3::X);
        let elevation = direction
            .normalize_or_zero()
            .z
            .clamp(0., 1.)
            .asin()
            .clamp(35_f32.to_radians(), 75_f32.to_radians());
        let sun = if angle <= elevation || angle >= std::f32::consts::PI - elevation {
            horizontal * angle.cos() + Vec3::Z * angle.sin()
        } else {
            let across = Vec3::Z.cross(horizontal).normalize_or_zero();
            let swing = (angle - elevation) / (std::f32::consts::PI - 2. * elevation)
                * std::f32::consts::PI;
            (horizontal * swing.cos() + across * swing.sin()) * elevation.cos()
                + Vec3::Z * elevation.sin()
        };
        frame_for_direction(authored, sun)
    }
}

/// Apply the existing atmosphere response to an explicitly directed sun.
pub(in crate::world_materials) fn frame_for_direction(
    authored: sjk_shader::SunParms,
    sun: Vec3,
) -> Frame {
    let daylight = (sun.z / 0.25).clamp(0., 1.);
    // Sun colour by elevation: ember at the horizon, gold through the low hours, near
    // white when high; the sky light goes from a deep night blue through a warm dusk
    // to a blue day.
    let low = (sun.z / 0.15).clamp(0., 1.);
    let high = ((sun.z - 0.15) / 0.5).clamp(0., 1.);
    let color = Vec3::new(1., 0.32, 0.08)
        .lerp(Vec3::new(1., 0.72, 0.42), low)
        .lerp(Vec3::new(1., 0.96, 0.88), high);
    let dusk = (sun.z / 0.35).clamp(0., 1.);
    // The sky light stays cool at dusk: sunlit faces go gold while shadows keep a
    // blue cast, the contrast that reads as a low sun rather than a colour filter.
    let ambient = Vec3::new(0.02, 0.03, 0.06)
        .lerp(Vec3::new(0.09, 0.10, 0.15), daylight)
        .lerp(Vec3::new(0.19, 0.23, 0.31), dusk);
    Frame {
        sun: sjk_shader::SunParms {
            direction: sun.to_array(),
            color: color.to_array(),
            // A low sun is also weak: the long path through the air takes most of
            // its light before it reaches the ground.
            intensity: authored.intensity * daylight * (0.3 + 0.7 * high),
        },
        ambient: ambient.to_array(),
    }
}

/// The morning hour whose day-model sun has the authored elevation.
pub(super) fn authored_hour(authored: sjk_shader::SunParms) -> f32 {
    let direction = Vec3::from_array(authored.direction).normalize_or_zero();
    6. + direction.z.clamp(0., 1.).asin() * 12. / std::f32::consts::PI
}

/// How much of the lighting is computed live (`r_liveLighting`): 2 everything, 0 baked
/// indirect light under a live sun. `SJK_REALTIME` overrides the cvar for one run.
pub(crate) fn realtime_tier(console: Option<&crate::console::ViewerConsole>) -> i64 {
    std::env::var("SJK_REALTIME")
        .ok()
        .and_then(|tier| tier.parse().ok())
        .or_else(|| console.and_then(|console| console.integer_cvar("r_liveLighting")))
        .unwrap_or(0)
}

/// Enabling allocates at startup; hour and cycle rate update installed day resources live.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    if cvars.get("r_dayNight").is_some() {
        return Ok(());
    }
    cvars.register(CvarDefinition::new(
        "r_dayNight",
        1_i64,
        CvarFlags::ARCHIVE,
        "Map-relative sun and sky atmosphere; implies world shadows; vid_restart applies it",
    ))?;
    cvars.register(CvarDefinition::new(
        "r_liveLighting",
        0_i64,
        CvarFlags::ARCHIVE,
        "How much of the lighting is computed live: 2 everything; 1 the same with the world's \
        shadow casters kept between frames; 0 also baked indirect light (where the map brings \
        any) under a live sun; applies at map load",
    ))?;
    for (name, value, help) in [
        (
            "r_dayHour",
            12.,
            "Solar hour 0..24; live when day/night is installed",
        ),
        (
            "r_dayMinutes",
            0.,
            "Minutes per day, 0 holds hour, 1..1440 cycles; live",
        ),
        (
            "r_volumetricClarity",
            1.,
            "Light shafts show only contrast in sunlit air: 1 = clear when fully lit, \
             0 = plain in-scatter haze; live",
        ),
        (
            "r_dayBrightness",
            1.,
            "Real-time lighting brightness 0.1..10 relative to the sun; live",
        ),
        (
            "r_ambientFill",
            0.025,
            "Subtle fill in dark areas, 0 off..0.2; fades in existing light; live",
        ),
        (
            "r_indirectBoost",
            1.,
            "Indirect sky and bounce brightness 0..4, 1 original; direct lights unchanged; live",
        ),
        (
            "r_ambientFillOcclusion",
            1.,
            "Corner shading on readability fill: 1 original, 0 unoccluded; live",
        ),
        (
            "r_dayDebug",
            0.,
            "Real-time lighting diagnostics, bits: 1 no occlusion, 2 no contact shadows, \
             4 no lamps, 8 no far cascade, 16 no bounce, 32 no sun shadow maps, \
             64 no close cascade, 128 sun visibility as the light, 256 no volumetrics, \
             512 no sun highlight, 1024 material maps ignore lamp/bounce direction; live",
        ),
        (
            "r_sunShadowGapClose",
            0.,
            "Sun shadow maps close slits narrower than this many units (0 off); live",
        ),
        (
            "r_contactShadows",
            0.,
            "Screen-space contact shadows toward the sun (off: self-shadowing on curved \
             ground and false shadows at a distance are still being worked out); live",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for name in ["r_dayNight"] {
        cvars.on_change(name, |_| {
            crate::log::progress(format_args!(
                "Day/night setting changed: {}",
                crate::graphics_reload::APPLY
            ));
            crate::graphics_reload::notice();
        })?;
    }
    Ok(())
}

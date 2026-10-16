//! Cvar adapters for the verified JKA guide subset.
use crate::console::ViewerConsole;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::Color;

/// Register functional guide options and speed-label placement.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value, help) in [
        (
            "cg_strafeHelper",
            3008,
            "JKA lines (2) or CGAZ markers (4), with direction bits",
        ),
        ("cg_snapHud", 0, "JKA velocity snap zones"),
        (
            "cg_snapHudAuto",
            1,
            "Snap heading: 0 manual, 1/2 movement-aware",
        ),
    ] {
        cvars.register(CvarDefinition::new(
            name,
            value as i64,
            CvarFlags::ARCHIVE,
            help,
        ))?;
    }
    for (name, value, help) in [
        (
            "cg_strafeHelperInactiveAlpha",
            200.0,
            "Inactive marker alpha byte",
        ),
        (
            "cg_strafeHelperCutoff",
            0.0,
            "Line cutoff from the bottom; CGAZ marker height",
        ),
        (
            "cg_strafeHelperLineWidth",
            1.0,
            "Line and marker width in virtual pixels",
        ),
        (
            "cg_strafeHelperOffset",
            75.0,
            "Optimum angle offset in hundredths of degrees",
        ),
        (
            "cg_strafeHelperPrecision",
            256.0,
            "Projected marker distance",
        ),
        (
            "cg_strafeHelper_FPS",
            0.0,
            "Helper FPS; zero uses com_maxFPS",
        ),
        ("cg_snapHudDef", 45.0, "Default snap heading offset"),
        ("cg_snapHudFps", 0.0, "Snap FPS; zero uses com_maxFPS"),
        ("cg_snapHudHeight", 5.0, "Snap zone height"),
        ("cg_snapHudSpeed", 0.0, "Snap speed; zero uses player speed"),
        ("cg_snapHudY", 235.0, "Snap zone vertical position"),
        ("cg_speedometerX", 132.0, "Speed label virtual X"),
        ("cg_speedometerY", 459.0, "Speed label virtual Y"),
        ("cg_speedometerSize", 0.75, "Speed label scale"),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for definition in [
        CvarDefinition::new(
            "cg_strafeHelperActiveColor",
            "0 255 0 200",
            CvarFlags::ARCHIVE,
            "Active marker RGBA bytes",
        ),
        CvarDefinition::new(
            "cg_snapHudRgba1",
            "127 179 230 179",
            CvarFlags::ARCHIVE,
            "First snap zone RGBA bytes",
        ),
        CvarDefinition::new(
            "cg_snapHudRgba2",
            "127 127 127 38",
            CvarFlags::ARCHIVE,
            "Second snap zone RGBA bytes",
        ),
    ] {
        cvars.register(definition)?;
    }
    Ok(())
}

/// The Settings row of the JoF EJK lines (not a cvar): on is `cg_strafeHelper`
/// option 1, the lines style.
pub(crate) const LINES_ROW: &str = "strafelines";
/// The styles `cg_strafeHelper` draws one at a time (options 0-3 and 13, as JoF's
/// `strafeHelper` command switches them).
const STYLES: i64 = 0b1111 | (1 << 13);
/// The keys JoF EJK's profile draws lines for when none is picked: WA and WD.
const DEFAULT_KEYS: i64 = (1 << 6) | (1 << 7);
/// Every key option the lines can show (W, WA, WD, A, D, rear).
const LINE_KEYS: i64 = 0b11111 << 5 | (1 << 10);

/// Whether the strafe helper draws JoF EJK's lines.
pub(crate) fn lines_on(console: &ViewerConsole) -> bool {
    console.integer_cvar("cg_strafeHelper").unwrap_or(0) & 2 != 0
}

/// Switch JoF EJK's lines on (in place of another style, with WA and WD when no
/// key is picked) or off.
pub(crate) fn toggle_lines(console: &mut ViewerConsole) {
    let value = console.integer_cvar("cg_strafeHelper").unwrap_or(0);
    console.set_cvar("cg_strafeHelper", &lines_value(value).to_string());
}

fn lines_value(value: i64) -> i64 {
    if value & 2 != 0 {
        value & !2
    } else if value & LINE_KEYS == 0 {
        (value & !STYLES) | 2 | DEFAULT_KEYS
    } else {
        (value & !STYLES) | 2
    }
}

/// Immutable settings consumed by guide projection.
pub(super) struct Settings {
    /// Helper mode/direction bits.
    pub(super) helper: i64,
    /// Reciprocal frame rate used for air acceleration.
    pub(super) helper_fps: f32,
    /// Angle offset.
    pub(super) offset: f32,
    /// Marker projection length.
    pub(super) precision: f32,
    /// Marker width.
    pub(super) width: f32,
    /// Marker height adjustment.
    pub(super) cutoff: f32,
    /// Active color.
    pub(super) active: Color,
    /// Inactive opacity.
    pub(super) inactive: f32,
    /// Snap visibility.
    pub(super) snap: bool,
    /// Snap frame rate.
    pub(super) snap_fps: f32,
    /// Manual speed override.
    pub(super) snap_speed: f32,
    /// Heading mode.
    pub(super) snap_auto: i64,
    /// Idle/manual offset.
    pub(super) snap_def: f32,
    /// Virtual top edge.
    pub(super) snap_y: f32,
    /// Virtual height.
    pub(super) snap_height: f32,
    /// Alternating zone colors.
    pub(super) snap_colors: [Color; 2],
    /// Base horizontal FOV.
    pub(super) fov: f32,
    /// Widescreen adjustment toggle.
    pub(super) aspect_adjust: bool,
}

impl Settings {
    /// Read canonical names without per-frame allocation.
    pub(super) fn read(console: Option<&ViewerConsole>) -> Self {
        let f = |name, fallback| crate::cgame_options::scalar(console, name, fallback);
        let i = |name, fallback| super::family::integer(console, name, fallback);
        let fps = |value: f32| {
            let v = if value < 1.0 {
                i("com_maxfps", 125) as f32
            } else {
                value.trunc()
            };
            if v < 1.0 { 125.0 } else { v.min(1000.0) }
        };
        Self {
            helper: i("cg_strafehelper", 3008),
            helper_fps: fps(f("cg_strafehelper_fps", 0.0)),
            offset: f("cg_strafehelperoffset", 75.0) * 0.01,
            precision: f("cg_strafehelperprecision", 256.0)
                .trunc()
                .clamp(100.0, 10000.0),
            width: f("cg_strafehelperlinewidth", 1.0).clamp(0.25, 5.0),
            cutoff: f("cg_strafehelpercutoff", 0.0),
            active: rgba(
                console,
                "cg_strafehelperactivecolor",
                [0.0, 255.0, 0.0, 200.0],
            ),
            inactive: f("cg_strafehelperinactivealpha", 200.0).clamp(0.0, 255.0) / 255.0,
            snap: i("cg_snaphud", 0) != 0,
            snap_fps: fps(f("cg_snaphudfps", 0.0)),
            snap_speed: f("cg_snaphudspeed", 0.0),
            snap_auto: i("cg_snaphudauto", 1),
            snap_def: f("cg_snaphuddef", 45.0),
            snap_y: f("cg_snaphudy", 235.0),
            snap_height: f("cg_snaphudheight", 5.0),
            snap_colors: [
                rgba(console, "cg_snaphudrgba1", [127.0, 179.0, 230.0, 179.0]),
                rgba(console, "cg_snaphudrgba2", [127.0, 127.0, 127.0, 38.0]),
            ],
            fov: f("cg_fov", 80.0).clamp(1.0, 130.0),
            aspect_adjust: console
                .and_then(|c| c.bool_cvar("cg_fovaspectadjust"))
                .unwrap_or(true),
        }
    }
}

fn rgba(console: Option<&ViewerConsole>, name: &str, fallback: [f32; 4]) -> Color {
    let mut values = fallback;
    if let Some(sjk_shell::CvarValue::Text(text)) = console.and_then(|c| c.cvar(name)) {
        let mut words = text.split_whitespace();
        for v in &mut values {
            if let Some(n) = words
                .next()
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|v| v.is_finite())
            {
                *v = n;
            }
        }
    }
    let v = values.map(|v| v.clamp(0.0, 255.0) / 255.0);
    Color::new(v[0], v[1], v[2], v[3])
}

#[cfg(test)]
mod tests {
    use super::lines_value;

    #[test]
    fn the_lines_switch_keeps_keys_and_replaces_the_style() {
        // SJK's default (keys, no style) gets the lines; JoF's 2242 is that plus the centre.
        assert_eq!(lines_value(3008), 3010);
        assert_eq!(lines_value(2242), 2240);
        // CGAZ markers give way to the lines, as JoF's radio options do.
        assert_eq!(lines_value(3044), 3042);
        // No key picked: WA and WD, as Sol's JoF EJK profile.
        assert_eq!(lines_value(0), 194);
    }
}

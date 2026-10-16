//! Graphics quality: the renderer, Vanilla or SJK's, and the SJK renderer's
//! levels, each one choice that sets the renderer's costly settings together.
//!
//! Vanilla is the original game's look at the least cost (the EJK level until
//! 12/10/2026, named after EternalJK, the client closest to it that players
//! know). The SJK renderer is SJK's own lighting, shadows and post-processing,
//! still in alpha, at four levels from Performance to Ultra. A new profile
//! starts on Vanilla ([`apply_new_profile`]); the renderer cvars' own defaults
//! are the SJK renderer's High, so turning it on gives the look it was tuned for.
//! The level is not stored: it is whichever level the settings match, and
//! Custom once one of them is changed on its own. Only settings with a row in
//! Settings are touched, so every change a level makes can be seen and undone
//! there; the FPS cap, vertical sync, resolution, supersampling and anything
//! that changes gameplay or a map's content are left alone.

use crate::console::ViewerConsole;

/// Console command that names or sets the level.
pub(crate) const COMMAND: &str = "graphicsquality";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str =
    "Show or set graphics quality: vanilla, performance, balanced, high or ultra";
/// The SJK quality row's stand-in for a cvar name: the command that sets it.
pub(crate) const ROW_NAME: &str = COMMAND;
/// Console command that names or sets the renderer, and the Renderer row's
/// stand-in for a cvar name: Vanilla while the settings are Vanilla's, not a
/// value of its own.
pub(crate) const RENDERER_ROW: &str = "renderer";
/// Help text of [`RENDERER_ROW`] for completion and `cmdlist`.
pub(crate) const RENDERER_HELP: &str =
    "Show or set the renderer: vanilla (the original look) or sjk (SJK's renderer, alpha)";
/// What turning Vanilla on put aside, `cvar=value` pairs joined by `;`, so
/// turning the SJK renderer on again restores the player's own settings. Its
/// name is from when Vanilla was Ultra low.
pub(crate) const RESTORE_CVAR: &str = "r_ultraLowRestore";
/// The Renderer row's two values ([`renderer_value`]).
pub(crate) const VANILLA: &str = "vanilla";
pub(crate) const SJK: &str = "sjk";

/// A graphics quality level, cheapest first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Level {
    /// The original game's look at the least cost (EJK, and Ultra low,
    /// before): Performance with the material packs' normal and specular maps
    /// and ambient occlusion off too, so maps show their baked lightmaps and
    /// nothing of SJK's lighting. It is the renderer choice "Vanilla".
    Vanilla,
    /// The most frames short of that: the retail look with SJK's lighting,
    /// shadows, post-processing and weather extras off.
    Performance,
    /// Most of SJK's look at a lower cost: no floor mirrors, reflection probes or
    /// light shafts (the two largest costs a setting removes in the 09/10/2026 frame
    /// test), smaller, softer-filtered shadows.
    Balanced,
    /// SJK's defaults.
    High,
    /// Everything at its best: larger, finer shadows and the richest weather.
    Ultra,
}

/// Each setting a level sets, with its value at Vanilla, Performance,
/// Balanced, High and Ultra. High is each setting's default (pinned by a test).
const VALUES: &[(&str, [&str; 5])] = &[
    // Image.
    ("r_sceneHdr", ["0", "0", "1", "1", "1"]),
    ("r_sceneBloom", ["0", "0", "1", "1", "1"]),
    ("r_DynamicGlow", ["0", "0", "1", "1", "1"]),
    ("r_fxaa", ["0", "0", "1", "1", "1"]),
    ("r_softParticles", ["0", "0", "1", "1", "1"]),
    ("r_ssao", ["0", "1", "1", "1", "1"]),
    // Dust shows only in light shafts.
    (crate::dust_motes::CVAR, ["0", "0", "0", "1", "1"]),
    ("r_modelPixelLight", ["0", "0", "1", "1", "1"]),
    ("r_cubeMapping", ["0", "0", "0", "1", "1"]),
    ("r_floorReflections", ["0", "0", "0", "1", "1"]),
    ("r_normalMapping", ["0", "1", "1", "1", "1"]),
    ("r_specularMapping", ["0", "1", "1", "1", "1"]),
    ("r_parallaxMapping", ["0", "0", "1", "1", "1"]),
    ("r_emissiveMaps", ["0", "0", "1", "1", "1"]),
    ("r_emissiveGlow", ["0", "0", "1", "1", "1"]),
    // Lighting.
    ("r_dayNight", ["0", "0", "1", "1", "1"]),
    ("r_emissiveLights", ["0", "0", "1", "1", "1"]),
    ("r_volumetrics", ["0", "0", "0", "3", "3"]),
    // Shadows.
    ("r_worldSunShadows", ["0", "0", "1", "1", "1"]),
    ("r_actorSunShadows", ["0", "0", "1", "1", "1"]),
    (
        "r_sunShadowResolution",
        ["1024", "1024", "1024", "2048", "4096"],
    ),
    ("r_sunShadowTaps", ["8", "8", "8", "16", "24"]),
    // Weather.
    (crate::weather::DENSITY_CVAR, ["1", "1", "1.5", "2", "2"]),
    (crate::weather::QUALITY_CVAR, ["0", "0", "1", "2", "3"]),
    (crate::weather::CLOUDS_CVAR, ["0", "0", "1", "1", "1"]),
];

impl Level {
    pub(crate) const ALL: [Self; 5] = [
        Self::Vanilla,
        Self::Performance,
        Self::Balanced,
        Self::High,
        Self::Ultra,
    ];

    /// The level's name in Settings.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Performance => "Performance",
            Self::Balanced => "Balanced",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// The SJK renderer's levels, cheapest first: every level but Vanilla.
    pub(crate) const SJK: [Self; 4] = [Self::Performance, Self::Balanced, Self::High, Self::Ultra];

    /// The level `name` names, in any case, with or without its space; EJK and
    /// Ultra low are Vanilla's old names.
    fn parse(name: &str) -> Option<Self> {
        let name: String = name.split_whitespace().collect();
        if ["ultralow", "ejk"]
            .iter()
            .any(|old| name.eq_ignore_ascii_case(old))
        {
            return Some(Self::Vanilla);
        }
        Self::ALL
            .into_iter()
            .find(|level| level.command_name().eq_ignore_ascii_case(&name))
    }

    /// The level's name in the command: its label without the space.
    fn command_name(self) -> String {
        self.label().replace(' ', "").to_ascii_lowercase()
    }

    /// The SJK level a step of `direction` from this one reaches, stopping at
    /// the ends rather than wrapping from Ultra to Performance; a step from
    /// Vanilla starts the SJK renderer at Performance (right) or High (left).
    pub(crate) fn step(self, direction: i32) -> Self {
        if self == Self::Vanilla {
            return if direction > 0 {
                Self::Performance
            } else {
                Self::High
            };
        }
        let last = Self::ALL.len() as i32 - 1;
        Self::ALL[(self.index() as i32 + direction).clamp(1, last) as usize]
    }

    /// The level whose values `console` holds, if every one matches.
    pub(crate) fn current(console: &ViewerConsole) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|level| level.matches(console) == VALUES.len())
    }

    /// The current level, or for custom settings the level they are nearest,
    /// sharing the most values (the higher on a tie). Steps and lists start here.
    pub(crate) fn nearest(console: &ViewerConsole) -> Self {
        Self::ALL
            .into_iter()
            .max_by_key(|level| level.matches(console))
            .unwrap_or(Self::High)
    }

    /// How many of the level's values `console` holds.
    fn matches(self, console: &ViewerConsole) -> usize {
        VALUES
            .iter()
            .filter(|(cvar, values)| holds(console, cvar, values[self.index()]))
            .count()
    }

    /// Set the level's values, saving the profile once.
    pub(crate) fn apply(self, console: &mut ViewerConsole) {
        console.set_cvars(
            VALUES
                .iter()
                .map(|(cvar, values)| (*cvar, values[self.index()])),
        );
    }
}

/// Whether the renderer is Vanilla: the settings are Vanilla's.
pub(crate) fn vanilla(console: &ViewerConsole) -> bool {
    Level::current(console) == Some(Level::Vanilla)
}

/// The Renderer row's value: [`VANILLA`] or [`SJK`].
pub(crate) fn renderer_value(console: &ViewerConsole) -> &'static str {
    if vanilla(console) { VANILLA } else { SJK }
}

/// Set the renderer to `value` ([`VANILLA`] or [`SJK`], any case), as the
/// Renderer row does; another value changes nothing.
pub(crate) fn set_renderer(console: &mut ViewerConsole, value: &str) {
    let value = value.trim();
    let to_vanilla = value.eq_ignore_ascii_case(VANILLA);
    if (to_vanilla || value.eq_ignore_ascii_case(SJK)) && to_vanilla != vanilla(console) {
        toggle_vanilla(console);
    }
}

/// Switch between Vanilla and the SJK renderer. To Vanilla, it keeps the
/// player's value of every setting a level sets, then applies Vanilla; back,
/// it puts the kept values back (High when none were kept, as on a new profile).
pub(crate) fn toggle_vanilla(console: &mut ViewerConsole) {
    if vanilla(console) {
        let kept = console
            .cvar(RESTORE_CVAR)
            .map(sjk_shell::CvarValue::as_text)
            .unwrap_or_default();
        let restored: Vec<(&str, &str)> = kept
            .split(';')
            .filter_map(|pair| pair.split_once('='))
            .filter(|(cvar, _)| VALUES.iter().any(|(known, _)| known == cvar))
            .chain([(RESTORE_CVAR, "")])
            .collect();
        if restored.len() == 1 {
            Level::High.apply(console);
        }
        console.set_cvars(restored);
    } else {
        let kept = VALUES
            .iter()
            .filter_map(|(cvar, _)| Some(format!("{cvar}={}", console.cvar(cvar)?.as_text())))
            .collect::<Vec<_>>()
            .join(";");
        console.set_cvar(RESTORE_CVAR, &kept);
        Level::Vanilla.apply(console);
    }
}

/// Start a new profile on Vanilla: a player new to SJK gets the original look
/// and its frame rate, and turns the SJK renderer on when they choose to.
pub(crate) fn apply_new_profile(console: &mut ViewerConsole) {
    Level::Vanilla.apply(console);
}

/// What the SJK quality row shows: the level, Custom, or Off on Vanilla.
pub(crate) fn shown(console: &ViewerConsole) -> &'static str {
    match Level::current(console) {
        Some(Level::Vanilla) => "Off",
        Some(level) => level.label(),
        None => "Custom",
    }
}

/// The default a renderer setting's row shows and returns to: Vanilla's value
/// while the renderer is Vanilla, so its rows do not all read as changed; `None`
/// under the SJK renderer, or for a setting no level sets, whose default is
/// its cvar's.
pub(crate) fn row_default(console: &ViewerConsole, cvar: &str) -> Option<&'static str> {
    if !vanilla(console) {
        return None;
    }
    VALUES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(cvar))
        .map(|(_, values)| values[Level::Vanilla.index()])
}

/// `renderer [vanilla|sjk]`: name the renderer, or set it.
pub(crate) fn renderer_command(
    console: &mut ViewerConsole,
    args: &[String],
) -> Result<Vec<String>, String> {
    let named = |console: &ViewerConsole| {
        if vanilla(console) {
            "Renderer: Vanilla (the original look)".to_owned()
        } else {
            format!(
                "Renderer: SJK (alpha), {} quality; it can still show glitches, which the \
                 SJK menu's Report a bug sends to the team",
                shown(console)
            )
        }
    };
    match args {
        [] => Ok(vec![named(console)]),
        [value]
            if [VANILLA, SJK, "ejk"]
                .iter()
                .any(|name| value.eq_ignore_ascii_case(name)) =>
        {
            let value = if value.eq_ignore_ascii_case(SJK) {
                SJK
            } else {
                VANILLA
            };
            set_renderer(console, value);
            Ok(vec![
                named(console),
                "It applies after a graphics reload (vid_restart)".to_owned(),
            ])
        }
        _ => Err(format!("usage: {RENDERER_ROW} [{VANILLA}|{SJK}]")),
    }
}

/// Whether `cvar` holds `value`: numbers compare as numbers ("1.0" holds "1").
fn holds(console: &ViewerConsole, cvar: &str, value: &str) -> bool {
    let Some(current) = console.cvar(cvar).map(sjk_shell::CvarValue::as_text) else {
        return false;
    };
    match (current.trim().parse::<f64>(), value.parse::<f64>()) {
        (Ok(current), Ok(value)) => (current - value).abs() < 1e-6,
        _ => current.trim().eq_ignore_ascii_case(value),
    }
}

/// `graphicsquality [level]`: name the level, or set it.
pub(crate) fn command(console: &mut ViewerConsole, args: &[String]) -> Result<Vec<String>, String> {
    let names = || Level::ALL.map(Level::command_name).join(", ");
    match args {
        [] => Ok(vec![format!(
            "Graphics quality: {} ({})",
            shown(console),
            names()
        )]),
        [name] => {
            let level =
                Level::parse(name).ok_or_else(|| format!("usage: {COMMAND} [{}]", names()))?;
            level.apply(console);
            Ok(vec![format!(
                "Graphics quality: {}; shadows, light shafts, HDR, FXAA, sun and sky, \
                 reflection probes and material maps apply after a graphics reload \
                 (vid_restart), emission-map lights on the next map",
                level.label()
            )])
        }
        _ => Err(format!("usage: {COMMAND} [{}]", names())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    #[test]
    fn a_fresh_profile_is_high() {
        let (_directory, console) = console();
        for (cvar, values) in VALUES {
            let default = console
                .cvar_default(cvar)
                .unwrap_or_else(|| panic!("{cvar} is not registered"))
                .as_text();
            assert_eq!(
                default.parse::<f64>().ok(),
                values[Level::High.index()].parse::<f64>().ok(),
                "{cvar}: High is not its default"
            );
        }
        assert_eq!(Level::current(&console), Some(Level::High));
        assert_eq!(shown(&console), "High");
    }

    #[test]
    fn each_level_sets_its_values_and_reads_back() {
        let (directory, mut console) = console();
        for level in Level::ALL.into_iter().rev() {
            level.apply(&mut console);
            assert_eq!(Level::current(&console), Some(level));
            let shown_as = if level == Level::Vanilla {
                "Off"
            } else {
                level.label()
            };
            assert_eq!(shown(&console), shown_as);
        }
        // The profile was saved: a new start reads the last level.
        drop(console);
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert_eq!(Level::current(&console), Some(Level::Vanilla));
    }

    #[test]
    fn no_level_costs_less_than_the_one_below_it() {
        for (cvar, values) in VALUES {
            let numbers: Vec<f64> = values.iter().map(|v| v.parse().unwrap()).collect();
            assert!(
                numbers.windows(2).all(|pair| pair[0] <= pair[1]),
                "{cvar}: {values:?}"
            );
            assert!(numbers[0] < numbers[4], "{cvar} is the same at every level");
        }
    }

    #[test]
    fn every_value_is_one_its_settings_row_can_show() {
        let (_directory, mut console) = console();
        for level in Level::ALL {
            for (cvar, values) in VALUES {
                let value = values[level.index()];
                assert!(
                    crate::settings::row_takes(cvar, value),
                    "{cvar} {value}: no row in Settings shows it"
                );
                assert!(console.set_cvar(cvar, value), "{cvar} {value}");
                assert!(holds(&console, cvar, value), "{cvar} refused {value}");
            }
        }
    }

    #[test]
    fn a_setting_changed_on_its_own_is_custom_near_its_level() {
        let (_directory, mut console) = console();
        Level::Ultra.apply(&mut console);
        console.set_cvar("r_sunShadowTaps", "12");
        assert_eq!(Level::current(&console), None);
        assert_eq!(shown(&console), "Custom");
        assert_eq!(Level::nearest(&console), Level::Ultra);
        Level::Performance.apply(&mut console);
        console.set_cvar("r_sceneBloom", "1");
        assert_eq!(Level::nearest(&console), Level::Performance);
    }

    #[test]
    fn steps_stop_at_the_ends_of_the_sjk_levels() {
        assert_eq!(Level::Vanilla.step(1), Level::Performance);
        assert_eq!(Level::Vanilla.step(-1), Level::High);
        assert_eq!(Level::Performance.step(-1), Level::Performance);
        assert_eq!(Level::Performance.step(1), Level::Balanced);
        assert_eq!(Level::High.step(1), Level::Ultra);
        assert_eq!(Level::Ultra.step(1), Level::Ultra);
    }

    #[test]
    fn the_command_names_and_sets_the_level() {
        let (_directory, mut console) = console();
        let lines = command(&mut console, &[]).unwrap();
        assert!(lines[0].starts_with("Graphics quality: High"), "{lines:?}");
        command(&mut console, &["ULTRA".to_owned()]).unwrap();
        assert_eq!(Level::current(&console), Some(Level::Ultra));
        assert!(command(&mut console, &["max".to_owned()]).is_err());
        assert_eq!(Level::current(&console), Some(Level::Ultra));
        command(&mut console, &["ultralow".to_owned()]).unwrap();
        assert_eq!(Level::current(&console), Some(Level::Vanilla));
        command(&mut console, &["Ultra Low".to_owned()]).unwrap();
        assert_eq!(shown(&console), "Off");
        Level::High.apply(&mut console);
        command(&mut console, &["ejk".to_owned()]).unwrap();
        assert_eq!(Level::current(&console), Some(Level::Vanilla));
        Level::High.apply(&mut console);
        command(&mut console, &["Vanilla".to_owned()]).unwrap();
        assert!(vanilla(&console));
    }

    #[test]
    fn vanilla_turns_sjk_lighting_and_material_maps_off() {
        let (_directory, mut console) = console();
        Level::Vanilla.apply(&mut console);
        for cvar in [
            "r_dayNight",
            "r_worldSunShadows",
            "r_volumetrics",
            "r_cubeMapping",
            "r_floorReflections",
            "r_normalMapping",
            "r_specularMapping",
            "r_parallaxMapping",
            "r_emissiveMaps",
            "r_ssao",
            "r_sceneHdr",
        ] {
            assert!(holds(&console, cvar, "0"), "{cvar}");
        }
    }

    #[test]
    fn vanilla_and_back_restores_the_players_settings() {
        let (directory, mut console) = console();
        Level::Balanced.apply(&mut console);
        console.set_cvar("r_sunShadowTaps", "12");
        assert!(!vanilla(&console));
        assert_eq!(renderer_value(&console), SJK);
        set_renderer(&mut console, VANILLA);
        assert!(vanilla(&console));
        assert_eq!(renderer_value(&console), VANILLA);
        // Kept across a restart.
        drop(console);
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert!(vanilla(&console));
        set_renderer(&mut console, VANILLA);
        assert!(vanilla(&console), "Vanilla again changes nothing");
        set_renderer(&mut console, SJK);
        assert!(!vanilla(&console));
        assert!(holds(&console, "r_sunShadowTaps", "12"));
        console.set_cvar("r_sunShadowTaps", "8");
        assert_eq!(Level::current(&console), Some(Level::Balanced));
        assert!(holds(&console, RESTORE_CVAR, ""));
        // Vanilla picked as a level has nothing kept: the SJK renderer is High.
        Level::Vanilla.apply(&mut console);
        toggle_vanilla(&mut console);
        assert_eq!(Level::current(&console), Some(Level::High));
    }

    #[test]
    fn a_new_profile_is_vanilla_and_its_renderer_rows_read_unchanged() {
        let (_directory, mut console) = console();
        assert_eq!(row_default(&console, "r_sceneHdr"), None);
        apply_new_profile(&mut console);
        assert!(vanilla(&console));
        assert_eq!(shown(&console), "Off");
        assert_eq!(row_default(&console, "r_sceneHdr"), Some("0"));
        assert_eq!(row_default(&console, "r_sunShadowResolution"), Some("1024"));
        assert_eq!(row_default(&console, "cg_fov"), None);
        let lines = renderer_command(&mut console, &[]).unwrap();
        assert!(lines[0].contains("Vanilla"), "{lines:?}");
        renderer_command(&mut console, &["SJK".to_owned()]).unwrap();
        assert_eq!(Level::current(&console), Some(Level::High));
        assert!(renderer_command(&mut console, &["rtx".to_owned()]).is_err());
    }
}

//! OpenJK cl_console.cpp:492-502,610-612,737-773,1038-1067.
//! notifylines extends it with TaystJK cl_console.cpp:650-668.
use super::*;

/// Archived cvar naming the console style.
pub(crate) const STYLE_CVAR: &str = "con_style";
/// Internal marker of the one-time move of a saved `classic` to `auto`.
pub(crate) const STYLE_VERSION_CVAR: &str = "con_styleDefaultVersion";
/// Archived switch for the console feed, the notify lines at the top left
/// while the console is closed; off by default. Chat never goes through them.
pub(crate) const DRAW_NOTIFY_CVAR: &str = "con_drawNotify";

/// How the console looks and behaves (`con_style`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ConsoleStyle {
    /// After EternalJK (`cl_console.cpp`): the `console` shader's background,
    /// a monospaced character grid, timestamps, a clock and the version line.
    /// What `auto` gives with the classic menus.
    #[default]
    Classic,
    /// The SJK UI's console (`sjk`, the deck): the classic console's grid,
    /// keys and behaviour in a full-width navy panel with a header, a framed
    /// input band and a lit rail along its bottom edge, in the SJK UI's colours
    /// and type ([`super::sjk`]). What `auto` gives with the SJK UI's menus.
    Sjk,
}

impl ConsoleStyle {
    /// Values the settings screen offers: `auto` first, then each look.
    pub(crate) const NAMES: [&'static str; 3] = ["auto", "sjk", "classic"];
    /// The `con_style` value of a new profile: follow the menu style.
    pub(crate) const DEFAULT_NAME: &'static str = Self::NAMES[0];

    /// The look the cvar `value` gives; `sjk_menus` is whether the menus are
    /// the SJK UI (`ui_menuStyle sjk`). `classic` is the classic console, `sjk`
    /// the SJK UI's; `auto`, no value or any other one follow the menus: the
    /// SJK UI's console with its menus, the classic console otherwise.
    /// `horizon` and `dock`, two retired SJK designs, are among the others, as
    /// is `modern` (or `0`), the retired modern console.
    pub(crate) fn resolve(value: Option<&str>, sjk_menus: bool) -> Self {
        let text = value.map(str::trim).unwrap_or_default();
        let is = |name: &str| text.eq_ignore_ascii_case(name);
        if is("classic") {
            Self::Classic
        } else if is("sjk") || sjk_menus {
            Self::Sjk
        } else {
            Self::Classic
        }
    }

    /// Whether this is the SJK UI's console.
    pub(crate) const fn is_sjk(self) -> bool {
        matches!(self, Self::Sjk)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Options {
    pub style: ConsoleStyle,
    /// `con_ratioFix`: a half-height or lower classic console shows the middle
    /// of its background picture instead of squashing all of it.
    pub ratio_fix: bool,
    pub height: f32,
    pub scale: f32,
    pub opacity: f32,
    pub speed: f32,
    pub timestamps: i64,
    /// `con_drawNotify`: whether the closed console draws its notify lines.
    pub draw_notify: bool,
    pub notify_millis: u64,
    pub notify_lines: usize,
    /// Notify-only horizontal displacement, in virtual 640-wide coordinates.
    pub notify_x: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            style: ConsoleStyle::Classic,
            ratio_fix: true,
            height: 0.5,
            scale: 1.0,
            opacity: 1.0,
            speed: 3.0,
            timestamps: 0,
            draw_notify: false,
            notify_millis: 3000,
            notify_lines: 3,
            notify_x: 0.0,
        }
    }
}

pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value, help) in [
        ("con_notifytime", 3.0, "Notify lifetime in seconds"),
        ("con_opacity", 1.0, "Console background opacity"),
        ("con_scale", 1.0, "Console font scale"),
        (
            "con_height",
            0.5,
            "Open console height as a screen fraction",
        ),
        ("scr_conspeed", 3.0, "Console opening/closing speed"),
    ] {
        let flags = if matches!(name, "con_notifytime" | "scr_conspeed") {
            CvarFlags::NONE
        } else {
            CvarFlags::ARCHIVE
        };
        cvars.register(CvarDefinition::new(name, value, flags, help))?;
    }
    cvars.register(CvarDefinition::new(
        STYLE_CVAR,
        ConsoleStyle::DEFAULT_NAME,
        CvarFlags::ARCHIVE,
        "Console style: auto (the SJK UI's with its menus, else classic), sjk or classic \
         (after EternalJK)",
    ))?;
    cvars.register(CvarDefinition::new(
        STYLE_VERSION_CVAR,
        0_i64,
        CvarFlags::ARCHIVE,
        "Internal migration marker for the auto console style default",
    ))?;
    cvars.register(CvarDefinition::new(
        DRAW_NOTIFY_CVAR,
        false,
        CvarFlags::ARCHIVE,
        "Console feed: show the newest console lines at the top left while playing (chat          keeps its own box)",
    ))?;
    for (name, value, help) in [
        ("con_notifylines", 3_i64, "Maximum visible notify lines"),
        (
            "con_timestamps",
            0,
            "Timestamps: 0 off, 1 console and notify, 2 console only (EternalJK style)",
        ),
        (
            "con_ratioFix",
            1,
            "Classic console: a console of half the screen or less shows the middle of              its background instead of squashing it; disable for custom backgrounds",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for (name, value, help) in [
        ("con_autoclear", true, "Clear console input when closing"),
        ("cl_noprint", false, "Suppress console output"),
    ] {
        let flags = if name == "cl_noprint" {
            CvarFlags::NONE
        } else {
            CvarFlags::ARCHIVE
        };
        cvars.register(CvarDefinition::new(name, value, flags, help))?;
    }
    Ok(())
}

impl ViewerConsole {
    /// The player's `con_style`.
    pub(crate) fn console_style(&self) -> ConsoleStyle {
        let menus =
            crate::menu::style::MenuStyle::from_cvar(self.text_value(crate::menu::style::CVAR));
        ConsoleStyle::resolve(
            self.text_value(STYLE_CVAR),
            menus == crate::menu::style::MenuStyle::Sjk,
        )
    }

    pub(super) fn options(&self) -> Options {
        Options {
            style: self.console_style(),
            ratio_fix: self.integer_cvar("con_ratioFix").unwrap_or(1) != 0,
            notify_x: self.float_cvar("cl_conxoffset").unwrap_or(0.0) as f32,
            height: self.float_cvar("con_height").unwrap_or(0.5).clamp(0.0, 1.0) as f32,
            scale: self
                .float_cvar("con_scale")
                .filter(|value| *value > 0.0)
                .unwrap_or(1.0) as f32,
            opacity: self
                .float_cvar("con_opacity")
                .unwrap_or(1.0)
                .clamp(0.0, 1.0) as f32,
            speed: self
                .float_cvar("scr_conspeed")
                .unwrap_or(3.0)
                .clamp(1.0, 100.0) as f32,
            timestamps: self.integer_cvar("con_timestamps").unwrap_or(0),
            draw_notify: self.bool_cvar(DRAW_NOTIFY_CVAR).unwrap_or(false),
            notify_millis: (self.float_cvar("con_notifytime").unwrap_or(3.0).max(0.0) * 1000.0)
                as u64,
            notify_lines: self
                .integer_cvar("con_notifylines")
                .unwrap_or(3)
                .clamp(0, 64) as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_and_mistyped_values_follow_the_menus() {
        // `horizon`, `dock` and `modern` (or `0`): retired designs a profile
        // may have saved.
        for value in [
            None,
            Some("auto"),
            Some(" AUTO "),
            Some("modren"),
            Some("horizon"),
            Some("Dock"),
            Some(" Modern "),
            Some("0"),
        ] {
            assert_eq!(ConsoleStyle::resolve(value, false), ConsoleStyle::Classic);
            assert_eq!(ConsoleStyle::resolve(value, true), ConsoleStyle::Sjk);
        }
        assert_eq!(
            ConsoleStyle::resolve(Some(ConsoleStyle::DEFAULT_NAME), false),
            ConsoleStyle::default()
        );
    }

    #[test]
    fn a_named_look_wins_over_the_menus() {
        for menus in [false, true] {
            let resolve = |value| ConsoleStyle::resolve(Some(value), menus);
            assert_eq!(resolve("classic"), ConsoleStyle::Classic);
            assert_eq!(resolve(" SJK "), ConsoleStyle::Sjk);
        }
    }

    #[test]
    fn offered_names_parse_to_distinct_looks() {
        let looks = ConsoleStyle::NAMES[1..]
            .iter()
            .map(|name| ConsoleStyle::resolve(Some(name), false))
            .collect::<Vec<_>>();
        for (index, look) in looks.iter().enumerate() {
            assert!(!looks[..index].contains(look), "{look:?} twice");
        }
    }

    /// The console feed is off in a new profile, and a saved choice is kept.
    #[test]
    fn the_console_feed_is_off_by_default_and_saved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert!(!console.options().draw_notify);
        assert!(!Options::default().draw_notify);
        assert!(console.set_cvar(DRAW_NOTIFY_CVAR, "1"));
        assert!(console.options().draw_notify);
        drop(console);
        let console = ViewerConsole::new(path).unwrap();
        assert!(console.options().draw_notify);
    }

    /// The registered default, a saved `classic` from before `auto` existed and
    /// a classic chosen afterwards.
    #[test]
    fn a_saved_classic_moves_once_to_auto() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(STYLE_CVAR), Some("auto"));
        assert_eq!(console.console_style(), ConsoleStyle::Sjk);
        console.set_cvar(crate::menu::style::CVAR, "classic");
        assert_eq!(console.console_style(), ConsoleStyle::Classic);
        drop(console);
        std::fs::write(
            &path,
            "seta ui_menuStyle \"sjk\"\nseta con_style \"classic\"\n",
        )
        .unwrap();
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(STYLE_CVAR), Some("auto"));
        assert_eq!(console.console_style(), ConsoleStyle::Sjk);
        assert!(console.set_cvar(STYLE_CVAR, "classic"));
        drop(console);
        let console = ViewerConsole::new(path).unwrap();
        assert_eq!(console.text_value(STYLE_CVAR), Some("classic"));
        assert_eq!(console.console_style(), ConsoleStyle::Classic);
    }
}

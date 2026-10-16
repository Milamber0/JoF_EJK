//! Settings' two modes (`docs/sjk-ui.md`, Settings): Simple, the default, shows
//! the settings that matter most in a few plain categories; Advanced shows every
//! setting. The search finds every setting in both.

use crate::console::ViewerConsole;

/// The setting that keeps the mode.
pub(crate) const CVAR: &str = "ui_settingsMode";

/// Which settings the SJK UI's Settings shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SettingsMode {
    /// The settings that matter most (the default).
    #[default]
    Simple,
    /// Every setting.
    Advanced,
}

impl SettingsMode {
    /// Both, in the order the top bar shows them.
    pub(crate) const ALL: [Self; 2] = [Self::Simple, Self::Advanced];

    /// The mode's name in its cvar.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Advanced => "advanced",
        }
    }

    /// The mode's name on its segment.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Advanced => "Advanced",
        }
    }

    /// The mode's name on the key hint that switches to it (F2).
    pub(crate) const fn hint(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Advanced => "advanced",
        }
    }

    /// The mode `console` keeps; anything but "advanced" is Simple.
    pub(crate) fn of(console: &ViewerConsole) -> Self {
        match console.text_value(CVAR) {
            Some(name) if name.trim().eq_ignore_ascii_case("advanced") => Self::Advanced,
            _ => Self::Simple,
        }
    }

    /// Keep this mode.
    pub(crate) fn store(self, console: &mut ViewerConsole) {
        console.set_cvar(CVAR, self.name());
    }

    /// The other mode.
    pub(crate) const fn other(self) -> Self {
        match self {
            Self::Simple => Self::Advanced,
            Self::Advanced => Self::Simple,
        }
    }

    /// The mode as settings usage names it.
    pub(crate) const fn usage(self) -> sjk_identity::usage::Mode {
        match self {
            Self::Simple => sjk_identity::usage::Mode::Simple,
            Self::Advanced => sjk_identity::usage::Mode::Advanced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_profile_is_simple_and_the_mode_is_kept() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(SettingsMode::of(&console), SettingsMode::Simple);
        SettingsMode::Advanced.store(&mut console);
        drop(console);
        let console = ViewerConsole::new(path).unwrap();
        assert_eq!(SettingsMode::of(&console), SettingsMode::Advanced);
        assert_eq!(SettingsMode::Advanced.other(), SettingsMode::Simple);
    }
}

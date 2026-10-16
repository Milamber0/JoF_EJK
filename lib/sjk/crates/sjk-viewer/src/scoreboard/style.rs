//! The `cg_scoreboardStyle` setting and the classic scoreboard's options.
//!
//! `classic` follows the retail scoreboard as EternalJK-derived clients draw it
//! ([`super::classic`]),
//! with their `cg_smallScoreboard`, `cg_showClientIDs`,
//! `cg_drawScoreboardIcons` and `cg_drawScoreboardPlayerCount` options; `sjk`
//! is the SJK UI's own ([`super::sjk`]). `auto`, the default, follows the menu
//! style: the SJK UI's scoreboard while `ui_menuStyle` is `sjk`, the classic one
//! otherwise (what SJK drew before the choice existed). Every profile had saved
//! the old default `classic`, so it moves once to `auto`
//! (`cg_scoreboardStyleDefaultVersion`, in the console's start); a look chosen
//! after that keeps it whatever the menu style. `cg_compactScoreboard` (on by
//! default) packs the SJK look's rows so every player fits one column, in a
//! board as wide as its names, centred.

use crate::console::ViewerConsole;
use crate::menu::style::MenuStyle;

/// Archived cvar naming the scoreboard style.
pub(crate) const CVAR: &str = "cg_scoreboardStyle";
/// Archived cvar: the SJK look's thin rows, every player in one column.
pub(crate) const COMPACT_CVAR: &str = "cg_compactScoreboard";

/// Whether the SJK look packs its rows (`cg_compactScoreboard`, on unless
/// switched off).
pub(crate) fn compact(console: Option<&ViewerConsole>) -> bool {
    console
        .and_then(|console| console.bool_cvar(COMPACT_CVAR))
        .unwrap_or(true)
}

/// Whether the SJK look shows the frame rate on its header line, left of the clock
/// (Sol's request, 11/10/2026: on by default).
pub(crate) const FPS_CVAR: &str = "cg_scoreboardFps";

/// Whether the SJK look shows the frame rate (`cg_scoreboardFps`, on unless set off).
pub(crate) fn fps(console: Option<&ViewerConsole>) -> bool {
    console
        .and_then(|console| console.bool_cvar(FPS_CVAR))
        .unwrap_or(true)
}

/// Layout family of the scoreboard, as drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ScoreboardStyle {
    /// The retail layout: centred columns, team bands and the client ID.
    #[default]
    Classic,
    /// The SJK UI's: columns floating over the dimmed game, in its type.
    Sjk,
}

impl ScoreboardStyle {
    /// Values the settings screen offers: `auto` first, then each look.
    pub(crate) const NAMES: [&'static str; 3] = ["auto", "sjk", "classic"];
    /// The `cg_scoreboardStyle` value of a new profile: follow the menu style.
    pub(crate) const DEFAULT_NAME: &'static str = Self::NAMES[0];

    /// The look the cvar `value` gives under the menu style `menus`: `sjk` the
    /// SJK UI's, `classic` the classic one; `auto` (or no value) the SJK UI's
    /// while the menus are the SJK UI and the classic one otherwise. A mistyped
    /// value keeps the classic board, as it did before `auto` and `sjk` existed.
    pub(crate) fn resolve(value: Option<&str>, menus: MenuStyle) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("sjk") => Self::Sjk,
            None => Self::follow(menus),
            Some(text) if text.eq_ignore_ascii_case("auto") => Self::follow(menus),
            _ => Self::Classic,
        }
    }

    /// What `auto` draws under `menus`.
    fn follow(menus: MenuStyle) -> Self {
        if menus == MenuStyle::Sjk {
            Self::Sjk
        } else {
            Self::Classic
        }
    }

    /// The player's current look.
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        let menus = MenuStyle::from_cvar(
            console.and_then(|console| console.text_value(crate::menu::style::CVAR)),
        );
        Self::resolve(console.and_then(|console| console.text_value(CVAR)), menus)
    }
}

/// Classic scoreboard options, sampled once per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClassicOptions {
    /// `cg_smallScoreboard`: always use the interleaved (small) rows.
    pub(crate) small: bool,
    /// `cg_showClientIDs`: the client ID column.
    pub(crate) client_ids: bool,
    /// `cg_drawScoreboardIcons`: each player's head icon before the name.
    pub(crate) icons: bool,
    /// `cg_drawScoreboardPlayerCount`: 0 off, 1 host name and counts,
    /// 2 counts only (team games always show "N vs. M").
    pub(crate) player_count: i64,
}

impl Default for ClassicOptions {
    fn default() -> Self {
        Self {
            small: false,
            client_ids: true,
            icons: true,
            player_count: 1,
        }
    }
}

impl ClassicOptions {
    /// Read the options, keeping the defaults for missing cvars.
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        let defaults = Self::default();
        let Some(console) = console else {
            return defaults;
        };
        Self {
            small: console
                .bool_cvar("cg_smallScoreboard")
                .unwrap_or(defaults.small),
            client_ids: console
                .bool_cvar("cg_showClientIDs")
                .unwrap_or(defaults.client_ids),
            icons: console
                .bool_cvar("cg_drawScoreboardIcons")
                .unwrap_or(defaults.icons),
            player_count: console
                .integer_cvar("cg_drawScoreboardPlayerCount")
                .unwrap_or(defaults.player_count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_MENU: [MenuStyle; 2] = [MenuStyle::Classic, MenuStyle::Sjk];

    #[test]
    fn a_mistyped_value_keeps_the_classic_board() {
        for menus in EVERY_MENU {
            let resolve = |value| ScoreboardStyle::resolve(value, menus);
            assert_eq!(resolve(Some("")), ScoreboardStyle::Classic);
            assert_eq!(resolve(Some("modrn")), ScoreboardStyle::Classic);
        }
    }

    #[test]
    fn auto_follows_the_menu_style() {
        for value in [None, Some("auto"), Some(" AUTO ")] {
            assert_eq!(
                ScoreboardStyle::resolve(value, MenuStyle::Sjk),
                ScoreboardStyle::Sjk
            );
            // The classic menus keep the board SJK drew before: classic.
            assert_eq!(
                ScoreboardStyle::resolve(value, MenuStyle::Classic),
                ScoreboardStyle::Classic
            );
        }
    }

    #[test]
    fn a_saved_look_wins_over_the_menu_style() {
        for menus in EVERY_MENU {
            assert_eq!(
                ScoreboardStyle::resolve(Some("classic"), menus),
                ScoreboardStyle::Classic
            );
            assert_eq!(
                ScoreboardStyle::resolve(Some(" SJK "), menus),
                ScoreboardStyle::Sjk
            );
        }
    }

    #[test]
    fn offered_names_parse_in_order() {
        let parsed = ScoreboardStyle::NAMES
            .map(|name| ScoreboardStyle::resolve(Some(name), MenuStyle::Classic));
        assert_eq!(
            parsed,
            [
                ScoreboardStyle::Classic,
                ScoreboardStyle::Sjk,
                ScoreboardStyle::Classic
            ]
        );
        assert_eq!(ScoreboardStyle::DEFAULT_NAME, "auto");
    }

    /// The console's registered default, a saved `classic` and the menu style
    /// switched between the SJK UI and classic: a new profile follows it, a
    /// saved choice stays.
    #[test]
    fn the_console_default_follows_the_menus_and_a_saved_value_stays() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(CVAR), Some("auto"));
        // The SJK UI is the default menu style, so its board is too.
        assert_eq!(
            ScoreboardStyle::from_console(Some(&console)),
            ScoreboardStyle::Sjk
        );
        console.set_cvar(crate::menu::style::CVAR, "classic");
        assert_eq!(
            ScoreboardStyle::from_console(Some(&console)),
            ScoreboardStyle::Classic
        );
        // Every profile saved the old default `classic` before `auto` existed:
        // it moves once to `auto`, so it follows the SJK UI's menus.
        drop(console);
        std::fs::write(
            &path,
            "seta ui_menuStyle \"sjk\"\nseta cg_scoreboardStyle \"classic\"\n",
        )
        .unwrap();
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.text_value(CVAR), Some("auto"));
        assert_eq!(
            ScoreboardStyle::from_console(Some(&console)),
            ScoreboardStyle::Sjk
        );
        // A classic chosen after the move stays, start after start.
        assert!(console.set_cvar(CVAR, "classic"));
        drop(console);
        let console = ViewerConsole::new(path).unwrap();
        assert_eq!(console.text_value(CVAR), Some("classic"));
        assert_eq!(
            ScoreboardStyle::from_console(Some(&console)),
            ScoreboardStyle::Classic
        );
    }
}

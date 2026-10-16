//! What Settings tells [`crate::settings_usage`]: each setting the player
//! changes, from what to what as its row shows it, and each search once it
//! settles. Every change goes through [`SettingsMenu::refresh`], so the rows'
//! values before and after a key or a click are compared there; only the row
//! the player acted on is recorded (a quality level changes many rows at once).

use super::classic_view::choice_label;
use super::mode::SettingsMode;
use super::*;

/// What a text row records in place of its value: an address or a key is
/// never sent.
const TEXT_VALUE: &str = "(text)";

/// Settings' record of what the player does, for settings usage.
#[derive(Default)]
pub(super) struct UsageTrack {
    /// A key or a click is being handled: changes the rows show are the
    /// player's.
    pub(super) recording: bool,
    /// Which rows [`SettingsMenu::values`] were read for (their address and
    /// count), so values of other rows are never compared.
    pub(super) rows: (usize, usize),
    /// The longest search of the one being typed, recorded when it settles.
    peak: String,
    /// The search recorded last, not recorded twice in a row.
    recorded: String,
}

impl SettingsMenu {
    /// The SJK UI's Settings shows the rows in `mode` under category `label`;
    /// `None` when they are a classic+ panel's (which shows every setting).
    pub(crate) fn set_screen(&mut self, mode: Option<SettingsMode>, label: &'static str) {
        self.screen = mode;
        self.screen_label = label;
    }

    /// The mode settings usage names: the SJK UI's, or Advanced for a classic+
    /// panel, which shows every setting.
    fn usage_mode(&self) -> SettingsMode {
        self.screen.unwrap_or(SettingsMode::Advanced)
    }

    /// Where row `row` was shown: a search's group, the SJK UI's category or
    /// the classic+ tab.
    fn usage_group(&self, row: usize) -> &'static str {
        if self.section == Section::Search {
            return search::group(row).unwrap_or("Search");
        }
        if !self.screen_label.is_empty() {
            return self.screen_label;
        }
        self.tabs().get(self.tab).copied().unwrap_or_default()
    }

    /// Row `row`'s value `value` as settings usage records it: as its row
    /// names it, and never a text setting's own.
    fn usage_value(&self, row: usize, value: &str) -> String {
        let Some(setting) = self.rows().get(row) else {
            return value.to_owned();
        };
        match setting.kind {
            ValueKind::Text => TEXT_VALUE.to_owned(),
            ValueKind::Bool => {
                crate::menu::classic::view::Sentence(&value.to_ascii_lowercase()).to_string()
            }
            ValueKind::Choice(_) => choice_label(setting.cvar, value).to_owned(),
            _ if value == "AUTO" => "Auto".to_owned(),
            _ => value.to_owned(),
        }
    }

    /// The rows' values changed from `before` while the player acted: record
    /// the change of the row they acted on, if it changed.
    pub(super) fn record_change(&mut self, before: &[String]) {
        let row = self.selected;
        let (Some(old), Some(new)) = (before.get(row), self.values.get(row)) else {
            return;
        };
        if old == new {
            return;
        }
        let Some(setting) = self.rows().get(row).copied() else {
            return;
        };
        let (from, to) = (self.usage_value(row, old), self.usage_value(row, new));
        let searching = self.section == Section::Search;
        if searching {
            self.settle_search();
        }
        let search = searching.then(|| self.search.trim().to_owned());
        crate::settings_usage::change(
            self.usage_mode().usage(),
            self.usage_group(row),
            setting.cvar,
            &from,
            &to,
            search.as_deref().filter(|search| !search.is_empty()),
        );
    }

    /// The search changes to `next`: a search that went another way than the
    /// one typed (not longer, not shorter) settles the one before.
    pub(super) fn search_moves(&mut self, next: &str) {
        let next = next.trim();
        let peak = self.usage.peak.as_str();
        if next.starts_with(peak) {
            self.usage.peak = next.to_owned();
        } else if !peak.starts_with(next) {
            self.settle_search();
            self.usage.peak = next.to_owned();
        }
    }

    /// Record the search being typed, at its longest, and how many settings it
    /// finds: the field was left, a result changed, or Settings closed.
    pub(super) fn settle_search(&mut self) {
        let query = std::mem::take(&mut self.usage.peak);
        let query = query.trim();
        if query.is_empty() || query == self.usage.recorded {
            return;
        }
        crate::settings_usage::search(self.usage_mode().usage(), query, search::count(query));
        query.clone_into(&mut self.usage.recorded);
        // A search going on again from here starts from what is typed.
        self.usage.peak = self.search.trim().to_owned();
    }

    /// After a key or a click: a search whose field was left settles, and
    /// Settings closing sends what was recorded.
    pub(super) fn after_input(
        &mut self,
        console: &ViewerConsole,
        was_searching: bool,
        result: &SettingsResult,
    ) {
        if was_searching && !self.searching {
            self.settle_search();
        }
        if matches!(result, SettingsResult::Back) {
            self.settle_search();
            crate::settings_usage::flush(console);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::usage::Event;
    use winit::keyboard::KeyCode;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    #[test]
    fn a_change_records_the_row_acted_on_and_a_settled_search() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_classic_group(
            &console,
            Group::Simple(super::super::Simple::Display),
            crate::menu::classic::panel::Frame::Main,
        );
        menu.set_screen(Some(SettingsMode::Simple), "Display");
        menu.selected = menu
            .rows()
            .iter()
            .position(|setting| setting.cvar == "cg_fov")
            .unwrap();
        menu.usage.recording = true;
        menu.adjust(&mut console, 1);
        menu.adjust(&mut console, 1);
        menu.usage.recording = false;
        // A search typed, then left.
        menu.searching = true;
        for key in [KeyCode::KeyF, KeyCode::KeyO, KeyCode::KeyV] {
            let text = match key {
                KeyCode::KeyF => "f",
                KeyCode::KeyO => "o",
                _ => "v",
            };
            menu.search_key(key, Some(text), &console);
        }
        menu.search_key(KeyCode::Backspace, None, &console);
        menu.search_key(KeyCode::Enter, None, &console);
        menu.after_input(&console, true, &SettingsResult::None);
        let events = crate::settings_usage::take_for_test();
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(
            matches!(
                &events[0],
                Event::Change { setting, group, from, to, search: None, .. }
                    if setting == "cg_fov" && group == "Display" && from == "90.0" && to == "100.0"
            ),
            "{events:?}"
        );
        assert!(
            matches!(
                &events[1],
                Event::Search { query, results, mode: sjk_identity::usage::Mode::Simple, .. }
                    if query == "fov" && *results >= 1
            ),
            "{events:?}"
        );
    }

    #[test]
    fn a_text_setting_never_records_its_value() {
        let (_directory, console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_classic(
            &console,
            SettingsMenu::tab_index("NETWORK").unwrap(),
            crate::menu::classic::layout::Span::ALL,
            crate::menu::classic::panel::Frame::Main,
        );
        let key = menu
            .rows()
            .iter()
            .position(|setting| setting.cvar == crate::chat_gifs::search::KEY_CVAR)
            .unwrap();
        assert_eq!(menu.usage_value(key, "secret-key"), TEXT_VALUE);
        let remaps = super::super::search::rows()
            .iter()
            .position(|setting| setting.cvar == "cg_remaps");
        assert!(remaps.is_some());
    }
}

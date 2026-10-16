//! Test list of every change in Sol's build, toggled with the `debug_panel` console
//! command.
//!
//! **Personal to Sol's build (SJK's `main` branch).** Each entry names its area, says
//! in a line or two what changed and lists the steps to test it. Entries can be ticked
//! as tested; the ticks are saved by entry id in `debug_panel_tested.txt` beside
//! `config.cfg` (see `debug_panel_ticks.rs`), so they survive restarts and edits.
//!
//! The list is `assets/debug_panel.txt`, built into the client and parsed once when
//! the console starts (`debug_panel_data.rs`, whose tests check the file). It keeps
//! the file's order on every tab; the tabs filter it: TO TEST (the default, so a
//! ticked entry drops out and the next one is selected), TESTED and ALL. Filtering
//! rather than sorting keeps every entry in the same place relative to its
//! neighbours, so ticking never reshuffles the list under the pointer.
//!
//! Like the command and cvar browser, the panel lives in the console and is drawn in
//! place of it, so it opens over the main menu, the in-game menus and a live match.
//! Run with the console closed (a bound key), it opens the console behind itself and
//! closes it again when the panel closes. It is drawn in the SJK UI's look in every
//! menu style (`debug_panel_view.rs`, routed by `console_sjk_pages.rs`), pointer
//! input in `debug_panel_pointer.rs`.

use crate::menu_widgets::MenuCanvas;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "debug_panel_data.rs"]
mod data;
#[path = "debug_panel_pointer.rs"]
mod pointer;
#[path = "debug_panel_ticks.rs"]
mod ticks;
#[path = "debug_panel_view.rs"]
mod view;

/// Console command that toggles the panel; keys bound to it toggle it too.
pub(crate) const COMMAND: &str = "debug_panel";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "Toggle the test list of Sol's build, with tested ticks";

/// Filter tabs; `Tab` and `Shift+Tab` cycle them.
const TABS: [&str; 3] = ["To test", "Tested", "All"];
/// Rows one wheel notch scrolls.
const WHEEL_ROWS: isize = 3;

/// What the console does after the panel handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    /// Close the panel.
    Close,
}

/// A keyboard command, separated from winit events so it can be tested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Move(isize),
    Page(isize),
    Toggle,
    Tab(isize),
    Close,
}

pub(crate) struct Panel {
    open: bool,
    /// The panel opened the console, so closing the panel closes it too.
    owns_console: bool,
    entries: Vec<data::Entry>,
    /// Tick of each entry, by index into `entries`.
    tested: Vec<bool>,
    /// Ticked ids that are not in the list (removed or renamed entries), kept on save.
    kept: Vec<String>,
    /// Indices into `entries` that the tab keeps, in list order.
    visible: Vec<usize>,
    tab: usize,
    /// Index into `visible`.
    selected: usize,
    /// First visible row on screen.
    first: usize,
    /// Rows that fit, as laid out by the last `append_sjk`.
    rows: usize,
    /// `"<ticked> of <total> tested"` at the top right.
    summary: String,
    /// The chosen entry's lines wrapped to the detail column, reused every frame.
    lines: Vec<view::Line>,
    /// Outcome of the last tick, or a load or save error, shown at the bottom left.
    status: String,
    status_error: bool,
    path: PathBuf,
    ui: MenuCanvas,
}

impl Panel {
    /// Parse the built-in list and load the ticks saved in `config_directory`.
    pub(crate) fn new(config_directory: &Path) -> Self {
        let path = config_directory.join(ticks::FILE_NAME);
        let (entries, mut status, mut status_error) = match data::parse(data::EMBEDDED) {
            Ok(entries) => (entries, String::new(), false),
            Err(error) => (Vec::new(), error.to_string(), true),
        };
        let saved = ticks::load(&path).unwrap_or_else(|error| {
            status = format!("Could not read {}: {error}", path.display());
            status_error = true;
            Vec::new()
        });
        let mut panel = Self {
            open: false,
            owns_console: false,
            tested: vec![false; entries.len()],
            entries,
            kept: Vec::new(),
            visible: Vec::new(),
            tab: 0,
            selected: 0,
            first: 0,
            rows: 1,
            summary: String::new(),
            lines: Vec::with_capacity(64),
            status,
            status_error,
            path,
            ui: MenuCanvas::with_text_capacity(192),
        };
        panel.apply_saved(saved);
        panel.rebuild_visible(None);
        panel
    }

    /// Tick the entries named in `saved`, keeping ids the list does not have.
    fn apply_saved(&mut self, saved: Vec<String>) {
        for id in saved {
            match self.entries.iter().position(|entry| entry.id == id) {
                Some(index) => self.tested[index] = true,
                None => self.kept.push(id),
            }
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the panel; `owns_console` when the console was closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        if !self.status_error {
            self.status.clear();
        }
        self.reveal_selection();
    }

    /// Hide the panel; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Handle a key while the panel is open; `shift` is the console's Shift state.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent, shift: bool) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        let step = match key {
            KeyCode::Escape => Step::Close,
            KeyCode::ArrowUp => Step::Move(-1),
            KeyCode::ArrowDown => Step::Move(1),
            KeyCode::PageUp => Step::Page(-1),
            KeyCode::PageDown => Step::Page(1),
            KeyCode::Home => Step::Move(isize::MIN),
            KeyCode::End => Step::Move(isize::MAX),
            KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter if !event.repeat => Step::Toggle,
            KeyCode::Tab => Step::Tab(if shift { -1 } else { 1 }),
            _ => return PanelAction::None,
        };
        self.apply(step)
    }

    fn apply(&mut self, step: Step) -> PanelAction {
        match step {
            Step::Close => return PanelAction::Close,
            Step::Move(rows) => self.move_selection(rows),
            Step::Page(pages) => self.move_selection(pages * self.rows.max(1) as isize),
            Step::Toggle => self.toggle_selected(),
            Step::Tab(direction) => {
                let count = TABS.len() as isize;
                self.set_tab((self.tab as isize + direction).rem_euclid(count) as usize);
            }
        }
        PanelAction::None
    }

    /// Index into `entries` of the selected row.
    fn selected_entry(&self) -> Option<usize> {
        self.visible.get(self.selected).copied()
    }

    fn in_tab(&self, index: usize) -> bool {
        match self.tab {
            0 => !self.tested[index],
            1 => self.tested[index],
            _ => true,
        }
    }

    /// Apply the tab, then select `keep` again if it is still shown, else stay at
    /// the same position in the list.
    fn rebuild_visible(&mut self, keep: Option<usize>) {
        let position = self.selected;
        self.visible.clear();
        for index in 0..self.entries.len() {
            if self.in_tab(index) {
                self.visible.push(index);
            }
        }
        self.selected = keep
            .and_then(|keep| self.visible.iter().position(|&index| index == keep))
            .unwrap_or_else(|| position.min(self.visible.len().saturating_sub(1)));
        self.reveal_selection();
        let ticked = self.tested.iter().filter(|&&tested| tested).count();
        self.summary.clear();
        let _ = write!(self.summary, "{ticked} of {} tested", self.entries.len());
    }

    fn set_tab(&mut self, tab: usize) {
        self.tab = tab % TABS.len();
        let keep = self.selected_entry();
        self.first = 0;
        self.rebuild_visible(keep);
    }

    /// Flip the selected entry's tick and save the ticks.
    fn toggle_selected(&mut self) {
        let Some(index) = self.selected_entry() else {
            return;
        };
        self.tested[index] = !self.tested[index];
        let entry = &self.entries[index];
        let verb = if self.tested[index] {
            "Ticked"
        } else {
            "Unticked"
        };
        self.status = format!("{verb} {}", entry.title);
        self.status_error = false;
        if let Err(error) = self.save() {
            self.status = format!("Could not save {}: {error}", self.path.display());
            self.status_error = true;
        }
        // On a filtered tab the entry leaves the list; the next one takes its row.
        self.rebuild_visible(Some(index).filter(|_| self.tab == 2));
    }

    fn save(&self) -> std::io::Result<()> {
        let ticked = self
            .entries
            .iter()
            .zip(&self.tested)
            .filter(|(_, tested)| **tested)
            .map(|(entry, _)| entry.id.as_str());
        let contents = ticks::render(ticked.chain(self.kept.iter().map(String::as_str)));
        ticks::save(&self.path, &contents)
    }

    /// Move the selection by `rows` (negative = up), clamped to the list.
    fn move_selection(&mut self, rows: isize) {
        let last = self.visible.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
        self.reveal_selection();
    }

    /// Scroll so the selection is on screen.
    fn reveal_selection(&mut self) {
        let rows = self.rows.max(1);
        self.first = self
            .first
            .min(self.selected)
            .max((self.selected + 1).saturating_sub(rows));
    }

    /// Scroll by `rows` (negative = up) without moving the selection.
    fn scroll_by(&mut self, rows: isize) {
        let max_first = self.visible.len().saturating_sub(self.rows);
        self.first = self.first.saturating_add_signed(rows).min(max_first);
    }

    /// Select the first entry with notes on the ALL tab, for a world shot.
    #[cfg(test)]
    pub(crate) fn select_noted_for_shot(&mut self) {
        self.set_tab(2);
        if let Some(position) = self
            .visible
            .iter()
            .position(|&index| !self.entries[index].notes.is_empty())
        {
            self.selected = position;
            self.reveal_selection();
        }
        self.status = format!("Ticked {}", self.entries[self.visible[0]].title);
    }

    /// Scroll so the list shows `ratio` (0 = top, 1 = bottom) of the entries.
    fn scroll_to_ratio(&mut self, ratio: f32) {
        let max_first = self.visible.len().saturating_sub(self.rows);
        self.first = (ratio.clamp(0.0, 1.0) * max_first as f32).round() as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(directory: &Path) -> Panel {
        let panel = Panel::new(directory);
        assert!(!panel.status_error, "{}", panel.status);
        panel
    }

    fn ids(panel: &Panel) -> Vec<&str> {
        panel
            .visible
            .iter()
            .map(|&index| panel.entries[index].id.as_str())
            .collect()
    }

    #[test]
    fn ticking_on_to_test_moves_to_the_next_entry_and_persists() {
        let directory = tempfile::tempdir().unwrap();
        let mut panel = panel(directory.path());
        let total = panel.entries.len();
        assert_eq!(panel.visible.len(), total);
        let first = ids(&panel)[0].to_owned();
        let second = ids(&panel)[1].to_owned();
        assert_eq!(panel.apply(Step::Toggle), PanelAction::None);
        assert_eq!(panel.visible.len(), total - 1);
        assert_eq!(ids(&panel)[panel.selected], second);
        assert!(panel.summary.starts_with("1 of "), "{}", panel.summary);

        let reopened = Panel::new(directory.path());
        let ticked = reopened.entries.iter().position(|entry| entry.id == first);
        assert_eq!(ticked.map(|index| reopened.tested[index]), Some(true));
        assert_eq!(reopened.visible.len(), total - 1);
    }

    #[test]
    fn tabs_filter_without_reordering() {
        let directory = tempfile::tempdir().unwrap();
        let mut panel = panel(directory.path());
        let all = ids(&panel)
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        panel.apply(Step::Move(2));
        panel.apply(Step::Toggle);
        panel.apply(Step::Tab(1));
        assert_eq!(ids(&panel), [all[2].as_str()]);
        panel.apply(Step::Tab(1));
        assert_eq!(ids(&panel), all);
        // ALL keeps the selection on the entry it ticks or unticks.
        panel.apply(Step::Move(isize::MIN));
        panel.apply(Step::Move(2));
        panel.apply(Step::Toggle);
        assert_eq!(ids(&panel)[panel.selected], all[2]);
        assert!(panel.tested.iter().all(|tested| !tested));
        panel.apply(Step::Tab(-1));
        assert!(ids(&panel).is_empty());
        assert_eq!(panel.selected_entry(), None);
        panel.apply(Step::Toggle);
        assert_eq!(panel.apply(Step::Close), PanelAction::Close);
    }

    #[test]
    fn unknown_saved_ids_survive_a_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(ticks::FILE_NAME);
        ticks::save(&path, &ticks::render(["retired-entry", "noclip"])).unwrap();
        let mut panel = panel(directory.path());
        assert_eq!(panel.kept, ["retired-entry"]);
        assert!(!ids(&panel).contains(&"noclip"));
        panel.apply(Step::Toggle);
        let saved = ticks::load(&path).unwrap();
        assert!(saved.contains(&"retired-entry".to_owned()));
        assert!(saved.contains(&"noclip".to_owned()));
        assert_eq!(saved.len(), 3);
    }

    #[test]
    fn selection_and_scrolling_stay_in_range() {
        let directory = tempfile::tempdir().unwrap();
        let mut panel = panel(directory.path());
        panel.rows = 5;
        let last = panel.visible.len() - 1;
        panel.apply(Step::Move(isize::MAX));
        assert_eq!(panel.selected, last);
        assert_eq!(panel.first, last + 1 - 5);
        panel.apply(Step::Page(-1));
        assert_eq!(panel.selected, last - 5);
        panel.scroll_by(isize::MIN / 2);
        assert_eq!(panel.first, 0);
        panel.scroll_to_ratio(1.0);
        assert_eq!(panel.first, last + 1 - 5);
        assert!(!panel.close());
        panel.open(true);
        assert!(panel.close());
    }
}

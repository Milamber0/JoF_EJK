//! The key bindings as a classic option panel (`controls.menu` bind items):
//! the action's label, then its keys as retail's `BindingFromName` writes
//! them ("A or B", one key, or "???"); the action being rebound turns red
//! and the description line asks for the new key.
//!
//! The panel is classic+ (`docs/classic-plus.md`): every binding is in one
//! list under its category's heading (retail kept a page per category), the
//! categories down the left jump to their heading, and a search field over
//! the list finds actions by name, command, key or category. The detail box
//! under the rows names the focused action's keys, its console command and
//! its default key, an action bound differently from its default carries a
//! mark, and the description line names the keys of the panel.

use super::*;
use crate::menu::classic::layout::Span;
use crate::menu::classic::panel::{BINDING, Detail, OPTION, PanelFrame, focus_text};
use crate::menu::classic::view::Caps;
use std::fmt::Write as _;
use winit::keyboard::KeyCode;

/// The rows' wheel target, under them, while the list scrolls; clear of the
/// action rows, the secondary slots (600), the chrome (800) and the footer
/// and scrollbar tokens (902-910).
pub(super) const ROWS_SCROLL_TOKEN: u16 = 911;

/// The categories' headings in the one list.
pub(super) const HEADINGS: [&str; 5] = [
    "Movement",
    "Interaction",
    "Weapons",
    "Force powers",
    "Other",
];

/// One row of the classic list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ListRow {
    /// A category's heading (`Category as usize`).
    Heading(usize),
    /// An action, by index into `ACTIONS`.
    Action(usize),
}

/// The classic panel's list: every action under its category's heading, or
/// those the search finds.
pub(super) struct ClassicList {
    pub(super) rows: Vec<ListRow>,
    /// What is typed in the search field, and whether it has the keyboard.
    pub(super) search: String,
    pub(super) searching: bool,
}

impl ClassicList {
    fn new() -> Self {
        let mut list = Self {
            rows: Vec::with_capacity(ACTIONS.len() + HEADINGS.len()),
            search: String::new(),
            searching: false,
        };
        list.filter(&[]);
        list
    }

    /// Rebuild the rows for the search; `keys` are the actions' keys as
    /// shown, so a key name finds what it does.
    fn filter(&mut self, keys: &[[String; 2]]) {
        self.rows.clear();
        let query = self.search.trim().to_ascii_lowercase();
        for (category, heading) in HEADINGS.iter().enumerate() {
            let heading_hit = heading.to_ascii_lowercase().contains(&query);
            let mut first = true;
            for action in category_range(category) {
                if !action_hit(action, heading_hit, &query, keys) {
                    continue;
                }
                if std::mem::take(&mut first) {
                    self.rows.push(ListRow::Heading(category));
                }
                self.rows.push(ListRow::Action(action));
            }
        }
    }

    /// Place of action `action` in the list.
    pub(super) fn position(&self, action: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| *row == ListRow::Action(action))
    }

    /// Place of category `category`'s heading.
    fn heading(&self, category: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| *row == ListRow::Heading(category))
    }

    /// Actions found.
    pub(super) fn actions(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row, ListRow::Action(_)))
            .count()
    }

    /// The first action of the list.
    fn first_action(&self) -> Option<usize> {
        self.rows.iter().find_map(|row| match row {
            ListRow::Action(action) => Some(*action),
            ListRow::Heading(_) => None,
        })
    }
}

/// Whether action `action` answers `query` (lower case, trimmed): its name,
/// console command or key, or its category's heading (`heading_hit`).
fn action_hit(action: usize, heading_hit: bool, query: &str, keys: &[[String; 2]]) -> bool {
    query.is_empty()
        || heading_hit
        || ACTIONS[action].label.to_ascii_lowercase().contains(query)
        || ACTIONS[action].command.to_ascii_lowercase().contains(query)
        || keys
            .get(action)
            .is_some_and(|keys| keys.iter().any(|key| key.eq_ignore_ascii_case(query)))
}

/// Retail `WAITING_FOR_NEW_KEY` (`mp_ingame.str`).
const WAITING: &str = "Enter new key, or ESC to cancel, BACKSPACE to clear.";
/// The description line's keys otherwise.
const KEYS: &str =
    "ENTER to bind a key   \u{b7}   BACKSPACE clears the action's keys   \u{b7}   / to search";
/// The description line while the search field has the keyboard.
const SEARCHING: &str =
    "Type to find an action, a command or a key   \u{b7}   ENTER to the results, ESC to clear";

/// Whether action `action`'s keys (`keys`, as the panel shows them) differ
/// from its default key.
pub(super) fn rebound(action: usize, keys: &[String; 2]) -> bool {
    let default = ACTIONS[action].default_key;
    let default = (!default.is_empty()).then(|| sjk_shell::key_names::display_key(default));
    let bound = keys
        .iter()
        .filter(|key| *key != "UNBOUND" && *key != "-")
        .collect::<Vec<_>>();
    match default {
        None => !bound.is_empty(),
        Some(default) => bound.len() != 1 || !bound[0].eq_ignore_ascii_case(&default),
    }
}

impl KeybindEditor {
    /// Show every binding as a classic option panel, at category
    /// `category`'s heading (its first `span` row selected). A search typed
    /// before is kept, so a category jump moves within its results.
    pub(crate) fn open_classic(&mut self, console: &ViewerConsole, category: usize, span: Span) {
        let tab = category.min(CATEGORIES.len() - 1);
        self.set_tab(tab);
        self.refresh(console);
        let mut list = self.classic.take().unwrap_or_else(ClassicList::new);
        list.searching = false;
        list.filter(&self.keys);
        let all = category_range(tab);
        let wanted = span.within(all.start, all.len()).start;
        self.selected = if list.position(wanted).is_some() {
            wanted
        } else {
            list.first_action().unwrap_or(wanted)
        };
        self.first = list
            .heading(tab)
            .or_else(|| list.position(self.selected))
            .unwrap_or(0);
        self.classic = Some(list);
        self.binding_slot = 0;
    }

    /// Search for `text` as if it had been typed, for the menu snapshots.
    #[cfg(test)]
    pub(crate) fn search_for_snapshot(&mut self, text: &str) {
        self.set_search(text.to_owned());
    }

    /// What is typed in the search field (empty when the list is closed).
    pub(crate) fn search_text(&self) -> &str {
        self.classic
            .as_ref()
            .map_or("", |list| list.search.as_str())
    }

    /// How many actions match `text`, for the options tab's search.
    pub(crate) fn count_matches(&self, text: &str) -> usize {
        let query = text.trim().to_ascii_lowercase();
        if query.is_empty() {
            return 0;
        }
        HEADINGS
            .iter()
            .enumerate()
            .map(|(category, heading)| {
                let heading_hit = heading.to_ascii_lowercase().contains(&query);
                category_range(category)
                    .filter(|action| action_hit(*action, heading_hit, &query, &self.keys))
                    .count()
            })
            .sum()
    }

    /// Options that match the search, for the panel to offer.
    pub(crate) fn set_elsewhere(&mut self, options: usize) {
        self.elsewhere = options;
    }

    /// Carry a search typed on the options tab over to this list.
    pub(crate) fn carry_search(&mut self, text: &str) {
        if self.classic.is_some() && !text.trim().is_empty() {
            self.set_search(text.to_owned());
        }
    }

    /// The category of the selected action, for the group list's mark.
    pub(crate) fn classic_category(&self) -> Option<usize> {
        self.classic.as_ref()?;
        ACTIONS
            .get(self.selected)
            .map(|action| action.category as usize)
    }

    /// The classic panel's keys: the search field takes typing while it has
    /// the keyboard; otherwise retail's keys, Left and Right through the
    /// groups, Backspace clearing the action, and `/` or Up from the top to
    /// the search.
    pub(super) fn classic_key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        console: &mut ViewerConsole,
    ) -> EditorResult {
        if self.classic.as_ref().is_some_and(|list| list.searching) {
            self.search_key(key, text);
            return EditorResult::None;
        }
        match key {
            KeyCode::Tab | KeyCode::ArrowRight | KeyCode::KeyD => EditorResult::ClassicCycle(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => EditorResult::ClassicCycle(-1),
            KeyCode::Delete | KeyCode::Backspace => {
                self.clear_both(console);
                EditorResult::None
            }
            // Ctrl+F and F3 find an action too, as in other programs.
            KeyCode::Slash | KeyCode::NumpadDivide | KeyCode::F3 => {
                self.begin_search();
                EditorResult::None
            }
            KeyCode::KeyF if console.control_held() => {
                self.begin_search();
                EditorResult::None
            }
            // The SJK UI's other mode (a classic+ page has no slot for it).
            KeyCode::F2 => EditorResult::Classic(crate::menu::sjk::settings::MODE_TOGGLE_SLOT),
            KeyCode::ArrowUp | KeyCode::KeyW
                if self
                    .classic
                    .as_ref()
                    .is_some_and(|list| list.first_action() == Some(self.selected)) =>
            {
                self.begin_search();
                EditorResult::None
            }
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.move_selection(-1);
                EditorResult::None
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                self.move_selection(1);
                EditorResult::None
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                self.begin_capture();
                EditorResult::None
            }
            KeyCode::Escape => {
                // A search still applied is cleared first.
                if self
                    .classic
                    .as_ref()
                    .is_some_and(|list| !list.search.is_empty())
                {
                    self.set_search(String::new());
                    return EditorResult::None;
                }
                EditorResult::Back
            }
            _ => EditorResult::None,
        }
    }

    /// A key while the search field has the keyboard.
    fn search_key(&mut self, key: KeyCode, text: Option<&str>) {
        let Some(list) = &self.classic else {
            return;
        };
        let mut search = list.search.clone();
        match key {
            KeyCode::Escape if search.is_empty() => self.end_search(),
            KeyCode::Escape => search.clear(),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::ArrowDown | KeyCode::Tab => {
                self.end_search();
                return;
            }
            KeyCode::Backspace => {
                search.pop();
            }
            _ => {
                if let Some(text) = text {
                    search.extend(text.chars().filter(|c| !c.is_control()).take(32));
                }
            }
        }
        self.set_search(search);
    }

    /// Give the search field the keyboard.
    pub(super) fn begin_search(&mut self) {
        if let Some(list) = &mut self.classic {
            list.searching = true;
            self.capture = false;
            // Typing or a paste replaces what was typed before.
            crate::text_entry::select_all(&list.search);
        }
    }

    /// Take the keyboard back from the search field, keeping its results.
    pub(super) fn end_search(&mut self) {
        if let Some(list) = &mut self.classic {
            list.searching = false;
        }
    }

    /// Search for `search`: the results from the top, the first one
    /// selected; an emptied search returns to the selected action's place.
    fn set_search(&mut self, search: String) {
        let Some(list) = &mut self.classic else {
            return;
        };
        list.search = search;
        list.filter(&self.keys);
        self.first = 0;
        if list.search.trim().is_empty() {
            if list.position(self.selected).is_none()
                && let Some(first) = list.first_action()
            {
                self.selected = first;
            }
            self.reveal_selected_action();
        } else if let Some(first) = list.first_action() {
            self.selected = first;
        }
    }

    /// Scroll so the selected action is on show, with its category's heading
    /// when it is the first action under it.
    fn reveal_selected_action(&mut self) {
        let Some(list) = &self.classic else {
            return;
        };
        let Some(position) = list.position(self.selected) else {
            return;
        };
        let above = position.saturating_sub(1);
        let heading = matches!(list.rows.get(above), Some(ListRow::Heading(_)));
        if heading {
            self.reveal_list(above);
        }
        self.reveal_list(position);
    }

    /// Step the selection through the list's actions, over the headings,
    /// wrapping.
    pub(super) fn classic_step(&mut self, direction: i32) {
        let Some(list) = &self.classic else {
            return;
        };
        let actions: Vec<usize> = list
            .rows
            .iter()
            .filter_map(|row| match row {
                ListRow::Action(action) => Some(*action),
                ListRow::Heading(_) => None,
            })
            .collect();
        if actions.is_empty() {
            return;
        }
        let current = actions.iter().position(|action| *action == self.selected);
        let next = match current {
            Some(index) => (index as i32 + direction).rem_euclid(actions.len() as i32) as usize,
            None => 0,
        };
        self.selected = actions[next];
        self.reveal_selected_action();
    }

    /// Scroll so list row `position` is on show.
    fn reveal_list(&mut self, position: usize) {
        let visible = self.visible.max(1);
        self.first = self
            .first
            .min(position)
            .max((position + 1).saturating_sub(visible));
    }

    /// Back to the full tabbed editor on the same category.
    pub(crate) fn leave_classic(&mut self) {
        if self.classic.take().is_some() {
            self.set_tab(self.tab);
        }
    }

    /// Draw the classic panel screen around the open span's bindings.
    pub(crate) fn append_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
        frame: &PanelFrame,
    ) {
        let mut place = frame.begin(&mut self.ui, viewport, reveal);
        if icons::any(0..ACTIONS.len()) {
            place = place.with_icon_column();
        }
        let Some(list) = self.classic.take() else {
            return;
        };
        place.search_field(
            &mut self.ui,
            &list.search,
            list.searching,
            "type to find",
            (!list.search.is_empty()).then(|| list.actions()),
        );
        self.visible = place.capacity();
        let total = list.rows.len();
        self.first = self.first.min(total.saturating_sub(self.visible));
        let shown = self.first..total.min(self.first + self.visible);
        // A list longer than the panel scrolls with the wheel anywhere over
        // its rows, not only over the thin bar.
        if total > self.visible {
            let top = place.row(0);
            self.ui.scroll_region(
                ROWS_SCROLL_TOKEN,
                Rect::new(top.x, top.y, top.width, self.visible as f32 * top.height),
            );
        }
        if let Some(position) = list.position(self.selected).filter(|p| shown.contains(p)) {
            place.highlight(&mut self.ui, position - shown.start);
        }
        if list.rows.is_empty() {
            place.value_plain(&mut self.ui, 0, "No action matches the search.", OPTION);
            if self.elsewhere > 0 {
                place.value_fmt(
                    &mut self.ui,
                    1,
                    format_args!("{} on OPTIONS: click its tab.", self.elsewhere),
                    OPTION,
                );
            }
        }
        for (slot, row) in list.rows[shown.clone()].iter().enumerate() {
            let action = match *row {
                ListRow::Heading(category) => {
                    place.heading(&mut self.ui, slot, HEADINGS[category]);
                    continue;
                }
                ListRow::Action(action) => action,
            };
            let focused = action == self.selected;
            let color = if focused { focus_text() } else { OPTION };
            place.label(&mut self.ui, slot, ACTIONS[action].label, color);
            if let Some(texture) = self.icons.ready(action) {
                place.row_icon(&mut self.ui, slot, texture);
            }
            if self
                .keys
                .get(action)
                .is_some_and(|keys| rebound(action, keys))
            {
                place.changed_mark(&mut self.ui, slot);
            }
            self.ui.hit_region(action as u16, place.row(slot));
            let value_color = if focused && self.capture {
                BINDING
            } else {
                color
            };
            match self.keys.get(action) {
                Some([first, _]) if first == "UNBOUND" => {
                    place.value(&mut self.ui, slot, "???", value_color);
                }
                Some([first, second]) if second == "-" => {
                    place.value(&mut self.ui, slot, first, value_color);
                }
                // Retail writes "A or B" and raises the whole line to capitals.
                Some([first, second]) => place.value_fmt(
                    &mut self.ui,
                    slot,
                    format_args!("{} OR {}", Caps(first), Caps(second)),
                    value_color,
                ),
                None => place.value(&mut self.ui, slot, "???", value_color),
            }
        }
        if total > self.visible {
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                place.scrollbar_track(self.visible),
                self.first,
                self.visible,
                total,
            );
        }
        let focused = list.position(self.selected).map(|_| self.selected);
        let searching = list.searching;
        self.classic = Some(list);
        self.write_detail(focused);
        let detail = match focused {
            Some(action) => Detail {
                title: ACTIONS[action].label,
                value: &self.detail[0],
                lines: [&self.detail[1], &self.detail[2]],
                facts: &self.detail[3],
                name: "",
                icon: self.icons.ready(action),
                // The category's icon, as on the group list.
                badge: crate::menu::classic::layout::Entry::of_category(
                    ACTIONS[action].category as usize,
                )
                .and_then(crate::menu::classic::layout::Entry::icon)
                .and_then(crate::settings_icons::texture),
            },
            None => Detail::default(),
        };
        place.detail(&mut self.ui, &detail);
        self.hint.clear();
        let searched = self
            .classic
            .as_ref()
            .is_some_and(|list| !list.search.trim().is_empty());
        if !self.capture && !searching && self.elsewhere > 0 && searched {
            let _ = std::fmt::Write::write_fmt(
                &mut self.hint,
                format_args!(
                    "{} option{} match too: click the OPTIONS tab, the search goes with you",
                    self.elsewhere,
                    if self.elsewhere == 1 { "" } else { "s" }
                ),
            );
        }
        place.finish(
            &mut self.ui,
            Some(if self.capture {
                WAITING
            } else if searching {
                SEARCHING
            } else if !self.hint.is_empty() {
                &self.hint
            } else {
                KEYS
            }),
        );
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The detail box's lines for action `action`: its keys, its command, the
    /// other actions its keys also do, and its default key.
    pub(super) fn write_detail(&mut self, action: Option<usize>) {
        for line in &mut self.detail {
            line.clear();
        }
        let Some(action) = action else {
            return;
        };
        let [value, command, shared, facts] = &mut self.detail;
        match self.keys.get(action) {
            Some([first, second]) if first != "UNBOUND" && second != "-" => {
                let _ = write!(value, "{first} or {second}");
            }
            Some([first, _]) if first != "UNBOUND" => value.push_str(first),
            _ => value.push_str("not bound"),
        }
        let _ = write!(command, "Console command: {}", ACTIONS[action].command);
        let bound = self.keys.get(action).into_iter().flatten();
        for key in bound.filter(|key| *key != "UNBOUND" && *key != "-") {
            let others = self.keys.iter().enumerate().filter(|(other, keys)| {
                *other != action
                    && keys
                        .iter()
                        .any(|other_key| other_key.eq_ignore_ascii_case(key))
            });
            for (count, (other, _)) in others.enumerate() {
                let label = ACTIONS[other].label;
                let _ = match (count, shared.is_empty()) {
                    (0, true) => write!(shared, "{key} also does: {label}"),
                    (0, false) => write!(shared, "; {key} also does: {label}"),
                    _ => write!(shared, ", {label}"),
                };
            }
        }
        let default = ACTIONS[action].default_key;
        if default.is_empty() {
            facts.push_str("No default key");
        } else {
            let _ = write!(
                facts,
                "Default {}",
                Caps(&sjk_shell::key_names::display_key(default))
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(first: &str, second: &str) -> [String; 2] {
        [first.to_owned(), second.to_owned()]
    }

    fn action(command: &str) -> usize {
        ACTIONS
            .iter()
            .position(|action| action.command == command)
            .unwrap()
    }

    #[test]
    fn actions_on_their_default_key_are_not_marked() {
        let forward = action("+forward");
        let shown = sjk_shell::key_names::display_key(ACTIONS[forward].default_key).into_owned();
        assert!(!rebound(forward, &keys(&shown, "-")));
        assert!(rebound(forward, &keys(&shown, "UPARROW")));
        assert!(rebound(forward, &keys("UNBOUND", "-")));
        let unbound_by_default = action("invnext");
        assert!(!rebound(unbound_by_default, &keys("UNBOUND", "-")));
        assert!(rebound(unbound_by_default, &keys("]", "-")));
    }

    #[test]
    fn the_detail_names_keys_command_and_default() {
        let (_directory, console) = {
            let directory = tempfile::tempdir().unwrap();
            let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
            (directory, console)
        };
        let mut editor = KeybindEditor::new();
        editor.open_classic(&console, 0, Span::ALL);
        let forward = action("+forward");
        editor.write_detail(Some(forward));
        assert_eq!(editor.detail[1], "Console command: +forward");
        assert!(
            editor.detail[3].starts_with("Default "),
            "{}",
            editor.detail[3]
        );
        assert!(!editor.detail[0].is_empty());
        // A key two actions share is named on both.
        let jump = action("+moveup");
        let shown = editor.keys[forward][0].clone();
        editor.keys[jump] = [shown.clone(), "-".to_owned()];
        editor.write_detail(Some(forward));
        assert_eq!(editor.detail[2], format!("{shown} also does: Jump"));
        editor.write_detail(None);
        assert!(editor.detail.iter().all(String::is_empty));
    }

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    #[test]
    fn another_tabs_search_is_counted_and_carried_over() {
        let (_directory, console) = console();
        let mut editor = KeybindEditor::new();
        editor.open_classic(&console, 0, Span::ALL);
        assert_eq!(editor.count_matches("   "), 0);
        assert_eq!(editor.count_matches("no such action at all"), 0);
        // A heading finds all of its category, an action's name only itself.
        assert_eq!(
            editor.count_matches("weapons"),
            category_range(Category::Weapons as usize).len()
        );
        assert!(editor.count_matches("jump") >= 1);
        assert_eq!(editor.search_text(), "");
        editor.carry_search("jump");
        assert_eq!(editor.search_text(), "jump");
        let list = editor.classic.as_ref().unwrap();
        assert_eq!(list.actions(), editor.count_matches("jump"));
        // Nothing carries when the other tab had no search.
        editor.carry_search("  ");
        assert_eq!(editor.search_text(), "jump");
    }

    #[test]
    fn one_list_holds_every_action_under_its_heading() {
        let (_directory, console) = console();
        let mut editor = KeybindEditor::new();
        editor.open_classic(&console, Category::Weapons as usize, Span::ALL);
        let list = editor.classic.as_ref().unwrap();
        assert_eq!(list.actions(), ACTIONS.len());
        assert_eq!(list.rows.len(), ACTIONS.len() + HEADINGS.len());
        assert_eq!(list.rows[0], ListRow::Heading(0));
        // The category opens at its heading, its first action selected.
        assert_eq!(
            list.rows[editor.first],
            ListRow::Heading(Category::Weapons as usize)
        );
        assert_eq!(
            editor.selected,
            category_range(Category::Weapons as usize).start
        );
        // Down walks over the next heading into the next category.
        let last_weapon = category_range(Category::Weapons as usize).end - 1;
        editor.selected = last_weapon;
        editor.classic_step(1);
        assert_eq!(editor.selected, last_weapon + 1);
        assert_eq!(ACTIONS[editor.selected].category, Category::Force);
    }

    #[test]
    fn search_finds_names_commands_and_keys_and_escape_clears_it() {
        let (_directory, mut console) = console();
        let mut editor = KeybindEditor::new();
        editor.open_classic(&console, 0, Span::ALL);
        let press = |editor: &mut KeybindEditor, console: &mut ViewerConsole, key, text| {
            editor.classic_key(key, text, console)
        };
        press(&mut editor, &mut console, KeyCode::Slash, None);
        assert!(editor.classic.as_ref().unwrap().searching);
        for letter in ["j", "u", "m", "p"] {
            press(&mut editor, &mut console, KeyCode::KeyJ, Some(letter));
        }
        let list = editor.classic.as_ref().unwrap();
        assert_eq!(list.search, "jump");
        assert_eq!(ACTIONS[editor.selected].command, "+moveup");
        assert!(list.rows.contains(&ListRow::Heading(0)));
        // A command and a key name find their action too.
        editor.set_search("+attack".to_owned());
        assert_eq!(ACTIONS[editor.selected].command, "+attack");
        editor.set_search("space".to_owned());
        assert_eq!(ACTIONS[editor.selected].command, "+moveup");
        editor.set_search("zzzz".to_owned());
        assert_eq!(editor.classic.as_ref().unwrap().actions(), 0);
        // Escape clears the text, then leaves the field, then the panel.
        press(&mut editor, &mut console, KeyCode::Escape, None);
        assert!(editor.classic.as_ref().unwrap().search.is_empty());
        press(&mut editor, &mut console, KeyCode::Escape, None);
        assert!(!editor.classic.as_ref().unwrap().searching);
        assert_eq!(
            press(&mut editor, &mut console, KeyCode::Escape, None),
            EditorResult::Back
        );
    }
}

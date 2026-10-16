//! Visual action-to-key editor and stock multiplayer bindings.

use super::{TextVertex, UiFont};
use crate::console::ViewerConsole;
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas, TAB_BASE};
mod catalog;
mod classic_view;
mod icons;
mod pointer;
mod sjk_view;

use catalog::CATEGORIES;
pub(crate) use catalog::{
    ACTIONS, Category, category_range, default_bindings, migrate_chat_default,
    migrate_missing_defaults,
};
use sjk_ui::{DrawList, Rect};
use std::ops::Range;
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Footer cap that restores the stock bindings.
const RESET_TOKEN: u16 = 903;
/// Footer cap that unbinds the selected action.
const UNBIND_TOKEN: u16 = 902;
/// Draggable thumb beside the row column.
const SCROLLBAR_TOKEN: u16 = 910;
/// Rows one wheel notch scrolls.
const WHEEL_ROWS: usize = 3;
/// Secondary-slot hit targets, separate from the action row.
const SECONDARY_BASE: u16 = 600;
/// Keys the editor never binds, clears or moves: the menu's own pointer
/// buttons and its cancel key. A stray click or Escape while a capture is
/// pending must not take `+attack` off the mouse or bind the menu key; the
/// console's `bind` and `unbind` still change them.
const LOCKED_KEYS: [&str; 3] = ["MOUSE1", "MOUSE2", "ESCAPE"];

/// Whether `key` is one of the [`LOCKED_KEYS`] (any case, as binds match).
pub(crate) fn is_locked_key(key: &str) -> bool {
    LOCKED_KEYS
        .iter()
        .any(|locked| locked.eq_ignore_ascii_case(key))
}

/// Outcome of one key or pointer event on the editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditorResult {
    None,
    Back,
    /// A button of the classic panel screen around the bindings: index into
    /// its page's slots.
    Classic(usize),
    /// Move the classic panel to the next (1) or previous (-1) group.
    ClassicCycle(i32),
}

pub(crate) struct KeybindEditor {
    tab: usize,
    /// Index into `ACTIONS`; always inside the current tab's range.
    selected: usize,
    /// First row of the current tab that is on screen.
    first: usize,
    /// Rows that fit, as measured by the last `append`.
    visible: usize,
    capture: bool,
    binding_slot: usize,
    keys: Vec<[String; 2]>,
    /// The classic option panel's one list of every binding, while the
    /// screen is one.
    classic: Option<classic_view::ClassicList>,
    /// The classic+ detail box's value, command, shared-key and default
    /// lines, rewritten each frame.
    detail: [String; 4],
    /// Options matching the search typed here, offered through the OPTIONS tab
    /// (0 when none or no search), and the description line they are named in.
    elsewhere: usize,
    hint: String,
    /// The actions' weapon, item and Force pictures.
    icons: icons::BindIcons,
    ui: MenuCanvas,
}

impl KeybindEditor {
    pub(crate) fn new() -> Self {
        Self {
            tab: 0,
            selected: 0,
            first: 0,
            visible: 1,
            capture: false,
            binding_slot: 0,
            keys: Vec::with_capacity(ACTIONS.len()),
            classic: None,
            detail: std::array::from_fn(|_| String::with_capacity(48)),
            elsewhere: 0,
            hint: String::with_capacity(96),
            icons: icons::BindIcons::new(),
            ui: MenuCanvas::new(),
        }
    }

    /// The game data the actions' pictures come from.
    pub(crate) fn attach_vfs(&mut self, vfs: std::sync::Arc<sjk_vfs::VirtualFileSystem>) {
        self.icons.attach_vfs(vfs);
    }

    /// Upload the pictures again, into another world's atlas.
    pub(crate) fn forget_uploads(&mut self) {
        self.icons.forget_uploads();
    }

    /// Decode the pictures (once) and collect them, while the screen is up.
    pub(crate) fn poll_icons(&mut self) {
        self.icons.poll();
    }

    /// Focus the action bound to `command`.
    #[cfg(test)]
    pub(crate) fn select_command(&mut self, command: &str) {
        if let Some(action) = ACTIONS.iter().position(|action| action.command == command) {
            self.selected = action;
        }
    }

    /// Focus the action bound to `command` in the list, scrolled to it, and
    /// await its second key when `capture` (world shots).
    #[cfg(test)]
    pub(crate) fn focus_for_shot(&mut self, command: &str, capture: bool) {
        self.select_command(command);
        self.visible = crate::settings::sjk_view::VISIBLE;
        if let Some(position) = self
            .classic
            .as_ref()
            .and_then(|list| list.position(self.selected))
        {
            self.first = position.saturating_sub(3);
        }
        self.binding_slot = 1;
        self.capture = capture;
    }

    /// The pictures' cells and files, for snapshots, which count them as
    /// uploaded.
    #[cfg(test)]
    pub(crate) fn snapshot_icons(&mut self) -> Vec<crate::player_menu::icons::IconRequest> {
        self.icons.assume_ready();
        icons::requests()
    }

    /// Move a few decoded pictures into the UI atlas.
    pub(crate) fn upload_icons(
        &mut self,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.icons.upload(renderer, queue);
    }

    /// Open on the tabbed categories, without a classic+ list (tests).
    #[cfg(test)]
    pub(crate) fn open(&mut self, console: &ViewerConsole) {
        self.capture = false;
        if self.classic.take().is_some() {
            self.set_tab(self.tab);
        }
        self.refresh(console);
    }

    pub(crate) fn visual_selection(&self) -> usize {
        self.selected
    }

    /// The bindings are the classic+ panel's one list, which the classic+ and
    /// SJK UI views draw.
    pub(crate) fn has_list(&self) -> bool {
        self.classic.is_some()
    }

    /// Whether the classic list's search field has the keys.
    pub(crate) fn typing(&self) -> bool {
        self.classic.as_ref().is_some_and(|list| list.searching)
    }

    /// The text of the search field that has the keys, for Ctrl+C.
    pub(crate) fn field_text(&self) -> Option<&str> {
        self.typing().then(|| self.search_text())
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> EditorResult {
        // A held Backspace or letter goes on in the search field; every other key acts once.
        if event.state != ElementState::Pressed || (event.repeat && !self.typing()) {
            return EditorResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return EditorResult::None;
        };
        if self.capture {
            if key == KeyCode::Escape {
                self.capture = false;
                return EditorResult::None;
            }
            // Retail's prompt: "Enter new key, or ESC to cancel, BACKSPACE
            // to clear."
            if self.classic.is_some() && key == KeyCode::Backspace {
                self.clear_both(console);
                self.capture = false;
                return EditorResult::None;
            }
            let Some(key) = crate::input::keys::key_name(event) else {
                return EditorResult::None;
            };
            console.rebind_action(
                ACTIONS[self.selected].command,
                self.binding_slot,
                key.as_str(),
            );
            self.capture = false;
            self.refresh(console);
            return EditorResult::None;
        }
        if self.classic.is_some() {
            return self.classic_key(key, event.text.as_deref(), console);
        }
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => self.move_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.move_selection(1),
            KeyCode::Tab | KeyCode::ArrowRight | KeyCode::KeyD => self.cycle_tab(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.cycle_tab(-1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => self.begin_capture(),
            KeyCode::Delete | KeyCode::Backspace => self.clear_selected_slot(console),
            KeyCode::KeyR => {
                console.reset_default_binds();
                self.refresh(console);
            }
            KeyCode::Escape => return EditorResult::Back,
            _ => {}
        }
        EditorResult::None
    }

    pub(crate) fn capture_mouse(
        &mut self,
        button: MouseButton,
        console: &mut ViewerConsole,
    ) -> bool {
        if !self.capture {
            return false;
        }
        let Some(key) = crate::input::keys::name(crate::input::keys::Source::Mouse(button)) else {
            return true;
        };
        // A click on a locked button is the player clicking away, not a key to
        // bind: it cancels, as Escape does, and is consumed so it activates
        // nothing under the pointer.
        if is_locked_key(key) {
            self.capture = false;
            return true;
        }
        console.rebind_action(ACTIONS[self.selected].command, self.binding_slot, key);
        self.capture = false;
        self.refresh(console);
        true
    }

    /// While a slot awaits a key, a wheel notch binds `MWHEELUP` or `MWHEELDOWN`
    /// (`up` = away from the player), as the retail controls menu takes any key
    /// event; without this the notch only scrolled the list. False when not
    /// capturing, so the wheel scrolls as usual.
    pub(crate) fn capture_wheel(&mut self, up: bool, console: &mut ViewerConsole) -> bool {
        if !self.capture {
            return false;
        }
        let Some(key) = crate::input::keys::name(crate::input::keys::Source::Wheel(up)) else {
            return true;
        };
        console.rebind_action(ACTIONS[self.selected].command, self.binding_slot, key);
        self.capture = false;
        self.refresh(console);
        true
    }

    /// Whether the selected action's chosen slot holds a locked key.
    fn selected_slot_locked(&self) -> bool {
        self.keys
            .get(self.selected)
            .is_some_and(|keys| is_locked_key(&keys[self.binding_slot]))
    }

    /// Await a key for the selected slot, unless that slot is locked.
    fn begin_capture(&mut self) {
        self.capture = !self.selected_slot_locked();
    }

    /// Unbind the selected slot, unless it holds a locked key.
    fn clear_selected_slot(&mut self, console: &mut ViewerConsole) {
        if !self.selected_slot_locked() {
            console.clear_action(ACTIONS[self.selected].command, self.binding_slot);
            self.refresh(console);
        }
    }

    /// `ACTIONS` range of the current tab, or every action in the classic
    /// panel (whose list [`classic_view::ClassicList`] orders and filters them).
    fn rows(&self) -> Range<usize> {
        match &self.classic {
            Some(_) => 0..ACTIONS.len(),
            None => category_range(self.tab),
        }
    }

    /// Whether action `action` is a row on show: in the tab, or in the
    /// classic list after its search.
    fn shows(&self, action: usize) -> bool {
        match &self.classic {
            Some(list) => list.position(action).is_some(),
            None => category_range(self.tab).contains(&action),
        }
    }

    /// Clear every key bound to the selected action, as retail's Backspace
    /// does. Slots are cleared last first, so a slot the console declines to
    /// clear (a locked key) does not stop the ones after it.
    fn clear_both(&mut self, console: &mut ViewerConsole) {
        let command = ACTIONS[self.selected].command;
        for slot in (0..console.keys_for_command(command).len()).rev() {
            console.clear_action(command, slot);
        }
        self.refresh(console);
    }

    fn set_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.selected = category_range(tab).start;
        self.first = 0;
        self.capture = false;
    }

    fn cycle_tab(&mut self, direction: i32) {
        let count = CATEGORIES.len() as i32;
        self.set_tab(((self.tab as i32 + direction).rem_euclid(count)) as usize);
    }

    /// Move the keyboard selection within the tab, wrapping, and scroll so
    /// it stays on screen.
    fn move_selection(&mut self, direction: i32) {
        if self.classic.is_some() {
            self.classic_step(direction);
            return;
        }
        let rows = self.rows();
        let count = rows.len() as i32;
        if count == 0 {
            return;
        }
        let offset = (self.selected - rows.start) as i32 + direction;
        let offset = offset.rem_euclid(count) as usize;
        self.selected = rows.start + offset;
        self.first = self
            .first
            .min(offset)
            .max(offset.saturating_sub(self.visible - 1));
    }

    /// Rows the list scrolls through: the tab's, or the classic list's
    /// (headings included).
    fn list_len(&self) -> usize {
        match &self.classic {
            Some(list) => list.rows.len(),
            None => self.rows().len(),
        }
    }

    /// Scroll the tab by `rows` (negative = up) without moving the selection.
    fn scroll_by(&mut self, rows: i32) {
        let max_first = self.list_len().saturating_sub(self.visible);
        self.first = (self.first as i32 + rows).clamp(0, max_first as i32) as usize;
    }

    /// Scroll so the row column shows `ratio` (0 = top, 1 = bottom) of the tab.
    fn scroll_to_ratio(&mut self, ratio: f32) {
        let max_first = self.list_len().saturating_sub(self.visible);
        self.first = (ratio.clamp(0.0, 1.0) * max_first as f32).round() as usize;
    }

    fn refresh(&mut self, console: &ViewerConsole) {
        self.keys.clear();
        for action in ACTIONS {
            // Shown as players see key names (`display_key`); binding goes
            // through `ViewerConsole`, which matches names case-insensitively.
            let keys = console.keys_for_command(action.command);
            let shown = |key: &String| sjk_shell::key_names::display_key(key).into_owned();
            self.keys.push([
                keys.first()
                    .map(shown)
                    .unwrap_or_else(|| "UNBOUND".to_owned()),
                keys.get(1).map(shown).unwrap_or_else(|| "-".to_owned()),
            ]);
        }
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

    fn action(command: &str) -> usize {
        ACTIONS
            .iter()
            .position(|action| action.command == command)
            .unwrap()
    }

    #[test]
    fn mouse_buttons_one_two_and_escape_are_locked_in_any_case() {
        for key in ["MOUSE1", "mouse2", "Escape"] {
            assert!(is_locked_key(key), "{key}");
        }
        for key in ["MOUSE3", "a", "SPACE", "ESC"] {
            assert!(!is_locked_key(key), "{key}");
        }
    }

    #[test]
    fn the_editor_never_unbinds_or_moves_a_locked_key() {
        let (_directory, mut console) = console();
        console.clear_action("+attack", 0);
        assert_eq!(console.keys_for_command("+attack"), ["MOUSE1"]);
        console.rebind_action("+use", 0, "MOUSE1");
        console.rebind_action("+attack", 0, "k");
        assert_eq!(console.keys_for_command("+attack"), ["MOUSE1"]);
        assert!(
            !console
                .keys_for_command("+use")
                .iter()
                .any(|key| key == "MOUSE1")
        );
    }

    #[test]
    fn a_locked_slot_does_not_start_a_capture() {
        let (_directory, console) = console();
        let mut editor = KeybindEditor::new();
        editor.open(&console);
        editor.selected = action("+altattack");
        editor.binding_slot = 0;
        editor.begin_capture();
        assert!(!editor.capture);
        // The empty second slot is free.
        editor.binding_slot = 1;
        editor.begin_capture();
        assert!(editor.capture);
    }

    #[test]
    fn a_wheel_notch_binds_while_capturing_and_scrolls_otherwise() {
        let (_directory, mut console) = console();
        let mut editor = KeybindEditor::new();
        editor.open(&console);
        editor.selected = action("+moveup");
        editor.binding_slot = 1;
        assert!(!editor.capture_wheel(true, &mut console));
        editor.begin_capture();
        assert!(editor.capture_wheel(true, &mut console));
        assert!(!editor.capture);
        assert!(
            console
                .keys_for_command("+moveup")
                .iter()
                .any(|key| key == "MWHEELUP")
        );
        editor.binding_slot = 0;
        editor.begin_capture();
        assert!(editor.capture_wheel(false, &mut console));
        assert!(
            console
                .keys_for_command("+moveup")
                .iter()
                .any(|key| key == "MWHEELDOWN")
        );
    }

    #[test]
    fn a_click_cancels_a_capture_without_binding_it() {
        let (_directory, mut console) = console();
        let mut editor = KeybindEditor::new();
        editor.open(&console);
        let jump = action("+moveup");
        editor.selected = jump;
        editor.binding_slot = 0;
        editor.begin_capture();
        let before = console.keys_for_command("+moveup");
        for button in [MouseButton::Left, MouseButton::Right] {
            editor.begin_capture();
            assert!(editor.capture);
            assert!(editor.capture_mouse(button, &mut console));
            assert!(!editor.capture);
        }
        assert_eq!(console.keys_for_command("+moveup"), before);
        assert_eq!(console.keys_for_command("+attack"), ["MOUSE1"]);
        // Other mouse buttons still bind.
        editor.begin_capture();
        assert!(editor.capture_mouse(MouseButton::Middle, &mut console));
        assert!(
            console
                .keys_for_command("+moveup")
                .iter()
                .any(|key| key == "MOUSE3")
        );
    }
}

//! Settings > Quick wheel: the quick wheel's pages, edited (`docs/client.md`,
//! Quick wheels). The pages in a column on the left (add, rename, move, remove,
//! restore the defaults; under them the switch of the wheel's sounds,
//! [`crate::quick_wheel::SOUNDS_CVAR`]), the shown page's choices in the middle (add, change,
//! move round the ring, remove), and on the right a live preview of the page's
//! ring with what the focused item does, or, while a choice is being picked, the
//! catalogue of actions ([`crate::quick_wheel::catalog`]) with a custom console
//! command last. Every change is saved at once to `wheel.json`
//! ([`crate::quick_wheel::pages`]).
//!
//! The Force page is renamed, moved and removed like any page, but its choices
//! are the player's powers ([`crate::quick_wheel::force_page`]): its middle column
//! says so instead of listing choices, and its preview is an example. Removed, a
//! row under Add a page puts it back. The Toys page is an ordinary page whose
//! choices come from the catalogue's Toys group, so a deleted toy is added again
//! from there.
//!
//! In the SJK UI it is a category of Settings' rail; the classic+ and tabbed
//! settings open it from Interface's "Quick wheel pages" row as a screen of its
//! own in the SJK UI's look ([`WheelMode`]). [`super::wheel_editor_view`] draws it.

use super::*;
use crate::menu_widgets::MenuCanvas;
use crate::quick_wheel::catalog::{ACTIONS, Group as ActionGroup};
use crate::quick_wheel::pages::{self, MAX_CHOICES, MAX_PAGES, Page, Slot};
use sjk_ui::{InputEvent, UiEventKind};

/// Where the editor is shown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WheelMode {
    /// A category of the SJK UI's Settings: Escape leaves Settings, Tab goes
    /// on to the next category.
    Category,
    /// Opened from a settings row (classic+, tabbed): Escape returns to it.
    Overlay,
}

/// The column that has the keyboard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Column {
    Pages,
    Choices,
}

/// Text being typed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Typing {
    /// Page `page`'s name; `first` for a page just added, which takes its id
    /// from the name.
    Page {
        page: usize,
        text: String,
        first: bool,
    },
    /// A custom choice: its name and command, in place of choice `at` or added.
    Custom {
        at: Option<usize>,
        label: String,
        command: String,
        field: Field,
    },
}

/// The custom choice's two fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Field {
    Label,
    Command,
}

/// A line of the catalogue: a group's sub-heading, an action, an installed emote,
/// or the custom command at the end.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PickLine {
    Heading(&'static str),
    Action(usize),
    /// An emote of the installed catalogue (`emotes.rs`), by its index there; it
    /// goes on the page as a custom choice running `sjkemote <id>`.
    Emote(usize),
    Custom,
}

/// Installed emote `index`'s name and the command that plays it.
pub(super) fn emote_choice(index: usize) -> Option<(String, String)> {
    let state = crate::emotes::lock();
    let def = state.catalogue.as_ref()?.get(index)?;
    Some((
        def.name.clone(),
        format!("{} {}", crate::emotes::COMMAND, def.id),
    ))
}

/// The catalogue, open to put an action in place of choice `at` or to add one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Picker {
    pub(super) at: Option<usize>,
    /// The highlighted line (never a heading) and the first line shown.
    pub(super) highlighted: usize,
    pub(super) first: usize,
}

/// A step that asks to be confirmed by repeating it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Confirm {
    RemovePage(usize),
    Restore,
}

/// Lines of the catalogue shown at once.
pub(super) const PICK_LINES: usize = 17;

/// Pointer tokens, clear of the rail (800) and the way back (900).
pub(super) const PAGE_BASE: u16 = 3000;
pub(super) const ADD_PAGE: u16 = 3040;
pub(super) const RESTORE: u16 = 3041;
pub(super) const PAGE_RENAME: u16 = 3042;
pub(super) const PAGE_UP: u16 = 3043;
pub(super) const PAGE_DOWN: u16 = 3044;
pub(super) const PAGE_REMOVE: u16 = 3045;
pub(super) const SOUNDS: u16 = 3046;
pub(super) const ADD_FORCE: u16 = 3047;
pub(super) const CHOICE_BASE: u16 = 3100;
pub(super) const ADD_CHOICE: u16 = 3140;
pub(super) const CHOICE_UP: u16 = 3143;
pub(super) const CHOICE_DOWN: u16 = 3144;
pub(super) const CHOICE_REMOVE: u16 = 3145;
pub(super) const PICK_BASE: u16 = 3200;
pub(super) const PICK_SCROLL: u16 = 3290;
pub(super) const PICK_CLOSE: u16 = 3291;
pub(super) const FORM_LABEL: u16 = 3300;
pub(super) const FORM_COMMAND: u16 = 3301;
pub(super) const FORM_SAVE: u16 = 3302;
pub(super) const FORM_CANCEL: u16 = 3303;
/// The top bar's way back (the settings form's back cap).
pub(super) const BACK: u16 = 900;

/// The editor's state, its copy of the pages and its own canvas.
pub(crate) struct WheelEditor {
    open: bool,
    pub(super) mode: WheelMode,
    pub(super) column: Column,
    /// The page whose choices show.
    pub(super) page: usize,
    /// The focused row of each column: a page (then Add a page, Restore the
    /// default pages, the wheel's sounds), a choice (then Add a choice).
    pub(super) rows: [usize; 2],
    pub(super) picker: Option<Picker>,
    pub(super) typing: Option<Typing>,
    pub(super) confirm: Option<Confirm>,
    /// The pages as last saved, and the keys bound to open each.
    pub(super) pages: Vec<Page>,
    pub(super) keys: Vec<String>,
    /// Whether the pages are the defaults.
    pub(super) default: bool,
    /// Whether the wheel plays its sounds ([`crate::quick_wheel::SOUNDS_CVAR`]).
    pub(super) sounds: bool,
    pub(super) ui: MenuCanvas,
}

impl Default for WheelEditor {
    fn default() -> Self {
        Self {
            open: false,
            mode: WheelMode::Category,
            column: Column::Pages,
            page: 0,
            rows: [0; 2],
            picker: None,
            typing: None,
            confirm: None,
            pages: Vec::new(),
            keys: Vec::new(),
            default: true,
            sounds: true,
            ui: MenuCanvas::with_capacities(224, 64, 1024),
        }
    }
}

/// The catalogue's lines: each group under its sub-heading (the installed emotes
/// after the stock ones), then the custom command under its own.
pub(super) fn pick_lines() -> Vec<PickLine> {
    let emotes = crate::emotes::lock().catalogue.as_ref().map_or(0, Vec::len);
    let mut lines = Vec::with_capacity(ACTIONS.len() + ActionGroup::ALL.len() + emotes + 2);
    for group in ActionGroup::ALL {
        lines.push(PickLine::Heading(group.label()));
        lines.extend(
            ACTIONS
                .iter()
                .enumerate()
                .filter(|(_, action)| action.group == group)
                .map(|(index, _)| PickLine::Action(index)),
        );
        if group == ActionGroup::Emotes {
            lines.extend((0..emotes).map(PickLine::Emote));
        }
    }
    lines.push(PickLine::Heading("Your own"));
    lines.push(PickLine::Custom);
    lines
}

impl WheelEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Open the editor as `mode` on the first page, its pages read from `console`.
    pub(super) fn open(&mut self, console: &ViewerConsole, mode: WheelMode) {
        self.open = true;
        self.mode = mode;
        self.column = Column::Pages;
        self.page = 0;
        self.rows = [0; 2];
        self.picker = None;
        self.typing = None;
        self.confirm = None;
        self.sync(console);
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.picker = None;
        self.typing = None;
        self.confirm = None;
    }

    /// Take the pages again from `console` after a change, keeping the focus
    /// on what is there.
    pub(super) fn sync(&mut self, console: &ViewerConsole) {
        self.pages = console.wheel_pages.pages().to_vec();
        self.default = console.wheel_pages.is_default();
        self.sounds = console
            .bool_cvar(crate::quick_wheel::SOUNDS_CVAR)
            .unwrap_or(true);
        // A page with no key of its own names the keys of the bare `+wheel`
        // (Q by default), which opens on the page used last. Key names show
        // in capitals, the note after them as written.
        let last = console.keys_for_command(crate::quick_wheel::OPEN_COMMAND);
        let last =
            (!last.is_empty()).then(|| format!("{} (last page)", last.join(", ").to_uppercase()));
        self.keys = self
            .pages
            .iter()
            .map(|page| {
                let own = console
                    .keys_for_command(&format!("{} {}", crate::quick_wheel::OPEN_COMMAND, page.id))
                    .join(", ")
                    .to_uppercase();
                match &last {
                    Some(last) if own.is_empty() => last.clone(),
                    _ => own,
                }
            })
            .collect();
        self.page = self.page.min(self.pages.len().saturating_sub(1));
        self.rows[0] = self.rows[0].min(self.page_rows() - 1);
        self.rows[1] = self.rows[1].min(self.choice_rows() - 1);
    }

    /// Rows of the pages column: the pages, Add a page, Add the Force page
    /// (while the wheel has none), Restore the defaults, the wheel's sounds.
    pub(super) fn page_rows(&self) -> usize {
        self.sounds_row() + 1
    }

    /// The pages column's row that puts the Force page back, while it is missing.
    pub(super) fn force_row(&self) -> Option<usize> {
        (!self.pages.iter().any(|page| page.force)).then_some(self.pages.len() + 1)
    }

    /// The pages column's row of Restore the default pages.
    pub(super) fn restore_row(&self) -> usize {
        self.pages.len() + 1 + usize::from(self.force_row().is_some())
    }

    /// The pages column's row of the wheel's sounds.
    pub(super) fn sounds_row(&self) -> usize {
        self.restore_row() + 1
    }

    /// Whether the shown page is the Force page, whose choices are the
    /// player's powers.
    pub(super) fn force_shown(&self) -> bool {
        self.pages.get(self.page).is_some_and(|page| page.force)
    }

    /// Put the Force page back, and show it.
    fn add_force(&mut self, console: &mut ViewerConsole) {
        let Some(page) = console.wheel_pages.add_force_page() else {
            return;
        };
        self.sync(console);
        self.page = page;
        self.column = Column::Pages;
        self.rows[0] = page;
    }

    /// Turn the wheel's sounds on or off.
    fn toggle_sounds(&mut self, console: &mut ViewerConsole) {
        let on = !self.sounds;
        console.set_cvar(crate::quick_wheel::SOUNDS_CVAR, if on { "1" } else { "0" });
        self.sync(console);
    }

    /// Rows of the choices column: the shown page's choices, Add a choice.
    pub(super) fn choice_rows(&self) -> usize {
        self.choices().len() + 1
    }

    /// The shown page's choices.
    pub(super) fn choices(&self) -> &[Slot] {
        self.pages
            .get(self.page)
            .map_or(&[][..], |page| &page.choices)
    }

    /// The focused row of the column that has the keyboard.
    pub(super) fn row(&self) -> usize {
        self.rows[self.column as usize]
    }

    /// Move the focus `direction` rows in its column, wrapping; in the pages
    /// column the shown page follows it.
    fn step(&mut self, direction: i32) {
        let count = match self.column {
            Column::Pages => self.page_rows(),
            Column::Choices => self.choice_rows(),
        };
        let row = &mut self.rows[self.column as usize];
        *row = (*row as i32 + direction).rem_euclid(count as i32) as usize;
        if self.column == Column::Pages && *row < self.pages.len() {
            self.page = *row;
            self.rows[1] = 0;
        }
    }

    /// The catalogue opened to fill choice `at` (or add one), on the action it
    /// holds or the first one.
    fn open_picker(&mut self, at: Option<usize>) {
        let current = at
            .and_then(|at| self.choices().get(at))
            .map(|slot| match slot {
                Slot::Action(index) => PickLine::Action(*index),
                Slot::Custom { command, .. } => {
                    let emotes = crate::emotes::lock().catalogue.as_ref().map_or(0, Vec::len);
                    (0..emotes)
                        .find(|&index| emote_choice(index).is_some_and(|(_, run)| run == *command))
                        .map_or(PickLine::Custom, PickLine::Emote)
                }
            });
        let lines = pick_lines();
        let highlighted = current
            .and_then(|line| lines.iter().position(|candidate| *candidate == line))
            .unwrap_or(1);
        let first = highlighted.saturating_sub(PICK_LINES / 2);
        let first = first.min(lines.len().saturating_sub(PICK_LINES));
        self.picker = Some(Picker {
            at,
            highlighted,
            first,
        });
    }

    /// Open the custom choice's form for choice `at` (filled from it when it is
    /// a custom one) or a new one.
    fn open_custom(&mut self, at: Option<usize>) {
        let (label, command) = match at.and_then(|at| self.choices().get(at)) {
            Some(Slot::Custom { label, command }) => (label.clone(), command.clone()),
            _ => (String::new(), String::new()),
        };
        self.picker = None;
        self.typing = Some(Typing::Custom {
            at,
            label,
            command,
            field: Field::Label,
        });
    }

    /// Move the catalogue's highlight to the line `direction` selectable lines
    /// away (or the nearest from `from`), keeping it in view.
    fn pick_step(&mut self, direction: i32) {
        let lines = pick_lines();
        let Some(picker) = &mut self.picker else {
            return;
        };
        let mut at = picker.highlighted as i32;
        let mut moved = 0;
        while moved < direction.abs() {
            let next = at + direction.signum();
            if !(0..lines.len() as i32).contains(&next) {
                break;
            }
            at = next;
            if !matches!(lines[at as usize], PickLine::Heading(_)) {
                moved += 1;
                picker.highlighted = at as usize;
            }
        }
        reveal_pick(picker, lines.len());
    }

    /// Put catalogue line `line` on the page: an action, or the custom form.
    fn pick(&mut self, line: usize, console: &mut ViewerConsole) {
        let Some(picker) = self.picker else {
            return;
        };
        match pick_lines().get(line) {
            Some(PickLine::Action(index)) => {
                let set =
                    console
                        .wheel_pages
                        .set_choice(self.page, picker.at, Slot::Action(*index));
                self.picker = None;
                self.sync(console);
                if let Some(row) = set {
                    self.column = Column::Choices;
                    self.rows[1] = row;
                }
            }
            Some(PickLine::Emote(index)) => {
                let Some((label, command)) = emote_choice(*index) else {
                    return;
                };
                let set = console.wheel_pages.set_choice(
                    self.page,
                    picker.at,
                    Slot::Custom { label, command },
                );
                self.picker = None;
                self.sync(console);
                if let Some(row) = set {
                    self.column = Column::Choices;
                    self.rows[1] = row;
                }
            }
            Some(PickLine::Custom) => self.open_custom(picker.at),
            _ => {}
        }
    }

    /// Enter on the focused row.
    fn activate(&mut self, console: &mut ViewerConsole) {
        let pages = self.pages.len();
        match (self.column, self.row()) {
            (Column::Pages, row) if row < pages => {
                self.page = row;
                self.rows[1] = 0;
                // The Force page has no choices to go to.
                if !self.force_shown() {
                    self.column = Column::Choices;
                }
            }
            (Column::Pages, row) if row == pages => self.add_page(console),
            (Column::Pages, row) if Some(row) == self.force_row() => self.add_force(console),
            (Column::Pages, row) if row == self.restore_row() => self.restore(console),
            (Column::Pages, _) => self.toggle_sounds(console),
            (Column::Choices, _) if self.force_shown() => {}
            (Column::Choices, row) => match self.choices().get(row) {
                Some(Slot::Custom { .. }) => self.open_custom(Some(row)),
                Some(Slot::Action(_)) => self.open_picker(Some(row)),
                None if row < MAX_CHOICES => self.open_picker(None),
                None => {}
            },
        }
    }

    /// Add a page and start naming it.
    fn add_page(&mut self, console: &mut ViewerConsole) {
        let Some(page) = console.wheel_pages.add_page("") else {
            return;
        };
        self.sync(console);
        self.page = page;
        self.column = Column::Pages;
        self.rows[0] = page;
        self.typing = Some(Typing::Page {
            page,
            text: String::new(),
            first: true,
        });
    }

    /// Start renaming the focused page (or editing the focused custom choice).
    fn rename(&mut self) {
        match self.column {
            Column::Pages => {
                if let Some(page) = self.pages.get(self.rows[0]) {
                    self.typing = Some(Typing::Page {
                        page: self.rows[0],
                        text: page.name.clone(),
                        first: false,
                    });
                }
            }
            Column::Choices => {
                if matches!(self.choices().get(self.rows[1]), Some(Slot::Custom { .. })) {
                    self.open_custom(Some(self.rows[1]));
                }
            }
        }
    }

    /// Restore the default pages, once confirmed (a second Enter or click).
    fn restore(&mut self, console: &mut ViewerConsole) {
        if self.default {
            return;
        }
        if self.confirm != Some(Confirm::Restore) {
            self.confirm = Some(Confirm::Restore);
            return;
        }
        self.confirm = None;
        console.wheel_pages.restore_defaults();
        self.page = 0;
        self.rows = [0; 2];
        self.sync(console);
    }

    /// Remove the focused page (once confirmed) or choice.
    fn remove(&mut self, console: &mut ViewerConsole) {
        match self.column {
            Column::Pages => {
                let page = self.rows[0];
                if page >= self.pages.len() || self.pages.len() <= 1 {
                    return;
                }
                if self.confirm != Some(Confirm::RemovePage(page)) {
                    self.confirm = Some(Confirm::RemovePage(page));
                    return;
                }
                self.confirm = None;
                console.wheel_pages.remove_page(page);
                self.page = page.min(self.pages.len() - 2);
                self.rows[0] = self.page;
                self.rows[1] = 0;
                self.sync(console);
            }
            Column::Choices => {
                if self.rows[1] < self.choices().len() {
                    console.wheel_pages.remove_choice(self.page, self.rows[1]);
                    self.sync(console);
                }
            }
        }
    }

    /// Move the focused page or choice `direction` places.
    fn shift(&mut self, direction: i32, console: &mut ViewerConsole) {
        match self.column {
            Column::Pages if self.rows[0] < self.pages.len() => {
                let to = console.wheel_pages.move_page(self.rows[0], direction);
                self.page = to;
                self.rows[0] = to;
            }
            Column::Choices if self.rows[1] < self.choices().len() => {
                self.rows[1] = console
                    .wheel_pages
                    .move_choice(self.page, self.rows[1], direction);
            }
            _ => return,
        }
        self.sync(console);
    }

    /// A key while a name or command is typed.
    fn typing_key(&mut self, key: KeyCode, text: Option<&str>, console: &mut ViewerConsole) {
        let Some(typing) = &mut self.typing else {
            return;
        };
        match key {
            KeyCode::Escape => self.typing = None,
            KeyCode::Enter | KeyCode::NumpadEnter => self.commit(console),
            KeyCode::Tab | KeyCode::ArrowUp | KeyCode::ArrowDown => {
                if let Typing::Custom { field, .. } = typing {
                    *field = match field {
                        Field::Label => Field::Command,
                        Field::Command => Field::Label,
                    };
                }
            }
            KeyCode::Backspace => {
                typing_text(typing).0.pop();
            }
            _ => {
                if let Some(text) = text {
                    let (buffer, limit) = typing_text(typing);
                    let room = limit.saturating_sub(buffer.chars().count());
                    buffer.extend(text.chars().filter(|c| !c.is_control()).take(room));
                }
            }
        }
    }

    /// Enter while typing: a page's name is saved; a custom choice's name goes
    /// on to its command, and its command saves it.
    fn commit(&mut self, console: &mut ViewerConsole) {
        match self.typing.take() {
            Some(Typing::Page { page, text, first }) => {
                console.wheel_pages.rename_page(page, &text, first);
                self.sync(console);
            }
            Some(Typing::Custom {
                at,
                label,
                command,
                field,
            }) => {
                let label_done = !label.trim().is_empty();
                if field == Field::Label && label_done {
                    self.typing = Some(Typing::Custom {
                        at,
                        label,
                        command,
                        field: Field::Command,
                    });
                    return;
                }
                if !label_done || command.trim().is_empty() {
                    // Stay on the form, on the field still empty.
                    let field = if label_done {
                        Field::Command
                    } else {
                        Field::Label
                    };
                    self.typing = Some(Typing::Custom {
                        at,
                        label,
                        command,
                        field,
                    });
                    return;
                }
                let set =
                    console
                        .wheel_pages
                        .set_choice(self.page, at, Slot::Custom { label, command });
                self.sync(console);
                if let Some(row) = set {
                    self.column = Column::Choices;
                    self.rows[1] = row;
                }
            }
            None => {}
        }
    }

    /// A key while the catalogue is open.
    fn picker_key(&mut self, key: KeyCode, console: &mut ViewerConsole) {
        match key {
            KeyCode::Escape | KeyCode::Backspace => self.picker = None,
            KeyCode::ArrowUp | KeyCode::KeyW => self.pick_step(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.pick_step(1),
            KeyCode::PageUp => self.pick_step(-(PICK_LINES as i32 - 2)),
            KeyCode::PageDown => self.pick_step(PICK_LINES as i32 - 2),
            KeyCode::Home => self.pick_step(-(pick_lines().len() as i32)),
            KeyCode::End => self.pick_step(pick_lines().len() as i32),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(picker) = self.picker {
                    self.pick(picker.highlighted, console);
                }
            }
            _ => {}
        }
    }
}

/// The text a typing state is filling, with its length limit.
fn typing_text(typing: &mut Typing) -> (&mut String, usize) {
    match typing {
        Typing::Page { text, .. } => (text, pages::NAME_CHARS),
        Typing::Custom {
            label,
            field: Field::Label,
            ..
        } => (label, pages::LABEL_CHARS),
        Typing::Custom { command, .. } => (command, pages::COMMAND_CHARS),
    }
}

/// Scroll the catalogue so its highlighted line shows, with its heading when
/// it is the first under one.
fn reveal_pick(picker: &mut Picker, count: usize) {
    let lines = pick_lines();
    let top = match picker.highlighted.checked_sub(1) {
        Some(above) if matches!(lines[above], PickLine::Heading(_)) => above,
        _ => picker.highlighted,
    };
    if top < picker.first {
        picker.first = top;
    } else if picker.highlighted >= picker.first + PICK_LINES {
        picker.first = picker.highlighted + 1 - PICK_LINES;
    }
    picker.first = picker.first.min(count.saturating_sub(PICK_LINES));
}

impl SettingsMenu {
    /// Open the quick wheel's page editor as `mode`.
    pub(crate) fn open_wheel_editor(&mut self, console: &ViewerConsole, mode: WheelMode) {
        self.wheel.open(console, mode);
    }

    /// Whether the quick wheel's page editor is on show.
    #[cfg(test)]
    pub(crate) fn wheel_editor_open(&self) -> bool {
        self.wheel.is_open()
    }

    /// Press `keys` in the editor, each typing its text if it has one (world
    /// shots).
    #[cfg(test)]
    pub(crate) fn wheel_keys_for_shot(
        &mut self,
        keys: &[(KeyCode, Option<&str>)],
        console: &mut ViewerConsole,
    ) {
        for (key, text) in keys {
            self.wheel_key(*key, *text, console);
        }
    }

    /// Key `key` (typing `text`) while the editor is open.
    pub(super) fn wheel_key(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        let editor = &mut self.wheel;
        if editor.typing.is_some() {
            editor.typing_key(key, text, console);
            return SettingsResult::None;
        }
        if editor.picker.is_some() {
            editor.picker_key(key, console);
            return SettingsResult::None;
        }
        // A step awaiting its confirmation takes the next key: the same key
        // again confirms it, anything else keeps things as they are.
        if let Some(confirm) = editor.confirm {
            match (confirm, key) {
                (Confirm::RemovePage(_), KeyCode::Delete) => editor.remove(console),
                (Confirm::Restore, KeyCode::Enter | KeyCode::NumpadEnter) => {
                    editor.restore(console);
                }
                _ => editor.confirm = None,
            }
            return SettingsResult::None;
        }
        let category = editor.mode == WheelMode::Category;
        let moving = console.modifier_held();
        match key {
            KeyCode::Escape if category => return self.back(),
            KeyCode::Escape => editor.close(),
            KeyCode::Tab | KeyCode::BracketRight if category => {
                return SettingsResult::ClassicCycle(1);
            }
            KeyCode::BracketLeft if category => return SettingsResult::ClassicCycle(-1),
            KeyCode::ArrowUp | KeyCode::KeyW if moving => editor.shift(-1, console),
            KeyCode::ArrowDown | KeyCode::KeyS if moving => editor.shift(1, console),
            KeyCode::ArrowUp | KeyCode::KeyW => editor.step(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => editor.step(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => editor.column = Column::Pages,
            KeyCode::ArrowRight | KeyCode::KeyD | KeyCode::Tab => {
                if editor.column == Column::Pages && editor.rows[0] < editor.pages.len() {
                    editor.page = editor.rows[0];
                }
                if !editor.force_shown() {
                    editor.column = Column::Choices;
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => editor.activate(console),
            KeyCode::Delete => editor.remove(console),
            KeyCode::F2 => editor.rename(),
            _ => {}
        }
        SettingsResult::None
    }

    /// The pointer while the editor is open: hovering a row focuses it, a click
    /// acts; the rail and the way back answer as on Settings.
    pub(super) fn wheel_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        let Some(event) = self.wheel.ui.pointer(event) else {
            return SettingsResult::None;
        };
        let editor = &mut self.wheel;
        let category = editor.mode == WheelMode::Category;
        if let Some(slot) = event
            .token
            .and_then(crate::menu::classic::panel::chrome_slot)
        {
            return match event.kind {
                UiEventKind::Activate if category => SettingsResult::Classic(slot),
                _ => SettingsResult::None,
            };
        }
        if event.kind == UiEventKind::Activate && event.token == Some(BACK) {
            editor.typing = None;
            editor.picker = None;
            if category {
                return self.back();
            }
            editor.close();
            return SettingsResult::None;
        }
        match event.kind {
            UiEventKind::Wheel => {
                if let Some(picker) = &mut editor.picker {
                    let notches = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
                    let last = pick_lines().len().saturating_sub(PICK_LINES);
                    picker.first =
                        (picker.first as i32 + notches * 3).clamp(0, last as i32) as usize;
                }
            }
            UiEventKind::HoverEnter | UiEventKind::Hover => {
                if let Some(token) = event.token {
                    editor.hover(token);
                }
            }
            UiEventKind::Activate => editor.click(event.token, console),
            _ => {}
        }
        SettingsResult::None
    }
}

impl WheelEditor {
    /// The pointer resting on `token`: a row takes the focus (in the pages
    /// column without changing the page shown, which a click does), a line of
    /// the catalogue the highlight.
    fn hover(&mut self, token: u16) {
        if self.typing.is_some() {
            return;
        }
        if let Some(picker) = &mut self.picker {
            if let Some(slot) = token
                .checked_sub(PICK_BASE)
                .filter(|slot| *slot < PICK_LINES as u16)
            {
                let line = picker.first + usize::from(slot);
                if matches!(
                    pick_lines().get(line),
                    Some(PickLine::Action(_) | PickLine::Emote(_) | PickLine::Custom)
                ) {
                    picker.highlighted = line;
                }
            }
            return;
        }
        let pages = self.pages.len();
        match token {
            PAGE_BASE.. if token < PAGE_BASE + pages as u16 => {
                self.column = Column::Pages;
                self.rows[0] = usize::from(token - PAGE_BASE);
            }
            ADD_PAGE => {
                self.column = Column::Pages;
                self.rows[0] = pages;
            }
            RESTORE => {
                self.column = Column::Pages;
                self.rows[0] = self.restore_row();
            }
            ADD_FORCE => {
                if let Some(row) = self.force_row() {
                    self.column = Column::Pages;
                    self.rows[0] = row;
                }
            }
            SOUNDS => {
                self.column = Column::Pages;
                self.rows[0] = self.sounds_row();
            }
            ADD_CHOICE => {
                self.column = Column::Choices;
                self.rows[1] = self.choices().len();
            }
            CHOICE_BASE.. if token < CHOICE_BASE + MAX_CHOICES as u16 => {
                self.column = Column::Choices;
                self.rows[1] = usize::from(token - CHOICE_BASE).min(self.choice_rows() - 1);
            }
            _ => {}
        }
    }

    /// A click on `token` (nothing when `None`).
    fn click(&mut self, token: Option<u16>, console: &mut ViewerConsole) {
        let confirm = self.confirm.take();
        // While typing, the form's own controls act; a click elsewhere saves a
        // page's name and leaves a custom choice's form as it is.
        if let Some(typing) = &mut self.typing {
            match (typing, token) {
                (Typing::Custom { field, .. }, Some(FORM_LABEL)) => *field = Field::Label,
                (Typing::Custom { field, .. }, Some(FORM_COMMAND)) => *field = Field::Command,
                (Typing::Custom { .. }, Some(FORM_SAVE)) => {
                    // Saving from the button goes straight to the command's check.
                    if let Some(Typing::Custom { field, .. }) = &mut self.typing {
                        *field = Field::Command;
                    }
                    self.commit(console);
                }
                (Typing::Custom { .. }, Some(FORM_CANCEL)) => self.typing = None,
                (Typing::Custom { .. }, _) => {}
                (Typing::Page { page, .. }, Some(token)) if token == PAGE_BASE + *page as u16 => {}
                (Typing::Page { .. }, _) => self.commit(console),
            }
            return;
        }
        if let Some(picker) = self.picker {
            match token {
                Some(token) if (PICK_BASE..PICK_BASE + PICK_LINES as u16).contains(&token) => {
                    let line = picker.first + usize::from(token - PICK_BASE);
                    self.pick(line, console);
                }
                Some(PICK_SCROLL) => {}
                _ => self.picker = None,
            }
            return;
        }
        let Some(token) = token else {
            return;
        };
        let pages = self.pages.len();
        match token {
            PAGE_BASE.. if token < PAGE_BASE + pages as u16 => {
                let page = usize::from(token - PAGE_BASE);
                self.column = Column::Pages;
                self.rows[0] = page;
                if self.page != page {
                    self.page = page;
                    self.rows[1] = 0;
                }
            }
            ADD_PAGE => self.add_page(console),
            ADD_FORCE => self.add_force(console),
            RESTORE => {
                self.confirm = confirm.filter(|confirm| *confirm == Confirm::Restore);
                self.column = Column::Pages;
                self.restore(console);
            }
            PAGE_RENAME => {
                self.column = Column::Pages;
                self.rename();
            }
            PAGE_UP | PAGE_DOWN => {
                self.column = Column::Pages;
                self.shift(if token == PAGE_UP { -1 } else { 1 }, console);
            }
            PAGE_REMOVE => {
                self.confirm =
                    confirm.filter(|confirm| *confirm == Confirm::RemovePage(self.rows[0]));
                self.column = Column::Pages;
                self.remove(console);
            }
            SOUNDS => {
                self.column = Column::Pages;
                self.rows[0] = self.sounds_row();
                self.toggle_sounds(console);
            }
            CHOICE_BASE.. if token < CHOICE_BASE + MAX_CHOICES as u16 || token == ADD_CHOICE => {
                self.column = Column::Choices;
                self.rows[1] = if token == ADD_CHOICE {
                    self.choices().len()
                } else {
                    usize::from(token - CHOICE_BASE)
                };
                self.activate(console);
            }
            CHOICE_UP | CHOICE_DOWN => {
                self.column = Column::Choices;
                self.shift(if token == CHOICE_UP { -1 } else { 1 }, console);
            }
            CHOICE_REMOVE => {
                self.column = Column::Choices;
                self.remove(console);
            }
            _ => {}
        }
    }
}

// Every token stays clear of the rail's and of one another's.
const _: () = assert!(PAGE_BASE as usize + MAX_PAGES <= ADD_PAGE as usize);
const _: () = assert!(CHOICE_BASE as usize + MAX_CHOICES <= ADD_CHOICE as usize);
const _: () = assert!(PICK_BASE as usize + PICK_LINES <= PICK_SCROLL as usize);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_wheel::catalog;

    #[test]
    fn installed_emotes_follow_the_stock_ones_and_pick_as_sjkemote() {
        let dance = crate::emotes::parse(
            "kane_dance",
            r#"{"name": "Dance", "clip": "kane_dance", "length_ms": 3400, "local": true}"#,
        )
        .unwrap();
        let saved = crate::emotes::lock().catalogue.replace(vec![dance]);
        let lines = pick_lines();
        let last = catalog::action_index("emote_stop").unwrap();
        let at = lines
            .iter()
            .position(|line| *line == PickLine::Action(last))
            .unwrap();
        assert_eq!(lines[at + 1], PickLine::Emote(0));
        assert_eq!(
            emote_choice(0),
            Some(("Dance".to_owned(), "sjkemote kane_dance".to_owned()))
        );
        crate::emotes::lock().catalogue = saved;
    }

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn press(
        menu: &mut SettingsMenu,
        console: &mut ViewerConsole,
        code: KeyCode,
    ) -> SettingsResult {
        menu.wheel_key(code, None, console)
    }

    fn type_text(menu: &mut SettingsMenu, console: &mut ViewerConsole, text: &str) {
        menu.wheel_key(KeyCode::KeyA, Some(text), console);
    }

    fn names(console: &ViewerConsole) -> Vec<String> {
        console
            .wheel_pages
            .pages()
            .iter()
            .map(|page| page.name.clone())
            .collect()
    }

    #[test]
    fn the_catalogue_lists_every_action_once_under_its_group() {
        let lines = pick_lines();
        for index in 0..ACTIONS.len() {
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| **line == PickLine::Action(index))
                    .count(),
                1
            );
        }
        assert_eq!(lines.last(), Some(&PickLine::Custom));
        assert!(matches!(lines[0], PickLine::Heading("Camera")));
    }

    #[test]
    fn a_page_is_added_named_filled_and_moved_from_the_keys() {
        let (directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_wheel_editor(&console, WheelMode::Overlay);
        // Down past the four pages to Add a page: a new page, named at once.
        for _ in 0..4 {
            press(&mut menu, &mut console, KeyCode::ArrowDown);
        }
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(
            names(&console),
            ["Force", "General", "Toys", "Weather", "Page 5"]
        );
        type_text(&mut menu, &mut console, "Duels");
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(names(&console)[4], "Duels");
        assert_eq!(console.wheel_pages.pages()[4].id, "duels");
        // Its choices: add an action from the catalogue.
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(menu.wheel.column, Column::Choices);
        press(&mut menu, &mut console, KeyCode::Enter);
        assert!(menu.wheel.picker.is_some());
        let duel = catalog::action_index("duel").unwrap();
        let line = pick_lines()
            .iter()
            .position(|line| *line == PickLine::Action(duel))
            .unwrap();
        while menu.wheel.picker.unwrap().highlighted < line {
            press(&mut menu, &mut console, KeyCode::ArrowDown);
        }
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(console.wheel_pages.pages()[4].choices, [Slot::Action(duel)]);
        // A custom command: picked last in the catalogue, then named and typed.
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        press(&mut menu, &mut console, KeyCode::Enter);
        press(&mut menu, &mut console, KeyCode::End);
        press(&mut menu, &mut console, KeyCode::Enter);
        type_text(&mut menu, &mut console, "Ready");
        press(&mut menu, &mut console, KeyCode::Enter);
        // Enter on an empty command stays on the form.
        press(&mut menu, &mut console, KeyCode::Enter);
        assert!(menu.wheel.typing.is_some());
        type_text(&mut menu, &mut console, "ready");
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(
            console.wheel_pages.pages()[4].choices[1],
            Slot::Custom {
                label: "Ready".into(),
                command: "ready".into()
            }
        );
        // Shift and Up moves it before the duel; Delete removes it.
        console.set_shift(true);
        press(&mut menu, &mut console, KeyCode::ArrowUp);
        console.set_shift(false);
        assert_eq!(console.wheel_pages.pages()[4].choices[0].label(), "Ready");
        assert_eq!(menu.wheel.rows[1], 0);
        press(&mut menu, &mut console, KeyCode::Delete);
        assert_eq!(console.wheel_pages.pages()[4].choices.len(), 1);
        // Left to the pages, the page moved up before Weather.
        press(&mut menu, &mut console, KeyCode::ArrowLeft);
        console.set_shift(true);
        press(&mut menu, &mut console, KeyCode::ArrowUp);
        console.set_shift(false);
        assert_eq!(
            names(&console),
            ["Force", "General", "Toys", "Duels", "Weather"]
        );
        // All of it is in the file.
        let saved = crate::quick_wheel::pages::WheelPages::load(directory.path());
        assert_eq!(saved.pages(), console.wheel_pages.pages());
        // Escape closes the overlay.
        press(&mut menu, &mut console, KeyCode::Escape);
        assert!(!menu.wheel_editor_open());
    }

    #[test]
    fn removing_a_page_and_restoring_the_defaults_ask_twice() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_wheel_editor(&console, WheelMode::Category);
        press(&mut menu, &mut console, KeyCode::Delete);
        assert_eq!(console.wheel_pages.pages().len(), 4, "asks first");
        // Any other key keeps it.
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        press(&mut menu, &mut console, KeyCode::Delete);
        assert_eq!(console.wheel_pages.pages().len(), 4);
        press(&mut menu, &mut console, KeyCode::Delete);
        // The Force page goes the same way, and a row offers it back.
        assert_eq!(names(&console), ["General", "Toys", "Weather"]);
        assert_eq!(menu.wheel.force_row(), Some(4));
        // So does the Toys page, an ordinary page: no row offers it back (Add a
        // page and the catalogue do).
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        press(&mut menu, &mut console, KeyCode::Delete);
        press(&mut menu, &mut console, KeyCode::Delete);
        assert_eq!(names(&console), ["General", "Weather"]);
        assert_eq!(menu.wheel.force_row(), Some(3));
        press(&mut menu, &mut console, KeyCode::ArrowUp);
        press(&mut menu, &mut console, KeyCode::Delete);
        press(&mut menu, &mut console, KeyCode::Delete);
        assert_eq!(names(&console), ["Weather"]);
        assert_eq!(menu.wheel.force_row(), Some(2));
        // The last page stays.
        assert_eq!(menu.wheel.rows[0], 0);
        press(&mut menu, &mut console, KeyCode::Delete);
        press(&mut menu, &mut console, KeyCode::Delete);
        assert_eq!(names(&console), ["Weather"]);
        assert_eq!(menu.wheel.confirm, None);
        // Up from the first row wraps to the sounds, then Restore, then Add the
        // Force page: it comes back last, as there is no General.
        press(&mut menu, &mut console, KeyCode::ArrowUp);
        assert_eq!(menu.wheel.rows[0], menu.wheel.sounds_row());
        press(&mut menu, &mut console, KeyCode::ArrowUp);
        assert_eq!(menu.wheel.rows[0], menu.wheel.restore_row());
        press(&mut menu, &mut console, KeyCode::ArrowUp);
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(names(&console), ["Weather", "Force"]);
        assert_eq!((menu.wheel.rows[0], menu.wheel.page), (1, 1));
        assert_eq!(menu.wheel.force_row(), None);
        // On the Force page, Enter and Right stay with the pages: no choices.
        press(&mut menu, &mut console, KeyCode::Enter);
        press(&mut menu, &mut console, KeyCode::ArrowRight);
        assert_eq!(menu.wheel.column, Column::Pages);
        // Restore: Enter twice.
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        assert_eq!(menu.wheel.rows[0], menu.wheel.restore_row());
        press(&mut menu, &mut console, KeyCode::Enter);
        assert!(!console.wheel_pages.is_default());
        press(&mut menu, &mut console, KeyCode::Enter);
        assert!(console.wheel_pages.is_default());
        // In Settings' rail, Tab goes on and Escape leaves Settings.
        assert!(matches!(
            press(&mut menu, &mut console, KeyCode::Tab),
            SettingsResult::ClassicCycle(1)
        ));
        assert!(matches!(
            press(&mut menu, &mut console, KeyCode::Escape),
            SettingsResult::Back
        ));
    }

    #[test]
    fn a_choice_is_changed_from_the_catalogue_and_escape_keeps_it() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_wheel_editor(&console, WheelMode::Overlay);
        // General is second now; the Force page above it has no choices.
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        press(&mut menu, &mut console, KeyCode::ArrowRight);
        press(&mut menu, &mut console, KeyCode::Enter);
        // The catalogue opens on the choice's own action.
        let picker = menu.wheel.picker.unwrap();
        assert_eq!(pick_lines()[picker.highlighted], PickLine::Action(0));
        press(&mut menu, &mut console, KeyCode::Escape);
        assert!(menu.wheel.picker.is_none());
        assert!(console.wheel_pages.is_default());
        press(&mut menu, &mut console, KeyCode::Enter);
        press(&mut menu, &mut console, KeyCode::ArrowDown);
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(
            console.wheel_pages.pages()[1].choices[0].label(),
            "Free camera"
        );
        // F2 on a page renames it; Escape leaves the name.
        press(&mut menu, &mut console, KeyCode::ArrowLeft);
        press(&mut menu, &mut console, KeyCode::F2);
        press(&mut menu, &mut console, KeyCode::Backspace);
        press(&mut menu, &mut console, KeyCode::Escape);
        assert_eq!(names(&console)[1], "General");
        press(&mut menu, &mut console, KeyCode::F2);
        for _ in 0.."General".len() {
            press(&mut menu, &mut console, KeyCode::Backspace);
        }
        type_text(&mut menu, &mut console, "Main");
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(names(&console)[1], "Main");
        assert_eq!(
            console.wheel_pages.pages()[1].id,
            "general",
            "binds keep working"
        );
    }

    #[test]
    fn the_sounds_row_switches_the_wheels_sounds_from_the_keys_and_a_click() {
        let (_directory, mut console) = console();
        let sounds = |console: &ViewerConsole| console.bool_cvar(crate::quick_wheel::SOUNDS_CVAR);
        assert_eq!(sounds(&console), Some(true), "on by default");
        let mut menu = SettingsMenu::new();
        menu.open_wheel_editor(&console, WheelMode::Category);
        // Under the four pages, Add a page and Restore: the last row.
        for _ in 0..6 {
            press(&mut menu, &mut console, KeyCode::ArrowDown);
        }
        assert_eq!(menu.wheel.rows[0], menu.wheel.sounds_row());
        press(&mut menu, &mut console, KeyCode::Enter);
        assert_eq!(sounds(&console), Some(false));
        assert!(!menu.wheel.sounds);
        // The pages are untouched, and Space turns them back on.
        assert!(console.wheel_pages.is_default());
        press(&mut menu, &mut console, KeyCode::Space);
        assert_eq!(sounds(&console), Some(true));
        // A click on the row, wherever the focus was.
        press(&mut menu, &mut console, KeyCode::ArrowRight);
        menu.wheel.click(Some(SOUNDS), &mut console);
        assert_eq!(sounds(&console), Some(false));
        assert_eq!(menu.wheel.column, Column::Pages);
        // Set elsewhere (the Interface row, the console), the editor follows.
        console.set_cvar(crate::quick_wheel::SOUNDS_CVAR, "1");
        menu.open_wheel_editor(&console, WheelMode::Overlay);
        assert!(menu.wheel.sounds);
    }

    #[test]
    fn the_shown_page_names_the_key_that_opens_it() {
        let (_directory, console) = console();
        let mut editor = WheelEditor::default();
        editor.open(&console, WheelMode::Category);
        // Q opens the page used last; Force, General and Toys have no key of
        // their own.
        assert_eq!(editor.keys[0], "Q (last page)");
        assert_eq!(editor.keys[1], "Q (last page)");
        assert_eq!(editor.keys[2], "Q (last page)");
        assert_eq!(editor.keys[3], "R");
    }
}

//! Retained tabbed settings UI backed directly by archived shell cvars.

use crate::console::ViewerConsole;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use sjk_shell::CvarValue;
use sjk_ui::{DrawList, Rect};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

mod catalog;
mod classic_view;
mod display;
mod groups;
mod help;
mod hud_picker;
pub(crate) mod mode;
mod numeric;
mod pointer;
pub(crate) mod quick;
mod resolution;
mod resolution_list;
mod search;
mod sjk_popup;
pub(crate) mod sjk_view;
mod usage;
mod wheel_editor;
mod wheel_editor_view;

#[cfg(test)]
pub(crate) use catalog::IMPORT_ROW;
use catalog::*;
pub(crate) use catalog::{FIRST_SETUP_CAPTION, RESOLUTIONS};
pub(crate) use display::{
    DisplayMode, EXCLUSIVE_CVAR, MonitorModes, exclusive_supported, exclusive_video_mode,
    suspended_after_focus,
};
pub(crate) use groups::{Advanced, Group, Simple};
use resolution::{PickResult, ResolutionChoice, ResolutionPicker};
pub(crate) use sjk_view::Rail;
pub(crate) use wheel_editor::WheelMode;

/// The renderer settings' rows (Image, Lighting, Shadows, Weather): tab, label,
/// cvar and help text, for a hosting engine to mirror and offer them.
pub(crate) fn renderer_rows() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    [RENDER_IMAGE, RENDER_LIGHTING, RENDER_SHADOWS, RENDER_WEATHER]
        .into_iter()
        .zip(RENDERER_TABS)
        .flat_map(|(rows, tab)| {
            rows.iter().map(move |row| {
                (tab, row.label, row.cvar, help::help(row.cvar).unwrap_or(""))
            })
        })
        .collect()
}
pub(crate) enum SettingsResult {
    None,
    Back,
    /// The "Quick wheel pages" row: the SJK UI shows its Quick wheel category,
    /// the other styles the editor on its own ([`WheelMode`]).
    OpenWheelPages,
    /// The "SJK identity key" row: the Identity page, over the settings.
    OpenIdentity,
    /// First setup's "Import from another client" row: the Import page with
    /// the configs found (or the file dialog), over the settings.
    OpenImport,
    /// The Gameplay FPS row's Enter: the card that explains it, over the settings.
    OpenGameplayFps,
    /// The installed-pack browser, over the settings.
    OpenAssets,
    /// A button of the classic panel screen around the options: index into
    /// its page's slots.
    Classic(usize),
    /// Move the classic panel to the next (1) or previous (-1) group.
    ClassicCycle(i32),
    /// Leave First setup's pop-up for the whole Settings screen.
    AllSettings,
}

/// What First setup's pop-up does with `key` before its rows see it: Tab
/// leaves for the whole Settings screen; the search's and the groups' keys do
/// nothing, as it has neither.
fn popup_key(key: KeyCode) -> Option<SettingsResult> {
    match key {
        KeyCode::Tab => Some(SettingsResult::AllSettings),
        KeyCode::Slash | KeyCode::NumpadDivide | KeyCode::BracketLeft | KeyCode::BracketRight => {
            Some(SettingsResult::None)
        }
        _ => None,
    }
}

/// One line of a classic option panel: a heading (a group of search
/// results) or a setting row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Line {
    Heading(&'static str),
    Row(usize),
}

/// The lines a classic option panel shows: a span of the tab, or a search's
/// results under their groups; where its slider bars sit across a row, and
/// which part is scrolled into the panel when it has more lines than fit.
struct ClassicRows {
    lines: Vec<Line>,
    slider_span: (f32, f32),
    /// First shown line.
    first: usize,
    /// Lines the panel showed last frame.
    visible: usize,
    /// A row shown apart from the lines, after them in the keyboard's order:
    /// First setup's pop-up keeps "Don't show at start" at its foot, always
    /// in view. Only that pop-up pins one.
    pinned: Option<usize>,
}

impl ClassicRows {
    /// The lines of rows `rows`, without headings.
    fn span(rows: std::ops::Range<usize>) -> Vec<Line> {
        rows.map(Line::Row).collect()
    }

    /// Furthest the lines scroll.
    fn max_first(&self) -> usize {
        self.lines.len().saturating_sub(self.visible)
    }

    /// Line of setting row `row`, if the panel shows it.
    fn position(&self, row: usize) -> Option<usize> {
        self.lines.iter().position(|line| *line == Line::Row(row))
    }

    /// The setting rows, in order, the pinned one last.
    fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Row(row) => Some(*row),
                Line::Heading(_) => None,
            })
            .chain(self.pinned)
    }

    /// Whether the panel shows row `row`, in its lines or pinned.
    fn shows(&self, row: usize) -> bool {
        self.pinned == Some(row) || self.position(row).is_some()
    }

    /// The row `direction` rows away from `selected`, wrapping; the first
    /// row when `selected` is not shown.
    fn step(&self, selected: usize, direction: i32) -> Option<usize> {
        let rows: Vec<usize> = self.rows().collect();
        let first = *rows.first()?;
        let Some(index) = rows.iter().position(|row| *row == selected) else {
            return Some(first);
        };
        Some(rows[(index as i32 + direction).rem_euclid(rows.len() as i32) as usize])
    }

    /// Scroll by `rows` (negative = up) without moving the selection.
    fn scroll_by(&mut self, rows: i32) {
        self.first = (self.first as i32 + rows).clamp(0, self.max_first() as i32) as usize;
    }

    /// Scroll so the panel shows `ratio` (0 = top, 1 = bottom) of the span.
    fn scroll_to_ratio(&mut self, ratio: f32) {
        self.first = (ratio.clamp(0.0, 1.0) * self.max_first() as f32).round() as usize;
    }

    /// Scroll just enough to show row `selected`, and its heading when it
    /// is the first row under one.
    fn reveal(&mut self, selected: usize) {
        let Some(offset) = self.position(selected) else {
            return;
        };
        let visible = self.visible.max(1);
        let heading = offset
            .checked_sub(1)
            .filter(|above| matches!(self.lines[*above], Line::Heading(_)));
        for line in heading.into_iter().chain([offset]) {
            self.first = self.first.min(line).max((line + 1).saturating_sub(visible));
        }
    }
}

/// Where the panel was before a search, to return to when it is cleared.
struct SearchReturn {
    section: Section,
    tab: usize,
    lines: Vec<Line>,
    selected: usize,
    first: usize,
}

/// One choice of a classic dropdown.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pick {
    /// One of a choice row's values.
    Value(&'static str),
    /// A display mode.
    Mode(DisplayMode),
    /// A graphics quality level.
    Quality(crate::graphics_quality::Level),
}

/// A classic+ dropdown open on row `row`: its choices, the highlighted one
/// and the one in use. Nothing changes until a choice is applied.
struct Dropdown {
    row: usize,
    picks: Vec<Pick>,
    highlighted: usize,
    current: usize,
}

/// A Text setting being typed. Its row is fixed when typing starts, so Enter
/// writes the setting that was opened even if the pointer moved meanwhile.
struct TextDraft {
    row: usize,
    text: String,
}

/// Which set of tabs the form shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    /// The general settings ([`TABS`]).
    General,
    /// SJK's renderer settings ([`RENDERER_TABS`]).
    Renderer,
    /// A classic Setup group gathering rows of several tabs ([`Group`]).
    Group(Group),
    /// Every OPTIONS setting, for the classic panel's search ([`search`]).
    Search,
}

pub(crate) struct SettingsMenu {
    section: Section,
    tab: usize,
    selected: usize,
    values: Vec<String>,
    /// Each row's default and whether its value differs (classic+ panels).
    defaults: Vec<classic_view::RowDefault>,
    /// The classic+ detail box's facts line and the description line's keys,
    /// rewritten each frame.
    detail_facts: String,
    key_hint: String,
    editing: Option<TextDraft>,
    /// What the window's monitor offers; asked for each time the screen opens.
    monitor: Option<MonitorModes>,
    /// The screen opened and wants fresh [`MonitorModes`].
    wants_monitor: bool,
    /// Scratch list of the resolutions on offer.
    choices: Vec<ResolutionChoice>,
    /// The resolution list, open over the form.
    picker: ResolutionPicker,
    /// The HUD picker, open over the form.
    hud: hud_picker::HudPicker,
    numeric: Option<crate::menu_widgets::numeric::NumericEdit>,
    /// Set while the screen is a classic option panel.
    classic: Option<ClassicRows>,
    /// The classic panel's search: what is typed, whether the field has the
    /// keyboard, and where the panel was before the search.
    search: String,
    searching: bool,
    search_return: Option<SearchReturn>,
    /// Key bindings matching the search typed here, which the panel offers
    /// through the KEY BINDINGS tab (0 when none or no search).
    elsewhere: usize,
    /// The classic panel's dropdown, while one is open.
    dropdown: Option<Dropdown>,
    /// Where the rows' controls begin (window x) when the SJK UI drew the
    /// screen last frame: a click left of it, on a row's name, only chooses
    /// the row. `None` under the other views.
    sjk_controls: Option<f32>,
    ui: MenuCanvas,
    /// The quick wheel's page editor, drawn in place of the rows while open.
    wheel: wheel_editor::WheelEditor,
    /// The SJK renderer is on (not Vanilla): renderer settings carry its alpha
    /// warning. Read with the values.
    alpha: bool,
    /// The SJK UI's Settings shows the rows, in this mode: the top bar offers
    /// the other mode (F2). `None` for a classic+ panel.
    screen: Option<mode::SettingsMode>,
    /// The SJK UI's category on show, for settings usage.
    screen_label: &'static str,
    /// What the player does, for settings usage ([`usage`]).
    usage: usage::UsageTrack,
}

impl SettingsMenu {
    /// The renderer section is the one on show (or the SJK UI's Graphics,
    /// which gathers its tabs).
    pub(crate) fn renderer_open(&self) -> bool {
        matches!(
            self.section,
            Section::Renderer | Section::Group(Group::Graphics)
        )
    }

    /// The HUD picker covers the screen; it draws itself whatever the menu
    /// style ([`Self::append_hud_overlay`]).
    pub(crate) fn hud_picker_open(&self) -> bool {
        self.hud.is_open()
    }

    /// The resolution list covers the screen: the SJK UI's card
    /// ([`Self::append_resolutions_sjk`]), whatever the menu style.
    pub(crate) fn resolutions_open(&self) -> bool {
        self.picker.is_open()
    }

    /// Draw the open resolution list as the SJK UI's card.
    pub(crate) fn append_resolutions_sjk(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        self.sjk_controls = None;
        self.append_resolutions(target, viewport, reveal);
    }

    /// The rows are a classic+ panel's (a group, a span of a tab, a search),
    /// which the classic+ and SJK UI views draw.
    pub(crate) fn has_panel_rows(&self) -> bool {
        self.classic.is_some()
    }

    /// The panel shows a search's results instead of its group.
    pub(crate) fn searching_results(&self) -> bool {
        self.section == Section::Search
    }

    pub(crate) fn new() -> Self {
        Self {
            section: Section::General,
            tab: 0,
            selected: 0,
            values: Vec::with_capacity(12),
            defaults: Vec::with_capacity(20),
            detail_facts: String::with_capacity(96),
            key_hint: String::with_capacity(96),
            editing: None,
            monitor: None,
            wants_monitor: false,
            choices: Vec::with_capacity(48),
            picker: ResolutionPicker::new(),
            hud: hud_picker::HudPicker::default(),
            numeric: None,
            classic: None,
            search: String::new(),
            searching: false,
            search_return: None,
            elsewhere: 0,
            dropdown: None,
            sjk_controls: None,
            ui: MenuCanvas::new(),
            wheel: wheel_editor::WheelEditor::default(),
            alpha: false,
            screen: None,
            screen_label: "",
            usage: usage::UsageTrack::default(),
        }
    }

    /// Whether First setup's rows are on show: its classic group (the SJK UI's
    /// category or pop-up), or the tabbed screen's FIRST SETUP tab.
    pub(crate) fn on_first_setup(&self) -> bool {
        match self.section {
            Section::Group(group) => group == Group::Quick,
            Section::General => self.tab == QUICK_TAB,
            _ => false,
        }
    }

    /// The caption of tab `tab` (`"AUDIO"`), if there is one.
    pub(crate) fn tab_caption(tab: usize) -> Option<&'static str> {
        TABS.get(tab).copied()
    }

    /// Index of the tab captioned `caption` (`"AUDIO"`), if there is one.
    pub(crate) fn tab_index(caption: &str) -> Option<usize> {
        TABS.iter().position(|tab| *tab == caption)
    }

    /// Open on tab `tab` (clamped to the catalogue).
    pub(crate) fn open_tab(&mut self, console: &ViewerConsole, tab: usize) {
        self.section = Section::General;
        self.tab = tab.min(TABS.len() - 1);
        self.selected = 0;
        self.editing = None;
        self.picker.close();
        self.hud.mark_stale();
        self.wants_monitor = true;
        self.numeric = None;
        self.classic = None;
        self.wheel.close();
        self.refresh(console);
    }

    /// Leave the screen; a search or a list closes first where the keys
    /// handle them.
    fn back(&self) -> SettingsResult {
        SettingsResult::Back
    }

    /// Tab names of the current section.
    fn tabs(&self) -> &'static [&'static str] {
        match self.section {
            Section::General => &TABS,
            Section::Renderer => &RENDERER_TABS,
            Section::Group(group) => group.tabs(),
            Section::Search => &["SEARCH"],
        }
    }

    /// Settings of the current tab.
    fn rows(&self) -> &'static [Setting] {
        section_settings(self.section, self.tab)
    }

    /// Switch to tab `tab` of the current section.
    fn select_tab(&mut self, console: &ViewerConsole, tab: usize) {
        self.tab = tab % self.tabs().len();
        self.selected = 0;
        self.editing = None;
        self.refresh(console);
    }

    /// Open the renderer settings ([`RENDERER_TABS`]) on their first tab.
    fn open_renderer(&mut self, console: &ViewerConsole) {
        self.open_tab(console, RENDERER_TAB);
        self.section = Section::Renderer;
        self.tab = 0;
        self.selected = 0;
        self.refresh(console);
    }

    /// Whether the screen wants [`Self::set_monitor_modes`] (it just opened).
    pub(crate) fn wants_monitor_modes(&self) -> bool {
        self.wants_monitor
    }

    /// Take the window's monitor facts, which shape the resolution and
    /// display-mode choices.
    pub(crate) fn set_monitor_modes(&mut self, modes: MonitorModes, console: &ViewerConsole) {
        self.wants_monitor = false;
        self.monitor = Some(modes);
        if self.picker.is_open() {
            self.build_choices(console);
            self.picker.update_choices(&self.choices);
        }
        self.refresh(console);
    }

    /// Whether keyboard focus has rows to move through: the classic panel's
    /// lines, or the whole tab (with its key-bindings row).
    fn has_rows(&self) -> bool {
        match &self.classic {
            Some(classic) => classic.rows().next().is_some(),
            None => !self.rows().is_empty(),
        }
    }

    /// Whether row `row` is on show: in the classic panel's lines, or a row
    /// of the tab.
    fn shows(&self, row: usize) -> bool {
        match &self.classic {
            Some(classic) => classic.shows(row),
            None => row < self.rows().len(),
        }
    }

    /// Move the selection `direction` rows: through the classic panel's rows
    /// (over its headings), or the tab's, wrapping.
    fn step_selection(&mut self, direction: i32) {
        if let Some(classic) = &self.classic {
            if let Some(row) = classic.step(self.selected, direction) {
                self.selected = row;
            }
        } else {
            let count = self.rows().len();
            if count > 0 {
                self.selected =
                    (self.selected as i32 + direction).rem_euclid(count as i32) as usize;
            }
        }
        self.reveal_selected();
    }
    /// Keep the selected row inside a scrolled classic panel.
    fn reveal_selected(&mut self) {
        let selected = self.selected;
        if let Some(classic) = &mut self.classic {
            classic.reveal(selected);
        }
    }

    /// Select the "Quick wheel pages" row, scrolled into view: under the
    /// quick wheel's editor, the row it returns to.
    pub(crate) fn select_wheel_row(&mut self) {
        if let Some(row) = self
            .rows()
            .iter()
            .position(|setting| matches!(setting.kind, ValueKind::WheelPages))
        {
            self.selected = row;
            self.reveal_selected();
        }
    }

    /// Select the row of setting `cvar`, scrolled into a panel (menu snapshots).
    #[cfg(test)]
    pub(crate) fn select_cvar(&mut self, cvar: &str) {
        if let Some(row) = self.rows().iter().position(|setting| setting.cvar == cvar) {
            self.selected = row;
            self.reveal_selected();
        }
    }

    pub(crate) fn visual_selection(&self) -> (usize, bool) {
        (self.selected, false)
    }
    /// Whether a field takes the keys: the search, a text row, a number typed or the
    /// wheel editor's name and command fields.
    pub(crate) fn typing(&self) -> bool {
        self.searching
            || self.editing.is_some()
            || self.numeric.is_some()
            || self.wheel.is_open() && self.wheel.typing.is_some()
    }

    /// The text of the field that has the keys, for Ctrl+C.
    pub(crate) fn field_text(&self) -> Option<&str> {
        if self.searching {
            return Some(&self.search);
        }
        self.editing.as_ref().map(|draft| draft.text.as_str())
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        if self.wheel.is_open() {
            return self.wheel.draw_list();
        }
        self.ui.draw_list()
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        let was_searching = self.searching;
        self.usage.recording = true;
        let result = self.key(event, console);
        self.usage.recording = false;
        self.after_input(console, was_searching, &result);
        result
    }

    fn key(&mut self, event: &KeyEvent, console: &mut ViewerConsole) -> SettingsResult {
        if event.state != ElementState::Pressed {
            return SettingsResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return SettingsResult::None;
        };
        if self.wheel.is_open() {
            return self.wheel_key(key, event.text.as_deref(), console);
        }
        if self.hud.is_open() {
            self.hud_picker_key(key, event.repeat, console);
            return SettingsResult::None;
        }
        if self.picker.is_open() {
            self.resolution_key(key, event.repeat, console);
            return SettingsResult::None;
        }
        if self.dropdown.is_some() {
            if !event.repeat || matches!(key, KeyCode::ArrowUp | KeyCode::ArrowDown) {
                self.dropdown_key(key, console);
            }
            return SettingsResult::None;
        }
        if self.searching {
            self.search_key(key, event.text.as_deref(), console);
            return SettingsResult::None;
        }
        if self.edit_numeric(key, event.text.as_deref(), event.repeat, console) {
            return SettingsResult::None;
        }
        if let Some(draft) = &mut self.editing {
            let buffer = &mut draft.text;
            match key {
                KeyCode::Escape => self.editing = None,
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if let Some(draft) = self.editing.take()
                        && let Some(setting) = self.rows().get(draft.row)
                    {
                        console.set_cvar(setting.cvar, draft.text.trim());
                    }
                    self.refresh(console);
                }
                KeyCode::Backspace => {
                    buffer.pop();
                }
                _ if !event.repeat => {
                    if let Some(text) = event.text.as_deref() {
                        buffer.extend(
                            text.chars()
                                .filter(|c| !c.is_control())
                                .take(128usize.saturating_sub(buffer.len())),
                        );
                    }
                }
                _ => {}
            }
            return SettingsResult::None;
        }
        if event.repeat {
            return SettingsResult::None;
        }
        let classic = self.classic.is_some();
        // Up from the pop-up's first row wraps to its foot, as it has no search.
        if self.popup() {
            if let Some(result) = popup_key(key) {
                return result;
            }
        } else if classic {
            // Classic+: `/` or Up from the first row gives the search field
            // the keyboard; Escape clears a search before it leaves.
            let at_top = self.classic.as_ref().and_then(|c| c.rows().next()) == Some(self.selected);
            if let Some(result) = self.screen_key(key, console.control_held()) {
                return result;
            }
            match key {
                KeyCode::ArrowUp | KeyCode::KeyW if at_top => {
                    self.searching = true;
                    return SettingsResult::None;
                }
                KeyCode::Escape if !self.search.is_empty() => {
                    self.set_search(console, String::new());
                    return SettingsResult::None;
                }
                _ => {}
            }
        }
        if !self.has_rows() {
            return match key {
                KeyCode::Escape => self.back(),
                _ => SettingsResult::None,
            };
        }
        // SJK: a digit (or '.', ',' and '-' where the slider allows them) typed
        // on a selected slider opens entry with it.
        if let Some(text) = event.text.as_deref()
            && text.starts_with(|c: char| c.is_ascii_digit() || ".,-".contains(c))
            && self.begin_typed(self.selected, text)
        {
            return SettingsResult::None;
        }
        match key {
            KeyCode::Tab | KeyCode::BracketRight if classic => {
                return SettingsResult::ClassicCycle(1);
            }
            KeyCode::BracketLeft if classic => return SettingsResult::ClassicCycle(-1),
            KeyCode::Tab | KeyCode::BracketRight => self.select_tab(console, self.tab + 1),
            KeyCode::BracketLeft => {
                let tab = self.tab.checked_sub(1).unwrap_or(self.tabs().len() - 1);
                self.select_tab(console, tab);
            }
            KeyCode::ArrowUp | KeyCode::KeyW => self.step_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.step_selection(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust(console, 1),
            // SJK: Space keeps stepping a slider; Enter opens its entry.
            KeyCode::Space
                if self.rows().get(self.selected).is_some_and(|setting| {
                    matches!(
                        setting.kind,
                        ValueKind::Integer { .. } | ValueKind::Float { .. }
                    )
                }) =>
            {
                self.adjust(console, 1);
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(setting) = self.rows().get(self.selected) {
                    if matches!(setting.kind, ValueKind::Text) {
                        self.begin_text(console, self.selected);
                    } else if matches!(setting.kind, ValueKind::Resolution) {
                        self.open_resolutions(console);
                    } else if matches!(setting.kind, ValueKind::HudPicker) {
                        self.open_hud_picker(console);
                    } else if matches!(setting.kind, ValueKind::WheelPages) {
                        return SettingsResult::OpenWheelPages;
                    } else if matches!(setting.kind, ValueKind::IdentityPage) {
                        return SettingsResult::OpenIdentity;
                    } else if matches!(setting.kind, ValueKind::AssetBrowser) {
                        return SettingsResult::OpenAssets;
                    } else if matches!(setting.kind, ValueKind::ImportPage) {
                        return SettingsResult::OpenImport;
                    } else if matches!(setting.kind, ValueKind::GameplayFps) {
                        return SettingsResult::OpenGameplayFps;
                    } else if classic && self.open_dropdown(console, self.selected) {
                        // Classic+: a choice opens its list; nothing changes yet.
                    } else if !self.begin_numeric(console, self.selected) {
                        self.adjust(console, 1);
                    }
                }
            }
            KeyCode::Escape => return self.back(),
            // Classic+: back to the default.
            KeyCode::Backspace | KeyCode::Delete if classic => {
                self.reset_to_default(console, self.selected);
            }
            _ => {}
        }
        SettingsResult::None
    }

    fn adjust(&mut self, console: &mut ViewerConsole, direction: i32) {
        let Some(setting) = self.rows().get(self.selected) else {
            return;
        };
        if let Some(switch) = derived_switch(setting.cvar) {
            (switch.toggle)(console);
            self.refresh(console);
            return;
        }
        // Choices name text or integer cvars (`cg_saberTrail` is an integer), or
        // a value other cvars hold (the renderer).
        if let ValueKind::Choice(values) = setting.kind {
            let cvar = setting.cvar;
            let value = choice_value(console, cvar).unwrap_or_default();
            let index = values
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(&value))
                .unwrap_or(0);
            let next = values[(index as i32 + direction).rem_euclid(values.len() as i32) as usize];
            set_choice(console, cvar, next);
            self.refresh(console);
            return;
        }
        let next = match (setting.kind, console.cvar(setting.cvar)) {
            (ValueKind::Bool, Some(CvarValue::Bool(value))) => (!value).to_string(),
            (ValueKind::Bool, Some(value)) => if switch_on(value) { "0" } else { "1" }.to_owned(),
            (ValueKind::Integer { min, max, step }, Some(CvarValue::Integer(value))) => {
                step_integer(*value, direction, min, max, step).to_string()
            }
            (ValueKind::Float { .. }, Some(CvarValue::Float(value))) => {
                match setting.kind.stepped(*value, direction) {
                    Some(next) => next,
                    None => return,
                }
            }
            (ValueKind::Resolution, _) => {
                self.step_resolution(console, direction);
                return;
            }
            (ValueKind::HudPicker, _) => {
                self.step_hud(console, direction);
                return;
            }
            (ValueKind::DisplayMode, _) => {
                DisplayMode::requested(console)
                    .step(direction, self.exclusive_available())
                    .store(console);
                self.refresh(console);
                return;
            }
            (ValueKind::GameplayFps, Some(CvarValue::Integer(value))) => {
                crate::gameplay_fps::step_preset(*value, direction).to_string()
            }
            (ValueKind::Quality, _) => {
                // Custom settings step from the level they are nearest.
                crate::graphics_quality::Level::nearest(console)
                    .step(direction)
                    .apply(console);
                self.refresh(console);
                return;
            }
            _ => return,
        };
        console.set_cvar(setting.cvar, &next);
        self.refresh(console);
    }

    /// Whether exclusive fullscreen can be offered; assumed until the
    /// monitor facts arrive, since the window falls back to borderless.
    fn exclusive_available(&self) -> bool {
        self.monitor
            .as_ref()
            .is_none_or(|monitor| monitor.exclusive)
    }

    /// Start typing Text row `row`, keeping an edit already open on it.
    fn begin_text(&mut self, console: &ViewerConsole, row: usize) {
        if self.editing.as_ref().is_some_and(|draft| draft.row == row) {
            return;
        }
        let Some(setting) = self.rows().get(row) else {
            return;
        };
        self.numeric = None;
        self.selected = row;
        self.editing = Some(TextDraft {
            row,
            text: value_text(console, setting.cvar),
        });
    }

    /// Whether a typed draft (text or number) is open. While one is, the pointer
    /// does not move the selection away from it.
    fn drafting(&self) -> bool {
        self.editing.is_some() || self.numeric.is_some()
    }

    /// A press on anything but the text draft's own row discards the draft, as
    /// clicking another control discards a numeric one.
    fn press_elsewhere(&mut self, row: Option<usize>) {
        if self
            .editing
            .as_ref()
            .is_some_and(|draft| Some(draft.row) != row)
        {
            self.editing = None;
        }
    }

    /// Hover selects `row` unless a draft is open.
    fn hover_row(&mut self, row: usize) {
        if !self.drafting() {
            self.selected = row;
        }
    }

    /// The wheel moves the selection by `direction` unless a draft is open: in
    /// a classic panel within its rows, otherwise through the scrolled tab.
    fn wheel(&mut self, direction: i32) {
        if direction == 0 || self.drafting() {
            return;
        }
        if let Some(classic) = &self.classic {
            let rows: Vec<usize> = classic.rows().collect();
            if let Some(index) = rows.iter().position(|row| *row == self.selected) {
                let next = (index as i32 + direction).clamp(0, rows.len() as i32 - 1);
                self.selected = rows[next as usize];
            } else if let Some(first) = rows.first() {
                self.selected = *first;
            }
        } else {
            let last = self.rows().len().saturating_sub(1) as i32;
            self.selected = (self.selected as i32 + direction).clamp(0, last) as usize;
        }
    }

    fn refresh(&mut self, console: &ViewerConsole) {
        self.alpha = !crate::graphics_quality::vanilla(console);
        let display = DisplayMode::requested(console).effective(self.exclusive_available());
        self.hud.read(console);
        let hud = self.hud.label();
        // While the player acts on the same rows, their values before, to
        // record what changed ([`usage`]).
        let rows = self.rows();
        let read_for = (rows.as_ptr() as usize, rows.len());
        let before = (self.usage.recording && self.usage.rows == read_for)
            .then(|| std::mem::take(&mut self.values));
        self.usage.rows = read_for;
        self.values.clear();
        self.values
            .extend(self.rows().iter().map(|setting| match setting.kind {
                ValueKind::DisplayMode => display.label().to_owned(),
                ValueKind::HudPicker => hud.clone(),
                ValueKind::WheelPages => wheel_pages_text(console),
                ValueKind::IdentityPage => IDENTITY_TEXT.to_owned(),
                ValueKind::ImportPage => IMPORT_TEXT.to_owned(),
                ValueKind::GameplayFps => crate::gameplay_fps::row_text(console),
                ValueKind::AssetBrowser => "Browse packs".to_owned(),
                ValueKind::Quality => crate::graphics_quality::shown(console).to_owned(),
                ValueKind::Bool if derived_switch(setting.cvar).is_some() => {
                    let on =
                        derived_switch(setting.cvar).is_some_and(|switch| (switch.on)(console));
                    if on { "ON" } else { "OFF" }.to_owned()
                }
                ValueKind::Bool => toggle_text(console, setting.cvar),
                _ => row_text(console, setting),
            }));
        self.defaults.clear();
        self.defaults.extend(
            self.rows()
                .iter()
                .map(|setting| classic_view::row_default(console, setting)),
        );
        if let Some(before) = before {
            self.record_change(&before);
        }
    }

    /// What `key` (with Ctrl held when `control`) asks of the screen around
    /// the rows: `/`, F3 and Ctrl+F find a setting, as in other programs; F2
    /// shows the SJK UI's other mode. `None` for a key the rows take.
    fn screen_key(&mut self, key: KeyCode, control: bool) -> Option<SettingsResult> {
        match key {
            KeyCode::Slash | KeyCode::NumpadDivide | KeyCode::F3 => {
                self.begin_search();
                Some(SettingsResult::None)
            }
            KeyCode::KeyF if control => {
                self.begin_search();
                Some(SettingsResult::None)
            }
            KeyCode::F2 if self.screen.is_some() => Some(SettingsResult::Classic(
                crate::menu::sjk::settings::MODE_TOGGLE_SLOT,
            )),
            _ => None,
        }
    }

    /// Give the search field the keyboard, what is typed in it selected so
    /// typing or a paste replaces it.
    fn begin_search(&mut self) {
        self.numeric = None;
        self.editing = None;
        self.searching = true;
        crate::text_entry::select_all(&self.search);
    }
}

/// Settings of `tab` in `section`.
fn section_settings(section: Section, tab: usize) -> &'static [Setting] {
    match section {
        Section::General => settings(tab),
        Section::Renderer => match tab {
            0 => RENDER_IMAGE,
            1 => RENDER_LIGHTING,
            2 => RENDER_SHADOWS,
            3 => RENDER_WEATHER,
            _ => &[],
        },
        Section::Group(group) => group.rows(),
        Section::Search => search::rows(),
    }
}

/// Whether `cvar` has a row a Settings search finds (any case), so a chip naming it
/// (`inline_code`) can open Settings on it.
pub(crate) fn has_row(cvar: &str) -> bool {
    search::rows()
        .iter()
        .any(|setting| setting.cvar.eq_ignore_ascii_case(cvar))
}

/// Whether `cvar` has a row on a general or renderer tab that can show
/// `value`: a switch's 0 or 1, or a number within its slider's range.
#[cfg(test)]
pub(crate) fn row_takes(cvar: &str, value: &str) -> bool {
    let row = (0..TABS.len())
        .map(settings)
        .chain([
            RENDER_IMAGE,
            RENDER_LIGHTING,
            RENDER_SHADOWS,
            RENDER_WEATHER,
        ])
        .flatten()
        .find(|setting| setting.cvar.eq_ignore_ascii_case(cvar));
    let Some(row) = row else {
        return false;
    };
    let number = value.parse::<f64>();
    match (row.kind, number) {
        (ValueKind::Choice(values), _) => values.contains(&value),
        (ValueKind::Bool, Ok(number)) => number == 0.0 || number == 1.0,
        (ValueKind::Integer { min, max, .. }, Ok(number)) => {
            number.fract() == 0.0 && (min as f64..=max as f64).contains(&number)
        }
        (ValueKind::Float { min, max, .. }, Ok(number)) => (min..=max).contains(&number),
        _ => false,
    }
}

/// Settings of general tab `tab`.
fn settings(tab: usize) -> &'static [Setting] {
    match tab {
        0 => VIDEO,
        1 => AUDIO,
        2 => HUD,
        3 => CONTROLS,
        4 => GAME,
        5 => NETWORK,
        6 => HUD_OPTIONS,
        7 => TEXT,
        QUICK_TAB => quick::rows(),
        _ => &[],
    }
}
/// A negative minimum on an integer row is one special value below the range,
/// shown as AUTO (`com_maxfps -1`). Stepping moves between it and zero, then
/// along the row's step.
/// A switch row whose state other cvars hold, not a cvar of its own (its name is
/// the row's stand-in): reading and flipping it go through these.
#[derive(Clone, Copy)]
pub(super) struct DerivedSwitch {
    /// Whether the switch shows on.
    pub(super) on: fn(&ViewerConsole) -> bool,
    /// Flip it.
    pub(super) toggle: fn(&mut ViewerConsole),
}

/// A choice row whose value other cvars hold, not a cvar of its own (its name
/// is the row's stand-in), like [`DerivedSwitch`].
#[derive(Clone, Copy)]
pub(super) struct DerivedChoice {
    /// The choice in use, one of the row's values.
    pub(super) get: fn(&ViewerConsole) -> &'static str,
    /// Make a choice.
    pub(super) set: fn(&mut ViewerConsole, &str),
    /// The choice of a new profile, which Backspace returns to.
    pub(super) default: &'static str,
}

/// The derived choice row `cvar` names, if it is one: the renderer.
pub(super) fn derived_choice(cvar: &str) -> Option<DerivedChoice> {
    match cvar {
        crate::graphics_quality::RENDERER_ROW => Some(DerivedChoice {
            get: crate::graphics_quality::renderer_value,
            set: crate::graphics_quality::set_renderer,
            default: crate::graphics_quality::VANILLA,
        }),
        _ => None,
    }
}

/// The value choice row `cvar` holds: its cvar's text, or a derived choice's.
fn choice_value(console: &ViewerConsole, cvar: &str) -> Option<String> {
    match derived_choice(cvar) {
        Some(choice) => Some((choice.get)(console).to_owned()),
        None => console.cvar(cvar).map(CvarValue::as_text),
    }
}

/// Make choice `value` of row `cvar`: set its cvar, or a derived choice.
fn set_choice(console: &mut ViewerConsole, cvar: &str, value: &str) {
    match derived_choice(cvar) {
        Some(choice) => (choice.set)(console, value),
        None => {
            console.set_cvar(cvar, value);
        }
    }
}

/// Whether `cvar` is one of the renderer's settings: the renderer and its
/// quality, and every row of Graphics' image, lighting, shadows and weather.
fn renderer_setting(cvar: &str) -> bool {
    [
        crate::graphics_quality::RENDERER_ROW,
        crate::graphics_quality::ROW_NAME,
    ]
    .iter()
    .any(|row| row.eq_ignore_ascii_case(cvar))
        || [
            RENDER_IMAGE,
            RENDER_LIGHTING,
            RENDER_SHADOWS,
            RENDER_WEATHER,
        ]
        .iter()
        .flat_map(|tab| tab.iter())
        .any(|setting| setting.cvar.eq_ignore_ascii_case(cvar))
}

/// The derived switch row `cvar` names, if it is one: the strafe lines.
pub(super) fn derived_switch(cvar: &str) -> Option<DerivedSwitch> {
    match cvar {
        crate::hud::movement::LINES_ROW => Some(DerivedSwitch {
            on: crate::hud::movement::lines_on,
            toggle: crate::hud::movement::toggle_lines,
        }),
        _ => None,
    }
}

fn step_integer(value: i64, direction: i32, min: i64, max: i64, step: i64) -> i64 {
    if min < 0 {
        if value < 0 {
            return if direction > 0 { 0 } else { min };
        }
        if value == 0 && direction < 0 {
            return min;
        }
        return (value + i64::from(direction) * step).clamp(0, max);
    }
    (value + i64::from(direction) * step).clamp(min, max)
}

/// The value a row shows: AUTO for the one special value below a negative
/// minimum (`com_maxfps -1`), otherwise the cvar's text.
fn row_text(console: &ViewerConsole, setting: &Setting) -> String {
    if let Some(choice) = derived_choice(setting.cvar) {
        return (choice.get)(console).to_owned();
    }
    match (setting.kind, console.cvar(setting.cvar)) {
        (ValueKind::Integer { min, .. }, Some(CvarValue::Integer(value)))
            if min < 0 && *value < 0 =>
        {
            "AUTO".to_owned()
        }
        (ValueKind::Float { .. }, Some(CvarValue::Float(value))) => float_text(*value),
        _ => value_text(console, setting.cvar),
    }
}

/// A slider's number to four decimals at most, keeping one (`0.9`, not
/// `0.8999999761581421` from a single-precision default; `100.0`).
fn float_text(value: f64) -> String {
    let mut text = format!("{value:.4}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text
}

/// What the "SJK identity key" row shows: never the key itself.
const IDENTITY_TEXT: &str = "Hidden: open to see";
const IMPORT_TEXT: &str = "Base, EJK, TaystJK...";

/// What the "Quick wheel pages" row shows: how many pages, and whether they
/// are the defaults.
fn wheel_pages_text(console: &ViewerConsole) -> String {
    let pages = &console.wheel_pages;
    match (pages.pages().len(), pages.is_default()) {
        (_, true) => "Force, General, Toys, Weather".to_owned(),
        (1, false) => "1 page".to_owned(),
        (count, false) => format!("{count} pages"),
    }
}

/// Whether a switch row's cvar is on: true, or any nonzero number.
fn switch_on(value: &CvarValue) -> bool {
    match value {
        CvarValue::Bool(value) => *value,
        CvarValue::Integer(value) => *value != 0,
        CvarValue::Float(value) => *value != 0.0,
        CvarValue::Text(value) => value.trim().parse::<f64>().is_ok_and(|value| value != 0.0),
    }
}

/// ON/OFF for a toggle row; a numeric cvar is on when nonzero.
fn toggle_text(console: &ViewerConsole, name: &str) -> String {
    match console.cvar(name) {
        Some(value @ (CvarValue::Integer(_) | CvarValue::Float(_))) => {
            if switch_on(value) { "ON" } else { "OFF" }.to_owned()
        }
        _ => value_text(console, name),
    }
}

fn value_text(console: &ViewerConsole, name: &str) -> String {
    console.cvar(name).map_or_else(
        || "?".to_owned(),
        |value| match value {
            CvarValue::Bool(v) => {
                if *v {
                    "ON".to_owned()
                } else {
                    "OFF".to_owned()
                }
            }
            _ => value.as_text(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f3_and_ctrl_f_find_a_setting_and_f2_switches_the_sjk_uis_mode() {
        let (_directory, console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_classic_group(
            &console,
            Group::Hud,
            crate::menu::classic::panel::Frame::Main,
        );
        for (key, control) in [(KeyCode::F3, false), (KeyCode::KeyF, true)] {
            menu.searching = false;
            menu.search = "fov".to_owned();
            assert!(menu.screen_key(key, control).is_some());
            assert!(menu.searching, "{key:?}");
            // What was typed is selected: typing replaces it.
            assert!(crate::text_entry::is_selected(&menu.search));
        }
        menu.searching = false;
        // F alone is a row's key; F2 only on the SJK UI's screen.
        assert!(menu.screen_key(KeyCode::KeyF, false).is_none());
        assert!(menu.screen_key(KeyCode::F2, false).is_none());
        menu.set_screen(Some(mode::SettingsMode::Simple), "HUD");
        assert!(matches!(
            menu.screen_key(KeyCode::F2, false),
            Some(SettingsResult::Classic(
                crate::menu::sjk::settings::MODE_TOGGLE_SLOT
            ))
        ));
    }

    #[test]
    fn slider_numbers_lose_float_noise() {
        assert_eq!(float_text(f64::from(0.9_f32)), "0.9");
        assert_eq!(float_text(100.0), "100.0");
        assert_eq!(float_text(0.005), "0.005");
        assert_eq!(float_text(-2.5), "-2.5");
    }

    const SECTIONS: [(Section, usize); 2] = [
        (Section::General, TABS.len()),
        (Section::Renderer, RENDERER_TABS.len()),
    ];

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn rows() -> impl Iterator<Item = &'static Setting> {
        SECTIONS.into_iter().flat_map(|(section, tabs)| {
            (0..tabs).flat_map(move |tab| section_settings(section, tab))
        })
    }

    #[test]
    fn the_sjk_chat_setting_defaults_on_beside_the_hub() {
        let (_directory, console) = console();
        assert_eq!(console.bool_cvar("cl_sjkChat"), Some(true));
        let network: Vec<&str> = catalog::NETWORK.iter().map(|row| row.cvar).collect();
        let hub = network
            .iter()
            .position(|cvar| *cvar == "cl_hubUrl")
            .unwrap();
        assert_eq!(network.get(hub + 1), Some(&"cl_sjkChat"));
        // Its sound right under it, on by default.
        assert_eq!(
            network.get(hub + 2),
            Some(&crate::sjk_chat_frame::SOUND_CVAR)
        );
        assert_eq!(
            console.bool_cvar(crate::sjk_chat_frame::SOUND_CVAR),
            Some(true)
        );
        // Its GIFs under the sound, on by default.
        assert_eq!(network.get(hub + 3), Some(&crate::chat_gifs::CVAR));
        assert_eq!(console.bool_cvar(crate::chat_gifs::CVAR), Some(true));
    }

    #[test]
    fn every_row_names_a_registered_cvar_of_its_kind() {
        let (_directory, console) = console();
        for setting in rows() {
            // Rows whose value is not one cvar's (resolution, display mode, HUD,
            // the quick wheel's pages, graphics quality and the renderer).
            if derived_switch(setting.cvar).is_some() || derived_choice(setting.cvar).is_some() {
                continue;
            }
            if matches!(
                setting.kind,
                ValueKind::Resolution
                    | ValueKind::DisplayMode
                    | ValueKind::HudPicker
                    | ValueKind::WheelPages
                    | ValueKind::IdentityPage
                    | ValueKind::ImportPage
                    | ValueKind::AssetBrowser
                    | ValueKind::Quality
            ) {
                continue;
            }
            let value = console
                .cvar(setting.cvar)
                .unwrap_or_else(|| panic!("{} is not registered", setting.cvar));
            let fits = match (setting.kind, value) {
                (
                    ValueKind::Bool,
                    CvarValue::Bool(_) | CvarValue::Integer(_) | CvarValue::Float(_),
                ) => true,
                // A negative minimum is the one special value below the range (AUTO).
                (ValueKind::Integer { min, max, .. }, CvarValue::Integer(value)) => {
                    (min.min(0)..=max).contains(value) || (min < 0 && *value == min)
                }
                (ValueKind::Float { min, max, .. }, CvarValue::Float(value)) => {
                    (min..=max).contains(value)
                }
                // Choices cycle the cvar's text form, whatever its type.
                (ValueKind::Choice(values), value) => values.contains(&value.as_text().as_str()),
                (ValueKind::Text, CvarValue::Text(_)) => true,
                (ValueKind::GameplayFps, CvarValue::Integer(_)) => true,
                _ => false,
            };
            assert!(
                fits,
                "{} default {value:?} does not fit its row",
                setting.cvar
            );
        }
    }

    #[test]
    fn the_renderer_tabs_hold_every_engine_rendering_cvar() {
        // Bookkeeping, diagnostics, the ground HUD (on the HUD tab) and exclusive
        // fullscreen (the Video tab's display-mode row) are not listed.
        const RENDERER: [&str; 27] = [
            "r_sceneHdr",
            "r_hdrExposure",
            "r_toneCurve",
            "r_sceneBloom",
            "r_fxaa",
            "r_superSample",
            "r_softParticles",
            "r_modelPixelLight",
            "r_actorSunShadows",
            "r_worldSunShadows",
            "r_sunShadowDistance",
            "r_sunShadowNear",
            "r_sunShadowResolution",
            "r_sunShadowTaps",
            "r_sunShadowGapClose",
            "r_contactShadows",
            "r_volumetrics",
            "r_volumetricClarity",
            "r_dayNight",
            "r_dayHour",
            "r_dayMinutes",
            "r_dayBrightness",
            "r_ambientFill",
            "r_ambientFillOcclusion",
            "r_indirectBoost",
            "r_liveLighting",
            "r_dustMotes",
        ];
        let (_directory, console) = console();
        let renderer: Vec<_> = (0..RENDERER_TABS.len())
            .flat_map(|tab| section_settings(Section::Renderer, tab))
            .map(|setting| setting.cvar)
            .collect();
        for name in RENDERER {
            assert!(console.cvar(name).is_some(), "{name} is not registered");
            assert!(
                renderer.contains(&name),
                "{name} is missing from the renderer tabs"
            );
        }
        for tab in 0..TABS.len() {
            for setting in settings(tab) {
                assert!(
                    !renderer.contains(&setting.cvar),
                    "{} is on both pages",
                    setting.cvar
                );
            }
        }
    }

    #[test]
    fn the_renderer_settings_open_on_their_first_tab_and_back_out_of_the_screen() {
        let (_directory, console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_renderer(&console);
        assert_eq!(
            (menu.section, menu.tab, menu.selected),
            (Section::Renderer, 0, 0)
        );
        assert_eq!(menu.tabs(), &RENDERER_TABS);
        assert_eq!(menu.values.len(), RENDER_IMAGE.len());
        menu.select_tab(&console, RENDERER_TABS.len());
        assert_eq!(menu.tab, 0, "renderer tabs wrap within their own section");
        assert!(matches!(menu.back(), SettingsResult::Back));
        // A later ordinary open is the general screen again.
        menu.open_tab(&console, RENDERER_TAB);
        assert_eq!(menu.section, Section::General);
    }

    #[test]
    fn switch_rows_toggle_integer_and_float_cvars() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_renderer(&console);
        for (tab, cvar) in [(0, "r_sceneBloom"), (2, "r_contactShadows")] {
            menu.select_tab(&console, tab);
            menu.selected = menu
                .rows()
                .iter()
                .position(|setting| setting.cvar == cvar)
                .unwrap();
            let before = switch_on(console.cvar(cvar).unwrap());
            menu.adjust(&mut console, 1);
            assert_eq!(switch_on(console.cvar(cvar).unwrap()), !before, "{cvar}");
            assert_eq!(
                menu.values[menu.selected],
                if before { "OFF" } else { "ON" }
            );
            menu.adjust(&mut console, 1);
            assert_eq!(switch_on(console.cvar(cvar).unwrap()), before, "{cvar}");
        }
    }

    #[test]
    fn auto_sits_below_zero_and_steps_join_the_grid() {
        let step = |value, direction| step_integer(value, direction, -1, 2000, 25);
        assert_eq!(step(-1, 1), 0);
        assert_eq!(step(-1, -1), -1);
        assert_eq!(step(0, -1), -1);
        assert_eq!(step(0, 1), 25);
        assert_eq!(step(144, -1), 119);
        assert_eq!(step(1990, 1), 2000);
    }

    #[test]
    fn rows_without_a_special_value_step_as_before() {
        assert_eq!(step_integer(80, 1, 80, 130, 5), 85);
        assert_eq!(step_integer(80, -1, 80, 130, 5), 80);
    }

    #[test]
    fn a_fresh_profile_takes_sols_visual_defaults_and_a_saved_one_keeps_its_values() {
        use crate::world_materials::shadows::day;
        let (_directory, console) = console();
        for cvar in [
            "r_sceneBloom",
            "r_normalMapping",
            "r_specularMapping",
            "r_parallaxMapping",
        ] {
            assert_eq!(console.integer_cvar(cvar), Some(1), "{cvar}");
        }
        assert_eq!(console.float_cvar("r_parallaxStrength"), Some(0.1));
        assert_eq!(console.float_cvar("r_dayHour"), Some(12.0));
        assert_eq!(console.float_cvar(crate::dust_motes::CVAR), Some(1.0));
        // The live state the renderer reads starts from the same defaults.
        assert!(console.post_color.policy().bloom);
        assert_eq!(console.dust_motes.intensity(), 1.0);
        assert_eq!(day::Settings::sample(Some(&console)).hour, 12.0);
        assert_eq!(day::live::Clock::default().values()[0], 12.0);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        std::fs::write(
            &path,
            "seta r_dayHour \"11\"\nseta r_sceneBloom \"0\"\nseta r_dustMotes \"0\"\n\
             seta r_normalMapping \"0\"\n",
        )
        .unwrap();
        let console = ViewerConsole::new(path).unwrap();
        assert_eq!(console.float_cvar("r_dayHour"), Some(11.0));
        assert_eq!(console.integer_cvar("r_sceneBloom"), Some(0));
        assert!(!console.post_color.policy().bloom);
        assert_eq!(console.dust_motes.intensity(), 0.0);
        assert_eq!(console.integer_cvar("r_normalMapping"), Some(0));
    }
}

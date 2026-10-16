//! The settings rows as a classic option panel (`setup.menu` items): the
//! label set against the label column, the value after it.
//!
//! The panel is classic+ (`docs/classic-plus.md`). Labels are in sentence
//! case, values brighter than them; an on/off setting is a switch, a number a
//! slim slider with its value in a small frame, a choice of up to
//! [`MAX_SEGMENTS`] values a row of segments, and a longer list (or the display
//! mode, resolution and HUD) a field whose click opens it; long groups are
//! divided by sub-headings. The detail box under the rows says what the
//! focused setting does ([`super::help`]), its value, default and range, when
//! a change applies and its console name, under its group's icon. A row
//! changed from its default carries a gold dot, and the focused one a reset
//! arrow; a setting that applies after a restart, a graphics reload or on the
//! next map a `*`.
//! Backspace, the arrow or the right button returns the focused setting to
//! its default, and the description line names the keys of the focused row.
//! The renderer settings show this way too, as the groups of the classic
//! renderer page.

use super::help::{self, Timing};
use super::*;
use crate::graphics_quality::Level;
use crate::menu::classic::layout::Span;
use crate::menu::classic::panel::{Detail, OPTION, PanelFrame, VALUE};
use crate::menu::classic::view::{FOCUS, Sentence};
use std::fmt::Write as _;
use winit::keyboard::KeyCode;

/// The rows' wheel target over the panel and the scrollbar beside it, clear
/// of the row (0-499), tab (500), slider value (700), chrome (800) and back
/// (900) tokens.
pub(super) const CLASSIC_SCROLL_TOKEN: u16 = 911;
pub(super) const CLASSIC_SCROLLBAR_TOKEN: u16 = 912;
/// Choice `i` of an open dropdown answers to `DROPDOWN_BASE + i`.
pub(super) const DROPDOWN_BASE: u16 = 1_000;
/// Rows one wheel notch scrolls, as the settings form steps.
pub(super) const CLASSIC_WHEEL_ROWS: i32 = 1;
/// Segment `i` of row `row`'s segmented choice answers to
/// `SEGMENT_BASE + row * MAX_SEGMENTS + i`, and row `row`'s reset button to
/// `RESET_BASE + row`; both clear of every other panel token.
const SEGMENT_BASE: u16 = 2_000;
const RESET_BASE: u16 = 6_000;
/// Most choices a row shows side by side; longer lists open a dropdown.
pub(super) const MAX_SEGMENTS: usize = 3;

/// What a classic+ panel token beyond the rows names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RowControl {
    /// Choice `index` of row `row`'s segments.
    Segment { row: usize, index: usize },
    /// Row `row`'s reset button.
    Reset(usize),
}

impl RowControl {
    pub(super) fn token(self) -> u16 {
        match self {
            Self::Segment { row, index } => SEGMENT_BASE + (row * MAX_SEGMENTS + index) as u16,
            Self::Reset(row) => RESET_BASE + row as u16,
        }
    }

    /// The control `token` names, if it names one.
    pub(super) fn of(token: u16) -> Option<Self> {
        match token {
            SEGMENT_BASE..RESET_BASE => {
                let offset = usize::from(token - SEGMENT_BASE);
                Some(Self::Segment {
                    row: offset / MAX_SEGMENTS,
                    index: offset % MAX_SEGMENTS,
                })
            }
            RESET_BASE.. => Some(Self::Reset(usize::from(token - RESET_BASE))),
            _ => None,
        }
    }

    /// The setting row the control belongs to.
    pub(super) fn row(self) -> usize {
        match self {
            Self::Segment { row, .. } | Self::Reset(row) => row,
        }
    }
}

/// The choices row `kind` shows side by side, when it has few enough.
pub(super) fn segment_choices(kind: ValueKind) -> Option<&'static [&'static str]> {
    match kind {
        ValueKind::Choice(values) if values.len() <= MAX_SEGMENTS => Some(values),
        _ => None,
    }
}

/// What the classic+ panel knows about a row beyond its value: its default
/// as the row would show it, and whether the value differs from it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RowDefault {
    pub(super) text: Option<String>,
    pub(super) changed: bool,
}

impl SettingsMenu {
    /// Show rows `span` of tab `tab` as a classic option panel drawn in
    /// `frame`'s geometry.
    pub(crate) fn open_classic(
        &mut self,
        console: &ViewerConsole,
        tab: usize,
        span: Span,
        frame: crate::menu::classic::panel::Frame,
    ) {
        self.open_tab(console, tab);
        let rows = span.within(0, self.rows().len());
        self.selected = rows.start;
        self.clear_search();
        self.classic = Some(ClassicRows {
            lines: ClassicRows::span(rows),
            slider_span: frame.slider_span(),
            first: 0,
            visible: frame.capacity(),
            pinned: None,
        });
    }

    /// Show every row of renderer tab `tab` (IMAGE, LIGHTING, SHADOWS) as a
    /// classic option panel; backing out leaves the screen, to the classic
    /// renderer page's owner.
    pub(crate) fn open_classic_renderer(
        &mut self,
        console: &ViewerConsole,
        tab: usize,
        frame: crate::menu::classic::panel::Frame,
    ) {
        self.open_renderer(console);
        self.select_tab(console, tab);
        self.clear_search();
        self.classic = Some(ClassicRows {
            lines: ClassicRows::span(0..self.rows().len()),
            slider_span: frame.slider_span(),
            first: 0,
            visible: frame.capacity(),
            pinned: None,
        });
    }

    /// Show classic Setup group `group`'s rows as a classic option panel.
    pub(crate) fn open_classic_group(
        &mut self,
        console: &ViewerConsole,
        group: Group,
        frame: crate::menu::classic::panel::Frame,
    ) {
        self.open_tab(console, 0);
        self.section = Section::Group(group);
        self.tab = 0;
        self.refresh(console);
        self.selected = 0;
        self.clear_search();
        self.classic = Some(ClassicRows {
            lines: group.lines(),
            slider_span: frame.slider_span(),
            first: 0,
            visible: frame.capacity(),
            pinned: None,
        });
    }

    /// Show First setup as the SJK UI's pop-up: its rows, but for "Don't show
    /// at start", which is pinned at the pop-up's foot, in view however far the
    /// rows scroll.
    pub(crate) fn open_first_setup_popup(&mut self, console: &ViewerConsole) {
        self.open_classic_group(
            console,
            Group::Quick,
            crate::menu::classic::panel::Frame::Main,
        );
        let hide = self
            .rows()
            .iter()
            .position(|setting| setting.cvar == super::quick::HIDE_CVAR);
        if let (Some(classic), Some(hide)) = (&mut self.classic, hide) {
            classic.lines.retain(|line| *line != Line::Row(hide));
            classic.pinned = Some(hide);
        }
    }

    /// Whether First setup's pop-up is on show (its rows pin one at its foot).
    pub(crate) fn popup(&self) -> bool {
        self.classic
            .as_ref()
            .is_some_and(|classic| classic.pinned.is_some())
    }

    /// Back to the full tabbed screen, keeping the tab.
    pub(crate) fn leave_classic(&mut self) {
        if self.classic.take().is_some() {
            self.editing = None;
            self.numeric = None;
            self.dropdown = None;
            self.searching = false;
        }
    }

    /// What is typed in the search field.
    pub(crate) fn search_text(&self) -> &str {
        &self.search
    }

    /// How many options match `text`, for the other tab's search.
    pub(crate) fn count_matches(text: &str) -> usize {
        if text.trim().is_empty() {
            0
        } else {
            search::count(text)
        }
    }

    /// Key bindings that match the search, for the panel to offer.
    pub(crate) fn set_elsewhere(&mut self, bindings: usize) {
        self.elsewhere = bindings;
    }

    /// Carry a search typed on the other tab over to this panel.
    pub(crate) fn carry_search(&mut self, console: &ViewerConsole, text: &str) {
        if self.classic.is_some() && !text.trim().is_empty() {
            self.set_search(console, text.to_owned());
        }
    }

    /// Search for `text` as if it had been typed, for the menu snapshots.
    #[cfg(test)]
    pub(crate) fn search_for_snapshot(&mut self, console: &ViewerConsole, text: &str) {
        self.set_search(console, text.to_owned());
    }

    /// Open the selected row's dropdown, for the menu snapshots.
    #[cfg(test)]
    pub(crate) fn dropdown_for_snapshot(&mut self, console: &ViewerConsole) {
        let row = self.selected;
        assert!(self.open_dropdown(console, row), "no dropdown on row {row}");
        if let Some(dropdown) = &mut self.dropdown {
            dropdown.highlighted = (dropdown.current + 1) % dropdown.picks.len();
        }
    }

    /// Forget the search: a group opened from the list shows its own rows.
    fn clear_search(&mut self) {
        self.search.clear();
        self.searching = false;
        self.search_return = None;
        self.dropdown = None;
    }

    /// A key while the search field has the keyboard: typing searches every
    /// option; Enter, Down or Tab go to the results; Escape clears the text,
    /// then leaves the field.
    pub(super) fn search_key(&mut self, key: KeyCode, text: Option<&str>, console: &ViewerConsole) {
        let mut search = self.search.clone();
        match key {
            KeyCode::Escape if search.is_empty() => {
                self.searching = false;
                return;
            }
            KeyCode::Escape => search.clear(),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::ArrowDown | KeyCode::Tab => {
                self.searching = false;
                return;
            }
            KeyCode::Backspace => {
                search.pop();
            }
            _ => match text {
                Some(text) => search.extend(text.chars().filter(|c| !c.is_control()).take(32)),
                None => return,
            },
        }
        self.set_search(console, search);
    }

    /// Search every option for `search`: the panel shows the results under
    /// their groups' headings; an empty search returns to the group the
    /// panel showed before.
    pub(super) fn set_search(&mut self, console: &ViewerConsole, search: String) {
        if search == self.search && !search.is_empty() {
            return;
        }
        self.search_moves(&search);
        self.search = search;
        self.dropdown = None;
        self.editing = None;
        self.numeric = None;
        let Some(classic) = &mut self.classic else {
            return;
        };
        if self.search.trim().is_empty() {
            if let Some(back) = self.search_return.take() {
                self.section = back.section;
                self.tab = back.tab;
                classic.lines = back.lines;
                classic.first = back.first;
                self.selected = back.selected;
                self.refresh(console);
            }
            return;
        }
        if self.search_return.is_none() {
            self.search_return = Some(SearchReturn {
                section: self.section,
                tab: self.tab,
                lines: std::mem::take(&mut classic.lines),
                selected: self.selected,
                first: classic.first,
            });
        }
        self.section = Section::Search;
        self.tab = 0;
        let Some(classic) = &mut self.classic else {
            return;
        };
        classic.lines = search::lines(&self.search);
        classic.first = 0;
        // The results from the top, the first one selected.
        self.selected = classic.rows().next().unwrap_or(0);
        self.refresh(console);
    }

    /// Open the dropdown of row `row` when it is a choice of more than
    /// [`MAX_SEGMENTS`] values or the display mode; false for other rows (a
    /// switch flips and a segmented choice steps instead).
    pub(super) fn open_dropdown(&mut self, console: &ViewerConsole, row: usize) -> bool {
        let Some(setting) = self.rows().get(row).copied() else {
            return false;
        };
        if segment_choices(setting.kind).is_some() {
            return false;
        }
        let picks: Vec<Pick> = match setting.kind {
            ValueKind::Choice(values) => values.iter().map(|value| Pick::Value(value)).collect(),
            ValueKind::DisplayMode => {
                let exclusive = self.exclusive_available();
                DisplayMode::ALL
                    .into_iter()
                    .filter(|mode| exclusive || *mode != DisplayMode::Exclusive)
                    .map(Pick::Mode)
                    .collect()
            }
            ValueKind::Quality => Level::SJK.into_iter().map(Pick::Quality).collect(),
            _ => return false,
        };
        let current = match setting.kind {
            ValueKind::Choice(values) => {
                let value = super::choice_value(console, setting.cvar);
                values
                    .iter()
                    .position(|candidate| Some(*candidate) == value.as_deref())
                    .unwrap_or(0)
            }
            ValueKind::Quality => {
                let level = Level::nearest(console);
                picks
                    .iter()
                    .position(|pick| *pick == Pick::Quality(level))
                    .unwrap_or(0)
            }
            _ => {
                let mode = DisplayMode::requested(console).effective(self.exclusive_available());
                picks
                    .iter()
                    .position(|pick| *pick == Pick::Mode(mode))
                    .unwrap_or(0)
            }
        };
        self.selected = row;
        self.dropdown = Some(Dropdown {
            row,
            picks,
            highlighted: current,
            current,
        });
        true
    }

    /// A key while a dropdown is open: Up and Down choose, Enter or Space
    /// apply, Escape (or Backspace) closes it with nothing changed.
    pub(super) fn dropdown_key(&mut self, key: KeyCode, console: &mut ViewerConsole) {
        let Some(dropdown) = &mut self.dropdown else {
            return;
        };
        let count = dropdown.picks.len();
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => {
                dropdown.highlighted = (dropdown.highlighted + count - 1) % count;
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                dropdown.highlighted = (dropdown.highlighted + 1) % count;
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                let pick = dropdown.highlighted;
                self.apply_pick(console, pick);
            }
            KeyCode::Escape | KeyCode::Backspace => self.dropdown = None,
            _ => {}
        }
    }

    /// Apply choice `index` of the open dropdown and close it.
    pub(super) fn apply_pick(&mut self, console: &mut ViewerConsole, index: usize) {
        let Some(dropdown) = self.dropdown.take() else {
            return;
        };
        let (Some(pick), Some(setting)) = (
            dropdown.picks.get(index).copied(),
            self.rows().get(dropdown.row).copied(),
        ) else {
            return;
        };
        match pick {
            Pick::Value(value) => super::set_choice(console, setting.cvar, value),
            Pick::Mode(mode) => mode.store(console),
            Pick::Quality(level) => level.apply(console),
        }
        self.refresh(console);
    }

    /// The label of dropdown choice `pick` of setting `cvar` as the row would
    /// show it.
    pub(super) fn pick_label(cvar: &str, pick: Pick) -> &'static str {
        match pick {
            Pick::Value(value) => choice_label(cvar, value),
            Pick::Mode(mode) => mode.label(),
            Pick::Quality(level) => level.label(),
        }
    }

    /// Act on a row's segment or reset button.
    pub(super) fn activate_control(&mut self, console: &mut ViewerConsole, control: RowControl) {
        let row = control.row();
        if !self.shows(row) {
            return;
        }
        self.selected = row;
        match control {
            RowControl::Segment { index, .. } => {
                let Some(setting) = self.rows().get(row).copied() else {
                    return;
                };
                if let Some(value) =
                    segment_choices(setting.kind).and_then(|values| values.get(index))
                {
                    super::set_choice(console, setting.cvar, value);
                    self.refresh(console);
                }
            }
            RowControl::Reset(_) => self.reset_to_default(console, row),
        }
    }

    /// Return row `row` to its default value. Rows whose value is not one
    /// cvar's (resolution, display mode) are left alone; SJK quality's default
    /// is High, the SJK renderer's own values, and Vanilla keeps it off; the
    /// renderer's is Vanilla, a new profile's, and under Vanilla a renderer
    /// setting's is Vanilla's value ([`crate::graphics_quality::row_default`]).
    pub(super) fn reset_to_default(&mut self, console: &mut ViewerConsole, row: usize) {
        let Some(setting) = self.rows().get(row) else {
            return;
        };
        if matches!(setting.kind, ValueKind::Resolution | ValueKind::DisplayMode) {
            return;
        }
        if matches!(setting.kind, ValueKind::Quality) {
            if !crate::graphics_quality::vanilla(console) {
                Level::High.apply(console);
            }
        } else if let Some(choice) = super::derived_choice(setting.cvar) {
            (choice.set)(console, choice.default);
        } else if let Some(default) = crate::graphics_quality::row_default(console, setting.cvar) {
            console.set_cvar(setting.cvar, default);
        } else if let Some(switch) = super::derived_switch(setting.cvar) {
            if (switch.on)(console) {
                (switch.toggle)(console);
            }
        } else if let Some(default) = console.cvar_default(setting.cvar).map(CvarValue::as_text) {
            console.set_cvar(setting.cvar, &default);
        }
        self.editing = None;
        self.numeric = None;
        self.refresh(console);
    }

    /// Draw the classic panel screen around the rows of the open span.
    pub(crate) fn append_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
        frame: &PanelFrame,
    ) {
        self.sjk_controls = None;
        if self.wheel.is_open() {
            self.append_wheel_overlay(vertices, font, viewport, reveal);
            return;
        }
        if self.hud.is_open() {
            self.append_hud_picker(vertices, font, viewport, reveal, Some(frame.art));
            return;
        }
        // The resolution list is the SJK UI's card in every style.
        if self.picker.is_open() {
            let target = crate::menu::sjk::TextTarget::Inter(vertices, font);
            self.append_resolutions(target, viewport, reveal);
            return;
        }
        let place = frame.begin(&mut self.ui, viewport, reveal);
        let found = (!self.search.is_empty()).then(|| {
            self.classic
                .as_ref()
                .map_or(0, |classic| classic.rows().count())
        });
        place.search_field(
            &mut self.ui,
            &self.search,
            self.searching,
            "type to find",
            found,
        );
        let visible = place.capacity();
        let lines: Vec<Line> = match &mut self.classic {
            Some(classic) => {
                classic.visible = visible;
                classic.first = classic.first.min(classic.max_first());
                classic.lines.clone()
            }
            None => Vec::new(),
        };
        let first = self.classic.as_ref().map_or(0, |classic| classic.first);
        let shown_lines = first..lines.len().min(first + visible);
        // A list longer than the panel scrolls with the wheel anywhere over
        // its rows and shows a scrollbar, as the classic key bindings do.
        if lines.len() > visible {
            let top = place.row(0);
            self.ui.scroll_region(
                CLASSIC_SCROLL_TOKEN,
                Rect::new(top.x, top.y, top.width, visible as f32 * top.height),
            );
        }
        if lines.is_empty() && found.is_some() {
            place.value_plain(&mut self.ui, 0, "No option matches the search.", OPTION);
            if self.elsewhere > 0 {
                place.value_fmt(
                    &mut self.ui,
                    1,
                    format_args!("{} on KEY BINDINGS: click its tab.", self.elsewhere),
                    OPTION,
                );
            }
        }
        if let Some(slot) = lines[shown_lines.clone()]
            .iter()
            .position(|line| *line == Line::Row(self.selected))
        {
            place.highlight(&mut self.ui, slot);
        }
        // Slot of each shown row, for the thumbs and the dropdown.
        let mut slots: Vec<(usize, usize)> = Vec::with_capacity(visible);
        // Rows under an open dropdown leave their values out (text draws over
        // every shape, so the list could not hide them).
        let dropdown_slot = self.dropdown.as_ref().and_then(|dropdown| {
            let slot = lines[shown_lines.clone()]
                .iter()
                .position(|line| *line == Line::Row(dropdown.row))?;
            Some((slot, dropdown.picks.len()))
        });
        let covered = |slot: usize| {
            dropdown_slot.is_some_and(|(open, count)| place.dropdown_covers(open, count, slot))
        };
        for (slot, line) in lines[shown_lines.clone()].iter().enumerate() {
            let row = match *line {
                Line::Heading(text) => {
                    place.heading(&mut self.ui, slot, text);
                    continue;
                }
                Line::Row(row) => row,
            };
            slots.push((row, slot));
            let Some(setting) = self.rows().get(row) else {
                continue;
            };
            let focused = row == self.selected;
            // Labels in the option blue, values brighter; the focused row's
            // label in steady white over its band.
            let label_color = if focused { FOCUS } else { OPTION };
            let value_color = if focused { FOCUS } else { VALUE };
            let (label, timing) = help::classic_label(setting.cvar, setting.label);
            place.label_marked(
                &mut self.ui,
                slot,
                label,
                label_color,
                timing != Timing::Now,
            );
            let changed = self
                .defaults
                .get(row)
                .is_some_and(|default| default.changed);
            if changed {
                place.changed_mark(&mut self.ui, slot);
            }
            self.ui.hit_region(row as u16, place.row(slot));
            if covered(slot) {
                continue;
            }
            let value = self.values.get(row).map_or("?", String::as_str);
            match setting.kind {
                ValueKind::Bool => {
                    place.switch(&mut self.ui, slot, switch_text_on(value), focused);
                }
                ValueKind::Integer { .. } | ValueKind::Float { .. } => {
                    place.draw_slider_bar(&mut self.ui, slot);
                    let editing = self.numeric.as_ref().filter(|edit| edit.row == row);
                    let target = place.slider_value_rect(slot);
                    self.ui.hit_region(
                        crate::menu_widgets::numeric::VALUE_BASE + row as u16,
                        target,
                    );
                    match editing {
                        Some(edit) => edit.draw(&mut self.ui, target, place.scale()),
                        None => place.slider_value(&mut self.ui, slot, value, value_color, focused),
                    }
                }
                ValueKind::Choice(values) if values.len() <= MAX_SEGMENTS => {
                    let current = values
                        .iter()
                        .position(|candidate| candidate.eq_ignore_ascii_case(value))
                        .unwrap_or(0);
                    let base = RowControl::Segment { row, index: 0 }.token();
                    let labels: [&str; MAX_SEGMENTS] = std::array::from_fn(|index| {
                        values
                            .get(index)
                            .map_or("", |v| choice_label(setting.cvar, v))
                    });
                    let labels = &labels[..values.len()];
                    place.segments(&mut self.ui, slot, labels, current, focused, base);
                }
                ValueKind::Choice(_)
                | ValueKind::Resolution
                | ValueKind::DisplayMode
                | ValueKind::HudPicker
                | ValueKind::WheelPages
                | ValueKind::IdentityPage
                | ValueKind::ImportPage
                | ValueKind::AssetBrowser
                | ValueKind::GameplayFps
                | ValueKind::Quality => {
                    let open = self.dropdown.as_ref().is_some_and(|open| open.row == row);
                    place.choice_field(
                        &mut self.ui,
                        slot,
                        format_args!("{}", Sentence(choice_label(setting.cvar, value))),
                        focused || open,
                        true,
                    );
                }
                ValueKind::Text => match self
                    .editing
                    .as_ref()
                    .filter(|draft| draft.row == row)
                    .map(|draft| draft.text.as_str())
                {
                    Some(buffer) => place.choice_field(
                        &mut self.ui,
                        slot,
                        format_args!("{buffer}_"),
                        true,
                        false,
                    ),
                    None => place.choice_field(
                        &mut self.ui,
                        slot,
                        format_args!("{value}"),
                        focused,
                        false,
                    ),
                },
            }
            // The focused row, changed from its default, offers it back.
            if focused && changed {
                place.reset_button(&mut self.ui, slot, RowControl::Reset(row).token());
            }
        }
        // Knobs after every rail.
        for &(row, slot) in &slots {
            let Some(setting) = self.rows().get(row) else {
                continue;
            };
            let value = self
                .values
                .get(row)
                .and_then(|value| value.parse::<f64>().ok());
            let ratio = match (setting.kind, value) {
                (ValueKind::Integer { min, max, .. }, Some(value)) if max > min => {
                    ((value - min as f64) / (max - min) as f64) as f32
                }
                (ValueKind::Float { min, max, .. }, Some(value)) if max > min => {
                    ((value - min) / (max - min)) as f32
                }
                (ValueKind::Integer { .. } | ValueKind::Float { .. }, _) => 0.0,
                _ => continue,
            };
            place.draw_slider_thumb(&mut self.ui, slot, ratio, row == self.selected);
        }
        if lines.len() > visible {
            self.ui.scrollbar(
                CLASSIC_SCROLLBAR_TOKEN,
                place.scrollbar_track(visible),
                first,
                visible,
                lines.len(),
            );
        }
        self.write_detail_facts();
        let badge = self.badge();
        let detail = detail_of(
            self.rows(),
            self.shows(self.selected).then_some(self.selected),
            &self.values,
            &self.detail_facts,
            badge,
        );
        place.detail(&mut self.ui, &detail);
        // The dropdown last, over the rows and the detail box, so it takes the
        // pointer.
        if let Some(dropdown) = &self.dropdown {
            match slots.iter().find(|(row, _)| *row == dropdown.row) {
                Some(&(_, slot)) => {
                    let cvar = self.rows().get(dropdown.row).map_or("", |row| row.cvar);
                    let labels: Vec<&str> = dropdown
                        .picks
                        .iter()
                        .map(|pick| Self::pick_label(cvar, *pick))
                        .collect();
                    place.dropdown(
                        &mut self.ui,
                        slot,
                        &labels,
                        dropdown.highlighted,
                        dropdown.current,
                        DROPDOWN_BASE,
                    );
                }
                None => self.dropdown = None,
            }
        }
        self.write_key_hint();
        place.finish(&mut self.ui, Some(&self.key_hint));
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// What the detail box says about the selected row.
    #[cfg(test)]
    fn detail(&self) -> Detail<'_> {
        detail_of(
            self.rows(),
            self.shows(self.selected).then_some(self.selected),
            &self.values,
            &self.detail_facts,
            self.badge(),
        )
    }

    /// The detail box's facts line for the selected row: its default, its
    /// range or choices, and when a change applies.
    fn write_detail_facts(&mut self) {
        let facts = &mut self.detail_facts;
        facts.clear();
        let Some(setting) = section_settings(self.section, self.tab).get(self.selected) else {
            return;
        };
        let part = |facts: &mut String, args: std::fmt::Arguments<'_>| {
            if !facts.is_empty() {
                facts.push_str("   \u{b7}   ");
            }
            let _ = facts.write_fmt(args);
        };
        if let Some(default) = self
            .defaults
            .get(self.selected)
            .and_then(|default| default.text.as_deref())
        {
            part(facts, format_args!("Default {default}"));
        }
        match setting.kind {
            ValueKind::Integer { min, max, .. } if min < 0 => {
                part(facts, format_args!("AUTO, 0 to {max}"));
            }
            ValueKind::Integer { min, max, .. } => part(facts, format_args!("{min} to {max}")),
            ValueKind::Float { min, max, .. } => {
                part(facts, format_args!("{} to {}", Number(min), Number(max)));
            }
            ValueKind::Choice(choices) => {
                part(facts, format_args!("{} choices", choices.len()));
            }
            _ => {}
        }
        if let Some(note) = help::timing(setting.label).1.note() {
            part(facts, format_args!("{note}"));
        }
        if self.alpha && super::renderer_setting(setting.cvar) {
            part(facts, format_args!("SJK renderer: alpha"));
        }
        if self.section == Section::Search
            && let Some(group) = search::group(self.selected)
        {
            part(facts, format_args!("In {group}"));
        }
        if self.section == Section::Group(Group::Quick) {
            part(
                facts,
                format_args!("New here? Start with the first row, or drop a .cfg"),
            );
        }
    }

    /// The description line's keys for the selected row.
    fn write_key_hint(&mut self) {
        self.key_hint.clear();
        let hint = &mut self.key_hint;
        if self.numeric.is_some() {
            hint.push_str("Type a value, then ENTER to set it or ESC to cancel.");
            return;
        }
        if self.editing.is_some() {
            hint.push_str("Type the new text, then ENTER to set it or ESC to cancel.");
            return;
        }
        if self.dropdown.is_some() {
            hint.push_str("UP or DOWN to choose, ENTER to apply, ESC to keep it as it is");
            return;
        }
        if self.searching {
            hint.push_str("Type to find any option   \u{b7}   ENTER to the results, ESC to clear");
            return;
        }
        if self.elsewhere > 0 && !self.search.trim().is_empty() {
            let _ = write!(
                hint,
                "{} key binding{} match too: click the KEY BINDINGS tab, the search goes with you",
                self.elsewhere,
                if self.elsewhere == 1 { "" } else { "s" }
            );
            return;
        }
        let Some(setting) = section_settings(self.section, self.tab).get(self.selected) else {
            return;
        };
        hint.push_str(match setting.kind {
            ValueKind::Bool => "ENTER or a click to switch it, LEFT or RIGHT too",
            kind if segment_choices(kind).is_some() => "Click a choice, or LEFT or RIGHT to step",
            ValueKind::Choice(_) | ValueKind::DisplayMode | ValueKind::Quality => {
                "ENTER or a click for the choices, LEFT or RIGHT to step"
            }
            ValueKind::Integer { .. } | ValueKind::Float { .. } => {
                "LEFT or RIGHT to change it, or type a number"
            }
            ValueKind::Text => "ENTER to type a new value",
            ValueKind::Resolution => "LEFT or RIGHT to step, ENTER for the list of sizes",
            ValueKind::HudPicker => "LEFT or RIGHT to step, ENTER to pick from pictures",
            ValueKind::WheelPages => "ENTER to edit the pages and their choices",
            ValueKind::IdentityPage => "ENTER to open your key's page; the key stays hidden",
            ValueKind::ImportPage => "ENTER to bring your setup from another client",
            ValueKind::GameplayFps => {
                "LEFT or RIGHT for 125, 142, 250 or 333, ENTER to explain it and type any value"
            }
            ValueKind::AssetBrowser => {
                "ENTER to enable or disable installed PK3 packs for the next start"
            }
        });
        if self
            .defaults
            .get(self.selected)
            .is_some_and(|default| default.changed)
        {
            hint.push_str("   \u{b7}   BACKSPACE or the arrow for the default");
        }
    }
}

/// The settings icon (`settings_icons`) of a group, tab or search group named
/// `name` (any case): `HUD+` is the scoreboard's, `TEXT` the interface's.
pub(super) fn group_icon(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    let name = name
        .strip_prefix("renderer: ")
        .or_else(|| name.strip_prefix("graphics: "))
        .unwrap_or(&name);
    Some(match name {
        "video" | "display" => "video",
        "audio" | "sound" => "sound",
        "hud" => "hud",
        "controls" | "mouse" => "mouse_joystick",
        "game" | "game options" | "gameplay" => "game_options",
        "chat" => "text",
        "mods" => "options",
        "quick wheel" => crate::quick_wheel::catalog::WHEEL_ICON,
        "network" => "network",
        "hud+" | "scoreboard" => "scoreboard",
        "text" | "interface" => "interface",
        "first setup" => "first_setup",
        "graphics" => "graphics",
        "image" => "image",
        "lighting" => "lighting",
        "shadows" => "shadows",
        "weather" => "weather",
        _ => return None,
    })
}

impl SettingsMenu {
    /// The icon of the selected row's group, for the detail box's title: the
    /// open group or tab, or a search result's own group.
    fn badge(&self) -> Option<sjk_ui::TextureId> {
        let name = match self.section {
            Section::Search => search::group(self.selected)?,
            _ => self.tabs().get(self.tab)?,
        };
        group_icon(name).and_then(crate::settings_icons::texture)
    }
}

/// The detail box for row `row` of `rows` (none when no row of the panel is
/// selected), with the rows' `values`, the `facts` line and its group's
/// `badge`.
fn detail_of<'a>(
    rows: &'static [Setting],
    row: Option<usize>,
    values: &'a [String],
    facts: &'a str,
    badge: Option<sjk_ui::TextureId>,
) -> Detail<'a> {
    let Some((row, setting)) = row.and_then(|row| Some((row, rows.get(row)?))) else {
        return Detail::default();
    };
    let value = values.get(row).map_or("", String::as_str);
    Detail {
        title: help::timing(setting.label).0,
        value: match setting.kind {
            ValueKind::Bool => on_off(value),
            _ => value,
        },
        lines: help::lines(help::help(setting.cvar).unwrap_or_default()),
        facts,
        name: setting.cvar,
        icon: None,
        badge,
    }
}

/// How choice `value` of setting `cvar` is named on its segment, list or
/// field: its own name when it has one ([`help::choice_name`]: a number or a
/// code), else as written (in sentence case), but for a name that is an
/// initialism.
pub(super) fn choice_label<'a>(cvar: &str, value: &'a str) -> &'a str {
    if let Some(name) = help::choice_name(cvar, value) {
        return name;
    }
    // Client names stay in capitals: the menu style's "sjk", the camera style's.
    ["SJK", "EJK"]
        .into_iter()
        .find(|name| value.eq_ignore_ascii_case(name))
        .unwrap_or(value)
}

/// Whether a switch row's value text (`ON` or `OFF`) is on.
pub(super) fn switch_text_on(value: &str) -> bool {
    value.eq_ignore_ascii_case("on")
}

/// A switch's value as the detail box names it.
fn on_off(value: &str) -> &'static str {
    if switch_text_on(value) { "On" } else { "Off" }
}

/// A range end without trailing zeros: `0.5`, `70`, `0.005`.
pub(super) struct Number(pub(super) f64);

impl std::fmt::Display for Number {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = format!("{:.3}", self.0);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        formatter.write_str(text)
    }
}

/// What row `setting` shows for its default, and whether `console`'s value
/// differs from it. Rows whose value is not one cvar's have neither.
pub(super) fn row_default(console: &ViewerConsole, setting: &Setting) -> RowDefault {
    if matches!(setting.kind, ValueKind::Quality) {
        // Off under Vanilla, as the renderer row says, not a change of its own.
        return RowDefault {
            text: Some(Level::High.label().to_owned()),
            changed: !crate::graphics_quality::vanilla(console)
                && Level::current(console) != Some(Level::High),
        };
    }
    if let Some(choice) = super::derived_choice(setting.cvar) {
        return RowDefault {
            text: Some(choice.default.to_owned()),
            changed: !(choice.get)(console).eq_ignore_ascii_case(choice.default),
        };
    }
    if let (Some(default), Some(value)) = (
        crate::graphics_quality::row_default(console, setting.cvar),
        console.cvar(setting.cvar),
    ) {
        let text = match setting.kind {
            ValueKind::Bool => if default == "0" { "Off" } else { "On" }.to_owned(),
            _ => default.to_owned(),
        };
        let value = value.as_text();
        let changed = match (value.trim().parse::<f64>(), default.parse::<f64>()) {
            (Ok(value), Ok(default)) => (value - default).abs() > 1e-6,
            _ => !value.trim().eq_ignore_ascii_case(default),
        };
        return RowDefault {
            text: Some(text),
            changed,
        };
    }
    if let Some(switch) = super::derived_switch(setting.cvar) {
        return RowDefault {
            text: Some("Off".to_owned()),
            changed: (switch.on)(console),
        };
    }
    // The HUD row names a HUD that two cvars select together; the quick
    // wheel's pages are a file, restored to the defaults in their editor.
    if matches!(
        setting.kind,
        ValueKind::Resolution
            | ValueKind::DisplayMode
            | ValueKind::HudPicker
            | ValueKind::WheelPages
            | ValueKind::IdentityPage
            | ValueKind::ImportPage
            | ValueKind::AssetBrowser
    ) {
        return RowDefault::default();
    }
    let (Some(default), Some(value)) = (
        console.cvar_default(setting.cvar),
        console.cvar(setting.cvar),
    ) else {
        return RowDefault::default();
    };
    let text = match (setting.kind, default) {
        (ValueKind::Bool, value) => if switch_on(value) { "On" } else { "Off" }.to_owned(),
        (ValueKind::Integer { min, .. }, CvarValue::Integer(value)) if min < 0 && *value < 0 => {
            "AUTO".to_owned()
        }
        (ValueKind::Float { .. }, CvarValue::Float(value)) => Number(*value).to_string(),
        (_, value) => value.as_text(),
    };
    let changed = match (setting.kind, default, value) {
        (ValueKind::Bool, default, value) => switch_on(default) != switch_on(value),
        (_, CvarValue::Float(default), CvarValue::Float(value)) => (default - value).abs() > 1e-6,
        (_, default, value) => !default
            .as_text()
            .trim()
            .eq_ignore_ascii_case(value.as_text().trim()),
    };
    RowDefault {
        text: Some(text),
        changed,
    }
}

#[cfg(test)]
mod tests {
    use super::super::ClassicRows;
    use super::*;

    fn classic(rows: std::ops::Range<usize>, visible: usize) -> ClassicRows {
        ClassicRows {
            pinned: None,
            lines: ClassicRows::span(rows),
            slider_span: (0.5, 0.25),
            first: 0,
            visible,
        }
    }

    #[test]
    fn keyboard_selection_stays_in_view() {
        let mut panel = classic(10..40, 15);
        panel.reveal(30);
        assert_eq!(panel.first, 6, "row 30 is the last of lines 6..21");
        panel.reveal(12);
        assert_eq!(panel.first, 2);
        panel.reveal(14);
        assert_eq!(panel.first, 2, "already shown");
        // A row the panel does not show (the key-bindings row) leaves it alone.
        panel.reveal(3);
        assert_eq!(panel.first, 2);
    }

    #[test]
    fn wheel_and_bar_stop_at_the_ends() {
        let mut panel = classic(0..20, 15);
        assert_eq!(panel.max_first(), 5);
        panel.scroll_by(-3);
        assert_eq!(panel.first, 0);
        panel.scroll_by(9);
        assert_eq!(panel.first, 5);
        panel.scroll_to_ratio(0.4);
        assert_eq!(panel.first, 2);
        panel.scroll_to_ratio(7.0);
        assert_eq!(panel.first, 5);
        // A span that fits never scrolls.
        let mut short = classic(0..9, 15);
        short.scroll_by(4);
        assert_eq!(short.first, 0);
    }

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn row_of(menu: &SettingsMenu, cvar: &str) -> usize {
        menu.rows()
            .iter()
            .position(|setting| setting.cvar == cvar)
            .unwrap()
    }

    #[test]
    fn a_search_from_the_key_bindings_tab_is_counted_and_carried_over() {
        let (_directory, console) = console();
        assert_eq!(SettingsMenu::count_matches("  "), 0);
        assert!(SettingsMenu::count_matches("fov") >= 1);
        let mut menu = SettingsMenu::new();
        menu.open_classic_group(
            &console,
            Group::Hud,
            crate::menu::classic::panel::Frame::Main,
        );
        menu.carry_search(&console, "  ");
        assert_eq!(menu.search_text(), "", "a blank search is not carried");
        menu.carry_search(&console, "fov");
        assert_eq!(menu.search_text(), "fov");
        let shown = menu.classic.as_ref().unwrap().rows().count();
        assert_eq!(shown, SettingsMenu::count_matches("fov"));
        // The panel names what the other tab found.
        menu.set_elsewhere(3);
        menu.write_key_hint();
        assert!(menu.key_hint.starts_with("3 key bindings match too"));
    }

    #[test]
    fn the_detail_box_describes_the_focused_setting() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_classic_renderer(&console, 0, crate::menu::classic::panel::Frame::Main);
        assert_eq!(menu.rows().len(), RENDER_IMAGE.len());
        assert!(menu.classic.is_some() && menu.renderer_open());
        menu.selected = row_of(&menu, "r_sceneHdr");
        menu.write_detail_facts();
        let detail = menu.detail();
        assert_eq!(detail.title, "HDR scene");
        assert_eq!(detail.name, "r_sceneHdr");
        assert!(detail.lines[0].starts_with("Renders the scene"));
        assert!(
            menu.detail_facts
                .contains("applies after a graphics reload"),
            "{}",
            menu.detail_facts
        );
        // A slider: its default and range; changed, then back to the default.
        menu.selected = row_of(&menu, "r_hdrExposure");
        menu.write_detail_facts();
        assert!(
            menu.detail_facts.contains("0.25 to 4"),
            "{}",
            menu.detail_facts
        );
        assert!(!menu.defaults[menu.selected].changed);
        menu.adjust(&mut console, 1);
        assert!(menu.defaults[menu.selected].changed);
        menu.write_key_hint();
        assert!(menu.key_hint.contains("BACKSPACE"), "{}", menu.key_hint);
        menu.reset_to_default(&mut console, menu.selected);
        assert!(!menu.defaults[menu.selected].changed);
        assert_eq!(
            console.cvar("r_hdrExposure"),
            console.cvar_default("r_hdrExposure")
        );
    }

    #[test]
    fn switches_show_their_default_as_on_or_off() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_tab(&console, SettingsMenu::tab_index("VIDEO").unwrap());
        let row = row_of(&menu, "cg_marks");
        assert_eq!(menu.defaults[row].text.as_deref(), Some("On"));
        menu.selected = row;
        menu.adjust(&mut console, 1);
        assert!(menu.defaults[row].changed);
        // AUTO stands for the frame cap's special value.
        let cap = row_of(&menu, "com_maxfps");
        assert_eq!(menu.defaults[cap].text.as_deref(), Some("AUTO"));
        // Resolution and display mode are not one cvar's value.
        let resolution = row_of(&menu, "r_resolution");
        assert_eq!(menu.defaults[resolution], RowDefault::default());
    }

    #[test]
    fn graphics_quality_lists_its_levels_and_returns_to_high() {
        use crate::graphics_quality::{Level, ROW_NAME};
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_tab(&console, SettingsMenu::tab_index("VIDEO").unwrap());
        let row = row_of(&menu, ROW_NAME);
        assert_eq!(menu.values[row], "High");
        assert_eq!(
            menu.defaults[row],
            RowDefault {
                text: Some("High".to_owned()),
                changed: false
            }
        );
        // The list opens on the level in use; picking one sets it.
        assert!(menu.open_dropdown(&console, row));
        let open = menu.dropdown.as_ref().unwrap();
        assert_eq!(open.picks, Level::SJK.map(Pick::Quality));
        assert_eq!(open.current, 2);
        menu.apply_pick(&mut console, 3);
        assert_eq!(Level::current(&console), Some(Level::Ultra));
        assert_eq!(menu.values[row], "Ultra");
        assert!(menu.defaults[row].changed);
        // Changed on its own, a setting makes it Custom; Left steps down from
        // the level it is nearest.
        console.set_cvar("r_sunShadowTaps", "20");
        menu.refresh(&console);
        assert_eq!(menu.values[row], "Custom");
        menu.selected = row;
        menu.adjust(&mut console, -1);
        assert_eq!(menu.values[row], "High");
        menu.adjust(&mut console, -1);
        assert_eq!(menu.values[row], "Balanced");
        // Backspace's default is High.
        menu.reset_to_default(&mut console, row);
        assert_eq!(Level::current(&console), Some(Level::High));
        assert!(!menu.defaults[row].changed);
        // Under Vanilla the row is off, unchanged, and Backspace leaves it so;
        // the renderer's rows read Vanilla's values as their defaults.
        let renderer = row_of(&menu, crate::graphics_quality::RENDERER_ROW);
        menu.selected = renderer;
        menu.adjust(&mut console, -1);
        assert!(crate::graphics_quality::vanilla(&console));
        assert_eq!(menu.values[renderer], crate::graphics_quality::VANILLA);
        assert!(!menu.defaults[renderer].changed);
        assert_eq!(menu.values[row], "Off");
        assert!(!menu.defaults[row].changed);
        menu.reset_to_default(&mut console, row);
        assert!(crate::graphics_quality::vanilla(&console));
        // Right on the quality starts the SJK renderer at Performance.
        menu.selected = row;
        menu.adjust(&mut console, 1);
        assert_eq!(Level::current(&console), Some(Level::Performance));
        assert!(menu.defaults[renderer].changed);
        // Backspace on the renderer: Vanilla again.
        menu.reset_to_default(&mut console, renderer);
        assert!(crate::graphics_quality::vanilla(&console));
    }

    #[test]
    fn row_controls_round_trip_their_tokens_clear_of_the_other_panel_tokens() {
        for control in [
            RowControl::Segment { row: 0, index: 0 },
            RowControl::Segment { row: 7, index: 2 },
            RowControl::Reset(0),
            RowControl::Reset(42),
        ] {
            assert_eq!(RowControl::of(control.token()), Some(control));
        }
        // Rows, tabs, values, chrome, footer, search and dropdown choices are not
        // controls.
        for token in [0, 499, 500, 700, 800, 911, 913, DROPDOWN_BASE + 5] {
            assert_eq!(RowControl::of(token), None, "{token}");
        }
        // Every searchable row's segments fit below the reset buttons.
        let rows = search::rows().len();
        assert!(SEGMENT_BASE as usize + rows * MAX_SEGMENTS <= RESET_BASE as usize);
        assert!(RESET_BASE as usize + rows <= usize::from(u16::MAX));
    }

    #[test]
    fn a_switch_flips_and_a_short_choice_steps_instead_of_opening_a_list() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_classic_group(
            &console,
            Group::Interface,
            crate::menu::classic::panel::Frame::Main,
        );
        let fonts = row_of(&menu, crate::game_font::CVAR);
        assert!(!menu.open_dropdown(&console, fonts));
        let contrast = row_of(&menu, crate::console::console_options::STYLE_CVAR);
        assert!(!menu.open_dropdown(&console, contrast));
        // A longer choice still opens a list: Video's display modes.
        let mut video = SettingsMenu::new();
        video.open_tab(&console, SettingsMenu::tab_index("VIDEO").unwrap());
        assert!(video.open_dropdown(&console, row_of(&video, "r_fullscreen")));
        // A segment sets its own value; the reset button the default.
        menu.activate_control(
            &mut console,
            RowControl::Segment {
                row: contrast,
                index: 2,
            },
        );
        assert_eq!(
            console
                .cvar(crate::console::console_options::STYLE_CVAR)
                .map(CvarValue::as_text),
            Some("classic".to_owned())
        );
        assert!(menu.defaults[contrast].changed);
        menu.activate_control(&mut console, RowControl::Reset(contrast));
        assert!(!menu.defaults[contrast].changed);
        assert_eq!(menu.selected, contrast);
    }

    #[test]
    fn the_detail_box_carries_its_groups_icon() {
        let (_directory, console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_classic_group(
            &console,
            Group::Hud,
            crate::menu::classic::panel::Frame::Main,
        );
        assert_eq!(menu.badge(), crate::settings_icons::texture("hud"));
        menu.open_classic_renderer(&console, 3, crate::menu::classic::panel::Frame::Main);
        assert_eq!(menu.badge(), crate::settings_icons::texture("weather"));
        // Every tab, renderer tab, group and search group has one.
        let names = TABS
            .iter()
            .chain(&RENDERER_TABS)
            .copied()
            .chain(
                Group::ALL
                    .into_iter()
                    .flat_map(|group| group.tabs().iter().copied()),
            )
            .chain((0..search::rows().len()).filter_map(search::group));
        for name in names {
            assert!(group_icon(name).is_some(), "{name}");
        }
    }

    #[test]
    fn range_ends_drop_trailing_zeros() {
        assert_eq!(Number(0.5).to_string(), "0.5");
        assert_eq!(Number(70.0).to_string(), "70");
        assert_eq!(Number(0.005).to_string(), "0.005");
        assert_eq!(Number(-2.0).to_string(), "-2");
    }
}

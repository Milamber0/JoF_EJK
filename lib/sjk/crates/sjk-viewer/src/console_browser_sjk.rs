//! The console's command and cvar browser (F3) in the SJK UI's look, drawn
//! while the console is the SJK UI's (`con_style sjk`): laid out as the SJK
//! UI's Settings, over the darkened
//! frame in its families. The way back and the screen's name with the search
//! pill at the top; the four filters down a lit rail with their counts; the
//! entries in the middle (name and description, value right, a gold dot when
//! changed); the chosen entry's detail column on the right: its kind, name,
//! value and default and its whole description, all selectable with the mouse
//! ([`crate::text_select`]), and its actions. Keys, pointer tokens and editing
//! are the other looks' ([`super::pointer`]).

use super::pointer::{
    ACTIVATE_TOKEN, COPY_TOKEN, RESET_TOKEN, ROW_BASE, SCROLLBAR_TOKEN, SEARCH_TOKEN,
};
use super::{Browser, Entry, Kind, TABS};
use crate::menu::sjk::{
    Frame, SearchPill, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar, wrap,
};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas, TAB_BASE, TextFamily};
use crate::text::{TextFace, TextStyle, UiFont};
use crate::text_select::Joiner;
use sjk_shell::CommandSource;
use sjk_ui::{Color, FontWeight, TextAlign};

/// The filters' rail.
const RAIL_X: f32 = 96.0;
const RAIL_TOP: f32 = 210.0;
const FILTER_STEP: f32 = 70.0;
const FILTER_HEIGHT: f32 = 60.0;
const RAIL_WIDTH: f32 = 330.0;
/// The entries' column and its rows.
const ROWS_X: f32 = 470.0;
const ROWS_WIDTH: f32 = 800.0;
const ROWS_TOP: f32 = 176.0;
const LINE: f32 = 56.0;
pub(super) const VISIBLE: usize = 14;
/// Where a row's value ends and how wide it may be.
const VALUE_RIGHT: f32 = ROWS_X + ROWS_WIDTH - 44.0;
const VALUE_WIDTH: f32 = 250.0;
/// The detail column.
const DETAIL_X: f32 = 1360.0;
const DETAIL_TOP: f32 = 186.0;
const DETAIL_WIDTH: f32 = 464.0;
/// Where the description stops, and the buttons under it.
const DETAIL_BOTTOM: f32 = 846.0;
const BUTTONS_Y: f32 = 864.0;
/// Characters a description line holds (Exo 2 at 18, 464 wide).
const DETAIL_CHARS: usize = 54;
/// The keys' line.
const KEYS_Y: f32 = 992.0;

// The rail, the rows with their scroll bar and the detail column side by side
// inside the frame's margins; the rows end above the keys.
const _: () = assert!(RAIL_X + RAIL_WIDTH < ROWS_X);
const _: () = assert!(ROWS_X + ROWS_WIDTH + 18.0 < DETAIL_X);
const _: () = assert!(DETAIL_X + DETAIL_WIDTH <= 1824.0);
const _: () = assert!(ROWS_TOP + VISIBLE as f32 * LINE < KEYS_Y - 20.0);
const _: () = assert!(DETAIL_BOTTOM < BUTTONS_Y && BUTTONS_Y + 44.0 < KEYS_Y - 20.0);

/// The filters' names in the SJK UI's sentence case.
const FILTERS: [&str; TABS.len()] = ["All", "Commands", "Cvars", "Changed"];

/// Selected text: gold, translucent so the text stays readable.
const HIGHLIGHT: Color = color::alpha(color::GOLD, 0.34);

/// Measures text as the families draw it in the player's menu text style.
#[derive(Clone, Copy)]
struct Measure<'a> {
    display: &'a UiFont,
    body: &'a UiFont,
    style: TextStyle,
}

impl Measure<'_> {
    /// Glyph advances of `family` at `size` with `weight`, as drawn.
    fn advance(
        &self,
        family: TextFamily,
        size: f32,
        weight: FontWeight,
    ) -> impl Fn(crate::text::GlyphKey) -> f32 + '_ {
        let font = match family {
            TextFamily::Display => self.display,
            TextFamily::Body | TextFamily::Code => self.body,
        };
        let styled = size * self.style.scale;
        let scale = styled / font.height.max(1.0);
        let spacing = self.style.tracking * styled;
        let face = match weight {
            FontWeight::Regular => TextFace::Regular,
            FontWeight::Semibold => TextFace::Semibold,
        };
        move |glyph| font.glyph_of(face, glyph).advance * scale + spacing
    }
}

impl Browser {
    /// Draw the browser in the SJK UI's look over the whole frame, its text to
    /// `target`.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let measure = match &target {
            TextTarget::Families(fonts, style) => Measure {
                display: fonts.display.1,
                body: fonts.body.1,
                style: *style,
            },
            TextTarget::Inter(_, font) => Measure {
                display: font,
                body: font,
                style: font.style(),
            },
        };
        let frame = Frame::new(viewport);
        self.rows = VISIBLE;
        self.first = self.first.min(self.visible.len().saturating_sub(self.rows));
        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        let found = (!self.filter.is_empty()).then_some(self.visible.len());
        top_bar(
            &mut self.ui,
            &frame,
            "Back",
            BACK_TOKEN,
            "Commands and cvars",
            Some(SearchPill {
                query: &self.filter,
                active: self.editing.is_none(),
                prompt: "Search names and descriptions",
                found,
                token: SEARCH_TOKEN,
            }),
        );
        self.sjk_filters(&frame);
        self.sjk_rows(&frame);
        self.sjk_detail(&frame, measure);
        self.sjk_keys(&frame);
        let selected = self.selected.saturating_sub(self.first);
        self.ui
            .finish(ROW_BASE + selected.min(usize::from(u16::MAX - ROW_BASE)) as u16);
        target.append(&self.ui, viewport);
    }

    /// The four filters down the lit rail, with how many each shows.
    fn sjk_filters(&mut self, frame: &Frame) {
        let s = frame.s;
        let bottom = RAIL_TOP + FILTERS.len() as f32 * FILTER_STEP;
        let lit = RAIL_TOP + self.tab as f32 * FILTER_STEP + FILTER_HEIGHT * 0.5;
        kit::rail(
            &mut self.ui,
            frame,
            RAIL_X,
            RAIL_TOP - 16.0,
            bottom,
            Some(lit),
        );
        for (index, label) in FILTERS.iter().enumerate() {
            let top = RAIL_TOP + index as f32 * FILTER_STEP;
            let token = TAB_BASE + index as u16;
            let current = index == self.tab;
            let hovered = self.ui.token_hovered(token);
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{label}"),
                frame.rect(RAIL_X + 28.0, top + 2.0, RAIL_WIDTH - 28.0, 32.0),
                26.0 * s,
                match (current, hovered) {
                    (true, _) => color::GOLD_BRIGHT,
                    (false, true) => color::TEXT,
                    (false, false) => color::MUTED,
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            let count = self.tab_counts[index];
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{count} {}",
                    match (index, count == 1) {
                        (1, true) => "command",
                        (1, false) => "commands",
                        (2, true) => "cvar",
                        (2, false) => "cvars",
                        (_, true) => "entry",
                        (_, false) => "entries",
                    }
                ),
                frame.rect(RAIL_X + 28.0, top + 34.0, RAIL_WIDTH - 28.0, 22.0),
                15.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            self.ui
                .hit_region(token, frame.rect(RAIL_X, top, RAIL_WIDTH, FILTER_HEIGHT));
        }
    }

    /// The entries: name and description, the value at the right with a gold
    /// dot when it differs from the default, the chosen one on the kit's band.
    fn sjk_rows(&mut self, frame: &Frame) {
        let s = frame.s;
        let shown = self.first..self.visible.len().min(self.first + self.rows);
        for (slot, position) in shown.enumerate() {
            let top = ROWS_TOP + slot as f32 * LINE;
            let token = ROW_BASE + slot as u16;
            let chosen = position == self.selected;
            if chosen {
                kit::band(&mut self.ui, frame, [ROWS_X, top, ROWS_WIDTH, LINE - 4.0]);
            }
            let entry = &self.entries[self.visible[position]];
            let name_room = ROWS_WIDTH - 44.0 - VALUE_WIDTH - 16.0;
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", entry.name),
                frame.rect(ROWS_X + 22.0, top + 4.0, name_room, 26.0),
                19.0 * s,
                if chosen {
                    color::GOLD_BRIGHT
                } else {
                    color::TEXT
                },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let description = if entry.description.is_empty() {
                "No description"
            } else {
                entry.description.as_str()
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{description}"),
                frame.rect(ROWS_X + 22.0, top + 30.0, ROWS_WIDTH - 88.0, 20.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let value = [VALUE_RIGHT - VALUE_WIDTH, top + 4.0, VALUE_WIDTH, 26.0];
            match &entry.kind {
                Kind::Cvar { value: current, .. } => {
                    let changed = entry.changed();
                    if let Some(edit) = self.editing.as_deref().filter(|_| chosen) {
                        kit::field(
                            &mut self.ui,
                            frame,
                            [VALUE_RIGHT - VALUE_WIDTH, top + 7.0, VALUE_WIDTH, 38.0],
                            format_args!("{edit}_"),
                            true,
                            false,
                        );
                    } else {
                        text(
                            &mut self.ui,
                            TextFamily::Display,
                            format_args!("{current}"),
                            frame.rect(value[0], value[1], value[2], value[3]),
                            22.0 * s,
                            if changed {
                                color::GOLD_BRIGHT
                            } else {
                                color::TEXT
                            },
                            FontWeight::Regular,
                            TextAlign::End,
                        );
                    }
                    if changed {
                        kit::changed_dot(
                            &mut self.ui,
                            frame,
                            ROWS_X + ROWS_WIDTH - 22.0,
                            top + 17.0,
                        );
                    }
                }
                Kind::Command(source) => {
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("{}", command_kind(*source)),
                        frame.rect(value[0], value[1], value[2], value[3]),
                        15.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::End,
                    );
                }
            }
            self.ui
                .hit_region(token, frame.rect(ROWS_X, top, ROWS_WIDTH, LINE - 4.0));
        }
        if self.visible.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("No command or cvar matches the search."),
                frame.rect(ROWS_X + 22.0, ROWS_TOP + 8.0, ROWS_WIDTH - 44.0, 28.0),
                19.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else if self.visible.len() > self.rows {
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                frame.rect(
                    ROWS_X + ROWS_WIDTH + 6.0,
                    ROWS_TOP,
                    4.0,
                    self.rows as f32 * LINE - 4.0,
                ),
                self.first,
                self.rows,
                self.visible.len(),
            );
        }
    }

    /// The chosen entry's detail: its kind, name, value and default and its
    /// description, selectable with the mouse, then its actions.
    fn sjk_detail(&mut self, frame: &Frame, measure: Measure<'_>) {
        let s = frame.s;
        let position = self.visible.get(self.selected).copied();
        // The selection belongs to the entry shown, and to its value being typed
        // or not.
        let key =
            position.map_or(0, |index| (index as u64 + 1) << 1) | u64::from(self.editing.is_some());
        self.select.begin(key);
        let Some(index) = position else {
            return;
        };
        let entry = &self.entries[index];
        let ui = &mut self.ui;
        let select = &mut self.select;
        // One selectable line: its rectangle in frame pixels.
        let mut line = |ui: &mut MenuCanvas,
                        value: &str,
                        rect: [f32; 4],
                        family: TextFamily,
                        size: f32,
                        colour: Color,
                        joiner: Joiner| {
            let [x, y, width, height] = rect;
            let window = frame.rect(x, y, width, height);
            select.line(
                ui,
                window,
                window.x,
                value,
                measure.advance(family, size * s, FontWeight::Regular),
                joiner,
                HIGHLIGHT,
            );
            text(
                ui,
                family,
                format_args!("{value}"),
                window,
                size * s,
                colour,
                FontWeight::Regular,
                TextAlign::Start,
            );
        };
        text(
            ui,
            TextFamily::Body,
            format_args!("{}", kind_label(entry)),
            frame.rect(DETAIL_X, DETAIL_TOP, DETAIL_WIDTH, 22.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        line(
            ui,
            &entry.name,
            [DETAIL_X, DETAIL_TOP + 26.0, DETAIL_WIDTH, 44.0],
            TextFamily::Display,
            34.0,
            color::TEXT,
            Joiner::Newline,
        );
        let _ = ui.draw_list_mut().push(sjk_ui::DrawCommand::SolidRect {
            rect: frame.rect(DETAIL_X, DETAIL_TOP + 80.0, DETAIL_WIDTH, 1.0),
            color: color::alpha(color::HOLO, 0.22),
        });
        let mut y = DETAIL_TOP + 98.0;
        if let Kind::Cvar { value, default, .. } = &entry.kind {
            let fact = |ui: &mut MenuCanvas, label: &str, y: f32| {
                text(
                    ui,
                    TextFamily::Body,
                    format_args!("{label}"),
                    frame.rect(DETAIL_X, y, 120.0, 30.0),
                    16.0 * s,
                    color::QUIET,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            };
            fact(ui, "Value", y);
            if let Some(edit) = self.editing.as_deref() {
                kit::field(
                    ui,
                    frame,
                    [DETAIL_X + 110.0, y - 4.0, DETAIL_WIDTH - 110.0, 38.0],
                    format_args!("{edit}_"),
                    true,
                    false,
                );
            } else {
                line(
                    ui,
                    value,
                    [DETAIL_X + 110.0, y, DETAIL_WIDTH - 110.0, 30.0],
                    TextFamily::Display,
                    24.0,
                    if entry.changed() {
                        color::GOLD_BRIGHT
                    } else {
                        color::TEXT
                    },
                    Joiner::Newline,
                );
            }
            y += 40.0;
            fact(ui, "Default", y);
            line(
                ui,
                default,
                [DETAIL_X + 110.0, y, DETAIL_WIDTH - 110.0, 30.0],
                TextFamily::Display,
                24.0,
                color::MUTED,
                Joiner::Newline,
            );
            y += 52.0;
        }
        let description = if entry.description.is_empty() {
            "No description."
        } else {
            entry.description.as_str()
        };
        for row in wrap(description, DETAIL_CHARS) {
            if y + 28.0 > DETAIL_BOTTOM {
                break;
            }
            line(
                ui,
                row,
                [DETAIL_X, y, DETAIL_WIDTH, 28.0],
                TextFamily::Body,
                18.0,
                color::TEXT,
                Joiner::Space,
            );
            y += 28.0;
        }
        self.sjk_actions(frame, index);
    }

    /// The chosen entry's actions: edit or insert (gold), default, copy.
    fn sjk_actions(&mut self, frame: &Frame, index: usize) {
        let entry = &self.entries[index];
        let editing = self.editing.is_some();
        let (primary, writable) = match &entry.kind {
            Kind::Command(_) => ("Insert", false),
            Kind::Cvar {
                read_only: true, ..
            } => ("Read-only", false),
            Kind::Cvar { .. } if editing => ("Apply", true),
            Kind::Cvar { .. } => ("Edit value", true),
        };
        let can_act = !matches!(
            entry.kind,
            Kind::Cvar {
                read_only: true,
                ..
            }
        );
        kit::button(
            &mut self.ui,
            frame,
            [DETAIL_X, BUTTONS_Y, 170.0, 44.0],
            primary,
            true,
            can_act,
            false,
            ACTIVATE_TOKEN,
        );
        kit::button(
            &mut self.ui,
            frame,
            [DETAIL_X + 182.0, BUTTONS_Y, 130.0, 44.0],
            "Default",
            false,
            writable && !editing,
            false,
            RESET_TOKEN,
        );
        kit::button(
            &mut self.ui,
            frame,
            [DETAIL_X + 324.0, BUTTONS_Y, 140.0, 44.0],
            if self.select.selected().is_some() {
                "Copy text"
            } else {
                "Copy line"
            },
            false,
            true,
            false,
            COPY_TOKEN,
        );
    }

    /// The keys bottom right, the last action's outcome bottom left.
    fn sjk_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let editing = self.editing.is_some();
        let keys: [(&[&str], &str); 6] = [
            (&["Up", "Down"], "Choose"),
            (&["Tab"], "Filter"),
            (&["Enter"], if editing { "Apply" } else { "Edit" }),
            (&["Del"], "Default"),
            (&["Ctrl", "C"], "Copy"),
            (&["Esc"], if editing { "Cancel" } else { "Back" }),
        ];
        let width: f32 = keys
            .iter()
            .map(|(keys, action)| key_hint_width(keys, action, s) + 18.0 * s)
            .sum();
        let [right, y] = frame.point(1824.0, KEYS_Y - 12.0);
        let mut x = right - width + 18.0 * s;
        for (keys, action) in keys {
            x = key_hint(&mut self.ui, keys, action, x, y, s) + 18.0 * s;
        }
        if !self.status.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", self.status),
                frame.rect(RAIL_X, KEYS_Y - 12.0, 860.0, 24.0),
                16.0 * s,
                if self.status_error {
                    Color::new(1.0, 0.56, 0.5, 1.0)
                } else {
                    color::GOLD_BRIGHT
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }
}

/// A command's origin in words.
fn command_kind(source: CommandSource) -> &'static str {
    match source {
        CommandSource::External => "Server command",
        _ => "Command",
    }
}

/// An entry's kind in words, over its name in the detail column.
fn kind_label(entry: &Entry) -> &'static str {
    match entry.kind {
        Kind::Command(source) => command_kind(source),
        Kind::Cvar {
            read_only: true, ..
        } => "Cvar, read-only",
        Kind::Cvar { archived: true, .. } => "Cvar, saved in your profile",
        Kind::Cvar { .. } => "Cvar",
    }
}

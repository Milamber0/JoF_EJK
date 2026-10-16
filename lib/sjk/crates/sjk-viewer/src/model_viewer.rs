//! The `modelviewer` page: every installed player model and skin in a list that a search
//! narrows, the chosen one standing on the stage beside it (the menu map's, or the live
//! preview over a match, as the Collection's Shaders tab shows the player's model). It is
//! a developer tool: nothing opens it but its command, and it changes no setting. It is
//! drawn in the SJK UI's look whatever the menu style. `console_model_viewer.rs` is its
//! console side.

use crate::console::collection_panel::{Backstage, MODEL_AREA};
use crate::menu::sjk::{Frame, TextTarget, color, fade, fade_across, kit, text};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, InputEvent, Rect, TextAlign, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub(crate) const COMMAND: &str = "modelviewer";
pub(crate) const HELP: &str = "Browse and search every installed player model, shown on the stage";

/// Rows the list shows at a time, and their pitch (frame pixels).
const ROWS: usize = 14;
const ROW_HEIGHT: f32 = 52.0;
const LEFT_X: f32 = 96.0;
const LIST_Y: f32 = 262.0;
const LIST_WIDTH: f32 = 620.0;
/// Longest search the field takes.
const SEARCH_MAX: usize = 40;
/// Pointer tokens: the rows are `0..ROWS`.
const SEARCH_TOKEN: u16 = 500;
const WHEEL_TOKEN: u16 = 910;

/// What kind of model an entry is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// A player or NPC model with its skin (Ghoul2): worn as the player's is, posed.
    Player,
    /// A rigid `.md3` (items, weapons, map objects, hats).
    Md3,
    /// A rigid `.glm` that is not a player body.
    Glm,
}

impl Kind {
    /// A model shown as a prop (spun), not as a posed player.
    pub(crate) fn is_prop(self) -> bool {
        self != Self::Player
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Player => "",
            Self::Md3 => "md3",
            Self::Glm => "glm",
        }
    }
}

/// One model the list offers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    /// A player's `model/skin` (or a species' model alone), else the asset's path.
    pub(crate) name: String,
    /// The retail game's own (it comes first).
    pub(crate) retail: bool,
    pub(crate) kind: Kind,
}

/// What the page asks of the console.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
}

pub(crate) struct Panel {
    open: bool,
    owns_console: bool,
    /// Every model, retail first, in the order the catalogue gives; empty until the
    /// catalogue is read.
    entries: Vec<Entry>,
    /// The model names in lower case, one per entry, so a search compares without
    /// allocating.
    lowered: Vec<String>,
    query: String,
    /// The entries the search keeps, as indexes into `entries`.
    shown: Vec<usize>,
    /// The chosen row of `shown`, and the first row on screen.
    selected: usize,
    first: usize,
    backstage: Backstage,
    ui: MenuCanvas,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            open: false,
            owns_console: false,
            entries: Vec::new(),
            lowered: Vec::new(),
            query: String::new(),
            shown: Vec::new(),
            selected: 0,
            first: 0,
            backstage: Backstage::None,
            ui: MenuCanvas::with_text_capacity(96),
        }
    }
}

impl Panel {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page with an empty search; `owns_console` when it opened the console.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        self.query.clear();
        self.refilter();
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        self.backstage = Backstage::None;
        owned
    }

    pub(crate) fn has_models(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Give the page the models to list, once the catalogue is read.
    pub(crate) fn set_models(&mut self, entries: Vec<Entry>) {
        self.lowered = entries.iter().map(|e| e.name.to_lowercase()).collect();
        self.entries = entries;
        self.refilter();
    }

    /// Where the model stands this frame.
    pub(crate) fn set_backstage(&mut self, backstage: Backstage) {
        self.backstage = backstage;
    }

    /// The entry chosen, while the page is open.
    pub(crate) fn choice(&self) -> Option<&Entry> {
        if !self.open {
            return None;
        }
        let index = *self.shown.get(self.selected)?;
        self.entries.get(index)
    }

    /// The name of the model chosen.
    #[cfg(test)]
    fn model(&self) -> Option<&str> {
        self.choice().map(|entry| entry.name.as_str())
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Keep the entries whose name has every word of the search, and the chosen model
    /// if it is still among them.
    fn refilter(&mut self) {
        let chosen = self.shown.get(self.selected).copied();
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.shown = (0..self.entries.len())
            .filter(|&index| words.iter().all(|word| self.lowered[index].contains(word)))
            .collect();
        let at = chosen.and_then(|chosen| self.shown.iter().position(|&index| index == chosen));
        self.select(at.unwrap_or(0));
    }

    fn select(&mut self, row: usize) {
        self.selected = row.min(self.shown.len().saturating_sub(1));
        if self.selected < self.first {
            self.first = self.selected;
        }
        if self.selected >= self.first + ROWS {
            self.first = self.selected + 1 - ROWS;
        }
        self.first = self.first.min(self.shown.len().saturating_sub(ROWS));
    }

    fn step(&mut self, by: isize) {
        self.select(self.selected.saturating_add_signed(by));
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        shift: bool,
        control: bool,
    ) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        match key {
            KeyCode::Escape if !event.repeat => return PanelAction::Close,
            KeyCode::ArrowUp => self.step(-1),
            KeyCode::ArrowDown => self.step(1),
            KeyCode::PageUp => self.step(-(ROWS as isize)),
            KeyCode::PageDown => self.step(ROWS as isize),
            KeyCode::Home => self.select(0),
            KeyCode::End => self.select(usize::MAX),
            KeyCode::Escape => {}
            _ => {
                let text = event.text.as_deref();
                if crate::text_entry::edit(
                    &mut self.query,
                    key,
                    text,
                    shift,
                    control,
                    SEARCH_MAX,
                    true,
                ) {
                    self.refilter();
                }
            }
        }
        PanelAction::None
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        match event.kind {
            UiEventKind::Wheel => {
                let direction = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
                self.step(direction * 3);
            }
            UiEventKind::Activate => match event.token {
                Some(BACK_TOKEN) => return PanelAction::Close,
                Some(row) if usize::from(row) < ROWS => {
                    let row = self.first + usize::from(row);
                    if row < self.shown.len() {
                        self.select(row);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        PanelAction::None
    }

    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        self.ui.begin_transparent(viewport);
        self.scrims(viewport, &frame);
        if let Backstage::Preview { ready: true } = self.backstage {
            let [x, y, width, height] = MODEL_AREA;
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x, y, width, height),
                texture: crate::ui_renderer::PREVIEW_TEXTURE,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("Model viewer"),
            frame.rect(LEFT_X, 70.0, 700.0, 56.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let retail = self.entries.iter().filter(|entry| entry.retail).count();
        if self.entries.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("Reading the installed models..."),
                frame.rect(LEFT_X, 134.0, LIST_WIDTH, 28.0),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{} models · {retail} Jedi Academy, {} community",
                    self.entries.len(),
                    self.entries.len() - retail,
                ),
                frame.rect(LEFT_X, 134.0, LIST_WIDTH, 28.0),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        kit::search(
            &mut self.ui,
            &frame,
            [LEFT_X, 184.0, LIST_WIDTH, 56.0],
            &self.query,
            true,
            "Type to search models",
            Some(self.shown.len()),
            SEARCH_TOKEN,
        );
        self.ui.scroll_region(
            WHEEL_TOKEN,
            frame.rect(LEFT_X, LIST_Y, LIST_WIDTH, ROWS as f32 * ROW_HEIGHT),
        );
        for (slot, &index) in self.shown.iter().skip(self.first).take(ROWS).enumerate() {
            let top = LIST_Y + slot as f32 * ROW_HEIGHT;
            let entry = &self.entries[index];
            if self.first + slot == self.selected {
                kit::band(
                    &mut self.ui,
                    &frame,
                    [LEFT_X, top, LIST_WIDTH, ROW_HEIGHT - 4.0],
                );
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", entry.name),
                frame.rect(LEFT_X + 20.0, top + 9.0, LIST_WIDTH - 200.0, 28.0),
                19.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            if entry.retail || entry.kind.is_prop() {
                let tag = match (entry.retail, entry.kind.tag()) {
                    (true, "") => "Jedi Academy".to_owned(),
                    (true, kind) => format!("Jedi Academy {kind}"),
                    (false, kind) => kind.to_owned(),
                };
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{tag}"),
                    frame.rect(LEFT_X + LIST_WIDTH - 180.0, top + 11.0, 160.0, 24.0),
                    15.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
            self.ui.hit_region(
                slot as u16,
                frame.rect(LEFT_X, top, LIST_WIDTH, ROW_HEIGHT - 4.0),
            );
        }
        if self.shown.is_empty() && !self.entries.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("No model matches \"{}\".", self.query),
                frame.rect(LEFT_X + 20.0, LIST_Y + 8.0, LIST_WIDTH - 40.0, 28.0),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if let Some(name) = self.choice().map(|entry| entry.name.clone()) {
            // A player is its model and skin; a prop is its file and the folder it is in.
            let (model, skin) = match name.rsplit_once('/') {
                Some((folder, file)) if self.choice().is_some_and(|e| e.kind.is_prop()) => {
                    (file, folder)
                }
                _ => name.split_once('/').unwrap_or((&name, "")),
            };
            let [x, y, width, _] = MODEL_AREA;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{model}"),
                frame.rect(x, y + 740.0, width, 44.0),
                32.0 * s,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Center,
            );
            if !skin.is_empty() {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{skin}"),
                    frame.rect(x, y + 786.0, width, 30.0),
                    20.0 * s,
                    color::GOLD_BRIGHT,
                    FontWeight::Regular,
                    TextAlign::Center,
                );
            }
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Type to search · Up/Down, Page Up/Down, wheel to choose · Esc to close"),
            frame.rect(LEFT_X, 1010.0, 1100.0, 28.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui.hit_region(
            BACK_TOKEN,
            frame.rect(LEFT_X + LIST_WIDTH - 80.0, 76.0, 80.0, 42.0),
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Close"),
            frame.rect(LEFT_X + LIST_WIDTH - 80.0, 83.0, 80.0, 28.0),
            17.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::End,
        );
        self.ui
            .finish(self.selected.saturating_sub(self.first) as u16);
        target.append(&self.ui, viewport);
    }

    /// Dark on the left under the list, clear over the model on the right; the map
    /// darkened whole when no model stands behind the page.
    fn scrims(&mut self, viewport: [f32; 2], frame: &Frame) {
        if self.backstage == Backstage::None {
            crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
            return;
        }
        let [width, height] = viewport;
        let space = |alpha| color::alpha(color::SPACE, alpha);
        let x = |frame_x: f32| frame.point(frame_x, 0.0)[0];
        fade_across(
            &mut self.ui,
            Rect::new(0.0, 0.0, x(780.0), height),
            space(0.94),
            space(0.86),
        );
        fade_across(
            &mut self.ui,
            Rect::new(x(780.0), 0.0, x(1_060.0) - x(780.0), height),
            space(0.86),
            space(0.0),
        );
        let y = |frame_y: f32| frame.point(0.0, frame_y)[1];
        fade(
            &mut self.ui,
            Rect::new(0.0, y(560.0), width, height - y(560.0)),
            space(0.0),
            space(0.7),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<Entry> {
        [
            "kyle/default",
            "luke/default",
            "luke/hoth",
            "rodian/default",
            "Jan/Default",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| Entry {
            name: name.to_owned(),
            retail: index < 2,
            kind: Kind::Player,
        })
        .collect()
    }

    fn panel() -> Panel {
        let mut panel = Panel::default();
        panel.open(false);
        panel.set_models(entries());
        panel
    }

    #[test]
    fn a_search_keeps_the_models_with_every_word_whatever_the_case() {
        let mut panel = panel();
        assert_eq!(panel.shown.len(), 5);
        panel.query = "LUKE hoth".into();
        panel.refilter();
        assert_eq!(panel.model(), Some("luke/hoth"));
        panel.query = "default".into();
        panel.refilter();
        assert_eq!(panel.shown.len(), 4);
        panel.query = "nobody".into();
        panel.refilter();
        assert_eq!(panel.model(), None);
    }

    #[test]
    fn the_chosen_model_stays_chosen_when_a_search_still_holds_it() {
        let mut panel = panel();
        panel.select(2);
        assert_eq!(panel.model(), Some("luke/hoth"));
        panel.query = "luke".into();
        panel.refilter();
        assert_eq!(panel.model(), Some("luke/hoth"));
        panel.step(-5);
        assert_eq!(panel.model(), Some("luke/default"));
        panel.step(50);
        assert_eq!(panel.model(), Some("luke/hoth"));
    }

    #[test]
    fn the_page_shows_a_model_only_while_it_is_open_and_hands_back_its_console() {
        let mut panel = Panel::default();
        panel.open(true);
        panel.set_models(entries());
        assert_eq!(panel.model(), Some("kyle/default"));
        assert!(panel.close());
        assert_eq!(panel.model(), None);
        panel.open(false);
        assert!(!panel.close());
    }
}

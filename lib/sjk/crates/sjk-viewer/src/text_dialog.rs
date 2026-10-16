//! The text dialog: a panel in the middle of the screen with a text box, Send and Cancel,
//! for a world note (`world_notes`, the second `worldnote` press), a bug report
//! (`bug_report`, the Report a bug button under the game menu) or a player report
//! (`player_report`, a reason on the game menu's Report page). Escape or Cancel drops
//! it; Enter or Send sends it.
//!
//! All keep only what the hub accepts as it is typed or pasted (letters and digits of
//! any script, spaces and `. , ! ? ' - : ( )`, line breaks turned into spaces), a report
//! at most 600 characters, a note at most [`NOTE_MAX`], a player report at most 300.
//!
//! The text has an insertion point (the console's `LineEdit`): the arrows, Home, End,
//! Backspace and Delete work at it, typing and pasting insert there, a click places it.
//! [`field`] wraps the text and finds each caret position with the renderer's own
//! measure (the player's text size and spacing), so both looks draw the caret between
//! two glyphs.
//!
//! The launcher is the Report a bug button drawn centred at the bottom of the screen
//! while the game menu is open. With the classic menus both take the classic+ look
//! ([`classic`]); with the SJK UI the dialog is its pop-up card ([`sjk`]) and has no
//! launcher (Report a bug is on the in-game menu's SJK page).
//!
//! In the SJK UI a bug or player report keeps its card after Send: "Sending..." until
//! the hub answers, then what it stored the report as, or why it did not go, with Edit
//! to change the text and send it again ([`TextDialog::answer`]). The other looks close
//! on Send and the answer comes as a centre print, as it does when the card was closed
//! first. A note always closes on Send: its screenshot is taken of the next frame.
//!
//! A note's dialog names the model it is about, when the selection has one, with Clear
//! model to drop it ([`Action::ClearModel`]); a click on the scene round the dialog
//! asks for what is under the pointer to be noted instead ([`Action::PickAt`]).

use crate::console::line_edit::{LineEdit, Motion};
use crate::menu::art::ArtSet;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use field::FieldLayout;
use sjk_ui::{InputEvent, UiEventKind};
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "text_dialog_classic.rs"]
mod classic;
#[path = "text_dialog_field.rs"]
mod field;
#[path = "text_dialog_sjk.rs"]
mod sjk;

/// Longest world note, in characters.
pub(crate) const NOTE_MAX: usize = sjk_identity::report::NOTE_MAX;
/// Lines of text the box shows; a longer text shows its end.
const LINES: usize = 6;
/// Lines a note's box shows in the SJK UI's card: a note may run to a page.
const NOTE_LINES: usize = 10;

const FIELD_TOKEN: u16 = 960;
const SEND_TOKEN: u16 = 961;
const CANCEL_TOKEN: u16 = 962;
const LAUNCH_TOKEN: u16 = 963;
/// The SJK UI card's Close (Done once sent) and Edit, after Send.
const CLOSE_TOKEN: u16 = 964;
const EDIT_TOKEN: u16 = 965;
/// A note's Clear model.
const CLEAR_MODEL_TOKEN: u16 = 966;

/// Which look the dialog is drawn in, following `ui_menuStyle`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Look {
    /// The classic+ pop-up, with the retail menu art it can draw.
    #[default]
    Classic,
    /// The SJK UI's pop-up card.
    Sjk,
}

/// Where a report stands in the SJK UI's card.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum Phase {
    /// The text is being written.
    #[default]
    Writing,
    /// Handed to the identity service; the hub has not answered yet.
    Sending,
    /// The hub stored it: what as ("report #12").
    Sent(String),
    /// It did not go: why.
    Failed(String),
}

/// Which report an answer is for ([`TextDialog::answer`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Report {
    Bug,
    Player,
}

/// What the dialog is for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// A note about the selection named by `subject`, and the path of its model.
    Note {
        subject: String,
        model: Option<String>,
    },
    /// A bug report for the hub.
    Report,
    /// A report about a player for the hub: `subject` names them and the reason.
    PlayerReport {
        subject: String,
        category: sjk_identity::Category,
    },
}

/// What the caller does after the dialog handled an event.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    None,
    /// Closed without sending, or (the SJK UI's card) after its answer.
    Cancel,
    /// Send this text (the dialog has closed).
    Send(Kind, String),
    /// A note's Clear model: note what is behind the model.
    ClearModel,
    /// A click on the scene round a note's dialog, at this point in physical pixels:
    /// note what is under it.
    PickAt([f32; 2]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    Field,
    Send,
    Cancel,
    /// A note's Clear model, while it has a model.
    ClearModel,
}

pub(crate) struct TextDialog {
    kind: Option<Kind>,
    text: String,
    /// The insertion point in `text`.
    edit: LineEdit,
    /// How the last frame wrapped `text`, for the keys and the pointer that move by lines.
    layout: FieldLayout,
    focus: Focus,
    /// Why the last Send was refused.
    message: String,
    ui: MenuCanvas,
    launcher: MenuCanvas,
    epoch: Instant,
    /// Shift is held (Shift+Tab goes back).
    shift: bool,
    /// Control is held (the arrows and Backspace work on words).
    control: bool,
    /// The look drawn, and the retail menu art the classic+ one can use.
    look: Look,
    art: ArtSet,
    /// Where a report stands; always [`Phase::Writing`] outside the SJK UI.
    phase: Phase,
    /// The panel or card the last frame drew, in physical pixels: a note's click
    /// outside it picks in the scene.
    card: Option<sjk_ui::Rect>,
}

impl Default for TextDialog {
    fn default() -> Self {
        Self {
            kind: None,
            text: String::with_capacity(2_400),
            edit: LineEdit::default(),
            layout: FieldLayout::default(),
            focus: Focus::Field,
            message: String::new(),
            // The SJK UI's card draws about 30 runs and 70 commands.
            ui: MenuCanvas::with_capacities(48, 192, 160),
            launcher: MenuCanvas::with_capacities(4, 32, 16),
            epoch: Instant::now(),
            shift: false,
            control: false,
            look: Look::default(),
            art: ArtSet::default(),
            phase: Phase::Writing,
            card: None,
        }
    }
}

/// The longest text `kind` takes, in characters.
fn limit(kind: &Kind) -> usize {
    match kind {
        Kind::Note { .. } => NOTE_MAX,
        Kind::Report => sjk_identity::report::TEXT_MAX,
        Kind::PlayerReport { .. } => sjk_identity::report::PLAYER_MAX,
    }
}

/// Why `text` cannot be sent as `kind`, or `None` when it can.
fn refusal(kind: &Kind, text: &str) -> Option<&'static str> {
    match kind {
        Kind::Report => sjk_identity::report::text(text).err(),
        Kind::Note { .. } => sjk_identity::report::note_text(text).err(),
        Kind::PlayerReport { .. } => sjk_identity::report::player_text(text).err(),
    }
}

/// `text` as `kind` keeps it when typed or pasted onto `field`.
fn typed(kind: &Kind, field: &str, text: &str) -> String {
    let room = limit(kind).saturating_sub(field.chars().count());
    text.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| sjk_identity::report::allowed(*c))
        .take(room)
        .collect()
}

/// `text` cut to its first `max` characters.
fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// `text` broken into lines of at most `width` characters, at spaces where it can be.
#[cfg(test)]
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    wrap_by(text, |line| line.chars().count() <= width)
}

/// `text` broken into lines that `fits` accepts, at spaces where it can be; a word longer
/// than a line is cut where the line ends.
fn wrap_by(text: &str, fits: impl Fn(&str) -> bool) -> Vec<String> {
    let mut ranges = Vec::new();
    field::wrap_ranges(text, fits, &mut ranges);
    ranges
        .into_iter()
        .map(|range| text[range].to_owned())
        .collect()
}

/// `text` broken into lines no wider than `width` pixels when drawn `size` pixels high
/// in `font` with `spacing` pixels between glyphs, leaving room for the caret.
pub(crate) fn wrap_to(
    text: &str,
    font: &UiFont,
    size: f32,
    spacing: f32,
    width: f32,
) -> Vec<String> {
    let scale = size / font.height.max(1.0);
    let room = width - size * 0.5;
    wrap_by(text, |line| {
        crate::text::visible_text_width(font, line, scale) + spacing * line.chars().count() as f32
            <= room
    })
}

impl TextDialog {
    pub(crate) fn is_open(&self) -> bool {
        self.kind.is_some()
    }

    /// Open empty for `kind`.
    pub(crate) fn open(&mut self, kind: Kind) {
        self.kind = Some(kind);
        self.text.clear();
        self.edit.to_end(&self.text);
        self.layout.reset();
        self.message.clear();
        self.focus = Focus::Field;
        self.phase = Phase::Writing;
        self.epoch = Instant::now();
        self.card = None;
    }

    fn close(&mut self) -> Action {
        self.kind = None;
        self.phase = Phase::Writing;
        self.card = None;
        Action::Cancel
    }

    /// A note's selection changed (Clear model, or a click on the scene): its new
    /// subject and model, the text kept.
    pub(crate) fn set_note(&mut self, subject: String, model: Option<String>) {
        if let Some(Kind::Note {
            subject: shown,
            model: shown_model,
        }) = &mut self.kind
        {
            *shown = subject;
            *shown_model = model;
            if self.focus == Focus::ClearModel && shown_model.is_none() {
                self.focus = Focus::Field;
            }
        }
    }

    /// The open note's model, if it has one.
    fn note_model(&self) -> Option<&str> {
        match &self.kind {
            Some(Kind::Note { model, .. }) => model.as_deref(),
            _ => None,
        }
    }

    /// Enter or Send: hand the text over, or say why it cannot go. The SJK UI's card
    /// stays open on a report, to show the hub's answer ([`Self::answer`]), and keeps
    /// the text for Edit.
    fn send(&mut self) -> Action {
        let Some(kind) = self.kind.clone() else {
            return Action::None;
        };
        if let Some(why) = refusal(&kind, &self.text) {
            self.message = why.to_owned();
            self.focus = Focus::Field;
            return Action::None;
        }
        if self.look == Look::Sjk && !matches!(kind, Kind::Note { .. }) {
            self.phase = Phase::Sending;
            self.focus = Focus::Send;
            return Action::Send(kind, self.text.clone());
        }
        self.kind = None;
        self.card = None;
        Action::Send(kind, std::mem::take(&mut self.text))
    }

    /// Edit after a report failed: back to the text, kept as it was.
    fn edit(&mut self) {
        self.phase = Phase::Writing;
        self.focus = Focus::Field;
        self.message.clear();
        self.epoch = Instant::now();
    }

    /// Whether the card shows a `report` waiting for the hub's answer.
    pub(crate) fn sending(&self, report: Report) -> bool {
        self.phase == Phase::Sending && self.reports(report)
    }

    /// Whether the open dialog is for `report`.
    fn reports(&self, report: Report) -> bool {
        matches!(
            (&self.kind, report),
            (Some(Kind::Report), Report::Bug) | (Some(Kind::PlayerReport { .. }), Report::Player)
        )
    }

    /// The hub's answer to `report`: what it stored it as, or why it did not go. True
    /// when the card waiting for it shows it; false when there is none (another look,
    /// or the card closed first), and the caller shows it as a centre print.
    pub(crate) fn answer(&mut self, report: Report, outcome: Result<String, String>) -> bool {
        if !self.sending(report) {
            return false;
        }
        self.phase = match outcome {
            Ok(stored) => Phase::Sent(stored),
            Err(why) => Phase::Failed(why),
        };
        self.focus = Focus::Send;
        true
    }

    /// Fill the dialog for the menu snapshots: its `text`, the focus on Send or the field,
    /// and a refusal `message`.
    #[cfg(test)]
    pub(crate) fn preview(&mut self, text: &str, on_send: bool, message: &str) {
        self.text = text.to_owned();
        self.edit.to_end(&self.text);
        self.layout.reset();
        self.focus = if on_send { Focus::Send } else { Focus::Field };
        self.message = message.to_owned();
    }

    /// Hold the caret lit, for the world shots.
    #[cfg(test)]
    pub(crate) fn caret_for_shot(&mut self) {
        self.epoch = Instant::now() + std::time::Duration::from_secs(60);
    }

    /// Press Send, for the world shots.
    #[cfg(test)]
    pub(crate) fn send_for_shot(&mut self) -> Action {
        self.send()
    }

    /// Choose the look, with the retail `art` the classic+ one can draw.
    pub(crate) fn set_look(&mut self, look: Look, art: ArtSet) {
        if look != Look::Sjk && self.phase != Phase::Writing {
            // Only the SJK UI's card waits for an answer.
            self.close();
        }
        self.look = look;
        self.art = art;
    }

    /// Follow the Shift key (`ModifiersChanged`).
    pub(crate) fn set_shift(&mut self, shift: bool) {
        self.shift = shift;
    }

    /// Follow the Control key (`ModifiersChanged`).
    pub(crate) fn set_control(&mut self, control: bool) {
        self.control = control;
    }

    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> Action {
        if event.state != ElementState::Pressed {
            return Action::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return Action::None;
        };
        self.key(key, event.text.as_deref())
    }

    /// A key pressed, with the `text` it types (Ctrl+V types `\u{16}`, which pastes).
    fn key(&mut self, key: KeyCode, text: Option<&str>) -> Action {
        if self.kind.is_none() {
            return Action::None;
        }
        if self.phase != Phase::Writing {
            return self.answered_key(key);
        }
        let shift = self.shift;
        match key {
            KeyCode::Escape => return self.close(),
            KeyCode::Tab => self.step_focus(shift),
            KeyCode::Enter | KeyCode::NumpadEnter => return self.press_focused(),
            KeyCode::Space if self.focus != Focus::Field => return self.press_focused(),
            KeyCode::Backspace if self.focus == Focus::Field => {
                let word = self.control;
                self.erase(if word { Motion::WordLeft } else { Motion::Left });
            }
            KeyCode::Delete if self.focus == Focus::Field => {
                let word = self.control;
                self.erase(if word {
                    Motion::WordRight
                } else {
                    Motion::Right
                });
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight if self.focus == Focus::Field => {
                let left = key == KeyCode::ArrowLeft;
                let motion = match (self.control, left) {
                    (false, true) => Motion::Left,
                    (false, false) => Motion::Right,
                    (true, true) => Motion::WordLeft,
                    (true, false) => Motion::WordRight,
                };
                self.edit.motion(&self.text, motion, false);
                self.moved();
            }
            KeyCode::ArrowUp | KeyCode::ArrowDown if self.focus == Focus::Field => {
                let at = self.edit.cursor(&self.text);
                let to = self
                    .layout
                    .vertical(&self.text, at, key == KeyCode::ArrowDown);
                self.edit.place(&self.text, to, false);
                self.moved();
            }
            KeyCode::Home | KeyCode::End if self.focus == Focus::Field => {
                let home = key == KeyCode::Home;
                let at = self.edit.cursor(&self.text);
                // Control takes the caret to the ends of the whole text.
                let to = match (self.control, home) {
                    (true, true) => 0,
                    (true, false) => self.text.len(),
                    _ => self.layout.edge(&self.text, at, home),
                };
                self.edit.place(&self.text, to, false);
                self.moved();
            }
            // The arrows walk the buttons as Tab does.
            KeyCode::ArrowLeft | KeyCode::ArrowUp => self.step_focus(true),
            KeyCode::ArrowRight | KeyCode::ArrowDown => self.step_focus(false),
            _ if self.focus == Focus::Field
                && crate::text_entry::is_select_all(key, text, self.control) =>
            {
                self.edit.select_all(&self.text);
                self.moved();
            }
            _ if self.focus == Focus::Field => {
                if crate::text_entry::is_copy(key, text, self.control) {
                    let _ = crate::console::clipboard::copy(&self.text);
                    return Action::None;
                }
                let pasted;
                let text = match text {
                    // Ctrl+V (Windows hands the letter without Ctrl) or Shift+Insert.
                    text if crate::sjk_chat_field::is_paste(
                        key,
                        text,
                        self.shift,
                        self.control,
                    ) =>
                    {
                        pasted = crate::console::clipboard::paste().unwrap_or_default();
                        pasted.as_str()
                    }
                    Some(text) => text,
                    None => return Action::None,
                };
                let kind = self.kind.as_ref().expect("open");
                let kept = typed(kind, &self.text, text);
                if !kept.is_empty() {
                    let limit = limit(kind);
                    self.edit
                        .insert_counted(&mut self.text, &kept, limit, |_| 1);
                    self.touched();
                }
            }
            _ => {}
        }
        Action::None
    }

    /// Delete from the caret to where `motion` goes (Backspace is `Left`, Delete `Right`).
    fn erase(&mut self, motion: Motion) {
        self.edit.delete(&mut self.text, motion);
        self.touched();
    }

    /// The text changed: the refusal no longer applies and the caret shows.
    fn touched(&mut self) {
        self.message.clear();
        self.layout.touch();
        self.moved();
    }

    /// The caret moved: it stays lit, as while typing.
    fn moved(&mut self) {
        self.epoch = Instant::now();
    }

    /// Enter (or Space on a button): what the focused control does.
    fn press_focused(&mut self) -> Action {
        match self.focus {
            Focus::Cancel => self.close(),
            Focus::ClearModel => Action::ClearModel,
            Focus::Field | Focus::Send => self.send(),
        }
    }

    /// Tab's order over the field and the buttons (a note's Clear model last, while it
    /// has a model), one step.
    fn step_focus(&mut self, back: bool) {
        let all = [Focus::Field, Focus::Send, Focus::Cancel, Focus::ClearModel];
        let count = if self.note_model().is_some() { 4 } else { 3 };
        let order = &all[..count];
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = order[if back { at + count - 1 } else { at + 1 } % count];
    }

    /// A key on the SJK UI's card after Send: Escape closes; Enter or Space takes the
    /// focused button (Close while sending, Done once sent, Edit or Close after a
    /// failure, between which Tab moves).
    fn answered_key(&mut self, key: KeyCode) -> Action {
        let failed = matches!(self.phase, Phase::Failed(_));
        match key {
            KeyCode::Escape => self.close(),
            KeyCode::Tab | KeyCode::ArrowLeft | KeyCode::ArrowRight if failed => {
                self.focus = if self.focus == Focus::Send {
                    Focus::Cancel
                } else {
                    Focus::Send
                };
                Action::None
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if failed && self.focus == Focus::Send {
                    self.edit();
                    Action::None
                } else {
                    self.close()
                }
            }
            _ => Action::None,
        }
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> Action {
        // A note's press on the scene round its card picks there.
        if let InputEvent::PointerPress {
            position,
            button: sjk_ui::PointerButton::Primary,
        } = event
            && matches!(self.kind, Some(Kind::Note { .. }))
            && self.phase == Phase::Writing
            && self.card.is_some_and(|card| !card.contains(position))
        {
            return Action::PickAt([position.x, position.y]);
        }
        let Some(event) = self.ui.pointer(event) else {
            return Action::None;
        };
        if event.kind != UiEventKind::Activate {
            return Action::None;
        }
        let writing = self.phase == Phase::Writing;
        match event.token {
            Some(FIELD_TOKEN) if writing => {
                self.focus = Focus::Field;
                // The caret goes where the click landed.
                if let Some(at) = event
                    .position
                    .and_then(|at| self.layout.at_point(&self.text, [at.x, at.y]))
                {
                    self.edit.place(&self.text, at, false);
                    self.moved();
                }
                Action::None
            }
            // A click on a failed report's text edits it.
            Some(FIELD_TOKEN | EDIT_TOKEN) if matches!(self.phase, Phase::Failed(_)) => {
                self.edit();
                Action::None
            }
            Some(SEND_TOKEN) if writing => self.send(),
            Some(CANCEL_TOKEN) if writing => self.close(),
            Some(CLEAR_MODEL_TOKEN) if writing && self.note_model().is_some() => Action::ClearModel,
            Some(CLOSE_TOKEN) => self.close(),
            _ => Action::None,
        }
    }

    /// The launcher's pointer: true when its button was clicked.
    pub(crate) fn launcher_pointer(&mut self, event: InputEvent) -> bool {
        self.launcher.pointer(event).is_some_and(|event| {
            event.kind == UiEventKind::Activate && event.token == Some(LAUNCH_TOKEN)
        })
    }

    /// Draw the Report a bug button centred at the bottom of the screen (the
    /// classic menus only; the SJK UI has it on its Sol JK page).
    pub(crate) fn append_launcher(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        self.append_launcher_classic(vertices, font, viewport);
    }

    pub(crate) fn launcher_draw_list(&self) -> &sjk_ui::DrawList {
        self.launcher.draw_list()
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Draw the dialog over the whole frame; text other overlays appended earlier this
    /// frame is dropped rather than shown through the panel. The SJK UI's card draws in
    /// `font` here; [`crate::GpuState::append_text_dialog`] gives it its families.
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        let Some(kind) = self.kind.clone() else {
            return;
        };
        vertices.clear();
        if self.look == Look::Sjk {
            self.append_sjk(
                crate::menu::sjk::TextTarget::Inter(vertices, font),
                viewport,
            );
            return;
        }
        self.append_classic(&kind, vertices, font, viewport);
    }

    /// Whether the dialog is drawn as the SJK UI's card.
    pub(crate) fn is_sjk(&self) -> bool {
        self.look == Look::Sjk
    }
}

impl crate::GpuState {
    /// Draw the open text dialog over the frame. The SJK UI's card darkens the whole
    /// frame, so every font batch appended before it is dropped (text draws above all
    /// shapes), and its own text goes to the UI's families once they are loaded.
    pub(crate) fn append_text_dialog(&mut self, viewport: [f32; 2]) {
        if self.text_dialog.is_sjk() {
            self.text_vertices.clear();
            self.classic_text_vertices.clear();
            self.game_fonts.clear_text();
            let target = crate::ingame_menu::sjk_view::text_target(
                &mut self.game_fonts,
                &mut self.text_vertices,
                &self.ui_font,
            );
            self.text_dialog.append_sjk(target, viewport);
        } else {
            let (vertices, font) = self.game_fonts.menu(&mut self.text_vertices, &self.ui_font);
            self.text_dialog.append(vertices, font, viewport);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_and_notes_keep_only_what_the_hub_accepts() {
        assert_eq!(
            typed(
                &Kind::Report,
                "",
                "Door <b>flickers</b>\n\u{1F642} \"left\"!"
            ),
            "Door bflickersb  left!"
        );
        let note = Kind::Note {
            subject: String::new(),
            model: None,
        };
        assert_eq!(typed(&note, "", "a <b> \"x\"\tz?"), "a b x z?");
        // Nothing past the limit.
        let full = "x".repeat(NOTE_MAX);
        assert_eq!(typed(&note, &full, "more"), "");
        assert_eq!(
            typed(&Kind::Report, "", &"word ".repeat(200))
                .chars()
                .count(),
            sjk_identity::report::TEXT_MAX
        );
    }

    #[test]
    fn long_text_wraps_at_spaces_and_cuts_long_words() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
        assert_eq!(wrap("abcdefghijklmnop", 8), ["abcdefgh", "ijklmnop"]);
        assert_eq!(wrap("", 10), [""]);
    }

    #[test]
    fn send_checks_the_text_and_escape_cancels() {
        let mut dialog = TextDialog::default();
        dialog.open(Kind::Report);
        dialog.text = "short".into();
        assert_eq!(dialog.send(), Action::None);
        assert!(dialog.is_open() && !dialog.message.is_empty());
        dialog.text = "The door flickers on ffa3".into();
        assert_eq!(
            dialog.send(),
            Action::Send(Kind::Report, "The door flickers on ffa3".into())
        );
        assert!(!dialog.is_open());
        dialog.open(Kind::Note {
            subject: "wall".into(),
            model: None,
        });
        assert_eq!(dialog.send(), Action::None, "an empty note waits");
        dialog.text = "too shiny".into();
        assert_eq!(
            dialog.send(),
            Action::Send(
                Kind::Note {
                    subject: "wall".into(),
                    model: None,
                },
                "too shiny".into()
            )
        );
        dialog.open(Kind::Note {
            subject: "wall".into(),
            model: None,
        });
        assert_eq!(dialog.close(), Action::Cancel);
        assert!(!dialog.is_open());
    }

    #[test]
    fn a_player_report_takes_a_few_words_up_to_its_limit() {
        let kind = Kind::PlayerReport {
            subject: "Troll: Griefing".into(),
            category: sjk_identity::Category::Griefing,
        };
        assert_eq!(
            typed(&kind, "", &"word ".repeat(100)).chars().count(),
            sjk_identity::report::PLAYER_MAX
        );
        let mut dialog = TextDialog::default();
        dialog.open(kind.clone());
        dialog.text = "troll".into();
        assert_eq!(dialog.send(), Action::None, "too short");
        dialog.text = "Team killing all match long".into();
        assert_eq!(
            dialog.send(),
            Action::Send(kind, "Team killing all match long".into())
        );
    }

    fn typed_keys(dialog: &mut TextDialog, text: &str) {
        for character in text.chars() {
            let mut buffer = [0; 4];
            dialog.key(KeyCode::KeyA, Some(character.encode_utf8(&mut buffer)));
        }
    }

    fn cursor(dialog: &TextDialog) -> usize {
        dialog.edit.cursor(&dialog.text)
    }

    /// Left, Right, Home, End, Delete and Backspace work at the caret, not at the end.
    #[test]
    fn the_caret_edits_in_the_middle_of_the_text() {
        let mut dialog = TextDialog::default();
        dialog.open(Kind::Report);
        typed_keys(&mut dialog, "hello world");
        assert_eq!(cursor(&dialog), 11);
        for _ in 0..5 {
            dialog.key(KeyCode::ArrowLeft, None);
        }
        assert_eq!(cursor(&dialog), 6);
        dialog.message = "a report needs a few real words".into();
        dialog.key(KeyCode::ArrowRight, None);
        dialog.key(KeyCode::ArrowLeft, None);
        assert!(!dialog.message.is_empty(), "moving is not editing");
        typed_keys(&mut dialog, "X");
        assert_eq!(dialog.text, "hello Xworld");
        assert!(dialog.message.is_empty(), "typing clears the refusal");
        assert_eq!(cursor(&dialog), 7);
        dialog.key(KeyCode::Backspace, None);
        assert_eq!(dialog.text, "hello world");
        dialog.key(KeyCode::Delete, None);
        assert_eq!(dialog.text, "hello orld");
        assert_eq!(cursor(&dialog), 6);
        dialog.key(KeyCode::Home, None);
        assert_eq!(cursor(&dialog), 0);
        dialog.key(KeyCode::Backspace, None);
        dialog.key(KeyCode::ArrowLeft, None);
        assert_eq!((dialog.text.as_str(), cursor(&dialog)), ("hello orld", 0));
        typed_keys(&mut dialog, "Oh ");
        assert_eq!(dialog.text, "Oh hello orld");
        dialog.key(KeyCode::End, None);
        assert_eq!(cursor(&dialog), dialog.text.len());
        dialog.key(KeyCode::Delete, None);
        dialog.key(KeyCode::ArrowRight, None);
        assert_eq!(dialog.text, "Oh hello orld");
        dialog.key(KeyCode::ArrowLeft, None);
        typed_keys(&mut dialog, "!!");
        assert_eq!(dialog.text, "Oh hello orl!!d");
    }

    #[test]
    fn control_moves_and_deletes_by_words() {
        let mut dialog = TextDialog::default();
        dialog.open(Kind::Report);
        typed_keys(&mut dialog, "one two three");
        dialog.set_control(true);
        dialog.key(KeyCode::ArrowLeft, None);
        assert_eq!(cursor(&dialog), 8);
        dialog.key(KeyCode::Backspace, None);
        assert_eq!(dialog.text, "one three");
        assert_eq!(cursor(&dialog), 4);
        dialog.key(KeyCode::Delete, None);
        assert_eq!(dialog.text, "one ");
        dialog.key(KeyCode::ArrowLeft, None);
        assert_eq!(cursor(&dialog), 0);
        dialog.key(KeyCode::ArrowRight, None);
        assert_eq!(cursor(&dialog), 4);
        dialog.key(KeyCode::Home, None);
        assert_eq!(cursor(&dialog), 0);
        dialog.key(KeyCode::End, None);
        assert_eq!(cursor(&dialog), 4);
    }

    /// The limits are the same wherever the caret is, and the text is still filtered.
    #[test]
    fn inserting_keeps_the_text_limits() {
        let mut dialog = TextDialog::default();
        dialog.open(Kind::Report);
        let full = "a".repeat(sjk_identity::report::TEXT_MAX);
        dialog.key(KeyCode::KeyA, Some(&full));
        assert_eq!(dialog.text.chars().count(), sjk_identity::report::TEXT_MAX);
        dialog.key(KeyCode::Home, None);
        typed_keys(&mut dialog, "b");
        assert_eq!(dialog.text, full, "no room");
        for _ in 0..4 {
            dialog.key(KeyCode::Delete, None);
        }
        // Pasted: what the hub refuses is dropped, a line break is a space.
        dialog.key(KeyCode::KeyV, Some("<b>\n\u{1F642}ö"));
        assert_eq!(
            dialog.text.chars().count(),
            sjk_identity::report::TEXT_MAX - 1
        );
        assert!(dialog.text.starts_with("b ö"), "{:?}", &dialog.text[..8]);
        assert_eq!(cursor(&dialog), "b ö".len());
        typed_keys(&mut dialog, "zz");
        assert_eq!(dialog.text.chars().count(), sjk_identity::report::TEXT_MAX);
        assert!(dialog.text.starts_with("b özaaa"));
        // At the start, Backspace has nothing to remove; none of it can panic.
        dialog.key(KeyCode::Home, None);
        dialog.key(KeyCode::Backspace, None);
        dialog.key(KeyCode::ArrowLeft, None);
        dialog.key(KeyCode::ArrowUp, None);
        assert_eq!(cursor(&dialog), 0);
    }

    /// The arrows walk Send and Cancel when the field does not have the keyboard.
    #[test]
    fn the_arrows_walk_the_buttons() {
        let mut dialog = TextDialog::default();
        dialog.open(Kind::Report);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::Send);
        dialog.key(KeyCode::ArrowRight, None);
        assert_eq!(dialog.focus, Focus::Cancel);
        dialog.key(KeyCode::ArrowLeft, None);
        assert_eq!(dialog.focus, Focus::Send);
        dialog.key(KeyCode::ArrowUp, None);
        assert_eq!(dialog.focus, Focus::Field);
    }

    fn body_font() -> crate::text::FontAtlas {
        crate::text::load_family(&crate::text::BODY, 1.0, None).expect("a bundled family")
    }

    const SAMPLE: &str = "The door by the tower's foot flickers when I walk through it, and the light behind it goes black for a second. It happens every time on this server (ffa3).";

    /// A caret position's x is the renderer's own measure of the line before it, in every
    /// text style, on every wrapped line; and no line is wider than the box.
    #[test]
    fn the_caret_sits_between_glyphs_in_any_text_style() {
        use crate::text::{TextFace, TextStyle, visible_text_width_style};
        let atlas = body_font();
        let font = &atlas.font;
        for (scale, tracking) in [
            (1.0, 0.0),
            (1.2, 0.0),
            (0.8, 0.0),
            (1.0, 0.15),
            (1.2, -0.05),
        ] {
            let style = TextStyle { scale, tracking };
            for spacing in [0.0, 0.3] {
                let face = field::Face::new(font, style, 19.0, spacing);
                let mut layout = FieldLayout::default();
                layout.lay_out(SAMPLE, &face, 400.0);
                layout.show(0, 100, [0.0, 0.0], 30.0);
                let (_, lines) = layout.visible();
                assert!(lines.len() > 2, "the sample wraps");
                let styled = 19.0 * scale;
                for line in lines {
                    let width = visible_text_width_style(
                        font,
                        &SAMPLE[line.clone()],
                        styled / font.height,
                        TextFace::Regular,
                        spacing + tracking * styled,
                    );
                    assert!(width <= 400.0 - styled * 0.5, "{scale} {tracking}: {width}");
                }
                let lines = lines.to_vec();
                for (at, _) in SAMPLE.char_indices().chain([(SAMPLE.len(), ' ')]) {
                    let (line, x) = layout.locate(SAMPLE, at);
                    let range = &lines[line];
                    assert!(range.start <= at && at <= range.end, "{at} on {range:?}");
                    let expected = visible_text_width_style(
                        font,
                        &SAMPLE[range.start..at],
                        styled / font.height,
                        TextFace::Regular,
                        spacing + tracking * styled,
                    );
                    assert!((x - expected).abs() < 0.01, "{at}: {x} against {expected}");
                }
            }
        }
    }

    /// Up and Down keep the column across wrapped lines; Home and End stay on the line;
    /// a click lands on the glyph under it.
    #[test]
    fn the_caret_moves_over_wrapped_lines_and_to_clicks() {
        let atlas = body_font();
        let face = field::Face::new(&atlas.font, crate::text::TextStyle::NEUTRAL, 19.0, 0.0);
        let mut layout = FieldLayout::default();
        layout.lay_out(SAMPLE, &face, 400.0);
        layout.show(0, 100, [50.0, 100.0], 30.0);
        let (_, lines) = layout.visible();
        let lines = lines.to_vec();
        let second = lines[1].clone();
        let in_second = second.start + 7;
        let (line, x) = layout.locate(SAMPLE, in_second);
        assert_eq!(line, 1);
        // Up lands in the first line at about the same x, Down in the third.
        let up = layout.vertical(SAMPLE, in_second, false);
        let (up_line, up_x) = layout.locate(SAMPLE, up);
        assert_eq!(up_line, 0);
        assert!((up_x - x).abs() < 12.0, "{up_x} against {x}");
        let down = layout.vertical(SAMPLE, in_second, true);
        assert_eq!(layout.locate(SAMPLE, down).0, 2);
        // From the first line, Up goes to the start; from the last, Down to the end.
        assert_eq!(layout.vertical(SAMPLE, 3, false), 0);
        assert_eq!(
            layout.vertical(SAMPLE, SAMPLE.len() - 2, true),
            SAMPLE.len()
        );
        // Home and End stay on the line.
        assert_eq!(layout.edge(SAMPLE, in_second, true), second.start);
        assert_eq!(layout.edge(SAMPLE, in_second, false), second.end);
        // A click a little left of the middle of a glyph is before it; left of the line, at
        // its start; below the last row, at the end of the last line.
        let stop = |index: usize| layout.locate(SAMPLE, index).1;
        let target = second.start + 12;
        let point = [
            50.0 + (stop(target) + stop(target + 1)) * 0.5 - 0.5,
            100.0 + 30.0 + 10.0,
        ];
        assert_eq!(layout.at_point(SAMPLE, point), Some(target));
        assert_eq!(
            layout.at_point(SAMPLE, [0.0, 100.0 + 30.0]),
            Some(second.start)
        );
        let last = lines.last().expect("lines").clone();
        assert_eq!(layout.at_point(SAMPLE, [5000.0, 5000.0]), Some(last.end));
        // A changed text waits for the next layout.
        layout.touch();
        assert_eq!(layout.at_point(SAMPLE, point), None);
        assert_eq!(layout.edge(SAMPLE, in_second, true), 0);
    }

    fn click_at(dialog: &mut TextDialog, point: sjk_ui::Vec2) -> Action {
        let button = sjk_ui::PointerButton::Primary;
        dialog.handle_pointer(InputEvent::PointerMove(point));
        dialog.handle_pointer(InputEvent::PointerPress {
            position: point,
            button,
        });
        dialog.handle_pointer(InputEvent::PointerRelease {
            position: point,
            button,
        })
    }

    fn click_token(dialog: &mut TextDialog, token: u16) -> Action {
        let rect = dialog.ui.rect_for(token).expect("drawn");
        click_at(
            dialog,
            sjk_ui::Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
        )
    }

    /// The classic look: the field, Send and Cancel answer to the pointer, a refused Send
    /// says why, and the caret is a bar at the insertion point in the player's text style.
    #[test]
    fn the_classic_look_answers_the_pointer_and_draws_the_caret() {
        use crate::text::{TextFace, TextStyle, visible_text_width_style};
        let mut atlas = crate::text::load_modern(1.0, None).expect("Inter");
        let style = TextStyle {
            scale: 1.2,
            tracking: 0.1,
        };
        atlas.font.set_style(style);
        let font = &atlas.font;
        let viewport = [1920.0, 1080.0];
        let mut dialog = TextDialog::default();
        dialog.set_look(Look::Classic, ArtSet::default());
        dialog.open(Kind::Report);
        let mut vertices = Vec::new();
        let mut draw = |dialog: &mut TextDialog| {
            vertices.clear();
            dialog.caret_for_shot();
            dialog.append_classic(&Kind::Report, &mut vertices, font, viewport);
        };
        draw(&mut dialog);
        for token in [FIELD_TOKEN, SEND_TOKEN, CANCEL_TOKEN] {
            assert!(dialog.ui.rect_for(token).is_some(), "{token}");
        }
        // A refused Send is explained, and the field takes the keyboard back.
        dialog.focus = Focus::Cancel;
        assert_eq!(click_token(&mut dialog, SEND_TOKEN), Action::None);
        assert!(!dialog.message.is_empty() && dialog.focus == Focus::Field);
        typed_keys(&mut dialog, SAMPLE);
        for _ in 0..40 {
            dialog.key(KeyCode::ArrowLeft, None);
        }
        let at = cursor(&dialog);
        assert_eq!(at, SAMPLE.len() - 40);
        draw(&mut dialog);
        // The caret bar is the gold solid rectangle.
        let bar = dialog
            .ui
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                sjk_ui::DrawCommand::SolidRect { rect, color }
                    if *color == crate::menu::classic::view::GOLD =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("the caret is lit");
        let place = crate::menu::classic::layout::Placement::new(viewport);
        let size = 11.0 * place.scale * style.scale;
        let (line, _) = dialog.layout.locate(&dialog.text, at);
        let (first, shown) = dialog.layout.visible();
        assert!(line >= first && line < first + shown.len());
        let start = shown[line - first].start;
        let before = visible_text_width_style(
            font,
            &dialog.text[start..at],
            size / font.height,
            TextFace::Regular,
            0.3 * place.scale + style.tracking * size,
        );
        let left = place.rect([106.0, 154.0, 0.0, 0.0]).x;
        let centre = bar.x + bar.width * 0.5;
        assert!(
            (centre - (left + before)).abs() < 0.01,
            "{centre} {left} {before}"
        );
        // Clicking in the field puts the caret there.
        let rect = dialog.ui.rect_for(FIELD_TOKEN).expect("drawn");
        click_at(&mut dialog, sjk_ui::Vec2::new(rect.x + 3.0, rect.y + 3.0));
        assert_eq!(cursor(&dialog), 0, "left of the first line");
        assert_eq!(click_token(&mut dialog, CANCEL_TOKEN), Action::Cancel);
        assert!(!dialog.is_open());
    }
}

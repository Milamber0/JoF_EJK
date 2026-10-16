//! The text dialog in the SJK UI (`docs/sjk-ui.md`, "Report a bug and its dialogs"):
//! a pop-up card over the darkened scene, as Update's and the browser's prompts. What
//! it is (gold) and where it goes along its top, the question as its headline over a
//! line or two of guidance, the text in a tall field with a gold caret, the rules and
//! the count of characters under it, why a Send was refused in gold, then Cancel and
//! Send (gold, dimmed until the text would go) along its foot; the keys sit bottom
//! right. After Send a report's card says "Sending...", then what the hub stored it as
//! or why it did not go, with Edit to change the text and send it again.
//!
//! The state, keys and pointer tokens are the dialog's own ([`super`]); only the
//! drawing is this look's. Everything is laid out on the SJK UI's 16:9 frame
//! ([`Frame`]) and drawn with its kit.

use super::field::{Face, FieldLayout};
use super::{
    CANCEL_TOKEN, CLEAR_MODEL_TOKEN, CLOSE_TOKEN, EDIT_TOKEN, FIELD_TOKEN, Focus, Kind, LINES,
    NOTE_LINES, Phase, SEND_TOKEN, TextDialog,
};
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::text::{TextStyle, UiFont};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// The card: centred on the frame, as tall as its parts.
const CARD_WIDTH: f32 = 920.0;
const CARD_X: f32 = (1920.0 - CARD_WIDTH) * 0.5;
const CARD_HEIGHT: f32 = 592.0;
const CARD_TOP: f32 = (1080.0 - CARD_HEIGHT) * 0.5;
/// The card's inner margin, and its text's column.
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
/// What it is and where it goes, the headline, and the guidance's lines.
const EYEBROW_Y: f32 = CARD_TOP + 30.0;
const HEADLINE_Y: f32 = CARD_TOP + 70.0;
const DETAIL_Y: f32 = CARD_TOP + 132.0;
const DETAIL_LINE: f32 = 28.0;
const DETAIL_LINES: usize = 2;
/// Characters of the guidance a line holds (Exo 2 at 18 averages about 7
/// pixels a character of running text).
const DETAIL_CHARS: usize = 108;
/// The text's field: its top, inner padding and line step.
const FIELD_TOP: f32 = DETAIL_Y + DETAIL_LINES as f32 * DETAIL_LINE + 12.0;
const FIELD_PAD_X: f32 = 22.0;
const FIELD_PAD_Y: f32 = 16.0;
const LINE: f32 = 30.0;
const FIELD_HEIGHT: f32 = LINES as f32 * LINE + FIELD_PAD_Y * 2.0;
/// The rules and the count under the field, then why a Send was refused.
const INFO_Y: f32 = FIELD_TOP + FIELD_HEIGHT + 12.0;
const MESSAGE_Y: f32 = INFO_Y + 28.0;
/// The buttons along the foot.
const BUTTONS_Y: f32 = CARD_TOP + CARD_HEIGHT - MARGIN - BUTTON_HEIGHT;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
/// The keys' line, as on the other cards.
const KEYS_Y: f32 = 992.0;
/// Text sizes: the headline, the guidance, the text.
const HEADLINE_SIZE: f32 = 40.0;
const TEXT_SIZE: f32 = 19.0;
/// What the scene keeps of its light under a note's card, whose selection should
/// still show round it (a report's card takes [`kit::scrim`]'s).
const NOTE_SCRIM: f32 = 0.5;
/// A note's model row, between its guidance and its field: the model and Clear model,
/// or how to pick one. Its button's height.
const MODEL_ROW: f32 = 48.0;
const MODEL_BUTTON_HEIGHT: f32 = 36.0;
/// Why a Send was refused: warm red, as retail's warnings are.
const WARN: Color = Color::new(1.0, 0.44, 0.4, 1.0);
/// The rules every kind keeps to as it is typed.
const RULES: &str = "Letters, digits, spaces and . , ! ? ' - : ( ) only";

/// Where the card's parts sit for one kind. A report's card is the constants' own; a
/// note's is taller, for its model row and a field of [`NOTE_LINES`], and stays centred.
#[derive(Clone, Copy, Debug)]
struct Geometry {
    top: f32,
    height: f32,
    lines: usize,
    model_row: bool,
}

impl Geometry {
    fn of(kind: &Kind) -> Self {
        let note = matches!(kind, Kind::Note { .. });
        let lines = if note { NOTE_LINES } else { LINES };
        let height =
            CARD_HEIGHT + (lines - LINES) as f32 * LINE + if note { MODEL_ROW } else { 0.0 };
        Self {
            top: (1080.0 - height) * 0.5,
            height,
            lines,
            model_row: note,
        }
    }
    fn shift(self) -> f32 {
        self.top - CARD_TOP
    }
    fn eyebrow_y(self) -> f32 {
        EYEBROW_Y + self.shift()
    }
    fn headline_y(self) -> f32 {
        HEADLINE_Y + self.shift()
    }
    fn detail_y(self) -> f32 {
        DETAIL_Y + self.shift()
    }
    /// The model row's top, where a report's field starts.
    fn model_y(self) -> f32 {
        FIELD_TOP + self.shift()
    }
    fn field_top(self) -> f32 {
        self.model_y() + if self.model_row { MODEL_ROW } else { 0.0 }
    }
    fn field_height(self) -> f32 {
        self.lines as f32 * LINE + FIELD_PAD_Y * 2.0
    }
    fn info_y(self) -> f32 {
        self.field_top() + self.field_height() + 12.0
    }
    fn message_y(self) -> f32 {
        self.info_y() + 28.0
    }
    fn buttons_y(self) -> f32 {
        self.top + self.height - MARGIN - BUTTON_HEIGHT
    }
}

/// What the card shows this frame, borrowed from the dialog.
struct View<'a> {
    kind: &'a Kind,
    /// Where its parts sit.
    geometry: Geometry,
    text: &'a str,
    focus: Focus,
    message: &'a str,
    phase: &'a Phase,
    /// The insertion point, a byte offset in `text`.
    cursor: usize,
    /// The caret is lit this frame.
    caret: bool,
    /// The text would be accepted (Send is gold).
    ready: bool,
}

/// `text` with its first letter a capital and a full stop after it unless it ends in
/// one already, for a reason given in lower case ("the SJK identity is off").
pub(super) struct Sentence<'a>(pub(super) &'a str);

impl std::fmt::Display for Sentence<'_> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut chars = self.0.chars();
        if let Some(first) = chars.next() {
            write!(out, "{}{}", first.to_uppercase(), chars.as_str())?;
        }
        if !self.0.ends_with(['.', '!', '?', ')']) {
            out.write_str(".")?;
        }
        Ok(())
    }
}

impl TextDialog {
    /// Draw the SJK UI's card over the whole frame, its text to `target`.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        // The fonts outlive the target's vertex lists: measure with them, then append.
        // The text is drawn in the player's menu text style, which the field measures with.
        let (display, body, style) = match &target {
            TextTarget::Families(fonts, style) => (fonts.display.1, fonts.body.1, *style),
            TextTarget::Inter(_, font) => (*font, *font, font.style()),
        };
        self.build_sjk(display, body, style, viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the card out on the dialog's canvas, measuring with the `display` and
    /// `body` families' metrics, the body's text drawn in `style`.
    fn build_sjk(&mut self, display: &UiFont, body: &UiFont, style: TextStyle, viewport: [f32; 2]) {
        let Self {
            kind: Some(kind),
            text,
            edit,
            layout,
            focus,
            message,
            ui,
            epoch,
            phase,
            card,
            ..
        } = self
        else {
            return;
        };
        let geometry = Geometry::of(kind);
        let view = View {
            kind,
            geometry,
            text,
            focus: *focus,
            message,
            phase,
            cursor: edit.cursor(text),
            caret: *focus == Focus::Field
                && *phase == Phase::Writing
                && (epoch.elapsed().as_millis() / 500).is_multiple_of(2),
            ready: super::refusal(kind, text).is_none(),
        };
        let frame = Frame::new(viewport);
        ui.begin_transparent(viewport);
        if matches!(kind, Kind::Note { .. }) {
            let _ = ui.draw_list_mut().push(DrawCommand::SolidRect {
                rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
                color: color::alpha(color::SPACE, NOTE_SCRIM),
            });
        } else {
            kit::scrim(ui, viewport);
        }
        kit::card(
            ui,
            &frame,
            [CARD_X, geometry.top, CARD_WIDTH, geometry.height],
        );
        *card = Some(frame.rect(CARD_X, geometry.top, CARD_WIDTH, geometry.height));
        heading(ui, &frame, display, &view);
        if geometry.model_row {
            model_row(ui, &frame, body, &view);
        }
        field(ui, &frame, body, style, layout, &view);
        if *phase == Phase::Writing {
            info(ui, &frame, &view);
        }
        foot(ui, &frame, &view);
        keys(ui, &frame, &view);
        ui.finish(match (view.phase, view.focus) {
            (Phase::Writing, Focus::Field) => FIELD_TOKEN,
            (Phase::Writing, Focus::Send) => SEND_TOKEN,
            (Phase::Writing, Focus::Cancel) => CANCEL_TOKEN,
            (Phase::Writing, Focus::ClearModel) => CLEAR_MODEL_TOKEN,
            (Phase::Failed(_), Focus::Send) => EDIT_TOKEN,
            _ => CLOSE_TOKEN,
        });
    }
}

/// What the card is, where it goes, its headline and its guidance (or the answer).
fn heading(ui: &mut MenuCanvas, frame: &Frame, display: &UiFont, view: &View<'_>) {
    let s = frame.s;
    let (title, bound) = match view.kind {
        Kind::Report => ("Report a bug", "To the SJK team, signed with your identity"),
        Kind::Note { .. } => ("Note for Claude", "Kept on this PC with a screenshot"),
        Kind::PlayerReport { .. } => (
            "Report a player",
            "To the SJK team, signed with your identity",
        ),
    };
    text(
        ui,
        TextFamily::Display,
        format_args!("{title}"),
        frame.rect(TEXT_X, view.geometry.eyebrow_y(), TEXT_WIDTH, 30.0),
        22.0 * s,
        color::GOLD_BRIGHT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    text(
        ui,
        TextFamily::Body,
        format_args!("{bound}"),
        frame.rect(TEXT_X, view.geometry.eyebrow_y() + 3.0, TEXT_WIDTH, 26.0),
        16.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::End,
    );
    let headline = match (view.phase, view.kind) {
        (Phase::Writing, Kind::Report) => "What went wrong?",
        (Phase::Writing, Kind::Note { .. }) => "What should change here?",
        (Phase::Writing, Kind::PlayerReport { subject, .. }) => subject.as_str(),
        (Phase::Sending, _) => "Sending...",
        (Phase::Sent(_), _) => "Sent. Thank you!",
        (Phase::Failed(_), _) => "Not sent",
    };
    let (shown, cut) = fit(display, headline, HEADLINE_SIZE * s, TEXT_WIDTH * s);
    text(
        ui,
        TextFamily::Display,
        format_args!("{shown}{}", if cut { "..." } else { "" }),
        frame.rect(TEXT_X, view.geometry.headline_y(), TEXT_WIDTH, 52.0),
        HEADLINE_SIZE * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    let mut detail = |line: usize, value: &dyn std::fmt::Display| {
        text(
            ui,
            TextFamily::Body,
            format_args!("{value}"),
            frame.rect(
                TEXT_X,
                view.geometry.detail_y() + line as f32 * DETAIL_LINE,
                TEXT_WIDTH,
                DETAIL_LINE,
            ),
            18.0 * s,
            color::alpha(color::TEXT, 0.86),
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    let guidance = match (view.phase, view.kind) {
        (Phase::Writing, Kind::Report) => {
            "Where were you, what did you do, and what did you expect? The map, the build and the server you are on go with it."
        }
        (Phase::Writing, Kind::Note { subject, .. }) => subject.as_str(),
        (Phase::Writing, Kind::PlayerReport { .. }) => {
            "What did they do? The report names them with the server, the map and the match clock."
        }
        (Phase::Sending, _) => {
            "Your report is on its way to the SJK team. You can close this card: the answer then shows on screen."
        }
        (Phase::Sent(stored), _) => {
            detail(0, &format_args!("The SJK team has it as {stored}."));
            return;
        }
        (Phase::Failed(why), _) => {
            detail(0, &Sentence(why));
            return;
        }
    };
    for (line, part) in wrap(guidance, DETAIL_CHARS).take(DETAIL_LINES).enumerate() {
        detail(line, &part);
    }
}

/// A note's model row: the model it is about with Clear model at the right, or how to
/// pick one with the pointer.
fn model_row(ui: &mut MenuCanvas, frame: &Frame, body: &UiFont, view: &View<'_>) {
    let s = frame.s;
    let Kind::Note { model, .. } = view.kind else {
        return;
    };
    let top = view.geometry.model_y();
    let middle = top + (MODEL_ROW - 12.0) * 0.5;
    let writing = *view.phase == Phase::Writing;
    let Some(path) = model else {
        text(
            ui,
            TextFamily::Body,
            format_args!("Click the scene round this card to note what is there, a model too"),
            frame.rect(TEXT_X, middle - 12.0, TEXT_WIDTH, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        return;
    };
    let label = "Clear model";
    let width = button_width(label);
    text(
        ui,
        TextFamily::Display,
        format_args!("MODEL"),
        frame.rect(TEXT_X, middle - 12.0, 80.0, 24.0),
        17.0 * s,
        color::GOLD_BRIGHT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    let room = TEXT_WIDTH - 80.0 - width - BUTTON_GAP;
    let (shown, cut) = fit(body, path, 17.0 * s, room * s);
    text(
        ui,
        TextFamily::Body,
        format_args!("{shown}{}", if cut { "..." } else { "" }),
        frame.rect(TEXT_X + 80.0, middle - 12.0, room, 24.0),
        17.0 * s,
        color::TEXT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    kit::button(
        ui,
        frame,
        [
            TEXT_X + TEXT_WIDTH - width,
            middle - MODEL_BUTTON_HEIGHT * 0.5,
            width,
            MODEL_BUTTON_HEIGHT,
        ],
        label,
        false,
        writing,
        view.focus == Focus::ClearModel,
        CLEAR_MODEL_TOKEN,
    );
}

/// The longest start of `value` that fits `width` pixels at `size` in `font`, and
/// whether it was cut (an ellipsis then follows it, which the width leaves room for).
fn fit<'t>(font: &UiFont, value: &'t str, size: f32, width: f32) -> (&'t str, bool) {
    let scale = size / font.height.max(1.0);
    let measure = |part: &str| crate::text::visible_text_width(font, part, scale);
    if measure(value) <= width {
        return (value, false);
    }
    let room = width - measure("...");
    let mut shown = value;
    while let Some((last, _)) = shown.char_indices().next_back() {
        shown = &shown[..last];
        if measure(shown) <= room {
            break;
        }
    }
    (shown.trim_end(), true)
}

/// The text's field: its lines (scrolled to keep the caret in view), the gold caret at the
/// insertion point while it has the keyboard, "Type here" while it is empty; dimmed after
/// Send. The wrap, the caret and the clicks measure with the player's text style.
fn field(
    ui: &mut MenuCanvas,
    frame: &Frame,
    body: &UiFont,
    style: TextStyle,
    layout: &mut FieldLayout,
    view: &View<'_>,
) {
    let s = frame.s;
    let writing = *view.phase == Phase::Writing;
    let focused = writing && view.focus == Focus::Field;
    let hovered = ui.token_hovered(FIELD_TOKEN);
    let geometry = view.geometry;
    let rect = frame.rect(
        TEXT_X,
        geometry.field_top(),
        TEXT_WIDTH,
        geometry.field_height(),
    );
    let _ = ui.draw_list_mut().push(DrawCommand::RoundedRect {
        rect,
        radius: 14.0 * s,
        color: color::alpha(color::SPACE, 0.6),
    });
    let edge = match (focused, writing && hovered) {
        (true, _) => Color::new(1.0, 1.0, 1.0, 0.7),
        (false, true) => color::alpha(color::HOLO, 0.6),
        _ if writing => color::alpha(color::HOLO, 0.4),
        _ => color::alpha(color::HOLO, 0.2),
    };
    let _ = ui.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius: 14.0 * s,
        width: 1.5 * s,
        color: edge,
    });
    let size = TEXT_SIZE * s;
    let left = TEXT_X + FIELD_PAD_X;
    let inner = TEXT_WIDTH - FIELD_PAD_X * 2.0;
    let row = |line: usize| {
        frame.rect(
            left,
            geometry.field_top() + FIELD_PAD_Y + line as f32 * LINE,
            inner,
            LINE,
        )
    };
    let ink = if writing {
        color::TEXT
    } else {
        color::alpha(color::TEXT, 0.6)
    };
    let face = Face::new(body, style, size, 0.0);
    layout.lay_out(view.text, &face, inner * s);
    let (caret_line, caret_x) = layout.locate(view.text, view.cursor);
    let top = row(0);
    layout.show(caret_line, geometry.lines, [top.x, top.y], row(1).y - top.y);
    let (first, shown) = layout.visible();
    if view.text.is_empty() {
        // Held clear of the caret whenever the field has the keyboard, so it does not
        // jump as the caret blinks.
        let mut rect = row(0);
        if focused {
            rect.x += 10.0 * s;
        }
        text(
            ui,
            TextFamily::Body,
            format_args!("Type here"),
            rect,
            size,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    for (line, range) in shown.iter().enumerate() {
        text(
            ui,
            TextFamily::Body,
            format_args!("{}", &view.text[range.clone()]),
            row(line),
            size,
            ink,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    if view.caret && caret_line >= first {
        let line_rect = row(caret_line - first);
        let height = (24.0 * style.scale).min(LINE - 2.0) * s;
        let _ = ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: caret_rect(line_rect, caret_x, height, s),
            color: color::GOLD_BRIGHT,
        });
    }
    // A click chooses the field while writing, and edits a failed report.
    if writing || matches!(view.phase, Phase::Failed(_)) {
        ui.hit_region(FIELD_TOKEN, rect);
    }
}

/// The caret's bar, centred on the boundary `x` pixels into `line`, `height` high.
fn caret_rect(line: Rect, x: f32, height: f32, s: f32) -> Rect {
    Rect::new(
        line.x + x - 1.0 * s,
        line.y + (line.height - height) * 0.5,
        2.0 * s,
        height,
    )
}

/// Under the field while writing: the rules, the count against the limit (gold at
/// it), and why the last Send was refused.
fn info(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let s = frame.s;
    text(
        ui,
        TextFamily::Body,
        format_args!("{RULES}"),
        frame.rect(
            TEXT_X + 4.0,
            view.geometry.info_y(),
            TEXT_WIDTH - 160.0,
            24.0,
        ),
        15.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let count = view.text.chars().count();
    let limit = super::limit(view.kind);
    text(
        ui,
        TextFamily::Display,
        format_args!("{count} / {limit}"),
        frame.rect(
            TEXT_X + TEXT_WIDTH - 150.0,
            view.geometry.info_y(),
            146.0,
            24.0,
        ),
        18.0 * s,
        if count >= limit {
            color::GOLD_BRIGHT
        } else {
            color::MUTED
        },
        FontWeight::Regular,
        TextAlign::End,
    );
    // Why a Send was refused, or what it would be refused for while the pointer or the
    // keyboard is on Send, in a warm band just above the buttons.
    let previewing = !view.ready && (view.focus == Focus::Send || ui.token_hovered(SEND_TOKEN));
    let reason = if !view.message.is_empty() {
        Some(view.message)
    } else if previewing {
        super::refusal(view.kind, view.text)
    } else {
        None
    };
    if let Some(reason) = reason {
        let message_y = view.geometry.message_y();
        let band = frame.rect(TEXT_X, message_y - 2.0, TEXT_WIDTH, 28.0);
        let _ = ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: band,
            radius: 8.0 * s,
            color: color::alpha(WARN, 0.14),
        });
        text(
            ui,
            TextFamily::Body,
            format_args!("{}", Sentence(reason)),
            frame.rect(TEXT_X + 12.0, message_y, TEXT_WIDTH - 24.0, 24.0),
            16.0 * s,
            WARN,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
}

/// A button's width for `label`: Rajdhani at 20 is about 9.5 pixels a character.
fn button_width(label: &str) -> f32 {
    (48.0 + 9.5 * label.chars().count() as f32).max(150.0)
}

/// The foot: on the left how a sent report stands, on the right the buttons (the one
/// Enter takes, gold, at the right end).
fn foot(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let s = frame.s;
    let buttons_y = view.geometry.buttons_y();
    let middle = buttons_y + BUTTON_HEIGHT * 0.5;
    let status = |ui: &mut MenuCanvas, x: f32, value: &str, colour: Color| {
        text(
            ui,
            TextFamily::Body,
            format_args!("{value}"),
            frame.rect(x, middle - 13.0, 420.0, 26.0),
            16.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    match view.phase {
        Phase::Writing => {}
        Phase::Sending => {
            let seconds = crate::menu::art::motion::seconds();
            crate::menu::sjk::loading::activity(ui, frame, TEXT_X + 10.0, middle, seconds);
            status(ui, TEXT_X + 32.0, "Waiting for the SJK hub", color::MUTED);
        }
        Phase::Sent(_) => {
            let right = kit::tick(ui, frame, TEXT_X, middle, true, false);
            status(
                ui,
                right + 14.0,
                "Stored by the SJK hub",
                color::GOLD_BRIGHT,
            );
        }
        Phase::Failed(_) => {
            status(
                ui,
                TEXT_X,
                "Your text is kept: edit it and send again",
                color::MUTED,
            );
        }
    }
    let button = |label, primary, enabled, focus: Focus, token| {
        Some(Button {
            label,
            primary,
            enabled,
            focused: view.focus == focus,
            token,
        })
    };
    // From the right end.
    let buttons = match view.phase {
        Phase::Writing => [
            button("Send", true, view.ready, Focus::Send, SEND_TOKEN),
            button("Cancel", false, true, Focus::Cancel, CANCEL_TOKEN),
        ],
        Phase::Sending => [button("Close", false, true, view.focus, CLOSE_TOKEN), None],
        Phase::Sent(_) => [button("Done", true, true, view.focus, CLOSE_TOKEN), None],
        Phase::Failed(_) => [
            button("Edit", true, true, Focus::Send, EDIT_TOKEN),
            button("Close", false, true, Focus::Cancel, CLOSE_TOKEN),
        ],
    };
    let mut right = TEXT_X + TEXT_WIDTH;
    for button in buttons.into_iter().flatten() {
        let width = button_width(button.label);
        right -= width;
        kit::button(
            ui,
            frame,
            [right, buttons_y, width, BUTTON_HEIGHT],
            button.label,
            button.primary,
            button.enabled,
            button.focused,
            button.token,
        );
        right -= BUTTON_GAP;
    }
}

/// One button along the foot: gold when it is the one Enter takes (`primary`).
struct Button {
    label: &'static str,
    primary: bool,
    enabled: bool,
    focused: bool,
    token: u16,
}

/// The keys of what has the keyboard, right-aligned under the card.
fn keys(ui: &mut MenuCanvas, frame: &Frame, view: &View<'_>) {
    let s = frame.s;
    let mut keys: [Option<(&[&str], &str)>; 4] = [None; 4];
    match (view.phase, view.focus) {
        (Phase::Writing, focus) => {
            keys[0] = Some((
                &["Enter"][..],
                if focus == Focus::Cancel {
                    "cancel"
                } else {
                    "send"
                },
            ));
            if focus == Focus::Field {
                keys[1] = Some((&["Ctrl", "V"][..], "paste"));
            }
            keys[2] = Some((&["Tab"][..], "next"));
            keys[3] = Some((&["Esc"][..], "cancel"));
        }
        (Phase::Sending, _) => keys[3] = Some((&["Esc"][..], "close")),
        (Phase::Sent(_), _) => keys[0] = Some((&["Enter"][..], "done")),
        (Phase::Failed(_), focus) => {
            keys[0] = Some((
                &["Enter"][..],
                if focus == Focus::Send {
                    "edit"
                } else {
                    "close"
                },
            ));
            keys[2] = Some((&["Tab"][..], "next"));
            keys[3] = Some((&["Esc"][..], "close"));
        }
    }
    let gap = 30.0 * s;
    let shown = keys.iter().flatten();
    let width: f32 = shown
        .clone()
        .map(|(caps, action)| key_hint_width(caps, action, s))
        .sum::<f32>()
        + gap * shown.clone().count().saturating_sub(1) as f32;
    let [right, y] = frame.point(1_824.0, KEYS_Y);
    let mut x = right - width;
    for (caps, action) in shown {
        x = key_hint(ui, caps, action, x, y, s) + gap;
    }
}

// Top to bottom: the eyebrow, the headline, the guidance, the field, the rules and
// the refusal, then the buttons, inside the card, which ends above the keys.
const _: () = assert!(EYEBROW_Y + 30.0 <= HEADLINE_Y && HEADLINE_Y + 52.0 <= DETAIL_Y);
const _: () = assert!(DETAIL_Y + DETAIL_LINES as f32 * DETAIL_LINE <= FIELD_TOP);
const _: () = assert!(MESSAGE_Y + 24.0 + 12.0 <= BUTTONS_Y);
const _: () = assert!(CARD_TOP + CARD_HEIGHT < KEYS_Y - 20.0);
const _: () = assert!(CARD_TOP >= 100.0);
// A note's card, taller for its field and model row, still sits inside the frame's
// margins and ends above the keys.
const NOTE_HEIGHT: f32 = CARD_HEIGHT + (NOTE_LINES - LINES) as f32 * LINE + MODEL_ROW;
const _: () = assert!((1080.0 + NOTE_HEIGHT) * 0.5 < KEYS_Y - 20.0);
const _: () = assert!((1080.0 - NOTE_HEIGHT) * 0.5 >= 100.0);
const _: () = assert!(MODEL_BUTTON_HEIGHT < MODEL_ROW);

#[cfg(test)]
mod tests {
    use super::super::{Action, Report};
    use super::*;
    use crate::text_dialog::Look;
    use sjk_ui::{InputEvent, PointerButton, Vec2};
    use winit::keyboard::KeyCode;

    struct Fonts {
        display: crate::text::FontAtlas,
        body: crate::text::FontAtlas,
    }

    fn fonts() -> Fonts {
        let load = |family| crate::text::load_family(family, 1.0, None).expect("a bundled family");
        Fonts {
            display: load(&crate::text::DISPLAY),
            body: load(&crate::text::BODY),
        }
    }

    fn sjk(kind: Kind) -> TextDialog {
        let mut dialog = TextDialog::default();
        dialog.set_look(Look::Sjk, crate::menu::art::ArtSet::default());
        dialog.open(kind);
        dialog
    }

    fn player() -> Kind {
        Kind::PlayerReport {
            subject: "Kyle: Cheating".into(),
            category: sjk_identity::Category::Cheating,
        }
    }

    fn note() -> Kind {
        Kind::Note {
            subject: "misc_model_static models/map_objects/taspir/lamp2_double.md3 (models/map_objects/taspir/lamp remapped to textures/video/raven), 212 units"
                .into(),
            model: Some("models/map_objects/taspir/lamp2_double_with_a_much_longer_name_than_usual.md3".into()),
        }
    }

    fn typed(dialog: &mut TextDialog, text: &str) {
        for character in text.chars() {
            let mut buffer = [0; 4];
            dialog.key(KeyCode::KeyA, Some(character.encode_utf8(&mut buffer)));
        }
    }

    fn draw(dialog: &mut TextDialog, fonts: &Fonts, viewport: [f32; 2]) {
        dialog.build_sjk(
            &fonts.display.font,
            &fonts.body.font,
            TextStyle::NEUTRAL,
            viewport,
        );
    }

    fn click(dialog: &mut TextDialog, token: u16) -> Action {
        let rect = dialog.ui.rect_for(token).expect("drawn");
        let at = Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        let button = PointerButton::Primary;
        dialog.handle_pointer(InputEvent::PointerMove(at));
        dialog.handle_pointer(InputEvent::PointerPress {
            position: at,
            button,
        });
        dialog.handle_pointer(InputEvent::PointerRelease {
            position: at,
            button,
        })
    }

    /// Every kind and state, at its longest, fits the canvas at 1080 lines, 4K and in
    /// a 4:3 window, in the families and in Inter.
    #[test]
    fn every_state_fits_the_canvas() {
        let families = fonts();
        let inter = crate::text::load_modern(1.0, None).expect("Inter");
        let inter = Fonts {
            display: crate::text::load_modern(1.0, None).expect("Inter"),
            body: inter,
        };
        // The longest text: words of a script with wide letters, and one long word.
        let long = format!("{} {}", "Äöüß word ".repeat(55), "x".repeat(40));
        let phases = [
            Phase::Writing,
            Phase::Sending,
            Phase::Sent("report #123456".into()),
            Phase::Failed("not connected to the hub (is identity on, cl_identity 1?)".into()),
        ];
        for kind in [Kind::Report, note(), player()] {
            let mut dialog = sjk(kind.clone());
            typed(&mut dialog, &long);
            dialog.message = "a report needs a few real words".into();
            for fonts in [&families, &inter] {
                for viewport in [[1920.0, 1080.0], [3840.0, 2160.0], [1440.0, 1080.0]] {
                    for phase in &phases {
                        for focus in [Focus::Field, Focus::Send, Focus::Cancel] {
                            dialog.phase = phase.clone();
                            dialog.focus = focus;
                            draw(&mut dialog, fonts, viewport);
                            assert!(
                                !dialog.ui.overflowed(),
                                "{kind:?} {phase:?} {focus:?} at {viewport:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The text never runs past the field: every line drawn fits its width.
    #[test]
    fn the_text_wraps_inside_the_field() {
        let fonts = fonts();
        let mut dialog = sjk(Kind::Report);
        typed(&mut dialog, &"wonderful ".repeat(60));
        draw(&mut dialog, &fonts, [1920.0, 1080.0]);
        let field = dialog.ui.rect_for(FIELD_TOKEN).expect("the field");
        let size = TEXT_SIZE;
        let scale = size / fonts.body.font.height;
        let lines: Vec<String> =
            super::super::wrap_to(&dialog.text, &fonts.body.font, size, 0.0, 788.0);
        assert!(lines.len() > LINES, "the text is longer than the field");
        for line in &lines {
            let width = crate::text::visible_text_width(&fonts.body.font, line, scale);
            assert!(width + FIELD_PAD_X * 2.0 <= field.width, "{line}");
        }
    }

    #[test]
    fn a_long_headline_is_cut_to_fit() {
        let fonts = fonts();
        let display = &fonts.display.font;
        let (shown, cut) = fit(display, "Short", 40.0, 832.0);
        assert_eq!((shown, cut), ("Short", false));
        let name = "A very long player name indeed: Spam or advertising and more".repeat(3);
        let (shown, cut) = fit(display, &name, 40.0, 832.0);
        assert!(cut && shown.len() < name.len());
        let scale = 40.0 / display.height;
        let width = crate::text::visible_text_width(display, &format!("{shown}..."), scale);
        // Measured apart, the last letter's bearing may add a pixel or two.
        assert!(width <= 836.0, "{width}");
    }

    #[test]
    fn a_reason_reads_as_a_sentence() {
        assert_eq!(
            Sentence("the SJK identity is off (cl_identity 1 turns it on)").to_string(),
            "The SJK identity is off (cl_identity 1 turns it on)"
        );
        assert_eq!(
            Sentence("too many reports").to_string(),
            "Too many reports."
        );
        assert_eq!(Sentence("done!").to_string(), "Done!");
    }

    /// Writing: Tab walks the field, Send and Cancel; Enter on Send sends, on Cancel
    /// cancels; Space acts on a button but types in the field.
    #[test]
    fn keys_walk_the_card_and_act() {
        let mut dialog = sjk(Kind::Report);
        typed(&mut dialog, "The door flickers");
        assert_eq!(dialog.text, "The door flickers");
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::Send);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::Cancel);
        dialog.set_shift(true);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::Send);
        dialog.set_shift(false);
        dialog.key(KeyCode::Tab, None);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::Field);
        dialog.key(KeyCode::Space, Some(" "));
        dialog.key(KeyCode::Backspace, None);
        assert_eq!(dialog.text, "The door flickers");
        dialog.key(KeyCode::Tab, None);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.key(KeyCode::Enter, None), Action::Cancel);
        assert!(!dialog.is_open());
    }

    /// A report sent from the card waits for the hub's answer, then shows it; a
    /// failure's Edit goes back to the text, kept; Escape closes it whatever it shows.
    #[test]
    fn a_report_waits_for_its_answer_on_the_card() {
        let mut dialog = sjk(Kind::Report);
        typed(&mut dialog, "short");
        assert_eq!(dialog.key(KeyCode::Enter, None), Action::None);
        assert!(!dialog.message.is_empty(), "refused, and says why");
        typed(&mut dialog, " door on ffa3");
        assert_eq!(
            dialog.key(KeyCode::Enter, None),
            Action::Send(Kind::Report, "short door on ffa3".into())
        );
        assert!(dialog.is_open() && dialog.sending(Report::Bug));
        assert!(!dialog.sending(Report::Player));
        // Typing goes nowhere while it sends; a player report's answer is not this one's.
        typed(&mut dialog, "x");
        assert_eq!(dialog.text, "short door on ffa3");
        assert!(!dialog.answer(Report::Player, Ok("player report #1".into())));
        assert!(dialog.answer(Report::Bug, Err("too many reports".into())));
        assert_eq!(dialog.phase, Phase::Failed("too many reports".into()));
        // A second answer has no card waiting.
        assert!(!dialog.answer(Report::Bug, Ok("report #2".into())));
        // Tab moves between Edit and Close; Enter on Edit goes back to the text.
        assert_eq!(dialog.focus, Focus::Send);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::Cancel);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.key(KeyCode::Enter, None), Action::None);
        assert_eq!(
            (&dialog.phase, dialog.focus),
            (&Phase::Writing, Focus::Field)
        );
        assert_eq!(dialog.text, "short door on ffa3");
        // Sent: Enter closes.
        dialog.key(KeyCode::Enter, None);
        assert!(dialog.answer(Report::Bug, Ok("report #12".into())));
        assert_eq!(dialog.key(KeyCode::Enter, None), Action::Cancel);
        assert!(!dialog.is_open() && dialog.phase == Phase::Writing);
        // Escape closes while sending; the answer then has no card.
        dialog.open(player());
        typed(&mut dialog, "Speed hacking all match");
        assert!(matches!(dialog.key(KeyCode::Enter, None), Action::Send(..)));
        assert!(dialog.sending(Report::Player));
        assert_eq!(dialog.key(KeyCode::Escape, None), Action::Cancel);
        assert!(!dialog.answer(Report::Player, Ok("player report #3".into())));
    }

    /// A note closes on Send in every look: its screenshot is of the next frame.
    #[test]
    fn a_note_closes_on_send() {
        let mut dialog = sjk(note());
        typed(&mut dialog, "too shiny");
        assert!(matches!(
            dialog.key(KeyCode::Enter, None),
            Action::Send(Kind::Note { .. }, _)
        ));
        assert!(!dialog.is_open());
    }

    /// The classic look closes on Send as before, and a card waiting for an answer is
    /// dropped when the look changes.
    #[test]
    fn the_classic_look_closes_on_send() {
        let mut dialog = TextDialog::default();
        dialog.set_look(Look::Classic, crate::menu::art::ArtSet::default());
        dialog.open(Kind::Report);
        typed(&mut dialog, "The door flickers on ffa3");
        assert!(matches!(dialog.key(KeyCode::Enter, None), Action::Send(..)));
        assert!(!dialog.is_open() && !dialog.sending(Report::Bug));
        let mut dialog = sjk(Kind::Report);
        typed(&mut dialog, "The door flickers on ffa3");
        dialog.key(KeyCode::Enter, None);
        dialog.set_look(Look::Classic, crate::menu::art::ArtSet::default());
        assert!(!dialog.is_open());
    }

    /// The pointer: the field, Send (refused while the text is short), Cancel; after
    /// Send, Close, Done and Edit; every control under its token.
    #[test]
    fn the_pointer_reaches_every_control() {
        let fonts = fonts();
        let viewport = [1920.0, 1080.0];
        let mut dialog = sjk(Kind::Report);
        draw(&mut dialog, &fonts, viewport);
        assert!(dialog.ui.rect_for(EDIT_TOKEN).is_none());
        assert!(dialog.ui.rect_for(CLOSE_TOKEN).is_none());
        click(&mut dialog, CANCEL_TOKEN);
        assert!(!dialog.is_open());
        dialog.open(Kind::Report);
        dialog.focus = Focus::Cancel;
        draw(&mut dialog, &fonts, viewport);
        click(&mut dialog, FIELD_TOKEN);
        assert_eq!(dialog.focus, Focus::Field);
        typed(&mut dialog, "short");
        draw(&mut dialog, &fonts, viewport);
        assert_eq!(click(&mut dialog, SEND_TOKEN), Action::None);
        assert!(!dialog.message.is_empty());
        typed(&mut dialog, " door on ffa3");
        draw(&mut dialog, &fonts, viewport);
        assert!(matches!(click(&mut dialog, SEND_TOKEN), Action::Send(..)));
        draw(&mut dialog, &fonts, viewport);
        assert!(
            dialog.ui.rect_for(SEND_TOKEN).is_none(),
            "no Send while it sends"
        );
        dialog.answer(Report::Bug, Err("the hub did not answer".into()));
        draw(&mut dialog, &fonts, viewport);
        click(&mut dialog, EDIT_TOKEN);
        assert_eq!(dialog.phase, Phase::Writing);
        draw(&mut dialog, &fonts, viewport);
        click(&mut dialog, SEND_TOKEN);
        dialog.answer(Report::Bug, Ok("report #7".into()));
        draw(&mut dialog, &fonts, viewport);
        assert_eq!(click(&mut dialog, CLOSE_TOKEN), Action::Cancel);
        assert!(!dialog.is_open());
    }

    /// Every control sits inside the card, the keys under it, in the frame.
    #[test]
    fn the_card_holds_its_controls() {
        let fonts = fonts();
        let viewport = [1920.0, 1080.0];
        let card = Rect::new(CARD_X, CARD_TOP, CARD_WIDTH, CARD_HEIGHT);
        let inside = |rect: Rect| {
            rect.x >= card.x
                && rect.y >= card.y
                && rect.right() <= card.right()
                && rect.bottom() <= card.bottom()
        };
        let mut dialog = sjk(Kind::Report);
        draw(&mut dialog, &fonts, viewport);
        for token in [FIELD_TOKEN, SEND_TOKEN, CANCEL_TOKEN] {
            assert!(inside(dialog.ui.rect_for(token).expect("drawn")), "{token}");
        }
        dialog.phase = Phase::Failed("why".into());
        draw(&mut dialog, &fonts, viewport);
        for token in [FIELD_TOKEN, EDIT_TOKEN, CLOSE_TOKEN] {
            assert!(inside(dialog.ui.rect_for(token).expect("drawn")), "{token}");
        }
    }

    /// The gold caret bar the last draw left, if it is lit.
    fn caret_bar(dialog: &TextDialog) -> Option<Rect> {
        dialog
            .ui
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::SolidRect { rect, color }
                    if *color == color::GOLD_BRIGHT && rect.width < 6.0 =>
                {
                    Some(*rect)
                }
                _ => None,
            })
    }

    const LONG: &str = "The door by the tower's foot flickers when I walk through it, and the light behind it goes black for a second. I expected it to open smoothly, as it does on ffa3. It happens every time on this server, in every match I have played since the last update, and it did not before it. The same door on duel6 is fine, so it looks like a problem with how the lightmap of that one surface is read.";

    /// The caret sits between the two glyphs of the insertion point, whatever the player's
    /// text size and spacing, wherever in the text it is, and the lines stay in the field.
    #[test]
    fn the_caret_follows_the_insertion_point_in_any_text_style() {
        use crate::text::{TextFace, visible_text_width_style};
        let fonts = fonts();
        let body = &fonts.body.font;
        for viewport in [[1920.0, 1080.0], [3840.0, 2160.0]] {
            for (scale, tracking) in [(1.0, 0.0), (1.2, 0.1), (0.8, -0.05)] {
                let style = TextStyle { scale, tracking };
                let mut dialog = sjk(Kind::Report);
                typed(&mut dialog, LONG);
                // From the end back to the middle of the text, then to its start.
                for back in [0, 130, usize::MAX] {
                    if back == usize::MAX {
                        dialog.key(KeyCode::Home, None);
                        dialog.set_control(true);
                        dialog.key(KeyCode::Home, None);
                        dialog.set_control(false);
                    } else {
                        for _ in 0..back {
                            dialog.key(KeyCode::ArrowLeft, None);
                        }
                    }
                    dialog.caret_for_shot();
                    dialog.build_sjk(&fonts.display.font, body, style, viewport);
                    let frame = Frame::new(viewport);
                    let at = dialog.edit.cursor(&dialog.text);
                    let (line, _) = dialog.layout.locate(&dialog.text, at);
                    let (first, shown) = dialog.layout.visible();
                    assert!(shown.len() <= LINES && line >= first && line < first + shown.len());
                    let range = &shown[line - first];
                    let size = TEXT_SIZE * frame.s * scale;
                    let measure = |text: &str| {
                        visible_text_width_style(
                            body,
                            text,
                            size / body.height,
                            TextFace::Regular,
                            tracking * size,
                        )
                    };
                    let row = frame.rect(
                        TEXT_X + FIELD_PAD_X,
                        FIELD_TOP + FIELD_PAD_Y + (line - first) as f32 * LINE,
                        TEXT_WIDTH - FIELD_PAD_X * 2.0,
                        LINE,
                    );
                    let bar = caret_bar(&dialog).expect("lit");
                    let centre = bar.x + bar.width * 0.5;
                    let expected = row.x + measure(&dialog.text[range.start..at]);
                    assert!(
                        (centre - expected).abs() < 0.02,
                        "{viewport:?} {scale} {at}: {centre} against {expected}"
                    );
                    assert!(bar.y >= row.y && bar.bottom() <= row.bottom(), "{bar:?}");
                    for range in shown {
                        let width = measure(&dialog.text[range.clone()]);
                        assert!(width <= row.width, "{scale}: {width} wide");
                    }
                }
            }
        }
    }

    /// A click in the field puts the caret between the glyphs it lands on; the field
    /// keeps the keyboard, and typing goes in at the caret.
    #[test]
    fn a_click_places_the_caret() {
        use crate::text::{TextFace, visible_text_width_style};
        let fonts = fonts();
        let body = &fonts.body.font;
        let viewport = [1920.0, 1080.0];
        let style = TextStyle {
            scale: 1.2,
            tracking: 0.05,
        };
        let mut dialog = sjk(Kind::Report);
        typed(&mut dialog, LONG);
        dialog.focus = Focus::Cancel;
        dialog.build_sjk(&fonts.display.font, body, style, viewport);
        let size = TEXT_SIZE * 1.2;
        let before = |count: usize| {
            visible_text_width_style(
                body,
                &dialog.text[..count],
                size / body.height,
                TextFace::Regular,
                style.tracking * size,
            )
        };
        // Just right of the boundary after the ninth character of the first line, a little
        // below the middle of its row.
        let point = Vec2::new(
            TEXT_X + FIELD_PAD_X + before(9) + 1.0,
            FIELD_TOP + FIELD_PAD_Y + LINE * 0.5 + 3.0,
        );
        click_point(&mut dialog, point);
        assert_eq!(dialog.focus, Focus::Field);
        assert_eq!(dialog.edit.cursor(&dialog.text), 9);
        typed(&mut dialog, "X");
        assert_eq!(&dialog.text[..12], "The door Xby");
        // A click on the last visible row lands on that row, past its end at its end.
        dialog.build_sjk(&fonts.display.font, body, style, viewport);
        let point = Vec2::new(
            TEXT_X + TEXT_WIDTH - 4.0,
            FIELD_TOP + FIELD_PAD_Y + LINE * 5.5,
        );
        click_point(&mut dialog, point);
        let (line, _) = dialog
            .layout
            .locate(&dialog.text, dialog.edit.cursor(&dialog.text));
        let (first, shown) = dialog.layout.visible();
        assert_eq!(line, first + shown.len() - 1);
        assert_eq!(dialog.edit.cursor(&dialog.text), shown[shown.len() - 1].end);
    }

    fn click_point(dialog: &mut TextDialog, at: Vec2) -> Action {
        let button = PointerButton::Primary;
        dialog.handle_pointer(InputEvent::PointerMove(at));
        dialog.handle_pointer(InputEvent::PointerPress {
            position: at,
            button,
        });
        dialog.handle_pointer(InputEvent::PointerRelease {
            position: at,
            button,
        })
    }

    /// The reason a Send would be refused shows, in a warm band, as soon as Send has the
    /// keyboard or the pointer, not only after it was pressed.
    #[test]
    fn why_send_is_refused_shows_before_and_after_the_press() {
        let fonts = fonts();
        let viewport = [1920.0, 1080.0];
        let band = |dialog: &TextDialog| {
            dialog.ui.draw_list().commands().iter().any(|command| {
                matches!(command, DrawCommand::RoundedRect { color, .. }
                        if *color == color::alpha(WARN, 0.14))
            })
        };
        let mut dialog = sjk(Kind::Report);
        typed(&mut dialog, "short");
        draw(&mut dialog, &fonts, viewport);
        assert!(!band(&dialog), "nothing to say while writing");
        // The pointer on Send says why it would not go.
        let rect = dialog.ui.rect_for(SEND_TOKEN).expect("drawn");
        dialog.handle_pointer(InputEvent::PointerMove(Vec2::new(
            rect.x + 4.0,
            rect.y + 4.0,
        )));
        draw(&mut dialog, &fonts, viewport);
        draw(&mut dialog, &fonts, viewport);
        assert!(band(&dialog));
        // So does the keyboard, and pressing it leaves the reason up while typing goes on.
        dialog.handle_pointer(InputEvent::PointerLeave);
        draw(&mut dialog, &fonts, viewport);
        assert!(!band(&dialog));
        dialog.key(KeyCode::Tab, None);
        draw(&mut dialog, &fonts, viewport);
        assert!(band(&dialog));
        dialog.key(KeyCode::Enter, None);
        assert!(!dialog.message.is_empty());
        draw(&mut dialog, &fonts, viewport);
        assert!(band(&dialog));
        typed(&mut dialog, " door on ffa3");
        draw(&mut dialog, &fonts, viewport);
        assert!(!band(&dialog), "sendable text has nothing to explain");
    }

    /// A note's card is taller, for its model row and ten lines of text, and holds Clear
    /// model while the note has a model; Tab reaches it last and Enter or a click on it
    /// asks for the model to go; once gone, the row says how to pick one.
    #[test]
    fn a_notes_model_row_clears_the_model() {
        let fonts = fonts();
        let viewport = [1920.0, 1080.0];
        let mut dialog = sjk(note());
        typed(&mut dialog, &format!("{LONG} {LONG} {LONG}"));
        draw(&mut dialog, &fonts, viewport);
        let geometry = Geometry::of(&note());
        assert!(geometry.height > CARD_HEIGHT && geometry.lines == super::super::NOTE_LINES);
        let card = Rect::new(CARD_X, geometry.top, CARD_WIDTH, geometry.height);
        assert_eq!(dialog.card, Some(card));
        let inside = |rect: Rect| {
            rect.x >= card.x
                && rect.y >= card.y
                && rect.right() <= card.right()
                && rect.bottom() <= card.bottom()
        };
        for token in [FIELD_TOKEN, SEND_TOKEN, CANCEL_TOKEN, CLEAR_MODEL_TOKEN] {
            assert!(inside(dialog.ui.rect_for(token).expect("drawn")), "{token}");
        }
        // The field sits under the model row and shows more lines than a report's.
        let field = dialog.ui.rect_for(FIELD_TOKEN).expect("the field");
        let clear = dialog.ui.rect_for(CLEAR_MODEL_TOKEN).expect("Clear model");
        assert!(clear.bottom() <= field.y);
        assert_eq!(dialog.layout.visible().1.len(), super::super::NOTE_LINES);
        // Tab: field, Send, Cancel, Clear model, field.
        for focus in [Focus::Send, Focus::Cancel, Focus::ClearModel, Focus::Field] {
            dialog.key(KeyCode::Tab, None);
            assert_eq!(dialog.focus, focus);
        }
        dialog.set_shift(true);
        dialog.key(KeyCode::Tab, None);
        assert_eq!(dialog.focus, Focus::ClearModel);
        dialog.set_shift(false);
        assert_eq!(dialog.key(KeyCode::Enter, None), Action::ClearModel);
        draw(&mut dialog, &fonts, viewport);
        assert_eq!(click(&mut dialog, CLEAR_MODEL_TOKEN), Action::ClearModel);
        // The caller clears it: the row and the focus follow, the text stays.
        dialog.set_note(
            "textures/test/wall (surface 3, lightmapped), 400 units".into(),
            None,
        );
        assert_eq!(dialog.focus, Focus::Field);
        assert!(dialog.text.starts_with("The door"));
        draw(&mut dialog, &fonts, viewport);
        assert!(dialog.ui.rect_for(CLEAR_MODEL_TOKEN).is_none());
        for focus in [Focus::Send, Focus::Cancel, Focus::Field] {
            dialog.key(KeyCode::Tab, None);
            assert_eq!(dialog.focus, focus);
        }
    }

    /// A press on the scene round a note's card asks to note what is there; inside the
    /// card, or round a report's, it does not.
    #[test]
    fn a_click_round_a_notes_card_picks_in_the_scene() {
        let fonts = fonts();
        let viewport = [1920.0, 1080.0];
        let press = |dialog: &mut TextDialog, at: Vec2| {
            dialog.handle_pointer(InputEvent::PointerPress {
                position: at,
                button: PointerButton::Primary,
            })
        };
        let mut dialog = sjk(note());
        draw(&mut dialog, &fonts, viewport);
        assert_eq!(
            press(&mut dialog, Vec2::new(120.0, 540.0)),
            Action::PickAt([120.0, 540.0])
        );
        assert_ne!(
            press(&mut dialog, Vec2::new(960.0, 540.0)),
            Action::PickAt([960.0, 540.0])
        );
        let mut report = sjk(Kind::Report);
        draw(&mut report, &fonts, viewport);
        assert_eq!(press(&mut report, Vec2::new(120.0, 540.0)), Action::None);
    }
}

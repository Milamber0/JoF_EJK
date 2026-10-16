//! The log request card (`docs/sjk-ui.md`, "Log request"): when the SJK team asks the
//! player for their game's logs (`docs/identity.md`, "Log requests"), an SJK pop-up card
//! over the darkened screen says who asks, why, and what the logs hold, and offers
//! Send logs (gold, Enter) or Don't send (Escape). Nothing leaves the PC without Send
//! logs. The card shows on the main menu or over the game menu, never in the middle
//! of play: the console says when a request comes in, and it waits for the menu. It is
//! drawn in the SJK UI's look in every menu style, and the menus under it are not
//! drawn and take no input, as under the other cards.

use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_identity::LogRequest;
use sjk_ui::{FontWeight, InputEvent, TextAlign, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Send logs and Don't send.
const SEND_TOKEN: u16 = 1;
const DECLINE_TOKEN: u16 = 2;

/// The card's left edge, top and width; it is as tall as what it holds.
const CARD_X: f32 = 560.0;
const CARD_TOP: f32 = 300.0;
const CARD_WIDTH: f32 = 800.0;
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
/// The explanation's first line, its step and most lines, and characters a line.
const BODY_TOP: f32 = CARD_TOP + 134.0;
const LINE: f32 = 28.0;
const LINES: usize = 9;
const LINE_CHARS: usize = 90;
/// The buttons along the card's foot, this far under the explanation.
const BUTTONS_GAP: f32 = 30.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
const SEND_WIDTH: f32 = 200.0;
const DECLINE_WIDTH: f32 = 170.0;
/// The keys' line, as on the other screens.
const KEYS_Y: f32 = 992.0;
const KEYS_RIGHT: f32 = 1824.0;

/// What the player chose on the card, for request `id`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Choice {
    Send(i64),
    Decline(i64),
}

pub(crate) struct Card {
    /// The request on show, the oldest open one.
    request: Option<LogRequest>,
    /// The requests the console was told about.
    announced: Vec<i64>,
    /// The last answer outcome the console was told about.
    outcome_seen: u64,
    /// Shown this frame.
    open: bool,
    ui: MenuCanvas,
    /// What a world shot shows in place of the live requests.
    #[cfg(test)]
    pub(crate) preview: Option<Vec<LogRequest>>,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            request: None,
            announced: Vec::new(),
            outcome_seen: 0,
            open: false,
            ui: MenuCanvas::with_capacities(16, 200, 64),
            #[cfg(test)]
            preview: None,
        }
    }
}

impl Card {
    /// The open requests now (oldest first): the card shows the first. Returns the
    /// console lines for requests that just came in.
    pub(crate) fn offer(&mut self, requests: &[LogRequest]) -> Vec<String> {
        self.request = requests.first().cloned();
        let mut lines = Vec::new();
        for request in requests {
            if !self.announced.contains(&request.id) {
                self.announced.push(request.id);
                lines.push(format!(
                    "{} from the SJK team asks for your game's logs: open the menu to answer.",
                    from(request)
                ));
            }
        }
        lines
    }

    /// The console line for an answer's outcome, once per outcome.
    pub(crate) fn outcome_line(&mut self, outcome: &sjk_identity::ReportOutcome) -> Option<String> {
        if outcome.serial <= self.outcome_seen {
            return None;
        }
        self.outcome_seen = outcome.serial;
        Some(match (outcome.sent, outcome.message.as_str()) {
            (true, "logs sent") => "Your logs were sent to the SJK team. Thank you.".to_owned(),
            (true, _) => "The SJK team was told you won't send your logs.".to_owned(),
            (false, why) => format!("Your answer to the SJK team did not go: {why}"),
        })
    }

    /// Whether the card has something to show.
    pub(crate) fn wants(&self) -> bool {
        self.request.is_some()
    }

    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Enter takes Send logs, Escape or Backspace Don't send; every other key is
    /// swallowed while the card shows. Space does nothing: sending needs a deliberate key.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> Option<Choice> {
        if event.state != ElementState::Pressed || event.repeat {
            return None;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return None;
        };
        self.key(code)
    }

    fn key(&mut self, code: KeyCode) -> Option<Choice> {
        let id = self.request.as_ref()?.id;
        let choice = match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Choice::Send(id),
            KeyCode::Escape | KeyCode::Backspace => Choice::Decline(id),
            _ => return None,
        };
        self.choose(choice)
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> Option<Choice> {
        let event = self.ui.pointer(event)?;
        if event.kind != UiEventKind::Activate {
            return None;
        }
        let id = self.request.as_ref()?.id;
        match event.token {
            Some(SEND_TOKEN) => self.choose(Choice::Send(id)),
            Some(DECLINE_TOKEN) => self.choose(Choice::Decline(id)),
            _ => None,
        }
    }

    /// The request is answered: the card puts it away until the next one.
    fn choose(&mut self, choice: Choice) -> Option<Choice> {
        self.request = None;
        Some(choice)
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// What the card says under its headline: who asks and why, then what the logs hold.
    fn words(request: &LogRequest) -> String {
        let note = crate::medals::plain_note(&request.note);
        let why = if note.is_empty() {
            "to look into a problem you have.".to_owned()
        } else {
            format!("to look into a problem you have: \u{201c}{note}\u{201d}.")
        };
        format!(
            "{} from the SJK team asks for your game's logs {why} They hold the console lines \
             of this session and the one before (no chat), your last crash report, and your \
             build, system and graphics card. Your Windows user name is left out. Only the \
             SJK team reads them, and nothing is sent unless you choose Send logs.",
            from(request)
        )
    }

    /// Draw the card over the whole frame.
    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let Some(request) = self.request.clone() else {
            return;
        };
        let frame = Frame::new(viewport);
        let s = frame.s;
        let explanation = Self::words(&request);
        let lines: Vec<&str> = wrap(&explanation, LINE_CHARS).take(LINES).collect();
        let body_bottom = BODY_TOP + lines.len() as f32 * LINE;
        let buttons_y = body_bottom + BUTTONS_GAP;
        let height = buttons_y + BUTTON_HEIGHT + MARGIN - CARD_TOP;
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);
        kit::scrim(ui, viewport);
        kit::card(ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            ui,
            TextFamily::Display,
            format_args!("SJK team"),
            frame.rect(TEXT_X, CARD_TOP + 30.0, TEXT_WIDTH, 30.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            ui,
            TextFamily::Display,
            format_args!("Send your game's logs?"),
            frame.rect(TEXT_X, CARD_TOP + 66.0, TEXT_WIDTH, 48.0),
            36.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        for (index, line) in lines.iter().enumerate() {
            text(
                ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(TEXT_X, BODY_TOP + index as f32 * LINE, TEXT_WIDTH, LINE),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let right = CARD_X + CARD_WIDTH - MARGIN;
        kit::button(
            ui,
            &frame,
            [right - SEND_WIDTH, buttons_y, SEND_WIDTH, BUTTON_HEIGHT],
            "Send logs",
            true,
            true,
            false,
            SEND_TOKEN,
        );
        kit::button(
            ui,
            &frame,
            [
                right - SEND_WIDTH - BUTTON_GAP - DECLINE_WIDTH,
                buttons_y,
                DECLINE_WIDTH,
                BUTTON_HEIGHT,
            ],
            "Don't send",
            false,
            true,
            false,
            DECLINE_TOKEN,
        );
        let [right, keys_y] = frame.point(KEYS_RIGHT, KEYS_Y);
        let decline = key_hint_width(&["Esc"], "don't send", s);
        let send = key_hint_width(&["Enter"], "send logs", s);
        let x = right - decline - send - 18.0 * s;
        let x = key_hint(ui, &["Enter"], "send logs", x, keys_y, s);
        key_hint(ui, &["Esc"], "don't send", x + 18.0 * s, keys_y, s);
        // Focus starts on Don't send: a stray click or key sends nothing.
        ui.finish(DECLINE_TOKEN);
        target.append(&self.ui, viewport);
    }
}

/// Who asks, colour codes left out, or "Someone" when the hub gave no name.
fn from(request: &LogRequest) -> String {
    let name = crate::medals::plain_note(&request.from);
    if name.is_empty() {
        "Someone".to_owned()
    } else {
        name
    }
}

impl crate::GpuState {
    /// Whether the card shows this frame: a request open, and the player on the main
    /// menu or the game menu with nothing else over it. `covered` is another card or
    /// the console over the frame. Also tells the console of new requests and of what
    /// became of an answer.
    pub(crate) fn prepare_log_request_card(&mut self, covered: bool) -> bool {
        #[cfg(test)]
        let preview = self.log_request_card.preview.clone();
        #[cfg(not(test))]
        let preview: Option<Vec<LogRequest>> = None;
        let (requests, outcome) = match preview {
            Some(requests) => (requests, None),
            None => crate::player_identity::with_snapshot(|snapshot| {
                (snapshot.log_requests.clone(), snapshot.log_answer.clone())
            })
            .unwrap_or_default(),
        };
        let mut lines = self.log_request_card.offer(&requests);
        if let Some(outcome) = outcome
            && let Some(line) = self.log_request_card.outcome_line(&outcome)
        {
            lines.push(line);
        }
        if let Some(console) = self.console.as_mut() {
            for line in lines {
                console.push_log(line);
            }
        }
        let menu_visible = self
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_visible);
        let main_menu = self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible() && menu.on_main_menu());
        let place = main_menu || (self.game_menu && !menu_visible);
        let console_open = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::is_open);
        let away = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::window_away);
        let open = self.log_request_card.wants()
            && place
            && !covered
            && !console_open
            && !away
            && !self.text_dialog.is_open()
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
            && !self.pending_map_reload;
        self.log_request_card.set_open(open);
        open
    }

    /// Draw the card. It darkens everything, so every font batch appended before it is
    /// dropped (text draws above all shapes).
    pub(crate) fn append_log_request_card(&mut self, viewport: [f32; 2]) {
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        self.game_fonts.clear_text();
        let target = crate::ingame_menu::sjk_view::text_target(
            &mut self.game_fonts,
            &mut self.text_vertices,
            &self.ui_font,
        );
        self.log_request_card.append(target, viewport);
    }

    /// Act on the player's choice: gather and send the logs, or decline.
    pub(crate) fn log_request_card_choice(&mut self, choice: Option<Choice>) {
        match choice {
            Some(Choice::Send(id)) => {
                crate::player_identity::answer_logs(id, Some(crate::crash_report::log_upload()));
            }
            Some(Choice::Decline(id)) => crate::player_identity::answer_logs(id, None),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: i64, note: &str) -> LogRequest {
        LogRequest {
            id,
            from: "^1Sol".into(),
            note: note.into(),
            created: 1,
        }
    }

    #[test]
    fn requests_are_announced_once_and_shown_oldest_first() {
        let mut card = Card::default();
        assert!(!card.wants());
        let lines = card.offer(&[request(3, ""), request(4, "")]);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "Sol from the SJK team asks for your game's logs: open the menu to answer."
        );
        assert!(card.wants());
        assert!(card.offer(&[request(3, ""), request(4, "")]).is_empty());
        assert_eq!(card.key(KeyCode::Enter), Some(Choice::Send(3)));
        assert!(!card.wants());
        // The service lists the next one.
        card.offer(&[request(4, "")]);
        assert_eq!(card.key(KeyCode::Space), None);
        assert_eq!(card.key(KeyCode::Escape), Some(Choice::Decline(4)));
        card.offer(&[]);
        assert!(!card.wants());
        assert_eq!(card.key(KeyCode::Enter), None);
    }

    #[test]
    fn the_words_say_who_why_and_what() {
        let words = Card::words(&request(1, "^3your freeze on ffa3"));
        assert!(
            words.starts_with(
                "Sol from the SJK team asks for your game's logs to look into a problem you \
                 have: \u{201c}your freeze on ffa3\u{201d}."
            ),
            "{words}"
        );
        assert!(words.contains("(no chat)"), "{words}");
        assert!(wrap(&words, LINE_CHARS).count() <= LINES, "{words}");
        let longest = Card::words(&request(1, &"a".repeat(200)));
        assert!(wrap(&longest, LINE_CHARS).count() <= LINES, "{longest}");
        let plain = Card::words(&LogRequest {
            from: String::new(),
            ..request(1, "")
        });
        assert!(plain.starts_with("Someone from the SJK team"), "{plain}");
    }

    #[test]
    fn each_outcome_is_told_once() {
        let mut card = Card::default();
        let outcome = |serial, sent, message: &str| sjk_identity::ReportOutcome {
            serial,
            sent,
            message: message.into(),
        };
        assert_eq!(
            card.outcome_line(&outcome(1, true, "logs sent")).as_deref(),
            Some("Your logs were sent to the SJK team. Thank you.")
        );
        assert_eq!(card.outcome_line(&outcome(1, true, "logs sent")), None);
        assert!(
            card.outcome_line(&outcome(2, false, "too many"))
                .unwrap()
                .ends_with("did not go: too many")
        );
    }
}

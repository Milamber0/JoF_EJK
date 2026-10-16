//! The low-FPS card (`docs/sjk-ui.md`, "Low FPS help"): an SJK pop-up card over the
//! darkened screen, as the graphics reload card, that says what SJK's lighting costs
//! on this map and offers Vanilla graphics (gold, Enter) or Not now (Escape), with a
//! "Don't suggest this again" tick (D) for the card that comes by itself. Vanilla graphics
//! is the Settings renderer choice: it keeps the player's settings and `vid_restart` applies
//! it; with Vanilla on, the same button turns it off and the settings come back.

use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{FontWeight, InputEvent, TextAlign, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const PRIMARY_TOKEN: u16 = 1;
const CLOSE_TOKEN: u16 = 2;
const TICK_TOKEN: u16 = 3;

/// The card's left edge, top and width; it is as tall as what it holds.
const CARD_X: f32 = 560.0;
const CARD_TOP: f32 = 330.0;
const CARD_WIDTH: f32 = 800.0;
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
const BODY_TOP: f32 = CARD_TOP + 134.0;
const LINE: f32 = 28.0;
const LINES: usize = 6;
const LINE_CHARS: usize = 88;
const PARAGRAPH_GAP: f32 = 12.0;
const FOOT_GAP: f32 = 30.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
const PRIMARY_WIDTH: f32 = 260.0;
const CLOSE_WIDTH: f32 = 150.0;
const KEYS_Y: f32 = 992.0;
const KEYS_RIGHT: f32 = 1824.0;

/// Why the card shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// The map's check found the lighting heavy.
    Offered,
    /// The player typed `help_fps`.
    Asked,
}

/// What the player chose on the card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Choice {
    /// Turn Vanilla graphics on, or off when they are.
    Switch,
    Close,
    /// Flip "Don't suggest this again".
    Tick,
}

/// What the card shows, read from the console and the check each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Facts {
    pub(crate) finding: Option<super::Finding>,
    pub(crate) vanilla: bool,
    pub(crate) suggest: bool,
    pub(crate) on_server: bool,
}

pub(crate) struct Card {
    mode: Option<Mode>,
    /// Shown this frame.
    open: bool,
    ui: MenuCanvas,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            mode: None,
            open: false,
            ui: MenuCanvas::with_capacities(24, 220, 80),
        }
    }
}

impl Card {
    pub(crate) fn show(&mut self, mode: Mode) {
        self.mode = Some(mode);
    }

    /// Whether the card has something to show.
    pub(crate) fn wants(&self) -> bool {
        self.mode.is_some()
    }

    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Enter or Space switch, Escape or Backspace close, D ticks; every other key is
    /// swallowed while the card shows.
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
        let choice = match code {
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Choice::Switch,
            KeyCode::Escape | KeyCode::Backspace => Choice::Close,
            KeyCode::KeyD => Choice::Tick,
            _ => return None,
        };
        Some(self.choose(choice))
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> Option<Choice> {
        let event = self.ui.pointer(event)?;
        if event.kind != UiEventKind::Activate {
            return None;
        }
        let choice = match event.token {
            Some(PRIMARY_TOKEN) => Choice::Switch,
            Some(CLOSE_TOKEN) => Choice::Close,
            Some(TICK_TOKEN) => Choice::Tick,
            _ => return None,
        };
        Some(self.choose(choice))
    }

    /// The card goes on any choice but the tick.
    fn choose(&mut self, choice: Choice) -> Choice {
        if choice != Choice::Tick {
            self.mode = None;
        }
        choice
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// The headline, the paragraphs and the main button's label.
    fn words(&self, facts: Facts) -> (&'static str, Vec<String>, &'static str) {
        let headline = match (self.mode, facts.vanilla) {
            (_, true) => "Vanilla graphics are on",
            (Some(Mode::Offered), false) => "This map is heavy on SJK's lighting",
            _ => "Low FPS?",
        };
        let mut paragraphs = Vec::new();
        if let Some(finding) = facts.finding {
            let sample = finding.sample;
            let most = (1_000.0 / sample.frame_ms.max(0.1)).round() as u32;
            paragraphs.push(format!(
                "Here the graphics card spends {:.1} ms on each frame, {:.1} ms of it on SJK's \
                 lighting: at most about {most} FPS.",
                sample.frame_ms, sample.light_ms
            ));
        }
        let reload = if facts.on_server {
            "The map loads again in a few seconds and you stay on the server."
        } else {
            "The menu map loads again in a few seconds."
        };
        let button = if facts.vanilla {
            paragraphs.push(format!(
                "SJK's lighting, sun, shadows, reflections and material maps are off. Turning Vanilla \
                 graphics off brings your own settings back. {reload}"
            ));
            "Turn Vanilla graphics off"
        } else {
            paragraphs.push(format!(
                "Vanilla graphics turn off SJK's lighting, sun, shadows, reflections and material \
                 maps: the maps' baked lighting, as in EternalJK, and the most FPS. Your \
                 settings are kept for when you turn it off (Settings > Video). {reload}"
            ));
            "Use Vanilla graphics"
        };
        (headline, paragraphs, button)
    }

    /// Draw the card over the whole frame.
    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2], facts: Facts) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        let (headline, paragraphs, button) = self.words(facts);
        let mut lines: Vec<(f32, String)> = Vec::new();
        let mut y = BODY_TOP;
        for paragraph in &paragraphs {
            for line in wrap(paragraph, LINE_CHARS) {
                if lines.len() >= LINES {
                    break;
                }
                lines.push((y, line.to_owned()));
                y += LINE;
            }
            y += PARAGRAPH_GAP;
        }
        let foot = y - PARAGRAPH_GAP + FOOT_GAP;
        let height = foot + BUTTON_HEIGHT + MARGIN - CARD_TOP;
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);
        kit::scrim(ui, viewport);
        kit::card(ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            ui,
            TextFamily::Display,
            format_args!("FPS"),
            frame.rect(TEXT_X, CARD_TOP + 30.0, TEXT_WIDTH, 30.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            ui,
            TextFamily::Display,
            format_args!("{headline}"),
            frame.rect(TEXT_X, CARD_TOP + 66.0, TEXT_WIDTH, 48.0),
            36.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        for (line_y, line) in &lines {
            text(
                ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(TEXT_X, *line_y, TEXT_WIDTH, LINE),
                18.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        // "Don't suggest this again", ticked while cg_fpsHelp is 0.
        let middle = foot + BUTTON_HEIGHT * 0.5;
        let ticked = !facts.suggest;
        let after = kit::tick(ui, &frame, TEXT_X, middle, ticked, false);
        text(
            ui,
            TextFamily::Body,
            format_args!("Don't suggest this again"),
            frame.rect(after + 12.0, middle - 14.0, 260.0, 28.0),
            18.0 * s,
            if ticked { color::TEXT } else { color::MUTED },
            FontWeight::Regular,
            TextAlign::Start,
        );
        ui.hit_region(
            TICK_TOKEN,
            frame.rect(TEXT_X - 8.0, foot, 300.0, BUTTON_HEIGHT),
        );
        let right = CARD_X + CARD_WIDTH - MARGIN;
        kit::button(
            ui,
            &frame,
            [right - PRIMARY_WIDTH, foot, PRIMARY_WIDTH, BUTTON_HEIGHT],
            button,
            true,
            true,
            false,
            PRIMARY_TOKEN,
        );
        kit::button(
            ui,
            &frame,
            [
                right - PRIMARY_WIDTH - BUTTON_GAP - CLOSE_WIDTH,
                foot,
                CLOSE_WIDTH,
                BUTTON_HEIGHT,
            ],
            "Not now",
            false,
            true,
            false,
            CLOSE_TOKEN,
        );
        let [right, keys_y] = frame.point(KEYS_RIGHT, KEYS_Y);
        let hints: [(&[&str], &str); 3] = [
            (&["D"], "don't suggest"),
            (&["Esc"], "not now"),
            (
                &["Enter"],
                if facts.vanilla {
                    "turn off"
                } else {
                    "use Vanilla"
                },
            ),
        ];
        let width: f32 = hints
            .iter()
            .map(|(keys, label)| key_hint_width(keys, label, s) + 18.0 * s)
            .sum();
        let mut x = right - width;
        for (keys, label) in hints {
            x = key_hint(ui, keys, label, x, keys_y, s) + 18.0 * s;
        }
        ui.finish(PRIMARY_TOKEN);
        target.append(&self.ui, viewport);
    }
}

impl crate::GpuState {
    /// What the card shows this frame.
    fn fps_card_facts(&self) -> Facts {
        let console = self.console.as_ref();
        Facts {
            finding: self.fps_help.finding,
            vanilla: console.is_some_and(crate::graphics_quality::vanilla),
            suggest: console
                .and_then(|console| console.bool_cvar(super::CVAR))
                .unwrap_or(true),
            on_server: self.live_session.is_some(),
        }
    }

    /// Whether the card shows this frame: asked for, and the player on a menu or the
    /// game menu with nothing else over it. `covered` is another card or the console.
    pub(crate) fn prepare_fps_card(&mut self, covered: bool) -> bool {
        let menu_visible = self
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_visible);
        let console_open = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::is_open);
        let open = self.fps_card.wants()
            && (menu_visible || self.game_menu)
            && !covered
            && !console_open
            && !self.text_dialog.is_open()
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
            && !self.pending_map_reload;
        self.fps_card.set_open(open);
        open
    }

    /// Draw the card. It darkens everything, so every font batch appended before it is
    /// dropped (text draws above all shapes).
    pub(crate) fn append_fps_card(&mut self, viewport: [f32; 2]) {
        let facts = self.fps_card_facts();
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        self.game_fonts.clear_text();
        let target = crate::ingame_menu::sjk_view::text_target(
            &mut self.game_fonts,
            &mut self.text_vertices,
            &self.ui_font,
        );
        self.fps_card.append(target, viewport, facts);
    }

    /// Act on the player's choice on the card.
    pub(crate) fn fps_card_choice(&mut self, choice: Option<Choice>) {
        // Back to play when the card opened the game menu for itself.
        if matches!(choice, Some(Choice::Switch | Choice::Close))
            && std::mem::take(&mut self.fps_help.opened_menu)
        {
            self.game_menu = false;
            self.sync_cursor_policy();
        }
        let Some(console) = &mut self.console else {
            return;
        };
        match choice {
            Some(Choice::Switch) => {
                crate::graphics_quality::toggle_vanilla(console);
                let _ = console.queue_command("vid_restart");
                crate::log::progress(format_args!(
                    "fps help: Vanilla graphics {}",
                    if crate::graphics_quality::vanilla(console) {
                        "on"
                    } else {
                        "off"
                    }
                ));
            }
            Some(Choice::Tick) => {
                let suggest = console.bool_cvar(super::CVAR).unwrap_or(true);
                console.set_cvar(super::CVAR, if suggest { "0" } else { "1" });
            }
            Some(Choice::Close) | None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_phases::Sample;

    fn heavy() -> Facts {
        Facts {
            finding: Some(super::super::Finding {
                sample: Sample {
                    frame_ms: 8.0,
                    light_ms: 5.6,
                },
                target_fps: 144,
            }),
            vanilla: false,
            suggest: true,
            on_server: true,
        }
    }

    #[test]
    fn the_keys_choose_and_the_tick_keeps_the_card() {
        let mut card = Card::default();
        assert!(!card.wants());
        card.show(Mode::Offered);
        assert_eq!(card.key(KeyCode::KeyD), Some(Choice::Tick));
        assert!(card.wants());
        assert_eq!(card.key(KeyCode::KeyA), None);
        assert_eq!(card.key(KeyCode::Escape), Some(Choice::Close));
        assert!(!card.wants());
        card.show(Mode::Asked);
        assert_eq!(card.key(KeyCode::Enter), Some(Choice::Switch));
        assert!(!card.wants());
    }

    #[test]
    fn the_words_follow_the_finding_and_the_switch() {
        let mut card = Card::default();
        card.show(Mode::Offered);
        let (headline, paragraphs, button) = card.words(heavy());
        assert_eq!(headline, "This map is heavy on SJK's lighting");
        assert!(paragraphs[0].contains("8.0 ms"), "{paragraphs:?}");
        assert!(paragraphs[0].contains("5.6 ms"), "{paragraphs:?}");
        assert!(paragraphs[0].ends_with("about 125 FPS."), "{paragraphs:?}");
        assert!(paragraphs[1].ends_with("you stay on the server."));
        assert_eq!(button, "Use Vanilla graphics");
        // Asked, with nothing measured yet, in the menu.
        card.show(Mode::Asked);
        let facts = Facts {
            finding: None,
            on_server: false,
            ..heavy()
        };
        let (headline, paragraphs, _) = card.words(facts);
        assert_eq!(headline, "Low FPS?");
        assert_eq!(paragraphs.len(), 1);
        assert!(paragraphs[0].ends_with("The menu map loads again in a few seconds."));
        // Vanilla on: the button turns it off.
        let (headline, _, button) = card.words(Facts {
            vanilla: true,
            ..heavy()
        });
        assert_eq!(headline, "Vanilla graphics are on");
        assert_eq!(button, "Turn Vanilla graphics off");
    }
}

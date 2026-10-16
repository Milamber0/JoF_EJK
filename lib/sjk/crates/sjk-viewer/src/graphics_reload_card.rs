//! The reload card (`docs/sjk-ui.md`, "Graphics reload"): when a setting the graphics
//! read only at their start changed and the player is back on the main menu or the
//! game menu, an SJK pop-up card over the darkened screen names what changed and offers
//! Reload now (gold, Enter) or Later (Escape). Reload now queues `vid_restart`, which
//! builds the world on show again on the new settings ([`super`]); Later puts the card
//! away until another such setting changes. While the menu map is built again the card
//! stays, without its buttons, until the new world takes over; on a server the loading
//! screen shows instead. It is drawn in the SJK UI's look in every menu style, and the
//! menus under it are not drawn and take no input, as under the other cards.

use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{FontWeight, InputEvent, TextAlign, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Reload now and Later.
const RELOAD_TOKEN: u16 = 1;
const LATER_TOKEN: u16 = 2;

/// The card's left edge, top and width; it is as tall as what it holds.
const CARD_X: f32 = 580.0;
const CARD_TOP: f32 = 372.0;
const CARD_WIDTH: f32 = 760.0;
const MARGIN: f32 = 44.0;
const TEXT_X: f32 = CARD_X + MARGIN;
const TEXT_WIDTH: f32 = CARD_WIDTH - MARGIN * 2.0;
/// The explanation's first line, its step and most lines, and characters a line.
const BODY_TOP: f32 = CARD_TOP + 134.0;
const LINE: f32 = 28.0;
const LINES: usize = 3;
const LINE_CHARS: usize = 84;
/// The buttons along the card's foot, this far under the explanation.
const BUTTONS_GAP: f32 = 30.0;
const BUTTON_HEIGHT: f32 = 46.0;
const BUTTON_GAP: f32 = 14.0;
const RELOAD_WIDTH: f32 = 200.0;
const LATER_WIDTH: f32 = 150.0;
/// The keys' line, as on the other screens.
const KEYS_Y: f32 = 992.0;
const KEYS_RIGHT: f32 = 1824.0;

/// What the player chose on the card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Choice {
    Reload,
    Later,
}

pub(crate) struct Card {
    /// What a reload would change, in Settings' words; empty when nothing.
    changes: Vec<&'static str>,
    /// The graphics quality level the settings are, which the card names instead.
    level: Option<&'static str>,
    /// The player chose Later for these changes.
    later: bool,
    /// The world on show is being built again.
    reloading: bool,
    /// Shown this frame.
    open: bool,
    ui: MenuCanvas,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            changes: Vec::new(),
            level: None,
            later: false,
            reloading: false,
            open: false,
            ui: MenuCanvas::with_capacities(16, 160, 64),
        }
    }
}

impl Card {
    /// The settings changed: `changes` is what a reload would change now, and `level`
    /// the graphics quality level they are, if one. New changes are offered again after
    /// a Later; none take the offer back.
    pub(crate) fn offer(&mut self, changes: &[&'static str], level: Option<&'static str>) {
        if changes != self.changes.as_slice() || level != self.level {
            self.later = false;
            self.changes = changes.to_vec();
            self.level = level;
        }
    }

    /// The world on show is being built again (`true`), or that is over: the reload
    /// either changed everything it offered or failed (`false`). A failed reload is not
    /// offered again until another setting changes.
    pub(crate) fn reloading(&mut self, on: bool) {
        self.reloading = on;
        if !on {
            self.later = true;
        }
    }

    /// The reload is done: the graphics run with the settings.
    pub(crate) fn reloaded(&mut self) {
        self.reloading = false;
        self.changes.clear();
        self.later = false;
    }

    /// Whether the card has something to show.
    pub(crate) fn wants(&self) -> bool {
        self.reloading || (!self.changes.is_empty() && !self.later)
    }

    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Enter or Space take Reload now, Escape or Backspace Later; every other key is
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
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Choice::Reload,
            KeyCode::Escape | KeyCode::Backspace => Choice::Later,
            _ => return None,
        };
        self.choose(choice)
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> Option<Choice> {
        let event = self.ui.pointer(event)?;
        if event.kind != UiEventKind::Activate {
            return None;
        }
        match event.token {
            Some(RELOAD_TOKEN) => self.choose(Choice::Reload),
            Some(LATER_TOKEN) => self.choose(Choice::Later),
            _ => None,
        }
    }

    /// Nothing is chosen while the world is being built again.
    fn choose(&mut self, choice: Choice) -> Option<Choice> {
        if self.reloading {
            return None;
        }
        self.later = true;
        Some(choice)
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// What the card says: its headline and explanation. `on_server` tells the player the
    /// connection stays.
    fn words(&self, on_server: bool) -> (&'static str, String) {
        if self.reloading {
            return (
                "Reloading the graphics...",
                "The menu map is being built again on your settings; this takes a few seconds."
                    .to_owned(),
            );
        }
        let vanilla = self.level == Some(crate::graphics_quality::Level::Vanilla.label());
        let what = match self.level {
            Some(level) if vanilla => format!("{level} graphics"),
            Some(level) => format!("{level} graphics quality"),
            None => super::summary(&self.changes),
        };
        let verb = if !vanilla && (self.level.is_some() || self.changes.len() == 1) {
            "applies"
        } else {
            "apply"
        };
        let stay = if on_server {
            "The map loads again in a few seconds; you stay on the server."
        } else {
            "The menu map loads again; it takes a few seconds."
        };
        (
            "Reload the graphics?",
            format!("{what} {verb} after a graphics reload. {stay}"),
        )
    }

    /// Draw the card over the whole frame.
    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2], on_server: bool) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        let (headline, explanation) = self.words(on_server);
        let lines: Vec<&str> = wrap(&explanation, LINE_CHARS).take(LINES).collect();
        let body_bottom = BODY_TOP + lines.len() as f32 * LINE;
        let buttons_y = body_bottom + BUTTONS_GAP;
        let height = if self.reloading {
            body_bottom + MARGIN - CARD_TOP
        } else {
            buttons_y + BUTTON_HEIGHT + MARGIN - CARD_TOP
        };
        let ui = &mut self.ui;
        ui.begin_transparent(viewport);
        kit::scrim(ui, viewport);
        kit::card(ui, &frame, [CARD_X, CARD_TOP, CARD_WIDTH, height]);
        text(
            ui,
            TextFamily::Display,
            format_args!("Graphics"),
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
        if !self.reloading {
            let right = CARD_X + CARD_WIDTH - MARGIN;
            kit::button(
                ui,
                &frame,
                [right - RELOAD_WIDTH, buttons_y, RELOAD_WIDTH, BUTTON_HEIGHT],
                "Reload now",
                true,
                true,
                false,
                RELOAD_TOKEN,
            );
            kit::button(
                ui,
                &frame,
                [
                    right - RELOAD_WIDTH - BUTTON_GAP - LATER_WIDTH,
                    buttons_y,
                    LATER_WIDTH,
                    BUTTON_HEIGHT,
                ],
                "Later",
                false,
                true,
                false,
                LATER_TOKEN,
            );
            let [right, keys_y] = frame.point(KEYS_RIGHT, KEYS_Y);
            let later = key_hint_width(&["Esc"], "later", s);
            let reload = key_hint_width(&["Enter"], "reload now", s);
            let x = right - later - reload - 18.0 * s;
            let x = key_hint(ui, &["Enter"], "reload now", x, keys_y, s);
            key_hint(ui, &["Esc"], "later", x + 18.0 * s, keys_y, s);
        }
        ui.finish(RELOAD_TOKEN);
        target.append(&self.ui, viewport);
    }
}

impl crate::GpuState {
    /// Whether the card shows this frame: something to reload (or a rebuild under way),
    /// and the player on the main menu or the game menu with nothing else over it.
    /// `covered` is another card or the console over the frame.
    pub(crate) fn prepare_reload_card(&mut self, covered: bool) -> bool {
        self.sync_graphics_reload();
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
        let open = self.graphics_reload.card.wants()
            && place
            && !covered
            && !console_open
            && !self.text_dialog.is_open()
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
            && !self.pending_map_reload;
        self.graphics_reload.card.set_open(open);
        open
    }

    /// Draw the card. It darkens everything, so every font batch appended before it is
    /// dropped (text draws above all shapes).
    pub(crate) fn append_reload_card(&mut self, viewport: [f32; 2]) {
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        self.game_fonts.clear_text();
        let on_server = self.live_session.is_some();
        let target = crate::ingame_menu::sjk_view::text_target(
            &mut self.game_fonts,
            &mut self.text_vertices,
            &self.ui_font,
        );
        self.graphics_reload
            .card
            .append(target, viewport, on_server);
    }

    /// Act on the player's choice on the card.
    pub(crate) fn reload_card_choice(&mut self, choice: Option<Choice>) {
        if choice != Some(Choice::Reload) {
            return;
        }
        if let Some(console) = &mut self.console {
            let _ = console.queue_command("vid_restart");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_waits_for_another_change() {
        let mut card = Card::default();
        assert!(!card.wants());
        card.offer(&["sun and sky"], None);
        assert!(card.wants());
        assert_eq!(card.key(KeyCode::Escape), Some(Choice::Later));
        assert!(!card.wants());
        // The same changes again: still put away.
        card.offer(&["sun and sky"], None);
        assert!(!card.wants());
        card.offer(&["sun and sky", "sun shadows"], None);
        assert!(card.wants());
        // Back to the running settings: nothing to offer.
        card.offer(&[], Some("High"));
        assert!(!card.wants());
    }

    #[test]
    fn nothing_is_chosen_while_reloading() {
        let mut card = Card::default();
        card.offer(&["HDR"], None);
        assert_eq!(card.key(KeyCode::Enter), Some(Choice::Reload));
        card.reloading(true);
        assert!(card.wants());
        assert_eq!(card.key(KeyCode::Enter), None);
        assert_eq!(card.key(KeyCode::Escape), None);
        card.reloaded();
        assert!(!card.wants());
        // Other keys do nothing.
        card.offer(&["FXAA"], None);
        assert_eq!(card.key(KeyCode::KeyA), None);
        assert!(card.wants());
    }

    #[test]
    fn the_words_name_the_changes() {
        let mut card = Card::default();
        card.offer(&["sun and sky"], None);
        let (headline, words) = card.words(true);
        assert_eq!(headline, "Reload the graphics?");
        assert!(
            words.starts_with("Sun and sky applies after a graphics reload."),
            "{words}"
        );
        assert!(words.ends_with("you stay on the server."), "{words}");
        card.offer(&["HDR", "FXAA"], None);
        assert!(card.words(false).1.starts_with("HDR and FXAA apply"));
        // A graphics quality level is named.
        card.offer(&["HDR", "FXAA", "sun and sky"], Some("Vanilla"));
        assert!(card.words(false).1.starts_with("Vanilla graphics apply"));
        card.offer(&["HDR", "FXAA", "sun and sky"], Some("Balanced"));
        assert!(
            card.words(false)
                .1
                .starts_with("Balanced graphics quality applies")
        );
    }
}

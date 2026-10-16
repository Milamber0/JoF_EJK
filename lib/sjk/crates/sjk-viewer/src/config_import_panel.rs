//! The Import page: what a dropped `.cfg` holds (`config_import.rs`), one tick per
//! part, and Enter to copy the ticked parts. Opened by dropping a file on the
//! window, by `firstsetup import` or by First setup's "Import from another
//! client" row, which lists the configs the clients installed beside SJK wrote
//! ([`crate::config_import::detect`]); Browse (B) picks the file with the system's
//! file dialog. After the import it asks for the gameplay FPS, with the old
//! config's frame rate picked (`gameplay_fps.rs`); the `gameplayfps` command and
//! the Gameplay FPS setting open that step on its own. Like the Update page it
//! lives in the console and is drawn in place of it, so it opens over the menus
//! and in a match. It is drawn as the SJK UI's pop-up card in every menu style
//! (`config_import_panel_sjk.rs`, `config_import_panel_fps.rs`).

use crate::config_import::{Detected, Found, Item};
use crate::gameplay_fps::Choice;
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use sjk_ui::{FontWeight, InputEvent, UiEventKind};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

#[path = "config_import_panel_fps.rs"]
mod fps;
#[path = "config_import_panel_sjk.rs"]
mod sjk;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Pointer targets of the footer's actions; the rows use their index.
const IMPORT_TOKEN: u16 = 930;
const TICK_TOKEN: u16 = 931;
const BROWSE_TOKEN: u16 = 932;
/// The gameplay FPS choices' tiles: the presets, then "Other".
const FPS_TOKEN: u16 = 940;

/// Ask the player for a config with the system's file dialog; it blocks until the
/// dialog closes, so it runs on a worker thread.
fn pick_config() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a config to import")
        .add_filter("Configs (.cfg)", &["cfg"])
        .pick_file()
}

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Copy the ticked parts ([`Panel::chosen`]).
    Import,
    /// Read a config another client wrote, offered on the page.
    ImportDetected(PathBuf),
    /// Set the gameplay FPS to this, then [`Panel::fps_done`].
    UseFps(i64),
}

/// What the gameplay FPS step shows besides the choice, read when it opens.
#[derive(Clone, Debug, Default)]
pub(crate) struct FpsInfo {
    /// The FPS cap (`com_maxfps`) as Settings shows it.
    pub(crate) cap: String,
    /// The server's step while it is `pmove_fixed`.
    pub(crate) server_fixed: Option<i32>,
    /// Why the card opened by itself: the inputs were not going through.
    pub(crate) alert: Option<String>,
    /// The known server family whose cap [`Self::server_fixed`] is, which
    /// Ignore server caps lifts ([`crate::server_caps`]).
    pub(crate) cap_name: Option<&'static str>,
}

/// What the file dialog gave once it closed ([`Panel::take_browsed`]).
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Browsed {
    File(PathBuf),
    /// Closed without a file (no reason), or broke off (the reason).
    Nothing(Option<String>),
}

enum State {
    /// Opened without a file: the configs found on this computer, or how to give one.
    Waiting,
    Loaded(Found),
    Failed(String),
    /// The gameplay FPS: after an import, or on its own (`gameplayfps`).
    Fps {
        choice: Choice,
        info: FpsInfo,
        /// What the import changed, shown over the choice and again at the end;
        /// `None` when the step opened on its own.
        imported: Option<Vec<String>>,
    },
    /// Imported; the lines say what changed.
    Done(Vec<String>),
}

pub(crate) struct Panel {
    open: bool,
    /// The page opened the console, so closing the page closes it too.
    owns_console: bool,
    /// The file's name, without its folder.
    file: String,
    state: State,
    /// The parts found, in [`Item::ALL`] order, and whether each is ticked.
    rows: Vec<(Item, bool)>,
    /// The configs other clients wrote, offered while no file is given.
    detected: Vec<Detected>,
    /// Where they are looked for: the game folder ([`Self::set_game_data`]) and
    /// `My Games`.
    game_data: Option<PathBuf>,
    my_games: Option<PathBuf>,
    selected: usize,
    ui: MenuCanvas,
    /// The file dialog while it is open, answering on its worker.
    browsing: Option<Receiver<Option<PathBuf>>>,
    /// What opens the file dialog ([`pick_config`]; tests put their own).
    picker: fn() -> Option<PathBuf>,
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            owns_console: false,
            file: String::new(),
            state: State::Waiting,
            rows: Vec::with_capacity(Item::ALL.len()),
            detected: Vec::new(),
            game_data: None,
            my_games: None,
            selected: 0,
            ui: MenuCanvas::with_text_capacity(64),
            browsing: None,
            picker: pick_config,
        }
    }

    /// The game folder, where retail and portable clients keep their configs.
    pub(crate) fn set_game_data(&mut self, game_data: &std::path::Path) {
        self.game_data = Some(game_data.to_path_buf());
        self.my_games = crate::config_import::my_games();
    }

    /// Whether configs from other clients were found when the page last opened
    /// without a file.
    pub(crate) fn has_detected(&self) -> bool {
        !self.detected.is_empty()
    }

    /// Browse: open the system's file dialog on a worker thread; the file chosen is
    /// read as a dropped one is ([`Self::take_browsed`]). Nothing happens while one
    /// is open.
    pub(crate) fn browse(&mut self) {
        if self.browsing.is_some() {
            return;
        }
        let picker = self.picker;
        let (outbox, result) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("sjk-file-dialog".to_owned())
            .spawn(move || {
                let _ = outbox.send(picker());
            });
        match spawned {
            Ok(_) => self.browsing = Some(result),
            Err(error) => self.fail(format!("Cannot open the file dialog: {error}.")),
        }
    }

    /// What the file dialog gave, once it has closed.
    pub(crate) fn take_browsed(&mut self) -> Option<Browsed> {
        let browsed = match self.browsing.as_ref()?.try_recv() {
            Ok(Some(path)) => Browsed::File(path),
            Ok(None) => Browsed::Nothing(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Browsed::Nothing(Some("The file dialog closed without a file.".to_owned()))
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return None,
        };
        self.browsing = None;
        Some(browsed)
    }

    /// Say why nothing was imported.
    pub(crate) fn fail(&mut self, reason: String) {
        self.state = State::Failed(reason);
        self.rows.clear();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page with `file`'s parts (`None` before a file is given);
    /// `owns_console` when the console was closed before it. Every part found
    /// starts ticked.
    pub(crate) fn open(
        &mut self,
        owns_console: bool,
        file: String,
        found: Option<Result<Found, String>>,
    ) {
        // A file dropped while the page is up keeps the console's owner.
        self.owns_console = if self.open {
            self.owns_console
        } else {
            owns_console
        };
        self.open = true;
        self.file = file;
        self.selected = 0;
        self.rows.clear();
        self.detected.clear();
        self.state = match found {
            None => {
                self.detected = crate::config_import::detect(
                    self.game_data.as_deref(),
                    self.my_games.as_deref(),
                );
                State::Waiting
            }
            Some(Err(reason)) => State::Failed(reason),
            Some(Ok(found)) => {
                self.rows.extend(
                    Item::ALL
                        .into_iter()
                        .filter(|item| found.has(*item))
                        .map(|item| (item, true)),
                );
                State::Loaded(found)
            }
        };
    }

    /// Show the gameplay FPS step on its own (`gameplayfps`, the Gameplay FPS
    /// setting); `owns_console` as for [`Self::open`].
    pub(crate) fn open_fps(&mut self, owns_console: bool, choice: Choice, info: FpsInfo) {
        self.owns_console = if self.open {
            self.owns_console
        } else {
            owns_console
        };
        self.open = true;
        self.file.clear();
        self.rows.clear();
        self.state = State::Fps {
            choice,
            info,
            imported: None,
        };
    }

    /// The card opened because the inputs were not going through: say so and
    /// pick `suggested`.
    pub(crate) fn alert_fps(&mut self, alert: String, suggested: i64) {
        if let State::Fps { choice, info, .. } = &mut self.state {
            info.alert = Some(alert);
            choice.suggest(suggested);
        }
    }

    /// The import is done (its `lines`): ask for the gameplay FPS with what
    /// the file said about its frame rate.
    pub(crate) fn ask_fps(&mut self, lines: Vec<String>, choice: Choice, info: FpsInfo) {
        self.rows.clear();
        self.state = State::Fps {
            choice,
            info,
            imported: Some(lines),
        };
    }

    /// The gameplay FPS was set (`line` says to what): after an import the page
    /// shows everything that changed; returns true when it opened on its own and
    /// is to close.
    pub(crate) fn fps_done(&mut self, line: Option<String>) -> bool {
        let State::Fps { imported, .. } = &mut self.state else {
            return false;
        };
        match imported.take() {
            Some(mut lines) => {
                lines.extend(line);
                self.state = State::Done(lines);
                false
            }
            None => true,
        }
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    /// The loaded file's frame rate, for the gameplay FPS step after the import.
    pub(crate) fn origin(&self) -> Option<crate::gameplay_fps::Origin> {
        let State::Loaded(found) = &self.state else {
            return None;
        };
        Some(crate::config_import::origin(&self.file, found))
    }

    /// The loaded parts and the ticked items, for the import.
    pub(crate) fn chosen(&self) -> Option<(&Found, Vec<Item>)> {
        let State::Loaded(found) = &self.state else {
            return None;
        };
        let items: Vec<Item> = self
            .rows
            .iter()
            .filter(|(_, ticked)| *ticked)
            .map(|(item, _)| *item)
            .collect();
        (!items.is_empty()).then_some((found, items))
    }

    /// Show what the import changed.
    #[cfg(test)]
    pub(crate) fn finish(&mut self, lines: Vec<String>) {
        self.state = State::Done(lines);
        self.rows.clear();
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    fn toggle(&mut self, row: usize) {
        if let Some((_, ticked)) = self.rows.get_mut(row) {
            *ticked = !*ticked;
        }
    }

    /// What Enter does: import while a file's parts are on show, read the chosen
    /// config while some are offered, use the gameplay FPS chosen, close otherwise.
    fn primary(&self) -> PanelAction {
        match &self.state {
            State::Loaded(_) if self.chosen().is_some() => PanelAction::Import,
            State::Loaded(_) => PanelAction::None,
            State::Waiting if !self.detected.is_empty() => self
                .detected
                .get(self.selected)
                .map_or(PanelAction::None, |config| {
                    PanelAction::ImportDetected(config.path.clone())
                }),
            State::Fps { choice, .. } => choice
                .value()
                .map_or(PanelAction::None, PanelAction::UseFps),
            _ => PanelAction::Close,
        }
    }

    #[cfg(test)]
    pub(crate) fn primary_for_shot(&self) -> PanelAction {
        self.primary()
    }

    #[cfg(test)]
    pub(crate) fn type_fps_for_shot(&mut self, digits: &str) {
        if let State::Fps { choice, .. } = &mut self.state {
            digits.chars().for_each(|digit| choice.type_digit(digit));
        }
    }

    /// Escape: leave the gameplay FPS as it is (after an import, show what the
    /// import changed), or close.
    fn back(&mut self) -> PanelAction {
        if matches!(self.state, State::Fps { .. }) && !self.fps_done(None) {
            return PanelAction::None;
        }
        PanelAction::Close
    }

    /// Keys of the gameplay FPS step; false when the key is not one of its own.
    fn fps_key(&mut self, key: KeyCode) -> bool {
        let State::Fps { choice, .. } = &mut self.state else {
            return false;
        };
        let digit = match key {
            KeyCode::Digit0 | KeyCode::Numpad0 => Some('0'),
            KeyCode::Digit1 | KeyCode::Numpad1 => Some('1'),
            KeyCode::Digit2 | KeyCode::Numpad2 => Some('2'),
            KeyCode::Digit3 | KeyCode::Numpad3 => Some('3'),
            KeyCode::Digit4 | KeyCode::Numpad4 => Some('4'),
            KeyCode::Digit5 | KeyCode::Numpad5 => Some('5'),
            KeyCode::Digit6 | KeyCode::Numpad6 => Some('6'),
            KeyCode::Digit7 | KeyCode::Numpad7 => Some('7'),
            KeyCode::Digit8 | KeyCode::Numpad8 => Some('8'),
            KeyCode::Digit9 | KeyCode::Numpad9 => Some('9'),
            _ => None,
        };
        match (key, digit) {
            (_, Some(digit)) => choice.type_digit(digit),
            (KeyCode::Backspace, _) => choice.backspace(),
            (KeyCode::ArrowLeft | KeyCode::ArrowUp, _) => choice.step(-1),
            (KeyCode::ArrowRight | KeyCode::ArrowDown | KeyCode::Tab, _) => choice.step(1),
            _ => return false,
        }
        true
    }

    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        if self.fps_key(key) {
            return PanelAction::None;
        }
        let count = match self.state {
            State::Waiting => self.detected.len(),
            _ => self.rows.len(),
        };
        match key {
            KeyCode::Escape => return self.back(),
            // Without a file to import or one found to read, Enter browses for one.
            KeyCode::Enter | KeyCode::NumpadEnter
                if matches!(self.state, State::Failed(_))
                    || matches!(self.state, State::Waiting) && self.detected.is_empty() =>
            {
                self.browse();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => return self.primary(),
            KeyCode::KeyB if !matches!(self.state, State::Done(_) | State::Fps { .. }) => {
                self.browse();
            }
            KeyCode::ArrowUp if count > 0 => self.selected = (self.selected + count - 1) % count,
            KeyCode::ArrowDown | KeyCode::Tab if count > 0 => {
                self.selected = (self.selected + 1) % count;
            }
            KeyCode::Space | KeyCode::ArrowLeft | KeyCode::ArrowRight
                if matches!(self.state, State::Loaded(_)) =>
            {
                self.toggle(self.selected);
            }
            _ => {}
        }
        PanelAction::None
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        if let (State::Fps { choice, .. }, Some(token)) = (&mut self.state, event.token)
            && (FPS_TOKEN..=FPS_TOKEN + crate::gameplay_fps::OTHER as u16).contains(&token)
        {
            choice.select(usize::from(token - FPS_TOKEN));
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => self.back(),
            Some(IMPORT_TOKEN) => self.primary(),
            Some(TICK_TOKEN) => {
                self.toggle(self.selected);
                PanelAction::None
            }
            Some(BROWSE_TOKEN) => {
                self.browse();
                PanelAction::None
            }
            Some(row)
                if matches!(self.state, State::Waiting)
                    && usize::from(row) < self.detected.len() =>
            {
                self.selected = usize::from(row);
                self.primary()
            }
            Some(row) if usize::from(row) < self.rows.len() => {
                self.selected = usize::from(row);
                self.toggle(self.selected);
                PanelAction::None
            }
            _ => PanelAction::None,
        }
    }

    /// The value a row shows for `item`.
    fn value(found: &Found, item: Item) -> String {
        match item {
            Item::Name => plain(found.name.as_deref().unwrap_or_default()),
            Item::Model => {
                let model = found.model.as_deref().unwrap_or_default();
                if found.colors.iter().any(Option::is_some) {
                    format!("{model} + tint")
                } else {
                    model.to_owned()
                }
            }
            Item::Fov => found.fov.clone().unwrap_or_default(),
            Item::FpsCap => match found.max_fps {
                Some(0) => "uncapped".to_owned(),
                Some(fps) => format!("{fps} frames a second"),
                None => String::new(),
            },
            Item::Binds => format!(
                "{} key{}, {}",
                found.binds.len(),
                if found.binds.len() == 1 { "" } else { "s" },
                if found.whole_table {
                    "replace yours"
                } else {
                    "added to yours"
                }
            ),
        }
    }
}

/// The note under the parts when `count` bindings name keys SJK does not know.
fn unknown_note(count: usize) -> String {
    if count == 1 {
        "1 binding uses a key SJK does not know and is left out.".to_owned()
    } else {
        format!("{count} bindings use keys SJK does not know and are left out.")
    }
}

/// `text` without its `^0`..`^9` colour codes.
pub(crate) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
            continue;
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_import::parse;

    fn loaded(text: &str) -> Panel {
        let mut panel = Panel::new();
        panel.open(true, "jampconfig.cfg".to_owned(), Some(Ok(parse(text))));
        panel
    }

    #[test]
    fn every_part_found_starts_ticked_and_untick_leaves_it_out() {
        let mut panel = loaded("seta name Sol\nbind w +forward\n");
        let rows: Vec<Item> = panel.rows.iter().map(|(item, _)| *item).collect();
        assert_eq!(rows, [Item::Name, Item::Binds]);
        assert_eq!(panel.primary(), PanelAction::Import);
        panel.toggle(0);
        assert_eq!(panel.chosen().unwrap().1, [Item::Binds]);
        panel.toggle(1);
        assert!(panel.chosen().is_none());
        assert_eq!(panel.primary(), PanelAction::None);
    }

    #[test]
    fn a_failed_or_finished_page_closes_on_enter() {
        let mut panel = Panel::new();
        panel.open(false, "notes.txt".to_owned(), Some(Err("no".to_owned())));
        assert_eq!(panel.primary(), PanelAction::Close);
        let mut panel = loaded("seta name Sol\n");
        panel.finish(vec!["Name: Sol".to_owned()]);
        assert!(panel.chosen().is_none());
        assert_eq!(panel.primary(), PanelAction::Close);
    }

    #[test]
    fn a_second_drop_keeps_who_opened_the_console() {
        let mut panel = loaded("seta name Sol\n");
        panel.open(
            false,
            "other.cfg".to_owned(),
            Some(Ok(parse("seta cg_fov 100"))),
        );
        assert!(panel.close());
    }

    fn wait_for_dialog(panel: &mut Panel) -> Browsed {
        loop {
            if let Some(browsed) = panel.take_browsed() {
                return browsed;
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn browse_gives_the_chosen_file_once() {
        let mut panel = Panel::new();
        panel.picker = || Some(PathBuf::from("C:/games/jampconfig.cfg"));
        panel.open(true, String::new(), None);
        panel.browse();
        assert_eq!(
            wait_for_dialog(&mut panel),
            Browsed::File(PathBuf::from("C:/games/jampconfig.cfg"))
        );
        assert!(panel.take_browsed().is_none());
        panel.picker = || None;
        panel.browse();
        assert_eq!(wait_for_dialog(&mut panel), Browsed::Nothing(None));
    }

    fn fps_panel(imported: Option<Vec<String>>) -> Panel {
        let mut panel = Panel::new();
        let choice = Choice::new(125, None);
        match imported {
            Some(lines) => {
                panel.open(true, "x.cfg".to_owned(), Some(Ok(parse("seta name a"))));
                panel.ask_fps(lines, choice, FpsInfo::default());
            }
            None => panel.open_fps(true, choice, FpsInfo::default()),
        }
        panel
    }

    #[test]
    fn the_fps_step_uses_the_choice_and_ends_on_what_changed() {
        let mut panel = fps_panel(Some(vec!["Name: a".to_owned()]));
        assert_eq!(panel.primary(), PanelAction::UseFps(125));
        assert!(panel.fps_key(KeyCode::ArrowLeft));
        assert_eq!(panel.primary(), PanelAction::UseFps(0), "Auto");
        assert!(panel.fps_key(KeyCode::ArrowLeft));
        // Left from Auto wraps to Other, empty until a value is typed.
        assert_eq!(panel.primary(), PanelAction::None);
        assert!(panel.fps_key(KeyCode::Digit4));
        assert!(panel.fps_key(KeyCode::Numpad0));
        assert!(panel.fps_key(KeyCode::Digit0));
        assert_eq!(panel.primary(), PanelAction::UseFps(400));
        assert!(!panel.fps_done(Some("Gameplay FPS: 400".to_owned())));
        assert!(matches!(&panel.state, State::Done(lines) if lines.len() == 2));
        // On its own the step closes once used, and Escape closes it.
        let mut panel = fps_panel(None);
        assert!(panel.fps_done(Some("x".to_owned())));
        let mut panel = fps_panel(None);
        assert_eq!(panel.back(), PanelAction::Close);
        // After an import, Escape skips to what the import changed.
        let mut panel = fps_panel(Some(Vec::new()));
        assert_eq!(panel.back(), PanelAction::None);
        assert!(matches!(panel.state, State::Done(_)));
    }

    #[test]
    fn names_show_without_colour_codes() {
        assert_eq!(plain("^1Sol^7Fox"), "SolFox");
        assert_eq!(plain("a^b ^^9"), "a^b ^");
    }
}

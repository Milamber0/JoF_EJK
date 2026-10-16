//! What the SJK UI's in-game menu ([`super::sjk_view`]) does where it differs
//! from the shared pages: its main page's entries, the row of icons under the
//! emblem, the match card's controls and the docked SJK chat
//! ([`super::sjk_focus`]) with the keys that move between them and the keys typed in
//! the chat's field, its Leave page, Escape returning to what opened a page, and the
//! match card read from the live session. Siege's classes, the call-vote lists
//! and Team's rows keep the shared actions (`game_menu_actions.rs`).

use super::Page;
use super::sjk_focus::{self, Control, Focus, Icon, Move, Step};
use super::sjk_view::{self, Entry, Local, leave};
use crate::GpuState;
use crate::menu::sjk::chat_dock::DockAction;
use sjk_client::ClientSession;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// `persistant[PERS_SCORE]` and `persistant[PERS_RANK]`.
const PERS_SCORE: usize = 0;
const PERS_RANK: usize = 2;

impl GpuState {
    /// Activate the focused row of an SJK UI page whose rows are its own.
    /// Returns false for rows that keep the shared actions.
    pub(crate) fn activate_sjk_ui_row(&mut self) -> bool {
        let row = self.game_menu_row;
        match self.game_menu_page {
            Page::Main => match self.in_game_menu.focus {
                Focus::List => self.activate_sjk_entry(row),
                Focus::Row(icon) => self.activate_sjk_icon(icon),
                Focus::Card(control) => self.activate_sjk_control(control),
                Focus::Chat => self.in_game_menu.start_chat_typing(),
            },
            // Team's last row is Back; the others choose a side.
            Page::Team if row + 1 >= self.game_menu_row_count() => self.sjk_back(),
            Page::Leave => match row {
                leave::SERVER => self.disconnect_to_menu(),
                leave::QUIT => self.quit_requested = true,
                _ => self.sjk_back(),
            },
            Page::ConfirmLeave | Page::ConfirmQuit if row != 0 => self.sjk_back(),
            Page::ConfirmLeave => self.disconnect_to_menu(),
            Page::ConfirmQuit => self.quit_requested = true,
            Page::About => self.sjk_back(),
            page if page.is_vote_page() => self.activate_sjk_call_vote(page, row),
            _ => return false,
        }
        true
    }

    /// Take entry `row` of the SJK UI's main page.
    fn activate_sjk_entry(&mut self, row: usize) {
        let Some(entry) = Entry::at(row) else {
            return;
        };
        match entry {
            Entry::Resume => self.close_game_menu(),
            Entry::Profile => {
                self.in_game_menu.remember_return(row);
                self.open_profile_hub_from_game(None);
            }
            Entry::Collection => {
                self.in_game_menu.remember_return(row);
                self.open_collection_from_game();
            }
            Entry::Players => self.open_players_page(),
            Entry::Settings => {
                self.in_game_menu.remember_return(row);
                if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
                    menu.open_sjk_settings_from_game(console);
                    self.game_menu = false;
                }
            }
            Entry::Servers => {
                self.in_game_menu.remember_return(row);
                self.open_browser_from_game();
            }
            Entry::Leave => {
                // As the main page's Quit, it opens on Stay: its rows act at once.
                self.open_game_menu_page(Page::Leave);
                self.game_menu_row = leave::STAY;
            }
        }
    }

    /// Take an icon of the row under the emblem. SJK's pages are drawn by the console
    /// over the game menu, which shows again when they close.
    pub(crate) fn activate_sjk_icon(&mut self, icon: Icon) {
        match icon {
            Icon::Camera => {
                self.in_game_menu.focus = Focus::List;
                self.open_shot_panel();
                return;
            }
            Icon::ReportBug => {
                self.open_bug_report();
                return;
            }
            Icon::WhatsNew | Icon::Credits | Icon::Chat | Icon::Staff => {}
        }
        if let Some(console) = &mut self.console {
            match icon {
                Icon::WhatsNew => console.open_changelog(),
                Icon::Credits => console.open_credits(),
                Icon::Chat => console.open_sjk_chat_panel(),
                Icon::Staff => console.open_staff_panel(),
                Icon::Camera | Icon::ReportBug => {}
            }
        }
        self.sync_cursor_policy();
    }

    /// Take a control of the match card: vote or change side (and back to the match,
    /// as retail's pop-ups do), or open Siege's classes or the call-vote page.
    pub(crate) fn activate_sjk_control(&mut self, control: Control) {
        if !self.in_game_menu.sjk_controls().takes(control) {
            return;
        }
        match control {
            Control::VoteYes | Control::VoteNo => {
                if let Some(command) = control
                    .command()
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                {
                    self.send_menu_reliable(command);
                }
                self.close_game_menu();
            }
            Control::Team(team) => self.select_team(team),
            Control::SiegeClass => {
                self.open_siege_classes();
            }
            Control::CallVote => self.open_call_vote(),
        }
    }

    /// A key on the SJK UI's main page: the list, the icons and the card share the
    /// arrows, Tab, Enter and Escape ([`sjk_focus::step`]). Returns false for keys it
    /// leaves to the shared handling.
    pub(crate) fn sjk_main_key(&mut self, key: KeyCode) -> bool {
        if !self.in_game_menu.is_sjk() || self.game_menu_page != Page::Main {
            return false;
        }
        let shift = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::shift_held);
        let movement = match key {
            KeyCode::ArrowUp | KeyCode::KeyW => Move::Up,
            KeyCode::ArrowDown | KeyCode::KeyS => Move::Down,
            KeyCode::ArrowLeft | KeyCode::KeyA => Move::Left,
            KeyCode::ArrowRight | KeyCode::KeyD => Move::Right,
            KeyCode::Tab if shift => Move::BackTab,
            KeyCode::Tab => Move::Tab,
            KeyCode::Escape if self.in_game_menu.focus != Focus::List => {
                self.in_game_menu.focus = Focus::List;
                return true;
            }
            _ => return false,
        };
        let step = sjk_focus::step(
            self.in_game_menu.focus,
            movement,
            self.in_game_menu.sjk_icons(),
            self.in_game_menu.sjk_controls(),
            self.in_game_menu.sjk_chat(),
        );
        match step {
            Step::To(focus) => self.in_game_menu.focus = focus,
            Step::List(forward) => {
                self.game_menu_row = self.in_game_menu.sjk_step(
                    self.game_menu_page,
                    self.game_menu_row,
                    self.game_menu_row_count(),
                    forward,
                );
            }
            Step::Stay => {}
        }
        true
    }

    /// A pointer event on the SJK UI's main page's row of icons, match card or docked
    /// chat (`token`): hovering gives it the keyboard (a chat sender's name only shows
    /// their card), a click acts. Returns false for the other tokens.
    pub(crate) fn sjk_main_pointer(&mut self, kind: sjk_ui::UiEventKind, token: usize) -> bool {
        use sjk_ui::UiEventKind;
        if !self.in_game_menu.is_sjk() || self.game_menu_page != Page::Main {
            return false;
        }
        let hover = matches!(kind, UiEventKind::HoverEnter | UiEventKind::Hover);
        let activate = kind == UiEventKind::Activate;
        if let Some(reply) = self.in_game_menu.chat_pointer(token, activate) {
            if (hover || activate) && super::InGameMenu::chat_takes_focus(token) {
                self.in_game_menu.focus = Focus::Chat;
            }
            if let Some(action) = reply {
                self.act_on_sjk_chat(action);
            }
            return true;
        }
        if let Some(icon) = u16::try_from(token).ok().and_then(sjk_view::icon_of) {
            if hover || activate {
                self.in_game_menu.focus = Focus::Row(icon);
            }
            if activate {
                self.activate_sjk_icon(icon);
            }
            return true;
        }
        if let Some(placed) = self.in_game_menu.sjk_control_of(token) {
            if placed.enabled && (hover || activate) {
                self.in_game_menu.focus = Focus::Card(placed.control);
            }
            if placed.enabled && activate {
                self.activate_sjk_control(placed.control);
            }
            return true;
        }
        false
    }

    /// A key while the SJK UI's docked chat is being typed in: every key goes to its
    /// field, before the console's key and the game's bindings, held keys repeating
    /// (Backspace); a release is swallowed too. Returns false when nobody types there.
    pub(crate) fn sjk_chat_typing(&mut self, event: &KeyEvent) -> bool {
        if !self.game_menu
            || !self.in_game_menu.is_sjk()
            || self.game_menu_page != Page::Main
            || !self.in_game_menu.chat_typing()
        {
            return false;
        }
        if event.state != ElementState::Pressed {
            return true;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return true;
        };
        let (shift, control) = self.console.as_ref().map_or((false, false), |console| {
            (console.shift_held(), console.control_held())
        });
        if let Some(action) = self
            .in_game_menu
            .chat_key(key, event.text.as_deref(), shift, control)
        {
            self.act_on_sjk_chat(action);
        }
        true
    }

    /// Carry out what the docked chat asks: send what was typed, open the SJK chat's
    /// page over the menu (it shows again when the page closes), or mute a sender.
    fn act_on_sjk_chat(&mut self, action: DockAction) {
        match action {
            DockAction::Send => self.in_game_menu.send_chat(),
            DockAction::Open => {
                if let Some(console) = &mut self.console {
                    console.open_sjk_chat_panel();
                }
                self.sync_cursor_policy();
            }
            DockAction::Mute => self.in_game_menu.mute_from_chat(),
        }
    }

    /// A row of the call-vote page or one of its lists: open a list, call the
    /// vote and return to the match, or go back.
    fn activate_sjk_call_vote(&mut self, page: Page, row: usize) {
        match self.in_game_menu.callvote_action(page, row) {
            super::CallVoteAction::Open(list) => self.open_game_menu_page(list),
            super::CallVoteAction::Back => self.sjk_back(),
            super::CallVoteAction::Send(command) => {
                self.send_menu_reliable(&command);
                self.close_game_menu();
            }
            super::CallVoteAction::None => {}
        }
    }

    /// The call-vote page, its maps and players read now.
    fn open_call_vote(&mut self) {
        let game_state = self.live_session.as_ref().map(ClientSession::game_state);
        self.in_game_menu
            .refresh_callvote(game_state, self.vfs.as_deref());
        self.open_game_menu_page(Page::CallVote);
    }

    /// Escape in the SJK UI: back to the entry or card control that opened the page,
    /// or out of the menu from the main page.
    pub(crate) fn sjk_back(&mut self) {
        if self.game_menu_page == Page::ReportPlayer {
            // Back on the reported player's row.
            self.back_to_players();
            return;
        }
        match sjk_view::parent(self.game_menu_page) {
            Some((page, row)) => {
                self.in_game_menu.focus = sjk_view::return_focus(self.game_menu_page);
                self.game_menu_page = page;
                self.game_menu_row = row;
            }
            None => self.close_game_menu(),
        }
    }

    /// Back to the match, anything typed in the docked chat dropped.
    fn close_game_menu(&mut self) {
        self.game_menu = false;
        self.in_game_menu.stop_chat_typing();
        self.capture_pointer();
    }

    /// Read the match card's facts from the live session this frame; without
    /// one (a map explored alone) the card hides.
    pub(crate) fn refresh_game_menu_card(&mut self) {
        let Some(session) = self.live_session.as_ref() else {
            self.in_game_menu.card.forget();
            return;
        };
        let snapshot = session.latest_snapshot();
        let player = &snapshot.player;
        let local = Local {
            team: player.team(),
            spectator: player.is_spectator(),
            score: player.persistent[PERS_SCORE] as i32,
            rank: player.persistent[PERS_RANK],
        };
        self.in_game_menu.card.refresh(
            session.game_state(),
            local,
            snapshot.server_time,
            session.team_scores(),
            self.last_connect_address.as_deref(),
        );
    }
}

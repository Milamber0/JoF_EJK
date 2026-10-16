//! Retained in-game menu: the pages reached with Escape during a match, drawn
//! with `ui_menuStyle sjk` as the SJK UI's arc and match card ([`sjk_view`]),
//! with the SJK chat docked under the match, or with `ui_menuStyle classic` as the
//! retail bar and pop-ups ([`classic`]).

use crate::menu::art::ArtSet;
use crate::menu::sjk::TextTarget;
use crate::menu::sjk::chat_dock::{self, DockAction};
use crate::menu::style::MenuStyle;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use sjk_protocol::{GameState, InfoString};
use sjk_ui::{AbstractAction, DrawList, InputEvent, UiEventKind};

mod about;
mod callvote;
pub(crate) mod classic;
mod classic_actions;
mod classic_view;
pub(crate) mod players;
pub(crate) mod shot;
mod siege;
pub(crate) mod siege_data;
pub(crate) mod sjk;
mod sjk_actions;
pub(crate) mod sjk_focus;
pub(crate) mod sjk_view;
pub(crate) use callvote::Action as CallVoteAction;

const VOTE_SCROLL_TOKEN: u16 = u16::MAX;

/// What players read for the camera and sunlight panel ([`Page::Shot`]): its
/// game-menu entry and title in every style (Shot controls until 08/10/2026).
pub(crate) const CAMERA_CONTROL: &str = "Camera control";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    Main,
    /// Camera control: camera and sun controls over the live world
    /// ([`shot`]).
    Shot,
    Team,
    /// Map/theme constrained Siege class selection.
    Siege,
    /// SJK's own screens ([`sjk`]): the changelog, more later.
    Sjk,
    /// Host, map, game type and limits (`ingame_about` in the stock UI).
    About,
    /// Disconnect or quit, with the choice as the confirmation (classic:
    /// the retail exit pop-up, confirmed on the next two pages).
    Leave,
    /// Classic only: the retail vote pop-up (Yes, No).
    Vote,
    /// Classic only: "Go to Main Menu?" after the exit pop-up's Main Menu.
    ConfirmLeave,
    /// Classic only: "Quit Program?" after the exit pop-up's Quit Program.
    ConfirmQuit,
    CallVote,
    VoteMap,
    VoteGameType,
    VoteKick,
    VoteClientKick,
    VoteWarmup,
    VoteTimeLimit,
    VoteFragLimit,
    /// Everyone on the server, a small scoreboard; choosing one opens
    /// [`Page::ReportPlayer`] ([`players`]).
    Players,
    /// Why the chosen player is reported; a reason opens the text dialog.
    ReportPlayer,
}

pub(crate) struct View<'a> {
    pub(crate) page: Page,
    pub(crate) selected_row: usize,
    pub(crate) team: u8,
    pub(crate) team_game: bool,
    /// The server runs Siege, where retail swaps two bar buttons.
    pub(crate) siege: bool,
    pub(crate) red_players: usize,
    pub(crate) blue_players: usize,
    pub(crate) vote_active: bool,
    /// The player's key is staff at the SJK hub: the SJK UI offers Staff tools.
    pub(crate) staff: bool,
    /// Keeps the struct open for per-page data borrowed from the frame.
    pub(crate) _frame: std::marker::PhantomData<&'a ()>,
}

/// Fixed-storage retained UI state shared by every in-game page.
pub(crate) struct InGameMenu {
    canvas: MenuCanvas,
    rows: [String; 24],
    enabled: [bool; 24],
    row_count: usize,
    callvote: callvote::State,
    about: about::State,
    active_page: Page,
    siege: siege::State,
    pub(crate) shot: shot::Panel,
    /// The Players and Report pages' roster and chosen player.
    pub(crate) players: players::State,
    /// Layout family (`ui_menuStyle`).
    style: MenuStyle,
    /// Retail artwork the classic layout can draw.
    art: ArtSet,
    /// The SJK UI's line under each row (what it opens, who is on a team).
    hints: [String; 24],
    /// The SJK UI's match card.
    card: sjk_view::Card,
    /// The SJK UI's gold mark along its arc.
    motion: sjk_view::Motion,
    /// The SJK UI's main-page row that handed over to another screen, which
    /// the menu comes back on.
    return_row: usize,
    /// Where the keyboard is on the SJK UI's main page: its list, the row of icons
    /// or the match card ([`sjk_focus`]).
    pub(crate) focus: sjk_focus::Focus,
    /// The SJK UI's match card controls and whether Staff tools showed, as the last
    /// frame drew them, for the keys and the pointer.
    controls: sjk_focus::Controls,
    staff: bool,
    /// The SJK UI's docked SJK chat on the main page: what it keeps between frames, its
    /// copy of the chat, whether `cl_sjkChat` is on and whether the last frame docked it.
    chat: chat_dock::Dock,
    chat_cache: chat_dock::DockCache,
    chat_on: bool,
    chat_shown: bool,
    /// The world shots' made-up match facts in place of the frame's.
    #[cfg(test)]
    shot_view: Option<ShotView>,
}

/// The facts a world shot puts in place of the frame's (it has no server).
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShotView {
    pub(crate) team: u8,
    pub(crate) team_game: bool,
    pub(crate) red_players: usize,
    pub(crate) blue_players: usize,
    pub(crate) vote_active: bool,
    /// Staff tools offered (a staff key).
    pub(crate) staff: bool,
    /// The match is Siege.
    pub(crate) siege: bool,
}

impl InGameMenu {
    pub(crate) fn new() -> Self {
        Self {
            canvas: MenuCanvas::new(),
            rows: std::array::from_fn(|_| String::with_capacity(96)),
            enabled: [true; 24],
            row_count: 0,
            callvote: callvote::State::new(),
            about: about::State::new(),
            active_page: Page::Main,
            siege: siege::State::default(),
            shot: shot::Panel::default(),
            players: players::State::default(),
            style: MenuStyle::default(),
            art: ArtSet::default(),
            hints: std::array::from_fn(|_| String::with_capacity(96)),
            card: sjk_view::Card::default(),
            motion: sjk_view::Motion::default(),
            return_row: 0,
            focus: sjk_focus::Focus::List,
            controls: sjk_focus::Controls::none(),
            staff: false,
            chat: chat_dock::Dock::default(),
            chat_cache: chat_dock::DockCache::default(),
            chat_on: false,
            chat_shown: false,
            #[cfg(test)]
            shot_view: None,
        }
    }

    /// Follow the player's `ui_menuStyle`, with the retail artwork `art`
    /// the classic layout can draw.
    pub(crate) fn set_style(&mut self, style: MenuStyle, art: ArtSet) {
        self.style = style;
        self.art = art;
    }

    /// Whether the classic (retail bar and pop-ups) layout is in use.
    pub(crate) fn is_classic(&self) -> bool {
        self.style == MenuStyle::Classic
    }

    /// Whether the SJK UI's layout is in use ([`sjk_view`]).
    pub(crate) fn is_sjk(&self) -> bool {
        self.style == MenuStyle::Sjk
    }

    /// The SJK UI's main-page `row` hands over to another screen (Character,
    /// Settings, Servers): the menu comes back on it.
    pub(crate) fn remember_return(&mut self, row: usize) {
        self.return_row = row;
    }

    /// The row the main page shows when a screen opened from it returns: the
    /// entry that opened it in the SJK UI, the first in the classic look.
    pub(crate) fn return_row(&mut self) -> usize {
        if self.is_sjk() {
            std::mem::take(&mut self.return_row)
        } else {
            0
        }
    }

    /// The icons of the SJK UI's bottom row, as the last frame showed them.
    pub(crate) fn sjk_icons(&self) -> &'static [sjk_focus::Icon] {
        sjk_focus::Icon::shown(self.staff)
    }

    /// The SJK UI's match card controls, as the last frame drew them.
    pub(crate) fn sjk_controls(&self) -> &sjk_focus::Controls {
        &self.controls
    }

    /// Whether the last frame docked the SJK chat on the SJK UI's main page.
    pub(crate) fn sjk_chat(&self) -> bool {
        self.chat_shown
    }

    /// Follow `cl_sjkChat` (`on`): the dock shows only while it is on, its copy of the
    /// chat read again when the chat changed.
    pub(crate) fn sync_chat(&mut self, on: bool) {
        self.chat_on = on;
        if on {
            self.chat_cache.refresh();
        }
    }

    /// Whether the docked chat's field takes the keys.
    pub(crate) fn chat_typing(&self) -> bool {
        self.chat.is_typing()
    }

    /// Start typing in the docked chat's field.
    pub(crate) fn start_chat_typing(&mut self) {
        if self.chat_shown {
            self.chat.start_typing();
        }
    }

    /// Stop typing in the docked chat's field, what was typed dropped.
    pub(crate) fn stop_chat_typing(&mut self) {
        self.chat.stop_typing();
    }

    /// A key while typing in the docked chat's field ([`chat_dock::Dock::typing_key`]).
    pub(crate) fn chat_key(
        &mut self,
        key: winit::keyboard::KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> Option<DockAction> {
        self.chat.typing_key(key, text, shift, control)
    }

    /// The pointer over (or clicking) `token`: `None` when it is not the docked chat's,
    /// else what the dock asks.
    pub(crate) fn chat_pointer(
        &mut self,
        token: usize,
        activate: bool,
    ) -> Option<Option<DockAction>> {
        let token = u16::try_from(token).ok()?;
        if !self.chat_shown {
            return None;
        }
        self.chat.pointer(sjk_view::CHAT_TOKENS, token, activate)
    }

    /// Whether `token` is the docked chat's field or its Open chat, which take the
    /// keyboard when hovered.
    pub(crate) fn chat_takes_focus(token: usize) -> bool {
        token == usize::from(sjk_view::CHAT_TOKENS.field)
            || token == usize::from(sjk_view::CHAT_TOKENS.open)
    }

    /// Hand what Enter sent from the docked chat's field to the hub.
    pub(crate) fn send_chat(&mut self) {
        let text = self.chat.take_draft();
        self.chat_cache.send(text, self.chat_on);
    }

    /// Mute on this PC the player the docked chat's card asked for.
    pub(crate) fn mute_from_chat(&mut self) {
        chat_dock::mute(&mut self.chat);
    }

    /// The card control a pointer token names, as the last frame drew them.
    pub(crate) fn sjk_control_of(&self, token: usize) -> Option<sjk_focus::Placed> {
        let index = token.checked_sub(usize::from(sjk_view::CONTROL_TOKEN))?;
        (index < 16)
            .then(|| self.controls.as_slice().get(index).copied())
            .flatten()
    }

    /// The SJK UI's next row from `current` of `count` on `page`, `forward`
    /// or back, wrapping and passing over rows that cannot be taken (the team
    /// the player is on, a ballot with no vote on).
    pub(crate) fn sjk_step(
        &self,
        page: Page,
        current: usize,
        count: usize,
        forward: bool,
    ) -> usize {
        if count == 0 {
            return 0;
        }
        let takeable =
            |row: usize| page != self.active_page || self.enabled.get(row).copied().unwrap_or(true);
        let mut row = current.min(count - 1);
        for _ in 0..count {
            row = if forward {
                (row + 1) % count
            } else {
                (row + count - 1) % count
            };
            if takeable(row) {
                return row;
            }
        }
        current.min(count - 1)
    }

    /// Draw `view`'s page in the SJK UI's look, its text to `target`.
    pub(crate) fn append_sjk(
        &mut self,
        view: View<'_>,
        target: TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        if view.page == Page::Shot {
            // Camera control has its own column, not the arc.
            self.active_page = Page::Shot;
            self.shot.build_sjk(&mut self.canvas, viewport);
        } else {
            let measure = target.body_measure();
            self.build_sjk_measured(view, viewport, Some(measure));
        }
        target.append(&self.canvas, viewport);
    }

    /// Lay `view`'s page out on the canvas in the SJK UI's look, the docked chat's text
    /// estimated (tests).
    #[cfg(test)]
    fn build_sjk(&mut self, view: View<'_>, viewport: [f32; 2]) {
        self.build_sjk_measured(view, viewport, None);
    }

    /// Lay `view`'s page out on the canvas in the SJK UI's look, the docked chat's text
    /// measured with `measure` (else estimated).
    fn build_sjk_measured(
        &mut self,
        view: View<'_>,
        viewport: [f32; 2],
        measure: Option<crate::sjk_chat_look::Measure<'_>>,
    ) {
        #[cfg(test)]
        let view = match self.shot_view {
            Some(shot) => View {
                team: shot.team,
                team_game: shot.team_game,
                red_players: shot.red_players,
                blue_players: shot.blue_players,
                vote_active: shot.vote_active,
                staff: shot.staff,
                siege: shot.siege,
                ..view
            },
            None => view,
        };
        self.prepare_rows(&view);
        self.active_page = view.page;
        // The main page's row and card: what they offer this frame, and the keyboard
        // kept where it can still be.
        self.staff = view.staff;
        let main = view.page == Page::Main;
        self.controls = if main {
            sjk_focus::Controls::for_match(sjk_focus::Match {
                known: self.card.is_known(),
                team_game: view.team_game,
                siege: view.siege,
                team: view.team,
                vote: view.vote_active,
            })
        } else {
            sjk_focus::Controls::none()
        };
        let icons = sjk_focus::Icon::shown(self.staff);
        // The SJK chat docks on the main page while it is on.
        let chat = main && self.chat_on;
        self.chat_shown = chat;
        if !chat {
            self.chat.hide();
        }
        self.focus = if main {
            self.focus.settle(icons, &self.controls, chat)
        } else {
            sjk_focus::Focus::List
        };
        // The sender card's picture and place, asked before the profile card's lock.
        self.chat
            .picture_sender_card(crate::player_identity::avatar_version);
        self.chat.locate_sender_card(crate::player_mutes::place);
        let mut lines = [chat_dock::BLANK; chat_dock::LINES];
        let chat_view = chat.then(|| chat_dock::ChatDock {
            measure,
            ..self.chat_cache.view(&mut lines)
        });
        let count = self.row_count;
        let rows = sjk_view::Rows {
            labels: &self.rows[..count],
            hints: &self.hints[..count],
            enabled: &self.enabled[..count],
        };
        let mut extras = if main {
            sjk_view::Extras {
                focus: self.focus,
                icons,
                controls: &self.controls,
                chat: chat_view.map(|view| sjk_view::Chat {
                    dock: &mut self.chat,
                    view,
                }),
            }
        } else {
            sjk_view::Extras::NONE
        };
        sjk_view::build(
            &mut self.canvas,
            &view,
            &rows,
            &sjk_view::Sides {
                card: &self.card,
                players: &self.players,
            },
            &mut extras,
            &mut self.motion,
            viewport,
        );
    }

    /// Draw `view`'s page in the classic look.
    pub(crate) fn append(
        &mut self,
        view: View<'_>,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        if view.page == Page::Shot {
            // Camera control has only the SJK UI's look; its text in this font.
            self.active_page = Page::Shot;
            self.shot.build_sjk(&mut self.canvas, viewport);
            self.canvas.append_text(vertices, font, viewport);
            return;
        }
        self.prepare_rows(&view);
        self.active_page = view.page;
        let rows = classic_view::Rows {
            labels: &self.rows[..self.row_count],
            enabled: &self.enabled[..self.row_count],
            scroll: self.callvote.scroll_metrics(view.page),
            info: info_lines(&self.about, &self.players, view.page),
        };
        classic_view::build(&mut self.canvas, &view, rows, self.art, viewport);
        self.canvas.append_text(vertices, font, viewport);
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.canvas.draw_list()
    }
    pub(crate) fn navigate(&mut self, action: AbstractAction) -> Option<usize> {
        if self.active_page.is_vote_page() {
            return None;
        }
        self.canvas.action(action).map(usize::from)
    }
    pub(crate) fn pointer(&mut self, event: InputEvent) -> Option<(UiEventKind, usize)> {
        let wheel = match event {
            InputEvent::PointerWheel { delta, .. } => Some(delta),
            _ => None,
        };
        let event = self.canvas.pointer(event);
        if let Some(delta) = wheel
            && self.active_page.is_vote_page()
        {
            self.callvote
                .scroll(self.active_page, -delta.y.signum() as isize * 3);
            return None;
        }
        let event = event?;
        if event.kind == UiEventKind::Drag && event.token == Some(VOTE_SCROLL_TOKEN) {
            if let (Some(point), Some(track)) =
                (event.position, self.canvas.rect_for(VOTE_SCROLL_TOKEN))
            {
                let ratio = ((point.y - track.y) / track.height).clamp(0.0, 1.0);
                self.callvote.scroll_to(self.active_page, ratio);
            }
            return None;
        }
        Some((event.kind, usize::from(event.token?)))
    }
    pub(crate) fn activation_allowed(&self, row: usize) -> bool {
        self.enabled.get(row).copied().unwrap_or(false)
    }

    /// Rebuild the server-info page from the live game state.
    pub(crate) fn refresh_about(&mut self, game_state: Option<&GameState>, address: Option<&str>) {
        self.about.refresh(game_state, address);
    }

    pub(crate) fn refresh_callvote(
        &mut self,
        game_state: Option<&GameState>,
        vfs: Option<&sjk_vfs::VirtualFileSystem>,
    ) {
        self.callvote.refresh(game_state, vfs);
    }

    pub(crate) fn callvote_action(&mut self, page: Page, row: usize) -> CallVoteAction {
        self.callvote.activate(page, row)
    }

    pub(crate) fn row_count(&self, page: Page, team_game: bool) -> usize {
        if matches!(page, Page::Players | Page::ReportPlayer) {
            self.players.row_count(page)
        } else if let Some(count) =
            classic::row_count(page, team_game).filter(|_| self.is_classic())
        {
            count
        } else if let Some(count) = sjk_view::row_count(page, team_game).filter(|_| self.is_sjk()) {
            count
        } else if page == Page::Siege {
            self.siege.row_count()
        } else if page.is_vote_page() {
            self.callvote.row_count(page)
        } else {
            shared_row_count(page)
        }
    }

    fn prepare_rows(&mut self, view: &View<'_>) {
        for row in &mut self.rows {
            row.clear();
        }
        for hint in &mut self.hints {
            hint.clear();
        }
        self.enabled = [true; 24];
        let own_rows = if self.is_classic() {
            classic::prepare(view, &mut self.rows, &mut self.enabled)
        } else if self.is_sjk() {
            sjk_view::prepare(view, &mut self.rows, &mut self.hints)
        } else {
            None
        };
        self.row_count = if let Some(count) = own_rows {
            count
        } else if matches!(view.page, Page::Players | Page::ReportPlayer) {
            self.players.prepare(
                view.page,
                &mut self.rows,
                &mut self.hints,
                &mut self.enabled,
            )
        } else if view.page == Page::Siege {
            self.siege.prepare(&mut self.rows)
        } else if view.page.is_vote_page() {
            self.callvote.prepare_rows(view.page, &mut self.rows)
        } else {
            self.prepare_standard_rows(view)
        };
        // The current team cannot be taken: the classic pop-up dims it and
        // the SJK UI says so under it.
        for row in 0..self.row_count {
            self.enabled[row] &= !current_team_row(view, row);
        }
        if self.is_sjk() {
            sjk_view::split_hints(
                view.page,
                &mut self.rows[..self.row_count],
                &mut self.hints[..self.row_count],
            );
        }
    }

    /// The rows of a page neither look draws itself: the classic look's
    /// server-info page.
    fn prepare_standard_rows(&mut self, view: &View<'_>) -> usize {
        match view.page {
            Page::About => {
                self.rows[0].push_str("Back");
                1
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
impl InGameMenu {
    /// The SJK UI's in-game menu over a made-up match: `card` for the match
    /// card, `view` in place of the frame's facts (world shots and tests).
    pub(crate) fn sjk_for_shot(&mut self, card: sjk_view::Card, view: ShotView) {
        self.style = MenuStyle::Sjk;
        self.card = card;
        self.shot_view = Some(view);
    }

    /// Whether the last frame ran out of room on the canvas (world shots).
    pub(crate) fn overflowed(&self) -> bool {
        self.canvas.overflowed()
    }

    /// Where the last frame put the pointer target `token` (world shots).
    pub(crate) fn rect_for(&self, token: u16) -> Option<sjk_ui::Rect> {
        self.canvas.rect_for(token)
    }

    /// Made-up SJK chat messages (sender, text, verified) and who is online in the
    /// docked chat, and `draft` typed in its field (world shots and tests).
    pub(crate) fn chat_for_shot(
        &mut self,
        lines: &[(&str, &str, bool)],
        online: u32,
        draft: Option<&str>,
    ) {
        self.chat_cache.for_shot(lines, online);
        self.chat_on = true;
        match draft {
            Some(draft) => self.chat.type_for_shot(draft),
            None => self.chat.stop_typing(),
        }
    }
}

impl Page {
    pub(crate) const fn is_vote_page(self) -> bool {
        matches!(
            self,
            Self::CallVote
                | Self::VoteMap
                | Self::VoteGameType
                | Self::VoteKick
                | Self::VoteClientKick
                | Self::VoteWarmup
                | Self::VoteTimeLimit
                | Self::VoteFragLimit
        )
    }
}

pub(crate) fn weapon_name(weapon: u8) -> &'static str {
    match weapon {
        0 => "Unarmed",
        1 => "Stun Baton",
        2 => "Melee",
        3 => "Lightsaber",
        4 => "Blaster Pistol",
        5 => "E-11 Blaster Rifle",
        6 => "Disruptor Rifle",
        7 => "Bowcaster",
        8 => "Heavy Repeater",
        9 => "DEMP2",
        10 => "Flechette",
        11 => "Rocket Launcher",
        12 => "Thermal Detonator",
        13 => "Trip Mine",
        14 => "Det Pack",
        15 => "Concussion Rifle",
        16 => "Bryar Pistol (legacy)",
        17 => "Emplaced Gun",
        18 => "Turret",
        _ => "Unknown weapon",
    }
}

/// The row count of a page neither look draws itself (the classic look's
/// server-info page).
fn shared_row_count(page: Page) -> usize {
    match page {
        Page::About => 1,
        _ => 0,
    }
}

/// Read-only lines above the entries of `page` (the server-info page, and the
/// Players and Report pages' who and why).
fn info_lines<'a>(
    about: &'a about::State,
    players: &'a players::State,
    page: Page,
) -> &'a [String] {
    match page {
        Page::About => about.lines(),
        Page::Players | Page::ReportPlayer => players.info(page),
        _ => &[],
    }
}

/// Whether `row` of the join page is the team the player is already on.
fn current_team_row(view: &View<'_>, row: usize) -> bool {
    if view.page != Page::Team {
        return false;
    }
    if view.team_game {
        matches!((row, view.team), (1, 1) | (2, 2) | (3, 3))
    } else {
        matches!((row, view.team), (0, 0..=2) | (1, 3))
    }
}

/// Count populated red/blue clientinfo records for the join modal.
pub(crate) fn team_sizes(game_state: &GameState) -> [usize; 2] {
    let mut sizes = [0; 2];
    for client in 0..32 {
        let Some(bytes) = game_state.config_string(1_131 + client) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let Ok(info) = InfoString::parse(text) else {
            continue;
        };
        match info.get_i32("t") {
            Some(1) => sizes[0] += 1,
            Some(2) => sizes[1] += 1,
            _ => {}
        }
    }
    sizes
}

#[cfg(test)]
mod sjk_tests {
    use super::*;

    fn view(page: Page, selected_row: usize, team_game: bool, team: u8) -> View<'static> {
        View {
            page,
            selected_row,
            team,
            team_game,
            siege: false,
            red_players: 4,
            blue_players: 3,
            vote_active: false,
            staff: false,
            _frame: std::marker::PhantomData,
        }
    }

    fn sjk_menu() -> InGameMenu {
        let mut menu = InGameMenu::new();
        menu.set_style(MenuStyle::Sjk, ArtSet::default());
        menu
    }

    #[test]
    fn each_style_keeps_its_own_menu() {
        let mut menu = InGameMenu::new();
        for (style, classic, sjk, main_rows) in [
            (MenuStyle::Classic, true, false, classic::Tab::ALL.len()),
            (MenuStyle::Sjk, false, true, sjk_view::Entry::MAIN.len()),
        ] {
            menu.set_style(style, ArtSet::default());
            assert_eq!(
                (menu.is_classic(), menu.is_sjk()),
                (classic, sjk),
                "{style:?}"
            );
            assert_eq!(menu.row_count(Page::Main, false), main_rows);
        }
        // Pages the SJK UI shares keep the shared row counts.
        let menu = sjk_menu();
        assert_eq!(
            menu.row_count(Page::CallVote, false),
            menu.callvote.row_count(Page::CallVote)
        );
        assert_eq!(menu.row_count(Page::Leave, false), 3);
    }

    #[test]
    fn every_page_draws_its_rows_within_the_canvas() {
        let mut game = GameState::empty_local(0);
        let maps: String = (0..40).map(|index| format!("mp/map{index} ")).collect();
        game.replace_config_string(0, format!("\\sv_maplist\\{maps}").into_bytes())
            .expect("serverinfo");
        let mut menu = sjk_menu();
        menu.refresh_callvote(Some(&game), None);
        menu.card = sjk_view::Card::for_shot(true, false);
        for viewport in [[1_920.0, 1_080.0], [3_840.0, 2_160.0], [1_024.0, 768.0]] {
            for (page, team_game) in [
                (Page::Main, false),
                (Page::Team, true),
                (Page::Team, false),
                (Page::CallVote, false),
                (Page::VoteMap, false),
                (Page::VoteGameType, false),
                (Page::Leave, false),
            ] {
                let count = menu.row_count(page, team_game);
                assert!(count > 0, "{page:?}");
                menu.build_sjk(view(page, count - 1, team_game, 2), viewport);
                assert_eq!(menu.row_count, count, "{page:?}: prepared rows");
                assert!(!menu.canvas.overflowed(), "{page:?} {viewport:?}");
                for row in 0..count {
                    assert!(
                        menu.canvas.rect_for(row as u16).is_some(),
                        "{page:?} row {row}"
                    );
                }
                assert!(menu.canvas.rect_for(count as u16).is_none(), "{page:?}");
            }
        }
        // Forty maps page through sixteen at a time, with More and Back.
        assert_eq!(menu.row_count(Page::VoteMap, false), 18);
        menu.build_sjk(view(Page::VoteMap, 0, false, 0), [1_920.0, 1_080.0]);
        assert_eq!(menu.rows[16], "More maps...");
    }

    #[test]
    fn the_arc_passes_over_rows_that_cannot_be_taken() {
        let mut menu = sjk_menu();
        // On blue in a team game: Blue team cannot be taken.
        menu.build_sjk(view(Page::Team, 1, true, 2), [1_920.0, 1_080.0]);
        assert!(!menu.activation_allowed(2));
        assert_eq!(menu.sjk_step(Page::Team, 1, 5, true), 3);
        assert_eq!(menu.sjk_step(Page::Team, 3, 5, false), 1);
        assert_eq!(menu.sjk_step(Page::Team, 4, 5, true), 0, "wraps");
        // Another page than the one drawn: every row counts.
        assert_eq!(menu.sjk_step(Page::Main, 0, 9, true), 1);
        assert_eq!(menu.sjk_step(Page::Main, 0, 0, true), 0);
    }

    #[test]
    fn every_style_calls_the_panel_camera_control() {
        let mut menu = sjk_menu();
        for vote_active in [false, true] {
            let mut main = view(Page::Main, 0, false, 0);
            main.vote_active = vote_active;
            menu.prepare_rows(&main);
            let rows = &menu.rows[..menu.row_count];
            // The icon under the emblem the panel opens from.
            assert_eq!(sjk_focus::Icon::Camera.label(), CAMERA_CONTROL);
            assert!(
                rows.iter()
                    .all(|row| !row.to_ascii_lowercase().contains("shot")),
                "{rows:?}"
            );
        }
        // The classic bar has no entry: F8 opens the panel there.
        let mut classic = InGameMenu::new();
        classic.set_style(MenuStyle::Classic, ArtSet::default());
        classic.prepare_rows(&view(Page::Main, 0, false, 0));
        assert!(
            classic.rows[..classic.row_count]
                .iter()
                .all(|row| !row.to_ascii_lowercase().contains("shot"))
        );
    }

    /// The keyboard stays only where the frame drawn offers something: a Staff icon
    /// once the key is no longer staff, a vote's button once the vote ended, and
    /// anything but the list on another page fall back.
    #[test]
    fn the_keyboard_stays_where_the_page_offers_something() {
        let mut menu = sjk_menu();
        menu.card = sjk_view::Card::for_shot(false, false);
        let mut main = view(Page::Main, 0, false, 0);
        menu.focus = sjk_focus::Focus::Row(sjk_focus::Icon::Staff);
        main.staff = true;
        menu.build_sjk(main, [1_920.0, 1_080.0]);
        assert_eq!(menu.focus, sjk_focus::Focus::Row(sjk_focus::Icon::Staff));
        assert_eq!(menu.sjk_icons().len(), 6);
        let mut main = view(Page::Main, 0, false, 0);
        main.staff = false;
        menu.build_sjk(main, [1_920.0, 1_080.0]);
        assert_eq!(menu.focus, sjk_focus::Focus::List);
        assert_eq!(menu.sjk_icons().len(), 5);
        // A vote on: Yes and No on the card; the vote ends, the card's first control.
        let mut voting = view(Page::Main, 0, false, 0);
        voting.vote_active = true;
        menu.focus = sjk_focus::Focus::Card(sjk_focus::Control::VoteNo);
        menu.build_sjk(voting, [1_920.0, 1_080.0]);
        assert_eq!(
            menu.focus,
            sjk_focus::Focus::Card(sjk_focus::Control::VoteNo)
        );
        let placed = menu
            .sjk_control_of(usize::from(sjk_view::CONTROL_TOKEN) + 1)
            .expect("No");
        assert_eq!(placed.control, sjk_focus::Control::VoteNo);
        menu.build_sjk(view(Page::Main, 0, false, 0), [1_920.0, 1_080.0]);
        assert_eq!(
            menu.focus,
            sjk_focus::Focus::Card(sjk_focus::Control::Team(
                sjk_client::LegacyTeamChoice::Spectator
            ))
        );
        // Another page: the list.
        menu.build_sjk(view(Page::Leave, 0, false, 0), [1_920.0, 1_080.0]);
        assert_eq!(menu.focus, sjk_focus::Focus::List);
        assert!(menu.sjk_controls().as_slice().is_empty());
    }

    #[test]
    fn a_screen_opened_from_the_main_page_returns_to_its_entry() {
        let mut menu = sjk_menu();
        menu.remember_return(sjk_view::Entry::Settings.index());
        assert_eq!(menu.return_row(), sjk_view::Entry::Settings.index());
        assert_eq!(menu.return_row(), 0, "taken once");
        menu.set_style(MenuStyle::Classic, ArtSet::default());
        menu.remember_return(4);
        assert_eq!(menu.return_row(), 0, "the classic look opens on the first");
    }

    /// The SJK chat docks on the main page while `cl_sjkChat` is on: Enter on its field
    /// types, the keys go to the field, Enter sends (here without the identity, so the
    /// dock says to turn it on); another page, or the chat turned off, stops the typing
    /// and gives the keyboard back to the list.
    #[test]
    fn the_docked_chat_types_sends_and_goes_with_its_page() {
        use winit::keyboard::KeyCode;
        let mut menu = sjk_menu();
        menu.card = sjk_view::Card::for_shot(false, false);
        menu.chat_for_shot(&[("Fox", "gg", false)], 3, None);
        menu.build_sjk(view(Page::Main, 0, false, 0), [1_920.0, 1_080.0]);
        assert!(menu.sjk_chat());
        assert!(
            menu.rect_for(sjk_view::CHAT_TOKENS.field).is_some(),
            "the field"
        );
        // A click on the field types; the keys go into it; Enter sends.
        assert_eq!(
            menu.chat_pointer(usize::from(sjk_view::CHAT_TOKENS.field), true),
            Some(None)
        );
        assert!(menu.chat_typing());
        for (key, text) in [(KeyCode::KeyW, "w"), (KeyCode::KeyS, "s")] {
            assert_eq!(menu.chat_key(key, Some(text), false, false), None);
        }
        assert_eq!(menu.chat_key(KeyCode::Tab, Some("\t"), false, false), None);
        assert_eq!(
            menu.chat_key(KeyCode::Enter, Some("\r"), false, false),
            Some(DockAction::Send)
        );
        menu.send_chat();
        assert!(!menu.chat_typing());
        assert_eq!(menu.chat_cache.local, "Turn the SJK identity on to chat");
        // Escape stops typing first, the keyboard staying on the field.
        menu.focus = sjk_focus::Focus::Chat;
        menu.start_chat_typing();
        assert_eq!(menu.chat_key(KeyCode::Escape, None, false, false), None);
        assert!(!menu.chat_typing());
        menu.build_sjk(view(Page::Main, 0, false, 0), [1_920.0, 1_080.0]);
        assert_eq!(menu.focus, sjk_focus::Focus::Chat);
        // Another page: no dock, no typing, the list.
        menu.start_chat_typing();
        menu.build_sjk(view(Page::Leave, 0, false, 0), [1_920.0, 1_080.0]);
        assert!(!menu.sjk_chat() && !menu.chat_typing());
        assert_eq!(menu.focus, sjk_focus::Focus::List);
        assert_eq!(
            menu.chat_pointer(usize::from(sjk_view::CHAT_TOKENS.field), true),
            None
        );
        // The chat turned off: the main page without it.
        menu.focus = sjk_focus::Focus::Chat;
        menu.sync_chat(false);
        menu.build_sjk(view(Page::Main, 0, false, 0), [1_920.0, 1_080.0]);
        assert!(!menu.sjk_chat());
        assert_eq!(menu.focus, sjk_focus::Focus::List);
        assert!(menu.rect_for(sjk_view::CHAT_TOKENS.field).is_none());
        menu.start_chat_typing();
        assert!(!menu.chat_typing(), "nothing to type in");
    }
}

//! The SJK chat page (`docs/hub-chat.md`): the chat every SJK player shares through
//! the SJK hub, in full. The history with names, verified and staff marks and how
//! long ago each message came; a field to write in and Send; a small window of who is
//! online and who was seen most recently ([`people`]); and, for a chosen message, Mute
//! on this PC, and for SJK staff Delete and Mute at the hub. Resting the pointer on a
//! name, in the chat or the window, shows that player's sender card with Mute or
//! Unmute ([`crate::sender_card`], "Muting a player"). GIF beside Send opens the GIF
//! picker over the messages ([`gif_picker`]): favourites, GIPHY's search and the GIFs
//! seen in the chat; a chosen message's GIF can be made a favourite. Search needs the
//! player's own GIPHY key: without one the key card says how to get it and takes it.
//!
//! Opened by the main page's docked chat (Open chat), the `sjkchat` command or the
//! in-game SJK menu. Like the Staff page it lives in the console and has the SJK UI's
//! look ([`view`]). The field has the keyboard when the page opens: typing goes to
//! it, Enter sends, Up goes to the newest message; Up and Down move through the
//! messages, Page Up and Page Down scroll, Tab walks every control, Escape goes back
//! (or closes the GIF picker first).

use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use sjk_identity::{ChatState, StaffRequest};
use sjk_ui::{InputEvent, UiEventKind};
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "sjk_chat_gif_picker.rs"]
mod gif_picker;
#[path = "sjk_chat_people.rs"]
mod people;
#[path = "sjk_chat_panel_view.rs"]
mod view;

const FIELD_TOKEN: u16 = 1_300;
const SEND_TOKEN: u16 = 1_301;
const MUTE_TOKEN: u16 = 1_302;
const DELETE_TOKEN: u16 = 1_303;
const HUB_MUTE_TOKEN: u16 = 1_304;
const HUB_UNMUTE_TOKEN: u16 = 1_305;
/// GIF beside Send (the picker), and the chosen message's Add GIF to favourites.
const GIF_TOKEN: u16 = 1_306;
const FAVOURITE_TOKEN: u16 = 1_307;
/// Retry, beside the notice that says the hub did not take the last message.
const RETRY_TOKEN: u16 = 1_308;
/// Messages on show, one token each.
const MESSAGE_BASE: u16 = 1_310;
const MESSAGES_SHOWN: usize = 40;
/// The sender card a name shows under the pointer, and its Mute or Unmute.
const CARD_TOKEN: u16 = 1_360;
const CARD_MUTE_TOKEN: u16 = 1_361;
/// The names of the messages on show, one token each.
const NAME_BASE: u16 = 1_370;
/// The players in the who-is-online window, one token each.
const PERSON_BASE: u16 = 1_420;
const PEOPLE_SHOWN: usize = people::ONLINE_FACES + people::RECENT_FACES;
/// Pictures' versions the page keeps for faces whose row does not carry one.
const PICTURES_KEPT: usize = 32;
/// Longest message, as the hub takes it.
const DRAFT_MAX: usize = crate::sjk_chat_field::MAX;
/// Messages Page Up and Page Down scroll by.
const PAGE: usize = 5;

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Send this message to the SJK chat.
    Send(String),
    /// Mute or unmute, on this PC, the player with this key, last seen with this name.
    Mute {
        key_id: String,
        name: String,
        muted: bool,
    },
    /// A staff request to the hub.
    Staff(StaffRequest),
    /// Save this as the player's GIPHY key (`cl_giphyKey`), from the key card.
    GiphyKey(String),
}

/// What the page knows this frame.
pub(crate) struct Inputs<'a> {
    /// The chat, or `None` while the identity is not running.
    pub(crate) chat: Option<&'a ChatState>,
    /// The keys muted on this PC.
    pub(crate) muted: &'a [String],
    /// Whether the player's key is SJK staff.
    pub(crate) staff: bool,
    /// Whether `cl_sjkChat` is on.
    pub(crate) enabled: bool,
    /// Now, unix seconds, for how long ago messages came.
    pub(crate) now: u64,
    /// What the staff requests came to, for a staff key.
    pub(crate) staff_state: Option<StaffShown<'a>>,
    /// `cl_giphyKey`, the player's own GIPHY key for the picker's search (empty: none).
    pub(crate) giphy_key: &'a str,
}

/// The part of `sjk_identity::StaffState` the page shows.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StaffShown<'a> {
    /// Counts answers.
    pub(crate) serial: u64,
    pub(crate) message: &'a str,
    pub(crate) failed: bool,
    /// A request is on its way.
    pub(crate) busy: bool,
}

/// What the last frame showed, for keys and clicks.
#[derive(Debug, Default)]
struct Shown {
    /// The messages on show, by token, as (id, sender's key), oldest first.
    messages: Vec<(u64, String)>,
    /// The chosen message's id, its sender's key and name, and whether they are
    /// muted here.
    chosen: Option<(u64, String, String, bool)>,
    /// The chosen message's GIF, when it has one on show.
    chosen_gif: Option<crate::chat_gifs::GifId>,
    /// How many messages there are in all.
    total: usize,
    /// Whether the sender card's player is muted here.
    card_muted: bool,
    /// The keys of the players in the who-is-online window, by token.
    people: Vec<String>,
    /// The keys of faces in the window drawn without knowing their picture: the
    /// chat's last senders, whose messages do not say ([`Panel::find_pictures`]).
    unpictured: Vec<String>,
}

pub(crate) struct Panel {
    open: bool,
    owns_console: bool,
    ui: MenuCanvas,
    draft: String,
    /// The last message handed to the hub, which Retry sends again.
    last_sent: String,
    /// The control the keyboard is on, by its token.
    focus: u16,
    /// The controls in the order Tab visits them, laid out by the last frame.
    order: Vec<u16>,
    /// The chosen message's id.
    selected: Option<u64>,
    /// Messages hidden below the list, scrolled up past.
    scroll: usize,
    /// The staff answers' serial when the page last sent a staff request: answers
    /// past it are this page's.
    staff_after: Option<u64>,
    shown: Shown,
    /// The sender card on show.
    card: Option<Hovered>,
    /// The pictures' versions found for faces without one (key, version; empty for
    /// none), newest last.
    pictures: Vec<(String, String)>,
    /// Where the pointer last moved to, and the window's faces the frame before: the
    /// canvas knows the target under the pointer by its place in the frame's list, so
    /// when the faces change under a resting pointer it is looked for again.
    pointer: Option<sjk_ui::Vec2>,
    previous_people: Vec<String>,
    epoch: Instant,
    /// The GIF picker.
    gifs: gif_picker::Picker,
    /// `cl_giphyKey` as the last frame had it, for the key card's Change GIPHY key.
    giphy_key: String,
    /// The chat to show in place of the live one, for a world shot.
    #[cfg(test)]
    pub(crate) preview: Option<ChatState>,
}

/// The sender card on show: the message whose sender it is about (or the player in the
/// who-is-online window), who, and the name it is beside. Where they are on the server
/// is asked outside the chat's lock ([`Panel::place_card`]), so it shows a frame later.
struct Hovered {
    source: CardSource,
    person: crate::sender_card::Person,
    anchor: sjk_ui::Rect,
    placed: bool,
}

/// What a sender card on show is about.
#[derive(Clone, Debug, Eq, PartialEq)]
enum CardSource {
    /// The sender of the message with this id.
    Message(u64),
    /// The player with this key in the who-is-online window.
    Person(String),
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            owns_console: false,
            ui: MenuCanvas::with_capacities(260, 160, 1_600),
            draft: String::new(),
            last_sent: String::new(),
            focus: FIELD_TOKEN,
            order: Vec::with_capacity(64),
            selected: None,
            scroll: 0,
            staff_after: None,
            shown: Shown::default(),
            card: None,
            pictures: Vec::new(),
            pointer: None,
            previous_people: Vec::with_capacity(PEOPLE_SHOWN),
            epoch: Instant::now(),
            gifs: gif_picker::Picker::new(),
            giphy_key: String::new(),
            #[cfg(test)]
            preview: None,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page, the keyboard in the field; `owns_console` when the console was
    /// closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        self.focus = FIELD_TOKEN;
        self.scroll = 0;
        self.gifs.close();
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    /// The page sent a staff request while the staff answers' serial was `serial`.
    pub(crate) fn staff_sent(&mut self, serial: u64) {
        self.staff_after = Some(serial);
    }

    /// Say on the sender card on show where its player is on the server being played
    /// (`place`, from their key and name, once) and their picture's version (`picture`,
    /// from their key, until it is known). Called before the chat's lock is taken.
    pub(crate) fn place_card(
        &mut self,
        place: impl FnOnce(Option<&str>, &str) -> crate::sender_card::Place,
        picture: impl FnOnce(&str) -> Option<String>,
    ) {
        let Some(card) = self.card.as_mut() else {
            return;
        };
        if !card.placed {
            card.person.place = place(card.person.key_id.as_deref(), &card.person.name);
            card.placed = true;
        }
        if card.person.avatar.is_none()
            && let Some(key_id) = &card.person.key_id
        {
            card.person.avatar = picture(key_id);
        }
    }

    /// Find the pictures of the faces the last frame drew without one, through
    /// `picture` (a key's picture version, empty for none, `None` while not known),
    /// outside the chat's lock as [`Panel::place_card`] does.
    pub(crate) fn find_pictures(&mut self, picture: impl Fn(&str) -> Option<String>) {
        for key in std::mem::take(&mut self.shown.unpictured) {
            if self.pictures.iter().any(|(known, _)| *known == key) {
                continue;
            }
            if let Some(version) = picture(&key) {
                if self.pictures.len() == PICTURES_KEPT {
                    self.pictures.remove(0);
                }
                self.pictures.push((key, version));
            }
        }
    }

    /// The picture version found for `key_id`'s face, if any.
    fn picture_of(&self, key_id: &str) -> Option<&str> {
        self.pictures
            .iter()
            .find(|(key, _)| key == key_id)
            .map(|(_, version)| version.as_str())
    }

    /// Choose the message with `id`, for a world shot.
    #[cfg(test)]
    pub(crate) fn choose_for_shot(&mut self, id: u64) {
        self.selected = Some(id);
    }

    /// Rest the pointer on the face of the `index`th player of the who-is-online
    /// window, as the last frame laid it out, or on the field for `None`, for a world
    /// shot.
    #[cfg(test)]
    pub(crate) fn hover_person_for_shot(&mut self, index: Option<u16>) {
        let token = index.map_or(FIELD_TOKEN, |index| PERSON_BASE + index);
        if let Some(rect) = self.ui.rect_for(token) {
            let centre = sjk_ui::Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
            let _ = self.handle_pointer(InputEvent::PointerMove(centre));
        }
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Open the GIF picker on tab `tab` (0 Favourites, 1 Search, 2 Seen in chat), the
    /// words `typed` searched, the keyboard on the second tile, for a world shot.
    #[cfg(test)]
    pub(crate) fn open_gifs_for_shot(&mut self, tab: usize, typed: &str) {
        let _ = self.gifs.open();
        self.gifs.choose_tab(gif_picker::Tab::ALL[tab]);
        self.gifs.typed = typed.to_owned();
        self.gifs.search_now();
        self.focus = gif_picker::TILE_BASE + 1;
    }

    /// Open the GIF picker's key card with `typed` in its field, the keyboard on it,
    /// for a world shot.
    #[cfg(test)]
    pub(crate) fn open_key_card_for_shot(&mut self, typed: &str) {
        let _ = self.gifs.open();
        self.gifs.choose_tab(gif_picker::Tab::Search);
        self.gifs.open_key_card(typed);
        self.focus = gif_picker::KEY_FIELD_TOKEN;
    }

    /// Answer the picker's search on its way with `ids` of `total`, for a world shot.
    #[cfg(test)]
    pub(crate) fn answer_gif_search_for_shot(
        &mut self,
        ids: Vec<crate::chat_gifs::GifId>,
        total: usize,
    ) {
        self.gifs
            .searcher
            .answer_for_test(Ok(crate::chat_gifs::search::Page { ids, total }));
    }

    /// Open or close the GIF picker; the keyboard goes into it, or back to the field.
    fn toggle_gifs(&mut self) {
        if self.gifs.open {
            self.gifs.close();
            self.focus = FIELD_TOKEN;
        } else if crate::chat_gifs::enabled() {
            self.focus = self.gifs.open();
        }
    }

    /// What a click (or Enter) on one of the picker's controls does; `None` for a token
    /// that is not the picker's.
    fn activate_gifs(&mut self, token: u16) -> Option<PanelAction> {
        use gif_picker::{
            KEY_ASK_TOKEN, KEY_FIELD_TOKEN, KEY_LATER_TOKEN, KEY_OPEN_TOKEN, KEY_SAVE_TOKEN,
            SEARCH_TOKEN, STAR_BASE, TAB_BASE, TILE_BASE, TILES, Tab,
        };
        if !self.gifs.open {
            return None;
        }
        match token {
            KEY_ASK_TOKEN => {
                self.gifs.open_key_card(&self.giphy_key);
                self.focus = KEY_FIELD_TOKEN;
                return Some(PanelAction::None);
            }
            KEY_FIELD_TOKEN | KEY_OPEN_TOKEN | KEY_SAVE_TOKEN | KEY_LATER_TOKEN
                if self.gifs.key_card.is_some() =>
            {
                self.focus = token;
                match token {
                    KEY_OPEN_TOKEN if !cfg!(test) => {
                        crate::update::open_page(crate::chat_gifs::search::KEY_PAGE);
                    }
                    KEY_SAVE_TOKEN => {
                        if let Some(key) = self.gifs.save_key() {
                            self.focus = SEARCH_TOKEN;
                            return Some(PanelAction::GiphyKey(key));
                        }
                        self.focus = KEY_FIELD_TOKEN;
                    }
                    KEY_LATER_TOKEN => {
                        self.gifs.put_off_key_card();
                        self.focus = SEARCH_TOKEN;
                    }
                    _ => {}
                }
                return Some(PanelAction::None);
            }
            _ => {}
        }
        if let Some(index) = token
            .checked_sub(TAB_BASE)
            .map(usize::from)
            .filter(|index| *index < Tab::ALL.len())
        {
            self.gifs.choose_tab(Tab::ALL[index]);
            self.focus = TAB_BASE + index as u16;
            return Some(PanelAction::None);
        }
        if token == SEARCH_TOKEN {
            self.focus = SEARCH_TOKEN;
            self.gifs.search_now();
            return Some(PanelAction::None);
        }
        if (TILE_BASE..TILE_BASE + TILES as u16).contains(&token) {
            let id = self.gifs.tile(token)?.clone();
            match gif_picker::with_gif(&self.draft, &id) {
                Some(draft) => {
                    self.draft = draft;
                    self.gifs.close();
                    self.focus = FIELD_TOKEN;
                }
                None => {
                    self.gifs.notice =
                        Some("No room for the GIF's link: make the message shorter first");
                }
            }
            return Some(PanelAction::None);
        }
        if (STAR_BASE..STAR_BASE + TILES as u16).contains(&token) {
            if let Some(id) = self.gifs.tile(TILE_BASE + (token - STAR_BASE)).cloned() {
                let favourite = crate::chat_gifs::favourites::contains(&id);
                crate::chat_gifs::favourites::set(&id, !favourite);
            }
            return Some(PanelAction::None);
        }
        None
    }

    /// A key while the GIF picker is open, before the page's own; `None` lets the page
    /// have it.
    fn edit_gifs(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> Option<PanelAction> {
        use gif_picker::{
            KEY_FIELD_TOKEN, KEY_LATER_TOKEN, SEARCH_TOKEN, TAB_BASE, TILE_BASE, TILES, Tab,
        };
        if !self.gifs.open {
            return None;
        }
        if self.gifs.key_card.is_some() {
            // The key card has the keyboard: Escape is Not now, Enter in the field saves,
            // and the field takes typing, Ctrl+A, Ctrl+C, Ctrl+V and Backspace.
            let in_field = self.focus == KEY_FIELD_TOKEN;
            return match key {
                KeyCode::Escape => self.activate_gifs(KEY_LATER_TOKEN),
                KeyCode::Enter | KeyCode::NumpadEnter if in_field => {
                    self.activate_gifs(gif_picker::KEY_SAVE_TOKEN)
                }
                KeyCode::Tab
                | KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown => None,
                KeyCode::Space if !in_field => None,
                _ if in_field => {
                    self.gifs.edit_key(key, text, shift, control);
                    Some(PanelAction::None)
                }
                _ => Some(PanelAction::None),
            };
        }
        let on_tile = (TILE_BASE..TILE_BASE + TILES as u16).contains(&self.focus);
        let on_tabs = (TAB_BASE..TAB_BASE + Tab::ALL.len() as u16).contains(&self.focus);
        let searching = self.focus == SEARCH_TOKEN;
        if searching && self.gifs.before_edit(key, text, shift, control) {
            return Some(PanelAction::None);
        }
        if searching && crate::text_entry::is_copy(key, text, control) {
            if !self.gifs.typed.is_empty() {
                let _ = crate::console::clipboard::copy(&self.gifs.typed);
            }
            return Some(PanelAction::None);
        }
        match key {
            KeyCode::Escape => {
                self.toggle_gifs();
            }
            KeyCode::PageUp => self.gifs.scroll(-(gif_picker::ROWS as isize)),
            KeyCode::PageDown => self.gifs.scroll(gif_picker::ROWS as isize),
            KeyCode::Enter | KeyCode::NumpadEnter if searching => self.gifs.search_now(),
            KeyCode::Backspace if searching => self.gifs.backspace(),
            KeyCode::ArrowDown if searching || on_tabs => {
                if self.gifs.tile(TILE_BASE).is_some() {
                    self.focus = TILE_BASE;
                }
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight if on_tabs => {
                let at = self.gifs.tab.index();
                let count = Tab::ALL.len();
                let next = if key == KeyCode::ArrowRight {
                    (at + 1) % count
                } else {
                    (at + count - 1) % count
                };
                return self.activate_gifs(TAB_BASE + next as u16);
            }
            KeyCode::Tab => return None,
            _ if searching => self.gifs.type_key(key, text, shift, control),
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown
                if on_tile =>
            {
                let (dx, dy) = match key {
                    KeyCode::ArrowLeft => (-1, 0),
                    KeyCode::ArrowRight => (1, 0),
                    KeyCode::ArrowUp => (0, -1),
                    _ => (0, 1),
                };
                self.focus = match self.gifs.step(self.focus, dx, dy) {
                    Some(token) => token,
                    None if self.gifs.tab == Tab::Search => SEARCH_TOKEN,
                    None => TAB_BASE + self.gifs.tab.index() as u16,
                };
            }
            KeyCode::KeyF if on_tile && !control => {
                return self.activate_gifs(gif_picker::STAR_BASE + (self.focus - TILE_BASE));
            }
            KeyCode::Slash if !searching && self.focus != FIELD_TOKEN => {
                let _ = self.activate_gifs(TAB_BASE + Tab::Search.index() as u16);
                self.focus = SEARCH_TOKEN;
            }
            KeyCode::ArrowUp if self.focus == FIELD_TOKEN => {
                self.focus = if self.gifs.tile(TILE_BASE).is_some() {
                    TILE_BASE
                } else {
                    self.gifs.first_control()
                };
            }
            _ => return None,
        }
        Some(PanelAction::None)
    }

    /// What Enter (or a click) on `token` does.
    fn activate(&mut self, token: u16) -> PanelAction {
        if token == CARD_TOKEN {
            return PanelAction::None;
        }
        if let Some(action) = self.activate_gifs(token) {
            return action;
        }
        if token == GIF_TOKEN {
            self.toggle_gifs();
            return PanelAction::None;
        }
        if token == FAVOURITE_TOKEN {
            // The chosen message's GIF; the keyboard stays on the button.
            self.focus = token;
            if let Some(id) = self.shown.chosen_gif.clone() {
                let favourite = crate::chat_gifs::favourites::contains(&id);
                crate::chat_gifs::favourites::set(&id, !favourite);
            }
            return PanelAction::None;
        }
        if token == CARD_MUTE_TOKEN {
            // The card's Mute or Unmute; the keyboard stays where it was.
            let Some(card) = &self.card else {
                return PanelAction::None;
            };
            return PanelAction::Mute {
                key_id: card.person.key_id.clone().unwrap_or_default(),
                name: card.person.name.clone(),
                muted: !self.shown.card_muted,
            };
        }
        if let Some(index) = token
            .checked_sub(NAME_BASE)
            .filter(|index| usize::from(*index) < MESSAGES_SHOWN)
        {
            // A click on a name chooses its message, as one on the row does.
            return self.activate(MESSAGE_BASE + index);
        }
        if let Some(index) = token
            .checked_sub(PERSON_BASE)
            .filter(|index| usize::from(*index) < PEOPLE_SHOWN)
        {
            // A click on a player in the window chooses their newest message on show;
            // the card shows under the pointer either way.
            let key = self.shown.people.get(usize::from(index));
            let newest = key.and_then(|key| {
                self.shown
                    .messages
                    .iter()
                    .rposition(|(_, sender)| sender == key)
            });
            if let Some(at) = newest {
                return self.activate(MESSAGE_BASE + at as u16);
            }
            return PanelAction::None;
        }
        self.focus = token;
        let chosen = self.shown.chosen.clone();
        match token {
            FIELD_TOKEN => PanelAction::None,
            SEND_TOKEN => {
                let text = self.draft.trim().to_owned();
                if text.is_empty() {
                    return PanelAction::None;
                }
                self.draft.clear();
                self.focus = FIELD_TOKEN;
                self.last_sent.clone_from(&text);
                PanelAction::Send(text)
            }
            RETRY_TOKEN => {
                // Only laid out while a refusal and the text are there.
                if self.last_sent.is_empty() || !self.order.contains(&RETRY_TOKEN) {
                    return PanelAction::None;
                }
                self.focus = FIELD_TOKEN;
                PanelAction::Send(self.last_sent.clone())
            }
            MUTE_TOKEN => match chosen {
                Some((_, key_id, name, muted)) => PanelAction::Mute {
                    key_id,
                    name,
                    muted: !muted,
                },
                None => PanelAction::None,
            },
            DELETE_TOKEN | HUB_MUTE_TOKEN | HUB_UNMUTE_TOKEN => {
                // Only offered (laid out) for staff; the hub refuses others too.
                let Some((id, key_id, ..)) = chosen.filter(|_| self.order.contains(&token)) else {
                    return PanelAction::None;
                };
                PanelAction::Staff(match token {
                    DELETE_TOKEN => StaffRequest::ChatDelete { id },
                    _ => StaffRequest::ChatMute {
                        key_id,
                        muted: token == HUB_MUTE_TOKEN,
                    },
                })
            }
            token if (MESSAGE_BASE..MESSAGE_BASE + MESSAGES_SHOWN as u16).contains(&token) => {
                if let Some((id, _)) = self.shown.messages.get(usize::from(token - MESSAGE_BASE)) {
                    self.selected = Some(*id);
                }
                PanelAction::None
            }
            _ => PanelAction::None,
        }
    }

    /// Move the keyboard to the next control (or the one before) of the last frame.
    fn step(&mut self, forward: bool) {
        if self.order.is_empty() {
            return;
        }
        let at = self.order.iter().position(|token| *token == self.focus);
        let next = match (at, forward) {
            (None, _) => 0,
            (Some(at), true) => (at + 1) % self.order.len(),
            (Some(at), false) => (at + self.order.len() - 1) % self.order.len(),
        };
        self.focus = self.order[next];
    }

    /// Choose message `index` of those on show, scrolling when it is past an end.
    fn choose(&mut self, index: isize) {
        let count = self.shown.messages.len() as isize;
        if count == 0 {
            return;
        }
        if index < 0 {
            // Older than the oldest on show: scroll up by one, if there is more.
            if self.scroll + self.shown.messages.len() < self.shown.total {
                self.scroll += 1;
                self.selected = None;
            }
            let _ = self.activate(MESSAGE_BASE);
            return;
        }
        if index >= count {
            if self.scroll > 0 {
                self.scroll -= 1;
                let _ = self.activate(MESSAGE_BASE + (count - 1) as u16);
            } else {
                self.focus = FIELD_TOKEN;
            }
            return;
        }
        let _ = self.activate(MESSAGE_BASE + index as u16);
    }

    /// A key.
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
        // Held keys repeat in the fields (Backspace); elsewhere a key acts once.
        let field = [
            FIELD_TOKEN,
            gif_picker::SEARCH_TOKEN,
            gif_picker::KEY_FIELD_TOKEN,
        ];
        if event.repeat && !field.contains(&self.focus) {
            return PanelAction::None;
        }
        self.edit(key, event.text.as_deref(), shift, control)
    }

    /// A pressed `key` that typed `text`, with Shift and Ctrl held or not; in the field,
    /// Ctrl+V and Shift+Insert paste ([`crate::sjk_chat_field`]).
    fn edit(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        shift: bool,
        control: bool,
    ) -> PanelAction {
        if let Some(action) = self.edit_gifs(key, text, shift, control) {
            return action;
        }
        let typing = self.focus == FIELD_TOKEN;
        if typing && crate::text_entry::before_edit(&mut self.draft, key, text, shift, control) {
            return PanelAction::None;
        }
        if typing && crate::text_entry::is_copy(key, text, control) {
            if !self.draft.is_empty() {
                let _ = crate::console::clipboard::copy(&self.draft);
            }
            return PanelAction::None;
        }
        let in_list = (MESSAGE_BASE..MESSAGE_BASE + MESSAGES_SHOWN as u16).contains(&self.focus);
        match key {
            KeyCode::Escape => return PanelAction::Close,
            KeyCode::Tab => self.step(!shift),
            KeyCode::PageUp => {
                let most = self.shown.total.saturating_sub(1);
                self.scroll = (self.scroll + PAGE).min(most);
            }
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(PAGE),
            KeyCode::ArrowUp if typing => self.choose(self.shown.messages.len() as isize - 1),
            KeyCode::ArrowUp | KeyCode::ArrowDown if in_list => {
                let at = (self.focus - MESSAGE_BASE) as isize;
                self.choose(if key == KeyCode::ArrowUp {
                    at - 1
                } else {
                    at + 1
                });
            }
            KeyCode::ArrowDown if !typing => self.step(true),
            KeyCode::ArrowUp => self.step(false),
            KeyCode::Enter | KeyCode::NumpadEnter if typing => {
                // The keyboard stays in the field, sent or not.
                let action = self.activate(SEND_TOKEN);
                self.focus = FIELD_TOKEN;
                return action;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => return self.activate(self.focus),
            KeyCode::Space if !typing => return self.activate(self.focus),
            KeyCode::Backspace if typing => {
                self.draft.pop();
            }
            _ if typing => {
                crate::sjk_chat_field::key_into(&mut self.draft, key, text, shift, control);
            }
            _ => {}
        }
        PanelAction::None
    }

    /// A pointer event.
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        if let InputEvent::PointerWheel { delta, .. } = event {
            if self.gifs.open {
                // Over the picker, the wheel scrolls its tiles.
                if delta.y != 0.0 {
                    self.gifs.scroll(if delta.y > 0.0 { -1 } else { 1 });
                }
                return PanelAction::None;
            }
            if delta.y > 0.0 {
                let most = self.shown.total.saturating_sub(1);
                self.scroll = (self.scroll + 1).min(most);
            } else if delta.y < 0.0 {
                self.scroll = self.scroll.saturating_sub(1);
            }
            return PanelAction::None;
        }
        if let InputEvent::PointerMove(at) = event {
            self.pointer = Some(at);
        }
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            Some(token) => self.activate(token),
            None => PanelAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::ChatMessage;

    pub(super) fn message(id: u64, key: &str, text: &str) -> ChatMessage {
        ChatMessage {
            id,
            at: 1_791_300_000 + id as i64 * 60,
            key_id: key.to_owned(),
            name: format!("^{}Player {id}", id % 8),
            verified: id.is_multiple_of(3),
            staff: id.is_multiple_of(5),
            text: text.to_owned(),
            holocron: None,
        }
    }

    pub(super) fn chat(count: u64) -> ChatState {
        ChatState {
            messages: (1..=count)
                .map(|id| message(id, &format!("{id:016x}"), &format!("message {id}")))
                .collect(),
            revision: 1,
            online: 9,
            people: None,
            live: true,
            outcome: None,
            loaded: Some(1),
        }
    }

    pub(super) fn inputs<'a>(chat: &'a ChatState, muted: &'a [String], staff: bool) -> Inputs<'a> {
        Inputs {
            chat: Some(chat),
            muted,
            staff,
            enabled: true,
            now: 1_791_400_000,
            staff_state: None,
            giphy_key: "",
        }
    }

    /// The panel after one frame, so its controls are known.
    pub(super) fn drawn(panel: &mut Panel, inputs: &Inputs<'_>) {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        panel.build_with(inputs, &fonts.font, [1920.0, 1080.0]);
    }

    fn press(panel: &mut Panel, key: KeyCode, text: Option<&str>) -> PanelAction {
        panel.edit(key, text, false, false)
    }

    #[test]
    fn the_page_lists_messages_newest_at_the_bottom_and_scrolls() {
        let chat = chat(60);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], false));
        let ids: Vec<u64> = panel.shown.messages.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.last(), Some(&60), "the newest at the bottom");
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(ids.len() < 60, "a page of them");
        // Up from the field chooses the newest; Page Up scrolls older ones in.
        press(&mut panel, KeyCode::ArrowUp, None);
        assert_eq!(panel.selected, Some(60));
        press(&mut panel, KeyCode::PageUp, None);
        drawn(&mut panel, &inputs(&chat, &[], false));
        assert_eq!(panel.shown.messages.last().map(|(id, _)| *id), Some(55));
        press(&mut panel, KeyCode::PageDown, None);
        assert_eq!(panel.scroll, 0);
    }

    #[test]
    fn up_and_down_move_through_the_messages() {
        let chat = chat(3);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], false));
        press(&mut panel, KeyCode::ArrowUp, None);
        press(&mut panel, KeyCode::ArrowUp, None);
        assert_eq!(panel.selected, Some(2));
        press(&mut panel, KeyCode::ArrowDown, None);
        assert_eq!(panel.selected, Some(3));
        // Down from the newest goes back to the field.
        press(&mut panel, KeyCode::ArrowDown, None);
        assert_eq!(panel.focus, FIELD_TOKEN);
    }

    #[test]
    fn send_takes_the_draft_and_clears_it() {
        let chat = chat(1);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], false));
        for c in "gg wp".chars() {
            let text = c.to_string();
            assert_eq!(
                press(&mut panel, KeyCode::KeyA, Some(&text)),
                PanelAction::None
            );
        }
        // Keys that would act elsewhere type here.
        press(&mut panel, KeyCode::KeyS, Some("s"));
        press(&mut panel, KeyCode::Backspace, Some("\u{8}"));
        assert_eq!(
            press(&mut panel, KeyCode::Enter, Some("\r")),
            PanelAction::Send("gg wp".into())
        );
        assert!(panel.draft.is_empty());
        assert_eq!(
            press(&mut panel, KeyCode::Enter, Some("\r")),
            PanelAction::None
        );
        // The draft holds at most what the hub takes.
        for _ in 0..200 {
            press(&mut panel, KeyCode::KeyA, Some("a"));
        }
        assert_eq!(panel.draft.chars().count(), DRAFT_MAX);
        assert_eq!(press(&mut panel, KeyCode::Escape, None), PanelAction::Close);
    }

    #[test]
    fn a_chosen_message_can_be_muted_on_this_pc() {
        let chat = chat(2);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], false));
        assert!(!panel.order.contains(&MUTE_TOKEN), "nothing chosen yet");
        let _ = panel.activate(MESSAGE_BASE + 1);
        assert_eq!(panel.selected, Some(2));
        drawn(&mut panel, &inputs(&chat, &[], false));
        let key = format!("{:016x}", 2);
        let mute = |muted| PanelAction::Mute {
            key_id: key.clone(),
            name: "^2Player 2".to_owned(),
            muted,
        };
        assert_eq!(panel.activate(MUTE_TOKEN), mute(true));
        let muted = [key.clone()];
        drawn(&mut panel, &inputs(&chat, &muted, false));
        assert_eq!(panel.activate(MUTE_TOKEN), mute(false));
    }

    #[test]
    fn resting_on_a_name_shows_the_sender_card_with_mute() {
        use crate::sender_card::Place;
        use sjk_ui::{InputEvent, Rect, Vec2};
        let centre = |rect: Rect| Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        let chat = chat(3);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], false));
        assert!(panel.card.is_none());
        // The second message's name, not its row.
        let name = panel.ui.rect_for(NAME_BASE + 1).expect("a name");
        let row = panel.ui.rect_for(MESSAGE_BASE + 1).expect("its row");
        assert!(
            name.width < row.width * 0.5,
            "the name alone: {name:?} {row:?}"
        );
        let _ = panel.handle_pointer(InputEvent::PointerMove(centre(name)));
        drawn(&mut panel, &inputs(&chat, &[], false));
        let card = panel.card.as_ref().expect("the card");
        assert_eq!(card.source, CardSource::Message(2));
        assert_eq!(card.person.name, "^2Player 2");
        assert_eq!(card.person.place, Place::Unknown, "placed outside the lock");
        // Where they are comes from outside the chat's lock, once.
        let asked = std::cell::Cell::new(0);
        let place = |key: Option<&str>, name: &str| {
            asked.set(asked.get() + 1);
            assert_eq!(
                (key, name),
                (Some(format!("{:016x}", 2).as_str()), "^2Player 2")
            );
            Place::SlotByName(5)
        };
        // Its picture's version is asked until it is known: not yet, then none.
        let pictures = std::cell::Cell::new(0);
        let picture = |answer: Option<&str>| {
            let pictures = &pictures;
            let answer = answer.map(str::to_owned);
            move |key: &str| {
                assert_eq!(key, format!("{:016x}", 2));
                pictures.set(pictures.get() + 1);
                answer
            }
        };
        panel.place_card(place, picture(None));
        panel.place_card(place, picture(Some("")));
        panel.place_card(place, picture(Some("0123456789abcdef")));
        assert_eq!((asked.get(), pictures.get()), (1, 2));
        assert_eq!(
            panel.card.as_ref().unwrap().person.avatar.as_deref(),
            Some("")
        );
        drawn(&mut panel, &inputs(&chat, &[], false));
        let texts: Vec<&str> = panel.ui.text_runs().collect();
        for wanted in ["On this server, slot 5, matched by name", "Mute"] {
            assert!(texts.contains(&wanted), "{wanted:?} in {texts:?}");
        }
        let card = panel.ui.rect_for(CARD_TOKEN).expect("the card's target");
        assert!(
            card.x >= name.right() || card.right() <= name.x,
            "beside the name"
        );
        // Its Mute asks for this sender by key and name; muted, it offers Unmute.
        let key = format!("{:016x}", 2);
        let mute = |muted| PanelAction::Mute {
            key_id: key.clone(),
            name: "^2Player 2".to_owned(),
            muted,
        };
        assert_eq!(panel.activate(CARD_MUTE_TOKEN), mute(true));
        let button = panel.ui.rect_for(CARD_MUTE_TOKEN).expect("Mute");
        let _ = panel.handle_pointer(InputEvent::PointerMove(centre(button)));
        let muted = [key.clone()];
        drawn(&mut panel, &inputs(&chat, &muted, false));
        assert!(panel.card.is_some(), "kept while the pointer is on it");
        assert!(panel.ui.text_runs().any(|text| text == "Unmute"));
        assert_eq!(panel.activate(CARD_MUTE_TOKEN), mute(false));
        // A click on the name chooses its message.
        let _ = panel.activate(NAME_BASE + 1);
        assert_eq!(panel.selected, Some(2));
        // Away from both: gone.
        let _ = panel.handle_pointer(InputEvent::PointerMove(Vec2::new(4.0, 1_070.0)));
        drawn(&mut panel, &inputs(&chat, &muted, false));
        assert!(panel.card.is_none());
    }

    #[test]
    fn staff_actions_only_for_staff() {
        let chat = chat(4);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], false));
        let _ = panel.activate(MESSAGE_BASE + 3);
        drawn(&mut panel, &inputs(&chat, &[], false));
        for token in [DELETE_TOKEN, HUB_MUTE_TOKEN, HUB_UNMUTE_TOKEN] {
            assert!(!panel.order.contains(&token));
            assert_eq!(panel.activate(token), PanelAction::None);
        }
        drawn(&mut panel, &inputs(&chat, &[], true));
        let key = format!("{:016x}", 4);
        assert_eq!(
            panel.activate(DELETE_TOKEN),
            PanelAction::Staff(StaffRequest::ChatDelete { id: 4 })
        );
        assert_eq!(
            panel.activate(HUB_MUTE_TOKEN),
            PanelAction::Staff(StaffRequest::ChatMute {
                key_id: key.clone(),
                muted: true
            })
        );
        assert_eq!(
            panel.activate(HUB_UNMUTE_TOKEN),
            PanelAction::Staff(StaffRequest::ChatMute {
                key_id: key,
                muted: false
            })
        );
    }

    #[test]
    fn tab_walks_every_control() {
        let chat = chat(3);
        let mut panel = Panel::new();
        panel.open(true);
        drawn(&mut panel, &inputs(&chat, &[], true));
        let _ = panel.activate(MESSAGE_BASE);
        drawn(&mut panel, &inputs(&chat, &[], true));
        for token in [
            FIELD_TOKEN,
            SEND_TOKEN,
            MESSAGE_BASE,
            MUTE_TOKEN,
            DELETE_TOKEN,
        ] {
            assert!(panel.order.contains(&token), "{token}");
        }
        let start = panel.focus;
        for _ in 0..panel.order.len() {
            panel.edit(KeyCode::Tab, Some("\t"), false, false);
        }
        assert_eq!(panel.focus, start, "Tab comes round");
        panel.edit(KeyCode::Tab, Some("\t"), true, false);
        assert_ne!(panel.focus, start);
    }
}

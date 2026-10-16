//! Floating conversation overlay. Chat owns presentation/history only; player
//! identity and stock command semantics come from the client adapter.

mod card;
mod editing;
mod editor;
pub(crate) mod emoji;
mod feed_gif;

mod interaction;
mod layout;
mod options;
mod player_actions;
pub(crate) mod reply;
pub(crate) mod sjk;
mod social;

mod view;

pub(crate) use view::star;

pub(crate) mod combat;

use crate::game_font::{GameFonts, RetailFont};
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use editor::Editor;
use sjk_client::{
    ChatRoster, ChatTarget, ServerEventKind, chat_body, chat_display_text, chat_unescape,
};
use sjk_ui::{DrawList, Tween};
use std::collections::VecDeque;
use std::time::Instant;

const HISTORY_LIMIT: usize = 64;
const MAX_VISIBLE: usize = 8;
const HOLD_MS: u64 = 9_000;
const FADE_MS: u64 = 2_000;
const ENTER_MS: u32 = 220;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Channel {
    Global,
    Team,
    Whisper,
    /// The SJK chat, through the SJK hub (`chat/sjk.rs`).
    Sjk,
}

struct ChatLine {
    name: String,
    body: String,
    sender: Option<ChatTarget>,
    channel: Channel,
    received_ms: u64,
    muted: bool,
    /// The emojis the body's marks stand for, in order ([`emoji::Emojis::markup`]).
    emojis: Vec<u16>,
    wrap: layout::Wrapped,
    y: Option<Tween>,
    /// What the hub said of an SJK chat line.
    hub: Option<HubLine>,
    /// The GIF an SJK chat line links to, shown under it (`chat/feed_gif.rs`).
    gif: Option<Box<feed_gif::LineGif>>,
}

/// An SJK chat line's id at the hub and its sender.
struct HubLine {
    id: u64,
    verified: bool,
    staff: bool,
    key_id: String,
    /// The line is a holocron drop of this tier (`holocrons/line.rs`): in the tier's
    /// colour, a gem before it instead of the verified tick.
    tier: Option<&'static crate::holocrons::Tier>,
}

#[derive(Clone, Copy)]
struct PlayerMenu {
    target: ChatTarget,
    anchor_y: f32,
    selected: Option<u16>,
}

/// Bounded incoming history and the currently active unboxed composer.
pub(crate) struct ChatOverlay {
    lines: VecDeque<ChatLine>,
    center: Option<(String, u64)>,
    /// Text ids of this frame's centre-print rows, which retail drew with the
    /// medium font rather than the chat box's small one.
    center_text: (u32, u32),
    combat: combat::Combat,
    input: Option<Editor>,
    modifiers: winit::keyboard::ModifiersState,
    roster: ChatRoster,
    muted: Vec<ChatTarget>,
    friends: social::Friends,
    player_menu: Option<PlayerMenu>,
    scroll: usize,
    unread: usize,
    clock: Instant,
    opened_ms: u64,
    notice: &'static str,
    ui: MenuCanvas,
    visible_targets: [Option<ChatTarget>; MAX_VISIBLE],
    pressed_action: Option<(u16, Option<ChatTarget>)>,
    /// Whose name each feed token is, for the profile card (`chat/card.rs`).
    visible_people: [Option<card::Who>; MAX_VISIBLE],
    /// The profile card on show while the composer is open.
    card: Option<card::Card>,
    /// Mute or Unmute asked from the card, until the frame takes it.
    mute_request: Option<card::MuteRequest>,
    /// Client slots muted on this PC (bits, `muted_players.rs`): their lines are hidden.
    muted_slots: u32,
    layout_viewport: [f32; 2],
    scoreboard_layout: crate::scoreboard::behavior::ChatLayout,
    options: options::Options,
    emojis: emoji::Emojis,
    /// The newest SJK chat id the feed has seen; `None` before the first sync.
    sjk_seen: Option<u64>,
    /// The reading of the hub (`ChatState::loaded`) whose backlog was marked.
    sjk_epoch: Option<u64>,
    /// The serial of the last SJK chat outcome shown.
    sjk_outcome: u64,
    /// The SJK chat's revision, outcome serial and mutes revision last followed.
    sjk_mark: Option<(u64, u64, u64)>,
    /// When the SJK chat's sound last played (`chat/sjk.rs`, [`sjk::SOUND_GAP`]).
    sjk_sound_at: Option<Instant>,
    /// Whether the lines with a GIF read "GIF" and show it (`cl_sjkChatGifs`), or keep
    /// their links as text (`chat/feed_gif.rs`).
    gifs_shown: bool,
    /// Our own client slot, from the gamestate: our whispers echoed back are not
    /// whispers to reply to (`chat/reply.rs`).
    own_slot: Option<u16>,
    /// The last player who whispered to us, for `reply`.
    last_whisperer: Option<ChatTarget>,
    /// The channel (and whisper recipient) the composer last sent on, which the chat
    /// key opens on while `cg_chatRememberChannel` is on.
    last_channel: (Channel, Option<ChatTarget>),
    /// The feed draws during a world shot, which has no session.
    #[cfg(test)]
    pub(crate) for_shot: bool,
}

impl ChatOverlay {
    /// Borrow an existing coloured roster name for the overhead HUD; never copy it.
    pub(crate) fn player_label(&self, slot: u16) -> &str {
        self.roster
            .target(Some(slot))
            .and_then(|target| self.roster.display_name(target))
            .unwrap_or("")
    }
    pub(crate) fn new() -> Self {
        Self {
            lines: VecDeque::with_capacity(HISTORY_LIMIT),
            center: None,
            center_text: (0, 0),
            combat: combat::Combat::default(),
            input: None,
            modifiers: winit::keyboard::ModifiersState::empty(),
            roster: ChatRoster::default(),
            muted: Vec::with_capacity(HISTORY_LIMIT),
            friends: social::Friends::default(),
            player_menu: None,
            scroll: 0,
            unread: 0,
            clock: Instant::now(),
            opened_ms: 0,
            notice: "",
            ui: MenuCanvas::with_text_capacity(512),
            visible_targets: [None; MAX_VISIBLE],
            pressed_action: None,
            visible_people: [None; MAX_VISIBLE],
            card: None,
            mute_request: None,
            muted_slots: 0,
            layout_viewport: [0.0; 2],
            scoreboard_layout: crate::scoreboard::behavior::ChatLayout::Normal,
            options: options::Options::default(),
            emojis: emoji::Emojis::default(),
            sjk_seen: None,
            sjk_epoch: None,
            sjk_outcome: 0,
            sjk_mark: None,
            sjk_sound_at: None,
            gifs_shown: true,
            own_slot: None,
            last_whisperer: None,
            last_channel: (Channel::Global, None),
            #[cfg(test)]
            for_shot: false,
        }
    }

    /// An overlay showing the installed world's emoji pictures.
    pub(crate) fn with_emojis(emojis: emoji::Emojis) -> Self {
        Self {
            emojis,
            ..Self::new()
        }
    }

    fn millis(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.clock)
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    pub(crate) fn update_roster(&mut self, game: &sjk_protocol::GameState) {
        self.roster.update(game);
        self.own_slot = u16::try_from(game.client_num).ok();
        self.muted
            .retain(|target| self.roster.name(*target).is_some());
    }

    /// Who is in each slot, as roster observations: one changes when its player
    /// leaves, another takes the slot or they rename.
    pub(crate) fn roster_targets(&self) -> [Option<ChatTarget>; crate::chat_mutes::SLOTS] {
        std::array::from_fn(|slot| self.roster.target(Some(slot as u16)))
    }

    /// The client slots muted on this PC changed (or not: this is called every frame):
    /// hide their lines, and show again those of players no longer muted.
    pub(crate) fn set_muted_slots(&mut self, slots: u32) {
        if self.muted_slots == slots {
            return;
        }
        self.muted_slots = slots;
        for index in 0..self.lines.len() {
            let line = &self.lines[index];
            if line.hub.is_some() {
                continue;
            }
            let muted = match line.sender {
                Some(target) => self.muted.contains(&target) || self.slot_muted(target),
                None => self.prefix_muted(&line.body),
            };
            self.lines[index].muted = muted;
        }
    }

    /// Whether `target`'s slot is muted on this PC.
    fn slot_muted(&self, target: ChatTarget) -> bool {
        target.slot() < 32 && self.muted_slots & (1 << target.slot()) != 0
    }

    /// Whether a line the server did not attribute starts with the name the game shows
    /// in a muted slot, then `: ` (colour codes ignored), as JA+ servers send chat.
    fn prefix_muted(&self, text: &str) -> bool {
        self.muted_slots != 0
            && (0..32_u16)
                .filter(|slot| self.muted_slots & (1 << slot) != 0)
                .filter_map(|slot| self.roster.target(Some(slot)))
                .filter_map(|target| self.roster.display_name(target))
                .any(|name| chat_body(text, name).0.len() < text.len())
    }

    pub(crate) fn receive(
        &mut self,
        kind: ServerEventKind,
        mut text: String,
        sender: Option<u16>,
        now: Instant,
    ) {
        // Console output never enters conversation history, including direct callers.
        if kind == ServerEventKind::Print {
            return;
        }
        let ms = self.millis(now);
        if kind == ServerEventKind::CenterPrint {
            text.reserve(1024usize.saturating_sub(text.len()));
            self.combat.height = None;
            self.center = Some((text, ms));
            return;
        }
        if kind == ServerEventKind::Chat && self.options.clean != 0 {
            let plain = options::clean_body(&text, 1).to_ascii_lowercase();
            if plain.contains("media - currently playing: ") || plain.contains("hi everybody!") {
                return;
            }
        }
        // EternalJK's escapes for `%` and `"` show as what was typed, in the
        // chat box only, as in EternalJK.
        let display = chat_display_text(&chat_unescape(&text));
        let target = self.roster.target(sender);
        // Display keeps `^n`; identity, whisper destinations and muting keep
        // using the plain roster name.
        let name = target
            .and_then(|target| self.roster.display_name(target))
            .unwrap_or("")
            .to_owned();
        let (body, private) = if name.is_empty() {
            (display.as_str(), false)
        } else {
            chat_body(&display, &name)
        };
        match target {
            Some(_) => self.note_whisper(private, target),
            // The line stays unattributed; only `reply` reads its sender from it.
            None if kind == ServerEventKind::Chat => {
                let sender = self.whisper_sender(&display);
                self.note_whisper(sender.is_some(), sender);
            }
            None => {}
        }
        let body = options::clean_body(body, self.options.clean);
        let (body, emojis) = if self.options.emojis {
            self.emojis.markup(&body)
        } else {
            (body, Vec::new())
        };
        if self.options.clean != 0
            && self.lines.back().is_some_and(|line| {
                line.sender == target && line.body == body && line.emojis == emojis
            })
        {
            return;
        }
        let line = ChatLine {
            name,
            body,
            sender: target,
            channel: if private {
                Channel::Whisper
            } else if kind == ServerEventKind::TeamChat {
                Channel::Team
            } else {
                Channel::Global
            },
            received_ms: ms,
            // Ignored for this map, or muted on this PC (`muted_players.rs`).
            muted: match target {
                Some(target) => self.muted.contains(&target) || self.slot_muted(target),
                None => self.prefix_muted(&display),
            },
            emojis,
            wrap: layout::Wrapped::default(),
            y: None,
            hub: None,
            gif: None,
        };
        if self.lines.len() == HISTORY_LIMIT {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
        if self.is_typing() && self.scroll > 0 {
            self.scroll = (self.scroll + 1).min(self.lines.len().saturating_sub(1));
            self.unread = (self.unread + 1).min(HISTORY_LIMIT);
        }
    }

    pub(crate) fn open(&mut self, team: bool) {
        self.input = Some(Editor::new(if team {
            Channel::Team
        } else {
            Channel::Global
        }));
        self.player_menu = None;
        self.pressed_action = None;
        self.scroll = 0;
        self.unread = 0;
        self.notice = "";
        self.opened_ms = self.millis(Instant::now());
    }

    /// Open the field and type `text` into it, for world shots.
    #[cfg(test)]
    pub(crate) fn draft_for_shot(&mut self, text: &str) {
        self.open(false);
        if let Some(input) = &mut self.input {
            input.insert(text);
        }
    }

    pub(crate) fn is_typing(&self) -> bool {
        self.input.is_some()
    }

    /// Toggle a whisper composer only for a currently occupied roster slot.
    pub(crate) fn whisper_to(&mut self, slot: Option<u16>) -> bool {
        let Some(target) = self.roster.target(slot) else {
            return false;
        };
        if self.is_typing() {
            self.input = None;
            return true;
        }
        self.open(false);
        let input = self.input.as_mut().expect("opened composer");
        input.channel = Channel::Whisper;
        input.recipient = Some(target);
        true
    }

    /// Whether a world shot asked for the feed (never outside the tests).
    fn shown_for_shot(&self) -> bool {
        #[cfg(test)]
        return self.for_shot;
        #[cfg(not(test))]
        false
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn set_scoreboard_layout(
        &mut self,
        enabled: crate::scoreboard::behavior::ChatLayout,
    ) {
        if self.scoreboard_layout != enabled {
            self.scoreboard_layout = enabled;
            for line in &mut self.lines {
                line.y = None;
            }
            self.pressed_action = None;
        }
    }

    /// Lay out and append chat text: the chat box in the small game font and
    /// centre prints in the medium one when `ui_gameFont` has them loaded
    /// (`CG_ChatBox_DrawStrings` paints with `FONT_SMALL`, `CG_DrawCenterString`
    /// with `FONT_MEDIUM`), otherwise in `font`.
    pub(crate) fn append(
        &mut self,
        draw_feed: bool,
        fonts: &mut GameFonts,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        _scale: f32,
    ) {
        let ms = self.millis(Instant::now());
        self.build(
            draw_feed,
            fonts.font(RetailFont::Small).unwrap_or(font),
            viewport,
            ms,
        );
        let (start, end) = self.center_text;
        self.ui.append_text_routed(
            fonts,
            |id, _| {
                Some(if (start..end).contains(&id.0) {
                    RetailFont::Medium
                } else {
                    RetailFont::Small
                })
            },
            vertices,
            font,
            viewport,
        );
    }
}

pub(crate) enum ChatInputResult {
    None,
    Submit(String),
    /// A message for the SJK chat, to send through the hub.
    Sjk(String),
}

#[cfg(test)]
mod escape_tests {
    use super::*;

    #[test]
    fn eternaljk_escapes_show_as_what_was_typed() {
        let mut chat = ChatOverlay::new();
        chat.receive(
            ServerEventKind::Chat,
            "^7Creyon: ^2100\u{b0}/. ''sure''".to_owned(),
            None,
            Instant::now(),
        );
        assert_eq!(chat.lines[0].body, "^7Creyon: ^2100% \"sure\"");
    }

    #[test]
    fn a_servers_chat_bytes_show_the_percent() {
        // As a JA+ server sends it: a Latin-1 name, 0x19 separators and the
        // escape's degree sign as byte 0xB0, which is not UTF-8.
        let bytes = b"jof\xbbdmg\xabcreyon^7\x19: ^2\xb0/.\x19";
        let text = sjk_client::decode_legacy(bytes).into_owned();
        for emojis in [false, true] {
            let mut chat = ChatOverlay::new();
            chat.options.emojis = emojis;
            chat.receive(ServerEventKind::Chat, text.clone(), None, Instant::now());
            assert!(
                chat.lines[0].body.ends_with("^2%"),
                "{:?}",
                chat.lines[0].body
            );
        }
    }
}

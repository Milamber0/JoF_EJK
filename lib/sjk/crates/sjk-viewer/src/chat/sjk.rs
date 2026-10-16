//! The SJK channel (`docs/hub-chat.md`): the chat every SJK player shares through the
//! SJK hub, shown in the feed beside the game server's and typed in the same composer.
//! Its messages come from the identity service's chat state, never from the game
//! server, and what is typed on it goes to the hub, never to the game server.

use super::*;
use sjk_identity::{ChatMessage, ChatState, ReportOutcome};

/// Messages the identity service keeps (`sjk_identity::feed::MESSAGES_KEPT`): while it
/// holds fewer, none has dropped off its end.
const KEPT: usize = sjk_identity::feed::MESSAGES_KEPT;

impl ChatOverlay {
    /// Open the composer on the SJK channel.
    pub(crate) fn open_sjk(&mut self) {
        self.open(false);
        if let Some(input) = &mut self.input {
            input.channel = Channel::Sjk;
        }
    }

    /// A message for the SJK channel: its name and text through the chat rules, as
    /// anything a hub sends is shown, the text without its colour codes, as it is
    /// drawn in the SJK chat's gold ([`crate::sjk_chat_look::message_text`]).
    fn push_sjk(&mut self, message: &ChatMessage, muted: bool, now: Instant) {
        // A holocron drop is one sentence with no sender column, in its tier's colour.
        let drop = crate::holocrons::line::words(message);
        let name = if drop.is_some() {
            String::new()
        } else {
            sjk_identity::chat::name_for_display(&message.name)
        };
        let body = match &drop {
            Some(words) => words.text.clone(),
            None => crate::sjk_chat_look::message_text(&message.text),
        };
        // A message's GIPHY link reads "GIF", its GIF under it (`chat/feed_gif.rs`).
        let (body, emojis, gif) = if drop.is_none() {
            self.with_gif(body)
        } else {
            (body, Vec::new(), None)
        };
        if self.lines.len() == HISTORY_LIMIT {
            self.lines.pop_front();
        }
        self.lines.push_back(ChatLine {
            name,
            body,
            sender: None,
            channel: Channel::Sjk,
            received_ms: self.millis(now),
            muted,
            emojis,
            wrap: layout::Wrapped::default(),
            y: None,
            hub: Some(HubLine {
                id: message.id,
                verified: message.verified && drop.is_none(),
                staff: message.staff,
                key_id: message.key_id.clone(),
                tier: drop.map(|words| words.tier),
            }),
            gif,
        });
        if self.is_typing() && self.scroll > 0 {
            self.scroll = (self.scroll + 1).min(self.lines.len().saturating_sub(1));
            self.unread = (self.unread + 1).min(HISTORY_LIMIT);
        }
    }

    /// Put the line a holocron drop of `tier` makes into the feed, for `debug_holocron`
    /// (as the player's own drop). The real lines come from the hub's feed
    /// ([`Self::sync_sjk`]).
    pub(crate) fn rehearse_holocron(&mut self, tier: &str, now: Instant) {
        let message = ChatMessage {
            id: u64::MAX,
            at: 0,
            key_id: String::new(),
            name: String::new(),
            verified: false,
            staff: false,
            text: String::new(),
            holocron: Some(sjk_identity::DropMark {
                tier: tier.to_owned(),
                own: true,
            }),
        };
        self.push_sjk(&message, false, now);
    }

    /// Whether the SJK chat's `mark` (its revision, outcome serial and the mutes'
    /// revision) differs from the one the feed last followed; it is the one followed
    /// from now on.
    pub(crate) fn sjk_changed(&mut self, mark: (u64, u64, u64)) -> bool {
        let changed = self.sjk_mark != Some(mark);
        self.sjk_mark = Some(mark);
        changed
    }

    /// Say in the feed why the hub refused what was sent, once per outcome.
    pub(crate) fn sjk_outcome(&mut self, outcome: Option<&ReportOutcome>, now: Instant) {
        let Some(outcome) = outcome else { return };
        if self.sjk_outcome == outcome.serial {
            return;
        }
        self.sjk_outcome = outcome.serial;
        if !outcome.sent {
            self.receive(
                ServerEventKind::Chat,
                format!("^3SJK chat: {}", outcome.message),
                None,
                now,
            );
        }
    }

    /// Bring the feed in line with the SJK chat `state`: new messages join it, ones
    /// staff deleted leave it, and lines follow the mutes (`muted` says whether the
    /// player muted a key). The backlog a hub sends when the reading starts is only
    /// marked, never replayed over the game: nothing is marked before the hub's first
    /// answer, and a new reading (`ChatState::loaded`) marks again.
    ///
    /// Returns the id the feed had seen when messages newer than it joined the feed now
    /// (those with a higher id in `state`), `None` when nothing new joined: a marked
    /// backlog is not news ([`worth_a_sound`] picks the ones worth a sound).
    pub(crate) fn sync_sjk(
        &mut self,
        state: &ChatState,
        muted: impl Fn(&str) -> bool,
        now: Instant,
    ) -> Option<u64> {
        for line in &mut self.lines {
            if let Some(hub) = &line.hub {
                line.muted = muted(&hub.key_id);
            }
        }
        let Some(epoch) = state.loaded else {
            return None;
        };
        let messages = &state.messages;
        let newest = messages.back().map_or(0, |message| message.id);
        if self.sjk_epoch != Some(epoch) {
            self.sjk_epoch = Some(epoch);
            self.sjk_seen = Some(newest);
            return None;
        }
        let seen = match self.sjk_seen {
            // A first look, or a hub whose ids began again: mark, show nothing.
            None => {
                self.sjk_seen = Some(newest);
                return None;
            }
            Some(seen) if newest < seen => {
                self.sjk_seen = Some(newest);
                return None;
            }
            Some(seen) => seen,
        };
        // A shown message the chat should hold but does not was deleted by staff.
        let lowest = if messages.len() >= KEPT {
            messages.front().map_or(0, |message| message.id)
        } else {
            0
        };
        self.lines.retain(|line| match &line.hub {
            Some(hub) if hub.id >= lowest && hub.id <= newest => {
                messages.iter().any(|message| message.id == hub.id)
            }
            _ => true,
        });
        for message in messages.iter().filter(|message| message.id > seen) {
            let muted = muted(&message.key_id);
            // A muted player's drops are not shown, rather than hidden behind a note;
            // a drop of a tier this client does not know is left out.
            if message.holocron.is_some() && (muted || crate::holocrons::line::unknown(message)) {
                continue;
            }
            self.push_sjk(message, muted, now);
        }
        self.sjk_seen = Some(newest);
        (newest > seen).then_some(seen)
    }

    /// Whether the SJK chat's sound may play at `now`: at most once in
    /// [`SOUND_GAP`], so a burst of messages plays it once. It counts as played.
    pub(crate) fn sjk_sound_due(&mut self, now: Instant) -> bool {
        if self
            .sjk_sound_at
            .is_some_and(|at| now.saturating_duration_since(at) < SOUND_GAP)
        {
            return false;
        }
        self.sjk_sound_at = Some(now);
        true
    }
}

/// The shortest time between two of the SJK chat's sounds.
pub(crate) const SOUND_GAP: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether `messages` hold one newer than `after` worth the SJK chat's sound: a message
/// (not a holocron drop) not muted on this PC (`muted`). The player's own messages
/// count (Sol's request, 11/10/2026): one joins the feed once the hub took it, so its
/// sound says it went out; a refused one never joins and plays nothing.
pub(crate) fn worth_a_sound<'a>(
    messages: impl IntoIterator<Item = &'a ChatMessage>,
    after: u64,
    muted: impl Fn(&str) -> bool,
) -> bool {
    messages
        .into_iter()
        .any(|message| message.id > after && message.holocron.is_none() && !muted(&message.key_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::ChatState;
    use std::collections::VecDeque;
    use winit::keyboard::{Key, KeyCode, NamedKey, SmolStr};

    fn message(id: u64, text: &str) -> ChatMessage {
        ChatMessage {
            id,
            at: 0,
            key_id: format!("{id:016x}"),
            name: "^2Sol".to_owned(),
            verified: id.is_multiple_of(2),
            staff: false,
            text: text.to_owned(),
            holocron: None,
        }
    }

    /// The chat as the service holds it, the hub's first answer in.
    fn hub(messages: &[(u64, &str)]) -> ChatState {
        loaded(
            1,
            messages
                .iter()
                .map(|(id, text)| message(*id, text))
                .collect(),
        )
    }

    fn loaded(epoch: u64, messages: VecDeque<ChatMessage>) -> ChatState {
        ChatState {
            messages,
            loaded: Some(epoch),
            ..ChatState::default()
        }
    }

    fn type_text(chat: &mut ChatOverlay, text: &str) {
        for c in text.chars() {
            let s = c.to_string();
            chat.edit_key(KeyCode::KeyA, &Key::Character(SmolStr::new(&s)), Some(&s));
        }
    }

    fn enter(chat: &mut ChatOverlay) -> ChatInputResult {
        chat.edit_key(KeyCode::Enter, &Key::Named(NamedKey::Enter), Some("\r"))
    }

    fn tab(chat: &mut ChatOverlay) {
        chat.edit_key(KeyCode::Tab, &Key::Named(NamedKey::Tab), Some("\t"));
    }

    fn channel(chat: &ChatOverlay) -> Channel {
        chat.input.as_ref().expect("composer open").channel
    }

    fn bodies(chat: &ChatOverlay) -> Vec<&str> {
        chat.lines.iter().map(|line| line.body.as_str()).collect()
    }

    #[test]
    fn tab_cycles_global_team_sjk() {
        let mut chat = ChatOverlay::new();
        chat.open(false);
        tab(&mut chat);
        assert!(channel(&chat) == Channel::Team);
        tab(&mut chat);
        assert!(channel(&chat) == Channel::Sjk);
        tab(&mut chat);
        assert!(channel(&chat) == Channel::Global);
        chat.activate(interaction::SJK);
        assert!(channel(&chat) == Channel::Sjk);
    }

    #[test]
    fn enter_on_sjk_hands_the_text_to_the_hub_not_the_server() {
        let mut chat = ChatOverlay::new();
        chat.open_sjk();
        assert!(channel(&chat) == Channel::Sjk);
        type_text(&mut chat, "gg all");
        assert!(matches!(enter(&mut chat), ChatInputResult::Sjk(text) if text == "gg all"));
        assert!(!chat.is_typing());
    }

    #[test]
    fn an_empty_sjk_draft_sends_nothing() {
        let mut chat = ChatOverlay::new();
        chat.open_sjk();
        type_text(&mut chat, "   ");
        assert!(matches!(enter(&mut chat), ChatInputResult::None));
    }

    #[test]
    fn the_first_sync_marks_the_backlog_without_showing_it() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        chat.sync_sjk(&hub(&[(1, "old"), (2, "older")]), |_| false, now);
        assert!(chat.lines.is_empty());
        chat.sync_sjk(
            &hub(&[(1, "old"), (2, "older"), (3, "new")]),
            |_| false,
            now,
        );
        assert_eq!(bodies(&chat), ["new"]);
    }

    #[test]
    fn hub_lines_join_the_feed_once_with_their_tag() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        chat.sync_sjk(&hub(&[]), |_| false, now);
        let messages = hub(&[(1, "hello ^1there"), (2, "bad \u{221E}x")]);
        chat.sync_sjk(&messages, |_| false, now);
        chat.sync_sjk(&messages, |_| false, now);
        // The text is drawn in the SJK chat's gold, without its colour codes; the
        // name keeps them.
        assert_eq!(bodies(&chat), ["hello there", "bad x"]);
        let line = &chat.lines[0];
        assert!(line.channel == Channel::Sjk);
        assert_eq!(line.name, "^2Sol");
        assert!(line.sender.is_none(), "no game slot: no player menu");
        let font = crate::text::test_font();
        chat.build(true, &font, [1920.0, 1080.0], 1_000);
        let runs: Vec<&str> = chat.ui.text_runs().collect();
        assert!(
            !runs.contains(&"SJK"),
            "the emblem, not the letters: {runs:?}"
        );
        assert!(!runs.contains(&"SJK VERIFIED"), "the tick alone: {runs:?}");
        let logos = chat
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, sjk_ui::DrawCommand::TexturedQuad { texture, .. }
                    if *texture == crate::ui_renderer::LOGO_TEXTURE)
            })
            .count();
        assert_eq!(logos, 2, "each line has the SJK emblem");
    }

    #[test]
    fn deleted_hub_messages_leave_the_feed() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        chat.sync_sjk(&hub(&[]), |_| false, now);
        chat.receive(ServerEventKind::Chat, "server line".to_owned(), None, now);
        chat.sync_sjk(&hub(&[(4, "spam"), (5, "fine")]), |_| false, now);
        chat.sync_sjk(&hub(&[(5, "fine")]), |_| false, now);
        assert_eq!(bodies(&chat), ["server line", "fine"]);
        // A hub that restarted begins its ids again: what was shown stays, and its
        // backlog is not replayed.
        chat.sync_sjk(&hub(&[(1, "after a restart")]), |_| false, now);
        assert_eq!(bodies(&chat), ["server line", "fine"]);
        chat.sync_sjk(&hub(&[(1, "after a restart"), (2, "new")]), |_| false, now);
        assert_eq!(bodies(&chat), ["server line", "fine", "new"]);
    }

    #[test]
    fn a_full_chat_does_not_take_old_lines_for_deleted() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        chat.sync_sjk(&hub(&[]), |_| false, now);
        chat.sync_sjk(&hub(&[(1, "first")]), |_| false, now);
        let full = loaded(1, (2..2 + KEPT as u64).map(|id| message(id, "x")).collect());
        chat.sync_sjk(&full, |_| false, now);
        assert_eq!(
            bodies(&chat)[..HISTORY_LIMIT - 1],
            vec!["x"; HISTORY_LIMIT - 1][..]
        );
        let mut chat = ChatOverlay::new();
        chat.sync_sjk(&hub(&[]), |_| false, now);
        chat.sync_sjk(&hub(&[(1, "first")]), |_| false, now);
        // The service dropped message 1 off its end: still shown.
        let mut later = full.clone();
        later.messages.pop_back();
        chat.sync_sjk(&later, |_| false, now);
        assert_eq!(chat.lines.front().map(|line| line.body.as_str()), Some("x"));
    }

    #[test]
    fn only_messages_newer_than_the_feed_had_seen_are_news() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        // The hub not answered yet, then its backlog at start or a reconnection: no news.
        assert_eq!(chat.sync_sjk(&ChatState::default(), |_| false, now), None);
        assert_eq!(
            chat.sync_sjk(&hub(&[(1, "old"), (2, "older")]), |_| false, now),
            None
        );
        assert_eq!(
            chat.sync_sjk(&hub(&[(1, "old"), (2, "older")]), |_| false, now),
            None
        );
        let state = hub(&[(1, "old"), (2, "older"), (3, "new"), (4, "newer")]);
        assert_eq!(chat.sync_sjk(&state, |_| false, now), Some(2));
        assert_eq!(chat.sync_sjk(&state, |_| false, now), None, "once");
        // A new reading of the hub marks its backlog again.
        let again = loaded(2, state.messages.clone());
        assert_eq!(chat.sync_sjk(&again, |_| false, now), None);
    }

    #[test]
    fn a_sound_is_for_messages_own_included_not_muted_or_drops() {
        let mut messages: Vec<ChatMessage> = (1..=3).map(|id| message(id, "hi")).collect();
        messages[2].holocron = Some(sjk_identity::DropMark {
            tier: "legendary".to_owned(),
            own: false,
        });
        let troll = message(2, "").key_id;
        let news = |after| worth_a_sound(&messages, after, |key| key == troll);
        assert!(news(0), "message 1, the player's own or another's");
        assert!(!news(1), "2 is muted, 3 a holocron drop");
        assert!(!news(3), "nothing newer");
        assert!(worth_a_sound(&messages, 1, |_| false));
    }

    #[test]
    fn a_burst_plays_the_sound_once_a_second() {
        let mut chat = ChatOverlay::new();
        let start = Instant::now();
        assert!(chat.sjk_sound_due(start));
        assert!(!chat.sjk_sound_due(start + std::time::Duration::from_millis(300)));
        assert!(!chat.sjk_sound_due(start + std::time::Duration::from_millis(999)));
        assert!(chat.sjk_sound_due(start + SOUND_GAP));
    }

    #[test]
    fn the_feed_follows_the_chat_only_when_it_changed() {
        let mut chat = ChatOverlay::new();
        assert!(chat.sjk_changed((0, 0, 0)));
        assert!(!chat.sjk_changed((0, 0, 0)));
        assert!(chat.sjk_changed((1, 0, 0)));
        assert!(chat.sjk_changed((1, 1, 0)));
        assert!(chat.sjk_changed((1, 1, 1)), "a mute changed");
    }

    #[test]
    fn refused_messages_say_why_once() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        let refused = ReportOutcome {
            serial: 1,
            sent: false,
            message: "you send messages too fast; wait a moment".to_owned(),
        };
        chat.sjk_outcome(Some(&refused), now);
        chat.sjk_outcome(Some(&refused), now);
        assert_eq!(
            bodies(&chat),
            ["^3SJK chat: you send messages too fast; wait a moment"]
        );
        let sent = ReportOutcome {
            serial: 2,
            sent: true,
            message: String::new(),
        };
        chat.sjk_outcome(Some(&sent), now);
        assert_eq!(chat.lines.len(), 1);
    }

    #[test]
    fn a_muted_key_stays_muted() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        chat.sync_sjk(&hub(&[]), |_| false, now);
        let troll = message(3, "spam");
        chat.sync_sjk(
            &loaded(1, VecDeque::from([troll.clone()])),
            |key| key == troll.key_id,
            now,
        );
        assert!(chat.lines[0].muted);
    }

    #[test]
    fn a_muted_key_hides_lines_already_shown() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        chat.sync_sjk(&hub(&[]), |_| false, now);
        let state = hub(&[(1, "spam"), (2, "fine")]);
        chat.sync_sjk(&state, |_| false, now);
        assert!(!chat.lines[0].muted);
        let troll = message(1, "").key_id;
        chat.sync_sjk(&state, |key| key == troll, now);
        assert!(chat.lines[0].muted && !chat.lines[1].muted);
        chat.sync_sjk(&state, |_| false, now);
        assert!(!chat.lines[0].muted, "unmuted again");
    }

    #[test]
    fn the_backlog_is_not_replayed_while_or_after_it_loads() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        // The service has started but the hub has not answered: nothing is marked.
        chat.sync_sjk(&ChatState::default(), |_| false, now);
        // Its first answer brings the backlog: marked, not shown.
        chat.sync_sjk(&hub(&[(1, "old"), (2, "older")]), |_| false, now);
        assert!(chat.lines.is_empty());
        chat.sync_sjk(
            &hub(&[(1, "old"), (2, "older"), (3, "new")]),
            |_| false,
            now,
        );
        assert_eq!(bodies(&chat), ["new"]);
        // Reading starts again (another hub, the chat turned off and on): its backlog
        // is marked again rather than replayed.
        chat.sync_sjk(&ChatState::default(), |_| false, now);
        let mut again = hub(&[(1, "old"), (2, "older"), (3, "new"), (4, "newer")]);
        again.loaded = Some(2);
        chat.sync_sjk(&again, |_| false, now);
        assert_eq!(bodies(&chat), ["new"]);
    }
}

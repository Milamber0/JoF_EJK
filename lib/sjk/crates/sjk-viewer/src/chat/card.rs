//! The sender card in the game's chat (`docs/hub-chat.md`, "Muting a player"). While
//! the composer is open the pointer is free, and resting it on a name (an SJK chat
//! sender's, or the name of a player the server says sent a line) shows their card
//! ([`crate::sender_card`]) beside it, with Mute or Unmute. The card stays while the
//! pointer moves onto it. A muted player's lines are hidden as ignored ones are.

use super::*;
use crate::sender_card::{self, Person, Place};
use sjk_ui::Rect;

/// The card's pointer targets, after the player menu's.
pub(super) const CARD: u16 = 120;
pub(super) const CARD_MUTE: u16 = 121;

/// Whose name a feed token is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Who {
    /// An SJK chat line's sender, by the message's id at the hub.
    Hub(u64),
    /// A player on the server.
    Player(ChatTarget),
}

/// The card on show: built when the name under the pointer changes, drawn every frame.
pub(super) struct Card {
    pub(super) who: Who,
    pub(super) person: Person,
    pub(super) muted: bool,
    /// The name it is beside.
    pub(super) anchor: Rect,
}

/// Mute or unmute asked from a card, which the frame carries out
/// (`crate::player_mutes::set_muted`): local only, nothing is sent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MuteRequest {
    pub(crate) key_id: Option<String>,
    pub(crate) name: String,
    pub(crate) muted: bool,
}

impl ChatOverlay {
    /// After the feed is laid out: show the card of the name under the pointer, keep
    /// it while the pointer is on the card, else hide it. Not while the composer is
    /// closed (`active` false) or the player menu is open.
    pub(super) fn follow_card(&mut self, active: bool) {
        if !active || self.player_menu.is_some() {
            self.card = None;
            return;
        }
        let hovered = (0..MAX_VISIBLE).find_map(|token| {
            let who = self.visible_people[token]?;
            self.ui
                .token_hovered(token as u16)
                .then(|| (who, self.ui.rect_for(token as u16)))
        });
        match hovered {
            Some((who, anchor)) => {
                let anchor = anchor.unwrap_or_default();
                match &mut self.card {
                    Some(card) if card.who == who => card.anchor = anchor,
                    _ => {
                        self.card = self.person(who).map(|(person, muted)| Card {
                            who,
                            person,
                            muted,
                            anchor,
                        });
                    }
                }
            }
            None if self.ui.token_hovered(CARD) || self.ui.token_hovered(CARD_MUTE) => {}
            None => self.card = None,
        }
        // Their picture's version, until the hub's players or their profile say it.
        if let Some(card) = self
            .card
            .as_mut()
            .filter(|card| card.person.avatar.is_none())
            && let Some(key_id) = &card.person.key_id
        {
            card.person.avatar = crate::player_identity::avatar_version(key_id);
        }
    }

    /// Who `who` is, from the line or the server and what the hub and the mute list
    /// say, and whether they are muted.
    fn person(&self, who: Who) -> Option<(Person, bool)> {
        let person = match who {
            Who::Hub(id) => {
                let (line, hub) = self.lines.iter().find_map(|line| {
                    line.hub
                        .as_ref()
                        .filter(|hub| hub.id == id)
                        .map(|hub| (line, hub))
                })?;
                let place = crate::player_mutes::place(Some(&hub.key_id), &line.name);
                let mark = match place {
                    Place::Slot(slot) | Place::SlotByName(slot) => {
                        crate::player_identity::hub_mark(slot, self.player_label(u16::from(slot)))
                    }
                    _ => None,
                };
                Person {
                    name: line.name.clone(),
                    key_id: Some(hub.key_id.clone()),
                    hub_name: None,
                    verified: hub.verified,
                    staff: hub.staff,
                    avatar: mark.as_ref().map(|mark| mark.avatar.clone()),
                    medals: mark.map(|mark| mark.medals).unwrap_or_default(),
                    place,
                    note: None,
                }
            }
            Who::Player(target) => {
                let name = self.roster.display_name(target)?;
                let slot = u8::try_from(target.slot()).ok()?;
                // The hub's claim on the slot, under the name shown there, says whose
                // key it is (the badges' rule); without one they are muted by name.
                let mark = crate::player_identity::hub_mark(slot, name);
                Person {
                    name: name.to_owned(),
                    key_id: mark.as_ref().map(|mark| mark.key_id.clone()),
                    hub_name: mark
                        .as_ref()
                        .map(|mark| mark.name.clone())
                        .filter(|hub| !sjk_identity::names_match(hub, name)),
                    verified: mark.as_ref().is_some_and(|mark| mark.verified),
                    staff: false,
                    place: if mark.is_some() {
                        Place::Slot(slot)
                    } else {
                        Place::SlotByName(slot)
                    },
                    avatar: mark.as_ref().map(|mark| mark.avatar.clone()),
                    medals: mark.map(|mark| mark.medals).unwrap_or_default(),
                    note: None,
                }
            }
        };
        let muted = crate::player_mutes::is_muted(person.key_id.as_deref(), &person.name);
        Some((person, muted))
    }

    /// Draw the card on show beside its name, its targets over the feed.
    pub(super) fn build_card(&mut self, font: &UiFont, g: &layout::Geometry, viewport: [f32; 2]) {
        let Some(card) = &self.card else {
            return;
        };
        let measure = crate::sjk_chat_look::Measure::new(font, crate::text::TextStyle::NEUTRAL);
        let size = sender_card::size(&card.person, g.scale);
        let origin = sender_card::beside(card.anchor, size, viewport, 10.0 * g.scale);
        sender_card::draw(
            &mut self.ui,
            &sender_card::Card {
                person: &card.person,
                muted: card.muted,
                measure: Some(&measure),
            },
            origin,
            g.scale,
            sender_card::Tokens {
                card: CARD,
                mute: CARD_MUTE,
            },
        );
    }

    /// Mute or Unmute on the card: ask the frame to change the list.
    pub(super) fn toggle_card_mute(&mut self) {
        let Some(card) = &mut self.card else {
            return;
        };
        card.muted = !card.muted;
        self.mute_request = Some(MuteRequest {
            key_id: card.person.key_id.clone(),
            name: card.person.name.clone(),
            muted: card.muted,
        });
    }

    /// What a card asked, once.
    pub(crate) fn take_mute_request(&mut self) -> Option<MuteRequest> {
        self.mute_request.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::{ChatMessage, ChatState};
    use sjk_ui::{InputEvent, PointerButton, Vec2};

    const VIEWPORT: [f32; 2] = [1920.0, 1080.0];
    /// A key no other test mutes: the mute list is the process's own.
    const KEY: &str = "c0ffee00c0ffee01";

    fn centre(rect: Rect) -> Vec2 {
        Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    /// The composer open over one SJK line from `KEY` and one unattributed game line.
    fn chat() -> ChatOverlay {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        let state = |messages| ChatState {
            messages,
            loaded: Some(1),
            ..ChatState::default()
        };
        chat.sync_sjk(&state(Default::default()), |_| false, now);
        chat.receive(ServerEventKind::Chat, "Fox: hi".to_owned(), None, now);
        let message = ChatMessage {
            id: 7,
            at: 0,
            key_id: KEY.to_owned(),
            name: "^2Sol".to_owned(),
            verified: true,
            staff: true,
            text: "gg all".to_owned(),
            holocron: None,
        };
        chat.sync_sjk(&state([message].into_iter().collect()), |_| false, now);
        chat.open(false);
        chat
    }

    fn build(chat: &mut ChatOverlay) {
        chat.build(true, &crate::text::test_font(), VIEWPORT, 1_000);
    }

    fn texts(chat: &ChatOverlay) -> Vec<String> {
        chat.ui.text_runs().map(str::to_owned).collect()
    }

    #[test]
    fn resting_on_a_name_shows_the_sender_card_with_mute() {
        let mut chat = chat();
        build(&mut chat);
        assert!(chat.card.is_none());
        // Only the SJK line's name is a target: the game line names nobody.
        let names: Vec<_> = (0..MAX_VISIBLE)
            .filter_map(|token| chat.visible_people[token].zip(Some(token)))
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        let (who, token) = names[0];
        assert_eq!(who, Who::Hub(7));
        let name = chat.ui.rect_for(token as u16).expect("the name's target");
        chat.pointer(InputEvent::PointerMove(centre(name)));
        build(&mut chat);
        let card = chat.card.as_ref().expect("the card");
        assert_eq!(card.person.key_id.as_deref(), Some(KEY));
        assert!(card.person.verified && card.person.staff && !card.muted);
        let texts = texts(&chat);
        for wanted in ["^2Sol", "SJK player, SJK staff", "Mute"] {
            assert!(
                texts.iter().any(|text| text == wanted),
                "{wanted:?} in {texts:?}"
            );
        }
        let drawn = chat.ui.rect_for(CARD).expect("the card's target");
        assert!(drawn.x >= name.right(), "beside the name");
        // Onto the card and its Mute: it stays, and a click asks for the mute.
        let mute = chat.ui.rect_for(CARD_MUTE).expect("Mute");
        chat.pointer(InputEvent::PointerMove(centre(mute)));
        build(&mut chat);
        assert!(chat.card.is_some(), "kept while the pointer is on it");
        let mute = chat.ui.rect_for(CARD_MUTE).expect("Mute");
        let (position, button) = (centre(mute), PointerButton::Primary);
        chat.pointer(InputEvent::PointerPress { position, button });
        chat.pointer(InputEvent::PointerRelease { position, button });
        assert_eq!(
            chat.take_mute_request(),
            Some(MuteRequest {
                key_id: Some(KEY.to_owned()),
                name: "^2Sol".to_owned(),
                muted: true,
            })
        );
        assert_eq!(chat.take_mute_request(), None, "once");
        assert!(
            chat.card.as_ref().is_some_and(|card| card.muted),
            "now Unmute"
        );
        // Away from both: gone. Closed, the feed has no targets and no card.
        chat.pointer(InputEvent::PointerMove(Vec2::new(1_900.0, 10.0)));
        build(&mut chat);
        assert!(chat.card.is_none());
        chat.input = None;
        build(&mut chat);
        assert!(chat.card.is_none() && chat.visible_people.iter().all(Option::is_none));
    }

    #[test]
    fn a_muted_slots_lines_are_hidden_and_shown_again() {
        let mut chat = ChatOverlay::new();
        let mut game = sjk_protocol::GameState::empty_local(0);
        game.replace_config_string(1_131 + 3, b"n\\^1Dark^7Lord\\t\\0".to_vec())
            .expect("a player");
        chat.update_roster(&game);
        let now = Instant::now();
        // Attributed by the server, and a JA+ line naming nobody but starting with the
        // name the game shows in slot 3 (its colours aside).
        chat.receive(
            ServerEventKind::Chat,
            "^1Dark^7Lord^7: one".to_owned(),
            Some(3),
            now,
        );
        chat.receive(
            ServerEventKind::Chat,
            "DarkLord^7: two".to_owned(),
            None,
            now,
        );
        chat.receive(ServerEventKind::Chat, "Fox: three".to_owned(), None, now);
        assert!(chat.lines.iter().all(|line| !line.muted));
        chat.set_muted_slots(1 << 3);
        let muted: Vec<bool> = chat.lines.iter().map(|line| line.muted).collect();
        assert_eq!(muted, [true, true, false]);
        chat.receive(
            ServerEventKind::Chat,
            "^1Dark^7Lord^7: four".to_owned(),
            Some(3),
            now,
        );
        assert!(chat.lines[3].muted, "new lines too");
        chat.set_muted_slots(0);
        assert!(chat.lines.iter().all(|line| !line.muted));
    }
}

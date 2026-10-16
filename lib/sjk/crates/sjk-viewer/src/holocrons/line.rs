//! A holocron drop as an SJK chat line (`docs/holocrons.md`, "In the chat"): the game's
//! chat, the docked chat and the chat page show it where it was said, in the tier's
//! colour with a small gem before it. Other players' legendary and mythical drops read
//! `Sol found a Legendary Holocron!`; the player's own read `You found an Uncommon
//! Holocron.` (with a `!` for the two grand tiers). The identity service relays a drop
//! as a chat message with no text and [`sjk_identity::ChatMessage::holocron`] set; this
//! words it from the tier the client knows, and leaves out a tier it does not.

use super::Tier;
use sjk_identity::ChatMessage;

/// The words of one drop line.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Words {
    /// The whole sentence, in the tier's colour and without colour codes. The line has
    /// no separate name: the sender is in the sentence.
    pub(crate) text: String,
    /// The sentence without its subject (`found a Legendary Holocron!`), for the chat
    /// page, which sets the sender's name in a column of its own.
    pub(crate) rest: String,
    pub(crate) tier: &'static Tier,
}

/// The words of `message` if it is a drop of a tier the client knows.
pub(crate) fn words(message: &ChatMessage) -> Option<Words> {
    let mark = message.holocron.as_ref()?;
    let tier = Tier::from_id(&mark.tier)?;
    let end = if tier.is_announced() { '!' } else { '.' };
    let rest = format!("found {} {}{end}", tier.article(), tier.name);
    let text = if mark.own {
        format!("You {rest}")
    } else {
        let who =
            crate::text::Plain(&sjk_identity::chat::name_for_display(&message.name)).to_string();
        let who = who.trim();
        let who = if who.is_empty() { "Someone" } else { who };
        format!("{who} {rest}")
    };
    Some(Words { text, rest, tier })
}

/// Whether `message` is a drop the client cannot show (a tier it does not know): the
/// chat leaves it out.
pub(crate) fn unknown(message: &ChatMessage) -> bool {
    message.holocron.is_some() && words(message).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::DropMark;

    fn drop(name: &str, tier: &str, own: bool) -> ChatMessage {
        ChatMessage {
            id: 5,
            at: 0,
            key_id: "aa".to_owned(),
            name: name.to_owned(),
            verified: true,
            staff: false,
            text: String::new(),
            holocron: Some(DropMark {
                tier: tier.to_owned(),
                own,
            }),
        }
    }

    fn said(message: &ChatMessage) -> String {
        words(message).expect("a drop").text
    }

    #[test]
    fn other_players_grand_drops_and_the_players_own_are_worded_as_decided() {
        assert_eq!(
            said(&drop("^2Sol", "legendary", false)),
            "Sol found a Legendary Holocron!"
        );
        assert_eq!(
            said(&drop("Fox", "mythical", false)),
            "Fox found a Mythical Holocron!"
        );
        assert_eq!(
            said(&drop("^2Sol", "uncommon", true)),
            "You found an Uncommon Holocron."
        );
        assert_eq!(
            said(&drop("^2Sol", "rare", true)),
            "You found a Rare Holocron."
        );
        assert_eq!(
            said(&drop("^2Sol", "legendary", true)),
            "You found a Legendary Holocron!"
        );
    }

    #[test]
    fn the_page_gets_the_sentence_without_its_subject() {
        for own in [false, true] {
            let words = words(&drop("^2Sol", "legendary", own)).unwrap();
            assert_eq!(words.rest, "found a Legendary Holocron!");
        }
        assert_eq!(
            words(&drop("Sol", "uncommon", false)).unwrap().rest,
            "found an Uncommon Holocron."
        );
    }

    #[test]
    fn the_sender_is_plain_text_and_never_nothing() {
        assert_eq!(
            said(&drop("^1S^2o^3l \u{221E}", "legendary", false)),
            "Sol found a Legendary Holocron!"
        );
        assert_eq!(
            said(&drop("", "mythical", false)),
            "Someone found a Mythical Holocron!"
        );
        assert_eq!(
            said(&drop("^1^2", "mythical", false)),
            "Someone found a Mythical Holocron!"
        );
    }

    #[test]
    fn the_tier_colours_the_line() {
        let words = words(&drop("Sol", "rare", false)).unwrap();
        assert_eq!(words.tier.id, "rare");
        assert_eq!(words.tier.colour, crate::holocrons::TIERS[1].colour);
    }

    #[test]
    fn an_unknown_tier_or_a_plain_message_is_no_drop_line() {
        let future = drop("Sol", "from_the_future", false);
        assert!(words(&future).is_none() && unknown(&future));
        let mut plain = drop("Sol", "rare", false);
        plain.holocron = None;
        assert!(words(&plain).is_none() && !unknown(&plain));
    }
}

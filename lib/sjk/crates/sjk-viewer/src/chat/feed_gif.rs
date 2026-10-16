//! GIFs in the in-play chat box (`docs/hub-chat.md`, "GIFs from GIPHY"): an SJK chat
//! line whose message links to a GIPHY GIF reads "GIF" in the link's place and shows the
//! GIF under its text, small ([`gif_draw::FEED`], three rows high), with the line's alpha
//! and place, so it slides in, scrolls and fades out with it. The cache, fetching and
//! decoding are [`crate::chat_gifs`]'s, as on the SJK chat page and the docks.
//!
//! Only SJK chat lines: a game server's chat lines never have a GIF. A muted sender's
//! GIF is never asked for (the line shows "Messages hidden on this client" and only
//! [`crate::chat_gifs::show`] fetches), and with `cl_sjkChatGifs 0` the lines show their
//! links as text: each line keeps the text it shows the other way, so the switch takes
//! effect on the lines already in the box.

use super::*;
use crate::chat_gifs::{self, GifId, Shown, draw as gif_draw};

/// How the chat box shows a GIF.
pub(super) const LOOK: gif_draw::Look = gif_draw::FEED;

/// An SJK line's GIF and its text the other way (link as text while GIFs show, "GIF"
/// while they do not).
pub(super) struct LineGif {
    pub(super) id: GifId,
    other: String,
    other_emojis: Vec<u16>,
}

impl ChatOverlay {
    /// An SJK message's `text` as its line shows it, the emojis its marks stand for and
    /// its GIF: the first GIPHY link worded "GIF" while GIFs show, else as typed.
    pub(super) fn with_gif(&self, text: String) -> (String, Vec<u16>, Option<Box<LineGif>>) {
        let markup = |text: String| {
            if self.options.emojis {
                self.emojis.markup(&text)
            } else {
                (text, Vec::new())
            }
        };
        let (labelled, id) = chat_gifs::link::with_label(&text);
        let Some(id) = id else {
            let (body, emojis) = markup(text);
            return (body, emojis, None);
        };
        let (shown, other) = if self.gifs_shown {
            (labelled, text)
        } else {
            (text, labelled)
        };
        let (body, emojis) = markup(shown);
        let (other, other_emojis) = markup(other);
        let gif = LineGif {
            id,
            other,
            other_emojis,
        };
        (body, emojis, Some(Box::new(gif)))
    }

    /// Follow `cl_sjkChatGifs` ([`options::Options::gifs`]): the lines with a GIF take
    /// the text of the other way when it changed, and are wrapped again.
    pub(super) fn follow_gif_switch(&mut self) {
        if self.gifs_shown == self.options.gifs {
            return;
        }
        self.gifs_shown = self.options.gifs;
        for line in &mut self.lines {
            if let Some(gif) = &mut line.gif {
                std::mem::swap(&mut line.body, &mut gif.other);
                std::mem::swap(&mut line.emojis, &mut gif.other_emojis);
                line.wrap = layout::Wrapped::default();
            }
        }
    }

    /// What the feed shows under the line at `index` this frame: its GIF while GIFs show
    /// and its sender is not muted, else nothing. Asks for the GIF the first time.
    pub(super) fn gif_shown(&self, index: usize) -> Option<Shown> {
        let line = &self.lines[index];
        let gif = line
            .gif
            .as_ref()
            .filter(|_| self.gifs_shown && !line.muted)?;
        Some(chat_gifs::show(&gif.id))
    }
}

/// The room (pixels) a GIF block takes under its line in geometry `g`.
pub(super) fn room(g: &layout::Geometry, shown: Option<Shown>) -> f32 {
    shown.map_or(0.0, |shown| gif_draw::room(LOOK, shown) * g.scale)
}

#[cfg(test)]
mod tests {
    use crate::chat::ChatOverlay;
    use crate::chat_gifs::{self, GifId};
    use sjk_client::ServerEventKind;
    use sjk_identity::{ChatMessage, ChatState};
    use sjk_ui::{Color, DrawCommand, Rect};
    use std::time::Instant;

    fn message(id: u64, name: &str, text: &str) -> ChatMessage {
        ChatMessage {
            id,
            at: 0,
            key_id: format!("{id:016x}"),
            name: name.to_owned(),
            verified: false,
            staff: false,
            text: text.to_owned(),
            holocron: None,
        }
    }

    /// A chat box that took `messages` from the hub, as they arrive in play.
    fn chat_with(messages: Vec<ChatMessage>, muted: &str, now: Instant) -> ChatOverlay {
        let mut chat = ChatOverlay::new();
        let state = |messages| ChatState {
            messages,
            loaded: Some(1),
            ..ChatState::default()
        };
        chat.sync_sjk(&state(Default::default()), |_| false, now);
        chat.sync_sjk(
            &state(messages.into_iter().collect()),
            |key| key == muted,
            now,
        );
        chat
    }

    /// Every text drawn and its rectangle and colour.
    fn texts(chat: &ChatOverlay) -> Vec<(String, Rect, Color)> {
        chat.ui
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    rect, text, color, ..
                } => Some((chat.ui.stored_text(*text).to_owned(), *rect, *color)),
                _ => None,
            })
            .collect()
    }

    fn find(texts: &[(String, Rect, Color)], wanted: &str) -> Option<(Rect, Color)> {
        texts
            .iter()
            .find(|(text, ..)| text == wanted)
            .map(|(_, rect, color)| (*rect, *color))
    }

    /// The loading GIF's box (the only rounded box the feed draws).
    fn placeholder(chat: &ChatOverlay) -> Option<(Rect, Color)> {
        chat.ui
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::RoundedRect { rect, color, .. } => Some((*rect, *color)),
                _ => None,
            })
    }

    /// At 1080p and 4K, a GIF line shows "GIF" and its block under the text, three rows
    /// high, inside the chat box's column; the line after it starts below the block.
    #[test]
    fn a_gif_takes_the_room_of_three_rows_under_its_line() {
        for (viewport, size) in [([1920.0, 1080.0], 1.0), ([3840.0, 2160.0], 1.5)] {
            let now = Instant::now();
            let mut chat = chat_with(
                vec![
                    message(1, "Sol", "look https://giphy.com/gifs/wave-FeedGif01Room"),
                    message(2, "Fox", "after"),
                ],
                "",
                now,
            );
            chat.options.font = size;
            chat.build(true, &crate::text::test_font(), viewport, 1_000);
            let g = chat.options.geometry(viewport);
            let texts = texts(&chat);
            let (label, _) = find(&texts, "look GIF").expect("the link reads GIF");
            assert!(texts.iter().all(|(text, ..)| !text.contains("giphy")));
            let (block, _) = placeholder(&chat).expect("the GIF's box");
            let (after, _) = find(&texts, "after").expect("the next line");
            assert!(block.y >= label.y + g.row - 0.5, "{block:?} {label:?}");
            assert!((block.height - 3.0 * g.row).abs() < 0.5, "{block:?}");
            assert!(block.x >= g.left - 0.5 && block.right() <= g.left + g.width + 0.5);
            assert!(after.y >= block.bottom() - 0.5, "{after:?} {block:?}");
            assert!(find(&texts, "Loading GIF").is_some());
            assert!(chat_gifs::asked_for(&GifId::new("FeedGif01Room").unwrap()));
        }
    }

    /// The GIF fades with its line and goes when it does; a new line moves both up.
    #[test]
    fn a_gif_lives_and_fades_with_its_line() {
        let now = Instant::now();
        let mut chat = chat_with(
            vec![message(
                1,
                "Sol",
                "https://media.giphy.com/media/FeedGif02Fade/giphy.gif",
            )],
            "",
            now,
        );
        let font = crate::text::test_font();
        let viewport = [1920.0, 1080.0];
        let received = chat.lines[0].received_ms;
        // Half way through the last two seconds of its ten.
        chat.build(true, &font, viewport, received + 9_000);
        let texts = texts(&chat);
        let (_, line) = find(&texts, "GIF").expect("the line");
        let (_, block) = placeholder(&chat).expect("the GIF's box");
        assert!(line.a > 0.3 && line.a < 0.7, "{line:?}");
        assert!((block.a - 0.07 * line.a).abs() < 1e-4, "{block:?} {line:?}");
        let (_, words) = find(&texts, "Loading GIF").expect("its words");
        assert!((words.a - line.a).abs() < 1e-4, "{words:?} {line:?}");
        // Gone with the line.
        chat.build(true, &font, viewport, received + 10_500);
        assert!(placeholder(&chat).is_none());
        assert!(find(&self::texts(&chat), "GIF").is_none());
        // A newer line pushes the line and its GIF up together.
        let mut chat = chat_with(
            vec![message(1, "Sol", "https://giphy.com/gifs/FeedGif03Move")],
            "",
            now,
        );
        chat.build(true, &font, viewport, received + 1_000);
        let (before, _) = placeholder(&chat).unwrap();
        let (line_before, _) = find(&self::texts(&chat), "GIF").unwrap();
        chat.receive(ServerEventKind::Chat, "a game line".to_owned(), None, now);
        // The move begins with the frame that sees the new line; long after, it has
        // settled.
        chat.build(true, &font, viewport, received + 1_000);
        chat.build(true, &font, viewport, received + 3_000);
        let (after, _) = placeholder(&chat).unwrap();
        let (line_after, _) = find(&self::texts(&chat), "GIF").unwrap();
        assert!(after.y < before.y - 1.0, "{before:?} {after:?}");
        let (game, _) = find(&self::texts(&chat), "a game line").unwrap();
        assert!(game.y >= after.bottom() - 0.5, "{game:?} {after:?}");
        assert!(((before.y - line_before.y) - (after.y - line_after.y)).abs() < 0.01);
    }

    /// A muted sender's GIF is never asked for; GIFs off keep the link as text and ask
    /// for nothing, and the lines follow the switch both ways.
    #[test]
    fn muted_senders_and_gifs_off_show_no_gif() {
        let now = Instant::now();
        let muted = message(1, "Troll", "https://giphy.com/gifs/FeedGif04Muted");
        let key = muted.key_id.clone();
        let mut chat = chat_with(vec![muted], &key, now);
        chat.open_sjk();
        chat.build(true, &crate::text::test_font(), [1920.0, 1080.0], 1_000);
        assert!(!chat_gifs::asked_for(
            &GifId::new("FeedGif04Muted").unwrap()
        ));
        assert!(placeholder(&chat).is_none());

        let link = "see https://giphy.com/gifs/FeedGif05Off";
        let mut chat = ChatOverlay::new();
        chat.options.gifs = false;
        let state = |messages| ChatState {
            messages,
            loaded: Some(1),
            ..ChatState::default()
        };
        chat.sync_sjk(&state(Default::default()), |_| false, now);
        chat.sync_sjk(
            &state([message(1, "Sol", link)].into_iter().collect()),
            |_| false,
            now,
        );
        let font = crate::text::test_font();
        chat.build(true, &font, [1920.0, 1080.0], 1_000);
        let words: String = texts(&chat)
            .iter()
            .map(|(text, ..)| text.as_str())
            .collect();
        assert!(words.contains("giphy.com/gifs/FeedGif05Off"), "{words}");
        assert!(placeholder(&chat).is_none());
        assert!(!chat_gifs::asked_for(&GifId::new("FeedGif05Off").unwrap()));
        // Switched on: the same line shows "GIF" and its GIF.
        chat.options.gifs = true;
        chat.build(true, &font, [1920.0, 1080.0], 1_000);
        assert!(find(&texts(&chat), "see GIF").is_some());
        assert!(placeholder(&chat).is_some());
        // And off again: the link is back.
        chat.options.gifs = false;
        chat.build(true, &font, [1920.0, 1080.0], 1_000);
        let words: String = texts(&chat)
            .iter()
            .map(|(text, ..)| text.as_str())
            .collect();
        assert!(words.contains("giphy.com"), "{words}");
        assert!(placeholder(&chat).is_none());
    }

    /// A game server's chat line keeps its GIPHY link as text and fetches nothing.
    #[test]
    fn a_game_line_keeps_its_link() {
        let mut chat = ChatOverlay::new();
        chat.receive(
            ServerEventKind::Chat,
            "^7Fox^7: ^2https://giphy.com/gifs/FeedGif06Server".to_owned(),
            None,
            Instant::now(),
        );
        chat.build(true, &crate::text::test_font(), [1920.0, 1080.0], 1_000);
        assert!(placeholder(&chat).is_none());
        assert!(!chat_gifs::asked_for(
            &GifId::new("FeedGif06Server").unwrap()
        ));
    }

    /// With the composer open, the history (lines long faded) shows its GIFs too.
    #[test]
    fn the_history_shows_gifs_while_typing() {
        let now = Instant::now();
        let mut chat = chat_with(
            vec![message(1, "Sol", "https://giphy.com/gifs/FeedGif07Typing")],
            "",
            now,
        );
        let font = crate::text::test_font();
        let received = chat.lines[0].received_ms;
        chat.build(true, &font, [1920.0, 1080.0], received + 60_000);
        assert!(placeholder(&chat).is_none(), "faded in play");
        chat.open_sjk();
        chat.build(true, &font, [1920.0, 1080.0], received + 60_000);
        let (block, colour) = placeholder(&chat).expect("in the history");
        assert!((colour.a - 0.07).abs() < 1e-4);
        assert!(block.height > 0.0);
    }

    /// A GIF line too tall for the box with its GIF shows its text without it.
    #[test]
    fn a_line_too_tall_with_its_gif_shows_its_text() {
        let now = Instant::now();
        let mut chat = chat_with(
            vec![message(1, "Sol", "hi https://giphy.com/gifs/FeedGif08Tall")],
            "",
            now,
        );
        // The box's bottom 40 pixels from the top: room for the row, not the GIF.
        chat.options.height = 40.0 / 1080.0 * 480.0;
        chat.build(true, &crate::text::test_font(), [1920.0, 1080.0], 1_000);
        assert!(find(&texts(&chat), "hi GIF").is_some());
        assert!(placeholder(&chat).is_none());
    }
}

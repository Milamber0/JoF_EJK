//! An SJK chat line in the feed (`docs/hub-chat.md`): one flowing line, as a game
//! chat line is, never a name row with the message under it. Its first row starts
//! with SJK's emblem, small (the logo, in its own colours, Sol's request 11/10/2026;
//! it was the letters SJK before), the JoF emblem for a sender with the clan's tag
//! ([`crate::jof_tag`]), the sender's name in its own colour codes and, for a
//! verified sender, the verified tick alone; the message follows on the same row in
//! the SJK chat's gold ([`crate::sjk_chat_look::GOLD`]) and wraps only when it is too
//! long ([`layout::Wrapped::update_indented`]).

use super::*;
use crate::holocrons::Tier;
use crate::sjk_chat_look;

/// The SJK emblem's side before an SJK line's name, against the text size: the
/// starburst's spikes reach the box's edges, its medallion is about half of it.
const LOGO_SIDE: f32 = 1.35;

/// What an SJK line's first row holds before its message, measured in the feed's
/// font. Nothing is stored: it is measured where it is drawn.
pub(in crate::chat) struct Prefix {
    /// The body's text size, which the name and the colon share.
    size: f32,
    /// The SJK emblem's side.
    logo: f32,
    /// Where the shown name ends in the line's name (cut to half the feed).
    pub(in crate::chat) name_end: usize,
    name_width: f32,
    verified: bool,
    /// The name carries the JoF tag: the clan's emblem comes before it.
    jof: Option<crate::jof_tag::Clan>,
    colon_width: f32,
    /// The line is a holocron drop: its mark is the tier's gem, not the verified tick.
    tier: Option<&'static Tier>,
}

impl Prefix {
    pub(in crate::chat) fn new(
        name: &str,
        verified: bool,
        tier: Option<&'static Tier>,
        font: &UiFont,
        g: &Geometry,
    ) -> Self {
        let size = g.font;
        let scale = size / font.height.max(1.0);
        let name_end = layout::fitting_end(name, font, g.width * 0.5, size);
        Self {
            size,
            logo: size * LOGO_SIDE,
            name_end,
            name_width: visible_text_width_face(font, &name[..name_end], scale, TextFace::Semibold),
            verified: verified || tier.is_some(),
            tier,
            jof: crate::jof_tag::clan(name),
            colon_width: if name.is_empty() {
                0.0
            } else {
                visible_text_width_face(font, ": ", scale, TextFace::Regular)
            },
        }
    }

    /// Where the JoF emblem's ink starts after the SJK emblem.
    fn jof_x(&self) -> f32 {
        self.logo + self.size * 0.2
    }

    /// Where the name starts after the SJK and JoF emblems.
    fn name_x(&self) -> f32 {
        self.jof_x()
            + if let Some(clan) = self.jof {
                crate::jof_tag::room(clan, crate::jof_tag::side(self.size))
            } else {
                0.0
            }
    }

    /// Where the colon starts after the name and its tick.
    fn colon_x(&self) -> f32 {
        self.name_x()
            + self.name_width
            + if self.verified {
                sjk_chat_look::tick_room(self.size)
            } else {
                0.0
            }
    }

    /// The room before the message on the first row.
    pub(in crate::chat) fn width(&self) -> f32 {
        self.colon_x() + self.colon_width
    }
}

/// The parts of one SJK line to draw.
pub(in crate::chat) struct Line<'a> {
    pub(in crate::chat) name: &'a str,
    pub(in crate::chat) body: &'a str,
    pub(in crate::chat) muted: bool,
    pub(in crate::chat) wrap: &'a layout::Wrapped,
    pub(in crate::chat) marks: &'a [u16],
    /// The emoji pictures, while `cg_chatBoxEmojis` is on.
    pub(in crate::chat) emojis: Option<&'a emoji::Emojis>,
    /// The pointer rests on the name (its profile card shows): it is lit.
    pub(in crate::chat) hovered: bool,
    /// The line is a holocron drop of this tier.
    pub(in crate::chat) tier: Option<&'static Tier>,
}

/// Draw `line` with its first row's top-left at `origin`, at `alpha`; returns the
/// shown name's rectangle.
pub(in crate::chat) fn draw(
    ui: &mut MenuCanvas,
    line: Line<'_>,
    prefix: &Prefix,
    font: &UiFont,
    g: &Geometry,
    origin: [f32; 2],
    alpha: f32,
) -> Rect {
    let [x, y] = origin;
    let size = prefix.size;
    let middle = y + font.capital_middle(size / font.height.max(1.0));
    let _ = ui.draw_list_mut().push(DrawCommand::TexturedQuad {
        rect: Rect::new(x, middle - prefix.logo * 0.5, prefix.logo, prefix.logo),
        texture: crate::ui_renderer::LOGO_TEXTURE,
        color: Color::new(1.0, 1.0, 1.0, alpha),
    });
    if let Some(clan) = prefix.jof {
        crate::jof_tag::draw(
            clan,
            x + prefix.jof_x(),
            middle,
            crate::jof_tag::side(size),
            alpha,
            |command| {
                let _ = ui.draw_list_mut().push(command);
            },
        );
    }
    let ink = Color::new(0.982, 0.987, 0.996, alpha);
    let name = &line.name[..prefix.name_end];
    let name_x = x + prefix.name_x();
    let name_rect = name::ink_bounds(font, name, [name_x, y], size);
    if line.hovered {
        // As a game line's name under the pointer (`view.rs`).
        ui.accent_bar(name_rect, Color::new(0.70, 0.88, 0.98, 0.16));
    }
    if !name.is_empty() {
        ui.text(
            name,
            Rect::new(name_x, y, prefix.name_width + 1.0, row_box(g)),
            size,
            ink,
            FontWeight::Semibold,
            0.0,
        );
    }
    if let Some(tier) = prefix.tier {
        sjk_chat_look::gem_mark(
            ui.draw_list_mut(),
            name_x + prefix.name_width,
            middle,
            size,
            tier,
            alpha,
        );
    } else if prefix.verified {
        sjk_chat_look::tick(
            ui.draw_list_mut(),
            name_x + prefix.name_width,
            middle,
            size,
            alpha,
        );
    }
    if prefix.colon_width > 0.0 {
        ui.text(
            ":",
            Rect::new(x + prefix.colon_x(), y, prefix.colon_width, row_box(g)),
            size,
            ink,
            FontWeight::Regular,
            0.0,
        );
    }
    let indent = prefix.width();
    if line.muted {
        ui.text(
            "Messages hidden on this client",
            Rect::new(x + indent, y, (g.width - indent).max(1.0), row_box(g)),
            15.0 * g.scale,
            Color::new(0.821, 0.854, 0.886, alpha * 0.853),
            FontWeight::Regular,
            0.0,
        );
        return name_rect;
    }
    let colour = line.tier.map_or_else(
        || sjk_chat_look::gold(alpha),
        |tier| tier.colour_alpha(alpha),
    );
    for (row, range) in line.wrap.rows[..line.wrap.len].iter().enumerate() {
        if range.is_empty() {
            continue;
        }
        let left = if row == 0 { x + indent } else { x };
        let truncated = row + 1 == line.wrap.len && range.end < line.body.len();
        body_row(
            ui,
            BodyRow {
                text: &line.body[range.clone()],
                carry: line.wrap.carry[row],
                marks: line.marks,
                emojis: line.emojis,
                truncated,
                spacing: g.spacing,
            },
            font,
            Rect::new(
                left,
                y + row as f32 * g.row,
                (x + g.width - left).max(1.0),
                row_box(g),
            ),
            g.font,
            colour,
        );
    }
    name_rect
}

#[cfg(test)]
mod tests {
    use crate::chat::ChatOverlay;
    use crate::sjk_chat_look::GOLD;
    use sjk_client::ServerEventKind;
    use sjk_identity::{ChatMessage, ChatState};
    use sjk_ui::{Color, DrawCommand, Rect};
    use std::time::Instant;

    const VIEWPORT: [f32; 2] = [1920.0, 1080.0];

    fn message(id: u64, name: &str, text: &str, verified: bool) -> ChatMessage {
        ChatMessage {
            id,
            at: 0,
            key_id: format!("{id:016x}"),
            name: name.to_owned(),
            verified,
            staff: false,
            text: text.to_owned(),
            holocron: None,
        }
    }

    /// A feed holding `messages` from the hub after `server` lines from the game.
    fn feed(server: &[&str], messages: Vec<ChatMessage>) -> ChatOverlay {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        let state = |messages| ChatState {
            messages,
            loaded: Some(1),
            ..ChatState::default()
        };
        chat.sync_sjk(&state(Default::default()), |_| false, now);
        for line in server {
            chat.receive(ServerEventKind::Chat, (*line).to_owned(), None, now);
        }
        chat.sync_sjk(&state(messages.into_iter().collect()), |_| false, now);
        chat.build(true, &crate::text::test_font(), VIEWPORT, 1_000);
        chat
    }

    /// Every text drawn, with its rectangle and colour.
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

    fn drawn<'a>(texts: &'a [(String, Rect, Color)], text: &str) -> &'a (String, Rect, Color) {
        texts
            .iter()
            .find(|(drawn, ..)| drawn == text)
            .unwrap_or_else(|| panic!("{text:?} not in {texts:?}"))
    }

    fn ticks(chat: &ChatOverlay) -> Vec<Rect> {
        quads(chat, crate::ui_renderer::VERIFIED_TEXTURE)
    }

    /// The SJK emblems before the lines.
    fn logos(chat: &ChatOverlay) -> Vec<Rect> {
        quads(chat, crate::ui_renderer::LOGO_TEXTURE)
    }

    fn quads(chat: &ChatOverlay, wanted: sjk_ui::TextureId) -> Vec<Rect> {
        chat.ui
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::TexturedQuad { rect, texture, .. } if *texture == wanted => {
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    }

    fn rgb(color: Color) -> [f32; 3] {
        [color.r, color.g, color.b]
    }

    #[test]
    fn only_sjk_messages_are_gold() {
        let chat = feed(
            &["^7Fox^7: ^2a game line", "plain game words"],
            vec![message(1, "^2Sol", "hello ^1there", false)],
        );
        let texts = texts(&chat);
        let (_, _, colour) = drawn(&texts, "hello there");
        assert_eq!(rgb(*colour), rgb(GOLD));
        for line in ["^7Fox^7: ^2a game line", "plain game words"] {
            let (_, _, colour) = drawn(&texts, line);
            assert_ne!(rgb(*colour), rgb(GOLD), "{line}");
        }
        // The name keeps its own colour codes, over the game's white, not gold.
        let (_, _, colour) = drawn(&texts, "^2Sol");
        assert_ne!(rgb(*colour), rgb(GOLD));
    }

    #[test]
    fn a_short_sjk_line_is_one_row_with_its_name() {
        let chat = feed(&[], vec![message(1, "^2Sol", "gg all", true)]);
        assert_eq!(chat.lines[0].wrap.len, 1);
        let texts = texts(&chat);
        let tag = logos(&chat)[0];
        let (_, name, _) = drawn(&texts, "^2Sol");
        let (_, colon, _) = drawn(&texts, ":");
        let (_, body, _) = drawn(&texts, "gg all");
        // Emblem, name, tick, colon and message, left to right on the name's row.
        assert_eq!(name.y, body.y, "the message stays on the name's row");
        assert!(tag.right() < name.x && name.right() <= colon.x && colon.x < body.x);
        assert!(!texts.iter().any(|(text, ..)| text == "SJK"), "no letters");
        let tick = ticks(&chat);
        assert_eq!(tick.len(), 1);
        assert!(tick[0].x >= name.x && tick[0].right() <= colon.x);
    }

    #[test]
    fn a_long_sjk_line_wraps_like_a_game_line() {
        // 131 characters, as long as the hub lets a message be (150) and more than
        // a row holds.
        let words = "the quick brown fox jumps over the lazy dog ".repeat(3);
        let text = words.trim_end();
        let chat = feed(
            &[&format!("Sol: {text}")],
            vec![message(1, "Sol", text, false)],
        );
        let game = &chat.lines[0].wrap;
        let sjk = &chat.lines[1].wrap;
        assert!(sjk.len >= 2, "it wraps");
        let last = sjk.rows[sjk.len - 1].end;
        assert_eq!(last, text.len(), "nothing cut");
        // The game line holds the name on its first row too: as many rows.
        assert_eq!(sjk.len, game.len);
        let texts = texts(&chat);
        let (_, name, _) = drawn(&texts, "Sol");
        // The game line sits above: look for the SJK line's rows from its name down.
        let row = |index: usize| {
            let wanted = &text[sjk.rows[index].clone()];
            texts
                .iter()
                .find(|(drawn, rect, _)| drawn == wanted && rect.y >= name.y)
                .unwrap_or_else(|| panic!("{wanted:?} not in {texts:?}"))
                .1
        };
        assert_eq!(row(0).y, name.y, "the first row is the name's");
        for index in 1..sjk.len {
            assert!(row(index).y > row(index - 1).y);
            assert!(row(index).x < name.x, "later rows start at the edge");
        }
    }

    fn drop(id: u64, name: &str, tier: &str, own: bool) -> ChatMessage {
        ChatMessage {
            name: name.to_owned(),
            verified: true,
            holocron: Some(sjk_identity::DropMark {
                tier: tier.to_owned(),
                own,
            }),
            ..message(id, name, "", true)
        }
    }

    /// A holocron drop is one sentence in its tier's colour (not the SJK chat's gold),
    /// with the tier's gem where the tick goes: no name column, no colon, no tick.
    #[test]
    fn a_holocron_drop_is_one_sentence_in_its_tiers_colour_with_a_gem() {
        use crate::holocrons::TIERS;
        let chat = feed(
            &[],
            vec![
                drop(1, "^2Sol", "legendary", false),
                drop(2, "^2Sol", "uncommon", true),
                drop(3, "^2Sol", "from_the_future", false),
                message(4, "Fox", "gg", false),
            ],
        );
        assert_eq!(chat.lines.len(), 3, "the unknown tier is left out");
        let texts = texts(&chat);
        let (_, grand, colour) = drawn(&texts, "Sol found a Legendary Holocron!");
        assert_eq!(rgb(*colour), rgb(TIERS[2].colour));
        let (_, own, colour) = drawn(&texts, "You found an Uncommon Holocron.");
        assert_eq!(rgb(*colour), rgb(TIERS[0].colour));
        assert!(own.y > grand.y, "in the order they came");
        assert_eq!(
            texts.iter().filter(|(text, ..)| text == ":").count(),
            1,
            "Fox's colon only: {texts:?}"
        );
        assert!(
            !texts.iter().any(|(text, ..)| text.is_empty()),
            "a drop has no empty name run"
        );
        assert!(ticks(&chat).is_empty(), "the gem stands for the tick");
        // Each drop draws a gem; the SJK tag is on every line.
        let bars = chat
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::SolidRect { .. }))
            .count();
        assert!(bars >= 2 * crate::holocrons::gem::MARK_ROWS, "{bars}");
        let tags = logos(&chat);
        assert_eq!(tags.len(), 3);
        // The sentence follows the emblem on its row.
        assert!(grand.x > tags[0].right());
    }

    /// A muted player's drops are not shown at all, not hidden behind a note.
    #[test]
    fn a_muted_players_drops_are_left_out() {
        let mut chat = ChatOverlay::new();
        let now = Instant::now();
        let state = |messages| ChatState {
            messages,
            loaded: Some(1),
            ..ChatState::default()
        };
        chat.sync_sjk(&state(Default::default()), |_| false, now);
        let troll = drop(2, "Troll", "mythical", false);
        let friend = drop(3, "Fox", "mythical", false);
        let key = troll.key_id.clone();
        chat.sync_sjk(
            &state([troll, friend].into_iter().collect()),
            |candidate| candidate == key,
            now,
        );
        let bodies: Vec<&str> = chat.lines.iter().map(|line| line.body.as_str()).collect();
        assert_eq!(bodies, ["Fox found a Mythical Holocron!"]);
    }

    #[test]
    fn a_verified_sender_has_the_tick_alone() {
        let chat = feed(
            &[],
            vec![
                message(1, "Sol", "first words", true),
                message(2, "Fox", "second words", false),
            ],
        );
        assert_eq!(ticks(&chat).len(), 1, "one tick, for the verified sender");
        let texts = texts(&chat);
        assert!(
            texts
                .iter()
                .all(|(text, ..)| !text.to_ascii_lowercase().contains("verified")),
            "{texts:?}"
        );
        assert_eq!(logos(&chat).len(), 2, "each line has its emblem");
    }
}

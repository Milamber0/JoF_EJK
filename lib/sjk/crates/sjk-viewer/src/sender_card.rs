//! The sender card a player's name shows when the pointer rests on it in a chat
//! (`docs/hub-chat.md`, "Muting a player"): in the game's chat while the composer is
//! open, on the main page's dock and on the SJK chat page. It shows what the SJK hub and
//! the server say of them, as the Players page's card does (their picture, or their
//! initial, as [`crate::profile_card::avatar`] draws it; their name and the verified
//! tick, whether the hub knows them, their key and hub name, their medals' medallions,
//! where they are on this server and how they were matched), and Mute or Unmute, which
//! hides their chat, draws them as Kyle with the default saber and silences them on this
//! PC only.
//!
//! The card is built when the name under the pointer changes and drawn every frame
//! without allocating; each place that shows it keeps it up while the pointer moves
//! from the name onto the card (its [`Tokens::card`] target).

use crate::menu::sjk::color;
use crate::menu_widgets::MenuCanvas;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// Where a player is on the server the player is on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Place {
    /// Not playing on a server, or not known.
    #[default]
    Unknown,
    /// On a server, but not matched to any of its slots.
    Away,
    /// In this slot, by the hub's claim on it under the name the game shows (or the
    /// server's own word that the line is theirs).
    Slot(u8),
    /// In this slot, matched by name alone (colour codes ignored).
    SlotByName(u8),
}

/// Who a card is about.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Person {
    /// The name as shown in the chat, colour codes kept.
    pub(crate) name: String,
    /// Their SJK key, when the hub knows them.
    pub(crate) key_id: Option<String>,
    /// Their hub name, when it is not the name shown.
    pub(crate) hub_name: Option<String>,
    pub(crate) verified: bool,
    pub(crate) staff: bool,
    pub(crate) medals: crate::medals::Medals,
    pub(crate) place: Place,
    /// Their picture's version, empty for none; `None` while not known (the place that
    /// shows the card asks, outside any lock: `player_identity::avatar_version`).
    pub(crate) avatar: Option<String>,
    /// A line on when they were about, from the place that shows the card ("Online
    /// now, in a match", "Seen 3 minutes ago"), or none.
    pub(crate) note: Option<String>,
}

impl Person {
    /// The card's status line: whether the hub knows them.
    fn status(&self) -> &'static str {
        match (self.key_id.is_some(), self.staff) {
            (true, true) => "SJK player, SJK staff",
            (true, false) => "SJK player",
            (false, _) => "Not known to the SJK hub",
        }
    }
    /// The key id the card prints: none for the player's own.
    fn printed_key(&self) -> Option<&str> {
        self.key_id
            .as_deref()
            .and_then(crate::player_identity::printable_key_id)
    }
}

/// A card to draw: who, whether they are muted on this PC (the button then
/// unmutes), and how its text measures.
#[derive(Clone, Copy)]
pub(crate) struct Card<'a> {
    pub(crate) person: &'a Person,
    pub(crate) muted: bool,
    /// How the card's text measures, to set the tick after the name; without it the
    /// name's width is estimated.
    pub(crate) measure: Option<&'a crate::sjk_chat_look::Measure<'a>>,
}

/// The card's two pointer targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Tokens {
    /// The card itself, which keeps it up under the pointer.
    pub(crate) card: u16,
    /// Mute or Unmute.
    pub(crate) mute: u16,
}

/// The card's width in 1080p pixels.
const WIDTH: f32 = 340.0;
const PAD: f32 = 16.0;
/// Medallions a card shows at most.
const MEDALS: usize = 6;
/// The picture's radius, beside the name and the status line (1080p pixels).
const PICTURE_RADIUS: f32 = 24.0;

/// The card's size for `person`, at the scale `u` (1 at 1080 lines).
pub(crate) fn size(person: &Person, u: f32) -> [f32; 2] {
    let mut height = PAD + 30.0 + 22.0;
    if person.hub_name.is_some() {
        height += 20.0;
    }
    if person.printed_key().is_some() {
        height += 20.0;
    }
    if person.place != Place::Unknown {
        height += 20.0;
    }
    if person.note.is_some() {
        height += 20.0;
    }
    if !person.medals.is_empty() {
        height += 32.0;
    }
    height += 12.0 + 38.0 + 22.0 + PAD;
    if person.key_id.is_none() {
        height += 18.0;
    }
    [WIDTH * u, height * u]
}

/// Where a card of `size` goes beside `anchor` (the name): to its right if it fits in
/// `viewport`, else to its left, level with its top, kept on screen.
pub(crate) fn beside(anchor: Rect, size: [f32; 2], viewport: [f32; 2], gap: f32) -> [f32; 2] {
    let right = anchor.right() + gap;
    let x = if right + size[0] <= viewport[0] {
        right
    } else {
        (anchor.x - gap - size[0]).max(0.0)
    };
    let y = anchor.y.min(viewport[1] - size[1]).max(0.0);
    [x, y]
}

/// Draw `person`'s card with its top-left at `origin`, at the scale `u`; `muted` says
/// whether they are muted on this PC (the button then unmutes). Returns the card's
/// rectangle. Its targets are added last, over whatever lies under it.
pub(crate) fn draw(
    canvas: &mut MenuCanvas,
    card: &Card<'_>,
    origin: [f32; 2],
    u: f32,
    tokens: Tokens,
) -> Rect {
    let Card {
        person,
        muted,
        measure,
    } = *card;
    let [width, height] = size(person, u);
    let card = Rect::new(origin[0], origin[1], width, height);
    let list = canvas.draw_list_mut();
    let _ = list.push(DrawCommand::RoundedRect {
        rect: card,
        radius: 10.0 * u,
        color: color::alpha(color::SPACE, 0.96),
    });
    let _ = list.push(DrawCommand::Border {
        rect: card,
        radius: 10.0 * u,
        width: 1.5 * u,
        color: color::alpha(color::HOLO, 0.45),
    });
    let x = card.x + PAD * u;
    let inner = width - 2.0 * PAD * u;
    let mut y = card.y + PAD * u;
    let line = |canvas: &mut MenuCanvas,
                y: f32,
                size: f32,
                colour: Color,
                value: std::fmt::Arguments<'_>| {
        canvas.text_fmt_aligned(
            value,
            Rect::new(x, y, inner, size * 1.35),
            size,
            colour,
            FontWeight::Regular,
            0.0,
            TextAlign::Start,
        );
    };
    // Their picture, or their initial until it is loaded or when they have none,
    // beside the name and the status line. The tick after the name says verified.
    let radius = PICTURE_RADIUS * u;
    crate::profile_card::avatar(
        canvas,
        [x + radius, y + 26.0 * u],
        radius,
        &crate::profile_card::Avatar {
            key_id: person.key_id.as_deref().unwrap_or(""),
            version: person.avatar.as_deref().unwrap_or(""),
            name: &person.name,
            verified: false,
            preview: false,
            lit: false,
        },
    );
    let head_x = x + radius * 2.0 + 12.0 * u;
    let head_width = (inner - (head_x - x)).max(1.0);
    // The name and, for a verified player, the tick alone.
    let tick_room = if person.verified {
        crate::sjk_chat_look::tick_room(20.0 * u)
    } else {
        0.0
    };
    canvas.text_fmt_aligned(
        format_args!("{}", person.name),
        Rect::new(head_x, y, head_width - tick_room, 28.0 * u),
        20.0 * u,
        color::TEXT,
        FontWeight::Semibold,
        0.0,
        TextAlign::Start,
    );
    if person.verified {
        let name = match measure {
            Some(measure) => measure.width(&person.name, 20.0 * u, crate::text::TextFace::Semibold),
            None => estimated_width(&person.name, 20.0 * u),
        };
        let end = head_x + (head_width - tick_room).min(name);
        crate::sjk_chat_look::tick(canvas.draw_list_mut(), end, y + 13.0 * u, 20.0 * u, 1.0);
    }
    y += 30.0 * u;
    canvas.text_fmt_aligned(
        format_args!("{}", person.status()),
        Rect::new(head_x, y, head_width, 15.0 * u * 1.35),
        15.0 * u,
        if person.key_id.is_some() {
            color::GOLD_BRIGHT
        } else {
            color::MUTED
        },
        FontWeight::Regular,
        0.0,
        TextAlign::Start,
    );
    y += 22.0 * u;
    if let Some(hub) = &person.hub_name {
        line(
            canvas,
            y,
            14.0 * u,
            color::MUTED,
            format_args!("Known as {hub}"),
        );
        y += 20.0 * u;
    }
    if let Some(key) = person.printed_key() {
        line(canvas, y, 14.0 * u, color::QUIET, format_args!("Key {key}"));
        y += 20.0 * u;
    }
    match person.place {
        Place::Unknown => {}
        Place::Away => {
            line(
                canvas,
                y,
                14.0 * u,
                color::QUIET,
                format_args!("Not on this server"),
            );
            y += 20.0 * u;
        }
        Place::Slot(slot) => {
            line(
                canvas,
                y,
                14.0 * u,
                color::MUTED,
                format_args!("On this server, slot {slot}"),
            );
            y += 20.0 * u;
        }
        Place::SlotByName(slot) => {
            line(
                canvas,
                y,
                14.0 * u,
                color::MUTED,
                format_args!("On this server, slot {slot}, matched by name"),
            );
            y += 20.0 * u;
        }
    }
    if let Some(note) = &person.note {
        line(canvas, y, 14.0 * u, color::MUTED, format_args!("{note}"));
        y += 20.0 * u;
    }
    if !person.medals.is_empty() {
        for (index, (medal, _)) in person.medals.iter().take(MEDALS).enumerate() {
            let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: Rect::new(x + index as f32 * 30.0 * u, y + 2.0 * u, 26.0 * u, 26.0 * u),
                texture: medal.icon(),
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
        y += 32.0 * u;
    }
    y += 12.0 * u;
    let button = Rect::new(x, y, 150.0 * u, 38.0 * u);
    let lit = canvas.token_hovered(tokens.mute);
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect: button,
        radius: 8.0 * u,
        color: if lit {
            color::alpha(color::GOLD, 0.3)
        } else {
            color::alpha(color::GOLD, 0.14)
        },
    });
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect: button,
        radius: 8.0 * u,
        width: 1.5 * u,
        color: color::GOLD,
    });
    canvas.text_fmt_aligned(
        format_args!("{}", if muted { "Unmute" } else { "Mute" }),
        Rect::new(button.x, button.y + 9.0 * u, button.width, 22.0 * u),
        17.0 * u,
        if lit { color::GOLD_BRIGHT } else { color::TEXT },
        FontWeight::Semibold,
        0.0,
        TextAlign::Center,
    );
    y += 38.0 * u + 4.0 * u;
    line(
        canvas,
        y,
        13.0 * u,
        color::QUIET,
        format_args!(
            "{}",
            if muted {
                "Muted on this PC: no chat, Kyle, no sounds"
            } else {
                "Hides their chat, model, sabers and sounds here"
            }
        ),
    );
    if person.key_id.is_none() {
        // Without a key, the mute can only follow the name.
        line(
            canvas,
            y + 18.0 * u,
            13.0 * u,
            color::QUIET,
            format_args!("By name, colours aside: anyone wearing it"),
        );
    }
    canvas.hit_region(tokens.card, card);
    canvas.hit_region(tokens.mute, button);
    card
}

/// About how wide `name` is at `size` without a font at hand: a little over half the
/// size a character, colour codes taking no room.
fn estimated_width(name: &str, size: f32) -> f32 {
    let codes = name
        .as_bytes()
        .windows(2)
        .filter(|pair| pair[0] == b'^' && pair[1].is_ascii_digit())
        .count();
    name.chars().count().saturating_sub(codes * 2) as f32 * size * 0.55
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{TextStyle, test_font};

    const TOKENS: Tokens = Tokens { card: 7, mute: 8 };

    fn sol() -> Person {
        Person {
            name: "^2Sol".to_owned(),
            key_id: Some("0123456789abcdef".to_owned()),
            hub_name: None,
            verified: true,
            staff: false,
            medals: crate::medals::Medals::default(),
            place: Place::SlotByName(5),
            avatar: Some(String::new()),
            note: None,
        }
    }

    /// The card drawn at (100, 100) on a 1080p canvas: its texts and whether it has
    /// the verified tick.
    fn drawn(person: &Person, muted: bool) -> (MenuCanvas, Rect, Vec<String>, usize) {
        let font = test_font();
        let measure = crate::sjk_chat_look::Measure::new(&font, TextStyle::NEUTRAL);
        let mut canvas = MenuCanvas::new();
        canvas.begin_transparent([1920.0, 1080.0]);
        let rect = draw(
            &mut canvas,
            &Card {
                person,
                muted,
                measure: Some(&measure),
            },
            [100.0, 100.0],
            1.0,
            TOKENS,
        );
        let texts = canvas.text_runs().map(str::to_owned).collect();
        let ticks = canvas
            .draw_list()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::TexturedQuad { texture, .. }
                    if *texture == crate::ui_renderer::VERIFIED_TEXTURE)
            })
            .count();
        (canvas, rect, texts, ticks)
    }

    #[test]
    fn the_card_says_who_they_are_and_offers_mute() {
        let (canvas, rect, texts, ticks) = drawn(&sol(), false);
        assert!(texts.contains(&"^2Sol".to_owned()), "{texts:?}");
        assert!(texts.contains(&"SJK player".to_owned()), "{texts:?}");
        assert!(
            texts.contains(&"Key 0123456789abcdef".to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"On this server, slot 5, matched by name".to_owned()),
            "{texts:?}"
        );
        assert!(texts.contains(&"Mute".to_owned()), "{texts:?}");
        assert_eq!(ticks, 1, "the tick alone says verified");
        assert!(
            !texts.iter().any(|text| text.contains("erified")),
            "{texts:?}"
        );
        // No picture (or not loaded): their initial in its disc, left of the name.
        let at = |wanted: &str| {
            canvas
                .draw_list()
                .commands()
                .iter()
                .find_map(|command| match command {
                    DrawCommand::Text { rect, text, .. } if canvas.stored_text(*text) == wanted => {
                        Some(*rect)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{wanted:?} in {texts:?}"))
        };
        let (initial, name) = (at("S"), at("^2Sol"));
        assert!(initial.right() <= name.x && initial.y <= name.y + name.height);
        // Both targets: the card, and the button inside it.
        assert_eq!(canvas.rect_for(TOKENS.card), Some(rect));
        let button = canvas.rect_for(TOKENS.mute).expect("the button");
        assert!(rect.contains(sjk_ui::Vec2::new(button.x + 1.0, button.y + 1.0)));
        assert!(button.bottom() <= rect.bottom());
    }

    /// A note from the place that shows the card (the chat page's window: when they
    /// were about) takes a line of its own, inside the card.
    #[test]
    fn a_note_gets_its_line_and_the_card_grows_for_it() {
        let plain = sol();
        let noted = Person {
            note: Some("Seen 3 minutes ago".to_owned()),
            ..sol()
        };
        let (_, rect, texts, _) = drawn(&noted, false);
        assert!(
            texts.contains(&"Seen 3 minutes ago".to_owned()),
            "{texts:?}"
        );
        assert_eq!(size(&noted, 1.0)[1], size(&plain, 1.0)[1] + 20.0);
        assert_eq!(rect.height, size(&noted, 1.0)[1]);
        let (_, _, texts, _) = drawn(&plain, false);
        assert!(!texts.iter().any(|text| text.starts_with("Seen")));
    }

    #[test]
    fn a_muted_player_is_offered_unmute_and_a_stranger_says_so() {
        let stranger = Person {
            name: "Fox".to_owned(),
            place: Place::Slot(3),
            ..Person::default()
        };
        let (_, _, texts, ticks) = drawn(&stranger, true);
        assert!(texts.contains(&"Unmute".to_owned()), "{texts:?}");
        assert!(texts.contains(&"F".to_owned()), "their initial: {texts:?}");
        assert!(
            texts.contains(&"Not known to the SJK hub".to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"On this server, slot 3".to_owned()),
            "{texts:?}"
        );
        assert!(
            texts.contains(&"By name, colours aside: anyone wearing it".to_owned()),
            "{texts:?}"
        );
        assert_eq!(ticks, 0);
    }

    #[test]
    fn the_card_goes_beside_the_name_and_stays_on_screen() {
        let viewport = [1920.0, 1080.0];
        let size = size(&sol(), 1.0);
        let left_name = Rect::new(100.0, 500.0, 80.0, 20.0);
        assert_eq!(beside(left_name, size, viewport, 10.0), [190.0, 500.0]);
        // At the right edge it goes to the name's left.
        let right_name = Rect::new(1800.0, 500.0, 80.0, 20.0);
        let [x, _] = beside(right_name, size, viewport, 10.0);
        assert!(x + size[0] <= 1790.0 + 0.01);
        // Low on the screen it is raised to fit.
        let low = Rect::new(100.0, 1070.0, 80.0, 20.0);
        let [_, y] = beside(low, size, viewport, 10.0);
        assert!(y + size[1] <= 1080.0 + 0.01);
    }
}

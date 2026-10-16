//! Clan emblems (`docs/identity.md`, "Clan tags"): a player whose name carries a known
//! clan's tag gets the clan's emblem on the left of their name in the chat, on the
//! scoreboard, on the player card and on the SJK UI's Players page. The tag is read from
//! the name alone, so it is what the player says, not something the hub vouches for.
//!
//! The clans are the entries of [`KNOWN`], read by generic code: supporting a clan means
//! adding an entry and its emblem, not code. [`CVAR`] 0 turns every emblem off.
//!
//! The rule, for every clan: the tag's letters in any case, with no letter right before
//! the first (the name starts there or a separator comes first: a space, a bracket, a
//! dot, a digit...). Colour codes are dropped first, so `^1J^7oF` counts and `MrJoF`
//! does not. Each letter may be written with a look-alike ([`looks_like`]: `jøf`, `J0F`,
//! `Jöƒ`). A letter right after the tag means it is stuck to the name, which counts as
//! the entry's [`Stuck`] rule says.

use sjk_ui::{Color, DrawCommand, Rect, TextureId};
use std::sync::atomic::{AtomicBool, Ordering};

/// 0 hides every clan emblem.
pub(crate) const CVAR: &str = "cg_clanEmblems";

/// When a tag stuck to the name (a letter right after it) still counts.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Stuck {
    /// In any case, unless the next letter is one of these (a name such as Joffrey).
    AnyCase { not_before: &'static [char] },
    /// Only with the tag's capitals written as capitals and a capitalised word right
    /// after it (`JoFAKen`), since in lower case the tag cannot be told from a shorter
    /// clan's followed by a name (`jofaken`).
    Capitals,
}

/// A clan whose tag earns its emblem.
#[derive(Debug)]
pub(crate) struct ClanTag {
    /// The clan's name.
    pub(crate) name: &'static str,
    /// The tag as the clan writes it: its letters are read in any case and its capitals
    /// are the ones [`Stuck::Capitals`] asks for.
    tag: &'static str,
    /// How a tag stuck to the name counts.
    stuck: Stuck,
    /// The emblem, a 128-pixel square PNG uploaded into the clan's atlas cell at start.
    pub(crate) emblem: &'static [u8],
    /// The emblem's tint (white: its own colours).
    tint: Color,
    /// Share of the emblem's square its ink spans across.
    ink_width: f32,
}

/// The clans known, longest tag first, so `JoFA` is read before `JoF`.
pub(crate) const KNOWN: &[ClanTag] = &[
    // JoF Academy (Sol's request, 11/10/2026): the academy's HD logo, shrunk by
    // `scripts/clan_emblem.py`. Players write `JoFA}Ken`, `^5JoFA^7<3Toad`, `JoFAKen`.
    ClanTag {
        name: "JoFA",
        tag: "JoFA",
        stuck: Stuck::Capitals,
        emblem: include_bytes!("../../../assets/branding/jofa-emblem.png"),
        tint: Color::new(1.0, 1.0, 1.0, 1.0),
        ink_width: 0.79,
    },
    // JoF, Jedi of Freedom: the clan's J, o, F, white on transparent, rendered from its
    // SVG path by `scripts/jof_emblem.py`, in the teal of the clan's logo (`#02B99D`,
    // Creyon's request, 11/10/2026; it was crimson). Players write `{JoF}Name`,
    // `jøf'p Evi`, `JoFSanjiJ` and `jofsanji` (Creyon, 11/10/2026).
    ClanTag {
        name: "JoF",
        tag: "JoF",
        stuck: Stuck::AnyCase { not_before: &['f'] },
        emblem: include_bytes!("../../../assets/branding/jof-emblem.png"),
        tint: Color::new(0.008, 0.725, 0.616, 1.0),
        ink_width: 0.65,
    },
];

/// A known clan: an entry of [`KNOWN`].
pub(crate) type Clan = &'static ClanTag;

impl ClanTag {
    /// The emblem's atlas cell.
    pub(crate) fn texture(&'static self) -> TextureId {
        let index = KNOWN
            .iter()
            .position(|clan| std::ptr::eq(clan, self))
            .unwrap_or(0);
        crate::ui_renderer::clan_texture(index)
    }

    /// Whether the tag stands at `at` in `chars`: its letters (or their look-alikes)
    /// with no letter before, and nothing stuck to it or stuck as [`Stuck`] allows.
    fn at(&self, chars: &[char], at: usize) -> bool {
        if at > 0 && chars[at - 1].is_alphabetic() {
            return false;
        }
        let tag: Vec<char> = self.tag.chars().collect();
        let after = at + tag.len();
        if after > chars.len()
            || !tag
                .iter()
                .zip(&chars[at..after])
                .all(|(letter, c)| looks_like(*c) == Some(letter.to_ascii_lowercase()))
        {
            return false;
        }
        let Some(&next) = chars.get(after).filter(|next| next.is_alphabetic()) else {
            return true;
        };
        match self.stuck {
            Stuck::AnyCase { not_before } => {
                !not_before.contains(&looks_like(next).unwrap_or(next))
            }
            Stuck::Capitals => {
                tag.iter()
                    .zip(&chars[at..after])
                    .all(|(letter, c)| !letter.is_uppercase() || c.is_uppercase())
                    && next.is_uppercase()
                    && chars.get(after + 1).is_some_and(|c| c.is_lowercase())
            }
        }
    }
}

static SHOWN: AtomicBool = AtomicBool::new(true);

/// Follow [`CVAR`] (once a frame).
pub(crate) fn set_shown(on: bool) {
    SHOWN.store(on, Ordering::Relaxed);
}

/// The gap after the emblem's ink, as a share of its side.
const GAP: f32 = 0.3;
/// The emblem's side beside a name in text `size` pixels tall.
pub(crate) fn side(size: f32) -> f32 {
    size * 0.95
}

/// The room `clan`'s emblem `side` tall takes before a name, the gap after it included.
pub(crate) fn room(clan: Clan, side: f32) -> f32 {
    side * (clan.ink_width + GAP)
}

/// The room before `name` set with an emblem `side` tall: its clan's emblem's, or 0.
pub(crate) fn room_for(name: &str, side: f32) -> f32 {
    clan(name).map_or(0.0, |clan| room(clan, side))
}

/// `clan`'s emblem `side` tall with its ink from `x`, centred on `middle`, at `alpha`.
pub(crate) fn draw(
    clan: Clan,
    x: f32,
    middle: f32,
    side: f32,
    alpha: f32,
    mut push: impl FnMut(DrawCommand),
) {
    push(DrawCommand::TexturedQuad {
        rect: Rect::new(
            x - side * (1.0 - clan.ink_width) * 0.5,
            middle - side * 0.5,
            side,
            side,
        ),
        texture: clan.texture(),
        color: Color {
            a: clan.tint.a * alpha,
            ..clan.tint
        },
    });
}

/// The clan `name` (with its colour codes) is tagged with, if any, while emblems show.
pub(crate) fn clan(name: &str) -> Option<Clan> {
    clan_if(SHOWN.load(Ordering::Relaxed), name)
}

/// The clan `name` is tagged with when emblems are `shown`, else none.
fn clan_if(shown: bool, name: &str) -> Option<Clan> {
    shown.then(|| clan_in(name)).flatten()
}

/// The clan `name` is tagged with, whatever [`CVAR`] says.
fn clan_in(name: &str) -> Option<Clan> {
    let plain = sjk_game_jka::client_view::strip_colours(name.as_bytes());
    let plain = String::from_utf8_lossy(&plain);
    let chars: Vec<char> = plain.chars().collect();
    (0..chars.len()).find_map(|at| KNOWN.iter().find(|clan| clan.at(&chars, at)))
}

/// The lower-case letter `c` stands for in a tag, if any: an ASCII letter in either
/// case, or a look-alike players write the tags' letters with, such as `ø`, `ö` or `0`
/// for the o, `ƒ` for the f or `å` for the a (Creyon's request, 11/10/2026: `jøf'p Evi`
/// is JoF's). A clan whose tag has other letters may need their look-alikes added.
fn looks_like(c: char) -> Option<char> {
    let letter = match c {
        'ĵ' | 'Ĵ' | 'ǰ' | 'ȷ' | 'ɉ' | 'Ɉ' | 'ʝ' | 'ϳ' | 'ј' | 'Ј' | 'ｊ' | 'Ｊ' => 'j',
        '0' | 'ø' | 'Ø' | 'ö' | 'Ö' | 'ó' | 'Ó' | 'ò' | 'Ò' | 'ô' | 'Ô' | 'õ' | 'Õ' | 'ō' | 'Ō'
        | 'ŏ' | 'Ŏ' | 'ő' | 'Ő' | 'ǒ' | 'Ǒ' | 'ǫ' | 'Ǫ' | 'ơ' | 'Ơ' | 'ɵ' | 'Ɵ' | 'ο' | 'Ο'
        | 'σ' | 'θ' | 'Θ' | 'о' | 'О' | '°' | 'º' | '○' | '◯' | '⊙' | 'ｏ' | 'Ｏ' => {
            'o'
        }
        'ƒ' | 'Ƒ' | 'ғ' | 'Ғ' | 'ꜰ' | 'ϝ' | 'Ϝ' | 'ｆ' | 'Ｆ' => 'f',
        'à' | 'À' | 'á' | 'Á' | 'â' | 'Â' | 'ã' | 'Ã' | 'ä' | 'Ä' | 'å' | 'Å' | 'ā' | 'Ā' | 'ă'
        | 'Ă' | 'ą' | 'Ą' | 'ǎ' | 'Ǎ' | 'ª' | 'α' | 'Α' | 'а' | 'А' | 'Λ' | 'Δ' | '∆' | 'ａ'
        | 'Ａ' => 'a',
        c if c.is_ascii_alphabetic() => c.to_ascii_lowercase(),
        _ => return None,
    };
    Some(letter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Option<&'static str> {
        clan_in(name).map(|clan| clan.name)
    }

    #[test]
    fn the_table_reads_longer_tags_first_and_fits_the_atlas() {
        for pair in KNOWN.windows(2) {
            assert!(pair[0].tag.len() >= pair[1].tag.len(), "{}", pair[0].name);
        }
        assert!(KNOWN.len() <= crate::ui_renderer::CLAN_ICON_CELLS);
        for clan in KNOWN {
            assert!(
                image::load_from_memory(clan.emblem).is_ok(),
                "{}",
                clan.name
            );
            assert!(
                clan.tag.chars().all(|c| looks_like(c).is_some()),
                "{}",
                clan.name
            );
        }
    }

    #[test]
    fn the_setting_hides_every_emblem() {
        assert_eq!(clan_if(false, "JoF|Sol").map(|clan| clan.name), None);
        assert_eq!(clan_if(true, "JoF|Sol").map(|clan| clan.name), Some("JoF"));
    }

    #[test]
    fn the_tag_counts_in_any_case_behind_any_separator() {
        for name in [
            "{JoF}^1emiah{I}",
            "JoF|Sol",
            "jof.Sol",
            "JOF-Sol",
            "[jof] Sol",
            "Sol JoF",
            "Sol-JoF",
            "^1J^7oF^3|Sol",
            "2jof3",
            "JoF",
            "*~JoF~*",
            "jøf'p Evi",
            "^7jØf|Evi",
            "J0F",
            "Jöƒ Ken",
            "{J°F}Ken",
            "JoFSanjiJ",
            "^1JoF^7Sanji",
            "JØFKen",
            "J0FEvi",
            "JoFAlex",
            "jofsanji",
            "Jofsanji",
            "^3jof^7evi",
            "JoFX",
            "jøfken",
            "jofaken",
        ] {
            assert_eq!(named(name), Some("JoF"), "{name}");
        }
    }

    #[test]
    fn the_academys_tag_is_jofa() {
        for name in [
            "JoFA}»«{Ken",
            "^5JoFA^7<3Toad Instructor",
            "Jofa",
            "[jofa] Ken",
            "Ken|JOFA",
            "jøfå}Ken",
            "JØFΛ Ken",
            "JoFAKen",
            "^5JoFA^7Ken",
        ] {
            assert_eq!(named(name), Some("JoFA"), "{name}");
        }
        for name in ["JoFAx", "JoFAlex", "MrJoFA", "JoF", "JoF|Sol"] {
            assert_ne!(named(name), Some("JoFA"), "{name}");
        }
        assert_eq!(named("JoF|Sol"), Some("JoF"));
        // The academy's emblem is wider.
        assert!(room(&KNOWN[0], 20.0) > room(&KNOWN[1], 20.0));
        assert_eq!(room_for("Padawan", 20.0), 0.0);
    }

    #[test]
    fn a_letter_on_either_side_is_not_the_tag() {
        // A letter before the tag, or a second f after it, means a name, not the tag.
        for name in [
            "Joffrey", "MrJoF", "ajof", "éJoF", "J oF", "Jøffrey", "MrJøF", "JOFFREY", "joffre", "",
        ] {
            assert_eq!(named(name), None, "{name}");
        }
    }
}

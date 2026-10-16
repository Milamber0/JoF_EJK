//! Medals: recognition the SJK team gives a player by hand (`docs/identity.md`,
//! "Medals"). Most bring one saber blade while held ([`Medal::blade`]); nothing else. The hub lists a player's medals in their profile and
//! in their presence entry ([`sjk_identity::Medal`]); this catalogue is the one place
//! that says which ids the client knows and how each looks: its name, its description,
//! whether it can be given again, its ribbon's colours and its two pictures. An id the
//! client does not know is left out.
//!
//! Adding a medal is one entry in [`Medal::ALL`] and the tables below, and its two
//! pictures in `assets/medals`: `<id>.png`, the whole medal hanging from its ribbon
//! (512 square), and `<id>_small.png`, the medallion alone (128 square, one atlas cell)
//! for anything under about 64 pixels, where the whole one cannot be read. Changing a
//! medal's art is replacing those files.

pub(crate) mod art;
pub(crate) mod seen;

use sjk_ui::{Color, TextureId};

/// First `TextureId` naming a medal's whole picture: clear of the atlas cells, the
/// classic art (`0x4000_0000` on), the menu emblem (`0x5000_0000` on) and the reserved
/// ids at the top.
const ART_TEXTURE_BASE: u32 = 0x6000_0000;
/// Longest note shown, in characters (the hub's own limit).
const NOTE_LIMIT: usize = 200;

/// A medal the client knows, in the hub's catalogue order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Medal {
    EarlyTester,
    EarlyContributor,
    BugHunter,
    Supporter,
}

/// One stripe across a ribbon: where it starts and how wide it is, as fractions of
/// the ribbon's width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stripe {
    pub(crate) at: f32,
    pub(crate) width: f32,
    pub(crate) color: Color,
}

/// A ribbon bar as the scoreboard draws it: a base colour and vertical stripes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ribbon {
    pub(crate) base: Color,
    pub(crate) stripes: &'static [Stripe],
}

/// `0xRRGGBB` as an opaque colour.
const fn rgb(hex: u32) -> Color {
    Color::new(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    )
}

const BLACK: Color = rgb(0x1A_1A1A);
const fn stripe(at: f32, width: f32, color: Color) -> Stripe {
    Stripe { at, width, color }
}

const EARLY_TESTER: Ribbon = Ribbon {
    base: rgb(0xE8_A13A),
    stripes: &[
        stripe(0.22, 0.07, BLACK),
        stripe(0.465, 0.07, BLACK),
        stripe(0.71, 0.07, BLACK),
    ],
};
const EARLY_CONTRIBUTOR: Ribbon = Ribbon {
    base: rgb(0x1F_2E66),
    stripes: &[stripe(0.45, 0.1, rgb(0xE8_ECF2))],
};
const BUG_HUNTER: Ribbon = Ribbon {
    base: rgb(0x1E_9E6E),
    stripes: &[stripe(0.0, 0.13, BLACK), stripe(0.87, 0.13, BLACK)],
};
const GOLD: Color = rgb(0xE8_B84A);
const SUPPORTER: Ribbon = Ribbon {
    base: rgb(0x8A_1538),
    stripes: &[stripe(0.0, 0.1, GOLD), stripe(0.9, 0.1, GOLD)],
};

impl Medal {
    /// How many medals the client knows.
    pub(crate) const COUNT: usize = 4;
    /// Every medal, in the hub's catalogue order.
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::EarlyTester,
        Self::EarlyContributor,
        Self::BugHunter,
        Self::Supporter,
    ];

    /// The medal the hub's `id` names, if the client knows it.
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|medal| medal.id() == id)
    }

    /// Position in [`Self::ALL`].
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The hub's id.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::EarlyTester => "early_tester",
            Self::EarlyContributor => "early_contributor",
            Self::BugHunter => "bug_hunter",
            Self::Supporter => "supporter",
        }
    }

    /// The name players read.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::EarlyTester => "Early Tester",
            Self::EarlyContributor => "Early Contributor",
            Self::BugHunter => "Bug Hunter",
            Self::Supporter => "Supporter",
        }
    }

    /// What it is for, in a sentence.
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::EarlyTester => "Helped test SJK in its early days.",
            Self::EarlyContributor => "Contributed to SJK's code in its early days.",
            Self::BugHunter => "Found bugs that got fixed.",
            Self::Supporter => "Donated to SJK's development.",
        }
    }

    /// How a player comes to hold it, in a sentence (the Collection's hover card).
    pub(crate) const fn how_to_get(self) -> &'static str {
        match self {
            Self::EarlyTester => {
                "Given by the SJK team to players who helped test SJK in its early days."
            }
            Self::EarlyContributor => {
                "Given by the SJK team to players who contributed to SJK's code in its early days."
            }
            Self::BugHunter => {
                "Given by the SJK team when a bug you found gets fixed; it can be given again."
            }
            Self::Supporter => {
                "Given by the SJK team to players who donate more than 1 euro to SJK, through the SJK site."
            }
        }
    }

    /// The saber blade (an unlockable's id) the medal brings while held, if any: the
    /// hub's `MedalKind::grants`. Supporter brings none (no decision yet).
    pub(crate) const fn blade(self) -> Option<&'static str> {
        match self {
            Self::EarlyTester => Some("saber_hologram"),
            Self::EarlyContributor => Some("saber_runic"),
            Self::BugHunter => Some("saber_glitch"),
            Self::Supporter => None,
        }
    }

    /// The edition it belongs to ([`crate::editions`]).
    pub(crate) const fn edition(self) -> &'static str {
        match self {
            Self::EarlyTester | Self::EarlyContributor | Self::BugHunter | Self::Supporter => {
                crate::editions::ALPHA
            }
        }
    }

    /// Whether the SJK team may give it again, so its count can pass 1.
    pub(crate) const fn repeatable(self) -> bool {
        matches!(self, Self::BugHunter)
    }

    /// The ribbon bar's colours.
    pub(crate) const fn ribbon(self) -> Ribbon {
        match self {
            Self::EarlyTester => EARLY_TESTER,
            Self::EarlyContributor => EARLY_CONTRIBUTOR,
            Self::BugHunter => BUG_HUNTER,
            Self::Supporter => SUPPORTER,
        }
    }

    /// The whole medal on its ribbon, 512 square.
    pub(crate) const fn art_png(self) -> &'static [u8] {
        match self {
            Self::EarlyTester => include_bytes!("../assets/medals/early_tester.png"),
            Self::EarlyContributor => include_bytes!("../assets/medals/early_contributor.png"),
            Self::BugHunter => include_bytes!("../assets/medals/bug_hunter.png"),
            Self::Supporter => include_bytes!("../assets/medals/supporter.png"),
        }
    }

    /// The medallion alone, one atlas cell (128 square).
    pub(crate) const fn small_png(self) -> &'static [u8] {
        match self {
            Self::EarlyTester => include_bytes!("../assets/medals/early_tester_small.png"),
            Self::EarlyContributor => {
                include_bytes!("../assets/medals/early_contributor_small.png")
            }
            Self::BugHunter => include_bytes!("../assets/medals/bug_hunter_small.png"),
            Self::Supporter => include_bytes!("../assets/medals/supporter_small.png"),
        }
    }

    /// The `TexturedQuad` texture of the medallion alone, uploaded into the icon atlas
    /// at start: for small sizes.
    pub(crate) fn icon(self) -> TextureId {
        crate::ui_renderer::medal_icon(self.index())
    }

    /// The `TexturedQuad` texture of the whole medal, decoded the first time a screen
    /// draws it ([`art`]); nothing shows until it is ready.
    pub(crate) const fn art(self) -> TextureId {
        TextureId(ART_TEXTURE_BASE + self as u32)
    }

    /// The medal whose whole picture a `TexturedQuad` texture names, if it names one.
    pub(crate) fn from_art(texture: TextureId) -> Option<Self> {
        let index = texture.0.checked_sub(ART_TEXTURE_BASE)?;
        Self::ALL.get(index as usize).copied()
    }

    /// The name with the count after it when it was given more than once
    /// ("Bug Hunter x2").
    pub(crate) fn label(self, count: u32) -> String {
        if count > 1 {
            format!("{} x{count}", self.name())
        } else {
            self.name().to_owned()
        }
    }
}

/// A player's known medals and how many times each was given, as the scoreboard, the
/// player card and the Players page carry them: `Copy`, so a row holds it without
/// allocating, and always in catalogue order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Medals {
    counts: [u16; Medal::COUNT],
}

impl Medals {
    /// The known medals of a hub list; unknown ids are left out.
    pub(crate) fn from_wire(list: &[sjk_identity::Medal]) -> Self {
        let mut medals = Self::default();
        for entry in list {
            if let Some(medal) = Medal::from_id(&entry.id) {
                let count = if medal.repeatable() {
                    entry.count.clamp(1, u32::from(u16::MAX)) as u16
                } else {
                    1
                };
                medals.counts[medal.index()] = count;
            }
        }
        medals
    }

    pub(crate) fn is_empty(self) -> bool {
        self.counts.iter().all(|count| *count == 0)
    }

    /// How many different medals.
    pub(crate) fn len(self) -> usize {
        self.counts.iter().filter(|count| **count > 0).count()
    }

    /// Each medal held and its count, in catalogue order.
    pub(crate) fn iter(self) -> impl Iterator<Item = (Medal, u32)> {
        Medal::ALL
            .into_iter()
            .zip(self.counts)
            .filter(|(_, count)| *count > 0)
            .map(|(medal, count)| (medal, u32::from(count)))
    }
}

/// One of the player's own medals as their profile gives it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Award {
    pub(crate) medal: Medal,
    pub(crate) count: u32,
    /// When it was last given, unix seconds; 0 when the hub did not say.
    pub(crate) awarded: i64,
    /// The team's note as plain text, often empty.
    pub(crate) note: String,
}

impl Award {
    /// The name, with the count when it was given more than once.
    pub(crate) fn label(&self) -> String {
        self.medal.label(self.count)
    }

    /// "Given 07/10/2026", or nothing when the hub did not say when.
    pub(crate) fn given(&self) -> String {
        let date = date_text(self.awarded);
        if date.is_empty() {
            date
        } else {
            format!("Given {date}")
        }
    }
}

/// The known medals of a profile's list, with their dates and notes, in catalogue order.
pub(crate) fn awards(list: &[sjk_identity::Medal]) -> Vec<Award> {
    let mut awards: Vec<Award> = list
        .iter()
        .filter_map(|entry| {
            let medal = Medal::from_id(&entry.id)?;
            Some(Award {
                medal,
                count: if medal.repeatable() {
                    entry.count.max(1)
                } else {
                    1
                },
                awarded: entry.awarded,
                note: plain_note(&entry.note),
            })
        })
        .collect();
    awards.sort_by_key(|award| award.medal.index());
    awards.dedup_by_key(|award| award.medal);
    awards
}

/// A note as plain text: colour codes and control characters dropped, spaces
/// collapsed, at most the hub's 200 characters.
pub(crate) fn plain_note(note: &str) -> String {
    let mut out = String::new();
    let mut chars = note.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(|next| *next != '^') {
            chars.next();
        } else if c.is_control() || c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out.trim_end().chars().take(NOTE_LIMIT).collect()
}

/// `dd/mm/yyyy` (UTC) of unix `seconds`; empty when it is not after 1970.
pub(crate) fn date_text(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }
    // Howard Hinnant's civil_from_days.
    let z = seconds.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day:02}/{month:02}/{year}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_medal_brings_its_own_blade_which_says_so() {
        for medal in Medal::ALL {
            let Some(id) = medal.blade() else {
                continue;
            };
            let blade = crate::unlockables::blade_skin(id).expect("a blade skin");
            assert!(
                blade.how_to_get.contains(medal.name()),
                "{}: {}",
                medal.id(),
                blade.how_to_get
            );
            let others = Medal::ALL
                .iter()
                .filter(|other| other.blade() == medal.blade());
            assert_eq!(others.count(), 1);
        }
    }

    fn wire(id: &str, count: u32) -> sjk_identity::Medal {
        sjk_identity::Medal {
            id: id.to_owned(),
            count,
            awarded: 0,
            note: String::new(),
        }
    }

    #[test]
    fn ids_round_trip_and_unknown_ones_are_left_out() {
        for medal in Medal::ALL {
            assert_eq!(Medal::from_id(medal.id()), Some(medal));
            assert_eq!(Medal::from_art(medal.art()), Some(medal));
            assert_eq!(Medal::ALL[medal.index()], medal);
        }
        assert_eq!(Medal::from_id("from_the_future"), None);
        assert_eq!(Medal::from_id("supporter"), Some(Medal::Supporter));
        assert_eq!(Medal::Supporter.blade(), None);
        assert!(!Medal::Supporter.repeatable());
        // The JoF Clan medal is gone (the clan's emblem now follows its tag,
        // `jof_tag`): an old award is an unknown id.
        let medals = Medals::from_wire(&[
            wire("from_the_future", 1),
            wire("jof_clan", 1),
            wire("bug_hunter", 1),
        ]);
        assert_eq!(medals.iter().collect::<Vec<_>>(), [(Medal::BugHunter, 1)]);
        assert_eq!(medals.len(), 1);
        assert!(Medals::from_wire(&[wire("nope", 3)]).is_empty());
    }

    #[test]
    fn only_a_repeatable_medal_counts_past_one_and_order_is_the_catalogues() {
        let medals = Medals::from_wire(&[
            wire("bug_hunter", 3),
            wire("early_tester", 4),
            wire("early_contributor", 0),
        ]);
        assert_eq!(
            medals.iter().collect::<Vec<_>>(),
            [
                (Medal::EarlyTester, 1),
                (Medal::EarlyContributor, 1),
                (Medal::BugHunter, 3)
            ]
        );
        assert_eq!(Medal::BugHunter.label(3), "Bug Hunter x3");
        assert_eq!(Medal::EarlyTester.label(1), "Early Tester");
    }

    #[test]
    fn the_pictures_are_the_sizes_the_renderer_expects() {
        for medal in Medal::ALL {
            let art = image::load_from_memory(medal.art_png()).expect("the whole medal");
            assert_eq!((art.width(), art.height()), (512, 512), "{medal:?}");
            let small = image::load_from_memory(medal.small_png()).expect("the medallion");
            assert_eq!(
                (small.width(), small.height()),
                (crate::ui_renderer::ICON_SIZE, crate::ui_renderer::ICON_SIZE),
                "{medal:?}"
            );
        }
    }

    #[test]
    fn every_ribbon_stripe_lies_on_its_ribbon() {
        for medal in Medal::ALL {
            for stripe in medal.ribbon().stripes {
                assert!(stripe.at >= 0.0 && stripe.at + stripe.width <= 1.0 + 1e-6);
            }
        }
    }

    #[test]
    fn notes_are_plain_text_and_dates_european() {
        assert_eq!(plain_note("The ^1fog^7\n bug\tfixed "), "The fog bug fixed");
        assert_eq!(plain_note(&"a".repeat(300)).len(), 200);
        assert_eq!(date_text(0), "");
        assert_eq!(date_text(1_791_336_225), "07/10/2026");
        let profile = [
            sjk_identity::Medal {
                id: "bug_hunter".to_owned(),
                count: 2,
                awarded: 1_791_336_225,
                note: "^3Fog".to_owned(),
            },
            wire("early_tester", 1),
            wire("unknown", 1),
        ];
        let awards = awards(&profile);
        assert_eq!(awards.len(), 2);
        assert_eq!(awards[0].medal, Medal::EarlyTester);
        assert_eq!(awards[0].given(), "");
        assert_eq!(awards[1].label(), "Bug Hunter x2");
        assert_eq!(awards[1].given(), "Given 07/10/2026");
        assert_eq!(awards[1].note, "Fog");
    }
}

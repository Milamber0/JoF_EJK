//! Holocrons (`docs/holocrons.md`): loot a player earns by playing. The SJK hub rolls and
//! stores them; the client shows them. Opening one gives a sealed card and credits
//! ([`crate::cards`]).
//!
//! This is the one place that says which tiers the client knows and how each looks: its
//! wire id, name, colour and odds ([`TIERS`]), and how a profile's entries and the hub's
//! progress read ([`entries`], [`counts_of`], [`next_text`]). Everything that draws a
//! holocron reads these: the drop pop-up ([`crate::holocron_popup`]), the chat lines
//! ([`line`]), the staff page and the Profile screen's Holocrons tab (which reads the
//! counts, the list and the progress from the identity's snapshot once, in
//! [`crate::console::holocrons_panel::Data::of`]). An id the client does not know is left
//! out.
//!
//! | piece | module |
//! | --- | --- |
//! | what counts as actively playing, and the input clock | [`activity`] |
//! | the tier gem, drawn when a tier has no icon | [`gem`] |
//! | the tiers' icons in the UI atlas | [`icons`] |
//! | the chat lines' words | [`line`] |
//! | `debug_holocron` | [`rehearsal`] |
//! | `holocrons_seen.txt` | [`seen`] |
//! | the Holocrons tab's 3D holocron | [`stage`] |
//!
//! Adding a tier is one entry in [`TIERS`] (the hub's catalogue has the same ids, names
//! and order) and its icon `gfx/sjk/holocron_<id>.png`.

pub(crate) mod activity;
pub(crate) mod gem;
pub(crate) mod icons;
pub(crate) mod line;
pub(crate) mod rehearsal;
pub(crate) mod seen;
pub(crate) mod stage;

use sjk_identity::{Holocron, HolocronCounts, HolocronProgress};
use sjk_ui::Color;

/// How many tiers the client knows.
pub(crate) const COUNT: usize = 4;
/// The most holocrons the pop-up shows at once when a profile first lists many (a
/// reinstalled PC): the newest of them.
pub(crate) const FIRST_READ_MAX: usize = 20;

/// `0xRRGGBB` as an opaque colour.
pub(crate) const fn rgb(hex: u32) -> Color {
    Color::new(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    )
}

/// One tier of holocron, in the hub's catalogue order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tier {
    /// The hub's id (`rare`), also the end of its icon's name.
    pub(crate) id: &'static str,
    /// The name players read ("Rare Holocron").
    pub(crate) name: &'static str,
    /// The tier alone ("Rare"), as a saber shader's tier reads (`unlockables::Rarity`).
    pub(crate) label: &'static str,
    /// The tier's colour: its pop-up, chat lines, gem and counts. None of the game's
    /// `^n` colours, and four colours that stay apart for players who tell colours
    /// apart poorly.
    pub(crate) colour: Color,
    /// The odds a drop is this tier, in parts per thousand; the four sum to 1000. This
    /// client's copy: what players read is the hub's ([`Tier::odds`]).
    pub(crate) per_mille: u16,
    /// Position in [`TIERS`].
    pub(crate) index: usize,
    /// The colour of the point light the Profile screen's holocron casts, linear RGB on
    /// the scale of Illuminate's `[1.5, 1.3, 1.0]` and not clamped: the one in
    /// `scripts/holocron_assets.py` (`Tier.light`) and `assets/holocron/README.md`.
    pub(crate) light: [f32; 3],
}

/// Every tier, in the hub's catalogue order (`uncommon`, `rare`, `legendary`,
/// `mythical`: the commonest first).
pub(crate) const TIERS: [Tier; COUNT] = [
    Tier {
        id: "uncommon",
        name: "Uncommon Holocron",
        label: "Uncommon",
        colour: rgb(0x2F_C77A),
        per_mille: 600,
        index: 0,
        light: [0.55, 1.50, 0.60],
    },
    Tier {
        id: "rare",
        name: "Rare Holocron",
        label: "Rare",
        colour: rgb(0x2E_7BFF),
        per_mille: 280,
        index: 1,
        light: [0.50, 1.00, 1.75],
    },
    Tier {
        id: "legendary",
        name: "Legendary Holocron",
        label: "Legendary",
        colour: rgb(0xA6_4DFF),
        per_mille: 105,
        index: 2,
        light: [1.25, 0.45, 1.75],
    },
    Tier {
        id: "mythical",
        name: "Mythical Holocron",
        label: "Mythical",
        colour: rgb(0xFF_C933),
        per_mille: 15,
        index: 3,
        light: [1.85, 1.30, 0.40],
    },
];

impl Tier {
    /// How often a drop is this tier, as players read it ("10.5%"): the hub's odds.
    pub(crate) fn odds(&self) -> String {
        crate::cards::loot::percent(crate::cards::loot::drop(self.index))
    }

    /// The tier the hub's `id` names, if the client knows it.
    pub(crate) fn from_id(id: &str) -> Option<&'static Tier> {
        TIERS.iter().find(|tier| tier.id == id)
    }

    /// The game path of the tier's icon, without its extension
    /// (`gfx/sjk/holocron_rare`). It is a bundled file mounted below all game data
    /// ([`crate::illuminate`]'s list); where it is missing the tier draws as a gem
    /// ([`gem`]).
    pub(crate) fn icon_path(&self) -> String {
        format!("gfx/sjk/holocron_{}", self.id)
    }

    /// What the Profile screen says of the tier under its list: how often it drops and who
    /// hears of a drop (what opening one gives is [`crate::cards::inside`]).
    pub(crate) const fn about(&self) -> &'static str {
        match self.index {
            0 => "The commonest holocron: about 6 in 10 drops.",
            1 => "About 3 in 10 drops.",
            2 => "About 1 in 10 drops. Every SJK player sees it in chat when one drops.",
            _ => {
                "About 1 in 67 drops, at most 1 a day for a player. Every SJK player sees it in chat when one drops."
            }
        }
    }

    /// "a" or "an" for the name: "an Uncommon Holocron", "a Rare Holocron".
    pub(crate) fn article(&self) -> &'static str {
        if self.name.starts_with(['A', 'E', 'I', 'O', 'U']) {
            "an"
        } else {
            "a"
        }
    }

    /// The tier's colour at `alpha`.
    pub(crate) fn colour_alpha(&self, alpha: f32) -> Color {
        Color::new(self.colour.r, self.colour.g, self.colour.b, alpha)
    }

    /// Whether a drop of this tier is told to every SJK player (legendary and mythical)
    /// and not just to its owner.
    pub(crate) fn is_announced(&self) -> bool {
        self.index >= 2
    }
}

/// The tier the hub's `id` names, if the client knows it.
#[allow(dead_code)] // for screens that know a tier by the hub's id
pub(crate) fn by_id(id: &str) -> Option<&'static Tier> {
    Tier::from_id(id)
}

/// The colour of the tier `id` names; the chat's gold for an id the client does not know.
#[allow(dead_code)] // for screens that know a tier by the hub's id
pub(crate) fn colour(id: &str) -> Color {
    by_id(id).map_or(crate::sjk_chat_look::GOLD, |tier| tier.colour)
}

/// The name of the tier `id` names, if the client knows it.
#[allow(dead_code)] // for screens that know a tier by the hub's id
pub(crate) fn name(id: &str) -> Option<&'static str> {
    by_id(id).map(|tier| tier.name)
}

/// One of the player's own holocrons as their profile lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    /// Its number at the hub, which only rises.
    pub(crate) id: u64,
    pub(crate) tier: &'static Tier,
    /// When it dropped, unix seconds; 0 when the hub did not say.
    pub(crate) dropped: i64,
    /// Staff gave it rather than play.
    pub(crate) gift: bool,
    /// The team's note as plain text; only on a gift, often empty.
    pub(crate) note: String,
}

impl Entry {
    /// Read a hub's entry; `None` for a tier the client does not know.
    pub(crate) fn from_wire(holocron: &Holocron) -> Option<Self> {
        Some(Self {
            id: holocron.id,
            tier: Tier::from_id(&holocron.tier)?,
            dropped: holocron.dropped,
            gift: holocron.source == "staff",
            note: crate::medals::plain_note(&holocron.note),
        })
    }

    /// "Found 10/10/2026 14:05" (UTC, day first, 24 hours), or "A gift from the SJK
    /// team 10/10/2026 14:05" for a gift; no date when the hub did not say when.
    pub(crate) fn when(&self) -> String {
        let date = when_text(self.dropped);
        match (self.gift, date.is_empty()) {
            (true, true) => "A gift from the SJK team".to_owned(),
            (true, false) => format!("A gift from the SJK team, {date}"),
            (false, true) => String::new(),
            (false, false) => format!("Found {date}"),
        }
    }
}

/// The known holocrons of a profile's list, in the hub's order (newest first).
pub(crate) fn entries(list: &[Holocron]) -> Vec<Entry> {
    list.iter().filter_map(Entry::from_wire).collect()
}

/// A profile's counts as an array in [`TIERS`] order.
pub(crate) fn counts_of(counts: &HolocronCounts) -> [u32; COUNT] {
    std::array::from_fn(|index| counts.of(TIERS[index].id))
}

/// `dd/mm/yyyy HH:MM` (UTC) of unix `seconds`; empty when it is not after 1970.
pub(crate) fn when_text(seconds: i64) -> String {
    let date = crate::medals::date_text(seconds);
    if date.is_empty() {
        return date;
    }
    let of_day = seconds.rem_euclid(86_400);
    format!("{date} {:02}:{:02}", of_day / 3_600, of_day % 3_600 / 60)
}

/// Ask the hub for fresh progress (a page that shows it, as it opens); at most once
/// every 30 seconds.
pub(crate) fn refresh() {
    crate::player_identity::refresh_holocrons();
}

/// What the progress says in a line: how long to the next holocron, or that the day's
/// limit is reached. The time does not count down on its own (it is as the hub last said,
/// and counts only while the player plays), so it is rounded up to whole minutes.
pub(crate) fn next_text(progress: &HolocronProgress) -> String {
    if progress.capped() {
        return format!(
            "Daily limit reached: {} of {} holocrons today",
            progress.today, progress.daily_cap
        );
    }
    let minutes = progress.remaining_secs().div_ceil(60).max(1);
    if minutes == 1 {
        "Next holocron in about 1 minute of play".to_owned()
    } else {
        format!("Next holocron in about {minutes} minutes of play")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::CodePalette;

    fn distance(a: Color, b: [f32; 4]) -> f32 {
        ((a.r - b[0]).powi(2) + (a.g - b[1]).powi(2) + (a.b - b[2]).powi(2)).sqrt()
    }

    /// The list is pinned, in the hub's catalogue order: `src/holocrons.rs` in
    /// Sol-Vulpes/SJK-hub carries the same ids, names and odds, and its test pins them.
    #[test]
    fn the_catalogue_is_the_hubs() {
        let list: Vec<(&str, &str, u16, String)> = TIERS
            .iter()
            .map(|tier| (tier.id, tier.name, tier.per_mille, tier.odds()))
            .collect();
        assert_eq!(
            list,
            [
                ("uncommon", "Uncommon Holocron", 600, "60%".to_owned()),
                ("rare", "Rare Holocron", 280, "28%".to_owned()),
                ("legendary", "Legendary Holocron", 105, "10.5%".to_owned()),
                ("mythical", "Mythical Holocron", 15, "1.5%".to_owned()),
            ]
        );
        assert_eq!(
            TIERS.iter().map(|t| u32::from(t.per_mille)).sum::<u32>(),
            1000
        );
        for (index, tier) in TIERS.iter().enumerate() {
            assert_eq!(tier.index, index);
            assert_eq!(Tier::from_id(tier.id), Some(tier));
            assert_eq!(by_id(tier.id).map(|t| t.name), Some(tier.name));
            assert!(tier.name.ends_with(" Holocron"));
            assert_eq!(tier.name, format!("{} Holocron", tier.label));
        }
        assert_eq!(by_id("from_the_future"), None);
        assert_eq!(name("rare"), Some("Rare Holocron"));
        assert_eq!(name("nope"), None);
    }

    /// The tier colours are none of the game's `^n` codes (in the game's palette and in
    /// the SJK UI's lifted one), as the SJK chat's gold is none of them
    /// (`sjk_chat_look.rs`): a drop line is never mistaken for a coloured player name.
    /// And the four are apart from one another, for players who tell hues apart poorly.
    #[test]
    fn the_colours_are_none_of_the_colour_codes_and_apart_from_each_other() {
        for tier in &TIERS {
            for index in 0..=9 {
                for palette in [CodePalette::Game, CodePalette::Legible] {
                    let code = palette.colour(index);
                    assert!(
                        distance(tier.colour, code) > 0.28,
                        "{} is too near ^{index} {palette:?}: {code:?}",
                        tier.id
                    );
                }
            }
            assert_eq!(tier.colour.a, 1.0);
        }
        for (a, first) in TIERS.iter().enumerate() {
            for second in &TIERS[a + 1..] {
                let apart = distance(
                    first.colour,
                    [second.colour.r, second.colour.g, second.colour.b, 1.0],
                );
                assert!(apart > 0.4, "{} and {}: {apart}", first.id, second.id);
            }
        }
    }

    #[test]
    fn names_articles_icons_and_who_hears_of_a_drop() {
        assert_eq!(TIERS[0].article(), "an");
        assert_eq!(TIERS[1].article(), "a");
        assert_eq!(TIERS[1].icon_path(), "gfx/sjk/holocron_rare");
        let announced: Vec<&str> = TIERS
            .iter()
            .filter(|tier| tier.is_announced())
            .map(|tier| tier.id)
            .collect();
        assert_eq!(announced, ["legendary", "mythical"], "as the hub delivers");
        assert_eq!(TIERS[2].colour_alpha(0.5).a, 0.5);
        assert_eq!(colour("legendary"), TIERS[2].colour);
        assert_eq!(colour("nope"), crate::sjk_chat_look::GOLD);
    }

    fn wire(id: u64, tier: &str, source: &str, note: &str) -> Holocron {
        Holocron {
            opened: None,
            id,
            tier: tier.to_owned(),
            dropped: 1_791_641_100,
            source: source.to_owned(),
            note: note.to_owned(),
        }
    }

    #[test]
    fn entries_keep_the_known_tiers_in_the_hubs_order_with_plain_notes() {
        let list = [
            wire(9, "legendary", "play", ""),
            wire(8, "from_the_future", "play", ""),
            wire(7, "rare", "staff", "For the ^1fog^7 bug"),
        ];
        let entries = entries(&list);
        let ids: Vec<u64> = entries.iter().map(|entry| entry.id).collect();
        assert_eq!(ids, [9, 7]);
        assert!(!entries[0].gift && entries[1].gift);
        assert_eq!(entries[1].note, "For the fog bug");
        assert_eq!(entries[0].when(), "Found 10/10/2026 14:05");
        assert_eq!(
            entries[1].when(),
            "A gift from the SJK team, 10/10/2026 14:05"
        );
        let undated = Entry {
            dropped: 0,
            ..entries[0].clone()
        };
        assert_eq!(undated.when(), "");
        let undated_gift = Entry {
            dropped: 0,
            ..entries[1].clone()
        };
        assert_eq!(undated_gift.when(), "A gift from the SJK team");
    }

    #[test]
    fn counts_are_in_the_catalogues_order() {
        let counts = HolocronCounts {
            uncommon: 5,
            rare: 3,
            legendary: 1,
            mythical: 0,
        };
        assert_eq!(counts_of(&counts), [5, 3, 1, 0]);
        assert_eq!(when_text(0), "");
        assert_eq!(when_text(1_791_641_100), "10/10/2026 14:05");
        assert_eq!(when_text(86_399), "01/01/1970 23:59");
    }

    fn progress(progress_secs: u64, today: u32) -> HolocronProgress {
        HolocronProgress {
            progress_secs,
            every_secs: 1_800,
            today,
            daily_cap: 8,
            server_time: 0,
            can_open: true,
            read_at: std::time::Instant::now(),
        }
    }

    #[test]
    fn the_next_holocron_is_said_in_whole_minutes_or_the_limit() {
        assert_eq!(
            next_text(&progress(0, 0)),
            "Next holocron in about 30 minutes of play"
        );
        assert_eq!(
            next_text(&progress(600, 3)),
            "Next holocron in about 20 minutes of play"
        );
        assert_eq!(
            next_text(&progress(1_741, 3)),
            "Next holocron in about 1 minute of play",
            "rounded up"
        );
        assert_eq!(
            next_text(&progress(1_800, 3)),
            "Next holocron in about 1 minute of play"
        );
        assert_eq!(
            next_text(&progress(300, 8)),
            "Daily limit reached: 8 of 8 holocrons today"
        );
    }
}

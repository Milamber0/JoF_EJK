//! The quick wheel's Force page: a live page whose choices are the Force powers the
//! player can use right now, read from the playerstate as the wheel opens (the
//! known bits the Force bar reads, [`sjk_client::force_wheel`], with JoF JA+'s
//! Stasis, Repulse and Dash where granted). Jump and the saber powers are passive
//! and left out. SJK's Illuminate is a toy, not a Force power: it has a choice of
//! its own on the Toys page ([`super::catalog`]'s Toys).
//!
//! A choice selects its power (`forceselect`, so the Use Force key uses it next)
//! and, for an instant power, uses it at once with its own command (`force_throw`,
//! `force_heal`, ...). The held powers, Grip, Lightning, Drain and Stasis, cannot
//! be held from a wheel, so choosing one only selects it. The power selected now
//! wears the wheel's gold dot.
//!
//! The order is fixed, so a power keeps its place round the ring while the build
//! does: the neutral powers, then the light side's, then the dark side's, each in
//! the retail default keys' order (F1 Push to F12 Drain), then JoF's abilities last.
//! Up to [`MAX_FORCE_CHOICES`] go on the ring; the rest continue on
//! a second page right after it ("Force 2"), which only an admin's every-power
//! build fills. Out of a game, spectating or with no power, the page says so.

use super::pages::MAX_FORCE_CHOICES;
use super::{ShownChoice, ShownPage};
use crate::hud::force_wheel as bar;
use sjk_client::force_wheel::{self, DASH, REPULSE, STASIS};
use sjk_ui::TextureId;

/// The page's order, by Force wheel entry (`forcePowers_t`, then the pseudo-slots).
pub(crate) const ORDER: [u8; 17] = [
    // Neutral: Push, Pull, Speed, Sense.
    3, 4, 2, 14, //
    // Light: Heal, Protect, Absorb, Mind Trick, Team Heal.
    0, 9, 10, 5, 11, //
    // Dark: Grip, Lightning, Dark Rage, Drain, Team Energize.
    6, 7, 8, 13, 12, //
    DASH, STASIS, REPULSE,
];

/// What the id of the second page adds to the Force page's; never part of an id
/// `wheel.json` gives (letters and digits only).
pub(crate) const SECOND: &str = "~2";
/// What the page shows without powers.
pub(crate) const EMPTY: &str = "No Force powers here";

/// The player's Force as the page reads it when the wheel opens.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Powers {
    /// `forcePowersKnown` as the Force bar reads it (`Selection::known`).
    pub(crate) known: u32,
    /// The entry selected now (`Selection::selected_force`).
    pub(crate) selected: u8,
    /// JA+ merc mode: Lightning is the flamethrower.
    pub(crate) flamethrower: bool,
    /// The Force bar's pictures ([`bar::load`]).
    pub(crate) icons: [Option<TextureId>; bar::ICONS],
}

/// The command that uses entry `slot` at once; `None` for a power that is held.
pub(crate) fn use_command(slot: u8) -> Option<&'static str> {
    Some(match slot {
        0 => "force_heal",
        2 => "force_speed",
        3 => "force_throw",
        4 => "force_pull",
        5 => "force_distract",
        8 => "force_rage",
        9 => "force_protect",
        10 => "force_absorb",
        11 => "force_healother",
        12 => "force_forcepowerother",
        14 => "force_seeing",
        REPULSE => "force_repulse",
        DASH => "force_dash",
        // Grip, Lightning, Drain, Stasis.
        _ => return None,
    })
}

/// The console command choosing entry `slot` runs: select it, then use it when
/// it is instant.
pub(crate) fn command(slot: u8) -> String {
    match use_command(slot) {
        Some(use_it) => format!("forceselect {slot}; {use_it}"),
        None => format!("forceselect {slot}"),
    }
}

/// The powers `powers` can use, in the page's order.
pub(crate) fn usable(known: u32) -> impl Iterator<Item = u8> {
    ORDER
        .into_iter()
        .filter(move |&slot| force_wheel::valid(known, slot))
}

/// The page's choices for `powers`.
pub(crate) fn choices(powers: &Powers) -> Vec<ShownChoice> {
    usable(powers.known)
        .map(|slot| ShownChoice {
            label: bar::name(slot, powers.flamethrower).to_owned(),
            command: command(slot),
            icon: bar::icon(&powers.icons, slot, powers.flamethrower),
            on: slot == powers.selected,
            gif: None,
        })
        .collect()
}

/// The Force page `id`, `name` as the wheel shows it: its first
/// [`MAX_FORCE_CHOICES`] powers, and the rest on a second page after it, named
/// with a 2. Without `powers` (no game, spectating) it is one empty page.
pub(crate) fn pages(id: &str, name: &str, powers: Option<&Powers>) -> Vec<ShownPage> {
    let mut choices = powers.map(choices).unwrap_or_default();
    let rest = choices.split_off(choices.len().min(MAX_FORCE_CHOICES));
    let mut pages = vec![ShownPage {
        id: id.to_owned(),
        name: name.to_owned(),
        choices,
        force: true,
        gifs: false,
    }];
    if !rest.is_empty() {
        pages.push(ShownPage {
            id: format!("{id}{SECOND}"),
            name: format!("{name} 2"),
            choices: rest,
            force: true,
            gifs: false,
        });
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Light side: the neutral four and the light five, Jump and the saber
    /// powers known too.
    const LIGHT: u32 = (1 << 3)
        | (1 << 4)
        | (1 << 2)
        | (1 << 14)
        | (1 << 0)
        | (1 << 9)
        | (1 << 10)
        | (1 << 5)
        | (1 << 11)
        | (1 << 1)
        | (7 << 15);

    fn powers(known: u32, selected: u8) -> Powers {
        Powers {
            known,
            selected,
            flamethrower: false,
            icons: [None; bar::ICONS],
        }
    }

    fn labels(pages: &[ShownPage]) -> Vec<Vec<&str>> {
        pages
            .iter()
            .map(|page| page.choices.iter().map(|c| c.label.as_str()).collect())
            .collect()
    }

    #[test]
    fn the_order_holds_every_wheel_entry_once_and_no_passive_power() {
        let mut seen = ORDER.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), ORDER.len());
        // Jump and the saber powers are never on it.
        for passive in [1, 15, 16, 17] {
            assert!(!ORDER.contains(&passive));
        }
        // Every entry the Force bar can hold, and nothing else.
        let (wheel, count) = force_wheel::build(u32::MAX >> (32 - force_wheel::MAX_SLOTS));
        let mut bar: Vec<u8> = wheel[..count].to_vec();
        bar.sort_unstable();
        assert_eq!(bar, seen);
    }

    #[test]
    fn a_light_build_is_nine_choices_on_one_ring_and_illuminate_is_not_one() {
        // Illuminate is a toy: no bit of the known powers brings it to the page.
        let pages = pages("force", "Force", Some(&powers(LIGHT | 1 << 21, 9)));
        assert_eq!(
            labels(&pages),
            [[
                "Push",
                "Pull",
                "Speed",
                "Sense",
                "Heal",
                "Protect",
                "Absorb",
                "Mind Trick",
                "Team Heal"
            ]]
        );
        // The selected power wears the dot, alone.
        let marked: Vec<&str> = pages[0]
            .choices
            .iter()
            .filter(|c| c.on)
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(marked, ["Protect"]);
        assert!(pages[0].force);
    }

    #[test]
    fn instant_powers_are_used_and_held_ones_only_selected() {
        assert_eq!(command(3), "forceselect 3; force_throw");
        assert_eq!(command(14), "forceselect 14; force_seeing");
        assert_eq!(command(REPULSE), "forceselect 19; force_repulse");
        for held in [6, 7, 13, STASIS] {
            assert_eq!(command(held), format!("forceselect {held}"));
        }
        // Every instant command is a bind the client knows.
        for slot in ORDER {
            if let Some(command) = use_command(slot) {
                assert!(
                    crate::input::generic_commands::generic_command(command).is_some()
                        || matches!(command, "force_repulse" | "force_dash"),
                    "{command}"
                );
            }
        }
    }

    #[test]
    fn past_twelve_the_rest_go_on_a_second_page() {
        // Every power and JoF's three: 17.
        let known = u32::MAX >> (32 - force_wheel::MAX_SLOTS);
        let pages = pages("force", "Force", Some(&powers(known, 0)));
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].choices.len(), MAX_FORCE_CHOICES);
        assert_eq!(
            (pages[1].id.as_str(), pages[1].name.as_str()),
            ("force~2", "Force 2")
        );
        assert_eq!(
            labels(&pages)[1],
            ["Drain", "Team Energize", "Dash", "Stasis", "Repulse"]
        );
        // Merc mode names Lightning the flamethrower; it is still only selected.
        let merc = Powers {
            flamethrower: true,
            ..powers(1 << 7, 7)
        };
        let shown = choices(&merc);
        assert_eq!(shown[0].label, "Flamethrower");
        assert_eq!(shown[0].command, "forceselect 7");
    }

    #[test]
    fn without_powers_the_page_is_one_empty_page() {
        for powers in [None, Some(&powers(1 << 1, 0))] {
            let pages = pages("force", "Mine", powers);
            assert_eq!(pages.len(), 1);
            assert!(pages[0].choices.is_empty() && pages[0].force);
            assert_eq!(pages[0].name, "Mine");
        }
    }
}

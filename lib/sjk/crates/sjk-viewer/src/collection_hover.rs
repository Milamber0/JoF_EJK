//! The Collection's hover cards (`docs/sjk-ui.md`, "Hover cards"): resting the pointer on
//! a medal, an achievement, a shader, a toy or a card shows beside it what it is, how it
//! is obtained and where the player stands with it ([`crate::menu::sjk::hover`]). The
//! words come from the same tables as the rest of the page: a medal's
//! [`crate::medals::Medal::how_to_get`], an achievement's goal, a blade skin's
//! `how_to_get` ([`crate::unlockables`]), a card's odds ([`crate::cards::loot`]).

use super::*;
use crate::achievements::Source;
use crate::menu::sjk::hover::{Card, Line};
use crate::menu::sjk::{color, hover};

impl Panel {
    /// Follow the pointer and draw the card of the thing it has rested on, over the
    /// page: the page's last drawing.
    pub(super) fn hover_card(&mut self, frame: &crate::menu::sjk::Frame, inputs: &Inputs<'_>) {
        let now = Instant::now();
        self.hover.follow(self.ui.hovered(), now);
        let Some((token, alpha)) = self.hover.showing(now) else {
            return;
        };
        let (Some(card), Some(anchor)) = (self.card_of(token, inputs), self.ui.rect_for(token))
        else {
            return;
        };
        hover::draw(&mut self.ui, frame, anchor, &card, alpha);
    }

    /// What the card of the thing `token` stands for says; `None` for anything else (a
    /// tab, a button).
    pub(super) fn card_of(&self, token: u16, inputs: &Inputs<'_>) -> Option<Card> {
        let within = |base: u16, count: usize| {
            token
                .checked_sub(base)
                .map(usize::from)
                .filter(|index| *index < count)
        };
        if let Some(index) = within(MEDAL_BASE, self.medals_shown) {
            let (_, shown) = medals::showing(inputs);
            return shown.get(index).map(medal_card);
        }
        if let Some(index) = within(ACHIEVEMENT_BASE, crate::achievements::ALL.len()) {
            let kind = &crate::achievements::ALL[index];
            let standing = inputs
                .standings
                .iter()
                .find(|standing| standing.kind.id == kind.id);
            return Some(achievement_card(kind, standing));
        }
        if let Some(row) = within(SHADER_BASE, self.shader_count) {
            return Some(shader_card(self.skin_at(row), &inputs.holdings));
        }
        if token == TOY_TOKEN {
            return Some(Card {
                title: "Illuminate holocron".to_owned(),
                colour: color::GOLD_BRIGHT,
                lines: vec![
                    Line::how(
                        "Every SJK player has it. Light it here, from the quick wheel's Toys \
                         page or with a key bound to toy_illuminate.",
                    ),
                    Line::status(crate::editions::name(crate::editions::ALPHA)),
                    Line::held("Yours"),
                ],
            });
        }
        if let Some(index) = within(CARD_BASE, self.cards_shown) {
            let (stacks, _) = self.holdings(inputs);
            return stacks?.get(index).map(card_card);
        }
        None
    }
}

/// A medal's card: how the SJK team gives it, the blade it brings, and whether the
/// player holds it.
fn medal_card(showing: &medals::Showing) -> Card {
    let medal = showing.medal;
    let mut lines = vec![Line::how(medal.how_to_get())];
    if let Some(blade) = medal.blade().and_then(crate::unlockables::find) {
        lines.push(Line::status(format!(
            "Brings the {} while you hold it.",
            blade.name
        )));
    }
    lines.push(Line::status(crate::editions::name(medal.edition())));
    lines.push(match &showing.award {
        Some(award) if award.given().is_empty() => Line::held("Yours"),
        Some(award) => Line::held(format!("Yours: {}", award.given().to_lowercase())),
        None => Line::status("Not given to you yet"),
    });
    Card {
        title: medal.name().to_owned(),
        colour: color::GOLD_BRIGHT,
        lines,
    }
}

/// An achievement's card: what to do, who counts it, and how far the player is.
fn achievement_card(
    kind: &'static crate::achievements::Kind,
    standing: Option<&crate::achievements::Standing>,
) -> Card {
    let counted = match kind.source {
        Source::Client(_) => "Counted by SJK in your matches on servers.",
        Source::Hub => "Counted by the SJK hub.",
    };
    let counted = format!("{counted} {}.", crate::editions::name(kind.edition));
    let status = match standing {
        Some(standing) => match standing.unlocked {
            Some(at) if at > 0 => Line::held(format!("Unlocked {}", crate::medals::date_text(at))),
            Some(_) => Line::held("Unlocked"),
            None if kind.goal > 1 => Line::status(format!(
                "{} of {} so far",
                kind.amount(standing.progress.min(kind.goal)),
                kind.amount(kind.goal)
            )),
            None => Line::status("Not unlocked yet"),
        },
        None => Line::status("Not unlocked yet"),
    };
    let done = standing.is_some_and(|standing| standing.unlocked.is_some());
    Card {
        title: kind.name.to_owned(),
        colour: if done {
            color::GOLD_BRIGHT
        } else {
            crate::achievements::medallion::tint(kind.category)
        },
        lines: vec![Line::how(kind.description), Line::status(counted), status],
    }
}

/// `words` with how many are held after it when there are several ("Yours, 2 held").
fn held_count(words: &str, count: u32) -> String {
    if count > 1 {
        format!("{words}, {count} held")
    } else {
        words.to_owned()
    }
}

/// A shader's card: the stock blade (`None`) or a blade skin, how it is obtained, and
/// whether the player holds it.
fn shader_card(
    skin: Option<&'static crate::unlockables::Unlockable>,
    holdings: &crate::unlockables::Holdings<'_>,
) -> Card {
    let Some(skin) = skin else {
        return Card {
            title: "Stock blade".to_owned(),
            colour: color::HOLO,
            lines: vec![
                Line::how("Every player has it: the plain blade, in your saber's colour."),
                Line::held("Yours"),
            ],
        };
    };
    let status = match (holdings.unlock(skin.id), holdings.reason()) {
        (Some(unlock), _) if crate::debug_collectibles::added(unlock) => {
            Line::held(crate::debug_collectibles::NOTE.to_owned())
        }
        (Some(unlock), _) => match crate::medals::date_text(unlock.granted) {
            date if date.is_empty() => Line::held(held_count("Yours", unlock.count)),
            date => Line::held(held_count(&format!("Yours since {date}"), unlock.count)),
        },
        (None, Some(reason)) => Line::status(reason),
        (None, None) => Line::status("Locked: you can still try it on here."),
    };
    Card {
        title: skin.name.to_owned(),
        colour: skin.tier.colour(),
        lines: vec![
            Line::how(skin.how_to_get),
            Line::status(format!(
                "{} blade skin. {}.",
                skin.tier.label(),
                crate::editions::name(skin.edition)
            )),
            Line::status(format!("Saber smith: {}", skin.smith)),
            status,
        ],
    }
}

/// A kind of card's card: where cards come from, how likely this kind is, and how many
/// the player holds.
fn card_card(stack: &crate::cards::Stack) -> Card {
    use crate::cards::loot;
    let chance = loot::kind_any(stack.rarity.index, stack.finish.index);
    let name = if stack.finish.index == 0 {
        stack.rarity.name.to_owned()
    } else {
        format!("{} {}", stack.rarity.name, stack.finish.name)
    };
    let odds = match loot::one_in(chance) {
        rare if rare.is_empty() => format!(
            "{} of holocrons found hold a {name} card.",
            loot::percent(chance)
        ),
        rare => format!(
            "About {rare} holocrons found holds a {name} card ({}).",
            loot::percent(chance)
        ),
    };
    let faces = crate::cards::faces::current();
    let mut lines = vec![
        Line::how("Opening a holocron gives one card and some credits, or a saber shader instead."),
        Line::status(odds),
    ];
    match crate::cards::shown(&faces, stack.card.as_deref()) {
        Some(shown) => lines.push(Line::status(format!(
            "{}, {}: card {} of {}.",
            shown.face.kind.name(),
            shown.face.set_name,
            shown.face.number,
            shown.of
        ))),
        None if stack.sealed() => {
            lines.push(Line::status(
                "Sealed: its card is picked when it is revealed.",
            ));
        }
        None => lines.push(Line::status("Its face is not in your card pack yet.")),
    }
    let date = crate::medals::date_text(stack.first);
    lines.push(Line::held(match (stack.count, date.is_empty()) {
        (1, true) => "You hold 1".to_owned(),
        (1, false) => format!("You hold 1, since {date}"),
        (count, true) => format!("You hold {count}"),
        (count, false) => format!("You hold {count}, the first since {date}"),
    }));
    Card {
        title: stack.name_in(&faces),
        colour: stack.rarity.colour,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_medal_says_how_it_is_given() {
        for medal in crate::medals::Medal::ALL {
            let card = medal_card(&medals::Showing { medal, award: None });
            assert!(card.lines[0].words.starts_with("Given by the SJK team"));
            let names_a_blade = card.lines.iter().any(|line| line.words.contains("blade"));
            assert_eq!(names_a_blade, medal.blade().is_some(), "{medal:?}");
            assert_eq!(card.lines.last().unwrap().words, "Not given to you yet");
        }
    }

    #[test]
    fn every_achievement_says_what_to_do_and_how_far() {
        for kind in &crate::achievements::ALL {
            let card = achievement_card(kind, None);
            assert_eq!(card.lines[0].words, kind.description);
            assert_eq!(card.lines.len(), 3);
        }
        let kind = crate::achievements::ALL
            .iter()
            .find(|kind| kind.goal == 100)
            .unwrap();
        let standing = crate::achievements::Standing {
            kind,
            progress: 37,
            unlocked: None,
        };
        let card = achievement_card(kind, Some(&standing));
        assert_eq!(card.lines[2].words, "37 of 100 so far");
        let done = crate::achievements::Standing {
            unlocked: Some(1_791_641_100),
            ..standing
        };
        let card = achievement_card(kind, Some(&done));
        assert_eq!(card.lines[2].words, "Unlocked 10/10/2026");
        assert_eq!(card.colour, color::GOLD_BRIGHT);
    }

    #[test]
    fn every_shader_says_how_to_get_it() {
        let card = shader_card(None, &crate::unlockables::Holdings::Known(&[]));
        assert_eq!(card.title, "Stock blade");
        for skin in crate::unlockables::blade_skins_by_tier() {
            let card = shader_card(Some(skin), &crate::unlockables::Holdings::Known(&[]));
            assert_eq!(card.lines[0].words, skin.how_to_get);
            assert_eq!(card.lines[2].words, format!("Saber smith: {}", skin.smith));
            assert!(card.lines[3].words.starts_with("Locked"));
        }
        let held = [sjk_identity::Unlock {
            id: "saber_sun".into(),
            granted: 1_791_336_225,
            note: String::new(),
            count: 1,
            medal: None,
        }];
        let sun = crate::unlockables::find("saber_sun").unwrap();
        let card = shader_card(Some(sun), &crate::unlockables::Holdings::Known(&held));
        assert!(card.lines[3].words.starts_with("Yours since "), "{card:?}");
    }

    #[test]
    fn a_card_says_where_it_comes_from_and_its_odds() {
        let stacks = crate::cards::stacks(&[sjk_identity::CardStack {
            card: None,
            rarity: "legendary".to_owned(),
            finish: "solar".to_owned(),
            edition: "alpha".to_owned(),
            count: 2,
            first: 1_791_641_100,
            last: 1_791_641_100,
        }]);
        let card = card_card(&stacks[0]);
        assert_eq!(card.title, "Legendary card, Solar");
        assert_eq!(
            card.lines[1].words,
            "About 1 in 11 001 holocrons found holds a Legendary Solar card (0.0091%)."
        );
        assert_eq!(
            card.lines.last().unwrap().words,
            "You hold 2, the first since 10/10/2026"
        );
    }
}

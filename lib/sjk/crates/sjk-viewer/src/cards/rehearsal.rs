//! `debug_open`: rehearse opening holocrons without the hub (`docs/cards.md`). It makes
//! what an opening would bring, which the Holocrons tab plays in its ceremony as it would
//! a real one, so the burst, the card turning in, its finish, the credits counting up and
//! Next can all be tried offline.
//!
//! Nothing is sent: the command never reaches the identity service, and nothing is kept.
//! The cards it makes go round the rarities and finishes so a few show them all; a finish
//! named among the words makes every card that finish. A rarity the card packs have faces
//! of shows one of them, as the hub would pick; the others stay sealed. `shader` makes every opening a saber shader (the droppable
//! ones in turn, the second of each pair held twice) instead of a card, and shows the
//! crate spin and the shader's entrance.
//!
//! ```text
//! debug_open                     the tiers and how to use it
//! debug_open rare                one Rare Holocron opened
//! debug_open mythical x3         three Mythical Holocrons, one after another
//! debug_open all solar           one of every tier, every card Solar
//! debug_open rare shader x3      three Rare Holocrons, each a droppable shader
//! ```

use super::{FINISHES, RARITIES};
use crate::holocrons::{TIERS, Tier};
use sjk_identity::{Opened, OpenedCard, OpenedUnlock};

/// The console command.
pub(crate) const COMMAND: &str = "debug_open";
/// Its line in the command list and browser.
pub(crate) const HELP: &str = "Rehearse opening holocrons, nothing sent: debug_open <tier|all> [x<count>] [normal|holo|solar|shader]";
/// The word that names every tier.
const ALL: &str = "all";
/// The most one command rehearses: as many as one opening takes.
const MOST: u32 = sjk_identity::OPEN_MAX as u32;

/// The command's usage and the tiers it knows.
pub(crate) fn listing() -> Vec<String> {
    let mut lines = vec![
        format!("usage: {COMMAND} <tier|all> [x<count>] [normal|holo|solar|shader]"),
        "Plays the Holocrons tab's opening as if the hub had opened holocrons: the burst, the sealed card, its finish and the credits. Nothing is sent to the SJK hub and nothing is kept.".to_owned(),
    ];
    for (index, tier) in TIERS.iter().enumerate() {
        let (least, most) = crate::loot_odds::current().credits[index];
        lines.push(format!(
            "  {:<12} {}: a sealed card and {least} to {most} credits",
            tier.id, tier.name
        ));
    }
    lines.push(format!("  {ALL:<12} one of every tier"));
    lines
}

/// What opening the holocrons `args` name would bring (made up, never rolled by the hub).
pub(crate) fn parse(args: &[String]) -> Result<Vec<Opened>, String> {
    let usage = || format!("usage: {COMMAND} <tier|all> [x<count>] [normal|holo|solar|shader]");
    let Some((tier, rest)) = args.split_first() else {
        return Err(usage());
    };
    let tier = tier.to_ascii_lowercase();
    let tiers: Vec<&'static Tier> = if tier == ALL {
        TIERS.iter().collect()
    } else {
        let found = Tier::from_id(&tier).ok_or_else(|| {
            let ids: Vec<&str> = TIERS.iter().map(|tier| tier.id).collect();
            format!("Unknown tier '{tier}': {} or {ALL}", ids.join(", "))
        })?;
        vec![found]
    };
    let mut count = 1;
    let mut finish = None;
    let mut shader = false;
    for word in rest {
        let word = word.to_ascii_lowercase();
        if let Some(number) = word.strip_prefix('x') {
            count = number
                .parse::<u32>()
                .ok()
                .filter(|count| (1..=MOST).contains(count))
                .ok_or_else(|| format!("x<count> is 1 to {MOST}"))?;
        } else if word == "shader" {
            shader = true;
        } else if let Some(found) = FINISHES.iter().find(|finish| finish.id == word) {
            finish = Some(found);
        } else {
            return Err(usage());
        }
    }
    let mut opened = Vec::new();
    // A rarity with faces in the packs shows one of them, as the hub would pick.
    let faces = super::faces::current();
    for tier in tiers {
        for _ in 0..count {
            if opened.len() >= MOST as usize {
                break;
            }
            let at = opened.len();
            if shader {
                let skins: Vec<_> = crate::unlockables::blade_skins()
                    .filter(|skin| skin.droppable)
                    .collect();
                let skin = skins[at % skins.len()];
                opened.push(Opened {
                    holocron: crate::holocrons::rehearsal::FIRST_ID + at as u64,
                    tier: tier.id.to_owned(),
                    card: None,
                    credits: 0,
                    unlock: Some(OpenedUnlock {
                        id: skin.id.to_owned(),
                        name: skin.name.to_owned(),
                        count: 1 + (at as u32 % 2),
                    }),
                });
                continue;
            }
            let rarity = &RARITIES[(tier.index + at) % RARITIES.len()];
            let (least, most) = crate::loot_odds::current().credits[tier.index];
            opened.push(Opened {
                holocron: crate::holocrons::rehearsal::FIRST_ID + at as u64,
                tier: tier.id.to_owned(),
                card: Some(OpenedCard {
                    id: at as u64,
                    card: face_of(&faces, rarity, at),
                    rarity: rarity.id.to_owned(),
                    finish: finish
                        .unwrap_or(&FINISHES[(tier.index + at) % FINISHES.len()])
                        .id
                        .to_owned(),
                    edition: "alpha".to_owned(),
                }),
                credits: u64::from(least + (at as u32 * 7) % (most - least + 1)),
                unlock: None,
            });
        }
    }
    Ok(opened)
}

/// The face a rehearsed copy of `rarity` shows, going round that rarity's faces by
/// `at`; `None` (sealed) when the packs have none of it.
fn face_of(faces: &super::faces::Faces, rarity: &super::Rarity, at: usize) -> Option<String> {
    let of_rarity: Vec<&super::faces::Face> = faces
        .list
        .iter()
        .filter(|face| face.rarity.id == rarity.id)
        .collect();
    of_rarity
        .get(at % of_rarity.len().max(1))
        .map(|face| face.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_shader_word_makes_every_opening_a_droppable_shader() {
        let opened = parse(&words("all shader x2")).unwrap();
        assert_eq!(opened.len(), 2 * TIERS.len());
        for opening in &opened {
            assert!(opening.card.is_none() && opening.credits == 0);
            let unlock = opening.unlock.as_ref().unwrap();
            let skin = crate::unlockables::blade_skin(&unlock.id).unwrap();
            assert!(skin.droppable);
        }
    }

    #[test]
    fn the_words_make_the_openings() {
        let one = parse(&words("rare")).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].tier, "rare");
        assert!((30..=50).contains(&one[0].credits));
        let three = parse(&words("Mythical x3")).unwrap();
        assert_eq!(three.len(), 3);
        assert!(three.iter().all(|opened| opened.tier == "mythical"));
        let finishes: Vec<&str> = three
            .iter()
            .map(|opened| opened.card.as_ref().unwrap().finish.as_str())
            .collect();
        assert_eq!(finishes, ["normal", "holo", "solar"], "round the finishes");
        let solar = parse(&words("all solar")).unwrap();
        assert_eq!(solar.len(), TIERS.len());
        assert!(
            solar
                .iter()
                .all(|opened| opened.card.as_ref().unwrap().finish == "solar")
        );
        assert_eq!(
            parse(&words("all x20")).unwrap().len(),
            20,
            "at most twenty"
        );
        assert!(parse(&[]).is_err());
        assert!(
            parse(&words("golden"))
                .unwrap_err()
                .contains("Unknown tier")
        );
        assert!(parse(&words("rare x0")).is_err());
        assert!(parse(&words("rare x21")).is_err());
        assert!(parse(&words("rare shiny")).is_err());
        assert!(listing().len() > TIERS.len());
    }
}

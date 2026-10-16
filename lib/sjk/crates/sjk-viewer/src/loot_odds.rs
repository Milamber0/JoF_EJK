//! The odds holocrons roll with, as the hub sends them (`PROTOCOL.md`, "Holocrons":
//! `loot` in `GET /v1/holocrons`, 14/10/2026): how often a drop is each tier, the card's
//! rarity and finish, the credits, and how often an opening brings a saber shader and of
//! which tier. The Holocrons tab's loot table, the card odds on hover cards, a holocron's
//! line and the crate spin all read [`current`], so they show what the hub rolls with now.
//!
//! [`BUILT_IN`] is this client's copy, for before the hub's first answer and for hubs
//! older than the tables. The hub's tables replace it only when they fit what this client
//! can show (the same tiers in the same order, four rarities, three finishes, five shader
//! tiers, every share summing to a thousand); otherwise the copy stays.

use crate::cards::{self, FINISHES, RARITIES};
use crate::holocrons::{COUNT, TIERS};
use std::sync::Mutex;

/// The saber shader tiers a shader drop rolls among, commonest first.
pub(crate) const SHADER_TIERS: usize = 5;

/// Every odds table, one entry per holocron tier in [`TIERS`] order; shares per mille.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Table {
    /// How often a drop is each tier.
    pub(crate) drop: [u16; COUNT],
    /// The card's rarity and finish inside each tier.
    pub(crate) card: [cards::Odds; COUNT],
    /// The credits a card opening gives: the least and the most.
    pub(crate) credits: [(u32, u32); COUNT],
    /// How often an opening brings a shader instead of the card and its credits.
    pub(crate) shader: [u32; COUNT],
    /// The shader's tier then, common to mythical.
    pub(crate) shader_tier: [[u32; SHADER_TIERS]; COUNT],
}

/// This client's copy of the hub's tables (`src/holocrons.rs`, `src/cards.rs` and
/// `src/loot.rs` there), shown until the hub sends its own.
pub(crate) const BUILT_IN: Table = Table {
    drop: [
        TIERS[0].per_mille,
        TIERS[1].per_mille,
        TIERS[2].per_mille,
        TIERS[3].per_mille,
    ],
    card: cards::ODDS,
    credits: cards::CREDITS,
    shader: [300, 400, 450, 500],
    shader_tier: [
        [750, 220, 30, 0, 0],
        [450, 380, 150, 20, 0],
        [200, 300, 350, 130, 20],
        [50, 150, 350, 330, 120],
    ],
};

static HUB: Mutex<Option<Table>> = Mutex::new(None);

/// The odds to show: the hub's last tables that fit, else [`BUILT_IN`].
pub(crate) fn current() -> Table {
    HUB.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .unwrap_or(BUILT_IN)
}

/// Take the tables the hub sent last (`Snapshot::loot`); nothing changes when it sent
/// none or tables this client cannot show.
pub(crate) fn follow(tiers: &[sjk_identity::LootTier]) {
    if tiers.is_empty() {
        return;
    }
    let table = of_hub(tiers);
    let mut hub = HUB.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if table.is_none() && hub.is_none() {
        // Said once: the copy stays.
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| {
            eprintln!("holocron odds: the hub's tables do not fit; showing this client's copy")
        });
    }
    if table.is_some() {
        *hub = table;
    }
}

/// The hub's tables as this client shows them, if they fit.
fn of_hub(tiers: &[sjk_identity::LootTier]) -> Option<Table> {
    if tiers.len() != COUNT {
        return None;
    }
    let whole = |shares: &[u32]| shares.iter().sum::<u32>() == 1_000;
    let mut table = BUILT_IN;
    for (index, tier) in tiers.iter().enumerate() {
        let fits = tier.tier == TIERS[index].id
            && tier.rarity.len() == RARITIES.len()
            && tier.finish.len() == FINISHES.len()
            && tier.shader_tier.len() == SHADER_TIERS
            && whole(&tier.rarity)
            && whole(&tier.finish)
            && whole(&tier.shader_tier)
            && tier.shader <= 1_000
            && tier.credits[0] <= tier.credits[1];
        if !fits {
            return None;
        }
        table.drop[index] = u16::try_from(tier.drop).ok()?;
        for (to, from) in table.card[index].rarity.iter_mut().zip(&tier.rarity) {
            *to = u16::try_from(*from).ok()?;
        }
        for (to, from) in table.card[index].finish.iter_mut().zip(&tier.finish) {
            *to = u16::try_from(*from).ok()?;
        }
        table.credits[index] = (
            u32::try_from(tier.credits[0]).ok()?,
            u32::try_from(tier.credits[1]).ok()?,
        );
        table.shader[index] = tier.shader;
        table.shader_tier[index].copy_from_slice(&tier.shader_tier);
    }
    (table.drop.iter().map(|&d| u32::from(d)).sum::<u32>() == 1_000).then_some(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hub's answer as it sends the built-in tables.
    fn wire(table: &Table) -> Vec<sjk_identity::LootTier> {
        (0..COUNT)
            .map(|index| sjk_identity::LootTier {
                tier: TIERS[index].id.to_owned(),
                drop: u32::from(table.drop[index]),
                rarity: table.card[index]
                    .rarity
                    .iter()
                    .map(|&r| u32::from(r))
                    .collect(),
                finish: table.card[index]
                    .finish
                    .iter()
                    .map(|&f| u32::from(f))
                    .collect(),
                credits: [
                    u64::from(table.credits[index].0),
                    u64::from(table.credits[index].1),
                ],
                shader: table.shader[index],
                shader_tier: table.shader_tier[index].to_vec(),
            })
            .collect()
    }

    #[test]
    fn the_built_in_copy_is_whole() {
        assert_eq!(
            BUILT_IN.drop.iter().map(|&d| u32::from(d)).sum::<u32>(),
            1_000
        );
        for index in 0..COUNT {
            assert_eq!(BUILT_IN.shader_tier[index].iter().sum::<u32>(), 1_000);
            assert!(BUILT_IN.shader[index] <= 1_000);
        }
        assert_eq!(of_hub(&wire(&BUILT_IN)), Some(BUILT_IN));
    }

    #[test]
    fn the_hubs_tables_replace_the_copy_only_when_they_fit() {
        let mut changed = BUILT_IN;
        changed.shader[0] = 100;
        changed.credits[3] = (300, 400);
        changed.card[1].rarity = [400, 400, 180, 20];
        assert_eq!(of_hub(&wire(&changed)), Some(changed));
        // Another tier order, a missing finish, shares not summing to a thousand.
        let mut swapped = wire(&changed);
        swapped.swap(0, 1);
        assert_eq!(of_hub(&swapped), None);
        let mut short = wire(&changed);
        short[2].finish.pop();
        assert_eq!(of_hub(&short), None);
        let mut uneven = wire(&changed);
        uneven[3].shader_tier[0] += 1;
        assert_eq!(of_hub(&uneven), None);
        assert_eq!(of_hub(&wire(&changed)[..3]), None);
    }
}

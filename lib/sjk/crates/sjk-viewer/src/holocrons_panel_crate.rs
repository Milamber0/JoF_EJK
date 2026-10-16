//! The opening's crate spin (`docs/holocrons.md`, "Opening"): before a holocron's burst, a
//! strip of tiles runs past a gold marker and slows to a stop on what the hub rolled, as a
//! loot crate does. The hub has rolled already: the strip only dresses it, so the winner
//! is the real prize, put at a fixed place of a strip of made-up neighbours, and the stop
//! lands somewhere inside its tile (never always dead centre).
//!
//! The neighbours are what the holocron's tier can drop and nothing else: the cards it can
//! hold (each rarity and finish with odds above zero) and the droppable shaders it can
//! bring, each weighted by its chance, by the hub's odds ([`crate::loot_odds`]); the hub
//! decides, this only dresses it.

use crate::cards::{FINISHES, RARITIES};
use crate::unlockables::{self, Rarity, Unlockable};

/// Tiles in a strip, and the one the stop lands on.
pub(crate) const TILES: usize = 58;
pub(crate) const WIN: usize = 46;
/// A tile's width and the gap after it (frame pixels).
pub(crate) const TILE: f32 = 176.0;
pub(crate) const GAP: f32 = 14.0;
/// How far the stop may land from the winner's middle, as a share of a tile's half.
const JITTER: f32 = 0.7;

/// What a tile shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    /// A card of rarity and finish (positions in [`RARITIES`] and [`FINISHES`]).
    Card { rarity: usize, finish: usize },
    /// A saber shader.
    Shader(&'static Unlockable),
    /// A saber shader newer than this client (only ever the prize).
    New,
}

/// The shaders an opening can bring from shader tier `tier`: its droppable ones, else
/// those of the nearest commoner tier that has any, else the commonest above (the hub's
/// `loot::pick_shader`).
fn shaders_of(tier: Rarity) -> Vec<&'static Unlockable> {
    let of = |tier: Rarity| -> Vec<&'static Unlockable> {
        unlockables::blade_skins()
            .filter(|skin| skin.droppable && skin.tier == tier)
            .collect()
    };
    const ORDER: [Rarity; 5] = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Legendary,
        Rarity::Mythical,
    ];
    let at = ORDER.iter().position(|&t| t == tier).unwrap_or(0);
    for below in (0..=at).rev() {
        let pool = of(ORDER[below]);
        if !pool.is_empty() {
            return pool;
        }
    }
    ORDER[at + 1..]
        .iter()
        .map(|&tier| of(tier))
        .find(|pool| !pool.is_empty())
        .unwrap_or_default()
}

/// Everything a holocron of tier `tier` (a position in [`crate::holocrons::TIERS`]) can
/// bring, each with its weight (parts of a thousand million).
pub(crate) fn pool(tier: usize) -> Vec<(Item, u64)> {
    let table = crate::loot_odds::current();
    let shader = u64::from(table.shader[tier]);
    let odds = &table.card[tier];
    let mut pool = Vec::new();
    for rarity in 0..RARITIES.len() {
        for finish in 0..FINISHES.len() {
            let weight =
                (1_000 - shader) * u64::from(odds.rarity[rarity]) * u64::from(odds.finish[finish]);
            if weight > 0 {
                pool.push((Item::Card { rarity, finish }, weight));
            }
        }
    }
    let tiers = [
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Legendary,
        Rarity::Mythical,
    ];
    for (at, share) in table.shader_tier[tier].iter().enumerate() {
        let skins = shaders_of(tiers[at]);
        if *share == 0 || skins.is_empty() {
            continue;
        }
        let weight = shader * u64::from(*share) * 1_000 / skins.len() as u64;
        for skin in skins {
            // A shader reached by a fallback adds to its tier's weight.
            match pool
                .iter_mut()
                .find(|(item, _)| *item == Item::Shader(skin))
            {
                Some((_, sum)) => *sum += weight,
                None => pool.push((Item::Shader(skin), weight)),
            }
        }
    }
    pool
}

/// A small deterministic generator (splitmix64): the same holocron always spins the same
/// strip.
struct Mix(u64);

impl Mix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0 up to but not including 1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// The strip of a holocron of tier `tier` numbered `holocron` that ends on `prize`: made-up
/// neighbours picked by the tier's odds and the prize in place [`WIN`].
pub(crate) fn strip(tier: usize, holocron: u64, prize: Item) -> Vec<Item> {
    let pool = pool(tier);
    let total: u64 = pool.iter().map(|(_, weight)| weight).sum();
    let mut mix = Mix(holocron ^ 0x5EED_C4A7_E000_0000);
    let mut tiles: Vec<Item> = (0..TILES)
        .map(|_| {
            let mut roll = mix.next() % total.max(1);
            for (item, weight) in &pool {
                if roll < *weight {
                    return *item;
                }
                roll -= weight;
            }
            pool[0].0
        })
        .collect();
    tiles[WIN] = prize;
    tiles
}

/// Where the stop lands inside the winner's tile, as a share of the tile's width from its
/// middle (-0.35 to 0.35): the same for the same holocron.
pub(crate) fn landing(holocron: u64) -> f32 {
    let mut mix = Mix(holocron.wrapping_mul(31) ^ 0xA11C_E000);
    (mix.unit() * 2.0 - 1.0) * JITTER * 0.5
}

/// How far of the whole run the strip has gone at `t` (0 to 1 through the spin): quick
/// off the mark, then slowing without a lurch to a standstill.
pub(crate) fn travelled(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(4)
}

/// The distance the whole run covers (frame pixels), from tile 0 at the marker to the
/// stop inside the winner's tile.
pub(crate) fn run(holocron: u64) -> f32 {
    WIN as f32 * (TILE + GAP) + landing(holocron) * TILE
}

/// The tile under the marker after `distance` of the run.
pub(crate) fn under_marker(distance: f32) -> usize {
    ((distance / (TILE + GAP)) + 0.5).floor().max(0.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards;
    use crate::holocrons::{COUNT, TIERS};

    #[test]
    fn the_shader_odds_rise_with_the_tier() {
        let table = crate::loot_odds::current();
        assert!(table.shader.windows(2).all(|pair| pair[0] <= pair[1]));
        for (odds, tier) in table.shader_tier.iter().zip(&TIERS) {
            assert_eq!(odds.iter().sum::<u32>(), 1_000, "{}", tier.id);
        }
    }

    #[test]
    fn a_pool_holds_only_what_the_tier_can_drop() {
        for tier in 0..COUNT {
            let pool = pool(tier);
            assert!(!pool.is_empty());
            for (item, weight) in &pool {
                assert!(*weight > 0);
                match item {
                    Item::Card { rarity, finish } => {
                        assert!(cards::ODDS[tier].rarity[*rarity] > 0);
                        assert!(cards::ODDS[tier].finish[*finish] > 0);
                    }
                    Item::Shader(skin) => assert!(skin.droppable, "{}", skin.id),
                    Item::New => panic!("only a prize is new"),
                }
            }
        }
        // A Solar card cannot come out of an Uncommon holocron; a Common shader can.
        let uncommon = pool(0);
        assert!(
            !uncommon
                .iter()
                .any(|(item, _)| matches!(item, Item::Card { finish: 2, .. }))
        );
        assert!(
            uncommon
                .iter()
                .any(|(item, _)| matches!(item, Item::Shader(s) if s.id == "saber_tidal"))
        );
        // Nothing the holocrons never drop, whatever the tier: the team's shaders.
        for tier in 0..COUNT {
            assert!(
                !pool(tier)
                    .iter()
                    .any(|(item, _)| matches!(item, Item::Shader(s) if s.id == "saber_sun"))
            );
        }
    }

    #[test]
    fn a_shader_chance_adds_up_to_its_tables_share() {
        for (tier, odds) in crate::loot_odds::current().shader.iter().enumerate() {
            let pool = pool(tier);
            let shaders: u64 = pool
                .iter()
                .filter(|(item, _)| matches!(item, Item::Shader(_)))
                .map(|(_, weight)| weight)
                .sum();
            let all: u64 = pool.iter().map(|(_, weight)| weight).sum();
            let share = shaders as f64 / all as f64 * 1_000.0;
            assert!((share - f64::from(*odds)).abs() < 1.0, "{share}");
        }
    }

    #[test]
    fn a_strip_ends_on_its_prize_and_is_the_same_each_time() {
        let prize = Item::Card {
            rarity: 3,
            finish: 1,
        };
        let strip_a = strip(2, 17, prize);
        assert_eq!(strip_a.len(), TILES);
        assert_eq!(strip_a[WIN], prize);
        assert_eq!(strip_a, strip(2, 17, prize));
        assert_ne!(strip_a, strip(2, 18, prize));
    }

    #[test]
    fn the_run_slows_to_a_stop_inside_the_winning_tile() {
        assert_eq!(travelled(0.0), 0.0);
        assert_eq!(travelled(1.0), 1.0);
        // Slowing: each step covers less than the one before.
        let steps: Vec<f32> = (0..10)
            .map(|i| travelled((i + 1) as f32 / 10.0) - travelled(i as f32 / 10.0))
            .collect();
        assert!(steps.windows(2).all(|pair| pair[0] > pair[1]));
        for holocron in 0..50 {
            let landing = landing(holocron);
            assert!(landing.abs() <= JITTER * 0.5 + 1e-6);
            assert_eq!(under_marker(run(holocron)), WIN, "holocron {holocron}");
        }
    }
}

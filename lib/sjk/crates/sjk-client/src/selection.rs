//! Stock local Force/holdable selection (codemp/cgame/cg_main.c:2720-2867),
//! walking JoF EJK's Force wheel ([`crate::force_wheel`]).
use crate::force_wheel;
use sjk_protocol::PlayerState;

/// Stock display/cycle order, codemp/game/bg_misc.c:200-220.
pub const FORCE_ORDER: [u8; 18] = [5, 0, 10, 9, 11, 1, 2, 3, 4, 14, 7, 13, 8, 6, 12, 15, 16, 17];
/// WEAPON_SELECT_TIME, codemp/cgame/cg_local.h:50.
pub const SELECT_MS: i32 = 1400;
const FORCE_MASK: u32 = ((1 << 18) - 1) & !((1 << 1) | (7 << 15));
const ITEM_MASK: u32 = ((1 << 12) - 2) & !((1 << 7) | (1 << 8) | (1 << 9));

/// Local overrides, deliberately independent of snapshot reconciliation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Selection {
    /// None uses the server's selected force power.
    pub force: Option<u8>,
    /// None sends stock's -1 sentinel, leaving the server item unchanged.
    pub inventory: Option<u8>,
    /// A selected JoF pseudo-slot ([`force_wheel::STASIS`] and the others): shown
    /// and used by `+useforce`, never sent, kept while the server grants it.
    pseudo: Option<u8>,
    force_time: Option<i32>,
    item_time: Option<i32>,
}

/// Read-only selector projection for a renderer, with no strings or allocations.
#[derive(Clone, Copy, Debug)]
pub struct SelectionView {
    /// True for the inventory selector, false for Force.
    pub inventory: bool,
    /// Available selectable power/item tags.
    pub available: u32,
    /// The highlighted tag.
    pub selected: u8,
    /// UI fade within the stock 1400 ms lifetime.
    pub alpha: f32,
}

impl Selection {
    /// Cycle locally; no snapshot, spectator and follow states cannot select.
    pub fn cycle(
        &mut self,
        player: Option<&PlayerState>,
        time: i32,
        inventory: bool,
        direction: i8,
        use_held: bool,
    ) {
        let Some(player) = player else { return };
        if player.movement_type() == 4 || player.movement_flags() & 4096 != 0 {
            return;
        }
        self.sync(player, time);
        let known = self.known(player);
        // CG_NoUseableForce checks known bits only, not energy or force levels;
        // JoF EJK counts the granted pseudo-slots as usable.
        if inventory || use_held || known & (FORCE_MASK | force_wheel::PSEUDO_MASK) == 0 {
            let current = self.inventory.unwrap_or_else(|| item_tag(player));
            let mut next = current as i32;
            // BG_CycleInven's bounded scan includes the empty initial selection.
            for _ in 0..32 {
                next += if direction > 0 { 1 } else { -1 };
                if next <= 0 {
                    next = 11;
                }
                if next >= 12 {
                    next = 1;
                }
                if next == current as i32 {
                    break;
                }
                if player.stats[2] & ITEM_MASK & (1 << next) != 0 {
                    self.inventory = Some(next as u8);
                    break;
                }
            }
            if self.inventory.is_none() && current != 0 {
                self.inventory = Some(current);
            }
            if self.inventory.is_some() {
                self.item_time = Some(time);
            }
        } else {
            let current = self
                .pseudo
                .unwrap_or_else(|| self.force.unwrap_or_else(|| player.selected_force_power()));
            // JoF EJK's CG_NextForcePower_f walks its wheel, pseudo-slots included.
            if let Some(next) = force_wheel::step(known, current, direction) {
                if force_wheel::is_pseudo(next) {
                    self.pseudo = Some(next);
                } else {
                    self.pseudo = None;
                    self.force = Some(next);
                }
                self.force_time = Some(time);
            } else if let Some(index) = FORCE_ORDER.iter().position(|&p| p == current) {
                // A selection that is not on the wheel: stock scans on from its place.
                self.pseudo = None;
                for step in 1..18 {
                    let offset = if direction > 0 { step } else { 18 - step };
                    let next = FORCE_ORDER[(index + offset) % 18];
                    if known & FORCE_MASK & (1 << next) != 0 {
                        self.force = Some(next);
                        break;
                    }
                }
                if known & (1 << self.force.unwrap_or(current)) != 0 {
                    self.force = Some(self.force.unwrap_or(current));
                    self.force_time = Some(time);
                }
            }
            if self.force_time != Some(time) {
                // Only pseudo-slots granted: JoF EJK starts at the wheel's first entry.
                let (slots, count) = force_wheel::build(known);
                if count > 0 && force_wheel::is_pseudo(slots[0]) {
                    self.pseudo = Some(slots[0]);
                    self.force_time = Some(time);
                }
            }
        }
    }

    /// Select Force wheel entry `slot` at once (`forceselect`, SJK's quick wheel
    /// Force page): a real power becomes the selection sent, a pseudo-slot the one
    /// `+useforce` uses, as if `forcenext` had stopped on it. Nothing changes, and
    /// false is returned, for an entry not on the player's wheel or while
    /// spectating or following, where cycling cannot select either.
    pub fn select(&mut self, player: &PlayerState, time: i32, slot: u8) -> bool {
        if player.movement_type() == 4 || player.movement_flags() & 4096 != 0 {
            return false;
        }
        self.sync(player, time);
        if !force_wheel::valid(self.known(player), slot) {
            return false;
        }
        if force_wheel::is_pseudo(slot) {
            self.pseudo = Some(slot);
        } else {
            self.pseudo = None;
            self.force = Some(slot);
        }
        self.force_time = Some(time);
        true
    }

    /// The Force wheel entry selected now: a pseudo-slot, else the local
    /// selection, else the server's.
    pub fn selected_force(&self, player: &PlayerState) -> u8 {
        self.pseudo
            .unwrap_or_else(|| self.force.unwrap_or_else(|| player.selected_force_power()))
    }

    /// The selected JoF pseudo-slot, which `+useforce` uses instead of a power.
    pub fn wheel_pseudo(&self) -> Option<u8> {
        self.pseudo
    }

    /// The player's `forcePowersKnown` as the wheel reads it: the server's bits.
    pub fn known(&self, player: &PlayerState) -> u32 {
        player.raw_field(51).unwrap_or(0)
    }

    /// Reset expired Force overrides and consumed items to authoritative selection.
    /// A pseudo-slot outlives the selector, as in JoF EJK, until the server revokes it.
    pub fn sync(&mut self, player: &PlayerState, time: i32) {
        let known = self.known(player);
        if self
            .pseudo
            .is_some_and(|slot| !force_wheel::valid(known, slot))
        {
            self.pseudo = None;
        }
        if self
            .force_time
            .is_some_and(|at| time < at || time - at > SELECT_MS)
        {
            self.force = None;
            self.force_time = None;
        }
        if self
            .inventory
            .is_some_and(|tag| player.stats[2] & (1 << tag) == 0)
            || self.item_time.is_some_and(|at| time < at)
        {
            self.inventory = None;
            self.item_time = None;
        }
    }

    /// Most recently cycled selector; only visible alive and outside spectator/follow.
    pub fn view(&self, player: &PlayerState, time: i32) -> Option<SelectionView> {
        if player.health() <= 0
            || player.movement_type() == 4
            || player.movement_flags() & 4096 != 0
        {
            return None;
        }
        let inventory = self.item_time > self.force_time;
        let at = if inventory {
            self.item_time
        } else {
            self.force_time
        }?;
        let age = time - at;
        if !(0..=SELECT_MS).contains(&age) {
            return None;
        }
        let view = SelectionView {
            inventory,
            available: if inventory {
                player.stats[2] & ITEM_MASK
            } else {
                self.known(player) & (FORCE_MASK | force_wheel::PSEUDO_MASK)
            },
            selected: if inventory {
                self.inventory.unwrap_or_else(|| item_tag(player))
            } else {
                self.pseudo
                    .unwrap_or_else(|| self.force.unwrap_or_else(|| player.selected_force_power()))
            },
            alpha: ((SELECT_MS - age) as f32 / 300.0).clamp(0.0, 1.0),
        };
        (view.selected < 32 && view.available & (1 << view.selected) != 0).then_some(view)
    }
}

fn item_tag(player: &PlayerState) -> u8 {
    // bg_itemlist's holdables, codemp/game/bg_misc.c:795-1003 (index = tag + 3).
    if (4..=14).contains(&player.stats[1]) {
        (player.stats[1] - 3) as u8
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::force_wheel::{REPULSE, STASIS};

    fn player(known: u32, selected: u8) -> PlayerState {
        let mut player = PlayerState::default();
        player.set_raw_field(51, known);
        player.set_raw_field(54, u32::from(selected));
        player.stats[0] = 100;
        player
    }

    #[test]
    fn cycling_walks_the_wheel_through_pseudo_slots() {
        // Heal and Push known, Stasis granted: the wheel is Heal, Push, Stasis.
        let player = player(1 | (1 << 3) | (1 << STASIS), 0);
        let mut selection = Selection::default();
        selection.cycle(Some(&player), 100, false, 1, false);
        assert_eq!((selection.force, selection.wheel_pseudo()), (Some(3), None));
        selection.cycle(Some(&player), 110, false, 1, false);
        // Stasis is shown and used, but Push stays the selection that is sent.
        assert_eq!(
            (selection.force, selection.wheel_pseudo()),
            (Some(3), Some(STASIS))
        );
        assert_eq!(selection.view(&player, 120).unwrap().selected, STASIS);
        selection.cycle(Some(&player), 130, false, 1, false);
        assert_eq!((selection.force, selection.wheel_pseudo()), (Some(0), None));
        selection.cycle(Some(&player), 140, false, -1, false);
        assert_eq!(selection.wheel_pseudo(), Some(STASIS));
    }

    #[test]
    fn a_pseudo_slot_outlives_the_selector_until_revoked() {
        let granted = player((1 << 3) | (1 << REPULSE), 3);
        let mut selection = Selection::default();
        selection.cycle(Some(&granted), 0, false, 1, false);
        assert_eq!(selection.wheel_pseudo(), Some(REPULSE));
        selection.sync(&granted, SELECT_MS + 10);
        assert_eq!(selection.wheel_pseudo(), Some(REPULSE));
        selection.sync(&player(1 << 3, 3), SELECT_MS + 20);
        assert_eq!(selection.wheel_pseudo(), None);
    }

    #[test]
    fn granted_pseudo_slots_alone_are_usable_force() {
        let only = player(1 << REPULSE, 0);
        let mut selection = Selection::default();
        selection.cycle(Some(&only), 0, false, 1, false);
        assert_eq!(selection.wheel_pseudo(), Some(REPULSE));
        assert_eq!(selection.inventory, None);
    }

    #[test]
    fn the_selector_never_offers_a_toy_or_a_bit_nothing_grants() {
        // Slot 21 was Illuminate's: set by a server or not, it is no entry now, and
        // a player with nothing else known gets no Force selector at all.
        let only = player(1 << 21, 21);
        let mut selection = Selection::default();
        selection.cycle(Some(&only), 0, false, 1, false);
        assert_eq!((selection.wheel_pseudo(), selection.force), (None, None));
        assert!(!selection.select(&only, 10, 21));
        assert!(selection.view(&only, 20).is_none());
        // With real powers known, the selector's bits stay the powers and JoF's.
        let both = player((1 << 3) | (1 << 21), 3);
        selection.cycle(Some(&both), 100, false, 1, false);
        let view = selection.view(&both, 110).unwrap();
        assert_eq!(view.available, 1 << 3);
        assert_eq!(selection.known(&both) & force_wheel::PSEUDO_MASK, 0);
    }

    #[test]
    fn a_gun_server_without_force_leaves_the_keys_on_the_inventory() {
        // No Force at all: forcenext walks the items, as in stock.
        let mut gunner = player(0, 0);
        gunner.stats[2] = 1 << 3;
        let mut selection = Selection::default();
        selection.cycle(Some(&gunner), 0, false, 1, false);
        assert_eq!(selection.wheel_pseudo(), None);
        assert_eq!(selection.inventory, Some(3));
    }

    #[test]
    fn selecting_an_entry_directly_follows_the_wheel() {
        // Heal, Push and Grip known, Repulse granted, the server on Heal.
        let player = player(1 | (1 << 3) | (1 << 6) | (1 << REPULSE), 0);
        let mut selection = Selection::default();
        assert_eq!(selection.selected_force(&player), 0);
        assert!(selection.select(&player, 100, 6));
        assert_eq!((selection.force, selection.wheel_pseudo()), (Some(6), None));
        assert_eq!(selection.view(&player, 110).unwrap().selected, 6);
        // A pseudo-slot is used by +useforce; the power sent stays.
        assert!(selection.select(&player, 120, REPULSE));
        assert_eq!(
            (selection.force, selection.wheel_pseudo()),
            (Some(6), Some(REPULSE))
        );
        assert_eq!(selection.selected_force(&player), REPULSE);
        assert!(selection.select(&player, 130, 3));
        assert_eq!(selection.selected_force(&player), 3);
        // Off the wheel: unknown, Jump, and slot 21, which nothing grants.
        assert!(!selection.select(&player, 140, 4));
        assert!(!selection.select(&player, 140, 1));
        assert!(!selection.select(&player, 140, 21));
        assert_eq!(selection.selected_force(&player), 3);
        // A spectator selects nothing.
        let mut spectator = player.clone();
        spectator.set_raw_field(63, 4);
        assert!(!Selection::default().select(&spectator, 0, 3));
    }
}

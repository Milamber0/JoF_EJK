//! BaseJKA weapon-selection policy used by binds, HUD selection and usercmds.
//!
//! This mirrors `CG_WeaponSelectable`, `CG_NextWeapon_f`, `CG_PrevWeapon_f`,
//! and `CG_Weapon_f` in `codemp/cgame/cg_weapons.c:1054-1080,
//! 1382-1507,1514-1636`. In particular, cycling uses the non-numeric
//! Flechette -> Concussion -> Rocket and Det Pack -> Bryar Old order, requires
//! ownership and enough ammo for at least one firing mode, and preserves the
//! planted-detpack exception. [`legacy_melee_weapon`] is SJK's own `weapmelee`: the
//! fists by name, which retail reaches only by cycling.

use crate::{LEGACY_WEAPON_COUNT, legacy_weapon_data};
use sjk_protocol::PlayerState;

const WEAPON_COUNT: u8 = LEGACY_WEAPON_COUNT as u8;
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_DET_PACK: u8 = 14;
const WP_CONCUSSION: u8 = 15;
const WP_BRYAR_OLD: u8 = 16;

/// Fixed-size inputs read by codemp's weapon selection functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyWeaponInventory {
    /// `STAT_WEAPONS` bitset.
    pub owned: u32,
    /// `playerState_t::ammo` pools.
    pub ammo: [u32; 16],
    /// Whether a detpack exists in the world even when inventory ammo is zero.
    pub detpack_planted: bool,
    /// `PMF_FOLLOW` prevents switching.
    pub following: bool,
    /// `PM_SPECTATOR` prevents next/previous switching.
    pub spectator: bool,
    /// An emplaced gun owns selection while non-zero.
    pub emplaced: bool,
}

impl LegacyWeaponInventory {
    /// Project the relevant fields without modifying the protocol codec.
    pub fn from_player_state(player: &PlayerState) -> Self {
        Self {
            owned: player.stats[4],
            ammo: player.ammo,
            detpack_planted: player.has_detpack_planted(),
            following: player.movement_flags() & 4096 != 0,
            spectator: player.movement_type() == 4,
            emplaced: player.emplaced_index() != 0,
        }
    }
}

/// Apply `CG_WeaponSelectable` to one actual `weapon_t` value.
pub fn legacy_weapon_selectable(inventory: &LegacyWeaponInventory, weapon: u8) -> bool {
    let Some(data) = legacy_weapon_data(weapon) else {
        return false;
    };
    if weapon == 0 || inventory.owned & (1_u32 << weapon) == 0 {
        return false;
    }
    let ammo = inventory.ammo[data.ammo_index];
    if ammo < data.primary_cost as u32 && ammo < data.alternate_cost as u32 {
        return false;
    }
    weapon != WP_DET_PACK || ammo >= 1 || inventory.detpack_planted
}

/// Select next/previous weapon using codemp's deliberately non-numeric order.
pub fn legacy_cycle_weapon(inventory: &LegacyWeaponInventory, current: u8, direction: i8) -> u8 {
    if inventory.following || inventory.spectator || inventory.emplaced || direction == 0 {
        return current;
    }
    let original = current;
    let mut selected = current;
    for _ in 0..WEAPON_COUNT {
        selected = if direction > 0 {
            match selected {
                WP_FLECHETTE => WP_CONCUSSION,
                WP_CONCUSSION => WP_ROCKET,
                WP_DET_PACK => WP_BRYAR_OLD,
                value => (value + 1) % WEAPON_COUNT,
            }
        } else {
            match selected {
                WP_ROCKET => WP_CONCUSSION,
                WP_CONCUSSION => WP_FLECHETTE,
                WP_BRYAR_OLD => WP_DET_PACK,
                0 => WEAPON_COUNT - 1,
                value => value - 1,
            }
        };
        if legacy_weapon_selectable(inventory, selected) {
            return selected;
        }
    }
    original
}

/// The weapon `weapmelee` selects, if it changes anything: the fists (`WP_MELEE`,
/// the melee `weapon 1` falls back to when the player has no saber), never the
/// saber and never a toggle. `current` is the weapon now selected. `None` while
/// following, on an emplaced gun, when `current` already is the fists or when
/// the player does not hold them.
pub fn legacy_melee_weapon(inventory: &LegacyWeaponInventory, current: u8) -> Option<u8> {
    if inventory.following || inventory.emplaced || current == WP_MELEE {
        return None;
    }
    legacy_weapon_selectable(inventory, WP_MELEE).then_some(WP_MELEE)
}

/// Apply `weapon N`'s SP-compatible slot mapping and explosive sub-cycle.
pub fn legacy_direct_weapon(
    inventory: &LegacyWeaponInventory,
    current_weapon: u8,
    slot: u8,
) -> Option<u8> {
    if inventory.following || inventory.emplaced || !(1..=WP_BRYAR_OLD).contains(&slot) {
        return None;
    }
    let mut selected = if slot == 1 {
        if inventory.owned & (1 << WP_SABER) != 0 {
            WP_SABER
        } else {
            2
        }
    } else {
        slot.saturating_add(2)
    };
    if selected > WP_BRYAR_OLD + 1 {
        return None;
    }
    if (WP_THERMAL..=WP_DET_PACK).contains(&selected) {
        let mut candidate = if (WP_THERMAL..=WP_DET_PACK).contains(&current_weapon) {
            current_weapon + 1
        } else {
            WP_THERMAL
        };
        for _ in 0..=4 {
            if candidate > WP_DET_PACK {
                candidate = WP_THERMAL;
            }
            if legacy_weapon_selectable(inventory, candidate) {
                selected = candidate;
                break;
            }
            candidate += 1;
        }
    }
    legacy_weapon_selectable(inventory, selected).then_some(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STUN_BATON: u8 = 1;
    const BRYAR: u8 = 4;

    fn inventory(weapons: &[u8]) -> LegacyWeaponInventory {
        LegacyWeaponInventory {
            owned: weapons.iter().fold(0, |owned, weapon| owned | 1 << weapon),
            ammo: [0; 16],
            detpack_planted: false,
            following: false,
            spectator: false,
            emplaced: false,
        }
    }

    #[test]
    fn weapmelee_selects_the_fists_from_the_saber_and_from_any_weapon() {
        let held = inventory(&[WP_MELEE, WP_SABER, BRYAR]);
        assert_eq!(legacy_melee_weapon(&held, WP_SABER), Some(WP_MELEE));
        assert_eq!(legacy_melee_weapon(&held, BRYAR), Some(WP_MELEE));
        // `weapon 1` is the saber while one is held: that is why it needs this.
        assert_eq!(legacy_direct_weapon(&held, BRYAR, 1), Some(WP_SABER));
        // It is the weapon retail's previous-weapon key reaches from the saber.
        assert_eq!(legacy_cycle_weapon(&held, WP_SABER, -1), WP_MELEE);
        // Without a saber `weapon 1` is the fists too.
        let no_saber = inventory(&[WP_MELEE, BRYAR]);
        assert_eq!(legacy_direct_weapon(&no_saber, BRYAR, 1), Some(WP_MELEE));
        assert_eq!(legacy_melee_weapon(&no_saber, BRYAR), Some(WP_MELEE));
    }

    #[test]
    fn weapmelee_does_nothing_when_already_on_the_fists_or_without_them() {
        let held = inventory(&[WP_MELEE, WP_SABER]);
        // Already the fists: no toggle back, no change.
        assert_eq!(legacy_melee_weapon(&held, WP_MELEE), None);
        // No fists in the inventory, or only the stun baton, which is another
        // weapon: nothing is selected and nothing breaks.
        assert_eq!(legacy_melee_weapon(&inventory(&[WP_SABER]), WP_SABER), None);
        assert_eq!(
            legacy_melee_weapon(&inventory(&[STUN_BATON, WP_SABER]), WP_SABER),
            None
        );
        assert_eq!(legacy_melee_weapon(&inventory(&[]), 0), None);
    }

    #[test]
    fn weapmelee_stays_put_while_following_or_on_an_emplaced_gun() {
        let held = inventory(&[WP_MELEE, WP_SABER]);
        let following = LegacyWeaponInventory {
            following: true,
            ..held
        };
        assert_eq!(legacy_melee_weapon(&following, WP_SABER), None);
        let emplaced = LegacyWeaponInventory {
            emplaced: true,
            ..held
        };
        assert_eq!(legacy_melee_weapon(&emplaced, WP_SABER), None);
    }
}

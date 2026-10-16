//! What a bot holds (OpenJK `codemp/game/ai_main.c`): the weapon it picks by its weights
//! and ammunition (`BotSelectIdealWeapon`, `BotSelectChoiceWeapon`, `BotTryAnotherWeapon`,
//! `BotSelectMelee`), what it knows of the weapon in hand (`BotGetWeaponRange`,
//! `BotWeaponCanLead`, `ShouldSecondaryFire`) and the holdable item it wants to use
//! (`BotUseInventoryItem`).
//!
//! A choice is made through the bot's input (`EA_SelectWeapon`) and remembered as its
//! `virtualWeapon`, so that it is not asked for twice before the switch shows.

use crate::bot_think::BotMind;
use crate::weapon_data::{LEGACY_WEAPON_COUNT, LEGACY_WEAPON_DATA};

const WP_STUN_BATON: i32 = 1;
const WP_MELEE: i32 = 2;
const WP_SABER: i32 = 3;
const WP_BRYAR_PISTOL: i32 = 4;
const WP_BLASTER: i32 = 5;
const WP_DISRUPTOR: i32 = 6;
const WP_BOWCASTER: i32 = 7;
const WP_REPEATER: i32 = 8;
const WP_DEMP2: i32 = 9;
const WP_FLECHETTE: i32 = 10;
const WP_ROCKET_LAUNCHER: i32 = 11;
const WP_THERMAL: i32 = 12;
const WP_TRIP_MINE: i32 = 13;
const WP_DET_PACK: i32 = 14;
/// `WEAPON_CHARGING_ALT`.
const WEAPON_CHARGING_ALT: i32 = 5;

/// `holdable_t`: the holdable items a bot uses by itself.
const HI_SEEKER: i32 = 1;
const HI_SHIELD: i32 = 2;
const HI_MEDPAC: i32 = 3;
const HI_MEDPAC_BIG: i32 = 4;
const HI_SENTRY_GUN: i32 = 6;

/// The enemy a bot has, as the weapon choice weighs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemySighting {
    /// `frame_Enemy_Len`: how far it was this frame.
    pub distance: f32,
    /// `frame_Enemy_Vis`.
    pub visible: bool,
    /// The weapon a client enemy holds; `None` for one that is no client.
    pub weapon: Option<i32>,
}

/// What a bot's weapon choice reads of its own state (`cur_ps`).
#[derive(Clone, Copy, Debug)]
pub struct Armoury<'a> {
    /// `ammo[]`.
    pub ammo: &'a [i32],
    /// `stats[STAT_WEAPONS]`.
    pub weapons: i32,
    /// `weapon`: the one in hand.
    pub weapon: i32,
    pub enemy: Option<EnemySighting>,
}

impl Armoury<'_> {
    /// Whether `weapon` has at least a shot's worth (or, `strictly`, more than one).
    pub(crate) fn loaded(&self, weapon: usize, strictly: bool) -> bool {
        let data = LEGACY_WEAPON_DATA[weapon];
        let ammo = self.ammo.get(data.ammo_index).copied().unwrap_or(0);
        if strictly {
            ammo > data.primary_cost
        } else {
            ammo >= data.primary_cost
        }
    }

    fn owned(&self, weapon: usize) -> bool {
        self.weapons & (1 << weapon) != 0
    }

    /// `BotWeaponSelectable`.
    fn selectable(&self, weapon: i32) -> bool {
        weapon != 0 && self.loaded(weapon as usize, false) && self.owned(weapon as usize)
    }
}

/// `BotSelectWeapon`: nothing for no weapon.
fn select(mind: &mut BotMind, weapon: i32) {
    if weapon > 0 {
        mind.input.select_weapon(weapon);
    }
}

/// `BotSelectIdealWeapon`: the heaviest weapon it has with a shot's ammunition (the
/// thermal only for an enemy under 700 units), the saber instead of a light gun for an
/// enemy under 300, a gun instead of the saber for a gunman over 300. Whether a switch
/// was asked for.
pub fn select_ideal_weapon(mind: &mut BotMind, weights: &[f32], armoury: &Armoury) -> bool {
    let (mut best_weight, mut best_weapon) = (-1_i32, 0_i32);
    for weapon in 0..LEGACY_WEAPON_COUNT {
        let weight = weights[weapon];
        // `bestweight` is an int: the float compares against it and is truncated into it.
        if armoury.loaded(weapon, false) && weight > best_weight as f32 && armoury.owned(weapon) {
            if weapon as i32 == WP_THERMAL {
                if armoury.enemy.is_some_and(|enemy| enemy.distance < 700.0) {
                    (best_weight, best_weapon) = (weight as i32, weapon as i32);
                }
            } else {
                (best_weight, best_weapon) = (weight as i32, weapon as i32);
            }
        }
    }
    if let Some(enemy) = armoury.enemy {
        if enemy.distance < 300.0
            && matches!(best_weapon, WP_BRYAR_PISTOL | WP_BLASTER | WP_BOWCASTER)
            && armoury.owned(WP_SABER as usize)
        {
            (best_weapon, best_weight) = (WP_SABER, 1);
        }
        if enemy.distance > 300.0
            && enemy.weapon.is_some_and(|weapon| weapon != WP_SABER)
            && best_weapon == WP_SABER
        {
            if let Some(gun) = [
                WP_DISRUPTOR,
                WP_ROCKET_LAUNCHER,
                WP_BOWCASTER,
                WP_BLASTER,
                WP_REPEATER,
                WP_DEMP2,
            ]
            .into_iter()
            .find(|&gun| armoury.selectable(gun))
            {
                (best_weapon, best_weight) = (gun, 1);
            }
        }
    }
    if best_weight != -1 && armoury.weapon != best_weapon && mind.virtual_weapon != best_weapon {
        mind.virtual_weapon = best_weapon;
        select(mind, best_weapon);
        return true;
    }
    false
}

/// `BotSelectChoiceWeapon`: 0 without `weapon` (and more than a shot's ammunition for
/// it), 2 where it was newly asked for (`select`), 1 otherwise.
pub fn select_choice_weapon(
    mind: &mut BotMind,
    weapon: i32,
    select_it: bool,
    armoury: &Armoury,
) -> i32 {
    let has = (0..LEGACY_WEAPON_COUNT)
        .any(|index| armoury.loaded(index, true) && index as i32 == weapon && armoury.owned(index));
    if has && armoury.weapon != weapon && select_it && mind.virtual_weapon != weapon {
        mind.virtual_weapon = weapon;
        select(mind, weapon);
        return 2;
    }
    i32::from(has)
}

/// `BotTryAnotherWeapon`: out of ammunition, the first weapon it has a shot for, else the
/// stun baton. Whether a switch was asked for.
pub fn try_another_weapon(mind: &mut BotMind, armoury: &Armoury) -> bool {
    if let Some(weapon) = (1..LEGACY_WEAPON_COUNT)
        .find(|&weapon| armoury.loaded(weapon, false) && armoury.owned(weapon))
    {
        mind.virtual_weapon = weapon as i32;
        select(mind, weapon as i32);
        return true;
    }
    select_melee(mind, armoury)
}

/// `BotSelectMelee` (and `BotTryAnotherWeapon`'s last resort): the stun baton.
pub fn select_melee(mind: &mut BotMind, armoury: &Armoury) -> bool {
    if armoury.weapon != WP_STUN_BATON && mind.virtual_weapon != WP_STUN_BATON {
        mind.virtual_weapon = WP_STUN_BATON;
        select(mind, WP_STUN_BATON);
        return true;
    }
    false
}

/// `BotUseInventoryItem`: the holdable item (by its `holdable_t`) the bot wants to use —
/// a medpack when hurt, a seeker or sentry at a visible enemy, the shield while it
/// runs from a threat it sees. The caller makes it the held item
/// (`BG_GetItemIndexByTag`) and presses use at random.
pub fn wanted_item(
    holdables: i32,
    health: i32,
    enemy: Option<EnemySighting>,
    escaping: bool,
) -> Option<i32> {
    let has = |item: i32| holdables & (1 << item) != 0;
    let sees_enemy = enemy.is_some_and(|enemy| enemy.visible);
    if has(HI_MEDPAC) && health <= 75 {
        return Some(HI_MEDPAC);
    }
    if has(HI_MEDPAC_BIG) && health <= 50 {
        return Some(HI_MEDPAC_BIG);
    }
    if has(HI_SEEKER) && sees_enemy {
        return Some(HI_SEEKER);
    }
    if has(HI_SENTRY_GUN) && sees_enemy {
        return Some(HI_SENTRY_GUN);
    }
    if has(HI_SHIELD) && sees_enemy && escaping {
        return Some(HI_SHIELD);
    }
    None
}

/// `BWEAPONRANGE_*`: how close a weapon wants its enemy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponRange {
    Melee = 1,
    Mid = 2,
    Long = 3,
    Saber = 4,
}

/// `BotGetWeaponRange` for the weapon in hand.
pub fn weapon_range(weapon: i32) -> WeaponRange {
    match weapon {
        WP_STUN_BATON | WP_MELEE => WeaponRange::Melee,
        WP_SABER => WeaponRange::Saber,
        WP_BOWCASTER | WP_DEMP2 | WP_FLECHETTE | WP_ROCKET_LAUNCHER | WP_THERMAL | WP_TRIP_MINE
        | WP_DET_PACK => WeaponRange::Long,
        _ => WeaponRange::Mid,
    }
}

/// `BotWeaponCanLead`: how far ahead of a moving enemy the weapon in hand is aimed.
pub fn lead_factor(weapon: i32) -> f32 {
    match weapon {
        WP_BRYAR_PISTOL | WP_BOWCASTER | WP_THERMAL => 0.5,
        WP_BLASTER | WP_DEMP2 => 0.35,
        WP_REPEATER => 0.45,
        WP_ROCKET_LAUNCHER => 0.7,
        _ => 0.0,
    }
}

/// What `ShouldSecondaryFire` reads of the bot's weapon.
#[derive(Clone, Copy, Debug)]
pub struct AltFireState<'a> {
    pub ammo: &'a [i32],
    pub weapon: i32,
    /// `weaponstate`.
    pub weapon_state: i32,
    pub weapon_charge_time: i32,
    pub rocket_lock_time: f32,
    pub rocket_last_valid_time: f32,
    /// `altChargeTime`: how long the bot means to charge.
    pub alt_charge_time: i32,
    /// `frame_Enemy_Len`.
    pub enemy_distance: f32,
    pub level_time: i32,
}

/// `ShouldSecondaryFire`: 0 not at all, 1 fire (or keep charging) the alternate fire, 2
/// let a charge go — by the weapon, the enemy's distance and the charge (the rockets'
/// lock).
pub fn should_secondary_fire(state: &AltFireState) -> i32 {
    let weapon = state.weapon;
    let data = LEGACY_WEAPON_DATA[weapon.clamp(0, LEGACY_WEAPON_COUNT as i32 - 1) as usize];
    if state.ammo.get(data.ammo_index).copied().unwrap_or(0) < data.alternate_cost {
        return 0;
    }
    let charging = state.weapon_state == WEAPON_CHARGING_ALT;
    if charging && weapon == WP_ROCKET_LAUNCHER {
        let held = (state.level_time - state.weapon_charge_time) as f32;
        let mut lock = state.rocket_lock_time;
        if lock < 1.0 {
            lock = state.rocket_last_valid_time;
        }
        if held > 5000.0 {
            return 2;
        }
        if lock > 0.0 {
            let steps = ((state.level_time as f32 - lock) / (1200.0 / 16.0)) as i32;
            if steps >= 10 {
                return 2;
            } else if state.enemy_distance > 250.0 {
                return 1;
            }
        } else if state.enemy_distance > 250.0 {
            return 1;
        }
    } else if charging && state.level_time - state.weapon_charge_time > state.alt_charge_time {
        return 2;
    } else if charging {
        return 1;
    }
    let distance = state.enemy_distance;
    let fire = match weapon {
        WP_BRYAR_PISTOL | WP_BLASTER => distance < 300.0,
        WP_BOWCASTER => distance > 300.0,
        WP_REPEATER => distance < 600.0 && distance > 250.0,
        WP_ROCKET_LAUNCHER => distance > 250.0,
        _ => false,
    };
    i32::from(fire)
}

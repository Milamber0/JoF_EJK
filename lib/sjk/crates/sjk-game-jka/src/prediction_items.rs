//! CG_TouchItem eligibility: codemp/cgame/cg_predict.c:558-675 and
//! BG_CanItemBeGrabbed, codemp/game/bg_misc.c:2068-2218.
use crate::pmove::MovementState;
use sjk_protocol::{EntityState, PlayerState};

/// Compact snapshot trigger record; copying the trigger list never clones netfield boxes.
#[derive(Clone, Copy, Debug)]
pub struct PredictionTrigger {
    pub number: u16,
    pub kind: u8,
    pub model: i16,
    pub solid: u32,
    pub flags: u32,
    pub dropped: u8,
    pub owner: u8,
    pub powerups: u32,
    pub launch: [f32; 3],
    pub base: [f32; 3],
    pub delta: [f32; 3],
    pub trajectory: u8,
    pub start: i32,
    pub duration: i32,
}

impl From<&EntityState> for PredictionTrigger {
    fn from(entity: &EntityState) -> Self {
        Self {
            number: entity.number(),
            kind: entity.entity_type(),
            model: entity.model_index(),
            solid: entity.solid(),
            flags: entity.e_flags(),
            dropped: entity.broken_limbs(),
            owner: entity.generic1(),
            powerups: entity.powerups(),
            launch: entity.origin2(),
            base: entity.trajectory_base(),
            delta: entity.trajectory_delta(),
            trajectory: entity.trajectory_type(),
            start: entity.trajectory_time(),
            duration: entity.trajectory_duration(),
        }
    }
}

/// Authoritative pickup limits, carried alongside predicted movement/inventory.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickupLimits {
    pub armor: i32,
    pub max_health: i32,
    pub holdables: u32,
    pub true_jedi: bool,
    pub jedi_master: bool,
    pub force_side: u8,
}

impl PickupLimits {
    pub(crate) fn from_player(player: &PlayerState) -> Self {
        Self {
            armor: player.armor(),
            max_health: player.max_health(),
            holdables: player.stats[2],
            true_jedi: player.is_true_jedi(),
            jedi_master: player.is_jedi_master(),
            force_side: player.force_side(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Armor,
    Health,
    Holdable(u8),
    Powerup(u8),
    Weapon(u8),
    Ammo(i8),
    Team(u8),
}

// bg_itemlist, bg_misc.c:710-1686. Tags are bg_public.h/bg_weapons.h enums.
fn item(index: i16) -> Option<Item> {
    Some(match index {
        1 | 2 => Item::Armor,
        3 => Item::Health,
        4..=14 => Item::Holdable((index - 3) as u8),
        15..=18 => Item::Powerup((index - 3) as u8),
        19..=22 => Item::Weapon((index - 18) as u8),
        23 => Item::Weapon(15),
        24 => Item::Weapon(16),
        25..=31 => Item::Weapon((index - 20) as u8),
        32..=34 => Item::Ammo((index - 25) as i8),
        35..=39 => Item::Weapon((index - 23) as u8 + if index > 37 { 2 } else { 0 }),
        40..=44 => Item::Ammo((index - 39) as i8),
        45 => Item::Ammo(-1),
        46..=48 => Item::Team((index - 42) as u8),
        49 | 50 => Item::Team(0),
        _ => return None,
    })
}

const AMMO_MAX: [i32; 10] = [0, 100, 300, 300, 300, 25, 800, 10, 10, 10];

/// Stock asymmetric proximity test, bg_misc.c:1894-1910 (not a collision trace).
pub fn touches_item(player: [f32; 3], item: [f32; 3]) -> bool {
    let delta = std::array::from_fn::<_, 3, _>(|i| player[i] - item[i]);
    (-50.0..=44.0).contains(&delta[0])
        && (-36.0..=36.0).contains(&delta[1])
        && (-36.0..=36.0).contains(&delta[2])
}

/// Combined server grab rules and cgame's stricter prediction exclusions.
pub fn can_predict_item(state: &MovementState, entity: &PredictionTrigger, gametype: u8) -> bool {
    let Some(item) = item(entity.model) else {
        return false;
    };
    if entity.dropped != 0 || entity.flags & ((1 << 8) | (1 << 23)) != 0 || state.duel_in_progress {
        return false;
    }
    let limits = &state.pickup_limits;
    if limits.true_jedi {
        if !matches!(
            item,
            Item::Team(_) | Item::Armor | Item::Weapon(3) | Item::Holdable(1)
        ) && !matches!(item, Item::Powerup(tag) if tag != 15)
        {
            return false;
        }
    } else if state.true_non_jedi
        && (matches!(item, Item::Powerup(tag) if tag != 15)
            || matches!(item, Item::Holdable(1) | Item::Weapon(3)))
    {
        return false;
    }
    if limits.jedi_master && matches!(item, Item::Weapon(_) | Item::Ammo(_)) {
        return false;
    }
    match item {
        Item::Weapon(tag) => {
            if u16::from(entity.owner) == state.client_num && entity.powerups != 0 {
                return false;
            }
            if entity.flags & (1 << 25) == 0
                && state.weapons & (1 << tag) != 0
                && !(12..=14).contains(&tag)
            {
                return false;
            }
            !(12..=14).contains(&tag) || state.ammo[usize::from(tag - 5)] < 10
        }
        Item::Ammo(tag) => tag == -1 || state.ammo[tag as usize] < AMMO_MAX[tag as usize],
        Item::Armor => limits.armor < limits.max_health,
        Item::Health => {
            state.force_powers_active & (1 << 8) == 0 && state.health < limits.max_health
        } // retail health quantity is 25.
        Item::Holdable(tag) => limits.holdables & (1 << tag) == 0,
        Item::Powerup(tag) => {
            (state.powerup_deadlines[15] == 0 || tag == 15)
                && (tag != 12 || limits.force_side == 1)
                && (tag != 13 || limits.force_side == 2)
        }
        // Cgame never predicts returning its own flag, even if dropped.
        Item::Team(tag) => {
            matches!(gametype, 8 | 9)
                && ((state.team == 1 && tag == 5) || (state.team == 2 && tag == 4))
        }
    }
}

/// CG_TouchItem's weapon/autoswitch seed, not a predicted quantity grant (:668-674).
pub(crate) fn give_weapon_seed(state: &mut MovementState, index: i16) {
    if let Some(Item::Weapon(tag)) = item(index) {
        state.weapons |= 1 << tag;
        // Stock deliberately indexes ammo by weapon tag here, not ammoIndex.
        if let Some(ammo) = state.ammo.get_mut(usize::from(tag)) {
            if *ammo == 0 {
                *ammo = 1;
            }
        }
    }
}

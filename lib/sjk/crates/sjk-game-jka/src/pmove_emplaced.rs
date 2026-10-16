//! A player at an emplaced gun in the movement (OpenJK `codemp/game/bg_pmove.c`): the
//! alternate button is the primary on a gun (`PmoveSingle`, `:10186-10193`), and
//! `PM_Weapon` (`:6660-7440`) holds the gunner's weapon on the gun, keeps it in its seated
//! pose (`BOTH_GUNSIT1`) and fires every `fireTime` (100 ms) while an attack button is
//! down — `EV_FIRE_WEAPON` or `EV_ALT_FIRE`, which the game turns into the gun's shot
//! ([`crate::emplaced::fire`]). No ammunition is counted and the weapon never enters its
//! firing state. A player whose gun was taken from it (`emplacedIndex` cleared, the
//! weapon still the gun's) is handed the first weapon it holds.
//!
//! Getting onto and off a gun, and every shot, are the game's; the movement's own rule
//! that backing off or leaving the ground lets go of the gun is
//! [`crate::pmove_input_freeze`]'s.

use crate::pmove::MovementState;
use crate::pmove_anim::{AnimationLengths, start_torso};
use crate::predicted_events::PredictedEvents;
use sjk_protocol::UserCommand;

/// `WP_NONE`, `WP_SABER`, `WP_EMPLACED_GUN`, `WP_ROCKET_LAUNCHER`.
const WP_NONE: u8 = 0;
const WP_SABER: u8 = 3;
pub(crate) const WP_EMPLACED_GUN: u8 = 17;
const WP_ROCKET_LAUNCHER: u8 = 11;
/// `WP_NUM_WEAPONS`.
const WP_NUM_WEAPONS: u8 = 19;
/// `BUTTON_ATTACK`, `BUTTON_USE_HOLDABLE`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_USE_HOLDABLE: u16 = 4;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `PMF_RESPAWNED`, `PMF_USE_ITEM_HELD`.
const PMF_RESPAWNED: u16 = 512;
const PMF_USE_ITEM_HELD: u16 = 1_024;
/// `weaponstate_t`.
const WEAPON_READY: u8 = 0;
const WEAPON_RAISING: u8 = 1;
const WEAPON_DROPPING: u8 = 2;
const WEAPON_CHARGING: u8 = 4;
const WEAPON_CHARGING_ALT: u8 = 5;
/// `MAX_WEAPON_CHARGE_TIME`.
const MAX_WEAPON_CHARGE_TIME: i32 = 5_000;
/// `TEAM_SPECTATOR`; `MAX_CLIENTS`.
const TEAM_SPECTATOR: u8 = 3;
const MAX_CLIENTS: u16 = 32;
/// `BOTH_GUNSIT1`, `TORSO_WEAPONREADY4`, `BOTH_ATTACK4`.
const BOTH_GUNSIT1: u16 = 1_014;
const TORSO_WEAPONREADY4: u16 = 1_419;
const BOTH_ATTACK4: u16 = 116;
/// `EV_FIRE_WEAPON`, `EV_ALT_FIRE`.
const EV_FIRE_WEAPON: u16 = 27;
const EV_ALT_FIRE: u16 = 28;
/// `weaponData[WP_EMPLACED_GUN].fireTime`.
const FIRE_TIME: i32 = 100;

/// Whether `PM_Weapon` is the emplaced gun's for this state: the gun in hand, whether or
/// not the player is still on it.
pub(crate) fn owns(state: &MovementState) -> bool {
    state.weapon == WP_EMPLACED_GUN && !state.duel_in_progress
}

/// `PmoveSingle`'s "hackerrific" (`bg_pmove.c:10186-10193`): on a gun, the alternate
/// button is the primary.
pub(crate) fn alternate_is_primary(state: &MovementState, command: &mut UserCommand) {
    if state.emplaced_index != 0 && command.buttons & BUTTON_ALT_ATTACK != 0 {
        command.buttons &= !BUTTON_ALT_ATTACK;
        command.buttons |= BUTTON_ATTACK;
    }
}

/// `PM_Weapon` (`bg_pmove.c:6660-7440`) for a player holding the emplaced gun's weapon
/// (see [`owns`]).
pub(crate) fn weapon(
    state: &mut MovementState,
    command: &mut UserCommand,
    millis: i32,
    lengths: Option<&dyn AnimationLengths>,
    events: &mut PredictedEvents,
) {
    // "oh no!" (`:6685-6711`): the gun was taken away; the first weapon it holds instead.
    if state.emplaced_index == 0 {
        if let Some(weapon) = (1..WP_NUM_WEAPONS).find(|weapon| state.weapons & (1 << weapon) != 0)
        {
            command.weapon = weapon;
            state.weapon = weapon;
            return;
        }
    }
    // The charge's limit (`:6737-6757`), for a charge the gun's weapon inherited.
    if state.vehicle_entity_num == 0 {
        if state.weapon_state == WEAPON_CHARGING_ALT
            && command.server_time - state.weapon_charge_time > MAX_WEAPON_CHARGE_TIME
        {
            command.buttons &= !BUTTON_ALT_ATTACK;
        }
        if state.weapon_state == WEAPON_CHARGING
            && command.server_time - state.weapon_charge_time > MAX_WEAPON_CHARGE_TIME
        {
            command.buttons &= !BUTTON_ATTACK;
        }
    }
    // The hand-extend block (`:6759-6960`).
    if crate::pmove_hand_extend::hand_extend(state, lengths) {
        return;
    }
    // A special jump or a roll keeps the weapon busy (`:6971-6989`).
    if crate::pmove_roll_anim::special_jump(state.legs_anim)
        || crate::pmove_roll::in_roll(state)
        || crate::pmove_roll::in_roll_complete(state)
    {
        state.weapon_time = state.weapon_time.max(state.legs_timer);
    }
    if state.movement_flags & PMF_RESPAWNED != 0
        || (state.client_num < MAX_CLIENTS && state.team == TEAM_SPECTATOR)
    {
        return;
    }
    if state.health <= 0 {
        state.weapon = WP_NONE;
        return;
    }
    // The holdable (`:7065-7126`): a gunner has no use for one on a gun; the rocket lock
    // goes all the same.
    if command.buttons & BUTTON_USE_HOLDABLE != 0 {
        (
            state.rocket_lock_index,
            state.rocket_lock_time,
            state.rocket_target_time,
        ) = (sjk_protocol::ENTITY_NUMBER_NONE, -1.0, 0.0);
        if state.movement_flags & PMF_USE_ITEM_HELD == 0 {
            if state.holdable_item == 0 {
                return;
            }
            // Using an item is not ported (no gun holds one): held until let go.
            state.movement_flags |= PMF_USE_ITEM_HELD;
            return;
        }
    } else {
        state.movement_flags &= !PMF_USE_ITEM_HELD;
    }
    if state.weapon_time > 0 {
        state.weapon_time -= millis;
    }
    // A duel lets go of the gun (`:7149-7153`; a Jedi Master cannot take one).
    if state.duel_in_progress && state.emplaced_index != 0 {
        state.emplaced_index = 0;
        state.saber_holstered = 0;
    }
    if state.emplaced_index != 0 {
        // "No switch for you!" (`:7155-7159`).
        command.weapon = WP_EMPLACED_GUN;
        start_torso(state, BOTH_GUNSIT1);
    }
    // A Jedi Master, a true Jedi or a duellist holds its saber (`:7161-7170`); the rest
    // of the command is the saber's, which a gunner never is (the gun refuses a Jedi
    // Master, a duel lets go of it above).
    if state.forced_saber {
        command.weapon = WP_SABER;
        state.weapon = WP_SABER;
        return;
    }
    // The empty-weapon rule (`:7172-7205`) never holds for the gun: it costs nothing.
    // A switch (`:7208-7215`) needs a command asking for another weapon, which the gun
    // forbids and the gun taken away answered above; so does `WEAPON_DROPPING`.
    if state.weapon_time > 0 || state.weapon_state == WEAPON_DROPPING {
        return;
    }
    if state.weapon_state == WEAPON_RAISING {
        // `:7256-7285`: ready, seated.
        state.weapon_state = WEAPON_READY;
        start_torso(state, BOTH_GUNSIT1);
        return;
    }
    // `:7325-7338`: out of the scoped poses, into the seat.
    if matches!(state.torso_anim, TORSO_WEAPONREADY4 | BOTH_ATTACK4) {
        start_torso(state, BOTH_GUNSIT1);
    }
    // No rocket lock but the launcher's (`:7366-7381`).
    if state.weapon != WP_ROCKET_LAUNCHER && state.vehicle_entity_num == 0 {
        (
            state.rocket_lock_index,
            state.rocket_lock_time,
            state.rocket_target_time,
        ) = (sjk_protocol::ENTITY_NUMBER_NONE, 0.0, 0.0);
    }
    if command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0 {
        state.weapon_time = 0;
        state.weapon_state = WEAPON_READY;
        return;
    }
    // `:7397-7409`: the gun's shot, every `fireTime`.
    state.weapon_time += FIRE_TIME;
    let event = if command.buttons & BUTTON_ALT_ATTACK != 0 {
        EV_ALT_FIRE
    } else {
        EV_FIRE_WEAPON
    };
    crate::pmove_weapon_charge::event(state, events, event, 0);
}

//! `PM_Weapon`'s use key (`bg_pmove.c:7061-7120`): the selected holdable used — its
//! `EV_USE_ITEM0 + tag` event, which the game's `ClientEvents` acts on — or refused, and
//! `PMF_USE_ITEM_HELD` until the key is let go. An item used up leaves the inventory and
//! the next one held is selected (`BG_CycleInven`). The rules of which item may be used
//! are [`crate::holdables`]'.

use crate::holdables::{
    EV_ITEMUSEFAIL, EV_USE_ITEM0, Holder, PMF_USE_ITEM_HELD, cycle, kept_after_use, tag_of, usable,
};
use crate::pmove::{MovementCollision, MovementState, MovementTrace};
use crate::predicted_events::PredictedEvents;
use sjk_protocol::UserCommand;

/// `BUTTON_USE_HOLDABLE`.
const BUTTON_USE_HOLDABLE: u16 = 4;

/// A world with nothing in it, for a movement run without one.
struct Open;

impl MovementCollision for Open {
    fn trace(&self, _: [f32; 3], _: [f32; 3], _: [f32; 3], end: [f32; 3], _: u32) -> MovementTrace {
        MovementTrace::miss(end)
    }
}

/// The use key's branch, after `PM_Weapon`'s respawn, spectator and death checks. Returns
/// whether `PM_Weapon` returns here (the key pressed anew: used, refused or nothing to
/// use); held on, or let go, the weapon's code goes on.
pub(crate) fn use_key(
    state: &mut MovementState,
    command: &UserCommand,
    events: &mut PredictedEvents,
    collision: Option<&dyn MovementCollision>,
) -> bool {
    if command.buttons & BUTTON_USE_HOLDABLE == 0 {
        state.movement_flags &= !PMF_USE_ITEM_HELD;
        return false;
    }
    // "fix: rocket lock bug, one of many..." (`BG_ClearRocketLock`).
    state.rocket_lock_index = sjk_protocol::ENTITY_NUMBER_NONE;
    state.rocket_last_valid_time = 0.0;
    state.rocket_lock_time = -1.0;
    state.rocket_target_time = 0.0;
    if state.movement_flags & PMF_USE_ITEM_HELD != 0 {
        return false;
    }
    // Riding a vehicle, the key links and unlinks its weapons instead.
    if state.vehicle_entity_num != 0 || state.holdable_item == 0 {
        return true;
    }
    let tag = tag_of(state.holdable_item);
    let holder = Holder {
        client: state.client_num,
        origin: state.origin,
        view_angles: state.view_angles,
        health: state.health,
        max_health: state.max_health,
        entity_flags: state.entity_flags,
        movement_flags: state.movement_flags,
        riding: state.vehicle_entity_num != 0,
        duel_in_progress: state.duel_in_progress,
        sentry_deployed: state.sentry_deployed,
    };
    match usable(&holder, tag, true, collision.unwrap_or(&Open)) {
        Err(fail) => {
            if let Some(fail) = fail {
                crate::pmove_weapon_charge::event(state, events, EV_ITEMUSEFAIL, fail as u16);
            }
            state.movement_flags |= PMF_USE_ITEM_HELD;
        }
        Ok(()) => {
            // "this should not happen...": the selection names an item not held.
            if state.holdable_items & (1 << tag) == 0 {
                return true;
            }
            if !kept_after_use(tag) {
                state.holdable_items &= !(1 << tag);
            }
            state.movement_flags |= PMF_USE_ITEM_HELD;
            crate::pmove_weapon_charge::event(state, events, EV_USE_ITEM0 + tag as u16, 0);
            if !kept_after_use(tag) {
                state.holdable_item = cycle(state.holdable_items, 0, 1);
            }
        }
    }
    true
}

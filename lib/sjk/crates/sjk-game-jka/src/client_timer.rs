//! `ClientTimerActions` (OpenJK `codemp/game/g_active.c:800-820`): once a second of a
//! living player's own time, health and armour above the maximum count down by one.
//! `ClientThink_real` runs it after the move for a player that is not dead.

use sjk_protocol::PlayerState;

const STAT_ARMOR: usize = 5;

/// One think's worth of the once-a-second actions: `msec` is the think's, `residual`
/// the player's `timeResidual`. `health` is the entity's, which the frame's end copies
/// to the stat.
pub fn timer_actions(state: &mut PlayerState, health: &mut i32, residual: &mut i32, msec: i32) {
    *residual += msec;
    while *residual >= 1_000 {
        *residual -= 1_000;
        let max_health = state.max_health();
        if *health > max_health {
            *health -= 1;
        }
        if state.stats[STAT_ARMOR] as i32 > max_health {
            // `stats` are C `int`s: an armour of 0 over a negative maximum goes to -1.
            state.stats[STAT_ARMOR] = state.stats[STAT_ARMOR].wrapping_sub(1);
        }
    }
}

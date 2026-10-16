//! On-foot PM_CheckDuck clearance, codemp/game/bg_pmove.c:4382-4406.
use crate::pmove::{MovementCollision, MovementState};

/// Sweep nine small head probes from the crouched top to the standing top.
/// PM_CanStand rejects allsolid or fraction < 1, not startsolid by itself.
pub(crate) fn can_stand(
    state: &MovementState,
    collision: &impl MovementCollision,
    minimums: [f32; 3],
    content_mask: u32,
) -> bool {
    // The ordinary player's horizontal hull is -15..15; inset each probe by five.
    let mut x = minimums[0] + 5.0;
    while x <= 10.0 {
        let mut y = minimums[1] + 5.0;
        while y <= 10.0 {
            let start = [
                state.origin[0] + x,
                state.origin[1] + y,
                state.origin[2] + state.crouching_height,
            ];
            let end = [start[0], start[1], state.origin[2] + state.standing_height];
            let trace = collision.trace(
                start,
                [-5.0, -5.0, -2.5],
                [5.0, 5.0, 0.0],
                end,
                content_mask,
            );
            if trace.all_solid || trace.fraction < 1.0 {
                return false;
            }
            y += 10.0;
        }
        x += 10.0;
    }
    true
}

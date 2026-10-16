//! `NAV_CheckAhead`'s post-trace decisions (`codemp/game/g_nav.c:534–571`).

use crate::npc_nav_setup::NavObstacle;
use crate::npc_senses::distance_squared;
use crate::pmove::MovementTrace;

/// `MIN_DOOR_BLOCK_DIST_SQR`, `ENTITYNUM_WORLD` in the compatibility profile.
const MIN_DOOR_BLOCK_DIST_SQR: f32 = 16.0 * 16.0;
const ENTITYNUM_WORLD: u16 = 1022;

/// Whether the trace reaches close enough to the goal or an approachable unlocked
/// door. Door classification is requested only after the ordinary trace gates fail.
pub(crate) fn clear_ahead(
    origin: [f32; 3],
    maxs: [f32; 3],
    end: [f32; 3],
    trace: &MovementTrace,
    obstacle: impl FnOnce(u16) -> NavObstacle,
) -> bool {
    if !trace.all_solid && !trace.start_solid && trace.fraction == 1.0 {
        return true;
    }
    if f64::from(origin[2] - end[2]).abs() > 48.0 {
        return false;
    }
    let radius = maxs[0].max(maxs[1]);
    let distance = distance_squared(origin, end).sqrt();
    if trace.fraction >= 1.0 - radius / distance {
        return true;
    }
    // `g_nav.c:548–568`: do not route around an unlocked door unless already
    // caught on its lip. The comparison is strictly below 16 units in 3D.
    trace.entity_number < ENTITYNUM_WORLD
        && matches!(
            obstacle(trace.entity_number),
            NavObstacle::Door { unlocked: true }
        )
        && distance_squared(origin, trace.end_position) >= MIN_DOOR_BLOCK_DIST_SQR
}

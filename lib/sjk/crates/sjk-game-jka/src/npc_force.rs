//! An NPC's Force data (`forcedata_t` on its client): what the spawn's cleared client leaves
//! and what `WP_InitForcePowers` (`w_force.c:423-541`) sets as it begins — the players'
//! [`crate::force_powers::ForcePowers`], which the powers an NPC uses run on.

use crate::force_powers::{ForcePowers, NUM_FORCE_POWERS};

/// The Force of a client the spawn cleared (`memset` in `NPC_Spawn_Do`), with the levels
/// its definition gave: nothing running and every entity number 0, and the pool's ceiling
/// its `forcePowerMax` (`NPC_ParseParms`), else 0 — so the pool gains nothing before the
/// NPC begins (`WP_SpawnInitForcePowers` then gives it `FORCE_POWER_MAX`).
pub fn cleared(levels: &[i32; NUM_FORCE_POWERS], max: i32) -> ForcePowers {
    let mut force = ForcePowers::with_levels(levels.map(|level| level.clamp(0, 255) as u8));
    force.max = max;
    force.grip_entity = 0;
    force.drain_entity = 0;
    force
}

/// `WP_InitForcePowers`' Force data for an NPC beginning at `level_time`: the timers
/// cleared, the pool regenerating from now, nobody gripped or drained.
pub fn init(force: &mut ForcePowers, levels: &[i32; NUM_FORCE_POWERS], level_time: i32) {
    force.respawned(levels.map(|level| level.clamp(0, 255) as u8), level_time);
}

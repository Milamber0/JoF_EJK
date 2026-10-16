//! `EF_DISINTEGRATION`: a body burning away after a disruptor kill, or a corpse shot or
//! cut until its health runs out (`body_die`, `g_combat.c`).
//!
//! cgame freezes the pose (`cg_players.c`, the `EF_DISINTEGRATION` block of `CG_Player`;
//! `cg_ents.c` for `ET_BODY`) and draws the entity twice in `CG_Disintegration`
//! (`cg_ents.c`): the model with `RF_DISINTEGRATE1`, eaten away from the hit point, and
//! again with `gfx/effects/burn` and `RF_DISINTEGRATE2`, the glowing edge. rd-vanilla
//! does the work per vertex (`RB_CalcDisintegrateColors`, `RB_CalcDisintegrateVertDeform`,
//! `tr_shade_calc.cpp`); the stage program does it here from the instance this module
//! marks. A player stops drawing 1.5 s in; a body draws until the server frees it.

use crate::ActorInstance;
use sjk_protocol::Snapshot;
use std::cell::Cell;

/// `EF_DISINTEGRATION` (`bg_public.h`).
pub(crate) const EF_DISINTEGRATION: u32 = 1 << 26;
/// Instance `view_flags` bit for `RF_DISINTEGRATE1`, read by `stage_runtime.wgsl`.
pub(crate) const RF_DISINTEGRATE1: u32 = 1 << 24;
/// Instance `view_flags` bit for `RF_DISINTEGRATE2`, read by `stage_runtime.wgsl`.
pub(crate) const RF_DISINTEGRATE2: u32 = 1 << 25;
/// A disintegrating player is no longer drawn after this (`cg_players.c`).
pub(crate) const PLAYER_MILLIS: i64 = 1_500;
/// `cgs.media.disruptorShader`.
pub(crate) const BURN_SHADER: &str = "gfx/effects/burn";
/// `cgs.effects.mDisruptorDeathSmoke`.
pub(crate) const SMOKE_EFFECT: &str = "disruptor/death_smoke";

/// One actor's disintegration (`centity_t::dustTrailTime`).
#[derive(Debug)]
pub(crate) struct State {
    /// Presentation time the flag was first seen.
    pub(crate) started: i64,
    /// World-space hit location: `origin2`, the player state's `lastHitLoc`.
    pub(crate) hit: [f32; 3],
    smoke_tick: Cell<i64>,
}

impl State {
    pub(crate) fn new(started: i64, hit: [f32; 3]) -> Self {
        Self {
            started,
            hit,
            smoke_tick: Cell::new(i64::MIN),
        }
    }

    /// The burn radius `(time - endTime) * 0.045` (`RB_CalcDisintegrateColors`).
    pub(crate) fn threshold(&self, now: i64) -> f32 {
        (now - self.started).max(0) as f32 * 0.045
    }

    /// `CG_Disintegration` puffs smoke once per 50 ms frame for the first second.
    pub(crate) fn smoke_due(&self, now: i64) -> bool {
        let tick = now.div_euclid(50);
        if now - self.started >= 1_000 || self.smoke_tick.get() == tick {
            return false;
        }
        self.smoke_tick.set(tick);
        true
    }

    /// Mark `instance` for one of the two passes, with the hit point and radius the
    /// stage program reads in place of the (unused) entity light.
    pub(crate) fn mark(&self, instance: &mut ActorInstance, pass: u32, now: i64) {
        instance.view_flags |= pass;
        instance.light_direction = self.hit;
        instance.light_directed = [self.threshold(now), 0.0, 0.0];
    }
}

/// The hit location of entity `number` when it is disintegrating in `snapshot`.
/// The viewer's own player comes from the player state (`lastHitLoc`), others from
/// their entity state (`origin2`, where `BG_PlayerStateToEntityState` copies it).
pub(crate) fn hit_location(snapshot: &Snapshot, local: bool, number: u16) -> Option<[f32; 3]> {
    if local {
        let player = &snapshot.player;
        if player.entity_flags() & EF_DISINTEGRATION == 0 {
            return None;
        }
        // lastHitLoc[0..3] are protocol fields 102, 105 and 100.
        return Some(
            [102, 105, 100].map(|field| f32::from_bits(player.raw_field(field).unwrap_or(0))),
        );
    }
    // Snapshot entities are ordered by number, so this is one binary search per
    // actor, not a scan of every entity.
    let index = snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .ok()?;
    let state = &snapshot.entities[index];
    (state.e_flags() & EF_DISINTEGRATION != 0).then(|| state.origin2())
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn the_burn_radius_grows_from_the_hit() {
        let state = State::new(1_000, [0.0; 3]);
        assert_eq!(state.threshold(900), 0.0);
        assert!((state.threshold(2_000) - 45.0).abs() < 1e-4);
    }

    #[test]
    fn smoke_puffs_once_per_tick_for_a_second() {
        let state = State::new(1_000, [0.0; 3]);
        assert!(state.smoke_due(1_010));
        assert!(!state.smoke_due(1_020));
        assert!(state.smoke_due(1_060));
        assert!(!state.smoke_due(2_000));
    }
}

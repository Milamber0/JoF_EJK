//! Snapshot translation policy for the JKA presentation adapter.
//!
//! `codemp/cgame/cg_ents.c:3086-3091` (`CG_CalcEntityLerpPositions`) forces only
//! `pos.trType` to `TR_INTERPOLATE` for remote client slots and `ET_NPC` when
//! `cg_smoothClients` is zero. `CG_InterpolateEntityPosition` (3045-3068) then
//! interpolates the snapshots' `trBase` values. Angular trajectories stay intact.

use super::legacy_evaluate_trajectory;
use sjk_protocol::EntityState;

const MAX_CLIENTS: u16 = 32;
const ET_NPC: u8 = 13;
const ET_EVENTS: u8 = 18;

impl super::LegacyWorldAdapter {
    /// Set `cg_smoothClients`: false (TaystJK default) uses remote snapshot bases.
    pub fn set_smooth_clients(&mut self, enabled: bool) {
        self.smooth_clients = enabled;
    }
}

/// Select a snapshot endpoint; runtime world sampling interpolates endpoints.
pub(super) fn translation(
    state: &EntityState,
    local_client_num: u16,
    server_time: i32,
    smooth_clients: bool,
) -> [f32; 3] {
    if state.entity_type() >= ET_EVENTS {
        return state.event_origin();
    }
    let remote_client = state.number() < MAX_CLIENTS && state.number() != local_client_num;
    if !smooth_clients && (remote_client || state.entity_type() == ET_NPC) {
        return state.trajectory_base();
    }
    legacy_evaluate_trajectory(
        state.trajectory_base(),
        state.trajectory_delta(),
        state.trajectory_type(),
        state.trajectory_time(),
        state.trajectory_duration(),
        server_time,
    )
}

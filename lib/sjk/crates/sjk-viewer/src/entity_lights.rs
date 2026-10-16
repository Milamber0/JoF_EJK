//! Additional codemp entity emitters; no simulation or renderer policy lives here.
use super::{PointLight, PointLightList};
use sjk_protocol::{EntityState, Snapshot};
use sjk_runtime::{EntityId, World};

const POWERUP_LIGHTS: u32 = (1 << 1) | (1 << 4) | (1 << 5) | (1 << 6);

/// Complete old EFX submission first so new persistent lights never evict existing effects.
pub(crate) fn finish(
    gpu: &mut crate::GpuState,
    time: i64,
    now: std::time::Instant,
    audio: &mut Option<crate::GameAudio>,
) {
    gpu.effect_aux.submit_lights(now, &mut gpu.dynamic_lights);
    let snapshot = crate::first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    );

    if let Some(snapshot) = snapshot {
        let world = gpu
            .demo_session
            .as_ref()
            .map_or(&gpu.live_world, crate::demo_playback::Session::world);
        append(snapshot, world, time, &mut gpu.dynamic_lights);
    }
    crate::effect_aux::saber_contacts::frame(gpu, time, now, audio);
    crate::effect_aux::saber_rain_steam::frame(gpu, time, now, audio);
}

/// Rebuild from current authority each frame; clearing/dropping needs no persistent light handle.
pub(crate) fn append(snapshot: &Snapshot, world: &World, time: i64, out: &mut PointLightList) {
    let local = snapshot.player.client_num();
    for state in &snapshot.entities {
        let kind = state.entity_type();
        // CG_AddCEntity, cg_ents.c:3290-3308: events and intermission exceptions.
        if kind >= 18 || (snapshot.player.movement_type() == 7 && matches!(kind, 0 | 1 | 12)) {
            continue;
        }
        let player_light = kind == 1
            && state.number() != local
            && state.powerups() & POWERUP_LIGHTS != 0
            && player_visible(state, local);
        if !player_light && packed(state, [0.; 3]).is_none() {
            continue;
        }
        let origin = world
            .entity(EntityId::new(u64::from(state.number()) + 1))
            .map(|e| e.sample(time).translation)
            .unwrap_or_else(|| {
                sjk_client::legacy_evaluate_trajectory(
                    state.trajectory_base(),
                    state.trajectory_delta(),
                    state.trajectory_type(),
                    state.trajectory_time(),
                    state.trajectory_duration(),
                    time as i32,
                )
            });
        if let Some(light) = packed(state, origin) {
            out.push(light);
        }
        // NPC/body powerups need their model-specific CG_Player early-exit rules; not claimed.
        if player_light {
            powerups(state.powerups(), origin, state.number(), time, out);
        }
    }
    // BG_PlayerStateToEntityState (bg_misc.c:2864-2869) tests nonzero, not expiry time.
    // The local player need not be in snapshot.entities; do not double-submit it if present.
    if !matches!(snapshot.player.movement_type(), 4 | 7 | 8)
        && snapshot.player.entity_flags() & ((1 << 8) | (1 << 26)) == 0
    {
        let mask = snapshot
            .player
            .powerups
            .iter()
            .enumerate()
            .fold(0_u32, |mask, (i, value)| {
                mask | (u32::from(*value != 0) << i)
            });
        if mask & POWERUP_LIGHTS == 0 {
            return;
        }
        let origin = world
            .entity(EntityId::new(u64::from(local) + 1))
            .map_or(snapshot.player.origin(), |e| e.sample(time).translation);
        powerups(mask, origin, local, time, out);
    }
}

fn player_visible(state: &EntityState, local: u16) -> bool {
    // cg_players.c:8843-8849, 9655-9660, 10664: hidden/disintegrating players skip powerups.
    state.e_flags() & ((1 << 8) | (1 << 26)) == 0
        && state.raw_field(96).unwrap_or(0) & (1 << 7) == 0
        && !state.client_bitflag(local)
}

/// CG_EntityEffects, cg_ents.c:343-357. Player constantLight is a charge timestamp, not RGB.
pub(crate) fn packed(state: &EntityState, origin: [f32; 3]) -> Option<PointLight> {
    if matches!(state.entity_type(), 1 | 12 | 13 | 15) {
        return None;
    }
    let bits = state.raw_field(64).unwrap_or(0);
    let radius = ((bits >> 24) & 255) as f32 * 4.0;
    (radius > 0.0).then_some(PointLight {
        origin,
        radius,
        color: [bits & 255, (bits >> 8) & 255, (bits >> 16) & 255].map(|v| v as f32 / 255.0),
    })
}

/// CG_PlayerPowerups, cg_players.c:4494-4524: quad, red, blue and neutral, independently.
pub(crate) fn powerups(
    mask: u32,
    origin: [f32; 3],
    entity: u16,
    time: i64,
    out: &mut PointLightList,
) {
    for (bit, color) in [
        (1, [0.2, 0.2, 1.0]),
        (4, [1.0, 0.2, 0.2]),
        (5, [0.2, 0.2, 1.0]),
        (6, [1.0; 3]),
    ] {
        if mask & (1 << bit) == 0 {
            continue;
        }
        // Stock rand()&31 range, presentation-local deterministic jitter; no shared combat RNG.
        let mut random = (time as u32) ^ u32::from(entity).wrapping_mul(0x9e37_79b9) ^ bit;
        random ^= random >> 16;
        random = random.wrapping_mul(0x85eb_ca6b);
        random ^= random >> 13;
        out.push(PointLight {
            origin,
            color,
            radius: 200.0 + (random & 31) as f32,
        });
    }
}

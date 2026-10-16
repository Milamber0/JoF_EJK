//! Viewer dispatch for allocation-free legacy missile effect requests.

use crate::dynamic_lights::{PointLight, PointLightList};
use crate::effect_runtime::EffectLibrary;
use crate::{GameAudio, Particle, combat_effects, effect_runtime};
use glam::Vec3;
use sjk_client::{LegacyMissileEffectMetrics, LegacyMissileEffects};
use sjk_protocol::Snapshot;
use sjk_vfs::VirtualFileSystem;
use std::time::Instant;

/// Cache weapon definitions while preparing the world, never while firing a shot.
pub(crate) fn load(
    game: Option<&sjk_protocol::GameState>,
    vfs: &VirtualFileSystem,
) -> LegacyMissileEffects {
    let Some(game) = game else {
        return LegacyMissileEffects::empty();
    };
    let mut effects = LegacyMissileEffects::from_game_state(game);
    effects.load_vehicles(game, vfs);
    effects
}

/// Evaluate and play one logical projectile-think request per eligible
/// ET_MISSILE for the rendered frame. EFX graph storage, particles, and point
/// lights are all preallocated by their owning runtimes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_and_spawn(
    runtime: &mut LegacyMissileEffects,
    snapshot: &Snapshot,
    presentation_time: i32,
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    lights: &mut PointLightList,
    now: Instant,
) -> LegacyMissileEffectMetrics {
    let metrics = runtime.update(snapshot, presentation_time);
    // Trail EFX on the reference 8 ms cadence (`effect_cadence.rs`); lights every frame.
    let play = auxiliary.continuous.due(now);
    for request in runtime.requests() {
        if let Some(light) = request.light {
            lights.push(PointLight {
                origin: light.origin,
                radius: light.radius,
                color: light.color,
            });
        }
        if !play {
            continue;
        }
        if let Some(name) = request.extra_effect_name {
            for repetition in 0..request.extra_repetitions {
                spawn(
                    particles,
                    auxiliary,
                    effects,
                    vfs,
                    audio,
                    name,
                    request.origin,
                    request.direction,
                    now,
                    seed(request.entity_number, presentation_time, repetition),
                );
            }
        }
        spawn(
            particles,
            auxiliary,
            effects,
            vfs,
            audio,
            request.effect_name,
            request.origin,
            request.direction,
            now,
            seed(request.entity_number, presentation_time, u8::MAX),
        );
    }
    metrics
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    name: &str,
    origin: [f32; 3],
    direction: [f32; 3],
    now: Instant,
    seed: u32,
) {
    effect_runtime::spawn_effect(
        particles,
        auxiliary,
        effects,
        vfs,
        name,
        Vec3::from_array(origin),
        now,
        seed,
        0,
        audio,
        combat_effects::rotation_from_direction(direction),
    );
}

fn seed(entity: u16, presentation_time: i32, component: u8) -> u32 {
    u32::from(entity)
        ^ (presentation_time as u32).rotate_left(11)
        ^ u32::from(component).wrapping_mul(0x9e37_79b9)
}

/// `cg_debugMissiles 1`: once a second, log what the missile trails saw, so a
/// missing trail can be traced to its cause (no missile in the snapshot, an
/// override slot, or an effect file that did not load).
pub(crate) fn debug_report(
    runtime: &LegacyMissileEffects,
    metrics: LegacyMissileEffectMetrics,
    snapshot: &Snapshot,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    presentation_time: i32,
) {
    use std::sync::atomic::{AtomicI32, Ordering};
    static LAST_SECOND: AtomicI32 = AtomicI32::new(i32::MIN);
    let second = presentation_time.div_euclid(1_000);
    if LAST_SECOND.swap(second, Ordering::Relaxed) == second {
        return;
    }
    crate::log::progress(format_args!(
        "missiles: {} in snapshot, {} trails, {} without trail, {} custom, {} custom missing",
        metrics.decoded_missiles,
        metrics.play_requests,
        metrics.no_trail_missiles,
        metrics.custom_effect_requests,
        metrics.missing_custom_effects,
    ));
    for state in snapshot
        .entities
        .iter()
        .filter(|state| state.entity_type() == crate::projectiles::ET_MISSILE)
        .take(4)
    {
        crate::log::progress(format_args!(
            "  missile {}: weapon {} flags {:#x} otherEntityNum2 {} trType {}",
            state.number(),
            state.weapon(),
            state.e_flags(),
            state.other_entity_num2(),
            state.trajectory_type(),
        ));
    }
    for request in runtime.requests().take(4) {
        let loaded = effects.definition(vfs, request.effect_name).is_some();
        crate::log::progress(format_args!(
            "  trail {} on {}: {}",
            request.effect_name,
            request.entity_number,
            if loaded {
                "effect loaded"
            } else {
                "effect NOT loaded"
            },
        ));
    }
}

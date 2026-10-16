//! Runtime spawning for retail `.efx` graphs.
//!
//! The compatibility parser owns asset semantics; this module turns supported
//! sprite, spark/line, tail, flash, oriented-particle, decal, cylinder, and
//! electricity primitives into the viewer's fixed pools. Unsupported graph
//! primitives are ignored explicitly instead of substituted with invented art.

use super::*;
pub(crate) use crate::effect_library::EffectLibrary;
pub(crate) use crate::particle_types::{
    Particle, ParticleLayerSamples, blend_for_stage as particle_blend_for_stage,
    fade as particle_fade,
};

/// Resolve a shader's first-stage OpenGL blend state to a prebuilt particle
/// pipeline. `codemp/client/FxPrimitives.cpp:112-138,1686-1755,1828-1846`
/// supplies vertex RGBA and submits the primitive; its registered shader stage
/// remains authoritative for framebuffer blending. The add, filter, and alpha
/// blend shorthands are the pairs parsed by
/// `codemp/rd-vanilla/tr_shader.cpp:1355-1395`.

/// Return the RGB and alpha lifetime multipliers used by stock FX particles.
///
/// `FX_USE_ALPHA` selects `CParticle::UpdateAlpha`; otherwise the same life
/// envelope fades RGB while vertex alpha remains one
/// (`codemp/client/FxPrimitives.cpp:502-588`).

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_snapshot_particles(
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    previous_events: &mut HashMap<u16, u16>,
    snapshot: &Snapshot,
    game_state: &GameState,
    vfs: &VirtualFileSystem,
    effects: &mut EffectLibrary,
    audio: &mut Option<GameAudio>,
    now: Instant,
    bsp: &sjk_bsp::Bsp,
    trace_scratch: &mut sjk_bsp::TraceScratch,
) {
    for entity in &snapshot.entities {
        if entity.entity_type() == 17 {
            // ET_FX, bg_public.h:1262
            // Persistent map runners are scheduled by sjk-client's fixed
            // LegacyMapEffects state at presentation time, not by event latches.
            continue;
        }
        let raw_event = if entity.entity_type() >= 18 {
            u16::from(entity.entity_type() - 18)
        } else {
            entity.event() & 0xff
        };
        let latch = if entity.entity_type() >= 18 {
            raw_event
        } else {
            entity.event()
        };
        if raw_event == 0 || previous_events.get(&entity.number()) == Some(&latch) {
            continue;
        }
        previous_events.insert(entity.number(), latch);
        let Some(effect_name) = combat_effects::for_event(raw_event, entity, game_state) else {
            continue;
        };
        let origin = if matches!(raw_event, 69 | 70) {
            Vec3::from_array(sjk_client::legacy_evaluate_trajectory(
                entity.trajectory_base(),
                entity.trajectory_delta(),
                entity.trajectory_type(),
                entity.trajectory_time(),
                entity.trajectory_duration(),
                snapshot.server_time,
            ))
        } else if raw_event == 68 || entity.entity_type() >= 18 {
            // `G_PlayEffect` writes `s.origin`; a plain `G_TempEntity` (teleport in and out,
            // `g_utils.c`) only sets the trajectory base, and cgame reads that as the event
            // position. Taking the unset origin put every spawn effect at the map origin.
            let origin = entity.event_origin();
            Vec3::from_array(if origin == [0.0; 3] {
                entity.trajectory_base()
            } else {
                origin
            })
        } else {
            Vec3::from_array(entity.trajectory_base())
        };
        let origin = if matches!(raw_event, 34 | 64 | 65) {
            // `EV_PLAYER_TELEPORT_IN/OUT` and `EV_BECOME_JEDIMASTER` (`cg_event.c:2409-2430,
            // 2699-2751`) drop the player box 4096 units onto the floor and play `mp/spawn`
            // or `mp/jedispawn` there; nothing below, no effect.
            match teleport_floor(bsp, trace_scratch, origin) {
                Some(floor) => floor,
                None => continue,
            }
        } else {
            origin
        };
        spawn_effect(
            particles,
            auxiliary,
            effects,
            vfs,
            effect_name,
            origin,
            now,
            u32::from(entity.number()) | (u32::from(raw_event) << 16),
            0,
            audio,
            combat_effects::rotation_from_direction(combat_effects::event_direction(
                raw_event, entity,
            )),
        );
    }
    previous_events.retain(|number, _| {
        snapshot
            .entities
            .binary_search_by_key(number, |entity| entity.number())
            .is_ok()
    });
}

/// Where `mp/spawn` plays: `origin` dropped onto the floor with the player's box (mins z
/// `DEFAULT_MINS_2 + 8`), or `None` over a void.
fn teleport_floor(
    bsp: &sjk_bsp::Bsp,
    scratch: &mut sjk_bsp::TraceScratch,
    origin: Vec3,
) -> Option<Vec3> {
    // `MASK_SOLID`, `CONTENTS_SOLID | CONTENTS_TERRAIN` (`bg_public.h:1225`), as
    // `cg_event.c` traces it.
    const MASK_SOLID: u32 = 0x0000_1001;
    let bounds = sjk_bsp::Aabb::new([-15.0, -15.0, -16.0], [15.0, 15.0, 40.0]).ok()?;
    let end = origin - Vec3::Z * 4096.0;
    let trace = bsp.trace_box_with(
        scratch,
        origin.to_array(),
        end.to_array(),
        bounds,
        MASK_SOLID,
    );
    (trace.fraction < 1.0).then(|| Vec3::from_array(trace.end_position))
}

/// Play the requests produced by the legacy ET_FX adapter through the same
/// retail EFX graph runtime used by impacts and muzzle effects.
pub(crate) fn spawn_map_effect_requests(
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    map_effects: &sjk_client::LegacyMapEffects,
    vfs: &VirtualFileSystem,
    effects: &mut EffectLibrary,
    audio: &mut Option<GameAudio>,
    now: Instant,
) {
    for request in map_effects.requests() {
        spawn_effect(
            particles,
            auxiliary,
            effects,
            vfs,
            request.effect_name,
            Vec3::from_array(request.origin),
            now,
            u32::from(request.entity_number),
            0,
            audio,
            combat_effects::rotation_from_direction(request.direction),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_effect(
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    effect_name: &str,
    origin: Vec3,
    now: Instant,
    seed: u32,
    depth: u8,
    audio: &mut Option<GameAudio>,
    rotation: Quat,
) {
    if depth >= 4 || !crate::particle_room::effect_fits(particles.len()) {
        return;
    }

    let Some(definition) = effects.definition(vfs, effect_name) else {
        return;
    };
    if depth == 0 {
        crate::effect_debug::report_effect(effect_name, &definition);
    }
    for (component_index, component) in definition.components.iter().enumerate() {
        if component.kind == ComponentKind::Sound {
            if !component.sounds.is_empty() {
                let index =
                    mix_seed(seed, component_index as u32, 0) as usize % component.sounds.len();
                if let Some(audio) = audio.as_mut() {
                    audio.play(
                        vfs,
                        &component.sounds[index],
                        0.7,
                        origin.to_array(),
                        sjk_audio::SourceId(seed | (1 << 31)),
                    );
                }
            }
            continue;
        }
        let component_seed = mix_seed(seed, component_index as u32, 0);
        if component.kind == ComponentKind::Light {
            let count = component
                .count
                .sample(random_unit(component_seed))
                .round()
                .max(0.0) as usize;
            for primitive_index in 0..count {
                let primitive_seed = mix_seed(component_seed, primitive_index as u32, 1);
                let units = [
                    random_unit(primitive_seed),
                    random_unit(primitive_seed.wrapping_add(1)),
                    random_unit(primitive_seed.wrapping_add(2)),
                ];
                let primitive_origin =
                    origin + rotation * Vec3::from_array(component.origin.sample(units));
                auxiliary.spawn_light(component, primitive_origin, now, primitive_seed);
            }
            continue;
        }
        if component.kind == ComponentKind::CameraShake {
            let count = component
                .count
                .sample(random_unit(component_seed))
                .round()
                .max(0.0) as usize;
            for primitive_index in 0..count {
                let primitive_seed = mix_seed(component_seed, primitive_index as u32, 1);
                let units = [
                    random_unit(primitive_seed),
                    random_unit(primitive_seed.wrapping_add(1)),
                    random_unit(primitive_seed.wrapping_add(2)),
                ];
                let primitive_origin =
                    origin + rotation * Vec3::from_array(component.origin.sample(units));
                auxiliary.spawn_shake(component, primitive_origin, now, primitive_seed);
            }
            continue;
        }
        if component.kind == ComponentKind::Emitter {
            auxiliary.emitters.spawn(
                component,
                Arc::clone(&definition),
                component_index,
                effects,
                origin,
                rotation,
                now,
                component_seed,
                depth,
            );
            continue;
        }
        if component.kind == ComponentKind::FxRunner {
            for (nested_index, nested) in component.effects.iter().enumerate() {
                spawn_effect(
                    particles,
                    auxiliary,
                    effects,
                    vfs,
                    nested,
                    origin,
                    now,
                    mix_seed(seed, component_index as u32, nested_index as u32),
                    depth + 1,
                    audio,
                    rotation,
                );
            }
            continue;
        }
        if component.kind == ComponentKind::Decal {
            spawn_decals(
                auxiliary,
                effects,
                component,
                origin,
                rotation,
                component_seed,
            );
            continue;
        }
        if !matches!(
            component.kind,
            ComponentKind::Particle
                | ComponentKind::OrientedParticle
                | ComponentKind::Line
                | ComponentKind::Tail
                | ComponentKind::Cylinder
                | ComponentKind::Electricity
                | ComponentKind::Flash
        ) || component.shaders.is_empty()
        {
            continue;
        }
        let count_unit = random_unit(mix_seed(seed, component_index as u32, 0));
        let count = component.count.sample(count_unit).round().max(0.0) as usize;
        for particle_index in 0..count {
            if !crate::particle_room::effect_fits(particles.len()) {
                return;
            }
            let particle_seed = mix_seed(seed, component_index as u32, particle_index as u32 + 1);
            let units = [
                random_unit(particle_seed),
                random_unit(particle_seed.wrapping_add(1)),
                random_unit(particle_seed.wrapping_add(2)),
            ];
            let (particle_origin, particle_rotation) = crate::particle_spawn::placement(
                component,
                origin,
                rotation,
                [
                    units[0],
                    units[1],
                    units[2],
                    random_unit(particle_seed.wrapping_add(30)),
                    random_unit(particle_seed.wrapping_add(31)),
                    random_unit(particle_seed.wrapping_add(32)),
                ],
            );
            let sampled_velocity = Vec3::from_array(component.velocity.sample([
                random_unit(particle_seed.wrapping_add(3)),
                random_unit(particle_seed.wrapping_add(4)),
                random_unit(particle_seed.wrapping_add(5)),
            ]));
            let velocity = if component.spawn_flags.absolute_velocity {
                sampled_velocity
            } else {
                particle_rotation * sampled_velocity
            };
            let sampled_acceleration = Vec3::from_array(component.acceleration.sample([
                random_unit(particle_seed.wrapping_add(25)),
                random_unit(particle_seed.wrapping_add(26)),
                random_unit(particle_seed.wrapping_add(27)),
            ]));
            let acceleration = if component.spawn_flags.absolute_acceleration {
                sampled_acceleration
            } else {
                particle_rotation * sampled_acceleration
            };
            let line_origin2 = || {
                Vec3::from_array(component.origin2.sample([
                    random_unit(particle_seed.wrapping_add(20)),
                    random_unit(particle_seed.wrapping_add(21)),
                    random_unit(particle_seed.wrapping_add(22)),
                ]))
            };
            // An `org2fromTrace` line (the beams of `mp/spawn`, `mp/jedispawn`, `env/beam`)
            // ends where a trace along its forward axis meets a solid. Its streak points at
            // the untraced end until the first draw traces it; taken as an offset from the
            // origin, as other lines are, it had no length and drew as a square at the origin.
            let trace_streak =
                component.kind == ComponentKind::Line && component.spawn_flags.origin2_from_trace;
            let streak = match component.kind {
                ComponentKind::Line if trace_streak => Some(
                    crate::effect_shapes::origin2_trace_target(
                        component,
                        particle_origin,
                        particle_rotation,
                        line_origin2(),
                    ) - particle_origin,
                ),
                ComponentKind::Line => Some(particle_rotation * line_origin2()),
                ComponentKind::Tail => Some(-velocity),
                _ => None,
            };
            let normal = (component.kind == ComponentKind::OrientedParticle)
                .then(|| particle_rotation * Vec3::X);
            let shader_index = particle_seed as usize % component.shaders.len();
            let gravity = component
                .gravity
                .sample(random_unit(particle_seed.wrapping_add(6)));
            let delay_millis = crate::particle_spawn::delay_millis(
                component,
                particle_index,
                count,
                random_unit(particle_seed.wrapping_add(7)),
            );
            let delay = Duration::from_secs_f32(delay_millis.max(0.0) / 1_000.0);
            let starts_at = now + delay;
            let rgb = crate::particle_spawn::rgb_envelopes(
                component,
                [
                    random_unit(particle_seed.wrapping_add(13)),
                    random_unit(particle_seed.wrapping_add(14)),
                ],
                particle_seed,
            );
            let rotates = matches!(
                component.kind,
                ComponentKind::Particle | ComponentKind::OrientedParticle
            );
            let shape = crate::effect_shapes::from_component(
                component,
                origin,
                particle_origin,
                particle_rotation,
                particle_seed,
            );
            particles.push(Particle {
                motion: crate::particle_motion::Motion::new(
                    particle_origin,
                    velocity,
                    acceleration + Vec3::Z * gravity,
                    if rotates {
                        component
                            .rotation
                            .sample(random_unit(particle_seed.wrapping_add(28)))
                    } else {
                        0.0
                    },
                    if rotates {
                        component
                            .rotation_delta
                            .sample(random_unit(particle_seed.wrapping_add(29)))
                    } else {
                        0.0
                    },
                    starts_at,
                ),

                spawned_at: now,
                delay,
                lifetime: Duration::from_secs_f32(
                    component
                        .life
                        .sample(random_unit(particle_seed.wrapping_add(8)))
                        .max(1.0)
                        / 1_000.0,
                ),
                size: crate::effect_envelope::Envelope::from_curve(
                    component.size,
                    particle_seed.wrapping_add(9),
                ),
                start_length: component
                    .length
                    .start
                    .sample(random_unit(particle_seed.wrapping_add(23))),
                end_length: component
                    .length
                    .end
                    .sample(random_unit(particle_seed.wrapping_add(24))),
                streak,
                trace_streak,
                normal,
                alpha: crate::effect_envelope::Envelope::from_curve(
                    component.alpha,
                    particle_seed.wrapping_add(11),
                ),
                use_alpha: component.flags.use_alpha,
                set_shader_time: component.flags.set_shader_time,
                rgb,
                seed: particle_seed,
                shader: effects.shader(&component.shaders[shader_index]),
                physics: crate::particle_physics::State::new(
                    Arc::clone(&definition),
                    component_index,
                    depth,
                    component
                        .elasticity
                        .sample(random_unit(particle_seed.wrapping_add(30))),
                    component.flags,
                ),
                shape,
            });
        }
    }
}

/// `FxScheduler.cpp:1558-1621` (`case Decal`): every instance of the
/// component becomes a world mark through `AddDecalToScene` with the
/// primitive's normal, sampled rotation, start colour, start alpha and start
/// size. Decals never inherit `FX_RELATIVE` placement (`:1215-1236`), and the
/// `FX_GHOUL2_DECALS` companion mark stays deferred.
fn spawn_decals(
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    component: &sjk_effect::Component,
    origin: Vec3,
    rotation: Quat,
    component_seed: u32,
) {
    if component.shaders.is_empty() {
        return;
    }
    let count = component
        .count
        .sample(random_unit(component_seed))
        .round()
        .max(0.0) as usize;
    for particle_index in 0..count {
        let particle_seed = mix_seed(component_seed, particle_index as u32, 1);
        let units =
            [0, 1, 2, 30, 31, 32].map(|offset| random_unit(particle_seed.wrapping_add(offset)));
        let (mark_origin, mark_rotation) =
            crate::particle_spawn::placement(component, origin, rotation, units);
        let rgb_unit = random_unit(particle_seed.wrapping_add(13));
        let rgb = component.rgb_start.map(|channel| channel.sample(rgb_unit));
        let alpha = component
            .alpha
            .start
            .sample(random_unit(particle_seed.wrapping_add(11)));
        let radius = component
            .size
            .start
            .sample(random_unit(particle_seed.wrapping_add(9)));
        let shader_index = particle_seed as usize % component.shaders.len();
        auxiliary.decals.request(crate::decal_marks::DecalRequest {
            origin: mark_origin,
            direction: mark_rotation * Vec3::X,
            orientation: component
                .rotation
                .sample(random_unit(particle_seed.wrapping_add(28))),
            color: [rgb[0], rgb[1], rgb[2], alpha],
            radius,
            shader: effects.shader(&component.shaders[shader_index]),
        });
    }
}

fn mix_seed(seed: u32, first: u32, second: u32) -> u32 {
    seed ^ first.wrapping_mul(0x9e37_79b9) ^ second.wrapping_mul(0x85eb_ca6b)
}

fn random_unit(mut seed: u32) -> f32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    (seed as f64 / u32::MAX as f64) as f32
}

#[cfg(test)]
#[path = "effect_runtime_tests.rs"]
mod tests;

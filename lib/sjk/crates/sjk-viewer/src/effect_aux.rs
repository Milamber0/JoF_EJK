//! Fixed-capacity runtime state for non-particle EFX primitives.
//!
//! Lights mirror `FX_AddLight` and `CLight::Update` in
//! `codemp/client/FxUtil.cpp:853-915` and
//! `codemp/client/FxPrimitives.cpp:1491-1673`. Camera shake mirrors
//! `CG_DoCameraShake`/`CG_SE_UpdateShake` in
//! `codemp/cgame/cg_view.c:1908-1945,2019-2039`.

use crate::dynamic_lights::{PointLight, PointLightList};
use crate::effect_envelope::{Envelope, random_unit};
use glam::Vec3;
use sjk_effect::Component;
use std::time::{Duration, Instant};

#[path = "saber_contacts.rs"]
pub(crate) mod saber_contacts;

#[path = "saber_rain_steam.rs"]
pub(crate) mod saber_rain_steam;

#[path = "effect_cadence.rs"]
pub(crate) mod cadence;

const MAX_EFFECT_PRIMITIVES: usize = 1_800;
const MAX_PENDING_SHAKES: usize = 64;

#[derive(Clone, Copy, Debug)]
struct ActiveLight {
    origin: Vec3,
    spawned_at: Instant,
    delay: Duration,
    lifetime: Duration,
    size: Envelope,
    rgb: [Envelope; 3],
    seed: u32,
}

#[derive(Clone, Copy, Debug)]
struct ShakeRequest {
    origin: Vec3,
    spawned_at: Instant,
    delay: Duration,
    intensity: f32,
    radius: f32,
    lifetime: Duration,
    seed: u32,
}

#[derive(Clone, Copy, Debug)]
struct ActiveShake {
    intensity: f32,
    started_at: Instant,
    lifetime: Duration,
    seed: u32,
}

/// Additive camera offsets generated after the normal view calculation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CameraOffset {
    pub(crate) origin: Vec3,
    /// Pitch/yaw/roll in degrees. Codemp deliberately leaves roll unchanged.
    pub(crate) angles: Vec3,
    pub(crate) magnitude: f32,
}

/// Map-lifetime, allocation-free storage for EFX lights and camera shakes.
pub(crate) struct Runtime {
    lights: Vec<ActiveLight>,
    pending_shakes: Vec<ShakeRequest>,
    active_shake: Option<ActiveShake>,
    last_camera_offset: CameraOffset,
    dropped_primitives: usize,
    pub(crate) emitters: crate::effect_emitter::Runtime,
    /// World marks requested by `Decal` components (`decal_store`).
    pub(crate) decals: crate::decal_store::DecalStore,
    pub(crate) saber_contacts: saber_contacts::Runtime,
    pub(crate) saber_rain_steam: saber_rain_steam::Runtime,
    /// Paces effects that stock re-plays every rendered frame (`effect_cadence.rs`).
    pub(crate) continuous: cadence::Cadence,
    /// Lights a world shot holds in place every frame (`world_shot_notes.rs`).
    #[cfg(test)]
    pub(crate) held_lights: Vec<PointLight>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            lights: Vec::with_capacity(MAX_EFFECT_PRIMITIVES),
            pending_shakes: Vec::with_capacity(MAX_PENDING_SHAKES),
            active_shake: None,
            last_camera_offset: CameraOffset::default(),
            dropped_primitives: 0,
            emitters: crate::effect_emitter::Runtime::default(),
            decals: crate::decal_store::DecalStore::default(),
            saber_contacts: saber_contacts::Runtime::default(),
            saber_rain_steam: saber_rain_steam::Runtime::default(),
            continuous: cadence::Cadence::default(),
            #[cfg(test)]
            held_lights: Vec::new(),
        }
    }
}

impl Runtime {
    pub(crate) fn spawn_light(
        &mut self,
        component: &Component,
        origin: Vec3,
        now: Instant,
        seed: u32,
    ) {
        if self.lights.len() == self.lights.capacity() {
            self.dropped_primitives += 1;
            return;
        }
        let lifetime_millis = component
            .life
            .sample(random_unit(seed.wrapping_add(8)))
            .max(1.0)
            .trunc();
        let size = Envelope::from_curve(component.size, seed.wrapping_add(9));
        let rgb = std::array::from_fn(|axis| {
            Envelope::from_ranges(
                component.rgb_start[axis],
                component.rgb_end[axis],
                component.rgb_parameter,
                component.rgb_flags,
                seed.wrapping_add(13 + axis as u32 * 3),
            )
        });
        self.lights.push(ActiveLight {
            origin,
            spawned_at: now,
            delay: Duration::from_secs_f32(
                component
                    .delay
                    .sample(random_unit(seed.wrapping_add(7)))
                    .max(0.0)
                    .trunc()
                    / 1_000.0,
            ),
            lifetime: Duration::from_secs_f32(lifetime_millis / 1_000.0),
            size,
            rgb,
            seed,
        });
    }

    pub(crate) fn spawn_shake(
        &mut self,
        component: &Component,
        origin: Vec3,
        now: Instant,
        seed: u32,
    ) {
        if self.pending_shakes.len() == self.pending_shakes.capacity() {
            self.dropped_primitives += 1;
            return;
        }
        self.pending_shakes.push(ShakeRequest {
            origin,
            spawned_at: now,
            delay: Duration::from_secs_f32(
                component
                    .delay
                    .sample(random_unit(seed.wrapping_add(24)))
                    .max(0.0)
                    .trunc()
                    / 1_000.0,
            ),
            intensity: component
                .intensity
                .sample(random_unit(seed.wrapping_add(25))),
            radius: component
                .radius
                .sample(random_unit(seed.wrapping_add(26)))
                .trunc(),
            lifetime: Duration::from_secs_f32(
                component
                    .life
                    .sample(random_unit(seed.wrapping_add(27)))
                    .max(0.0)
                    .trunc()
                    / 1_000.0,
            ),
            seed,
        });
    }

    /// Pending shake requests, for [`Self::drop_shakes_since`].
    pub(crate) fn pending_shakes(&self) -> usize {
        self.pending_shakes.len()
    }

    /// Forget shake requests queued after `count` (see `muzzle_effects::spawn`).
    pub(crate) fn drop_shakes_since(&mut self, count: usize) {
        self.pending_shakes.truncate(count);
    }

    /// Resolve pending requests against the local viewer, like
    /// `CG_DoCameraShake`. Remote presentation cameras are never mutated.
    pub(crate) fn resolve_shakes(&mut self, local_origin: Vec3, now: Instant, local_view: bool) {
        if !local_view {
            self.pending_shakes.clear();
            return;
        }
        let mut active = None;
        self.pending_shakes.retain(|request| {
            if now.saturating_duration_since(request.spawned_at) < request.delay {
                return true;
            }
            let distance = local_origin.distance(request.origin);
            if request.radius > 0.0 && distance <= request.radius {
                let falloff = 1.0 - distance / request.radius;
                active = Some(ActiveShake {
                    intensity: (request.intensity * falloff).min(16.0),
                    started_at: now,
                    lifetime: request.lifetime,
                    seed: request.seed,
                });
            }
            false
        });
        if active.is_some() {
            self.active_shake = active;
        }
    }

    /// Sample the current shake. At 90-degree FOV, codemp's
    /// `CG_SE_UpdateShake` scale is exactly `1 - elapsed / duration`.
    pub(crate) fn camera_offset(&mut self, now: Instant) -> CameraOffset {
        let Some(shake) = self.active_shake else {
            self.last_camera_offset = CameraOffset::default();
            return CameraOffset::default();
        };
        let elapsed = now.saturating_duration_since(shake.started_at);
        if shake.lifetime.is_zero() || elapsed > shake.lifetime {
            self.active_shake = None;
            self.last_camera_offset = CameraOffset::default();
            return CameraOffset::default();
        }
        let scale = 1.0 - elapsed.as_secs_f32() / shake.lifetime.as_secs_f32();
        let magnitude = shake.intensity * scale;
        let frame_seed = shake.seed ^ (elapsed.as_millis() as u32).rotate_left(13);
        let signed = |index: u32| random_unit(frame_seed.wrapping_add(index)) * 2.0 - 1.0;
        let offset = CameraOffset {
            origin: Vec3::new(signed(0), signed(1), signed(2)) * magnitude,
            angles: Vec3::new(signed(3), signed(4), 0.0) * magnitude,
            magnitude,
        };
        self.last_camera_offset = offset;
        offset
    }

    /// Expire EFX lights and append their current samples to the shared
    /// rd-vanilla-sized point-light list.
    pub(crate) fn submit_lights(&mut self, now: Instant, output: &mut PointLightList) {
        #[cfg(test)]
        for light in &self.held_lights {
            output.push(*light);
        }
        self.lights.retain(|light| {
            now.saturating_duration_since(light.spawned_at) <= light.delay + light.lifetime
        });
        for light in &self.lights {
            let age = now.saturating_duration_since(light.spawned_at);
            if age < light.delay {
                continue;
            }
            let elapsed = age.saturating_sub(light.delay).as_secs_f32() * 1_000.0;
            let lifetime = light.lifetime.as_secs_f32() * 1_000.0;
            output.push(PointLight {
                origin: light.origin.to_array(),
                radius: light
                    .size
                    .sample(elapsed, lifetime, light.seed.wrapping_add(31)),
                color: std::array::from_fn(|axis| {
                    light.rgb[axis].sample(
                        elapsed,
                        lifetime,
                        light.seed.wrapping_add(40 + axis as u32),
                    )
                }),
            });
        }
    }
}

/// Apply codemp's refdef-only shake without contaminating actor continuity.
pub(crate) fn apply_camera_offset(
    position: Vec3,
    target: Vec3,
    offset: CameraOffset,
) -> (Vec3, Vec3) {
    let distance = position.distance(target).max(1.0);
    let direction = (target - position).normalize_or(Vec3::X);
    let yaw = direction.y.atan2(direction.x) + offset.angles.y.to_radians();
    let pitch = direction.z.asin() + offset.angles.x.to_radians();
    let shaken_position = position + offset.origin;
    let shaken_direction = Vec3::new(
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    );
    (
        shaken_position,
        shaken_position + shaken_direction * distance,
    )
}

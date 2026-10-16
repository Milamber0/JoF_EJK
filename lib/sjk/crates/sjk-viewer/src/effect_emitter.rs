//! Fixed-capacity Raven EFX emitter simulation and attached-model output.
//!
//! Scheduling follows `FxScheduler.cpp:1526-1555`; motion, angular integration,
//! and distance-driven child effects follow `FxPrimitives.cpp:1305-1473`.

use super::*;
use crate::effect_envelope::{Envelope, random_unit};

pub(crate) const MAX_EMITTERS: usize = 512;
pub(crate) const MAX_EMITTER_MODELS: usize = 512;
const MAX_EMISSIONS: usize = 512;

#[derive(Clone)]
struct Emitter {
    motion: crate::particle_motion::Motion,
    physics: crate::particle_physics::State,
    spawned_at: Instant,
    delay: Duration,
    lifetime: Duration,
    updated_at: Instant,
    angles: Vec3,
    angle_delta: Vec3,
    base_rotation: Quat,
    size: Envelope,
    model: Option<Arc<str>>,
    emit_effect: Option<Arc<str>>,
    density: f32,
    distance_until_emit: f32,
    variance: f32,
    seed: u32,
    emission_index: u32,
    depth: u8,
}

#[derive(Clone)]
struct Emission {
    effect: Arc<str>,
    origin: Vec3,
    rotation: Quat,
    seed: u32,
    depth: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct ModelInstance {
    pub(crate) model: Arc<str>,
    pub(crate) origin: [f32; 3],
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
}

pub(crate) struct Runtime {
    active: Vec<Emitter>,
    models: Vec<ModelInstance>,
    emissions: Vec<Emission>,
    dropped_emitters: u64,
    dropped_models: u64,
    dropped_emissions: u64,
}

/// Load-time lookup from an EFX model path to the shared rigid-model mesh.
pub(crate) struct ModelCatalog {
    by_path: HashMap<String, usize>,
}

impl ModelCatalog {
    pub(crate) fn build(meshes: &[StaticModelMesh]) -> Self {
        let by_path = meshes
            .iter()
            .enumerate()
            .map(|(index, mesh)| (mesh.appearance.model.to_ascii_lowercase(), index))
            .collect();
        Self { by_path }
    }

    /// Extend the lookup after a configstring adds geometry to the shared buffers.
    pub(crate) fn insert(&mut self, path: &str, index: usize) {
        self.by_path.insert(path.to_ascii_lowercase(), index);
    }

    fn mesh(&self, path: &str) -> Option<usize> {
        self.by_path.get(&path.to_ascii_lowercase()).copied()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            active: Vec::with_capacity(MAX_EMITTERS),
            models: Vec::with_capacity(MAX_EMITTER_MODELS),
            emissions: Vec::with_capacity(MAX_EMISSIONS),
            dropped_emitters: 0,
            dropped_models: 0,
            dropped_emissions: 0,
        }
    }
}

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn(
        &mut self,
        component: &sjk_effect::Component,
        definition: Arc<sjk_effect::EffectDefinition>,
        component_index: usize,
        effects: &mut EffectLibrary,
        origin: Vec3,
        rotation: Quat,
        now: Instant,
        seed: u32,
        depth: u8,
    ) {
        let count = component.count.sample(random_unit(seed)).round().max(0.0) as usize;
        for index in 0..count {
            if self.active.len() == self.active.capacity() {
                self.dropped_emitters = self.dropped_emitters.saturating_add(1);
                return;
            }
            let item_seed = mix_seed(seed, index as u32);
            self.active.push(build_emitter(
                component,
                Arc::clone(&definition),
                component_index,
                effects,
                origin,
                rotation,
                now,
                item_seed,
                depth,
            ));
        }
    }

    pub(crate) fn update(
        &mut self,
        now: Instant,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        pending: &mut crate::particle_physics::PendingEffects,
    ) {
        self.models.clear();
        self.emissions.clear();
        let models = &mut self.models;
        let emissions = &mut self.emissions;
        let dropped_models = &mut self.dropped_models;
        let dropped_emissions = &mut self.dropped_emissions;
        self.active.retain_mut(|emitter| {
            let age = now.saturating_duration_since(emitter.spawned_at);
            if age >= emitter.delay + emitter.lifetime {
                crate::particle_physics::schedule_emitter_death(
                    &emitter.physics,
                    emitter.seed,
                    emitter.motion.sample().origin,
                    pending,
                );
                return false;
            }
            if age < emitter.delay {
                return true;
            }
            let previous = emitter.motion.sample().origin;
            if !crate::particle_physics::advance_emitter(
                &mut emitter.motion,
                &mut emitter.physics,
                emitter.seed,
                now,
                bsp,
                scratch,
                pending,
            ) {
                return false;
            }
            let current = emitter.motion.sample().origin;
            emitter.integrate_angles(now, previous == current);
            emitter.emit_along(previous, current, emissions, dropped_emissions);
            emitter.append_model(now, models, dropped_models);
            true
        });
    }

    pub(crate) fn pop_emission(&mut self) -> Option<(Arc<str>, Vec3, Quat, u32, u8)> {
        self.emissions.pop().map(|emission| {
            (
                emission.effect,
                emission.origin,
                emission.rotation,
                emission.seed,
                emission.depth,
            )
        })
    }
}

pub(crate) fn append_model_instances(
    runtime: &mut Runtime,
    catalog: &ModelCatalog,
    groups: &mut [Vec<ActorInstance>],
) {
    for instance in &runtime.models {
        let Some(mesh_index) = catalog.mesh(&instance.model) else {
            continue;
        };
        if groups[mesh_index].len() == groups[mesh_index].capacity() {
            runtime.dropped_models = runtime.dropped_models.saturating_add(1);
            continue;
        }
        groups[mesh_index].push(ActorInstance::new(
            instance.origin,
            instance.rotation,
            instance.scale,
        ));
    }
}

impl Emitter {
    fn integrate_angles(&mut self, now: Instant, stopped: bool) {
        let millis = now.saturating_duration_since(self.updated_at).as_secs_f32() * 1_000.0;
        if stopped {
            self.angle_delta *= 0.7;
        }
        self.angles += self.angle_delta * (millis * 0.01);
        self.updated_at = now;
    }

    fn emit_along(
        &mut self,
        start: Vec3,
        end: Vec3,
        output: &mut Vec<Emission>,
        dropped: &mut u64,
    ) {
        let Some(effect) = self.emit_effect.as_ref() else {
            return;
        };
        let distance = start.distance(end);
        if distance <= f32::EPSILON {
            return;
        }
        let direction = (end - start) / distance;
        let mut travelled = 0.0;
        while distance - travelled + 1.0e-5 >= self.distance_until_emit {
            travelled += self.distance_until_emit;
            if output.len() == output.capacity() {
                *dropped = dropped.saturating_add(1);
                return;
            }
            output.push(Emission {
                effect: Arc::clone(effect),
                origin: start + direction * travelled,
                rotation: self.rotation(),
                seed: mix_seed(self.seed, self.emission_index),
                depth: self.depth.saturating_add(1),
            });
            self.emission_index = self.emission_index.wrapping_add(1);
            self.distance_until_emit = self.next_distance();
        }
        self.distance_until_emit -= distance - travelled;
    }

    fn next_distance(&self) -> f32 {
        let signed = random_unit(mix_seed(self.seed, self.emission_index)) * 2.0 - 1.0;
        (self.density + signed * self.variance).max(0.001)
    }

    fn append_model(&self, now: Instant, output: &mut Vec<ModelInstance>, dropped: &mut u64) {
        let Some(model) = self.model.as_ref() else {
            return;
        };
        if output.len() == output.capacity() {
            *dropped = dropped.saturating_add(1);
            return;
        }
        let elapsed = now
            .saturating_duration_since(self.spawned_at + self.delay)
            .as_secs_f32()
            * 1_000.0;
        let lifetime = self.lifetime.as_secs_f32() * 1_000.0;
        let scale = self
            .size
            .sample(elapsed, lifetime, self.seed.wrapping_add(19));
        output.push(ModelInstance {
            model: Arc::clone(model),
            origin: self.motion.sample().origin.to_array(),
            rotation: self.rotation().to_array(),
            scale: [scale; 3],
        });
    }

    fn rotation(&self) -> Quat {
        self.base_rotation
            * Quat::from_rotation_z(self.angles.y.to_radians())
            * Quat::from_rotation_y(self.angles.x.to_radians())
            * Quat::from_rotation_x(self.angles.z.to_radians())
    }
}

#[allow(clippy::too_many_arguments)]
fn build_emitter(
    component: &sjk_effect::Component,
    definition: Arc<sjk_effect::EffectDefinition>,
    component_index: usize,
    effects: &mut EffectLibrary,
    origin: Vec3,
    rotation: Quat,
    now: Instant,
    seed: u32,
    depth: u8,
) -> Emitter {
    let units = std::array::from_fn(|offset| random_unit(seed.wrapping_add(offset as u32)));
    let (placed_origin, placed_rotation) =
        crate::particle_spawn::placement(component, origin, rotation, units);
    let sample_vector = |range: sjk_effect::VectorRange, offset: u32| {
        Vec3::from_array(range.sample(std::array::from_fn(|axis| {
            random_unit(seed.wrapping_add(offset + axis as u32))
        })))
    };
    let velocity = sample_vector(component.velocity, 10);
    let acceleration = sample_vector(component.acceleration, 13);
    let velocity = if component.spawn_flags.absolute_velocity {
        velocity
    } else {
        placed_rotation * velocity
    };
    let acceleration = if component.spawn_flags.absolute_acceleration {
        acceleration
    } else {
        placed_rotation * acceleration
    } + Vec3::Z * component.gravity.sample(random_unit(seed.wrapping_add(16)));
    let density = component.density.sample(random_unit(seed.wrapping_add(17)));
    Emitter {
        motion: crate::particle_motion::Motion::new(
            placed_origin,
            velocity,
            acceleration,
            0.0,
            0.0,
            now,
        ),
        physics: crate::particle_physics::State::new(
            definition,
            component_index,
            depth,
            component
                .elasticity
                .sample(random_unit(seed.wrapping_add(18))),
            component.flags,
        ),
        spawned_at: now,
        delay: Duration::from_secs_f32(
            component
                .delay
                .sample(random_unit(seed.wrapping_add(19)))
                .max(0.0)
                / 1_000.0,
        ),
        lifetime: Duration::from_secs_f32(
            component
                .life
                .sample(random_unit(seed.wrapping_add(20)))
                .max(1.0)
                / 1_000.0,
        ),
        updated_at: now,
        angles: sample_vector(component.angles, 21),
        angle_delta: sample_vector(component.angle_delta, 24),
        base_rotation: placed_rotation,
        size: Envelope::from_curve(component.size, seed.wrapping_add(27)),
        model: choose_name(&component.models, effects, seed.wrapping_add(29)),
        emit_effect: choose_name(&component.emit_effects, effects, seed.wrapping_add(30)),
        density,
        distance_until_emit: density.max(0.001),
        variance: component
            .variance
            .sample(random_unit(seed.wrapping_add(28))),
        seed,
        emission_index: 0,
        depth,
    }
}

fn choose_name(names: &[String], effects: &mut EffectLibrary, seed: u32) -> Option<Arc<str>> {
    (!names.is_empty()).then(|| {
        let index = (seed as usize) % names.len();
        effects.asset_name(&names[index])
    })
}

fn mix_seed(seed: u32, index: u32) -> u32 {
    seed ^ index.wrapping_mul(0x9e37_79b9)
}

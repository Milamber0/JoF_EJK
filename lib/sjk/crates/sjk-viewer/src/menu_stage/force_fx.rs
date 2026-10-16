//! What the preview's model gives off on the SJK UI's Force page
//! ([`crate::player_menu::force_pose::effect`]): the power's beam, puffs, spray or
//! speed trail, as `CG_Player` draws them on a player using it.
//!
//! The preview is a picture of its own, drawn with its own camera, so the world's
//! effects never reach it. It keeps a small effect world instead: its own particles,
//! nested-effect queue, emitters and cadence, spawned from the game's effect files
//! (`force/lightning`, `mp/drain`, `force/rage2`...) and the world's own puff code at the
//! model's hand, chest or head, and advanced with the world's particle physics. Each
//! frame they are built into billboards and effect geometry for the preview's camera
//! and drawn into its effect layer, where the blades go, so what lies beyond the body
//! shows as it does round a blade. No sound is played: the preview is silent.
//!
//! The speed trail is the body itself: two copies of the actor behind it, spaced as a
//! Force-speed run spaces them, seen through at alpha 100 and 50 as `CG_Player` draws
//! them. Each copy is drawn whole, as the body is, into a picture of its own, and the
//! preview's picture (`preview.wgsl`, `TRAIL`) lays them behind the body at those
//! alphas. Drawn see-through surface by surface (`RF_FORCE_ENT_ALPHA`), a copy's
//! overlapping surfaces stacked and its face lost its jaw (Creyon, 13/10/2026); a copy
//! faded as one shows the whole model.
//!
//! The effects play in an empty world of their own ([`sjk_bsp::Bsp::empty`]), not the
//! map the player is on: no wall of the map stops a particle, and no map's light or fog
//! reaches them, so the preview looks the same on every map.

use super::*;
use crate::player_menu::force_pose::Effect;

/// Spacing of the speed trail's copies: `SPEED_TRAIL_DISTANCE` (6) times a Force-speed
/// run's 500 units a second times 0.004, as `speed_trail.rs` spaces them.
const TRAIL_SPACING: f32 = 12.0;
/// How far across the view the runner's back must point for the copies to follow it,
/// and how much farther from the camera each copy stands than the one before.
const TRAIL_SIDEWAYS: f32 = 0.35;
const TRAIL_DEPTH: f32 = 4.0;
/// Half the side of the effects' empty world, round any map's preview origin.
const WORLD_EXTENT: f32 = 1_048_576.0;
/// JoF EJK's `CG_ForceAnimFx` sprays every 120 ms.
const SPRAY_INTERVAL: Duration = Duration::from_millis(120);
/// The chest above the origin, for the sprays (`*chestg`'s height on a standing
/// humanoid), and the middle of the head, for the mind trick.
const CHEST_ABOVE_ORIGIN: f32 = 18.0;
const HEAD_ABOVE_ORIGIN: f32 = 32.0;
/// At most this many preview particles: one beam at a time needs far fewer.
const MAX_PARTICLES: usize = 4_096;

/// The preview's effect world.
#[derive(Default)]
pub(crate) struct ForceFx {
    effect: Option<Effect>,
    particles: Vec<Particle>,
    pending: Option<crate::particle_physics::PendingEffects>,
    aux: crate::effect_aux::Runtime,
    decals: crate::decal_store::DecalStore,
    next_spray: Option<Instant>,
    /// Made the first time an effect is shown.
    gpu: Option<Gpu>,
    /// This frame's billboards and their blend ranges, once built.
    ranges: Option<crate::effect_submission::Ranges>,
    instances: Vec<EntityInstance>,
    blended: [Vec<EntityInstance>; crate::effect_blend::PIPELINE_COUNT],
    /// The speed trail's two copies, for the actor build they were made for.
    trail: Option<(u64, [wgpu::Buffer; 2])>,
    /// Whether the copies are drawn: Speed on, not between its uses.
    trail_on: bool,
    /// The empty world the effects play in.
    world: EmptyWorld,
}

/// A map with nothing in it, the preview effects' world.
struct EmptyWorld(sjk_bsp::Bsp);

impl Default for EmptyWorld {
    fn default() -> Self {
        Self(sjk_bsp::Bsp::empty([-WORLD_EXTENT; 3], [WORLD_EXTENT; 3]))
    }
}

struct Gpu {
    geometry: crate::effect_geometry_gpu::Runtime,
    instance_buffer: wgpu::Buffer,
}

/// Where the actor's left hand is, the way it faces (yaw in degrees) and its origin.
struct Anchor {
    origin: Vec3,
    hand: Option<Vec3>,
    facing: Vec3,
}

fn anchor(actor: &StageActor) -> Anchor {
    let facing = actor.rotation * Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2) * Vec3::X;
    let hand = actor.hands[1]
        .map(|bolt| crate::saber::world_attachment(actor.origin, actor.rotation, bolt).0);
    Anchor {
        origin: actor.origin,
        hand,
        facing,
    }
}

/// The trail's copies of `instance`, see-through, for a camera looking along `forward`.
///
/// In the game the camera runs behind the runner, so the copies left behind him are
/// seen. The preview's camera looks at the model from wherever it was turned to, and
/// copies straight behind a model facing it hid behind the body (Creyon, 12/10/2026).
/// So the copies go behind the runner as the picture shows it: back along `facing`
/// across the view, or to the side when the model faces the camera or away from it,
/// and a little farther from the camera than the body, which covers them where they
/// overlap.
pub(super) fn trail_instances(
    instance: ActorInstance,
    facing: Vec3,
    forward: Vec3,
) -> [ActorInstance; 2] {
    let forward = Vec3::new(forward.x, forward.y, 0.0).normalize_or(Vec3::X);
    let back = -Vec3::new(facing.x, facing.y, 0.0).normalize_or(Vec3::X);
    let across = back - forward * back.dot(forward);
    let away = if across.length() > TRAIL_SIDEWAYS {
        across.normalize()
    } else {
        // Facing the camera or away: to the screen's left.
        Vec3::Z.cross(forward).normalize_or(Vec3::Y)
    };
    std::array::from_fn(|copy| {
        let mut ghost = instance;
        let step = (copy + 1) as f32;
        let behind = away * TRAIL_SPACING * step + forward * TRAIL_DEPTH * step;
        for (axis, offset) in behind.to_array().into_iter().enumerate() {
            ghost.position[axis] += offset;
        }
        ghost
    })
}

/// Whether a spray is due at `now`, and when the next one is.
fn spray_due(next: &mut Option<Instant>, now: Instant) -> bool {
    if next.is_some_and(|next| now < next) {
        return false;
    }
    *next = Some(now + SPRAY_INTERVAL);
    true
}

impl GpuState {
    /// Spawn this frame's share of `effect` at the preview's model while `emitting` (the
    /// gesture is using the power) and advance what it spawned before; with no effect,
    /// or between uses, the particles already out play on and fade.
    pub(super) fn advance_force_fx(
        &mut self,
        effect: Option<Effect>,
        emitting: bool,
        now: Instant,
    ) {
        let fx = &mut self.menu_stage.force_fx;
        let showing = effect.filter(|_| self.menu_stage.preview_only);
        if fx.effect != showing {
            // A new power starts clean: another power's beam does not linger.
            fx.particles.clear();
            fx.aux = crate::effect_aux::Runtime::default();
            fx.next_spray = None;
            fx.effect = showing;
        }
        let Some(vfs) = self.vfs.clone() else {
            return;
        };
        let pending = fx
            .pending
            .get_or_insert_with(crate::particle_physics::PendingEffects::new);
        if let (Some(effect), Some(actor), true) =
            (showing, self.menu_stage.actor.as_ref(), emitting)
        {
            let at = anchor(actor);
            let yaw = at.facing.y.atan2(at.facing.x).to_degrees();
            let frame = crate::actor_world_submission::force_powers::beam_rotation(0.0, yaw);
            let continuous = fx.aux.continuous.due(now);
            let seed = now.saturating_duration_since(epoch()).as_millis() as u32;
            let mut no_audio = None;
            let (name, origin, rotation) = match effect {
                Effect::Beam(name) => (Some(name), at.hand.filter(|_| continuous), frame),
                Effect::Chest(name) => (
                    Some(name),
                    spray_due(&mut fx.next_spray, now)
                        .then(|| at.origin + Vec3::Z * CHEST_ABOVE_ORIGIN + at.facing * 4.0),
                    frame,
                ),
                Effect::OverHead(name) => (
                    Some(name),
                    continuous.then(|| at.origin + Vec3::Z * HEAD_ABOVE_ORIGIN),
                    Quat::from_rotation_z(yaw.to_radians()),
                ),
                Effect::Puffs { grip } => {
                    if let Some(hand) = at.hand.filter(|_| continuous) {
                        // The pair spreads across the view, as the world spreads it.
                        let left = Vec3::Z.cross(at.facing).normalize_or_zero();
                        for _ in 0..if grip { 2 } else { 1 } {
                            crate::actor_world_submission::force_powers::sprites::pair(
                                &mut fx.particles,
                                &mut self.effects,
                                hand,
                                left,
                                grip,
                                seed as i32,
                                now,
                            );
                        }
                    }
                    (None, None, frame)
                }
                // The body itself moves (`force_motion.rs`), or copies of it trail.
                Effect::SpeedTrail | Effect::Jump { .. } | Effect::Throw | Effect::Moves { .. } => {
                    (None, None, frame)
                }
            };
            if let (Some(name), Some(origin)) = (name, origin)
                && fx.particles.len() < MAX_PARTICLES
                && self.effects.contains_definition(name)
            {
                crate::effect_runtime::spawn_effect(
                    &mut fx.particles,
                    &mut fx.aux,
                    &mut self.effects,
                    &vfs,
                    name,
                    origin,
                    now,
                    seed,
                    0,
                    &mut no_audio,
                    rotation,
                );
            }
        }
        if fx.particles.is_empty() {
            return;
        }
        let mut no_audio = None;
        crate::particle_physics::update_and_spawn(
            &mut fx.particles,
            pending,
            &mut fx.aux,
            &mut self.effects,
            &vfs,
            &mut no_audio,
            now,
            &fx.world.0,
            &mut self.trace_scratch,
        );
        fx.particles.truncate(MAX_PARTICLES);
    }

    /// Build this frame's preview billboards and effect geometry for the camera at
    /// `eye`, into `encoder`; nothing when no effect is showing.
    pub(super) fn prepare_force_fx(&mut self, encoder: &mut wgpu::CommandEncoder, eye: Vec3) {
        let fx = &mut self.menu_stage.force_fx;
        fx.ranges = None;
        if fx.particles.is_empty() {
            return;
        }
        if fx.gpu.is_none() {
            let atlas_layout = crate::particle_atlas::layout(&self.device);
            fx.gpu = Some(Gpu {
                geometry: crate::effect_geometry_gpu::Runtime::new(
                    &self.device,
                    &self.camera_layout,
                    &atlas_layout,
                    crate::frame_target::aa::effects::FORMAT,
                ),
                instance_buffer: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("SJK preview Force effect instances"),
                    size: (crate::particle_types::INSTANCE_CAPACITY
                        * std::mem::size_of::<EntityInstance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            });
        }
        let Some(gpu) = fx.gpu.as_mut() else {
            return;
        };
        fx.instances.clear();
        for slot in &mut fx.blended {
            slot.clear();
        }
        let now = Instant::now();
        let mut timing = crate::frame_pacing::budget::Timer::new(now);
        // The preview shows the stock look: a map's recolours of effect pictures (one
        // turns the push puffs red for its "empowered" players) are swapped out while
        // its effects are built, and back in after.
        self.particle_atlas.swap_remap_originals();
        let ranges = crate::effect_submission::prepare(
            &mut timing,
            crate::effect_submission::Inputs {
                geometry: &mut gpu.geometry,
                queue: &self.queue,
                encoder,
                particles: &mut fx.particles,
                decals: &mut fx.decals,
                atlas: &self.particle_atlas,
                now,
                global_seconds: crate::menu::art::motion::seconds() as f32,
                camera: eye,
                field_of_view: preview::FIELD_OF_VIEW_DEGREES,
                bsp: &fx.world.0,
                trace_scratch: &mut self.trace_scratch,
                entity_instances: &mut fx.instances,
                blended: &mut fx.blended,
            },
        );
        self.particle_atlas.swap_remap_originals();
        if !fx.instances.is_empty() {
            self.queue
                .write_buffer(&gpu.instance_buffer, 0, bytemuck::cast_slice(&fx.instances));
        }
        fx.ranges = Some(ranges);
    }

    /// The speed trail's copies for the current actor, made once per model.
    pub(super) fn sync_force_trail(&mut self, on: bool) {
        let fx = &mut self.menu_stage.force_fx;
        fx.trail_on = on;
        let Some(actor) = &self.menu_stage.actor else {
            fx.trail = None;
            return;
        };
        if fx.effect != Some(Effect::SpeedTrail) {
            return;
        }
        if fx
            .trail
            .as_ref()
            .is_some_and(|(build, _)| *build == actor.build)
        {
            return;
        }
        let at = anchor(actor);
        let copies = trail_instances(actor.instance, at.facing, -at.facing);
        let buffers = copies.map(|copy| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("SJK preview speed trail copy"),
                    contents: bytemuck::bytes_of(&copy),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                })
        });
        fx.trail = Some((actor.build, buffers));
    }

    /// Place the speed trail's copies for this frame's camera at `eye`.
    pub(super) fn place_force_trail(&self, eye: Vec3) {
        let fx = &self.menu_stage.force_fx;
        let (Some(actor), Some((build, buffers))) = (&self.menu_stage.actor, &fx.trail) else {
            return;
        };
        if fx.effect != Some(Effect::SpeedTrail) || *build != actor.build {
            return;
        }
        let copies = trail_instances(actor.instance, anchor(actor).facing, actor.origin - eye);
        for (buffer, copy) in buffers.iter().zip(copies) {
            self.queue
                .write_buffer(buffer, 0, bytemuck::bytes_of(&copy));
        }
    }
}

/// The instant seeds count from; any fixed one will do.
fn epoch() -> Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

impl ForceFx {
    /// Whether the speed trail's copies are drawn this frame.
    pub(super) fn shows_trail(&self) -> bool {
        self.effect == Some(Effect::SpeedTrail) && self.trail_on && self.trail.is_some()
    }

    /// Draw the built effects into the preview's effect layer with its `camera`.
    pub(super) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        atlas: &'pass wgpu::BindGroup,
        pipelines: &'pass crate::particle_draw::Pipelines,
    ) {
        let (Some(ranges), Some(gpu)) = (&self.ranges, &self.gpu) else {
            return;
        };
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, atlas, &[]);
        pass.set_vertex_buffer(0, gpu.instance_buffer.slice(..));
        pipelines.draw(pass, ranges.blended(), false);
        gpu.geometry.draw(pass, camera, atlas);
    }

    /// Draw the speed trail's `copy` (0 the nearer) whole, as the body is drawn, with
    /// the actor's surfaces, its `blended` or opaque ones.
    pub(super) fn draw_copy<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        actor: &'pass StageActor,
        copy: usize,
        blended: bool,
    ) {
        let Some((build, buffers)) = &self.trail else {
            return;
        };
        let Some(buffer) = buffers.get(copy).filter(|_| *build == actor.build) else {
            return;
        };
        world_materials.draw_detached(
            pass,
            camera,
            &actor.geometry_binding,
            &actor.vertex_buffer,
            &actor.index_buffer,
            buffer,
            &actor.materials,
            &actor.draws,
            blended,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seen from the side, two copies back along the run, 12 and 24 units, a little
    /// farther from the camera, at alpha 100 then 50.
    #[test]
    fn the_trail_stands_behind_the_runner_and_fades() {
        let instance = ActorInstance::new([100.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        // Running along +x, the camera looking along +y.
        let copies = trail_instances(instance, Vec3::X, Vec3::Y);
        assert_eq!(copies[0].position[0], 88.0);
        assert_eq!(copies[1].position[0], 76.0);
        assert_eq!(copies[0].position[1], TRAIL_DEPTH);
        assert_eq!(copies[1].position[1], 2.0 * TRAIL_DEPTH);
        // Drawn whole, as the body is; the picture fades them.
        assert!(
            copies
                .iter()
                .all(|copy| copy.entity_color == instance.entity_color)
        );
        // Faded at `shaderRGBA[3]` 100 and 50 by the picture.
        assert!(
            include_str!("preview.wgsl")
                .contains("const COPY_ALPHAS = vec2<f32>(100.0 / 255.0, 50.0 / 255.0);")
        );
        // Lit as the model is.
        assert!(
            copies
                .iter()
                .all(|copy| copy.light_ambient == instance.light_ambient)
        );
        // Running at the camera, the copies still stand beside the body on screen,
        // not hidden behind it.
        let facing_camera = trail_instances(instance, -Vec3::Y, Vec3::Y);
        assert!((facing_camera[0].position[0] - 100.0).abs() >= TRAIL_SPACING - 1e-3);
    }

    /// The effects' world is empty: a trace through it hits nothing.
    #[test]
    fn the_effects_play_in_an_empty_world() {
        let world = EmptyWorld::default();
        let trace = world.0.trace_box(
            [0.0, 0.0, 100.0],
            [0.0, 0.0, -100.0],
            sjk_bsp::Aabb::POINT,
            u32::MAX,
        );
        assert!(trace.fraction >= 1.0 && !trace.start_solid);
    }

    #[test]
    fn sprays_come_every_120_ms() {
        let start = Instant::now();
        let mut next = None;
        assert!(spray_due(&mut next, start));
        assert!(!spray_due(&mut next, start + Duration::from_millis(119)));
        assert!(spray_due(&mut next, start + Duration::from_millis(120)));
    }
}

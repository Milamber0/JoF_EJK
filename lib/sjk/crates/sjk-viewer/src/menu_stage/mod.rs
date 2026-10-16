//! The live player model on the menu's stage: while the backdrop camera is
//! on a route with a [`Stage`], the current `model` cvar is skinned into
//! its own vertex/index buffers, lit from the map like any actor, and drawn
//! through the shared material runtime. Changing the model rebuilds the
//! buffers on the spot, so the swap is visible the same frame. The sabers
//! in its hands live in [`sabers`]; the Saber tab's throw in [`throw`]; the
//! hat and cape it wears in [`cosmetics`]. The classic profile shows the
//! same actor in a preview of its own ([`preview`]) instead of on the stage;
//! lightsaber creation's preview shows the sabers alone ([`showcase`]).

mod cosmetics;
mod force_fx;
mod force_motion;
pub(crate) mod preview;
mod prop;
mod sabers;
mod shell;
mod showcase;
mod throw;
mod tracks;

use super::*;
use crate::actor_instance::EntityLight;
use crate::menu_backdrop::Stage;
use crate::player_assets::{GlaCache, load_player_appearance_with, split_model_cvar};
use crate::saber::Attachment;
use crate::world_materials::{DetachedDraw, DetachedMaterials};
use sabers::StageSaber;

/// Q3 `CONTENTS_SOLID`: the only content the stage floor trace stops on.
const CONTENTS_SOLID: u32 = 1;
/// Distance from a standing player's origin down to the soles: the legacy
/// standing box bottom (`DEFAULT_MINS_2`, `codemp/game/bg_public.h`).
const PLAYER_FEET_BELOW_ORIGIN: f32 = 24.0;
/// Farthest a stage may be dropped onto the floor below it. A longer fall
/// means the authored floor is not brush collision (a curved patch, say)
/// and the authored height is kept instead.
const MAX_STAGE_DROP: f32 = 96.0;
/// Stance before the menu has said which sabers are held.
const DEFAULT_STANCE: &str = "BOTH_STAND2";
/// Torso held while the saber is out (`PM_WeaponLightsaber`,
/// `codemp/game/bg_saber.c`: `BOTH_SABERPULL` with `SETANIM_FLAG_HOLD`
/// while `saberInFlight`); the stage plays it whole-body.
const THROW_STANCE: &str = "BOTH_SABERPULL";

/// Stage state owned by [`GpuState`]; empty until a player screen asks for a
/// model.
#[derive(Default)]
pub(crate) struct MenuStage {
    gla_cache: GlaCache,
    actor: Option<StageActor>,
    /// Model cvar whose load failed; not retried until the value changes.
    failed: Option<String>,
    /// `.sab` definitions, read once the first hilt is needed.
    saber_definitions: Option<BTreeMap<String, crate::saber_defs::Definition>>,
    /// The rigid model the model viewer shows in place of the player, and the path whose
    /// load failed (not retried until another is chosen).
    prop: Option<prop::StageProp>,
    prop_failed: Option<String>,
    /// Right and left hand.
    sabers: [Option<StageSaber>; 2],
    request: sabers::Request,
    /// Hilt instances need rewriting (pose or hilts changed).
    sabers_dirty: bool,
    /// Each hand's throw, and its hilt's pose while it is out of the hand.
    throw: [throw::Throw; 2],
    flying: [Option<throw::Pose>; 2],
    /// The hat and cape `color1`/`color2` wear, and what they were loaded for.
    cosmetics: [Option<cosmetics::StageCosmetic>; 2],
    cosmetic_request: cosmetics::Request,
    /// The pieces need placing on the current pose.
    cosmetics_dirty: bool,
    /// The actor is for the classic profile's preview, not the stage: the
    /// world pass and the blade list leave it out.
    preview_only: bool,
    /// The preview shows the sabers alone, laid out as retail's lightsaber
    /// creation spun its hilt; the actor only lends them its light.
    showcase: bool,
    preview: preview::Preview,
    /// The blade the Collection's Shaders tab has the model hold instead of the one
    /// worn (only drawn here, never sent); `None` outside it.
    skin_override: Option<crate::console::collection_panel::PreviewSkin>,
    /// The Collection's Toys tab: the Illuminate holocron floats by the model.
    holocron_wanted: bool,
    /// The Force shell the preview's model wears (the Force page's power).
    shell: Option<shell::StageShell>,
    /// The beam, puffs, spray or trail the preview's model gives off (the same power).
    force_fx: force_fx::ForceFx,
    /// The jump, throw or moves the preview's model shows, and when they began.
    motion: Option<(Instant, crate::player_menu::force_pose::Effect)>,
    /// The gesture the preview's model plays (the power under the pointer's), and when
    /// it began.
    gesture: Option<(Instant, &'static [crate::player_menu::force_pose::Move])>,
    /// The shell the preview's model wears with no gesture to time it (Sense's), and
    /// since when.
    lone_shell: Option<(Instant, crate::player_menu::force_pose::Shell)>,
    /// How far the blades are drawn in, from 0 (out) to 1 (off): a jumping preview's
    /// sabers go off in the air and light again on landing ([`force_motion::blade_in_at`]).
    blades_in: f32,
    /// Actors built so far: the shell and the speed trail are made for one of them.
    actor_builds: u64,
    /// That holocron, lit, fading in and out as the player's own does.
    pub(crate) holocron: crate::illuminate::Holocron,
}

struct StageActor {
    model: String,
    /// Which build this is ([`MenuStage::actor_builds`]).
    build: u64,
    /// Lit as a preview is ([`EntityLight::FALLBACK`], whatever the map), not by the map
    /// where it stands.
    preview_lit: bool,
    /// Animation the actor idles in, by `animation.cfg` name.
    stance: &'static str,
    /// How fast its animations play (1: the game's speed).
    rate: f32,
    preview: PlayerPreview,
    origin: Vec3,
    rotation: Quat,
    /// The way it faces, radians round the vertical (the stage's yaw).
    yaw: f32,
    light: EntityLight,
    /// Right and left hand bolts of the current pose.
    hands: [Option<Attachment>; 2],
    vertex_ranges: Vec<PreviewVertexRange>,
    draws: Vec<DetachedDraw>,
    materials: DetachedMaterials,
    vertex_buffer: wgpu::Buffer,
    geometry_binding: wgpu::BindGroup,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    /// What `instance_buffer` holds: where the actor stands, turned and lit.
    instance: ActorInstance,
    /// The pose skinned last ([`tracks::PoseKey`]); `None` asks for a new skinning.
    current_sample: Option<tracks::PoseKey>,
    /// That pose's bone matrices, which the hands, the hat and the cape are bolted to.
    pose: Vec<[[f32; 4]; 3]>,
    /// The legs' and the torso's animations ([`tracks`]).
    lower: tracks::Track,
    upper: tracks::Track,
    /// The takeover from the pose on screen when the animation last changed, while it
    /// is under way ([`tracks::pose_matrices`]).
    from: Option<Takeover>,
}

/// A takeover under way: the bone matrices on screen when it began, when, and how the
/// new animation takes over from them.
struct Takeover {
    pose: Vec<[[f32; 4]; 3]>,
    began: Instant,
    how: crate::player_menu::force_pose::Takeover,
}

impl GpuState {
    /// Load, swap or animate the stage model for this frame, and feed the
    /// menu images (model icons, map preview) to the UI atlas.
    pub(crate) fn update_menu_stage(&mut self, now: Instant) {
        menu::upload_menu_images(
            &mut self.client_menu,
            &mut self.ui_shapes,
            &self.device,
            &self.queue,
            self.vfs.as_ref(),
            &self.shaders,
        );
        // Last frame's preview, if any, is what the profile can show now.
        let ready = self.menu_stage.preview_ready();
        if let Some(menu) = &mut self.client_menu {
            menu.set_preview_ready(ready);
        }
        // The Collection's Shaders, Toys and Nameplates want the player's model.
        let wish = self
            .console
            .as_ref()
            .and_then(crate::console::ViewerConsole::collection_stage_wish);
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.set_collection_stage(wish.is_some(), console);
        }
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &mut self.console) {
            if console.model_viewer_needs_models()
                && let Some(vfs) = self.vfs.as_deref()
            {
                console.set_model_viewer_models(crate::console::model_viewer_scan::scan(vfs));
            }
            menu.follow_model_viewer(console);
        }
        self.menu_stage.skin_override = wish.map(|wish| wish.skin);
        self.menu_stage.holocron_wanted = wish.is_some_and(|wish| wish.holocron);
        let staged = menu_backdrop::standalone_menu_visible(self)
            .then(|| {
                self.client_menu
                    .as_ref()
                    .and_then(menu::ClientMenu::stage_model)
            })
            .flatten()
            .map(|(stage, model)| (Some(stage), model, None));
        // Otherwise the classic profile's preview may want the actor, or the
        // Collection's where it has no stage. In a match the menu world's stage is not
        // in this world.
        let sessions = self.live_session.is_some() || self.demo_session.is_some();
        let on_stage = staged.is_some();
        let wanted = staged.or_else(|| {
            let (stage, model, preview) = self
                .client_menu
                .as_ref()
                .and_then(|menu| menu.preview_model().or_else(|| menu.collection_preview()))?;
            Some((stage.filter(|_| !sessions), model, Some(preview)))
        });
        let backstage = match (wish, on_stage, &wanted) {
            (None, _, _) | (Some(_), false, None) => {
                crate::console::collection_panel::Backstage::None
            }
            (Some(_), true, _) => crate::console::collection_panel::Backstage::World {
                head: self.stage_head(),
            },
            (Some(_), false, Some(_)) => {
                crate::console::collection_panel::Backstage::Preview { ready }
            }
        };
        if let Some(console) = &mut self.console {
            console.set_collection_backstage(backstage);
        }
        let Some((stage, model, preview)) = wanted else {
            self.menu_stage.actor = None;
            self.menu_stage.preview.wanted = None;
            return;
        };
        self.menu_stage.preview_only = preview.is_some();
        self.menu_stage.showcase = preview.is_some_and(|preview| preview.showcase);
        self.menu_stage.preview.wanted = preview.map(|preview| {
            let viewport = [
                self.configuration.width as f32,
                self.configuration.height as f32,
            ];
            let rect = preview.area.window_rect(viewport);
            [rect.width.round() as u32, rect.height.round() as u32]
        });
        self.menu_stage.preview.room = preview.map_or(1.0, |preview| preview.room);
        self.menu_stage.preview.angle = preview.and_then(|preview| preview.angle);
        // The preview holds no sabers (retail's and JoF's held none) but on
        // lightsaber creation.
        let wanted = match preview {
            Some(preview) if !preview.sabers => None,
            _ => self
                .client_menu
                .as_ref()
                .map(menu::ClientMenu::stage_sabers),
        };
        // Only the Saber page over the menu map's stage (not a preview)
        // throws the saber to its shot.
        let thrown = preview.is_none() && wanted.as_ref().is_some_and(|sabers| sabers.thrown);
        let focus = self
            .client_menu
            .as_ref()
            .and_then(menu::ClientMenu::saber_focus);
        // A jump, throw or moves preview runs on its own clock from when it began.
        let moving = preview.and_then(|preview| preview.effect).filter(|effect| {
            matches!(
                effect,
                crate::player_menu::force_pose::Effect::Jump { .. }
                    | crate::player_menu::force_pose::Effect::Throw
                    | crate::player_menu::force_pose::Effect::Moves { .. }
            )
        });
        if self.menu_stage.motion.map(|(_, effect)| effect) != moving {
            self.menu_stage.motion = moving.map(|effect| (now, effect));
        }
        let motion_seconds = self.menu_stage.motion.map_or(0.0, |(started, _)| {
            now.saturating_duration_since(started).as_secs_f32()
        });
        // The jump's and the moves' poses come from their script; `Some(None)` stands.
        let scripted = match self.menu_stage.motion {
            // A jump moves the whole body, arms and all, as the game's does (Creyon's
            // videos of the game, 13/10/2026: the torso keeping the stance held the hands
            // where a jumping player's are not).
            Some((_, crate::player_menu::force_pose::Effect::Jump { height })) => Some(
                force_motion::jump_at(height, motion_seconds)
                    .map(|name| (name, tracks::HOVER_TAKEOVER)),
            ),
            // The throwing arm stays out while the saber flies, then rests with it.
            Some((_, crate::player_menu::force_pose::Effect::Throw)) => Some(
                force_motion::saber_out_at(motion_seconds)
                    .then_some((THROW_STANCE, tracks::HOVER_TAKEOVER)),
            ),
            Some((_, crate::player_menu::force_pose::Effect::Moves { moves, stance })) => {
                self.menu_stage.actor.as_ref().map(|actor| {
                    Some(force_motion::moves_at(
                        moves,
                        stance,
                        |name| actor.preview.config.get(name).map_or(0.0, travel_seconds),
                        motion_seconds,
                    ))
                })
            }
            _ => None,
        };
        // How the moves' next pose takes over, once the moves are under way; the first,
        // and anything else, as the pointer's change does.
        let under_way = match (self.menu_stage.motion, self.menu_stage.actor.as_ref()) {
            (
                Some((_, crate::player_menu::force_pose::Effect::Moves { moves, stance })),
                Some(actor),
            ) => actor.stance == stance || moves.iter().any(|step| step.animation == actor.stance),
            _ => false,
        };
        let takeover = scripted
            .flatten()
            .filter(|_| under_way)
            .map_or(tracks::HOVER_TAKEOVER, |(_, how)| how);
        let scripted = scripted.map(|step| step.map(|(name, _)| name));
        // A jumping model is seen to rise and come back down to its spot.
        self.menu_stage.preview.lift = match self.menu_stage.motion {
            Some((_, crate::player_menu::force_pose::Effect::Jump { height })) => {
                force_motion::rise_seen(height, motion_seconds)
            }
            _ => 0.0,
        };
        // Its sabers go off for the jump and light again on landing.
        self.menu_stage.blades_in = match self.menu_stage.motion {
            Some((_, crate::player_menu::force_pose::Effect::Jump { height })) => {
                force_motion::blade_in_at(height, motion_seconds)
            }
            _ => 0.0,
        };
        // A gesture plays from its script on its own clock: once and a wait, held, or
        // Heal's whole move ([`force_pose::gesture`]).
        let gesture = preview.and_then(|preview| preview.pose);
        let same = match (self.menu_stage.gesture, gesture) {
            (Some((_, old)), Some(new)) => std::ptr::eq(old, new),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.menu_stage.gesture = gesture.map(|moves| (now, moves));
        }
        let gesture_step = match (self.menu_stage.gesture, &self.menu_stage.actor) {
            (Some((started, moves)), Some(actor)) => Some(force_motion::script_at(
                moves,
                |name| actor.preview.config.get(name).map_or(0.0, travel_seconds),
                now.saturating_duration_since(started).as_secs_f32(),
            )),
            _ => None,
        };
        let takeover = match (scripted, gesture_step) {
            (None, Some(step)) => step.takeover,
            _ => takeover,
        };
        // The power's effect plays while its gesture does and while the power stays on
        // after it, not in the wait after that; the puffs come from the hand only while
        // the gesture puts it out (the team powers' shells stay on, their puffs do not).
        let emitting = gesture_step.is_none_or(|step| {
            step.effect
                && (step.animation.is_some()
                    || !matches!(
                        preview.and_then(|preview| preview.effect),
                        Some(crate::player_menu::force_pose::Effect::Puffs { .. })
                    ))
        });
        let pose = scripted
            .flatten()
            .or(gesture_step.and_then(|step| step.animation));
        let (stance, request) = match (wanted, preview) {
            (Some(sabers), _) => (
                Some(pose.unwrap_or(sabers.stance)),
                (!self.menu_stage.request.matches(&sabers)).then(|| sabers::Request::from(&sabers)),
            ),
            (None, Some(preview)) => (
                Some(preview.stance),
                (self.menu_stage.request != sabers::Request::default())
                    .then(sabers::Request::default),
            ),
            (None, None) => (None, None),
        };
        let loaded = self
            .menu_stage
            .actor
            .as_ref()
            .map(|actor| actor.model.as_str());
        // A preview is lit the same on every map; the stage by the map it stands in.
        let relit = self
            .menu_stage
            .actor
            .as_ref()
            .is_some_and(|actor| actor.preview_lit != self.menu_stage.preview_only);
        if loaded != Some(model) || relit {
            if self.menu_stage.failed.as_deref() == Some(model) {
                return;
            }
            let model = model.to_owned();
            match self.build_stage_actor(stage, &model, now) {
                Ok(actor) => {
                    self.menu_stage.actor = Some(actor);
                    self.menu_stage.cosmetics_dirty = true;
                }
                Err(error) => {
                    eprintln!("player stage could not load {model}: {error}");
                    self.menu_stage.failed = Some(model);
                    return;
                }
            }
        }
        self.sync_stage_prop(now);
        if let Some(request) = request {
            self.sync_stage_sabers(request);
        }
        self.sync_stage_cosmetics();
        self.advance_stage_throw(thrown, focus, now);
        if matches!(
            self.menu_stage.motion,
            Some((_, crate::player_menu::force_pose::Effect::Throw))
        ) {
            self.advance_preview_throw(motion_seconds);
        }
        // Everything plays at the game's own speed, as a player sees it (Creyon,
        // 13/10/2026: the saber skills at 0.9 felt slowed down), Speed's run fast while
        // the power is on, at a run's own pace while it is off.
        let speeding = matches!(
            preview.and_then(|preview| preview.effect),
            Some(crate::player_menu::force_pose::Effect::SpeedTrail)
        ) && emitting;
        let rate = if speeding {
            force_motion::SPEED_RUN_RATE
        } else {
            1.0
        };
        if let Some(actor) = &mut self.menu_stage.actor {
            actor.rate = rate;
        }
        if let Some(stance) = stance {
            let held = self.menu_stage.throw.iter().all(throw::Throw::is_held);
            self.set_stage_stance(if held { stance } else { THROW_STANCE }, now, takeover);
        }
        // The shell shows while the power is on: with its gesture and a while after, or,
        // with no gesture (Sense), on and off in turn.
        let shell = preview.and_then(|preview| preview.shell);
        let lone = shell.filter(|_| gesture_step.is_none());
        if self.menu_stage.lone_shell.map(|(_, worn)| worn) != lone {
            self.menu_stage.lone_shell = lone.map(|worn| (now, worn));
        }
        let shell_on = match (gesture_step, self.menu_stage.lone_shell) {
            (Some(step), _) => step.effect,
            (None, Some((since, _))) => {
                force_motion::lone_shell_on(now.saturating_duration_since(since).as_secs_f32())
            }
            (None, None) => true,
        };
        self.sync_stage_shell(shell.filter(|_| shell_on));
        self.advance_force_fx(preview.and_then(|preview| preview.effect), emitting, now);
        self.sync_force_trail(emitting);
        // The showcase draws no actor: its pose and what it wears can wait.
        if self.menu_stage.showcase {
            return;
        }
        if let Err(error) = self.animate_stage_actor(now) {
            eprintln!("player stage animation stopped: {error}");
            self.menu_stage.actor = None;
        }
        self.place_stage_cosmetics();
    }

    /// Where the top of the stage model's head is in the window this frame, from the
    /// last frame's view: the Collection's Nameplates stand the plate there.
    fn stage_head(&self) -> Option<[f32; 2]> {
        let actor = self.menu_stage.actor.as_ref()?;
        if self.menu_stage.preview_only {
            return None;
        }
        // The top of the head in the stage's stance (a saber stance, knees bent): about
        // 36 units over the origin, under the standing box's top (40), as measured on a
        // world shot.
        let head = actor.origin + Vec3::Z * 36.0;
        self.frame_view?.project(head)
    }

    /// The eye and yaw (radians) of the model on the stage, while the Collection wants
    /// the Illuminate holocron by it.
    pub(crate) fn stage_holocron_anchor(&self) -> Option<(Vec3, f32)> {
        if !self.menu_stage.holocron_wanted || self.menu_stage.preview_only {
            return None;
        }
        let actor = self.menu_stage.actor.as_ref()?;
        // A standing player's eye: `DEFAULT_VIEWHEIGHT` over the origin.
        Some((actor.origin + Vec3::Z * 36.0, actor.yaw))
    }

    /// Restart the actor in `stance` when the menu's style changed it. A
    /// model without that sequence keeps the one it has.
    fn set_stage_stance(
        &mut self,
        stance: &'static str,
        now: Instant,
        takeover: crate::player_menu::force_pose::Takeover,
    ) {
        let Some(actor) = &mut self.menu_stage.actor else {
            return;
        };
        if actor.stance == stance {
            // The same animation at another speed (Speed's run on and off) carries on
            // where it is, faster or slower.
            if actor.upper.rate != actor.rate {
                actor.lower.set_rate(actor.rate, now);
                actor.upper.set_rate(actor.rate, now);
                actor.current_sample = None;
            }
            return;
        }
        if let Some(sequence) = actor.preview.config.get(stance).cloned() {
            // A move that starts where the shown pose is carries straight on; anything
            // else takes over as `takeover` says: a saber skill's next move as the game
            // takes one over from the last (most of the game's swings start a few units
            // off where the last ended), a change of power over a quarter of a second.
            // The old sequence's last frame when it ends (the script moves on as it gets
            // there, give or take a frame), else the frame it shows.
            let old = &actor.upper.sequence;
            let shown = if old.loop_frame == -1 {
                crate::animation_timing::animation_sample(old, f32::MAX / 4.0).0
            } else {
                actor.upper.sample(now).0.0
            };
            let first = crate::animation_timing::animation_sample(&sequence, 0.0).0;
            // The takeover starts from the pose on screen, whatever takeover it was in.
            if !tracks::poses_meet(&actor.preview.animation, shown, first) && !actor.pose.is_empty()
            {
                actor.from = Some(Takeover {
                    pose: actor.pose.clone(),
                    began: now,
                    how: takeover,
                });
            }
            actor.lower.start(sequence.clone(), now, actor.rate);
            actor.upper.start(sequence.clone(), now, actor.rate);
            actor.preview.sequence = sequence;
            actor.current_sample = None;
        }
        actor.stance = stance;
    }

    fn build_stage_actor(
        &mut self,
        stage: Option<Stage>,
        model: &str,
        now: Instant,
    ) -> Result<StageActor, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("player stage has no VFS")?;
        let (directory, variant) = split_model_cvar(model);
        let mut preview = load_player_appearance_with(
            &vfs,
            &directory,
            variant,
            [0.0; 3],
            0.0,
            &mut self.menu_stage.gla_cache,
        )?;
        preview.origin = [0.0; 3];
        preview.yaw = 0.0;
        let stance = self
            .menu_stage
            .actor
            .as_ref()
            .map_or(DEFAULT_STANCE, |actor| actor.stance);
        if let Some(sequence) = preview.config.get(stance).cloned() {
            preview.sequence = sequence;
        }
        let started = now;
        let sequence = preview.sequence.clone();
        let frame = animation_frame(&preview.sequence, 0.0);
        let mut flattened = FlattenedScene::default();
        let (actor_draws, vertex_ranges) = append_actor_mesh(&mut flattened, &preview, frame)?;
        let materials = self.world_materials.compile_detached(
            &self.device,
            &self.queue,
            &vfs,
            &self.shaders,
            &flattened.materials,
        )?;
        let draws = actor_draws
            .into_iter()
            .map(|draw| DetachedDraw {
                indices: draw.indices,
                material: draw.material,
            })
            .collect();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK player stage vertices"),
                contents: bytemuck::cast_slice(&flattened.vertices),
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::STORAGE,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK player stage indices"),
                contents: bytemuck::cast_slice(&flattened.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let quads = crate::shared_geometry::quads::upload(
            &self.device,
            flattened.vertices.len(),
            &flattened.indices,
            0,
        );
        let geometry_binding =
            crate::shared_geometry::quads::bind(&self.device, &vertex_buffer, &quads);
        let (origin, yaw) = match stage {
            Some(stage) => (self.stage_floor(stage), stage.yaw),
            None => (self.preview_origin(), 0.0),
        };
        let rotation =
            weapon_view::actor_world_rotation(Quat::from_rotation_z(yaw.to_radians()).to_array());
        // A preview is a picture of the model, not of the map: it is lit as retail lit a
        // menu model with no world (`R_SetupEntityLighting` without a light grid), the
        // same on every map. The stage stands in the menu map and takes its light.
        let preview_lit = self.menu_stage.preview_only;
        let light = if preview_lit {
            EntityLight::FALLBACK
        } else {
            self.entity_lighting.sample(&self.bsp, origin, &[])
        };
        let mut instance = ActorInstance::new(origin, rotation.to_array(), [1.0; 3]);
        instance.set_light(light);
        let instance_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK player stage instance"),
                contents: bytemuck::bytes_of(&instance),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.menu_stage.sabers_dirty = true;
        let pose = preview.animation.frame_matrices(frame).unwrap_or_default();
        self.menu_stage.actor_builds += 1;
        Ok(StageActor {
            model: model.to_owned(),
            build: self.menu_stage.actor_builds,
            preview_lit,
            stance,
            rate: 1.0,
            hands: sabers::hand_attachments(&preview, frame),
            preview,
            origin: Vec3::from_array(origin),
            rotation,
            yaw: yaw.to_radians(),
            light,
            vertex_ranges,
            draws,
            materials,
            vertex_buffer,
            geometry_binding,
            index_buffer,
            instance_buffer,
            instance,
            current_sample: None,
            pose,
            lower: tracks::Track::new(sequence.clone(), started),
            upper: tracks::Track::new(sequence, started),
            from: None,
        })
    }

    /// Where a preview actor stands without a stage, so the map lights it:
    /// the local player's origin in a match, else the camera's position.
    fn preview_origin(&self) -> [f32; 3] {
        self.live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .map_or(self.camera_position.to_array(), |snapshot| {
                snapshot.player.origin()
            })
    }

    /// Settle the stage origin onto the floor just below the authored point
    /// (which names the floor height, give or take) so the model stands on
    /// the map. Player models hang `PLAYER_FEET_BELOW_ORIGIN` under their
    /// origin, so the origin sits that far above the floor.
    fn stage_floor(&self, stage: Stage) -> [f32; 3] {
        let [x, y, z] = stage.origin;
        let start = [x, y, z + PLAYER_FEET_BELOW_ORIGIN];
        let end = [x, y, z - MAX_STAGE_DROP];
        let trace = self.bsp.trace_box(start, end, Aabb::POINT, CONTENTS_SOLID);
        let floor = if trace.start_solid || trace.fraction >= 1.0 {
            z
        } else {
            trace.end_position[2]
        };
        [x, y, floor + PLAYER_FEET_BELOW_ORIGIN]
    }

    fn animate_stage_actor(&mut self, now: Instant) -> Result<(), Box<dyn Error>> {
        let Some(actor) = &mut self.menu_stage.actor else {
            return Ok(());
        };
        // Each half between its clip's frames (most run at 20 a second), and a new clip
        // taking over from the pose on screen when it began ([`tracks`]), so nothing
        // steps.
        let weight = actor.from.as_ref().map_or(1.0, |from| {
            tracks::takeover(
                now.saturating_duration_since(from.began).as_secs_f32(),
                from.how,
            )
        });
        if weight >= 1.0 {
            actor.from = None;
        }
        let key = tracks::PoseKey::new(actor.lower.sample(now), actor.upper.sample(now), weight);
        if actor.current_sample == Some(key) {
            return Ok(());
        }
        let preview = &actor.preview;
        let current = key.current();
        let from = actor.from.as_ref().map(|from| from.pose.as_slice());
        let matrices = tracks::pose_matrices(&preview.animation, &key, "lower_lumbar", from)
            .or_else(|_| preview.animation.frame_matrices(current.upper_frames.0))?;
        let surfaces = preview
            .mesh
            .skin_pose_matrices(&preview.skin, 0, &matrices)?;
        for range in &actor.vertex_ranges {
            let surface = surfaces
                .get(range.surface_index)
                .ok_or("stage model surface disappeared")?;
            if surface.vertices.len() != range.vertices.len() {
                return Err("stage model vertex count changed".into());
            }
            let vertices = surface
                .vertices
                .iter()
                .map(|vertex| preview_gpu_vertex(preview, vertex))
                .collect::<Vec<_>>();
            let byte_offset = u64::try_from(range.vertices.start)?
                .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
                .ok_or("stage model buffer offset overflow")?;
            self.queue.write_buffer(
                &actor.vertex_buffer,
                byte_offset,
                bytemuck::cast_slice(&vertices),
            );
        }
        actor.hands = sabers::hand_attachments_at(preview, &matrices);
        actor.pose = matrices;
        actor.current_sample = Some(key);
        self.menu_stage.sabers_dirty = true;
        self.menu_stage.cosmetics_dirty = true;
        Ok(())
    }
}

/// How long `sequence` takes to reach its last frame, in seconds: one frame less than
/// its full length (`CG_SetLerpFrameAnimation`'s frame time, each frame rounded up to
/// whole milliseconds), as its last frame is reached a frame's time before that. The saber skills' moves follow one another after this long;
/// after the full length each held its last frame a frame's time, a pause between the
/// swings (Creyon, 13/10/2026).
fn travel_seconds(sequence: &sjk_model::AnimationSequence) -> f32 {
    let rate = sequence.frames_per_second.abs();
    if rate <= 0.0 {
        return 0.0;
    }
    sequence.frame_count.saturating_sub(1) as f32 * (1_000.0 / rate).ceil() / 1_000.0
}

impl MenuStage {
    /// Draw the stage model's opaque or blended surfaces into the world
    /// pass (nothing while the actor is the classic preview's).
    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        blended: bool,
    ) {
        if !self.preview_only {
            self.draw_parts(pass, world_materials, camera, blended);
        }
    }

    /// Whether the classic preview has a frame for the UI to show.
    pub(crate) fn preview_ready(&self) -> bool {
        self.preview_only && self.preview.ready
    }

    /// The actor, its hilts and its cosmetics, with `camera`; the hilts
    /// alone for the showcase.
    fn draw_parts<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        blended: bool,
    ) {
        let Some(actor) = &self.actor else {
            return;
        };
        // The model viewer's rigid model stands in for the player, and what it wears.
        if let Some(prop) = &self.prop {
            prop.draw(pass, world_materials, camera, blended);
            return;
        }
        if self.showcase {
            for saber in self.sabers.iter().flatten() {
                saber.draw(pass, world_materials, camera, blended);
            }
            return;
        }
        world_materials.draw_detached(
            pass,
            camera,
            &actor.geometry_binding,
            &actor.vertex_buffer,
            &actor.index_buffer,
            &actor.instance_buffer,
            &actor.materials,
            &actor.draws,
            blended,
        );
        for saber in self.sabers.iter().flatten() {
            saber.draw(pass, world_materials, camera, blended);
        }
        for cosmetic in self.cosmetics.iter().flatten() {
            cosmetic.draw(pass, world_materials, camera, blended);
        }
        if let Some(shell) = &self.shell {
            shell.draw(pass, world_materials, camera, actor, blended);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(loop_frame: i32) -> sjk_model::AnimationSequence {
        sjk_model::AnimationSequence {
            name: "BOTH_FORCEPUSH".into(),
            first_frame: 0,
            frame_count: 20,
            loop_frame,
            frames_per_second: 20.0,
        }
    }

    /// A move hands over to the next as it reaches its last frame: a frame's time
    /// before its full length, so it does not stand still a frame between swings.
    #[test]
    fn a_move_hands_over_on_reaching_its_last_frame() {
        let once = sequence(-1);
        assert!((travel_seconds(&once) - 0.95).abs() < 1e-6);
        let (last, _, _) = crate::animation_timing::animation_sample(&once, 0.95);
        assert_eq!(last, 19);
        let (before, _, _) = crate::animation_timing::animation_sample(&once, 0.94);
        assert_eq!(before, 18);
    }

    /// Speed's run plays as fast as the game runs a speeding player's legs.
    #[test]
    fn speed_runs_as_fast_as_in_the_game() {
        assert_eq!(force_motion::SPEED_RUN_RATE, 1.7);
    }
}

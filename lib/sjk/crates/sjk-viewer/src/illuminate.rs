//! Illuminate, SJK's own toy (`toy_illuminate`; not a Force power):
//! a holocron that floats by the local player's left shoulder, turning slowly, and
//! lights the way with a warm point light. No game server knows of it: its lit state
//! travels through the SJK hub as part of the player's look (`looks.rs`), so other SJK
//! players on the server see it by that player ([`Others`]). The `toy_illuminate`
//! command turns it on and off, always available; the lit state is the client's run
//! only, not saved. In first person only its light shows (the
//! cube is drawn for mirrors, like the body); another player's cube always shows while
//! that player is drawn, except a followed (spectated) player's in first person, which
//! is drawn as one's own.
//!
//! Its model, pictures and shader are bundled ([`FILES`], made by
//! `scripts/holocron_assets.py`) and mounted below all game data, so a PK3 with
//! the same paths replaces them.

use crate::GpuState;
use crate::actor_instance::ActorInstance;
use crate::dynamic_lights::PointLight;
use glam::{Quat, Vec3};
use sjk_protocol::{EntityState, PlayerState};
use sjk_vfs::{VfsError, VirtualFileSystem};
use std::time::Instant;

/// The toy's console command (a toy is a `toy_<name>` command).
pub(crate) const COMMAND: &str = "toy_illuminate";
/// The command's old name, while it was a Force-wheel entry: still runs
/// [`COMMAND`] for a bind or a wheel choice saved under it, but is not listed.
pub(crate) const OLD_COMMAND: &str = "force_illuminate";
/// The setting that put it on the Force wheel, retired: a saved one is dropped, so
/// that neither 0 (off the wheel) nor 1 (on it) means anything now.
pub(crate) const OLD_CVAR: &str = "cg_illuminate";
pub(crate) const MODEL: &str = "models/sjk/holocron.md3";

/// A bundled file's bytes, by its name in `assets/holocron`.
macro_rules! bundled {
    ($name:literal) => {
        include_bytes!(concat!("../assets/holocron/", $name))
    };
}

/// The loot-box tiers' ids, in rising rarity (`holocron_<tier>` in the game paths).
#[cfg(test)]
const TIERS: [&str; 4] = ["uncommon", "rare", "legendary", "mythical"];

/// The bundled files at their game paths: the holocron, then the four tiers' models,
/// faces, glows and icons (large and small), the mythical sheen and the tiers' shader,
/// then the locked look (the Profile screen's tier the player holds none of).
///
/// A tier has a model of its own because a surface's shader is named by its model file:
/// the shared `holocron.md3` always draws `models/sjk/holocron`.
const FILES: [(&str, &[u8]); 30] = [
    (MODEL, bundled!("holocron.md3")),
    ("models/sjk/holocron.jpg", bundled!("holocron.jpg")),
    (
        "models/sjk/holocron_glow.jpg",
        bundled!("holocron_glow.jpg"),
    ),
    ("gfx/sjk/toy_illuminate.png", bundled!("toy_illuminate.png")),
    ("shaders/sjk_holocron.shader", bundled!("holocron.shader")),
    (
        "models/sjk/holocron_uncommon.md3",
        bundled!("holocron_uncommon.md3"),
    ),
    (
        "models/sjk/holocron_uncommon.jpg",
        bundled!("holocron_uncommon.jpg"),
    ),
    (
        "models/sjk/holocron_uncommon_glow.jpg",
        bundled!("holocron_uncommon_glow.jpg"),
    ),
    (
        "gfx/sjk/holocron_uncommon.png",
        bundled!("holocron_uncommon.png"),
    ),
    (
        "gfx/sjk/holocron_uncommon_small.png",
        bundled!("holocron_uncommon_small.png"),
    ),
    (
        "models/sjk/holocron_rare.md3",
        bundled!("holocron_rare.md3"),
    ),
    (
        "models/sjk/holocron_rare.jpg",
        bundled!("holocron_rare.jpg"),
    ),
    (
        "models/sjk/holocron_rare_glow.jpg",
        bundled!("holocron_rare_glow.jpg"),
    ),
    ("gfx/sjk/holocron_rare.png", bundled!("holocron_rare.png")),
    (
        "gfx/sjk/holocron_rare_small.png",
        bundled!("holocron_rare_small.png"),
    ),
    (
        "models/sjk/holocron_legendary.md3",
        bundled!("holocron_legendary.md3"),
    ),
    (
        "models/sjk/holocron_legendary.jpg",
        bundled!("holocron_legendary.jpg"),
    ),
    (
        "models/sjk/holocron_legendary_glow.jpg",
        bundled!("holocron_legendary_glow.jpg"),
    ),
    (
        "gfx/sjk/holocron_legendary.png",
        bundled!("holocron_legendary.png"),
    ),
    (
        "gfx/sjk/holocron_legendary_small.png",
        bundled!("holocron_legendary_small.png"),
    ),
    (
        "models/sjk/holocron_mythical.md3",
        bundled!("holocron_mythical.md3"),
    ),
    (
        "models/sjk/holocron_mythical.jpg",
        bundled!("holocron_mythical.jpg"),
    ),
    (
        "models/sjk/holocron_mythical_glow.jpg",
        bundled!("holocron_mythical_glow.jpg"),
    ),
    (
        "gfx/sjk/holocron_mythical.png",
        bundled!("holocron_mythical.png"),
    ),
    (
        "gfx/sjk/holocron_mythical_small.png",
        bundled!("holocron_mythical_small.png"),
    ),
    (
        "models/sjk/holocron_mythical_sheen.jpg",
        bundled!("holocron_mythical_sheen.jpg"),
    ),
    (
        "shaders/sjk_holocron_tiers.shader",
        bundled!("holocron_tiers.shader"),
    ),
    (
        "models/sjk/holocron_locked.md3",
        bundled!("holocron_locked.md3"),
    ),
    (
        "models/sjk/holocron_locked.jpg",
        bundled!("holocron_locked.jpg"),
    ),
    (
        "shaders/sjk_holocron_locked.shader",
        bundled!("holocron_locked.shader"),
    ),
];

/// Where the holocron floats, from the eye in the view's yaw frame (x forward,
/// y left): a little behind and to the left, just under eye height.
const OFFSET: Vec3 = Vec3::new(-6.0, 18.0, -4.0);
/// How far it trails behind a moving player at most, and the jump past which it
/// is placed at once (a teleport, a respawn).
const MAX_LAG: f32 = 20.0;
const SNAP: f32 = 96.0;
/// Rate of its follow, per second.
const FOLLOW: f32 = 12.0;
/// Seconds to appear or go out.
const FADE: f32 = 0.3;
/// Its bob, up and down, and the bob's period in seconds.
const BOB: f32 = 1.0;
const BOB_PERIOD: f32 = 2.8;
/// Its turn about the vertical, radians per second, and its tilt, so that the
/// top shows as it turns.
const SPIN: f32 = 0.6;
const TILT: [f32; 2] = [0.38, 0.28];
/// The light: reach in units and a warm white.
const RADIUS: f32 = 300.0;
const COLOR: [f32; 3] = [1.5, 1.3, 1.0];
/// Other players' holocrons that light the world, the nearest the camera: the frame's
/// lights are few (`MAX_POINT_LIGHTS`) and the weapons need theirs. Their cubes
/// always show.
const OTHERS_LIT: usize = 4;
/// Slots of other players a holocron can float by, as the looks hold.
const SLOTS: usize = crate::looks::SLOTS;
/// How far apart in their bob and turn the holocrons of two slots are, in seconds.
const SLOT_PHASE: f32 = 0.37;
/// The eye above the origin of a player whose box the entity does not send:
/// `DEFAULT_VIEWHEIGHT` (`bg_public.h`).
const EYE_HEIGHT: f32 = 36.0;
/// `ET_PLAYER`, and the entity flags and powerup that hide a player: `EF_DEAD`,
/// `EF_NODRAW` and `PW_CLOAKED` (`bg_public.h`).
const ET_PLAYER: u8 = 1;
const EF_DEAD: u32 = 1 << 1;
const EF_NODRAW: u32 = 1 << 8;
const PW_CLOAKED: u32 = 1 << 11;

/// Mount the bundled files.
pub(crate) fn mount(vfs: &mut VirtualFileSystem) -> Result<(), VfsError> {
    vfs.mount_memory("SJK holocron", FILES.iter().copied())?;
    Ok(())
}

/// `PMF_FOLLOW`: the player state is that of the player being followed.
const PMF_FOLLOW: u16 = 4096;

/// Whether a holocron can show by the player whose state is `player`: alive, playing
/// and not at the intermission.
fn alive(player: &PlayerState) -> bool {
    // `pmtype_t`: PM_SPECTATOR 4, PM_DEAD 5, PM_INTERMISSION 7, PM_SPINTERMISSION 8.
    player.health() > 0 && !matches!(player.movement_type(), 4 | 5 | 7 | 8)
}

/// Whether the local player's own holocron can show: [`alive`], and not watching
/// someone else.
fn playing(player: &PlayerState) -> bool {
    alive(player) && player.movement_flags() & PMF_FOLLOW == 0
}

/// Where the holocron of the player the view follows floats: by the view's eye
/// (`view`, its eye and yaw) as one's own does, while the followed player's state
/// (`player`, the snapshot's) is [`alive`]. Their entity is not in the snapshot.
fn followed_anchor(player: &PlayerState, view: Option<(Vec3, f32)>) -> Option<(Vec3, f32)> {
    view.filter(|_| alive(player))
}

/// Where and how the holocron is drawn this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Pose {
    pub(crate) position: Vec3,
    pub(crate) rotation: Quat,
    /// 0 out, 1 fully there: its size and its light's reach.
    pub(crate) level: f32,
}

/// One holocron's state: on or off, and where it floats. The local player's, and one
/// per other player ([`Others`]), step alike.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Holocron {
    on: bool,
    level: f32,
    /// Its centre without the bob, following the player.
    position: Option<Vec3>,
    last: Option<Instant>,
    /// An eye and yaw to float by without a session, for the world shots.
    #[cfg(test)]
    pub(crate) shot_anchor: Option<(Vec3, f32)>,
}

impl Holocron {
    pub(crate) fn toggle(&mut self) {
        self.on = !self.on;
    }

    /// Light it (`true`) or put it out.
    pub(crate) fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    /// Whether it is lit (it may still be fading in, or out of sight).
    pub(crate) fn lit(&self) -> bool {
        self.on
    }

    /// Where it floats from `eye` facing `yaw` (radians), without its bob.
    pub(crate) fn place(eye: Vec3, yaw: f32) -> Vec3 {
        eye + Quat::from_rotation_z(yaw) * OFFSET
    }

    /// Step to `now`. `anchor` is the eye and the view's yaw in radians while the
    /// holocron may show, `None` otherwise (it then goes out where it is);
    /// `seconds` is the presentation time, for its bob and turn.
    pub(crate) fn advance(
        &mut self,
        anchor: Option<(Vec3, f32)>,
        seconds: f32,
        now: Instant,
    ) -> Option<Pose> {
        let dt = self
            .last
            .map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .min(0.1);
        self.last = Some(now);
        let target = anchor
            .filter(|_| self.on)
            .map(|(eye, yaw)| Self::place(eye, yaw));
        let shown = if target.is_some() { 1.0 } else { 0.0 };
        // Out, it comes back at the player rather than gliding from where it went out.
        let was_out = self.level <= 0.0;
        self.level = if self.level < shown {
            (self.level + dt / FADE).min(shown)
        } else {
            (self.level - dt / FADE).max(shown)
        };
        if let Some(target) = target {
            let position = match self.position {
                Some(position) if !was_out && position.distance(target) < SNAP => {
                    let followed = position + (target - position) * (1.0 - (-dt * FOLLOW).exp());
                    target + (followed - target).clamp_length_max(MAX_LAG)
                }
                _ => target,
            };
            self.position = Some(position);
        }
        if self.level <= 0.0 {
            return None;
        }
        Some(Pose {
            position: self.position? + Vec3::Z * idle_bob(seconds),
            rotation: idle_rotation(seconds),
            level: smooth(self.level),
        })
    }
}

/// How far up (or down) its bob has taken it at `seconds`, in units for the
/// player-sized holocron: the Profile screen's holocron bobs by this, scaled up with it.
pub(crate) fn idle_bob(seconds: f32) -> f32 {
    (seconds * std::f32::consts::TAU / BOB_PERIOD).sin() * BOB
}

/// Its turn about the vertical and its tilt at `seconds`, as it floats by the shoulder.
pub(crate) fn idle_rotation(seconds: f32) -> Quat {
    Quat::from_rotation_z(seconds * SPIN)
        * Quat::from_rotation_x(TILT[0])
        * Quat::from_rotation_y(TILT[1])
}

/// Ease in and out.
pub(crate) fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// The eye of a player entity and its view's yaw in radians, while the game draws the
/// player: alive, not hidden and not cloaked. `origin` is the entity's interpolated
/// origin and `yaw_degrees` its interpolated view yaw.
pub(crate) fn player_anchor(
    entity: &EntityState,
    origin: [f32; 3],
    yaw_degrees: f32,
) -> Option<(Vec3, f32)> {
    let hidden =
        entity.e_flags() & (EF_DEAD | EF_NODRAW) != 0 || entity.powerups() & PW_CLOAKED != 0;
    (entity.entity_type() == ET_PLAYER && !hidden).then(|| {
        (
            Vec3::from_array(origin) + Vec3::Z * eye_height(entity.solid()),
            yaw_degrees.to_radians(),
        )
    })
}

/// The eye above the origin for a packed box (`solid`): the box's top less four
/// (`DEFAULT_MAXS_2 - 4`, `CROUCH_MAXS_2 - 4`).
fn eye_height(solid: u32) -> f32 {
    let top = (solid >> 16) & 255;
    if solid == 0 || top == 0 {
        EYE_HEIGHT
    } else {
        top as f32 - 32.0 - 4.0
    }
}

/// The holocrons of the other players on the server, one per slot, lit by their
/// looks (`looks.rs`). Fixed-size: a frame allocates nothing.
pub(crate) struct Others {
    holocrons: [Holocron; SLOTS],
    /// Lit players to float by without a session, for the world shots: slot, eye, yaw.
    #[cfg(test)]
    pub(crate) shot_players: Vec<(usize, Vec3, f32)>,
}

impl Default for Others {
    fn default() -> Self {
        Self {
            holocrons: [Holocron::default(); SLOTS],
            #[cfg(test)]
            shot_players: Vec::new(),
        }
    }
}

impl Others {
    /// Step every slot to `now`: `lit` says whose look has it lit, `anchors` where each
    /// player's eye is and which way they face (`None` while they are not drawn), and
    /// `seconds` the presentation time. `pose` gets each holocron that shows.
    pub(crate) fn advance(
        &mut self,
        lit: impl Fn(usize) -> bool,
        anchors: &[Option<(Vec3, f32)>; SLOTS],
        seconds: f32,
        now: Instant,
        mut pose: impl FnMut(usize, Pose),
    ) {
        for (slot, holocron) in self.holocrons.iter_mut().enumerate() {
            holocron.on = lit(slot);
            if !holocron.on && holocron.level <= 0.0 {
                // Out and staying out: nothing to step.
                holocron.last = None;
                continue;
            }
            let phase = slot as f32 * SLOT_PHASE;
            if let Some(shown) = holocron.advance(anchors[slot], seconds + phase, now) {
                pose(slot, shown);
            }
        }
    }

    /// Whether the holocron of `slot` shows (lit, or still going out).
    pub(crate) fn showing(&self, slot: usize) -> bool {
        self.holocrons
            .get(slot)
            .is_some_and(|holocron| holocron.level > 0.0)
    }

    /// Put every holocron out at once: no game, or another server.
    pub(crate) fn clear(&mut self) {
        for holocron in &mut self.holocrons {
            *holocron = Holocron::default();
        }
    }
}

impl GpuState {
    /// `toy_illuminate`: light the holocron, or put it out.
    pub(crate) fn toggle_illuminate(&mut self) {
        self.illuminate.toggle();
    }

    /// Add this frame's holocrons and their lights: the local player's, then the
    /// other players'. Called right after the frame's lights are cleared, so a full
    /// list never drops the player's own light.
    pub(crate) fn submit_illuminate(&mut self, presentation_time: i64, now: Instant) {
        let mesh = self
            .object_meshes
            .iter()
            .position(|mesh| mesh.appearance.model == MODEL);
        self.submit_own_holocron(mesh, presentation_time, now);
        self.submit_other_holocrons(mesh, presentation_time, now);
        self.submit_stage_holocron(mesh, presentation_time, now);
    }

    /// The holocron by the model on the menu's stage while the Collection's Toys tab
    /// shows it ([`crate::menu_stage`]): lit, fading in and out as the player's own.
    fn submit_stage_holocron(&mut self, mesh: Option<usize>, presentation_time: i64, now: Instant) {
        let anchor = self.stage_holocron_anchor();
        let holocron = &mut self.menu_stage.holocron;
        holocron.set_on(true);
        let Some(pose) = holocron.advance(anchor, presentation_time as f32 * 0.001, now) else {
            return;
        };
        self.dynamic_lights.push_radiant(light(pose));
        if let Some(mesh) = mesh {
            self.object_groups[mesh].push(cube(pose));
        }
    }

    /// The local player's holocron.
    fn submit_own_holocron(&mut self, mesh: Option<usize>, presentation_time: i64, now: Instant) {
        let anchor = self
            .live_session
            .as_ref()
            .filter(|session| playing(&session.latest_snapshot().player))
            .and_then(|_| self.view_anchor());
        #[cfg(test)]
        let anchor = anchor.or(self.illuminate.shot_anchor);
        let Some(pose) = self
            .illuminate
            .advance(anchor, presentation_time as f32 * 0.001, now)
        else {
            return;
        };
        self.dynamic_lights.push_radiant(light(pose));
        let Some(mesh) = mesh else {
            return;
        };
        let mut instance = cube(pose);
        if !self.third_person {
            instance.view_flags |= 1;
        }
        self.object_groups[mesh].push(instance);
    }

    /// The view's eye and yaw while the camera is the player's view (not detached nor
    /// free).
    fn view_anchor(&self) -> Option<(Vec3, f32)> {
        (!self.detached_camera && !self.free_camera_active())
            .then_some((self.camera_position, self.camera_yaw))
    }

    /// The other players' holocrons: by each player the game draws whose look has
    /// Illuminate lit, from their entity's interpolated origin and view yaw. The
    /// local player's own slot is the game state's; a player the view follows is
    /// another player, whose holocron floats by the view's eye and, in first person,
    /// shows only its light (as one's own).
    fn submit_other_holocrons(
        &mut self,
        mesh: Option<usize>,
        presentation_time: i64,
        now: Instant,
    ) {
        let mut anchors = [None; SLOTS];
        let mut own = None;
        let mut followed = None;
        if let Some(session) = self.live_session.as_ref() {
            let snapshot = session.latest_snapshot();
            let slots = crate::looks::ViewSlots::of(session.game_state(), &snapshot.player);
            own = slots.own;
            followed = slots.followed().filter(|&slot| slot < SLOTS);
            if let Some(slot) = followed {
                anchors[slot] = followed_anchor(&snapshot.player, self.view_anchor());
            }
            for entity in &snapshot.entities {
                let slot = usize::from(entity.number());
                // Only players with a holocron lit, or still going out, are placed.
                if slot >= SLOTS
                    || !(self.looks.illuminated(slot) || self.illuminate_others.showing(slot))
                {
                    continue;
                }
                let Some(presented) = self
                    .live_world
                    .entity(sjk_runtime::EntityId::new(slot as u64 + 1))
                else {
                    continue;
                };
                let origin = presented.sample(presentation_time).translation;
                let yaw = presented
                    .sample_pose(presentation_time)
                    .map_or(0.0, |pose| pose.view_angles_degrees[1]);
                if Some(slot) != followed {
                    anchors[slot] = player_anchor(entity, origin, yaw);
                }
            }
        } else if self.illuminate_others_idle() {
            return;
        }
        #[cfg(test)]
        let mut shot_lit = 0_u64;
        #[cfg(test)]
        for &(slot, eye, yaw) in &self.illuminate_others.shot_players {
            anchors[slot] = Some((eye, yaw));
            shot_lit |= 1 << slot;
        }
        #[cfg(not(test))]
        let shot_lit = 0_u64;
        // The nearest few light the world; every cube shows.
        let camera = self.camera_position;
        let mut nearest = [(f32::INFINITY, Pose::default()); OTHERS_LIT];
        let looks = &self.looks;
        let groups = &mut self.object_groups;
        let first_person = !self.third_person;
        self.illuminate_others.advance(
            |slot| Some(slot) != own && (looks.illuminated(slot) || shot_lit & 1 << slot != 0),
            &anchors,
            presentation_time as f32 * 0.001,
            now,
            |slot, pose| {
                if let Some(mesh) = mesh {
                    let mut instance = cube(pose);
                    if first_person && Some(slot) == followed {
                        instance.view_flags |= 1;
                    }
                    groups[mesh].push(instance);
                }
                keep_nearest(&mut nearest, pose.position.distance_squared(camera), pose);
            },
        );
        nearest.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        for (distance, pose) in nearest {
            if distance.is_finite() {
                self.dynamic_lights.push_radiant(light(pose));
            }
        }
    }

    /// Out of a game there is nobody to float by: every other player's holocron goes
    /// at once, and there is nothing to do.
    fn illuminate_others_idle(&mut self) -> bool {
        #[cfg(test)]
        if !self.illuminate_others.shot_players.is_empty() {
            return false;
        }
        self.illuminate_others.clear();
        true
    }
}

/// Keep `pose`, `distance` from the camera, among the `nearest` if it is nearer than
/// the farthest kept (the unused places are infinitely far).
fn keep_nearest(nearest: &mut [(f32, Pose)], distance: f32, pose: Pose) {
    if let Some(farthest) = nearest
        .iter_mut()
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .filter(|farthest| distance < farthest.0)
    {
        *farthest = (distance, pose);
    }
}

/// A holocron's warm light at `pose`.
fn light(pose: Pose) -> PointLight {
    PointLight {
        origin: pose.position.to_array(),
        radius: RADIUS * pose.level,
        color: COLOR,
    }
}

/// A holocron's cube at `pose`.
fn cube(pose: Pose) -> ActorInstance {
    ActorInstance::new(
        pose.position.to_array(),
        pose.rotation.to_array(),
        [pose.level; 3],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(start: Instant, seconds: f32) -> Instant {
        start + Duration::from_secs_f32(seconds)
    }

    #[test]
    fn the_bundled_files_mount_and_the_model_is_a_cube() {
        let mut vfs = VirtualFileSystem::new();
        mount(&mut vfs).unwrap();
        for (path, _) in FILES {
            assert!(vfs.read(path).unwrap().is_some(), "{path}");
        }
        let model = sjk_model::Md3::parse(&vfs.read(MODEL).unwrap().unwrap().bytes).unwrap();
        let surface = &model.surfaces[0];
        assert_eq!(surface.shaders, ["models/sjk/holocron"]);
        assert_eq!((surface.frames[0].len(), surface.triangles.len()), (24, 12));
        for vertex in &surface.frames[0] {
            // Every corner 3 units out on each axis, normals of unit length
            // pointing out of their face.
            assert!(vertex.position.iter().all(|c| (c.abs() - 3.0).abs() < 1e-3));
            let normal = Vec3::from_array(vertex.normal);
            assert!((normal.length() - 1.0).abs() < 0.02);
            assert!(normal.dot(Vec3::from_array(vertex.position)) > 2.9);
        }
        // Clockwise seen from outside, stock's front side.
        for [a, b, c] in &surface.triangles {
            let [a, b, c] =
                [a, b, c].map(|&i| Vec3::from_array(surface.frames[0][i as usize].position));
            let facing = (b - a).cross(c - a);
            assert!(facing.dot(a + b + c) < 0.0);
        }
    }

    /// Every tier's art mounts, decodes at its size, and its shader parses and names
    /// only pictures that are mounted.
    #[test]
    fn every_tiers_art_and_shader_mount_and_parse() {
        let mut vfs = VirtualFileSystem::new();
        mount(&mut vfs).unwrap();
        let read = |path: &str| {
            vfs.read(path)
                .unwrap()
                .unwrap_or_else(|| panic!("{path} is not mounted"))
                .bytes
        };
        let picture = |path: &str| {
            image::load_from_memory(&read(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
        };
        for tier in TIERS {
            for (path, size) in [
                (format!("models/sjk/holocron_{tier}.jpg"), 512),
                (format!("models/sjk/holocron_{tier}_glow.jpg"), 512),
                (format!("gfx/sjk/holocron_{tier}.png"), 256),
                (format!("gfx/sjk/holocron_{tier}_small.png"), 64),
            ] {
                let decoded = picture(&path);
                assert_eq!((decoded.width(), decoded.height()), (size, size), "{path}");
                if path.ends_with(".png") {
                    // An icon has a clear corner and a solid middle.
                    let rgba = decoded.to_rgba8();
                    assert_eq!(rgba.get_pixel(0, 0)[3], 0, "{path}");
                    assert_eq!(rgba.get_pixel(size / 2, size / 2)[3], 255, "{path}");
                }
            }
        }
        let sheen = picture("models/sjk/holocron_mythical_sheen.jpg");
        assert_eq!((sheen.width(), sheen.height()), (128, 128));

        let definitions = sjk_shader::parse_shader_script(
            &read("shaders/sjk_holocron_tiers.shader"),
            "shaders/sjk_holocron_tiers.shader",
        )
        .unwrap();
        assert_eq!(definitions.len(), TIERS.len());
        let mut breathing = Vec::new();
        for (tier, definition) in TIERS.iter().zip(&definitions) {
            assert_eq!(definition.name, format!("models/sjk/holocron_{tier}"));
            // Lit face, the face's own light, the glowing emblem; the mythical one's
            // sheen after them.
            assert_eq!(
                definition.stages.len(),
                3 + usize::from(*tier == "mythical")
            );
            assert_eq!(
                definition.stages[0].rgb_generator.as_deref(),
                Some("lightingdiffuse")
            );
            for stage in &definition.stages {
                assert!(!stage.images.is_empty());
                for image in &stage.images {
                    assert!(
                        vfs.read(&format!("{image}.jpg")).unwrap().is_some(),
                        "{tier}: {image} is not mounted"
                    );
                }
            }
            let glow = &definition.stages[2];
            assert!(glow.glow, "{tier}");
            assert_eq!(glow.images, [format!("models/sjk/holocron_{tier}_glow")]);
            let wave = glow.rgb_wave.as_ref().unwrap();
            breathing.push((wave.base, wave.amplitude));
        }
        // The tiers climb in how brightly the emblem breathes.
        assert!(
            breathing
                .windows(2)
                .all(|pair| { pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1 })
        );
        let mythical = definitions.last().unwrap();
        let sheen = &mythical.stages[3];
        assert_eq!(sheen.images, ["models/sjk/holocron_mythical_sheen"]);
        let moves: Vec<_> = sheen
            .texture_modifications
            .iter()
            .map(|modification| modification.kind.as_str())
            .collect();
        assert_eq!(moves, ["scale", "scroll"]);
        assert!(mythical.stages[1].rgb_wave.as_ref().unwrap().amplitude > 0.0);
        // The cube names the base shader, which no tier's replaces.
        let base = sjk_shader::parse_shader_script(
            &read("shaders/sjk_holocron.shader"),
            "shaders/sjk_holocron.shader",
        )
        .unwrap();
        assert_eq!(base[0].name, "models/sjk/holocron");
        assert!(definitions.iter().all(|d| d.name != base[0].name));
    }

    /// A surface's shader comes from its model file, so every tier's cube names the
    /// tier's own shader (the shared cube would draw the base look for all four), and
    /// is the base cube otherwise: the same corners, normals, texture coordinates and
    /// triangles. The locked look has its own cube and shader the same way.
    #[test]
    fn every_tier_has_a_cube_naming_its_own_shader() {
        let mut vfs = VirtualFileSystem::new();
        mount(&mut vfs).unwrap();
        let model = |path: &str| {
            sjk_model::Md3::parse(
                &vfs.read(path)
                    .unwrap()
                    .unwrap_or_else(|| panic!("{path} is not mounted"))
                    .bytes,
            )
            .unwrap_or_else(|error| panic!("{path}: {error}"))
        };
        let base = model(MODEL);
        let shader_names = |path: &str| -> Vec<String> {
            sjk_shader::parse_shader_script(&vfs.read(path).unwrap().unwrap().bytes, path)
                .unwrap()
                .into_iter()
                .map(|definition| definition.name)
                .collect()
        };
        let defined = shader_names("shaders/sjk_holocron_tiers.shader");
        let locked_defined = shader_names("shaders/sjk_holocron_locked.shader");
        assert_eq!(locked_defined, ["models/sjk/holocron_locked"]);
        let mut named = Vec::new();
        for (look, shader) in TIERS
            .iter()
            .map(|tier| {
                (
                    format!("holocron_{tier}"),
                    format!("models/sjk/holocron_{tier}"),
                )
            })
            .chain([(
                "holocron_locked".to_owned(),
                "models/sjk/holocron_locked".to_owned(),
            )])
        {
            let cube = model(&format!("models/sjk/{look}.md3"));
            assert_eq!(
                (cube.surfaces.len(), cube.surfaces[0].shaders.len()),
                (1, 1)
            );
            let surface = &cube.surfaces[0];
            assert_eq!(surface.shaders, [shader.as_str()], "{look}");
            assert!(
                defined.contains(&shader) || locked_defined.contains(&shader),
                "{look}: {shader} is defined by no mounted shader file"
            );
            let first = &base.surfaces[0];
            assert_eq!(surface.triangles, first.triangles, "{look}");
            assert_eq!(surface.texture_coordinates, first.texture_coordinates);
            assert_eq!(surface.frames[0].len(), first.frames[0].len());
            for (a, b) in surface.frames[0].iter().zip(&first.frames[0]) {
                assert_eq!((a.position, a.normal), (b.position, b.normal), "{look}");
            }
            named.push(shader);
        }
        named.sort();
        named.dedup();
        assert_eq!(named.len(), TIERS.len() + 1, "no two looks share a shader");
        // The locked face is mounted at its size.
        let face = image::load_from_memory(
            &vfs.read("models/sjk/holocron_locked.jpg")
                .unwrap()
                .unwrap()
                .bytes,
        )
        .unwrap();
        assert_eq!((face.width(), face.height()), (512, 512));
    }

    /// Step at 60 frames a second from `from` to `to` seconds; the last pose.
    fn run(
        holocron: &mut Holocron,
        anchor: Option<(Vec3, f32)>,
        start: Instant,
        from: f32,
        to: f32,
    ) -> Option<Pose> {
        let mut pose = None;
        let mut t = from;
        while t <= to + 1e-4 {
            pose = holocron.advance(anchor, 0.0, at(start, t));
            t += 1.0 / 60.0;
        }
        pose
    }

    #[test]
    fn toggling_lights_it_and_puts_it_out_and_set_on_names_the_state() {
        let mut holocron = Holocron::default();
        assert!(!holocron.lit());
        holocron.toggle();
        assert!(holocron.lit());
        holocron.toggle();
        assert!(!holocron.lit());
        // The Collection's switch names the state it wants; asking twice changes nothing.
        holocron.set_on(true);
        holocron.set_on(true);
        assert!(holocron.lit());
        holocron.set_on(false);
        assert!(!holocron.lit());
        // The toy's command is its own name, never a Force power's.
        assert_eq!(COMMAND, "toy_illuminate");
        assert!(COMMAND.starts_with("toy_"));
    }

    #[test]
    fn it_appears_by_the_left_shoulder_and_goes_out() {
        let start = Instant::now();
        let mut holocron = Holocron::default();
        let eye = Vec3::new(100.0, 0.0, 50.0);
        // Off: nothing, even with somewhere to float.
        assert_eq!(run(&mut holocron, Some((eye, 0.0)), start, 0.0, 0.5), None);
        holocron.toggle();
        let pose = run(&mut holocron, Some((eye, 0.0)), start, 0.5, 0.6).unwrap();
        assert!(pose.level > 0.0 && pose.level < 1.0);
        let pose = run(&mut holocron, Some((eye, 0.0)), start, 0.6, 1.0).unwrap();
        assert_eq!(pose.level, 1.0);
        assert_eq!(pose.position, eye + OFFSET);
        // Facing +y (yaw 90 degrees), its left is -x.
        let mut turned = Holocron::default();
        turned.toggle();
        let pose = run(
            &mut turned,
            Some((eye, std::f32::consts::FRAC_PI_2)),
            start,
            0.0,
            1.0,
        )
        .unwrap();
        assert!((pose.position - eye - Vec3::new(-18.0, -6.0, -4.0)).length() < 1e-3);
        // Dead or spectating: it goes out where it was.
        let pose = run(&mut holocron, None, start, 1.0, 1.1).unwrap();
        assert_eq!(pose.position, eye + OFFSET);
        assert!(pose.level < 1.0);
        assert_eq!(run(&mut holocron, None, start, 1.1, 1.5), None);
        // Back: it fades in where the player now is, even close by.
        let elsewhere = eye + Vec3::X * 30.0;
        let pose = run(&mut holocron, Some((elsewhere, 0.0)), start, 1.5, 1.55).unwrap();
        assert_eq!(pose.position, elsewhere + OFFSET);
    }

    /// A player entity in `slot` with the flags and powerups given.
    fn player(slot: u16, flags: u32, powerups: u32) -> EntityState {
        let mut entity = EntityState::zero(slot, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        entity.set_raw_field(8, u32::from(ET_PLAYER));
        entity.set_raw_field(19, flags);
        entity.set_raw_field(77, powerups);
        entity
    }

    #[test]
    fn another_player_is_floated_by_from_their_eye_and_view_yaw() {
        let origin = [100.0, 50.0, 24.0];
        let (eye, yaw) = player_anchor(&player(3, 0, 0), origin, 90.0).unwrap();
        assert_eq!(eye, Vec3::new(100.0, 50.0, 24.0 + EYE_HEIGHT));
        assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        // Crouched (box top 16): the eye is at 12.
        let mut crouched = player(3, 0, 0);
        crouched.set_raw_field(26, (48 << 16) | (24 << 8) | 15);
        assert_eq!(player_anchor(&crouched, origin, 0.0).unwrap().0.z, 36.0);
        // Not drawn: dead, hidden, cloaked, or not a player.
        assert_eq!(player_anchor(&player(3, EF_DEAD, 0), origin, 0.0), None);
        assert_eq!(player_anchor(&player(3, EF_NODRAW, 0), origin, 0.0), None);
        assert_eq!(player_anchor(&player(3, 0, PW_CLOAKED), origin, 0.0), None);
        let mut missile = player(3, 0, 0);
        missile.set_raw_field(8, 3);
        assert_eq!(player_anchor(&missile, origin, 0.0), None);
    }

    /// Step the others at 60 frames a second from `from` to `to` seconds, the slots in
    /// `lit` lit, at presentation time 0; the poses of the last step.
    fn run_others(
        others: &mut Others,
        lit: &[usize],
        anchors: &[Option<(Vec3, f32)>; SLOTS],
        start: Instant,
        from: f32,
        to: f32,
    ) -> Vec<(usize, Pose)> {
        let mut poses = Vec::new();
        let mut t = from;
        while t <= to + 1e-4 {
            poses.clear();
            others.advance(
                |slot| lit.contains(&slot),
                anchors,
                0.0,
                at(start, t),
                |slot, pose| poses.push((slot, pose)),
            );
            t += 1.0 / 60.0;
        }
        poses
    }

    #[test]
    fn other_players_holocrons_fade_and_follow_as_ones_own() {
        let start = Instant::now();
        let mut others = Others::default();
        let eye = Vec3::new(100.0, 0.0, 60.0);
        let mut anchors = [None; SLOTS];
        anchors[3] = Some((eye, std::f32::consts::FRAC_PI_2));
        anchors[5] = Some((eye + Vec3::X * 200.0, 0.0));
        // Only lit looks show, and only where a player is drawn.
        let poses = run_others(&mut others, &[3, 7], &anchors, start, 0.0, 0.1);
        assert_eq!(poses.len(), 1);
        let (slot, pose) = poses[0];
        assert_eq!(slot, 3);
        assert!(pose.level > 0.0 && pose.level < 1.0, "fading in");
        let poses = run_others(&mut others, &[3, 7], &anchors, start, 0.1, 1.0);
        let pose = poses[0].1;
        assert_eq!(pose.level, 1.0);
        // By the left shoulder of a player facing +y, the bob of slot 3 included.
        let bob = (3.0 * SLOT_PHASE * std::f32::consts::TAU / BOB_PERIOD).sin() * BOB;
        let expected = Holocron::place(eye, std::f32::consts::FRAC_PI_2) + Vec3::Z * bob;
        assert!((pose.position - expected).length() < 1e-3);
        // The same as the local player's from the same eye and yaw.
        let mut own = Holocron::default();
        own.toggle();
        let local = run(&mut own, anchors[3], start, 0.0, 1.0).unwrap();
        assert!((local.position + Vec3::Z * bob - pose.position).length() < 1e-3);
        // The player is no longer drawn (dead, gone from the snapshot): it fades out
        // where it was.
        anchors[3] = None;
        let poses = run_others(&mut others, &[3], &anchors, start, 1.0, 1.1);
        assert!(poses[0].1.level < 1.0);
        assert!((poses[0].1.position - expected).length() < 1e-3);
        assert!(run_others(&mut others, &[3], &anchors, start, 1.1, 1.5).is_empty());
        // Put out by the look: it fades too.
        anchors[3] = Some((eye, 0.0));
        run_others(&mut others, &[3], &anchors, start, 1.5, 2.5);
        let poses = run_others(&mut others, &[], &anchors, start, 2.5, 2.55);
        assert!(poses[0].1.level < 1.0);
        assert!(run_others(&mut others, &[], &anchors, start, 2.55, 3.0).is_empty());
        // Cleared (another server): out at once.
        run_others(&mut others, &[3], &anchors, start, 3.0, 4.0);
        others.clear();
        assert!(run_others(&mut others, &[], &anchors, start, 4.0, 4.0).is_empty());
    }

    #[test]
    fn a_followed_players_holocron_floats_by_the_view_while_they_live() {
        let mut followed = PlayerState::zero();
        followed.set_client_num(5);
        followed.set_movement_flags(PMF_FOLLOW);
        followed.stats[0] = 100;
        let view = Some((Vec3::new(1.0, 2.0, 3.0), 0.5));
        // Following them: not the own holocron's place, but theirs is the view's.
        assert!(!playing(&followed));
        assert_eq!(followed_anchor(&followed, view), view);
        assert_eq!(followed_anchor(&followed, None), None, "a free camera");
        followed.stats[0] = 0;
        assert_eq!(followed_anchor(&followed, view), None, "dead");
        followed.stats[0] = 100;
        followed.set_movement_type(7);
        assert_eq!(followed_anchor(&followed, view), None, "the intermission");
    }

    #[test]
    fn only_the_nearest_others_light_the_world() {
        let mut nearest = [(f32::INFINITY, Pose::default()); OTHERS_LIT];
        let at = |x: f32| Pose {
            position: Vec3::X * x,
            ..Pose::default()
        };
        for x in [50.0, 10.0, 70.0, 30.0, 20.0, 60.0] {
            keep_nearest(&mut nearest, x * x, at(x));
        }
        let mut kept: Vec<f32> = nearest.iter().map(|(_, pose)| pose.position.x).collect();
        kept.sort_by(f32::total_cmp);
        assert_eq!(kept, [10.0, 20.0, 30.0, 50.0]);
    }

    #[test]
    fn it_trails_a_little_and_jumps_with_a_teleport() {
        let start = Instant::now();
        let mut holocron = Holocron::default();
        holocron.toggle();
        let eye = Vec3::new(0.0, 0.0, 50.0);
        run(&mut holocron, Some((eye, 0.0)), start, 0.0, 0.5);
        // Running at 300 units a second for a frame: it lags, never past MAX_LAG.
        let moved = eye + Vec3::X * 40.0;
        let pose = holocron
            .advance(Some((moved, 0.0)), 0.0, at(start, 0.51))
            .unwrap();
        let lag = pose.position.distance(moved + OFFSET);
        assert!(lag > 1.0 && lag <= MAX_LAG + 1e-3, "{lag}");
        let far = eye + Vec3::X * 1000.0;
        let pose = holocron
            .advance(Some((far, 0.0)), 0.0, at(start, 0.52))
            .unwrap();
        assert_eq!(pose.position, far + OFFSET);
    }
}

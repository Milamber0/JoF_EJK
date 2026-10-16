//! The Holocrons tab's 3D holocron ([`crate::console::holocrons_panel`]): one holocron floating in
//! the world behind the tab, over the left of the screen where the page leaves it clear.
//! It turns and bobs as Illuminate's does ([`crate::illuminate`]), wears the look of the
//! tier chosen in the list (that tier's model and shader, and a point light in the tier's
//! colour) and, for a tier the player holds none of, the locked look: the face drained
//! and dark, with a faint light (padlock-like dimming; it is never hidden, so every tier
//! can be seen).
//!
//! It is an ordinary object of the world pass: an instance of the look's model in
//! `object_groups`, loaded with the map's other rigid models ([`models`]), and a point
//! light, so the menu map's stage (the Character tab's) is not needed: it floats before
//! the backdrop camera ([`place`]), which the tab asks to park on its own shot
//! ([`crate::menu_backdrop::Shot::Holocrons`]). Over a match (the game menu's Collection)
//! it floats before the game's camera, the third-person one too: it is placed from the
//! view the frame renders (`GpuState::frame_view`), with that view's own field.
//!
//! A tier is a model of its own because a surface's shader is named by its model file
//! (`scripts/holocron_assets.py` writes `holocron_<tier>.md3` naming
//! `models/sjk/holocron_<tier>`); the shared `holocron.md3` would draw the base look for
//! every tier.
//!
//! Changing the tier shrinks the old look away and grows the new one ([`Stage`]); the
//! frame allocates nothing.

use super::COUNT;
use crate::GpuState;
use crate::actor_instance::ActorInstance;
use crate::dynamic_lights::PointLight;
use crate::hud::identification::Camera;
use glam::Vec3;
use std::time::Instant;

/// The model of each tier, in [`super::TIERS`] order.
pub(crate) const TIER_MODELS: [&str; COUNT] = [
    "models/sjk/holocron_uncommon.md3",
    "models/sjk/holocron_rare.md3",
    "models/sjk/holocron_legendary.md3",
    "models/sjk/holocron_mythical.md3",
];
/// The model of a tier the player holds none of.
pub(crate) const LOCKED_MODEL: &str = "models/sjk/holocron_locked.md3";

/// Every model the stage may draw, for the world to load with its other rigid models.
pub(crate) fn models() -> impl Iterator<Item = &'static str> {
    TIER_MODELS.into_iter().chain([LOCKED_MODEL])
}

/// What the tab asks the stage to show.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Request {
    /// The chosen tier, an index into [`super::TIERS`].
    pub(crate) tier: usize,
    /// The player holds at least one of it: its own look, else the locked one.
    pub(crate) owned: bool,
}

impl Request {
    /// The model that draws it.
    pub(crate) fn model(self) -> &'static str {
        if self.owned {
            TIER_MODELS[self.tier.min(COUNT - 1)]
        } else {
            LOCKED_MODEL
        }
    }

    /// The light it casts: the tier's colour, a fifth as strong while locked.
    pub(crate) fn light(self) -> [f32; 3] {
        let light = super::TIERS[self.tier.min(COUNT - 1)].light;
        let strength = if self.owned { 1.0 } else { LOCKED_LIGHT };
        light.map(|channel| channel * strength)
    }
}

/// How strong a locked holocron's light is against the tier's.
const LOCKED_LIGHT: f32 = 0.2;
/// Seconds to shrink away when the look changes, and to grow into the new one.
const OUT: f32 = 0.12;
const IN: f32 = 0.32;
/// The holocron's edge on screen, in the 1080p frame's pixels, and how far before the
/// camera it floats in world units (clear of the near plane and of the nearest walls).
pub(crate) const SIZE: f32 = 185.0;
const DISTANCE: f32 = 64.0;
/// The light's reach, in world units, at full level.
const RADIUS: f32 = 190.0;
/// The cube's edge in the model (`HOLOCRON_EDGE` in `scripts/holocron_assets.py`).
const EDGE: f32 = 6.0;

/// The look on show and how far it has grown (0 gone, 1 full size).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stage {
    shown: Option<Request>,
    level: f32,
    last: Option<Instant>,
}

impl Stage {
    /// Step to `now` toward `wanted` (`None`: the tab is not on show) and return what
    /// to draw: the look and its level, `None` while nothing shows.
    pub(crate) fn advance(
        &mut self,
        wanted: Option<Request>,
        now: Instant,
    ) -> Option<(Request, f32)> {
        let dt = self
            .last
            .map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32()
            })
            .min(0.1);
        self.last = Some(now);
        let Some(wanted) = wanted else {
            // Gone with the tab: the next opening grows it from nothing.
            *self = Self::default();
            return None;
        };
        if self.shown != Some(wanted) {
            if self.shown.is_some() && self.level > 0.0 {
                self.level = (self.level - dt / OUT).max(0.0);
            }
            if self.shown.is_none() || self.level <= 0.0 {
                self.shown = Some(wanted);
                self.level = 0.0;
            }
        } else {
            self.level = (self.level + dt / IN).min(1.0);
        }
        let shown = self.shown?;
        (self.level > 0.0).then_some((shown, crate::illuminate::smooth(self.level)))
    }
}

/// Where the holocron floats and how large to draw it before `view` (the camera the frame
/// renders, its vertical field and window): on the ray through the frame point `at` (1920
/// by 1080 frame pixels), [`DISTANCE`] ahead, its edge as wide as [`SIZE`] frame pixels
/// at `at`. Returns the centre and the model's scale.
pub(crate) fn place(view: &Camera, at: [f32; 2]) -> (Vec3, f32) {
    let viewport = view.viewport;
    let frame = crate::menu::sjk::Frame::new(viewport);
    let [x, y] = frame.point(at[0], at[1]);
    let (ndc_x, ndc_y) = (x / viewport[0] * 2.0 - 1.0, 1.0 - y / viewport[1] * 2.0);
    let half = (view.fov.to_radians() * 0.5).tan();
    let aspect = viewport[0] / viewport[1];
    // The basis `Camera::project` reads the view with.
    let forward = (view.target - view.eye).normalize_or_zero();
    let right = forward.cross(view.up).normalize_or_zero();
    let up = right.cross(forward);
    let centre = view.eye
        + forward * DISTANCE
        + right * (ndc_x * half * aspect * DISTANCE)
        + up * (ndc_y * half * DISTANCE);
    // World units across the window's height at that distance, over its pixels.
    let per_pixel = 2.0 * half * DISTANCE / viewport[1];
    (centre, SIZE * frame.s * per_pixel / EDGE)
}

impl GpuState {
    /// Add the Holocrons tab's holocron and its light to this frame's world, while the
    /// tab shows. Called with the other frame lights (`submit_illuminate`).
    pub(crate) fn submit_holocron_stage(&mut self, now: Instant) {
        let wanted = self
            .console
            .as_ref()
            .and_then(crate::console::ViewerConsole::holocrons_stage);
        let Some((request, level)) = self.holocron_stage.advance(wanted, now) else {
            return;
        };
        // The view this frame renders (set before the world is submitted), else the
        // camera's eye and angles with the renderer's vertical field.
        let view = self.frame_view.unwrap_or_else(|| {
            let (yaw, pitch) = (self.camera_yaw, self.camera_pitch);
            let forward = Vec3::new(
                yaw.cos() * pitch.cos(),
                yaw.sin() * pitch.cos(),
                pitch.sin(),
            );
            Camera {
                eye: self.camera_position,
                target: self.camera_position + forward,
                up: Vec3::Z,
                fov: self.field_of_view,
                viewport: [
                    self.configuration.width as f32,
                    self.configuration.height as f32,
                ],
            }
        });
        let (centre, scale) = place(&view, crate::console::holocrons_panel::STAGE_AT);
        let seconds = now.saturating_duration_since(self.ui_epoch).as_secs_f32();
        #[cfg(test)]
        let seconds = self.holocron_stage_seconds.unwrap_or(seconds);
        let position = centre + Vec3::Z * crate::illuminate::idle_bob(seconds) * scale;
        let size = scale * level;
        self.dynamic_lights.push_radiant(PointLight {
            origin: position.to_array(),
            radius: RADIUS * level,
            color: request.light(),
        });
        let Some(mesh) = self
            .object_meshes
            .iter()
            .position(|mesh| mesh.appearance.model == request.model())
        else {
            return;
        };
        self.object_groups[mesh].push(ActorInstance::new(
            position.to_array(),
            crate::illuminate::idle_rotation(seconds).to_array(),
            [size; 3],
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn every_tier_has_a_model_of_its_own_named_by_its_id() {
        for (tier, model) in super::super::TIERS.iter().zip(TIER_MODELS) {
            assert_eq!(model, format!("models/sjk/holocron_{}.md3", tier.id));
        }
        assert_eq!(models().count(), COUNT + 1);
        let all: std::collections::BTreeSet<_> = models().collect();
        assert_eq!(all.len(), COUNT + 1, "no model twice");
        assert!(
            !all.contains(crate::illuminate::MODEL),
            "Illuminate's stays its own"
        );
    }

    #[test]
    fn a_request_picks_the_tiers_model_and_light_or_the_locked_ones() {
        let owned = Request {
            tier: 3,
            owned: true,
        };
        assert_eq!(owned.model(), "models/sjk/holocron_mythical.md3");
        assert_eq!(owned.light(), super::super::TIERS[3].light);
        let locked = Request {
            tier: 3,
            owned: false,
        };
        assert_eq!(locked.model(), LOCKED_MODEL);
        let dim = locked.light();
        for (dim, full) in dim.into_iter().zip(owned.light()) {
            assert!((dim - full * LOCKED_LIGHT).abs() < 1e-6);
        }
        // The tier shows in the dimmed light: the colours stay apart.
        assert!(
            Request {
                tier: 0,
                owned: false
            }
            .light()[1]
                > Request {
                    tier: 0,
                    owned: false
                }
                .light()[0]
        );
        // An index out of range is the last tier, never a panic.
        assert_eq!(
            Request {
                tier: 9,
                owned: true
            }
            .model(),
            TIER_MODELS[COUNT - 1]
        );
    }

    /// Step 60 frames a second for `seconds`; the last answer.
    fn run(
        stage: &mut Stage,
        wanted: Option<Request>,
        start: Instant,
        from: f32,
        seconds: f32,
    ) -> Option<(Request, f32)> {
        let mut last = None;
        let mut t = from;
        while t <= from + seconds + 1e-4 {
            last = stage.advance(wanted, start + Duration::from_secs_f32(t));
            t += 1.0 / 60.0;
        }
        last
    }

    #[test]
    fn it_grows_in_swaps_through_nothing_and_goes_with_the_tab() {
        let start = Instant::now();
        let mut stage = Stage::default();
        let rare = Request {
            tier: 1,
            owned: true,
        };
        let mythical = Request {
            tier: 3,
            owned: true,
        };
        // Nothing shows until a tab asks, then it grows from nothing.
        assert_eq!(run(&mut stage, None, start, 0.0, 0.2), None);
        let (_, level) = run(&mut stage, Some(rare), start, 0.2, 0.1).unwrap();
        assert!(level > 0.0 && level < 1.0, "growing: {level}");
        let (look, level) = run(&mut stage, Some(rare), start, 0.3, 0.6).unwrap();
        assert_eq!((look, level), (rare, 1.0));
        // A new tier shrinks the old one away (never both at once), then grows.
        let (look, level) = run(&mut stage, Some(mythical), start, 0.9, 0.05).unwrap();
        assert_eq!(look, rare);
        assert!(level < 1.0, "shrinking: {level}");
        let mut grown = None;
        let mut t = 0.95;
        while t < 2.0 {
            if let Some((look, level)) =
                stage.advance(Some(mythical), start + Duration::from_secs_f32(t))
            {
                if look == mythical {
                    grown = Some(level);
                    break;
                }
                assert_eq!(look, rare, "the old look shrinks until it is gone");
            }
            t += 1.0 / 60.0;
        }
        assert!(
            grown.is_some_and(|level| level < 1.0),
            "the new look starts small"
        );
        let (look, level) = run(&mut stage, Some(mythical), start, 1.5, 0.6).unwrap();
        assert_eq!((look, level), (mythical, 1.0));
        // The same tier going from locked to owned (the hub answered) swaps too.
        let locked = Request {
            tier: 3,
            owned: false,
        };
        let (look, _) = run(&mut stage, Some(locked), start, 2.12, 0.04).unwrap();
        assert_eq!(look, mythical);
        // The tab closes: gone at once, and a new opening starts from nothing again.
        assert_eq!(run(&mut stage, None, start, 2.6, 0.1), None);
        let (_, level) =
            run(&mut stage, Some(mythical), start, 2.8, 0.0).unwrap_or((mythical, 0.0));
        assert!(level < 0.2);
    }

    /// The holocron sits on the ray through the frame point asked, [`DISTANCE`] ahead of
    /// the camera, in every window shape, with the menu's field and a match's narrower
    /// one (a match over 16:9 at `cg_fov 80` renders about 64 degrees up the window: the
    /// view's own field, never converted again), and as wide on screen as [`SIZE`] frame
    /// pixels.
    #[test]
    fn it_floats_where_the_page_leaves_room_in_every_window() {
        let eye = Vec3::new(10.0, -20.0, 300.0);
        let (yaw, pitch) = (0.7_f32, -0.1_f32);
        let forward = Vec3::new(
            yaw.cos() * pitch.cos(),
            yaw.sin() * pitch.cos(),
            pitch.sin(),
        );
        for fov in [90.0, 64.3] {
            for viewport in [
                [1920.0, 1080.0],
                [3840.0, 2160.0],
                [1440.0, 1080.0],
                [2560.0, 1080.0],
            ] {
                let view = Camera {
                    eye,
                    target: eye + forward * 10.0,
                    up: Vec3::Z,
                    fov,
                    viewport,
                };
                let at = crate::console::holocrons_panel::STAGE_AT;
                let (centre, scale) = place(&view, at);
                assert!(((centre - eye).dot(forward) - DISTANCE).abs() < 1e-3);
                // Projected back as the HUD projects the world, it lands on the point.
                let pixel = view.project(centre).expect("in view");
                let frame = crate::menu::sjk::Frame::new(viewport);
                let want = frame.point(at[0], at[1]);
                assert!(
                    (pixel[0] - want[0]).abs() < 0.5 && (pixel[1] - want[1]).abs() < 0.5,
                    "{fov} {viewport:?}: {pixel:?} against {want:?}"
                );
                // Its edge across the window is SIZE frame pixels.
                let half = (fov.to_radians() * 0.5).tan();
                let edge_pixels = scale * EDGE / (2.0 * half * DISTANCE) * viewport[1];
                assert!((edge_pixels - SIZE * frame.s).abs() < 0.5, "{viewport:?}");
                // Left of the middle of the window, where the list is not.
                assert!(pixel[0] < viewport[0] * 0.5, "{viewport:?}");
            }
        }
    }
}

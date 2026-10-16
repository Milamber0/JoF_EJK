//! World-projected player labels. Names belong to the existing chat roster.
use crate::{TextVertex, UiFont, chat::ChatOverlay, console::ViewerConsole};
use glam::Vec3;
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_protocol::{GameState, Snapshot};
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::{Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId, TextOverflow};

/// Register only the completed overhead capabilities.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        "cg_drawPlayerNames",
        0_i64,
        CvarFlags::ARCHIVE,
        "Overhead names; 2 adds health bars",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_drawPlayerNamesScale",
        0.5_f64,
        CvarFlags::ARCHIVE,
        "Overhead name scale",
    ))?;
    cvars.register(CvarDefinition::new(
        "cg_drawFriend",
        true,
        CvarFlags::ARCHIVE,
        "Team logos over allies' heads",
    ))?;
    Ok(())
}

/// Camera basis shared by projection tests and the production label pass.
#[derive(Clone, Copy)]
pub(crate) struct Camera {
    /// Actual rendered eye position.
    pub(crate) eye: Vec3,
    /// Actual rendered view target.
    pub(crate) target: Vec3,
    /// Rendered up vector.
    pub(crate) up: Vec3,
    /// Vertical field of view in degrees.
    pub(crate) fov: f32,
    /// Physical viewport dimensions.
    pub(crate) viewport: [f32; 2],
}

impl Camera {
    /// Reject behind-camera/off-screen positions before emitting an overhead HUD element.
    pub(crate) fn project(self, point: Vec3) -> Option<[f32; 2]> {
        self.project_within(point, 1.0)
    }

    /// Like [`Camera::project`], but keeps points up to `limit` half-screens
    /// from the centre (1.0 is the screen edge).
    pub(crate) fn project_within(self, point: Vec3, limit: f32) -> Option<[f32; 2]> {
        let forward = (self.target - self.eye).normalize_or_zero();
        let right = forward.cross(self.up).normalize_or_zero();
        let up = right.cross(forward);
        let delta = point - self.eye;
        let depth = delta.dot(forward);
        if depth <= 0.01 {
            return None;
        }
        let half = (self.fov.to_radians() * 0.5).tan() * depth;
        let x = delta.dot(right) / (half * self.viewport[0] / self.viewport[1]);
        let y = delta.dot(up) / half;
        if !x.is_finite() || !y.is_finite() || x.abs() > limit || y.abs() > limit {
            return None;
        }
        Some([
            (x + 1.0) * 0.5 * self.viewport[0],
            (1.0 - y) * 0.5 * self.viewport[1],
        ])
    }
}

/// Fixed-capacity display list: at most 32 clients, no owned name strings.
pub(crate) struct State {
    /// Shapes and text IDs submitted through the normal HUD renderer.
    pub(crate) list: DrawList,
    names: i64,
    scale: f32,
    /// Each slot's team logo top this frame ([`super::team_logo`]): the name stands
    /// above it.
    logo_tops: [Option<f32>; 32],
}

impl Default for State {
    fn default() -> Self {
        Self {
            list: DrawList::new(256),
            names: 0,
            scale: 0.5,
            logo_tops: [None; 32],
        }
    }
}

impl State {
    /// Sample settings once, outside text emission.
    pub(crate) fn sample(&mut self, console: Option<&ViewerConsole>) {
        self.names = console
            .and_then(|c| c.integer_cvar("cg_drawplayernames"))
            .unwrap_or(0);
        self.scale =
            crate::cgame_options::scalar(console, "cg_drawplayernamesscale", 0.5).clamp(0.1, 3.0);
    }

    /// This frame's team logo tops, by client slot, before [`Self::update`].
    pub(crate) fn set_logo_tops(&mut self, tops: [Option<f32>; 32]) {
        self.logo_tops = tops;
    }

    /// Collect visible players using fixed scratch and the actual rendered camera.
    pub(crate) fn update(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        world: &sjk_runtime::World,
        now: i64,
        camera: Camera,
        bsp: &Bsp,
        scratch: &mut TraceScratch,
        hidden: bool,
    ) {
        self.list.clear();
        let restrictions = info_number(game.config_string(0), "restricts");
        if hidden || self.names == 0 || restrictions & 64 != 0 {
            return;
        }
        let local = snapshot.player.client_num();
        for entity in &snapshot.entities {
            let slot = entity.number();
            if slot >= 32
                || slot == local
                || entity.entity_type() != 1
                || entity.e_flags() & 2 != 0
                || entity.client_bitflag(local)
                || entity.powerups() & (1 << 11) != 0
            {
                continue;
            }
            let info = game.config_string(1131 + usize::from(slot));
            if info.is_none() || info_number(info, "t") == 3 {
                continue;
            }
            let Some(presented) = world.entity(sjk_runtime::EntityId::new(u64::from(slot) + 1))
            else {
                continue;
            };
            let origin = Vec3::from_array(presented.sample(now).translation);
            if origin.distance_squared(Vec3::from_array(snapshot.player.origin())) >= 9_000_000.0 {
                continue;
            }
            let Some(screen) = camera.project(origin + Vec3::Z * 64.0) else {
                continue;
            };
            let trace = bsp.trace_box_with(
                scratch,
                snapshot.player.origin(),
                origin.to_array(),
                Aabb::new([0.0; 3], [0.0; 3]).unwrap(),
                1 | 0x0200_0000,
            );
            if !unoccluded(trace.fraction, trace.start_solid, trace.all_solid) {
                continue;
            }
            let health = entity.health() as f32;
            let maximum = entity.max_health() as f32;
            self.emit(
                slot,
                screen,
                camera.viewport,
                (maximum > 0.0).then_some((health / maximum).clamp(0.0, 1.0)),
            );
        }
    }

    /// Emit retained IDs; the standard glyph shadow provides contrast without a panel.
    /// A name over a team logo stands above it.
    pub(crate) fn emit(
        &mut self,
        slot: u16,
        point: [f32; 2],
        viewport: [f32; 2],
        health: Option<f32>,
    ) {
        let unit = crate::ui_scale::height_scale(viewport[1]);
        let size = 36.0 * self.scale * unit;
        let mut point = point;
        if let Some(top) = self.logo_tops.get(usize::from(slot)).copied().flatten() {
            point[1] = point[1].min(top - size * 1.5);
        }
        let width = (320.0 * unit).min(viewport[0]);
        let x = (point[0] - width * 0.5).clamp(0.0, viewport[0] - width);
        let _ = self.list.push(DrawCommand::Text {
            rect: Rect::new(x, point[1], width, size * 1.5),
            text: TextId(u32::from(slot)),
            size,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
            align: TextAlign::Center,
            overflow: TextOverflow::Ellipsis,
            weight: FontWeight::Regular,
            letter_spacing: 0.0,
        });
        if self.names > 1
            && let Some(ratio) = health
        {
            let _ = self.list.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    point[0] - 25.0 * unit,
                    point[1] - 7.0 * unit,
                    50.0 * unit * ratio,
                    3.0 * unit,
                ),
                color: Color::new(1.0 - ratio, ratio, 0.1, 1.0),
            });
        }
    }

    /// Resolve existing roster text at submission, never allocate another name store.
    pub(crate) fn append(
        &self,
        chat: &ChatOverlay,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        crate::ui_renderer::append_text_commands(
            &self.list,
            |id| chat.player_label(id.0 as u16),
            vertices,
            font,
            viewport,
            crate::text::TextStyle::NEUTRAL,
        );
    }
}

pub(super) fn info_number(bytes: Option<&[u8]>, key: &str) -> i32 {
    bytes
        .and_then(|b| sjk_client::LegacyClientInfo::new(b).integer(key))
        .unwrap_or(0)
}

pub(super) fn unoccluded(fraction: f32, start_solid: bool, all_solid: bool) -> bool {
    fraction >= 1.0 && !start_solid && !all_solid
}

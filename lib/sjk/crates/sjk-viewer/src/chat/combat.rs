//! Retained combat announcements and server-authored score/damage plums.
use super::*;
use crate::hud::identification::Camera;
use sjk_protocol::{GameState, Snapshot};
use sjk_ui::{Color, FontWeight, Rect, TextAlign};
use std::fmt::Write;

struct Plum {
    text: String,
    origin: glam::Vec3,
    value: i32,
    start: i32,
}

/// One snapshot latch, fixed plum slots, and a reusable centre-message buffer.
pub(super) struct Combat {
    snapshot: Option<(i32, i32)>,
    signatures: [u16; 1024],
    obituaries: u64,
    plums: [Plum; 16],
    next: usize,
    last_origin: glam::Vec3,
    /// Recycled centre-print allocation, retained after expiry.
    pub(super) spare: String,
    /// Event-specific centre height, before the user's explicit override.
    pub(super) height: Option<f32>,
    /// Production projection reused by the evidence fixture.
    pub(super) camera: Option<Camera>,
    time: i32,
    enabled: bool,
}

impl Default for Combat {
    fn default() -> Self {
        Self {
            snapshot: None,
            signatures: [0; 1024],
            obituaries: 0,
            plums: std::array::from_fn(|_| Plum {
                text: String::with_capacity(16),
                origin: glam::Vec3::ZERO,
                value: 0,
                start: i32::MIN,
            }),
            next: 0,
            last_origin: glam::Vec3::ZERO,
            spare: String::with_capacity(1024),
            height: None,
            camera: None,
            time: 0,
            enabled: true,
        }
    }
}

impl ChatOverlay {
    /// Project the existing shared obituary feed and newly accepted entity events.
    pub(crate) fn observe_combat(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        tracker: &sjk_client::ObituaryTracker,
        console: Option<&crate::console::ViewerConsole>,
        camera: Camera,
        time: i32,
    ) {
        let integer = |key, fallback| {
            console
                .and_then(|c| c.integer_cvar(key))
                .unwrap_or(fallback)
        };
        self.combat.enabled = integer("cg_scoreplums", 1) != 0;
        self.combat.camera = Some(camera);
        self.combat.time = time;
        let stamp = (snapshot.message_sequence, snapshot.server_time);
        if self.combat.snapshot == Some(stamp) {
            return;
        }
        if self
            .combat
            .snapshot
            .is_some_and(|(_, old)| old > snapshot.server_time)
            || tracker.decoded() < self.combat.obituaries
        {
            self.combat.signatures.fill(0);
            self.combat.obituaries = 0;
            for plum in &mut self.combat.plums {
                plum.start = i32::MIN;
            }
        }
        self.combat.snapshot = Some(stamp);
        let kill_mode = integer("cg_killmessage", 1);
        let gametype = game
            .config_string(0)
            .and_then(|b| sjk_client::LegacyClientInfo::new(b).integer("g_gametype"))
            .unwrap_or(0);
        for offset in (0..tracker
            .decoded()
            .saturating_sub(self.combat.obituaries)
            .min(8))
            .rev()
        {
            if let Some(event) = tracker.feed().newest(offset as usize)
                && event.local_fragged
                && kill_mode > 0
            {
                self.kill_announcement(event.target, &snapshot.player, gametype, kill_mode);
            }
        }
        self.combat.obituaries = tracker.decoded();
        let mut seen = [false; 1024];
        for entity in &snapshot.entities {
            let slot = usize::from(entity.number());
            if slot >= seen.len() {
                continue;
            }
            seen[slot] = true;
            let signature = if entity.entity_type() >= 18 {
                u16::from(entity.entity_type() - 18)
            } else {
                entity.event()
            };
            if self.combat.signatures[slot] == signature {
                continue;
            }
            self.combat.signatures[slot] = signature;
            match signature & 255 {
                98 if self.combat.enabled
                    && entity.other_entity_num() == snapshot.player.client_num() =>
                {
                    let spot = entity.event_parameter() == 1
                        && game.config_string(0).is_some_and(|b| {
                            b.windows(5).any(|w| w.eq_ignore_ascii_case(b"japro"))
                        });
                    if !spot {
                        let origin = sjk_client::legacy_evaluate_trajectory(
                            entity.trajectory_base(),
                            entity.trajectory_delta(),
                            entity.trajectory_type(),
                            entity.trajectory_time(),
                            entity.trajectory_duration(),
                            snapshot.server_time,
                        );
                        self.combat
                            .plum(origin, entity.time(), snapshot.server_time);
                    }
                }
                15 if entity.number() == snapshot.player.client_num()
                    && entity.event_parameter() == 2 =>
                {
                    let mode = integer("cg_duelsounds", 1);
                    if mode != 0 && mode != 2 && !race(game, &snapshot.player) {
                        self.announcement("BEGIN DUEL", 120.0);
                    }
                }
                _ => {}
            }
        }
        for (slot, present) in seen.into_iter().enumerate() {
            if !present {
                self.combat.signatures[slot] = 0;
            }
        }
    }

    fn prepare_announcement(&mut self, height: f32, append: bool) {
        let ms = self.millis(Instant::now());
        if self.center.is_none() {
            self.center = Some((std::mem::take(&mut self.combat.spare), ms));
        }
        let (text, old) = self.center.as_mut().unwrap();
        if !append || ms.saturating_sub(*old) >= self.options.center_time || text.len() > 650 {
            text.clear();
        } else if !text.is_empty() {
            text.push('\n');
        }
        *old = ms;
        self.combat.height = Some(height);
    }

    /// Reuse the ordinary centre-print channel and its expiry/contrast handling.
    pub(super) fn announcement(&mut self, value: &str, height: f32) {
        self.prepare_announcement(height, false);
        self.center.as_mut().unwrap().0.push_str(value);
    }

    pub(super) fn kill_announcement(
        &mut self,
        victim: u16,
        player: &sjk_protocol::PlayerState,
        gametype: i32,
        mode: i64,
    ) {
        self.prepare_announcement(if mode > 2 { 48.0 } else { 144.0 }, false);
        let name = self
            .roster
            .target(Some(victim))
            .and_then(|t| self.roster.display_name(t));
        let text = &mut self.center.as_mut().unwrap().0;
        text.push_str("You killed ");
        for character in name.unwrap_or("player").chars().take(48) {
            text.push(character);
        }
        if mode != 2 && gametype < 6 && !matches!(gametype, 2 | 3 | 4) {
            let _ = write!(
                text,
                "\nRank {} with {} points.",
                (player.persistent[2] & !0x4000) + 1,
                player.persistent[0] as i32
            );
        }
    }

    /// Draw pooled numeric strings; never format or allocate during rendering.
    pub(super) fn draw_plums(&mut self) {
        if !self.combat.enabled {
            return;
        }
        let Some(camera) = self.combat.camera else {
            return;
        };
        for plum in &self.combat.plums {
            let age = self.combat.time.saturating_sub(plum.start);
            if !(0..4000).contains(&age) {
                continue;
            }
            let c = 1.0 - age as f32 / 4000.0;
            let mut origin = plum.origin + glam::Vec3::Z * (110.0 - c * 100.0);
            let side = (camera.eye - origin)
                .cross(glam::Vec3::Z)
                .normalize_or_zero();
            origin += side * (-10.0 + 20.0 * (c * std::f32::consts::TAU).sin());
            if origin.distance(camera.eye) < 20.0 {
                continue;
            }
            let Some([x, y]) = camera.project(origin) else {
                continue;
            };
            let color = plum_color(plum.value, (c * 4.0).min(1.0));
            self.ui.text_aligned(
                &plum.text,
                Rect::new(x - 100.0, y - 16.0, 200.0, 32.0),
                22.0 * crate::ui_scale::height_scale(camera.viewport[1]),
                color,
                FontWeight::Semibold,
                0.0,
                TextAlign::Center,
            );
        }
    }
}

impl Combat {
    fn plum(&mut self, origin: [f32; 3], value: i32, start: i32) {
        let mut origin = glam::Vec3::from_array(origin);
        let original = origin;
        if (origin.z - self.last_origin.z).abs() <= 20.0 {
            origin.z -= 20.0;
        }
        self.last_origin = original;
        let next = (self.next + 1) % self.plums.len();
        let plum = &mut self.plums[self.next];
        self.next = next;
        plum.origin = origin;
        plum.value = value;
        plum.start = start;
        plum.text.clear();
        let _ = write!(plum.text, "{value}");
    }
}

fn plum_color(value: i32, alpha: f32) -> Color {
    let [r, g, b] = match value {
        ..0 => [1.0, 17.0 / 255.0, 17.0 / 255.0],
        0..2 => [1.0; 3],
        2..10 => [0.0, 1.0, 0.0],
        10..20 => [1.0, 1.0, 0.0],
        20..50 => [0.0, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0],
    };
    Color::new(r, g, b, alpha)
}

/// jaPRO alone assigns STAT_RACEMODE at index 11.
pub(crate) fn race(game: &GameState, player: &sjk_protocol::PlayerState) -> bool {
    player.stats[11] != 0
        && game
            .config_string(0)
            .is_some_and(|info| info.windows(5).any(|w| w.eq_ignore_ascii_case(b"japro")))
}

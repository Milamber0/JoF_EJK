//! Team logos over allies' heads (`cg_drawFriend`), as JoF EJK floats them
//! (`CG_PlayerFloatSprite` from `CG_Player`, `cg_players.c`): in team games the
//! Imperial logo (`sprites/team_red`) over a red teammate and the Rebel one
//! (`sprites/team_blue`) over a blue one, every player's while spectating; in power
//! duel the duel partner's icon; in Jedi Master the master's foes, as stock does.
//!
//! EJK's sprite is 20 units across in the world, 48 above the player's origin, and
//! drawn against the depth, so a wall hides it. Here the logo is a picture on the
//! screen: as large as a 24-unit sprite would be, but never smaller than can be read
//! nor larger than a name, standing just over the head where the nameplate's anchor
//! is (over the talk balloon or connection icon when one floats there); a trace from
//! the eye to that point hides it behind walls, as the depth did.
//! The nameplate and the plain overhead names sit above it ([`State::tops`]), so
//! neither covers it.
use super::identification::{Camera, info_number, unoccluded};
use super::nameplate_math::{HEAD_CLEARANCE, head_height};
use glam::Vec3;
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_protocol::{GameState, Snapshot};
use sjk_ui::{Color, DrawCommand, DrawList, Rect, TextureId};

/// `MAX_CLIENTS` and `CS_PLAYERS`.
const MAX_CLIENTS: usize = 32;
const CS_PLAYERS: usize = 1_131;
/// `ET_PLAYER`, `EF_DEAD`, `PW_CLOAKED` and `TEAM_SPECTATOR`.
const ET_PLAYER: u8 = 1;
const EF_DEAD: u32 = 1 << 1;
const PW_CLOAKED: u32 = 11;
const TEAM_SPECTATOR: i32 = 3;
/// The logo's size as a world sprite, in units (EJK's is 20: `radius` 10).
const WORLD_SIZE: f32 = 24.0;
/// Its smallest and largest size, in pixels of a 1080-line screen.
const MIN_SIZE: f32 = 20.0;
const MAX_SIZE: f32 = 40.0;
/// What the traces stop at: `CONTENTS_SOLID | CONTENTS_TERRAIN`, as the nameplates'.
const SIGHT_MASK: u32 = 1 | 0x0200_0000;

/// A logo floated over an ally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Logo {
    /// `cgs.media.teamRedShader`: the Imperial logo.
    Imperial,
    /// `cgs.media.teamBlueShader`: the Rebel logo.
    Rebel,
    /// `cgs.media.powerDuelAllyShader`.
    PowerDuelAlly,
}

impl Logo {
    /// In the HUD icon cells' order.
    pub(crate) const ALL: [Self; 3] = [Self::Imperial, Self::Rebel, Self::PowerDuelAlly];

    /// The shader `CG_RegisterGraphics` registers for it.
    pub(crate) const fn shader(self) -> &'static str {
        match self {
            Self::Imperial => "sprites/team_red",
            Self::Rebel => "sprites/team_blue",
            Self::PowerDuelAlly => "gfx/mp/pduel_icon_double",
        }
    }
}

/// The logo stock floats over a player of `team` (duel side `duel`) for a viewer of
/// `local` (duel side `local_duel`) in game type `mode`, if any (TaystJK and JoF EJK
/// `cg_players.c`: team games, power duel and Jedi Master each have their rule).
#[allow(clippy::too_many_arguments)]
pub(crate) fn ally_logo(
    mode: i32,
    local: i32,
    team: i32,
    local_duel: i32,
    duel: i32,
    master: bool,
    local_master: bool,
    target_master: bool,
) -> Option<Logo> {
    let spectator = local == TEAM_SPECTATOR;
    if mode >= 6 {
        (spectator || local == team).then_some(if team == 2 {
            Logo::Rebel
        } else {
            Logo::Imperial
        })
    } else if mode == 4 {
        // `DUELTEAM_DOUBLE` for a spectator: the pair.
        let ally = if spectator {
            duel == 2
        } else {
            local_duel == duel
        };
        ally.then_some(Logo::PowerDuelAlly)
    } else {
        (mode == 2 && (spectator || local == team) && master && !local_master && !target_master)
            .then_some(Logo::Imperial)
    }
}

/// The logos to draw this frame, and where each one's top is.
pub(crate) struct State {
    /// The logos, as textured quads, under the nameplates.
    pub(crate) list: DrawList,
    enabled: bool,
    /// Each client slot's logo top this frame, in pixels, if it has one.
    tops: [Option<f32>; MAX_CLIENTS],
    /// A world shot's staged team game, in place of the recorded one.
    #[cfg(test)]
    shot: Option<super::team_outline::ShotTeams>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            list: DrawList::new(MAX_CLIENTS),
            enabled: true,
            tops: [None; MAX_CLIENTS],
            #[cfg(test)]
            shot: None,
        }
    }
}

impl State {
    /// Sample the setting once a frame.
    pub(crate) fn sample(&mut self, console: Option<&crate::console::ViewerConsole>) {
        self.enabled = console
            .and_then(|c| c.bool_cvar("cg_drawfriend"))
            .unwrap_or(true);
    }

    /// Every slot's logo top, for the overhead labels to keep clear of.
    pub(crate) fn tops(&self) -> [Option<f32>; MAX_CLIENTS] {
        self.tops
    }

    /// Play `shot`'s team game over a recorded one, for world shots.
    #[cfg(test)]
    pub(crate) fn stage_for_shot(&mut self, shot: super::team_outline::ShotTeams) {
        self.shot = Some(shot);
    }

    /// Draw nothing (no session).
    pub(crate) fn clear(&mut self) {
        self.list.clear();
        self.tops = [None; MAX_CLIENTS];
    }

    /// Place this frame's logos with the rendered `camera`, unless `hidden`; `texture`
    /// gives each logo's picture (none drawn without one).
    #[allow(clippy::too_many_arguments)]
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
        texture: impl Fn(Logo) -> Option<TextureId>,
    ) {
        let sight = (bsp, scratch);
        #[cfg(test)]
        if let Some(shot) = self.shot.take() {
            let input = (snapshot, &shot.game, shot.local_team, world, now);
            self.collect(input, camera, sight, hidden, &texture);
            self.shot = Some(shot);
            return;
        }
        let local_team = i32::from(snapshot.player.team());
        let input = (snapshot, game, local_team, world, now);
        self.collect(input, camera, sight, hidden, &texture);
    }

    /// [`Self::update`] for a viewer on `local_team`.
    fn collect(
        &mut self,
        (snapshot, game, local_team, world, now): (
            &Snapshot,
            &GameState,
            i32,
            &sjk_runtime::World,
            i64,
        ),
        camera: Camera,
        (bsp, scratch): (&Bsp, &mut TraceScratch),
        hidden: bool,
        texture: &dyn Fn(Logo) -> Option<TextureId>,
    ) {
        self.clear();
        if !self.enabled || hidden {
            return;
        }
        let mode = info_number(game.config_string(0), "g_gametype");
        let local = snapshot.player.client_num();
        let local_duel = info_number(game.config_string(CS_PLAYERS + usize::from(local)), "ds");
        let master = snapshot
            .entities
            .iter()
            .any(|e| usize::from(e.number()) < MAX_CLIENTS && e.is_jedi_master());
        let local_master = snapshot.player.is_jedi_master();
        let unit = crate::ui_scale::height_scale(camera.viewport[1]);
        for entity in &snapshot.entities {
            let slot = entity.number();
            if usize::from(slot) >= MAX_CLIENTS
                || slot == local
                || entity.entity_type() != ET_PLAYER
                || entity.e_flags() & EF_DEAD != 0
                || entity.client_bitflag(local)
                || entity.powerups() & (1 << PW_CLOAKED) != 0
            {
                continue;
            }
            let info = game.config_string(CS_PLAYERS + usize::from(slot));
            let team = info_number(info, "t");
            if info.is_none() || team == TEAM_SPECTATOR {
                continue;
            }
            let Some(logo) = ally_logo(
                mode,
                local_team,
                team,
                local_duel,
                info_number(info, "ds"),
                master,
                local_master,
                entity.is_jedi_master(),
            ) else {
                continue;
            };
            let Some(picture) = texture(logo) else {
                continue;
            };
            let Some(presented) = world.entity(sjk_runtime::EntityId::new(u64::from(slot) + 1))
            else {
                continue;
            };
            let origin = Vec3::from_array(presented.sample(now).translation);
            let anchor = origin + Vec3::Z * height(entity);
            let Some(point) = camera.project(anchor) else {
                continue;
            };
            let trace = bsp.trace_box_with(
                scratch,
                camera.eye.to_array(),
                anchor.to_array(),
                Aabb::new([0.0; 3], [0.0; 3]).unwrap(),
                SIGHT_MASK,
            );
            if !unoccluded(trace.fraction, trace.start_solid, trace.all_solid) {
                continue;
            }
            let size = size(camera, anchor, unit);
            let top = point[1] - size;
            let _ = self.list.push(DrawCommand::TexturedQuad {
                rect: Rect::new(point[0] - size * 0.5, top, size, size),
                texture: picture,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
            self.tops[usize::from(slot)] = Some(top);
        }
    }
}

/// How far over `entity`'s origin its logo's foot stands: just over the head, or over
/// the stock sprite (`player_sprites.rs`) floating there.
fn height(entity: &sjk_protocol::EntityState) -> f32 {
    let head = head_height(entity.solid()) + HEAD_CLEARANCE;
    if sjk_client::legacy_player_sprite(entity.e_flags(), entity.entity_type(), false).is_some() {
        head.max(sjk_client::LEGACY_PLAYER_SPRITE_HEIGHT + sjk_client::LEGACY_PLAYER_SPRITE_RADIUS)
    } else {
        head
    }
}

/// The logo's size in pixels at world point `at`: a [`WORLD_SIZE`] sprite's, held
/// between [`MIN_SIZE`] and [`MAX_SIZE`] times the screen's `unit`.
fn size(camera: Camera, at: Vec3, unit: f32) -> f32 {
    let forward = (camera.target - camera.eye).normalize_or_zero();
    let depth = (at - camera.eye).dot(forward).max(1.0);
    let half = (camera.fov.to_radians() * 0.5).tan().max(1e-3);
    let projected = WORLD_SIZE * camera.viewport[1] / (2.0 * half * depth);
    projected.clamp(MIN_SIZE * unit, MAX_SIZE * unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_games_mark_teammates_with_their_sides_logo() {
        // Red on red: the Imperial logo; blue on blue: the Rebel one.
        assert_eq!(
            ally_logo(8, 1, 1, 0, 0, false, false, false),
            Some(Logo::Imperial)
        );
        assert_eq!(
            ally_logo(6, 2, 2, 0, 0, false, false, false),
            Some(Logo::Rebel)
        );
        // Never over an enemy.
        assert_eq!(ally_logo(8, 1, 2, 0, 0, false, false, false), None);
        // A spectator sees everyone's.
        assert_eq!(
            ally_logo(7, 3, 2, 0, 0, false, false, false),
            Some(Logo::Rebel)
        );
        // Free-for-all and duel: none.
        assert_eq!(ally_logo(0, 0, 0, 0, 0, false, false, false), None);
        assert_eq!(ally_logo(3, 0, 0, 0, 0, false, false, false), None);
    }

    #[test]
    fn power_duel_and_jedi_master_keep_stocks_rules() {
        assert_eq!(
            ally_logo(4, 0, 0, 2, 2, false, false, false),
            Some(Logo::PowerDuelAlly)
        );
        assert_eq!(ally_logo(4, 0, 0, 1, 2, false, false, false), None);
        assert_eq!(
            ally_logo(4, 3, 0, 0, 2, false, false, false),
            Some(Logo::PowerDuelAlly)
        );
        // Jedi Master: the master's foes, while there is one who is neither of them.
        assert_eq!(
            ally_logo(2, 0, 0, 0, 0, true, false, false),
            Some(Logo::Imperial)
        );
        assert_eq!(ally_logo(2, 0, 0, 0, 0, true, true, false), None);
        assert_eq!(ally_logo(2, 0, 0, 0, 0, false, false, false), None);
    }

    fn camera() -> Camera {
        Camera {
            eye: Vec3::ZERO,
            target: Vec3::X,
            up: Vec3::Z,
            fov: 74.0,
            viewport: [1920.0, 1080.0],
        }
    }

    #[test]
    fn the_logo_shrinks_with_range_but_stays_readable() {
        let near = size(camera(), Vec3::new(60.0, 0.0, 0.0), 1.0);
        let middle = size(camera(), Vec3::new(500.0, 0.0, 0.0), 1.0);
        let far = size(camera(), Vec3::new(5_000.0, 0.0, 0.0), 1.0);
        assert_eq!(near, MAX_SIZE);
        assert!(middle < MAX_SIZE && middle > MIN_SIZE, "{middle}");
        assert_eq!(far, MIN_SIZE);
        // Twice the screen, twice the pixels.
        assert_eq!(
            size(camera(), Vec3::new(5_000.0, 0.0, 0.0), 2.0),
            MIN_SIZE * 2.0
        );
    }
}

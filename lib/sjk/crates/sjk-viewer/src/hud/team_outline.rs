//! Team outlines (Sol's request, 13/10/2026): in team games (Team FFA, Siege, Capture
//! the Flag, Capture the Ysalamiri), every enemy is ringed in red and every teammate
//! in green, with the player card's ring ([`crate::card_outline`]).
//!
//! Like the card's, a ring follows only the part of a body that is in view: the mask
//! is drawn against the scene's depth, so nobody is outlined through a wall and the
//! outline gives away nothing the screen does not already show. Nobody is outlined
//! in free-for-all, duel or power duel, while spectating freely (a followed player's
//! team counts as the viewer's), nor a dead, cloaked or mind-tricking player. Each
//! outlined player costs one more draw of their body into a one-channel mask and the
//! ring's lookups inside a box around them, so a full server stays cheap.
use super::identification::{Camera, info_number};
use crate::card_outline::Group;
use glam::Vec3;
use sjk_protocol::{GameState, Snapshot};
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};

/// The setting that turns the team outlines on and off.
pub(crate) const CVAR: &str = "cg_teamOutline";

/// `GT_TEAM`, the first team game type (`bg_public.h` `gametype_t`).
const GT_TEAM: i32 = 6;
/// `TEAM_RED` and `TEAM_BLUE` (`teams.h`).
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
/// `ET_PLAYER`, `EF_DEAD` and `PW_CLOAKED`.
const ET_PLAYER: u8 = 1;
const EF_DEAD: u32 = 1 << 1;
const PW_CLOAKED: u32 = 11;
/// `CS_PLAYERS` and `MAX_CLIENTS`.
const CS_PLAYERS: usize = 1_131;
const MAX_CLIENTS: usize = 32;

/// Register the team outlines' setting.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        CVAR,
        true,
        CvarFlags::ARCHIVE,
        "In team games, outline enemies in red and teammates in green where you see them",
    ))
}

/// The side a player of `team` is on for a viewer on `local` in game type `mode`:
/// none outside team games, or when either is not on the red or blue team.
pub(crate) fn side(mode: i32, local: i32, team: i32) -> Option<Group> {
    let playing = |team| team == TEAM_RED || team == TEAM_BLUE;
    if mode < GT_TEAM || !playing(local) || !playing(team) {
        return None;
    }
    Some(if team == local {
        Group::Friend
    } else {
        Group::Enemy
    })
}

/// A team game staged over a recorded one, for world shots: the game state with the
/// game type and players' teams changed, and the viewer's team.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct ShotTeams {
    pub(crate) game: GameState,
    pub(crate) local_team: i32,
}

/// The client slots outlined this frame, as bits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Groups {
    friends: u32,
    enemies: u32,
}

impl Groups {
    /// The ring of the player in client slot `client`, if outlined.
    pub(crate) fn group(self, client: u64) -> Option<Group> {
        let bit = 1_u32.checked_shl(u32::try_from(client).ok()?)?;
        if self.enemies & bit != 0 {
            Some(Group::Enemy)
        } else if self.friends & bit != 0 {
            Some(Group::Friend)
        } else {
            None
        }
    }

    fn add(&mut self, client: u16, group: Group) {
        let bit = 1 << client;
        match group {
            Group::Friend => self.friends |= bit,
            Group::Enemy => self.enemies |= bit,
            Group::Card => {}
        }
    }
}

/// Who is outlined this frame and the screen areas their bodies may cover.
pub(crate) struct State {
    enabled: bool,
    groups: Groups,
    areas: [[f32; 4]; MAX_CLIENTS],
    count: usize,
    /// A world shot's staged team game, in place of the recorded one.
    #[cfg(test)]
    shot: Option<ShotTeams>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: true,
            groups: Groups::default(),
            areas: [[0.0; 4]; MAX_CLIENTS],
            count: 0,
            #[cfg(test)]
            shot: None,
        }
    }
}

impl State {
    /// Sample the setting once a frame.
    pub(crate) fn sample(&mut self, console: Option<&crate::console::ViewerConsole>) {
        self.enabled = console
            .and_then(|c| c.bool_cvar("cg_teamoutline"))
            .unwrap_or(true);
    }

    /// The players to outline, for the actor submission.
    pub(crate) fn groups(&self) -> Groups {
        self.groups
    }

    /// The screen areas (`[x, y, width, height]`) of the outlined players' bodies.
    pub(crate) fn areas(&self) -> &[[f32; 4]] {
        &self.areas[..self.count]
    }

    /// Outline nobody (no session).
    pub(crate) fn clear(&mut self) {
        self.groups = Groups::default();
        self.count = 0;
    }

    /// Play `shot`'s team game over a recorded one, for world shots.
    #[cfg(test)]
    pub(crate) fn stage_for_shot(&mut self, shot: ShotTeams) {
        self.shot = Some(shot);
    }

    /// Find this frame's outlined players: everyone on a side in view of the
    /// rendered `camera`, unless `hidden` (menus, the scoreboard, intermission).
    pub(crate) fn update(
        &mut self,
        snapshot: &Snapshot,
        game: &GameState,
        world: &sjk_runtime::World,
        now: i64,
        camera: Camera,
        hidden: bool,
    ) {
        #[cfg(test)]
        if let Some(shot) = self.shot.take() {
            let input = (snapshot, &shot.game, shot.local_team, world, now);
            self.collect(input, camera, hidden);
            self.shot = Some(shot);
            return;
        }
        let local_team = i32::from(snapshot.player.team());
        self.collect((snapshot, game, local_team, world, now), camera, hidden);
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
        hidden: bool,
    ) {
        self.clear();
        if !self.enabled || hidden {
            return;
        }
        let mode = info_number(game.config_string(0), "g_gametype");
        let local = snapshot.player.client_num();
        if side(mode, local_team, local_team).is_none() {
            return;
        }
        for entity in &snapshot.entities {
            let number = entity.number();
            if usize::from(number) >= MAX_CLIENTS
                || number == local
                || entity.entity_type() != ET_PLAYER
                || entity.e_flags() & EF_DEAD != 0
                || entity.client_bitflag(local)
                || entity.powerups() & (1 << PW_CLOAKED) != 0
            {
                continue;
            }
            let team = info_number(game.config_string(CS_PLAYERS + usize::from(number)), "t");
            let Some(group) = side(mode, local_team, team) else {
                continue;
            };
            let Some(presented) = world.entity(sjk_runtime::EntityId::new(u64::from(number) + 1))
            else {
                continue;
            };
            let origin = Vec3::from_array(presented.sample(now).translation);
            let top = super::nameplate_math::head_height(entity.solid());
            let area = super::player_card::body_area(camera, origin, top);
            if !on_screen(area, camera.viewport) {
                continue;
            }
            self.groups.add(number, group);
            self.areas[self.count] = area;
            self.count += 1;
        }
    }
}

/// Whether any of `area` lies on a `viewport`-sized screen.
fn on_screen(area: [f32; 4], viewport: [f32; 2]) -> bool {
    area[0] < viewport[0]
        && area[0] + area[2] > 0.0
        && area[1] < viewport[1]
        && area[1] + area[3] > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_team_games_outline_and_only_players_on_a_side() {
        // Team FFA, Siege, CTF, CTY: the other team red, one's own green.
        for mode in [6, 7, 8, 9] {
            assert_eq!(side(mode, TEAM_RED, TEAM_BLUE), Some(Group::Enemy));
            assert_eq!(side(mode, TEAM_BLUE, TEAM_BLUE), Some(Group::Friend));
        }
        // FFA, holocron, Jedi Master, duel, power duel, single player: nobody.
        for mode in 0..6 {
            assert_eq!(side(mode, TEAM_RED, TEAM_BLUE), None);
        }
        // A free spectator outlines nobody, and nobody outlines a spectator.
        assert_eq!(side(8, 3, TEAM_RED), None);
        assert_eq!(side(8, TEAM_RED, 3), None);
        assert_eq!(side(8, 0, TEAM_RED), None);
    }

    #[test]
    fn groups_answer_by_client_slot() {
        let mut groups = Groups::default();
        groups.add(0, Group::Friend);
        groups.add(31, Group::Enemy);
        assert_eq!(groups.group(0), Some(Group::Friend));
        assert_eq!(groups.group(31), Some(Group::Enemy));
        assert_eq!(groups.group(5), None);
        // Entities past the clients (corpses, NPCs) are never outlined.
        assert_eq!(groups.group(32), None);
        assert_eq!(groups.group(u64::MAX), None);
    }

    #[test]
    fn an_area_off_screen_is_skipped() {
        let viewport = [1920.0, 1080.0];
        assert!(on_screen([100.0, 100.0, 50.0, 50.0], viewport));
        assert!(on_screen([-40.0, -40.0, 50.0, 50.0], viewport));
        assert!(!on_screen([-60.0, 100.0, 50.0, 50.0], viewport));
        assert!(!on_screen([100.0, 1080.0, 50.0, 50.0], viewport));
    }
}

//! Retained siege class list; selection sends the stock class command, not userinfo.
use super::{InGameMenu, siege_data::Classes};
use sjk_protocol::GameState;
use sjk_vfs::VirtualFileSystem;

const PAGE_SIZE: usize = 8;

#[derive(Default)]
/// Selection cursor and load-time catalogue retained between menu frames.
pub(super) struct State {
    classes: Classes,
    team: usize,
    first: usize,
}

impl State {
    fn visible(&self) -> &[String] {
        let all = &self.classes.teams[self.team];
        &all[self.first..(self.first + PAGE_SIZE).min(all.len())]
    }

    fn paged(&self) -> bool {
        self.classes.teams[self.team].len() > PAGE_SIZE
    }

    /// Visible classes plus team switch, optional pagination, spectator and back actions.
    pub fn row_count(&self) -> usize {
        self.visible().len() + 3 + usize::from(self.paged())
    }

    /// Fill already allocated menu text slots without allocating.
    pub fn prepare(&self, rows: &mut [String; 24]) -> usize {
        rows[0].push_str(if self.team == 0 {
            "Red team  /  switch to Blue"
        } else {
            "Blue team  /  switch to Red"
        });
        for (row, name) in self.visible().iter().enumerate() {
            rows[row + 1].push_str(name);
        }
        let mut next = self.visible().len() + 1;
        if self.paged() {
            rows[next].push_str("Next page");
            next += 1;
        }
        rows[next].push_str("Spectate");
        rows[next + 1].push_str("Back");
        next + 2
    }
}

impl InGameMenu {
    /// Refresh from the mounted map and server's theme overrides on each open.
    pub(crate) fn refresh_siege(
        &mut self,
        game: &GameState,
        vfs: &VirtualFileSystem,
        team: u8,
    ) -> Result<(), String> {
        self.siege = State {
            classes: Classes::load(vfs, game)?,
            team: usize::from(team == 2),
            first: 0,
        };
        Ok(())
    }

    /// Resolve a visible row through the same path for mouse and keyboard activation.
    pub(crate) fn siege_action(&mut self, row: usize) -> super::CallVoteAction {
        use super::CallVoteAction as Action;
        let state = &mut self.siege;
        if row == 0 {
            state.team ^= 1;
            state.first = 0;
        } else if let Some(name) = state.visible().get(row - 1) {
            // Cmd_SiegeClass_f chooses the team from this name and enforces cooldowns.
            return Action::Send(format!("siegeclass \"{name}\""));
        } else if state.paged() && row == state.visible().len() + 1 {
            state.first += PAGE_SIZE;
            if state.first >= state.classes.teams[state.team].len() {
                state.first = 0;
            }
        } else if row == state.visible().len() + 1 + usize::from(state.paged()) {
            return Action::Send("team spectator".into());
        } else {
            return Action::Back;
        }
        Action::None
    }
}

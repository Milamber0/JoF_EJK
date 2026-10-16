//! Team votes and team leaders on this server: `callteamvote`, `teamvote`,
//! `CheckTeamVote` once a frame for each side, the ballots a client takes back when it
//! changes sides or leaves (`G_ClearVote`, `G_ClearTeamVote`), and `sess.teamLeader`
//! (`SetLeader`, `CheckTeamLeader`, the `tl` of its userinfo string). The rules are
//! `sjk_game_jka::team_vote`.
//!
//! A passed `leader` vote makes the leader three seconds on. The reference never does:
//! its `CheckTeamVote` sets the level's own `voteExecuteTime` instead, which runs the last
//! ordinary vote again; `g_stockRules 1` keeps that.

use super::NativeGame;
use sjk_game_jka::team_vote::{self, LeaderView, TeamVoteOutcome};

/// `TEAM_RED`, `TEAM_BLUE`, `TEAM_SPECTATOR`.
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
const TEAM_SPECTATOR: i32 = 3;

impl NativeGame {
    /// `callteamvote` and `teamvote` from `client`; `arguments` is the whole command.
    pub(super) fn team_vote_command(
        &mut self,
        client: usize,
        arguments: &[&[u8]],
        server_time: i32,
    ) {
        let voters = self.client_views();
        let allowed = self.cvars.integer(b"g_allowTeamVote") != 0;
        let said = if arguments[0].eq_ignore_ascii_case(b"callteamvote") {
            team_vote::call_team_vote(
                &mut self.vote.team,
                self.gametype,
                allowed,
                &voters,
                client,
                arguments,
                server_time,
            )
        } else {
            let Some(voter) = voters.iter().find(|voter| voter.client == client) else {
                return;
            };
            team_vote::cast_team_vote(
                &mut self.vote.team,
                voter,
                arguments.get(1).copied().unwrap_or_default(),
            )
        };
        self.tell_vote(said, Some(client));
    }

    /// `CheckTeamVote(TEAM_RED)` and `CheckTeamVote(TEAM_BLUE)`. A frame with no team vote
    /// running or waiting does nothing.
    pub(super) fn run_team_votes(&mut self, server_time: i32) {
        if self
            .vote
            .team
            .iter()
            .all(|vote| vote.time == 0 && vote.execute_time == 0)
        {
            return;
        }
        let (stock, voters) = (self.stock_rules(), self.client_views());
        for team in [TEAM_RED, TEAM_BLUE] {
            let voting = team_vote::team_voting_clients(&voters, team);
            let mut sides = std::mem::take(&mut self.vote.team);
            let (said, outcome) = team_vote::check_team_vote(
                &mut sides,
                team,
                voting,
                server_time,
                stock,
                &mut self.vote,
            );
            self.vote.team = sides;
            self.tell_vote(said, None);
            if let TeamVoteOutcome::Leader(team, client) = outcome {
                self.set_team_leader(team, client);
            }
        }
    }

    /// Everyone's side and leadership, as `SetLeader` and `CheckTeamLeader` read them.
    fn leader_views(&self) -> Vec<LeaderView> {
        (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                Some(LeaderView {
                    client,
                    team: peer.session.team,
                    connected: true,
                    bot: peer.bot,
                    name: peer.name.clone(),
                    leader: peer.session.team_leader,
                })
            })
            .collect()
    }

    fn keep_leaders(&mut self, views: &[LeaderView]) {
        for view in views {
            if let Some(peer) = self.peer_mut(view.client) {
                peer.session.team_leader = view.leader;
            }
        }
    }

    /// `SetLeader(team, client)`: the side's old leader and the new one have their
    /// userinfo strings published again, then the side is told.
    pub(super) fn set_team_leader(&mut self, team: i32, client: usize) {
        let mut views = self.leader_views();
        let (said, changed) = team_vote::set_leader(&mut views, team, client);
        self.keep_leaders(&views);
        for client in changed {
            if let Some(userinfo) = self.peer(client).map(|peer| peer.userinfo.clone()) {
                self.userinfo_changed(client, &userinfo);
            }
        }
        self.tell_vote(said, None);
    }

    /// What `SetTeam` does to the votes and the leaders (`g_cmds.c:883-904`) as `client`
    /// goes from `before` to the side it is on now: a spectator's ballot in the level's
    /// vote taken back, its ballot on the side it left taken back, its leadership given up,
    /// and that side given a leader if it has none.
    pub(in crate::bridge) fn votes_follow_team_change(&mut self, client: usize, before: i32) {
        let Some(after) = self.peer(client).map(|peer| peer.session.team) else {
            return;
        };
        let mut said = if after == TEAM_SPECTATOR {
            self.vote.clear(client)
        } else {
            Vec::new()
        };
        said.extend(team_vote::clear_team_vote(
            &mut self.vote.team,
            client,
            before,
        ));
        self.tell_vote(said, None);
        if let Some(peer) = self.peer_mut(client) {
            peer.session.team_leader = false;
        }
        if before == TEAM_RED || before == TEAM_BLUE {
            let mut views = self.leader_views();
            team_vote::check_team_leader(&mut views, before);
            self.keep_leaders(&views);
        }
    }

    /// `ClientDisconnect`'s `G_ClearVote` and `G_ClearTeamVote` for the leaving client.
    pub(in crate::bridge) fn clear_votes_of(&mut self, client: usize) {
        let team = self.peer(client).map_or(0, |peer| peer.session.team);
        let mut said = self.vote.clear(client);
        self.vote.forget(client);
        said.extend(team_vote::clear_team_vote(
            &mut self.vote.team,
            client,
            team,
        ));
        self.tell_vote(said, None);
    }
}

//! Spectators following players on the native server (`sjk_game_jka::follow`): the
//! `follow`, `follownext` and `followprev` commands, `SpectatorThink`'s buttons,
//! `StopFollowing`, and `SpectatorClientEndFrame` showing a follower the followed
//! player's own state.

use super::{GAMETYPE_DUEL, NativeGame, TEAM_SPECTATOR, Told};
use sjk_game_jka::client_begin::{SPECTATOR_FOLLOW, SPECTATOR_FREE, SPECTATOR_NOT};
use sjk_game_jka::client_view::client_number_from_string;
use sjk_game_jka::follow;
use sjk_protocol::UserCommand;

/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `persistant[PERS_TEAM]`.
const PERS_TEAM: usize = 3;

impl NativeGame {
    /// `follow`, `follownext`, `followprev`; `false` for any other command.
    pub(super) fn follow_command(
        &mut self,
        client: usize,
        words: &[&[u8]],
        server_time: i32,
    ) -> bool {
        let Some(&command) = words.first() else {
            return false;
        };
        if command.eq_ignore_ascii_case(b"follow") {
            self.cmd_follow(client, words.get(1).copied(), server_time);
        } else if command.eq_ignore_ascii_case(b"follownext") {
            self.follow_cycle(client, 1, server_time);
        } else if command.eq_ignore_ascii_case(b"followprev") {
            self.follow_cycle(client, -1, server_time);
        } else {
            return false;
        }
        true
    }

    /// Whether a team change is refused at this moment (`NOSWITCH`): a client in the game
    /// that changed teams less than five seconds ago.
    fn follow_refused(&mut self, client: usize) -> bool {
        let level_time = self.last_frame_time;
        let refused = self.peer(client).is_some_and(|peer| {
            peer.session.spectator_state == SPECTATOR_NOT
                && peer.session.switch_team_time > level_time
        });
        if refused {
            self.told
                .push(Told::One(client, b"print \"@@@NOSWITCH\n\"".to_vec()));
        }
        refused
    }

    /// A duellist asking to follow: "if they are playing a tournament game, count as a
    /// loss" (`WTF???`, the reference says).
    fn follow_forfeit(&mut self, client: usize) {
        let duel = self.gametype == GAMETYPE_DUEL || self.gametype == super::GAMETYPE_POWERDUEL;
        if let Some(peer) = self.peer_mut(client)
            && duel
            && peer.session.team == 0
        {
            peer.session.losses += 1;
        }
    }

    /// A player asking to follow becomes a spectator first, and may not change teams
    /// again for five seconds.
    fn spectate_to_follow(&mut self, client: usize, server_time: i32) {
        self.set_team_to(client, b"spectator", server_time);
        let level_time = self.last_frame_time;
        if let Some(peer) = self.peer_mut(client)
            && !peer.playing()
        {
            peer.session.switch_team_time = level_time + 5_000;
        }
    }

    /// `Cmd_Follow_f` (`g_cmds.c:1365-1418`).
    fn cmd_follow(&mut self, client: usize, argument: Option<&[u8]>, server_time: i32) {
        if self.follow_refused(client) {
            return;
        }
        let Some(argument) = argument else {
            if self
                .peer(client)
                .is_some_and(|peer| peer.session.spectator_state == SPECTATOR_FOLLOW)
            {
                self.stop_following(client);
            }
            return;
        };
        let views = self.client_views();
        let Some(target) =
            client_number_from_string(&views, argument, false).map(|view| view.client)
        else {
            let text = [
                b"print \"User ".as_slice(),
                argument,
                b" is not on the server\n\"",
            ]
            .concat();
            self.told.push(Told::One(client, text));
            return;
        };
        // Not oneself, not another spectator.
        if target == client || self.peer(target).is_none_or(|peer| !peer.playing()) {
            return;
        }
        self.follow_forfeit(client);
        if self.peer(client).is_some_and(|peer| peer.playing()) {
            self.spectate_to_follow(client, server_time);
        }
        if let Some(peer) = self.peer_mut(client) {
            (peer.session.spectator_state, peer.session.spectator_client) =
                (SPECTATOR_FOLLOW, target as i32);
        }
    }

    /// `Cmd_FollowCycle_f` (`g_cmds.c:1425-1497`): the next playing client either way.
    fn follow_cycle(&mut self, client: usize, dir: i32, server_time: i32) {
        if self.follow_refused(client) {
            return;
        }
        self.follow_forfeit(client);
        if self
            .peer(client)
            .is_some_and(|peer| peer.session.spectator_state == SPECTATOR_NOT)
        {
            self.spectate_to_follow(client, server_time);
        }
        let Some(current) = self.peer(client).map(|peer| peer.session.spectator_client) else {
            return;
        };
        let followable = |other: usize| {
            self.peer(other)
                .is_some_and(|peer| peer.begun && peer.playing())
        };
        if let Some(target) = follow::cycle(current, dir, self.players.places(), followable)
            && let Some(peer) = self.peer_mut(client)
        {
            (peer.session.spectator_state, peer.session.spectator_client) =
                (SPECTATOR_FOLLOW, target as i32);
        }
    }

    /// `StopFollowing` (`g_cmds.c:935-967`): a free spectator again, with its own view.
    fn stop_following(&mut self, client: usize) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        peer.state.persistent[PERS_TEAM] = u32::from(TEAM_SPECTATOR);
        peer.session.team = i32::from(TEAM_SPECTATOR);
        peer.session.spectator_state = SPECTATOR_FREE;
        follow::stop_following(&mut peer.state, client as u16);
        peer.knockdown.hand_extend_time = 0;
        peer.health = 100;
        peer.movement = peer.movement.reseeded(&peer.state);
    }

    /// `SpectatorThink`'s buttons (`g_active.c:741-757`), after its move: a press of attack
    /// follows the next player, of the alternate attack (while following) the one before;
    /// jumping stops following.
    pub(super) fn spectator_buttons(
        &mut self,
        client: usize,
        command: &UserCommand,
        server_time: i32,
    ) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let old = std::mem::replace(&mut peer.spectator_buttons, command.buttons);
        let pressed = |button: u16| command.buttons & button != 0 && old & button == 0;
        let following = peer.session.spectator_state == SPECTATOR_FOLLOW;
        if pressed(BUTTON_ATTACK) {
            self.follow_cycle(client, 1, server_time);
        } else if following && pressed(BUTTON_ALT_ATTACK) {
            self.follow_cycle(client, -1, server_time);
        }
        if self
            .peer(client)
            .is_some_and(|peer| peer.session.spectator_state == SPECTATOR_FOLLOW)
            && command.up_move > 0
        {
            self.stop_following(client);
        }
    }

    /// `level.follow1` and `level.follow2` (`CalculateRanks`): the first two clients in
    /// the game — in a duel, spectators too — in slot order.
    fn follow_slots(&self) -> [Option<usize>; 2] {
        let duel = self.gametype == GAMETYPE_DUEL || self.gametype == super::GAMETYPE_POWERDUEL;
        let mut slots = (0..self.players.places()).filter(|client| {
            self.peer(*client)
                .is_some_and(|peer| peer.begun && (peer.playing() || duel))
        });
        [slots.next(), slots.next()]
    }

    /// `SpectatorClientEndFrame` (`g_active.c:3654-3701`) for spectator `client`: a
    /// follower is shown the followed player's state; one whose player has left the game
    /// becomes a free spectator and begins again (unless it follows `follow1`/`follow2`,
    /// which find whoever plays next frame).
    pub(super) fn follow_end_frame(&mut self, client: usize, server_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        if peer.session.spectator_state != SPECTATOR_FOLLOW {
            return;
        }
        let wanted = peer.session.spectator_client;
        let slots = self.follow_slots();
        let target = match wanted {
            -1 => slots[0],
            -2 => slots[1],
            number => usize::try_from(number).ok(),
        };
        let Some(target) = target else { return };
        let followed = self
            .peer(target)
            .filter(|peer| peer.begun && peer.playing())
            .map(|peer| follow::followed_state(&peer.state));
        match followed {
            Some(state) => {
                if let Some(peer) = self.peer_mut(client) {
                    peer.state = state;
                }
            }
            None if wanted >= 0 => {
                if let Some(peer) = self.peer_mut(client) {
                    peer.session.spectator_state = SPECTATOR_FREE;
                }
                self.begin(client, server_time, None);
            }
            None => {}
        }
    }

    /// "send updated scores to any clients that are following this one" (`player_die`,
    /// `g_combat.c:2701-2714`): every spectator whose `spectatorClient` names the dead —
    /// never the dead itself, which `SetTeam` kills before it becomes a spectator (here
    /// the session's team has already changed).
    pub(super) fn scores_to_followers(&mut self, dead: usize, server_time: i32) {
        let followers: Vec<usize> = (0..self.players.places())
            .filter(|client| {
                *client != dead
                    && self.peer(*client).is_some_and(|peer| {
                        peer.begun
                            && !peer.playing()
                            && peer.session.spectator_client == dead as i32
                    })
            })
            .collect();
        if followers.is_empty() {
            return;
        }
        let message = self.scoreboard(server_time);
        for client in followers {
            self.told.push(Told::One(client, message.clone()));
        }
    }
}

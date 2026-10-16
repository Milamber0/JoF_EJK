//! What a player sends: `ClientCommand` (`g_cmds.c`) and `ClientThink` (`g_active.c`), for
//! the player at a place in the profile's order. The host boundary (`bridge_host.rs`) calls
//! them for legacy clients; the bots' frame calls them for bots.

use super::*;

impl NativeGame {
    /// `ClientCommand`: a reliable command from the player at `client`'s place, at the
    /// server's clock `server_time`.
    pub(super) fn player_command(&mut self, client: usize, text: &[u8], server_time: i32) {
        let mut words = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty());
        let Some(command) = words.next() else { return };
        if [
            b"kill".as_slice(),
            b"team",
            b"follow",
            b"follownext",
            b"followprev",
            b"setviewpos",
        ]
        .iter()
        .any(|name| command.eq_ignore_ascii_case(name))
        {}
        if command.eq_ignore_ascii_case(b"score") {
            // `Cmd_Score_f`: the scoreboard, as this client asked for it.
            let message = self.scoreboard(server_time);
            self.told.push(Told::One(client, message));
            return;
        }
        if command.eq_ignore_ascii_case(b"kill") {
            self.kill(client, server_time);
            return;
        }
        if command.eq_ignore_ascii_case(b"duelteam") {
            let word = text
                .split(|byte| byte.is_ascii_whitespace())
                .filter(|word| !word.is_empty())
                .nth(1);
            self.duel_team_command(client, word, server_time);
            return;
        }
        if [&b"follow"[..], b"follownext", b"followprev"]
            .iter()
            .any(|name| command.eq_ignore_ascii_case(name))
        {
            // `CMD_NOINTERMISSION` (`g_cmds.c:3398-3400`).
            if !self.refused_at_intermission(client, command) {
                let words: Vec<&[u8]> = text
                    .split(|byte| byte.is_ascii_whitespace())
                    .filter(|word| !word.is_empty())
                    .collect();
                self.follow_command(client, &words, server_time);
            }
            return;
        }
        if command.eq_ignore_ascii_case(b"setviewpos") {
            let arguments: Vec<&[u8]> = words.collect();
            self.set_view_position(client, &arguments, server_time);
            return;
        }
        if command.eq_ignore_ascii_case(b"give") {
            // `ClientCommand`'s gates (`g_cmds.c:3466-3480`): a cheat needs `sv_cheats`, and
            // this one a living player; then `Cmd_Give_f`.
            if !self.settings.cheats {
                self.told
                    .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
                return;
            }
            let arguments: Vec<&[u8]> = words.collect();
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            if peer.health <= 0 || !peer.playing() {
                self.told
                    .push(Told::One(client, b"print \"@@@MUSTBEALIVE\n\"".to_vec()));
                return;
            }
            let (name, rest) = arguments
                .split_first()
                .map_or((&b""[..], &[][..]), |(name, rest)| (*name, rest));
            let _ = give(&mut peer.state, &mut peer.health, name, rest);
            // The weapons and the ammo are the movement's to spend: it restarts from
            // the state as given.
            peer.movement = peer.movement.reseeded(&peer.state);
            peer.movement.set_health(peer.health);
            return;
        }
        if command.eq_ignore_ascii_case(b"siegeclass") {
            // `Cmd_SiegeClass_f`: only a siege game has classes.
            self.siege_class_command(client, text, server_time);
            return;
        }
        if self.chat_command(client, text, server_time)
            || self.vote_command(client, text, server_time)
            || self.npc_client_command(client, text)
            || self.cheat_command(client, text)
        {
            return;
        }
        if !command.eq_ignore_ascii_case(b"team") {
            // Every other game command is still to come.
            return;
        }
        let arguments: Vec<&[u8]> = words.collect();
        let gametype = self.gametype;
        // `SetTeam` on a siege server, once `Cmd_Team_f`'s own checks have passed.
        if self.siege.is_some()
            && let [word] = arguments.as_slice()
            && self
                .peer(client)
                .is_some_and(|peer| peer.session.switch_team_time <= server_time)
        {
            self.siege_team_command(client, word, server_time);
            return;
        }
        let sides = self.sides(client);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (before, name) = (peer.session.team, peer.name.clone());
        // The game this server is running, not a hard-coded free-for-all: `SetTeam`
        // only knows red and blue from `GT_TEAM` up, which is what a siege class's own
        // side change depends on.
        match team_command(
            &mut peer.session,
            &name,
            &arguments,
            gametype,
            sides,
            server_time,
        ) {
            TeamCommand::Told(text) => self.told.push(Told::One(client, text)),
            TeamCommand::Unchanged | TeamCommand::NotSupported => {}
            TeamCommand::Changed {
                announcement,
                queued,
            } => self.team_changed(client, before, announcement, queued, server_time),
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.session.team_command_done(before, server_time);
        }
    }

    /// `ClientThink`: one movement command from the player at `client`'s place.
    pub(super) fn player_think(&mut self, client: usize, command: &UserCommand, server_time: i32) {
        // `ClientThink_real` (g_active.c:2006-2012) keeps a command's time within a
        // second behind and a fifth of a second ahead of the server's.
        let mut command = *command;

        command.server_time = command.server_time.clamp(
            server_time.wrapping_sub(1000),
            server_time.wrapping_add(200),
        );

        // A pilot's command goes to its vehicle first (`g_active.c:1889-1910`).
        self.riding_opening(client, &mut command, self.last_frame_time);
        // `ClientThink_real`'s stance upkeep (`g_active.c:1914-1993`): the style the
        // sabers held allow.
        if let Some(peer) = self.peer_mut(client) {
            sjk_game_jka::saber_stance::upkeep_player(
                &mut peer.state,
                &mut peer.movement,
                &peer.sabers.hands,
            );
            // The powers the sabers forbid (`WP_ForcePowerUsable`), as they stand now.
            peer.force.saber_restrictions = peer.sabers.force_restrictions();
        }
        // `G_HeldByMonster` (`g_active.c:2000-2003`): a monster's victim goes where it is held.
        self.held_by_monster(client, &mut command);
        // `ClientThink_real` (`g_active.c:2356-2363`) sets the speed before EVERY move,
        // not once at the spawn: `g_speed`, times a siege class's own multiplier. Doing
        // it only at the spawn leaves a client that picked a class predicting one speed
        // while the server moves it at another, which shows up as prediction misses a
        // snapshot's worth of movement wide.
        if let Some(siege) = self.siege.as_ref() {
            let registry = std::sync::Arc::clone(&siege.registry);
            if let Some(peer) = self.peer_mut(client)
                && peer.playing()
            {
                let class_speed = peer
                    .siege_class_index
                    .map_or(1.0, |index| registry.classes[index].speed);
                peer.state.set_speed(PLAYER_SPEED * class_speed);
                peer.state
                    .set_base_speed((PLAYER_SPEED * class_speed) as i32);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        // `ClientThink_real` (`g_active.c:2046-2054`): at an intermission a player does
        // nothing at all but press the button that says it is ready to go.
        if self.match_end.intermission_time != 0 {
            self.intermission_think(client, &command, server_time);
            return;
        }
        // `SpectatorThink` (`g_active.c:697-760`): a follower does not move itself; the
        // buttons pick whom to follow.
        if let Some(following) = self
            .peer(client)
            .filter(|peer| !peer.playing())
            .map(|peer| {
                peer.session.spectator_state == sjk_game_jka::client_begin::SPECTATOR_FOLLOW
            })
        {
            if following {
                if let Some(peer) = self.peer_mut(client) {
                    peer.last_command_time = server_time;
                    peer.last_command = command;
                }
            } else {
                self.think_in_world(client, command, server_time);
            }
            self.spectator_buttons(client, &command, server_time);
            return;
        }
        self.think_in_world(client, command, server_time);
    }
}

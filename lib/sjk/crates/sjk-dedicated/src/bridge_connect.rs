//! `ClientConnect` (`g_client.c`) on this server, for players and bots alike.

use super::*;

impl NativeGame {
    /// `ClientConnect` for a player or, with `bot`, for a bot (`isBot`): a bot has no
    /// address of its own (`Bot`), no limit of connections from one, and in a team game
    /// takes the team its userinfo names. `client` is the player's place in the profile's
    /// order, which the host boundary has already worked out from the adapter's number.
    pub(super) fn connect_client(
        &mut self,
        client: usize,
        userinfo: &[u8],
        bot: bool,
    ) -> Result<(), Vec<u8>> {
        // `ClientConnect`: the IP filter first, the address limit, then the userinfo itself.
        let address = sjk_protocol::info_value(userinfo, b"ip")
            .unwrap_or_default()
            .to_vec();
        if self.filtered_out(&address) {
            return Err(b"Banned.".to_vec());
        }
        let address = if bot { b"Bot".to_vec() } else { address };
        let world = self
            .server
            .world(self.world)
            .ok_or_else(|| b"No world is running.".to_vec())?;
        if !bot
            && too_many_connections(
                &address,
                world.entities().map(|(_, peer)| peer.address.as_slice()),
                CONNECTIONS_PER_ADDRESS,
            )
        {
            return Err(b"Too many connections from the same IP".to_vec());
        }
        // `G_InitSessionData`: a newcomer's team (a free-for-all's free team, whose first
        // begin sends it to the spectators to set itself up; a full duel's line).
        let team = if bot {
            self.initial_bot_team(userinfo)
        } else {
            self.initial_team(userinfo)
        };
        // A bot's personality is not read yet: its Force configuration is empty, as the
        // reference's is for a bot whose personality file is missing.
        let bot_force = bot.then(|| (Vec::new(), 0.0));
        let mut session = PlayerSession {
            team,
            bot_force,
            spectator_state: sjk_game_jka::client_begin::SPECTATOR_FREE,
            duel_team: self.initial_duel_team(),
            ..PlayerSession::default()
        };
        self.siege_connect_session(&mut session);
        let accepted = judge(
            userinfo,
            self.userinfo_rules(),
            &session,
            self.gametype,
            &self.saber_parms(),
            None,
        )
        .map_err(|reason| {
            eprintln!("client {client} failed userinfo validation: {reason}");
            b"Failed userinfo validation".to_vec()
        })?;
        let movement = self.spectator(client);
        // As `ClientConnect` tells it: `ClientUserinfoChanged`'s advice and the newcomer's
        // string, then the join print.
        let advice = self.snaps_advice(userinfo);
        let world = self
            .server
            .world_mut(self.world)
            .ok_or_else(|| b"No world is running.".to_vec())?;
        let told = [
            advice.map(|text| Told::One(client, text)),
            Some(Told::PlayerString {
                client,
                previous: Vec::new(),
                value: accepted.client_info.clone(),
            }),
            Some(Told::Everyone(connect_print(&accepted.name))),
        ];
        let mut state = PlayerState::zero();
        movement.state().write_player_state(&mut state);
        let mut peer = Peer::connected(
            client,
            accepted,
            address,
            userinfo.to_vec(),
            session,
            state,
            movement,
        );
        peer.bot = bot;
        // The peer is the core's, under its own budget; its place in the profile's order is
        // the slot it came through. A place already held is never given twice.
        let admitted =
            world
                .spawn(peer)
                .ok()
                .and_then(|handle| match self.players.admit(client, handle) {
                    Some(_) => Some(handle),
                    None => {
                        world.despawn(handle);
                        None
                    }
                });
        match admitted {
            Some(_) => {
                self.jedi_master_slot_taken(client);
                // `G_InitSessionData`'s end: the newcomer at the back of the line.
                self.queue_at_back(client);
                // `ClientConnect` clears `pers` and the game flags: the slot has not voted.
                self.vote.forget(client);
                self.told.extend(told.into_iter().flatten());
                self.log_about(client, sjk_game_jka::game_log::client_connect);
                self.announce_team_on_connect(client);
                // The sabers are set as the player connects, their sounds registered;
                // then the ranks are recalculated, and everyone learns who joined.
                self.connect_sabers(client);
                self.calculate_ranks();
                // `ClientConnect`'s last act: everyone learns who joined. The server's
                // clock is not handed to connects; the event lives until the first frame
                // 300 ms on, which is longer than any connect takes to reach a snapshot.
                let _ = self.pool.spawn_temporary(
                    EventEntity::client_join(client as u16).state(),
                    self.last_frame_time,
                    None,
                );
                Ok(())
            }
            None => Err(b"Server is full.".to_vec()),
        }
    }

    /// `ClientDisconnect` (`g_client.c`) for the player at `client`'s place, which is free
    /// again afterwards: the next player there is another identity.
    pub(super) fn disconnect_client(&mut self, client: usize) {
        self.forget_queued_bot(client);
        self.log_about(client, sjk_game_jka::game_log::client_disconnect);
        self.clear_votes_of(client);
        let level_time = self.last_frame_time;

        self.duel_disconnect(client, level_time);
        let gametype = self.gametype;
        // Its powers stop first: a grip lets its victim go, its loops fall silent.
        if self
            .peer_mut(client)
            .is_some_and(|peer| peer.begun && peer.playing())
        {
            let _ = self.with_force_frame(client, level_time, |state, force, frame| {
                sjk_game_jka::force_powers::disconnect(state, force, frame)
            });
        }
        self.leave_vehicle(client, level_time, true);
        self.jedi_master_left(client);
        self.scripts_client_left(client);
        self.siege_carrier_left(client);
        let mut launched = Vec::new();
        if let (Some(handle), Some(world)) = (
            self.players.release(client),
            self.server.world_mut(self.world),
        ) {
            // A player in the game leaves in a flash, and nothing it carries goes with it
            // (`g_client.c:3955-3965`: `EV_PLAYER_TELEPORT_OUT`, `TossClientItems`).
            if let Some(peer) = world.entity_mut(handle)
                && peer.begun
                && peer.playing()
            {
                let flash = EventEntity::teleport_out(peer.state.origin(), client as u16);
                let _ = self.pool.spawn_temporary(flash.state(), level_time, None);
                let tossed = sjk_game_jka::dropped_items::toss_client_items(
                    &peer.state,
                    peer.entity.state(),
                    peer.last_command.weapon,
                    gametype,
                    level_time,
                    &mut self.deaths.rng,
                );
                if let Some(event) = tossed.event {
                    let _ = self.pool.spawn_temporary(event.state(), level_time, None);
                }
                for dropped in tossed.items {
                    let item = dropped.item;
                    if let Some(number) = self.pool.spawn_entity(dropped.state.clone(), level_time)
                    {
                        self.pool.set_bounds(number, dropped.bounds);
                        self.items.push((number, dropped));
                        launched.push(item);
                    }
                }
            }
            // `ClientDisconnect` empties the player's string for everyone.
            if let Some(peer) = world.entity_mut(handle) {
                self.told.push(Told::PlayerString {
                    client,
                    previous: std::mem::take(&mut peer.client_info),
                    value: Vec::new(),
                });
            }
            world.despawn(handle);
        }
        // A flag carried off is dropped where its carrier left (`Team_CheckDroppedItem`).
        for item in launched {
            self.flag_launched(item, level_time);
        }
    }

    /// `ClientConnect`'s `BroadcastTeamChange(client, -1)`: in a team game, a newcomer on
    /// red or blue is announced (`cp`) and logged as changing from `FREE`; not in siege.
    pub(super) fn announce_team_on_connect(&mut self, client: usize) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (name, team) = (peer.name.clone(), peer.session.team);
        // In a team game the teams are red and blue; a player not on one yet (here, one
        // whose first begin sends it to the spectators) is a spectator to the reference.
        let words: &[u8] = match team {
            1 => b"@@@JOINEDTHEREDTEAM",
            2 => b"@@@JOINEDTHEBLUETEAM",
            _ => return,
        };
        if self.gametype < GAMETYPE_TEAM || self.gametype == GAMETYPE_SIEGE {
            return;
        }
        self.told.push(Told::Everyone(
            [b"cp \"".as_slice(), &name, b"^7 ", words, b"\n\""].concat(),
        ));
        self.log_about(client, |who| {
            sjk_game_jka::game_log::change_team(who, -1, team)
        });
    }
}

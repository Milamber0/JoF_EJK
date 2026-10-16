//! The level played again on the same world: `map_restart` (`SV_MapRestart_f`,
//! `sv_ccmds.cpp:240-373`, and the game's `G_InitGame` with `restart` set).
//!
//! Unlike a map change, nobody is sent a new gamestate. The game's level is rebuilt —
//! every entity of the map spawned again, the match started again — while each
//! client keeps its session (team, spectator place, wins and losses: `G_ReadSessionData`
//! at its `ClientConnect(firstTime = qfalse)`). The endpoint is told
//! [`Told::MapRestarted`](super::Told::MapRestarted) and does the wire half: the
//! `map_restart` command, the snapshots' server bit, a full snapshot next.

use super::{NativeGame, Told};
use crate::peer::Peer;
use sjk_game_jka::entity_pool::EntityPool;
use sjk_game_jka::worldspawn::{CS_LEVEL_START_TIME, CS_WARMUP};
use sjk_protocol::PlayerState;

impl NativeGame {
    /// `g_gametype`: which game this server is running. It reaches the settings every
    /// Force rule is read against, the configstring clients are told, and — the part that
    /// matters for a siege map — which of the map's spawn points a player is put on.
    pub fn set_gametype(&mut self, gametype: i32) {
        // Which of the map's entities stand depends on the game (`G_SpawnGEntityFromSpawnVars`'s
        // `gametype` filter): the level's entities are spawned again, as the game type is
        // the level's from its start.
        let differs = gametype != self.gametype;
        self.gametype = gametype;
        self.settings.gametype = gametype;
        if differs {
            self.spawn_level_entities();
        }
        self.init_flags();
        if !differs {
            self.init_jedi_master(self.last_frame_time);
            self.init_holocrons(self.last_frame_time);
        }
        self.limits.gametype = gametype;
        // `InitSiegeMode` for the game type this server now runs.
        if differs {
            self.init_siege(self.last_frame_time);
        }
        // `CS_SERVERINFO`, which a legacy cgame reads `g_gametype` out of
        // (`CG_ParseServerinfo`): the one key is replaced in place, since the rest of
        // that string is the engine's and the game's own.
        let Some(slot) = self
            .config_strings
            .iter_mut()
            .find(|(index, _)| *index == 0)
        else {
            return;
        };
        let previous = slot.1.clone();
        let fields: Vec<&[u8]> = previous.split(|byte| *byte == b'\\').collect();
        let value = gametype.to_string();
        let mut rebuilt: Vec<u8> = Vec::with_capacity(previous.len() + 2);
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                rebuilt.push(b'\\');
            }
            // A value follows its key, and the pairs start after the leading separator.
            if index >= 2 && index % 2 == 0 && fields[index - 1] == b"g_gametype" {
                rebuilt.extend_from_slice(value.as_bytes());
            } else {
                rebuilt.extend_from_slice(field);
            }
        }
        slot.1 = rebuilt.clone();
        self.told.push(Told::ConfigString {
            index: 0,
            previous,
            value: rebuilt,
        });
    }

    /// Every entity of the map, spawned again into a new pool: the pool is rebuilt
    /// rather than emptied, so that no number of the old level can be mistaken for one
    /// of the new.
    pub(super) fn spawn_level_entities(&mut self) {
        self.pool = EntityPool::with_budget(0, self.entity_budget());
        self.missiles.clear();
        self.fired.clear();
        self.husks.clear();
        self.spheres.clear();
        self.charges.clear();
        self.items.clear();
        self.triggers.clear();
        self.movers.clear();
        self.multiples.clear();
        self.targets.clear();
        self.doors.clear();
        self.breakables.clear();
        self.usable_entities.clear();
        self.map_effects.forget_entities();
        self.map_turrets.forget_entities();
        self.spawn_items();
        self.spawn_triggers();
        self.spawn_movers();
        self.spawn_multiples();
        self.spawn_doors();
        self.spawn_breakables();
        self.spawn_usables();
        self.spawn_emplaced();
        self.spawn_map_effects();
        self.spawn_map_turrets();
        self.spawn_stock_entities();
        self.spawn_npcs();
        self.init_jedi_master(self.last_frame_time);
        self.init_holocrons(self.last_frame_time);
        self.spawn_scripts();
    }

    /// A new `level`: no scores, no limits hit, the clock from `server_time`, no
    /// siege round and no vote. A command a passed vote already queued still runs, as
    /// the reference's console buffer outlives the level.
    pub(super) fn start_level(&mut self, server_time: i32) {
        self.match_end = Default::default();
        self.power_duel = Default::default();
        self.team_scores = [0; 2];
        self.ready_mask = 0;
        self.level_start_time = server_time;
        // `InitSiegeMode`: the round of the new level, and its entities.
        self.init_siege(server_time);
        self.vote = Default::default();
    }

    /// `map_restart <delay>`: with a delay, `sv.restartTime` is set and shown as the
    /// warmup's end (`CS_WARMUP`), and the restart runs when it comes; without one, at
    /// once. A restart already waiting refuses another (`if ( sv.restartTime ) return`).
    pub(super) fn map_restart(&mut self, delay: i32, server_time: i32) {
        if self.restart_at != 0 {
            return;
        }
        if delay > 0 {
            self.restart_at = server_time + delay * 1_000;
            self.publish_config_string(CS_WARMUP, self.restart_at.to_string().as_bytes());
            return;
        }
        self.restart_level(server_time);
    }

    /// `map_restart 0` sent to the engine's command buffer (`SendConsoleCommand`), which
    /// runs it before the next frame: that frame is the restart's.
    pub(super) fn queue_restart(&mut self, server_time: i32) {
        if self.restart_at == 0 {
            self.restart_at = server_time + 1;
        }
    }

    /// `SV_Frame`'s check before the game's frame (`sv_main.cpp`): a delayed restart
    /// whose time has come runs now. Returns whether it did.
    pub(super) fn run_due_restart(&mut self, server_time: i32) -> bool {
        if self.restart_at == 0 || server_time < self.restart_at {
            return false;
        }
        self.restart_at = 0;
        self.restart_level(server_time);
        true
    }

    /// The restart itself.
    ///
    /// - A new serverId; `sv.restartedServerId` stays, so moves still naming this
    ///   world's earlier ids are ignored rather than answered with a gamestate.
    /// - `G_InitGame`'s strings, broadcast where they differ (`SV_SetConfigstring`
    ///   while `sv.restarting`), `CS_LEVEL_START_TIME` now; the registries keep what
    ///   they hold, as the configstrings do.
    /// - The level's entities and match, as new.
    /// - The endpoint told, then every client connected again with its session, and
    ///   those in the world begun again with their last command.
    pub(super) fn restart_level(&mut self, server_time: i32) {
        println!(
            "map_restart at {server_time} (world {} -> {})",
            self.map_generation,
            self.map_generation + 1
        );
        // The game is shut down and started again (`SV_MapRestart_f`).
        self.close_log();
        self.map_generation += 1;
        // `G_InitGame` reads `g_banIPs` again into the filter it kept: every filter is
        // added a second time, as in the reference.
        self.process_ip_bans();
        let (fresh, _) = super::world_config_strings(
            &self.status_info,
            self.map.as_ref(),
            self.gametype,
            self.map_generation,
        );
        // `SP_worldspawn` always writes the level's start time, a map or none.
        let start = (CS_LEVEL_START_TIME, server_time.to_string().into_bytes());
        for (index, value) in fresh
            .into_iter()
            .filter(|(index, _)| *index != CS_LEVEL_START_TIME)
            .chain([start])
        {
            let known = self
                .config_strings
                .iter()
                .find(|(known, _)| *known == index)
                .map(|(_, known)| known.as_slice());
            if known != Some(value.as_slice()) {
                self.publish_config_string(index, &value);
            }
        }
        self.spawn_level_entities();
        self.start_level(server_time);
        self.open_log();
        self.start_warmup();
        // `G_InitBots`.
        self.load_bots();
        self.reset_bot_minds(false);
        self.told.push(Told::MapRestarted);
        // `G_InitGame` clears every client before any connects again, so that nothing of
        // the last level — a score that reached the limit — is seen by the first to.
        let in_world: Vec<bool> = (0..self.players.places())
            .map(|client| self.clear_client(client))
            .collect();
        for (client, in_world) in in_world.into_iter().enumerate() {
            self.reconnect(client, in_world, server_time);
        }
    }

    /// `G_InitGame`'s `memset` of the client: as a fresh connect leaves it but for what
    /// the session and the connection carry — its session, its userinfo and what it said
    /// of itself, its sabers and posed model, its last command. Returns whether it was in
    /// the world.
    pub(super) fn clear_client(&mut self, client: usize) -> bool {
        let movement = self.spectator(client);
        let Some(peer) = self.peer_mut(client) else {
            return false;
        };
        let in_world = peer.begun;
        let mut state = PlayerState::zero();
        movement.state().write_player_state(&mut state);
        let fresh = Peer::connected(
            client,
            peer.accepted.clone(),
            peer.address.clone(),
            peer.userinfo.clone(),
            peer.session.clone(),
            state,
            movement,
        );
        let old = std::mem::replace(peer, fresh);
        peer.sabers = old.sabers;
        peer.skeleton = old.skeleton;
        peer.last_command = old.last_command;
        // ClientConnect(..., isBot) preserves bot ownership across restarts.
        peer.bot = old.bot;
        peer.personality = old.personality;
        in_world
    }

    /// `ClientConnect(clientNum, qfalse, isBot)` and, for a client in the world,
    /// `SV_ClientEnterWorld` with its last command.
    fn reconnect(&mut self, client: usize, in_world: bool, server_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let last_command = peer.last_command;
        self.vote.forget(client);
        // `ClientConnect`: a power duel's players wait in line again.
        if self.gametype == super::GAMETYPE_POWERDUEL
            && let Some(peer) = self.peer_mut(client)
        {
            peer.session.team = i32::from(super::TEAM_SPECTATOR);
        }
        // `BroadcastTeamChange(client, -1)` for a player on a team (not in siege), with no
        // "connected" print: this client was carried over from the level before.
        self.announce_team_on_connect(client);
        self.calculate_ranks();
        let join = sjk_game_jka::event_entity::EventEntity::client_join(client as u16);
        let _ = self
            .pool
            .spawn_temporary(join.state(), self.last_frame_time, None);
        if in_world {
            // `SV_ClientEnterWorld(client, &client->lastUsercmd)`.
            self.begin(client, server_time, Some(&last_command));
        }
    }
}

#[path = "bridge_warmup.rs"]
mod bridge_warmup;

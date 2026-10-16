//! Leaving a level: `ExitLevel` going on to the rotation's next map (or replaying this
//! one), and the new world a map change builds (`SV_SpawnServer`).

use super::*;

impl NativeGame {
    /// `ExitLevel` (`g_main.c:1434`). This server hosts one map and has no `nextmap`
    /// to run, so the level it goes to is the one it is on: the scores are cleared, the
    /// clock starts again, and everyone plays on. A map rotation is what would change
    /// here, and nothing else.
    pub(super) fn exit_level(&mut self, server_time: i32) {
        // A duel plays the next round on this level (`g_main.c:1440-1452`).
        // Siege plays its second round on this level, the sides swapped.
        if self.duel_exit_level(server_time) || self.siege_exit_level(server_time) {
            return;
        }
        // `vstr nextmap`: the next map of the rotation, if this server has one. Without
        // one it replays the map it is on, which is what a server with no `nextmap` set
        // amounts to.
        if let Some(next) = self.next_map() {
            self.at_map = self.at_map.wrapping_add(1);
            let loaded = self.game_data.as_ref().and_then(|game_data| {
                match crate::map::load(game_data, &next) {
                    Ok(map) => Some(map),
                    Err(error) => {
                        // A map that will not load must not take the server down with it:
                        // the match starts again on the map that is already loaded.
                        println!("map change to {next} failed ({error}); staying on this map");
                        None
                    }
                }
            });
            if loaded.is_some() || self.game_data.is_none() {
                let name = next.clone();
                self.change_map(&name, loaded, server_time);
                return;
            }
        }
        println!(
            "level exit at {server_time}; no nextmap on this server, so the match starts again"
        );
        self.match_end = Default::default();
        self.team_scores = [0; 2];
        self.ready_mask = 0;
        self.level_start_time = server_time;
        self.publish_config_string(sjk_game_jka::match_end::CS_INTERMISSION, b"");
        // The reference does not respawn here — its next level does. This server stays on
        // the map it has, so it puts everyone back into the world itself, and the exit
        // rules are held off while it does: a respawn ranks, ranking checks the exit
        // rules, and the scores are not cleared until every player is back.
        self.ranking = true;
        for client in 0..self.players.places() {
            let command = self.peer(client).map(|peer| peer.last_command);
            if let Some(command) = command {
                self.respawn(client, command, server_time, server_time);
            }
        }
        for client in 0..self.players.places() {
            if let Some(peer) = self.peer_mut(client) {
                peer.state.persistent[PERS_SCORE] = 0;
                peer.state.stats[sjk_game_jka::match_end::STAT_CLIENTS_READY] = 0;
                peer.ready_to_exit = false;
                peer.intermission_buttons = 0;
            }
        }
        self.ranking = false;
        self.calculate_ranks();
    }

    /// The maps this server plays after the one it started on, in order, for the process
    /// to set from its options. An empty rotation means the current map for ever, which
    /// is what a server started without one does.
    pub fn set_rotation(&mut self, maps: Vec<String>, game_data: Option<std::path::PathBuf>) {
        self.rotation = maps;
        self.at_map = 0;
        self.game_data = game_data;
    }

    /// The map `ExitLevel` should go to next, or `None` when this server has no rotation
    /// and therefore replays its own map. The rotation wraps, as a `nextmap` cycle does.
    fn next_map(&self) -> Option<String> {
        (!self.rotation.is_empty())
            .then(|| self.rotation[self.at_map % self.rotation.len()].clone())
    }

    /// `SV_SpawnServer` (`sv_init.cpp:451`) as far as one world's contents go: this
    /// server's answer to `vstr nextmap`.
    ///
    /// Everything of the old world goes and everything of the new one is built — a new
    /// serverId so that packets for the old world are recognised, new configstrings,
    /// new baselines, the map's entities spawned again, the match started again. The
    /// clients keep their slots: the reference reconnects each one with
    /// `firstTime = qfalse` and sets it back to `CS_CONNECTED`, and the endpoint does
    /// the wire half when it is told [`Told::MapChanged`].
    ///
    /// The old world's clock goes with that message, because each client is owed it as
    /// its `oldServerTime` until it acknowledges the new serverId.
    pub fn change_map(&mut self, mapname: &str, map: Option<LoadedMap>, server_time: i32) {
        // `G_ShutdownGame` for the level that ends; a stopped server has none.
        if !std::mem::take(&mut self.stopped) {
            self.close_log();
        }
        // `SV_Map_f`'s `Cvar_Get("g_gametype")`: a game type waiting for this map.
        self.take_latched_cvars();
        println!(
            "map change to {mapname} at {server_time} (world {} -> {})",
            self.map_generation,
            self.map_generation + 1
        );
        // A number no connected client has seen, so that the packets still arriving for
        // the world that just ended are recognised as stale rather than obeyed.
        self.map_generation += 1;
        self.restarted_generation = self.map_generation;
        self.identity.mapname = mapname.as_bytes().to_vec();
        self.cvars.set(b"mapname", mapname.as_bytes());
        self.status_info = server_info_with(&self.status_info, "mapname", mapname);
        let (config_strings, sounds) = world_config_strings(
            &self.status_info,
            map.as_ref(),
            self.gametype,
            self.map_generation,
        );
        self.config_strings = config_strings;
        self.sounds = sounds;
        self.map = map;
        self.models = ModelTable::default();
        // The old world's registrations, strings and effects end with it: its strings not
        // yet sent mean nothing to a client about to be sent the new gamestate.
        self.told
            .retain(|told| !matches!(told, Told::ConfigString { .. }));
        self.map_effects = Default::default();
        // The game module is loaded afresh for a new map: its filter starts empty and
        // `G_InitGame` reads `g_banIPs` into it.
        self.ip_filter = Default::default();
        self.process_ip_bans();
        self.spawn_level_entities();
        self.start_level(server_time);
        // `G_InitGame` for the new one.
        self.open_log();
        self.start_warmup();
        // `G_InitBots`.
        self.load_bots();
        self.reset_bot_minds(true);
        // Every player is a player of the new world, with nothing of the old one's on it.
        // It stays connected and stays in its slot; it re-enters when it acknowledges
        // the gamestate the endpoint is about to owe it.
        for client in 0..self.players.places() {
            // G_InitGame clears transient client state before ClientConnect.
            // In particular, entity-pool handles from the old map must not
            // survive and alias items or sabers in the new pool.
            self.clear_client(client);
        }
        self.told.push(Told::MapChanged { server_time });
    }
}

impl NativeGame {
    /// `SV_Shutdown("killserver")`'s game half: `G_ShutdownGame` (the log closed), and
    /// every player gone with the server — no `ClientDisconnect`, as the reference's
    /// clients vanish with `svs`. Until the next map ([`Self::change_map`]) the game
    /// takes no game commands.
    pub fn stop(&mut self) {
        self.close_log();
        for client in 0..self.players.places() {
            self.vote.forget(client);
            if let (Some(handle), Some(world)) = (
                self.players.release(client),
                self.server.world_mut(self.world),
            ) {
                world.despawn(handle);
            }
        }
        self.stopped = true;
    }

    /// Whether `killserver` stopped the server and no map has started it again.
    pub fn stopped(&self) -> bool {
        self.stopped
    }
}

/// Every configstring a world starts with, in index order: the two engine strings, what
/// `G_InitGame` publishes, what the map's worldspawn does, and the registered items.
///
/// A map change rebuilds exactly this, which is why it is written down once. The sound
/// table comes back with it because `init_game` fills one as it names its sounds, and the
/// new world needs that table rather than the old one's.
pub(super) fn world_config_strings(
    status_info: &[u8],
    map: Option<&LoadedMap>,
    gametype: i32,
    server_id: i32,
) -> (Vec<(usize, Vec<u8>)>, SoundTable) {
    // The two engine strings every legacy client reads first. The game's own strings
    // (version, models, sounds) arrive with the JKA game profile.
    let mut system_info = format!("\\sv_serverid\\{server_id}\\sv_pure\\0").into_bytes();
    system_info.extend_from_slice(b"\\sv_paks\\\\sv_pakNames\\");
    // `SV_SpawnServer`: the paks a client needs, which it downloads if it lacks them
    // (`FS_ReferencedPakChecksums`, `FS_ReferencedPakNames`). Not being pure, this server
    // references only the pak its map came from.
    if let Some(pak) = map.and_then(|map| map.pak.as_ref()) {
        system_info.extend_from_slice(
            format!(
                "\\sv_referencedPaks\\{} \\sv_referencedPakNames\\{}",
                pak.checksum, pak.name
            )
            .as_bytes(),
        );
    }
    let mut config_strings = vec![(0, status_info.to_vec()), (1, system_info)];
    // `G_InitGame`: its sounds, then what the map's worldspawn published, then the
    // location, duel and item strings; every index there is above these two.
    let mut set = |index: usize, value: &[u8]| config_strings.push((index, value.to_vec()));
    let sounds = init_game(&mut set);
    config_strings.extend(
        map.iter()
            .flat_map(|map| map.config_strings.iter().cloned()),
    );
    let mut set = |index: usize, value: &[u8]| config_strings.push((index, value.to_vec()));
    init_game_strings(gametype, &mut set);
    if let Some(map) = map {
        sjk_game_jka::chat::link_locations(&map.locations, &mut set);
    }
    // `SaveRegisteredItems` after the map's items: the four a player always has and
    // everything the map placed.
    if let Some(map) = map {
        config_strings.retain(|(index, _)| *index != CS_ITEMS);
        config_strings.push((CS_ITEMS, sjk_game_jka::items::registered(&map.items)));
    }
    // And which legacy animation fixes the simulation runs: clients predict from it.
    config_strings.push(authoritative().legacy_fixes_config_string());
    config_strings.sort_by_key(|(index, _)| *index);
    (config_strings, sounds)
}

/// `FS_MV_VerifyDownloadPath` and `FS_SV_FOpenFileRead` for a client's `download`: the
/// file must be `<the referenced pak's name>.pk3` (letter case aside) and not one of the
/// game's own paks (`FS_idPak`, `base/assets0` to `assets8`), which every client has.
pub(super) fn download_file(
    map: Option<&LoadedMap>,
    name: &[u8],
) -> sjk_network::LegacyDownloadFile {
    use sjk_network::LegacyDownloadFile;
    let Some(pak) = map.and_then(|map| map.pak.as_ref()) else {
        return LegacyDownloadFile::NotReferenced;
    };
    let id_pak = (0..9).any(|number| {
        pak.name
            .eq_ignore_ascii_case(&format!("base/assets{number}"))
    });
    if id_pak
        || !format!("{}.pk3", pak.name)
            .as_bytes()
            .eq_ignore_ascii_case(name)
    {
        return LegacyDownloadFile::NotReferenced;
    }
    match std::fs::read(&pak.path) {
        Ok(bytes) => LegacyDownloadFile::Found(bytes.into()),
        Err(_) => LegacyDownloadFile::Missing,
    }
}

/// Stored configstrings as the endpoint wants them.
pub(super) fn fixed(strings: &[(usize, Vec<u8>)]) -> impl Iterator<Item = (usize, &[u8])> {
    strings
        .iter()
        .map(|(index, value)| (*index, value.as_slice()))
}

/// The server runs the bare `Pmove`, without what a client's prediction adds around it.
pub(super) fn authoritative() -> sjk_game_jka::pmove::MovementConfig {
    sjk_game_jka::pmove::MovementConfig {
        authoritative: true,
        ..Default::default()
    }
}

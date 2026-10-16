//! The protocol-26 endpoint's view of the game ([`LegacyGameHost`]): the one place where a
//! legacy client number meets a player.
//!
//! Every call that names a client turns the adapter's number into the player's place in the
//! profile's order (`LegacyClientNumbers::ordinal`) before the game sees it; everything the
//! game tells a player goes back out under the client number of its place
//! (`LegacyClientNumbers::client`). The game itself never stores a wire client number
//! (`players.rs`).

use super::*;
use sjk_protocol::{LegacyClientNumbers, VehicleNetFields};

/// The place of the player a legacy client number names; `None` for a number no legacy
/// client can hold, which the adapter never offers.
fn place(client: usize) -> Option<usize> {
    LegacyClientNumbers::ordinal(client)
}

impl LegacyGameHost for NativeGame {
    fn client_connect(&mut self, client: usize, userinfo: &[u8]) -> Result<(), Vec<u8>> {
        // The adapter offers only numbers a legacy client can hold; one beyond them is
        // refused in the same words as a full server, never aliased.
        let Some(client) = place(client) else {
            return Err(b"Server is full.".to_vec());
        };
        self.connect_client(client, userinfo, false)
    }

    fn client_disconnect(&mut self, client: usize) {
        let Some(client) = place(client) else { return };
        self.disconnect_client(client);
    }

    fn client_userinfo_changed(&mut self, client: usize, userinfo: &[u8]) {
        let Some(client) = place(client) else { return };
        self.userinfo_changed(client, userinfo);
    }

    fn client_command(&mut self, client: usize, text: &[u8], server_time: i32) {
        let Some(client) = place(client) else { return };
        self.player_command(client, text, server_time);
    }

    fn enter_world(&mut self, client: usize, command: &UserCommand, server_time: i32) {
        let Some(client) = place(client) else { return };
        // `ClientBegin`, with the command the client entered with for the spawn's think.
        self.begin(client, server_time, Some(command));
    }

    fn client_think(&mut self, client: usize, command: &UserCommand, server_time: i32) {
        let Some(client) = place(client) else { return };
        self.player_think(client, command, server_time);
    }

    fn server_info(&self) -> LegacyServerInfo<'_> {
        let peers = self.peers() as i32;
        LegacyServerInfo {
            hostname: &self.identity.hostname,
            mapname: &self.identity.mapname,
            game_directory: b"",
            clients: peers,
            humans: peers,
            max_clients: self
                .server
                .world(self.world)
                .map_or(0, |world| world.entity_capacity() as i32),
            gametype: 0,
            needpass: 0,
            true_jedi: 0,
            weapon_disable: 0,
            duel_weapon_disable: 0,
            force_disable: 0,
            auto_demo: self.cvars.integer(b"sv_autoDemo"),
            min_ping: 0,
            max_ping: 0,
        }
    }

    fn status_info(&self) -> &[u8] {
        &self.status_info
    }

    fn status_players(&self) -> impl Iterator<Item = LegacyStatusPlayer<'_>> {
        // Every native peer, however it connected: the list is not the roster.
        self.server
            .world(self.world)
            .into_iter()
            .flat_map(|world| world.entities())
            .map(|(_, peer)| LegacyStatusPlayer {
                score: peer.state.persistent[PERS_SCORE] as i32,
                ping: peer.ping,
                name: &peer.name,
            })
    }

    fn server_ids(&self) -> (i32, i32) {
        // `sv.serverId` and `sv.restartedServerId`, which `SV_SpawnServer` sets to the
        // same value for a map change; they differ only across a `map_restart`.
        (self.map_generation, self.restarted_generation)
    }

    fn config_strings(&self) -> impl Iterator<Item = (usize, &[u8])> {
        // The map's strings with every connected legacy player's in between, in index
        // order. Those already connected hear of a newcomer through `take_output`.
        let players_at = self
            .config_strings
            .partition_point(|(index, _)| *index < CS_PLAYERS);
        let (before, after) = self.config_strings.split_at(players_at);
        let players = self.players.occupied().filter_map(|(id, handle)| {
            let peer = self.server.world(self.world)?.entity(handle)?;
            Some((
                CS_PLAYERS + id.legacy_client()?,
                peer.client_info.as_slice(),
            ))
        });
        fixed(before).chain(players).chain(fixed(after))
    }

    fn take_output(&mut self, tell: &mut dyn FnMut(LegacyGameOutput<'_>)) {
        // Every string a registration told (a sound, a model, an effect) is kept for the
        // gamestate of whoever connects later, not only sent to those already in (a map
        // change drops what the old world told before it: `change_map`).
        for told in &self.told {
            if let Told::ConfigString { index, value, .. } = told {
                match self
                    .config_strings
                    .iter_mut()
                    .find(|(known, _)| known == index)
                {
                    Some(slot) => slot.1.clone_from(value),
                    None => self.config_strings.insert(
                        self.config_strings
                            .partition_point(|(known, _)| known < index),
                        (*index, value.clone()),
                    ),
                }
            }
        }
        // What the game told a player it tells by the player's place; the wire names the
        // client. A place without a client number cannot be held by a legacy client (the
        // connect refuses it), so nothing addressed to one reaches the wire.
        for told in self.told.drain(..) {
            let output = match &told {
                Told::Everyone(text) => LegacyGameOutput::ServerCommand { client: None, text },
                Told::One(place, text) => match LegacyClientNumbers::client(*place) {
                    Some(client) => LegacyGameOutput::ServerCommand {
                        client: Some(client),
                        text,
                    },
                    None => continue,
                },
                Told::PlayerString {
                    client: place,
                    previous,
                    value,
                } => match LegacyClientNumbers::client(*place) {
                    Some(client) => LegacyGameOutput::ConfigString {
                        index: CS_PLAYERS + client,
                        previous,
                        value,
                    },
                    None => continue,
                },
                Told::ConfigString {
                    index,
                    previous,
                    value,
                } => LegacyGameOutput::ConfigString {
                    index: *index,
                    previous,
                    value,
                },
                Told::Drop {
                    client: place,
                    reason,
                } => match LegacyClientNumbers::client(*place) {
                    Some(client) => LegacyGameOutput::Drop { client, reason },
                    None => continue,
                },
                Told::MapChanged { server_time } => LegacyGameOutput::MapChanged {
                    server_time: *server_time,
                },
                Told::MapRestarted => LegacyGameOutput::MapRestarted,
            };
            tell(output);
        }
    }

    fn client_score(&self, client: usize) -> i32 {
        place(client)
            .and_then(|client| self.peer(client))
            .map_or(0, |peer| peer.state.persistent[PERS_SCORE] as i32)
    }
    fn console_command(
        &mut self,
        line: &[u8],
        server_time: i32,
        bots: &mut dyn sjk_network::LegacyBotSlots,
        print: &mut dyn FnMut(&[u8]),
    ) -> bool {
        self.console_line(line, server_time, bots, print)
    }
    fn client_ping(&mut self, client: usize, ping: i32) {
        if let Some(peer) = place(client).and_then(|client| self.peer_mut(client)) {
            peer.ping = ping;
        }
    }
    fn ban_file(&mut self) -> Option<Vec<u8>> {
        self.read_ban_file()
    }
    fn save_ban_file(&mut self, text: &[u8]) {
        self.write_ban_file(text);
    }
    fn resolve_host(&mut self, name: &str) -> Option<std::net::Ipv4Addr> {
        crate::config_files::resolve_host(name)
    }
    fn demo_open(&mut self, client: usize, path: &str) -> bool {
        self.open_demo(client, path)
    }
    fn demo_data(&mut self, client: usize, bytes: &[u8]) {
        self.write_demo(client, bytes);
    }
    fn demo_close(&mut self, client: usize) {
        self.close_demo(client);
    }
    fn file_exists(&mut self, path: &str) -> bool {
        self.config_files.home_has(path)
    }
    fn prune_auto_demos(&mut self, keep: usize) {
        self.config_files.prune_auto_demos(keep);
    }
    fn whitelist_file(&mut self) -> Option<Vec<u8>> {
        self.config_files.read_home(sjk_network::WHITELIST_FILE)
    }
    fn append_whitelist(&mut self, record: [u8; 4]) -> bool {
        self.config_files
            .append_record(sjk_network::WHITELIST_FILE, &record)
    }
    fn download_file(&self, name: &[u8]) -> sjk_network::LegacyDownloadFile {
        bridge_maps::download_file(self.map.as_ref(), name)
    }
    fn timestamp(&self) -> String {
        bridge_demos::timestamp(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default(),
        )
    }
    fn console_log(&mut self, text: &[u8]) {
        // A console nobody reads must not stop the server.
        let _ = std::io::Write::write_all(&mut std::io::stdout(), text);
    }

    fn baselines(&self) -> impl Iterator<Item = &EntityState> {
        // No map entities are spawned yet.
        std::iter::empty()
    }

    fn checksum_feed(&self) -> i32 {
        0
    }

    fn baseline(&self, _: u16) -> Option<&EntityState> {
        None
    }

    // Every client is a spectator so far: its simulated movement is its whole state.
    fn build_snapshot(&self, client: usize, frame: &mut LegacySnapshotFrame<'_>) {
        frame.player.clear();
        frame.player.set_client_num(client as u16);
        let Some(client) = place(client) else { return };
        let handle = self.players.at(client);
        let Some(world) = self.server.world(self.world) else {
            return;
        };
        if let Some(peer) = handle.and_then(|handle| world.entity(handle)) {
            frame.player.copy_from(&peer.state);
        }
        self.fill_ridden_vehicle(frame);

        // Everyone who plays, but never the client's own entity, "because it can be
        // regenerated from the playerstate" (`SV_BuildClientSnapshot`); a wire client
        // number is its entity's number, so the order is ascending. Only what the eye's
        // cluster can see and what no closed door cuts off (`visibility`).
        // `SV_BuildClientSnapshot`: seen from the eye, the origin raised by the view height.
        let eye = self.map.as_ref().map_or_else(Eye::everywhere, |map| {
            let mut eye = frame.player.origin();
            eye[2] += frame.player.view_height() as f32;
            Eye::new(&map.bsp, &map.areas, eye)
        });
        // The areas the eye's is joined to (`CM_WriteAreaBits`), inverted into the mask
        // the renderer wants: a set bit is an area not to draw.
        if let Some(map) = &self.map {
            let mut bits = [0_u8; 32];
            let bytes = map.areas.write_bits(eye.area(), &mut bits);
            bits.iter_mut().for_each(|byte| *byte ^= 255);
            frame.set_area_mask(&bits[..bytes]);
        }
        for (id, handle) in self.players.occupied() {
            // Or sent wherever it is (`broadcastClients`: the master in view, Force sight).
            // A player a legacy client cannot number is not sent (the connect admits none).
            let other = id.ordinal();
            let shown = world
                .entity(handle)
                .filter(|_| id.legacy_client().is_some())
                .filter(|peer| {
                    other != client
                        && peer.body_active()
                        && (self.map.is_none()
                            || peer.link.visible_from(&eye)
                            || peer
                                .broadcast_to
                                .get(client / 64)
                                .is_some_and(|word| word & (1 << (client % 64)) != 0))
                });
            if let Some(peer) = shown {
                // 32 players fit any snapshot; a refusal would leave this one unseen.
                let _ = frame.push(peer.entity.state());
            }
        }
        // Then the game's own entities, numbered above the players.
        self.pool_entities_for(client, &eye, |state| {
            let _ = frame.push(state);
        });
    }
}

impl NativeGame {
    /// `SV_BuildClientSnapshot` (`sv_snapshot.cpp:579-591`): a riding player's snapshot
    /// carries the player state of the vehicle its `m_iVehicleNum` names, here the NPC
    /// with that number. A vehicle that cannot be found leaves the frame's vehicle
    /// state as its history slot held it, as the reference leaves `frame->vps` when
    /// the entity has no player state.
    ///
    /// The vehicle-only netfields a stock client predicts the vehicle from come from
    /// the game's vehicle and the NPC's mind (`ps.vehOrientation` is the vehicle's
    /// `m_vOrientation`, `ps.moveDir` the NPC's movement direction).
    fn fill_ridden_vehicle(&self, frame: &mut LegacySnapshotFrame<'_>) {
        let number = frame.player.vehicle_entity_num();
        if number == 0 {
            return;
        }
        let Some(npc) = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)
        else {
            return;
        };
        vehicle_player_state(npc, frame.vehicle);
    }
}

/// The vehicle NPC's player state as a stock client predicts from it (`vps`): the NPC's
/// own, with the vehicle-only netfields of its vehicle and its movement direction.
pub(crate) fn vehicle_player_state(
    npc: &sjk_game_jka::npc_spawn::NpcActor,
    into: &mut sjk_protocol::PlayerState,
) {
    into.copy_from(&npc.player);
    let mut fields = VehicleNetFields {
        move_dir: npc.mind.move_dir,
        ..VehicleNetFields::default()
    };
    if let Some(vehicle) = npc.vehicle.as_deref() {
        fields.orientation = vehicle.orientation;
        fields.boarding = vehicle.ps_boarding;
        fields.weapons_linked = vehicle.ps_weapons_linked;
        fields.surfaces = vehicle.ps_surfaces;
        fields.hyperspace_time = vehicle.hyperspace_time;
        fields.hyperspace_angles = vehicle.hyperspace_angles;
        fields.turnaround_index = i32::from(vehicle.turnaround_index);
        fields.turnaround_time = vehicle.turnaround_time;
    }
    into.set_vehicle_fields(&fields);
}

//! The siege players (`g_client.c`, `g_cmds.c`, `g_saga.c`): a newcomer spectating with
//! the side it wants (`ClientConnect`), the class it plays (`ClientUserinfoChanged`,
//! `Cmd_SiegeClass_f`), a side change that waits for death (`SetTeam`'s siege branch,
//! `SetTeamQuick`), nobody in the world before the round begins (`ClientBegin`), what a
//! class hands its player (`ClientSpawn`), the respawn waves (`ClientRespawn`,
//! `SiegeRespawn`), the sides swapped for the second round (`SiegeDoTeamAssign`) and a stat
//! viewer's report on its team (`G_SiegeClientExData`).

use super::*;
use sjk_game_jka::client_begin::{SPECTATOR_FREE, SPECTATOR_NOT, team_for_word};
use sjk_game_jka::siege_class::{SiegeClass, loadout};
use std::sync::Arc;

/// `STAT_HOLDABLE_ITEM`: the holdable selected.
const STAT_HOLDABLE_ITEM: usize = 1;
/// `EF_DOUBLE_AMMO`.
const EF_DOUBLE_AMMO: u32 = 0x0100_0000;
/// `Q3_INFINITE`, a class's powerups' end.
const Q3_INFINITE: u32 = 16_777_216;
/// `MOD_TEAM_CHANGE`.
const MOD_TEAM_CHANGE: u32 = 42;

impl NativeGame {
    /// `ClientConnect`'s siege branch for a first connect (`g_client.c:2511-2530`): no side
    /// wanted yet — or, for one already on a side, that side wanted and the spectators until
    /// the round. The class is `none` (`G_InitSessionData`'s empty class, as the session's
    /// write and read leave it).
    pub(super) fn siege_connect_session(&self, session: &mut PlayerSession) {
        session.siege_class = "none".to_owned();
        if self.gametype != GAMETYPE_SIEGE {
            return;
        }
        session.siege_desired_team = 0;
        if session.team != i32::from(TEAM_SPECTATOR) {
            session.siege_desired_team = session.team;
            session.team = i32::from(TEAM_SPECTATOR);
        }
    }

    /// `ClientUserinfoChanged`'s siege class (`g_client.c:2217-2250`): the class the player
    /// plays on its team — its session's name resolved, `client->siegeClass` and the class's
    /// Force levels kept — or `None`.
    pub(super) fn siege_class_of(&mut self, client: usize) -> Option<SiegeClass> {
        let siege = self.siege.as_ref()?;
        let (registry, sides) = (Arc::clone(&siege.registry), siege.sides.clone());
        let peer = self.peer_mut(client)?;
        let (index, name) = sides.resolve(&registry, &peer.session.siege_class, peer.session.team);
        peer.session.siege_class = name;
        peer.siege_class_index = index;
        let class = index.map(|index| registry.classes[index].clone());
        peer.session.siege_force = class.as_ref().map(|class| class.force_levels);
        class
    }

    /// `ClientUserinfoChanged` with the userinfo the player already has: its string told
    /// again, as `SetTeamQuick` and `Cmd_SiegeClass_f` call it.
    pub(super) fn siege_userinfo_changed(&mut self, client: usize) {
        let Some(userinfo) = self.peer(client).map(|peer| peer.userinfo.clone()) else {
            return;
        };
        self.userinfo_changed(client, &userinfo);
    }

    /// `SetTeamQuick` (`g_saga.c:754-799`): the player moved to `team` without `SetTeam`'s
    /// ceremony — its class checked against the side, its userinfo's `team` key and its
    /// string rewritten — and begun again when `begin` asks for it.
    pub(super) fn set_team_quick(
        &mut self,
        client: usize,
        team: i32,
        begin: bool,
        server_time: i32,
    ) {
        if let Some(siege) = self.siege.as_ref() {
            let (registry, sides) = (Arc::clone(&siege.registry), siege.sides.clone());
            if let Some(peer) = self.peer_mut(client)
                && let Some(index) = sides.validated_class(&registry, team, peer.siege_class_index)
            {
                peer.siege_class_index = Some(index);
                peer.session.siege_class = registry.classes[index].name.clone();
            }
        }
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        peer.session.team = team;
        let key: &[u8] = match team {
            3 => b"s",
            1 => b"r",
            2 => b"b",
            _ => b"?",
        };
        peer.session.spectator_state = if team == i32::from(TEAM_SPECTATOR) {
            SPECTATOR_FREE
        } else {
            SPECTATOR_NOT
        };
        peer.session.spectator_client = 0;
        peer.userinfo = sjk_protocol::info_set_value(&peer.userinfo, b"team", key);
        self.siege_userinfo_changed(client);
        if begin {
            self.begin(client, server_time, None);
        }
    }

    /// `ClientBegin`'s siege rule (`g_client.c:2729-2730`): before the round begins, and once
    /// it has ended, a player begins as a spectator.
    pub(super) fn siege_begins_spectating(&mut self, client: usize, server_time: i32) {
        let Some(siege) = self.siege.as_ref() else {
            return;
        };
        if !siege.round.begun || siege.round.ended {
            self.set_team_quick(client, i32::from(TEAM_SPECTATOR), false, server_time);
        } else {
            // The class `SetTeam`'s `ClientUserinfoChanged` settled before this begin,
            // which `WP_InitForcePowers` reads.
            let _ = self.siege_class_of(client);
        }
    }

    /// `SetTeam`'s siege branch (`g_cmds.c:772-829`) for a side already read from the
    /// command's word: a waiting player may not spectate; asking for its own side does
    /// nothing; otherwise the side is wanted from now on, and a player already on a side
    /// dies (if it can) and moves over at once, to come back on the new side. Returns
    /// whether the branch dealt with it — `false` leaves a spectator's change to `SetTeam`.
    pub(super) fn siege_set_team(&mut self, client: usize, team: i32, level_time: i32) -> bool {
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        let waiting = peer.temp_spectate >= level_time;
        let spectator = i32::from(TEAM_SPECTATOR);
        if waiting && team == spectator {
            return true;
        }
        let old = peer.session.team;
        if team == old && team != spectator {
            return true;
        }
        peer.session.siege_desired_team = team;
        if old == spectator || team == spectator {
            return false;
        }
        if !waiting && peer.health > 0 {
            let (origin, sounds) = (peer.state.origin(), peer.saber_off_sounds());
            peer.health = 0;
            peer.state.stats[STAT_HEALTH] = 0;
            let mut death = DeathRequest::suicide(level_time, client as u16, origin, sounds, 0);
            death.means = MOD_TEAM_CHANGE;
            self.die(client, death);
        }
        if let Some(peer) = self.peer(client)
            && peer.session.team != peer.session.siege_desired_team
        {
            let desired = peer.session.siege_desired_team;
            self.set_team_quick(client, desired, false, level_time);
        }
        true
    }

    /// `SetTeam(ent, word)` on a siege server as the game itself calls it: the siege
    /// branch, else the ordinary change — begun afterwards unless `prevent_begin`
    /// (`g_preventTeamBegin`).
    fn siege_set_team_word(
        &mut self,
        client: usize,
        word: &[u8],
        prevent_begin: bool,
        server_time: i32,
    ) {
        let sides = self.sides(client);
        if self.siege_set_team(client, team_for_word(word, sides), server_time) {
            return;
        }
        let gametype = self.gametype;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (before, name) = (peer.session.team, peer.name.clone());
        if let TeamCommand::Changed {
            announcement,
            queued,
        } = sjk_game_jka::client_begin::set_team(
            &mut peer.session,
            &name,
            word,
            gametype,
            sides,
            server_time,
        ) {
            if prevent_begin {
                self.team_changed_unbegun(client, before, announcement, queued, server_time);
                self.siege_userinfo_changed(client);
            } else {
                self.team_changed(client, before, announcement, queued, server_time);
            }
        }
    }

    /// A `team` command on a siege server once `Cmd_Team_f`'s own checks have passed.
    pub(super) fn siege_team_command(&mut self, client: usize, word: &[u8], server_time: i32) {
        let before = self.peer(client).map_or(0, |peer| peer.session.team);
        self.siege_set_team_word(client, word, false, server_time);
        if let Some(peer) = self.peer_mut(client) {
            peer.session.team_command_done(before, server_time);
        }
    }

    /// `Cmd_SiegeClass_f` (`g_cmds.c:1171-1262`): the class named by the command's first
    /// argument — its side taken if need be (`SetTeam` without its begin), the class checked
    /// against the side, the player killed so that it comes back as it (or begun, from the
    /// spectators) unless it is waiting for a wave, its score kept, and no other class for
    /// five seconds.
    pub(super) fn siege_class_command(&mut self, client: usize, text: &[u8], server_time: i32) {
        let Some(siege) = self.siege.as_ref() else {
            return;
        };
        let (registry, sides) = (Arc::clone(&siege.registry), siege.sides.clone());
        let arguments: Vec<&[u8]> = sjk_network::LegacyTokens::new(text).collect();
        let Some(peer) = self.peer(client) else {
            return;
        };
        if peer.switch_class_time > server_time {
            self.told
                .push(Told::One(client, b"print \"@@@NOCLASSSWITCH\n\"".to_vec()));
            return;
        }
        let started_as_spectator = peer.session.team == i32::from(TEAM_SPECTATOR);
        let name =
            String::from_utf8_lossy(arguments.get(1).copied().unwrap_or_default()).into_owned();
        let Some(team) = sides.team_for_class(&registry, &name) else {
            return;
        };
        let team = i32::from(team);
        if self
            .peer(client)
            .is_some_and(|peer| peer.session.team != team)
        {
            self.siege_set_team_word(
                client,
                if team == 1 { b"red" } else { b"blue" },
                true,
                server_time,
            );
            let Some(peer) = self.peer(client) else {
                return;
            };
            if peer.session.team != team
                && (peer.session.team != i32::from(TEAM_SPECTATOR)
                    || peer.session.siege_desired_team != team)
            {
                self.told
                    .push(Told::One(client, b"print \"@@@NOCLASSTEAM\n\"".to_vec()));
                return;
            }
        }
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let score = peer.state.persistent[PERS_SCORE];
        peer.session.siege_class = sides
            .legal_class(&registry, team, &name)
            .err()
            .unwrap_or(name);
        self.siege_userinfo_changed(client);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.temp_spectate < server_time {
            if peer.health > 0 && !started_as_spectator {
                let (origin, sounds) = (peer.state.origin(), peer.saber_off_sounds());
                peer.health = 0;
                peer.state.stats[STAT_HEALTH] = 0;
                self.die(
                    client,
                    DeathRequest::suicide(server_time, client as u16, origin, sounds, 0),
                );
            }
            if self
                .peer(client)
                .is_some_and(|peer| peer.session.team == i32::from(TEAM_SPECTATOR))
                || started_as_spectator
            {
                self.begin(client, server_time, None);
            }
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.state.persistent[PERS_SCORE] = score;
            peer.switch_class_time = server_time + 5_000;
        }
    }

    /// `ClientRespawn`'s siege branch (`g_client.c:1240-1272`), after the body is left: with
    /// `g_siegeRespawn` a player not already waiting waits for the next wave — one health,
    /// nothing in hand, nothing to hurt, and every client told when the wave comes
    /// (`EV_SIEGESPEC`); otherwise it comes back now (`SiegeRespawn`).
    pub(super) fn siege_client_respawn(&mut self, client: usize, level_time: i32) {
        let waiting = self
            .peer(client)
            .is_some_and(|peer| peer.temp_spectate >= level_time);
        // `MaintainBodyQueue`: a player who was already waiting leaves no second body.
        if waiting && let Some(peer) = self.peer_mut(client) {
            peer.no_corpse = true;
        }
        self.leave_body(client, level_time);
        let wave = self.cvars.integer(b"g_siegeRespawn");
        if wave != 0 && !waiting {
            let check = self
                .siege
                .as_ref()
                .map_or(0, |siege| siege.round.respawn_check);
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            peer.temp_spectate = sjk_game_jka::siege::respawn_wait(wave, level_time);
            peer.health = 1;
            peer.state.stats[STAT_HEALTH] = 1;
            peer.state.set_raw_field(PS_WEAPON, 0);
            peer.state.stats[STAT_WEAPONS] = 0;
            peer.state.stats[STAT_HOLDABLE_ITEMS] = 0;
            peer.state.stats[STAT_HOLDABLE_ITEM] = 0;
            let origin = peer.state.origin();
            // Its thinks are a spectator's until the wave (`SpectatorThink`).
            peer.state.set_movement_type(PM_SPECTATOR);
            peer.movement = peer.movement.reseeded(&peer.state);
            let event = bridge_siege::siege_spec_event(origin, client, check);
            let _ = self.pool.spawn_temporary(event, level_time, None);
            return;
        }
        self.siege_respawn_now(client, level_time);
    }

    /// `SiegeRespawn` (`g_saga.c:801-811`): a player on the side it wants spawns again; one
    /// that wants the other side moves there and begins.
    pub(super) fn siege_respawn_now(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (team, desired, command) = (
            peer.session.team,
            peer.session.siege_desired_team,
            peer.last_command,
        );
        if team != desired {
            self.set_team_quick(client, desired, true, level_time);
        } else {
            self.respawn_spawn(client, command, level_time, level_time);
        }
    }

    /// `ExitLevel`'s siege half (`g_main.c:1456-1474`): with `g_siegeTeamSwitch`, the map is
    /// played again for the second round — or on to the next when the second is over —
    /// and every player's side, and the side it wants, are swapped (`SiegeDoTeamAssign`).
    /// Returns whether the level restarts here.
    pub(super) fn siege_exit_level(&mut self, server_time: i32) -> bool {
        let Some(siege) = self.siege.as_ref() else {
            return false;
        };
        let restart = self.siege_keep.persistent.beating_time && siege.round.team_switch;
        if !siege.round.team_switch {
            return false;
        }
        for client in 0..self.players.places() {
            let Some(peer) = self.peer_mut(client).filter(|peer| peer.begun) else {
                continue;
            };
            peer.session.siege_desired_team = match peer.session.siege_desired_team {
                1 => 2,
                2 => 1,
                other => other,
            };
            let swapped = match peer.session.team {
                1 => Some(2),
                2 => Some(1),
                _ => None,
            };
            if let Some(team) = swapped {
                self.set_team_quick(client, team, false, server_time);
            }
        }
        if restart {
            self.match_end.intermission_time = 0;
            for client in 0..self.players.places() {
                if let Some(peer) = self.peer_mut(client) {
                    peer.state.persistent[PERS_SCORE] = 0;
                }
            }
            self.queue_restart(server_time);
        }
        restart
    }

    /// `G_SiegeClientExData` (`g_saga.c:1935-1979`), once a second for a player whose class
    /// is a stat viewer (`CFL_STATVIEWER`): each teammate it can see, with its health, its
    /// maximum and the ammunition of the weapon it holds (`sxd`).
    pub(super) fn siege_ex_data(&mut self, client: usize, level_time: i32) {
        let Some(siege) = self.siege.as_ref() else {
            return;
        };
        let registry = Arc::clone(&siege.registry);
        let Some(peer) = self.peer(client) else {
            return;
        };
        let viewer = peer.siege_class_index.is_some_and(|index| {
            registry.classes[index].class_flags & (1 << sjk_game_jka::siege_class::CFL_STATVIEWER)
                != 0
        });
        if !viewer || peer.siege_data_time >= level_time {
            return;
        }
        let team = peer.session.team;
        let mut reports = Vec::new();
        for other in 0..self.players.places() {
            if other == client {
                continue;
            }
            let Some(them) = self
                .peer(other)
                .filter(|them| them.session.team == team && them.playing())
            else {
                continue;
            };
            let ammo = sjk_game_jka::weapon_data::legacy_weapon_data(them.state.weapon() as u8)
                .map_or(0, |data| them.state.ammo[data.ammo_index] as i32);
            reports.push(format!(
                "{}|{}|{}|{}",
                other,
                them.state.stats[STAT_HEALTH] as i32,
                them.state.stats[STAT_MAX_HEALTH] as i32,
                ammo
            ));
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.siege_data_time = level_time + 1_000;
        }
        if !reports.is_empty() {
            self.told.push(Told::One(
                client,
                format!("sxd {}", reports.join(" ")).into_bytes(),
            ));
        }
    }
}

/// `ClientSpawn`'s siege branch (`g_client.c:3528-3620`, `:3674-3722`) for a player of
/// `class`: in play, its weapons, the one it holds, their ammunition, its holdables and its
/// powerups; in play or not, its start health and armour.
pub(super) fn apply_siege_kit(state: &mut PlayerState, class: &SiegeClass, playing: bool) {
    let kit = loadout(class);
    if playing {
        state.stats[STAT_WEAPONS] = kit.weapons;
        state.set_raw_field(PS_WEAPON, kit.weapon);
        state.ammo.fill(0);
        for (kind, amount) in &kit.ammo {
            if let Some(slot) = state.ammo.get_mut(*kind) {
                *slot = *amount as u32;
            }
        }
        if kit.double_ammo {
            let flags = state.raw_field(EFLAGS).unwrap_or(0);
            state.set_raw_field(EFLAGS, flags | EF_DOUBLE_AMMO);
        }
        state.stats[STAT_HOLDABLE_ITEMS] = kit.holdables;
        state.stats[STAT_HOLDABLE_ITEM] = 0;
        for power in 0..16 {
            if kit.powerups & (1 << power) != 0 {
                state.powerups[power] = Q3_INFINITE;
            }
        }
    }
    if let Some(health) = kit.health {
        state.stats[STAT_HEALTH] = health as u32;
    }
    state.stats[STAT_ARMOR] = kit.armor as u32;
}

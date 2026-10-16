//! Bots on this server (`g_bot.c`): the definitions read as a level starts
//! (`G_InitBots`), `addbot` and `botlist`, and the spawn queue a delayed bot waits in
//! (`G_CheckBotSpawn`). A bot's slot is the legacy adapter's
//! ([`sjk_network::LegacyBotSlots`]); each bot's thinking (`botstates`) by slot, which
//! the bot frame runs ([`bridge_bot_frame`]).

use super::*;
use sjk_game_jka::bots::{ADDBOT_USAGE, AddBot, BOTS_FILE, BotRoster, bot_userinfo};
use sjk_game_jka::client_begin::pick_team;

/// `BOT_SPAWN_QUEUE_DEPTH`.
const SPAWN_QUEUE: usize = 16;

/// The game's bots: the definitions, the level's routes and the spawn queue.
#[derive(Clone, Debug, Default)]
pub(super) struct Bots {
    roster: BotRoster,
    /// The level's waypoints (`LoadPath_ThisLevel`).
    routes: sjk_game_jka::bot_routes::BotRoutes,
    /// `BotAIStartFrame`'s clock, the game module's own.
    clock: sjk_game_jka::bot_think::BotClock,
    /// `botSpawnQueue`: when each delayed bot begins, and which.
    queue: [(i32, usize); SPAWN_QUEUE],
    /// `botstates`: each bot's thinking, by its place, from its connect on.
    minds: Vec<Option<sjk_game_jka::bot_think::BotMind>>,
    /// What the bots see each frame, kept to be reused.
    views: bridge_bot_views::BotViews,
    /// `G_CheckMinimumPlayers`' `checkminimumplayers_time`, a `static` that no level resets.
    minimum_check_time: i32,
    /// The siege objectives' chain ends the routes are tied to.
    siege_links: Vec<bridge_routes::SiegeLink>,
    /// `level.arenas` (`G_LoadArenas`, which `G_InitBots` runs, or `G_InitGame` without
    /// `bot_enable`): the maps a vote may name and the game types each allows.
    pub(super) arenas: sjk_game_jka::arenas::Arenas,
}

/// No bot slots: for a console line the game runs on its own (a vote's), where no bot is
/// added.
pub(super) struct NoBotSlots;

impl sjk_network::LegacyBotSlots for NoBotSlots {
    fn allocate(&mut self) -> Option<usize> {
        None
    }
    fn free(&mut self, _client: usize) {}
    fn set_userinfo(&mut self, _client: usize, _userinfo: &[u8]) {}
}

/// `trap->Print`: the server's console.
fn console(text: &[u8]) {
    let _ = std::io::Write::write_all(&mut std::io::stdout(), text);
}

impl NativeGame {
    fn bots_enabled(&self) -> bool {
        self.cvars.integer(b"bot_enable") != 0
    }

    /// `BotAISetup`'s variables, then `G_InitBots`: the definitions (`G_LoadBots`:
    /// `g_botsFile` or `botfiles/bots.txt`, then every `scripts/*.bot`) and the level's
    /// routes. Nothing without `bot_enable`.
    pub(super) fn load_bots(&mut self) {
        self.load_arenas();
        if !self.bots_enabled() {
            return;
        }
        self.register_bot_cvars();
        let mut roster = BotRoster::default();
        let named = String::from_utf8_lossy(self.cvars.string(b"g_botsFile")).into_owned();
        let first = if named.is_empty() {
            BOTS_FILE.to_owned()
        } else {
            named
        };
        let text = self.config_files.read(&first);
        roster.load(&first, text.as_deref(), &mut console);
        for name in self.config_files.list_files("scripts", ".bot") {
            let path = format!("scripts/{name}");
            let text = self.config_files.read(&path);
            roster.load(&path, text.as_deref(), &mut console);
        }
        self.bots.roster = roster;
        self.cvars.get(
            b"bot_minplayers",
            b"0",
            crate::cvars::CVAR_SERVERINFO | crate::cvars::CVAR_VM_CREATED,
            None,
        );
        self.load_routes();
    }

    /// `G_LoadArenas`: every `scripts/*.arena`, in the archives' order.
    fn load_arenas(&mut self) {
        let names = self.config_files.list_files("scripts", ".arena");
        let texts: Vec<(String, Option<Vec<u8>>)> = names
            .into_iter()
            .map(|name| {
                let path = format!("scripts/{name}");
                let text = self.config_files.read(&path);
                (path, text)
            })
            .collect();
        self.bots.arenas = sjk_game_jka::arenas::Arenas::load(
            texts
                .iter()
                .map(|(path, text)| (path.as_str(), text.as_deref())),
            &mut console,
        );
    }

    /// `G_InitSessionData`'s team for a bot: in a team game the one its userinfo names
    /// (`r…` or `b…`), else the one `PickTeam(-1)` gives; otherwise as for a player.
    pub(super) fn initial_bot_team(&mut self, userinfo: &[u8]) -> i32 {
        if self.gametype < GAMETYPE_TEAM {
            return self.initial_team(userinfo);
        }
        match sjk_protocol::info_value(userinfo, b"team").and_then(|team| team.first()) {
            Some(b'r' | b'R') => 1,
            Some(b'b' | b'B') => 2,
            _ => pick_team(self.sides(usize::MAX)),
        }
    }

    /// `addbot` and `botlist` among the game's console commands; `None` for another.
    pub(super) fn bot_command(
        &mut self,
        words: &[&[u8]],
        server_time: i32,
        slots: &mut dyn sjk_network::LegacyBotSlots,
        print: &mut dyn FnMut(&[u8]),
    ) -> Option<()> {
        let name = words.first()?;
        if name.eq_ignore_ascii_case(b"addbot") {
            // `Svcmd_AddBot_f`: nothing at all while bots are off.
            if self.bots_enabled() {
                match AddBot::parse(&words[1..]) {
                    Some(request) => self.add_bot(&request, server_time, slots, print),
                    None => print(ADDBOT_USAGE),
                }
            }
            Some(())
        } else if name.eq_ignore_ascii_case(b"botlist") {
            self.bot_list(print);
            Some(())
        } else {
            None
        }
    }

    /// `G_AddBot`: a slot, the definition, the userinfo, and the bot connected as a
    /// client; then in the game at once, in a queue for its delay, or waiting in a duel's
    /// line.
    fn add_bot(
        &mut self,
        request: &AddBot,
        server_time: i32,
        slots: &mut dyn sjk_network::LegacyBotSlots,
        print: &mut dyn FnMut(&[u8]),
    ) {
        // The adapter hands out the wire slot; the bot's place is what that slot's number
        // projects to. A slot a legacy client could not hold is given back unused.
        let placed = slots.allocate().and_then(|slot| {
            match sjk_protocol::LegacyClientNumbers::ordinal(slot) {
                Some(client) => Some((slot, client)),
                None => {
                    slots.free(slot);
                    None
                }
            }
        });
        let Some((slot, client)) = placed else {
            self.told
                .push(Told::Everyone(b"print \"@@@UNABLE_TO_ADD_BOT\n\"".to_vec()));
            return;
        };
        let Some(botinfo) = self.bots.roster.by_name(&request.name).map(<[u8]>::to_vec) else {
            print(
                format!(
                    "^1Error: Bot '{}' not defined\n",
                    String::from_utf8_lossy(&request.name)
                )
                .as_bytes(),
            );
            slots.free(slot);
            return;
        };
        let team: &[u8] = if !request.team.is_empty() {
            &request.team
        } else if self.gametype >= GAMETYPE_TEAM {
            if pick_team(self.sides(client)) == 1 {
                b"red"
            } else {
                b"blue"
            }
        } else {
            b"red"
        };
        let userinfo = bot_userinfo(&botinfo, request.skill, team, &request.altname);
        slots.set_userinfo(slot, &userinfo);
        // `ClientConnect(clientNum, qtrue, qtrue)`. The reference keeps the slot of a bot
        // it refused; here it is given back.
        if self.connect_client(client, &userinfo, true).is_err() {
            slots.free(slot);
            return;
        }
        self.bot_connect(client, print);
        if let Some(peer) = self.peer(client) {
            slots.set_userinfo(slot, &peer.userinfo);
        }
        if self.gametype == GAMETYPE_DUEL || self.gametype == GAMETYPE_POWERDUEL {
            if let Some(peer) = self.peer_mut(client) {
                peer.session.duel_team = 0;
            }
            let duel_team = self.initial_duel_team();
            if let Some(peer) = self.peer_mut(client) {
                peer.session.duel_team = duel_team;
                peer.session.team = i32::from(TEAM_SPECTATOR);
            }
            self.set_team_to(client, b"s", server_time);
        } else if request.delay == 0 {
            self.begin(client, server_time, None);
        } else {
            self.queue_bot(client, server_time.wrapping_add(request.delay));
        }
    }

    /// `G_BotConnect` (`BotAISetupClient`): the bot's personality read
    /// (`BotUtilizePersonality`), its Force configuration and skill handed to
    /// `WP_InitForcePowers`. A missing file leaves the configuration empty, as the
    /// reference's cleared state has it.
    fn bot_connect(&mut self, client: usize, print: &mut dyn FnMut(&[u8])) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let value = |key: &[u8]| {
            sjk_protocol::info_value(&peer.userinfo, key)
                .unwrap_or_default()
                .to_vec()
        };
        let skill = sjk_game_jka::text_parse::atof(&value(b"skill"));
        // `FS_FOpenFileRead` takes a path without its leading slash.
        let path = String::from_utf8_lossy(&value(b"personality"))
            .trim_start_matches(['/', '\\'])
            .to_owned();
        let file = if path.is_empty() {
            None
        } else {
            self.config_files.read(&path)
        };
        let duel = self.gametype == GAMETYPE_DUEL || self.gametype == GAMETYPE_POWERDUEL;
        let found = file.is_some();
        let personality =
            sjk_game_jka::bot_personality::utilize_personality(file.as_deref(), duel, print);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let force = if found {
            personality.force_info.clone()
        } else {
            Vec::new()
        };
        peer.session.bot_force = Some((force, skill));
        let mind = sjk_game_jka::bot_think::BotMind::new(skill, personality.skills);
        peer.personality = Some(Box::new(personality));
        if self.bots.minds.len() <= client {
            self.bots.minds.resize(client + 1, None);
        }
        self.bots.minds[client] = Some(mind);
    }

    /// `AddBotToSpawnQueue`: begun once `spawn_time` comes, or at once with the queue full.
    fn queue_bot(&mut self, client: usize, spawn_time: i32) {
        match self.bots.queue.iter_mut().find(|(time, _)| *time == 0) {
            Some(entry) => *entry = (spawn_time, client),
            None => {
                console(b"^3Unable to delay spawn\n");
                let now = self.last_frame_time;
                self.begin(client, now, None);
            }
        }
    }

    /// `G_CheckBotSpawn` (from `BotAIStartFrame`, before the frame): `bot_minplayers`
    /// checked, then every queued bot whose time has come begins.
    pub(super) fn check_bot_spawn(&mut self, level_time: i32) {
        if self.bots_enabled() {
            self.check_minimum_players(level_time);
        }
        for index in 0..SPAWN_QUEUE {
            let (time, client) = self.bots.queue[index];
            if time == 0 || time > level_time {
                continue;
            }
            self.begin(client, level_time, None);
            self.bots.queue[index].0 = 0;
        }
    }

    /// `G_RemoveQueuedBotBegin`: a bot that leaves does not begin later.
    pub(super) fn forget_queued_bot(&mut self, client: usize) {
        if let Some(entry) = self
            .bots
            .queue
            .iter_mut()
            .find(|(_, queued)| *queued == client)
        {
            entry.0 = 0;
        }
    }

    /// `Svcmd_BotList_f`: each definition's name, model, personality file (its name
    /// alone, `COM_SkipPath`) and fun name, the reference's defaults where one is missing.
    fn bot_list(&self, print: &mut dyn FnMut(&[u8])) {
        print(b"name             model            personality              funname\n");
        for info in self.bots.roster.infos() {
            // `Q_strncpyz` into `MAX_NETNAME` (36) and `MAX_QPATH` (64).
            let value = |key: &[u8], size: usize, default: &[u8]| {
                let found = sjk_protocol::info_value(info, key).unwrap_or_default();
                let found = if found.is_empty() { default } else { found };
                String::from_utf8_lossy(&found[..found.len().min(size - 1)]).into_owned()
            };
            let name = value(b"name", 36, b"Padawan");
            let funname = value(b"funname", 36, b"");
            let model = value(b"model", 64, b"kyle/default");
            let personality = value(b"personality", 64, b"botfiles/kyle.jkb");
            let personality = personality.rsplit('/').next().unwrap_or_default();
            print(format!("{name:<16} {model:<16} {personality:<20} {funname:<20}\n").as_bytes());
        }
    }
}

#[path = "bridge_bot_frame.rs"]
mod bridge_bot_frame;
#[path = "bridge_bot_views.rs"]
mod bridge_bot_views;
#[path = "bridge_minplayers.rs"]
mod bridge_minplayers;
#[path = "bridge_routes.rs"]
mod bridge_routes;

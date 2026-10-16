//! Votes on this server: `callvote` and `vote` from clients, `CheckVote` every frame
//! (`sjk_game_jka::vote`), and what a passed vote does.
//!
//! The reference carries a passed vote out as a console command, which its command buffer
//! runs before the next frame. This server does the same with each of the commands a vote
//! can produce — a limit, `g_doWarmup`, `g_gametype` (latched until the map it goes on
//! to), `map` (with `set nextmap` kept), `map_restart`, a kick and `vstr nextmap` — so
//! every stock vote is offered ([`SUPPORTED_VOTES`]).

use super::{NativeGame, Told};
use sjk_game_jka::arenas::Arenas;
use sjk_game_jka::vote::{self, Connection, Said, VOTES, VoteSettings, VoteWorld, Voter};
use sjk_network::LegacyTokens;

/// The votes this server can carry out, as `g_allowVote` bits: all of `validVoteStrings`.
pub const SUPPORTED_VOTES: i32 = (1 << VOTES.len()) - 1;

/// What a vote reads of this server: its files, its arenas and its cvars.
struct ServerVoteWorld<'a> {
    files: &'a crate::config_files::ConfigFiles,
    arenas: &'a Arenas,
    nextmap: Vec<u8>,
    mapname: &'a [u8],
    auto_map_cycle: bool,
    limits: (i32, i32, bool),
}

impl VoteWorld for ServerVoteWorld<'_> {
    fn map_exists(&self, map: &[u8]) -> bool {
        self.files
            .has(&format!("maps/{}.bsp", String::from_utf8_lossy(map)))
    }
    fn arenas(&self) -> &Arenas {
        self.arenas
    }
    fn nextmap(&self) -> &[u8] {
        &self.nextmap
    }
    fn mapname(&self) -> &[u8] {
        self.mapname
    }
    fn auto_map_cycle(&self) -> bool {
        self.auto_map_cycle
    }
    fn limits(&self) -> (i32, i32, bool) {
        self.limits
    }
}

/// `SV_DropClient`'s reason for a kick (`sv_ccmds.cpp:571`).
const WAS_KICKED: &[u8] = b"@@@WAS_KICKED";

impl NativeGame {
    /// `g_allowVote` and `g_voteDelay`, for the process to set from its options. Bits
    /// for votes this server cannot carry out are cleared, and the result returned so
    /// that the operator can be told.
    pub fn set_vote_rules(&mut self, allow: i32, delay: i32) -> i32 {
        self.set_cvar("g_allowVote", &allow.to_string());
        self.set_cvar("g_voteDelay", &delay.to_string());
        self.vote_rules.0
    }

    /// The votes this server offers: `g_allowVote`'s bits it can carry out.
    pub fn offered_votes(&self) -> i32 {
        self.vote_rules.0
    }

    /// The settings a vote is read against, as this frame finds the server.
    fn vote_settings(&self) -> VoteSettings {
        VoteSettings {
            allow: self.vote_rules.0,
            delay: self.vote_rules.1,
            gametype: self.gametype,
            nextmap: !self.rotation.is_empty() || !self.cvars.string(b"nextmap").is_empty(),
            max_clients: self.players.places(),
        }
    }

    /// The server as a vote reads it.
    fn vote_world(&self) -> ServerVoteWorld<'_> {
        let integer = |name: &[u8]| self.cvars.integer(name);
        ServerVoteWorld {
            files: &self.config_files,
            arenas: &self.bots.arenas,
            nextmap: self.cvars.string(b"nextmap").to_vec(),
            mapname: &self.identity.mapname,
            auto_map_cycle: integer(b"g_autoMapCycle") != 0,
            limits: (
                integer(b"fraglimit"),
                integer(b"timelimit"),
                integer(b"g_fraglimitVoteCorrection") != 0,
            ),
        }
    }

    /// Every occupied slot, as the game's commands see it.
    pub(super) fn client_views(&self) -> Vec<Voter> {
        (0..self.players.places())
            .filter_map(|client| {
                let peer = self.peer(client)?;
                Some(Voter {
                    client,
                    connection: if peer.begun {
                        Connection::Connected
                    } else {
                        Connection::Connecting
                    },
                    team: peer.session.team,
                    bot: peer.bot,
                    name: peer.name.clone(),
                    temp_spectate: 0,
                })
            })
            .collect()
    }

    /// `callvote` and `vote`; `false` for any other command. Both are
    /// `CMD_NOINTERMISSION` (`g_cmds.c:3395,3430`): refused once the match is ending.
    pub(super) fn vote_command(&mut self, client: usize, text: &[u8], server_time: i32) -> bool {
        let arguments: Vec<&[u8]> = LegacyTokens::new(text).collect();
        let Some(&command) = arguments.first() else {
            return false;
        };
        let calling = command.eq_ignore_ascii_case(b"callvote");
        if command.eq_ignore_ascii_case(b"callteamvote")
            || command.eq_ignore_ascii_case(b"teamvote")
        {
            if !self.refused_at_intermission(client, command) {
                self.team_vote_command(client, &arguments, server_time);
            }
            return true;
        }
        if !calling && !command.eq_ignore_ascii_case(b"vote") {
            return false;
        }
        if self.refused_at_intermission(client, command) {
            return true;
        }
        let settings = self.vote_settings();
        let voters = self.client_views();
        let said = if calling {
            let mut running = std::mem::take(&mut self.vote);
            let said = vote::call_vote(
                &settings,
                &mut running,
                &voters,
                client,
                &arguments,
                server_time,
                &self.vote_world(),
            );
            self.vote = running;
            said
        } else {
            let Some(voter) = voters.iter().find(|voter| voter.client == client) else {
                return true;
            };
            vote::cast_vote(
                &settings,
                &mut self.vote,
                voter,
                arguments.get(1).copied().unwrap_or_default(),
            )
        };
        self.tell_vote(said, Some(client));
        true
    }

    /// `ClientCommand`'s `CMD_NOINTERMISSION` gate (`g_cmds.c:3460-3465`): whether the
    /// match is ending, having told the client so.
    pub(super) fn refused_at_intermission(&mut self, client: usize, command: &[u8]) -> bool {
        if !self.match_end.ending() {
            return false;
        }
        let refusal = [
            b"print \"@@@CANNOT_TASK_INTERMISSION (".as_slice(),
            command,
            b")\n\"",
        ]
        .concat();
        self.told.push(Told::One(client, refusal));
        true
    }

    /// `CheckVote`, once a frame after the exit rules (`g_main.c:3394-3400`); and before
    /// it, whatever an earlier frame's vote queued. The reference's console buffer runs
    /// between frames, so a vote passed in one frame takes effect at the start of the
    /// next — and the cvar change it makes is announced in that frame's `G_UpdateCvars`.
    pub(super) fn run_votes(&mut self, server_time: i32) {
        for command in std::mem::take(&mut self.vote_commands) {
            self.carry_out(&command, server_time);
        }
        // Only a running vote needs the count, so a frame without one gathers nobody.
        let voting = if self.vote.time == 0 {
            0
        } else {
            vote::voting_clients(&self.vote_settings(), &self.client_views())
        };
        let mut running = std::mem::take(&mut self.vote);
        let said = vote::check_vote(
            &mut running,
            voting,
            server_time,
            self.gametype,
            &self.vote_world(),
        );
        self.vote = running;
        // `CheckTeamVote` for each side, after `CheckVote`.
        self.run_team_votes(server_time);
        if !said.is_empty() {
            self.tell_vote(said, None);
        }
    }

    pub(super) fn tell_vote(&mut self, said: Vec<Said>, caller: Option<usize>) {
        for said in said {
            match said {
                Said::Caller(text) => {
                    if let Some(client) = caller {
                        self.told.push(Told::One(client, text));
                    }
                }
                Said::Everyone(text) => self.told.push(Told::Everyone(text)),
                Said::One(client, text) => self.told.push(Told::One(client, text)),
                Said::ConfigString(index, value) => {
                    // `SV_SetConfigstring` broadcasts nothing when the value is unchanged
                    // (`sv_init.cpp:77-80`).
                    let known = self
                        .config_strings
                        .iter()
                        .find(|(known, _)| *known == index)
                        .map(|(_, value)| value.as_slice());
                    if known.unwrap_or_default() != value.as_slice() {
                        self.publish_config_string(index, &value);
                    }
                }
                Said::Execute(command) => self.vote_commands.push(command),
                Said::SetCvar(name, value) => self.set_cvar(name, &String::from_utf8_lossy(&value)),
                // `G_KickAllBots`: `clientkick` for every bot, ahead of the map change.
                Said::KickBots => {
                    let bots: Vec<usize> = (0..self.players.places())
                        .filter(|&client| self.peer(client).is_some_and(|peer| peer.bot))
                        .collect();
                    self.told.extend(bots.into_iter().map(|client| Told::Drop {
                        client,
                        reason: WAS_KICKED.to_vec(),
                    }));
                }
            }
        }
    }

    /// A passed vote's console command, as the command buffer runs it: each of its
    /// `;`-separated commands through the console, a kick through the endpoint, and
    /// `vstr nextmap` to the rotation's next map where this server has a rotation, else
    /// the `nextmap` cvar's own commands.
    fn carry_out(&mut self, command: &[u8], server_time: i32) {
        println!(
            "vote carried out at {server_time}: {}",
            String::from_utf8_lossy(command)
        );
        self.run_vote_text(command, server_time, 0);
    }

    fn run_vote_text(&mut self, text: &[u8], server_time: i32, depth: usize) {
        for command in split_commands(text) {
            let words: Vec<&[u8]> = LegacyTokens::new(command).collect();
            let (Some(&name), value) = (words.first(), words.get(1).copied().unwrap_or_default())
            else {
                continue;
            };
            if name.eq_ignore_ascii_case(b"clientkick") {
                // `SV_KickNum_f`: the slot must still hold somebody.
                if let Ok(client) = String::from_utf8_lossy(value).parse::<usize>()
                    && self.peer(client).is_some()
                {
                    self.told.push(Told::Drop {
                        client,
                        reason: WAS_KICKED.to_vec(),
                    });
                }
            } else if name.eq_ignore_ascii_case(b"vstr")
                && value.eq_ignore_ascii_case(b"nextmap")
                && !self.rotation.is_empty()
            {
                self.exit_level(server_time);
            } else if name.eq_ignore_ascii_case(b"vstr") && depth < 8 {
                // `Cvar_Vstr_f`: the variable's text run as commands.
                let text = self.cvars.string(value).to_vec();
                self.run_vote_text(&text, server_time, depth + 1);
            } else if !self.console_line(
                command,
                server_time,
                &mut super::bridge_bots::NoBotSlots,
                &mut |text| print!("{}", String::from_utf8_lossy(text)),
            ) {
                println!(
                    "vote command {} is not one this server carries out",
                    String::from_utf8_lossy(command)
                );
            }
        }
    }
}

/// `Cbuf_Execute`'s cut: at each `;` outside quotes and at each line break.
fn split_commands(text: &[u8]) -> Vec<&[u8]> {
    let mut commands = Vec::new();
    let (mut quoted, mut start) = (false, 0);
    for (at, &byte) in text.iter().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b';' if !quoted => {
                commands.push(&text[start..at]);
                start = at + 1;
            }
            b'\n' | b'\r' => {
                commands.push(&text[start..at]);
                start = at + 1;
                quoted = false;
            }
            _ => {}
        }
    }
    commands.push(&text[start..]);
    commands
        .into_iter()
        .map(<[u8]>::trim_ascii)
        .filter(|command| !command.is_empty())
        .collect()
}

#[path = "bridge_team_votes.rs"]
mod bridge_team_votes;

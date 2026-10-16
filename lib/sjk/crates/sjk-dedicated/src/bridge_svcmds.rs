//! The game's own console commands (`g_svcmds.c:ConsoleCommand`): the IP filter
//! (`addip`, `removeip`, `listip`, kept in `g_banIPs`), `forceteam`, and the vote and
//! userinfo-validation toggles.

use super::NativeGame;
use sjk_game_jka::client_view::strip_colours;

impl NativeGame {
    /// One of the game's console commands; `false` for any other line.
    pub(super) fn game_console_command(
        &mut self,
        words: &[&[u8]],
        server_time: i32,
        print: &mut dyn FnMut(&[u8]),
    ) -> bool {
        let Some(&name) = words.first() else {
            return false;
        };
        let is = |command: &[u8]| name.eq_ignore_ascii_case(command);
        if is(b"addip") {
            match words.get(1) {
                Some(word) => {
                    let ban_ips = self.ip_filter.add(word, print);
                    self.set_cvar("g_banIPs", &String::from_utf8_lossy(&ban_ips));
                }
                None => print(b"Usage: addip <ip-mask>\n"),
            }
        } else if is(b"removeip") {
            match words.get(1) {
                Some(word) => {
                    if let Some(ban_ips) = self.ip_filter.remove(word, print) {
                        self.set_cvar("g_banIPs", &String::from_utf8_lossy(&ban_ips));
                    }
                }
                None => print(b"Usage: removeip <ip-mask>\n"),
            }
        } else if is(b"listip") {
            self.ip_filter.list(print);
        } else if is(b"forceteam") {
            self.force_team(words, server_time, print);
        } else if is(b"toggleallowvote") {
            let names: Vec<&str> = sjk_game_jka::vote::VOTES
                .iter()
                .map(|vote| vote.string)
                .collect();
            self.toggle_bits(
                "g_allowVote",
                &names,
                words.get(1).copied(),
                "ToggleAllowVote",
                print,
                |name, on| format!("{name} {}^7\n", if on { "^2Enabled" } else { "^1Disabled" }),
            );
        } else if is(b"toggleuserinfovalidation") {
            let names: Vec<&str> = sjk_game_jka::userinfo::validation_rule_names().collect();
            self.toggle_bits(
                "g_userinfoValidate",
                &names,
                words.get(1).copied(),
                "ToggleUserinfoValidation",
                print,
                |name, on| format!("{name} {}\n", if on { "Validated" } else { "Ignored" }),
            );
        } else {
            return false;
        }
        true
    }

    /// `Svcmd_ForceTeam_f` with `ClientForString`: a connected player by slot number or
    /// by its name without colours, put on a team as `SetTeam` puts it.
    fn force_team(&mut self, words: &[&[u8]], server_time: i32, print: &mut dyn FnMut(&[u8])) {
        if words.len() < 3 {
            print(b"Usage: forceteam <player> <team>\n");
            return;
        }
        let handle = words[1];
        let connected =
            |game: &NativeGame, client: usize| game.peer(client).is_some_and(|peer| peer.begun);
        // `StringIsInteger`: digits only, at least one.
        let by_number = (!handle.is_empty() && handle.iter().all(u8::is_ascii_digit))
            .then(|| sjk_game_jka::userinfo::atoi(handle))
            .and_then(|number| usize::try_from(number).ok())
            .filter(|&client| client < self.players.places() && connected(self, client));
        let wanted = strip_colours(handle);
        let found = by_number.or_else(|| {
            (0..self.players.places()).find(|&client| {
                connected(self, client)
                    && self
                        .peer(client)
                        .is_some_and(|peer| strip_colours(&peer.name).eq_ignore_ascii_case(&wanted))
            })
        });
        let Some(client) = found else {
            print(&[&b"User "[..], handle, b" is not on the server\n"].concat());
            return;
        };
        self.set_team_to(client, words[2], server_time);
    }

    /// `Svcmd_ToggleAllowVote_f` and `Svcmd_ToggleUserinfoValidation_f`: with no
    /// argument, every bit of the variable with its name; with one, that bit flipped
    /// (the variable cut to the bits that have names) and the outcome told.
    fn toggle_bits(
        &mut self,
        cvar: &str,
        names: &[&str],
        argument: Option<&[u8]>,
        title: &str,
        print: &mut dyn FnMut(&[u8]),
        told: impl Fn(&str, bool) -> String,
    ) {
        let value = self.cvars.integer(cvar.as_bytes());
        let Some(argument) = argument else {
            for (index, name) in names.iter().enumerate() {
                print(
                    format!(
                        "{index:2} [{}] {name}\n",
                        if value & (1 << index) != 0 { 'X' } else { ' ' }
                    )
                    .as_bytes(),
                );
            }
            return;
        };
        // `trap->Argv` into eight bytes, then `atoi`.
        let index = sjk_game_jka::userinfo::atoi(&argument[..argument.len().min(7)]);
        if index < 0 || index as usize >= names.len() {
            print(format!("{title}: Invalid range: {index} [0, {}]\n", names.len() - 1).as_bytes());
            return;
        }
        let flipped = (1 << index) ^ (value & ((1 << names.len()) - 1));
        self.set_cvar(cvar, &flipped.to_string());
        print(told(names[index as usize], flipped & (1 << index) != 0).as_bytes());
    }

    /// `g_userinfoValidate`: the rules a userinfo is held to.
    pub(super) fn userinfo_rules(&self) -> u32 {
        self.cvars.integer(b"g_userinfoValidate") as u32
    }

    /// `G_FilterPacket` for a connecting client's `ip` (`ClientConnect`'s first check).
    pub(super) fn filtered_out(&self, ip: &[u8]) -> bool {
        self.ip_filter
            .refuses(ip, self.cvars.integer(b"g_filterBan") != 0)
    }

    /// `G_ProcessIPBans`, as `G_InitGame` runs it at every level's start: `g_banIPs`
    /// read into the filter (on top of what it holds, as the reference's list is kept
    /// across a `map_restart`).
    pub(super) fn process_ip_bans(&mut self) {
        let ban_ips = self.cvars.string(b"g_banIPs").to_vec();
        let now = self.ip_filter.process(&ban_ips, &mut |message| {
            let _ = std::io::Write::write_all(&mut std::io::stdout(), message);
        });
        if now != ban_ips {
            self.set_cvar("g_banIPs", &String::from_utf8_lossy(&now));
        }
    }
}

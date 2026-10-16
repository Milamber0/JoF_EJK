//! The server's console variables at work: what a changed value does, checked after
//! every console line and every frame, as `G_UpdateCvars` and `SV_Frame`'s serverinfo
//! check do it.

use super::{NativeGame, Told};
use crate::cvars::{CVAR_INTERNAL, CVAR_SERVERINFO, Cvars};
use sjk_network::{LegacyOobRates, LegacySessionSettings};

impl NativeGame {
    /// The console variables, for the process to read what belongs to the endpoint
    /// (passwords, timeouts, rates, `sv_fps`).
    pub fn cvars(&self) -> &Cvars {
        &self.cvars
    }

    /// `duel_fraglimit`: the duels a player must win to end a tournament, 0 for none.
    pub fn set_duel_fraglimit(&mut self, duel_fraglimit: i32) {
        self.set_cvar("duel_fraglimit", &duel_fraglimit.to_string());
    }

    /// The limits that end a match — `fraglimit`, `timelimit`, `capturelimit` — set as
    /// the console variables they are.
    pub fn set_limits(&mut self, fraglimit: i32, timelimit: f32, capturelimit: i32) {
        self.set_cvar("fraglimit", &fraglimit.to_string());
        self.set_cvar("timelimit", &timelimit.to_string());
        self.set_cvar("capturelimit", &capturelimit.to_string());
    }

    /// `Cvar_Set`: the process's or the code's own change, past every protection, put
    /// into effect at once. How the command line's options reach the server.
    pub fn set_cvar(&mut self, name: &str, value: &str) {
        self.cvars.set(name.as_bytes(), value.as_bytes());
        self.apply_cvars();
    }

    /// Put every variable changed since the last look into effect: the game's own
    /// settings, the server-info string everyone is sent, and the announcement of a
    /// tracked variable (`print "Server: %s changed to %s\n"`).
    pub(super) fn apply_cvars(&mut self) {
        let mut seen = std::mem::take(&mut self.cvars_seen);
        let changed = self.cvars.changed_since(&mut seen);
        self.cvars_seen = seen;
        if !changed.is_empty() {
            self.cvars_generation += 1;
        }
        for name in changed {
            let Some(var) = self.cvars.var(&name) else {
                continue;
            };
            let (value, flags, announced, integer) =
                (var.string.clone(), var.flags, var.announced, var.integer());
            let text = String::from_utf8_lossy(&value).into_owned();
            let key = String::from_utf8_lossy(&name).into_owned();
            match key.to_ascii_lowercase().as_str() {
                "sv_hostname" => self.identity.hostname = value.clone(),
                "fraglimit" => self.limits.fraglimit = integer,
                "timelimit" => self.limits.timelimit = var.value(),
                "capturelimit" => self.limits.capturelimit = integer,
                "duel_fraglimit" => self.limits.duel_fraglimit = integer,
                "g_allowvote" => self.vote_rules.0 = integer & super::bridge_votes::SUPPORTED_VOTES,
                "g_votedelay" => self.vote_rules.1 = integer,
                "g_teamautojoin" => self.settings.team_auto_join = integer != 0,
                "sv_cheats" => self.settings.cheats = integer != 0,
                "g_gametype" if integer != self.gametype => self.set_gametype(integer),
                _ => {}
            }
            // `SV_SetConfigstring(CS_SERVERINFO, Cvar_InfoString(CVAR_SERVERINFO))`, one
            // key at a time into the string this server keeps in its own order; the
            // registration's own values are the level's first string, told to nobody.
            if flags & CVAR_SERVERINFO != 0 && flags & CVAR_INTERNAL == 0 {
                self.set_server_info(&key, &text, self.cvars_primed);
            }
            // The first look is the registration's own, which announces nothing.
            if announced && self.cvars_primed {
                self.told.push(Told::Everyone(
                    format!("print \"Server: {key} changed to {text}\n\"").into_bytes(),
                ));
            }
        }
        self.cvars_primed = true;
    }

    /// `Cvar_Get` of a latched variable when a level is built (`SV_Map_f`'s
    /// `g_gametype`): a change waiting for the next map takes effect.
    pub(super) fn take_latched_cvars(&mut self) {
        self.cvars.get(
            b"g_gametype",
            b"0",
            CVAR_SERVERINFO | crate::cvars::CVAR_LATCH,
            None,
        );
        self.apply_cvars();
    }

    /// How many times the variables have changed; the process compares it to know when
    /// to read [`Self::endpoint_settings`] again.
    pub fn cvars_generation(&self) -> u64 {
        self.cvars_generation
    }

    /// The endpoint's share of the variables: the remote console's password, the
    /// private slots, the timeouts, flood protection, downloads, automatic demos and the
    /// rate and snapshot limits.
    pub fn endpoint_settings(&self, settings: &mut LegacySessionSettings) {
        let cvars = &self.cvars;
        settings.console.rcon_password = cvars.string(b"rconPassword").to_vec();
        // `status` says whether this is a LAN or a public server.
        settings.console.dedicated = cvars.integer(b"dedicated");
        settings.private_clients = cvars.integer(b"sv_privateClients");
        settings.private_password = cvars.string(b"sv_privatePassword").to_vec();
        settings.timeout_seconds = cvars.integer(b"sv_timeout");
        settings.zombie_seconds = cvars.integer(b"sv_zombietime");
        settings.reconnect_limit_seconds = cvars.integer(b"sv_reconnectlimit");
        settings.flood_protect = cvars.integer(b"sv_floodProtect");
        settings.legacy_fixes = cvars.integer(b"sv_legacyFixes") != 0;
        settings.allow_download = cvars.integer(b"sv_allowDownload") != 0;
        settings.pure = cvars.integer(b"sv_pure") != 0;
        settings.auto_whitelist = cvars.integer(b"sv_autoWhitelist") != 0;
        settings.auto_demo = sjk_network::LegacyAutoDemoSettings {
            enabled: cvars.integer(b"sv_autoDemo") != 0,
            max_maps: cvars.integer(b"sv_autoDemoMaxMaps"),
        };
        settings.oob_rates = LegacyOobRates {
            per_address: cvars.integer(b"sv_maxOOBRateIP"),
            global: cvars.integer(b"sv_maxOOBRate"),
        };
        let rates = &mut settings.rates;
        rates.lan_force_rate = cvars.integer(b"sv_lanForceRate") != 0;
        rates.rate_policy = cvars.integer(b"sv_ratePolicy");
        rates.client_rate = cvars.integer(b"sv_clientRate");
        rates.min_rate = cvars.integer(b"sv_minRate");
        rates.max_rate = cvars.integer(b"sv_maxRate");
        rates.snaps_policy = cvars.integer(b"sv_snapsPolicy");
        rates.snaps_min = cvars.integer(b"sv_snapsMin");
        rates.snaps_max = cvars.integer(b"sv_snapsMax");
        // Simulation/snapshot time now advances in complete frames, so LAN peers
        // can use the actual server cadence without skipping an early wall-clock frame.
        rates.fps = cvars.integer(b"sv_fps").max(1);
    }

    /// `SV_Frame`'s frame length: a thousand milliseconds over `sv_fps`, never below
    /// one; an `sv_fps` below 1 is set back to 10 first, as the reference repairs it.
    pub fn frame_msec(&mut self) -> i32 {
        if self.cvars.integer(b"sv_fps") < 1 {
            self.set_cvar("sv_fps", "10");
        }
        (1000 / self.cvars.integer(b"sv_fps")).max(1)
    }

    /// One `CVAR_SERVERINFO` value rewritten in `CS_SERVERINFO` and the status reply;
    /// `tell` when clients already hold the old one.
    pub(super) fn set_server_info(&mut self, key: &str, value: &str, tell: bool) {
        self.status_info = server_info_with(&self.status_info, key, value);
        let info = self.status_info.clone();
        if tell {
            self.publish_config_string(0, &info);
        } else if let Some(slot) = self
            .config_strings
            .iter_mut()
            .find(|(index, _)| *index == 0)
        {
            slot.1 = info;
        }
    }

    /// A console line for the cvar table: a cvar command or a variable by name.
    pub(super) fn cvar_line(&mut self, line: &[u8], print: &mut dyn FnMut(&[u8])) -> bool {
        let taken = self.cvars.command(line, print);
        if taken {
            self.apply_cvars();
        }
        taken
    }
}

/// `CS_SERVERINFO` with one key's value replaced — `mapname` on a map change, a limit
/// when a vote sets one — and the rest of that string, the engine's and the game's own,
/// left alone. A key the string does not hold yet is added at its end, if the string
/// stays within `MAX_INFO_STRING`.
pub(super) fn server_info_with(server_info: &[u8], key: &str, value: &str) -> Vec<u8> {
    let fields: Vec<&[u8]> = server_info.split(|byte| *byte == b'\\').collect();
    let mut rebuilt: Vec<u8> = Vec::with_capacity(server_info.len() + value.len());
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            rebuilt.push(b'\\');
        }
        if index >= 2 && index % 2 == 0 && fields[index - 1] == key.as_bytes() {
            rebuilt.extend_from_slice(value.as_bytes());
        } else {
            rebuilt.extend_from_slice(field);
        }
    }
    // `Info_SetValueForKey` refuses a pair that would reach `MAX_INFO_STRING`.
    let held = fields
        .iter()
        .skip(1)
        .step_by(2)
        .any(|known| *known == key.as_bytes());
    if !held && !value.is_empty() && rebuilt.len() + key.len() + value.len() + 2 < 1024 {
        rebuilt.extend_from_slice(format!("\\{key}\\{value}").as_bytes());
    }
    rebuilt
}

/// The `CVAR_SERVERINFO` variables a legacy cgame reads from `CS_SERVERINFO`, in the
/// order this server has always sent them; their values are the variables' own, filled
/// in when the table is first read.
pub(super) const SERVERINFO_DEFAULTS: [(&str, &str); 20] = [
    ("g_gametype", "0"),
    ("sv_fps", "20"),
    ("timelimit", "0"),
    ("fraglimit", "20"),
    ("capturelimit", "8"),
    ("duel_fraglimit", "10"),
    ("dmflags", "0"),
    ("g_needpass", "0"),
    ("g_jediVmerc", "0"),
    ("g_forcePowerDisable", "0"),
    ("g_weaponDisable", "0"),
    ("g_stepSlideFix", "1"),
    ("g_noSpecMove", "0"),
    ("g_debugMelee", "0"),
    ("g_showDuelHealths", "0"),
    ("g_siegeTeam1", "none"),
    ("g_siegeTeam2", "none"),
    ("g_siegeTeamSwitch", "1"),
    ("g_redTeam", "Empire"),
    ("g_blueTeam", "Rebellion"),
];

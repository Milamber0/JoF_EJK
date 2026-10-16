//! Chat on this server: `say`, `say_team`, `tell` and `gc` (`sjk_game_jka::chat`), with
//! the location a teammate's message names found in the map's own PVS.

use super::{NativeGame, Told};
use crate::visibility::Eye;
use sjk_game_jka::chat::{self, ChatSettings};
use sjk_network::LegacyTokens;

impl NativeGame {
    /// A chat command from `client`; `false` for any other command.
    pub(super) fn chat_command(&mut self, client: usize, text: &[u8], server_time: i32) -> bool {
        let arguments: Vec<&[u8]> = LegacyTokens::new(text).collect();
        let Some(command) = arguments.first() else {
            return false;
        };
        if !["say", "say_team", "tell", "gc"]
            .iter()
            .any(|name| command.eq_ignore_ascii_case(name.as_bytes()))
        {
            return false;
        }
        // `gc` is `CMD_NOINTERMISSION` (`g_cmds.c:3406`); the three chat commands are not.
        if command.eq_ignore_ascii_case(b"gc") && self.refused_at_intermission(client, command) {
            return true;
        }
        let location = self.location_of(client);
        let settings = ChatSettings {
            gametype: self.gametype,
            now: server_time,
        };
        let views = self.client_views();
        let Some(spoken) =
            chat::chat_command(&settings, &views, client, &arguments, location.as_deref())
        else {
            return false;
        };
        for line in spoken.log {
            self.log(&format!("{}\n", String::from_utf8_lossy(&line)));
        }
        for line in spoken.console {
            println!("{}", String::from_utf8_lossy(&line));
        }
        for (to, text) in spoken.told {
            self.told.push(Told::One(to, text));
        }
        true
    }

    /// `Team_GetLocationMsg` for a client: the nearest of the map's locations its
    /// position can see, measured and seen from `r.currentOrigin` itself. Only a team game
    /// ever names one, so a free-for-all looks nothing up.
    fn location_of(&self, client: usize) -> Option<Vec<u8>> {
        if self.gametype < sjk_game_jka::match_end::GT_TEAM {
            return None;
        }
        let map = self.map.as_ref()?;
        let origin = self.peer(client)?.state.origin();
        let eye = Eye::new(&map.bsp, &map.areas, origin);
        chat::location_message(&map.locations, origin, |point| {
            eye.sees_point(&map.bsp, point)
        })
    }

    /// `Team_GetLocation`'s `cs_index` for a client in a team game, for its team overlay:
    /// 0 for none.
    pub(super) fn location_number(&self, client: usize) -> u32 {
        if self.gametype < sjk_game_jka::match_end::GT_TEAM {
            return 0;
        }
        let (Some(map), Some(peer)) = (self.map.as_ref(), self.peer(client)) else {
            return 0;
        };
        let origin = peer.state.origin();
        let eye = Eye::new(&map.bsp, &map.areas, origin);
        chat::location_number(chat::nearest_location(&map.locations, origin, |point| {
            eye.sees_point(&map.bsp, point)
        }))
    }
}

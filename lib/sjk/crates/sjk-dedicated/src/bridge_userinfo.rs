//! `ClientUserinfoChanged` for a userinfo a client sends while connected
//! (`g_client.c:2100-2372`): the userinfo judged again, the player's string published,
//! the maximum health taken from the handicap — and a new name announced and logged, or
//! refused within five seconds of the last.

use super::{
    AcceptedUserinfo, ClientSession, GAMETYPE_DUEL, GAMETYPE_POWERDUEL, GAMETYPE_SIEGE, NativeGame,
    PlayerSession, STAT_MAX_HEALTH, Told, accept_userinfo, bridge_sabers, validate_userinfo,
};
use sjk_game_jka::game_log;

/// `pers.netnameTime`'s wait between two names.
const RENAME_INTERVAL: i32 = 5_000;

/// `ClientUserinfoChanged` on `team`: the reason a userinfo is refused, or what the game
/// keeps of it. A team game names the player's leadership (`tl`); a siege game its class
/// and the side it wants (`siegeclass`, `sdt`), and a `class` it plays forces its model
/// and replaces the handicap with its own maximum health.
pub(super) fn judge(
    userinfo: &[u8],
    rules: u32,
    session: &PlayerSession,
    gametype: i32,
    sabers: &sjk_game_jka::saber_definition::SaberParms,
    class: Option<&sjk_game_jka::siege_class::SiegeClass>,
) -> Result<AcceptedUserinfo, String> {
    validate_userinfo(userinfo, rules)?;
    // A duel's scoreboard shows every client's wins and losses (`w`, `l`).
    let duel_record = matches!(gametype, GAMETYPE_DUEL | GAMETYPE_POWERDUEL)
        .then_some((session.wins, session.losses));
    let duel_team = (gametype == GAMETYPE_POWERDUEL).then_some(session.duel_team);
    // Team games: `tl`, whether it leads its side.
    let team_leader = (gametype >= super::GAMETYPE_TEAM).then_some(i32::from(session.team_leader));
    let siege = (gametype == GAMETYPE_SIEGE)
        .then_some((session.siege_class.as_bytes(), session.siege_desired_team));
    let forced_model = class
        .and_then(|class| class.model.as_deref())
        .map(str::as_bytes);
    let class_max_health = class.map(|class| {
        if class.max_health != 0 {
            class.max_health
        } else {
            100
        }
    });
    let session = ClientSession {
        team: session.team,
        duel_record,
        duel_team,
        team_leader,
        siege,
        forced_model,
        class_max_health,
        ..Default::default()
    };
    Ok(accept_userinfo(userinfo, session, sabers, true))
}

impl NativeGame {
    /// `ClientUserinfoChanged`'s advice (`g_client.c:2316-2318`): a client asking for
    /// fewer snapshots than the server makes frames is told to ask for more.
    pub(super) fn snaps_advice(&self, userinfo: &[u8]) -> Option<Vec<u8>> {
        let fps = self.cvars.integer(b"sv_fps");
        (sjk_game_jka::userinfo::atoi(sjk_protocol::info_value(userinfo, b"snaps").unwrap_or_default()) < fps)
            .then(|| format!("print \"^3Recommend setting /snaps {fps} or higher to match this server's sv_fps\n\"").into_bytes())
    }

    pub(super) fn userinfo_changed(&mut self, client: usize, userinfo: &[u8]) {
        let (parms, gametype, rules, level_time) = (
            self.saber_parms(),
            self.gametype,
            self.userinfo_rules(),
            self.last_frame_time,
        );
        let class = self.siege_class_of(client);
        let log_client_info = self.cvars.integer(b"g_logClientInfo") != 0;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let mut userinfo = userinfo.to_vec();
        let mut judged = judge(
            &bridge_sabers::with_saber_names(&userinfo, &peer.sabers),
            rules,
            &peer.session,
            gametype,
            &parms,
            class.as_ref(),
        );
        // A connected player's new name: refused within five seconds of the last one (the
        // old name written back into its userinfo), otherwise announced.
        let mut renamed = None;
        if let Ok(accepted) = &judged
            && peer.begun
            && accepted.name != peer.name
        {
            if peer.netname_time > level_time {
                userinfo = sjk_protocol::info_set_value(&userinfo, b"name", &peer.name);
                judged = judge(
                    &bridge_sabers::with_saber_names(&userinfo, &peer.sabers),
                    rules,
                    &peer.session,
                    gametype,
                    &parms,
                    class.as_ref(),
                );
                self.told
                    .push(Told::One(client, b"print \"@@@NONAMECHANGE\n\"".to_vec()));
            } else {
                peer.netname_time = level_time + RENAME_INTERVAL;
                renamed = Some((peer.name.clone(), accepted.name.clone()));
            }
        }
        let accepted = match judged {
            Ok(accepted) => accepted,
            // `ClientUserinfoChanged` drops the client, with the reason.
            Err(reason) => {
                self.told.push(Told::Drop {
                    client,
                    reason: format!("Failed userinfo validation: {reason}").into_bytes(),
                });
                return;
            }
        };
        let advice = self.snaps_advice(&userinfo);
        self.told.extend(advice.map(|text| Told::One(client, text)));
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let previous = std::mem::replace(&mut peer.client_info, accepted.client_info.clone());
        peer.name = accepted.name.clone();
        peer.userinfo = userinfo;
        // `ps.stats[STAT_MAX_HEALTH] = pers.maxHealth`: the handicap again (a siege class's
        // own maximum, which its spawn set, is the same here).
        peer.state.stats[STAT_MAX_HEALTH] = accepted.max_health as u32;
        let changed = previous != accepted.client_info;
        let client_info = accepted.client_info.clone();
        peer.accepted = accepted;
        if let Some((old, new)) = renamed {
            let print = [&b"print \""[..], &old, b"^7 @@@PLRENAME ", &new, b"\n\""].concat();
            self.told.push(Told::Everyone(print));
            self.log_about(client, |who| {
                game_log::client_rename(game_log::Who { name: &old, ..who }, &new)
            });
        }
        self.told.push(Told::PlayerString {
            client,
            previous,
            value: client_info.clone(),
        });
        if log_client_info {
            self.log(&game_log::userinfo_changed(
                client,
                changed.then_some(&client_info),
            ));
        }
    }
}

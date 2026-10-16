//! The match's end as players see it: the prints before `LogExit`, and
//! `BeginIntermission`.

use super::*;

impl NativeGame {
    /// `ClientIntermissionThink` (`g_active.c:831`): a player at the scoreboard does
    /// nothing but ask to leave, and having asked, stays asking.
    pub(super) fn intermission_think(
        &mut self,
        client: usize,
        command: &UserCommand,
        server_time: i32,
    ) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        // `ClientThink` marks the time and keeps the command before the intermission
        // branch in `ClientThink_real` is ever reached (`g_active.c:3553-3566`), so a
        // frozen player is still a player whose last command the server knows — which is
        // the one `ClientSpawn` thinks with when the level starts again.
        peer.last_command_time = server_time;
        peer.last_command = *command;
        let buttons = command.buttons as i32;
        let old = std::mem::replace(&mut peer.intermission_buttons, buttons);
        if sjk_game_jka::match_end::asked_to_leave(old, buttons) {
            peer.ready_to_exit = true;
        }
    }

    /// The prints `CheckExitRules` sends before `LogExit`, in its own order.
    pub(super) fn end_prints(&self, announce: sjk_game_jka::match_end::Announce) -> Vec<Vec<u8>> {
        use sjk_game_jka::match_end::{Announce, TEAM_RED};
        match announce {
            Announce::Silent => Vec::new(),
            Announce::TimeLimit => vec![b"print \"@@@TIMELIMIT_HIT.\n\"".to_vec()],
            Announce::TeamKillLimit(team) => {
                let name = if team == TEAM_RED { "Red" } else { "Blue" };
                vec![format!("print \"{name} @@@HIT_THE_KILL_LIMIT.\n\"").into_bytes()]
            }
            Announce::ClientKillLimit(client) => {
                let name = self
                    .peer(client)
                    .map_or_else(Vec::new, |peer| peer.name.clone());
                vec![
                    [
                        b"print \"".as_slice(),
                        &name,
                        b"^7 @@@HIT_THE_KILL_LIMIT.\n\"",
                    ]
                    .concat(),
                ]
            }
            // Two prints, as the reference sends them: the team's name, then the limit.
            Announce::CaptureLimit(team) => vec![
                format!(
                    "print \"@@@{}TEAM \"",
                    if team == TEAM_RED {
                        "PRINTRED"
                    } else {
                        "PRINTBLUE"
                    }
                )
                .into_bytes(),
                b"print \"@@@HIT_CAPTURE_LIMIT.\n\"".to_vec(),
            ],
        }
    }

    /// `BeginIntermission` (`g_main.c:1338`): everyone is moved to the intermission point
    /// and stops being drawn, and every scoreboard is sent.
    ///
    /// The reference respawns a dead player first, so that a corpse is not what stands
    /// there. This server does the same by putting the player back in the world before
    /// moving it, which is what `ClientRespawn` amounts to for a client about to be
    /// frozen.
    pub(super) fn begin_intermission(&mut self, server_time: i32) {
        // A duel's round is counted first (`BeginIntermission`, `g_main.c:1347-1361`).
        self.duel_intermission_begins();
        // "respawn if dead" — but a power duel's spectators, "or it will mess the line order
        // all up".
        for client in 0..self.players.places() {
            let Some(peer) = self
                .peer(client)
                .filter(|peer| peer.begun && peer.health <= 0)
            else {
                continue;
            };
            if self.gametype == GAMETYPE_POWERDUEL && !peer.playing() {
                continue;
            }
            let command = peer.last_command;
            self.respawn(client, command, server_time, server_time);
        }
        let (origin, angles) = self.intermission_point();
        println!("intermission at {server_time} from {origin:?} looking {angles:?}");
        for client in 0..self.players.places() {
            let Some(peer) = self.peer_mut(client) else {
                continue;
            };
            if !peer.begun {
                continue;
            }
            sjk_game_jka::match_end::move_client_to_intermission(&mut peer.state, origin, angles);
            peer.movement = peer.movement.reseeded(&peer.state);
            // `MoveClientToIntermission`'s entity half: nothing of the player is drawn,
            // sounded or solid while the scoreboard is up.
            let state = peer.entity.state_mut();
            state.set_raw_field(ES_ENTITY_TYPE, 0);
            state.set_raw_field(ES_MODELINDEX, 0);
            state.set_raw_field(ES_EFLAGS, 0);
            state.set_raw_field(ES_EVENT, 0);
        }
        // `G_LeaveVehicle(ent, qfalse)` after the move (`g_main.c:1256`).
        for client in 0..self.players.places() {
            if self.peer(client).is_some_and(|peer| peer.begun) {
                self.leave_vehicle(client, server_time, false);
            }
        }
        let message = self.scoreboard(server_time);
        for client in 0..self.players.places() {
            if self.peer(client).is_some_and(|peer| peer.begun) {
                self.told.push(Told::One(client, message.clone()));
            }
        }
    }

    /// `FindIntermissionPoint` (`g_main.c:1283`): a siege round that has ended looks from
    /// the winning side's own camera, anything else from the map's plain one — and a map
    /// with neither puts the camera where a spectator would start.
    ///
    /// That last branch is not a corner case. Measured in
    /// `the_intermission_cameras_of_the_retail_maps_are_where_the_maps_put_them`: the
    /// three stock siege maps place **no** intermission entity of any kind, so every
    /// siege intermission in retail JKA is the spectator-spawn fallback. A server that
    /// only looked for the camera would stand every player at the world origin.
    fn intermission_point(&mut self) -> ([f32; 3], [f32; 3]) {
        // `gSiegeRoundWinningTeam`, only while a round has ended.
        let winner = self
            .siege
            .as_ref()
            .filter(|siege| siege.round.ended)
            .map(|siege| siege.round.winner as u8);
        let wanted = winner.and_then(|winner| match winner {
            sjk_game_jka::siege_class::SIEGETEAM_TEAM1 => Some(sjk_game_jka::map::Team::Red),
            sjk_game_jka::siege_class::SIEGETEAM_TEAM2 => Some(sjk_game_jka::map::Team::Blue),
            _ => None,
        });
        let Some(map) = self.map.as_ref() else {
            return ([0.0; 3], [0.0; 3]);
        };
        let points = &map.spawn_points;
        let camera = wanted
            .and_then(|team| {
                points
                    .iter()
                    .find(|point| point.kind == SpawnKind::Intermission(Some(team)))
            })
            .or_else(|| {
                points
                    .iter()
                    .find(|point| point.kind == SpawnKind::Intermission(None))
            })
            .or_else(|| {
                points
                    .iter()
                    .find(|point| matches!(point.kind, SpawnKind::Intermission(Some(_))))
            });
        if let Some(point) = camera {
            return (point.origin, point.angles);
        }
        // `SelectSpawnPoint(vec3_origin, ..., TEAM_SPECTATOR, qfalse)`: a free point away
        // from the world origin, drawn with the game's own generator.
        select_spawn_point(&mut self.deaths.rng, points, |_| false, [0.0; 3])
            .unwrap_or(([0.0; 3], [0.0; 3]))
    }
}

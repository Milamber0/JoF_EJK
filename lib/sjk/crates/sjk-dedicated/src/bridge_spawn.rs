//! Where a player spawns: `SelectSpawnPoint`, the duels' own points, a flag game's team
//! points, and a spectator's start.

use super::*;

/// Where a player spawns (`SelectSpawnPoint`, `SelectSpectatorSpawnPoint`) and when. A
/// player joining the game is placed by `select_spawn_point` — one of the further half of
/// the deathmatch points nobody stands on (`SpotWouldTelefrag`: any linked client's box
/// on the point), drawn with the game's generator, away from `avoid` — or, with every
/// point taken, on the first, whoever stands there. A spectator starts at the map's
/// intermission point; a map without one puts it where a player avoiding the origin
/// would spawn (`FindIntermissionPoint`).
pub(super) fn spawn_place(
    order: SpawnOrder,
    gametype: i32,
    rng: &mut Rng,
    map: Option<&LoadedMap>,
    obstacles: &[BoxObstacle],
    playing: bool,
    avoid: [f32; 3],
    command: &UserCommand,
    level_time: i32,
    team_spot: Option<(i32, bool, &mut CrtRand)>,
    duel_team: i32,
) -> SpawnPlace {
    let points = map.map_or(&[][..], |map| &map.spawn_points);
    let occupied = |point: &sjk_game_jka::map::SpawnPoint| {
        obstacles.iter().any(|other| {
            (0..3).all(|axis| {
                point.origin[axis] + PLAYER_BOX.0[axis]
                    <= other.origin[axis] + other.bounds.1[axis] + 1.0
                    && point.origin[axis] + PLAYER_BOX.1[axis]
                        >= other.origin[axis] + other.bounds.0[axis] - 1.0
            })
        })
    };
    let intermission = points
        .iter()
        .find(|point| point.kind == SpawnKind::Intermission(None));
    // Siege maps place `info_player_siegeteam1`/`2` and (almost) no deathmatch points, so
    // a siege server puts a player on a team's own; anything else uses the free points.
    // A siege map that somehow has neither falls back to whatever it does have.
    let siege = gametype == GAMETYPE_SIEGE
        && points
            .iter()
            .any(|point| matches!(point.kind, SpawnKind::Siege(_)));
    let deathmatch = move || {
        points.iter().filter(move |point| {
            if siege {
                matches!(point.kind, SpawnKind::Siege(_))
            } else {
                point.kind == SpawnKind::Deathmatch
            }
        })
    };
    // `SelectCTFSpawnPoint`: a team's own points in a flag game, the first spawn on its
    // `team_CTF_*player`s and every later one on its `team_CTF_*spawn`s.
    if playing
        && bridge_ctf::flag_game(gametype)
        && let Some((team, begin, crt)) = team_spot
        && let Some(place) = team_spawn_point(points, team, begin, &occupied, crt)
    {
        return SpawnPlace {
            origin: place.0,
            angles: place.1,
            level_time,
            command_angles: command.angles,
        };
    }
    let (origin, angles) = match (playing, intermission, order) {
        (false, Some(point), _) => (point.origin, point.angles),
        (true, _, SpawnOrder::Map | SpawnOrder::MapFrom(_)) => {
            let first = match order {
                SpawnOrder::MapFrom(index) => index,
                _ => 0,
            };
            let count = deathmatch().count();
            let start = if count == 0 { 0 } else { first % count };
            let ordered = || deathmatch().skip(start).chain(deathmatch().take(start));
            let point = ordered()
                .find(|point| !occupied(point))
                .or_else(|| ordered().next());
            point.map_or(([0.0; 3], [0.0; 3]), |point| {
                (
                    [point.origin[0], point.origin[1], point.origin[2] + 9.0],
                    point.angles,
                )
            })
        }
        // A duel's players start on the duel points (`SelectDuelSpawnPoint`).
        (true, _, _) if gametype == GAMETYPE_DUEL || gametype == GAMETYPE_POWERDUEL => {
            // A power duel's lone and pair on their own points; a duel's on its.
            let kind = match (gametype, duel_team) {
                (GAMETYPE_DUEL, _) => SpawnKind::Duel(0),
                (_, sjk_game_jka::power_duel::DUELTEAM_LONE) => SpawnKind::Duel(1),
                (_, sjk_game_jka::power_duel::DUELTEAM_DOUBLE) => SpawnKind::Duel(2),
                _ => SpawnKind::Deathmatch,
            };
            sjk_game_jka::map::select_duel_spawn_point(rng, points, kind, occupied, avoid)
                .unwrap_or(([0.0; 3], [0.0; 3]))
        }
        (playing, _, _) => select_spawn_point(
            rng,
            points,
            occupied,
            if playing { avoid } else { [0.0; 3] },
        )
        .unwrap_or(([0.0; 3], [0.0; 3])),
    };
    SpawnPlace {
        origin,
        angles,
        level_time,
        command_angles: command.angles,
    }
}

/// `SelectRandomTeamSpawnPoint` for a flag game (`g_team.c:1032-1128`): `team`'s points of
/// the kind, in map order; a random one (`rand()`) of those nobody stands on, or the first
/// of them all when every one is taken — raised nine units. `None` where the map has none.
pub(super) fn team_spawn_point(
    points: &[sjk_game_jka::map::SpawnPoint],
    team: i32,
    begin: bool,
    occupied: &dyn Fn(&sjk_game_jka::map::SpawnPoint) -> bool,
    crt: &mut CrtRand,
) -> Option<([f32; 3], [f32; 3])> {
    use sjk_game_jka::map::Team;
    let side = match team {
        1 => Team::Red,
        2 => Team::Blue,
        _ => return None,
    };
    let kind = SpawnKind::Team {
        team: side,
        initial: begin,
    };
    let mut spots = points.iter().filter(|point| point.kind == kind);
    let first = spots.clone().next()?;
    // `MAX_TEAM_SPAWN_POINTS`.
    let free: Vec<&sjk_game_jka::map::SpawnPoint> = spots
        .by_ref()
        .filter(|point| !occupied(point))
        .take(32)
        .collect();
    let spot = if free.is_empty() {
        first
    } else {
        free[(crt.next() as usize) % free.len()]
    };
    Some((
        [spot.origin[0], spot.origin[1], spot.origin[2] + 9.0],
        spot.angles,
    ))
}

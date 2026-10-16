//! World shots of the team outlines and logos ([`crate::hud::team_outline`],
//! [`crate::hud::team_logo`]) on a recorded game: the demo `SJK_TEST_DEMO` names is
//! played without a server, with a Capture the Flag game staged over it (the recorder
//! and every other player after them alternately red, then blue), so teammates show
//! green rings and their team's logo and enemies red rings. Ignored like the other
//! world shots; skipped when `SJK_TEST_DEMO` is not set.

use super::card_outline_shots::open_demo;
use super::*;
use crate::hud::team_outline::ShotTeams;

/// `GT_CTF`.
const CTF: i32 = 8;

#[test]
#[ignore = "renders with the GPU, the installed game data and the demo SJK_TEST_DEMO names"]
fn team_outlines_on_a_demo() {
    on_big_stack(|| {
        let Some(demo) = std::env::var_os("SJK_TEST_DEMO").map(PathBuf::from) else {
            println!("SJK_TEST_DEMO is not set: skipped");
            return;
        };
        let probe = demo_playback::Session::open(&demo, demo_playback::Camera::FirstPerson, 60)
            .expect("the demo");
        let own = probe.latest_snapshot().player.client_num();
        let players: Vec<u16> = probe
            .latest_snapshot()
            .entities
            .iter()
            .map(|entity| entity.number())
            .filter(|number| *number < 32 && *number != own)
            .collect();
        let shot = staged_ctf(probe.game_state(), &players);
        let (friends, enemies): (Vec<u16>, Vec<u16>) = players
            .iter()
            .partition(|slot| team_of(&players, **slot) == 1);
        println!("recorder {own} (red), red {friends:?}, blue {enemies:?}");
        let (Some(&friend), Some(&enemy)) = (friends.first(), enemies.first()) else {
            println!("too few players in the demo's first snapshot: skipped");
            return;
        };
        let cameras = [
            (
                "team_outline_friend",
                demo_playback::Camera::Orbit(friend),
                None,
            ),
            (
                "team_outline_enemy",
                demo_playback::Camera::Orbit(enemy),
                None,
            ),
            // The card's ring wins over a team's on its player.
            (
                "team_outline_enemy_card",
                demo_playback::Camera::Orbit(enemy),
                Some(enemy),
            ),
        ];
        for (name, camera, card) in cameras {
            let Some(mut gpu) = open_demo(&demo, camera, [1600, 900]) else {
                return;
            };
            gpu.hud.team_outline.stage_for_shot(shot.clone());
            gpu.hud.team_logo.stage_for_shot(shot.clone());
            let _ = frame(&mut gpu, 20);
            if let Some(client) = card {
                gpu.hud.card.pin_for_shot(client);
            }
            println!("{}", shoot(&mut gpu, 40, name).display());
        }
    });
}

/// 1 (red) or 2 (blue) for the player in `slot`: alternately, the first red.
fn team_of(players: &[u16], slot: u16) -> i32 {
    let index = players.iter().position(|other| *other == slot).unwrap_or(0);
    if index % 2 == 0 { 1 } else { 2 }
}

/// `game` as a CTF game: the game type in `CS_SERVERINFO`, every player's team in
/// their `CS_PLAYERS` string, the viewer on red.
fn staged_ctf(game: &sjk_protocol::GameState, players: &[u16]) -> ShotTeams {
    let mut game = game.clone();
    let info = game.config_string(0).unwrap_or_default().to_vec();
    game.replace_config_string(0, with_key(&info, "g_gametype", &CTF.to_string()))
        .expect("server info");
    for slot in players {
        let index = 1_131 + usize::from(*slot);
        let info = game.config_string(index).unwrap_or_default().to_vec();
        let team = team_of(players, *slot).to_string();
        game.replace_config_string(index, with_key(&info, "t", &team))
            .expect("player info");
    }
    ShotTeams {
        game,
        local_team: 1,
    }
}

/// The info string `info` with `key` set to `value`.
fn with_key(info: &[u8], key: &str, value: &str) -> Vec<u8> {
    let body = info.strip_prefix(b"\\").unwrap_or(info);
    let mut out = Vec::with_capacity(info.len() + key.len() + value.len() + 2);
    let mut fields = body.split(|byte| *byte == b'\\');
    while let (Some(name), Some(old)) = (fields.next(), fields.next()) {
        if name != key.as_bytes() && !name.is_empty() {
            out.push(b'\\');
            out.extend_from_slice(name);
            out.push(b'\\');
            out.extend_from_slice(old);
        }
    }
    out.push(b'\\');
    out.extend_from_slice(key.as_bytes());
    out.push(b'\\');
    out.extend_from_slice(value.as_bytes());
    out
}

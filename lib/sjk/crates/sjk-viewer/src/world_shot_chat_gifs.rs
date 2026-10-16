//! World shots of GIFs in SJK chat ([`crate::chat_gifs`]) over duel6: the SJK chat page
//! with a GIF shown, one loading and one that could not be had; the main page's dock and
//! the in-game menu's dock with a GIF; the in-play chat box with GIF lines, in play and
//! with the composer open over its history; the GIF picker on Seen in chat, on Search with
//! GIPHY's answer and on Search without a key. The GIFs are small test animations made here and
//! put in the cache as if fetched: nothing is downloaded. Off-screen renders (no game
//! window), ignored like the other world shots: they need a GPU adapter and the installed
//! game data named by `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer sjk_chat_gifs -- --ignored
//! --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;
use crate::chat_gifs::{GifId, decode};

/// The test GIFs: a wide one (GIPHY's 200 pixels high) and a square one.
fn gifs() -> (GifId, GifId) {
    let wide = GifId::new("ShotGifWide01").unwrap();
    let square = GifId::new("ShotGifSquare02").unwrap();
    crate::chat_gifs::insert_for_shot(&wide, &decode::test_animation([356, 200], 8, 80), 250);
    crate::chat_gifs::insert_for_shot(&square, &decode::test_animation([200, 200], 6, 90), 250);
    crate::chat_gifs::fail_for_shot(&GifId::new("ShotGifGone03").unwrap());
    (wide, square)
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_chat_gifs_page() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64);
    let lines: [(&str, &str, &str, bool); 5] = [
        (
            "9a0c51e2b7d34f80",
            "^5Creyon",
            "anyone up for duels on ffa3?",
            true,
        ),
        (
            "44f3d0b36c9b2510",
            "^1Sol^7Vulpes",
            "GIFs in chat now https://giphy.com/gifs/hello-wave-ShotGifWide01",
            true,
        ),
        (
            "0b1c2d3e4f5a6b7c",
            "Padawan^1Fox",
            "https://media.giphy.com/media/ShotGifGone03/giphy.gif",
            false,
        ),
        (
            "1f2e3d4c5b6a7980",
            "^3Lumaya",
            "https://i.giphy.com/ShotGifSquare02.gif gg",
            true,
        ),
        (
            "7c6b5a4f3e2d1c0b",
            "Kyle",
            "this one is still coming https://giphy.com/gifs/ShotGifLoading04",
            false,
        ),
    ];
    let messages = lines
        .iter()
        .enumerate()
        .map(
            |(index, (key, name, text, verified))| sjk_identity::ChatMessage {
                id: index as u64 + 1,
                at: now - (lines.len() - index) as i64 * 90,
                key_id: (*key).to_owned(),
                name: (*name).to_owned(),
                verified: *verified,
                staff: false,
                text: (*text).to_owned(),
                holocron: None,
            },
        )
        .collect();
    let state = sjk_identity::ChatState {
        messages,
        revision: 1,
        online: 5,
        people: None,
        live: true,
        outcome: None,
        loaded: Some(1),
    };
    for (size, scale, suffix) in [
        ([1920, 1080], None, "1080p"),
        ([3840, 2160], Some("1.2"), "4k-styled"),
    ] {
        let state = state.clone();
        on_big_stack(move || {
            let _ = gifs();
            let menu = menu::ClientMenu::new(true, String::new());
            let mut cvars = vec![
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
            ];
            if let Some(scale) = scale {
                cvars.push((crate::text::style::SCALE_CVAR, scale));
            }
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let Some(console) = gpu.console.as_mut() {
                console.preview_sjk_chat(state, None);
            }
            let name = format!("duel6-sjk-chat-gifs-{suffix}");
            println!("{}", shoot(&mut gpu, 8, &name).display());
        });
    }
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_chat_gifs_dock() {
    use crate::ingame_menu::sjk_view::Card;
    use crate::ingame_menu::{Page, ShotView};
    const LINES: [(&str, &str, bool); 4] = [
        ("^1Fox", "in 5 min, finishing a CTF", false),
        ("^3Lumaya", "gg", true),
        ("Kyle", "anyone up for duels?", false),
        (
            "^2Sol",
            "hello https://giphy.com/gifs/hello-wave-ShotGifWide01",
            true,
        ),
    ];
    for (size, suffix) in [([1920, 1080], "1080p"), ([3840, 2160], "4k")] {
        on_big_stack(move || {
            let _ = gifs();
            let menu = menu::ClientMenu::new(true, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
                ("name", "^1Sol^7Vulpes"),
            ];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
            else {
                return;
            };
            gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
            let _ = frame(&mut gpu, 20);
            if let Some(menu) = gpu.client_menu.as_mut() {
                menu.sjk_home_chat_for_shot(&LINES, 12, None);
            }
            let name = format!("duel6-chat-dock-gif-{suffix}");
            println!("{}", shoot(&mut gpu, 8, &name).display());
            let menu = gpu.client_menu.as_ref().expect("the menu");
            assert!(!menu.sjk_home_overflowed(), "{name}");
        });
        on_big_stack(move || {
            let _ = gifs();
            let menu = menu::ClientMenu::new(false, String::new());
            let cvars = [
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
                ("name", "^1Sol^7Vulpes"),
            ];
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
            else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let shot = &shots[0];
            let (yaw, pitch) = look(shot.from, shot.at);
            aim(&mut gpu, shot.from, yaw, pitch);
            gpu.game_menu = true;
            if let Some(console) = gpu.console.as_mut() {
                console.close_for_connection();
            }
            let _ = frame(&mut gpu, 60);
            let ffa = ShotView {
                team: 0,
                team_game: false,
                red_players: 0,
                blue_players: 0,
                vote_active: false,
                staff: false,
                siege: false,
            };
            gpu.game_menu_page = Page::Main;
            gpu.game_menu_row = 0;
            gpu.in_game_menu
                .sjk_for_shot(Card::for_shot(false, false), ffa);
            gpu.in_game_menu.chat_for_shot(&LINES, 12, None);
            let name = format!("duel6-ingame-chat-gif-{suffix}");
            println!("{}", shoot(&mut gpu, 16, &name).display());
            assert!(!gpu.in_game_menu.overflowed(), "{name}");
        });
    }
}

/// The in-play chat box (`chat/feed_gif.rs`) over duel6, without a session: SJK lines
/// with a wide GIF and a square one among game lines, then the same with the SJK
/// composer open, at 1080p and 4K.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_chat_gifs_feed() {
    for (size, suffix) in [([1920, 1080], "1080p"), ([3840, 2160], "4k")] {
        on_big_stack(move || {
            let _ = gifs();
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &[]) else {
                return;
            };
            let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
            let shot = &shots[0];
            let (yaw, pitch) = look(shot.from, shot.at);
            aim(&mut gpu, shot.from, yaw, pitch);
            if let Some(console) = gpu.console.as_mut() {
                console.set_open(false);
            }
            let message =
                |id: u64, name: &str, text: &str, verified: bool| sjk_identity::ChatMessage {
                    id,
                    at: 0,
                    key_id: format!("{id:016x}"),
                    name: name.to_owned(),
                    verified,
                    staff: false,
                    text: text.to_owned(),
                    holocron: None,
                };
            let state = |messages: Vec<sjk_identity::ChatMessage>| sjk_identity::ChatState {
                messages: messages.into_iter().collect(),
                loaded: Some(1),
                ..sjk_identity::ChatState::default()
            };
            let now = std::time::Instant::now();
            let chat = &mut gpu.chat;
            chat.for_shot = true;
            chat.sync_sjk(&state(Vec::new()), |_| false, now);
            chat.receive(
                sjk_client::ServerEventKind::Chat,
                "^7Kyle^7: ^2anyone up for a duel?".to_owned(),
                None,
                now,
            );
            let first = vec![message(
                1,
                "^1Sol^7Vulpes",
                "hello https://giphy.com/gifs/hello-wave-ShotGifWide01",
                true,
            )];
            chat.sync_sjk(&state(first.clone()), |_| false, now);
            chat.receive(
                sjk_client::ServerEventKind::Chat,
                "^7Fox^7: ^2gg".to_owned(),
                None,
                now,
            );
            let mut all = first;
            all.push(message(
                2,
                "^3Lumaya",
                "https://i.giphy.com/ShotGifSquare02.gif nice",
                false,
            ));
            chat.sync_sjk(&state(all), |_| false, now);
            let name = format!("duel6-chat-box-gifs-{suffix}");
            println!("{}", shoot(&mut gpu, 12, &name).display());
            gpu.chat.open_sjk();
            let name = format!("duel6-chat-box-gifs-typing-{suffix}");
            println!("{}", shoot(&mut gpu, 12, &name).display());
        });
    }
}

/// The picker's test tiles, of several shapes, as if their previews were fetched.
fn tiles() -> Vec<GifId> {
    let shapes = [[200, 112], [200, 200], [200, 150], [200, 260], [200, 90]];
    let ids: Vec<GifId> = (0..25)
        .map(|n| GifId::new(&format!("ShotPick{n:02}X")).unwrap())
        .collect();
    for (n, id) in ids.iter().enumerate() {
        let frames = 4 + n % 4;
        crate::chat_gifs::insert_preview_for_shot(
            id,
            &decode::test_animation(shapes[n % shapes.len()], frames, 90),
            250,
        );
    }
    ids
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_chat_gif_picker() {
    let messages = (0..12_u64)
        .map(|n| sjk_identity::ChatMessage {
            id: n + 1,
            at: 0,
            key_id: format!("{:016x}", n + 1),
            name: format!("^{}Player {n}", n % 7 + 1),
            verified: n % 3 == 0,
            staff: false,
            text: format!("look https://giphy.com/gifs/ShotPick{:02}X", n),
            holocron: None,
        })
        .collect();
    let state = sjk_identity::ChatState {
        messages,
        revision: 1,
        online: 7,
        people: None,
        live: true,
        outcome: None,
        loaded: Some(1),
    };
    for (size, suffix) in [([1920, 1080], "1080p"), ([3840, 2160], "4k")] {
        for (tab, typed, key, shot) in [
            (2, "", "", "seen"),
            (1, "lightsaber", "test-key", "search"),
            // Search without a key: the key card shows by itself.
            (1, "", "", "key-card"),
            (1, "", "", "key-card-pasted"),
        ] {
            let state = state.clone();
            on_big_stack(move || {
                let ids = tiles();
                let menu = menu::ClientMenu::new(true, String::new());
                let cvars = [
                    ("ui_menuStyle", "sjk"),
                    (crate::settings::quick::HIDE_CVAR, "1"),
                    (crate::chat_gifs::search::KEY_CVAR, key),
                ];
                let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
                else {
                    return;
                };
                let _ = frame(&mut gpu, 10);
                // After the first frames, which read the (empty) favourites file.
                for id in [&ids[1], &ids[4]] {
                    crate::chat_gifs::favourites::set(id, true);
                }
                if let Some(console) = gpu.console.as_mut() {
                    if shot == "key-card-pasted" {
                        console
                            .preview_sjk_chat_key_card(state, "0123456789abcdefGHIJKL0123456789");
                    } else {
                        console.preview_sjk_chat_gifs(state, tab, typed);
                    }
                }
                let _ = frame(&mut gpu, 2);
                if !key.is_empty()
                    && let Some(console) = gpu.console.as_mut()
                {
                    console.answer_sjk_chat_gif_search(ids, 100);
                }
                let name = format!("duel6-sjk-chat-gif-picker-{shot}-{suffix}");
                println!("{}", shoot(&mut gpu, 8, &name).display());
            });
        }
    }
}

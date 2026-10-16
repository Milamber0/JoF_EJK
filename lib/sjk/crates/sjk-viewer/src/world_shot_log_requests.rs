//! World shots of log requests over duel6 under the SJK UI: the card a player gets when
//! the SJK team asks for their logs, on the main page, and the Staff page's Logs and
//! Crashes views (the kinds of crash, then one kind's reports). Off-screen renders (no
//! game window), ignored like the other world shots: they need a GPU adapter and the
//! installed game data named by `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer log_request_shots --
//! --ignored --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;
use sjk_identity::logs::{CrashGroup, CrashSummary, LogRequestRow};

fn profile(key: &str, name: &str) -> sjk_identity::Profile {
    sjk_identity::Profile {
        credits: 0,
        cards: Vec::new(),
        keys: Vec::new(),
        key_id: key.to_owned(),
        key: String::new(),
        name: name.to_owned(),
        bio: String::new(),
        verified: false,
        staff: false,
        created: 1_791_250_000,
        names: Vec::new(),
        medals: Vec::new(),
        achievements: Vec::new(),
        avatar: String::new(),
        unlocks: Vec::new(),
        holocron_counts: Default::default(),
        holocrons: Vec::new(),
    }
}

fn staff_state(players: Vec<sjk_identity::Profile>) -> sjk_identity::StaffState {
    let row = |id, key: &str, name: &str, state: &str, size| LogRequestRow {
        id,
        key_id: key.to_owned(),
        name: name.to_owned(),
        staff_key_id: "44f3d0b36c9b2510".to_owned(),
        staff_name: "^1Sol^7Vulpes".to_owned(),
        note: String::new(),
        created: 1_791_700_000 + id * 3_600,
        state: state.to_owned(),
        answered: if state == "sent" {
            1_791_710_000 + id * 3_600
        } else {
            0
        },
        size,
        ..LogRequestRow::default()
    };
    let group =
        |signature: &str, kind: &str, location: &str, message: &str, count, keys| CrashGroup {
            signature: signature.to_owned(),
            kind: kind.to_owned(),
            location: location.to_owned(),
            message: message.to_owned(),
            count,
            keys,
            first: 1_791_500_000,
            last: 1_791_720_000,
            builds: vec!["2026.1011.2".to_owned()],
        };
    let crash = |id, name: &str, build: &str| CrashSummary {
        id,
        name: name.to_owned(),
        created: 1_791_720_000 - id * 7_200,
        kind: "panic".to_owned(),
        build: build.to_owned(),
        ..CrashSummary::default()
    };
    sjk_identity::StaffState {
        serial: 3,
        players,
        message: "Asked Kushina for their logs".to_owned(),
        log_requests: vec![
            row(6, "1f2e3d4c5b6a7980", "^6Kushina", "open", 0),
            row(5, "9a0c51e2b7d34f80", "^5Creyon", "sent", 184_320),
            row(4, "0b1c2d3e4f5a6b7c", "Padawan^1Fox", "declined", 0),
            row(3, "1f2e3d4c5b6a7980", "^6Kushina", "sent", 251_904),
            row(2, "2a3b4c5d6e7f8091", "^2Mando", "expired", 0),
        ],
        crash_groups: vec![
            group(
                "00ff00ff00ff00ff",
                "panic",
                "crates/sjk-viewer/src/frame_split.rs:274:18",
                "depth attachment 1280x720 does not match the color attachment 1920x1080",
                7,
                2,
            ),
            group(
                "11ee11ee11ee11ee",
                "unclean",
                "",
                "SJK stopped without reporting an error (killed, an abort, a stack overflow or the graphics driver)",
                4,
                3,
            ),
            group(
                "22dd22dd22dd22dd",
                "error",
                "",
                "No graphics adapter: Vulkan and DirectX 12 both failed",
                1,
                1,
            ),
        ],
        crashes: vec![
            crash(31, "^6Kushina", "2026.1011.2"),
            crash(29, "^6Kushina", "2026.1011.1"),
            crash(22, "^5Creyon", "2026.1010.3"),
        ],
        crashes_of: "00ff00ff00ff00ff".to_owned(),
        ..sjk_identity::StaffState::default()
    }
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn log_request_shots() {
    on_big_stack(|| {
        let menu = menu::ClientMenu::new(true, String::new());
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
        ];
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
        else {
            return;
        };
        let _ = frame(&mut gpu, 10);
        // A player's game, asked: the card over the main page.
        gpu.log_request_card.preview = Some(vec![sjk_identity::LogRequest {
            id: 6,
            from: "^1Sol^7Vulpes".to_owned(),
            note: "The freeze you had on ffa3 last night".to_owned(),
            created: 1_791_700_000,
        }]);
        println!("{}", shoot(&mut gpu, 6, "duel6-log-request").display());
        assert!(gpu.log_request_card.is_open(), "the card shows");
        gpu.log_request_card.preview = Some(Vec::new());
        let _ = frame(&mut gpu, 2);

        let me = sjk_identity::Profile {
            staff: true,
            verified: true,
            ..profile("44f3d0b36c9b2510", "^1Sol^7Vulpes")
        };
        let kushina = profile("1f2e3d4c5b6a7980", "^6Kushina");
        let staff = staff_state(vec![
            me.clone(),
            kushina,
            profile("9a0c51e2b7d34f80", "^5Creyon"),
        ]);
        if let Some(console) = gpu.console.as_mut() {
            console.preview_staff(me, staff);
            console.staff_choose("1f2e3d4c5b6a7980");
            console.staff_view(true, None);
        }
        println!("{}", shoot(&mut gpu, 6, "duel6-staff-logs").display());
        if let Some(console) = gpu.console.as_mut() {
            console.staff_choose("44f3d0b36c9b2510");
        }
        println!("{}", shoot(&mut gpu, 4, "duel6-staff-logs-other").display());
        if let Some(console) = gpu.console.as_mut() {
            console.staff_view(false, None);
        }
        println!("{}", shoot(&mut gpu, 4, "duel6-staff-crashes").display());
        if let Some(console) = gpu.console.as_mut() {
            console.staff_view(false, Some("00ff00ff00ff00ff"));
        }
        println!("{}", shoot(&mut gpu, 4, "duel6-staff-crash-kind").display());
    });
}

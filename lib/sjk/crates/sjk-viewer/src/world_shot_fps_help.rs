//! World shots of the low-FPS card ([`crate::fps_help`]) over duel6 under the SJK UI:
//! offered with JoFTemple's numbers before its floor fix, then opened by `help_fps` with
//! EJK graphics on. Off-screen renders (no game window), ignored like the other world
//! shots: they need a GPU adapter and the installed game data named by `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer fps_help_card --
//! --ignored --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;
use std::time::Duration;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn fps_help_card() {
    on_big_stack(|| {
        let size = [1920, 1080];
        let menu = menu::ClientMenu::new(true, String::new());
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
        ];
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars) else {
            return;
        };
        gpu.graphics_reload.map = "maps/mp/duel6.bsp".to_owned();
        let _ = frame(&mut gpu, 20);
        gpu.ui_epoch -= Duration::from_millis(2_000);

        gpu.fps_help.finding = Some(crate::fps_help::Finding {
            sample: crate::gpu_phases::Sample {
                frame_ms: 8.07,
                light_ms: 5.64,
            },
            target_fps: 144,
        });
        gpu.open_fps_card(crate::fps_help::card::Mode::Offered);
        shoot(&mut gpu, 4, "fps-help-offered");
        assert!(gpu.fps_card.is_open(), "the card shows");

        // D ticks "Don't suggest this again"; the card stays.
        gpu.fps_card_choice(Some(crate::fps_help::card::Choice::Tick));
        assert_eq!(
            gpu.console
                .as_ref()
                .and_then(|console| console.bool_cvar(crate::fps_help::CVAR)),
            Some(false)
        );
        shoot(&mut gpu, 2, "fps-help-ticked");
        assert!(gpu.fps_card.is_open());

        // EJK graphics as the card's button sets them: the switch, then the reload.
        gpu.fps_card_choice(Some(crate::fps_help::card::Choice::Switch));
        gpu.reload_graphics(&mut None).expect("the reload starts");
        let started = std::time::Instant::now();
        let rebuilt = loop {
            let _ = gpu.render(&mut None);
            if let Some(world) = gpu.poll_rebuild() {
                break world;
            }
            assert!(
                started.elapsed() < Duration::from_secs(120),
                "duel6 built again"
            );
            std::thread::sleep(Duration::from_millis(16));
        };
        gpu = rebuilt;
        attach_target(&mut gpu, size);
        assert!(gpu.graphics_changes().is_empty());
        // `help_fps` with EJK graphics on: the button turns them off.
        gpu.fps_help_command().expect("help_fps opens the card");
        shoot(&mut gpu, 2, "fps-help-ejk-on");
        assert!(gpu.fps_card.is_open());
    });
}

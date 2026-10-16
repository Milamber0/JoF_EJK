//! World shots of the graphics reload ([`crate::graphics_reload`]) on the menu world:
//! duel6 under the SJK UI at High, the reload card once EJK is picked, the card
//! while the map is built again, and the menu over the world rebuilt on EJK's
//! settings, whose context is checked to run with them. Off-screen renders (no game
//! window), ignored like the other world shots: they need a GPU adapter and the installed
//! game data named by `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer graphics_reload_menu --
//! --ignored --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;
use std::time::{Duration, Instant};

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn graphics_reload_menu() {
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
        shoot(&mut gpu, 4, "graphics-reload-high");
        assert!(gpu.graphics_changes().is_empty());
        assert_eq!(gpu.context.hdr.mode, 1);

        // EJK, as the switch sets it: the card offers the reload on the main menu.
        if let Some(console) = gpu.console.as_mut() {
            crate::graphics_quality::Level::Vanilla.apply(console);
        }
        shoot(&mut gpu, 4, "graphics-reload-card");
        assert!(gpu.graphics_reload.card.is_open(), "the card shows");
        let changes = gpu.graphics_changes();
        assert!(changes.contains(&"sun and sky"), "{changes:?}");

        // Reload now: the card says the map is being built again over the old world.
        gpu.reload_graphics(&mut None).expect("the reload starts");
        shoot(&mut gpu, 2, "graphics-reload-reloading");
        let started = Instant::now();
        let rebuilt = loop {
            let _ = gpu.render(&mut None);
            if let Some(world) = gpu.poll_rebuild() {
                break world;
            }
            assert!(
                started.elapsed() < Duration::from_secs(120),
                "the menu map was built again"
            );
            std::thread::sleep(Duration::from_millis(16));
        };
        eprintln!(
            "graphics reload: duel6 rebuilt in {:.1} s",
            started.elapsed().as_secs_f32()
        );
        let previous_context = gpu.context.id;
        gpu = rebuilt;
        attach_target(&mut gpu, size);
        assert!(gpu.is_menu_world);
        assert!(gpu.client_menu.is_some() && gpu.console.is_some());
        assert!(gpu.context.id > previous_context);
        assert_eq!(gpu.context.hdr.mode, 0, "HDR off");
        assert!(!gpu.context.fxaa, "FXAA off");
        assert!(!gpu.context.sun_shadows.day.enabled, "sun and sky off");
        assert!(!gpu.context.material_maps.normal, "normal maps off");
        assert!(
            gpu.graphics_changes().is_empty(),
            "{:?}",
            gpu.graphics_changes()
        );
        let _ = frame(&mut gpu, 20);
        shoot(&mut gpu, 4, "graphics-reload-ultra-low");
        assert!(!gpu.graphics_reload.card.is_open(), "the card is gone");
    });
}

/// One frame as the application runs it (`viewer_app.rs`): draw, adopt a world that
/// finished installing (parking the menu world), take a rebuilt one, bring the parked
/// menu world back once off the server. New worlds get the shot's target.
fn step(gpu: &mut GpuState, parked: &mut crate::menu_world::Parked, size: [u32; 2]) {
    let _ = gpu.render(&mut None);
    match gpu.poll_world_install() {
        Ok(Some(installed)) => {
            let mut installed = gpu.adopt_world(installed);
            if installed.live_map_installed
                && let Some(menu) = &mut installed.client_menu
            {
                menu.joined();
            }
            if installed.live_map_installed {
                installed.present_latest_live_snapshot(&mut None);
            }
            attach_target(&mut installed, size);
            parked.install(gpu, installed);
        }
        Ok(None) => {}
        Err(error) => panic!("map transition failed: {error}"),
    }
    if let Some(rebuilt) = gpu.poll_rebuild() {
        *gpu = rebuilt;
        attach_target(gpu, size);
    }
    parked.restore_if_idle(gpu);
    std::thread::sleep(Duration::from_millis(16));
}

/// Step until `done`, for at most `seconds`.
fn until(
    gpu: &mut GpuState,
    parked: &mut crate::menu_world::Parked,
    size: [u32; 2],
    seconds: u64,
    what: &str,
    done: impl Fn(&GpuState) -> bool,
) {
    let started = Instant::now();
    while !done(gpu) {
        assert!(
            started.elapsed() < Duration::from_secs(seconds),
            "{what} within {seconds} s"
        );
        step(gpu, parked, size);
    }
    eprintln!("{what}: {:.1} s", started.elapsed().as_secs_f32());
}

/// The reload on a server: a local `devmap mp/ffa3` (its server named by
/// `JKA_DEDICATED`), EJK and `vid_restart`, which builds ffa3 again on the new
/// context with the connection kept; then `disconnect`, after which the parked menu
/// world is built again on that context too.
#[test]
#[ignore = "renders with the GPU, the installed game data named by JKA_GAME_DATA and a local sjk-server named by JKA_DEDICATED"]
fn graphics_reload_on_server() {
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
        let mut parked = crate::menu_world::Parked::new();
        let queue = |gpu: &mut GpuState, command: &str| {
            gpu.console
                .as_mut()
                .expect("a console")
                .queue_command(command)
                .expect("the command queues");
        };
        queue(&mut gpu, "devmap mp/ffa3");
        until(&mut gpu, &mut parked, size, 120, "joined ffa3", |gpu| {
            gpu.live_map_installed && gpu.live_session.is_some()
        });
        let _ = frame(&mut gpu, 30);
        shoot(&mut gpu, 4, "graphics-reload-server-high");
        let first = gpu.context.id;
        let server = gpu.live_session.as_ref().map(|s| s.server().to_string());

        if let Some(console) = gpu.console.as_mut() {
            crate::graphics_quality::Level::Vanilla.apply(console);
        }
        queue(&mut gpu, "vid_restart");
        until(&mut gpu, &mut parked, size, 120, "ffa3 reloaded", |gpu| {
            gpu.live_map_installed && gpu.live_session.is_some() && gpu.context.id != first
        });
        assert_eq!(
            gpu.live_session.as_ref().map(|s| s.server().to_string()),
            server,
            "the same server, never left"
        );
        assert_eq!(gpu.context.hdr.mode, 0, "HDR off");
        assert!(!gpu.context.sun_shadows.day.enabled, "sun and sky off");
        assert!(
            gpu.graphics_changes().is_empty(),
            "{:?}",
            gpu.graphics_changes()
        );
        assert!(gpu.next_context.is_none());
        // The first frames can show what the server did while the client was loading:
        // ffa3's force-field door, closed for want of the player's commands, until they
        // reach the server again.
        shoot(&mut gpu, 60, "graphics-reload-server-ultra-low");
        // Still connected after a few seconds of play.
        for _ in 0..120 {
            step(&mut gpu, &mut parked, size);
        }
        assert!(gpu.live_session.is_some(), "still on the server");

        // Off the server: the parked menu world catches up with the reload.
        let reloaded = gpu.context.id;
        queue(&mut gpu, "disconnect");
        until(
            &mut gpu,
            &mut parked,
            size,
            120,
            "menu world rebuilt",
            |gpu| {
                gpu.is_menu_world && gpu.context.id == reloaded && !gpu.graphics_reload.rebuilding()
            },
        );
        assert_eq!(gpu.context.hdr.mode, 0);
        assert!(!gpu.graphics_reload.card.is_open());
        let _ = frame(&mut gpu, 20);
        gpu.ui_epoch -= Duration::from_millis(2_000);
        shoot(&mut gpu, 4, "graphics-reload-menu-after-server");
    });
}

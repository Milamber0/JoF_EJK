//! World shots of Create game and its map list in the SJK UI's look over the
//! live duel6: the screen on Start, the Map row chosen, the server name being
//! typed, the map list, the list filtered and filtered to nothing; then the
//! screen and the list at 4K with `ui_textScale 1.2`, in a 4:3 window, and
//! with the classic menus (it has no classic+ version). Each canvas is checked
//! not to have run out of room. Ignored like the other world shots.

use super::*;

/// Wait for the shown map's levelshot, then save `name`.
fn settle_and_shoot(gpu: &mut GpuState, name: &str) {
    for _ in 0..240 {
        let _ = frame(gpu, 1);
        let settled = gpu
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.create_game_shot_state().0);
        if settled {
            break;
        }
    }
    println!("{}", shoot(gpu, 4, name).display());
    let overflowed = gpu
        .client_menu
        .as_ref()
        .is_some_and(|menu| menu.create_game_shot_state().1);
    assert!(!overflowed, "{name}: the canvas ran out of room");
}

/// Open Create game on row `selected` (with the map list and `filter` when
/// given).
fn show(gpu: &mut GpuState, selected: usize, filter: Option<&str>) {
    if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
        menu.create_game_for_shot(console, selected, filter);
    }
}

/// A client on duel6 at `size` with `cvars`, its menu opened on Create game.
fn client(size: [u32; 2], cvars: &[(&str, &str)]) -> Option<(GpuState, tempfile::TempDir)> {
    let menu = menu::ClientMenu::new(true, String::new());
    let (mut gpu, profile) = open("maps/mp/duel6.bsp", size, Some(menu), cvars)?;
    let _ = frame(&mut gpu, 10);
    show(&mut gpu, 8, None);
    gpu.ui_epoch -= std::time::Duration::from_millis(2_000);
    Some((gpu, profile))
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_create_game() {
    on_big_stack(|| {
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
        ];
        let Some((mut gpu, _profile)) = client([1920, 1080], &cvars) else {
            return;
        };
        settle_and_shoot(&mut gpu, "duel6-create-game");
        show(&mut gpu, 1, None);
        settle_and_shoot(&mut gpu, "duel6-create-game-map");
        show(&mut gpu, 6, None);
        if let Some(menu) = gpu.client_menu.as_mut() {
            menu.create_game_name_for_shot("Sol's saber school");
        }
        settle_and_shoot(&mut gpu, "duel6-create-game-name");
        show(&mut gpu, 1, Some(""));
        settle_and_shoot(&mut gpu, "duel6-create-game-maps");
        show(&mut gpu, 1, Some("mp/ffa"));
        settle_and_shoot(&mut gpu, "duel6-create-game-maps-filtered");
        show(&mut gpu, 1, Some("zzz"));
        settle_and_shoot(&mut gpu, "duel6-create-game-maps-none");
        drop(gpu);

        let styled = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::text::style::SCALE_CVAR, "1.2"),
        ];
        let Some((mut gpu, _profile)) = client([3840, 2160], &styled) else {
            return;
        };
        settle_and_shoot(&mut gpu, "duel6-create-game-4k");
        show(&mut gpu, 1, Some(""));
        settle_and_shoot(&mut gpu, "duel6-create-game-maps-4k");
        drop(gpu);

        let Some((mut gpu, _profile)) = client([1440, 1080], &styled) else {
            return;
        };
        settle_and_shoot(&mut gpu, "duel6-create-game-4x3");
        show(&mut gpu, 1, Some("tomb"));
        settle_and_shoot(&mut gpu, "duel6-create-game-maps-4x3");
        drop(gpu);

        let classic = [
            ("ui_menuStyle", "classic"),
            (crate::settings::quick::HIDE_CVAR, "1"),
        ];
        let Some((mut gpu, _profile)) = client([1920, 1080], &classic) else {
            return;
        };
        settle_and_shoot(&mut gpu, "duel6-create-game-classic");
        show(&mut gpu, 1, Some(""));
        settle_and_shoot(&mut gpu, "duel6-create-game-maps-classic");
    });
}

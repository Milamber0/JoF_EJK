//! World shots of the Profile screen's Holocrons tab ([`crate::console::holocrons_panel`])
//! over the live duel6 in the SJK UI, with the 3D holocron ([`crate::holocrons::stage`])
//! floating before the backdrop camera: made-up counts (a tier held, one held none of),
//! each tier chosen in turn, then a tier with the identity off, a 4:3, a 21:9 and a 4K
//! window, the page over the classic menus (where the world must still show) and the
//! page in a window with `ui_textScale 1.2`. They are off-screen renders (no game
//! window), ignored like the other world shots: they need a GPU adapter and the
//! installed game data named by `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer holocrons_tab -- --ignored
//! --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;
use crate::console::holocrons_panel::{Data, Recent, State};
use crate::profile_hub::Tab;

/// Made-up holdings: 12 uncommon, 3 rare, no legendary, 1 mythical; ten recent ones
/// (a gift with a long note among them), the next one a third of the way, 3 of 8 today.
pub(super) fn holdings() -> Data {
    let recent = |tier: usize, when: &str, note: Option<&str>| Recent {
        tier,
        when: when.to_owned(),
        gift: note.is_some(),
        note: note.unwrap_or_default().to_owned(),
        opened: false,
    };
    Data {
        counts: Some([12, 3, 0, 1]),
        recent: vec![
            recent(0, "10/10/2026 14:05", None),
            recent(
                3,
                "10/10/2026 09:12",
                Some("For finding the fog bug on duel6 and telling us"),
            ),
            recent(1, "09/10/2026 22:48", None),
            recent(0, "09/10/2026 20:15", None),
            recent(0, "09/10/2026 18:30", None),
            recent(1, "08/10/2026 23:01", None),
            recent(0, "08/10/2026 21:44", None),
            recent(1, "08/10/2026 19:09", None),
            recent(0, "07/10/2026 22:30", None),
            recent(0, "07/10/2026 20:02", None),
        ],
        next: "Next holocron in about 20 minutes of play".to_owned(),
        today: Some((3, 8)),
        fraction: Some(1.0 / 3.0),
        every: Some(1_800),
        can_open: true,
        ..Data::nothing(State::Known)
    }
}

/// Open the main menu over duel6 in `size`, with the tab on the Profile screen.
fn open_tab(size: [u32; 2], cvars: &[(&str, &str)], data: Data) -> Option<GpuState> {
    let menu = menu::ClientMenu::new(true, String::new());
    let (mut gpu, profile) = open("maps/mp/duel6.bsp", size, Some(menu), cvars)?;
    // The window's profile folder lives as long as the test.
    std::mem::forget(profile);
    let _ = frame(&mut gpu, 10);
    gpu.holocron_stage_seconds = Some(2.4);
    if let Some(console) = gpu.console.as_mut() {
        console.preview_holocrons(data, 0);
    }
    gpu.open_profile_hub(Tab::Holocrons);
    assert_eq!(gpu.profile_hub_tab(), Some(Tab::Holocrons));
    Some(gpu)
}

/// Choose tier `index` and let the holocron swap and grow into its look (the camera's
/// cut through dark on this toured map takes a second or two the first time).
fn choose(gpu: &mut GpuState, index: usize, frames: u32) {
    if let Some(console) = gpu.console.as_mut() {
        console.select_holocron(index);
    }
    let _ = frame(gpu, frames);
}

fn overflowed(gpu: &GpuState) -> bool {
    gpu.console
        .as_ref()
        .is_some_and(crate::console::ViewerConsole::holocrons_page_overflowed)
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_holocrons() {
    on_big_stack(|| {
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            ("name", "^1Sol^7Vulpes"),
        ];
        let Some(mut gpu) = open_tab([1920, 1080], &cvars, holdings()) else {
            return;
        };
        let _ = frame(&mut gpu, 150);
        let mut images = Vec::new();
        for (index, tier) in crate::holocrons::TIERS.iter().enumerate() {
            choose(&mut gpu, index, 60);
            assert!(!overflowed(&gpu), "{}", tier.id);
            let path = shoot(&mut gpu, 2, &format!("duel6-holocrons-{}", tier.id));
            println!("{}", path.display());
            images.push(image::open(path).expect("the shot").to_rgba8());
        }
        println!(
            "{}",
            sheet(&images, 2, 960, "duel6-holocrons-tiers").display()
        );
        // A tier held none of, close up on the stage: dimmed, not hidden.
        choose(&mut gpu, 2, 60);
        let locked = frame(&mut gpu, 2);
        let stage = image::imageops::crop_imm(&locked, 120, 220, 800, 560).to_image();
        println!(
            "{}",
            sheet(&[stage], 1, 800, "duel6-holocrons-locked-stage").display()
        );
        // The identity off: nothing known, every tier dimmed, the reason said.
        if let Some(console) = gpu.console.as_mut() {
            console.preview_holocrons(Data::nothing(State::IdentityOff), 3);
        }
        let _ = frame(&mut gpu, 60);
        println!(
            "{}",
            shoot(&mut gpu, 2, "duel6-holocrons-identity-off").display()
        );
        assert!(!overflowed(&gpu));
        // Waiting for the hub.
        if let Some(console) = gpu.console.as_mut() {
            console.preview_holocrons(Data::nothing(State::Waiting), 1);
        }
        let _ = frame(&mut gpu, 4);
        println!(
            "{}",
            shoot(&mut gpu, 2, "duel6-holocrons-waiting").display()
        );
    });
}

/// Other windows and styles: 4:3, 21:9, 4K, `ui_textScale 1.2`, and the page over the
/// classic menus (the world stays visible behind it).
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_holocrons_windows() {
    on_big_stack(|| {
        let sjk = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            ("name", "^1Sol^7Vulpes"),
        ];
        for (size, name, tier) in [
            ([1440, 1080], "4x3", 2),
            ([2560, 1080], "21x9", 3),
            ([3840, 2160], "4k", 1),
        ] {
            let Some(mut gpu) = open_tab(size, &sjk, holdings()) else {
                return;
            };
            let _ = frame(&mut gpu, 150);
            choose(&mut gpu, tier, 60);
            assert!(!overflowed(&gpu), "{name}");
            println!(
                "{}",
                shoot(&mut gpu, 2, &format!("duel6-holocrons-{name}")).display()
            );
        }
        let styled = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::text::style::SCALE_CVAR, "1.2"),
        ];
        if let Some(mut gpu) = open_tab([1920, 1080], &styled, holdings()) {
            let _ = frame(&mut gpu, 150);
            choose(&mut gpu, 1, 60);
            println!("{}", shoot(&mut gpu, 2, "duel6-holocrons-styled").display());
        }
        // The classic menus: the page opens on its own (`holocrons`), and the world,
        // which classic's pages hide, shows behind it.
        let classic = [("ui_menuStyle", "classic")];
        let menu = menu::ClientMenu::new(true, String::new());
        let Some((mut gpu, _profile)) =
            open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &classic)
        else {
            return;
        };
        let _ = frame(&mut gpu, 10);
        gpu.holocron_stage_seconds = Some(2.4);
        if let Some(console) = gpu.console.as_mut() {
            console.preview_holocrons(holdings(), 0);
            console.open_holocrons_panel();
        }
        choose(&mut gpu, 3, 90);
        println!(
            "{}",
            shoot(&mut gpu, 2, "duel6-holocrons-classic").display()
        );
    });
}

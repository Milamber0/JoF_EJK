//! World shots of the Collection screen ([`crate::console::collection_panel`]) over the
//! live duel6 in the SJK UI, on a made-up profile (two medals, the Sun blade owned and
//! worn, some achievements): from the main page, every tab, the model on the menu map's
//! stage behind Shaders (the Sun, then the locked Storm previewed on the saber), Toys
//! (the Illuminate holocron by it) and Nameplates (the plate over its head); the same
//! in a 4:3 window; then from the game menu over a match, the model in the page's live
//! preview; and the page on its own as the classic menus open it. Then the hover cards:
//! one on a thing of each tab, and one on a tier of the Holocrons tab. The blade skins'
//! looks come from the packs `SJK_TEST_PACKS` names. Ignored like the other world shots.

use super::*;
use crate::player_menu::ReturnTarget;
use crate::profile_hub::{Screen, Tab};

/// The made-up grant.
fn sun() -> sjk_identity::Unlock {
    sjk_identity::Unlock {
        id: "saber_sun".to_owned(),
        granted: 1_791_336_225,
        note: "Thanks for testing the blades with us".to_owned(),
        count: 1,
        medal: None,
    }
}

/// A made-up profile with the achievements half done.
fn preview() -> crate::console::profile_panel::Preview {
    let mut preview = super::tests::profile_preview();
    let held: Vec<sjk_identity::Achievement> = crate::achievements::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| sjk_identity::Achievement {
            id: kind.id.to_owned(),
            progress: if index % 2 == 0 {
                kind.goal
            } else {
                kind.goal * (index as u64 % 7 + 1) / 9
            },
            goal: kind.goal,
            unlocked: if index % 2 == 0 { 1_791_336_225 } else { 0 },
        })
        .collect();
    preview.standings = crate::achievements::standings(&held);
    preview
}

/// The Collection's page ran out of room.
fn overflowed(gpu: &GpuState) -> bool {
    gpu.console
        .as_ref()
        .is_some_and(crate::console::ViewerConsole::profile_pages_overflowed)
}

/// The part of a tab's name in a shot's file name.
fn part(tab: Tab) -> String {
    tab.label().to_lowercase().replace(' ', "-")
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_collection() {
    on_big_stack(|| {
        super::saber_skins::mount_test_packs();
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::unlockables::SABER_SKIN_CVAR, "saber_sun"),
            ("name", "^1Sol^7Vulpes"),
        ];
        for size in [[1920, 1080], [1440, 1080]] {
            let suffix = if size[0] == 1920 { "" } else { "-4x3" };
            let Some((mut gpu, _profile)) = open(
                "maps/mp/duel6.bsp",
                size,
                Some(menu::ClientMenu::new(true, String::new())),
                &cvars,
            ) else {
                return;
            };
            // The shots have no hub: they own the unlocks, as a profile listing them would.
            gpu.looks.shot_owns_unlocks = true;
            let _ = frame(&mut gpu, 10);
            if let Some(console) = gpu.console.as_mut() {
                console.preview_profile(preview());
                console.preview_collection(Some(vec![sun()]), 0.8);
                console.open_profile_hub_page(Tab::Medals, ReturnTarget::MainMenu);
            }
            assert_eq!(gpu.profile_hub_tab(), Some(Tab::Medals));
            for _ in 0..240 {
                let _ = frame(&mut gpu, 1);
                if crate::medals::art::decoded().is_some() {
                    break;
                }
            }
            for tab in &Screen::Collection.tabs()[..5] {
                gpu.profile_hub_show_for_shot(*tab);
                assert_eq!(gpu.profile_hub_tab(), Some(*tab));
                // The camera goes to the player's shot behind the model's tabs.
                let frames = if tab.index() >= 2 { 150 } else { 8 };
                gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
                let name = format!("duel6-collection-{}{suffix}", part(*tab));
                println!("{}", shoot(&mut gpu, frames, &name).display());
                assert!(!overflowed(&gpu), "{tab:?}{suffix}");
            }
            if size[0] != 1920 {
                // The grid of cards in the 4:3 window.
                gpu.profile_hub_show_for_shot(Tab::Shaders);
                if let Some(console) = gpu.console.as_mut() {
                    let _ =
                        console.set_cvar(crate::console::collection_panel::SHADER_VIEW_CVAR, "1");
                }
                println!(
                    "{}",
                    shoot(&mut gpu, 30, "duel6-collection-shaders-grid-4x3").display()
                );
                assert!(!overflowed(&gpu), "the grid, 4:3");
                if let Some(console) = gpu.console.as_mut() {
                    let _ =
                        console.set_cvar(crate::console::collection_panel::SHADER_VIEW_CVAR, "0");
                }
                continue;
            }
            // Toys with the holocron lit (`toy_illuminate`): the switch is on.
            gpu.profile_hub_show_for_shot(Tab::Toys);
            gpu.illuminate.set_on(true);
            gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
            println!(
                "{}",
                shoot(&mut gpu, 60, "duel6-collection-toys-lit").display()
            );
            gpu.illuminate.set_on(false);
            // A locked one, previewed on the saber.
            gpu.profile_hub_show_for_shot(Tab::Shaders);
            let storm = 1 + crate::unlockables::blade_skins_by_tier()
                .iter()
                .position(|skin| skin.id == "saber_storm")
                .expect("the Storm blade");
            if let Some(console) = gpu.console.as_mut() {
                console.collection_shader_for_shot(storm);
            }
            println!(
                "{}",
                shoot(&mut gpu, 30, "duel6-collection-shaders-preview").display()
            );
            // The grid of cards (`ui_shaderView 1`), a few more owned, a Mythical chosen.
            if let Some(console) = gpu.console.as_mut() {
                let owned = [
                    "saber_sun",
                    "saber_void",
                    "saber_storm",
                    "saber_runic",
                    "saber_glitch",
                ]
                .map(|id| sjk_identity::Unlock {
                    id: id.to_owned(),
                    ..sun()
                })
                .to_vec();
                console.preview_collection(Some(owned), 0.8);
                let _ = console.set_cvar(crate::console::collection_panel::SHADER_VIEW_CVAR, "1");
                console.collection_shader_for_shot(2);
            }
            println!(
                "{}",
                shoot(&mut gpu, 30, "duel6-collection-shaders-grid").display()
            );
            assert!(!overflowed(&gpu), "the grid");
            // The Spectral owned and chosen: its six parts in three columns, its smoky
            // edge switched off (`cg_saberSkinOptions`), on the model too.
            let spectral = 1 + crate::unlockables::blade_skins_by_tier()
                .iter()
                .position(|skin| skin.id == "saber_spectral")
                .expect("the Spectral blade");
            if let Some(console) = gpu.console.as_mut() {
                let _ = console.set_cvar(crate::console::collection_panel::SHADER_VIEW_CVAR, "0");
                let owned = ["saber_sun", "saber_spectral"]
                    .map(|id| sjk_identity::Unlock {
                        id: id.to_owned(),
                        ..sun()
                    })
                    .to_vec();
                console.preview_collection(Some(owned), 0.8);
                let _ = console.set_cvar(crate::saber_skin_options::CVAR, "saber_spectral.smoke");
                console.collection_shader_for_shot(spectral);
            }
            println!(
                "{}",
                shoot(&mut gpu, 60, "duel6-collection-shaders-parts").display()
            );
            assert!(!overflowed(&gpu), "the parts");
            if let Some(console) = gpu.console.as_mut() {
                let _ = console.set_cvar(crate::saber_skin_options::CVAR, "");
                console.preview_collection(Some(vec![sun()]), 0.8);
            }
            // Ctrl+Tab goes on to Holocrons (the Holocrons page, under the same row) and
            // on to Cards, on the same screen.
            gpu.profile_hub_show_for_shot(Tab::Nameplates);
            gpu.profile_hub_next_for_shot();
            assert_eq!(gpu.profile_hub_tab(), Some(Tab::Holocrons));
            println!(
                "{}",
                shoot(&mut gpu, 150, "duel6-collection-holocrons").display()
            );
            assert!(!overflowed(&gpu), "Holocrons");
            gpu.profile_hub_next_for_shot();
            assert_eq!(gpu.profile_hub_tab(), Some(Tab::Cards));
        }
        // From the game menu, over a "match" (no server): the live preview.
        let Some((mut gpu, _profile)) = open(
            "maps/mp/duel6.bsp",
            [1920, 1080],
            Some(menu::ClientMenu::new(false, String::new())),
            &cvars,
        ) else {
            return;
        };
        gpu.looks.shot_owns_unlocks = true;
        let shots = menu_backdrop::tour_for("Yavin Training Grounds").expect("duel6's tour");
        let (yaw, pitch) = look(shots[3].from, shots[3].at);
        aim(&mut gpu, shots[3].from, yaw, pitch);
        gpu.is_menu_world = false;
        gpu.game_menu = true;
        if let Some(console) = gpu.console.as_mut() {
            console.close_for_connection();
            console.preview_profile(preview());
            console.preview_collection(Some(vec![sun()]), 0.8);
        }
        // A match's view: `cg_fov 80` widened for 16:9 renders about 64 degrees up the
        // window (`GpuState::scope_fov`), narrower than the menu map's 90.
        gpu.field_of_view = 64.3;
        let _ = frame(&mut gpu, 20);
        gpu.open_collection_from_game();
        assert_eq!(gpu.profile_hub_tab(), Some(Tab::Medals));
        for tab in [Tab::Achievements, Tab::Shaders, Tab::Toys, Tab::Holocrons] {
            gpu.profile_hub_show_for_shot(tab);
            let name = format!("duel6-collection-in-game-{}", part(tab));
            println!("{}", shoot(&mut gpu, 40, &name).display());
            assert!(!overflowed(&gpu), "in game {tab:?}");
        }
        // Escape returns to the game menu, on Collection.
        if let Some(console) = gpu.console.as_mut() {
            console.close_profile_hub_page();
        }
        assert!(gpu.game_menu, "back on the game menu");
        assert_eq!(
            gpu.game_menu_row,
            crate::ingame_menu::sjk_view::Entry::Collection.index()
        );
        // On its own, as the classic menus' `collection` opens it.
        let Some((mut gpu, _profile)) = open(
            "maps/mp/duel6.bsp",
            [1920, 1080],
            Some(menu::ClientMenu::new(true, String::new())),
            &[("ui_menuStyle", "classic")],
        ) else {
            return;
        };
        let _ = frame(&mut gpu, 10);
        if let Some(console) = gpu.console.as_mut() {
            console.preview_profile(preview());
            console.preview_collection_alone(Tab::Shaders, Some(vec![sun()]), 0.8);
        }
        println!(
            "{}",
            shoot(&mut gpu, 30, "duel6-collection-classic").display()
        );
        assert!(!overflowed(&gpu), "on its own");
    });
}

/// The rarity effects (`rarity_fx`) in motion: the Shaders grid with every tier owned
/// at moments a third of a second apart over one sheen's period (the files
/// `duel6-rarity-grid-<n>`), the list at two moments, and the Holocrons tab.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_rarity_effects() {
    on_big_stack(|| {
        super::saber_skins::mount_test_packs();
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::unlockables::SABER_SKIN_CVAR, "saber_sun"),
            ("name", "^1Sol^7Vulpes"),
        ];
        let Some((mut gpu, _profile)) = open(
            "maps/mp/duel6.bsp",
            [1920, 1080],
            Some(menu::ClientMenu::new(true, String::new())),
            &cvars,
        ) else {
            return;
        };
        gpu.looks.shot_owns_unlocks = true;
        let _ = frame(&mut gpu, 10);
        let owned: Vec<sjk_identity::Unlock> = crate::unlockables::ALL
            .iter()
            .filter(|skin| !matches!(skin.id, "saber_molten" | "saber_frost" | "saber_banner"))
            .map(|skin| sjk_identity::Unlock {
                id: skin.id.to_owned(),
                ..sun()
            })
            .collect();
        if let Some(console) = gpu.console.as_mut() {
            console.preview_profile(preview());
            console.preview_collection(Some(owned.clone()), 0.0);
            console.open_profile_hub_page(Tab::Shaders, ReturnTarget::MainMenu);
        }
        gpu.profile_hub_show_for_shot(Tab::Shaders);
        if let Some(console) = gpu.console.as_mut() {
            let _ = console.set_cvar(crate::console::collection_panel::SHADER_VIEW_CVAR, "1");
            // Void, a Mythical, chosen: its white outline beside the worn Sun's gold.
            console.collection_shader_for_shot(2);
        }
        let _ = frame(&mut gpu, 150);
        for step in 0..12 {
            let seconds = step as f32 / 3.0;
            if let Some(console) = gpu.console.as_mut() {
                console.preview_collection(Some(owned.clone()), seconds);
            }
            let name = format!("duel6-rarity-grid-{step:02}");
            println!("{}", shoot(&mut gpu, 2, &name).display());
            assert!(!overflowed(&gpu), "the grid at {seconds}");
        }
        if let Some(console) = gpu.console.as_mut() {
            let _ = console.set_cvar(crate::console::collection_panel::SHADER_VIEW_CVAR, "0");
            console.collection_shader_for_shot(1);
        }
        for (step, seconds) in [0.4_f32, 2.2].into_iter().enumerate() {
            if let Some(console) = gpu.console.as_mut() {
                console.preview_collection(Some(owned.clone()), seconds);
            }
            let name = format!("duel6-rarity-list-{step}");
            println!("{}", shoot(&mut gpu, 2, &name).display());
            assert!(!overflowed(&gpu), "the list at {seconds}");
        }
        gpu.profile_hub_show_for_shot(Tab::Nameplates);
        gpu.profile_hub_next_for_shot();
        assert_eq!(gpu.profile_hub_tab(), Some(Tab::Holocrons));
        println!(
            "{}",
            shoot(&mut gpu, 150, "duel6-rarity-holocrons").display()
        );
        assert!(!overflowed(&gpu), "Holocrons");
    });
}

/// The hover cards: one on a thing of each tab (a medal, an achievement half done, a
/// locked blade skin, the toy, a card) and one on a tier of the Holocrons tab, each shot
/// alone and all in one sheet.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_hover_cards() {
    on_big_stack(|| {
        super::saber_skins::mount_test_packs();
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::unlockables::SABER_SKIN_CVAR, "saber_sun"),
            ("name", "^1Sol^7Vulpes"),
        ];
        let Some((mut gpu, _profile)) = open(
            "maps/mp/duel6.bsp",
            [1920, 1080],
            Some(menu::ClientMenu::new(true, String::new())),
            &cvars,
        ) else {
            return;
        };
        gpu.looks.shot_owns_unlocks = true;
        let _ = frame(&mut gpu, 10);
        let cards = vec![sjk_identity::CardStack {
            card: None,
            rarity: "legendary".to_owned(),
            finish: "holo".to_owned(),
            edition: "alpha".to_owned(),
            count: 2,
            first: 1_791_641_100,
            last: 1_791_641_100,
        }];
        if let Some(console) = gpu.console.as_mut() {
            console.preview_profile(preview());
            console.preview_collection(Some(vec![sun()]), 0.8);
            console.preview_cards(cards, 1_340);
            console.open_profile_hub_page(Tab::Medals, ReturnTarget::MainMenu);
        }
        for _ in 0..240 {
            let _ = frame(&mut gpu, 1);
            if crate::medals::art::decoded().is_some() {
                break;
            }
        }
        let mut images = Vec::new();
        for (tab, index) in [
            (Tab::Medals, 2),
            (Tab::Achievements, 5),
            (Tab::Shaders, 3),
            (Tab::Toys, 0),
            (Tab::Cards, 0),
        ] {
            gpu.profile_hub_show_for_shot(tab);
            if let Some(console) = gpu.console.as_mut() {
                console.collection_hover_for_shot(Some(index));
            }
            let frames = if tab.index() >= 2 { 150 } else { 8 };
            let name = format!("duel6-hover-{}", part(tab));
            let path = shoot(&mut gpu, frames, &name);
            println!("{}", path.display());
            assert!(!overflowed(&gpu), "{tab:?}");
            images.push(image::open(path).expect("the shot").to_rgba8());
            if let Some(console) = gpu.console.as_mut() {
                console.collection_hover_for_shot(None);
            }
        }
        gpu.profile_hub_show_for_shot(Tab::Nameplates);
        gpu.profile_hub_next_for_shot();
        assert_eq!(gpu.profile_hub_tab(), Some(Tab::Holocrons));
        if let Some(console) = gpu.console.as_mut() {
            console.preview_holocrons(super::holocrons_tab::holdings(), 1);
            console.holocron_hover_for_shot(Some(3));
        }
        let path = shoot(&mut gpu, 150, "duel6-hover-holocrons");
        println!("{}", path.display());
        assert!(!overflowed(&gpu), "Holocrons");
        images.push(image::open(path).expect("the shot").to_rgba8());
        println!("{}", sheet(&images, 2, 960, "duel6-hover-sheet").display());
    });
}

/// The Blade Lab's swatches (15/10/2026) on the Shaders list, every shader owned: the
/// list scrolled to each of eleven in turn (the Reactor, Soap Bubble, Neon, Hourglass, Fiber
/// Optic, Monarch, Quantum, Sketch, Comic, Quicksilver and Obsidian Edge), held on the
/// stage, at two moments.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_lab_swatches() {
    on_big_stack(|| {
        super::saber_skins::mount_test_packs();
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::unlockables::SABER_SKIN_CVAR, "saber_reactor"),
            ("name", "^1Sol^7Vulpes"),
        ];
        let Some((mut gpu, _profile)) = open(
            "maps/mp/duel6.bsp",
            [1920, 1080],
            Some(menu::ClientMenu::new(true, String::new())),
            &cvars,
        ) else {
            return;
        };
        gpu.looks.shot_owns_unlocks = true;
        let _ = frame(&mut gpu, 10);
        let owned: Vec<sjk_identity::Unlock> = crate::unlockables::ALL
            .iter()
            .map(|skin| sjk_identity::Unlock {
                id: skin.id.to_owned(),
                ..sun()
            })
            .collect();
        if let Some(console) = gpu.console.as_mut() {
            console.preview_profile(preview());
            console.preview_collection(Some(owned.clone()), 0.0);
            console.open_profile_hub_page(Tab::Shaders, ReturnTarget::MainMenu);
        }
        gpu.profile_hub_show_for_shot(Tab::Shaders);
        let row = |id: &str| {
            1 + crate::unlockables::blade_skins_by_tier()
                .iter()
                .position(|skin| skin.id == id)
                .expect("in the catalogue")
        };
        for id in [
            "saber_reactor",
            "saber_bubble",
            "saber_neon",
            "saber_hourglass",
            "saber_fiber",
            "saber_monarch",
            "saber_quantum",
            "saber_sketch",
            "saber_comic",
            "saber_quicksilver",
            "saber_obsidian",
        ] {
            if let Some(console) = gpu.console.as_mut() {
                console.collection_shader_for_shot(row(id));
            }
            let _ = frame(&mut gpu, 60);
            for seconds in [1.0_f32, 3.3] {
                if let Some(console) = gpu.console.as_mut() {
                    console.preview_collection(Some(owned.clone()), seconds);
                }
                let name = format!("duel6-lab-swatches-{}-{seconds}", &id[6..]);
                println!("{}", shoot(&mut gpu, 2, &name).display());
                assert!(!overflowed(&gpu), "{id} at {seconds}");
            }
        }
    });
}

/// The Collection with `debug_collectibles` on for a staff key: the made-up profile
/// dressed with every shader, medal, achievement and card (`debug_collectibles.rs`), on
/// the tabs that show them.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_collection_debug() {
    on_big_stack(|| {
        super::saber_skins::mount_test_packs();
        let cvars = [
            ("ui_menuStyle", "sjk"),
            (crate::settings::quick::HIDE_CVAR, "1"),
            (crate::unlockables::SABER_SKIN_CVAR, "saber_quantum"),
            ("name", "^1Sol^7Vulpes"),
            (crate::debug_collectibles::CVAR, "1"),
        ];
        let Some((mut gpu, _profile)) = open(
            "maps/mp/duel6.bsp",
            [1920, 1080],
            Some(menu::ClientMenu::new(true, String::new())),
            &cvars,
        ) else {
            return;
        };
        gpu.looks.shot_owns_unlocks = true;
        let _ = frame(&mut gpu, 10);
        let mut preview = super::tests::profile_preview();
        if let Some(me) = preview.snapshot.me.as_mut() {
            me.staff = true;
        }
        preview.snapshot = crate::debug_collectibles::dressed(true, preview.snapshot);
        let me = preview.snapshot.me.clone().expect("the made-up profile");
        preview.standings = crate::achievements::standings(&me.achievements);
        if let Some(console) = gpu.console.as_mut() {
            console.preview_profile(preview);
            console.preview_collection(Some(me.unlocks.clone()), 0.8);
            console.open_profile_hub_page(Tab::Medals, ReturnTarget::MainMenu);
        }
        for _ in 0..240 {
            let _ = frame(&mut gpu, 1);
            if crate::medals::art::decoded().is_some() {
                break;
            }
        }
        for tab in [Tab::Medals, Tab::Achievements, Tab::Shaders, Tab::Cards] {
            gpu.profile_hub_show_for_shot(tab);
            let frames = if tab == Tab::Shaders { 150 } else { 30 };
            gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
            let name = format!("duel6-collection-debug-{}", part(tab));
            println!("{}", shoot(&mut gpu, frames, &name).display());
            assert!(!overflowed(&gpu), "{tab:?}");
        }
    });
}

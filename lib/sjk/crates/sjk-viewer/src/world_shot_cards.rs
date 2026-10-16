//! World shots of opening holocrons and of the Collection's Cards tab
//! ([`crate::console::holocrons_panel::opening`], `collection_cards.rs`) over the live duel6
//! in the SJK UI: the Holocrons tab with its Open buttons and the credits; the ceremony at
//! each moment of a card's entrance (the burst, the card turning in, the credits counting,
//! the words) in each finish; the wait for the hub and a refusal; then the Cards tab with
//! every kind of sealed card, a Solar one chosen, and with none; and the loot table over
//! the Holocrons tab. Made-up holdings; they
//! are off-screen renders (no game window), ignored like the other world shots: they need
//! a GPU adapter and the installed game data named by `JKA_GAME_DATA`.
//!
//! Then the first revealed card's face (`duel6_sjk_card_faces`), from the `sjk_cards` pack
//! in the folder `SJK_TEST_PACKS` names (the hub's `assets/dist`): up close on the Cards
//! tab in each finish, and coming out of a holocron.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer cards_shots -- --ignored
//! --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;
use crate::cards::{FINISHES, RARITIES};
use crate::console::holocrons_panel::Data;
use crate::console::holocrons_panel::opening::{Prize, Shown};
use crate::profile_hub::Tab;

/// The Holocrons tab's made-up holdings with holocrons to open and credits.
fn holdings() -> Data {
    Data {
        sealed: [vec![11, 12, 15], vec![13], Vec::new(), vec![14]],
        credits: Some(1_340),
        ..super::holocrons_tab::holdings()
    }
}

/// One holocron of tier `tier` opened: a card of `rarity` in `finish`.
fn shown(tier: usize, rarity: usize, finish: usize, credits: u64) -> Shown {
    Shown {
        holocron: 1,
        tier,
        rarity: &RARITIES[rarity],
        finish: &FINISHES[finish],
        edition: "Alpha Edition".to_owned(),
        credits,
        card: None,
    }
}

/// Every kind of sealed card: each rarity in each finish.
fn every_kind() -> Vec<sjk_identity::CardStack> {
    let mut list = Vec::new();
    for finish in &FINISHES {
        for rarity in &RARITIES {
            list.push(sjk_identity::CardStack {
                card: None,
                rarity: rarity.id.to_owned(),
                finish: finish.id.to_owned(),
                edition: "alpha".to_owned(),
                count: (list.len() as u32 % 4) + 1,
                first: 1_791_641_100,
                last: 1_791_641_100,
            });
        }
    }
    list
}

fn sjk_cvars() -> [(&'static str, &'static str); 3] {
    [
        ("ui_menuStyle", "sjk"),
        (crate::settings::quick::HIDE_CVAR, "1"),
        ("name", "^1Sol^7Vulpes"),
    ]
}

/// The main menu over duel6 in `size`, the Collection open on its Holocrons tab (the
/// loot table shown by [`duel6_sjk_loot_table`]).
fn open_holocrons(size: [u32; 2]) -> Option<GpuState> {
    let menu = menu::ClientMenu::new(true, String::new());
    let (mut gpu, profile) = open("maps/mp/duel6.bsp", size, Some(menu), &sjk_cvars())?;
    // The window's profile folder lives as long as the test.
    std::mem::forget(profile);
    let _ = frame(&mut gpu, 10);
    gpu.holocron_stage_seconds = Some(2.4);
    if let Some(console) = gpu.console.as_mut() {
        console.preview_holocrons(holdings(), 0);
    }
    gpu.open_profile_hub(Tab::Holocrons);
    assert_eq!(gpu.profile_hub_tab(), Some(Tab::Holocrons));
    let _ = frame(&mut gpu, 150);
    Some(gpu)
}

fn overflowed(gpu: &GpuState) -> bool {
    gpu.console.as_ref().is_some_and(|console| {
        console.holocrons_page_overflowed() || console.profile_pages_overflowed()
    })
}

/// Play `cards`' ceremony drawn `elapsed` seconds into the first card's entrance.
fn ceremony(gpu: &mut GpuState, cards: Vec<Shown>, elapsed: f32) {
    if let Some(console) = gpu.console.as_mut() {
        let prizes = cards.into_iter().map(Prize::Card).collect();
        console.preview_opening(Some(prizes), elapsed, 3.1);
    }
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_opening() {
    on_big_stack(|| {
        let Some(mut gpu) = open_holocrons([1920, 1080]) else {
            return;
        };
        println!("{}", shoot(&mut gpu, 2, "duel6-opening-page").display());
        assert!(!overflowed(&gpu));
        // A player while the hub keeps opening to staff: a line in place of the buttons.
        if let Some(console) = gpu.console.as_mut() {
            console.preview_holocrons(
                Data {
                    can_open: false,
                    ..holdings()
                },
                0,
            );
        }
        let _ = frame(&mut gpu, 4);
        println!("{}", shoot(&mut gpu, 2, "duel6-opening-closed").display());
        assert!(!overflowed(&gpu));
        if let Some(console) = gpu.console.as_mut() {
            console.preview_holocrons(holdings(), 0);
        }
        // A Holo card's entrance, moment by moment.
        let mut moments = Vec::new();
        for (elapsed, name) in [
            (0.3, "burst"),
            (0.8, "turning"),
            (1.5, "counting"),
            (3.0, "words"),
        ] {
            ceremony(
                &mut gpu,
                vec![shown(1, 2, 1, 42), shown(0, 0, 0, 14)],
                elapsed,
            );
            let path = shoot(&mut gpu, 2, &format!("duel6-opening-holo-{name}"));
            assert!(!overflowed(&gpu), "{name}");
            println!("{}", path.display());
            moments.push(image::open(path).expect("the shot").to_rgba8());
        }
        println!(
            "{}",
            sheet(&moments, 2, 960, "duel6-opening-moments").display()
        );
        // Each finish, standing still.
        let mut finishes = Vec::new();
        for (tier, rarity, finish, credits) in [(0, 0, 0, 14), (2, 3, 1, 96), (3, 1, 2, 311)] {
            ceremony(&mut gpu, vec![shown(tier, rarity, finish, credits)], 3.0);
            let path = shoot(
                &mut gpu,
                2,
                &format!("duel6-opening-{}", FINISHES[finish].id),
            );
            assert!(!overflowed(&gpu));
            println!("{}", path.display());
            finishes.push(image::open(path).expect("the shot").to_rgba8());
        }
        println!(
            "{}",
            sheet(&finishes, 3, 640, "duel6-opening-finishes").display()
        );
        // The wait for the hub, then a refusal.
        if let Some(console) = gpu.console.as_mut() {
            console.preview_opening(None, 0.0, 0.0);
            console.preview_opening_state(None);
        }
        println!("{}", shoot(&mut gpu, 2, "duel6-opening-waiting").display());
        if let Some(console) = gpu.console.as_mut() {
            console.preview_opening_state(Some("That holocron was already opened."));
        }
        println!("{}", shoot(&mut gpu, 2, "duel6-opening-refused").display());
        assert!(!overflowed(&gpu));
    });
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_cards_tab() {
    on_big_stack(|| {
        for (size, name) in [([1920, 1080], "1080p"), ([1440, 1080], "4x3")] {
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", size, Some(menu), &sjk_cvars())
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            if let Some(console) = gpu.console.as_mut() {
                console.preview_cards(every_kind(), 1_340);
                console.preview_collection(Some(Vec::new()), 3.1);
            }
            gpu.open_profile_hub(Tab::Cards);
            assert_eq!(gpu.profile_hub_tab(), Some(Tab::Cards));
            let _ = frame(&mut gpu, 120);
            // The Solar Rare, which the list puts near the start.
            if let Some(console) = gpu.console.as_mut() {
                console.collection_card_for_shot(1);
            }
            println!(
                "{}",
                shoot(&mut gpu, 4, &format!("duel6-cards-{name}")).display()
            );
            assert!(!overflowed(&gpu), "{name}");
        }
        let menu = menu::ClientMenu::new(true, String::new());
        let Some((mut gpu, _profile)) =
            open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &sjk_cvars())
        else {
            return;
        };
        let _ = frame(&mut gpu, 10);
        if let Some(console) = gpu.console.as_mut() {
            console.preview_cards(Vec::new(), 0);
        }
        gpu.open_profile_hub(Tab::Cards);
        let _ = frame(&mut gpu, 120);
        println!("{}", shoot(&mut gpu, 2, "duel6-cards-none").display());
    });
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_loot_table() {
    on_big_stack(|| {
        for (size, name) in [([1920, 1080], "1080p"), ([1440, 1080], "4x3")] {
            let Some(mut gpu) = open_holocrons(size) else {
                return;
            };
            if size[0] == 1920 {
                // The tab with its Loot table button.
                println!("{}", shoot(&mut gpu, 2, "duel6-loot-button").display());
            }
            if let Some(console) = gpu.console.as_mut() {
                console.preview_loot(true);
            }
            println!(
                "{}",
                shoot(&mut gpu, 2, &format!("duel6-loot-table-{name}")).display()
            );
            assert!(!overflowed(&gpu), "{name}");
        }
    });
}

/// The first card, revealed, in `finish`, beside sealed ones.
fn with_face(finish: &str) -> Vec<sjk_identity::CardStack> {
    let mut list = vec![sjk_identity::CardStack {
        card: Some("sol_lore_001".to_owned()),
        rarity: "legendary".to_owned(),
        finish: finish.to_owned(),
        edition: "alpha".to_owned(),
        count: 1,
        first: 1_791_830_000,
        last: 1_791_830_000,
    }];
    list.extend(every_kind().into_iter().take(5));
    list
}

/// Mount the test packs and wait for the first card's picture to decode; false (and a
/// line) when the packs have no such card.
fn faces_ready() -> bool {
    super::saber_skins::mount_test_packs();
    for _ in 0..200 {
        let faces = crate::cards::faces::current();
        match faces.find("sol_lore_001") {
            Some((index, _)) if faces.art(index).is_some() => return true,
            Some(_) => std::thread::sleep(std::time::Duration::from_millis(25)),
            None => break,
        }
    }
    println!("no sol_lore_001 face in SJK_TEST_PACKS: skipped");
    false
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_card_faces() {
    on_big_stack(|| {
        if !faces_ready() {
            return;
        }
        let mut images = Vec::new();
        for finish in ["holo", "normal", "solar"] {
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) =
                open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &sjk_cvars())
            else {
                return;
            };
            let _ = frame(&mut gpu, 10);
            let list = with_face(finish);
            // The revealed card, wherever the best-first order puts it.
            let revealed = crate::cards::stacks(&list)
                .iter()
                .position(|stack| !stack.sealed())
                .unwrap_or(0);
            if let Some(console) = gpu.console.as_mut() {
                console.preview_cards(list, 1_340);
                console.preview_collection(Some(Vec::new()), 3.1);
            }
            gpu.open_profile_hub(Tab::Cards);
            if let Some(console) = gpu.console.as_mut() {
                console.collection_card_for_shot(revealed);
            }
            let _ = frame(&mut gpu, 120);
            let path = shoot(&mut gpu, 4, &format!("duel6-card-face-{finish}"));
            println!("{}", path.display());
            assert!(!overflowed(&gpu), "{finish}");
            images.push(image::open(path).expect("the shot").to_rgba8());
        }
        println!(
            "{}",
            sheet(&images, 3, 640, "duel6-card-face-finishes").display()
        );
        // Tilted towards the pointer, up close: Holo, the pointer at the upper right.
        let menu = menu::ClientMenu::new(true, String::new());
        let Some((mut gpu, _profile)) =
            open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &sjk_cvars())
        else {
            return;
        };
        let _ = frame(&mut gpu, 10);
        let list = with_face("holo");
        let revealed = crate::cards::stacks(&list)
            .iter()
            .position(|stack| !stack.sealed())
            .unwrap_or(0);
        if let Some(console) = gpu.console.as_mut() {
            console.preview_cards(list, 1_340);
            console.preview_collection(Some(Vec::new()), 3.1);
        }
        gpu.open_profile_hub(Tab::Cards);
        let mut tilts = Vec::new();
        for (name, at) in [("right", [0.7, -0.5]), ("left", [-0.8, 0.6])] {
            if let Some(console) = gpu.console.as_mut() {
                console.collection_card_for_shot(revealed);
                console.collection_tilt_for_shot(0, at);
            }
            let _ = frame(&mut gpu, 120);
            let path = shoot(&mut gpu, 4, &format!("duel6-card-face-tilt-{name}"));
            println!("{}", path.display());
            assert!(!overflowed(&gpu), "{name}");
            tilts.push(image::open(path).expect("the shot").to_rgba8());
        }
        println!(
            "{}",
            sheet(&tilts, 2, 960, "duel6-card-face-tilts").display()
        );
        // Coming out of a Mythical Holocron.
        let Some(mut gpu) = open_holocrons([1920, 1080]) else {
            return;
        };
        let mut card = shown(3, 3, 1, 311);
        card.card = Some("sol_lore_001".to_owned());
        ceremony(&mut gpu, vec![card, shown(0, 0, 0, 14)], 3.0);
        let path = shoot(&mut gpu, 4, "duel6-card-face-opening");
        println!("{}", path.display());
        assert!(!overflowed(&gpu));
    });
}

/// A shader found by a holocron of tier `tier`: Tidal, held twice.
fn found_tidal(tier: usize) -> Prize {
    Prize::Shader(crate::console::holocrons_panel::opening::Found {
        holocron: 7,
        tier,
        skin: crate::unlockables::find("saber_tidal"),
        id: "saber_tidal".to_owned(),
        name: "Tidal blade".to_owned(),
        count: 2,
    })
}

/// A shader newer than this client, from a Rare holocron.
fn found_new() -> Prize {
    Prize::Shader(crate::console::holocrons_panel::opening::Found {
        holocron: 9,
        tier: 1,
        skin: None,
        id: "saber_comet".to_owned(),
        name: "Comet blade".to_owned(),
        count: 1,
    })
}

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_crate_spin() {
    on_big_stack(|| {
        let Some(mut gpu) = open_holocrons([1920, 1080]) else {
            return;
        };
        let mut moments = Vec::new();
        // The spin (negative: seconds before the burst), a card's and a shader's.
        for (elapsed, name, prize) in [
            (-4.2, "spin-start", found_tidal(1)),
            (-2.6, "spin-mid", found_tidal(1)),
            (-0.6, "spin-stop", found_tidal(1)),
            (3.0, "shader-words", found_tidal(1)),
            (-0.6, "card-stop", Prize::Card(shown(2, 3, 1, 96))),
            (-0.6, "new-stop", found_new()),
            (3.0, "new-words", found_new()),
        ] {
            if let Some(console) = gpu.console.as_mut() {
                console.preview_opening(Some(vec![prize]), elapsed, 3.1);
            }
            let path = shoot(&mut gpu, 2, &format!("duel6-crate-{name}"));
            assert!(!overflowed(&gpu), "{name}");
            println!("{}", path.display());
            moments.push(image::open(path).expect("the shot").to_rgba8());
        }
        println!(
            "{}",
            sheet(&moments, 2, 960, "duel6-crate-moments").display()
        );
    });
}

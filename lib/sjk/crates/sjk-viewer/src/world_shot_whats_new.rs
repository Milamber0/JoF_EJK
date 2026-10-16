//! World shots of the what's-new card over duel6 under the SJK UI: one release's recap
//! (the newest, as `whatsnew` previews it), an update that skipped a release (two pages,
//! both shot) and a recap with chips. Off-screen renders (no game window), ignored like the other world shots:
//! they need a GPU adapter and the installed game data named by `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer whats_new_shots --
//! --ignored --test-threads=1 --nocapture`; the pictures are in `target/world-shots`.

use super::*;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn whats_new_shots() {
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
        // The first frame reads the marker; a local build has no recap.
        let _ = frame(&mut gpu, 10);
        gpu.preview_whats_new();
        println!("{}", shoot(&mut gpu, 6, "duel6-whats-new").display());
        assert!(gpu.whats_new_card.is_open(), "the card shows");
        gpu.whats_new_card.offer("", &[], true);
        let _ = frame(&mut gpu, 2);
        assert!(!gpu.whats_new_card.is_open(), "Close puts it away");

        // An update that skipped two releases.
        let digests = crate::console::changelog::digests("2026.1011.3", "2026.1011.1");
        assert_eq!(digests.len(), 2, "{digests:?}");
        gpu.whats_new_card.offer("2026.1011.3", &digests, false);
        println!(
            "{}",
            shoot(&mut gpu, 6, "duel6-whats-new-skipped").display()
        );
        assert!(gpu.whats_new_card.is_open(), "the card shows");
        gpu.whats_new_card.turn(1);
        println!(
            "{}",
            shoot(&mut gpu, 6, "duel6-whats-new-skipped-2").display()
        );

        // A recap whose sentences name commands, drawn in chips.
        let digests = crate::console::changelog::digests("2026.1012.1", "2026.1011.3");
        assert_eq!(digests.len(), 1, "{digests:?}");
        gpu.whats_new_card.offer("2026.1012.1", &digests, false);
        println!("{}", shoot(&mut gpu, 6, "duel6-whats-new-chips").display());
        assert!(gpu.whats_new_card.is_open(), "the card shows");
    });
}

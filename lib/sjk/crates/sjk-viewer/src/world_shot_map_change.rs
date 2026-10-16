//! A world built during a map change, adopted after the window changed size: the
//! new map is built on a worker at the size the window had when the change began
//! (`session_transition.rs`), and the window can be resized during those seconds
//! (a player's 2026.1011.1 crash: its depth stayed 1280x720 under a 1920x1080
//! frame, and wgpu refused the first frame). Off-screen, ignored like the other
//! world shots: it needs a GPU adapter and the installed game data named by
//! `JKA_GAME_DATA`.
//!
//! Run: `JKA_GAME_DATA=<GameData> cargo test -p sjk-viewer map_change_after_resize --
//! --ignored --test-threads=1 --nocapture`.

use super::*;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn map_change_after_resize() {
    on_big_stack(|| {
        let screen = [1920, 1080];
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", screen, None, &[]) else {
            return;
        };
        let _ = frame(&mut gpu, 4);

        // The next map, built on the same context at the window's earlier size, the
        // way the world-install worker builds it (no console, no menu: the running
        // world hands its own over).
        let (mut input, _next_profile) = world_input("maps/mp/duel6.bsp", None, &[]);
        input.console = None;
        let built = pollster::block_on(GpuState::new_with_context(
            Arc::clone(&gpu.context),
            [1280, 720],
            input,
            None,
        ))
        .expect("the next map is built");
        assert_eq!([built.size.width, built.size.height], [1280, 720]);

        let mut adopted = gpu.adopt_world(built);
        assert_eq!(
            [adopted.size.width, adopted.size.height],
            screen,
            "the adopted world takes the window's present size"
        );
        // Frames now go to an image of the window's size, as they go to the swapchain.
        attach_target(&mut adopted, screen);
        let image = frame(&mut adopted, 4);
        assert_eq!(image.dimensions(), (screen[0], screen[1]));
    });
}

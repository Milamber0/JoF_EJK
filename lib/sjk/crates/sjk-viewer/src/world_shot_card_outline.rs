//! World shots of the player card's outline ([`crate::card_outline`]) on a recorded
//! game: the demo `SJK_TEST_DEMO` names is played without a server, the camera
//! orbits another player and their card is pinned, so the ring shows around them.
//! Ignored like the other world shots; skipped when `SJK_TEST_DEMO` is not set.

use super::*;

#[test]
#[ignore = "renders with the GPU, the installed game data and the demo SJK_TEST_DEMO names"]
fn card_outline_on_a_demo() {
    on_big_stack(|| {
        let Some(demo) = std::env::var_os("SJK_TEST_DEMO").map(PathBuf::from) else {
            println!("SJK_TEST_DEMO is not set: skipped");
            return;
        };
        // Someone other than the recorder, as the card never shows for oneself.
        let probe = demo_playback::Session::open(&demo, demo_playback::Camera::FirstPerson, 60)
            .expect("the demo");
        let own = probe.latest_snapshot().player.client_num();
        let players: Vec<u16> = probe
            .latest_snapshot()
            .entities
            .iter()
            .map(|entity| entity.number())
            .filter(|number| *number < 32 && *number != own)
            .collect();
        println!("recorder {own}, other players {players:?}");
        let Some(&target) = players.first() else {
            println!("no other player in the demo's first snapshot: skipped");
            return;
        };
        for (name, camera) in [
            ("card_outline_orbit", demo_playback::Camera::Orbit(target)),
            ("card_outline_look", demo_playback::Camera::LookAt(target)),
        ] {
            let Some(mut gpu) = open_demo(&demo, camera, [1600, 900]) else {
                return;
            };
            let _ = frame(&mut gpu, 20);
            gpu.hud.card.pin_for_shot(target);
            println!("{}", shoot(&mut gpu, 40, name).display());
        }
    });
}

/// A windowless client playing `demo` with `camera`, its map and content from the
/// demo's game state, as `sjk --demo` starts.
pub(super) fn open_demo(
    demo: &std::path::Path,
    camera: demo_playback::Camera,
    size: [u32; 2],
) -> Option<GpuState> {
    let game_data = PathBuf::from(
        std::env::var_os("JKA_GAME_DATA").expect("JKA_GAME_DATA names the GameData directory"),
    );
    let session = demo_playback::Session::open(demo, camera, 60).expect("the demo");
    let map = crate::app_launch::map_path_from_gamestate(session.game_state()).expect("a map");
    let selection = assets::session_content::Selection::from_game(session.game_state())
        .expect("the demo's content");
    let vfs = selection.mount(&game_data).expect("the content");
    let bsp = selection.load_bsp(&vfs, &map).expect("the map");
    let (mut input, directory) = world_input("maps/mp/duel6.bsp", None, &[]);
    input.scene = StaticWorld::build(&bsp, MeshBuildOptions::default().with_sky_surfaces())
        .expect("the map's meshes");
    input.shaders = assets::load_shaders(&vfs);
    let bounds = bsp.render().models()[0].clone();
    (input.world_minimums, input.world_maximums) = (bounds.minimums, bounds.maximums);
    (input.camera_origin, input.camera_yaw) = assets::initial_camera(&bsp).expect("a camera");
    input.bsp = bsp;
    input.vfs = Arc::new(vfs);
    input.demo_session = Some(session);
    let mut gpu = match pollster::block_on(GpuState::new_for_target(None, size, input)) {
        Ok(gpu) => gpu,
        Err(error) => {
            eprintln!("no windowless GPU state ({error}); skipped");
            return None;
        }
    };
    attach_target(&mut gpu, size);
    // The profile only has to outlive the client's start.
    std::mem::forget(directory);
    Some(gpu)
}

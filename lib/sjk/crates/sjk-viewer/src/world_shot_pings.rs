//! World shots of world pings (`world_ping.rs`) over duel6: pings placed where the
//! camera's rays meet the map, one just made (its pulse), a teammate's, the player's
//! own and one behind the camera on the screen's border. Ignored like the other world
//! shots.

use super::*;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_world_pings() {
    on_big_stack(|| {
        for (size, name) in [
            ([1920, 1080], "world_pings_1080p"),
            ([3840, 2160], "world_pings_4k"),
        ] {
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, None, &[]) else {
                return;
            };
            gpu.is_menu_world = false;
            let _ = frame(&mut gpu, 10);
            let camera = gpu.frame_view.expect("a frame's view");
            let forward = (camera.target - camera.eye).normalize();
            let right = forward.cross(camera.up).normalize();
            let up = right.cross(forward);
            let hit = |gpu: &mut GpuState, direction: Vec3| {
                let trace = gpu.bsp.trace_box_with(
                    &mut gpu.trace_scratch,
                    camera.eye.to_array(),
                    (camera.eye + direction.normalize() * world_ping::REACH).to_array(),
                    Aabb::new([0.0; 3], [0.0; 3]).unwrap(),
                    1,
                );
                Vec3::from_array(trace.end_position)
            };
            let now = Instant::now();
            let ago = |ms| now - Duration::from_millis(ms);
            let views = [
                (forward, 3, false, "^2Sol", 150),
                (
                    forward + right * 0.35 - up * 0.12,
                    4,
                    false,
                    "^1Fox^7|^3JoF",
                    2500,
                ),
                (forward - right * 0.4 - up * 0.2, 0, true, "^7Visitor", 900),
            ];
            for (direction, slot, own, who, age) in views {
                let point = hit(&mut gpu, direction);
                gpu.world_pings.place(point, slot, own, who, ago(age));
            }
            // Behind and to the right: on the border.
            let behind = hit(&mut gpu, -forward + right * 0.5);
            gpu.world_pings.place(behind, 5, false, "Kyle", ago(4000));
            gpu.world_pings.forced = true;
            println!("{}", shoot(&mut gpu, 1, name).display());
        }
    });
}

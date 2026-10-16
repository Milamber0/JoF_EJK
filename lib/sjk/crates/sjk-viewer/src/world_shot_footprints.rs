//! World shots of footprints ([`crate::footprints`]) on `mp/siege_desert`'s untagged
//! sand (`textures/siege/siege2sand`), where Sol asked for sand steps and prints (world
//! note, 07/10/2026): a walk of steps placed through the same ground trace and print as
//! a player's feet, seen from the note's view and from above, then the same walk with
//! the texture-name guess off (stone: no prints). Ignored like the other world shots.

use super::*;
use glam::Vec3;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn siege_desert_footprints() {
    on_big_stack(|| {
        let Some((mut gpu, _profile)) = open("maps/mp/siege_desert.bsp", [1280, 720], None, &[])
        else {
            return;
        };
        gpu.is_menu_world = false;
        // Sol's note: from (-8925, -481, -303) looking at sand at (-8545, -344, -512).
        let views: [([f32; 3], f32, f32); 3] = [
            ([-8925.0, -481.0, -303.0], 19.8, -27.4),
            ([-8640.0, -344.0, -440.0], 0.0, -40.0),
            ([-8500.0, -344.0, -482.0], 0.0, -89.0),
        ];
        let mut images = Vec::new();
        for guess in [true, false] {
            // The first walk's prints would still show for ten seconds.
            gpu.effect_aux.decals = Default::default();
            let mut prints = 0;
            // Ten steps along +X, the feet 8 units either side, as a walk leaves them.
            for step in 0..10 {
                let right = step % 2 == 1;
                let side = if right { -8.0 } else { 8.0 };
                let foot = Vec3::new(-8590.0 + step as f32 * 18.0, -344.0 + side, -512.0);
                let Some(ground) = crate::footprints::ground(
                    &gpu.bsp,
                    &mut gpu.trace_scratch,
                    foot + Vec3::Z * 15.0,
                    guess,
                ) else {
                    continue;
                };
                if let Some(request) = crate::footprints::print(&ground, right, false, 0.0) {
                    gpu.effect_aux.decals.request_mark(request);
                    prints += 1;
                }
                if step == 9 {
                    println!(
                        "guess {guess}: ground family {} at {:?}",
                        ground.family, ground.point
                    );
                }
            }
            println!(
                "guess {guess}: {prints} prints; footstep_r layers {:?}",
                gpu.particle_atlas
                    .layers_for("footstep_r", 0.0)
                    .iter()
                    .map(|layer| (layer.blend, layer.uv_transform))
                    .collect::<Vec<_>>()
            );
            for (eye, yaw, pitch) in views {
                aim(&mut gpu, eye, yaw, pitch);
                images.push(frame(&mut gpu, 6));
                println!(
                    "marks (polys, pending): {:?}",
                    gpu.effect_aux.decals.mark_counts()
                );
            }
            if guess {
                assert_eq!(prints, 10, "every step on siege2sand leaves a print");
            } else {
                assert_eq!(prints, 0, "untagged sand is stone without the guess");
            }
        }
        println!(
            "{}",
            sheet(&images, 3, 960, "siege-desert-footprints").display()
        );
    });
}

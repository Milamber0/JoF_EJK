//! World shots of movers shadowing lamps (`mover_occlusion.rs`). For each chosen mover,
//! a camera between one of its door lamps and the mover looks past the mover to where
//! the lamp's light falls; the sheet shows it drawn at its spawn pose (closed, shadowing),
//! hidden (open) and every mover hidden, from two sides. Ignored like the other world shots.
//!
//! `SJK_MOVER_MAP` names the map (`maps/mp/siege_hoth.bsp` by default) and `SJK_MOVERS`
//! the occluders to shoot (`0,3`; the four with the most door lamps without it). Run once
//! more with `SJK_MOVER_OCCLUSION=0` for the same views without mover shadows: that run
//! has no door lamps, so it takes the views the first run saved beside the sheets.
//! Floor mirrors and eye adaptation are off, so the two runs differ only in lamp light.
//!
//! `SJK_MOVER_TIMING=1` (with `SJK_GPU_PHASES=1`, in a release build) also holds the
//! map's start camera for 256 frames, then the first view for 256 frames, then shows and hides its mover every frame for 256 more:
//! the `door-tiles` phase is the tracer's cost, `light-pass` the receivers'.

use super::*;
use glam::Vec3;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn movers_shadow_lamps() {
    on_big_stack(|| {
        let map = std::env::var("SJK_MOVER_MAP").unwrap_or("maps/mp/siege_hoth.bsp".to_owned());
        // Floor mirrors would show a lit doorway whatever the lamps do.
        let Some((mut gpu, _profile)) = open(
            &map,
            [960, 540],
            None,
            &[("r_floorReflections", "0"), ("r_autoExposure", "0")],
        ) else {
            return;
        };
        MOVERS_PINNED.with(|pinned| pinned.set(true));
        // Spawn origins by inline model, where the movers stand closed.
        let origins: std::collections::HashMap<usize, Vec3> =
            sjk_entity::parse_entity_lump(gpu.bsp.entities())
                .unwrap_or_default()
                .iter()
                .filter_map(|entity| {
                    let model = entity.get("model")?.strip_prefix('*')?.parse().ok()?;
                    let origin = entity.vector("origin").ok().flatten()?;
                    Some((model, Vec3::from_array(origin)))
                })
                .collect();
        let occluders: Vec<(usize, Vec3, Vec3, Vec3)> = gpu
            .world_materials
            .mover_occluders
            .iter()
            .map(|o| {
                let rest = gpu.mover_catalog.meshes[o.mesh]
                    .model_index
                    .and_then(|model| origins.get(&model).copied())
                    .unwrap_or_default();
                (o.mesh, o.lower, o.upper, rest)
            })
            .collect();
        let models: Vec<Option<usize>> = occluders
            .iter()
            .map(|&(mesh, ..)| gpu.mover_catalog.meshes[mesh].model_index)
            .collect();
        // `Some(usize::MAX)` hides them all.
        let present = |hidden: Option<usize>| -> Vec<crate::movers::Presented> {
            occluders
                .iter()
                .enumerate()
                .filter_map(|(index, &(_, _, _, rest))| {
                    Some(crate::movers::Presented {
                        entity_number: 100 + index as u16,
                        model_index: models[index]?,
                        origin: rest.to_array(),
                        angles: [0.; 3],
                        rotation: [0., 0., 0., 1.],
                        visible: hidden != Some(index) && hidden != Some(usize::MAX),
                    })
                })
                .collect()
        };
        let closed = present(None);
        gpu.movers.clone_from(&closed);
        // Place every mover and let the queued door tiles drain.
        for _ in 0..600 {
            let _ = frame(&mut gpu, 1);
            if !(0..occluders.len()).any(|i| gpu.world_materials.door_lamps(i).1) {
                break;
            }
        }
        let chosen: Vec<usize> = match std::env::var("SJK_MOVERS") {
            Ok(list) => list.split(',').filter_map(|n| n.parse().ok()).collect(),
            Err(_) => {
                let mut by_lamps: Vec<usize> = (0..occluders.len()).collect();
                by_lamps
                    .sort_by_key(|&i| std::cmp::Reverse(gpu.world_materials.door_lamps(i).0.len()));
                by_lamps.truncate(4);
                by_lamps
            }
        };
        let off = std::env::var_os("SJK_MOVER_OCCLUSION").is_some_and(|v| v == "0");
        if std::env::var_os("SJK_MOVER_TIMING").is_some() {
            // The map's own start camera: an ordinary view, not chosen for its doors.
            eprintln!("timing: start");
            let _ = frame(&mut gpu, 256);
        }
        let views_file = directory().join("movers-views.txt");
        // Per occluder: two (eye, at) views.
        let mut views: Vec<(usize, [Vec3; 4])> = Vec::new();
        if off {
            let text = std::fs::read_to_string(&views_file).expect("the first run's views");
            for line in text.lines() {
                let numbers: Vec<f32> = line
                    .split_whitespace()
                    .filter_map(|n| n.parse().ok())
                    .collect();
                if numbers.len() == 13 {
                    let v = |i: usize| Vec3::new(numbers[i], numbers[i + 1], numbers[i + 2]);
                    views.push((numbers[0] as usize, [v(1), v(4), v(7), v(10)]));
                }
            }
        } else {
            for &index in &chosen {
                let Some(&(_, lower, upper, rest)) = occluders.get(index) else {
                    continue;
                };
                let centre = rest + (lower + upper) * 0.5;
                let (lamps, _) = gpu.world_materials.door_lamps(index);
                // The door lamp nearest the mover.
                let Some(lamp) = lamps
                    .iter()
                    .copied()
                    .min_by(|a, b| a.distance(centre).total_cmp(&b.distance(centre)))
                else {
                    continue;
                };
                let toward = (centre - lamp).normalize_or_zero();
                let side = toward.cross(Vec3::Z).normalize_or(Vec3::X);
                let reach = (upper - lower).max_element().max(64.);
                // Beside the line from the lamp, looking past the mover.
                let eye = |sign: f32| lamp + (centre - lamp) * 0.4 + side * sign * reach * 0.8;
                let at = centre + toward * reach;
                views.push((index, [eye(1.), at, eye(-1.), at]));
            }
            let text: String = views
                .iter()
                .map(|(index, v)| {
                    let mut line = index.to_string();
                    for point in v {
                        line.push_str(&format!(" {} {} {}", point.x, point.y, point.z));
                    }
                    line + "\n"
                })
                .collect();
            std::fs::create_dir_all(directory()).expect("the shot directory");
            std::fs::write(&views_file, text).expect("save the views");
        }
        for (index, view) in views {
            let mesh = occluders[index].0;
            let mut images = Vec::new();
            for [eye, at] in [[view[0], view[1]], [view[2], view[3]]] {
                let (yaw, pitch) = look(eye.to_array(), at.to_array());
                aim(&mut gpu, eye.to_array(), yaw, pitch);
                for hidden in [None, Some(index), Some(usize::MAX)] {
                    gpu.movers.clone_from(&present(hidden));
                    let _ = frame(&mut gpu, 4);
                    for _ in 0..200 {
                        if !gpu.world_materials.door_lamps(index).1 {
                            break;
                        }
                        let _ = frame(&mut gpu, 1);
                    }
                    // GI probes refresh a few hundred a frame: let the bounce settle too.
                    images.push(frame(&mut gpu, 400));
                }
            }
            if std::env::var_os("SJK_MOVER_TIMING").is_some() && images.len() == 6 {
                let (yaw, pitch) = look(view[0].to_array(), view[1].to_array());
                aim(&mut gpu, view[0].to_array(), yaw, pitch);
                gpu.movers.clone_from(&present(None));
                eprintln!("timing: still");
                let _ = frame(&mut gpu, 256);
                eprintln!("timing: moving");
                for step in 0..256 {
                    gpu.movers
                        .clone_from(&present((step % 2 == 0).then_some(index)));
                    let _ = frame(&mut gpu, 1);
                }
                eprintln!("timing: done");
            }
            let name = format!(
                "movers-{}-{index:02}{}",
                map.trim_start_matches("maps/")
                    .trim_end_matches(".bsp")
                    .replace('/', "-"),
                if off { "-off" } else { "" }
            );
            println!(
                "{index}: mesh {mesh} {}",
                sheet(&images, 3, 640, &name).display()
            );
        }
    });
}

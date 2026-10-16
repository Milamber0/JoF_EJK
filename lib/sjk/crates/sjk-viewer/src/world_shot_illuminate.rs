//! World shots of Illuminate's holocron ([`crate::illuminate`]): it floats in front
//! of the camera at a map's first spawn point, without a session (the shots give it
//! an eye to float by). Ignored like the other world shots.
//!
//! `SJK_HOLOCRON_MAPS` names the maps (`duel6,ffa3`; default duel6). Each map gives
//! one sheet of four views: the spawn's view with the holocron out, the same with
//! it lit, close up, and from a step back and up, where the light's reach shows.

use super::*;

const VIEWS: [(&str, Vec3, Vec3); 3] = [
    // Name, camera from the holocron in its yaw frame, and where it looks at.
    ("lit", Vec3::new(-34.0, -6.0, 2.0), Vec3::ZERO),
    ("close", Vec3::new(-13.0, -3.0, 3.0), Vec3::ZERO),
    (
        "reach",
        Vec3::new(-150.0, -40.0, 70.0),
        Vec3::new(40.0, 0.0, -40.0),
    ),
];

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn holocron_at_the_first_spawn() {
    on_big_stack(|| {
        let maps = std::env::var("SJK_HOLOCRON_MAPS").unwrap_or_else(|_| "duel6".to_owned());
        for map in maps.split(',').map(str::trim).filter(|map| !map.is_empty()) {
            shoot_map(map);
        }
    });
}

fn shoot_map(map: &str) {
    let Some((mut gpu, _profile)) = open(&format!("maps/mp/{map}.bsp"), [1280, 720], None, &[])
    else {
        return;
    };
    // The spawn's view (32 units over its point, yaw in radians); the holocron 34
    // units ahead of it, a little left and down.
    let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
    let turn = glam::Quat::from_rotation_z(yaw);
    let eye = Vec3::from_array(spawn);
    let holocron = eye + turn * Vec3::new(34.0, 6.0, -2.0);
    let anchor = holocron - crate::illuminate::Holocron::place(Vec3::ZERO, yaw);
    // Third person, where the cube is drawn (the flag follows the choice each frame).
    gpu.third_person_choice = true;
    gpu.illuminate.shot_anchor = Some((anchor, yaw));
    let (look_yaw, look_pitch) = look(eye.to_array(), holocron.to_array());
    aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
    let mut images = vec![frame(&mut gpu, 40)];
    gpu.illuminate.toggle();
    for (index, (_, from, at)) in VIEWS.iter().enumerate() {
        let camera = holocron + turn * *from;
        let target = holocron + turn * *at;
        let (look_yaw, look_pitch) = look(camera.to_array(), target.to_array());
        aim(&mut gpu, camera.to_array(), look_yaw, look_pitch);
        images.push(frame(&mut gpu, if index == 0 { 40 } else { 12 }));
    }
    println!("{map}: holocron at {holocron:?}");
    println!(
        "{}",
        sheet(&images, 2, 960, &format!("holocron-{map}")).display()
    );
}

/// Other players' holocrons (`illuminate::Others`) on duel6, without a session: two
/// lit players (slots 3 and 5) stand ahead of the first spawn's view facing it, so each
/// holocron floats by their left shoulder, on the camera's right. One sheet: the
/// spawn's view before they light, lit, from a step back and up, and close by the
/// nearer one.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn other_players_holocrons_on_duel6() {
    on_big_stack(|| {
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let facing = yaw + std::f32::consts::PI;
        let players = [
            (3, eye + turn * Vec3::new(70.0, 30.0, 0.0)),
            (5, eye + turn * Vec3::new(120.0, -35.0, 0.0)),
        ];
        let target = eye + turn * Vec3::new(100.0, 0.0, -20.0);
        let (look_yaw, look_pitch) = look(eye.to_array(), target.to_array());
        aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
        let mut images = vec![frame(&mut gpu, 40)];
        gpu.illuminate_others.shot_players = players
            .iter()
            .map(|&(slot, at)| (slot, at, facing))
            .collect();
        images.push(frame(&mut gpu, 40));
        let back = eye + turn * Vec3::new(-60.0, -30.0, 60.0);
        let (look_yaw, look_pitch) = look(back.to_array(), target.to_array());
        aim(&mut gpu, back.to_array(), look_yaw, look_pitch);
        images.push(frame(&mut gpu, 12));
        let near = crate::illuminate::Holocron::place(players[0].1, facing);
        let close = near + turn * Vec3::new(-30.0, 8.0, 4.0);
        let (look_yaw, look_pitch) = look(close.to_array(), near.to_array());
        aim(&mut gpu, close.to_array(), look_yaw, look_pitch);
        images.push(frame(&mut gpu, 12));
        for (slot, at) in players {
            let holocron = crate::illuminate::Holocron::place(at, facing);
            println!("slot {slot}: eye {at:?}, holocron {holocron:?}");
        }
        println!(
            "{}",
            sheet(&images, 2, 960, "holocron-others-duel6").display()
        );
    });
}

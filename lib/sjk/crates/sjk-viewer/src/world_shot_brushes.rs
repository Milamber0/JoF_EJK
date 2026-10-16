//! World shots of the invisible-brush view ([`crate::brush_view`]), without a
//! session. Ignored like the other world shots.
//!
//! `SJK_BRUSH_MAPS` names the maps (`ffa3,duel6`; default ffa3). Each map gives
//! one sheet: the first spawn's view, then one view of each kind the map has,
//! from a step back and up. The kinds and their face counts are printed.
//! `SJK_BRUSH_MODE` sets `r_showBrushes` (default 1); views are of the kinds it shows.

use super::*;

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn invisible_brushes() {
    on_big_stack(|| {
        let maps = std::env::var("SJK_BRUSH_MAPS").unwrap_or_else(|_| "ffa3".to_owned());
        for map in maps.split(',').map(str::trim).filter(|map| !map.is_empty()) {
            shoot_map(map);
        }
    });
}

fn shoot_map(map: &str) {
    let mode = std::env::var("SJK_BRUSH_MODE").unwrap_or_else(|_| "1".to_owned());
    let Some((mut gpu, _profile)) = open(
        &format!("maps/mp/{map}.bsp"),
        [1280, 720],
        None,
        &[(crate::brush_view::CVAR, &mode)],
    ) else {
        return;
    };
    gpu.is_menu_world = false;
    let shown = crate::brush_view::kinds::KindSet::chosen(mode.parse().unwrap_or(1), "");
    let mut sites = crate::brush_view::kind_sites(&gpu.bsp);
    sites.retain(|(kind, ..)| shown.contains(*kind));
    for (kind, faces, at) in &sites {
        println!("{map}: {} {faces} faces, e.g. at {at:?}", kind.name());
    }
    let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
    aim(&mut gpu, spawn, yaw.to_degrees(), 0.0);
    let mut images = vec![frame(&mut gpu, 40)];
    for (_, _, at) in sites.iter().take(11) {
        let camera = *at + Vec3::new(-220.0, -160.0, 120.0);
        let (look_yaw, look_pitch) = look(camera.to_array(), at.to_array());
        aim(&mut gpu, camera.to_array(), look_yaw, look_pitch);
        images.push(frame(&mut gpu, 12));
    }
    println!(
        "{}",
        sheet(&images, 3, 800, &format!("brushes-{map}-{mode}")).display()
    );
}

//! World shots of the Sun blade ([`crate::saber_skins`]) on duel6, without a session:
//! lit blades stand in front of the first spawn's view, the Sun between a stock orange
//! and a stock blue blade, at several animation times, then the Sun alone up close; and
//! the player screen's model holding the Sun blade (`cg_saberSkin saber_sun`, owned as
//! a hub profile listing it would make it). Ignored like the other world shots.
//!
//! The Sun's look is not in this repository: it comes from the SJK hub's pack. The shots
//! mount the packs of the folder `SJK_TEST_PACKS` names (built from the hub's sources),
//! with no hub; without it the skin's blades are the stock blue they fall back to.

use super::*;
use crate::saber::Blade;
use crate::saber_rgb::BladeColor;
use crate::saber_skins::ShotColor;

/// Mount the packs of the folder `SJK_TEST_PACKS` names, as the cache folder would be
/// at start; say so when it is not set.
pub(super) fn mount_test_packs() {
    match std::env::var_os("SJK_TEST_PACKS") {
        Some(directory) => {
            let skins = crate::sjk_packs::mount(std::path::Path::new(&directory));
            println!(
                "test packs from {}: blade skins {:?}",
                std::path::Path::new(&directory).display(),
                skins.ids().collect::<Vec<_>>()
            );
        }
        None => println!("SJK_TEST_PACKS is not set: blade skins are not loaded"),
    }
}

/// Animation times the side-by-side views are drawn at, seconds.
const TIMES: [f64; 4] = [1.0, 2.6, 4.3, 7.9];

#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sun_blade() {
    on_big_stack(|| {
        mount_test_packs();
        side_by_side();
        held_on_the_stage();
    });
}

fn side_by_side() {
    let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
        return;
    };
    let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
    let turn = glam::Quat::from_rotation_z(yaw);
    let eye = Vec3::from_array(spawn);
    let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
    // Stock single-saber blades (40 units, radius 3), a little tilted as if held.
    let blade = |at: Vec3, lean: f32| Blade {
        base: at.to_array(),
        direction: (turn * Vec3::new(0.0, lean, 1.0)).normalize().to_array(),
        length: 40.0,
        radius: 3.0,
    };
    gpu.saber_skins.shot_blades = vec![
        (
            blade(ahead(48.0, 13.0, -22.0), 0.12),
            ShotColor::Stock(BladeColor::Retail(crate::saber::Color::Orange)),
            1,
        ),
        (
            blade(ahead(48.0, 0.0, -22.0), 0.0),
            ShotColor::Skin("saber_sun"),
            2,
        ),
        (
            blade(ahead(48.0, -13.0, -22.0), -0.12),
            ShotColor::Stock(BladeColor::Retail(crate::saber::Color::Blue)),
            3,
        ),
    ];
    // Looking down a little, so the blades stand over the shaded floor.
    let target = ahead(48.0, 0.0, -14.0);
    let (look_yaw, look_pitch) = look(eye.to_array(), target.to_array());
    aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
    let mut images = Vec::new();
    for (index, seconds) in TIMES.into_iter().enumerate() {
        gpu.saber_skins.shot_seconds = Some(seconds);
        images.push(frame(&mut gpu, if index == 0 { 40 } else { 4 }));
    }
    println!("{}", sheet(&images, 2, 960, "duel6-sun-blade").display());
    // Up close: the Sun alone, lying across the view over the floor, every 0.3 s.
    gpu.saber_skins.shot_blades = vec![(
        Blade {
            base: ahead(44.0, 20.0, -26.0).to_array(),
            direction: (turn * Vec3::new(0.0, -1.0, 0.12)).normalize().to_array(),
            length: 40.0,
            radius: 3.0,
        },
        ShotColor::Skin("saber_sun"),
        2,
    )];
    let camera = ahead(18.0, 0.0, -8.0);
    let (look_yaw, look_pitch) = look(camera.to_array(), ahead(44.0, 0.0, -24.0).to_array());
    aim(&mut gpu, camera.to_array(), look_yaw, look_pitch);
    let mut close = Vec::new();
    for step in 0..8 {
        let seconds = 3.0 + f64::from(step) * 0.3;
        gpu.saber_skins.shot_seconds = Some(seconds);
        close.push(frame(&mut gpu, 4));
    }
    println!(
        "{}",
        sheet(&close, 2, 960, "duel6-sun-blade-close").display()
    );
}

/// The player screen's model holding the Sun blade, as the Character page shows it.
fn held_on_the_stage() {
    let menu = menu::ClientMenu::new(true, String::new());
    let cvars = [
        ("ui_menuStyle", "sjk"),
        (crate::settings::quick::HIDE_CVAR, "1"),
        (crate::unlockables::SABER_SKIN_CVAR, "saber_sun"),
    ];
    let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1920, 1080], Some(menu), &cvars)
    else {
        return;
    };
    // The shots have no hub: they own the unlock, as a profile listing it would.
    gpu.looks.shot_owns_unlocks = true;
    let _ = frame(&mut gpu, 4);
    if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
        menu.open_player(console, crate::player_menu::ReturnTarget::MainMenu);
    }
    let _ = frame(&mut gpu, 2);
    gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
    println!("{}", shoot(&mut gpu, 8, "duel6-sun-blade-held").display());
}

/// The SJK UI Profile screen's Saber tab from the main page, on the menu map's stage:
/// the hilt search being typed ("ka") with its matches, and the blade choice offering
/// the stock blade and every blade skin (made-up grants; the Storm worn, the model
/// holding it); then a kept search that finds nothing with no skin owned; then Dual
/// with every skin. At 1080p, with the menu text at its largest (`ui_textScale` 1.2),
/// and at 4K with it, as Sol plays. No canvas runs out of room.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_sjk_saber_page() {
    on_big_stack(|| {
        mount_test_packs();
        let unlocks = || {
            crate::unlockables::blade_skins()
                .map(|skin| sjk_identity::Unlock {
                    id: skin.id.to_owned(),
                    granted: 1_791_336_225,
                    note: String::new(),
                    count: 1,
                    medal: None,
                })
                .collect::<Vec<_>>()
        };
        for (size, scale, suffix) in [
            ([1920, 1080], None, ""),
            ([1920, 1080], Some("1.2"), "-text-1.2"),
            ([3840, 2160], Some("1.2"), "-4k"),
        ] {
            let mut cvars = vec![
                ("ui_menuStyle", "sjk"),
                (crate::settings::quick::HIDE_CVAR, "1"),
                (crate::unlockables::SABER_SKIN_CVAR, "saber_storm"),
                ("name", "^1Sol^7Vulpes"),
            ];
            if let Some(scale) = scale {
                cvars.push((crate::text::style::SCALE_CVAR, scale));
            }
            let menu = menu::ClientMenu::new(true, String::new());
            let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", size, Some(menu), &cvars)
            else {
                return;
            };
            // The shots have no hub: they own the unlocks, as a profile listing them would.
            gpu.looks.shot_owns_unlocks = true;
            let _ = frame(&mut gpu, 4);
            if let (Some(menu), Some(console)) = (gpu.client_menu.as_mut(), gpu.console.as_ref()) {
                menu.open_player_hub(console, crate::player_menu::ReturnTarget::MainMenu);
            }
            // Search typed, then (the same rows: Style, Search, Hilt, Blade) the
            // search kept finding nothing and no skin, then Dual on its blade row.
            let steps: [(&str, bool, bool, usize, &str); 3] = [
                ("ka", true, true, 1, "search"),
                ("zzz", false, false, 3, "none"),
                ("", false, true, 3, "dual"),
            ];
            for (search, typing, owned, row, part) in steps {
                if let (Some(menu), Some(console)) =
                    (gpu.client_menu.as_mut(), gpu.console.as_ref())
                {
                    if part == "dual" {
                        menu.player_dual_for_shot();
                    }
                    menu.player_page_for_shot(1, row);
                    let owned = if owned { unlocks() } else { Vec::new() };
                    menu.player_saber_for_shot(console, search, typing, owned);
                }
                let _ = frame(&mut gpu, 2);
                gpu.ui_epoch -= std::time::Duration::from_millis(3_000);
                let name = format!("duel6-saber-{part}{suffix}");
                println!("{}", shoot(&mut gpu, 30, &name).display());
                assert!(
                    !gpu.client_menu
                        .as_ref()
                        .is_some_and(menu::ClientMenu::player_overflowed),
                    "{name}"
                );
            }
        }
    });
}

/// The second set of saber shaders (10/10/2026), each id with what its close-up shows.
const SECOND_SET: [&str; 9] = [
    "saber_unstable",
    "saber_molten",
    "saber_spectral",
    "saber_glitch",
    "saber_hologram",
    "saber_runic",
    "saber_chameleon",
    "saber_banner",
    "saber_heartbeat",
];

/// The second set of saber shaders on duel6: all nine standing side by side at two
/// times, then each alone up close, lying across the view over the floor, at eight
/// moments (sheets `duel6-blade-<name>`). The Spectral's afterimages trail a swing down
/// from above, the Runic spells "SolVulpes", the Banner is shown outside a team, red and
/// blue, the Chameleon in two places' light, the Heartbeat through a beat.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_second_blades() {
    on_big_stack(|| {
        mount_test_packs();
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
        gpu.saber_skins.shot_persona = crate::saber_persona::Persona::of(b"^1Sol^7Vulpes", 0);
        // All nine standing, a little tilted as if held.
        gpu.saber_skins.shot_blades = SECOND_SET
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let offset = (index as f32 - 4.0) * 8.5;
                (
                    Blade {
                        base: ahead(70.0, -offset, -24.0).to_array(),
                        direction: (turn * Vec3::new(0.0, -0.006 * offset, 1.0))
                            .normalize()
                            .to_array(),
                        length: 40.0,
                        radius: 3.0,
                    },
                    ShotColor::Skin(id),
                    index as u32 + 1,
                )
            })
            .collect();
        let (look_yaw, look_pitch) = look(eye.to_array(), ahead(70.0, 0.0, -6.0).to_array());
        aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
        let mut lineup = Vec::new();
        for (index, seconds) in [2.6, 5.15].into_iter().enumerate() {
            gpu.saber_skins.shot_seconds = Some(seconds);
            lineup.push(frame(&mut gpu, if index == 0 { 40 } else { 4 }));
        }
        println!(
            "{}",
            sheet(&lineup, 1, 1280, "duel6-blades-lineup").display()
        );
        // Each alone, up close.
        let lying = |at: Vec3, down: f32| Blade {
            base: at.to_array(),
            direction: (turn * Vec3::new(0.0, -1.0, down)).normalize().to_array(),
            length: 40.0,
            radius: 3.0,
        };
        // Closer than the Sun's, so the glyphs, veins and scan lines read.
        let camera = ahead(27.0, 0.0, -15.0);
        let (look_yaw, look_pitch) = look(camera.to_array(), ahead(44.0, 0.0, -24.0).to_array());
        for id in SECOND_SET {
            let blade = lying(ahead(44.0, 20.0, -26.0), 0.12);
            gpu.saber_skins.shot_blades = vec![(blade, ShotColor::Skin(id), 2)];
            aim(&mut gpu, camera.to_array(), look_yaw, look_pitch);
            let mut close = Vec::new();
            for step in 0..8 {
                let seconds = 3.0 + f64::from(step) * 0.13;
                gpu.saber_skins.shot_seconds = Some(seconds);
                gpu.saber_skins.shot_ghosts.clear();
                gpu.saber_skins.shot_persona =
                    crate::saber_persona::Persona::of(b"^1Sol^7Vulpes", 0);
                match id {
                    // A swing down from above: the poses it passed, 45 ms apart.
                    "saber_spectral" => {
                        let fade = [0.45_f32, 0.2, 0.09, 0.04];
                        for (back, light) in fade.into_iter().enumerate() {
                            let lift = (back as f32 + 1.0) * 3.5 * (1.0 + step as f32 * 0.1);
                            gpu.saber_skins.shot_ghosts.push((
                                lying(ahead(44.0, 20.0, -26.0 + lift), 0.12 + 0.05 * back as f32),
                                id,
                                light,
                            ));
                        }
                    }
                    // Outside a team, then red, then blue.
                    "saber_banner" => {
                        let team = [0, 0, 0, 1, 1, 1, 2, 2][step as usize];
                        gpu.saber_skins.shot_persona =
                            crate::saber_persona::Persona::of(b"Sol", team);
                    }
                    _ => {}
                }
                close.push(frame(&mut gpu, 4));
            }
            let name = format!("duel6-blade-{}", id.trim_start_matches("saber_"));
            println!("{}", sheet(&close, 2, 960, &name).display());
        }
        // The Chameleon elsewhere: its colour is the light's where it is.
        let mut places = Vec::new();
        for (forward, side, height) in [
            (44.0, 20.0, -26.0),
            (160.0, -120.0, -10.0),
            (-200.0, 90.0, 0.0),
        ] {
            let at = ahead(forward, side, height);
            gpu.saber_skins.shot_blades =
                vec![(lying(at, 0.12), ShotColor::Skin("saber_chameleon"), 2)];
            let camera = at + turn * Vec3::new(-26.0, -20.0, 18.0);
            let (look_yaw, look_pitch) = look(
                camera.to_array(),
                (at + turn * Vec3::new(0.0, -20.0, 2.0)).to_array(),
            );
            aim(&mut gpu, camera.to_array(), look_yaw, look_pitch);
            gpu.saber_skins.shot_seconds = Some(3.0);
            places.push(frame(&mut gpu, 6));
            // The light each instance took from the grid.
            let lights: Vec<String> = gpu
                .saber_instances
                .iter()
                .map(|i| format!("{:08x}", i.persona_lanes()[0]))
                .collect();
            println!("chameleon at {at:?}: lights {lights:?}");
        }
        println!(
            "{}",
            sheet(&places, 3, 640, "duel6-blade-chameleon-places").display()
        );
    });
}

/// Every option the Sun, the Spectral and the Void offer, switched off.
fn all_parts_off(id: &str) -> crate::saber_skin_options::OptionsOff {
    let options = match id {
        "saber_sun" => &["prominences", "sparks", "glint", "haze"][..],
        "saber_void" => &["holes", "stars", "warp", "pulse", "afterimages"][..],
        _ => &["wisps", "spirits", "smoke", "rim", "echo", "afterimages"][..],
    };
    crate::saber_skin_options::bits(options.iter().copied())
}

/// The Mythical blades' parts (11/10/2026): the Sun and the Spectral standing with every
/// part on beside every part off, at two times (`duel6-mythical-lineup`); then each up
/// close, lying across the view over the floor at eight moments, so the Sun's glint,
/// prominences, sparks and the haze bending the floor behind it show, and the Spectral's
/// wisps rise off it (`duel6-mythical-<name>`); then each held upright, its wisps a plume
/// over the tip (`duel6-mythical-<name>-upright`).
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_mythical_parts() {
    on_big_stack(|| {
        mount_test_packs();
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
        gpu.saber_skins.shot_persona = crate::saber_persona::Persona::of(b"^1Sol^7Vulpes", 0);
        let ids = ["saber_sun", "saber_spectral", "saber_void"];
        let standing: Vec<_> = ids
            .iter()
            .flat_map(|id| [ShotColor::Skin(id), ShotColor::Parts(id, all_parts_off(id))])
            .collect();
        gpu.saber_skins.shot_blades = standing
            .iter()
            .enumerate()
            .map(|(index, color)| {
                let offset = (index as f32 - 2.5) * 13.0;
                (
                    Blade {
                        base: ahead(70.0, -offset, -24.0).to_array(),
                        direction: (turn * Vec3::new(0.0, -0.004 * offset, 1.0))
                            .normalize()
                            .to_array(),
                        length: 40.0,
                        radius: 3.0,
                    },
                    *color,
                    index as u32 + 1,
                )
            })
            .collect();
        let (look_yaw, look_pitch) = look(eye.to_array(), ahead(70.0, 0.0, -6.0).to_array());
        aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
        let mut lineup = Vec::new();
        for (index, seconds) in [2.6, 5.15].into_iter().enumerate() {
            gpu.saber_skins.shot_seconds = Some(seconds);
            lineup.push(frame(&mut gpu, if index == 0 { 40 } else { 4 }));
        }
        println!(
            "{}",
            sheet(&lineup, 1, 1280, "duel6-mythical-lineup").display()
        );
        let lying = |at: Vec3, down: f32| Blade {
            base: at.to_array(),
            direction: (turn * Vec3::new(0.0, -1.0, down)).normalize().to_array(),
            length: 40.0,
            radius: 3.0,
        };
        let camera = ahead(27.0, 0.0, -15.0);
        let (look_yaw, look_pitch) = look(camera.to_array(), ahead(44.0, 0.0, -24.0).to_array());
        for id in ids {
            let blade = lying(ahead(44.0, 20.0, -22.0), 0.08);
            gpu.saber_skins.shot_blades = vec![(blade, ShotColor::Skin(id), 2)];
            aim(&mut gpu, camera.to_array(), look_yaw, look_pitch);
            let mut close = Vec::new();
            for step in 0..8 {
                gpu.saber_skins.shot_seconds = Some(3.0 + f64::from(step) * 0.37);
                close.push(frame(&mut gpu, 4));
            }
            let name = format!("duel6-mythical-{}", id.trim_start_matches("saber_"));
            println!("{}", sheet(&close, 2, 960, &name).display());
            if id == "saber_sun" {
                // The haze bends the floor behind the blade: the same moment with it off
                // differs there, and only there (a diff sheet shows where).
                let haze_off = crate::saber_skin_options::bits(["haze"]);
                gpu.saber_skins.shot_seconds = Some(3.0);
                let on = frame(&mut gpu, 4);
                gpu.saber_skins.shot_blades = vec![(blade, ShotColor::Parts(id, haze_off), 2)];
                let off = frame(&mut gpu, 4);
                let mut diff = image::RgbaImage::new(on.width(), on.height());
                let mut changed = 0;
                for (x, y, pixel) in diff.enumerate_pixels_mut() {
                    let (a, b) = (on.get_pixel(x, y), off.get_pixel(x, y));
                    let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
                    if d > 6 {
                        changed += 1;
                    }
                    let v = d.saturating_mul(8);
                    *pixel = image::Rgba([v, v, v, 255]);
                }
                println!("haze changes {changed} pixels");
                println!(
                    "{}",
                    sheet(&[on, off, diff], 3, 640, "duel6-mythical-sun-haze").display()
                );
                assert!(
                    changed > 2_000,
                    "the haze bends the scene: {changed} pixels"
                );
            }
            // Held upright, a little away.
            let upright = Blade {
                base: ahead(38.0, 16.0, -30.0).to_array(),
                direction: (turn * Vec3::new(0.0, 0.15, 1.0)).normalize().to_array(),
                length: 40.0,
                radius: 3.0,
            };
            gpu.saber_skins.shot_blades = vec![(upright, ShotColor::Skin(id), 3)];
            let (yaw_up, pitch_up) = look(eye.to_array(), ahead(38.0, 16.0, -4.0).to_array());
            aim(&mut gpu, eye.to_array(), yaw_up, pitch_up);
            let mut tall = Vec::new();
            for step in 0..4 {
                gpu.saber_skins.shot_seconds = Some(4.0 + f64::from(step) * 0.5);
                tall.push(frame(&mut gpu, 4));
            }
            let name = format!("duel6-mythical-{}-upright", id.trim_start_matches("saber_"));
            println!("{}", sheet(&tall, 2, 960, &name).display());
        }
    });
}

/// The Sun's glints and the Void's black holes circling their blades below the tip
/// (11/10/2026): an upright blade up close, one orbit in 36 frames, cropped round them
/// into `duel6-<name>-orbit.gif`, and four of them side by side (`duel6-<name>-orbit`).
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_orbits() {
    on_big_stack(|| {
        mount_test_packs();
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
        let blade = Blade {
            base: ahead(34.0, 0.0, -34.0).to_array(),
            direction: (turn * Vec3::new(0.0, 0.1, 1.0)).normalize().to_array(),
            length: 40.0,
            radius: 3.0,
        };
        let (look_yaw, look_pitch) = look(eye.to_array(), ahead(34.0, 3.0, -4.0).to_array());
        aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
        // One turn of each orbit: the Sun's 0.45 turns a second, the Void's 0.3.
        for (id, rate) in [("saber_sun", 0.45), ("saber_void", 0.3)] {
            gpu.saber_skins.shot_blades = vec![(blade, ShotColor::Skin(id), 2)];
            let period = 1.0 / rate;
            let name = format!("duel6-{}-orbit", id.trim_start_matches("saber_"));
            let mut frames = Vec::new();
            for step in 0..36 {
                gpu.saber_skins.shot_seconds = Some(3.0 + period * f64::from(step) / 36.0);
                let full = frame(&mut gpu, if step == 0 { 40 } else { 2 });
                frames.push(image::imageops::crop_imm(&full, 400, 200, 480, 400).to_image());
            }
            let path = super::directory().join(format!("{name}.gif"));
            let file = std::fs::File::create(&path).expect("the gif");
            let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(file, 10);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .expect("repeat");
            for shot in &frames {
                let delay = image::Delay::from_numer_denom_ms((period * 1000.0 / 36.0) as u32, 1);
                encoder
                    .encode_frame(image::Frame::from_parts(shot.clone(), 0, 0, delay))
                    .expect("a frame");
            }
            println!("{}", path.display());
            let four: Vec<_> = frames.iter().step_by(9).cloned().collect();
            println!("{}", sheet(&four, 4, 480, &name).display());
        }
    });
}

/// The chromas ([`crate::unlockables::Unlockable::chroma`]) take their saber's colour:
/// each in a row of the six retail colours and a custom white (which leaves the skin's
/// own), standing on duel6 as the lineup does.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_chroma_blades() {
    use crate::saber::Color;
    on_big_stack(|| {
        mount_test_packs();
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
        gpu.saber_skins.shot_persona = crate::saber_persona::Persona::of(b"^1Sol^7Vulpes", 0);
        let colours = [
            BladeColor::Retail(Color::Red),
            BladeColor::Retail(Color::Orange),
            BladeColor::Retail(Color::Yellow),
            BladeColor::Retail(Color::Green),
            BladeColor::Retail(Color::Blue),
            BladeColor::Retail(Color::Purple),
            BladeColor::Rgb([255, 255, 255]),
        ];
        let (look_yaw, look_pitch) = look(eye.to_array(), ahead(70.0, 0.0, -6.0).to_array());
        let chromas: Vec<_> = crate::unlockables::blade_skins()
            .filter(|skin| skin.chroma)
            .map(|skin| skin.id)
            .collect();
        let mut rows = Vec::new();
        for id in &chromas {
            gpu.saber_skins.shot_blades = colours
                .iter()
                .enumerate()
                .map(|(index, stock)| {
                    let offset = (index as f32 - 3.0) * 10.0;
                    (
                        Blade {
                            base: ahead(70.0, -offset, -24.0).to_array(),
                            direction: (turn * Vec3::new(0.0, -0.006 * offset, 1.0))
                                .normalize()
                                .to_array(),
                            length: 40.0,
                            radius: 3.0,
                        },
                        ShotColor::Worn(id, *stock),
                        index as u32 + 1,
                    )
                })
                .collect();
            aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
            gpu.saber_skins.shot_seconds = Some(2.6);
            rows.push(frame(&mut gpu, if rows.is_empty() { 40 } else { 4 }));
        }
        println!("{}", sheet(&rows, 2, 1280, "duel6-chroma-blades").display());
    });
}

/// The plain Common and Uncommon blade skins (the droppable ones) on duel6, each tier
/// standing side by side at two times (sheets `duel6-plain-common`, `duel6-plain-uncommon`).
/// Needs the hub's packs in `SJK_TEST_PACKS`.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_plain_blades() {
    on_big_stack(|| {
        mount_test_packs();
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1280, 720], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
        for (tier, name) in [
            (crate::unlockables::Rarity::Common, "duel6-plain-common"),
            (crate::unlockables::Rarity::Uncommon, "duel6-plain-uncommon"),
        ] {
            let ids: Vec<&'static str> = crate::unlockables::blade_skins()
                .filter(|skin| skin.tier == tier && skin.droppable)
                .map(|skin| skin.id)
                .collect();
            let middle = (ids.len() as f32 - 1.0) * 0.5;
            gpu.saber_skins.shot_blades = ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let offset = (index as f32 - middle) * 7.0;
                    (
                        Blade {
                            base: ahead(80.0, -offset, -24.0).to_array(),
                            direction: (turn * Vec3::new(0.0, 0.0, 1.0)).normalize().to_array(),
                            length: 40.0,
                            radius: 3.0,
                        },
                        ShotColor::Skin(id),
                        index as u32 + 1,
                    )
                })
                .collect();
            let (look_yaw, look_pitch) = look(eye.to_array(), ahead(80.0, 0.0, -6.0).to_array());
            aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
            let mut lineup = Vec::new();
            for (index, seconds) in [2.6, 5.15].into_iter().enumerate() {
                gpu.saber_skins.shot_seconds = Some(seconds);
                lineup.push(frame(&mut gpu, if index == 0 { 40 } else { 4 }));
            }
            println!("{}", sheet(&lineup, 1, 1280, name).display());
        }
    });
}

/// The Blade Lab's shaders (15/10/2026) side by side, close up, at eight moments: Neon,
/// Peppermint, Soap Bubble, Lava Lamp, Reactor and Bleeding Kyber, left to right.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_lab_blades() {
    lab_lineup(
        &[
            "saber_neon",
            "saber_peppermint",
            "saber_bubble",
            "saber_lavalamp",
            "saber_reactor",
            "saber_bleeding",
        ],
        7.0,
        "duel6-lab-blades",
    );
}

/// The Blade Lab's second batch (15/10/2026), as the first, wider apart for their wings
/// and butterflies: Sketch, Hourglass, Fiber Optic, Serpent, Monarch, Phoenix and Quantum.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_lab_blades_figures() {
    lab_lineup(
        &[
            "saber_sketch",
            "saber_hourglass",
            "saber_fiber",
            "saber_serpent",
            "saber_monarch",
            "saber_phoenix",
            "saber_quantum",
        ],
        10.0,
        "duel6-lab-blades-figures",
    );
}

/// The Blade Lab's solid blades (15/10/2026), drawn blended over their glow: Comic,
/// Quicksilver and Obsidian Edge.
#[test]
#[ignore = "renders with the GPU and the installed game data named by JKA_GAME_DATA"]
fn duel6_lab_blades_solids() {
    lab_lineup(
        &["saber_comic", "saber_quicksilver", "saber_obsidian"],
        10.0,
        "duel6-lab-blades-solids",
    );
}

/// `ids` upright `spacing` units apart in front of duel6's spawn, at eight moments, as one
/// sheet called `name`.
fn lab_lineup(ids: &'static [&'static str], spacing: f32, name: &'static str) {
    on_big_stack(move || {
        mount_test_packs();
        let Some((mut gpu, _profile)) = open("maps/mp/duel6.bsp", [1600, 900], None, &[]) else {
            return;
        };
        let (spawn, yaw) = assets::initial_camera(&gpu.bsp).expect("a spawn point");
        let turn = glam::Quat::from_rotation_z(yaw);
        let eye = Vec3::from_array(spawn);
        let ahead = |x: f32, y: f32, z: f32| eye + turn * Vec3::new(x, y, z);
        let middle = (ids.len() as f32 - 1.0) * 0.5;
        gpu.saber_skins.shot_blades = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let offset = (index as f32 - middle) * spacing;
                (
                    Blade {
                        base: ahead(48.0, -offset, -20.0).to_array(),
                        direction: (turn * Vec3::new(0.0, 0.0, 1.0)).normalize().to_array(),
                        length: 40.0,
                        radius: 3.0,
                    },
                    ShotColor::Skin(id),
                    index as u32 + 1,
                )
            })
            .collect();
        let (look_yaw, look_pitch) = look(eye.to_array(), ahead(48.0, 0.0, -1.0).to_array());
        aim(&mut gpu, eye.to_array(), look_yaw, look_pitch);
        let mut lineup = Vec::new();
        for (index, seconds) in [1.0, 2.6, 4.4, 6.3, 8.1, 9.9, 11.7, 13.5]
            .into_iter()
            .enumerate()
        {
            gpu.saber_skins.shot_seconds = Some(seconds);
            lineup.push(frame(&mut gpu, if index == 0 { 40 } else { 4 }));
        }
        println!("{}", sheet(&lineup, 4, 1600, name).display());
    });
}

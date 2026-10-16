//! JoF EJK PR #257's presentation-only rain strikes, using stock `saber/fizz`.
//! Fixed source slots pace held, dual, multi-blade and thrown sabers independently.

use glam::Vec3;
use std::time::Instant;

/// Archived JoF-compatible three-mode preference.
pub(crate) const CVAR: &str = "cg_saberRainSteam";
/// Stock effect graph supplies both steam particles and sizzle sounds.
pub(crate) const EFFECT: &str = "saber/fizz";

/// One deadline per existing blade contact key, allocated once per map.
pub(crate) struct Runtime {
    next: Box<[Option<i64>]>,
    last_time: Option<i64>,
    solids: Vec<crate::local_prediction::movers::Collider>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            next: vec![None; 1024 * 24].into_boxed_slice(),
            last_time: None,
            solids: Vec::with_capacity(sjk_protocol::MAX_LEGACY_ENTITIES),
        }
    }
}

impl Runtime {
    fn due(&self, key: usize, time: i64) -> bool {
        let key = if key % 24 >= 16 { key - 16 } else { key };
        self.next
            .get(key)
            .is_some_and(|next| next.is_none_or(|deadline| time >= deadline))
    }
    fn observe(&mut self, time: i64) {
        if self.last_time.is_some_and(|last| time < last) {
            self.next.fill(None);
        }
        self.last_time = Some(time);
    }

    /// Reserve before checking cover: sheltered blades also wait 300–600 ms.
    /// Duplicate glow/core submissions and repeated presentation times cannot replay.
    fn strike(&mut self, key: usize, time: i64) -> Option<(u32, f32)> {
        // Contact trails use a separate thrown lane; rain still belongs to the
        // primary blade, retaining its deadline when thrown or caught.
        let key = if key % 24 >= 16 { key - 16 } else { key };
        let next = self.next.get_mut(key)?;
        if next.is_some_and(|deadline| time < deadline) {
            return None;
        }
        let seed = (key as u32).wrapping_mul(0x9e37_79b9) ^ time as u32;
        let unit = crate::effect_envelope::random_unit;
        let delay = 300 + (unit(seed) * 301.0).floor().min(300.0) as i64;
        *next = Some(time.saturating_add(delay));
        Some((seed, 0.1 + 0.9 * unit(seed.wrapping_add(1))))
    }
}

fn allowed(mode: i64, key: usize, local: Option<u16>) -> bool {
    mode > 0 && (mode != 2 || local == Some((key / 24) as u16))
}

/// Consume actual visible blade submissions after wall cutoff, excluding ghosts,
/// trails and menu blades. The same path includes the local first-person and thrown hilt.
pub(crate) fn frame(
    gpu: &mut crate::GpuState,
    time: i64,
    now: Instant,
    audio: &mut Option<crate::GameAudio>,
) {
    gpu.effect_aux.saber_rain_steam.observe(time);
    let mode = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_saberrainsteam"))
        .unwrap_or(1);
    if mode <= 0 || gpu.resident.map_change_pending || !gpu.weather.saber_rain(&gpu.context.weather)
    {
        return;
    }
    let Some(vfs) = gpu.vfs.as_deref() else {
        return;
    };
    let solids = gpu
        .live_session
        .as_ref()
        .map(|session| (session.game_state(), session.latest_snapshot()))
        .or_else(|| {
            gpu.demo_session
                .as_ref()
                .map(|session| (session.game_state(), session.latest_snapshot()))
        });
    // The connection's client number stays ours when a spectator follows someone.
    let local = solids.and_then(|(game, _)| u16::try_from(game.client_num).ok());
    if !gpu.saber_instances.iter().any(|instance| {
        instance.contact().is_some_and(|(key, blade, _, _)| {
            blade.length > 0.0
                && allowed(mode, key, local)
                && gpu.effect_aux.saber_rain_steam.due(key, time)
        })
    }) {
        return;
    }
    // Decode inline solids once, rather than scanning all players per blade.
    let inline = &mut gpu.effect_aux.saber_rain_steam.solids;
    inline.clear();
    if let Some((game, snapshot)) = solids {
        for entity in sjk_client::legacy_scene_entities(game, snapshot) {
            if entity.solid() == 0x00ff_ffff
                && let Some(collider) = crate::local_prediction::movers::Collider::from_entity(
                    entity,
                    time as i32,
                    time as i32,
                    false,
                )
            {
                inline.push(collider);
            }
        }
    }
    for instance in &gpu.saber_instances {
        let Some((key, blade, _, _)) = instance.contact() else {
            continue;
        };
        if blade.length <= 0.0 || !allowed(mode, key, local) {
            continue;
        }
        let Some((seed, fraction)) = gpu.effect_aux.saber_rain_steam.strike(key, time) else {
            continue;
        };
        let point = Vec3::from_array(blade.base)
            + Vec3::from_array(blade.direction) * (blade.length * fraction);
        let exposed = crate::movement_collision::rain_exposed(
            &gpu.bsp,
            &mut gpu.trace_scratch,
            &gpu.effect_aux.saber_rain_steam.solids,
            (key / 24) as u16,
            point,
        );
        if exposed {
            crate::effect_runtime::spawn_effect(
                &mut gpu.particles,
                &mut gpu.effect_aux,
                &mut gpu.effects,
                vfs,
                EFFECT,
                point,
                now,
                seed,
                0,
                audio,
                crate::combat_effects::rotation_from_direction(Vec3::Z.to_array()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_bsp::{Bsp, CollisionShader, box_brush, write_collision_map};

    fn map(contents: u32, surface: u32, roof: bool) -> Bsp {
        let shaders = [CollisionShader {
            name: "textures/test/rain".into(),
            surface_flags: surface,
            content_flags: contents,
        }];
        let brush = if roof {
            box_brush([-100.0, -100.0, 128.0], [100.0, 100.0, 140.0], 0)
        } else {
            box_brush([-100.0; 3], [100.0; 3], 0)
        };
        Bsp::parse(&write_collision_map(
            "{\n\"classname\" \"worldspawn\"\n}\n",
            &shaders,
            &[brush],
        ))
        .unwrap()
    }

    fn exposed(bsp: &Bsp) -> bool {
        crate::movement_collision::rain_exposed(bsp, &mut bsp.trace_scratch(), &[], 0, Vec3::ZERO)
    }

    #[test]
    fn roofs_indoor_volumes_and_liquids_block_rain() {
        assert!(exposed(&map(1, 0x2000, true))); // sky roof
        assert!(!exposed(&map(1, 0, true))); // solid roof
        assert!(exposed(&map(0x10000, 0, false))); // explicitly outside
        assert!(!exposed(&map(0, 0, false))); // void with no sky
        for content in [1, 0x1000_0000, 0x20000, 2, 4] {
            assert!(!exposed(&map(content | 0x10000, 0, false)));
        }
    }

    #[test]
    fn a_translated_inline_door_blocks_then_uncovers_the_blade() {
        let mut bsp = map(1, 0x2000, true);
        let door_model = bsp.append_model_asset(map(1, 0, true)).unwrap().start;
        let mut door = sjk_protocol::EntityState::zero(300, &sjk_protocol::LEGACY_ENTITY_FIELDS);
        assert!(door.set_raw_field(8, 6)); // ET_MOVER
        assert!(door.set_raw_field(46, door_model as u32)); // modelindex
        assert!(door.set_raw_field(26, 0x00ff_ffff)); // SOLID_BMODEL
        // Model's ceiling is lowered beneath the map sky, then moved out of the way.
        assert!(door.set_raw_field(4, (-64.0_f32).to_bits())); // pos.trBase z
        let test = |door: &sjk_protocol::EntityState| {
            crate::movement_collision::rain_exposed(
                &bsp,
                &mut bsp.trace_scratch(),
                &[
                    crate::local_prediction::movers::Collider::from_entity(door, 0, 0, false)
                        .unwrap(),
                ],
                0,
                Vec3::ZERO,
            )
        };
        assert!(!test(&door));
        assert!(door.set_raw_field(2, 1000.0_f32.to_bits())); // pos.trBase x
        assert!(test(&door));
    }

    #[test]
    fn weather_commands_and_freeze_control_strikes() {
        let bsp = map(0x10000, 0, false);
        let settings = crate::weather::Settings::default();
        for rain in ["rain", "acidrain", "lightrain", "heavyrain"] {
            let mut weather = crate::weather::Runtime::new(None, &bsp);
            weather.command(rain).unwrap();
            assert!(weather.saber_rain(&settings));
            weather.command("snow").unwrap();
            weather.command("wind").unwrap();
            assert!(weather.saber_rain(&settings));
            weather.command("freeze").unwrap();
            assert!(!weather.saber_rain(&settings));
            weather.command("clear").unwrap();
            weather.command(rain).unwrap();
            assert!(!weather.saber_rain(&settings)); // clear retains freeze
            weather.command("die").unwrap();
            weather.command(rain).unwrap();
            assert!(weather.saber_rain(&settings)); // die unfreezes
            weather.command("clear").unwrap();
            weather.command("snow").unwrap();
            assert!(!weather.saber_rain(&settings));
        }
    }

    #[test]
    fn modes_follow_owner_even_for_thrown_lane() {
        for key in [24 * 7, 24 * 7 + 8, 24 * 7 + 16, 24 * 7 + 23] {
            assert!(!allowed(0, key, Some(7)));
            assert!(allowed(1, key, None));
            assert!(allowed(2, key, Some(7)));
            assert!(!allowed(2, key, Some(8)));
            assert!(!allowed(2, key, None));
        }
    }

    #[test]
    fn preference_defaults_on_and_is_archived() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("config.cfg");
        let mut console = crate::console::ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(console.integer_cvar(CVAR), Some(1));
        assert!(console.set_cvar(CVAR, "2"));
        console.flush_config();
        let console = crate::console::ViewerConsole::new(path).unwrap();
        assert_eq!(console.integer_cvar(CVAR), Some(2));
    }

    #[test]
    fn forced_rain_and_weather_disable_follow_live_settings() {
        let bsp = map(0x10000, 0, false);
        let mut cvars = sjk_shell::CvarRegistry::default();
        let settings = crate::weather::Settings::bind(&mut cvars).unwrap();
        let mut weather = crate::weather::Runtime::new(None, &bsp);
        assert!(!weather.saber_rain(&settings));
        for force in [1, 2, 3] {
            cvars
                .set_text("r_weatherForce", &force.to_string())
                .unwrap();
            assert!(weather.saber_rain(&settings));
        }
        cvars.set_text("r_weatherForce", "4").unwrap();
        assert!(!weather.saber_rain(&settings)); // snow
        cvars.set_text("r_weatherForce", "2").unwrap();
        cvars.set_text("r_weather", "0").unwrap();
        assert!(!weather.saber_rain(&settings));
        cvars.set_text("r_weather", "1").unwrap();
        assert!(weather.saber_rain(&settings));
    }

    #[test]
    fn cadence_and_position_are_bounded_and_independent() {
        for step in [8, 7, 4, 3] {
            let mut runtime = Runtime::default();
            let mut last = None;
            for time in (0..3000).step_by(step) {
                runtime.observe(time);
                if let Some((_, fraction)) = runtime.strike(0, time) {
                    assert!((0.1..=1.0).contains(&fraction));
                    if let Some(previous) = last {
                        assert!((300..=600 + step as i64).contains(&(time - previous)));
                    }
                    last = Some(time);
                    assert!(runtime.strike(0, time).is_none());
                }
            }
            assert!(last.unwrap() > 2000);
            assert!(runtime.strike(8, 3000).is_some());
            assert!(runtime.strike(24, 3000).is_some());
            runtime.observe(4000);
            assert!(runtime.strike(0, 4000).is_some());
            assert!(runtime.strike(16, 4000).is_none());
        }
    }

    #[test]
    fn demo_rewind_resets_deadlines() {
        let mut runtime = Runtime::default();
        runtime.observe(1000);
        assert!(runtime.strike(0, 1000).is_some());
        assert!(runtime.strike(0, 1000).is_none());
        runtime.observe(0);
        assert!(runtime.strike(0, 0).is_some());
    }

    #[test]
    #[ignore = "requires GPU and installed retail assets in JKA_GAME_DATA"]
    fn rain_steam_world_shot() {
        crate::world_shot::on_big_stack(|| {
            use crate::world_shot as shots;
            let (mut gpu, _profile) = shots::open(
                "maps/mp/duel6.bsp",
                [960, 540],
                None,
                &[
                    ("r_weatherForce", "2"),
                    ("r_weatherFog", "0"),
                    ("r_clouds", "0"),
                    (CVAR, "0"),
                ],
            )
            .expect("GPU startup must succeed");
            let (spawn, _) = crate::assets::initial_camera(&gpu.bsp).unwrap();
            let origin = Vec3::from_array(spawn);
            let mut base = None;
            'search: for x in (-512..=512).step_by(64) {
                for y in (-512..=512).step_by(64) {
                    let point = origin + Vec3::new(x as f32, y as f32, 16.0);
                    if [0.0, 40.0].into_iter().all(|height| {
                        crate::movement_collision::rain_exposed(
                            &gpu.bsp,
                            &mut gpu.trace_scratch,
                            &[],
                            0,
                            point + Vec3::Z * height,
                        )
                    }) {
                        base = Some(point);
                        break 'search;
                    }
                }
            }
            let base = base.expect("an exposed position on duel6");
            let blade = crate::saber::Blade {
                base: base.to_array(),
                direction: Vec3::Z.to_array(),
                length: 40.0,
                radius: 3.0,
            };
            let color = crate::saber_rgb::BladeColor::Retail(crate::saber::Color::Blue);
            gpu.saber_skins.shot_blades =
                vec![(blade, crate::saber_skins::ShotColor::Stock(color), 1)];
            let eye = base + Vec3::new(-65.0, 0.0, 20.0);
            let (yaw, pitch) = shots::look(eye.to_array(), (base + Vec3::Z * 20.0).to_array());
            shots::aim(&mut gpu, eye.to_array(), yaw, pitch);
            let off = shots::frame(&mut gpu, 40);
            let instances = crate::saber::Instance::pair(blade, color)
                .map(|i| i.with_contact(1, 0, 0, true, false));
            gpu.saber_instances.extend(instances);
            let before = gpu.particles.len();
            frame(&mut gpu, 1000, Instant::now(), &mut None);
            assert_eq!(gpu.particles.len(), before, "Off must not spawn");
            assert!(gpu.console.as_mut().unwrap().set_cvar(CVAR, "1"));
            let definition = gpu
                .effects
                .definition(gpu.vfs.as_deref().unwrap(), EFFECT)
                .expect("retail fizz effect");
            assert!(definition.components.iter().any(|component| component.kind
                == sjk_effect::ComponentKind::Sound
                && !component.sounds.is_empty()));
            frame(&mut gpu, 1000, Instant::now(), &mut None);
            assert!(
                gpu.particles.len() > before,
                "exposed blade must emit retail fizz particles"
            );
            let on = shots::frame(&mut gpu, 8);
            let changed = off
                .pixels()
                .zip(on.pixels())
                .filter(|(a, b)| a != b)
                .count();
            assert!(changed > 0);
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/parity-reports/saber-rain-steam.png");
            let mut image = image::RgbaImage::new(1920, 540);
            image::imageops::overlay(&mut image, &off, 0, 0);
            image::imageops::overlay(&mut image, &on, 960, 0);
            image.save(&path).unwrap();
            println!(
                "steam off/on: {} ({changed} changed pixels)",
                path.display()
            );
            // Release runs report the added submission cost, separate from GPU rendering.
            // No claim about a live crowd: one blade, static duel6, audio disabled.
            gpu.effect_aux.saber_rain_steam = Runtime::default();
            gpu.saber_instances.clear();
            gpu.saber_instances.extend(instances);
            let started = Instant::now();
            for time in (2000..10_000).step_by(8) {
                frame(&mut gpu, time, Instant::now(), &mut None);
            }
            println!(
                "one-blade rain submission: {:.3} us/frame over 1000 frames ({} build)",
                started.elapsed().as_secs_f64() * 1000.0,
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                }
            );
        });
    }
}

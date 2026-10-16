//! An offline rocket barrage over the installed game data: how many particles a
//! rocket trail keeps alive, when the pool runs out without [`Room`], and what the
//! room and the effect update cost. It is an ignored test because it reads the game
//! installation; it opens no window and touches no network:
//!
//! ```sh
//! JKA_GAME_DATA="/path/to/GameData" cargo test --release -p sjk-viewer \
//!     rocket_barrage -- --ignored --nocapture
//! ```
//!
//! Rockets circle a deathmatch spawn of `mp/ffa3` at the rocket's 900 units a second
//! and play `rocket/shot` every 8 ms, as `missile_trails` does, while the frame
//! advances 2 ms (500 FPS) and `particle_physics::update_and_spawn` runs the real
//! physics against the map. A rocket's trail head is visible while a particle spawned
//! in the last 24 ms lies within 96 units of it.

use super::*;
use crate::effect_runtime::EffectLibrary;
use glam::Vec3;
use std::path::PathBuf;
use std::time::Duration;

const FRAME: Duration = Duration::from_millis(2);
const SECONDS: u64 = 6;
const SPEED: f32 = 900.0;
const RADIUS: f32 = 320.0;

struct Outcome {
    live: f64,
    heads: f64,
    refused: u64,
    evicted: u64,
    update: Duration,
    room: Duration,
    sort: Duration,
}

fn spawn_point(bsp: &sjk_bsp::Bsp) -> Vec3 {
    let entities = sjk_entity::parse_entity_lump(bsp.entities()).expect("entity lump");
    let origin = entities
        .iter()
        .find(|e| e.classname() == Some("info_player_deathmatch"))
        .and_then(|e| e.fields().iter().find(|(k, _)| k == "origin"))
        .map(|(_, v)| v.clone())
        .expect("a deathmatch spawn");
    let values: Vec<f32> = origin
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    Vec3::new(values[0], values[1], values[2] + 96.0)
}

fn rocket(centre: Vec3, index: usize, seconds: f32) -> (Vec3, Vec3) {
    let phase = index as f32 * 0.7 + seconds * SPEED / RADIUS;
    let height = 24.0 * (index % 4) as f32;
    let position = centre + Vec3::new(phase.cos() * RADIUS, phase.sin() * RADIUS, height);
    (position, Vec3::new(-phase.sin(), phase.cos(), 0.0))
}

fn run(
    vfs: &sjk_vfs::VirtualFileSystem,
    bsp: &sjk_bsp::Bsp,
    rockets: usize,
    room: bool,
) -> Outcome {
    let centre = spawn_point(bsp);
    let mut effects = EffectLibrary::default();
    let mut aux = crate::effect_aux::Runtime::default();
    let mut pending = crate::particle_physics::PendingEffects::new();
    let mut scratch = sjk_bsp::TraceScratch::new();
    let mut particles = Vec::with_capacity(PARTICLE_POOL);
    let mut space = Room::default();
    let mut audio = None;
    let mut distances: Vec<(f32, u32)> = Vec::with_capacity(PARTICLE_POOL);
    let start = Instant::now();
    let steps = SECONDS * 1_000 / FRAME.as_millis() as u64;
    let measured_from = steps / 2;
    let mut outcome = Outcome {
        live: 0.0,
        heads: 0.0,
        refused: 0,
        evicted: 0,
        update: Duration::ZERO,
        room: Duration::ZERO,
        sort: Duration::ZERO,
    };
    take_refused();
    for step in 0..steps {
        let now = start + FRAME * step as u32;
        let seconds = (FRAME * step as u32).as_secs_f32();
        let refused = take_refused();
        if step >= measured_from {
            outcome.refused += u64::from(refused);
        }
        let clock = Instant::now();
        if room {
            space.make_room(&mut particles, now, refused);
        }
        let room_time = clock.elapsed();
        let clock = Instant::now();
        if step % 4 == 0 {
            for index in 0..rockets {
                let (position, direction) = rocket(centre, index, seconds);
                crate::effect_runtime::spawn_effect(
                    &mut particles,
                    &mut aux,
                    &mut effects,
                    vfs,
                    "rocket/shot",
                    position,
                    now,
                    (step as u32).wrapping_mul(31).wrapping_add(index as u32),
                    0,
                    &mut audio,
                    crate::combat_effects::rotation_from_direction(direction.to_array()),
                );
            }
        }
        crate::particle_physics::update_and_spawn(
            &mut particles,
            &mut pending,
            &mut aux,
            &mut effects,
            vfs,
            &mut audio,
            now,
            bsp,
            &mut scratch,
        );
        let update_time = clock.elapsed();
        // The billboard depth sort `effect_submission` does for alpha-blended layers.
        let clock = Instant::now();
        distances.clear();
        distances.extend(
            particles
                .iter()
                .enumerate()
                .map(|(i, p)| (p.motion.sample().origin.distance_squared(centre), i as u32)),
        );
        distances.sort_by(|a, b| b.0.total_cmp(&a.0));
        let sort_time = clock.elapsed();
        if step < measured_from {
            continue;
        }
        outcome.update += update_time;
        outcome.room += room_time;
        outcome.sort += sort_time;
        outcome.live += particles.len() as f64;
        let fresh = (0..rockets)
            .filter(|&index| {
                let (position, _) = rocket(centre, index, seconds);
                particles.iter().any(|p| {
                    now.saturating_duration_since(p.spawned_at) <= Duration::from_millis(24)
                        && p.motion.sample().origin.distance(position) <= 96.0
                })
            })
            .count();
        outcome.heads += fresh as f64 / rockets as f64;
    }
    let frames = (steps - measured_from) as f64;
    outcome.live /= frames;
    outcome.heads /= frames;
    outcome.update /= (steps - measured_from) as u32;
    outcome.room /= (steps - measured_from) as u32;
    outcome.sort /= (steps - measured_from) as u32;
    outcome.evicted = space.evicted;
    outcome
}

#[test]
#[ignore = "reads the installed game data named by JKA_GAME_DATA"]
fn rocket_barrage() {
    let game = PathBuf::from(
        std::env::var_os("JKA_GAME_DATA").expect("set JKA_GAME_DATA to the GameData directory"),
    );
    let vfs = crate::assets::mount_game_data(&game).expect("mount the game data");
    let map = vfs
        .read("maps/mp/ffa3.bsp")
        .expect("read the map")
        .expect("mp/ffa3 is installed");
    let bsp = sjk_bsp::Bsp::parse(&map.bytes).expect("parse mp/ffa3");
    println!(
        "pool {MAX_PARTICLES} effect particles (+{} billboards), headroom {MIN_HEADROOM}..{MAX_HEADROOM}",
        PARTICLE_POOL - MAX_PARTICLES
    );
    println!("rockets room   live  heads refused evicted  update   room   sort  (per 2 ms frame)");
    for rockets in [1, 2, 3, 4, 6, 8] {
        for room in [false, true] {
            let o = run(&vfs, &bsp, rockets, room);
            println!(
                "{rockets:>7} {:>4} {:>6.0} {:>5.0}% {:>7} {:>7} {:>6.0}us {:>4.0}us {:>4.0}us",
                if room { "yes" } else { "no" },
                o.live,
                o.heads * 100.0,
                o.refused,
                o.evicted,
                o.update.as_secs_f64() * 1e6,
                o.room.as_secs_f64() * 1e6,
                o.sort.as_secs_f64() * 1e6,
            );
        }
    }
}

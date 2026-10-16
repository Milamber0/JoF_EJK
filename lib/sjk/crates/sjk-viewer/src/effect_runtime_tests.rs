//! The teleport effect's floor and its `org2fromTrace` beam, in a made-up room.

use super::*;
use sjk_bsp::{CollisionShader, box_brush, write_collision_map};

/// Top of the floor and underside of the ceiling.
const FLOOR: f32 = 0.0;
const CEILING: f32 = 512.0;

/// A room 1024 units wide whose floor slab has `floor_contents`.
fn room(floor_contents: u32) -> sjk_bsp::Bsp {
    let shader = |name: &str, content_flags| CollisionShader {
        name: name.into(),
        surface_flags: 0,
        content_flags,
    };
    let shaders = [
        shader("textures/test/solid", 1),
        shader("textures/test/floor", floor_contents),
    ];
    let brushes = [
        box_brush([-512.0, -512.0, FLOOR - 64.0], [512.0, 512.0, FLOOR], 1),
        box_brush([-512.0, -512.0, CEILING], [512.0, 512.0, CEILING + 64.0], 0),
    ];
    let entities = "{\n\"classname\" \"worldspawn\"\n}\n";
    sjk_bsp::Bsp::parse(&write_collision_map(entities, &shaders, &brushes))
        .expect("synthetic map parses")
}

/// Two traced lines shaped like the beam of `mp/spawn` and `mp/jedispawn`: one from the
/// effect's origin, one from 20 units behind it along the effect's forward axis.
const BEAM: &str = "
Line
{
	spawnFlags	org2fromTrace
	life		500
	size { start 12 }
	shaders [ gfx/test/beam ]
}

Line
{
	spawnFlags	org2fromTrace
	life		500
	origin		-20 0 0
	size { start 40 }
	shaders [ gfx/test/beam ]
}
";

fn beam_vfs() -> VirtualFileSystem {
    let mut vfs = VirtualFileSystem::new();
    vfs.mount_memory("test", [("effects/test/beam.efx", BEAM)])
        .expect("mount the test effect");
    vfs
}

/// Play the test beam as `EV_PLAYER_TELEPORT_IN/OUT` play `mp/spawn`: at `origin`,
/// forward axis straight up.
fn play_beam(origin: Vec3) -> Vec<Particle> {
    let mut particles = Vec::new();
    spawn_effect(
        &mut particles,
        &mut crate::effect_aux::Runtime::default(),
        &mut EffectLibrary::default(),
        &beam_vfs(),
        "test/beam",
        origin,
        Instant::now(),
        64,
        0,
        &mut None,
        combat_effects::rotation_from_direction([0.0, 0.0, 1.0]),
    );
    particles
}

fn line_ends(particle: &Particle) -> (Vec3, Vec3) {
    let start = particle.motion.sample().origin;
    (start, start + particle.streak.expect("a line has a streak"))
}

#[test]
fn the_teleport_effect_stands_where_the_player_box_meets_the_floor() {
    let bsp = room(1);
    let mut scratch = bsp.trace_scratch();
    // A spawn point's origin is 9 units above its marker (`SelectSpawnPoint`); the box's
    // feet (mins z -16) rest on the floor, 16 units below the box origin.
    let floor = teleport_floor(&bsp, &mut scratch, Vec3::new(64.0, -32.0, FLOOR + 33.0))
        .expect("a floor below");
    assert!(
        (floor - Vec3::new(64.0, -32.0, FLOOR + 16.0)).length() < 0.2,
        "{floor}"
    );
    // Over a void the event plays no effect.
    assert_eq!(
        teleport_floor(&bsp, &mut scratch, Vec3::new(2_000.0, 0.0, 100.0)),
        None
    );
}

#[test]
fn the_teleport_floor_trace_stops_on_terrain_as_mask_solid_does() {
    const CONTENTS_TERRAIN: u32 = 0x1000;
    let bsp = room(CONTENTS_TERRAIN);
    let mut scratch = bsp.trace_scratch();
    let floor = teleport_floor(&bsp, &mut scratch, Vec3::new(0.0, 0.0, FLOOR + 33.0))
        .expect("a terrain floor below");
    assert!((floor.z - (FLOOR + 16.0)).abs() < 0.2, "{floor}");
}

#[test]
fn a_traced_line_runs_up_from_its_origin_to_the_ceiling() {
    let bsp = room(1);
    let mut scratch = bsp.trace_scratch();
    let origin = teleport_floor(&bsp, &mut scratch, Vec3::new(0.0, 0.0, FLOOR + 33.0))
        .expect("a floor below");
    let mut particles = play_beam(origin);
    assert_eq!(particles.len(), 2);
    for particle in &mut particles {
        assert!(particle.trace_streak, "traced when first drawn");
        crate::effect_geometry::resolve_traced_streak(particle, &bsp, &mut scratch);
        assert!(!particle.trace_streak);
    }
    // The first line starts at the effect's origin, the second 20 units below it (inside
    // the floor slab, which the trace leaves as stock's does); both reach the ceiling.
    let starts = [origin.z, origin.z - 20.0];
    for (particle, start_z) in particles.iter().zip(starts) {
        let (start, end) = line_ends(particle);
        assert!(
            (start - Vec3::new(origin.x, origin.y, start_z)).length() < 1e-3,
            "{start}"
        );
        assert!(end.x.abs() < 1e-3 && end.y.abs() < 1e-3, "{end}");
        assert!((end.z - CEILING).abs() < 0.2, "{end}");
    }
}

#[test]
fn a_traced_line_is_traced_once() {
    let bsp = room(1);
    let mut scratch = bsp.trace_scratch();
    let mut particles = play_beam(Vec3::new(0.0, 0.0, FLOOR + 16.0));
    let particle = &mut particles[0];
    crate::effect_geometry::resolve_traced_streak(particle, &bsp, &mut scratch);
    // Later draws keep the end, which a trace would cut at the ceiling; a trip mine
    // likewise gives its lines a kept trace's end and clears the flag.
    particle.streak = Some(Vec3::Z * 1_000.0);
    crate::effect_geometry::resolve_traced_streak(particle, &bsp, &mut scratch);
    assert_eq!(particle.streak, Some(Vec3::Z * 1_000.0));
}

#[test]
fn an_offset_traced_end_turns_with_the_effect() {
    // `org2isOffset` moves the untraced end by `origin2` in the effect's axes
    // (`FxScheduler.cpp:1402-1413`); `cheapOrg2Calc` leaves the offset in world axes.
    let mut component = sjk_effect::parse_effect(
        "Electricity { spawnFlags org2fromTrace org2isOffset origin2 0 -100 200 }",
    )
    .expect("parses")
    .components
    .remove(0);
    let up = combat_effects::rotation_from_direction([0.0, 0.0, 1.0]);
    let origin = Vec3::new(1.0, 2.0, 3.0);
    let offset = Vec3::new(0.0, -100.0, 200.0);
    let target = crate::effect_shapes::origin2_trace_target(&component, origin, up, offset);
    assert!((target - (origin + Vec3::Z * 16_384.0 + up * offset)).length() < 1e-2);
    component.spawn_flags.cheap_origin2 = true;
    let target = crate::effect_shapes::origin2_trace_target(&component, origin, up, offset);
    assert!((target - (origin + Vec3::Z * 16_384.0 + offset)).length() < 1e-2);
}

#[test]
fn the_spawn_beams_play_straight_up_whatever_the_player_faces() {
    // `cg_event.c` passes `ang = (0, 0, 1)` to `FX_PlayEffectID` for the teleport and
    // Jedi Master events, not the player's angles.
    let still = sjk_protocol::EntityState::zero(3, &sjk_protocol::LEGACY_ENTITY_FIELDS);
    let mut turned = still.clone();
    turned.set_raw_field(9, 90.0f32.to_bits()); // yaw
    for event in [34, 64, 65] {
        for player in [&still, &turned] {
            let direction = combat_effects::event_direction(event, player);
            assert_eq!(direction, [0.0, 0.0, 1.0], "event {event}");
        }
    }
}

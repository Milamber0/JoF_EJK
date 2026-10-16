use super::*;
use crate::lamp_lights::Lamp;

fn lamp(position: [f32; 3], radius: f32, power: f32) -> Lamp {
    Lamp {
        position: Vec3::from_array(position),
        normal: Vec3::Z,
        color: [1.; 3],
        power,
        radius,
        axis_u: Vec3::ZERO,
        axis_v: Vec3::ZERO,
    }
}

/// A door slab 64 wide, 8 thick and 128 high, modelled at `rest`, sliding 56 along +x.
fn door(mesh: usize, rest: [f32; 3]) -> Occluder {
    let (a, b, c, d) = (
        Vec3::new(0., 0., 0.),
        Vec3::new(64., 0., 0.),
        Vec3::new(64., 8., 128.),
        Vec3::new(0., 8., 128.),
    );
    let rest = Vec3::from_array(rest);
    let reach = (rest, rest + Vec3::new(120., 8., 128.));
    // The model sits at `rest`, as its entity's origin places it.
    let shift = |p: Vec3| p + rest;
    Occluder::new(
        mesh,
        vec![[a, b, c].map(shift), [a, c, d].map(shift)],
        reach,
    )
    .expect("triangles")
}

fn entity(fields: &[(&str, &str)]) -> sjk_entity::Entity {
    sjk_entity::Entity::from_fields(
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    )
}

#[test]
fn an_occluder_without_triangles_is_none() {
    assert!(Occluder::new(0, Vec::new(), (Vec3::ZERO, Vec3::ZERO)).is_none());
}

const SLAB: (Vec3, Vec3) = (Vec3::new(0., 0., 0.), Vec3::new(64., 8., 128.));

#[test]
fn a_door_reaches_from_its_spawn_bounds_to_where_it_slides() {
    // Yaw 0 slides along +x by its width less the default lip of 8.
    let door = entity(&[("classname", "func_door"), ("angle", "0")]);
    assert_eq!(
        reach(Some(&door), SLAB.0, SLAB.1),
        (Vec3::ZERO, Vec3::new(64. + 56., 8., 128.))
    );
    // -2 is down; a negative lip travels farther than the door's height.
    let lift = entity(&[
        ("classname", "func_door"),
        ("angle", "-2"),
        ("lip", "-72"),
        ("origin", "100 0 0"),
    ]);
    assert_eq!(
        reach(Some(&lift), SLAB.0, SLAB.1),
        (Vec3::new(100., 0., -200.), Vec3::new(164., 8., 128.))
    );
    let up = entity(&[("classname", "func_door"), ("angle", "-1"), ("lip", "0")]);
    assert_eq!(reach(Some(&up), SLAB.0, SLAB.1).1, Vec3::new(64., 8., 256.));
}

#[test]
fn a_plat_drops_by_its_height_and_still_brushes_keep_their_bounds() {
    let plat = entity(&[("classname", "func_plat"), ("height", "100")]);
    assert_eq!(
        reach(Some(&plat), SLAB.0, SLAB.1),
        (Vec3::new(0., 0., -100.), SLAB.1)
    );
    let wall = entity(&[("classname", "func_static"), ("origin", "0 0 10")]);
    assert_eq!(
        reach(Some(&wall), SLAB.0, SLAB.1),
        (Vec3::new(0., 0., 10.), Vec3::new(64., 8., 138.))
    );
}

#[test]
fn movers_that_never_stop_are_endless() {
    for class in ["func_bobbing", "func_rotating", "func_pendulum"] {
        assert!(endless(Some(&entity(&[("classname", class)]))), "{class}");
    }
    for class in [
        "func_door",
        "func_plat",
        "func_train",
        "func_static",
        "func_usable",
    ] {
        assert!(!endless(Some(&entity(&[("classname", class)]))), "{class}");
    }
    assert!(!endless(None));
}

#[test]
fn unknown_movers_grow_by_their_largest_extent() {
    let train = entity(&[("classname", "func_train")]);
    assert_eq!(
        reach(Some(&train), SLAB.0, SLAB.1),
        (Vec3::splat(-128.), Vec3::new(192., 136., 256.))
    );
    assert_eq!(
        reach(None, SLAB.0, SLAB.1),
        reach(Some(&train), SLAB.0, SLAB.1)
    );
}

#[test]
fn only_lamps_reaching_a_mover_get_door_tiles() {
    let lamps = [
        lamp([32., -100., 64.], 200., 1.),
        lamp([5000., 0., 0.], 300., 1.),
        lamp([32., 200., 64.], 250., 1.),
    ];
    let doors = Doors::assign(&lamps, &[door(3, [0.; 3])], 16);
    assert_eq!(
        doors.tiles,
        vec![
            Tile {
                lamp: 0,
                occluders: vec![0]
            },
            Tile {
                lamp: 2,
                occluders: vec![0]
            },
        ]
    );
    assert_eq!(doors.by_occluder, vec![vec![0, 1]]);
}

#[test]
fn over_capacity_the_most_powerful_lamps_keep_their_tiles_in_lamp_order() {
    let lamps = [
        lamp([0., -50., 64.], 200., 1.),
        lamp([0., 50., 64.], 200., 9.),
        lamp([64., 50., 64.], 200., 5.),
    ];
    let doors = Doors::assign(&lamps, &[door(0, [0.; 3])], 2);
    let kept: Vec<u32> = doors.tiles.iter().map(|tile| tile.lamp).collect();
    assert_eq!(kept, vec![1, 2]);
    assert_eq!(doors.by_occluder, vec![vec![0, 1]]);
}

#[test]
fn a_lamp_near_two_movers_lists_both() {
    let lamps = [lamp([100., -100., 64.], 300., 1.)];
    let doors = Doors::assign(&lamps, &[door(0, [0.; 3]), door(1, [200., 0., 0.])], 4);
    assert_eq!(doors.tiles[0].occluders, vec![0, 1]);
    assert_eq!(doors.by_occluder, vec![vec![0], vec![0]]);
}

#[test]
fn a_door_tile_refreshes_the_cache_texels_behind_its_mover() {
    use crate::world_materials::lamp_cache::CachedSurface;
    let lamps = [lamp([-200., 4., 64.], 600., 1.)];
    let doors = Doors::assign(&lamps, &[door(0, [0.; 3])], 4);
    let surface = |x: f32, texels: [u32; 4]| CachedSurface {
        texels,
        lower: Vec3::new(x, -64., 0.),
        upper: Vec3::new(x + 16., 64., 128.),
    };
    // A narrow panel between the lamp and the door.
    let before = CachedSurface {
        texels: [40, 40, 50, 50],
        lower: Vec3::new(-100., -8., 0.),
        upper: Vec3::new(-90., 8., 128.),
    };
    let surfaces = vec![
        // Behind the door, and before it.
        vec![surface(200., [10, 10, 20, 20]), before],
        // Behind it on another layer, at the page's edge.
        vec![surface(300., [0, 120, 8, 128])],
        // Out of the lamp's reach.
        vec![surface(5000., [0, 0, 8, 8])],
    ];
    let regions = cache_regions(&lamps, &doors, &[door(0, [0.; 3])], &surfaces, 128);
    assert_eq!(
        regions,
        vec![vec![(0, [8, 8, 22, 22]), (1, [0, 118, 10, 128])]]
    );
    assert_eq!(merge([0, 5, 10, 10], [2, 0, 4, 20]), [0, 0, 10, 20]);
}

fn pose(origin: [f32; 3], blocking: bool) -> Pose {
    Pose {
        origin: Vec3::from_array(origin),
        rotation: Quat::IDENTITY,
        blocking,
    }
}

#[test]
fn a_still_mover_queues_nothing_and_a_moved_one_queues_its_tiles_once() {
    let doors = Doors {
        tiles: Vec::new(),
        by_occluder: vec![vec![4, 7], vec![9]],
    };
    let mut queue = Queue::new(10);
    let mut poses = Poses::new(2);
    poses.set(0, pose([0.; 3], true), &doors, &mut queue);
    assert_eq!(queue.len(), 2);
    assert_eq!(poses.generation, 1);
    poses.set(0, pose([0.; 3], true), &doors, &mut queue);
    assert_eq!(queue.len(), 2);
    assert_eq!(poses.generation, 1);
    poses.set(0, pose([0., 0., 4.], true), &doors, &mut queue);
    assert_eq!(queue.len(), 2, "tiles already queued are not queued twice");
    assert_eq!(poses.generation, 2);
}

#[test]
fn hiding_a_mover_frees_its_light_and_hidden_moves_do_not_count() {
    let doors = Doors {
        tiles: Vec::new(),
        by_occluder: vec![vec![0]],
    };
    let mut queue = Queue::new(1);
    let mut poses = Poses::new(1);
    let mut batch = Vec::new();
    poses.set(0, pose([0.; 3], true), &doors, &mut queue);
    queue.take(8, &mut batch);
    poses.set(0, pose([0.; 3], false), &doors, &mut queue);
    assert_eq!(queue.len(), 1);
    queue.take(8, &mut batch);
    poses.set(0, pose([50.; 3], false), &doors, &mut queue);
    assert_eq!(queue.len(), 0);
    // Shown again where it went while hidden: traced there.
    poses.set(0, pose([50.; 3], true), &doors, &mut queue);
    assert_eq!(queue.len(), 1);
    assert_eq!(poses.current[0].origin, Vec3::splat(50.));
}

#[test]
fn slow_motion_adds_up_to_a_trace() {
    let doors = Doors {
        tiles: Vec::new(),
        by_occluder: vec![vec![0]],
    };
    let mut queue = Queue::new(1);
    let mut poses = Poses::new(1);
    let mut batch = Vec::new();
    poses.set(0, pose([0.; 3], true), &doors, &mut queue);
    queue.take(8, &mut batch);
    // A lift at 2 units a second drawn at 500 fps: 0.004 a frame, under the threshold.
    let mut traces = 0;
    for frame in 1..=250 {
        poses.set(
            0,
            pose([0., 0., frame as f32 * 0.004], true),
            &doors,
            &mut queue,
        );
        if queue.len() > 0 {
            traces += 1;
            queue.take(8, &mut batch);
        }
        let behind = frame as f32 * 0.004 - poses.current[0].origin.z;
        assert!(behind <= 0.0101, "frame {frame}: traced {behind} behind");
    }
    assert!(traces >= 80, "{traces} traces over one unit");
    // Turning 0.002 degrees a frame from yaw 90, a sixth of the threshold.
    let mut traces = 0;
    poses.set(0, turned_pose([0., 90., 0.]), &doors, &mut queue);
    queue.take(8, &mut batch);
    for frame in 1..=500 {
        poses.set(
            0,
            turned_pose([0., 90. + frame as f32 * 0.002, 0.]),
            &doors,
            &mut queue,
        );
        if queue.len() > 0 {
            traces += 1;
            queue.take(8, &mut batch);
        }
    }
    assert!(traces >= 40, "{traces} traces over one degree");
}

#[test]
fn a_baseline_places_only_movers_no_snapshot_has_shown() {
    let doors = Doors {
        tiles: Vec::new(),
        by_occluder: vec![vec![0], vec![1]],
    };
    let mut queue = Queue::new(2);
    let mut poses = Poses::new(2);
    assert!(poses.any_unknown());
    poses.set(0, pose([0., 0., 90.], true), &doors, &mut queue);
    poses.set_unknown(0, pose([0.; 3], true), &doors, &mut queue);
    poses.set_unknown(1, pose([0.; 3], true), &doors, &mut queue);
    assert_eq!(poses.current[0].origin.z, 90.);
    assert!(poses.current[1].blocking);
    assert!(!poses.any_unknown());
}

#[test]
fn the_queue_hands_out_tiles_oldest_first_within_the_budget() {
    let mut queue = Queue::new(8);
    for tile in [3, 1, 3, 5, 7] {
        queue.push(tile);
    }
    let mut batch = Vec::new();
    queue.take(2, &mut batch);
    assert_eq!(batch, vec![3, 1]);
    queue.push(3);
    queue.take(8, &mut batch);
    assert_eq!(batch, vec![5, 7, 3]);
    queue.push(99);
    assert_eq!(queue.len(), 0, "tiles outside the atlas are ignored");
}

#[test]
fn rotation_sign_does_not_count_as_a_move() {
    let a = Pose {
        origin: Vec3::ZERO,
        rotation: Quat::from_rotation_z(0.5),
        blocking: true,
    };
    let b = Pose {
        rotation: -a.rotation,
        ..a
    };
    assert!(!a.moved(&b));
    let c = Pose {
        rotation: Quat::from_rotation_z(0.6),
        ..a
    };
    assert!(a.moved(&c));
}

/// A pose drawn at `angles` (pitch, yaw, roll), as snapshot movers are presented.
fn turned_pose(angles: [f32; 3]) -> Pose {
    Pose {
        origin: Vec3::ZERO,
        rotation: Quat::from_array(sjk_client::legacy_angles_to_quaternion(angles)),
        blocking: true,
    }
}

#[test]
fn a_still_turned_mover_is_not_moving_and_a_hundredth_of_a_degree_is() {
    // Yaw 90 and 270 give a quaternion whose dot with itself is one step below 1 in f32.
    for yaw in [90., 270.] {
        let still = turned_pose([0., yaw, 0.]);
        assert!(!still.moved(&still), "yaw {yaw}");
        assert!(
            still.moved(&turned_pose([0., yaw + 0.02, 0.])),
            "yaw {yaw} + 0.02"
        );
        assert!(
            !still.moved(&turned_pose([0., yaw + 0.005, 0.])),
            "yaw {yaw} + 0.005"
        );
    }
    for pitch in [0., 15., 30., 45., 90.] {
        for yaw in 0..360 {
            let still = turned_pose([pitch, yaw as f32, 0.]);
            assert!(!still.moved(&still), "pitch {pitch} yaw {yaw}");
        }
    }
}

#[test]
fn only_lamps_that_see_into_the_reach_past_the_world_count() {
    let lamps = [
        lamp([-100., 4., 64.], 400., 1.),
        lamp([500., 4., 64.], 600., 1.),
        lamp([5000., 0., 0.], 100., 1.),
    ];
    // A wall at x = 300 hides the box from the second lamp.
    let seen = seen_by(
        &lamps,
        (Vec3::ZERO, Vec3::new(64., 8., 128.)),
        |from, to| (from.x - 300.) * (to.x - 300.) < 0.,
    );
    assert_eq!(seen, vec![0]);
    let mut occluder = door(0, [0.; 3]);
    occluder.seen_by = Some(seen);
    let doors = Doors::assign(&lamps, &[occluder], 8);
    assert_eq!(doors.tiles.len(), 1);
    assert_eq!(doors.tiles[0].lamp, 0);
}

#[test]
fn only_cells_behind_a_mover_from_the_lamp_can_be_in_its_shadow() {
    let lamp = Vec3::new(-200., 4., 64.);
    let (lower, upper) = (Vec3::new(0., 0., 0.), Vec3::new(8., 8., 128.));
    let cell = |x: f32, y: f32| (Vec3::new(x, y, 32.), Vec3::new(x + 64., y + 64., 96.));
    // Straight behind the slab.
    let (a, b) = cell(100., -28.);
    assert!(may_shadow(lamp, lower, upper, a, b));
    // Between the lamp and the slab.
    let (a, b) = cell(-150., -28.);
    assert!(!may_shadow(lamp, lower, upper, a, b));
    // Far off to the side.
    let (a, b) = cell(100., 600.);
    assert!(!may_shadow(lamp, lower, upper, a, b));
    // A lamp inside the box shadows anywhere.
    assert!(may_shadow(Vec3::new(4., 4., 64.), lower, upper, a, b));
}

/// Mover `model` drawn at `origin`, or hidden.
fn presented(model: usize, origin: [f32; 3], visible: bool) -> crate::movers::Presented {
    crate::movers::Presented {
        entity_number: 100 + model as u16,
        model_index: model,
        origin,
        angles: [0.; 3],
        rotation: [0., 0., 0., 1.],
        visible,
    }
}

#[test]
fn movers_without_door_tiles_still_count_their_moves_for_the_far_cascade() {
    // Inline model 7 is the catalog's mesh 2; no lamp gave it a door tile.
    let mut tracking = Tracking::new(&[door(2, [0.; 3])], Doors::none(1));
    let mesh_of = |model: usize| (model == 7).then_some(2);
    let none = || None::<std::iter::Empty<(crate::movers::Presented, bool)>>;
    tracking.observe(&[presented(7, [0.; 3], true)], none(), mesh_of, NO_EYE);
    let placed = tracking.poses.generation;
    assert_eq!(placed, 1);
    tracking.observe(&[presented(7, [0.; 3], true)], none(), mesh_of, NO_EYE);
    assert_eq!(tracking.poses.generation, placed, "still");
    tracking.observe(
        &[presented(7, [0., 0., 16.], true)],
        none(),
        mesh_of,
        NO_EYE,
    );
    assert_eq!(tracking.poses.generation, placed + 1, "moved");
    assert_eq!(tracking.queue.len(), 0, "no tiles to trace");
    // Door tiles from other occluders cannot index this one.
    let mismatched = Tracking::new(&[door(2, [0.; 3])], Doors::none(3));
    assert_eq!(mismatched.doors.by_occluder.len(), 1);
}

/// No snapshot behind the presented movers.
const NO_EYE: Option<fn(&Sight) -> bool> = None;

#[test]
fn a_mover_missing_from_a_snapshot_that_would_hold_it_stops_blocking() {
    // Mesh m is inline model 10 + m. The eye sees clusters 0 and 1, through area 0.
    let clusters = [0, 0, 0, 3, 1];
    let occluders: Vec<Occluder> = (0..5)
        .map(|mesh| {
            let mut occluder = door(mesh, [0.; 3]);
            occluder.sight = Sight::of(&[clusters[mesh]], &[0]);
            occluder
        })
        .collect();
    let doors = Doors {
        tiles: (0..5)
            .map(|i| Tile {
                lamp: i,
                occluders: vec![i],
            })
            .collect(),
        by_occluder: (0..5).map(|i| vec![i]).collect(),
    };
    let mut tracking = Tracking::new(&occluders, doors);
    let mut batch = Vec::new();
    let mesh_of = |model: usize| model.checked_sub(10).filter(|&mesh| mesh < 5);
    let eye = || Some(|sight: &Sight| sight.seen(|cluster| cluster < 2, |area| area == 0));
    // Only in the baselines: 12 is EF_PERMANENT, 13 stands out of view, 14 in view.
    let baselines = || {
        Some(
            [
                (presented(12, [0.; 3], true), true),
                (presented(13, [0.; 3], true), false),
                (presented(14, [0.; 3], true), false),
            ]
            .into_iter(),
        )
    };
    let blocking = |tracking: &Tracking| -> Vec<bool> {
        tracking.poses.current.iter().map(|p| p.blocking).collect()
    };
    let shown = [presented(10, [0.; 3], true), presented(11, [0.; 3], true)];
    tracking.observe(&shown, baselines(), mesh_of, eye());
    assert_eq!(
        blocking(&tracking),
        [true, true, true, true, false],
        "a never-sent mover whose place is in view was removed before we came"
    );
    tracking.queue.take(8, &mut batch);
    // 10 leaves the snapshot (func_usable switched off: SVF_NOCLIENT) though in view.
    let generation = tracking.poses.generation;
    tracking.observe(&shown[1..], baselines(), mesh_of, eye());
    assert_eq!(blocking(&tracking), [false, true, true, true, false]);
    assert_eq!(tracking.queue.len(), 1, "its door tile is traced again");
    assert_eq!(
        tracking.poses.generation,
        generation + 1,
        "the far cascade follows"
    );
    tracking.queue.take(8, &mut batch);
    // Frames without a snapshot take nothing away.
    tracking.observe(&[], baselines(), mesh_of, NO_EYE);
    assert_eq!(blocking(&tracking), [false, true, true, true, false]);
    // Sent again: it blocks again.
    tracking.observe(&shown, baselines(), mesh_of, eye());
    assert_eq!(blocking(&tracking), [true, true, true, true, false]);
}

#[test]
fn a_snapshot_holds_a_mover_with_a_visible_cluster_and_an_open_area() {
    let sight = Sight::of(&[4, 9], &[2]);
    assert!(sight.seen(|cluster| cluster == 9, |_| true));
    assert!(
        !sight.seen(|cluster| cluster == 5, |_| true),
        "out of the PVS"
    );
    assert!(
        !sight.seen(|_| true, |area| area != 2),
        "behind a closed area portal"
    );
    assert!(!Sight::default().seen(|_| true, |_| true), "in the void");
    // Past 16 clusters only the named ones count; past 4 areas any area will do.
    let crowded = Sight::of(&(0..20).collect::<Vec<u32>>(), &[0, 1, 2, 3, 4]);
    assert!(crowded.seen(|cluster| cluster == 15, |_| false));
    assert!(!crowded.seen(|cluster| cluster == 17, |_| true));
    // From a map: a synthetic map's leaves are all cluster 0, area 0.
    use sjk_bsp::{CollisionShader, box_brush, write_collision_map};
    let shader = CollisionShader {
        name: "textures/stone".into(),
        surface_flags: 0,
        content_flags: 1,
    };
    let map = write_collision_map(
        "{\n\"classname\" \"worldspawn\"\n}\n",
        &[shader],
        &[box_brush([-512., -512., -64.], [512., 512., 0.], 0)],
    );
    let bsp = sjk_bsp::Bsp::parse(&map).expect("synthetic map parses");
    let sight = Sight::new(&bsp, Vec3::new(0., 0., 8.), Vec3::new(64., 8., 136.));
    assert_eq!(sight, Sight::of(&[0], &[0]));
    assert!(sight.seen(|cluster| cluster == 0, |area| area == 0));
}

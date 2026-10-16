use super::*;

/// A stand-in for ground movement: the velocity follows the movement keys at once
/// (full speed 250, scaled by the largest key as `PM_CmdScale` does) and the origin
/// moves by it over the command's duration. Enough to check that a replay steers its
/// pose back onto the recorded one; Pmove itself is not under test.
fn simulate(pose: &mut Pose, command: &UserCommand, millis: i32) {
    let yaw = short_to_degrees(command.angles[1].wrapping_add(pose.delta_angles[1])).to_radians();
    let (forward, right) = (
        f32::from(command.forward_move),
        f32::from(command.right_move),
    );
    let size = forward.abs().max(right.abs());
    let wish = [
        forward * yaw.cos() + right * yaw.sin(),
        forward * yaw.sin() - right * yaw.cos(),
    ];
    let length = wish[0].hypot(wish[1]);
    pose.velocity = if length > 0.0 {
        let speed = 250.0 * size / 127.0;
        [wish[0] / length * speed, wish[1] / length * speed, 0.0]
    } else {
        [0.0; 3]
    };
    let seconds = millis as f32 / 1000.0;
    pose.origin[0] += pose.velocity[0] * seconds;
    pose.origin[1] += pose.velocity[1] * seconds;
    pose.view_angles[1] = yaw.to_degrees();
    pose.weapon = command.weapon;
}

fn standing() -> Pose {
    Pose {
        on_ground: true,
        speed: 250.0,
        weapon: 3,
        delta_angles: [0, 1000, 0],
        ..Pose::default()
    }
}

fn input(stamp: i32) -> UserCommand {
    UserCommand {
        server_time: stamp,
        weapon: 3,
        ..UserCommand::default()
    }
}

/// Record `steps` commands from `keys(step)` with a view turning slowly left.
fn record(steps: i32, keys: impl Fn(i32) -> (i8, i8, i8, u8)) -> Run {
    let mut ghost = Ghost {
        mode: Mode::Recording(Recording {
            name: "test".into(),
            run: Run {
                map: "mp/ffa3".into(),
                ..Run::default()
            },
            first_stamp: None,
        }),
        ..Ghost::default()
    };
    let mut pose = standing();
    for step in 0..steps {
        let (forward, right, up, generic) = keys(step);
        let command = UserCommand {
            server_time: 5_000 + step * 8,
            angles: [0, (step * 20) - pose.delta_angles[1], 0],
            forward_move: forward,
            right_move: right,
            up_move: up,
            generic_command: generic,
            ..input(0)
        };
        let before = pose;
        simulate(&mut pose, &command, 8);
        let after = sjk_game_jka::pmove::MovementState {
            origin: pose.origin,
            velocity: pose.velocity,
            ground_entity_number: 0,
            ..Default::default()
        };
        ghost.record(&command, &before, Some(&after));
    }
    let Mode::Recording(recording) = ghost.mode else {
        panic!("still recording");
    };
    recording.run
}

fn playing(run: Run) -> Ghost {
    Ghost {
        mode: Mode::Playing(Box::new(Playback {
            name: "test".into(),
            run,
            first_stamp: None,
            warp: 0,
            cursor: 0,
            consumed: None,
            commands_since_warp: 0,
            corrections: 0,
            worst_drift: 0.0,
        })),
        ..Ghost::default()
    }
}

/// Horizontal distance from `point` to the recorded path (its origins joined by
/// segments): a replay whose clock waited is on the path, just later.
fn distance_to_path(run: &Run, point: [f32; 3]) -> f32 {
    run.frames
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0].origin, pair[1].origin);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let ap = [point[0] - a[0], point[1] - a[1]];
            let length = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if length > 0.0 {
                ((ap[0] * ab[0] + ap[1] * ab[1]) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (ap[0] - t * ab[0]).hypot(ap[1] - t * ab[1])
        })
        .fold(f32::INFINITY, f32::min)
}

/// Play `ghost` from `pose` at the given command step until it stops; returns the
/// commands sent and the largest distance from the path over the last quarter of the
/// run.
fn play(ghost: &mut Ghost, pose: &mut Pose, step: i32, run: &Run) -> (Vec<UserCommand>, f32) {
    let mut sent = Vec::new();
    let mut late_drift = 0.0_f32;
    let mut stamp = 90_000;
    while let Some(drive) = ghost.drive(&input(stamp), pose) {
        simulate(pose, &drive.command, step);
        sent.push(drive.command);
        stamp += step;
        let elapsed = stamp - 90_000;
        if elapsed > run.frames.last().unwrap().time * 3 / 4 {
            late_drift = late_drift.max(distance_to_path(run, pose.origin));
        }
    }
    (sent, late_drift)
}

#[test]
fn an_exact_start_replays_the_recorded_commands_unchanged() {
    let run = record(250, |step| {
        (127, if step % 50 < 25 { 0 } else { -127 }, 0, 0)
    });
    let mut ghost = playing(run.clone());
    let mut pose = standing();
    let (sent, _) = play(&mut ghost, &mut pose, 8, &run);
    assert_eq!(sent.len(), run.frames.len(), "plays to the end, then stops");
    for (command, frame) in sent.iter().zip(&run.frames) {
        assert_eq!(
            (command.forward_move, command.right_move),
            (frame.forward, frame.right)
        );
        // The server sees the recorded view, whatever this player's delta.
        assert_eq!(
            command.angles[1].wrapping_add(pose.delta_angles[1]) & 0xFFFF,
            frame.view[1]
        );
    }
    let end = run.frames.last().unwrap().origin;
    assert!(steer::horizontal([end[0] - pose.origin[0], end[1] - pose.origin[1], 0.0]) < 0.01);
    assert!(
        ghost
            .take_messages()
            .iter()
            .any(|line| line.contains("played to the end"))
    );
}

#[test]
fn a_start_off_the_line_is_steered_back_at_every_command_step() {
    let run = record(375, |step| {
        (127, if step % 100 < 50 { 0 } else { 127 }, 0, 0)
    });
    for step in [8, 7, 4, 3] {
        let mut ghost = playing(run.clone());
        let mut pose = standing();
        pose.origin = [-2.0, 6.0, 0.0];
        let (_, late_drift) = play(&mut ghost, &mut pose, step, &run);
        let messages = ghost.take_messages();
        // Steering stops inside its deadband: a small constant offset stays.
        assert!(late_drift < 2.0, "{step} ms: still {late_drift} units off");
        assert!(
            messages
                .iter()
                .any(|line| line.contains("played to the end")),
            "{step} ms: {messages:?}"
        );
    }
}

#[test]
fn presses_in_skipped_commands_ride_on_the_next_one() {
    // A one-command jump and a one-command Force press that a 16 ms step passes over.
    let run = record(20, |step| match step {
        5 => (127, 0, 127, 0),
        9 => (127, 0, 0, 7),
        _ => (127, 0, 0, 0),
    });
    let mut ghost = playing(run.clone());
    let mut pose = standing();
    let (sent, _) = play(&mut ghost, &mut pose, 16, &run);
    assert!(sent.iter().any(|command| command.up_move > 0));
    assert_eq!(
        sent.iter()
            .filter(|command| command.generic_command == 7)
            .count(),
        1
    );
}

#[test]
fn pressing_a_key_or_drifting_away_stops_the_replay() {
    let run = record(100, |_| (127, 0, 0, 0));
    let mut ghost = playing(run.clone());
    let pose = standing();
    assert!(ghost.drive(&input(1_000), &pose).is_some());
    let mut pressed = input(1_008);
    pressed.buttons = 1;
    assert!(ghost.drive(&pressed, &pose).is_none());
    assert!(!ghost.driving());
    assert!(ghost.take_messages()[0].contains("took control"));

    // The walking and talk bits are the client's own, not a key.
    let mut ghost = playing(run.clone());
    let mut passive = input(1_000);
    passive.buttons = PASSIVE_BUTTONS;
    assert!(ghost.drive(&passive, &pose).is_some());

    let mut ghost = playing(run);
    let mut far = standing();
    far.origin = [0.0, 100.0, 0.0];
    assert!(ghost.drive(&input(1_000), &far).is_none());
    assert!(ghost.take_messages()[0].contains("lost the run"));
}

#[test]
fn the_walk_reaches_the_start_faces_it_and_begins() {
    let run = record(50, |_| (127, 0, 0, 0));
    let mut ghost = Ghost {
        mode: Mode::Approaching(Box::new(Approach {
            playback: match playing(run.clone()).mode {
                Mode::Playing(playback) => *playback,
                _ => unreachable!(),
            },
            first_stamp: None,
            settled: 0,
        })),
        ..Ghost::default()
    };
    let mut pose = standing();
    pose.origin = [40.0, -30.0, 0.0];
    pose.view_angles = [0.0, 123.0, 0.0];
    let mut stamp = 0;
    while matches!(ghost.mode, Mode::Approaching(_)) {
        let drive = ghost.drive(&input(stamp), &pose).expect("walking");
        simulate(&mut pose, &drive.command, 8);
        stamp += 8;
        assert!(stamp < 10_000, "never arrived");
    }
    assert!(matches!(ghost.mode, Mode::Playing(_)));
    assert!(steer::horizontal(pose.origin) < 4.0, "{:?}", pose.origin);
    assert!(ghost.take_messages()[0].contains("playing test"));
}

#[test]
fn a_far_start_asks_to_go_there() {
    let run = record(10, |_| (127, 0, 0, 0));
    let directory = std::env::temp_dir().join(format!("sjk-ghost-{}", std::process::id()));
    let ghosts = ghost_directory(&directory);
    std::fs::create_dir_all(&ghosts).unwrap();
    std::fs::write(ghosts.join("far.ghost"), run.to_text()).unwrap();
    let mut player = PlayerState::default();
    player.stats[0] = 100; // Alive.
    let mut pose = standing();
    pose.origin = [500.0, 0.0, 0.0];
    let mut ghost = Ghost::default();
    let args = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
    let refused = ghost
        .command(
            &args(&["play", "far"]),
            &directory,
            frame::Place {
                map: Some("mp/ffa3"),
                player: Some(&player),
                pose: Some(pose),
                can_drive: true,
                mimicking: false,
            },
        )
        .unwrap_err();
    assert!(refused.contains("teleport there"), "{refused}");
    let wrong_map = ghost
        .command(
            &args(&["play", "far"]),
            &directory,
            frame::Place {
                map: Some("mp/duel6"),
                player: Some(&player),
                pose: Some(standing()),
                can_drive: true,
                mimicking: false,
            },
        )
        .unwrap_err();
    assert!(wrong_map.contains("recorded on mp/ffa3"), "{wrong_map}");
    let listed = ghost
        .command(&args(&["list"]), &directory, frame::Place::default())
        .unwrap();
    assert!(listed[0].starts_with("far: mp/ffa3"), "{listed:?}");
    std::fs::remove_dir_all(&directory).unwrap();
}

#[path = "pillar_tests.rs"]
mod pillar_tests;

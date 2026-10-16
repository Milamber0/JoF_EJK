//! The real `Pmove` on a synthetic course: a start platform, pillars over a pit and an
//! end platform. A scripted run jumps from pillar to pillar and is recorded; replays
//! must land on every pillar.

use super::*;
use sjk_client::pmove::{MovementCollision, MovementConfig, MovementTrace, Predictor};

/// Pillars' centres along +x and their half width; tops at z 0, the pit far below.
const PILLARS: [f32; 4] = [160.0, 310.0, 460.0, 610.0];
const HALF: f32 = 32.0;
const PIT: f32 = -40.0;

struct Course {
    boxes: Vec<([f32; 3], [f32; 3])>,
}

impl Course {
    fn new() -> Self {
        let mut boxes = vec![
            ([-300.0, -200.0, -64.0], [24.0, 200.0, 0.0]),
            ([676.0, -200.0, -64.0], [1100.0, 200.0, 0.0]),
            ([-2000.0, -2000.0, -600.0], [2000.0, 2000.0, -512.0]),
        ];
        for centre in PILLARS {
            boxes.push(([centre - HALF, -HALF, -512.0], [centre + HALF, HALF, 0.0]));
        }
        Self { boxes }
    }
}

impl MovementCollision for Course {
    /// A swept box against the course's boxes (each grown by the player's box).
    fn trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        _: u32,
    ) -> MovementTrace {
        let mut best = MovementTrace::miss(end);
        let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        for (low, high) in &self.boxes {
            let low = [low[0] - maxs[0], low[1] - maxs[1], low[2] - maxs[2]];
            let high = [high[0] - mins[0], high[1] - mins[1], high[2] - mins[2]];
            let inside = (0..3).all(|i| start[i] > low[i] && start[i] < high[i]);
            if inside {
                return MovementTrace {
                    fraction: 0.0,
                    end_position: start,
                    start_solid: true,
                    all_solid: true,
                    entity_number: 1022,
                    ..MovementTrace::miss(start)
                };
            }
            let (mut enter, mut exit, mut normal) = (-1.0_f32, 1.0_f32, [0.0; 3]);
            let mut hit = true;
            for i in 0..3 {
                if delta[i].abs() < 1e-6 {
                    if start[i] < low[i] || start[i] > high[i] {
                        hit = false;
                        break;
                    }
                    continue;
                }
                let (near, far, side) = if delta[i] > 0.0 {
                    (low[i], high[i], -1.0)
                } else {
                    (high[i], low[i], 1.0)
                };
                let t_near = (near - start[i]) / delta[i];
                let t_far = (far - start[i]) / delta[i];
                if t_near > enter {
                    enter = t_near;
                    normal = [0.0; 3];
                    normal[i] = side;
                }
                exit = exit.min(t_far);
            }
            if !hit || enter > exit || enter < 0.0 || enter >= best.fraction {
                continue;
            }
            // Stop a little short of the surface, as the engine's traces do.
            let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            let fraction = (enter - 0.03125 / length.max(1e-6)).max(0.0);
            best = MovementTrace {
                fraction,
                end_position: std::array::from_fn(|i| start[i] + delta[i] * fraction),
                plane_normal: normal,
                entity_number: 1022,
                ..MovementTrace::miss(end)
            };
        }
        best
    }
}

/// A player standing at `origin` whose last command ran at `command_time`.
fn runner(origin: [f32; 3], command_time: i32) -> Predictor {
    let mut player = PlayerState::default();
    player.set_command_time(command_time);
    player.stats[0] = 100;
    player.set_origin(origin);
    player.set_gravity(800);
    player.set_speed(250.0);
    player.set_base_speed(250);
    player.set_ground_entity_num(1022);
    Predictor::from_player_state(&player, MovementConfig::default())
}

/// Commands at 125 a second for `seconds`, from `first` stamp.
fn stamps(first: i32, count: usize) -> impl Iterator<Item = i32> {
    (0..count as i32).map(move |i| first + i * 8)
}

/// The scripted perfect run: forward, and a jump (held 80 ms) whenever past a takeoff point
/// (a stock jump at full speed carries 148 units, so each lands near the next top's
/// centre and takes off again at once).
fn record_run(course: &Course) -> Run {
    let mut ghost = Ghost {
        mode: Mode::Recording(Recording {
            name: "pillars".into(),
            run: Run {
                map: "test".into(),
                ..Run::default()
            },
            first_stamp: None,
        }),
        ..Ghost::default()
    };
    let mut server = runner([-100.0, 0.0, 24.0], 0);
    let takeoffs = [8.0, 156.0, 306.0, 456.0, 606.0];
    // A player holds jump for a while; Pmove jumps once per press (`PMF_JUMP_HELD`).
    let mut held = 0;
    for stamp in stamps(8, 500) {
        let pose = Pose::of(server.state());
        let x = pose.origin[0];
        let want_jump = pose.on_ground
            && takeoffs
                .iter()
                .any(|&takeoff| x > takeoff && x < takeoff + 40.0);
        if want_jump && held == 0 {
            held = 10;
        }
        let up = if held > 0 { 127 } else { 0 };
        held = (held - 1).max(0);
        let command = UserCommand {
            server_time: stamp,
            angles: std::array::from_fn(|i| 0_i32.wrapping_sub(pose.delta_angles[i])),
            forward_move: if x < 760.0 { 127 } else { 0 },
            up_move: up,
            ..UserCommand::default()
        };
        server.predict_command(command, course);
        ghost.record(&command, &pose, Some(server.state()));
        assert!(server.state().origin[2] > PIT, "the script fell at x {x}");
    }
    let Mode::Recording(recording) = ghost.mode else {
        unreachable!()
    };
    let end = recording.run.frames.last().unwrap().origin;
    assert!(end[0] > 680.0, "the script never reached the end: {end:?}");
    recording.run
}

/// Replay `run` from `origin` on a server that loses the commands in `lost` (the
/// next command covers their time, as `ClientThink_real` does); returns the final
/// origin and the messages.
fn replay(course: &Course, run: &Run, origin: [f32; 3], lost: &[usize]) -> ([f32; 3], Vec<String>) {
    replay_bumped(course, run, origin, lost, None)
}

/// [`replay`] where, at command `bump.0`, something shoves the player sideways by
/// `bump.1` units a second (another player, a push).
fn replay_bumped(
    course: &Course,
    run: &Run,
    origin: [f32; 3],
    lost: &[usize],
    bump: Option<(usize, f32)>,
) -> ([f32; 3], Vec<String>) {
    let mut ghost = Ghost {
        mode: Mode::Approaching(Box::new(Approach {
            playback: Playback {
                name: "pillars".into(),
                run: run.clone(),
                first_stamp: None,
                warp: 0,
                cursor: 0,
                consumed: None,
                commands_since_warp: 0,
                corrections: 0,
                worst_drift: 0.0,
            },
            first_stamp: None,
            settled: 0,
        })),
        ..Ghost::default()
    };
    let mut server = runner(origin, 50_000 - 8);
    let mut client = server.clone();
    let mut lowest = f32::INFINITY;
    for (index, stamp) in stamps(50_000, 3_000).enumerate() {
        // The client predicts every command; the server misses the lost ones, and the
        // client learns of it at once (a snapshot with no ping, the worst case).
        let pose = Pose::of(client.state());
        let Some(drive) = ghost.drive(
            &UserCommand {
                server_time: stamp,
                ..UserCommand::default()
            },
            &pose,
        ) else {
            break;
        };
        if !lost.contains(&index) {
            server.predict_command(drive.command, course);
        }
        if let Some((at, sideways)) = bump
            && at == index
        {
            server.state_mut().velocity[1] += sideways;
        }
        client = server.clone();
        if matches!(ghost.mode, Mode::Playing(_)) {
            lowest = lowest.min(server.state().origin[2]);
        }
    }
    assert!(lowest > PIT, "fell into the pit: lowest z {lowest}");
    (server.state().origin, ghost.take_messages())
}

/// How many commands the walk to the start sends from `origin` before the run's first.
fn walk_commands(course: &Course, run: &Run, origin: [f32; 3]) -> usize {
    let mut ghost = Ghost {
        mode: Mode::Approaching(Box::new(Approach {
            playback: Playback {
                name: "pillars".into(),
                run: run.clone(),
                first_stamp: None,
                warp: 0,
                cursor: 0,
                consumed: None,
                commands_since_warp: 0,
                corrections: 0,
                worst_drift: 0.0,
            },
            first_stamp: None,
            settled: 0,
        })),
        ..Ghost::default()
    };
    let mut server = runner(origin, 50_000 - 8);
    for (count, stamp) in stamps(50_000, 3_000).enumerate() {
        let drive = ghost
            .drive(
                &UserCommand {
                    server_time: stamp,
                    ..UserCommand::default()
                },
                &Pose::of(server.state()),
            )
            .expect("walking");
        if matches!(ghost.mode, Mode::Playing(_)) {
            return count;
        }
        server.predict_command(drive.command, course);
    }
    panic!("never reached the start");
}

#[test]
fn a_recorded_pillar_run_replays_exactly_from_its_start() {
    let course = Course::new();
    let run = record_run(&course);
    let (end, messages) = replay(&course, &run, run.start.origin, &[]);
    let target = run.frames.last().unwrap().origin;
    assert!(
        steer::horizontal([target[0] - end[0], target[1] - end[1], 0.0]) < 0.01,
        "{end:?} vs {target:?}: {messages:?}"
    );
    assert!(
        messages.last().unwrap().contains("played to the end"),
        "{messages:?}"
    );
}

#[test]
fn the_replay_walks_to_the_start_and_lands_every_pillar_despite_lost_commands() {
    let course = Course::new();
    let run = record_run(&course);
    // Thirty units off the start; the server loses the command of the second takeoff
    // (the jump leaves 8 ms late) and one in the air after the third.
    let start = run.start.origin;
    let takeoffs: Vec<usize> = run
        .frames
        .windows(2)
        .enumerate()
        .filter(|(_, pair)| pair[0].up == 0 && pair[1].up > 0)
        .map(|(index, _)| index + 1)
        .collect();
    assert_eq!(takeoffs.len(), 5, "five jumps");
    // The walk to the start takes some commands first: find how many.
    let walk = walk_commands(&course, &run, [start[0] - 20.0, start[1] + 22.0, start[2]]);
    let (end, messages) = replay(
        &course,
        &run,
        [start[0] - 20.0, start[1] + 22.0, start[2]],
        &[walk + takeoffs[1], walk + takeoffs[2] + 20],
    );
    let target = run.frames.last().unwrap().origin;
    assert!(
        steer::horizontal([target[0] - end[0], target[1] - end[1], 0.0]) < 8.0,
        "{end:?} vs {target:?}: {messages:?}"
    );
    assert!(
        messages.last().unwrap().contains("played to the end"),
        "{messages:?}"
    );
}

#[test]
fn a_sideways_shove_on_the_start_platform_is_steered_out_before_the_pillars() {
    let course = Course::new();
    let run = record_run(&course);
    let start = run.start.origin;
    let walk = walk_commands(&course, &run, start);
    // A shove of 60 units a second to the left a quarter of a second into the run,
    // while still running on the start platform. Unsteered, the run ends 46 units off
    // the line (measured), on the very edge of the last top; steered, the sideways
    // speed is gone before the first jump and only a small offset stays.
    let (end, messages) = replay_bumped(&course, &run, start, &[], Some((walk + 30, 60.0)));
    let target = run.frames.last().unwrap().origin;
    let off = steer::horizontal([target[0] - end[0], target[1] - end[1], 0.0]);
    assert!(off < 8.0, "{end:?} vs {target:?}: {messages:?}");
    assert!(
        messages.last().unwrap().contains("played to the end"),
        "{messages:?}"
    );
}

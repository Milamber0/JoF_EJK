//! Stock `PM_JETPACK` flight, also used by JA+'s equipped jetpack.
//!
//! `codemp/game/bg_pmove.c`: `PmoveSingle` adjusts gravity and vertical thrust
//! before ground tracing. Thrust is per command slice, not scaled by frame time.
use super::*;

impl Predictor {
    pub(super) fn jetpack_move(
        &mut self,
        command: &UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) {
        let distance = self.ground_distance(bounds, collision);
        // The legacy gravity field is an integer: assignment truncates the product.
        self.state.gravity =
            (self.state.gravity * if distance < 128.0 { 0.1 } else { 0.25 }).trunc();
        crate::pmove_anim::continue_legs(
            &mut self.state,
            if command.right_move > 0 {
                1_149 // BOTH_INAIRRIGHT1
            } else if command.right_move < 0 {
                1_146 // BOTH_INAIRLEFT1
            } else if command.forward_move < 0 {
                1_143 // BOTH_INAIRBACK1
            } else {
                1_139 // BOTH_INAIR1
            },
        );
        let velocity = &mut self.state.velocity[2];
        if command.up_move > 0 && *velocity < 256.0 {
            let thrust = if *velocity > 0.0 {
                12.0 - distance / 64.0
            } else {
                12.0
            };
            if thrust > 0.0 {
                *velocity += thrust;
            }
            self.state.entity_flags |= 1 << 30;
        } else {
            self.state.entity_flags &= !(1 << 30);
            if *velocity < 256.0 {
                *velocity = velocity.max(-100.0);
                if distance < 64.0 {
                    *velocity += 2.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Floor;
    impl MovementCollision for Floor {
        fn trace(
            &self,
            start: [f32; 3],
            mins: [f32; 3],
            _: [f32; 3],
            end: [f32; 3],
            _: u32,
        ) -> MovementTrace {
            if end[2] + mins[2] < 0.0 {
                let fraction = (start[2] + mins[2]) / (start[2] - end[2]);
                MovementTrace {
                    fraction,
                    end_position: [end[0], end[1], -mins[2]],
                    plane_normal: [0.0, 0.0, 1.0],
                    entity_number: ENTITY_NUMBER_WORLD,
                    ..MovementTrace::miss(end)
                }
            } else {
                MovementTrace::miss(end)
            }
        }
    }

    fn mover(height: f32, velocity: f32) -> Predictor {
        let mut player = PlayerState::default();
        player.stats[0] = 100;
        player.set_movement_type(1);
        player.set_origin([0.0, 0.0, height + 24.0]);
        player.set_velocity([0.0, 0.0, velocity]);
        player.set_gravity(800);
        player.set_speed(250.0);
        player.set_base_speed(250);
        player.set_ground_entity_num(ENTITY_NUMBER_NONE);
        Predictor::from_player_state(&player, MovementConfig::default())
    }

    #[test]
    fn thrust_and_air_control_at_integer_command_steps() {
        for (millis, horizontal, vertical) in [
            (8, 4.0, 11.0),
            (7, 4.0, 11.0),
            (4, 2.0, 12.0),
            (3, 2.0, 12.0),
        ] {
            let mut predictor = mover(100.0, 0.0);
            predictor.predict_command(
                UserCommand {
                    server_time: millis,
                    forward_move: 127,
                    up_move: 127,
                    ..UserCommand::default()
                },
                &Floor,
            );
            let state = predictor.state();
            assert_eq!(state.velocity, [horizontal, 0.0, vertical], "{millis} ms");
            assert_eq!(state.gravity, 800.0, "gravity restored after slice");
            assert_eq!(state.ground_entity_number, ENTITY_NUMBER_NONE);
            assert_ne!(state.entity_flags & (1 << 30), 0);
            let seconds = frame_seconds(millis);
            assert!(
                (state.origin[2] - (124.0 + (12.0 - 40.0 * seconds) * seconds)).abs() < 0.000_02
            );
        }
    }

    #[test]
    fn hover_descent_and_thrust_boundaries() {
        let bounds = Bounds {
            minimums: [-15.0, -15.0, -24.0],
            maximums: [15.0, 15.0, 40.0],
        };
        for (height, initial, up, expected, gravity, flaming) in [
            (63.0, -200.0, 0, -98.0, 80.0, false),
            (64.0, -200.0, 0, -100.0, 80.0, false),
            (128.0, 100.0, 127, 110.0, 200.0, true),
            (768.0, 100.0, 127, 100.0, 200.0, true),
            (63.0, 256.0, 127, 256.0, 80.0, false),
            (63.0, 255.0, 127, 266.015_63, 80.0, true),
        ] {
            let mut predictor = mover(height, initial);
            predictor.jetpack_move(
                &UserCommand {
                    up_move: up,
                    ..UserCommand::default()
                },
                bounds,
                &Floor,
            );
            assert_eq!(predictor.state.velocity[2], expected);
            assert_eq!(predictor.state.gravity, gravity);
            assert_eq!(predictor.state.entity_flags & (1 << 30) != 0, flaming);
        }
    }

    #[test]
    fn idle_air_control_and_directional_poses() {
        for millis in [8, 7, 4, 3] {
            for (forward, right, animation) in [
                (127, 0, 1_139),
                (-127, 0, 1_143),
                (127, -127, 1_146),
                (127, 127, 1_149),
            ] {
                let mut predictor = mover(100.0, 0.0);
                predictor.predict_command(
                    UserCommand {
                        server_time: millis,
                        forward_move: forward,
                        right_move: right,
                        ..UserCommand::default()
                    },
                    &Floor,
                );
                assert_eq!(predictor.state.legs_anim, animation);
                assert_eq!(predictor.state.ground_entity_number, ENTITY_NUMBER_NONE);
                assert_eq!(predictor.state.gravity, 800.0);
                if right == 0 {
                    assert_eq!(
                        predictor.state.velocity[0].abs(),
                        (200.0 * frame_seconds(millis)).round_ties_even()
                    );
                }
            }
        }
    }

    #[test]
    fn jump_pads_accept_the_jetpack_movement_type() {
        let mut player = PlayerState::default();
        assert!(crate::triggers::touch_jump_pad(
            &mut player,
            crate::triggers::JumpPad {
                number: 40,
                velocity: [100.0, 0.0, 300.0]
            },
            1,
            0
        ));
        assert_eq!(player.velocity(), [100.0, 0.0, 300.0]);
        assert!(!crate::triggers::touch_jump_pad(
            &mut player,
            crate::triggers::JumpPad {
                number: 40,
                velocity: [100.0, 0.0, 300.0]
            },
            8,
            0
        ));
    }

    #[test]
    #[cfg(not(debug_assertions))]
    #[ignore = "release-only command-cost measurement"]
    fn release_command_cost() {
        for movement_type in [0, 1] {
            let mut predictor = mover(100.0, 0.0);
            predictor.state.movement_type = movement_type;
            let start = std::time::Instant::now();
            for step in 1..=100_000 {
                predictor.predict_command(
                    std::hint::black_box(UserCommand {
                        server_time: step * 8,
                        forward_move: 127,
                        up_move: 127,
                        ..UserCommand::default()
                    }),
                    &Floor,
                );
            }
            std::hint::black_box(predictor.state());
            println!(
                "movement_type {movement_type}: {:.3} microseconds/8 ms command (synthetic floor, no renderer or BSP)",
                start.elapsed().as_secs_f64() * 10.0
            );
        }
    }
}

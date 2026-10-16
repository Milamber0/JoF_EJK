//! Non-walking player movement from codemp/game/bg_pmove.c.
use super::*;

impl Predictor {
    /// Early-return types, bg_pmove.c:10678-10695: no final velocity snapping.
    pub(super) fn non_walking_move(
        &mut self,
        command: &UserCommand,
        millis: i32,
        collision: &impl MovementCollision,
    ) -> bool {
        let seconds = super::frame_seconds(millis);
        match self.state.movement_type {
            4 => {
                let collision = WithoutBodies(collision, BODY);
                let bounds = self.check_duck(command, &collision);
                self.box_bounds = (bounds.minimums, bounds.maximums);
                let ground = GroundState::air(MovementTrace::miss(self.state.origin));
                if !self.config.no_spectator_move {
                    self.fly_move(command, seconds, bounds, &ground, &collision);
                }
            }
            // An NPC (`clientNum >= MAX_CLIENTS`) under `PM_NOCLIP` moves as it would
            // walk (`bg_pmove.c:10688-10695`).
            3 if self.state.client_num < MAX_CLIENTS => self.noclip_move(command, seconds),
            _ => return false,
        }
        self.drop_timers(millis);
        true
    }

    /// PM_FlyMove, bg_pmove.c:2990-3027; shared by spectators and grip victims.
    pub(super) fn fly_move(
        &mut self,
        command: &UserCommand,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) {
        self.flight_friction(seconds, bounds, ground, collision);
        let (forward, right) = flight_axes(self.state.view_angles);
        let mut scale = self.command_scale(command);
        if self.state.movement_type == 4 && command.buttons & 128 != 0 {
            scale *= 10.0;
        }
        let wish = if scale == 0.0 {
            Vec3::Z * (self.state.speed * (f32::from(command.up_move) / 127.0))
        } else {
            scale * forward * f32::from(command.forward_move)
                + scale * right * f32::from(command.right_move)
                + Vec3::Z * (scale * f32::from(command.up_move))
        };
        self.accelerate(wish.normalize_or_zero(), wish.length(), 8.0, seconds);
        self.step_slide_move(false, seconds, bounds, ground, collision);
    }

    /// PM_Friction, bg_pmove.c:980-1090, including FLOAT's 0.1 drag.
    fn flight_friction(
        &mut self,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) {
        let mut velocity = Vec3::from_array(self.state.velocity);
        let mut measured = velocity;
        if ground.walking {
            measured.z = 0.0;
        }
        let speed = measured.length();
        if speed < 1.0 {
            velocity.x = 0.0;
            velocity.y = 0.0;
            if self.state.movement_type == 4 {
                velocity.z = 0.0;
            }
        } else {
            // Spectator returns before PM_SetWaterLevel; its level is zero.
            let water = if self.state.movement_type == 4 {
                0
            } else {
                events::water_level(&self.state, bounds, collision)
            };
            let mut drop = 0.0;
            // `FLY_NORMAL` skips the ground friction (`bg_pmove.c:1039`).
            if water <= 1
                && !self.flying_normal()
                && ground.walking
                && ground.trace.surface_flags & SURF_SLICK == 0
                && self.state.movement_flags & PMF_TIME_KNOCKBACK == 0
            {
                drop += speed.max(STOP_SPEED) * FRICTION * seconds;
            }
            if water != 0 {
                drop += speed * f32::from(water) * seconds;
            } else if self.state.ground_entity_number < 32 {
                drop = 0.0;
            }
            // Only a spectator or a floater has the flight's own drag (`bg_pmove.c:1072-1082`).
            match self.state.movement_type {
                2 => drop += speed * 0.1 * seconds,
                4 => drop += speed * 5.0 * seconds,
                _ => {}
            }
            velocity *= (speed - drop).max(0.0) / speed;
        }
        self.state.velocity = velocity.to_array();
    }

    /// `PM_CmdScale` with `upmove` counted, so jump and crouch alone fly up and down
    /// (`bg_pmove.c:5017-5035`, `pm->fakeNoclip`).
    fn fake_noclip_scale(&self, command: &UserCommand) -> f32 {
        let (forward, right, up) = (
            i32::from(command.forward_move),
            i32::from(command.right_move),
            i32::from(command.up_move),
        );
        let maximum = forward.abs().max(right.abs()).max(up.abs());
        if maximum == 0 {
            return 0.0;
        }
        let total = ((forward * forward + right * right + up * up) as f32).sqrt();
        self.state.speed * maximum as f32 / (127.0 * total)
    }

    /// PM_NoclipMove, bg_pmove.c:3487-3552: full view axes, no collision.
    fn noclip_move(&mut self, command: &UserCommand, seconds: f32) {
        self.state.view_height = STANDING_VIEW_HEIGHT;
        let velocity = Vec3::from_array(self.state.velocity);
        let speed = velocity.length();
        self.state.velocity = if speed < 1.0 {
            [0.0; 3]
        } else {
            let drop = speed.max(STOP_SPEED) * (FRICTION * 1.5) * seconds;
            (velocity * ((speed - drop).max(0.0) / speed)).to_array()
        };
        let mut scale = if self.fake_noclip {
            self.fake_noclip_scale(command)
        } else {
            self.command_scale(command)
        };
        let turbo = [1, 128]
            .into_iter()
            .any(|button| command.buttons & button != 0);
        for button in [1, 128] {
            if command.buttons & button != 0 {
                scale *= 10.0;
            }
        }
        let (forward, right) = flight_axes(self.state.view_angles);
        let wish = forward * f32::from(command.forward_move)
            + right * f32::from(command.right_move)
            + Vec3::Z * f32::from(command.up_move);
        if self.fake_noclip && turbo && wish.length() * scale > 0.0 {
            // Turbo flies exactly along the aim: `PM_Accelerate` never cancels sideways
            // speed, so aiming up at speed would barely climb (`bg_pmove.c:5063-5075`).
            self.state.velocity = (wish.normalize_or_zero() * (wish.length() * scale)).to_array();
        } else {
            self.accelerate(
                wish.normalize_or_zero(),
                wish.length() * scale,
                GROUND_ACCELERATION,
                seconds,
            );
        }
        self.state.origin = (Vec3::from_array(self.state.origin)
            + seconds * Vec3::from_array(self.state.velocity))
        .to_array();
    }
}

/// The movement's collision with some contents left out (`MASK_PLAYERSOLID &
/// ~CONTENTS_BODY` for player bodies): free spectators (cg_predict.c:1007-1008) and the dead
/// (`ClientThink_real`, `g_active.c:2823-2825`; cg_predict.c the same) collide with walls,
/// not with players.
pub(crate) struct WithoutBodies<'a, C>(pub(crate) &'a C, pub(crate) u32);

/// `CONTENTS_BODY`.
pub(crate) const BODY: u32 = 0x100;
/// `CONTENTS_BODY | CONTENTS_PLAYERCLIP`: what a player the server walks through others
/// (`GHOST_KNOWN_FLAG`) loses from the trace mask.
pub(crate) const BODY_AND_PLAYER_CLIP: u32 = 0x100 | 0x10;

impl<C: MovementCollision> MovementCollision for WithoutBodies<'_, C> {
    fn point_contents(&self, point: [f32; 3]) -> u32 {
        self.0.point_contents(point)
    }

    fn trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        self.0.trace(start, mins, maxs, end, mask & !self.1)
    }
}

/// AngleVectors forward/right rows, codemp/qcommon/q_math.c.
pub fn flight_axes(angles: [f32; 3]) -> (Vec3, Vec3) {
    // `AngleVectors` (`q_math.c:1319-1327`): degrees times a double constant, rounded
    // to float, then `sinf`/`cosf`. Converting in single precision is one ulp off for
    // some angles (120 degrees is one), which shows in the position's last bits.
    let trig = |degrees: f32| {
        ((f64::from(degrees) * (std::f64::consts::PI * 2.0 / 360.0)) as f32).sin_cos()
    };
    let ((sp, cp), (sy, cy), (sr, cr)) = (trig(angles[0]), trig(angles[1]), trig(angles[2]));
    (
        Vec3::new(cp * cy, cp * sy, -sp),
        Vec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp),
    )
}

/// `AnglesToAxis` (`q_math.c`): `AngleVectors`' forward, its right negated (left), and
/// up, with [`flight_axes`]' precision.
pub fn angles_to_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let trig = |degrees: f32| {
        ((f64::from(degrees) * (std::f64::consts::PI * 2.0 / 360.0)) as f32).sin_cos()
    };
    let ((sp, cp), (sy, cy), (sr, cr)) = (trig(angles[0]), trig(angles[1]), trig(angles[2]));
    let right = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    [
        [cp * cy, cp * sy, -sp],
        [-right[0], -right[1], -right[2]],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Everything solid, recording every content mask a move traced with.
    struct Solid(Cell<u32>);

    impl MovementCollision for Solid {
        fn trace(
            &self,
            start: [f32; 3],
            _: [f32; 3],
            _: [f32; 3],
            _: [f32; 3],
            mask: u32,
        ) -> MovementTrace {
            self.0.set(self.0.get() | mask);
            MovementTrace {
                fraction: 0.0,
                end_position: start,
                start_solid: true,
                all_solid: true,
                ..MovementTrace::miss(start)
            }
        }
    }

    fn player(force_powers_known: u32) -> sjk_protocol::PlayerState {
        let mut player = sjk_protocol::PlayerState::default();
        player.set_raw_field(51, force_powers_known);
        player.set_raw_field(37, 250); // base speed
        player
    }

    fn masks_traced(force_powers_known: u32) -> u32 {
        let player = player(force_powers_known);
        let mut predictor = Predictor::from_player_state(&player, MovementConfig::default());
        let world = Solid(Cell::new(0));
        predictor.predict_command(
            UserCommand {
                server_time: player.command_time() + 8,
                forward_move: 127,
                ..UserCommand::default()
            },
            &world,
        );
        world.0.get()
    }

    #[test]
    fn a_ghost_loses_bodies_and_player_clip_from_every_trace() {
        let normal = masks_traced(0);
        assert_eq!(normal & BODY_AND_PLAYER_CLIP, BODY_AND_PLAYER_CLIP);
        let ghost = masks_traced(crate::prediction_policy::GHOST_KNOWN_FLAG);
        assert_ne!(ghost, 0, "the move traced");
        assert_eq!(ghost & BODY_AND_PLAYER_CLIP, 0);
    }

    #[test]
    fn fake_noclip_flies_through_solid_and_fills_the_up_axis() {
        let player = player(0);
        let mut predictor = Predictor::from_player_state(&player, MovementConfig::default());
        predictor.set_fake_noclip(true);
        let start = predictor.state().origin;
        let world = Solid(Cell::new(0));
        let mut time = player.command_time();
        for _ in 0..10 {
            time += 8;
            // Jump alone: a plain noclip's scale ignores it and would stand still.
            predictor.predict_command(
                UserCommand {
                    server_time: time,
                    up_move: 127,
                    ..UserCommand::default()
                },
                &world,
            );
        }
        let state = predictor.state();
        assert_eq!(state.movement_type, 3);
        assert!(
            state.origin[2] > start[2] + 1.0,
            "flew up: {:?}",
            state.origin
        );
    }

    #[test]
    fn fake_noclip_turbo_follows_the_aim_exactly() {
        let player = player(0);
        let mut predictor = Predictor::from_player_state(&player, MovementConfig::default());
        predictor.set_fake_noclip(true);
        predictor.predict_command(
            UserCommand {
                server_time: player.command_time() + 8,
                forward_move: 127,
                buttons: 1,
                ..UserCommand::default()
            },
            &Solid(Cell::new(0)),
        );
        // 250 * 10 along the view, the aim being straight ahead (+X).
        let velocity = predictor.state().velocity;
        assert!((velocity[0] - 2500.0).abs() < 1.0, "{velocity:?}");
        assert!(velocity[1].abs() < 1e-3 && velocity[2].abs() < 1e-3);
    }
}

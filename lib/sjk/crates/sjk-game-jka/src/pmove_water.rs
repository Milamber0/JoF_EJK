//! On-foot liquid movement, OpenJK codemp/game/bg_pmove.c.
use super::*;

/// `EV_WATER_TOUCH`, `EV_WATER_LEAVE`, `EV_WATER_UNDER`, `EV_WATER_CLEAR`.
const EV_WATER_TOUCH: u16 = 18;
const EV_WATER_LEAVE: u16 = 19;
const EV_WATER_UNDER: u16 = 20;
const EV_WATER_CLEAR: u16 = 21;

/// MASK_WATER: WATER | SLIME | LAVA, codemp/game/bg_public.h.
pub(super) const MASK_WATER: u32 = 0x0002_0006;

/// PM_SetWaterLevel (:4253-4288), integer midpoint and complete lowest-sample contents.
pub(super) fn sample(state: &MovementState, collision: &impl MovementCollision) -> (u8, u32) {
    let sample2 = state.view_height + 24;
    let mut level = 0;
    let mut kind = 0;
    for height in [
        -23.0,
        -24.0 + (sample2 / 2) as f32,
        state.view_height as f32,
    ] {
        let mut point = state.origin;
        point[2] += height;
        let contents = collision.point_contents(point);
        if contents & MASK_WATER == 0 {
            break;
        }
        if level == 0 {
            kind = contents;
        }
        level += 1;
    }
    (level, kind)
}

impl Predictor {
    /// Refresh transient liquid state before movement and after the final ground trace.
    pub(super) fn sample_water(&mut self, collision: &impl MovementCollision) {
        (self.state.water_level, self.state.water_type) = sample(&self.state, collision);
    }

    /// `PM_WaterEvents` (`bg_pmove.c:5702-5781`): into or out of a liquid, the head under
    /// it or out of it, against the level the slice began with (`pml.previous_waterlevel`).
    /// The server's splash effect at a fast entry or exit (`EFFECT_WATER_SPLASH` and the
    /// lava's and acid's, `_GAME`) is not raised here.
    pub(super) fn water_events(&mut self) {
        let (before, now) = (self.water_entry, self.state.water_level);
        if before == 0 && now != 0 {
            self.add_event(EV_WATER_TOUCH, 0);
        }
        if before != 0 && now == 0 {
            self.add_event(EV_WATER_LEAVE, 0);
        }
        if before != 3 && now == 3 {
            self.add_event(EV_WATER_UNDER, 0);
        }
        if before == 3 && now != 3 {
            self.add_event(EV_WATER_CLEAR, 0);
        }
    }

    /// PM_Friction (:970-1090), normal player branch including wading.
    pub(super) fn friction(&mut self, seconds: f32, ground: &GroundState) {
        let mut velocity = Vec3::from_array(self.state.velocity);
        let mut measured = velocity;
        if ground.walking {
            measured.z = 0.0;
        }
        let speed = measured.length();
        if speed < 1.0 {
            velocity.x = 0.0;
            velocity.y = 0.0;
            self.state.velocity = velocity.to_array();
            return;
        }
        let mut drop = 0.0;
        let flying_vehicle = self.flying_vehicle();
        if let Some(friction) = self.vehicle_friction() {
            // A vehicle's own friction, on the ground or not (`bg_pmove.c:1002-1036`).
            if self.state.movement_flags & PMF_TIME_KNOCKBACK == 0 {
                drop += speed.max(STOP_SPEED) * friction * seconds;
            }
        } else if !flying_vehicle
            && self.state.water_level <= 1
            && ground.walking
            && ground.trace.surface_flags & SURF_SLICK == 0
            && self.state.movement_flags & PMF_TIME_KNOCKBACK == 0
        {
            drop += speed.max(STOP_SPEED) * FRICTION * seconds;
        }
        if flying_vehicle && self.state.movement_flags & PMF_TIME_KNOCKBACK == 0 {
            // A fighter's friction, anywhere (`bg_pmove.c:1053-1060`).
            drop += speed * FRICTION * seconds;
        }
        if self.state.water_level != 0 {
            drop += speed * f32::from(self.state.water_level) * seconds;
        } else if self.state.ground_entity_number < 32 && self.config.ja_plus.slides_on_players() {
            // Standing on a player is frictionless, unless a JA+ server turns the slide
            // off (`jp_slideOnPlayer` 0, [`crate::pmove_japlus`]).
            drop = 0.0;
        }
        velocity *= (speed - drop).max(0.0) / speed;
        self.state.velocity = velocity.to_array();
    }

    /// PM_CheckWaterJump (:2750-2789): level 2, unobstructed lip, no active pm_time.
    fn check_water_jump(&mut self, forward: Vec3, collision: &impl MovementCollision) -> bool {
        if self.state.movement_time != 0 || self.state.water_level != 2 {
            return false;
        }
        let flat = Vec3::new(forward.x, forward.y, 0.0).normalize_or_zero();
        let mut spot = Vec3::from_array(self.state.origin) + flat * 30.0;
        spot.z += 4.0;
        if collision.point_contents(spot.to_array()) & 1 == 0 {
            return false;
        }
        spot.z += 16.0;
        if collision.point_contents(spot.to_array()) & (1 | 0x10 | 0x100) != 0 {
            return false;
        }
        self.state.velocity = (forward * 200.0).to_array();
        self.state.velocity[2] = 350.0;
        self.state.movement_flags |= PMF_TIME_WATERJUMP;
        self.state.movement_time = 2000;
        true
    }

    /// PM_WaterJumpMove (:2802-2813): gravity step-slide PLUS post-slide gravity.
    pub(super) fn water_jump_move(
        &mut self,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) {
        self.step_slide_move(true, seconds, bounds, ground, collision);
        self.state.velocity[2] -= self.state.gravity * seconds;
        if self.state.velocity[2] < 0.0 {
            self.state.movement_flags &= !PMF_ALL_TIMES;
            self.state.movement_time = 0;
        }
    }

    /// PM_WaterMove (:2821-2890); the up-only swimming-jump block is disabled in MP.
    pub(super) fn water_move(
        &mut self,
        command: &UserCommand,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) {
        let (forward, right) = flight::flight_axes(self.state.view_angles);
        if self.check_water_jump(forward, collision) {
            self.water_jump_move(seconds, bounds, ground, collision);
            return;
        }
        self.friction(seconds, ground);
        let scale = self.command_scale(command);
        let wish = if scale == 0.0 {
            Vec3::new(0.0, 0.0, -60.0)
        } else {
            scale * forward * f32::from(command.forward_move)
                + scale * right * f32::from(command.right_move)
                + Vec3::Z * (scale * f32::from(command.up_move))
        };
        let direction = wish.normalize_or_zero();
        let speed = wish.length().min(self.state.speed * 0.5);
        if self.config.roll_rules.gametype == 7
            && self.state.movement_type == 0
            && self.state.client_num < 32
            && self.state.vehicle_entity_num == 0
        {
            // PM_Accelerate :1139-1154: Siege pushes toward the whole wish velocity.
            let push = direction * speed - Vec3::from_array(self.state.velocity);
            let amount = (4.0 * seconds * speed).min(push.length());
            self.state.velocity = (Vec3::from_array(self.state.velocity)
                + push.normalize_or_zero() * amount)
                .to_array();
        } else {
            self.accelerate(direction, speed, 4.0, seconds);
        }
        let velocity = Vec3::from_array(self.state.velocity);
        let normal = Vec3::from_array(ground.trace.plane_normal);
        if ground.ground_plane && velocity.dot(normal) < 0.0 {
            self.state.velocity = (self
                .clip_velocity(velocity, normal, OVERCLIP)
                .normalize_or_zero()
                * velocity.length())
            .to_array();
        }
        self.slide_move(false, seconds, bounds, ground, collision);
    }
}

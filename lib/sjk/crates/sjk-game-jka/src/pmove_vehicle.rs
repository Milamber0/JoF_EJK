//! What `Pmove` does differently for a vehicle NPC (`pm_entSelf->s.NPC_class ==
//! CLASS_VEHICLE`, OpenJK `codemp/game/bg_pmove.c`, `bg_slidemove.c`): the hover
//! (`PM_SetSpecialMoveValues`' `FLY_HOVER`, `PM_HoverTrace`, `PM_SetVehicleAngles`), the
//! box its definition gives it (`BG_VehicleAdjustBBoxForOrientation`), the steepest slope
//! it stands on, its friction and acceleration, the move along its own `moveDir` at its own
//! speed, no step up for a hovering one, and the game's vehicle functions where
//! `PmoveSingle` runs them (`Update`, `Animate`, `AttachRiders`, [`VehicleGame`]).
//!
//! The vehicle rides in the move ([`Predictor::set_vehicle`]); a player's move and any other
//! NPC's have none, and take none of these branches.
//!
//! A walker walks as a player's move does (its giant step, `pmove_npc`), its axes the
//! view before its `Update` turned it. A fighter flies (`FLY_VEHICLE`: `PM_FlyVehicleMove`
//! along its `moveDir`, its own friction, its box turned with it).

use super::*;
use crate::vehicle::{Vehicle, flags};
use crate::vehicle_fields::kind;

/// `pm_vehicleaccelerate`.
pub(super) const VEHICLE_ACCELERATION: f32 = 36.0;
/// `DEFAULT_MINS_2`.
const DEFAULT_MINS_2: f32 = -24.0;
/// `MASK_SOLID`; `CONTENTS_WATER | CONTENTS_SLIME | CONTENTS_LAVA`.
const MASK_SOLID: u32 = 1;
const MASK_LIQUID: u32 = 0x0002_0006;
/// `YAW`, `ROLL`.
const YAW: usize = 1;
const ROLL: usize = 2;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;

/// The game's part of a vehicle's move, which `PmoveSingle` calls on the server (`_GAME`).
pub trait VehicleGame {
    /// `Update` and `Animate` (`bg_pmove.c:10921-10951`): the vehicle's think inside its
    /// move — its ammunition and shields, its death, its steering and speed
    /// ([`crate::vehicle_move`]), its riders. `move_dir` is its `ps.moveDir`.
    ///
    /// With a pilot aboard it also holds the pilot's view (`PM_VehicleViewAngles`) and runs
    /// `UpdateRider`; a rider thrown off is traced for through `collision`, the vehicle's.
    ///
    /// `move_box` is the move's box (`pm->mins`, `pm->maxs`), which a client's fighter
    /// traces for the ground with.
    fn update(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        command: &UserCommand,
        move_dir: &mut [f32; 3],
        move_box: ([f32; 3], [f32; 3]),
        collision: &dyn MovementCollision,
    );
    /// `AttachRiders` (`bg_pmove.c:11138-11154`), after the move.
    fn attach_riders(&mut self, vehicle: &mut Vehicle, state: &MovementState);
    /// `G_CheapWeaponFire` (`g_active.c:884-935`): a piloted vehicle's fire button, from
    /// its `PM_Weapon` — the game's to fire (not predicted); `state` is the vehicle's
    /// move as it stands at the trigger.
    fn fire(&mut self, vehicle: &mut Vehicle, state: &mut MovementState, alternate: bool) {
        let _ = (vehicle, state, alternate);
    }
    /// What the vehicle's move may bump into (`PM_VehicleImpact` reads the entity it
    /// struck), written into `bodies` (cleared first) before the move; an entity it leaves
    /// out is nothing in particular. None by default.
    fn impact_bodies(&mut self, bodies: &mut Vec<super::vehicle_impact::ImpactBody>) {
        bodies.clear();
    }
    /// The game's end of a bump the move judged ([`super::vehicle_impact::JudgedImpact`]):
    /// on the server the vehicle's damage, its surfaces, what it rammed or turned; nothing
    /// for a client's prediction, whose effect is the presentation's.
    fn impact(
        &mut self,
        vehicle: &mut Vehicle,
        state: &mut MovementState,
        judged: &super::vehicle_impact::JudgedImpact,
        command_time: i32,
    ) {
        let _ = (vehicle, state, judged, command_time);
    }
    /// `PM_HoverTrace`'s splash (`bg_pmove.c:733-757`): a vehicle half out of the water at
    /// a speed draws `Q_irand(pml.frametime, 100)` — `Q_irand(0, 100)`, the frame time
    /// truncated — from the game's generator. Its wake effect is registered by clients only
    /// (`VF_EFFECT_CLIENT`), so the server raises nothing; a client's draw is its own.
    fn wake(&mut self) {}
}

impl Predictor {
    /// The vehicle this NPC's move is for, or `None` for anyone else's.
    pub fn set_vehicle(&mut self, vehicle: Option<Box<Vehicle>>) {
        self.vehicle = vehicle;
    }

    /// The vehicle back from the move.
    pub fn take_vehicle(&mut self) -> Option<Box<Vehicle>> {
        self.vehicle.take()
    }

    /// The vehicle this NPC's move is for, while it is in the move.
    pub fn vehicle(&self) -> Option<&Vehicle> {
        self.vehicle.as_deref()
    }

    /// `ps.moveDir` for a vehicle's move.
    pub fn set_move_dir(&mut self, move_dir: [f32; 3]) {
        self.move_dir = move_dir;
    }

    /// `ps.moveDir` as the move left it.
    pub fn move_dir(&self) -> [f32; 3] {
        self.move_dir
    }

    /// [`Self::predict_command_in`] for a vehicle NPC, with the game's vehicle functions
    /// ([`VehicleGame`]) where `PmoveSingle` calls them.
    pub fn predict_vehicle_command(
        &mut self,
        command: UserCommand,
        collision: &impl MovementCollision,
        context: &MoveContext,
        game: &mut dyn VehicleGame,
    ) {
        let mut outcome = crate::pmove_saber_lock::LockOutcome::default();
        game.impact_bodies(&mut self.impact_bodies);
        self.predict_with(command, collision, context, None, &mut outcome, Some(game));
        // Cleared, so a copy of the move carries none.
        self.impact_bodies.clear();
    }

    /// Whether this is a vehicle's move (`clientNum >= MAX_CLIENTS` with a vehicle).
    pub(super) fn vehicle_move(&self) -> bool {
        self.vehicle.is_some() && self.state.client_num >= MAX_CLIENTS
    }

    /// `pm_flying == FLY_HOVER` (`PM_SetSpecialMoveValues`, `bg_pmove.c:459-492`): a
    /// vehicle that is no fighter, with a hover height, not flying as an `EF2_FLYING` NPC.
    pub(super) fn hovering(&self) -> bool {
        const EF2_FLYING: u32 = 1 << 4;
        let flying = self.npc.is_some_and(|npc| npc.flags2 & EF2_FLYING != 0);
        self.vehicle_move()
            && !flying
            && self.vehicle.as_ref().is_some_and(|vehicle| {
                vehicle.kind() != kind::FIGHTER && vehicle.info.hover_height > 0.0
            })
    }

    /// The steepest ground the move stands on (`PM_GroundTrace`'s `minNormal`): a vehicle's
    /// `maxSlope`, `MIN_WALK_NORMAL` for anyone else.
    pub(super) fn min_walk_normal(&self) -> f32 {
        match &self.vehicle {
            Some(vehicle) if self.state.client_num >= MAX_CLIENTS => vehicle.info.max_slope,
            _ => MIN_WALK_NORMAL,
        }
    }

    /// `BG_VehicleAdjustBBoxForOrientation` (`bg_pmove.c:9987-10060`), where the
    /// vehicle's definition has a length, a width and a height: a fighter's or a flier's
    /// box around its nose, tail and wingtips as its orientation turns them (kept only
    /// where it is clear of everything), anyone else's the box its width and height give.
    pub(super) fn vehicle_box(&self, bounds: Bounds, collision: &impl MovementCollision) -> Bounds {
        let Some(vehicle) = self.vehicle.as_ref().filter(|_| self.vehicle_move()) else {
            return bounds;
        };
        let info = &vehicle.info;
        if info.length == 0.0 || info.width == 0.0 || info.height == 0.0 {
            return bounds;
        }
        if !matches!(info.kind, kind::FIGHTER | kind::FLIER) {
            return Bounds {
                minimums: [info.width / -2.0, info.width / -2.0, DEFAULT_MINS_2],
                maximums: [
                    info.width / 2.0,
                    info.width / 2.0,
                    info.height + DEFAULT_MINS_2,
                ],
            };
        }
        let axis = flight::angles_to_axis(vehicle.orientation);
        let origin = self.state.origin;
        let along = |from: [f32; 3], scale: f32, direction: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|at| from[at] + direction[at] * scale)
        };
        let nose = along(origin, info.length / 2.0, axis[0]);
        let tail = along(origin, -info.length / 2.0, axis[0]);
        let left = along(origin, info.width / 2.0, axis[1]);
        let right = along(origin, -info.width / 2.0, axis[1]);
        let mut points = [[0.0f32; 3]; 8];
        for (index, side) in [nose, tail, left, right].into_iter().enumerate() {
            let top = along(side, info.height / 2.0, axis[2]);
            let (first, second) = if index < 2 {
                (index, index + 2)
            } else {
                (index + 2, index + 4)
            };
            points[first] = top;
            points[second] = along(top, -info.height, axis[2]);
        }
        let (mut low, mut high) = (origin, origin);
        for at in 0..3 {
            for point in &points {
                if point[at] > high[at] {
                    high[at] = point[at];
                } else if point[at] < low[at] {
                    low[at] = point[at];
                }
            }
        }
        let minimums = std::array::from_fn(|at| low[at] - origin[at]);
        let maximums = std::array::from_fn(|at| high[at] - origin[at]);
        let found = collision.trace(origin, minimums, maximums, origin, PLAYER_CONTENT_MASK);
        if found.start_solid || found.all_solid {
            bounds
        } else {
            Bounds { minimums, maximums }
        }
    }

    /// `pm_flying == FLY_VEHICLE` (`PM_SetSpecialMoveValues`): a fighter's move, not
    /// flying as an `EF2_FLYING` NPC.
    pub(super) fn flying_vehicle(&self) -> bool {
        const EF2_FLYING: u32 = 1 << 4;
        let flying = self.npc.is_some_and(|npc| npc.flags2 & EF2_FLYING != 0);
        self.vehicle_move()
            && !flying
            && self
                .vehicle
                .as_ref()
                .is_some_and(|vehicle| vehicle.kind() == kind::FIGHTER)
    }

    /// `PM_FlyVehicleMove` (`bg_pmove.c:2893-2977`): friction (a falling fighter keeps its
    /// fall; one on the ground loses any downward speed), then toward its `moveDir` at
    /// its speed — backwards for a negative one — with an acceleration of 100, and the
    /// step-slide with gravity.
    pub(super) fn fly_vehicle_move(
        &mut self,
        seconds: f32,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
    ) {
        if self.state.gravity != 0.0
            && self.state.velocity[2] < 0.0
            && self.state.ground_entity_number == ENTITY_NUMBER_NONE
        {
            let falling = self.state.velocity[2];
            self.friction(seconds, ground);
            self.state.velocity[2] = falling;
        } else {
            self.friction(seconds, ground);
            if self.state.velocity[2] < 0.0 && self.state.ground_entity_number != ENTITY_NUMBER_NONE
            {
                self.state.velocity[2] = 0.0;
            }
        }
        let mut wish = Vec3::from_array(self.move_dir) * self.state.speed;
        if self.state.speed < 0.0 {
            wish = -wish;
        }
        let direction = vector_normalize(wish);
        self.accelerate(direction, wish.length(), 100.0, seconds);
        self.step_slide_move(true, seconds, bounds, ground, collision);
    }

    /// The vehicle's friction (`PM_Friction`, `bg_pmove.c:1002-1036`): a vehicle that is
    /// no fighter, animal or walker and has one. `None` for the ordinary friction.
    pub(super) fn vehicle_friction(&self) -> Option<f32> {
        let vehicle = self.vehicle.as_ref().filter(|_| self.vehicle_move())?;
        let info = &vehicle.info;
        (!matches!(info.kind, kind::FIGHTER | kind::ANIMAL | kind::WALKER) && info.friction != 0.0)
            .then_some(info.friction)
    }

    /// `PM_WalkMove`'s wish for a vehicle (`bg_pmove.c:3330-3375`): along its `moveDir` at
    /// its speed, where it has a direction. `None` for the command's wish.
    pub(super) fn vehicle_walk_wish(&self) -> Option<(Vec3, f32)> {
        if !self.vehicle_move() || self.move_dir == [0.0; 3] {
            return None;
        }
        let wish = Vec3::from_array(self.move_dir) * self.state.speed;
        let direction = vector_normalize(wish);
        Some((direction, wish.length()))
    }

    /// `PM_AirMove`'s wish for a hovering vehicle (`bg_pmove.c:3087-3095`): along its
    /// `moveDir` at its speed. `None` for anyone else's.
    pub(super) fn vehicle_air_wish(&self) -> Option<(Vec3, f32)> {
        let vehicle = self.vehicle.as_ref().filter(|_| self.vehicle_move())?;
        if vehicle.info.hover_height <= 0.0 {
            return None;
        }
        let wish = Vec3::from_array(self.move_dir) * self.state.speed;
        Some((vector_normalize(wish), wish.length()))
    }

    /// `PM_AirMove`'s acceleration for a speeder (`bg_pmove.c:3231-3238`): its traction,
    /// half of it on a slope. `None` for anyone else's.
    pub(super) fn vehicle_air_acceleration(&self, ground: &GroundState) -> Option<f32> {
        let vehicle = self.vehicle.as_ref().filter(|_| self.vehicle_move())?;
        (vehicle.kind() == kind::SPEEDER).then(|| {
            if ground.ground_plane {
                vehicle.info.traction * 0.5
            } else {
                vehicle.info.traction
            }
        })
    }

    /// The vehicle's own block of `PmoveSingle` on the server (`bg_pmove.c:10908-11026`):
    /// its pitch the view's (a fighter's aside), then the game's `Update` and `Animate` —
    /// on the move's command without a pilot, on the vehicle's own with one.
    pub(super) fn vehicle_think<'d>(
        &mut self,
        command: &UserCommand,
        collision: &impl MovementCollision,
        game: Option<&mut (dyn VehicleGame + 'd)>,
    ) {
        if !self.vehicle_move() {
            return;
        }
        let Self {
            vehicle,
            state,
            move_dir,
            box_bounds,
            ..
        } = self;
        let move_box = *box_bounds;
        let Some(vehicle) = vehicle.as_deref_mut() else {
            return;
        };
        if vehicle.kind() != kind::FIGHTER {
            vehicle.orientation[PITCH] = state.view_angles[PITCH];
        }
        let Some(game) = game else { return };
        if state.vehicle_entity_num == 0 {
            game.update(vehicle, state, command, move_dir, move_box, collision);
        } else {
            if state.movement_type == PM_DEAD && vehicle.flags & flags::CRASHING != 0 {
                vehicle.flags &= !flags::CRASHING;
            }
            let own = vehicle.ucmd;
            game.update(vehicle, state, &own, move_dir, move_box, collision);
        }
    }

    /// Whether this is a piloted vehicle's move, whose weapon [`Self::piloted_weapon`] runs.
    pub(super) fn piloted_vehicle(&self) -> bool {
        self.vehicle_move() && self.state.vehicle_entity_num != 0
    }

    /// `PM_Weapon` for a vehicle with a pilot (`bg_pmove.c:6656-7420`): no hands to raise,
    /// nothing to switch to unless it holds it; with the pilot's attack buttons it fires
    /// every 100 ms (`G_CheapWeaponFire`, the game's), without them it is ready. The
    /// vehicle moves by the pilot's command, which asks for the pilot's weapon.
    pub(super) fn piloted_weapon<'d>(
        &mut self,
        command: &UserCommand,
        millis: i32,
        game: Option<&mut (dyn VehicleGame + 'd)>,
    ) {
        const WP_EMPLACED_GUN: u8 = 17;
        const WP_ROCKET_LAUNCHER: u8 = 11;
        const USE_HOLDABLE: u16 = 4;
        const PMF_USE_ITEM_HELD: u16 = 1_024;
        const WEAPON_READY: u8 = 0;
        const WEAPON_RAISING: u8 = 1;
        const WEAPON_DROPPING: u8 = 2;
        const WEAPON_FIRING: u8 = 3;
        let state = &mut self.state;
        if state.weapon != WP_EMPLACED_GUN {
            state.saber_holstered = 0;
        }
        if state.health <= 0 {
            state.weapon = 0;
            return;
        }
        if command.buttons & USE_HOLDABLE != 0 {
            // `BG_ClearRocketLock`; a vehicle has no holdable to use.
            (
                state.rocket_lock_index,
                state.rocket_lock_time,
                state.rocket_target_time,
            ) = (ENTITY_NUMBER_NONE, -1.0, 0.0);
            if state.movement_flags & PMF_USE_ITEM_HELD == 0 {
                return;
            }
        } else {
            state.movement_flags &= !PMF_USE_ITEM_HELD;
        }
        if state.weapon_time > 0 {
            state.weapon_time -= millis;
        }
        if (state.weapon_time <= 0 || state.weapon_state != WEAPON_FIRING)
            && state.weapon != command.weapon
        {
            if let Some(lengths) = self.animation_lengths.clone() {
                crate::pmove_weapon::begin_weapon_change(
                    &mut self.state,
                    command.weapon,
                    lengths.as_ref(),
                    &mut self.events,
                );
            }
        }
        let state = &mut self.state;
        if state.weapon_time > 0 {
            return;
        }
        if state.weapon_state == WEAPON_DROPPING {
            // `PM_FinishWeaponChange`: to what the command asks, if it holds it.
            let requested = command.weapon;
            state.weapon = if usize::from(requested) < crate::LEGACY_WEAPON_COUNT
                && state.weapons & (1 << requested) != 0
            {
                requested
            } else {
                0
            };
            state.weapon_state = WEAPON_RAISING;
            state.weapon_time += 250;
            return;
        }
        if state.weapon_state == WEAPON_RAISING {
            state.weapon_state = WEAPON_READY;
            return;
        }
        // No homing vehicle weapon here keeps its lock (`vehicleRocketLock`).
        if state.weapon != WP_ROCKET_LAUNCHER {
            (
                state.rocket_lock_index,
                state.rocket_lock_time,
                state.rocket_target_time,
            ) = (ENTITY_NUMBER_NONE, 0.0, 0.0);
        }
        if command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0 {
            state.weapon_time = 0;
            state.weapon_state = WEAPON_READY;
            return;
        }
        state.weapon_state = WEAPON_FIRING;
        state.weapon_time += 100;
        if let (Some(game), Some(vehicle)) = (game, self.vehicle.as_deref_mut()) {
            game.fire(
                vehicle,
                &mut self.state,
                command.buttons & BUTTON_ALT_ATTACK != 0,
            );
        }
    }

    /// `PM_HoverTrace` (`bg_pmove.c:692-880`): the ground under the hover height pushes the
    /// vehicle up (or a steep slope down), its banking follows, and — whatever it found —
    /// it is off the ground (`PM_GroundTraceMissed`).
    pub(super) fn hover_trace<'d>(
        &mut self,
        command: &UserCommand,
        bounds: Bounds,
        ground: &mut GroundState,
        collision: &impl MovementCollision,
        seconds: f32,
        game: Option<&mut (dyn VehicleGame + 'd)>,
    ) {
        let Some(vehicle) = self.vehicle.as_deref() else {
            return;
        };
        let info = std::sync::Arc::clone(&vehicle.info);
        let modifier = vehicle.time_modifier;
        ground.ground_plane = false;
        let water_level = self.state.water_level;
        if water_level != 0 {
            // In water: float up to the buoyancy's level.
            if info.bouyancy > 0.0 {
                let float_height = info.bouyancy
                    * ((bounds.maximums[2] - bounds.minimums[2]) * 0.5)
                    - info.hover_height * 0.5;
                let relative = f32::from(water_level);
                if relative > float_height {
                    self.state.velocity[2] += (relative - float_height) * modifier;
                }
            }
            // Part of it out of the water at a decent speed: the splash's draw.
            if water_level <= 1
                && self.state.velocity[0].abs() + self.state.velocity[1].abs() > 100.0
                && let Some(game) = game
            {
                game.wake();
            }
        } else {
            let mut point = self.state.origin;
            point[2] -= info.hover_height;
            let mut mask = PLAYER_CONTENT_MASK;
            if info.bouyancy >= 2.0 {
                mask |= MASK_LIQUID;
            }
            let trace = collision.trace(
                self.state.origin,
                bounds.minimums,
                bounds.maximums,
                point,
                mask,
            );
            ground.trace = trace;
            let normal = trace.plane_normal;
            if normal[0] > 0.5 || normal[0] < -0.5 || normal[1] > 0.5 || normal[1] < -0.5 {
                // A steep hill: down it, never up.
                let steepest = normal[0].abs().max(normal[1].abs());
                self.state.velocity[2] = -300.0 * steepest;
            } else if normal[2] >= info.max_slope && trace.fraction < 1.0 {
                let force = info.hover_strength;
                if trace.fraction > 0.5 {
                    self.state.velocity[2] += (1.0 - trace.fraction) * force * modifier;
                } else {
                    self.state.velocity[2] +=
                        (0.5 - trace.fraction * trace.fraction) * force * 2.0 * modifier;
                }
                ground.ground_plane = true;
            }
        }
        let normal = ground.ground_plane.then_some(ground.trace.plane_normal);
        self.set_vehicle_angles(normal, seconds, bounds, collision);
        let vehicle = self
            .vehicle
            .as_deref_mut()
            .expect("the vehicle of the move");
        if ground.ground_plane {
            vehicle.flags &= !flags::FLYING;
            vehicle.angular_velocity = 0.0;
        } else {
            vehicle.flags |= flags::FLYING;
            if vehicle.angular_velocity == 0.0 {
                vehicle.angular_velocity =
                    (vehicle.orientation[YAW] - vehicle.prev_orientation[YAW]).clamp(-15.0, 15.0);
            }
            if vehicle.angular_velocity > 0.0 {
                vehicle.angular_velocity = (vehicle.angular_velocity - seconds).max(0.0);
            } else if vehicle.angular_velocity < 0.0 {
                vehicle.angular_velocity = (vehicle.angular_velocity + seconds).min(0.0);
            }
        }
        self.ground_trace_missed(command, bounds, collision);
        ground.ground_plane = false;
        ground.walking = false;
    }

    /// `PM_GroundTraceMissed` (`bg_pmove.c:4021-4104`): the legs react to leaving the
    /// ground, and the move is off it.
    pub(super) fn ground_trace_missed(
        &mut self,
        command: &UserCommand,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) {
        if let Some(lengths) = self.animation_lengths.as_deref() {
            crate::pmove_locomotion::left_the_ground(&mut self.state, command, lengths, |state| {
                let mut below = state.origin;
                below[2] -= 64.0;
                collision
                    .trace(
                        state.origin,
                        bounds.minimums,
                        bounds.maximums,
                        below,
                        PLAYER_CONTENT_MASK,
                    )
                    .fraction
                    == 1.0
                    || state.movement_type == 2
            });
        }
        self.state.ground_entity_number = ENTITY_NUMBER_NONE;
    }

    /// `PM_SetVehicleAngles` (`bg_pmove.c:494-647`): the vehicle's pitch and roll eased
    /// toward the slope under it (or the view's pitch in the air), banked into a turn.
    fn set_vehicle_angles(
        &mut self,
        normal: Option<[f32; 3]>,
        seconds: f32,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) {
        let Some(vehicle) = self.vehicle.as_deref() else {
            return;
        };
        let info = std::sync::Arc::clone(&vehicle.info);
        let mut banking = (info.banking_speed * 32.0) * seconds;
        if banking <= 0.0 || (info.pitch_limit == 0.0 && info.roll_limit == 0.0) {
            return;
        }
        let pitch_bias = if info.kind == kind::FIGHTER {
            0.0
        } else {
            90.0 * info.center_of_gravity[0]
        };
        let mut target = [0.0f32; 3];
        let view_pitch = self.state.view_angles[PITCH];
        if self.state.water_level > 0 {
            target[PITCH] = (f64::from(target[PITCH])
                + f64::from((view_pitch - target[PITCH]) * 0.75)
                + f64::from(pitch_bias) * 0.5) as f32;
        } else if let Some(normal) = normal {
            target = self
                .pitch_roll_for_slope(normal, vehicle.orientation[YAW], bounds, collision)
                .unwrap_or(target);
        } else {
            target[PITCH] = view_pitch * 0.5 + pitch_bias;
            banking *= 0.125 * seconds;
        }
        if info.roll_limit > 0.0 {
            let mut velocity = self.state.velocity;
            velocity[2] = 0.0;
            let mut speed = crate::saber_clash::normalize(&mut velocity);
            if speed > 32.0 || speed < -32.0 {
                // "Magic number fun!": the speed modulated by a sine of a double.
                speed = (f64::from(speed) * (f64::from(150.0 + seconds) * 0.003).sin()) as f32;
                if speed > 60.0 {
                    speed = 60.0;
                }
                let mut flat = vehicle.orientation;
                flat[ROLL] = 0.0;
                let right = flight::flight_axes(flat).1.to_array();
                let dot = velocity[0] * right[0] + velocity[1] * right[1] + velocity[2] * right[2];
                target[ROLL] -= speed * dot;
            }
        }
        if info.pitch_limit != -1.0 {
            if target[PITCH] > info.pitch_limit {
                target[PITCH] = info.pitch_limit;
            } else if target[PITCH] < -info.pitch_limit {
                target[PITCH] = -info.pitch_limit;
            }
        }
        if target[ROLL] > info.roll_limit {
            target[ROLL] = info.roll_limit;
        } else if target[ROLL] < -info.roll_limit {
            target[ROLL] = -info.roll_limit;
        }
        let vehicle = self
            .vehicle
            .as_deref_mut()
            .expect("the vehicle of the move");
        for axis in [PITCH, ROLL] {
            let angle = &mut vehicle.orientation[axis];
            if *angle >= target[axis] + banking {
                *angle -= banking;
            } else if *angle <= target[axis] - banking {
                *angle += banking;
            } else {
                *angle = target[axis];
            }
        }
    }

    /// `PM_pitch_roll_for_slope` into stored angles (`bg_pmove.c:361-439`) for a vehicle:
    /// its pitch and roll on a slope along its yaw. A zero normal is traced for, 300 units
    /// down; `None` where nothing is found.
    fn pitch_roll_for_slope(
        &self,
        normal: [f32; 3],
        yaw: f32,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) -> Option<[f32; 3]> {
        let slope = if normal == [0.0; 3] {
            let mut start = self.state.origin;
            start[2] += bounds.minimums[2] + 4.0;
            let mut end = start;
            end[2] -= 300.0;
            let trace = collision.trace(self.state.origin, [0.0; 3], [0.0; 3], end, MASK_SOLID);
            if trace.fraction >= 1.0 || trace.plane_normal == [0.0; 3] {
                return None;
            }
            trace.plane_normal
        } else {
            normal
        };
        let (forward, right) = flight::flight_axes([0.0, yaw, 0.0]);
        let mut angles = crate::player_angle_math::vector_angles(slope);
        let pitch = angles[PITCH] + 90.0;
        angles[ROLL] = 0.0;
        angles[PITCH] = 0.0;
        let slope_forward = flight::flight_axes(angles).0;
        let side = if slope_forward.dot(right) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let dot = slope_forward.dot(forward);
        Some([dot * pitch, 0.0, (1.0 - dot.abs()) * pitch * side])
    }
}

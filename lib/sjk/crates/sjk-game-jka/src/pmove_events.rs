//! Movement event moments from codemp Pmove; no audio/render dependencies.
use super::*;
use crate::predicted_events::PredictedEvent;

/// `EV_ROLL` (after `EV_JUMP`, 16).
pub(super) const EV_ROLL: u16 = 17;

impl Predictor {
    /// Deliver the events raised by the last command to an allocation-free sink.
    /// Replayed commands intentionally raise events again; consumers reconcile by sequence.
    pub fn emit_events(&self, mut sink: impl FnMut(PredictedEvent)) {
        for event in self.events.iter() {
            sink(event);
        }
    }

    pub(super) fn add_event(&mut self, event: u16, parameter: u16) {
        self.events.push(PredictedEvent {
            weapon: self.state.weapon,
            zoom_mode: self.state.zoom_mode,
            sequence: self.state.event_sequence,
            event,
            parameter,
            command_time: self.state.command_time,
            client: self.state.client_num,
            entity_flags: self.state.entity_flags,
            origin: self.state.origin,
        });
        self.state.event_sequence = self.state.event_sequence.wrapping_add(1);
    }

    pub(super) fn advance_bob(
        &mut self,
        rate: Option<f32>,
        millis: i32,
        server_time: i32,
        bounds: Bounds,
        collision: &impl MovementCollision,
    ) {
        let Some(rate) = rate else {
            return;
        };
        if rate == 0.0 {
            self.state.bob_cycle = 0;
            return;
        }
        let (cycle, crossed) = advance_cycle(self.state.bob_cycle, rate, millis);
        self.state.bob_cycle = cycle;
        // PM_Footsteps, bg_pmove.c:5676-5695. Dry footsteps are AEV_FOOTSTEP
        // animation events, NOT a predictable event at this cycle boundary; the step is
        // heard for 300 ms of the command's time (`footstepTime`).
        if crossed {
            self.state.footstep_time = server_time.wrapping_add(300);
            match water_level(&self.state, bounds, collision) {
                1 => self.add_event(4, 0), // EV_FOOTSPLASH
                2 => self.add_event(6, 0), // EV_SWIM
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn landing_events(
        &mut self,
        command: &UserCommand,
        bounds: Bounds,
        ground: &GroundState,
        collision: &impl MovementCollision,
        previous_origin: [f32; 3],
        previous_velocity: [f32; 3],
    ) {
        let water = water_level(&self.state, bounds, collision);
        let Some(mut delta) = landing_delta(&self.state, previous_origin, previous_velocity) else {
            self.state.in_air_animation = false;
            return;
        };
        // `PM_CrashLand` picks the landing animation before any of its other returns.
        if let Some(lengths) = self.animation_lengths.as_deref() {
            crate::pmove_locomotion::crash_land(&mut self.state, lengths);
        }
        self.state.in_air_animation = false;
        if self.state.vehicle_entity_num != 0 || water == 3 {
            return;
        }
        delta *= match water {
            1 => 0.5,
            2 => 0.25,
            _ => 1.0,
        };
        if delta < 1.0 {
            return;
        }
        let before_roll = crate::pmove_roll::in_roll(&self.state);
        // Existing roll selection; water attenuation also controls eligibility.
        if delta >= 2.0 {
            crate::pmove_roll_land::crash_land_roll(
                &mut self.state,
                command,
                collision,
                self.animation_lengths.as_deref(),
                self.config.roll_rules,
                previous_origin,
                previous_velocity,
            );
        }
        let rolled = !before_roll && crate::pmove_roll::in_roll(&self.state);
        if rolled {
            delta /= 3.0;
        }
        // PM_CrashLand, bg_pmove.c:3898-3972. Even NODAMAGE resets bobCycle.
        if ground.trace.surface_flags & 0x0004_0000 == 0 {
            if delta > 7.0 {
                let parameter = fall_parameter(&self.state, delta);
                self.add_event(if rolled { EV_ROLL } else { 11 }, parameter);
            } else if rolled {
                self.add_event(EV_ROLL, 0);
            } else {
                let material = if ground.trace.surface_flags & 0x0040_0000 != 0 {
                    0
                } else {
                    (ground.trace.surface_flags & 31) as u16
                };
                self.add_event(2, material);
            }
        }
        // `bg_pmove.c:3965-3966`: "make sure velocity resets so we don't bounce back up".
        // A landing found by the ground trace alone (within its 0.25 units, no slide
        // clip) still carries the fall; left in, the next walk move keeps that speed
        // and turns it into a horizontal boost.
        self.state.velocity[2] = 0.0;
        self.state.bob_cycle = 0;
    }
}

/// PM_Footsteps' on-foot rate selection, bg_pmove.c:5205-5547.
pub(super) fn bob_rate(state: &MovementState, cmd: &UserCommand) -> Option<f32> {
    if state.vehicle_entity_num != 0 {
        return None;
    }
    if state.saber_move == 30 {
        return Some(0.2);
    } // LS_SPINATTACK
    if state.ground_entity_number == ENTITY_NUMBER_NONE {
        return None;
    }
    if cmd.forward_move == 0 && cmd.right_move == 0 {
        let speed =
            (state.velocity[0] * state.velocity[0] + state.velocity[1] * state.velocity[1]).sqrt();
        return (speed < 5.0).then_some(0.0);
    }
    if state.movement_flags & PMF_DUCKED != 0
        || (state.movement_flags & crate::PMF_ROLLING != 0
            && !crate::pmove_roll::in_roll(state)
            && !crate::pmove_roll::in_roll_complete(state))
    {
        return Some(0.5);
    }
    let force_land = matches!(
        crate::legacy_animation_name(usize::from(state.legs_anim)),
        Some(
            "BOTH_FORCELAND1"
                | "BOTH_FORCELANDBACK1"
                | "BOTH_FORCELANDRIGHT1"
                | "BOTH_FORCELANDLEFT1"
        )
    );
    Some(
        if cmd.buttons & 16 != 0 || (force_land && state.legs_timer > 0) {
            0.2
        } else {
            0.4
        },
    )
}

/// Exact float addition followed by integer truncation, bg_pmove.c:5677-5680.
pub(super) fn advance_cycle(old: u8, rate: f32, millis: i32) -> (u8, bool) {
    let new = (f32::from(old) + rate * millis as f32) as i32 & 255;
    (new as u8, ((i32::from(old) + 64) ^ (new + 64)) & 128 != 0)
}

/// PM_StepSlideMove, bg_slidemove.c:1066-1080: classify only the accepted step.
pub(super) fn step_event(delta: f32) -> Option<u16> {
    if delta <= 2.0 {
        None
    } else {
        Some(if delta < 7.0 {
            7
        } else if delta < 11.0 {
            8
        } else if delta < 15.0 {
            9
        } else {
            10
        })
    }
}

/// PM_CrashLand's quadratic impact estimate, bg_pmove.c:3712-3737.
fn landing_delta(state: &MovementState, origin: [f32; 3], velocity: [f32; 3]) -> Option<f32> {
    let a = -state.gravity / 2.0;
    let b = velocity[2];
    let c = -(state.origin[2] - origin[2]);
    let den = b * b - 4.0 * a * c;
    if den < 0.0 {
        return None;
    }
    // `t = (-b - sqrt(den)) / (2 * a)`: `sqrt` is a double's, and so is the rest, kept as a
    // float. With no gravity (a flying NPC) it divides by zero, and the landing goes on
    // with a delta that is not a number — which no comparison takes, so a footstep.
    let time = ((f64::from(-b) - f64::from(den).sqrt()) / f64::from(2.0 * a)) as f32;
    let impact = b + time * -state.gravity;
    // `delta * delta * 0.0001`: the double literal makes the product a double's.
    let delta = (f64::from(impact * impact) * 0.0001) as f32;
    Some(if state.movement_flags & PMF_DUCKED != 0 {
        delta * 2.0
    } else {
        delta
    })
}

/// Force jump damage reduction and clamp, bg_pmove.c:3903-3943.
fn fall_parameter(state: &MovementState, delta: f32) -> u16 {
    let mut parameter = (delta as i32).min(600);
    let start = state.force_jump_start_height;
    if start != 0.0 && parameter > 8 {
        if state.origin[2] as i32 >= start as i32 {
            parameter = 8;
        } else {
            let heights = [32, 96, 192, 384];
            let difference = start as i32 - state.origin[2] as i32;
            let less = (heights[usize::from(state.levitation_level.min(3))] - difference).max(0);
            parameter = ((f64::from(parameter) - f64::from(less) * 0.3) as i32).max(8);
        }
    }
    parameter as u16
}

/// PM_SetWaterLevel's three integer-height samples, bg_pmove.c:4253-4285.
pub(super) fn water_level(
    state: &MovementState,
    _bounds: Bounds,
    collision: &impl MovementCollision,
) -> u8 {
    super::water::sample(state, collision).0
}

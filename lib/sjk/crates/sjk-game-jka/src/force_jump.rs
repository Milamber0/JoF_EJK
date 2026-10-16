//! The charged Force jump of `WP_ForcePowersUpdate` (`w_force.c:5304-5336`) and the jump
//! it lets go (`WP_DoSpecificPower(FP_LEVITATION)`, `ForceJump`,
//! `WP_GetVelocityForForceJump`, `w_force.c:2231-2356`, `4381-4393`).
//!
//! A player never charges one (the metroid jump is `Pmove`'s); a Jedi NPC's AI does
//! (`ps.fd.forceJumpCharge`, [`crate::force_powers::ForcePowers::jump_charge`]): once its
//! jump key is up, the charge is spent in a jump the next move shows with a flip.

use crate::force_powers::{FORCE_POWER_NEEDED, FP_LEVITATION, Forcer};
use sjk_protocol::UserCommand;

/// `JUMP_VELOCITY`; `forceJumpStrength` (`bg_pmove.c:185-191`); `FORCE_JUMP_CHARGE_TIME`
/// and `FRAMETIME` (`w_saber.h:54`, `g_local.h`).
const JUMP_VELOCITY: f32 = 225.0;
const FORCE_JUMP_STRENGTH: [f32; 4] = [JUMP_VELOCITY, 420.0, 590.0, 840.0];
const FORCE_JUMP_CHARGE_TIME: f32 = 6_400.0;
const FRAMETIME: f32 = 100.0;
/// `PMF_JUMP_HELD`; `BUTTON_FORCEPOWER`.
const PMF_JUMP_HELD: u16 = 2;
const BUTTON_FORCEPOWER: u16 = 512;
/// The player-state fields the jump reads and writes.
const PS_GROUND_ENTITY: usize = 16;
const PS_SELECTED: usize = 54;
const PS_JUMP_Z_START: usize = 74;
const ENTITY_NUMBER_NONE: u32 = 1_023;
/// `TRACK_CHANNEL_1` less `TRACK_CHANNEL_NONE`: the charge's sound; `CHAN_VOICE`.
const TRACK_CHANNEL_1: usize = 1;
const CHAN_VOICE: u32 = 3;
/// `PDSOUND_FORCEJUMP`.
const PDSOUND_FORCEJUMP: u32 = 5;

/// `FJ_FORWARD` .. `FJ_UP` (`w_force.c`): the way a Force jump goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpWay {
    Forward,
    Backward,
    Right,
    Left,
    Up,
}

impl Forcer<'_, '_> {
    /// `w_force.c:5304-5336`: on the ground the last jump is over; in the air, a charge
    /// whose jump key is let go is dropped; and with the jump key up, a charge left is
    /// jumped (`WP_DoSpecificPower(FP_LEVITATION)`), unless the Force button holds
    /// levitation.
    pub(crate) fn charged_jump(&mut self, command: &UserCommand) {
        let grounded = self.field(PS_GROUND_ENTITY) != ENTITY_NUMBER_NONE;
        if grounded {
            self.force.jumped = false;
        }
        let levitation_held = command.buttons & BUTTON_FORCEPOWER != 0
            && self.field(PS_SELECTED) as usize == FP_LEVITATION;
        if self.force.jump_charge != 0.0
            && !grounded
            && self.force.jumped
            && command.up_move < 10
            && !levitation_held
        {
            self.mute(self.force.kill_sounds[TRACK_CHANNEL_1], CHAN_VOICE);
            self.force.jump_charge = 0.0;
        }
        if self.state.movement_flags() & PMF_JUMP_HELD == 0
            && self.force.jump_charge != 0.0
            && !levitation_held
        {
            self.levitation(command);
        }
    }

    /// `WP_DoSpecificPower(self, ucmd, FP_LEVITATION)`: with the power available, a charge
    /// that left the ground some other way is dropped; on the ground it is jumped.
    fn levitation(&mut self, command: &UserCommand) {
        if !self.available(FP_LEVITATION, 0) {
            return;
        }
        if self.field(PS_GROUND_ENTITY) == ENTITY_NUMBER_NONE {
            self.force.jump_charge = 0.0;
            self.mute(self.force.kill_sounds[TRACK_CHANNEL_1], CHAN_VOICE);
        } else {
            self.force_jump(command);
        }
    }

    /// `ForceJump`: off the ground with the charge's height and the command's push, the
    /// power on and paid for by the charge, a flip for the next move.
    fn force_jump(&mut self, command: &UserCommand) {
        let level_time = self.frame.level_time;
        if self.force.duration[FP_LEVITATION] > level_time || !self.usable(FP_LEVITATION) {
            return;
        }
        // `self->s.groundEntityNum`: the entity's, as its last move left it — the state's.
        if self.field(PS_GROUND_ENTITY) == ENTITY_NUMBER_NONE || *self.frame.health <= 0 {
            return;
        }
        self.force.jumped = true;
        let level = usize::from(self.force.levels[FP_LEVITATION].min(3));
        let interval = FORCE_JUMP_STRENGTH[level] / (FORCE_JUMP_CHARGE_TIME / FRAMETIME);
        let (velocity, _) = self.jump_velocity(command);
        let origin = self.origin();
        self.state
            .set_raw_field(PS_JUMP_Z_START, origin[2].to_bits());
        self.state.set_velocity(velocity);
        let cost = self.force.jump_charge / interval / (FORCE_JUMP_CHARGE_TIME / FRAMETIME)
            * FORCE_POWER_NEEDED[level][FP_LEVITATION] as f32;
        self.start(FP_LEVITATION, cost as i32);
        self.force.jump_charge = 0.0;
        self.force.jump_flip = true;
        self.state
            .set_raw_field(PS_GROUND_ENTITY, ENTITY_NUMBER_NONE);
    }

    /// `WP_GetVelocityForForceJump`: the command's push along the level view (50 each way
    /// on a diagonal, 100 straight), the charge's rise (at least 625), a fall held to 30;
    /// the charge's sound muted and the jump's heard.
    pub(crate) fn jump_velocity(&mut self, command: &UserCommand) -> ([f32; 3], JumpWay) {
        let (forward_move, right_move) = (command.forward_move, command.right_move);
        let (push_forward, push_right) = match (forward_move, right_move) {
            (0, 0) => (0.0, 0.0),
            (forward, right) if forward != 0 && right != 0 => (
                if forward > 0 { 50.0 } else { -50.0 },
                if right > 0 { 50.0 } else { -50.0 },
            ),
            (forward, _) if forward > 0 => (100.0, 0.0),
            (forward, _) if forward < 0 => (-100.0, 0.0),
            (_, right) if right > 0 => (0.0, 100.0),
            _ => (0.0, -100.0),
        };
        let mut view = self.state.view_angles();
        view[0] = 0.0;
        // `pushFwd` along the view is lost: the second `VectorMA` below starts over.
        let right = crate::pmove::flight::flight_axes(view).1.to_array();
        self.mute(self.force.kill_sounds[TRACK_CHANNEL_1], CHAN_VOICE);
        self.raise(crate::knockdown::predef_sound(
            self.origin(),
            PDSOUND_FORCEJUMP,
        ));
        if self.force.jump_charge < JUMP_VELOCITY + 40.0 {
            self.force.jump_charge = JUMP_VELOCITY + 400.0;
        }
        let mut velocity = self.state.velocity();
        if velocity[2] < -30.0 {
            velocity[2] = -30.0;
            self.state.set_velocity(velocity);
        }
        let mut jump: [f32; 3] =
            std::array::from_fn(|axis| velocity[axis] + push_right * right[axis]);
        jump[2] += self.force.jump_charge;
        let charged = self.force.jump_charge > 200.0;
        let way = if push_forward > 0.0 && charged {
            JumpWay::Forward
        } else if push_forward < 0.0 && charged {
            JumpWay::Backward
        } else if push_right > 0.0 && charged {
            JumpWay::Right
        } else if push_right < 0.0 && charged {
            JumpWay::Left
        } else {
            JumpWay::Up
        };
        (jump, way)
    }
}

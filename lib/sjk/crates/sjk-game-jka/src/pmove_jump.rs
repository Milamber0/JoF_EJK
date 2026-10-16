//! `PM_CheckJump` (`codemp/game/bg_pmove.c:1757-2753`) whole, as `PM_WalkMove` and
//! `PM_AirMove` both call it: the gates, the Force jump's upkeep and its held rise
//! ([`super::force_jump`]), the special jumps off walls
//! ([`super::wall_moves`]) and the ordinary jump.
//!
//! Not ported: the `PMF_RESPAWNED` gate (`:1785-1788`), because this port clears that latch only where
//! it predicts the weapon (`pmove_weapon::adjust_attack_flags`), not unconditionally as
//! `PmoveSingle` does (`:10517-10522`); and the zero-gravity push (`:2070-2085`) — in the
//! air a player at gravity 0 or less is pushed twice a jump's speed along its view on
//! *every* command it does not hold jump — which would set adrift the players the
//! server's synthetic tests float at gravity 0 in an empty world.

use super::wall_moves::JumpRules;
use super::{
    Bounds, GroundState, JUMP_VELOCITY, MAX_CLIENTS, MoveContext, MovementCollision, PMF_JUMP_HELD,
    Predictor,
};
use sjk_protocol::{ENTITY_NUMBER_NONE, UserCommand};

/// `HANDEXTEND_KNOCKDOWN`, `HANDEXTEND_PRETHROWN`, `HANDEXTEND_POSTTHROWN`
/// (`bg_public.h:200-216`).
const HANDEXTEND_KNOCKDOWN: u8 = 8;
const HANDEXTEND_PRETHROWN: u8 = 13;
const HANDEXTEND_POSTTHROWN: u8 = 14;
/// `PM_JETPACK`, `CLASS_VEHICLE`, `EF2_FLYING`, `WP_SABER`, `WP_MELEE`.
const PM_JETPACK: u8 = 1;
const CLASS_VEHICLE: i32 = 53;
const EF2_FLYING: u32 = 1 << 4;
const WP_SABER: u8 = 3;
const WP_MELEE: u8 = 2;
/// `EV_JUMP`.
const EV_JUMP: u16 = 16;

impl Predictor {
    /// `PM_SetForceJumpZStart` (`bg_pmove.c:1728-1735`): where a jump began, nudged off
    /// zero so that a jump from height zero still counts as one.
    pub(super) fn set_force_jump_start(&mut self, height: f32) {
        self.state.force_jump_start_height = height;
        if self.state.force_jump_start_height == 0.0 {
            self.state.force_jump_start_height -= 0.1;
        }
    }

    /// `PM_CheckJump`: whether the player jumped off the ground. It may change the
    /// command, as the reference changes `pm->cmd`: a jump spent or refused clears its
    /// `upmove`, a run up a wall its stick.
    pub(super) fn check_jump(
        &mut self,
        command: &mut UserCommand,
        bounds: Bounds,
        ground: &mut GroundState,
        collision: &impl MovementCollision,
        context: &MoveContext,
    ) -> bool {
        let state = &self.state;
        // A flying NPC (`FLY_NORMAL`) moves by `PM_FlyMove` and never reaches
        // `PM_CheckJump`; a vehicle does not jump either.
        if state.client_num >= MAX_CLIENTS
            && self
                .npc
                .is_some_and(|npc| npc.class == CLASS_VEHICLE || npc.flags2 & EF2_FLYING != 0)
            || matches!(
                state.force_hand_extend,
                HANDEXTEND_KNOCKDOWN | HANDEXTEND_PRETHROWN | HANDEXTEND_POSTTHROWN
            )
            || state.movement_type == PM_JETPACK
        {
            return false;
        }
        // Knocked down, getting up or rolling, there is no jumping (`bg_pmove.c:1790-1793`).
        if crate::pmove_hand_extend::in_knockdown(state.legs_anim, state.legs_timer_at_entry)
            || crate::pmove_roll_anim::in_roll(state.legs_anim) && state.legs_timer_at_entry > 0
        {
            return false;
        }
        let rules = JumpRules::of(state, context);
        self.force_jump_upkeep(command.server_time);
        if self.state.force_jump_flip {
            self.force_jump_flip(command, rules.flips);
            return true;
        }
        if self.check_force_jump(command, rules.flips) {
            command.up_move = 0;
            return false;
        }
        let grounded =
            |predictor: &Self| predictor.state.ground_entity_number != ENTITY_NUMBER_NONE;
        if command.up_move < 10 && grounded(self) {
            return false;
        }
        // Must wait for the jump to be released.
        if self.state.movement_flags & PMF_JUMP_HELD != 0 {
            command.up_move = 0;
            return false;
        }
        // Zero gravity pushes off in the direction faced instead (`bg_pmove.c:2070-2085`),
        // which is not ported (see the module's notes): no special jump either way.
        if self.state.gravity > 0.0
            && command.up_move > 0
            && self.state.water_level < 2
            && self.state.levitation_level > 0
            && matches!(self.state.weapon, WP_SABER | WP_MELEE)
            && crate::pmove_saber_attack::can_levitate_now(
                &self.state,
                command.server_time,
                self.gametype,
            )
        {
            if grounded(self) {
                self.ground_special_jump(command, bounds, collision, rules);
            } else {
                self.air_special_jump(command, bounds, collision, context, rules);
            }
        }
        if !grounded(self) {
            return false;
        }
        if command.up_move > 0 {
            // No special jump.
            self.state.velocity[2] = JUMP_VELOCITY;
            self.set_force_jump_start(self.state.origin[2]);
            self.state.movement_flags |= PMF_JUMP_HELD;
        }
        ground.ground_plane = false;
        ground.walking = false;
        self.state.movement_flags |= PMF_JUMP_HELD;
        self.state.ground_entity_number = ENTITY_NUMBER_NONE;
        self.set_force_jump_start(self.state.origin[2]);
        self.add_event(EV_JUMP, 0); // bg_pmove.c:2739.
        if self.state.gravity > 0.0
            && !crate::pmove_roll_anim::special_jump(self.state.legs_anim)
            && let Some(lengths) = self.animation_lengths.as_deref()
        {
            crate::pmove_locomotion::jump_for_direction(&mut self.state, command, lengths);
        }
        true
    }
}

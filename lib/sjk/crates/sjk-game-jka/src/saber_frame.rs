//! What `WP_SaberPositionUpdate` (OpenJK `codemp/game/w_saber.c:8091-8470`) leaves in a
//! player's state every server frame — `G_RunFrame` runs it for each playing client
//! (`g_main.c:3314-3318`: not spectators, not followers, not during the intermission),
//! corpses included. The server-side saber model it positions for contact checks is not
//! ported; what is, is the two wire fields nothing else keeps: the style drawn follows the
//! style in use (or the change queued behind a swing), and once the throw delay is past a
//! saber in hand may be thrown. Held against the whole-game death transcript, where both
//! come back one frame after a respawn cleared them. What follows it in the frame,
//! `WP_SaberStartMissileBlockCheck`, is [`crate::saber_block::missile_block_check`].

use sjk_protocol::PlayerState;

/// `fd.saberAnimLevel`, `fd.saberDrawAnimLevel`, `saberEntityNum`, `saberCanThrow`.
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_DRAW_ANIM_LEVEL: usize = 25;
const PS_SABER_ENTITY: usize = 31;
const PS_SABER_CAN_THROW: usize = 49;
/// `saberHolstered`.
const PS_SABER_HOLSTERED: usize = 81;
/// `SFL_NOT_THROWABLE`, `SFL_SINGLE_BLADE_THROWABLE`.
const SFL_NOT_THROWABLE: u32 = 1 << 1;
const SFL_SINGLE_BLADE_THROWABLE: u32 = 1 << 5;

/// `WP_SaberPositionUpdate`'s leave to throw (`w_saber.c:8440-8464`): a first saber not
/// marked unthrowable, or a many-bladed one thrown with one blade lit.
pub fn throwable(first: &crate::saber_definition::SaberDefinition, holstered: u32) -> bool {
    first.flags & SFL_NOT_THROWABLE == 0
        || (first.flags & SFL_SINGLE_BLADE_THROWABLE != 0 && first.num_blades > 1 && holstered == 1)
}

/// `weaponTime`.
const PS_WEAPON_TIME: usize = 10;
const PS_TORSO_ANIM: usize = 15;
const PS_WEAPON_STATE: usize = 33;
/// `WEAPON_FIRING`.
const WEAPON_FIRING: u32 = 3;

/// A player's saber as the frames keep it, cleared with the rest of the state on each
/// spawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaberFrame {
    /// `saberCycleQueue`: a style change waiting for the swing to end, 0 for none.
    pub cycle_queue: u32,
    /// `ps.saberThrowDelay`: no throw before this time.
    pub throw_delay: i32,
}

impl SaberFrame {
    /// The frame's bookkeeping for a player whose saber entity is `state`'s
    /// (`saberEntityNum` 0 is a saber out of its hand — nothing decides its throw then).
    /// Every saber the game knows is a throwable single one until saber definitions are
    /// loaded (`SFL_NOT_THROWABLE`, `SFL_SINGLE_BLADE_THROWABLE`, `w_saber.c:8440-8464`).
    ///
    /// A style cycled during a swing (`saberCycleQueue`) is drawn at once and taken up
    /// once the weapon is idle — or the player dead (`w_saber.c:8130-8138`). A superbreak's
    /// winner is firing. Returns whether the style in use, the weapon's state or the
    /// leave to throw changed, for the movement to take it up.
    pub fn update(
        &mut self,
        state: &mut PlayerState,
        health: i32,
        level_time: i32,
        first: &crate::saber_definition::SaberDefinition,
    ) -> bool {
        let drawn = if self.cycle_queue != 0 {
            self.cycle_queue
        } else {
            state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0)
        };
        state.set_raw_field(PS_SABER_DRAW_ANIM_LEVEL, drawn);
        let mut changed = false;
        if self.cycle_queue != 0
            && (state.raw_field(PS_WEAPON_TIME).unwrap_or(0) as i32 <= 0 || health < 1)
        {
            changed = state.raw_field(PS_SABER_ANIM_LEVEL) != Some(self.cycle_queue);
            state.set_raw_field(PS_SABER_ANIM_LEVEL, self.cycle_queue);
            self.cycle_queue = 0;
        }
        // A superbreak's winner keeps swinging (`w_saber.c:8423-8426`).
        if crate::saber_rules::super_break_win(state.raw_field(PS_TORSO_ANIM).unwrap_or(0) as u16)
            && state.raw_field(PS_WEAPON_STATE) != Some(WEAPON_FIRING)
        {
            state.set_raw_field(PS_WEAPON_STATE, WEAPON_FIRING);
            changed = true;
        }
        if state.raw_field(PS_SABER_ENTITY).unwrap_or(0) != 0 && self.throw_delay < level_time {
            let can = u32::from(throwable(
                first,
                state.raw_field(PS_SABER_HOLSTERED).unwrap_or(0),
            ));
            if state.raw_field(PS_SABER_CAN_THROW) != Some(can) {
                state.set_raw_field(PS_SABER_CAN_THROW, can);
                changed = true;
            }
        }
        changed
    }
}

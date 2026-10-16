//! The behaviour states of the default set (`codemp/game/NPC_behavior.c`,
//! `NPC_AI_Default.c`) that no class AI owns: searching and wandering along the waypoints
//! ([`crate::npc_states_search`]), following a leader ([`crate::npc_states_follow`]),
//! fleeing ([`crate::npc_states_flee`]), the emplaced gunner and the advance on a capture
//! goal ([`crate::npc_states_fight`]), and the states only a script sets — asleep, jumping
//! to a navigation goal, waiting to be unseen and removed, going through walls
//! ([`crate::npc_states_script`]). With them the aim they share: the firing angles
//! (`NPC_UpdateFiringAngles`), the shot's own angles (`NPC_UpdateShootAngles`) and the
//! stand-and-shoot check (`NPC_CheckCanAttack`).
//!
//! What an NPC keeps for them beyond [`crate::npc_mind::NpcMind`] is a [`StatesMind`].
//! Scripts (ICARUS) are not run: the script-only states are reached when a host sets them
//! ([`crate::npc_spawn::NpcActor::behavior_state`] and the fields here), as `Q3_SetBState`
//! and its kin would.
//!
//! Held to `tools/game-oracle/npcstates.c` (`game-npcstates-*.txt`).

use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `jumpState_t` (`b_public.h:96-103`).
pub mod jump {
    pub const WAITING: i32 = 0;
    pub const FACING: i32 = 1;
    pub const CROUCHING: i32 = 2;
    pub const JUMPING: i32 = 3;
    pub const LANDING: i32 = 4;
}

/// What the states keep on an NPC (`gNPC_t`'s fields for them).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatesMind {
    /// `jumpState` (`BS_JUMP`, [`jump`]).
    pub jump_state: i32,
    /// `followDist`: how far from its leader it keeps (96 when zero).
    pub follow_dist: f32,
    /// `captureGoal`: the entity `BS_ADVANCE_FIGHT` heads for.
    pub capture_goal: Option<u16>,
    /// `aimErrorDebounceTime`, `aimOfs`, `lastAimErrorYaw`, `lastAimErrorPitch`: the aim's
    /// wobble, drawn again when the debounce runs out.
    pub aim_error_debounce_time: i32,
    pub aim_offset: [f32; 3],
    pub last_aim_error_yaw: f32,
    pub last_aim_error_pitch: f32,
    /// `shootAngles`: where `NPC_BSAdvanceFight` aims its shot.
    pub shoot_angles: [f32; 3],
    /// `duckDebounceTime`: until when it ducks (`NPC_StandTrackAndShoot`).
    pub duck_debounce_time: i32,
}

/// `BUTTON_WALKING`, `BUTTON_ATTACK`.
pub(crate) const BUTTON_WALKING: u16 = 16;
pub(crate) const BUTTON_ATTACK: u16 = 1;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The default set's states by their reference name: run, and `true`; `false` for a
    /// name that is none of them, or where the replay's driver stood them in
    /// ([`crate::npc_groups::NpcLevel::states_stood_in`]).
    pub(crate) fn run_state(&mut self, me: usize, name: &str, command: &mut UserCommand) -> bool {
        if self.level.states_stood_in() {
            return false;
        }
        match name {
            "NPC_BSSearch" => self.bs_search(me, command),
            "NPC_BSWander" => self.bs_wander(me, command),
            "NPC_BSFollowLeader" => self.bs_follow_leader(me, command),
            "NPC_BSFlee" => self.bs_flee(me, command),
            "NPC_BSEmplaced" => self.bs_emplaced(me, command),
            "NPC_BSAdvanceFight" => self.bs_advance_fight(me, command),
            "NPC_BSSleep" => self.bs_sleep(me),
            "NPC_BSJump" => self.bs_jump(me, command),
            "NPC_BSRemove" => self.bs_remove(me, command),
            "NPC_BSNoClip" => self.bs_noclip(me, command),
            _ => return false,
        }
        true
    }
}

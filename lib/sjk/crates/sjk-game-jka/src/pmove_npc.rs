//! What `Pmove` does differently for an NPC: the branches of OpenJK
//! `codemp/game/bg_pmove.c` for `pm->ps->clientNum >= MAX_CLIENTS` and for `pm_entSelf`'s
//! class, which a player's move never takes.
//!
//! An NPC moves with the box the game linked it by (`pm->mins`, `pm->maxs` from `r.mins`,
//! `r.maxs`, `g_active.c:3009-3011`), not a player's -15..15; it accelerates the other way
//! round (`PM_Accelerate`: no early return, and slowing down along the wish); a rancor and a
//! wampa stand and run in their own animations (`PM_Footsteps`), a Jawa runs in its own;
//! and a humanoid NPC with no weapon holds its torso in its legs' animation instead of
//! running the weapon (`PM_Weapon`, `_GAME` only). The game hands these in as an
//! [`NpcBody`] ([`Predictor::set_npc`]); a player's move has none.
//!
//! A flying NPC (`EF2_FLYING`: `FLY_NORMAL`, `PM_SetSpecialMoveValues`) moves by
//! `PM_FlyMove`, with no ground friction ([`Predictor::flying_normal`]).
//!
//! Not here yet: an NPC's own `moveDir` (`PM_WalkMove`, `bg_pmove.c:3330-3375`), which only the
//! navigation of the NPC plan's later steps sets.

use super::*;

/// `class_t`: the classes `Pmove` knows by name (`teams.h`).
pub const CLASS_JAWA: i32 = 50;
pub const CLASS_RANCOR: i32 = 54;
pub const CLASS_WAMPA: i32 = 55;
/// `EF2_USE_ALT_ANIM`, `EF2_ALERTED` (`bg_public.h`).
const EF2_USE_ALT_ANIM: u32 = 1 << 1;
const EF2_ALERTED: u32 = 1 << 2;
/// `MINS_Z`: the bottom of a box that gives none.
const MINS_Z: f32 = -24.0;
/// `BOTH_STAND1`, `BOTH_STAND2`, `BOTH_STAND4`, and the runs and walks the monsters
/// take instead of a humanoid's (`anims.h`).
const BOTH_STAND1: u16 = 915;
const BOTH_STAND2: u16 = 917;
const BOTH_STAND4: u16 = 922;
const BOTH_WALK1: u16 = 1_102;
const BOTH_RUN1: u16 = 1_111;
const BOTH_RUN2: u16 = 1_114;
const BOTH_RUN4: u16 = 1_117;
const BOTH_WALKBACK1: u16 = 1_134;
/// `PMF_BACKWARDS_RUN`.
const PMF_BACKWARDS_RUN: u16 = 16;

/// What the game hands an NPC's move that a player's does not have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcBody {
    /// `pm_entSelf->s.NPC_class`.
    pub class: i32,
    /// `r.mins`, `r.maxs`: the box the game linked it by, which the move starts from.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `ent->localAnimIndex == 0`: its skeleton is the humanoid one.
    pub humanoid: bool,
    /// `ps.eFlags2`, whose monster bits choose a rancor's and a wampa's stance.
    pub flags2: u32,
}

impl Predictor {
    /// Authoritative NPC heights are signed game values. The unsigned legacy player
    /// fields cannot round-trip a small body's negative top (mouse: -8).
    pub fn set_npc_heights(&mut self, standing: i32, crouching: i32) {
        self.state.standing_height = standing as f32;
        self.state.crouching_height = crouching as f32;
    }

    /// The NPC this move is for, or `None` for a player's.
    pub fn set_npc(&mut self, npc: Option<NpcBody>) {
        self.npc = npc;
        // `pmove.mins`, `pmove.maxs` start as the NPC's link box (`g_active.c:3009-3011`): a
        // command that does not move (a vehicle's own, older than its state) leaves it so.
        if let Some(npc) = npc {
            self.box_bounds = (npc.mins, npc.maxs);
        }
    }

    /// The animations the NPC's entity shows now (`s.legsAnim`, `s.torsoAnim`), for the
    /// rule that restarts one left and taken again before the entity caught up
    /// ([`MovementState::entity_animations`]).
    pub fn set_entity_animations(&mut self, legs: u16, torso: u16) {
        self.state.entity_animations = Some([legs, torso]);
    }

    /// `PM_CheckDuck` for an NPC (`bg_pmove.c:4458-4540`): the box from the game's, a
    /// bottom of `MINS_Z` where it has none, the top the standing or crouching height. A
    /// dead NPC keeps its box: the corpse's low box is a player's.
    pub(super) fn npc_check_duck(
        &mut self,
        _npc: NpcBody,
        command: &UserCommand,
        collision: &impl MovementCollision,
    ) -> Bounds {
        // The box the last slice left (`pm->mins`, `pm->maxs`; the entity's at the first).
        let (mut minimums, maximums) = self.box_bounds;
        if minimums[2] == 0.0 {
            minimums[2] = MINS_Z;
        }
        let (width, depth) = (maximums[0], maximums[1]);
        if let Some(top) = crate::pmove_roll::bounds_height(
            &mut self.state,
            collision,
            minimums,
            PLAYER_CONTENT_MASK,
        ) {
            return Bounds {
                minimums,
                maximums: [width, depth, top],
            };
        }
        if command.up_move < 0 || matches!(self.state.force_hand_extend, 8 | 13 | 14) {
            self.state.movement_flags |= PMF_DUCKED;
        } else if self.state.movement_flags & PMF_DUCKED != 0
            && crate::pmove_posture::can_stand(
                &self.state,
                collision,
                minimums,
                PLAYER_CONTENT_MASK,
            )
        {
            self.state.movement_flags &= !PMF_DUCKED;
        }
        let ducked = self.state.movement_flags & PMF_DUCKED != 0;
        self.state.view_height = if ducked {
            CROUCH_VIEW_HEIGHT
        } else {
            STANDING_VIEW_HEIGHT
        };
        let top = if ducked {
            self.state.crouching_height
        } else {
            self.state.standing_height
        };
        Bounds {
            minimums,
            maximums: [width, depth, top],
        }
    }

    /// `pm_flying == FLY_NORMAL` (`PM_SetSpecialMoveValues`, `bg_pmove.c:459-480`): an NPC
    /// flying by `EF2_FLYING` (the interrogator, the probe, a seeker), which `PmoveSingle`
    /// moves by `PM_FlyMove` (`bg_pmove.c:11034-11038`).
    pub(super) fn flying_normal(&self) -> bool {
        const EF2_FLYING: u32 = 1 << 4;
        self.state.client_num >= MAX_CLIENTS
            && self.npc.is_some_and(|npc| npc.flags2 & EF2_FLYING != 0)
    }

    /// `PmoveSingle`'s bounce (`bg_pmove.c:10873-10889`): whoever stands on an NPC — a
    /// player, or an NPC that is no rancor — is thrown up off its head with a jump.
    pub(super) fn bounce_off_npc(&mut self, command: &mut UserCommand, context: &MoveContext) {
        const CLASS_VEHICLE: i32 = 53;
        let class = self.npc.map_or(0, |npc| npc.class);
        let ground = self.state.ground_entity_number;
        if self.state.vehicle_entity_num != 0
            || class == CLASS_VEHICLE
            || class == CLASS_RANCOR
            || !(32..ENTITY_NUMBER_WORLD).contains(&ground)
        {
            return;
        }
        if (context.npcs)(ground) && self.state.velocity[2] < 270.0 {
            self.state.velocity[2] = 270.0;
            command.up_move = 127;
        }
    }

    /// How high the move steps up (`PM_StepSlideMove`, `bg_slidemove.c:925-950`): an
    /// AT-ST (or a walker vehicle) 66 units and a rancor 64 — giants — anyone else
    /// `STEPSIZE`.
    pub(super) fn step_height(&self) -> (f32, bool) {
        const CLASS_ATST: i32 = 1;
        if self.vehicle_move()
            && self
                .vehicle
                .as_deref()
                .is_some_and(|vehicle| vehicle.kind() == crate::vehicle_fields::kind::WALKER)
        {
            return (66.0, true);
        }
        match self.npc.map(|npc| npc.class) {
            Some(CLASS_ATST) => (66.0, true),
            Some(CLASS_RANCOR) => (64.0, true),
            _ => (STEP_SIZE, false),
        }
    }

    /// `PM_Weapon`'s first lines on the game's side (`bg_pmove.c:6668-6682`): a humanoid
    /// NPC with no weapon and none asked for holds its torso in its legs' animation and
    /// runs nothing else of the weapon. Whether it did.
    pub(super) fn npc_without_weapon(&mut self, command: &UserCommand) -> bool {
        let Some(npc) = self.npc else { return false };
        if !npc.humanoid || self.state.weapon != 0 || command.weapon != 0 {
            return false;
        }
        self.state.torso_anim = self.state.legs_anim;
        self.state.torso_timer = self.state.legs_timer;
        true
    }
}

/// `PM_Accelerate`'s standard method (`bg_pmove.c:1103-1137`): how much speed to add
/// along the wish. A player gains nothing where the wish is no faster than it already
/// goes; an NPC slows down to it instead.
pub(crate) fn acceleration_speed(
    npc: bool,
    add_speed: f32,
    acceleration: f32,
    seconds: f32,
    wish_speed: f32,
) -> Option<f32> {
    if add_speed <= 0.0 && !npc {
        return None;
    }
    Some(if add_speed < 0.0 {
        (-acceleration * seconds * wish_speed).max(add_speed)
    } else {
        (acceleration * seconds * wish_speed).min(add_speed)
    })
}

/// `PM_Footsteps`' stance for a rancor or a wampa standing still (`bg_pmove.c:5228-5262`):
/// holding someone, alerted, or plain. `None` for anyone else, who stands as a player does.
pub(crate) fn monster_stance(npc: Option<&NpcBody>) -> Option<u16> {
    let npc = npc?;
    match npc.class {
        CLASS_RANCOR if npc.flags2 & EF2_USE_ALT_ANIM != 0 => Some(BOTH_STAND4),
        CLASS_RANCOR if npc.flags2 & EF2_ALERTED != 0 => Some(BOTH_STAND2),
        CLASS_RANCOR => Some(BOTH_STAND1),
        CLASS_WAMPA if npc.flags2 & EF2_USE_ALT_ANIM != 0 => Some(BOTH_STAND2),
        CLASS_WAMPA => Some(BOTH_STAND1),
        _ => None,
    }
}

/// `PM_Footsteps`' run for the classes with their own (`bg_pmove.c:5402-5436`): a wampa
/// on all fours or upright, a rancor that only walks, a Jawa's own run.
pub(crate) fn monster_run(npc: Option<&NpcBody>, movement_flags: u16) -> Option<u16> {
    let npc = npc?;
    match npc.class {
        CLASS_WAMPA => Some(if npc.flags2 & EF2_USE_ALT_ANIM != 0 {
            BOTH_RUN1
        } else {
            BOTH_RUN2
        }),
        CLASS_RANCOR => Some(if movement_flags & PMF_BACKWARDS_RUN != 0 {
            BOTH_WALKBACK1
        } else {
            BOTH_WALK1
        }),
        CLASS_JAWA => Some(BOTH_RUN4),
        _ => None,
    }
}

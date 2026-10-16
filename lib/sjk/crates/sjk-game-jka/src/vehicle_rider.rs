//! A vehicle's rider as the vehicle functions reach it (`bgEntity_t *pEnt` in
//! `codemp/game/g_vehicles.c`, `SpeederNPC.c`, `AnimalNPC.c`): a player — its movement, its
//! wire state, its entity and the parts of `gentity_t` the vehicle code touches.
//!
//! The rider belongs to whoever keeps the players (the server's peers, a transcript's
//! replay); the vehicle code is handed a [`Rider`] of borrows for as long as it works on
//! it. Its movement state is the authoritative one for everything `Pmove` owns (view,
//! origin, animations and timers); the caller writes it back into the wire state once the
//! vehicle is done ([`crate::pmove::MovementState::write_player_state`]). What the game
//! owns beside the movement — `m_iVehicleNum`, `generic1`, `EF_NODRAW`, the external event
//! — the rider's functions set on both.

use crate::pmove::MovementState;
use crate::pmove_anim::AnimationLengths;
use sjk_protocol::{EntityState, PlayerState, UserCommand};

/// `ENTITYNUM_NONE`.
pub const ENTITYNUM_NONE: u16 = 1_023;
/// `CONTENTS_BODY`.
pub const CONTENTS_BODY: u32 = 0x100;
/// `SVF_NOCLIENT`.
pub const SVF_NOCLIENT: u32 = 0x1;
/// `EF_NODRAW`.
pub const EF_NODRAW: u32 = 1 << 8;
/// `FL_VEH_BOARDING`: a rider in the middle of getting off (`g_local.h`).
pub const FL_VEH_BOARDING: u32 = 0x0080_0000;

/// Wire fields of the rider's state and entity the vehicle code writes.
pub(crate) mod field {
    /// `ps.eFlags`, `ps.m_iVehicleNum`, `ps.generic1`.
    pub const PS_EFLAGS: usize = 17;
    pub const PS_VEHICLE: usize = 84;
    pub const PS_GENERIC1: usize = 85;
    /// `s.eFlags`, `s.owner`, `s.m_iVehicleNum`, `s.angles`.
    pub const ES_EFLAGS: usize = 19;
    pub const ES_OWNER: usize = 40;
    pub const ES_VEHICLE: usize = 93;
    pub const ES_ANGLES: [usize; 3] = [25, 9, 24];
    /// `s.pos`: its base, delta, type, time and duration.
    pub const ES_POS_BASE: [usize; 3] = [2, 1, 4];
    pub const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
    pub const ES_POS_TYPE: usize = 23;
    pub const ES_POS_TIME: usize = 0;
    pub const ES_POS_DURATION: usize = 20;
}

/// The parts of a rider's `gentity_t` beside its states: its link and its flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiderBody {
    /// `r.ownerNum`: the vehicle it rides, `ENTITYNUM_NONE` otherwise.
    pub owner: u16,
    /// `r.contents`.
    pub contents: u32,
    /// `r.svFlags`.
    pub server_flags: u32,
    /// `ent->flags` (`FL_*`): the vehicle code uses `FL_VEH_BOARDING`.
    pub flags: u32,
    /// `client->solidHack`: until when a rider that does not fit on its vehicle has no
    /// contents (`PM_CheckDuck`, `g_active.c:3052-3065`).
    pub solid_hack: i32,
    /// `ps.useDelay`, which is not on the wire: no use key before this time.
    pub use_delay: i32,
    /// `ps.emplacedTime`, which is not on the wire: no emplaced gun taken before this time.
    pub emplaced_time: i32,
}

impl Default for RiderBody {
    fn default() -> Self {
        Self {
            owner: ENTITYNUM_NONE,
            contents: CONTENTS_BODY,
            server_flags: 0,
            flags: 0,
            solid_hack: 0,
            use_delay: 0,
            emplaced_time: 0,
        }
    }
}

/// A rider, borrowed for the vehicle code.
pub struct Rider<'a> {
    /// Its entity number (a player's slot).
    pub number: u16,
    /// Its movement: what `Pmove` owns of its player state.
    pub movement: &'a mut MovementState,
    /// Its wire player state, for what the game owns beside the movement.
    pub player: &'a mut PlayerState,
    /// Its entity (`s`).
    pub entity: &'a mut EntityState,
    /// Its link and flags.
    pub body: &'a mut RiderBody,
    /// `pers.cmd`: the command it last sent.
    pub command: UserCommand,
    /// `ent->health`.
    pub health: i32,
    /// `pers.connected == CON_CONNECTED` and in use.
    pub connected: bool,
    /// Its skeleton's animations (`bgAllAnims[localAnimIndex]`).
    pub lengths: Option<std::sync::Arc<dyn AnimationLengths>>,
    /// `r.maxs` as the game keeps it: a player's is never kept (`VEH_TryEject` reads 15 for
    /// its sides), its height is.
    pub maxs: [f32; 3],
    /// `ent->clipmask`.
    pub clip_mask: u32,
}

impl Rider<'_> {
    /// The same rider borrowed again, for a callee that takes one by value.
    pub fn reborrow(&mut self) -> Rider<'_> {
        Rider {
            number: self.number,
            movement: &mut *self.movement,
            player: &mut *self.player,
            entity: &mut *self.entity,
            body: &mut *self.body,
            command: self.command,
            health: self.health,
            connected: self.connected,
            lengths: self.lengths.clone(),
            maxs: self.maxs,
            clip_mask: self.clip_mask,
        }
    }

    /// `ps.m_iVehicleNum` and `s.m_iVehicleNum`, on the movement too.
    pub fn set_vehicle(&mut self, number: u16) {
        self.movement.vehicle_entity_num = number;
        self.player
            .set_raw_field(field::PS_VEHICLE, u32::from(number));
        self.entity
            .set_raw_field(field::ES_VEHICLE, u32::from(number));
    }

    /// `G_SetOrigin`: the entity standing at `origin` (`s.pos`, stationary).
    pub fn set_origin(&mut self, origin: [f32; 3]) {
        for (axis, index) in field::ES_POS_BASE.into_iter().enumerate() {
            self.entity.set_raw_field(index, origin[axis].to_bits());
        }
        for index in field::ES_POS_DELTA {
            self.entity.set_raw_field(index, 0);
        }
        self.entity.set_raw_field(field::ES_POS_TYPE, 0);
        self.entity.set_raw_field(field::ES_POS_TIME, 0);
        self.entity.set_raw_field(field::ES_POS_DURATION, 0);
    }

    /// `ps.m_iVehicleNum`.
    pub fn vehicle(&self) -> u16 {
        self.movement.vehicle_entity_num
    }

    /// `r.ownerNum` and `s.owner` ("for prediction").
    pub fn set_owner(&mut self, owner: u16) {
        self.body.owner = owner;
        self.entity.set_raw_field(field::ES_OWNER, u32::from(owner));
    }

    /// `ps.generic1`: which passenger it is.
    pub fn set_passenger_slot(&mut self, slot: u32) {
        self.player.set_raw_field(field::PS_GENERIC1, slot);
    }

    /// `SetClientViewAngle` (`g_client.c:1171-1183`): the delta angles against its last
    /// command, the view, and `s.angles`.
    pub fn set_view_angle(&mut self, angles: [f32; 3]) {
        for axis in 0..3 {
            let short = crate::npc_think::angle_to_short(angles[axis]);
            self.movement.delta_angles[axis] =
                short.wrapping_sub(i32::from(self.command.angles[axis]));
            self.entity
                .set_raw_field(field::ES_ANGLES[axis], angles[axis].to_bits());
        }
        self.movement.view_angles = angles;
    }

    /// `PM_SetPMViewAngle` (`bg_pmove.c:1311-1323`): the delta angles against `command`,
    /// and the view.
    pub fn set_pm_view_angle(&mut self, angles: [f32; 3], command: &UserCommand) {
        for axis in 0..3 {
            let short = crate::npc_think::angle_to_short(angles[axis]);
            self.movement.delta_angles[axis] = short.wrapping_sub(i32::from(command.angles[axis]));
        }
        self.movement.view_angles = angles;
    }

    /// `BG_SetAnim` on the rider with its own skeleton (`Vehicle_SetAnim` then copies the
    /// legs to its entity, which the caller's conversion does).
    pub fn set_animation(&mut self, parts: u8, animation: u16, flags: u8) {
        if let Some(lengths) = self.lengths.clone() {
            crate::pmove_anim::set_animation(
                self.movement,
                parts,
                animation,
                flags,
                lengths.as_ref(),
            );
        }
    }

    /// `BG_AnimLength` on the rider's skeleton.
    pub fn animation_length(&self, animation: u16) -> i32 {
        self.lengths
            .as_ref()
            .and_then(|lengths| lengths.length_ms(animation))
            .unwrap_or(0)
    }

    /// `EF_NODRAW` on its state and its entity.
    pub fn set_hidden(&mut self, hidden: bool) {
        let apply = |flags: u32| {
            if hidden {
                flags | EF_NODRAW
            } else {
                flags & !EF_NODRAW
            }
        };
        let player = apply(self.player.raw_field(field::PS_EFLAGS).unwrap_or(0));
        self.player.set_raw_field(field::PS_EFLAGS, player);
        self.movement.entity_flags = apply(self.movement.entity_flags);
        let entity = apply(self.entity.raw_field(field::ES_EFLAGS).unwrap_or(0));
        self.entity.set_raw_field(field::ES_EFLAGS, entity);
    }

    /// `G_AddEvent` on the rider: its external event.
    pub fn add_event(&mut self, event: u32, parameter: u32) {
        crate::player_entity::add_event(self.player, event, parameter);
    }
}

/// Whether a trace for `pass` (owned by `pass_owner`) goes through `touch` (owned by
/// `touch_owner`), as `SV_ClipMoveToEntities` skips a mover's own riders and a rider's
/// vehicle (`sv_world.cpp:568-599`, for entities without `SVF_OWNERNOTSHARED`).
pub fn trace_skips(pass: u16, pass_owner: u16, touch: u16, touch_owner: u16) -> bool {
    if pass == ENTITYNUM_NONE {
        return false;
    }
    touch == pass
        || touch_owner == pass
        || (pass_owner != ENTITYNUM_NONE && touch_owner == pass_owner)
}

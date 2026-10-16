//! Mind trick (`ForceTelepathy`, `WP_UpdateMindtrickEnts`, `w_force.c:2581-2755` and
//! `4084-4131`): the players it tricks lose sight of the trickster. They are kept in the
//! trickster's four `forceMindtrickTargetIndex` words, which its entity carries to every
//! client (`trickedentindex`), whose game hides it from them.
//!
//! Level 1 tricks the one the eyes' aim strikes within 256 units; levels 2 and 3 every
//! player the power is usable on within 512 units (1024 at 3), in a 180-degree arc at 2 and
//! all round at 3. It lasts 20, 25 or 30 seconds. A tricked player is set free when it
//! dies, uses seeing, gains ysalamiri, or sees the trickster attack within four frames. A
//! trickster carrying a flag cannot trick.
//!
//! Not yet: tricking NPCs (`ForceTelepathyCheckDirectNPCTarget`'s scripts and turned
//! allegiances), siege items that hinder the Force.

use crate::force_powers::{FP_SEE, FP_TELEPATHY, Forcer};

/// `MAX_TRICK_DISTANCE` (`w_saber.h:53`).
const MAX_TRICK_DISTANCE: f32 = 512.0;
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
const MASK_SOLID: u32 = 0x1;
const ENTITY_NUMBER_NONE: u16 = 1_023;
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_FORCEPUSH: u32 = 1;
const PW_REDFLAG: usize = 4;
const PW_BLUEFLAG: usize = 5;
const CHAN_AUTO: u32 = 0;
const PS_WEAPON_TIME: usize = 10;
const PS_VIEW_HEIGHT: usize = 22;
const PS_ACTIVE: usize = 82;
const PS_FORCE_HAND_EXTEND: usize = 80;
/// `forceMindtrickTargetIndex` .. `4`: sixteen players each.
const PS_MINDTRICK: [usize; 4] = [98, 99, 101, 104];

/// Whether player `number` is among those `state`'s mind trick holds (`G_IsMindTricked`).
pub fn is_tricked(state: &sjk_protocol::PlayerState, number: u16) -> bool {
    let Some(&field) = PS_MINDTRICK.get(usize::from(number / 16)) else {
        return false;
    };
    state.raw_field(field).unwrap_or(0) & (1 << (number % 16)) != 0
}

/// `WP_AddAsMindtricked` or `RemoveTrickedEnt`.
fn set_tricked(state: &mut sjk_protocol::PlayerState, number: u16, tricked: bool) {
    let Some(&field) = PS_MINDTRICK.get(usize::from(number / 16)) else {
        return;
    };
    let bits = state.raw_field(field).unwrap_or(0);
    state.set_raw_field(
        field,
        if tricked {
            bits | 1 << (number % 16)
        } else {
            bits & !(1 << (number % 16))
        },
    );
}

impl Forcer<'_, '_> {
    /// `ForceTelepathy`.
    pub(crate) fn telepathy(&mut self) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0
            || self.field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_NONE
            || self.field(PS_WEAPON_TIME) as i32 > 0
        {
            return;
        }
        if self.state.powerups[PW_REDFLAG] != 0 || self.state.powerups[PW_BLUEFLAG] != 0 {
            return;
        }
        if self.force.allow_deactivate_time < level_time && self.active(FP_TELEPATHY) {
            self.stop(FP_TELEPATHY);
            return;
        }
        if !self.usable(FP_TELEPATHY) {
            return;
        }
        self.clear_rocket_lock();
        // `ForceTelepathyCheckDirectNPCTarget`'s trace, which level 1 aims with; no NPC is
        // ever struck directly here.
        let origin = self.origin();
        let eye = [
            origin[0],
            origin[1],
            origin[2] + self.field(PS_VIEW_HEIGHT) as i32 as f32,
        ];
        let view = self.state.view_angles();
        let forward = crate::pmove::flight::flight_axes(view).0.to_array();
        let end: [f32; 3] =
            std::array::from_fn(|axis| eye[axis] + forward[axis] * MAX_TRICK_DISTANCE / 2.0);
        let aim = self
            .frame
            .others
            .trace(eye, end, self.frame.client, MASK_PLAYERSOLID);
        let level = self.force.levels[FP_TELEPATHY];
        if level == 1 {
            let number = aim.entity_number;
            if aim.fraction == 1.0
                || number == ENTITY_NUMBER_NONE
                || self.frame.others.player(number).is_none()
            {
                return;
            }
            set_tricked(self.state, number, true);
            self.start(FP_TELEPATHY, 0);
            self.tricked_now(origin);
            return;
        }
        let (arc, radius) = if level == 2 {
            (180.0, MAX_TRICK_DISTANCE)
        } else {
            (360.0, MAX_TRICK_DISTANCE * 2.0)
        };
        let (gametype, team, client) = (self.frame.gametype, self.frame.team, self.frame.client);
        let mut any = false;
        for number in 0..self.frame.others.slots() {
            if number == client {
                continue;
            }
            let Some(player) = self.frame.others.player(number) else {
                continue;
            };
            // `EntitiesInBox`.
            if (0..3).any(|axis| {
                player.absmin[axis] > origin[axis] + radius
                    || player.absmax[axis] < origin[axis] - radius
            }) {
                continue;
            }
            let towards: [f32; 3] =
                std::array::from_fn(|axis| player.state.origin()[axis] - eye[axis]);
            let other_team = player.team;
            let (pitch, yaw) = crate::damage::vector_to_angles(towards);
            if !crate::force_throw::in_field_of_vision(view, arc, [pitch, yaw])
                || !self.usable_on(number, FP_TELEPATHY)
                || crate::force_dark::same_team(gametype, team, other_team)
            {
                continue;
            }
            any = true;
            set_tricked(self.state, number, true);
        }
        if any {
            self.force.allow_deactivate_time = level_time + 1_500;
            self.start(FP_TELEPATHY, 0);
            self.tricked_now(origin);
        }
    }

    /// What a trick that took does to the trickster: the sound and the hand out for a
    /// second.
    fn tricked_now(&mut self, origin: [f32; 3]) {
        self.sound_at(origin, CHAN_AUTO, b"sound/weapons/force/distract.wav");
        self.state
            .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_FORCEPUSH);
        *self.frame.hand_extend_time = self.frame.level_time + 1_000;
    }

    /// `WP_ForcePowerRun`'s mind trick (`WP_UpdateMindtrickEnts`): each tricked player
    /// that is gone, dead or seeing is set free; one that sees the trickster attack
    /// within four frames too, and one with ysalamiri; with nobody left, or a flag in
    /// hand, the power stops.
    pub(crate) fn run_telepathy(&mut self) {
        let level_time = self.frame.level_time;
        let origin = self.origin();
        let attacked = level_time - self.force.danger_time < self.frame.since_last_frame * 4;
        // The four words hold sixty-four players.
        for number in 0..64u16 {
            if !is_tricked(self.state, number) {
                continue;
            }
            let free = match self.frame.others.player(number) {
                None => true,
                Some(player)
                    if *player.health < 1
                        || player.state.raw_field(PS_ACTIVE).unwrap_or(0) & (1 << FP_SEE) != 0 =>
                {
                    true
                }
                Some(player) => {
                    let (seen_from, ysalamiri) = (
                        player.state.origin(),
                        crate::force_powers::has_ysalamiri(player.state, self.frame.gametype),
                    );
                    if attacked {
                        // `OrgVisible`: a clear line through the world.
                        self.frame.others.in_pvs(seen_from, origin)
                            && self
                                .frame
                                .others
                                .trace(seen_from, origin, number, MASK_SOLID)
                                .fraction
                                == 1.0
                    } else {
                        ysalamiri
                    }
                }
            };
            if free {
                set_tricked(self.state, number, false);
            }
        }
        let anyone = PS_MINDTRICK.iter().any(|field| self.field(*field) != 0);
        if !anyone || self.state.powerups[PW_REDFLAG] != 0 || self.state.powerups[PW_BLUEFLAG] != 0
        {
            self.stop(FP_TELEPATHY);
        }
    }
}

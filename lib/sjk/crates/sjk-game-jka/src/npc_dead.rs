//! A dead NPC's thinks (`codemp/game/NPC.c`): `DeadThink` (`:424-561`) every frame — its box
//! flattened to its eyes and widened while it lies still, until its body's time
//! (`BodyRemovalPadTime`, `:247-288`) — then `NPC_RemoveBody` (`:130-240`) until it is
//! freed; both move the body through `CorpsePhysics` (`:61-122`): an empty command's
//! `ClientThink`, the body tilted to the slope it lies on (`pitch_roll_for_slope`,
//! `:345-417`), its sight kept among the alerts, and — once its death animation is all but
//! over — made a corpse (`CONTENTS_CORPSE`), or nothing at all if it was disintegrated.
//!
//! Nothing runs scripts (ICARUS), so none holds a body back; nothing holds a body in its
//! jaws (the rancor's, step 7). Galak's mech dying (`GM_Dying`) and the Mark I's
//! (`Mark1_dying`) are their AI's ([`crate::npc_galak`], [`crate::npc_mark1`]).
//!
//! Held to `tools/game-oracle/npccombat.c` (`game-npccombat.txt`).

use crate::npc_spawn::{FRAMETIME, NpcHost, NpcThink};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// What a dead NPC's think came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeadThought {
    /// It lies there still.
    Lies,
    /// `G_FreeEntity`: the body is gone now (with its saber).
    Gone,
}

/// `class_t`s the body's fate reads.
const CLASS_GALAKMECH: i32 = 25;
const CLASS_INTERROGATOR: i32 = 16;
const CLASS_MARK1: i32 = 23;
const CLASS_MARK2: i32 = 24;
const CLASS_PROBE: i32 = 32;
const CLASS_REMOTE: i32 = 39;
const CLASS_SENTRY: i32 = 42;
/// The droids that leave no body (`BodyRemovalPadTime` 0).
const NO_PAD: [i32; 11] = [
    29,
    11,
    34,
    35,
    CLASS_MARK1,
    CLASS_MARK2,
    CLASS_PROBE,
    41,
    CLASS_REMOTE,
    CLASS_SENTRY,
    CLASS_INTERROGATOR,
];
/// The droids hidden as their body goes (`DeadThink`'s own list, `NPC.c:536-541`).
const HIDDEN_ON_REMOVAL: [i32; 9] = [
    41,
    CLASS_REMOTE,
    CLASS_PROBE,
    29,
    11,
    34,
    35,
    CLASS_MARK2,
    CLASS_SENTRY,
];
/// The classes that blow up: removed at once (`NPC.c:155-172`).
const BLOWN_UP: [i32; 5] = [
    CLASS_REMOTE,
    CLASS_SENTRY,
    CLASS_PROBE,
    CLASS_INTERROGATOR,
    CLASS_MARK2,
];
/// `EF_NODRAW`, `EF_DISINTEGRATION`; `ps.eFlags`.
const EF_NODRAW: u32 = 1 << 8;
const EF_DISINTEGRATION: u32 = 1 << 26;
const PS_EFLAGS: usize = 17;
/// `ps.viewangles` by axis, `ps.groundEntityNum`.
const PS_VIEW_PITCH: usize = 4;
const PS_VIEW_ROLL: usize = 50;
/// `CONTENTS_CORPSE`, `CONTENTS_TRIGGER`, `MASK_SOLID`.
const CONTENTS_CORPSE: u32 = 0x200;
const CONTENTS_TRIGGER: u32 = 0x400;
const MASK_SOLID: u32 = 0x1 | 0x1000;
/// `ALERT_CLEAR_TIME`; `AEL_DISCOVERED`.
const ALERT_CLEAR_TIME: i32 = 200;
/// `ENTITYNUM_NONE`.
const ENTITY_NONE: u16 = 1_023;

/// `BodyRemovalPadTime` (`NPC.c:247-288`): how long a body lies before it goes — none for
/// the droids, ten seconds for anyone else.
fn removal_pad(class: i32) -> i32 {
    if NO_PAD.contains(&class) { 0 } else { 10_000 }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `DeadThink` (`NPC.c:424-561`) for the dead NPC at `me`, from `NPC_Think`.
    pub fn dead_think(&mut self, me: usize) -> DeadThought {
        let level_time = self.level_time;
        self.flatten(me);
        let npc = &mut self.actors[me];
        if npc.player.velocity() == [0.0; 3] {
            for (axis, low) in [(0, true), (0, false), (1, true), (1, false)] {
                self.widen(me, axis, low);
            }
        }
        let npc = &mut self.actors[me];
        if level_time >= npc.mind.fight.time_of_death + removal_pad(npc.definition.client_class) {
            let flags = npc.player.raw_field(PS_EFLAGS).unwrap_or(0);
            if flags & EF_NODRAW != 0 {
                npc.think = NpcThink::Free(level_time + FRAMETIME);
            } else {
                // `NPC_RemoveBodyEffect` does nothing any more.
                npc.think = NpcThink::RemoveBody(level_time + FRAMETIME);
                if HIDDEN_ON_REMOVAL.contains(&npc.definition.client_class) {
                    npc.player.set_raw_field(PS_EFLAGS, flags | EF_NODRAW);
                    npc.mind.fight.time_of_death = level_time + FRAMETIME * 8;
                } else {
                    npc.mind.fight.time_of_death = level_time + FRAMETIME * 4;
                }
            }
            return DeadThought::Lies;
        }
        // (`bounceCount` never goes below zero: no nodrop check.)
        self.corpse_physics(me);
        DeadThought::Lies
    }

    /// `NPC_RemoveBody` (`NPC.c:130-240`), the dead NPC's think once its time has come.
    pub fn remove_body(&mut self, me: usize) -> DeadThought {
        let level_time = self.level_time;
        self.corpse_physics(me);
        let npc = &mut self.actors[me];
        npc.think = NpcThink::RemoveBody(level_time + FRAMETIME);
        npc.mind.next_bstate_think = level_time + FRAMETIME;
        if npc.message.is_some() {
            // It still has a key.
            return DeadThought::Lies;
        }
        let class = npc.definition.client_class;
        if class == CLASS_MARK1 {
            self.mark1_dying(me);
        }
        if BLOWN_UP.contains(&class) {
            return DeadThought::Gone;
        }
        self.flatten(me);
        let npc = &mut self.actors[me];
        if class == CLASS_GALAKMECH {
            return DeadThought::Lies;
        }
        if npc.mind.fight.time_of_death <= level_time {
            npc.mind.fight.time_of_death = level_time + 1_000;
            // An enemy's or a protocol droid's think a frame on — as it already is.
            // A body with no enemy was placed dead by the map; any other goes.
            if npc.mind.enemy.is_some() {
                return DeadThought::Gone;
            }
        }
        DeadThought::Lies
    }

    /// `r.maxs[2]` down to the eyes and four units, eight below the origin at least
    /// ("don't ever inflate back up?").
    fn flatten(&mut self, me: usize) {
        let npc = &mut self.actors[me];
        npc.maxs[2] = (npc.mind.eye_point[2] - npc.current_origin[2] + 4.0).max(-8.0);
    }

    /// One side of the box a unit wider, out to 32, where that does not leave it in
    /// something solid.
    fn widen(&mut self, me: usize, axis: usize, low: bool) {
        let npc = &mut self.actors[me];
        let before = if low { npc.mins[axis] } else { npc.maxs[axis] };
        if (low && before <= -32.0) || (!low && before >= 32.0) {
            return;
        }
        let after = if low { before - 1.0 } else { before + 1.0 };
        if low {
            npc.mins[axis] = after;
        } else {
            npc.maxs[axis] = after;
        }
        let (origin, mins, maxs, number, mask) = (
            npc.current_origin,
            npc.mins,
            npc.maxs,
            npc.number,
            npc.clip_mask,
        );
        self.gather_bodies(me);
        let trace = self
            .host
            .trace(origin, mins, maxs, origin, number, mask, self.bodies);
        let npc = &mut self.actors[me];
        if trace.all_solid {
            if low {
                npc.mins[axis] = before;
            } else {
                npc.maxs[axis] = before;
            }
        }
    }

    /// The other NPCs' bodies, into the scratch a trace takes.
    fn gather_bodies(&mut self, me: usize) {
        let Self { actors, bodies, .. } = self;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|other| other.number != actors[me].number && other.contents != 0)
                .map(|other| other.body()),
        );
    }

    /// `CorpsePhysics` (`NPC.c:61-122`).
    fn corpse_physics(&mut self, me: usize) {
        let level_time = self.level_time;
        self.client_think(me, UserCommand::default());
        let npc = &self.actors[me];
        if npc.definition.client_class == CLASS_GALAKMECH {
            self.gm_dying(me);
        }
        let npc = &self.actors[me];
        let on_ground = npc.player.ground_entity_num() != ENTITY_NONE;
        let disintegrated = npc
            .state
            .raw_field(crate::npc_spawn::es::EFLAGS)
            .unwrap_or(0)
            & EF_DISINTEGRATION
            != 0;
        if on_ground && !disintegrated {
            self.slope(me);
        }
        let npc = &self.actors[me];
        // The alerts were just cleared: the body is seen again.
        if self.alerts.clear_time() == level_time + ALERT_CLEAR_TIME
            && npc.player.raw_field(PS_EFLAGS).unwrap_or(0) & EF_NODRAW == 0
        {
            let (owner, origin) = (npc.mind.enemy, npc.current_origin);
            self.alerts.add_sight(
                owner,
                origin,
                384.0,
                crate::npc_senses::AEL_DISCOVERED,
                0.0,
                level_time,
            );
        }
        let npc = &mut self.actors[me];
        if npc.mind.fight.respawn_time < level_time + 500 {
            if npc.player.raw_field(PS_EFLAGS).unwrap_or(0) & EF_DISINTEGRATION != 0 {
                npc.contents = 0;
            } else if npc.definition.client_class != CLASS_MARK1
                && npc.definition.client_class != CLASS_INTERROGATOR
            {
                npc.contents = CONTENTS_CORPSE;
            }
            if npc.message.is_some() {
                npc.contents |= CONTENTS_TRIGGER;
            }
        }
    }

    /// `pitch_roll_for_slope(npc, NULL)` (`NPC.c:345-417`): the ground's normal under the
    /// body (a line from its origin to 300 units under its feet); the view pitched and
    /// rolled to it, by how far the body faces along the slope; the box's floor raised
    /// with the pitch, and the body with it.
    fn slope(&mut self, me: usize) {
        let npc = &self.actors[me];
        let origin = npc.current_origin;
        let mut start = origin;
        start[2] += npc.mins[2] + 4.0;
        let end = [start[0], start[1], start[2] - 300.0];
        let trace = self
            .host
            .trace(origin, [0.0; 3], [0.0; 3], end, npc.number, MASK_SOLID, &[]);
        if trace.fraction >= 1.0 || trace.plane_normal == [0.0; 3] {
            return;
        }
        let npc = &mut self.actors[me];
        let (forward, right) = crate::pmove::flight::flight_axes(npc.mind.current_angles);
        let (forward, right) = (forward.to_array(), right.to_array());
        let (pitch, yaw) = crate::damage::vector_to_angles(trace.plane_normal);
        let pitch = pitch + 90.0;
        let (slope_forward, _) = crate::pmove::flight::flight_axes([0.0, yaw, 0.0]);
        let slope_forward = slope_forward.to_array();
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let side = if dot(slope_forward, right) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let along = dot(slope_forward, forward);
        let view_pitch = along * pitch;
        npc.player
            .set_raw_field(PS_VIEW_PITCH, view_pitch.to_bits());
        npc.player
            .set_raw_field(PS_VIEW_ROLL, ((1.0 - along.abs()) * pitch * side).to_bits());
        let old_floor = npc.mins[2];
        // `-24 + 12 * fabs(viewangles[PITCH]) / 180.0f`: `fabs` is a double's.
        npc.mins[2] = (-24.0 + 12.0 * f64::from(view_pitch).abs() / 180.0) as f32;
        if old_floor > npc.mins[2] {
            let mut origin = npc.player.origin();
            origin[2] += old_floor - npc.mins[2];
            npc.player.set_origin(origin);
            npc.current_origin[2] = origin[2];
            npc.relink();
        }
    }
}

//! Dismemberment (`G_Dismember`, `g_combat.c:3341-3552`): a limb cut off a client — a
//! player or an NPC — flying off as its own entity (`playerlimb`: `ET_GENERAL` with
//! `G2_MODEL_PART`, which the clients draw from the victim's model), run by `G_RunItem` and
//! `LimbThink` (`g_items.c:3218-3279`, `g_combat.c:3298-3339`) on `G_RunExPhys`
//! (`g_exphysics.c:39-297`) until its time — 8 to 16 s — is up. An NPC's own server-side
//! model loses the limb's surface and shows the cap (`G2API_SetSurfaceOnOff`), so that a
//! blade no longer meets it and the same limb is not cut twice.
//!
//! A limb is a native record of the level ([`crate::npc_groups::NpcLevel::limbs`]) with the
//! entity number the host gave it; its wire state is published through the host, and it
//! runs at its number's turn in the frame ([`crate::npc_roster`]).

use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost, es};
use crate::npc_world::NpcWorld;
use sjk_protocol::EntityState;

/// `G2_MODELPART_*` (`bg_public.h:178-187`): the parts a limb can be.
pub mod part {
    pub const HEAD: i32 = 10;
    pub const WAIST: i32 = 11;
    pub const LARM: i32 = 12;
    pub const RARM: i32 = 13;
    pub const RHAND: i32 = 14;
    pub const LLEG: i32 = 15;
    pub const RLEG: i32 = 16;
}

/// `G2_MODEL_PART` (`s.weapon` of a limb), `ET_GENERAL`.
/// `CONTENTS_NODROP` (`surfaceflags.h:45`): no bodies or items left (death fog, lava).
const CONTENTS_NODROP: u32 = 0x800;
const G2_MODEL_PART: u32 = 50;
const ET_GENERAL: u32 = 0;
/// `CONTENTS_TRIGGER`; `MASK_SOLID` (`CONTENTS_SOLID | CONTENTS_TERRAIN`), a limb's clip mask.
const CONTENTS_TRIGGER: u32 = 0x400;
const MASK_SOLID: u32 = 0x1 | 0x1000;
/// `G2SURFACEFLAG_NODESCENDANTS`: the limb's surface and all below it hidden.
const SURFACE_NO_DESCENDANTS: u32 = 0x100;
/// `TR_STATIONARY`, `TR_GRAVITY`.
const TR_STATIONARY: u32 = 0;
const TR_GRAVITY: u32 = 6;
/// `FRAMETIME`.
const FRAMETIME: i32 = 100;
/// `GT_TEAM`; `TEAM_RED`, `TEAM_BLUE`.
const GT_TEAM: i32 = 6;
const TEAM_RED: i32 = 1;
const TEAM_BLUE: i32 = 2;
/// `MAX_GRAVITY_PULL` (`g_exphysics.c:34`).
const MAX_GRAVITY_PULL: f32 = 512.0;
/// A limb's box (`g_combat.c:3423-3424`).
const LIMB_MINS: [f32; 3] = [-6.0, -6.0, -3.0];
const LIMB_MAXS: [f32; 3] = [6.0, 6.0, 6.0];
/// `s.customRGBA[0..4]`'s wire fields, of an entity and of a player state.
const ES_CUSTOM_RGBA: [usize; 4] = [30, 35, 36, 29];
const PS_CUSTOM_RGBA: [usize; 4] = [29, 42, 45, 32];

/// A limb in flight: its entity and `LimbThink`'s state.
#[derive(Clone, Debug, PartialEq)]
pub struct Limb {
    /// Its entity number.
    pub number: u16,
    /// `s`.
    pub state: EntityState,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `epVelocity`, `epGravFactor`.
    pub velocity: [f32; 3],
    pub grav_factor: f32,
    /// `speed`: when it is freed (a float, as the reference keeps it).
    pub expires: f32,
    /// `genericValue5`: when its physics next runs.
    pub next_physics: i32,
    /// `nextthink`, and whether the think is `G_FreeEntity` (its time is up, or it is stuck
    /// in something solid).
    pub next_think: i32,
    pub freeing: bool,
    /// `level.previousTime` as its last run saw it: the frame before (`G_BounceItem`'s hit
    /// time lies between).
    pub previous_time: i32,
}

impl Limb {
    /// npcmonster.c's `limb` line of it: its non-zero wire fields, where it is, its
    /// velocity, when it goes, its physics' and its think's times.
    pub fn line(&self) -> String {
        let fields: Vec<String> = (0..sjk_protocol::LEGACY_ENTITY_FIELDS.len())
            .filter_map(|index| {
                self.state
                    .raw_field(index)
                    .filter(|value| *value != 0)
                    .map(|value| format!("{index}:{value:x}"))
            })
            .collect();
        let bits = |values: [f32; 3]| {
            values
                .map(|value| format!("{:08x}", value.to_bits()))
                .join(" ")
        };
        format!(
            "limb {} {} | current {} velocity {} speed {} think {} next {}",
            self.number,
            fields.join(" "),
            bits(self.origin),
            bits(self.velocity),
            self.expires as i32,
            self.next_physics,
            self.next_think
        )
    }

    /// `limb->s.modelGhoul2`: the part it is.
    fn part(&self) -> i32 {
        self.state.raw_field(es::MODEL_GHOUL2).unwrap_or(0) as i32
    }

    fn set(&mut self, index: usize, value: u32) {
        self.state.set_raw_field(index, value);
    }

    /// `G_SetOrigin`: stationary at `origin`.
    fn set_origin(&mut self, origin: [f32; 3]) {
        for axis in 0..3 {
            self.set(es::POS_BASE[axis], origin[axis].to_bits());
            self.set(es::POS_DELTA[axis], 0);
        }
        self.set(es::POS_TYPE, TR_STATIONARY);
        self.set(es::POS_TIME, 0);
        self.set(es::POS_DURATION, 0);
        self.origin = origin;
    }
}

/// The victim of a cut as `G_Dismember` reads it.
struct Cut {
    npc: bool,
    origin: [f32; 3],
    view: [f32; 3],
    velocity: [f32; 3],
    colours: [u32; 4],
    team: i32,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_Dismember(ent, enemy, point, limbType, limbRollBase, limbPitchBase, deathAnim,
    /// postDeath)` on client `victim` (a player or an NPC), cut by `enemy`: the limb's
    /// surface found (a variant where its root is hidden), nothing where it is gone already;
    /// the limb spawned where the cut is, flying off along the cut; an NPC's own model loses
    /// the surface. The roll, pitch, death animation and `postDeath` are unread, as in the
    /// reference.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dismember(
        &mut self,
        victim: u16,
        enemy: u16,
        point: [f32; 3],
        limb: i32,
        _roll_base: f32,
        _pitch_base: f32,
        _death_anim: u16,
        _post_death: bool,
    ) {
        let Some(cut) = self.cut_victim(victim) else {
            return;
        };
        let (limb_name, cap_name) = self.limb_surfaces(victim, limb);
        if self.host.surface_status(victim, &limb_name) != 0 {
            // "is it already off? If so there's no reason to be doing it again".
            return;
        }
        let Some(number) = self.host.spawn_entity() else {
            return;
        };
        let expires = (self.level_time + self.host.irand(8_000, 16_000)) as f32;
        let mut limb_entity = Limb {
            number,
            state: EntityState::zero(number, &sjk_protocol::LEGACY_ENTITY_FIELDS),
            origin: point,
            velocity: [0.0; 3],
            grav_factor: 0.0,
            expires,
            next_physics: 0,
            next_think: self.level_time + FRAMETIME,
            freeing: false,
            previous_time: self.level_time,
        };
        limb_entity.set_origin(point);
        limb_entity.set(es::G2_RADIUS, 200);
        limb_entity.set(es::TYPE, ET_GENERAL);
        limb_entity.set(es::WEAPON, G2_MODEL_PART);
        limb_entity.set(es::MODEL_GHOUL2, limb as u32);
        limb_entity.set(es::MODEL_INDEX, u32::from(victim));
        for axis in 0..3 {
            limb_entity.set(es::APOS_BASE[axis], cut.view[axis].to_bits());
        }
        // `epVelocity`: the owner's velocity, and 80 along the cut; a head or a trunk up.
        let mut direction: [f32; 3] = std::array::from_fn(|axis| point[axis] - cut.origin[axis]);
        crate::saber_clash::normalize(&mut direction);
        limb_entity.velocity =
            std::array::from_fn(|axis| cut.velocity[axis] + 80.0 * direction[axis]);
        if limb == part::HEAD || limb == part::WAIST {
            limb_entity.velocity[2] += 10.0;
        }
        self.saber_cut_velocity(&mut limb_entity, victim, enemy);
        if cut.npc {
            self.host
                .set_npc_surface(victim, &limb_name, SURFACE_NO_DESCENDANTS);
            self.host.set_npc_surface(victim, &cap_name, 0);
        }
        let colours = if self.host.gametype() >= GT_TEAM && !cut.npc {
            match cut.team {
                TEAM_RED => [255, 0, 0, 0],
                TEAM_BLUE => [0, 0, 255, 0],
                _ => cut.colours,
            }
        } else {
            cut.colours
        };
        for (index, value) in ES_CUSTOM_RGBA.into_iter().zip(colours) {
            limb_entity.set(index, value);
        }
        self.host.publish(
            number,
            &limb_entity.state,
            (LIMB_MINS, LIMB_MAXS),
            CONTENTS_TRIGGER,
        );
        self.level.limbs.push(limb_entity);
    }

    /// What `G_Dismember` reads of the victim: an NPC's own record, or the player the host
    /// hands out.
    fn cut_victim(&mut self, victim: u16) -> Option<Cut> {
        if let Some(at) = self.actor_at(victim) {
            let npc = &self.actors[at];
            let colours = ES_CUSTOM_RGBA.map(|index| npc.state.raw_field(index).unwrap_or(0));
            return Some(Cut {
                npc: true,
                origin: npc.current_origin,
                view: npc.player.view_angles(),
                velocity: npc.player.velocity(),
                colours,
                team: npc.session_team,
            });
        }
        let body = *self
            .host
            .players()
            .iter()
            .find(|body| body.number == victim)?;
        let (view, velocity, colours) = self.with_client(victim, |state, _| {
            (
                state.view_angles(),
                state.velocity(),
                PS_CUSTOM_RGBA.map(|index| state.raw_field(index).unwrap_or(0)),
            )
        })?;
        Some(Cut {
            npc: false,
            origin: body.origin,
            view,
            velocity,
            colours,
            team: body.session_team,
        })
    }

    /// The surface a limb of `part` is and the cap left on the stump (`g_combat.c:3349-3394`),
    /// by `BG_GetRootSurfNameWithVariant` on the victim's model.
    fn limb_surfaces(&mut self, victim: u16, limb: i32) -> (String, String) {
        let mut variant = |root: &str| self.root_surface_variant(victim, root);
        match limb {
            part::HEAD => ("head".to_owned(), "torso_cap_head".to_owned()),
            part::WAIST => ("torso".to_owned(), "hips_cap_torso".to_owned()),
            part::LARM => (variant("l_arm"), format!("{}_cap_l_arm", variant("torso"))),
            part::RARM => (variant("r_arm"), format!("{}_cap_r_arm", variant("torso"))),
            part::RHAND => (
                variant("r_hand"),
                format!("{}_cap_r_hand", variant("r_arm")),
            ),
            part::LLEG => (variant("l_leg"), format!("{}_cap_l_leg", variant("hips"))),
            // "umm... just default to the right leg, I guess (same as on client)".
            _ => (variant("r_leg"), format!("{}_cap_r_leg", variant("hips"))),
        }
    }

    /// `BG_GetRootSurfNameWithVariant` (`bg_g2_utils.c:108-129`): the root surface where it is
    /// drawn, else its first drawn variant (`l_arma` .. `l_armh`), else the root.
    fn root_surface_variant(&mut self, victim: u16, root: &str) -> String {
        if self.host.surface_status(victim, root) == 0 {
            return root.to_owned();
        }
        for variant in b'a'..b'a' + 8 {
            let name = format!("{root}{}", char::from(variant));
            if self.host.surface_status(victim, &name) == 0 {
                return name;
            }
        }
        root.to_owned()
    }

    /// `G_Dismember`'s saber rule (`g_combat.c:3473-3511`): an enemy holding a saber whose
    /// blade moved between its last two readings under 200 ms apart (an NPC's own, a
    /// player's the host's) throws the limb along the blade's sweep.
    fn saber_cut_velocity(&mut self, limb: &mut Limb, victim: u16, enemy: u16) {
        const WP_SABER: u8 = 3;
        if enemy == victim {
            return;
        }
        let storage = match self.actor_at(enemy) {
            Some(at) => {
                let npc = &self.actors[at];
                (npc.player.weapon() == WP_SABER).then_some(npc.saber.storage)
            }
            None => {
                let holds = self
                    .host
                    .players()
                    .iter()
                    .any(|body| body.number == enemy && body.weapon == i32::from(WP_SABER));
                if holds {
                    self.host.player_saber_storage(enemy)
                } else {
                    None
                }
            }
        };
        let Some(storage) = storage else { return };
        if !storage.older_valid || self.level_time - storage.last_time >= 200 {
            return;
        }
        limb.velocity = limb.velocity.map(|axis| axis * 0.4);
        let mut sweep: [f32; 3] =
            std::array::from_fn(|axis| storage.last_base[axis] - storage.older_base[axis]);
        let distance = crate::saber_clash::normalize(&mut sweep);
        for axis in 0..3 {
            limb.velocity[axis] += sweep[axis] * (distance * 1.2);
        }
        let Some((torso_timer, torso)) = self.with_client(victim, |state, _| {
            (state.torso_timer(), state.torso_animation())
        }) else {
            return;
        };
        if torso_timer > 0 || !crate::pmove_roll_anim::death(torso) {
            let mut across = [limb.velocity[0], limb.velocity[1], 0.0];
            if crate::saber_clash::normalize(&mut across) < 40.0 {
                limb.velocity[0] = across[0] * 40.0;
                limb.velocity[1] = across[1] * 40.0;
            }
        } else {
            limb.velocity = limb.velocity.map(|axis| axis * 0.3);
        }
    }

    /// `G_RunItem` for the limb at `at` in the level's list (`g_items.c:3218-3279`): pulled
    /// down once nothing is under it; at rest only its think (`LimbThink`); else traced to
    /// where its trajectory has it, its think, and — stopped short — `G_BounceItem`
    /// (`g_items.c:3165-3209`: `physicsBounce` is never set, so no speed is kept; on a floor
    /// it rests a unit above it, snapped). A limb's clip mask has no no-drop volume in it
    /// that frees it. Its trace passes its `r.ownerNum`, never set: entity 0.
    pub(crate) fn limb_run(&mut self, at: usize) {
        let level_time = self.level_time;
        let limb = &mut self.level.limbs[at];
        let previous_time = std::mem::replace(&mut limb.previous_time, level_time);
        if limb.state.raw_field(es::GROUND_ENTITY).unwrap_or(0) == u32::from(ENTITYNUM_NONE)
            && limb.state.raw_field(es::POS_TYPE) != Some(TR_GRAVITY)
        {
            limb.set(es::POS_TYPE, TR_GRAVITY);
            limb.set(es::POS_TIME, level_time as u32);
        }
        if limb.state.raw_field(es::POS_TYPE) == Some(TR_STATIONARY) {
            self.limb_run_think(at);
            return;
        }
        let number = limb.number;
        let read = |index: usize| limb.state.raw_field(index).unwrap_or(0);
        let vector =
            |fields: [usize; 3]| -> [f32; 3] { fields.map(|index| f32::from_bits(read(index))) };
        let (base, delta, kind, start, duration) = (
            vector(es::POS_BASE),
            vector(es::POS_DELTA),
            read(es::POS_TYPE) as u8,
            read(es::POS_TIME) as i32,
            read(es::POS_DURATION) as i32,
        );
        let target = crate::trajectory::legacy_evaluate_trajectory(
            base, delta, kind, start, duration, level_time,
        );
        let origin = limb.origin;
        let mut trace = self.trace_bodies(origin, LIMB_MINS, LIMB_MAXS, target, 0, MASK_SOLID);
        self.level.limbs[at].origin = trace.end_position;
        if trace.start_solid {
            trace.fraction = 0.0;
        }
        let state = self.level.limbs[at].state.clone();
        self.host
            .publish(number, &state, (LIMB_MINS, LIMB_MAXS), CONTENTS_TRIGGER);
        if trace.fraction == 1.0 {
            self.limb_run_think(at);
            return;
        }
        if !self.limb_run_think(at) {
            // Freed by its think, it meets `G_RunItem`'s no-drop test at its cleared origin,
            // where a no-drop volume frees the slot again (`G_FreeEntity` clears it whole);
            // otherwise it is bounced all the same: from a cleared, stationary trajectory,
            // so on a floor it "stops" and the freed slot keeps the floor as its
            // `s.groundEntityNum`.
            if self.host.world_point_contents([0.0; 3]) & CONTENTS_NODROP == 0
                && trace.plane_normal[2] > 0.0
            {
                self.host.freed_ground_residue(number, trace.entity_number);
            }
            return;
        }
        // "if it is in a nodrop volume, remove it" (`g_items.c:3265-3276`).
        let Some(at) = self
            .level
            .limbs
            .iter()
            .position(|limb| limb.number == number)
        else {
            return;
        };
        if self.host.world_point_contents(self.level.limbs[at].origin) & CONTENTS_NODROP != 0 {
            self.level.limbs.remove(at);
            self.host.free_model(number);
            return;
        }
        let limb = &mut self.level.limbs[at];
        let hit_time =
            previous_time + ((level_time - previous_time) as f32 * trace.fraction) as i32;
        let delta_now = crate::trajectory::legacy_evaluate_trajectory_delta(
            delta, kind, start, duration, hit_time,
        );
        let normal = trace.plane_normal;
        let dot = delta_now[0] * normal[0] + delta_now[1] * normal[1] + delta_now[2] * normal[2];
        let reflected: [f32; 3] =
            std::array::from_fn(|axis| (delta_now[axis] + -2.0 * dot * normal[axis]) * 0.0);
        for axis in 0..3 {
            limb.set(es::POS_DELTA[axis], reflected[axis].to_bits());
        }
        if normal[2] > 0.0 && reflected[2] < 40.0 {
            let mut end = trace.end_position;
            end[2] += 1.0;
            limb.set_origin(crate::weapon_fire::snap_vector(end));
            limb.set(es::GROUND_ENTITY, u32::from(trace.entity_number));
        } else {
            let current: [f32; 3] = std::array::from_fn(|axis| limb.origin[axis] + normal[axis]);
            limb.origin = current;
            for axis in 0..3 {
                limb.set(es::POS_BASE[axis], current[axis].to_bits());
            }
            limb.set(es::POS_TIME, level_time as u32);
        }
    }

    /// `G_RunThink` for the limb at `at`: its think (`LimbThink`) where it is due. Whether it
    /// is still there.
    fn limb_run_think(&mut self, at: usize) -> bool {
        let limb = &self.level.limbs[at];
        if limb.next_think <= 0 || limb.next_think > self.level_time {
            return true;
        }
        let number = limb.number;
        self.level.limbs[at].next_think = 0;
        self.limb_think(at);
        self.level.limbs.iter().any(|limb| limb.number == number)
    }

    /// `LimbThink` (`g_combat.c:3298-3339`): freed once its time is up; otherwise its
    /// physics every 50 ms, and its think every frame.
    fn limb_think(&mut self, at: usize) {
        let level_time = self.level_time;
        let limb = &mut self.level.limbs[at];
        if limb.freeing {
            // `G_FreeEntity`: its `s.modelGhoul2` names it for the clients to drop.
            let number = limb.number;
            self.level.limbs.remove(at);
            self.host.free_model(number);
            return;
        }
        let (mass, bounce) = match limb.part() {
            part::HEAD => (0.08, 1.4),
            part::WAIST => (0.1, 1.2),
            _ => (0.09, 1.3),
        };
        if limb.expires < level_time as f32 {
            limb.freeing = true;
            limb.next_think = level_time;
            return;
        }
        if limb.next_physics <= level_time {
            self.run_ex_phys(at, 3.0, mass, bounce);
            self.level.limbs[at].next_physics = level_time + 50;
        }
        self.level.limbs[at].next_think = level_time;
    }

    /// `G_RunExPhys(ent, gravity, mass, bounce, qtrue, NULL, 0)` (`g_exphysics.c:39-297`)
    /// for the limb at `at`: gravity while nothing is under it, the move along its
    /// velocity (a tenth of it a step), slowed by its mass, bouncing off what it meets —
    /// freed at once when it is stuck in something solid.
    fn run_ex_phys(&mut self, at: usize, gravity: f32, mass: f32, bounce: f32) {
        let (number, origin) = (self.level.limbs[at].number, self.level.limbs[at].origin);
        let mut ground = origin;
        ground[2] -= 0.1;
        let trace = self.trace_bodies(origin, LIMB_MINS, LIMB_MAXS, ground, number, MASK_SOLID);
        let limb = &mut self.level.limbs[at];
        let ground_entity = if trace.fraction == 1.0 {
            ENTITYNUM_NONE
        } else {
            trace.entity_number
        };
        limb.set(es::GROUND_ENTITY, u32::from(ground_entity));
        if ground_entity == ENTITYNUM_NONE {
            limb.grav_factor = (limb.grav_factor + gravity).min(MAX_GRAVITY_PULL);
            limb.velocity[2] -= limb.grav_factor;
        } else {
            limb.grav_factor = 0.0;
        }
        if limb.velocity == [0.0; 3] {
            // Its touch (`LimbTouch`) does nothing.
            return;
        }
        let projected: [f32; 3] =
            std::array::from_fn(|axis| limb.origin[axis] + 0.1 * limb.velocity[axis]);
        let keep = 1.0 - mass;
        limb.velocity = limb.velocity.map(|axis| axis * keep);
        let mut unit = limb.velocity;
        let mut total = crate::saber_clash::normalize(&mut unit);
        if total < 1.0 && ground_entity != ENTITYNUM_NONE {
            limb.velocity = [0.0; 3];
            limb.grav_factor = 0.0;
            let state = limb.state.clone();
            self.host
                .publish(number, &state, (LIMB_MINS, LIMB_MAXS), CONTENTS_TRIGGER);
            return;
        }
        let trace = self.trace_bodies(origin, LIMB_MINS, LIMB_MAXS, projected, number, MASK_SOLID);
        let limb = &mut self.level.limbs[at];
        if trace.start_solid || trace.all_solid {
            // "can't go anywhere from here": `autoKill`.
            limb.freeing = true;
            limb.next_think = self.level_time;
            return;
        }
        limb.set_origin(trace.end_position);
        let state = limb.state.clone();
        self.host
            .publish(number, &state, (LIMB_MINS, LIMB_MAXS), CONTENTS_TRIGGER);
        if trace.fraction == 1.0 {
            return;
        }
        let limb = &mut self.level.limbs[at];
        total *= bounce;
        let rebound = trace.plane_normal.map(|axis| axis * total);
        if rebound[2] > 0.0 {
            limb.grav_factor = (limb.grav_factor - rebound[2] * keep).max(0.0);
        }
        for axis in 0..3 {
            limb.velocity[axis] += rebound[axis];
        }
    }
}

//! An NPC's Ghoul2 instance on the server, as a saber meets it.
//!
//! `NPC_ParseParms` gives every NPC a server-side instance (`SetupGameGhoul2Model`,
//! `g_client.c:1562`): its `playerModel` with no humanoid check (an NPC keeps its own
//! skeleton), a missing model replaced by Kyle's, and — for an NPC whose definition arms it
//! with a saber — its sabers' hilts bolted to its hands (`G_SaberModelSetup`). Every frame,
//! at the NPC's turn in `G_RunFrame` (`g_main.c:3342-3346`), `WP_SaberPositionUpdate`
//! poses it as it poses a player's: `G_G2PlayerAngles` — only while some client has the
//! NPC in its potentially visible set (`w_saber.c:911-931`) — and `G_UpdateClientAnims`.
//! Its `modelScale` (the definition's `scale`) scales every bolt and every vertex.
//!
//! A blade that meets the NPC's box is then tested against these posed triangles
//! (`G_G2TraceCollide`, `w_saber.c:2315`, at the NPC's `ps.origin` and view yaw); a hit
//! stamps the surface it struck (`g2LastSurfaceHit`), which places the blow
//! (`G_LocationBasedDamageModifier`).
//!
//! A creature keeps its own skeleton (step 535): the rancor's, the wampa's, the howler's, a
//! droid's — its own GLA and `animation.cfg`, `G_UpdateClientAnims`' non-humanoid rules
//! (an animation its file lacks is skipped, the torso's only on a `lower_lumbar` bone, no
//! `Motion`), no spine turned, a vehicle's root alone. Its bolts are what its attacks read
//! ([`NpcSkeleton::bolt`]), and a blade meets its triangles as it meets a humanoid's.

use crate::damage::HitLocation;
use crate::g2_player_angles::{AngleInputs, AngleMemory, corrects_for_motion, player_angles};
use crate::npc_spawn::NpcActor;
use crate::saber_damage::{Ghoul2Answer, location_from_surface, placed_by_surface};
use crate::server_skeleton::{
    AnimationInputs, Blade, CollisionQuery, MAX_BLADES, ServerSkeleton, SkeletonModels,
    proper_origin,
};
use sjk_model::ModelError;
use sjk_model::g2_collision::{CollisionRecord, CollisionScratch};
use std::sync::Arc;

/// `WP_SABER`.
const WP_SABER: i32 = 3;
/// Player-state fields read without an accessor: `fd.saberAnimLevel`, `saberLockFrame`,
/// `m_iVehicleNum`, `hasLookTarget`, `lookTarget`.
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_LOCK_FRAME: usize = 108;
/// `FP_RAGE` in `fd.forcePowersActive`: animations at twice the speed.
const FP_RAGE: u32 = 1 << crate::force_powers::FP_RAGE;
/// The entity fields `BG_G2PlayerAngles` reads: `pos.trDelta`, `eFlags`, `angles2[YAW]`,
/// `groundEntityNum`, `saberMove`.
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_EFLAGS: usize = 19;
const ES_LEGS_ANIM: usize = 16;
const ES_TORSO_ANIM: usize = 17;
const ES_ANGLES2_YAW: usize = 51;
const ES_GROUND_ENTITY: usize = 22;
const ES_SABER_MOVE: usize = 43;
/// `s.forceFrame`, `s.m_iVehicleNum`.
const ES_FORCE_FRAME: usize = 85;
const ES_VEHICLE: usize = 93;

/// A `Motion` bolt with no offset, for the one frame a new skeleton has none.
const LEVEL_MOTION: [[f32; 4]; 3] = [
    [0.0, -1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];
/// `g_g2TraceLod`.
const TRACE_LOD: usize = 3;
/// `sv_fps`: the frame rate a blade's lead is measured by.
const SERVER_FPS: f32 = 20.0;

/// Every blade of an NPC's two hilts as its pose read them (`muzzlePoint`, `muzzleDir`):
/// `None` where it has no such blade or none was read.
pub type NpcBlades = [[Option<Blade>; MAX_BLADES]; 2];

/// The models an NPC's instance is built from: its `playerModel`, and the hilts of the
/// sabers its definition arms it with (none unless its weapon is the saber).
#[derive(Clone, Copy, Debug)]
pub struct NpcModels<'a> {
    /// `playerModel`: `models/players/<model>/model.glm`.
    pub model: &'a [u8],
    /// Each hand's saber, when the instance carries its hilt.
    pub sabers: Option<&'a [crate::saber_definition::SaberDefinition; 2]>,
}

/// The classes `WP_SaberDoHit` makes no blood for (`w_saber.c:3608-3616`): the droids, the
/// seeker, the probe, the AT-ST (`class_t`).
const BLOODLESS: [i32; 13] = [41, 32, 29, 39, 11, 34, 35, 33, 23, 24, 16, 1, 42];

/// `CLASS_VEHICLE`: its skeleton's root alone is animated.
const CLASS_VEHICLE: i32 = 53;
/// The classes `G_GetHitLocFromSurfName` places no blow on by the surface struck
/// (`g_combat.c:3717-3727`): R2-D2, R5-D2, the gonk, the mouse, the sentry, the
/// interrogator, the probe.
const SURFACELESS: [i32; 7] = [34, 35, 11, 29, 42, 16, 32];
/// `CLASS_RANCOR`.
const CLASS_RANCOR: i32 = 54;

/// The `renderInfo.torsoBolt` a creature's torso point is read from: the rancor's jaw
/// (`Rancor_SetBolts`); every other creature has none that its model holds (the wampa's
/// `lower_spine` is not on its skeleton), so its torso point is its origin.
fn creature_torso_bolt(class: i32) -> Option<&'static str> {
    (class == CLASS_RANCOR).then_some("jaw_bone")
}

/// Whether a blade's hit on an NPC of `class` bleeds (`EV_SABER_HIT`'s size by the
/// damage) rather than flares, as a droid's does.
pub fn bleeds(class: i32) -> bool {
    !BLOODLESS.contains(&class)
}

/// Which models `npc`'s instance is built from (`SetupGameGhoul2Model` with the parse's
/// weapon: `g_client.c:1896-1925`).
pub fn npc_models(npc: &NpcActor) -> NpcModels<'_> {
    let armed = npc.definition.weapon == Some(WP_SABER);
    NpcModels {
        model: &npc.definition.player_model,
        sabers: armed.then_some(&npc.definition.sabers),
    }
}

/// One NPC's server-side instance: its models, its pose, what `G_G2PlayerAngles`
/// remembers, and the surface a blade last struck.
pub struct NpcSkeleton {
    models: Arc<SkeletonModels>,
    skeleton: ServerSkeleton,
    memory: AngleMemory,
    /// `client->g2LastSurfaceHit`, `g2LastSurfaceTime`.
    last_surface: (usize, i32),
}

/// `G_UpdateClientAnims`' inputs for `npc`: its halves' animations and flips, weapon,
/// style, broken limbs, lock frame, rage's double speed and its hilts' speed scales.
fn animation_inputs(npc: &NpcActor) -> AnimationInputs {
    let state = &npc.player;
    let sabers = &npc.definition.sabers;
    let held = |hand: usize| {
        if sabers[hand].is_held() {
            sabers[hand].anim_speed_scale
        } else {
            1.0
        }
    };
    AnimationInputs {
        legs: state.leg_animation(),
        torso: state.torso_animation(),
        legs_flip: state.leg_flip(),
        torso_flip: state.torso_flip(),
        weapon: state.weapon(),
        saber_style: state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as i32,
        broken_limbs: i32::from(state.broken_limbs()),
        saber_lock_frame: state.raw_field(PS_SABER_LOCK_FRAME).unwrap_or(0) as i32,
        speed_scale: if state.force_powers_active() & FP_RAGE != 0 {
            2.0
        } else {
            1.0
        },
        hilt_speed_scales: [held(0), held(1)],
    }
}

impl NpcSkeleton {
    /// The instance for `npc` on `models` as `SetupGameGhoul2Model` left it at the NPC's
    /// spawn: scaled as its definition says (`scale`: an NPC's `modelScale`), a saber
    /// carrier's root looping its first frames from the spawn's time (`g_client.c:1905`),
    /// and nothing installed by `G_UpdateClientAnims` yet — the NPC's client was zeroed, so
    /// animation 0 is taken as installed.
    pub fn new(models: Arc<SkeletonModels>, npc: &NpcActor) -> Self {
        let mut skeleton = ServerSkeleton::new(&models);
        skeleton.assume_zeroed_client();
        if let Some(scale) = npc.definition.scale.filter(|scale| scale.percent != 100) {
            skeleton.set_scale([scale.scale; 3]);
        }
        if npc.definition.client_class == CLASS_VEHICLE {
            skeleton.set_root_only();
        }
        if npc_models(npc).sabers.is_some() {
            // A fresh skeleton takes any animation; the error is the models' own.
            skeleton
                .play_setup_loop(&models, npc.spawn_time)
                .expect("the humanoid skeleton has a root");
        }
        Self {
            models,
            skeleton,
            memory: AngleMemory::default(),
            last_surface: (0, 0),
        }
    }

    /// Whether the NPC's skeleton is the humanoid one (`localAnimIndex <= 1`).
    pub fn humanoid(&self) -> bool {
        self.models.humanoid()
    }

    /// `G_GetBoltPosition(npc, bolt, pos, 0)` (`NPC_utils.c:1723-1756`): where the named bolt
    /// of its model is, the model at its `r.currentOrigin` facing its view's yaw, posed at
    /// the Ghoul2 clock. A bolt the model lacks (`G2API_AddBolt`'s -1, `bolt` `None`) reads
    /// as the origin itself, as `G2API_GetBoltMatrix` answers for it.
    pub fn bolt(
        &mut self,
        npc: &NpcActor,
        bolt: Option<&str>,
        ghoul2_time: i32,
    ) -> Result<[f32; 3], ModelError> {
        let (origin, yaw) = (npc.current_origin, npc.player.view_angles()[1]);
        let Some(bolt) = bolt else { return Ok(origin) };
        Ok(self
            .skeleton
            .bolt_point(&self.models, bolt, yaw, origin, ghoul2_time)?
            .unwrap_or(origin))
    }

    /// `G2API_GetSurfaceRenderStatus` on the instance ([`ServerSkeleton::surface_status`]).
    pub fn surface_status(&self, name: &str) -> i32 {
        self.skeleton.surface_status(&self.models, name)
    }

    /// `NPC_SetBoneAngles`' `G2API_SetBoneAngles` on the instance (`NPC_utils.c:1010`): a
    /// 100 ms blend, set at `level_time` ([`ServerSkeleton::set_bone_angles_named`]).
    pub fn set_bone_angles(&mut self, bone: &str, angles: [f32; 3], level_time: i32) {
        self.skeleton
            .set_bone_angles_named(&self.models, bone, angles, 100, level_time);
    }

    /// `G2API_SetSurfaceOnOff` on the instance ([`ServerSkeleton::set_surface_on_off`]).
    pub fn set_surface(&mut self, name: &str, flags: u32) {
        self.skeleton.set_surface_on_off(&self.models, name, flags);
    }

    /// `G2API_GetBoltMatrix` of the named bolt with the model at `origin` facing `yaw`, whole
    /// (`BG_AttachToRancor` reads its axes); `None` for a bolt the model lacks.
    pub fn bolt_matrix(
        &mut self,
        bolt: &str,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        self.skeleton
            .bolt_matrix(&self.models, bolt, yaw, origin, ghoul2_time)
    }

    /// Where the feet are for `PM_FootSlopeTrace` with the model at `origin` facing `yaw`
    /// ([`ServerSkeleton::foot_points`]); `None` for a creature's model.
    pub fn foot_points(
        &mut self,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<([f32; 3], [f32; 3])>, ModelError> {
        self.skeleton
            .foot_points(&self.models, yaw, origin, ghoul2_time)
    }

    /// [`Self::bolt_matrix`] turned by all of `angles` ([`ServerSkeleton::bolt_matrix_turned`]).
    pub fn bolt_matrix_turned(
        &mut self,
        bolt: &str,
        angles: [f32; 3],
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[[f32; 4]; 3]>, ModelError> {
        self.skeleton
            .bolt_matrix_turned(&self.models, bolt, angles, origin, ghoul2_time)
    }

    /// `WP_SaberPositionUpdate`'s skeleton half for the NPC, at `level_time`: the spine's
    /// angles when a client might see it (`seen`), reading the `Motion` bolt at the Ghoul2
    /// clock (`ghoul2_time`) when the spine corrects for it; for a saber carrier holding its
    /// lit saber (`read_blades`, [`crate::npc_saber::reads_blades`]) every blade of its hilts
    /// read at that clock, from where it seems to be (`properOrigin`, its velocity's lead)
    /// facing the legs' angles (`w_saber.c:8566-8580`, `8871-8878`) — the view's yaw alone
    /// when nobody sees it. Call [`Self::update_animations`] after the blade sweeps, which
    /// can start a lock and change the animations that `finalUpdate` installs.
    /// `look_target` is where its look target stands, when it has one (`ps.hasLookTarget`).
    pub fn pose(
        &mut self,
        npc: &NpcActor,
        seen: bool,
        look_target: Option<[f32; 3]>,
        read_blades: bool,
        level_time: i32,
        ghoul2_time: i32,
    ) -> Result<NpcBlades, ModelError> {
        let state = &npc.player;
        let (legs, torso, weapon) = (
            state.leg_animation(),
            state.torso_animation(),
            state.weapon(),
        );
        // `properAngles`: the view's yaw, which `G_G2PlayerAngles` turns into the legs'.
        let mut legs_angles = [0.0, state.view_angles()[1], 0.0];
        // A creature's skeleton has no spine to turn ("don't do these things on
        // non-humanoids", `w_saber.c:938`); a droid's head is its own step's.
        if seen && self.models.humanoid() {
            // `BG_G2PlayerAngles` reads the entity (`&ent->s`), as the NPC's last move left it:
            // its velocity, flags, way, ground and saber move — which its Force update may have
            // changed in the player state since (a Force jump's velocity and ground).
            let entity = |index: usize| npc.state.raw_field(index).unwrap_or(0);
            let inputs = AngleInputs {
                client_animations: Some([legs, torso]),
                origin: state.origin(),
                view: state.view_angles(),
                velocity: ES_POS_DELTA.map(|index| f32::from_bits(entity(index))),
                legs: entity(ES_LEGS_ANIM) as u16,
                torso: entity(ES_TORSO_ANIM) as u16,
                weapon,
                eflags: entity(ES_EFLAGS),
                movement_dir: f32::from_bits(entity(ES_ANGLES2_YAW)) as i32,
                ground: entity(ES_GROUND_ENTITY) as u16,
                saber_move: entity(ES_SABER_MOVE),
                // `cent->forceFrame`, `cent->m_iVehicleNum`: the entity's, a lock's frame
                // reaching it at the NPC's next move (`bg_pmove.c:9119`).
                held: entity(ES_FORCE_FRAME) != 0 || entity(ES_VEHICLE) != 0,
                look_target,
            };
            let Self {
                models,
                skeleton,
                memory,
                ..
            } = self;
            let motion = if corrects_for_motion(&inputs) {
                skeleton.motion_bolt(models, inputs.origin, ghoul2_time)?
            } else {
                None
            };
            let mut angles = player_angles(&inputs, memory, level_time, || {
                motion.unwrap_or(LEVEL_MOTION)
            });
            // Held still, an NPC's spine keeps the angles it had (`bg_pmove.c:9130-9137`
            // zeroes a player's only).
            if crate::g2_player_angles::held_still(&inputs) {
                angles.bones = [None; 5];
            }
            skeleton.set_angles(models, &angles)?;
            legs_angles = angles.legs;
        }
        let mut blades = [[None; MAX_BLADES]; 2];
        if read_blades {
            let origin = proper_origin(state.origin(), state.velocity(), SERVER_FPS);
            let Self {
                models, skeleton, ..
            } = self;
            for (saber, row) in blades.iter_mut().enumerate() {
                for (blade, slot) in row.iter_mut().enumerate().take(models.blade_count(saber)) {
                    *slot = skeleton.blade_of(
                        models,
                        saber,
                        blade,
                        legs_angles,
                        origin,
                        ghoul2_time,
                    )?;
                }
            }
        }
        Ok(blades)
    }

    /// `G_UpdateClientAnims(npc, 1.0f)` alone (`g_client.c:2833`): the NPC's current
    /// animations given to its model at `level_time`, as `player_die` does before it reads a
    /// limb's bone ([`crate::npc_dismember_check`]).
    pub fn update_animations(&mut self, npc: &NpcActor, level_time: i32) -> Result<(), ModelError> {
        self.skeleton
            .update_animations(&self.models, &animation_inputs(npc), level_time)
    }

    /// Where the first hilt's first blade points (`G2API_GetBoltMatrix(ghoul2, 1, 0, ...)`'s
    /// negated second axis) with the model at `origin` turned by `angles`, at the Ghoul2 clock;
    /// `None` for an NPC that holds no hilt.
    pub fn hilt_direction(
        &mut self,
        angles: [f32; 3],
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Result<Option<[f32; 3]>, ModelError> {
        Ok(self
            .skeleton
            .blade_of(&self.models, 0, 0, angles, origin, ghoul2_time)?
            .map(|blade| blade.direction))
    }

    /// `g2LastSurfaceHit`, `g2LastSurfaceTime`: the surface a blade last struck, and when.
    pub fn last_surface(&self) -> (usize, i32) {
        self.last_surface
    }

    /// `G_G2TraceCollide` on the NPC for a blade's trace from `start` to `end` with a box
    /// of `radius` (`G2API_CollisionDetect` at its `ps.origin`, facing its view's yaw, posed
    /// at `level_time`): the first record, whose surface is stamped as the one struck.
    pub fn collide(
        &mut self,
        npc: &NpcActor,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
        scratch: &mut CollisionScratch,
        records: &mut Vec<CollisionRecord>,
    ) -> Result<Ghoul2Answer, ModelError> {
        let query = CollisionQuery {
            origin: npc.player.origin(),
            yaw: npc.player.view_angles()[1],
            time: level_time,
            start,
            end,
            lod: TRACE_LOD,
            radius,
        };
        if !self
            .skeleton
            .collide(&self.models, &query, scratch, records)?
        {
            return Ok(Ghoul2Answer::Miss);
        }
        Ok(match records.first() {
            Some(record) => {
                self.last_surface = (record.surface, level_time);
                Ghoul2Answer::Hit {
                    position: record.position,
                    normal: record.normal,
                }
            }
            None => Ghoul2Answer::Miss,
        })
    }

    /// `SV_ClipMoveToEntities`' Ghoul2 half on the NPC for a trace that asks for it (a
    /// missile's, `d_projectileGhoul2Collision`): `G2API_CollisionDetect` on its model at
    /// its `r.currentOrigin`, facing its `r.currentAngles`' yaw alone (`sv_world.cpp:
    /// 737-765`: an entity beyond the clients), posed at `level_time`. The first record's
    /// place, normal and surface; the surface is not stamped here — only the entity the
    /// whole trace ends on is ([`Self::stamp_surface`], `g_missile.c:853-861`).
    #[allow(clippy::too_many_arguments)]
    pub fn trace_collide(
        &mut self,
        npc: &NpcActor,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
        scratch: &mut CollisionScratch,
        records: &mut Vec<CollisionRecord>,
    ) -> Result<Option<([f32; 3], [f32; 3], usize)>, ModelError> {
        self.trace_collide_at(
            (npc.current_origin, npc.mind.current_angles[1]),
            start,
            end,
            radius,
            level_time,
            scratch,
            records,
        )
    }

    /// [`Self::trace_collide`] with the NPC's pose given (`r.currentOrigin`, the yaw of
    /// `r.currentAngles`), for a caller that holds no borrow of the NPC.
    #[allow(clippy::too_many_arguments)]
    pub fn trace_collide_at(
        &mut self,
        (origin, yaw): ([f32; 3], f32),
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
        scratch: &mut CollisionScratch,
        records: &mut Vec<CollisionRecord>,
    ) -> Result<Option<([f32; 3], [f32; 3], usize)>, ModelError> {
        let query = CollisionQuery {
            origin,
            yaw,
            time: level_time,
            start,
            end,
            lod: TRACE_LOD,
            radius,
        };
        if !self
            .skeleton
            .collide(&self.models, &query, scratch, records)?
        {
            return Ok(None);
        }
        Ok(records
            .first()
            .map(|record| (record.position, record.normal, record.surface)))
    }

    /// `gPainHitLoc`'s part of the NPC ([`crate::npc_machine_parts::surface_part`]): the
    /// machine part of the surface struck at `level_time`, if one was.
    pub fn struck_part(&self, npc: &NpcActor, level_time: i32) -> Option<i32> {
        let (surface, at) = self.last_surface;
        if at != level_time {
            return None;
        }
        crate::npc_machine_parts::surface_part(
            npc.definition.client_class,
            self.models.surface_name(surface)?,
        )
    }

    /// `g2LastSurfaceHit = surface`, `g2LastSurfaceTime = level_time`: what a missile's
    /// run records of the model surface it struck, which then places its blow.
    pub fn stamp_surface(&mut self, surface: usize, level_time: i32) {
        self.last_surface = (surface, level_time);
    }

    /// `G_LocationBasedDamageModifier`'s surface half for a blow on the NPC: when a blade
    /// struck its model this frame, the body part of that surface (`G_GetHitLocFromSurfName`
    /// on a humanoid): the knees, hands and feet read at its `r.currentOrigin` facing its
    /// `r.currentAngles`' yaw, the torso point at its `ps.origin` facing its view
    /// (`UpdateClientRenderBolts`), each at the Ghoul2 clock. `None` leaves the blow to its
    /// box (`G_GetHitLocation`).
    pub fn surface_location(
        &mut self,
        npc: &NpcActor,
        flags: u32,
        spot: [f32; 3],
        level_time: i32,
        ghoul2_time: i32,
    ) -> Option<HitLocation> {
        let (surface, struck_at) = self.last_surface;
        if !placed_by_surface(flags, struck_at, level_time) {
            return None;
        }
        // "we don't care about per-surface hit-locations or dismemberment for these guys"
        if SURFACELESS.contains(&npc.definition.client_class) {
            return Some(HitLocation::None);
        }
        let Self {
            models, skeleton, ..
        } = self;
        let name = models.surface_name(surface)?;
        // A machine's parts (`g_combat.c:3739-3822`).
        if let Some(location) =
            crate::npc_machine_parts::surface_location(npc.definition.client_class, name)
        {
            return Some(location);
        }
        let (current, current_yaw) = (npc.current_origin, npc.mind.current_angles[1]);
        let (origin, view_yaw) = (npc.player.origin(), npc.player.view_angles()[1]);
        let humanoid = models.humanoid();
        let torso = creature_torso_bolt(npc.definition.client_class);
        Some(location_from_surface(
            name,
            spot,
            &mut |bolt, facing_view| {
                let (at, yaw) = if facing_view {
                    (origin, view_yaw)
                } else {
                    (current, current_yaw)
                };
                if humanoid {
                    return skeleton
                        .bolt_point(models, bolt, yaw, at, ghoul2_time)
                        .ok()
                        .flatten();
                }
                // A creature: no knee, hand or foot bolt is added (`g_combat.c:3729-3737`); its
                // torso point is its own `torsoBolt`'s, or its origin where it has none.
                if !facing_view {
                    return None;
                }
                match torso {
                    Some(torso) => Some(
                        skeleton
                            .bolt_point(models, torso, yaw, at, ghoul2_time)
                            .ok()
                            .flatten()
                            .unwrap_or(at),
                    ),
                    None => Some(at),
                }
            },
        ))
    }
}

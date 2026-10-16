//! The NPCs' server-side models on this server ([`sjk_game_jka::npc_skeleton`]): each
//! NPC's Ghoul2 instance built from the level's files the first time its turn comes,
//! posed at that turn every frame (`WP_SaberPositionUpdate`, through
//! [`sjk_game_jka::npc_spawn::NpcHost::pose_npc`]), and forgotten with the NPC. A player's
//! blade meets them here: a blade stopped by an NPC's box is tested against its posed
//! triangles, and the blow is placed by the surface it struck.
//!
//! Kept by the NPC's entity number, which the pool gives out and takes back when the NPC is
//! freed; the list grows with the level's NPCs and has no fixed size.

use super::host::ServerHost;
use super::*;
use sjk_game_jka::damage::HitLocation;
use sjk_game_jka::npc_skeleton::{NpcBlades, NpcSkeleton, npc_models};
use sjk_game_jka::npc_spawn::NpcHost;
use sjk_game_jka::saber_damage::{Ghoul2Answer, SaberVictim};

/// `s.eFlags`' wire field.
const ES_EFLAGS: usize = 19;
/// `GT_POWERDUEL`: the one game type in which an NPC and a player can be on the same team
/// (`OnSameTeam` compares their duel teams; an NPC's is none).
const GT_POWERDUEL: i32 = 4;

/// Every NPC's server-side model, by its entity number, and the buffers an NPC's blade's
/// collisions reuse.
#[derive(Default)]
pub(super) struct NpcBodies {
    bodies: Vec<(u16, NpcSkeleton)>,
    /// The vehicles' own skeletons ([`super::vehicle_bodies`]).
    pub(super) vehicles: super::vehicle_bodies::VehicleBodies,
    pub(super) scratch: sjk_model::g2_collision::CollisionScratch,
    pub(super) records: Vec<sjk_model::g2_collision::CollisionRecord>,
}

impl NpcBodies {
    /// The model of NPC `number`, if it has one.
    pub(super) fn get(&mut self, number: u16) -> Option<&mut NpcSkeleton> {
        self.bodies
            .iter_mut()
            .find(|(known, _)| *known == number)
            .map(|(_, body)| body)
    }

    /// `PM_FootSlopeTrace`'s feet on NPC `number`'s model at `origin` facing `yaw`, posed at
    /// the Ghoul2 clock ([`NpcSkeleton::foot_points`]); `None` without a humanoid model.
    pub(super) fn foot_points(
        &mut self,
        number: u16,
        yaw: f32,
        origin: [f32; 3],
        ghoul2_time: i32,
    ) -> Option<([f32; 3], [f32; 3])> {
        self.get(number)?
            .foot_points(yaw, origin, ghoul2_time)
            .ok()
            .flatten()
    }

    /// Forgets entity `number`'s model, when it had one: the entity was freed.
    pub(super) fn forget(&mut self, number: u16) {
        self.bodies.retain(|(known, _)| *known != number);
        self.vehicles.forget(number);
    }
}

impl ServerHost<'_> {
    /// `WP_SaberPositionUpdate`'s skeleton half for `npc`: its model built the first time
    /// (its own if humanoid, Kyle's if its model is missing, none for another skeleton),
    /// then posed — its spine only while a player might see it (`G_G2PlayerAngles`' PVS
    /// test over the clients in the game) — its blades read when it holds its lit saber.
    pub(super) fn pose_body(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        look_target: Option<[f32; 3]>,
        level_time: i32,
    ) -> NpcBlades {
        let none = [[None; sjk_game_jka::server_skeleton::MAX_BLADES]; 2];
        if npc.definition.rigid_model {
            return none;
        }
        if npc.vehicle.is_some() {
            self.pose_vehicle(npc, level_time);
            return none;
        }
        let Some(map) = self.map else { return none };
        if self.bodies.get(npc.number).is_none() {
            let wanted = npc_models(npc);
            let hilts = wanted.sabers.map(|sabers| {
                [
                    super::super::bridge_saber::HiltKey::of(&sabers[0]),
                    super::super::bridge_saber::HiltKey::of(&sabers[1]),
                ]
            });
            let model = String::from_utf8_lossy(wanted.model).into_owned();
            let Some(models) = super::super::bridge_saber::load_skeleton_models(
                self.models_cache,
                map,
                &model,
                hilts.as_ref(),
            ) else {
                return none;
            };
            self.bodies
                .bodies
                .push((npc.number, NpcSkeleton::new(models, npc)));
        }
        let origin = npc.player.origin();
        let seen = self
            .players
            .iter()
            .any(|player| self.in_pvs(player.origin, origin));
        let ghoul2_time = self.ghoul2_time;
        let Some(body) = self.bodies.get(npc.number) else {
            return none;
        };
        body.pose(
            npc,
            seen,
            look_target,
            sjk_game_jka::npc_saber::reads_blades(npc),
            level_time,
            ghoul2_time,
        )
        .unwrap_or_else(|error| {
            eprintln!("npc {}'s skeleton: {error}", npc.number);
            none
        })
    }

    /// `G_G2TraceCollide` on `npc`'s posed model for an NPC's blade: its box alone where it
    /// has no model.
    pub(super) fn collide_body(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
    ) -> Ghoul2Answer {
        let NpcBodies {
            bodies,
            scratch,
            records,
            ..
        } = &mut *self.bodies;
        let Some((_, body)) = bodies.iter_mut().find(|(known, _)| *known == npc.number) else {
            return Ghoul2Answer::NoModel;
        };
        body.collide(npc, start, end, radius, level_time, scratch, records)
            .unwrap_or_else(|error| {
                eprintln!("npc {}'s collision: {error}", npc.number);
                Ghoul2Answer::NoModel
            })
    }

    /// Whether an NPC with `playerModel` `model` (Kyle's where it is missing) has the bolt
    /// `G2API_AddBolt` is asked for.
    pub(super) fn body_model_has_bolt(&mut self, model: &[u8], name: &str) -> bool {
        if model.ends_with(b".md3") {
            return false;
        }
        let Some(map) = self.map else { return false };
        let model = String::from_utf8_lossy(model).into_owned();
        super::super::bridge_saber::load_skeleton_models(self.models_cache, map, &model, None)
            .is_some_and(|models| models.has_bolt(name))
    }

    /// `G_GetBoltPosition` on `npc`'s posed model, at the Ghoul2 clock: its origin where it
    /// has no model or no such bolt.
    pub(super) fn body_bolt(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        bolt: Option<&str>,
    ) -> [f32; 3] {
        let ghoul2_time = self.ghoul2_time;
        let Some(body) = self.bodies.get(npc.number) else {
            return npc.current_origin;
        };
        body.bolt(npc, bolt, ghoul2_time)
            .unwrap_or(npc.current_origin)
    }

    /// `G2API_GetBoltMatrix` of NPC `number`'s bolt whole, its model at `origin` facing `yaw`.
    pub(super) fn body_bolt_matrix(
        &mut self,
        number: u16,
        bolt: &str,
        yaw: f32,
        origin: [f32; 3],
    ) -> Option<[[f32; 4]; 3]> {
        let ghoul2_time = self.ghoul2_time;
        self.bodies
            .get(number)?
            .bolt_matrix(bolt, yaw, origin, ghoul2_time)
            .ok()
            .flatten()
    }

    /// [`Self::body_bolt_matrix`] with the model turned by all of `angles` (a machine's
    /// muzzle at its `r.currentAngles`).
    pub(super) fn body_bolt_matrix_turned(
        &mut self,
        number: u16,
        bolt: &str,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> Option<[[f32; 4]; 3]> {
        let ghoul2_time = self.ghoul2_time;
        self.bodies
            .get(number)?
            .bolt_matrix_turned(bolt, angles, origin, ghoul2_time)
            .ok()
            .flatten()
    }

    /// `G2API_GetSurfaceRenderStatus` on client `number`'s instance: an NPC's own; a
    /// player's surfaces the game never turns off, so every one of them is drawn.
    pub(super) fn body_surface_status(&mut self, number: u16, name: &str) -> i32 {
        self.bodies
            .get(number)
            .map_or(0, |body| body.surface_status(name))
    }

    /// `NPC_SetBoneAngles`' turn on NPC `number`'s instance.
    pub(super) fn body_set_bone_angles(
        &mut self,
        number: u16,
        bone: &str,
        angles: [f32; 3],
        level_time: i32,
    ) {
        if let Some(body) = self.bodies.get(number) {
            body.set_bone_angles(bone, angles, level_time);
        }
    }

    /// `G2API_SetSurfaceOnOff` on NPC `number`'s instance.
    pub(super) fn body_set_surface(&mut self, number: u16, name: &str, flags: u32) {
        if let Some(body) = self.bodies.get(number) {
            body.set_surface(name, flags);
        }
    }

    /// `gPainHitLoc`'s machine part of `npc` (`g_combat.c:5417-5432`): the part of the
    /// surface a blade or a missile struck on its model at `level_time`, if one did.
    pub(super) fn body_struck_part(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        level_time: i32,
    ) -> Option<i32> {
        self.bodies.get(npc.number)?.struck_part(npc, level_time)
    }

    /// `G_LocationBasedDamageModifier`'s surface half for an NPC's blade's blow on `npc`.
    pub(super) fn body_surface_location(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        flags: u32,
        spot: [f32; 3],
        level_time: i32,
    ) -> Option<HitLocation> {
        let ghoul2_time = self.ghoul2_time;
        self.bodies
            .get(npc.number)?
            .surface_location(npc, flags, spot, level_time, ghoul2_time)
    }
}

impl Npcs {
    /// A missile's trace through NPC `number`'s box, tested on its posed model
    /// ([`NpcSkeleton::trace_collide`]): `NoModel` for an entity that is no NPC with a
    /// model (its box stands), else where the model was struck, or a miss.
    pub(crate) fn missile_collide(
        &mut self,
        number: u16,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
    ) -> sjk_game_jka::entity_clip::ModelAnswer {
        use sjk_game_jka::entity_clip::ModelAnswer;
        let Self { roster, bodies, .. } = self;
        let Some(npc) = roster.actors.iter().find(|npc| npc.number == number) else {
            return ModelAnswer::NoModel;
        };
        let NpcBodies {
            bodies: list,
            scratch,
            records,
            ..
        } = bodies;
        let Some((_, body)) = list.iter_mut().find(|(known, _)| *known == number) else {
            return ModelAnswer::NoModel;
        };
        match body.trace_collide(npc, start, end, radius, level_time, scratch, records) {
            Ok(Some((position, normal, surface))) => ModelAnswer::Hit {
                position,
                normal,
                surface: surface as u32,
            },
            Ok(None) => ModelAnswer::Miss,
            Err(error) => {
                eprintln!("npc {number}'s collision: {error}");
                ModelAnswer::NoModel
            }
        }
    }

    /// A missile's run ended on NPC `number`'s model at `surface`: stamped as the surface
    /// last struck. Whether the NPC has a model.
    pub(crate) fn stamp_struck(&mut self, number: u16, surface: u32, level_time: i32) -> bool {
        self.bodies
            .get(number)
            .map(|body| body.stamp_surface(surface as usize, level_time))
            .is_some()
    }

    /// Whether a blade's hit on entity `number` bleeds (`WP_SaberDoHit`, `w_saber.c:3587-3670`:
    /// an NPC that is not a droid) rather than flares.
    pub(crate) fn bleeds(&self, number: u16) -> bool {
        self.roster.actors.iter().any(|npc| {
            npc.number == number && sjk_game_jka::npc_skeleton::bleeds(npc.definition.client_class)
        })
    }
}

/// `ps.eFlags2`, `hasLookTarget`, `lookTarget` (the player state's wire fields);
/// `EF2_HELD_BY_MONSTER`, `EF2_GENERIC_NPC_FLAG` (a rancor's victim is in its mouth).
const PS_EFLAGS2: usize = 103;
const PS_HAS_LOOK_TARGET: usize = 76;
const PS_LOOK_TARGET: usize = 66;
const EF2_HELD_BY_MONSTER: u32 = 1 << 0;
const EF2_GENERIC_NPC_FLAG: u32 = 1 << 3;
/// `CLASS_RANCOR`.
const CLASS_RANCOR: i32 = 54;

impl NativeGame {
    /// `G_HeldByMonster` (`g_active.c:1552-1587`), run by `ClientThink_real` for a player a
    /// monster holds (`g_active.c:2000-2003`): placed where its holder's hand or jaw is — a
    /// rancor's (`BG_AttachToRancor`, [`sjk_game_jka::npc_creature::attach_to_rancor`]) —
    /// facing along it, still; and whatever it asks to move, it does not.
    pub(crate) fn held_by_monster(&mut self, client: usize, command: &mut UserCommand) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let state = &peer.state;
        if state.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_HELD_BY_MONSTER == 0 || !peer.playing() {
            return;
        }
        let holder = (state.raw_field(PS_HAS_LOOK_TARGET).unwrap_or(0) != 0)
            .then(|| state.raw_field(PS_LOOK_TARGET).unwrap_or(0) as u16);
        let ghoul2_time = if self.previous_frame_time == 0 {
            self.last_frame_time
        } else {
            self.previous_frame_time
        };
        let placed = holder.and_then(|holder| {
            let npc =
                self.npcs.roster.actors.iter().find(|npc| {
                    npc.number == holder && npc.definition.client_class == CLASS_RANCOR
                })?;
            let in_mouth =
                npc.player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_GENERIC_NPC_FLAG != 0;
            let (yaw, origin) = (npc.mind.current_angles[1], npc.current_origin);
            let body = self.npcs.bodies.get(holder)?;
            let bolt = body
                .bolt_matrix(
                    sjk_game_jka::npc_creature::rancor_attach_bolt(in_mouth),
                    yaw,
                    origin,
                    ghoul2_time,
                )
                .ok()
                .flatten()?;
            Some(sjk_game_jka::npc_creature::attach_to_rancor(bolt, in_mouth))
        });
        if let Some(peer) = self.peer_mut(client) {
            if let Some((origin, view)) = placed {
                peer.state.set_origin(origin);
                peer.state.set_velocity([0.0; 3]);
                // `SetClientViewAngle` against the command being thought.
                let delta: [i32; 3] = std::array::from_fn(|axis| {
                    (((view[axis] * 65_536.0 / 360.0) as i32) & 65_535)
                        - i32::from(command.angles[axis])
                });
                peer.state.set_delta_angles(delta);
                peer.state.set_view_angles(view);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            // "don't allow movement, weapon switching, and most kinds of button presses"
        }
        command.forward_move = 0;
        command.right_move = 0;
        command.up_move = 0;
    }

    /// `G_G2TraceCollide` for a blade whose trace an NPC's box stopped: its posed model's
    /// answer, or `None` when `number` is no NPC. An NPC without a model is its box.
    pub(crate) fn npc_collide(
        &mut self,
        number: u16,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
    ) -> Option<Ghoul2Answer> {
        let Self {
            npcs, saber_work, ..
        } = self;
        let npc = npcs.roster.actors.iter().find(|npc| npc.number == number)?;
        let Some(body) = npcs.bodies.get(number) else {
            return Some(Ghoul2Answer::NoModel);
        };
        let (scratch, records) = saber_work.buffers();
        Some(
            body.collide(npc, start, end, radius, level_time, scratch, records)
                .unwrap_or_else(|error| {
                    eprintln!("npc {number}'s collision: {error}");
                    Ghoul2Answer::NoModel
                }),
        )
    }

    /// What NPC `number` is to player `swinger`'s blade (`CheckSaberDamage`,
    /// `w_saber.c:4530-4570`): a client that can be hurt while it lives or was not taken
    /// apart; spared an idle touch only by a power duel's teammate; untouchable while the
    /// swinger duels someone else; cut by a stab down only when it lies knocked down.
    pub(crate) fn npc_saber_victim(&self, number: u16, swinger: usize) -> Option<SaberVictim> {
        let npc = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)?;
        let player = self.peer(swinger)?;
        let legs = npc.player.leg_animation();
        let length = npc
            .movement
            .animation_lengths()
            .and_then(|lengths| lengths.length_ms(legs))
            .unwrap_or(0);
        Some(SaberVictim {
            client: true,
            takes_damage: npc.takes_damage,
            health: npc.health,
            disintegrated: npc.state.raw_field(ES_EFLAGS).unwrap_or(0)
                & sjk_game_jka::disruptor::EF_DISINTEGRATION
                != 0,
            spared_by_idle: self.gametype == GT_POWERDUEL && player.session.duel_team == 0,
            duel_elsewhere: player.state.duel_in_progress() && player.state.duel_index() != number,
            knocked_down_on_ground: sjk_game_jka::saber_rules::knocked_down_on_ground(
                legs,
                npc.player.legs_timer(),
                length,
            ),
            player_team: npc.player_team,
        })
    }

    /// `G_LocationBasedDamageModifier`'s surface half for a blade's blow on NPC `number`
    /// ([`NpcSkeleton::surface_location`]).
    pub(crate) fn npc_surface_location(
        &mut self,
        number: u16,
        flags: u32,
        spot: [f32; 3],
        level_time: i32,
    ) -> Option<HitLocation> {
        let ghoul2_time = if self.previous_frame_time == 0 {
            level_time
        } else {
            self.previous_frame_time
        };
        let npc = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == number)?;
        self.npcs
            .bodies
            .get(number)?
            .surface_location(npc, flags, spot, level_time, ghoul2_time)
    }
}

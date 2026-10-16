//! Legacy snapshot-to-world presentation adapter.
//!
//! Position/angle endpoints mirror `codemp/cgame/cg_ents.c`
//! `CG_InterpolateEntityPosition`: runtime consumers sample between the current
//! and next snapshot server times. `codemp/cgame/cg_snapshot.c` disables that
//! interpolation when `EF_TELEPORT_BIT` toggles; this adapter records the same
//! discontinuity for both entity state and the locally reconstructed player.

use crate::Ghoul2Restore;
use crate::actor_color::{TeamColorPolicy, legacy_body_color, legacy_player_color};
use crate::animation_selection::LegacyAnimationSelection;
use crate::npc_identity::{ET_NPC, legacy_npc_appearance, legacy_npc_equipment};
use crate::player_identity::LegacyPlayerIdentities;
use crate::presentation_equipment::*;
pub use sjk_game_jka::{
    legacy_evaluate_trajectory, legacy_evaluate_trajectory_angles, legacy_evaluate_trajectory_delta,
};
use sjk_protocol::{EntityState, GameState, Snapshot};
use sjk_runtime::{
    Appearance, EntityId, EntityKind, HeldEquipment, ItemState, MotionSample, Transform, World,
    WorldId,
};
use std::collections::{BTreeMap, BTreeSet};

#[path = "remote_motion.rs"]
mod remote_motion;

const EF_TELEPORT_BIT: u32 = 1 << 3;
const EF_DEAD: u32 = 1 << 1;
/// `PW_CLOAKED` (`bg_public.h` powerup enum).
const PW_CLOAKED: usize = 11;
const ET_GENERAL: u8 = 0; // codemp/game/bg_public.h
const ET_MOVER: u8 = 6; // codemp/game/bg_public.h:1251
const EF_NODRAW: u32 = 1 << 8; // codemp/game/bg_public.h:645
const SOLID_BMODEL: u32 = 0x00ff_ffff; // codemp/game/q_shared.h:371

/// Exact per-frame presentation state for one codemp inline BSP mover.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyMoverPresentation {
    pub entity_number: u16,
    pub model_index: usize,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub rotation: [f32; 4],
    pub visible: bool,
}

/// Present one `ET_MOVER` exactly as `CG_CalcEntityLerpPositions` does.
///
/// Unlike ordinary snapshot entities, codemp evaluates both `pos` and `apos`
/// with `BG_EvaluateTrajectory` at the current `cg.time`
/// (`codemp/cgame/cg_ents.c:3128-3131`). It does not linearly interpolate the
/// two evaluated snapshot endpoints; that distinction is observable for sine
/// and stopped-linear mover trajectories.
pub fn legacy_present_mover(state: &EntityState, at_time: i32) -> Option<LegacyMoverPresentation> {
    if state.entity_type() != ET_MOVER {
        return None;
    }
    let model_index = usize::try_from(state.model_index()).ok()?;
    if model_index == 0 {
        return None;
    }
    let origin = legacy_evaluate_trajectory(
        state.trajectory_base(),
        state.trajectory_delta(),
        state.trajectory_type(),
        state.trajectory_time(),
        state.trajectory_duration(),
        at_time,
    );
    let angles = legacy_evaluate_trajectory_angles(
        state.angular_trajectory_base(),
        state.angular_trajectory_delta(),
        state.angular_trajectory_type(),
        state.angular_trajectory_time(),
        state.angular_trajectory_duration(),
        at_time,
    );
    Some(LegacyMoverPresentation {
        entity_number: state.number(),
        model_index,
        origin,
        angles,
        rotation: legacy_angles_to_quaternion(angles),
        visible: state.solid() == SOLID_BMODEL && state.e_flags() & EF_NODRAW == 0,
    })
}

/// Translates the fixed legacy snapshot model into an engine-owned world.
///
/// Network entity numbers remain private to this adapter. The world itself has
/// no protocol-size limit and may coexist with other local or remote worlds.
pub struct LegacyWorldAdapter {
    world_id: WorldId,
    active_entities: BTreeSet<EntityId>,
    entity_flags: BTreeMap<EntityId, u32>,
    player_identities: LegacyPlayerIdentities,
    smooth_clients: bool,
}

impl LegacyWorldAdapter {
    pub fn new(world_id: WorldId) -> Self {
        Self {
            world_id,
            active_entities: BTreeSet::new(),
            entity_flags: BTreeMap::new(),
            player_identities: LegacyPlayerIdentities::default(),
            smooth_clients: false,
        }
    }

    pub fn apply_snapshot(
        &mut self,
        snapshot: &Snapshot,
        game_state: &GameState,
        world: &mut World,
    ) -> bool {
        if world.id() != self.world_id {
            return false;
        }

        let mut next_entities = BTreeSet::new();
        let mut next_entity_flags = BTreeMap::new();
        let mut next_corpses = BTreeSet::new();
        let color_policy = TeamColorPolicy::from_game_state(game_state);
        let intermission = snapshot.player.movement_type() == crate::PM_INTERMISSION;
        for state in crate::legacy_scene_entities(game_state, snapshot) {
            // codemp CG_Player: hide players, corpses and vehicles at match end;
            // ordinary NPCs remain available for scripted intermission scenes.
            if intermission
                && matches!(state.entity_type(), 1 | 13 | 15)
                && !(state.entity_type() == ET_NPC && state.npc_class() != 53)
            {
                continue;
            }
            if !legacy_entity_visible(state.entity_type(), state.e_flags()) {
                continue;
            }
            let id = EntityId::new(u64::from(state.number()) + 1);
            let mut angles = legacy_evaluate_trajectory(
                state.angular_trajectory_base(),
                state.angular_trajectory_delta(),
                state.angular_trajectory_type(),
                state.angular_trajectory_time(),
                state.angular_trajectory_duration(),
                snapshot.server_time,
            );
            let pose_angles = angles;
            if matches!(state.entity_type(), 2 | 5) {
                // CG_Item uses entityState::angles for placed and dropped
                // items; apos is a separate trajectory used by movers and
                // ordinary render entities.
                angles = state.angles();
            }
            if matches!(state.entity_type(), 1 | 13 | 15) {
                // Player/NPC entity pitch is a view angle. JKA keeps the model
                // root upright and distributes look pitch through spine bone
                // overrides; pitching the complete Ghoul2 model tips feet and
                // attached weapons around the entity origin.
                angles[0] = 0.0;
                angles[2] = 0.0;
            }
            let transform = Transform {
                translation: remote_motion::translation(
                    state,
                    snapshot.player.client_num(),
                    snapshot.server_time,
                    self.smooth_clients,
                ),
                rotation: placed_charge_rotation(state, legacy_angles_to_quaternion(angles)),
                scale: legacy_general_scale(state),
            };
            let sample = MotionSample {
                time_millis: i64::from(snapshot.server_time),
                transform,
            };
            let teleport = self
                .entity_flags
                .get(&id)
                .is_some_and(|previous| (previous ^ state.e_flags()) & EF_TELEPORT_BIT != 0);
            if teleport {
                world.upsert_discontinuous(id, legacy_entity_kind(state.entity_type()), sample);
            } else {
                world.upsert(id, legacy_entity_kind(state.entity_type()), sample);
            }
            next_entity_flags.insert(id, state.e_flags());
            world.set_item(
                id,
                (state.entity_type() == 2).then(|| legacy_item_state(state)),
            );
            if matches!(state.entity_type(), 1 | 13 | 15) {
                let animation_time = i64::from(snapshot.server_time);
                let appearance = if state.entity_type() == 15 {
                    next_corpses.insert(id);
                    self.player_identities
                        .corpse(game_state, id, state.client_num())
                } else if state.entity_type() == ET_NPC {
                    legacy_npc_appearance(game_state, state)
                } else {
                    self.player_identities.live(game_state, state.client_num())
                };
                world.set_appearance(id, appearance);
                world.set_color(
                    id,
                    if state.entity_type() == 15 {
                        legacy_body_color(state.custom_rgba())
                    } else {
                        legacy_player_color(
                            game_state,
                            color_policy,
                            state.client_num(),
                            state.custom_rgba(),
                        )
                    },
                );
                LegacyAnimationSelection::from_entity(state).apply(world, id, animation_time);
                world.set_pose(
                    id,
                    Some(crate::player_angle_rules::entity_pose(state, pose_angles)),
                );
                let alive = state.entity_type() != 15 && state.e_flags() & EF_DEAD == 0;
                let equipment = if state.entity_type() == ET_NPC {
                    legacy_npc_equipment(state, alive)
                } else {
                    legacy_equipment(
                        game_state,
                        state.client_num(),
                        state.weapon(),
                        state.saber_holstered(),
                        state.saber_move(),
                        state.saber_in_flight(),
                        alive,
                    )
                };
                world.set_equipment(id, equipment);
                world.set_ground_shadow(
                    id,
                    state.entity_type() == 1
                        && legacy_ground_shadow(
                            state.e_flags(),
                            state.powerups(),
                            state.vehicle_entity_num(),
                            state.npc_class(),
                        )
                        && !crate::legacy_mind_tricked(
                            crate::legacy_entity_trick_targets(state),
                            snapshot.player.client_num(),
                            snapshot.player.force_powers_active(),
                        ),
                );
            } else {
                world.set_appearance(
                    id,
                    if state.entity_type() == 2 {
                        legacy_item_appearance(state.model_index())
                    } else {
                        crate::entity_models::legacy_entity_model_appearance(game_state, state)
                    },
                );
            }
            next_entities.insert(id);
        }

        let local_id = EntityId::new(u64::from(snapshot.player.client_num()) + 1);
        // A spectator player state describes the free camera, not a humanoid.
        // Its animation fields are commonly zero, so rendering it creates a
        // floating T-pose that follows the view. The difference pass below also
        // removes a previously active local actor after entering spectator mode.
        if !intermission
            && !snapshot.player.is_spectator()
            && snapshot.player.entity_flags() & EF_NODRAW == 0
        {
            let mut local_angles = snapshot.player.view_angles();
            local_angles[0] = 0.0;
            local_angles[2] = 0.0;
            let sample = MotionSample {
                time_millis: i64::from(snapshot.server_time),
                transform: Transform {
                    translation: snapshot.player.origin(),
                    rotation: legacy_angles_to_quaternion(local_angles),
                    scale: [1.0; 3],
                },
            };
            let teleport = self.entity_flags.get(&local_id).is_some_and(|previous| {
                (previous ^ snapshot.player.entity_flags()) & EF_TELEPORT_BIT != 0
            });
            if teleport {
                world.upsert_discontinuous(local_id, EntityKind::Actor, sample);
            } else {
                world.upsert(local_id, EntityKind::Actor, sample);
            }
            next_entity_flags.insert(local_id, snapshot.player.entity_flags());
            world.set_appearance(
                local_id,
                self.player_identities
                    .live(game_state, snapshot.player.client_num()),
            );
            world.set_color(
                local_id,
                legacy_player_color(
                    game_state,
                    color_policy,
                    snapshot.player.client_num(),
                    snapshot.player.custom_rgba(),
                ),
            );
            let animation_time = i64::from(snapshot.server_time);
            LegacyAnimationSelection::from_player(&snapshot.player).apply(
                world,
                local_id,
                animation_time,
            );
            world.set_pose(
                local_id,
                Some(crate::player_angle_rules::player_pose(&snapshot.player)),
            );
            world.set_equipment(
                local_id,
                legacy_equipment(
                    game_state,
                    snapshot.player.client_num(),
                    snapshot.player.weapon(),
                    snapshot.player.saber_holstered(),
                    snapshot.player.saber_move(),
                    snapshot.player.saber_in_flight(),
                    snapshot.player.entity_flags() & EF_DEAD == 0,
                ),
            );
            let cloaked = u32::from(snapshot.player.powerups[PW_CLOAKED] != 0) << PW_CLOAKED;
            world.set_ground_shadow(
                local_id,
                legacy_ground_shadow(
                    snapshot.player.entity_flags(),
                    cloaked,
                    snapshot.player.vehicle_entity_num(),
                    0,
                ),
            );
            next_entities.insert(local_id);
        }

        for removed in self.active_entities.difference(&next_entities) {
            world.remove(*removed);
        }
        self.player_identities.retain(&next_corpses);
        self.active_entities = next_entities;
        self.entity_flags = next_entity_flags;
        true
    }

    /// Apply the identity-visible part of `CG_RestoreClientGhoul_f`.
    ///
    /// Live appearances already resolve `CS_PLAYERS` every snapshot, and SJK
    /// has no detachable-limb, ragdoll, gore, or mutable Ghoul2 weapon cache.
    /// The `ircg` branch does have observable identity state: codemp calls
    /// `CG_BodyQueueCopy` (`cg_servercmds.c:1438-1455`), so capture the source
    /// client's current appearance for that body slot immediately.
    pub fn restore_client_ghoul(&mut self, game_state: &GameState, restore: Ghoul2Restore) {
        let Some(copy) = restore.immediate_body else {
            return;
        };
        let Ok(body_entity) = u64::try_from(copy.body_entity) else {
            return;
        };
        self.player_identities.copy_client_to_body(
            game_state,
            u16::from(restore.client_num),
            EntityId::new(body_entity + 1),
        );
    }

    /// Retain an `ircg` identity through PVS absence until replacement or `kg2`.
    pub fn copy_body_identity(&mut self, body: &crate::BodyIdentity) {
        self.player_identities.copy_body(body);
    }

    /// Release a body slot when cgame receives `kg2`.
    pub fn kill_body_identity(&mut self, entity_num: u16) {
        self.player_identities.kill_body(entity_num);
    }
}

/// `CG_PlayerShadow` (`cg_players.c:4649-4693`) skips the drop shadow for
/// cloaked, dead, and vehicle-riding players (`NPC_class != CLASS_VEHICLE`).
/// A player that mind-tricks the viewer is excluded by the caller.
fn legacy_ground_shadow(flags: u32, powerups: u32, vehicle: u16, npc_class: u8) -> bool {
    const CLASS_VEHICLE: u8 = 53;
    flags & EF_DEAD == 0
        && powerups & (1 << PW_CLOAKED) == 0
        && (vehicle == 0 || npc_class == CLASS_VEHICLE)
}

fn legacy_entity_visible(entity_type: u8, flags: u32) -> bool {
    const EF_DEAD: u32 = 1 << 1;
    const EF_NODRAW: u32 = 1 << 8;
    const EF_ITEMPLACEHOLDER: u32 = 1 << 23;
    // Event carriers (eType >= ET_EVENTS) are never added as entities
    // (cg_ents.c:3288-3291 CG_AddCEntity); they only feed event observers.
    const ET_EVENTS: u8 = 18;
    entity_type < ET_EVENTS
        && (flags & EF_NODRAW == 0
            || (entity_type == 2 && flags & EF_ITEMPLACEHOLDER != 0 && flags & EF_DEAD == 0))
}

fn legacy_item_state(state: &sjk_protocol::EntityState) -> ItemState {
    let index = state.model_index();
    let weapon = matches!(index, 19..=31 | 35..=39);
    let dropped = state.e_flags() & (1 << 25) != 0;
    let vertical_offset = if dropped && weapon {
        match index {
            25 | 28 | 35 => -12,
            26 => -13,
            27 | 36 | 37 => -16,
            29 => -10,
            30 => -6,
            31 => -11,
            _ => -8,
        }
    } else {
        match index {
            2 => 7,
            3 | 5 | 8 => 2,
            4 => 5,
            _ => 0,
        }
    };
    ItemState {
        dropped,
        weapon,
        powerup: matches!(index, 15..=18),
        vertical_offset,
    }
}

pub fn legacy_model_appearance(game_state: &GameState, model_index: i16) -> Option<Appearance> {
    const CS_MODELS: usize = 298;
    let index = usize::try_from(model_index).ok()?;
    if index == 0 {
        return None;
    }
    let model = std::str::from_utf8(game_state.config_string(CS_MODELS + index)?).ok()?;
    if model.is_empty() {
        return None;
    }
    Some(Appearance {
        model: model.to_owned(),
        variant: String::new(),
    })
}

pub fn legacy_item_appearance(item_index: i16) -> Option<Appearance> {
    let model = match item_index {
        1 => "models/map_objects/mp/psd_sm.md3",
        2 => "models/map_objects/mp/psd.md3",
        3 => "models/map_objects/mp/medpac.md3",
        4 => "models/items/remote.md3",
        5 => "models/map_objects/mp/shield.md3",
        6 | 11 | 12 => "models/map_objects/mp/bacta.md3",
        7 => "models/items/big_bacta.md3",
        8 => "models/items/binoculars.md3",
        9 | 10 | 14 => "models/items/psgun.glm",
        13 => "models/map_objects/hoth/eweb_model.glm",
        15 => "models/map_objects/mp/jedi_enlightenment.md3",
        16 => "models/map_objects/mp/dk_enlightenment.md3",
        17 => "models/map_objects/mp/force_boon.md3",
        18 => "models/map_objects/mp/ysalimari.md3",
        19 | 20 => "models/weapons2/stun_baton/baton_w.glm",
        21 => "models/weapons2/saber/saber_w.glm",
        22 => "models/weapons2/blaster_pistol/blaster_pistol_w.glm",
        23 => "models/weapons2/concussion/c_rifle_w.glm",
        24 => "models/weapons2/briar_pistol/briar_pistol_w.glm",
        25 | 38 | 39 => "models/weapons2/blaster_r/blaster_w.glm",
        26 => "models/weapons2/disruptor/disruptor_w.glm",
        27 => "models/weapons2/bowcaster/bowcaster_w.glm",
        28 => "models/weapons2/heavy_repeater/heavy_repeater_w.glm",
        29 => "models/weapons2/demp2/demp2_w.glm",
        30 => "models/weapons2/golan_arms/golan_arms_w.glm",
        31 => "models/weapons2/merr_sonn/merr_sonn_w.glm",
        32 => "models/weapons2/thermal/thermal_pu.md3",
        33 => "models/weapons2/laser_trap/laser_trap_pu.md3",
        34 => "models/weapons2/detpack/det_pack_pu.md3",
        // CG_RegisterItemVisuals selects world_model[1] for the three
        // throwable weapon pickups (codemp/cgame/cg_weapons.c:76-80).
        35 => "models/weapons2/thermal/thermal_pu.md3",
        36 => "models/weapons2/laser_trap/laser_trap_pu.md3",
        37 => "models/weapons2/detpack/det_pack_pu.md3",
        40 | 41 => "models/items/energy_cell.md3",
        42 => "models/items/power_cell.md3",
        43 => "models/items/metallic_bolts.md3",
        44 => "models/items/rockets.md3",
        45 => "models/items/battery.md3",
        46 => "models/flags/r_flag.md3",
        47 => "models/flags/b_flag.md3",
        48 => "models/flags/n_flag.md3",
        49 => "models/powerups/orb/r_orb.md3",
        50 => "models/powerups/orb/b_orb.md3",
        _ => return None,
    };
    Some(Appearance {
        model: model.to_owned(),
        variant: String::new(),
    })
}

/// Rewrite the snapshot-derived held equipment with predicted player fields.
///
/// Blade colors come from the configstrings and stay authoritative; kind,
/// blade activity and trail length follow the predicted `playerState_t`
/// fields the way `BG_PlayerStateToEntityState` (`game/bg_misc.c:2762-2830`)
/// copies them for the local entity. Returns `None` when there is no
/// authoritative equipment to base the presentation on.
pub fn legacy_predicted_equipment(
    authoritative: Option<HeldEquipment>,
    weapon: u8,
    saber_holstered: u8,
    saber_move: u32,
    alive: bool,
) -> Option<HeldEquipment> {
    let mut equipment = authoritative?;
    let kind = legacy_held_item_kind(weapon)?;
    equipment.kind = kind;
    equipment.weapon = weapon;
    equipment.active = equipment_active(kind, saber_holstered, alive);
    equipment.secondary_active = secondary_equipment_active(kind, saber_holstered, alive);
    equipment.trail_duration_millis = trail_duration(kind, saber_move);
    Some(equipment)
}

/// The model scale `CG_General` gives an `ET_GENERAL` entity (`cg_ents.c`): a nonzero
/// `iModelScale` is a percentage applied to all three axes, as a server's spawned or
/// placed prop (`modelscale`) sends it. Players, NPCs and bodies are scaled where their
/// actors are drawn, and a mover's brush model is never scaled.
fn legacy_general_scale(state: &EntityState) -> [f32; 3] {
    let percent = state.model_scale_percent();
    if state.entity_type() == ET_GENERAL && percent != 0 {
        [percent as f32 / 100.0; 3]
    } else {
        [1.0; 3]
    }
}

fn legacy_entity_kind(entity_type: u8) -> EntityKind {
    match entity_type {
        1 | 13 => EntityKind::Actor,
        15 => EntityKind::Corpse,
        2 | 5 => EntityKind::Item,
        3 => EntityKind::Projectile,
        7 => EntityKind::Other,
        6 => EntityKind::Mover,
        // ET_FX (17, bg_public.h:1262) is a persistent effect runner;
        // ET_TERRAIN (16) has no client representation. ET_EVENTS (18+) are
        // one-shot carriers rejected before this point.
        17 => EntityKind::Effect,
        _ => EntityKind::Other,
    }
}

/// A trip mine or det pack (`ET_GENERAL`, a Ghoul2 `*_w.glm`) faces along its
/// angles, which the game sets to the surface it sticks to (`laserTrapStick`,
/// `g_weapon.c`: `vectoangles` of the plane normal). The result is `rotation` turned
/// a quarter about the entity's own Z first, so the model lies flat against the wall
/// or floor, facing out, with the beam along the same facing.
///
/// Evidence: none for the quarter turn itself. It matches EternalJK's look (the
/// charge seen flat against the surface), and no source was found for it. EternalJK
/// draws the entity from `AnglesToAxis(cent->lerpAngles)` alone (`cg_ents.c:1544`,
/// `CG_SetGhoul2Info` `cg_ents.c:565-572`) and its Ghoul2 renderer builds the world
/// matrix from the same angles with no extra rotation (`rd-vanilla/G2_misc.cpp:1608-1664`),
/// so the offset can only come from the retail `.glm`'s rest pose or from how
/// this client orients Ghoul2 models; it should be re-derived from the model data.
fn placed_charge_rotation(state: &EntityState, rotation: [f32; 4]) -> [f32; 4] {
    const WP_TRIP_MINE: u8 = 13;
    const WP_DET_PACK: u8 = 14;
    if state.entity_type() != 0
        || state.model_ghoul2() != 1
        || !matches!(state.weapon(), WP_TRIP_MINE | WP_DET_PACK)
    {
        return rotation;
    }
    // `rotation * (90 degrees about Z)`, quaternions as [x, y, z, w].
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let [x, y, z, w] = rotation;
    [
        (x + y) * half,
        (y - x) * half,
        (z + w) * half,
        (w - z) * half,
    ]
}

/// Convert codemp pitch/yaw/roll Euler degrees into the runtime quaternion
/// used by the presentation boundary.
pub fn legacy_angles_to_quaternion(angles: [f32; 3]) -> [f32; 4] {
    let [pitch, yaw, roll] = angles.map(|angle| angle.to_radians() * 0.5);
    let (pitch_sine, pitch_cosine) = pitch.sin_cos();
    let (yaw_sine, yaw_cosine) = yaw.sin_cos();
    let (roll_sine, roll_cosine) = roll.sin_cos();
    [
        roll_sine * pitch_cosine * yaw_cosine - roll_cosine * pitch_sine * yaw_sine,
        roll_cosine * pitch_sine * yaw_cosine + roll_sine * pitch_cosine * yaw_sine,
        roll_cosine * pitch_cosine * yaw_sine - roll_sine * pitch_sine * yaw_cosine,
        roll_cosine * pitch_cosine * yaw_cosine + roll_sine * pitch_sine * yaw_sine,
    ]
}

#[cfg(test)]
mod general_scale_tests {
    use super::*;
    use sjk_protocol::LEGACY_ENTITY_FIELDS;

    fn entity(entity_type: u8, percent: u32) -> EntityState {
        let mut state = EntityState::zero(100, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(8, u32::from(entity_type));
        state.set_raw_field(76, percent);
        state
    }

    #[test]
    fn general_models_take_the_servers_model_scale() {
        assert_eq!(legacy_general_scale(&entity(ET_GENERAL, 250)), [2.5; 3]);
        assert_eq!(legacy_general_scale(&entity(ET_GENERAL, 50)), [0.5; 3]);
        // Zero is "no custom scale" (CG_General clears modelScale).
        assert_eq!(legacy_general_scale(&entity(ET_GENERAL, 0)), [1.0; 3]);
        // Movers, players and the rest are scaled elsewhere or not at all.
        assert_eq!(legacy_general_scale(&entity(ET_MOVER, 250)), [1.0; 3]);
        assert_eq!(legacy_general_scale(&entity(1, 250)), [1.0; 3]);
    }
}

#[cfg(test)]
mod placed_charge_tests {
    use super::*;
    use sjk_protocol::LEGACY_ENTITY_FIELDS;

    fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        let [x, y, z, w] = q;
        // v + 2w(u x v) + 2(u x (u x v)), u = (x, y, z).
        let u = [x, y, z];
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let t = cross(u, v).map(|c| 2.0 * c);
        let ut = cross(u, t);
        std::array::from_fn(|i| v[i] + w * t[i] + ut[i])
    }

    #[test]
    fn a_stuck_mine_faces_out_of_its_surface() {
        let mut mine = EntityState::zero(100, &LEGACY_ENTITY_FIELDS);
        mine.set_raw_field(14, 13);
        mine.set_raw_field(54, 1);
        // The model's top is -Y; on a floor (`vectoangles` of +Z is pitch -90) it
        // points up, on a wall facing +X it points along +X.
        for (angles, out) in [
            ([-90.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ] {
            let q = placed_charge_rotation(&mine, legacy_angles_to_quaternion(angles));
            let top = rotate(q, [0.0, -1.0, 0.0]);
            for axis in 0..3 {
                assert!(
                    (top[axis] - out[axis]).abs() < 1e-4,
                    "{angles:?} -> {top:?}"
                );
            }
        }
        // Other entities keep their rotation.
        let other = EntityState::zero(100, &LEGACY_ENTITY_FIELDS);
        let q = legacy_angles_to_quaternion([10.0, 20.0, 0.0]);
        assert_eq!(placed_charge_rotation(&other, q), q);
    }
}

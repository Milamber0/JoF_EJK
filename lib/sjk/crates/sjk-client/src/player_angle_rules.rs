//! Exact codemp animation predicates and snapshot-to-pose angle policy.

use sjk_protocol::{EntityState, PlayerState};
use sjk_runtime::{EntityId, PoseAngleState, PoseState};

use sjk_game_jka::player_angle_flags as flags;

fn animation_flags(clip: usize) -> u8 {
    flags::FLAGS.get(clip).copied().unwrap_or(0)
}

pub(super) fn special_move(saber_move: u32) -> bool {
    flags::SPECIAL_MOVES
        .get(saber_move as usize)
        .copied()
        .unwrap_or(false)
}

/// The ci track exclusions in bg_pmove.c:8904-8915 (roll2 alone is not a ci exclusion).
pub(super) fn correction_tracks(legs: usize, torso: usize) -> bool {
    let lower = animation_flags(legs);
    let upper = animation_flags(torso);
    legs != torso && lower & 63 == 0 && upper & 62 == 0
}

fn rules(legs: usize, torso: usize, weapon: u8, saber: u32, dead: bool) -> PoseAngleState {
    // bg_misc.c:245-269 WeaponReadyAnim, used by bg_pmove.c:9153-9163.
    let ready = match weapon {
        0 => "TORSO_DROPWEAP1",
        3 => "BOTH_STAND2",
        4 | 16 => "TORSO_WEAPONREADY2",
        12..=14 => "TORSO_WEAPONREADY10",
        17 => "BOTH_STAND1",
        18 => "TORSO_WEAPONREADY1",
        _ => "TORSO_WEAPONREADY3",
    };
    PoseAngleState {
        center_swing: crate::legacy_animation_name(legs) != Some("BOTH_STAND1")
            || crate::legacy_animation_name(torso) != Some(ready),
        // bg_pmove.c:8889-8923; current rolls already belong to the knockdown/roll2 set.
        correct_animation_motion: correction_tracks(legs, torso)
            && animation_flags(legs) & 64 == 0
            && !special_move(saber)
            && !dead,
        ..Default::default()
    }
}

pub(super) fn resolved(fields: PredictedPoseFields, force_frame: u16) -> PoseState {
    let legs = usize::from(fields.legs_animation);
    let torso = usize::from(fields.torso_animation);
    let dead = fields.entity_flags & 2 != 0;
    let mut angle = rules(legs, torso, fields.weapon, fields.saber_move, dead);
    angle.correct_animation_motion &= fields.vehicle_entity_num == 0;
    PoseState {
        view_angles_degrees: fields.view_angles_degrees,
        velocity: fields.velocity,
        // bg_pmove.c:9166-9174. Invalid live directions are retained for diagnostics;
        // no direction-indexed table is accessed by this adapter.
        movement_direction: if dead { 0 } else { fields.movement_direction },
        grounded: fields.ground_entity_num != 1_023,
        // bg_pmove.c:9185-9193,9067-9087: legs roll2 only; special *move* with saber.
        suppress_velocity_facing: animation_flags(legs) & 64 != 0
            || (fields.weapon == 3 && special_move(fields.saber_move)),
        // bg_pmove.c:9119. Death continues through BG when CG_RagDoll returns false.
        lock_root_angles: force_frame != 0
            || fields.vehicle_entity_num != 0
            || (animation_flags(legs) | animation_flags(torso)) & 128 != 0
            || fields.weapon == 17, // existing emplaced fallback; special pose is deferred
        angle,
    }
}

pub(super) fn entity_pose(state: &EntityState, view_angles: [f32; 3]) -> PoseState {
    // EternalJK remaps the animations before `CG_G2PlayerAngles` reads them.
    let (legs_animation, torso_animation) = crate::animation_selection::ejk_animation_fixes(
        state.leg_animation(),
        state.torso_animation(),
        state.weapon(),
        state.saber_in_flight(),
        state.entity_type() == crate::npc_identity::ET_NPC,
    );
    let mut pose = resolved(
        PredictedPoseFields {
            view_angles_degrees: view_angles,
            velocity: state.trajectory_delta(),
            movement_direction: state.movement_direction(),
            ground_entity_num: state.ground_entity_num(),
            legs_animation,
            torso_animation,
            vehicle_entity_num: state.vehicle_entity_num(),
            weapon: state.weapon(),
            entity_flags: state.e_flags(),
            saber_move: state.saber_move(),
        },
        state.force_frame(),
    );
    // Existing decoded netfields; no changes to the frozen wire adapter.
    // entity.rs field table: heldByClient=99, hasLookTarget=66, lookTarget=52.
    pose.angle.hold_view_yaw = state.raw_field(99).unwrap_or(0) != 0;
    pose.angle.preserve_overrides = state.number() >= 32;
    pose.angle.look_target = (state.raw_field(66).unwrap_or(0) != 0)
        .then(|| EntityId::new(u64::from(state.raw_field(52).unwrap_or(0)) + 1));
    pose
}

pub(super) fn player_pose(state: &PlayerState) -> PoseState {
    let (legs_animation, torso_animation) = crate::animation_selection::ejk_animation_fixes(
        state.leg_animation(),
        state.torso_animation(),
        state.weapon(),
        state.saber_in_flight(),
        false,
    );
    let mut pose = resolved(
        PredictedPoseFields {
            view_angles_degrees: state.view_angles(),
            velocity: state.velocity(),
            movement_direction: state.movement_direction(),
            ground_entity_num: state.ground_entity_num(),
            legs_animation,
            torso_animation,
            vehicle_entity_num: state.vehicle_entity_num(),
            weapon: state.weapon(),
            entity_flags: state.entity_flags(),
            saber_move: state.saber_move(),
        },
        state.raw_field(108).unwrap_or(0) as u16,
    );
    // bg_misc.c:2794 copies saberLockFrame to forceFrame. Read the schema slot
    // (msg.cpp:1424, snapshot.rs:135); the unrelated convenience accessor uses 119.
    // snapshot.rs mapping: heldByClient storage 121, hasLookTarget 76, lookTarget 66.
    pose.angle.hold_view_yaw = state.raw_field(121).unwrap_or(0) != 0;
    pose.angle.look_target = (state.raw_field(76).unwrap_or(0) != 0)
        .then(|| EntityId::new(u64::from(state.raw_field(66).unwrap_or(0)) + 1));
    pose
}

/// Predicted fields consumed by the angle adapter; prediction and selection stay unchanged.
#[derive(Clone, Copy, Debug)]
pub struct PredictedPoseFields {
    /// Predicted camera angles, in degrees.
    pub view_angles_degrees: [f32; 3],
    /// Predicted velocity.
    pub velocity: [f32; 3],
    /// Legacy supporting entity number.
    pub ground_entity_num: u16,
    /// Legacy movement-direction sector.
    pub movement_direction: i8,
    /// Declared lower animation index.
    pub legs_animation: u16,
    /// Declared upper animation index.
    pub torso_animation: u16,
    /// Legacy vehicle association.
    pub vehicle_entity_num: u16,
    /// Legacy weapon ID.
    pub weapon: u8,
    /// Legacy entity flags.
    pub entity_flags: u32,
    /// Declared saber move for bg_pmove.c:9190 (independent of animation selection).
    pub saber_move: u32,
}

/// Use the predicted local view/velocity, as cg_ents.c:3454-3476 rebuilds the local entity.
pub fn legacy_predicted_pose(
    authoritative: Option<PoseState>,
    predicted: PredictedPoseFields,
) -> Option<PoseState> {
    let authoritative = authoritative?;
    let mut pose = resolved(predicted, 0);
    // Force-frame/vehicle state remains server-owned during local pose prediction.
    // BG_PlayerStateToEntityState copies these alongside the predicted view (2794).
    pose.lock_root_angles |= authoritative.lock_root_angles;
    pose.angle.hold_view_yaw = authoritative.angle.hold_view_yaw;
    pose.angle.look_target = authoritative.angle.look_target;
    Some(pose)
}

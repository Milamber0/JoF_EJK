//! Predicted local-player animation and held-equipment presentation.
//!
//! OpenJK replaces the local `centity_t` state from
//! `cg.predictedPlayerState` before `CG_AddCEntity`
//! (`codemp/cgame/cg_ents.c:3457`) through
//! `BG_PlayerStateToEntityState` (`codemp/game/bg_misc.c:2762-2830`).

use sjk_client::pmove::MovementState;
use sjk_runtime::{
    AnimationState, AnimationTrackInput, AnimationTrackState, AnimationTrackTransition, EntityId,
    HeldEquipment, PoseState,
};

/// Persistent no-allocation clock state for the predicted local actor.
#[derive(Default)]
pub(crate) struct Tracker {
    entity: Option<EntityId>,
    animation: Option<AnimationState>,
}

impl Tracker {
    /// Resolve the animation presented for this frame.
    ///
    /// An ordinal or flip revision starts at the predicted command time;
    /// unchanged input preserves its running clock. Deferred saber branches
    /// return `None`, selecting the authoritative runtime-world animation.
    pub(crate) fn resolve(
        &mut self,
        entity: EntityId,
        predicted: Option<&MovementState>,
        authoritative: Option<AnimationState>,
        authoritative_saber_move: Option<u32>,
    ) -> Option<AnimationState> {
        let Some(predicted) =
            predicted.filter(|state| prediction_is_presentable(state, authoritative_saber_move))
        else {
            self.clear();
            return None;
        };
        if self.entity != Some(entity) {
            self.entity = Some(entity);
            self.animation = authoritative;
        }
        let previous = self.animation.or(authoritative);
        let command_time = i64::from(predicted.command_time);
        let [lower, upper] =
            sjk_client::legacy_predicted_animation_inputs(predicted, previous, command_time);
        let animation = AnimationState {
            lower: update_track(previous.map(|state| state.lower), lower, command_time),
            upper: update_track(previous.map(|state| state.upper), upper, command_time),
        };
        self.animation = Some(animation);
        Some(animation)
    }

    /// Whether the predicted animation is currently presented.
    pub(crate) fn is_active(&self) -> bool {
        self.animation.is_some()
    }

    fn clear(&mut self) {
        self.entity = None;
        self.animation = None;
    }
}

/// Replace snapshot saber/weapon presentation fields for the local actor.
pub(crate) fn equipment(
    authoritative: Option<HeldEquipment>,
    predicted: Option<&MovementState>,
    authoritative_saber_move: Option<u32>,
) -> Option<HeldEquipment> {
    let Some(predicted) =
        predicted.filter(|state| prediction_is_presentable(state, authoritative_saber_move))
    else {
        return authoritative;
    };
    sjk_client::legacy_predicted_equipment(
        authoritative,
        predicted.weapon,
        predicted.saber_holstered,
        predicted.saber_move,
        predicted.health > 0,
    )
}

/// Pose the local actor from the predicted view angles so the model turns in
/// the same frame as the camera (`cg_ents.c:3454-3476`); the interpolated
/// snapshot pose is a network round-trip behind the mouse.
pub(crate) fn pose(
    authoritative: Option<PoseState>,
    predicted: Option<&MovementState>,
) -> Option<PoseState> {
    let Some(predicted) = predicted else {
        return authoritative;
    };
    // EternalJK remaps the animations before `CG_G2PlayerAngles` reads them.
    let (legs_animation, torso_animation) = sjk_client::legacy_ejk_animation_fixes(
        predicted.legs_anim,
        predicted.torso_anim,
        predicted.weapon,
        predicted.saber_in_flight,
    );
    sjk_client::legacy_predicted_pose(
        authoritative,
        sjk_client::PredictedPoseFields {
            view_angles_degrees: predicted.view_angles,
            velocity: predicted.velocity,
            ground_entity_num: predicted.ground_entity_number,
            movement_direction: predicted.movement_direction,
            legs_animation,
            torso_animation,
            vehicle_entity_num: predicted.vehicle_entity_num,
            weapon: predicted.weapon,
            entity_flags: predicted.entity_flags,
            saber_move: predicted.saber_move,
        },
    )
}

fn prediction_is_presentable(state: &MovementState, authoritative_saber_move: Option<u32>) -> bool {
    state.saber_special_deferred == 0
        && (state.weapon != 3 || sjk_client::legacy_saber_move_is_predicted(state.saber_move))
        && authoritative_saber_move.is_none_or(sjk_client::legacy_saber_move_is_predicted)
}

fn update_track(
    current: Option<AnimationTrackState>,
    input: AnimationTrackInput,
    command_time: i64,
) -> AnimationTrackState {
    if let Some(mut current) = current
        && current.clip == input.clip
        && current.revision == input.revision
        && current.forced_frame == input.forced_frame
    {
        if current.speed_milli != input.speed_milli {
            current.phase_millis = current.elapsed_millis(command_time);
            current.started_at_millis = command_time;
            current.speed_milli = input.speed_milli;
            current.transition = None;
        }
        return current;
    }
    AnimationTrackState {
        clip: input.clip,
        revision: input.revision,
        started_at_millis: command_time,
        phase_millis: input.resume_phase_millis.unwrap_or(0),
        speed_milli: input.speed_milli,
        forced_frame: input.forced_frame,
        transition: current.map(|previous| AnimationTrackTransition {
            clip: previous.clip,
            revision: previous.revision,
            started_at_millis: previous.started_at_millis,
            phase_millis: previous.phase_millis,
            speed_milli: previous.speed_milli,
            forced_frame: previous.forced_frame,
            blend_started_at_millis: command_time,
            blend_duration_millis: input.blend_millis,
        }),
    }
}

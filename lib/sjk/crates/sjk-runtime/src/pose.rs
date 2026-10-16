//! Generic, adapter-resolved actor angle inputs.

use crate::EntityId;

/// Discrete pose policy resolved by the game adapter, without animation IDs or bone names.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseAngleState {
    /// Center the actor's angle swing while a non-idle animation is active.
    pub center_swing: bool,
    /// Compensate for animation-authored motion at the torso reference joint.
    pub correct_animation_motion: bool,
    /// Keep the rendered root at view yaw while retaining its internal swing history.
    pub hold_view_yaw: bool,
    /// Retain existing joint commands when taking a forced root path.
    pub preserve_overrides: bool,
    /// Optional entity whose interpolated position supplies the head-look direction.
    pub look_target: Option<EntityId>,
}

/// Interpolated view inputs and discrete adapter policy for an actor's procedural pose.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoseState {
    /// Presented view angles before actor-specific root/bone separation.
    pub view_angles_degrees: [f32; 3],
    /// Authoritative linear velocity used by an actor pose policy.
    pub velocity: [f32; 3],
    /// Adapter-defined directional animation sector.
    pub movement_direction: i8,
    /// Whether the actor has a supporting ground entity.
    pub grounded: bool,
    /// Adapter-resolved exception that disables velocity-facing pose changes.
    pub suppress_velocity_facing: bool,
    /// Actor state requires the neutral/root-only angle path.
    pub lock_root_angles: bool,
    /// Additional discrete procedural-pose decisions made by the adapter.
    pub angle: PoseAngleState,
}

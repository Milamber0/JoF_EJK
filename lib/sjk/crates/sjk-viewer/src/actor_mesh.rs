//! Retained actor geometry and animation state.
use super::*;

/// Map-lifetime actor mesh ranges and reusable pose storage.
pub(crate) struct ActorMesh {
    pub(crate) entity_id: Option<EntityId>,
    pub(crate) corpse_pool: bool,
    pub(crate) body_identity: Option<sjk_client::BodyIdentity>,
    /// A body whose animator is a `CG_BodyQueueCopy` of its source's
    /// ([`sjk_client::LegacyGhoul2Animator::body_queue_copy`]).
    pub(crate) body_copied: bool,
    pub(crate) appearance: Appearance,
    pub(crate) preview: PlayerPreview,
    pub(crate) draws: Vec<ActorDraw>,
    pub(crate) vertex_ranges: Vec<PreviewVertexRange>,
    pub(crate) current_frames: (usize, usize),
    pub(crate) weapon_attachments: [Option<saber::Attachment>; 2],
    /// A vehicle's driver bolt in this frame's pose, model space.
    pub(crate) driver_seat: Option<[f32; 3]>,
    pub(crate) saber_names: [Option<String>; 2],
    pub(crate) angle_controller: sjk_client::LegacyPlayerAngleController,
    pub(crate) render_yaw_degrees: Option<f32>,
    /// The held wall's facing during a wall rebound ([`crate::wall_hold_pose`]).
    pub(crate) wall_hold: crate::wall_hold_pose::WallHoldFacing,
    pub(crate) audio_events: crate::actor_pose::sounds::State,
    pub(crate) animator: crate::actor_pose::evaluation::Slot,
    pub(crate) pose_vertices: Vec<GpuVertex>,
    /// Optional render-only joint upload; CPU trace posing never depends on this buffer.
    pub(crate) gpu_palette: Option<crate::actor_pose::gpu_skinning::Palette>,
    /// This frame's CPU skinning result, shared with visual point queries after upload.
    pub(crate) retained_pose: crate::actor_pose::RetainedPose,
    pub(crate) force_bones: crate::actor_pose::ForceBones,
    /// JoF EJK's hat and cape this player wears, and their bolts.
    pub(crate) cosmetics: crate::cosmetics::actors::Worn,
    /// The `*chestg` bolt a jetpack hangs on, while one is worn.
    pub(crate) jetpack: crate::jetpack::Worn,
    /// `EF_DISINTEGRATION` while it lasts; the pose is frozen meanwhile.
    pub(crate) disintegration: Option<crate::disintegration::State>,
    /// Surface overrides from dismemberment, and which draws they leave visible.
    pub(crate) surfaces: crate::dismember::Surfaces,
    /// Set on a cut-off limb's mesh (`CG_General`'s client-limb case).
    pub(crate) limb: Option<crate::dismember::Limb>,
}

//! Ownership of the already-evaluated render pose; no skinning on the query path.

use crate::ActorMesh;
use sjk_model::SkinnedSurface;
use sjk_runtime::EntityId;

/// Actor-local geometry valid only after a successful upload in the current pose update.
#[derive(Default)]
pub(crate) struct RetainedPose {
    key: Option<(EntityId, i64)>,
    surfaces: Vec<SkinnedSurface>,
    trace_lod: Option<(usize, Vec<SkinnedSurface>)>,
    drawn: std::cell::Cell<bool>,
    trace_required: bool,
}

impl RetainedPose {
    /// Prepare the default collision LOD only for an explicit load-time consumer request.
    /// Render-only actors retain the upload source without allocating/skinning an extra LOD.
    pub(crate) fn new(
        model: &sjk_model::Glm,
        skin: &sjk_model::Skin,
        trace_required: bool,
    ) -> Result<Self, sjk_model::ModelError> {
        if !trace_required {
            return Ok(Self::default());
        }
        let lod = model.lods.len().saturating_sub(1).min(2);
        let matrices = vec![
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0]
            ];
            model.bone_count
        ];
        let trace_lod = if lod == 0 {
            None
        } else {
            Some((lod, model.skin_pose_matrices(skin, lod, &matrices)?))
        };
        Ok(Self {
            trace_lod,
            trace_required,
            ..Self::default()
        })
    }

    /// Whether a CPU geometry consumer explicitly requested retention at model load.
    pub(crate) fn trace_required(&self) -> bool {
        self.trace_required
    }

    /// Skin the trace LOD with the renderer's evaluated matrices, without evaluating bones.
    pub(crate) fn update_trace_lod(
        &mut self,
        model: &sjk_model::Glm,
        matrices: &[[[f32; 4]; 3]],
    ) -> Result<(), sjk_model::ModelError> {
        if let Some((lod, surfaces)) = &mut self.trace_lod {
            model.reskin_pose_matrices(*lod, matrices, surfaces)?;
        }
        Ok(())
    }
    /// Release the previous frame before skinning; skipped actors cannot expose stale poses.
    pub(crate) fn invalidate(&mut self) {
        self.key = None;
        self.drawn.set(false);
        self.surfaces = Vec::new();
    }

    /// Transfer the actual upload source, without cloning vertices or posing again.
    pub(crate) fn publish(&mut self, entity: EntityId, time: i64, surfaces: Vec<SkinnedSurface>) {
        self.surfaces = surfaces;
        self.key = Some((entity, time));
    }

    fn valid_for(&self, entity: EntityId, time: i64) -> bool {
        self.key == Some((entity, time))
    }

    /// Publish the draw stage only for a pose uploaded in this frame.
    pub(crate) fn mark_drawn(&self, entity: EntityId, time: i64) {
        self.drawn.set(self.valid_for(entity, time));
    }
}

impl ActorMesh {}

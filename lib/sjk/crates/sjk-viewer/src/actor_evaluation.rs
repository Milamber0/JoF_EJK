//! Owned actor evaluators: serial input preparation, independent evaluation, serial application.
use sjk_client::LegacyGhoul2Animator;
use sjk_model::{AnimationConfig, Gla, ModelError};
use sjk_runtime::AnimationState;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

#[path = "actor_evaluation_pool.rs"]
mod pool;
pub(crate) use pool::Pool;

/// An evaluator is absent only while its owned task is in flight; all tasks return before apply.
pub(crate) struct Slot {
    inner: Option<LegacyGhoul2Animator>,
    /// Serially prepared animation request; `None` leaves an inactive actor unevaluated.
    pub(crate) requested: Option<AnimationState>,
    error: Option<ModelError>,
    reported_error: bool,
}

impl Slot {
    /// Allocate the existing evaluator once, independently of worker count.
    pub(crate) fn new(animation: &Gla) -> Result<Self, ModelError> {
        Ok(Self {
            inner: Some(LegacyGhoul2Animator::new(animation)?),
            requested: None,
            error: None,
            reported_error: false,
        })
    }

    /// Own an evaluator prepared elsewhere, such as a body-queue copy.
    pub(crate) fn from_animator(animator: LegacyGhoul2Animator) -> Self {
        Self {
            inner: Some(animator),
            requested: None,
            error: None,
            reported_error: false,
        }
    }

    /// Report errors in actor order, after every in-flight evaluator has been returned.
    pub(crate) fn completed(&mut self) -> Result<(), ModelError> {
        self.error.take().map_or(Ok(()), Err)
    }

    /// Drop this frame's request (including audio cues); return true for its first failure.
    pub(crate) fn suppress_failed_frame(&mut self) -> bool {
        self.requested = None;
        !std::mem::replace(&mut self.reported_error, true)
    }

    /// A successful pose allows a future independent failure to be reported.
    pub(crate) fn clear_error_report(&mut self) {
        self.reported_error = false;
    }

    fn serial(&mut self, animation: &Gla, config: &AnimationConfig, time: i64) {
        if let Some(state) = self.requested {
            self.error = self
                .inner
                .as_mut()
                .expect("evaluator returned before next frame")
                .evaluate(animation, config, state, time)
                .err();
        }
    }
}

impl Deref for Slot {
    type Target = LegacyGhoul2Animator;
    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("evaluator returned before application")
    }
}

impl DerefMut for Slot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("evaluator returned before preparation")
    }
}

/// Immutable assets and exclusive evaluator ownership are the entire worker input.
struct Task {
    index: usize,
    animator: LegacyGhoul2Animator,
    animation: Arc<Gla>,
    config: Arc<AnimationConfig>,
    state: AnimationState,
    time: i64,
    error: Option<ModelError>,
    caller: std::thread::Thread,
}

impl Task {
    fn evaluate(&mut self) {
        self.error = self
            .animator
            .evaluate(&self.animation, &self.config, self.state, self.time)
            .err();
    }
}

/// Actor-independent access used by the production slice and controlled benchmark alike.
pub(crate) trait Actor {
    /// Borrow the exclusive slot and immutable assets without cloning animation storage.
    fn evaluation(&mut self) -> (&mut Slot, &Arc<Gla>, &Arc<AnimationConfig>);
}

impl Actor for crate::ActorMesh {
    fn evaluation(&mut self) -> (&mut Slot, &Arc<Gla>, &Arc<AnimationConfig>) {
        (
            &mut self.animator,
            &self.preview.animation,
            &self.preview.config,
        )
    }
}

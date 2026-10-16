//! Distinguish continuous solar motion from a discontinuous change of the clock.
use super::Light;

#[derive(Clone, Copy, Default)]
pub(super) struct History {
    pub(super) ready: bool,
    previous: Option<Light>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Update {
    Initial,
    Jump,
    Changed,
    Steady,
}

impl History {
    /// Compare consecutive frames, never cumulative rotation since an old restart.
    pub(super) fn observe(&mut self, light: Light, far_ready: bool) -> Update {
        let update = if !self.ready && far_ready {
            self.ready = true;
            Update::Initial
        } else if self.ready && self.previous.is_some_and(|old| old.changed_much(&light)) {
            Update::Jump
        } else if self.previous != Some(light) {
            Update::Changed
        } else {
            Update::Steady
        };
        self.previous = Some(light);
        update
    }
}

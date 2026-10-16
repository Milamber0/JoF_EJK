//! Tag interpolation for frame-animated MD3 models.
//!
//! Tags interpolate linearly between two frames with the renderer's `backlerp`
//! weight (`R_LerpTag`, rd-vanilla `tr_model.cpp:1792-1821`) and the axes are
//! renormalised afterwards; this is format policy, not game policy.

use super::{Md3, Md3Tag};

impl Md3 {
    /// Index of the tag called `name` (case-insensitive), from frame 0.
    pub fn tag_index(&self, name: &str) -> Option<usize> {
        self.tags
            .first()?
            .iter()
            .position(|tag| tag.name.eq_ignore_ascii_case(name))
    }

    /// Tag transform between `old_frame` and `frame`, weighted by `front_lerp`
    /// towards `frame` (`1.0` is exactly `frame`). Frames are clamped.
    pub fn lerp_tag(&self, tag: usize, old_frame: usize, frame: usize, front_lerp: f32) -> Md3Tag {
        let last = self.tags.len().saturating_sub(1);
        let old = &self.tags[old_frame.min(last)][tag];
        let new = &self.tags[frame.min(last)][tag];
        let back_lerp = 1.0 - front_lerp;
        Md3Tag {
            name: new.name.clone(),
            origin: lerp3(old.origin, new.origin, back_lerp, front_lerp),
            axes: [
                normalize(lerp3(old.axes[0], new.axes[0], back_lerp, front_lerp)),
                normalize(lerp3(old.axes[1], new.axes[1], back_lerp, front_lerp)),
                normalize(lerp3(old.axes[2], new.axes[2], back_lerp, front_lerp)),
            ],
        }
    }
}

fn lerp3(old: [f32; 3], new: [f32; 3], back_lerp: f32, front_lerp: f32) -> [f32; 3] {
    [
        old[0] * back_lerp + new[0] * front_lerp,
        old[1] * back_lerp + new[1] * front_lerp,
        old[2] * back_lerp + new[2] * front_lerp,
    ]
}

fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length > 0.0 {
        vector.map(|component| component / length)
    } else {
        vector
    }
}

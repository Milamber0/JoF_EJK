//! Ghoul2-compatible animation frame timing helpers.

use sjk_model::AnimationSequence;

pub(super) fn animation_frame(sequence: &AnimationSequence, elapsed_seconds: f32) -> usize {
    animation_sample(sequence, elapsed_seconds).0
}

pub(super) fn animation_sample(
    sequence: &AnimationSequence,
    elapsed_seconds: f32,
) -> (usize, usize, f32) {
    let fps = if sequence.frames_per_second == 0.0 {
        1.0
    } else {
        sequence.frames_per_second
    };
    let frame_lerp_millis = if fps.is_sign_negative() {
        (1_000.0 / fps).floor()
    } else {
        (1_000.0 / fps).ceil()
    };
    let elapsed_millis = elapsed_seconds.max(0.0) * 1_000.0;
    let looping = sequence.loop_frame != -1;
    if frame_lerp_millis.is_sign_positive() {
        let first = sequence.first_frame;
        let end = first + sequence.frame_count;
        let position = elapsed_millis / frame_lerp_millis;
        if !looping && position > (sequence.frame_count - 1) as f32 {
            return (end - 1, end - 1, 0.0);
        }
        let wrapped = if looping {
            position.rem_euclid(sequence.frame_count as f32)
        } else {
            position
        };
        let current = first + (wrapped.floor() as usize).min(sequence.frame_count - 1);
        let next = if current + 1 >= end {
            if looping { first } else { end - 1 }
        } else {
            current + 1
        };
        (
            current,
            next,
            if current == next {
                0.0
            } else {
                wrapped.fract()
            },
        )
    } else {
        let end = sequence.first_frame;
        let start = end + sequence.frame_count;
        let position = elapsed_millis / -frame_lerp_millis;
        if !looping && position >= sequence.frame_count as f32 {
            return (end + 1, end + 1, 0.0);
        }
        let wrapped = if looping {
            position.rem_euclid(sequence.frame_count as f32)
        } else {
            position
        };
        let current = start.saturating_sub(wrapped.floor() as usize);
        let next = if current <= end + 1 {
            if looping { start } else { end + 1 }
        } else {
            current - 1
        };
        (
            current,
            next,
            if current == next {
                0.0
            } else {
                wrapped.fract()
            },
        )
    }
}

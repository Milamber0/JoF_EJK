//! Ghoul2-compatible clock and endpoint handling for generic bone overrides.

use super::ModelError;
use super::bone_override::{BoneFrameSample, BoneOverride, OverrideEndBehavior};

pub(super) fn timing(
    state: BoneOverride,
    time_millis: i64,
    frame_count: usize,
) -> Result<BoneFrameSample, ModelError> {
    let elapsed = ((time_millis - state.start_time_millis).max(0) as f32) / 50.0;
    let position = state.start_frame as f32 + elapsed * state.speed;
    let (current, next, fraction) = if state.speed > 0.0 {
        timing_forward(state, position)
    } else {
        timing_reverse(state, position)
    };
    let current_frame = usize::try_from(current)
        .map_err(|_| ModelError::invalid(0, "negative evaluated animation frame"))?;
    let next_frame = usize::try_from(next)
        .map_err(|_| ModelError::invalid(0, "negative evaluated animation frame"))?;
    if current_frame >= frame_count || next_frame >= frame_count {
        return Err(ModelError::invalid(
            current_frame.max(next_frame),
            "evaluated animation frame is out of range",
        ));
    }
    Ok(BoneFrameSample {
        current_frame,
        next_frame,
        fraction,
    })
}

fn timing_forward(state: BoneOverride, mut position: f32) -> (i32, i32, f32) {
    if position > (state.end_frame - 1) as f32 {
        match state.end_behavior {
            OverrideEndBehavior::Loop => {
                let size = state.end_frame - state.start_frame;
                position = state.start_frame as f32
                    + (position - state.start_frame as f32).rem_euclid(size as f32);
            }
            OverrideEndBehavior::Freeze => {
                let terminal = state.end_frame - 1;
                return (terminal, terminal, 0.0);
            }
            OverrideEndBehavior::Stop => {}
        }
    }
    let current = position.floor() as i32;
    let next = if current + 1 >= state.end_frame {
        if state.end_behavior == OverrideEndBehavior::Loop {
            state.start_frame
        } else {
            state.end_frame - 1
        }
    } else {
        current + 1
    };
    (
        current,
        next,
        if current == next {
            0.0
        } else {
            position.fract()
        },
    )
}

fn timing_reverse(state: BoneOverride, mut position: f32) -> (i32, i32, f32) {
    if position < (state.end_frame + 1) as f32 {
        match state.end_behavior {
            OverrideEndBehavior::Loop => {
                if position >= state.end_frame as f32 {
                    return (
                        state.end_frame,
                        state.start_frame,
                        state.end_frame as f32 + 1.0 - position,
                    );
                }
                let signed_size = state.end_frame - state.start_frame;
                position = state.end_frame as f32
                    + (position - state.end_frame as f32) % signed_size as f32
                    - signed_size as f32;
            }
            OverrideEndBehavior::Freeze => {
                let terminal = state.end_frame + 1;
                return (terminal, terminal, 0.0);
            }
            OverrideEndBehavior::Stop => {}
        }
    }
    let current = position.ceil() as i32;
    let next = if current - 1 < state.end_frame + 1 {
        if state.end_behavior == OverrideEndBehavior::Loop {
            state.start_frame
        } else {
            state.end_frame + 1
        }
    } else {
        current - 1
    };
    let fraction = if current == next {
        0.0
    } else {
        current as f32 - position
    };
    (current, next, fraction)
}

pub(super) fn advance_override(state: &mut Option<BoneOverride>, time_millis: i64) {
    let Some(mut current) = *state else { return };
    let elapsed = ((time_millis - current.start_time_millis).max(0) as f32) / 50.0;
    let position = current.start_frame as f32 + elapsed * current.speed;
    let ended = if current.speed > 0.0 {
        position > (current.end_frame - 1) as f32
    } else {
        position < (current.end_frame + 1) as f32
    };
    if !ended {
        return;
    }
    match current.end_behavior {
        OverrideEndBehavior::Loop => {
            // Positive playback retains the virtual final→first interval.
            if current.speed > 0.0 && position < current.end_frame as f32 {
                return;
            }
            let size = (current.end_frame - current.start_frame) as f32;
            let wrapped =
                current.end_frame as f32 + (position - current.end_frame as f32) % size - size;
            let frame_time = wrapped - current.start_frame as f32;
            current.start_time_millis = time_millis - ((frame_time / current.speed) * 50.0) as i64;
            current.start_time_millis = current.start_time_millis.min(time_millis);
            *state = Some(current);
        }
        OverrideEndBehavior::Freeze => {}
        OverrideEndBehavior::Stop => *state = None,
    }
}

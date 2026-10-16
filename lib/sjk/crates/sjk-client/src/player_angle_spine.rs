//! Shared spine/neck helpers, OpenJK codemp/game/bg_pmove.c:8741-8999.

use super::math::{angle_subtract, normalized_angle};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LookState {
    last: [f32; 3],
    until: i64,
}

impl LookState {
    pub(super) fn update(
        &mut self,
        view: [f32; 3],
        target: Option<[f32; 3]>,
        time: i64,
    ) -> [f32; 3] {
        // cg_players.c:4279-4289: even a look target always has zero pitch.
        let mut look = target.unwrap_or(view);
        if target.is_some() {
            self.until = time + 1_000;
        }
        look[0] = 0.0;
        for axis in 0..3 {
            look[axis] = angle_subtract(normalized_angle(look[axis]), normalized_angle(view[axis]));
        }
        // bg_pmove.c:8749-8795: fixed .1*1.5 interpolation, not elapsed-time smoothing.
        if self.until > time {
            let limits = [50.0, 70.0, 30.0];
            let delta = std::array::from_fn::<_, 3, _>(|axis| {
                look[axis] = look[axis].clamp(-limits[axis], limits[axis]);
                normalized_angle(look[axis] - self.last[axis])
            });
            if delta.iter().any(|&v| v != 0.0) {
                for axis in 0..3 {
                    look[axis] = normalized_angle(self.last[axis] + delta[axis] * 0.1 * 1.5);
                }
            }
        }
        self.last = look;
        look
    }
}

pub(super) fn commands(
    mut view: [f32; 3],
    motion: Option<[f32; 3]>,
    look: [f32; 3],
) -> [[f32; 3]; 5] {
    // bg_pmove.c:8956-8983: normalize each motion component before subtracting.
    if let Some(motion) = motion {
        for axis in 0..3 {
            view[axis] = normalized_angle(view[axis] - normalized_angle(motion[axis]));
        }
    }
    // bg_pmove.c:8988-8998. The raw half-pitch input is intentionally unnormalized
    // when doCorr is false (9347-9349).
    let mut result = [
        [view[0] * 0.4, view[1] * 0.45, view[2] * 0.45],
        [view[0] * 0.4, view[1] * 0.35, view[2] * 0.35],
        [view[0] * 0.2, view[1] * 0.2, view[2] * 0.2],
        [0.0; 3],
        [0.0; 3],
    ];
    // bg_pmove.c:8804-8863: asymmetric head clamp, per-component nonzero blend.
    let min = [-25.0, -55.0, -10.0];
    let max = [50.0, 50.0, 10.0];
    for axis in 0..3 {
        let value = look[axis].clamp(min[axis], max[axis]);
        let contribution = value * [0.4, 0.1, 0.1][axis];
        result[2][axis] = if result[2][axis] != 0.0 {
            (result[2][axis] + contribution) * 0.5
        } else {
            contribution
        };
        result[3][axis] = value * [0.2, 0.3, 0.3][axis];
        result[4][axis] = value * [0.4, 0.6, 0.6][axis];
    }
    result
}

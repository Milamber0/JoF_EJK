//! A world of solid boxes, swept exactly (each box grown by the swept box, then a slab
//! test): a [`SweepWorld`] for tests and tools that need a world without a map.

use crate::walk::{Sweep, SweepWorld};

/// A world of axis-aligned solid boxes, each given by its lowest and highest corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Boxes(pub Vec<([f32; 3], [f32; 3])>);

impl SweepWorld for Boxes {
    fn sweep(&mut self, start: [f32; 3], mins: [f32; 3], maxs: [f32; 3], end: [f32; 3]) -> Sweep {
        const EPSILON: f32 = 0.03125;
        let delta: [f32; 3] = std::array::from_fn(|axis| end[axis] - start[axis]);
        let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        let (mut best, mut normal) = (1.0_f32, [0.0; 3]);
        for &(low, high) in &self.0 {
            let low: [f32; 3] = std::array::from_fn(|axis| low[axis] - maxs[axis]);
            let high: [f32; 3] = std::array::from_fn(|axis| high[axis] - mins[axis]);
            if (0..3)
                .all(|axis| start[axis] > low[axis] + 0.001 && start[axis] < high[axis] - 0.001)
            {
                return Sweep {
                    fraction: 0.0,
                    end: start,
                    normal: [0.0; 3],
                    start_solid: true,
                };
            }
            let (mut enter, mut leave, mut axis_in) = (-1.0_f32, 1.0_f32, None);
            let mut missed = false;
            for axis in 0..3 {
                if delta[axis].abs() < 1e-6 {
                    if start[axis] <= low[axis] || start[axis] >= high[axis] {
                        missed = true;
                    }
                    continue;
                }
                let (a, b) = (
                    (low[axis] - start[axis]) / delta[axis],
                    (high[axis] - start[axis]) / delta[axis],
                );
                let (near, far) = if a < b { (a, b) } else { (b, a) };
                if near > enter {
                    enter = near;
                    axis_in = Some((axis, if delta[axis] > 0.0 { -1.0 } else { 1.0 }));
                }
                leave = leave.min(far);
            }
            if missed || enter > leave || enter < 0.0 || enter >= best {
                continue;
            }
            best = enter;
            normal = [0.0; 3];
            if let Some((axis, sign)) = axis_in {
                normal[axis] = sign;
            }
        }
        let fraction = if best < 1.0 {
            (best - EPSILON / length.max(EPSILON)).max(0.0)
        } else {
            1.0
        };
        Sweep {
            fraction,
            end: std::array::from_fn(|axis| start[axis] + delta[axis] * fraction),
            normal,
            start_solid: false,
        }
    }
}

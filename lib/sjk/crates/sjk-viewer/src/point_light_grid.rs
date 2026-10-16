//! Fixed-storage conservative light masks shared by every camera in a frame.
use super::*;

const EDGE: usize = 12;
const CELLS: usize = EDGE * EDGE * EDGE;

/// Uniform-grid candidates; the original point-light array keeps its order and values.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct Grid {
    low: [f32; 4],
    high: [f32; 4],
    scale: [f32; 4],
    control: [u32; 4],
    masks: [[u32; 4]; CELLS / 4],
}

fn bounds(light: &PointLight) -> ([f32; 3], [f32; 3]) {
    let radius = light.radius.max(0.001);
    // Enclose rounded shader subtraction, including large coordinates and tiny radii.
    let guard =
        light.origin.iter().map(|v| v.abs()).fold(radius, f32::max) * (4.0 * f32::EPSILON) + 0.001;
    (
        light.origin.map(|v| (v - radius - guard).next_down()),
        light.origin.map(|v| (v + radius + guard).next_up()),
    )
}

/// Bin finite supports once per frame, with bounded linear work and no allocation.
pub(super) fn build(lights: &[PointLight]) -> Grid {
    let mut grid = Grid::zeroed();
    if lights.is_empty() {
        return grid;
    }
    grid.control[0] = u32::MAX >> (MAX_POINT_LIGHTS - lights.len());
    let mut boxes = [([0.0; 3], [0.0; 3]); MAX_POINT_LIGHTS];
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for (i, light) in lights.iter().enumerate() {
        if !light.radius.is_finite() || light.origin.iter().any(|v| !v.is_finite()) {
            grid.control[1] = 1;
            return grid;
        }
        boxes[i] = bounds(light);
        for axis in 0..3 {
            low[axis] = low[axis].min(boxes[i].0[axis]);
            high[axis] = high[axis].max(boxes[i].1[axis]);
        }
    }
    let scale = std::array::from_fn::<_, 3, _>(|i| EDGE as f32 / (high[i] - low[i]));
    if scale.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        grid.control[1] = 1;
        return grid;
    }
    grid.low = [low[0], low[1], low[2], 0.0];
    grid.high = [high[0], high[1], high[2], 0.0];
    grid.scale = [scale[0], scale[1], scale[2], 0.0];
    for (i, (a, b)) in boxes[..lights.len()].iter().enumerate() {
        let first = std::array::from_fn::<_, 3, _>(|k| {
            (((a[k] - low[k]) * scale[k]).next_down().max(0.0) as usize).min(EDGE - 1)
        });
        let last = std::array::from_fn::<_, 3, _>(|k| {
            (((b[k] - low[k]) * scale[k]).next_up().max(0.0) as usize).min(EDGE - 1)
        });
        for z in first[2]..=last[2] {
            for y in first[1]..=last[1] {
                for x in first[0]..=last[0] {
                    let cell = x + (y + z * EDGE) * EDGE;
                    grid.masks[cell / 4][cell % 4] |= 1u32 << i;
                }
            }
        }
    }
    grid
}

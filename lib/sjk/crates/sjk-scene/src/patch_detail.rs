//! Load-time curvature detail with shared-edge subdivision agreement. The
//! tolerance is geometric (map units), so detail does not pop as the eye moves.
//! This only changes rendering; BSP collision keeps its independent stock grid.
use super::*;
use std::collections::HashMap;

const MAX_STEPS: usize = 32;
const MAX_CHORD_ERROR: f32 = 0.5;

pub(super) struct Detail {
    pub horizontal: Vec<usize>,
    pub vertical: Vec<usize>,
}

/// Assign each quadratic span a power-of-two rate. Opposite edges use the
/// same rate, and matching boundary curves on different patches are unioned.
pub(super) fn build(bsp: &Bsp, minimum: usize) -> Vec<Option<Detail>> {
    let mut groups = Groups::default();
    let mut boundaries = HashMap::new();
    let mut spans = Vec::with_capacity(bsp.render().surfaces().len());
    for surface in bsp.render().surfaces() {
        let Some([width, height]) = surface.patch_dimensions else {
            spans.push(None);
            continue;
        };
        let controls = &bsp.render().vertices()[surface.vertices.clone()];
        let mut axes = [Vec::new(), Vec::new()];
        for axis in 0..2 {
            let (along, across) = if axis == 0 {
                (width, height)
            } else {
                (height, width)
            };
            for start in (0..along - 2).step_by(2) {
                let curve = |row: usize| {
                    std::array::from_fn(|i| {
                        controls[if axis == 0 {
                            row * width + start + i
                        } else {
                            (start + i) * width + row
                        }]
                        .position
                    })
                };
                let steps = (0..across)
                    .map(|row| subdivisions(curve(row), minimum))
                    .max()
                    .unwrap_or(minimum);
                let id = groups.add(steps);
                for row in [0, across - 1] {
                    let edge = edge_key(curve(row));
                    if let Some(other) = boundaries.insert(edge, id) {
                        groups.join(id, other);
                    }
                }
                axes[axis].push(id);
            }
        }
        spans.push(Some(axes));
    }
    spans
        .into_iter()
        .map(|axes| {
            axes.map(|[horizontal, vertical]| Detail {
                horizontal: horizontal.into_iter().map(|id| groups.steps(id)).collect(),
                vertical: vertical.into_iter().map(|id| groups.steps(id)).collect(),
            })
        })
        .collect()
}

fn subdivisions(curve: [[f32; 3]; 3], minimum: usize) -> usize {
    let mid_error =
        std::array::from_fn::<_, 3, _>(|i| (curve[0][i] - 2.0 * curve[1][i] + curve[2][i]) * 0.25);
    let error = mid_error.iter().map(|v| v * v).sum::<f32>().sqrt();
    let mut steps = minimum.next_power_of_two().min(MAX_STEPS);
    while steps < MAX_STEPS && error / (steps * steps) as f32 > MAX_CHORD_ERROR {
        steps *= 2;
    }
    steps
}

fn edge_key(curve: [[f32; 3]; 3]) -> [[u32; 3]; 3] {
    let forward = curve.map(|v| v.map(|x| if x == 0.0 { 0 } else { x.to_bits() }));
    let reverse = [forward[2], forward[1], forward[0]];
    forward.min(reverse)
}

#[derive(Default)]
struct Groups {
    parent: Vec<usize>,
    detail: Vec<usize>,
    rank: Vec<u8>,
}
impl Groups {
    fn add(&mut self, steps: usize) -> usize {
        let id = self.parent.len();
        self.parent.push(id);
        self.detail.push(steps);
        self.rank.push(0);
        id
    }
    fn root(&mut self, id: usize) -> usize {
        let parent = self.parent[id];
        if parent != id {
            self.parent[id] = self.root(parent);
        }
        self.parent[id]
    }
    fn join(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.root(a), self.root(b));
        if a == b {
            return;
        }
        if self.rank[a] < self.rank[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a;
        self.detail[a] = self.detail[a].max(self.detail[b]);
        if self.rank[a] == self.rank[b] {
            self.rank[a] += 1;
        }
    }
    fn steps(&mut self, id: usize) -> usize {
        let root = self.root(id);
        self.detail[root]
    }
}

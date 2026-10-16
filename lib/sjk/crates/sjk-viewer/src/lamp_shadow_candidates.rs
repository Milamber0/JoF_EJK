//! Map-lifetime bounds for the lamps that can receive an actor-shadow slot.
//! The point query only removes zero-score lamps; ranking remains unchanged.
use crate::lamp_lights::Lamp;
use glam::Vec3;

struct Node {
    low: Vec3,
    high: Vec3,
    first: u32,
    count: u32,
    right: u32,
}
/// Immutable influence bounds and source IDs, built once for a map.
pub(super) struct Candidates {
    nodes: Vec<Node>,
    ids: Vec<usize>,
}
impl Candidates {
    /// Include the same extra reach used by actor-shadow ranking.
    pub fn new(lamps: &[Lamp], extra_radius: f32) -> Self {
        let mut result = Self {
            nodes: Vec::with_capacity(lamps.len().div_ceil(8) * 2),
            ids: (0..lamps.len()).collect(),
        };
        if !lamps.is_empty() {
            result.build(lamps, extra_radius, 0, lamps.len());
        }
        result
    }
    fn build(&mut self, lamps: &[Lamp], extra: f32, first: usize, count: usize) -> u32 {
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = -low;
        let mut centers_low = low;
        let mut centers_high = high;
        for &id in &self.ids[first..first + count] {
            let lamp = &lamps[id];
            let reach = lamp.radius + extra;
            let guard = (lamp.position.abs().max_element() + reach) * f32::EPSILON * 8. + 0.001;
            low = low.min(lamp.position - Vec3::splat(reach + guard));
            high = high.max(lamp.position + Vec3::splat(reach + guard));
            centers_low = centers_low.min(lamp.position);
            centers_high = centers_high.max(lamp.position);
        }
        let index = self.nodes.len() as u32;
        self.nodes.push(Node {
            low,
            high,
            first: first as u32,
            count: count as u32,
            right: 0,
        });
        if count > 16 {
            let extent = centers_high - centers_low;
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let middle = count / 2;
            self.ids[first..first + count].select_nth_unstable_by(middle, |&a, &b| {
                lamps[a].position[axis]
                    .total_cmp(&lamps[b].position[axis])
                    .then(a.cmp(&b))
            });
            self.build(lamps, extra, first, middle);
            let right = self.build(lamps, extra, first + middle, count - middle);
            self.nodes[index as usize].count = 0;
            self.nodes[index as usize].right = right;
        }
        index
    }
    /// Visit a conservative superset of nonzero-score lamps without frame allocation.
    pub fn visit(&self, eye: Vec3, mut visit: impl FnMut(usize)) {
        if self.nodes.is_empty() {
            return;
        }
        // Median splitting bounds the depth by the number of bits in an index.
        let mut stack = [0u32; 64];
        let mut used = 1;
        while used != 0 {
            used -= 1;
            let index = stack[used];
            let node = &self.nodes[index as usize];
            if eye.cmplt(node.low).any() || eye.cmpgt(node.high).any() {
                continue;
            }
            if node.count != 0 {
                for &id in &self.ids[node.first as usize..(node.first + node.count) as usize] {
                    visit(id);
                }
            } else {
                stack[used] = node.right;
                stack[used + 1] = index + 1;
                used += 2;
            }
        }
    }
}

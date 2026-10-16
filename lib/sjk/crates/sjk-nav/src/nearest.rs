//! The nodes nearest a point (`CollectNearestNodes`, `navigator.cpp:1224-1297`).

use crate::{Graph, distance_squared};

/// A node found near a point, and its squared distance truncated to an unsigned integer
/// (`nodeList_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub node: i32,
    pub distance: u32,
}

impl Graph {
    /// `CollectNearestNodes`: into `out` (cleared first), the nodes within `radius` of
    /// `origin` in node order, each put before the first kept one it is nearer than, the
    /// list cut back to `max` whenever an insertion overflows it; a node nearer than none
    /// is appended while there is room. The first node found is always kept.
    pub fn collect_nearest(
        &self,
        origin: [f32; 3],
        radius: i32,
        max: usize,
        out: &mut Vec<Candidate>,
    ) {
        out.clear();
        let limit = radius.wrapping_mul(radius) as f32;
        for node in &self.nodes {
            let distance = distance_squared(node.position, origin);
            if distance > limit {
                continue;
            }
            let candidate = Candidate {
                node: node.id,
                distance: distance as u32,
            };
            if out.is_empty() {
                out.push(candidate);
                continue;
            }
            if let Some(at) = out.iter().position(|kept| distance < kept.distance as f32) {
                out.insert(at, candidate);
                if out.len() > max {
                    out.pop();
                }
            } else if out.len() < max {
                out.push(candidate);
            }
        }
    }
}

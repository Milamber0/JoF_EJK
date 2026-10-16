//! Which leaves a box touches: `CM_BoxLeafnums` (`cm_test.cpp`), the tree walk a server
//! links an entity with before it can ask what can see it.
use super::*;

/// What [`Bsp::box_leaves`] found.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoxLeaves {
    /// How many leaves were stored, in tree order, front children first.
    pub count: usize,
    /// The last leaf visited that belongs to a cluster, stored or not: all a caller
    /// learns of the leaves its list had no room for.
    pub last_leaf: Option<usize>,
}

impl Bsp {
    /// The leaves the box touches, solid ones included, into `list` as far as it has room.
    /// A box exactly on a plane goes down one side only, as `BoxOnPlaneSide` has it: the
    /// front for a box that starts at the plane, the back for one that ends at it.
    pub fn box_leaves(
        &self,
        minimums: [f32; 3],
        maximums: [f32; 3],
        list: &mut [usize],
    ) -> BoxLeaves {
        let mut found = BoxLeaves::default();
        if !self.nodes.is_empty() {
            self.box_leaves_below(NodeChild::Node(0), minimums, maximums, list, &mut found);
        }
        found
    }

    fn box_leaves_below(
        &self,
        mut child: NodeChild,
        minimums: [f32; 3],
        maximums: [f32; 3],
        list: &mut [usize],
        found: &mut BoxLeaves,
    ) {
        loop {
            let node = match child {
                NodeChild::Leaf(leaf) => {
                    if self.leaves[leaf].cluster != -1 {
                        found.last_leaf = Some(leaf);
                    }
                    if found.count < list.len() {
                        list[found.count] = leaf;
                        found.count += 1;
                    }
                    return;
                }
                NodeChild::Node(node) => &self.nodes[node],
            };
            match box_on_plane_side(minimums, maximums, self.planes[node.plane]) {
                1 => child = node.children[0],
                2 => child = node.children[1],
                _ => {
                    self.box_leaves_below(node.children[0], minimums, maximums, list, found);
                    child = node.children[1];
                }
            }
        }
    }
}

/// `BoxOnPlaneSide` (`q_math.c:948-982`): 1 in front, 2 behind, 3 across. A plane whose
/// normal has a component of exactly one is axial — a negative one is not.
fn box_on_plane_side(minimums: [f32; 3], maximums: [f32; 3], plane: Plane) -> u8 {
    if let Some(axis) = plane.normal.iter().position(|component| *component == 1.0) {
        return if plane.distance <= minimums[axis] {
            1
        } else if plane.distance >= maximums[axis] {
            2
        } else {
            3
        };
    }
    // The corner furthest along the normal and the one furthest against it.
    let mut distances = [0.0_f32; 2];
    for axis in 0..3 {
        let negative = usize::from(plane.normal[axis] < 0.0);
        distances[negative] += plane.normal[axis] * maximums[axis];
        distances[1 - negative] += plane.normal[axis] * minimums[axis];
    }
    u8::from(distances[0] >= plane.distance) | u8::from(distances[1] < plane.distance) << 1
}

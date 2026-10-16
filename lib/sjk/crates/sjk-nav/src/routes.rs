//! Routes along a graph whose ranks are known (`navigator.cpp:2352-2688`): the next node
//! toward a goal, the cost of the whole way, and whether a way is blocked by failed edges.

use crate::{Graph, NODE_NONE};

impl Graph {
    fn valid(&self, id: i32) -> bool {
        id >= 0 && (id as usize) < self.nodes.len()
    }

    /// `GetBestNode`: the neighbour of `start` whose rank in `end`'s flood is lowest — `end`
    /// itself if it is a neighbour — leaving out any ranked no better than `reject`.
    pub fn best_node(&self, start: i32, end: i32, reject: i32) -> i32 {
        if !self.valid(start) || !self.valid(end) {
            return NODE_NONE;
        }
        if start == end {
            return start;
        }
        let (from, to) = (&self.nodes[start as usize], &self.nodes[end as usize]);
        let mut reject_rank = 0;
        if reject != NODE_NONE && from.edges.iter().any(|edge| edge.node == reject) {
            reject_rank = to.rank(reject);
        }
        let (mut best, mut best_rank) = (NODE_NONE, self.limits.infinite_cost);
        for edge in &from.edges {
            if edge.node == end {
                return edge.node;
            }
            let rank = to.rank(edge.node);
            if rank <= reject_rank {
                continue;
            }
            if rank == NODE_NONE {
                return NODE_NONE;
            }
            if rank < best_rank {
                (best, best_rank) = (edge.node, rank);
            }
        }
        best
    }

    /// `GetPathCost`: the cost of the way from `start` to `end`, walked neighbour by
    /// neighbour to the one ranked lowest in `end`'s flood; the infinite cost where `start`
    /// has no edges or a neighbour has no route. (An unsigned sum in the reference, read
    /// back as an int by every caller.)
    pub fn path_cost(&self, start: i32, end: i32) -> i32 {
        let infinite = self.limits.infinite_cost;
        if !self.valid(start) || !self.valid(end) || self.nodes[start as usize].edges.is_empty() {
            return infinite;
        }
        let to = &self.nodes[end as usize];
        let mut at = start;
        let mut cost = 0_i32;
        // `dontScrewUp`: a walk of more than 40000 steps is given up with what it has.
        for _ in 0..=40_000 {
            if at == end {
                break;
            }
            let (mut best, mut best_rank, mut best_cost) = (NODE_NONE, self.limits.rank_ceiling, 0);
            for edge in &self.nodes[at as usize].edges {
                if edge.node == end {
                    return cost.wrapping_add(edge.cost);
                }
                let rank = to.rank(edge.node);
                if rank == NODE_NONE {
                    return infinite;
                }
                if rank < best_rank {
                    (best, best_rank, best_cost) = (edge.node, rank, edge.cost);
                }
            }
            cost = cost.wrapping_add(best_cost);
            if !self.valid(best) {
                break;
            }
            at = best;
        }
        cost
    }

    /// `GetBestNodeAltRoute`: the neighbour of `start` with the cheapest way on to `end`,
    /// and that way's cost with the step to it in `cost` — `end` itself at once when it is
    /// a neighbour. Neighbours whose way costs as much as `reject`'s are left out. With
    /// `alt_routes` (`d_altRoutes`), ways through failed edges are refused.
    ///
    /// As in the reference, `cost` is left untouched when there is no graph, a node is
    /// invalid, or `start` is `end`.
    pub fn best_node_alt_route(
        &self,
        start: i32,
        end: i32,
        cost: &mut i32,
        reject: i32,
        alt_routes: bool,
    ) -> i32 {
        let infinite = self.limits.infinite_cost;
        if self.nodes.is_empty() || !self.valid(start) || !self.valid(end) {
            return NODE_NONE;
        }
        if start == end {
            return if !alt_routes || self.failed.edge_failed(start, end).is_none() {
                start
            } else {
                NODE_NONE
            };
        }
        let from = &self.nodes[start as usize];
        let (mut best, mut best_rank, mut reject_rank, mut best_cost) =
            (NODE_NONE, infinite, infinite, infinite);
        *cost = 0;
        if reject != NODE_NONE && from.edges.iter().any(|edge| edge.node == reject) {
            reject_rank = self.path_cost(start, end);
        }
        for edge in &from.edges {
            let rank = self.path_cost(edge.node, end);
            if rank >= reject_rank {
                continue;
            }
            if edge.node == end {
                if !alt_routes || !self.route_blocked(start, edge.node, end, reject_rank) {
                    *cost = cost.wrapping_add(edge.cost);
                    return edge.node;
                }
                continue;
            }
            if rank == NODE_NONE {
                *cost = infinite;
                return NODE_NONE;
            }
            if rank < best_rank
                && (!alt_routes || !self.route_blocked(start, edge.node, end, reject_rank))
            {
                (best, best_rank, best_cost) = (edge.node, rank, edge.cost.wrapping_add(rank));
            }
        }
        *cost = best_cost;
        best
    }

    /// `RouteBlocked`: whether the way from `start` through its neighbour `test` to `end`
    /// meets only failed edges — walked by rank, never back, never through `start`, each
    /// step ranked better than the last (and than `reject_rank`).
    pub fn route_blocked(&self, start: i32, test: i32, end: i32, reject_rank: i32) -> bool {
        if self.failed.edge_failed(start, test).is_some() {
            return true;
        }
        if test == end {
            return false;
        }
        let to = &self.nodes[end as usize];
        let (mut next, mut last) = (test, start);
        let (mut best_next, mut best_rank) = (NODE_NONE, reject_rank);
        loop {
            let mut all_failed = true;
            for edge in &self.nodes[next as usize].edges {
                let id = edge.node;
                if id == last || id == start || self.failed.edge_failed(next, id).is_some() {
                    continue;
                }
                if id == end {
                    return false;
                }
                let rank = to.rank(id);
                if rank < 0 {
                    continue;
                }
                if rank < best_rank {
                    (best_next, best_rank, all_failed) = (id, rank, false);
                }
            }
            if all_failed {
                return true;
            }
            (last, next) = (next, best_next);
        }
    }

    /// `Connected`: whether `end` is `start`, a neighbour of it, or ranked from a
    /// neighbour of it.
    pub fn connected(&self, start: i32, end: i32) -> bool {
        if !self.valid(start) || !self.valid(end) {
            return false;
        }
        if start == end {
            return true;
        }
        let to = &self.nodes[end as usize];
        self.nodes[start as usize]
            .edges
            .iter()
            .any(|edge| edge.node == end || to.rank(edge.node) != NODE_NONE)
    }

    /// `NodesAreNeighbors`: whether `end` is one of `start`'s edges (never itself).
    pub fn neighbours(&self, start: i32, end: i32) -> bool {
        start != end
            && self
                .node(start)
                .is_some_and(|node| node.edges.iter().any(|edge| edge.node == end))
    }

    /// `GetProjectedNode`: the neighbour of `node` lying most nearly toward `origin`, never
    /// one behind it.
    pub fn projected_node(&self, origin: [f32; 3], node: i32) -> i32 {
        let Some(base) = self.node(node) else {
            return NODE_NONE;
        };
        let target = normalized(sub(origin, base.position));
        let (mut best, mut best_dot) = (NODE_NONE, 0.0_f32);
        for edge in &base.edges {
            let other = &self.nodes[edge.node as usize];
            let direction = normalized(sub(other.position, base.position));
            let dot =
                target[0] * direction[0] + target[1] * direction[1] + target[2] * direction[2];
            if dot < 0.0 {
                continue;
            }
            if dot > best_dot {
                (best, best_dot) = (other.id, dot);
            }
        }
        best
    }

    /// `GetNodeEdge`: the node edge `edge` of `node` leads to; -1 past its edges.
    pub fn node_edge(&self, node: i32, edge: i32) -> i32 {
        let Some(node) = self.node(node) else {
            return NODE_NONE;
        };
        usize::try_from(edge)
            .ok()
            .and_then(|at| node.edges.get(at))
            .map_or(NODE_NONE, |edge| edge.node)
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `VectorNormalize`: `sqrtf` of the float sum, scaled by its reciprocal.
fn normalized(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length == 0.0 {
        return v;
    }
    let inverse = 1.0 / length;
    [v[0] * inverse, v[1] * inverse, v[2] * inverse]
}

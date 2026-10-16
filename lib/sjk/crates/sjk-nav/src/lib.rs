//! Waypoint navigation graphs: nodes placed through a world, the edges between them, and
//! the routes an actor takes along them.
//!
//! This is engine functionality with no game in it. A game supplies the nodes (from its
//! own map entities or its own file format) and answers every question that needs its
//! world — whether an actor can walk to a node, whether a door is locked — itself; this
//! crate keeps the graph and answers the questions that are about the graph alone:
//!
//! - **ranks** ([`Graph::calculate_paths`]): for every node, the order in which a
//!   priority flood from it reaches every other node. A route is walked greedily by rank
//!   ([`Graph::best_node`], [`Graph::path_cost`]) instead of being stored;
//! - **nearest nodes** ([`Graph::collect_nearest`]): the nodes nearest a point, a
//!   bounded, sorted candidate list the game tests against its world;
//! - **failed edges** ([`FailedEdges`]): connections an actor found blocked, kept out of
//!   routes until they are found clear again;
//! - **graphs made from a world** ([`walk`], [`cover`]): for a level whose game supplies
//!   no nodes, walkable floor sampled against an actor's box (steps, drops and jumps
//!   between floor samples), links checked the same way, and the places beside walls
//!   worth taking cover at. The world is the game's, asked through [`walk::SweepWorld`].
//!
//! The behaviour is Raven's navigator's (`codemp/server/NPCNav/navigator.cpp` in
//! OpenJK), which the Jedi Academy compatibility profile (`sjk-game-jka`) holds to the
//! reference: the flood's tie-breaking is libstdc++'s binary heap, as the reference built
//! on Linux orders it, and quantities are the reference's integers and floats. What the
//! reference fixed at compile time — the "infinite" cost, the rank ceiling, the number
//! of failed edges kept — is a [`Limits`] value the game chooses.

pub mod boxes;
pub mod cover;
mod failed;
mod heap;
mod nearest;
mod routes;
pub mod walk;

pub use failed::{FailedEdge, FailedEdges};
pub use nearest::Candidate;

/// No node (`NODE_NONE`, `WAYPOINT_NONE`).
pub const NODE_NONE: i32 = -1;

/// Node flag: the node's ranks are stale and are recalculated before its next use
/// (`NF_RECALC`).
pub const NODE_RECALC: i32 = 0x4;

/// Edge flag: the world blocked the connection when it was made (`EFLAG_BLOCKED`).
pub const EDGE_BLOCKED: u8 = 0x1;
/// Edge flag: the way along the connection falls further than a step ([`walk`]); it
/// cannot be walked back.
pub const EDGE_DROP: u8 = 0x2;
/// Edge flag: the way along the connection takes a jump ([`walk`]).
pub const EDGE_JUMP: u8 = 0x4;

/// What a game fixes about its graphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The cost of a route that does not exist, and of an edge that failed.
    pub infinite_cost: i32,
    /// A rank no node reaches: the starting best when a route is walked.
    pub rank_ceiling: i32,
}

/// A connection from a node to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The node it leads to.
    pub node: i32,
    /// Its cost: the distance between the two nodes, truncated.
    pub cost: i32,
    /// [`EDGE_BLOCKED`] and the game's own flags.
    pub flags: u8,
}

/// A node of the graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Where it is.
    pub position: [f32; 3],
    /// [`NODE_RECALC`] and the game's own flags.
    pub flags: i32,
    /// The radius within which an actor is inside it.
    pub radius: i32,
    /// Its number, as given when it was added or read.
    pub id: i32,
    /// Its connections, in the order they were made.
    pub edges: Vec<Edge>,
    /// The rank of every node in the flood from this one, by node number: -1 for a node
    /// the flood never reached. Empty until ranks are calculated or read.
    pub ranks: Vec<i32>,
}

impl Node {
    /// A node with no edges and no ranks.
    pub fn new(position: [f32; 3], flags: i32, radius: i32, id: i32) -> Self {
        Self {
            position,
            flags,
            radius,
            id,
            edges: Vec::new(),
            ranks: Vec::new(),
        }
    }

    /// `CNode::AddEdge`: the connection to `node` made, or its cost and flags replaced if
    /// it already exists.
    pub fn add_edge(&mut self, node: i32, cost: i32, flags: u8) {
        if let Some(edge) = self.edges.iter_mut().find(|edge| edge.node == node) {
            edge.cost = cost;
            edge.flags = flags;
            return;
        }
        self.edges.push(Edge { node, cost, flags });
    }

    /// The rank of node `id` in this node's flood (`CNode::GetRank`); -1 where ranks were
    /// never calculated.
    pub fn rank(&self, id: i32) -> i32 {
        usize::try_from(id)
            .ok()
            .and_then(|at| self.ranks.get(at))
            .copied()
            .unwrap_or(NODE_NONE)
    }
}

/// `DistanceSquared` as C computes it: float products, summed left to right.
pub fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let v = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

/// `Distance`: the square root of [`distance_squared`].
pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    distance_squared(a, b).sqrt()
}

/// A navigation graph: its nodes, its failed edges, and whether its routes are known.
#[derive(Clone, Debug)]
pub struct Graph {
    limits: Limits,
    nodes: Vec<Node>,
    /// The connections actors found blocked.
    pub failed: FailedEdges,
    /// Whether every node's ranks have been calculated (`pathsCalculated`): a failed edge
    /// then makes every node recalculate before its next use.
    pub paths_calculated: bool,
    /// Whether a connection runs one way only ([`Graph::set_directed`]).
    directed: bool,
    /// The flood's scratch, kept from node to node: its queue, the nodes it reached, and —
    /// for a directed graph — every node's incoming connections.
    heap: heap::Heap,
    reached: Vec<bool>,
    incoming: Vec<Vec<Edge>>,
}

impl Graph {
    /// An empty graph keeping `failed_edges` failed edges.
    pub fn new(limits: Limits, failed_edges: usize) -> Self {
        Self {
            limits,
            nodes: Vec::new(),
            failed: FailedEdges::new(failed_edges),
            paths_calculated: false,
            directed: false,
            heap: heap::Heap::default(),
            reached: Vec::new(),
            incoming: Vec::new(),
        }
    }

    /// Whether connections run one way only. The reference's graphs are undirected — every
    /// connection made both ways ([`Graph::hard_connect`]) — and its flood follows a
    /// node's own connections out. A directed graph (a drop that cannot be climbed back,
    /// [`Graph::link`]) is flooded along the connections *into* each node instead, so
    /// that a node's ranks are the order in which the others can reach it — what the
    /// route walk ([`Graph::best_node`]) reads them as.
    pub fn set_directed(&mut self, directed: bool) {
        self.directed = directed;
    }

    /// Whether the graph is directed ([`Graph::set_directed`]).
    pub fn is_directed(&self) -> bool {
        self.directed
    }

    /// The limits the graph was made with.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Every node, by number.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Node `id`, if there is one.
    pub fn node(&self, id: i32) -> Option<&Node> {
        usize::try_from(id).ok().and_then(|at| self.nodes.get(at))
    }

    /// How many nodes there are.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every node dropped, and the failed edges' lookup with them (`CNavigator::Free`).
    /// The failed edges themselves are kept, as the reference keeps them.
    pub fn free(&mut self) {
        self.nodes.clear();
        self.failed.forget_lookup();
    }

    /// `AddRawPoint`: a node at `position`, numbered next. Returns its number.
    pub fn add_node(&mut self, position: [f32; 3], flags: i32, radius: i32) -> i32 {
        let id = self.nodes.len() as i32;
        self.nodes.push(Node::new(position, flags, radius, id));
        id
    }

    /// A node as a file had it, appended whole.
    pub fn push_node(&mut self, node: Node) {
        self.nodes.push(node);
    }

    /// `HardConnect`: `first` and `second` connected both ways at the distance between
    /// them, flagged [`EDGE_BLOCKED`] where the world blocked the connection.
    pub fn hard_connect(&mut self, first: i32, second: i32, blocked: bool) {
        let (Some(start), Some(end)) = (self.node(first), self.node(second)) else {
            return;
        };
        let cost = distance(start.position, end.position) as i32;
        let flags = if blocked { EDGE_BLOCKED } else { 0 };
        self.nodes[first as usize].add_edge(second, cost, flags);
        self.nodes[second as usize].add_edge(first, cost, flags);
    }

    /// The one-way connection from `first` to `second` at the distance between them, with
    /// `flags` (not the reference's: a made graph's, [`walk`]).
    pub fn link(&mut self, first: i32, second: i32, flags: u8) {
        let (Some(start), Some(end)) = (self.node(first), self.node(second)) else {
            return;
        };
        let cost = distance(start.position, end.position) as i32;
        self.nodes[first as usize].add_edge(second, cost, flags);
    }

    /// `SetEdgeCost`: the connection between `first` and `second` given `cost` both ways
    /// (its flags cleared), or — for `None` — the distance between them. Nothing for an
    /// invalid node.
    pub fn set_edge_cost(&mut self, first: i32, second: i32, cost: Option<i32>) {
        let (Some(start), Some(end)) = (self.node(first), self.node(second)) else {
            return;
        };
        let cost = cost.unwrap_or_else(|| distance(start.position, end.position) as i32);
        self.nodes[first as usize].add_edge(second, cost, 0);
        self.nodes[second as usize].add_edge(first, cost, 0);
    }

    /// `FlagAllNodes`.
    pub fn flag_all(&mut self, flag: i32) {
        for node in &mut self.nodes {
            node.flags |= flag;
        }
    }

    /// `CalculatePaths`: every node's ranks made afresh (all -1, then its flood), and the
    /// routes known.
    pub fn calculate_paths(&mut self) {
        let count = self.nodes.len();
        for node in &mut self.nodes {
            node.ranks.clear();
            node.ranks.resize(count, NODE_NONE);
        }
        self.index_incoming();
        for at in 0..count {
            self.flood(at);
        }
        self.paths_calculated = true;
    }

    /// `CalculatePath`: the ranks of node `at` from a priority flood — its edges' nodes
    /// queued by cost, each node queued once, when it is first seen, and ranked as it
    /// leaves the queue. Ranks of nodes the flood no longer reaches are left as they were;
    /// [`NODE_RECALC`] is cleared. (A directed graph is flooded along the connections into
    /// each node: [`Graph::set_directed`].)
    pub fn calculate_path(&mut self, at: usize) {
        self.index_incoming();
        self.flood(at);
    }

    /// For a directed graph, every node's incoming connections, in the order of their
    /// source nodes and then of the sources' own connections.
    fn index_incoming(&mut self) {
        if !self.directed {
            return;
        }
        self.incoming.iter_mut().for_each(Vec::clear);
        self.incoming.resize_with(self.nodes.len(), Vec::new);
        for (from, node) in self.nodes.iter().enumerate() {
            for edge in &node.edges {
                if let Some(into) = usize::try_from(edge.node)
                    .ok()
                    .and_then(|at| self.incoming.get_mut(at))
                {
                    into.push(Edge {
                        node: from as i32,
                        ..*edge
                    });
                }
            }
        }
    }

    /// The flood itself, over the node's own connections or — directed — the incoming ones
    /// ([`Graph::index_incoming`] made first).
    fn flood(&mut self, at: usize) {
        let Self {
            nodes,
            heap,
            reached,
            incoming,
            directed,
            ..
        } = self;
        let links = |nodes: &[Node], from: usize| -> usize {
            if *directed {
                incoming[from].len()
            } else {
                nodes[from].edges.len()
            }
        };
        let link = |nodes: &[Node], from: usize, index: usize| -> Edge {
            if *directed {
                incoming[from][index]
            } else {
                nodes[from].edges[index]
            }
        };
        reached.clear();
        reached.resize(nodes.len(), false);
        heap.clear();
        let root = nodes[at].id;
        let mut rank = 0;
        reached[root as usize] = true;
        set_rank(&mut nodes[at], root, rank);
        rank += 1;
        for index in 0..links(nodes, at) {
            let edge = link(nodes, at, index);
            reached[edge.node as usize] = true;
            heap.push(heap::Queued {
                first: edge.node,
                second: edge.node,
                cost: edge.cost,
            });
        }
        while let Some(test) = heap.pop() {
            set_rank(&mut nodes[at], test.first, rank);
            rank += 1;
            let from = test.first as usize;
            for index in 0..links(nodes, from) {
                let edge = link(nodes, from, index);
                if reached[edge.node as usize] {
                    continue;
                }
                heap.push(heap::Queued {
                    first: edge.node,
                    second: test.second,
                    cost: test.cost.wrapping_add(edge.cost),
                });
                reached[edge.node as usize] = true;
            }
        }
        nodes[at].flags &= !NODE_RECALC;
    }
}

/// `CNode::AddRank`.
fn set_rank(node: &mut Node, id: i32, rank: i32) {
    if let Some(slot) = usize::try_from(id)
        .ok()
        .and_then(|at| node.ranks.get_mut(at))
    {
        *slot = rank;
    }
}

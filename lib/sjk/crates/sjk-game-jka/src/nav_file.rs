//! Jedi Academy's navigation files, `maps/<map>.nav` (`CNavigator::Load` and `Save`,
//! `CNode::Load` and `Save`, `navigator.cpp:376-712`): little-endian, `JNV5`, the map's
//! checksum (`sv_mapChecksum`), the nodes each with its `NODE` header, position, flags,
//! number, radius, edges (`edge_t`: number, cost, a flag byte and three bytes of padding)
//! and ranks, and the 32 failed edges.
//!
//! A file is refused whole where the reference refuses it: no `JNV5`, a checksum not the
//! map's, a node without `NODE`. A file that ends early reads as zeros past its end, as
//! `FS_Read` leaves a zeroed buffer short.

use sjk_nav::{Edge, FailedEdge, Graph, Node};

/// `NAV_HEADER_ID`, `NODE_HEADER_ID` (`INT_ID('J','N','V','5')`, `INT_ID('N','O','D','E')`:
/// the first letter the high byte, so a file begins `5VNJ`).
const NAV_HEADER: u32 = u32::from_be_bytes(*b"JNV5");
const NODE_HEADER: u32 = u32::from_be_bytes(*b"NODE");
/// `MAX_FAILED_EDGES`.
pub const MAX_FAILED_EDGES: usize = 32;

/// A file read little-endian, zeros past its end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn word(&mut self) -> [u8; 4] {
        let mut word = [0; 4];
        for (index, byte) in word.iter_mut().enumerate() {
            *byte = self.bytes.get(self.at + index).copied().unwrap_or(0);
        }
        self.at += 4;
        word
    }

    fn int(&mut self) -> i32 {
        i32::from_le_bytes(self.word())
    }

    fn float(&mut self) -> f32 {
        f32::from_le_bytes(self.word())
    }
}

/// Why a file was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not a navigation file (`JNV5`).
    Header,
    /// Made for another build of the map: its checksum, the map's.
    Checksum { file: i32, map: i32 },
    /// A node without its `NODE` header.
    Node(usize),
}

/// `CNavigator::Load`: `bytes` read into `graph` (freed first, as the reference frees it)
/// if the file is for the map whose checksum is `checksum`. On a refusal the graph keeps
/// whatever nodes were read before the bad one, as the reference's does.
pub fn load(graph: &mut Graph, bytes: &[u8], checksum: i32) -> Result<(), Refused> {
    graph.free();
    let mut file = Reader { bytes, at: 0 };
    if file.int() as u32 != NAV_HEADER {
        return Err(Refused::Header);
    }
    let check = file.int();
    if check != checksum {
        return Err(Refused::Checksum {
            file: check,
            map: checksum,
        });
    }
    let count = file.int();
    for index in 0..count.max(0) as usize {
        if file.int() as u32 != NODE_HEADER {
            return Err(Refused::Node(index));
        }
        let position = [file.float(), file.float(), file.float()];
        let (flags, id, radius) = (file.int(), file.int(), file.int());
        let mut node = Node::new(position, flags, radius, id);
        let edges = file.int();
        for _ in 0..edges.max(0) {
            let (to, cost) = (file.int(), file.int());
            let flags = file.word()[0];
            // `STL_INSERT` appends: a file's repeated edge stays repeated.
            node.edges.push(Edge {
                node: to,
                cost,
                flags,
            });
        }
        let ranks = file.int();
        node.ranks = (0..ranks.max(0)).map(|_| file.int()).collect();
        graph.push_node(node);
    }
    let failed: Vec<FailedEdge> = (0..MAX_FAILED_EDGES)
        .map(|_| FailedEdge {
            start: file.int(),
            end: file.int(),
            check_time: file.int(),
            entity: file.int(),
        })
        .collect();
    graph.failed.load(&failed);
    Ok(())
}

/// `CNavigator::Save`: the graph as a file for the map whose checksum is `checksum`. The
/// padding after an edge's flag byte, which the reference writes from uninitialised
/// memory, is written as zeros.
pub fn save(graph: &Graph, checksum: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let int = |out: &mut Vec<u8>, value: i32| out.extend_from_slice(&value.to_le_bytes());
    int(&mut out, NAV_HEADER as i32);
    int(&mut out, checksum);
    let count = graph.len() as i32;
    int(&mut out, count);
    for node in graph.nodes() {
        int(&mut out, NODE_HEADER as i32);
        for axis in node.position {
            out.extend_from_slice(&axis.to_le_bytes());
        }
        int(&mut out, node.flags);
        int(&mut out, node.id);
        int(&mut out, node.radius);
        int(&mut out, node.edges.len() as i32);
        for edge in &node.edges {
            int(&mut out, edge.node);
            int(&mut out, edge.cost);
            out.extend_from_slice(&[edge.flags, 0, 0, 0]);
        }
        // `m_ranks[i]` for every node: a node never ranked writes -1s here, where the
        // reference would read through a null table.
        int(&mut out, count);
        for id in 0..count {
            int(&mut out, node.rank(id));
        }
    }
    for slot in graph.failed.slots() {
        for value in [slot.start, slot.end, slot.check_time, slot.entity] {
            int(&mut out, value);
        }
    }
    out
}

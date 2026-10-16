use std::error::Error;
use std::fmt;

#[path = "adaptive_huffman_read.rs"]
mod read;
pub use read::decompress_connect_block;

const SYMBOLS: usize = 256;
const NYT: usize = SYMBOLS;
const INTERNAL_NODE: usize = SYMBOLS + 1;
const MAX_BLOCK_BYTES: usize = u16::MAX as usize;

#[derive(Clone, Debug, Default)]
struct Node {
    symbol: usize,
    weight: u32,
    parent: Option<usize>,
    left: Option<usize>,
    right: Option<usize>,
    next: Option<usize>,
    previous: Option<usize>,
    head: Option<usize>,
}

/// Encodes a block with the adaptive Huffman codec used only by JKA's
/// connection request. The returned block includes the two-byte uncompressed
/// length prefix written by `Huff_Compress`.
pub fn compress_connect_block(input: &[u8]) -> Result<Vec<u8>, AdaptiveHuffmanError> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    if input.len() > MAX_BLOCK_BYTES {
        return Err(AdaptiveHuffmanError::InputTooLarge(input.len()));
    }

    let maximum_bits = input.len() * 8;
    let mut output = vec![(input.len() >> 8) as u8, input.len() as u8];
    let mut bit_position = 16;
    let mut tree = AdaptiveTree::new();
    for &symbol in input {
        tree.transmit(
            usize::from(symbol),
            &mut output,
            &mut bit_position,
            maximum_bits,
        )?;
        tree.add_reference(usize::from(symbol));
    }

    // OpenJK deliberately retains one final padding byte.
    output.resize((bit_position >> 3) + 1, 0);
    Ok(output)
}

struct AdaptiveTree {
    nodes: Vec<Node>,
    locations: [Option<usize>; SYMBOLS + 1],
    group_heads: Vec<Option<usize>>,
    free_group_heads: Vec<usize>,
    root: usize,
    list_head: usize,
}

impl AdaptiveTree {
    fn new() -> Self {
        let mut locations = [None; SYMBOLS + 1];
        locations[NYT] = Some(0);
        Self {
            nodes: vec![Node {
                symbol: NYT,
                ..Node::default()
            }],
            locations,
            group_heads: Vec::new(),
            free_group_heads: Vec::new(),
            root: 0,
            list_head: 0,
        }
    }

    fn allocate_node(&mut self) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node::default());
        index
    }

    fn allocate_group_head(&mut self, value: usize) -> usize {
        if let Some(index) = self.free_group_heads.pop() {
            self.group_heads[index] = Some(value);
            index
        } else {
            let index = self.group_heads.len();
            self.group_heads.push(Some(value));
            index
        }
    }

    fn release_group_head(&mut self, index: usize) {
        self.group_heads[index] = None;
        self.free_group_heads.push(index);
    }

    fn transmit(
        &self,
        symbol: usize,
        output: &mut Vec<u8>,
        bit_position: &mut usize,
        maximum_bits: usize,
    ) -> Result<(), AdaptiveHuffmanError> {
        if let Some(node) = self.locations[symbol] {
            self.transmit_node(node, output, bit_position, maximum_bits)?;
        } else {
            self.transmit_node(
                self.locations[NYT].expect("NYT node is permanent"),
                output,
                bit_position,
                maximum_bits,
            )?;
            for shift in (0..8).rev() {
                write_bit(
                    ((symbol >> shift) & 1) as u8,
                    output,
                    bit_position,
                    maximum_bits,
                )?;
            }
        }
        Ok(())
    }

    fn transmit_node(
        &self,
        node: usize,
        output: &mut Vec<u8>,
        bit_position: &mut usize,
        maximum_bits: usize,
    ) -> Result<(), AdaptiveHuffmanError> {
        let mut path = Vec::new();
        let mut child = node;
        while let Some(parent) = self.nodes[child].parent {
            path.push(u8::from(self.nodes[parent].right == Some(child)));
            child = parent;
        }
        for bit in path.into_iter().rev() {
            write_bit(bit, output, bit_position, maximum_bits)?;
        }
        Ok(())
    }

    fn add_reference(&mut self, symbol: usize) {
        if let Some(node) = self.locations[symbol] {
            self.increment(node);
            return;
        }

        let symbol_node = self.allocate_node();
        let internal_node = self.allocate_node();
        let old_next = self.nodes[self.list_head].next;

        self.nodes[internal_node].symbol = INTERNAL_NODE;
        self.nodes[internal_node].weight = 1;
        self.nodes[internal_node].next = old_next;
        self.nodes[internal_node].previous = Some(self.list_head);
        if let Some(next) = old_next {
            self.nodes[next].previous = Some(internal_node);
            self.nodes[internal_node].head = if self.nodes[next].weight == 1 {
                self.nodes[next].head
            } else {
                Some(self.allocate_group_head(internal_node))
            };
        } else {
            self.nodes[internal_node].head = Some(self.allocate_group_head(internal_node));
        }
        self.nodes[self.list_head].next = Some(internal_node);

        self.nodes[symbol_node].symbol = symbol;
        self.nodes[symbol_node].weight = 1;
        self.nodes[symbol_node].next = Some(internal_node);
        self.nodes[symbol_node].previous = Some(self.list_head);
        self.nodes[internal_node].previous = Some(symbol_node);
        self.nodes[symbol_node].head = self.nodes[internal_node].head;
        self.nodes[self.list_head].next = Some(symbol_node);

        let old_parent = self.nodes[self.list_head].parent;
        if let Some(parent) = old_parent {
            if self.nodes[parent].left == Some(self.list_head) {
                self.nodes[parent].left = Some(internal_node);
            } else {
                self.nodes[parent].right = Some(internal_node);
            }
        } else {
            self.root = internal_node;
        }

        self.nodes[internal_node].right = Some(symbol_node);
        self.nodes[internal_node].left = Some(self.list_head);
        self.nodes[internal_node].parent = old_parent;
        self.nodes[self.list_head].parent = Some(internal_node);
        self.nodes[symbol_node].parent = Some(internal_node);
        self.locations[symbol] = Some(symbol_node);

        if let Some(parent) = old_parent {
            self.increment(parent);
        }
    }

    fn increment(&mut self, node: usize) {
        if self.nodes[node]
            .next
            .is_some_and(|next| self.nodes[next].weight == self.nodes[node].weight)
        {
            let head = self.nodes[node]
                .head
                .expect("weighted node has a group head");
            let leader = self.group_heads[head].expect("live group head has a leader");
            if Some(leader) != self.nodes[node].parent {
                self.swap_tree_nodes(leader, node);
            }
            self.swap_list_nodes(leader, node);
        }

        let old_head = self.nodes[node]
            .head
            .expect("weighted node has a group head");
        if self.nodes[node]
            .previous
            .is_some_and(|previous| self.nodes[previous].weight == self.nodes[node].weight)
        {
            self.group_heads[old_head] = self.nodes[node].previous;
        } else {
            self.release_group_head(old_head);
        }

        self.nodes[node].weight += 1;
        if let Some(next) = self.nodes[node].next
            && self.nodes[next].weight == self.nodes[node].weight
        {
            self.nodes[node].head = self.nodes[next].head;
        } else {
            let head = self.allocate_group_head(node);
            self.nodes[node].head = Some(head);
        }

        if let Some(parent) = self.nodes[node].parent {
            self.increment(parent);
            if self.nodes[node].previous == Some(parent) {
                self.swap_list_nodes(node, parent);
                let head = self.nodes[node]
                    .head
                    .expect("weighted node has a group head");
                if self.group_heads[head] == Some(node) {
                    self.group_heads[head] = Some(parent);
                }
            }
        }
    }

    fn swap_tree_nodes(&mut self, first: usize, second: usize) {
        let first_parent = self.nodes[first].parent;
        let second_parent = self.nodes[second].parent;

        if let Some(parent) = first_parent {
            if self.nodes[parent].left == Some(first) {
                self.nodes[parent].left = Some(second);
            } else {
                self.nodes[parent].right = Some(second);
            }
        } else {
            self.root = second;
        }
        if let Some(parent) = second_parent {
            if self.nodes[parent].left == Some(second) {
                self.nodes[parent].left = Some(first);
            } else {
                self.nodes[parent].right = Some(first);
            }
        } else {
            self.root = first;
        }
        self.nodes[first].parent = second_parent;
        self.nodes[second].parent = first_parent;
    }

    fn swap_list_nodes(&mut self, first: usize, second: usize) {
        let first_next = self.nodes[first].next;
        self.nodes[first].next = self.nodes[second].next;
        self.nodes[second].next = first_next;

        let first_previous = self.nodes[first].previous;
        self.nodes[first].previous = self.nodes[second].previous;
        self.nodes[second].previous = first_previous;

        if self.nodes[first].next == Some(first) {
            self.nodes[first].next = Some(second);
        }
        if self.nodes[second].next == Some(second) {
            self.nodes[second].next = Some(first);
        }
        if let Some(next) = self.nodes[first].next {
            self.nodes[next].previous = Some(first);
        }
        if let Some(next) = self.nodes[second].next {
            self.nodes[next].previous = Some(second);
        }
        if let Some(previous) = self.nodes[first].previous {
            self.nodes[previous].next = Some(first);
        }
        if let Some(previous) = self.nodes[second].previous {
            self.nodes[previous].next = Some(second);
        }
    }
}

fn write_bit(
    bit: u8,
    output: &mut Vec<u8>,
    bit_position: &mut usize,
    maximum_bits: usize,
) -> Result<(), AdaptiveHuffmanError> {
    if *bit_position >= maximum_bits {
        return Err(AdaptiveHuffmanError::OutputWouldExpand);
    }
    let byte_index = *bit_position >> 3;
    if output.len() <= byte_index {
        output.resize(byte_index + 1, 0);
    }
    output[byte_index] |= (bit & 1) << (*bit_position & 7);
    *bit_position += 1;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdaptiveHuffmanError {
    InputTooLarge(usize),
    OutputWouldExpand,
    /// The declared output would exceed the caller's connection-message budget.
    OutputLimit {
        declared: usize,
        maximum: usize,
    },
    /// A length prefix, tree path or literal extends beyond the received block.
    TruncatedInput,
}

impl fmt::Display for AdaptiveHuffmanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge(length) => {
                write!(
                    formatter,
                    "adaptive Huffman input is too large: {length} bytes"
                )
            }
            Self::OutputWouldExpand => {
                formatter.write_str("adaptive Huffman output would exceed its input size")
            }
            Self::OutputLimit { declared, maximum } => write!(
                formatter,
                "adaptive Huffman output declares {declared} bytes, limit is {maximum}"
            ),
            Self::TruncatedInput => formatter.write_str("truncated adaptive Huffman input"),
        }
    }
}

impl Error for AdaptiveHuffmanError {}

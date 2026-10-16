//! Server-side connection-block decoding; shares the encoder's adaptive tree.
use super::{AdaptiveHuffmanError, AdaptiveTree, INTERNAL_NODE, NYT};

/// Decode the adaptive Huffman block following a legacy `connect ` header.
///
/// Matches multiplayer OpenJK `Huff_Decompress` for complete, valid blocks.
/// The two-byte big-endian length is checked against `maximum_output_bytes`
/// before allocating. Truncated input returns an error instead of reading past
/// the packet or returning partial userinfo. Padding after the declared symbols
/// is ignored, as in the reference. Bytes (including non-UTF-8 names) are retained.
///
/// This codec is only for connection establishment, not sequenced gameplay
/// messages. It allocates output and a bounded adaptive tree per invocation.
pub fn decompress_connect_block(
    input: &[u8],
    maximum_output_bytes: usize,
) -> Result<Vec<u8>, AdaptiveHuffmanError> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let prefix = input.get(..2).ok_or(AdaptiveHuffmanError::TruncatedInput)?;
    let length = usize::from(u16::from_be_bytes([prefix[0], prefix[1]]));
    if length > maximum_output_bytes {
        return Err(AdaptiveHuffmanError::OutputLimit {
            declared: length,
            maximum: maximum_output_bytes,
        });
    }
    let mut output = Vec::with_capacity(length);
    let mut tree = AdaptiveTree::new();
    let mut bits = InputBits {
        input,
        position: 16,
    };
    for _ in 0..length {
        let symbol = tree.receive(&mut bits)?;
        output.push(symbol);
        tree.add_reference(usize::from(symbol));
    }
    Ok(output)
}

struct InputBits<'a> {
    input: &'a [u8],
    position: usize,
}

impl InputBits<'_> {
    fn next(&mut self) -> Result<u8, AdaptiveHuffmanError> {
        let byte = self
            .input
            .get(self.position >> 3)
            .ok_or(AdaptiveHuffmanError::TruncatedInput)?;
        let bit = (byte >> (self.position & 7)) & 1;
        self.position += 1;
        Ok(bit)
    }
}

impl AdaptiveTree {
    fn receive(&self, bits: &mut InputBits<'_>) -> Result<u8, AdaptiveHuffmanError> {
        let mut node = &self.nodes[self.root];
        while node.symbol == INTERNAL_NODE {
            let next = if bits.next()? == 0 {
                node.left
            } else {
                node.right
            };
            // The tree is constructed locally; every internal node has two children.
            node = &self.nodes[next.expect("adaptive internal node has both children")];
        }
        if node.symbol == NYT {
            let mut symbol = 0;
            for _ in 0..8 {
                symbol = (symbol << 1) | bits.next()?;
            }
            Ok(symbol)
        } else {
            Ok(node.symbol as u8)
        }
    }
}

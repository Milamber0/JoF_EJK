//! Reading compiled script files (`CBlockStream`, `BlockStream.cpp`).
//!
//! A file is the four bytes `IBI\0`, the version as a little-endian float, then blocks:
//! `int id, int member count, byte flags`, then each member `int id, int size, bytes`.
//! A `random` member is read as the reference reads it: its size is taken to be four
//! bytes whatever the file says, and its value starts at "infinite" so that a `wait`
//! draws its time the first time it runs.

use std::sync::Arc;

use crate::block::{Block, Member};
use crate::ids::{IBI_VERSION, ID_RANDOM, INFINITE};

/// A script file being read, block by block (`CBlockStream`).
#[derive(Clone, Debug)]
pub struct BlockStream {
    data: Arc<[u8]>,
    position: usize,
}

impl BlockStream {
    /// Opens a file's bytes (`CBlockStream::Open`): `None` unless the header and version
    /// are right. The bytes are shared, not copied: the script cache keeps them.
    pub fn open(data: Arc<[u8]>) -> Option<Self> {
        if data.len() < 8 || &data[..4] != b"IBI\0" {
            return None;
        }
        let version = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        (version == IBI_VERSION).then_some(Self { data, position: 8 })
    }

    /// A stream with no blocks: what a file that failed to open leaves behind.
    pub fn empty() -> Self {
        Self {
            data: Arc::from(&[][..]),
            position: 0,
        }
    }

    /// Whether a block is left (`BlockAvailable`).
    pub fn block_available(&self) -> bool {
        self.position < self.data.len()
    }

    fn int(&mut self) -> Option<i32> {
        let bytes = self.data.get(self.position..self.position + 4)?;
        self.position += 4;
        Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// The next block (`ReadBlock`). A block the reference would not read — none left, a
    /// negative member count — comes back as an empty block of id 0, which is what the
    /// sequencer then finds in its fresh `CBlock`. A file that ends inside a block (the
    /// reference reads past its buffer) ends the stream the same way.
    pub fn read_block(&mut self) -> Block {
        let mut block = Block::new(0);
        if !self.block_available() {
            return block;
        }
        let (Some(id), Some(count), Some(&flags)) =
            (self.int(), self.int(), self.data.get(self.position))
        else {
            self.position = self.data.len();
            return block;
        };
        self.position += 1;
        if count < 0 {
            return block;
        }
        let mut members = Vec::with_capacity(count as usize);
        for _ in 0..count {
            match self.member() {
                Some(member) => members.push(member),
                None => {
                    self.position = self.data.len();
                    return block;
                }
            }
        }
        block.id = id;
        block.flags = flags;
        block.members = members;
        block
    }

    fn member(&mut self) -> Option<Member> {
        let id = self.int()?;
        if id == ID_RANDOM {
            // `ReadMember`: the size field is skipped and four bytes assumed.
            self.int()?;
            self.data.get(self.position..self.position + 4)?;
            self.position += 4;
            return Some(Member::float(ID_RANDOM, INFINITE));
        }
        let size = usize::try_from(self.int()?).ok()?;
        let data = self.data.get(self.position..self.position + size)?.to_vec();
        self.position += size;
        Some(Member { id, data })
    }
}

/// Every block of a file, for tools and tests; `None` if the header is wrong.
pub fn read_all(data: &[u8]) -> Option<Vec<Block>> {
    let mut stream = BlockStream::open(Arc::from(data))?;
    let mut blocks = Vec::new();
    while stream.block_available() {
        blocks.push(stream.read_block());
    }
    Some(blocks)
}

//! Blocks and their members (`CBlock`, `CBlockMember`).
//!
//! A block is one command of a script: an id (`ID_SET`, `ID_WAIT`, ...), flags, and
//! members, each an id and raw bytes — a float, an int, a string with its terminating
//! zero. The sequencer appends members to blocks as it routes them (the ids of the
//! sequences a `loop`, `if` or `affect` enters), and a `wait(random(...))` stores the
//! time it drew in its first member.

use std::borrow::Cow;

/// Set on an `if` block that has an `else` (`BF_ELSE`).
pub const BF_ELSE: u8 = 1;

/// One member of a block: an id and its bytes (`CBlockMember`).
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    /// What the member holds (`TK_FLOAT`, `TK_STRING`, `ID_GET`, ...).
    pub id: i32,
    /// Its bytes, as the block file stored them.
    pub data: Vec<u8>,
}

impl Member {
    /// A float member, as `CBlock::Write(id, float)` makes one.
    pub fn float(id: i32, value: f32) -> Self {
        Self {
            id,
            data: value.to_le_bytes().to_vec(),
        }
    }

    /// The member's bytes read as a float (`*(float *) GetData()`); zero if it holds
    /// fewer than four bytes.
    pub fn as_f32(&self) -> f32 {
        self.data.get(..4).map_or(0.0, |bytes| {
            f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    }

    /// The member's bytes read as an int (`*(int *) GetData()`); zero if it holds fewer
    /// than four bytes.
    pub fn as_i32(&self) -> i32 {
        self.data.get(..4).map_or(0, |bytes| {
            i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    }

    /// The member's bytes read as a C string: up to the first zero, each byte one
    /// character (Latin-1), borrowed when the text is ASCII.
    pub fn as_str(&self) -> Cow<'_, str> {
        let end = self
            .data
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.data.len());
        latin1(&self.data[..end])
    }
}

/// Bytes as text, one character per byte.
pub fn latin1(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) if bytes.is_ascii() => Cow::Borrowed(text),
        _ => Cow::Owned(bytes.iter().map(|&byte| char::from(byte)).collect()),
    }
}

/// One command of a script (`CBlock`).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The command (`ID_SET`, `ID_WAIT`, ...).
    pub id: i32,
    /// `BF_ELSE`, or nothing.
    pub flags: u8,
    /// Its members, in order.
    pub members: Vec<Member>,
}

impl Block {
    /// An empty block with this id (`CBlock::Create`).
    pub fn new(id: i32) -> Self {
        Self {
            id,
            flags: 0,
            members: Vec::new(),
        }
    }

    /// Appends a float member (`CBlock::Write(id, float)`).
    pub fn write_f32(&mut self, id: i32, value: f32) {
        self.members.push(Member::float(id, value));
    }

    /// The member at `index`, if there is one (`GetMember`).
    pub fn member(&self, index: usize) -> Option<&Member> {
        self.members.get(index)
    }

    /// The id of the member at `index`, if there is one.
    pub fn member_id(&self, index: usize) -> Option<i32> {
        self.members.get(index).map(|member| member.id)
    }

    /// The member at `index` read as a float; zero without one (the reference reads a
    /// null pointer there).
    pub fn f32_at(&self, index: usize) -> f32 {
        self.members.get(index).map_or(0.0, Member::as_f32)
    }

    /// The member at `index` read as an int; zero without one.
    pub fn i32_at(&self, index: usize) -> i32 {
        self.members.get(index).map_or(0, Member::as_i32)
    }

    /// The member at `index` read as a C string; empty without one.
    pub fn str_at(&self, index: usize) -> Cow<'_, str> {
        self.members
            .get(index)
            .map_or(Cow::Borrowed(""), Member::as_str)
    }

    /// Whether `BF_ELSE` is set.
    pub fn has_else(&self) -> bool {
        self.flags & BF_ELSE != 0
    }
}

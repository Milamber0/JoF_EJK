//! The engine's shared buffer with the game (`sv.mSharedMemory`), as far as the
//! interpreter can see it.
//!
//! The reference passes every `GVM_ICARUS_*` call's arguments through one buffer, and a
//! `get(STRING, ...)` answer is a pointer *into* it. Every call lays its arguments over
//! the same bytes, so what a script read with `get(STRING, ...)` is overwritten by the
//! next call that writes there: a later `get` of any kind, or the value a `set`, `play`
//! or `sound` hands over. The interpreter reads that pointer after such calls in a few
//! places (a `set` whose name and value are both `get`s, a condition comparing two), so
//! the part of the buffer the answer lives in is kept here, byte for byte.

use std::borrow::Cow;

use crate::block::latin1;

/// The 2048 bytes at offset 2056 of the shared buffer: `T_G_ICARUS_GETSTRING::value`,
/// and where `set`'s value, `play`'s name, `sound`'s channel and the `value`/`info`
/// floats of `get(FLOAT)`, `get(VECTOR)` and `tag` lie.
#[derive(Clone, Debug)]
pub(crate) struct SharedText {
    bytes: Box<[u8; 2048]>,
}

impl Default for SharedText {
    fn default() -> Self {
        Self {
            bytes: Box::new([0; 2048]),
        }
    }
}

impl SharedText {
    /// `strcpy` of a string there (cut to fit, where the reference would overrun).
    pub fn write_str(&mut self, text: &str) {
        let mut length = 0;
        for character in text.chars().take(2047) {
            self.bytes[length] = u32::from(character).min(255) as u8;
            length += 1;
        }
        self.bytes[length] = 0;
    }

    /// Floats written there, as `get(FLOAT)`, `get(VECTOR)` and `tag` write them.
    pub fn write_floats(&mut self, values: &[f32]) {
        for (index, value) in values.iter().enumerate() {
            self.bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }

    /// The bytes read as a C string.
    pub fn text(&self) -> Cow<'_, str> {
        let end = self
            .bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.bytes.len());
        latin1(&self.bytes[..end])
    }
}

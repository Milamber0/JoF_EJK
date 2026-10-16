//! Change notifications owned by command appliers, independent of `GameState`.

use crate::MAX_CONFIGSTRINGS;

/// Fixed storage for deduplicated configstring indices awaiting consumption.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigStringDirty {
    words: [u64; MAX_CONFIGSTRINGS.div_ceil(64)],
}

impl ConfigStringDirty {
    /// Mark a validated configstring index. Out-of-range indices are ignored.
    pub fn mark(&mut self, index: usize) {
        if index < MAX_CONFIGSTRINGS {
            self.words[index / 64] |= 1 << (index % 64);
        }
    }

    /// Notify every slot, including removed strings after a gamestate replacement.
    pub fn mark_all(&mut self) {
        self.words.fill(u64::MAX);
        let remainder = MAX_CONFIGSTRINGS % 64;
        if remainder != 0 {
            *self.words.last_mut().expect("nonempty configstring table") = (1 << remainder) - 1;
        }
    }

    /// Whether no notifications are pending.
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    /// Consume each pending index once, in ascending order, without allocation.
    pub fn drain(&mut self, mut visit: impl FnMut(usize)) {
        std::mem::take(self).visit(&mut visit);
    }

    /// Visit pending indices without consuming another consumer's notifications.
    pub fn visit(&self, mut visit: impl FnMut(usize)) {
        for (word_index, word) in self.words.iter().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                visit(word_index * 64 + bit);
            }
        }
    }
}

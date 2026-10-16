//! Intern immutable candidate lists without changing their order or contents.
//! Hash matches are checked against the complete list, so collisions cannot merge
//! distinct lighting. The pool is used only while a map is being prepared.
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};
struct Entry {
    first: u32,
    count: usize,
    next: Option<usize>,
}
pub(super) struct Lists {
    data: Vec<u32>,
    heads: Option<HashMap<u64, usize>>,
    entries: Vec<Entry>,
}
impl Lists {
    pub fn new(capacity: usize, deduplicate: bool) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            heads: deduplicate.then(HashMap::new),
            entries: Vec::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    fn hash(values: &[u32]) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        values.hash(&mut hash);
        hash.finish()
    }
    fn find(&self, hash: u64, values: &[u32]) -> Option<u32> {
        let mut next = self.heads.as_ref()?.get(&hash).copied();
        while let Some(index) = next {
            let e = &self.entries[index];
            if e.count == values.len()
                && self.data[e.first as usize..e.first as usize + e.count] == *values
            {
                return Some(e.first);
            }
            next = e.next;
        }
        None
    }
    pub fn insert(&mut self, values: &[u32]) -> u32 {
        if values.is_empty() {
            return 0;
        }
        let hash = self.heads.as_ref().map(|_| Self::hash(values));
        if let Some(first) = hash.and_then(|hash| self.find(hash, values)) {
            return first;
        }
        let first = self.data.len() as u32;
        self.data.extend_from_slice(values);
        if let (Some(heads), Some(hash)) = (&mut self.heads, hash) {
            let index = self.entries.len();
            let next = heads.insert(hash, index);
            self.entries.push(Entry {
                first,
                count: values.len(),
                next,
            });
        }
        first
    }
    /// Extra storage for one root's at most 64 child lists, including duplicates
    /// within that root. Scratch is stack-bound and no lists are copied.
    pub fn additional(&self, values: &[u32], children: &[[u32; 2]]) -> usize {
        if self.heads.is_none() {
            return values.len();
        }
        debug_assert!(children.len() <= 64);
        let mut known = [(0u64, 0usize, 0usize); 64];
        let mut used = 0;
        let mut extra = 0;
        for &[start, count] in children {
            let (start, count) = (start as usize, count as usize);
            if count == 0 {
                continue;
            }
            let run = &values[start..start + count];
            let hash = Self::hash(run);
            if self.find(hash, run).is_some()
                || known[..used]
                    .iter()
                    .any(|&(h, s, n)| h == hash && n == count && values[s..s + n] == *run)
            {
                continue;
            }
            known[used] = (hash, start, count);
            used += 1;
            extra += count;
        }
        extra
    }

    pub fn finish(self) -> Vec<u32> {
        self.data
    }
}

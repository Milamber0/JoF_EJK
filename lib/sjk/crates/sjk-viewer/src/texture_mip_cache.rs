//! Bounded reuse of immutable CPU mip chains across map installs.
//!
//! Keys refer to decoded image identities, not filenames. The image cache checks
//! mounted content before returning those identities; replacements cannot reuse an
//! old chain. Entries retain their source Arcs so pointer addresses cannot be recycled.
use image::RgbaImage;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

const MAX_BYTES: u64 = 128 << 20;
const MAX_ENTRIES: usize = 1_024;
type Chains = Arc<Vec<Vec<RgbaImage>>>;
type Key = (u32, u32, Vec<usize>);

struct Entry {
    _images: Vec<Arc<RgbaImage>>,
    chains: Chains,
    bytes: u64,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<Key, Entry>,
    bytes: u64,
    clock: u64,
}

static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

/// Return the unchanged resize algorithm's result, retaining at most 128 MiB.
/// Called only when loading textures; filtering and rendering never take this lock.
pub(crate) fn prepare(images: &[Arc<RgbaImage>], width: u32, height: u32) -> Chains {
    let cache = CACHE.get_or_init(|| Mutex::new(Cache::default()));
    let key = (
        width,
        height,
        images
            .iter()
            .map(|image| Arc::as_ptr(image) as usize)
            .collect(),
    );
    {
        let mut cache = cache.lock().expect("mip cache lock");
        cache.clock = cache.clock.wrapping_add(1);
        let used = cache.clock;
        if let Some(entry) = cache.entries.get_mut(&key) {
            entry.used = used;
            return Arc::clone(&entry.chains);
        }
    }
    let chains = Arc::new(
        images
            .iter()
            .map(|image| super::chain(image, width, height))
            .collect::<Vec<_>>(),
    );
    // Include retained source storage in the budget, even when the decoded cache
    // already owns it. Active uploads may temporarily keep evicted chains alive.
    let bytes = chains
        .iter()
        .flatten()
        .map(|image| image.as_raw().len() as u64)
        .sum::<u64>()
        + images
            .iter()
            .map(|image| image.as_raw().len() as u64)
            .sum::<u64>();
    if bytes > MAX_BYTES {
        return chains;
    }
    let mut cache = cache.lock().expect("mip cache lock");
    // Another map worker may have prepared the same texture concurrently.
    if let Some(entry) = cache.entries.get(&key) {
        return Arc::clone(&entry.chains);
    }
    while cache.bytes + bytes > MAX_BYTES || cache.entries.len() >= MAX_ENTRIES {
        let Some(oldest) = cache
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        let entry = cache
            .entries
            .remove(&oldest)
            .expect("selected existing entry");
        cache.bytes -= entry.bytes;
    }
    cache.clock = cache.clock.wrapping_add(1);
    let used = cache.clock;
    cache.bytes += bytes;
    cache.entries.insert(
        key,
        Entry {
            _images: images.to_vec(),
            chains: Arc::clone(&chains),
            bytes,
            used,
        },
    );
    chains
}

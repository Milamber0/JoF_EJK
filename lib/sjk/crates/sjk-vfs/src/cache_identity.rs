//! Content-cache hints for immutable mounted sources, without reading asset bytes.

use super::{VfsError, VirtualFileSystem, VirtualPath};
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-unique immutable source identity and the reader's asset-size limit.
///
/// Pair with a normalized virtual path. Clones share identities; independently opened
/// mounts do not, even if their names match. Loose files deliberately have no identity:
/// their contents can change in place. Mounted archives must remain immutable until remount.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AssetCacheIdentity(u64, u64);

pub(super) fn next() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("mounted source identity exhausted")
}

impl VirtualFileSystem {
    /// Resolve an immutable source without opening or decompressing the asset.
    ///
    /// `None` means missing or mutable, not permission to use an older mount's cache.
    /// This is a cache hint only; ordinary reads still validate size and decode errors.
    pub fn asset_cache_identity(&self, path: &str) -> Result<Option<AssetCacheIdentity>, VfsError> {
        let path = VirtualPath::for_read(path)?;
        Ok(self
            .mounts
            .iter()
            .rev()
            .find(|m| m.source.contains(&path))
            .filter(|m| m.source.cacheable())
            .map(|m| AssetCacheIdentity(m.cache_identity, self.max_asset_bytes)))
    }

    /// The mounts' process-unique identities, lowest priority first. Equal lists mean
    /// the same sources mounted in the same order (clones included), so a catalog read
    /// from every file of a kind can be kept until the mounts change. Loose directories
    /// are listed too: such a catalog does not see a file edited in place before the
    /// next remount, as the game reads them once a map.
    pub fn mount_identities(&self) -> Vec<u64> {
        self.mounts.iter().map(|m| m.cache_identity).collect()
    }
}

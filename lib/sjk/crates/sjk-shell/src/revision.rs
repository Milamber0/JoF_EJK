//! Process-wide stamps for persisted state, so a saver can tell "unchanged" from
//! one cheap comparison instead of rebuilding the config text.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

/// A stamp no earlier state carried. Zero is left for "new and empty".
pub(crate) fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

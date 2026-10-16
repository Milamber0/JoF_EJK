//! Cover large probe bursts without exceeding the adapter's per-axis limit.
/// Workgroup rows for one linear probe window; shader padding is rejected.
pub(super) fn grid(count: u32, limit: u32) -> [u32; 2] {
    let width = count.min(limit).max(1);
    let height = count.div_ceil(width);
    assert!(
        height <= limit,
        "probe dispatch exceeds two-dimensional capacity"
    );
    [width, height]
}

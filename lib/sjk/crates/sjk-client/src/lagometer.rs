//! Allocation-free lagometer samples from `codemp/cgame/cg_draw.c:4206-4260`.

/// Number of samples retained by the legacy graph.
pub const LAG_SAMPLES: usize = 128;

/// One snapshot latency sample and its protocol snapshot flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SnapshotSample {
    /// Round-trip latency in milliseconds, or `-1` for a missing snapshot.
    pub ping: i32,
    /// Protocol `snapFlags`; bit zero is `SNAPFLAG_RATE_DELAYED`.
    pub flags: u8,
}

/// Fixed rings used by the modern HUD's renderer-neutral lag graph.
#[derive(Clone)]
pub struct LagometerSamples {
    frame: [i32; LAG_SAMPLES],
    snapshots: [SnapshotSample; LAG_SAMPLES],
    frame_count: u64,
    snapshot_count: u64,
    last_sequence: Option<i32>,
}

impl LagometerSamples {
    pub const fn new() -> Self {
        Self {
            frame: [0; LAG_SAMPLES],
            snapshots: [SnapshotSample { ping: 0, flags: 0 }; LAG_SAMPLES],
            frame_count: 0,
            snapshot_count: 0,
            last_sequence: None,
        }
    }

    /// Add `cg.time - cg.latestSnapshotTime` as in `CG_AddLagometerFrameInfo`.
    pub fn add_frame(&mut self, frame_delta: i32) {
        self.frame[self.frame_count as usize & (LAG_SAMPLES - 1)] = frame_delta;
        self.frame_count += 1;
    }

    /// Add a received snapshot ping and `snapFlags`.
    pub fn add_snapshot(&mut self, ping: i32, flags: u8) {
        let index = self.snapshot_count as usize & (LAG_SAMPLES - 1);
        self.snapshots[index] = SnapshotSample { ping, flags };
        self.snapshot_count += 1;
    }

    /// Add codemp's `-1` marker for a dropped snapshot.
    pub fn add_dropped(&mut self) {
        self.add_snapshot(-1, 0);
    }

    /// Add sequence gaps as dropped samples before the received snapshot.
    pub fn add_received(&mut self, sequence: i32, ping: i32, flags: u8) {
        if let Some(previous) = self.last_sequence {
            let missing = sequence.wrapping_sub(previous).saturating_sub(1);
            for _ in 0..missing.min(LAG_SAMPLES as i32) {
                self.add_dropped();
            }
        }
        self.last_sequence = Some(sequence);
        self.add_snapshot(ping, flags);
    }

    /// Return a frame sample, where offset zero is the newest.
    pub fn frame_newest(&self, offset: usize) -> i32 {
        ring_get(&self.frame, self.frame_count, offset)
            .copied()
            .unwrap_or(0)
    }

    /// Return a snapshot sample, where offset zero is the newest.
    pub fn snapshot_newest(&self, offset: usize) -> SnapshotSample {
        ring_get(&self.snapshots, self.snapshot_count, offset)
            .copied()
            .unwrap_or_default()
    }
}

impl Default for LagometerSamples {
    fn default() -> Self {
        Self::new()
    }
}

/// Presentation threshold requested for the modern interrupted-connection badge.
///
/// Codemp's authoritative test instead compares the oldest unacknowledged
/// usercmd with `snap.ps.commandTime` (`cg_draw.c:4262-4292`).
pub const fn connection_interrupted(now: i32, latest_snapshot_time: i32) -> bool {
    now.wrapping_sub(latest_snapshot_time) > 500
}

fn ring_get<T>(ring: &[T; LAG_SAMPLES], count: u64, offset: usize) -> Option<&T> {
    if offset >= LAG_SAMPLES || offset as u64 >= count {
        return None;
    }
    let index = count.wrapping_sub(1 + offset as u64) as usize & (LAG_SAMPLES - 1);
    Some(&ring[index])
}

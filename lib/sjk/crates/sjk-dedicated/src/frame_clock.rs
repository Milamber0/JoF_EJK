//! Fixed simulation ticks, independent of UDP wakeups and operating-system jitter.
//! The wall clock still drives transport timeouts. Game/snapshot time advances
//! only by complete simulation steps, as in codemp `SV_Frame`.

/// Accumulates elapsed wall time without passing fractional frames to gameplay.
#[derive(Debug, Default)]
pub struct FrameClock {
    wall_millis: i32,
    residual: i64,
    server_millis: i32,
}

impl FrameClock {
    /// Account for a monotonic wall-clock observation. Repeated observations add no time.
    pub fn observe(&mut self, wall_millis: i32) {
        self.residual += i64::from(wall_millis.wrapping_sub(self.wall_millis).max(0));
        self.wall_millis = wall_millis;
    }

    /// Time of the last complete simulation tick, also used between ticks.
    pub fn server_time(&self) -> i32 {
        self.server_millis
    }

    /// Whether a whole frame is ready. Frame lengths must be positive.
    pub fn ready(&self, frame_millis: i32) -> bool {
        self.residual >= i64::from(frame_millis.max(1))
    }

    /// Consume one complete tick. Call again to catch up after a delayed wakeup.
    pub fn advance(&mut self, frame_millis: i32) -> Option<i32> {
        let frame_millis = frame_millis.max(1);
        if !self.ready(frame_millis) {
            return None;
        }
        self.residual -= i64::from(frame_millis);
        self.server_millis = self.server_millis.wrapping_add(frame_millis);
        Some(self.server_millis)
    }

    /// A stopped server does not accumulate a gameplay backlog before the next map.
    pub fn discard_pending(&mut self) {
        self.residual = 0;
    }
}

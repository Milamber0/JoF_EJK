//! Wind zones (`CWindZone`, rd-vanilla `tr_WorldEffects.cpp:278-355`): a constant wind,
//! or one that picks a random target velocity, eases towards it and sometimes drops to
//! nothing for a while. All zones are global; their velocities add up.
//!
//! The reference updates every rendered frame, so its gusts last 1000 to 3000 *frames*
//! and ease by 10 units per frame. SJK steps them at a fixed [`TICK_RATE`] instead, so a
//! gust lasts the same time at any frame rate (16 to 50 s).

/// Wind updates per second.
pub(crate) const TICK_RATE: f32 = 60.0;
/// Most updates one frame runs (a long stall does not replay minutes of wind).
const MAX_TICKS: u32 = 8;

/// One wind zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindZone {
    /// `mRVelocity`: where gust targets are picked from.
    velocity_range: [[f32; 3]; 2],
    /// `mRDuration`, `mRDeadTime`: in updates.
    duration: [i32; 2],
    dead_time: [i32; 2],
    /// `mMaxDeltaVelocityPerUpdate`.
    max_delta: f32,
    /// `mChanceOfDeadTime`.
    dead_chance: f32,
    current: [f32; 3],
    target: [f32; 3],
    /// `mTargetVelocityTimeRemaining`: updates left on this target; -1 never changes.
    remaining: i32,
}

impl WindZone {
    /// `wind`: `CWindZone::Initialize`'s defaults.
    pub(crate) fn basic() -> Self {
        Self {
            velocity_range: [[-1500.0, -1500.0, -10.0], [1500.0, 1500.0, 10.0]],
            duration: [1000, 2000],
            dead_time: [1000, 3000],
            max_delta: 10.0,
            dead_chance: 0.3,
            current: [0.0; 3],
            target: [0.0; 3],
            remaining: 0,
        }
    }

    /// `constantwind ( x y z )`: blows at `velocity` for good.
    pub(crate) fn constant(velocity: [f32; 3]) -> Self {
        Self {
            current: velocity,
            remaining: -1,
            ..Self::basic()
        }
    }

    /// `gustingwind`: stronger, shorter gusts with more calm between them.
    pub(crate) fn gusting() -> Self {
        Self {
            velocity_range: [[-3000.0, -3000.0, -100.0], [3000.0, 3000.0, 100.0]],
            duration: [1000, 3000],
            dead_time: [2000, 4000],
            dead_chance: 0.5,
            ..Self::basic()
        }
    }

    /// The velocity it blows at now.
    #[cfg(test)]
    pub(crate) fn current(&self) -> [f32; 3] {
        self.current
    }

    /// `CWindZone::Update`: one update.
    fn update(&mut self, random: &mut Random) {
        if self.remaining == 0 {
            if random.unit() < self.dead_chance {
                self.remaining = random.int(self.dead_time);
                self.target = [0.0; 3];
            } else {
                self.remaining = random.int(self.duration);
                let [low, high] = self.velocity_range;
                self.target = std::array::from_fn(|axis| random.range(low[axis], high[axis]));
            }
        } else if self.remaining != -1 {
            self.remaining -= 1;
            let delta: [f32; 3] =
                std::array::from_fn(|axis| self.target[axis] - self.current[axis]);
            let length = delta.iter().map(|value| value * value).sum::<f32>().sqrt();
            if length > 0.0 {
                let step = length.min(self.max_delta) / length;
                for (current, delta) in self.current.iter_mut().zip(delta) {
                    *current += delta * step;
                }
            }
        }
    }
}

/// Every wind zone and the fixed-rate clock that steps them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Wind {
    pending: f32,
    random: Random,
}

impl Wind {
    /// Advance `zones` by `seconds` and return the global wind (their sum).
    pub(crate) fn advance(&mut self, zones: &mut [WindZone], seconds: f32) -> [f32; 3] {
        self.pending = (self.pending + seconds.max(0.0)).min(MAX_TICKS as f32 / TICK_RATE);
        while self.pending >= 1.0 / TICK_RATE {
            self.pending -= 1.0 / TICK_RATE;
            for zone in zones.iter_mut() {
                zone.update(&mut self.random);
            }
        }
        zones.iter().fold([0.0; 3], |sum, zone| {
            std::array::from_fn(|axis| sum[axis] + zone.current[axis])
        })
    }
}

/// A small xorshift generator: wind needs variety, not the C library's sequence.
#[derive(Clone, Debug)]
struct Random(u32);

impl Default for Random {
    fn default() -> Self {
        Self(0x9e37_79b9)
    }
}

impl Random {
    fn next(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.0 = value;
        value
    }

    /// In `0..1`.
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    /// `Q_irand(min, max)`: inclusive.
    fn int(&mut self, [low, high]: [i32; 2]) -> i32 {
        low + (self.next() % (high - low + 1) as u32) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_wind_never_changes_and_zones_add_up() {
        let mut zones = [
            WindZone::constant([-5000.0, 0.0, 0.0]),
            WindZone::constant([0.0, 100.0, 0.0]),
        ];
        let mut wind = Wind::default();
        for _ in 0..100 {
            assert_eq!(wind.advance(&mut zones, 0.05), [-5000.0, 100.0, 0.0]);
        }
    }

    #[test]
    fn gusts_ease_towards_their_target_at_the_reference_rate() {
        let mut zones = [WindZone::gusting()];
        let mut wind = Wind::default();
        let mut previous = wind.advance(&mut zones, 0.0);
        let mut moved = false;
        // Three simulated minutes, a frame at a time.
        for _ in 0..(180 * 125) {
            let now = wind.advance(&mut zones, 0.008);
            let step = (0..3)
                .map(|axis| (now[axis] - previous[axis]).powi(2))
                .sum::<f32>()
                .sqrt();
            // Never more than 10 units per update, at most one update per 8 ms frame.
            assert!(step <= 10.0 + 1e-3, "{step}");
            moved |= step > 0.0;
            assert!(now[0].abs() <= 3000.0 && now[2].abs() <= 100.0);
            previous = now;
        }
        assert!(moved);
    }

    #[test]
    fn a_stall_runs_a_bounded_number_of_updates() {
        let mut zones = [WindZone::basic()];
        let mut wind = Wind::default();
        wind.advance(&mut zones, 0.0);
        // The first update picks a target; at most MAX_TICKS - 1 easing steps follow.
        let after = wind.advance(&mut zones, 60.0);
        let speed = after.iter().map(|value| value * value).sum::<f32>().sqrt();
        assert!(speed <= 10.0 * (MAX_TICKS - 1) as f32 + 1e-3, "{speed}");
    }
}

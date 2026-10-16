//! A value the nameplates estimate rather than read: the lowest and the highest it
//! can be, and the best guess between them. The plate draws the guess as the bar
//! and the space between the bounds as a grey haze, so a wide range reads as a
//! blurred edge and an exact one as a sharp edge.

/// An estimate with its bounds; `low <= best <= high` after every operation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Range {
    pub(super) low: f32,
    pub(super) best: f32,
    pub(super) high: f32,
}

impl Range {
    /// A value known exactly.
    pub(super) const fn exact(value: f32) -> Self {
        Self {
            low: value,
            best: value,
            high: value,
        }
    }

    /// A range from its parts, put back in order.
    pub(super) fn new(low: f32, best: f32, high: f32) -> Self {
        Self { low, best, high }.ordered()
    }

    /// Add a change known only within `low..=high` (best guess `best`): the low
    /// bound takes the smallest change and the high bound the largest.
    pub(super) fn add(self, low: f32, best: f32, high: f32) -> Self {
        Self {
            low: self.low + low,
            best: self.best + best,
            high: self.high + high,
        }
        .ordered()
    }

    /// Apply `f`, which must never decrease (a cap, a decay, a pickup), to each part.
    pub(super) fn map(self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            low: f(self.low),
            best: f(self.best),
            high: f(self.high),
        }
        .ordered()
    }

    /// Keep every part within `min..=max`.
    pub(super) fn clamp(self, min: f32, max: f32) -> Self {
        self.map(|value| value.clamp(min, max))
    }

    /// Raise every part to at least `floor`: something seen proves the value was
    /// at least that much.
    pub(super) fn at_least(self, floor: f32) -> Self {
        self.map(|value| value.max(floor))
    }

    /// Lower every part to at most `ceiling`.
    pub(super) fn at_most(self, ceiling: f32) -> Self {
        self.map(|value| value.min(ceiling))
    }

    /// The range as shares of `full`, each within `0..=1`.
    pub(super) fn share(self, full: f32) -> Self {
        if full <= 0.0 {
            return Self::default();
        }
        self.map(|value| (value / full).clamp(0.0, 1.0))
    }

    /// The range as shares of `full`, each within `0..=2`: health and armour go to
    /// twice the maximum (overheal, a large shield on a full one), drawn as a second
    /// layer over the bar.
    pub(super) fn share_over(self, full: f32) -> Self {
        if full <= 0.0 {
            return Self::default();
        }
        self.map(|value| (value / full).clamp(0.0, 2.0))
    }

    /// How far apart the bounds are.
    pub(super) fn width(self) -> f32 {
        self.high - self.low
    }

    /// This estimate, made when it was `then` at an earlier time, re-based on `value`,
    /// what it really was then (a player's shared vitals): the change since then is
    /// kept, and the uncertainty the bounds had then is taken off theirs.
    pub(super) fn rebased(self, then: Self, value: f32) -> Self {
        let best = value + (self.best - then.best);
        let below = ((self.best - self.low) - (then.best - then.low)).max(0.0);
        let above = ((self.high - self.best) - (then.high - then.best)).max(0.0);
        Self::new(best - below, best, best + above)
    }

    fn ordered(self) -> Self {
        let low = self.low.min(self.high);
        let high = self.low.max(self.high);
        Self {
            low,
            best: self.best.clamp(low, high),
            high,
        }
    }
}

/// Snapshots of an estimate kept to re-base it on a value known for one of them.
pub(super) const HISTORY: usize = 64;

/// The last [`HISTORY`] values of one estimate, by server time (milliseconds), so a
/// value shared from an earlier snapshot (`PROTOCOL.md` "Vitals", up to about a second
/// old) can correct it ([`History::anchor`]).
#[derive(Clone, Copy, Debug)]
pub(super) struct History {
    entries: [(i32, Range); HISTORY],
    next: usize,
    len: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: [(0, Range::default()); HISTORY],
            next: 0,
            len: 0,
        }
    }
}

impl History {
    /// Record the estimate at `time`; times must come in order.
    pub(super) fn push(&mut self, time: i32, range: Range) {
        self.entries[self.next] = (time, range);
        self.next = (self.next + 1) % HISTORY;
        self.len = (self.len + 1).min(HISTORY);
    }

    /// Forget every entry (a death, a respawn: older values say nothing of now).
    pub(super) fn clear(&mut self) {
        self.len = 0;
    }

    fn kept(&self) -> impl Iterator<Item = &(i32, Range)> {
        let start = (self.next + HISTORY - self.len) % HISTORY;
        (0..self.len).map(move |at| &self.entries[(start + at) % HISTORY])
    }

    /// The estimate as it was at `time`: the last entry not after it. `None` when
    /// every entry is after it (too old, or from before a reset).
    pub(super) fn at(&self, time: i32) -> Option<Range> {
        self.kept()
            .filter(|(at, _)| *at <= time)
            .last()
            .map(|(_, range)| *range)
    }

    /// Re-base `current` and every entry from `time` on `value`, what the estimate
    /// really was at `time`; false (nothing changed) when no entry is that old.
    pub(super) fn anchor(&mut self, time: i32, value: f32, current: &mut Range) -> bool {
        let Some(then) = self.at(time) else {
            return false;
        };
        let start = (self.next + HISTORY - self.len) % HISTORY;
        for at in 0..self.len {
            let entry = &mut self.entries[(start + at) % HISTORY];
            if entry.0 >= time {
                entry.1 = entry.1.rebased(then, value);
            }
        }
        *current = current.rebased(then, value);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_widen_the_bounds_and_keep_the_guess_inside() {
        let range = Range::exact(100.0).add(-50.0, -20.0, -5.0);
        assert_eq!(range, Range::new(50.0, 80.0, 95.0));
        assert_eq!(range.width(), 45.0);
        // A guess pushed past a bound is held at it.
        let held = Range {
            low: 10.0,
            best: 20.0,
            high: 20.0,
        };
        assert_eq!(Range::new(10.0, 40.0, 20.0), held);
        assert_eq!(Range::new(30.0, 5.0, 20.0).best, 20.0);
    }

    #[test]
    fn a_value_known_earlier_keeps_the_change_since_and_sheds_the_old_doubt() {
        // Then: 60..100, guess 80; since: a hit of 20 to 30 (guess 25).
        let then = Range::new(60.0, 80.0, 100.0);
        let now = then.add(-30.0, -25.0, -20.0);
        // It was really 70: now 45, with only the hit's doubt left.
        assert_eq!(now.rebased(then, 70.0), Range::new(40.0, 45.0, 50.0));
        // Nothing since: exact.
        assert_eq!(then.rebased(then, 70.0), Range::exact(70.0));
    }

    #[test]
    fn the_history_anchors_from_the_entry_at_the_time_shared() {
        let mut history = History::default();
        let mut current = Range::new(0.0, 50.0, 100.0);
        assert!(!history.anchor(100, 70.0, &mut current));
        history.push(100, current);
        current = current.add(-10.0, -10.0, -10.0);
        history.push(200, current);
        assert_eq!(history.at(150), Some(Range::new(0.0, 50.0, 100.0)));
        assert_eq!(history.at(99), None);
        assert!(history.anchor(150, 70.0, &mut current));
        assert_eq!(current, Range::exact(60.0));
        // The entries since were corrected too, so a later value is read alike.
        assert_eq!(history.at(200), Some(Range::exact(60.0)));
        assert!(history.anchor(200, 55.0, &mut current));
        assert_eq!(current, Range::exact(55.0));
        // It keeps the last HISTORY entries.
        for time in 0..HISTORY as i32 {
            history.push(1_000 + time, current);
        }
        assert_eq!(history.at(999), None);
        history.clear();
        assert_eq!(history.at(5_000), None);
    }

    #[test]
    fn floors_ceilings_and_shares() {
        let range = Range::new(0.0, 30.0, 90.0);
        assert_eq!(range.at_least(40.0), Range::new(40.0, 40.0, 90.0));
        assert_eq!(range.at_most(20.0), Range::new(0.0, 20.0, 20.0));
        assert_eq!(range.clamp(10.0, 50.0), Range::new(10.0, 30.0, 50.0));
        let share = Range::new(25.0, 50.0, 150.0).share(100.0);
        assert_eq!(share, Range::new(0.25, 0.5, 1.0));
        assert_eq!(range.share(0.0), Range::default());
        let over = Range::new(99.0, 150.0, 250.0).share_over(100.0);
        assert_eq!(over, Range::new(0.99, 1.5, 2.0));
    }
}

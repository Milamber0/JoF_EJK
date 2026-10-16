//! A map's camera tour behind the main menu: its authored shots
//! ([`routes::TourShot`]) played one after another, the first the map's own
//! intermission view and the rest in a shuffled order, each a slow glide
//! looking at one point, with a short fade through dark between them.

use super::Sample;
use super::routes::TourShot;
use glam::Vec3;

/// How long a shot lasts, its fades included.
const SHOT_MILLIS: u64 = 15_000;
/// The fade into and out of each shot.
pub(super) const FADE_MILLIS: u64 = 900;

/// The tour's state: the order of its shots, the one on screen and when it began.
pub(crate) struct Tour {
    shots: &'static [TourShot],
    order: Vec<usize>,
    /// Index into `order` of the shot on screen.
    index: usize,
    /// Backdrop time the shot on screen began, once the tour has started.
    began: Option<u64>,
    /// The shuffle's state.
    seed: u64,
}

impl Tour {
    /// The tour of `shots`, shuffled from `seed`, starting on the first.
    pub(crate) fn new(shots: &'static [TourShot], seed: u64) -> Self {
        let mut tour = Self {
            shots,
            order: (0..shots.len()).collect(),
            index: 0,
            began: None,
            seed: seed | 1,
        };
        tour.shuffle_after_first();
        tour
    }

    /// Shuffle every shot but the first in the order (Fisher-Yates over a
    /// xorshift), so the map's own view always opens the tour.
    fn shuffle_after_first(&mut self) {
        for slot in (2..self.order.len()).rev() {
            let pick = 1 + (self.next_random() % slot as u64) as usize;
            self.order.swap(slot, pick);
        }
    }

    fn next_random(&mut self) -> u64 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        x
    }

    /// Move on to the next shot from its start at `millis`; after the last,
    /// the order is shuffled again, never opening on the shot just shown.
    pub(crate) fn advance(&mut self, millis: u64) {
        self.began = Some(millis);
        self.index += 1;
        if self.index < self.order.len() {
            return;
        }
        let last = self.order[self.order.len() - 1];
        for slot in (1..self.order.len()).rev() {
            let pick = (self.next_random() % (slot as u64 + 1)) as usize;
            self.order.swap(slot, pick);
        }
        if self.order.len() > 1 && self.order[0] == last {
            self.order.swap(0, 1);
        }
        self.index = 0;
    }

    /// The camera at backdrop time `millis` and how dark the fade between
    /// shots makes the world (0 clear, 1 black).
    pub(crate) fn sample(&mut self, millis: u64) -> (Sample, f32) {
        let began = *self.began.get_or_insert(millis);
        if millis.saturating_sub(began) >= SHOT_MILLIS {
            self.advance(millis);
        }
        let elapsed = millis.saturating_sub(self.began.unwrap_or(millis));
        let shot = self.shots[self.order[self.index]];
        let t = elapsed as f32 / SHOT_MILLIS as f32;
        let origin = Vec3::from_array(shot.from).lerp(Vec3::from_array(shot.to), t);
        let direction = (Vec3::from_array(shot.at) - origin).normalize_or(Vec3::X);
        let sample = Sample {
            origin,
            yaw: direction.y.atan2(direction.x),
            pitch: direction.z.clamp(-1.0, 1.0).asin(),
        };
        (sample, fade(elapsed, SHOT_MILLIS))
    }

    /// The shot on screen (its index among the map's shots).
    #[cfg(test)]
    pub(crate) fn current(&self) -> usize {
        self.order[self.index]
    }
}

/// Darkness `elapsed` into something lasting `length`: fading in over its
/// first [`FADE_MILLIS`] and out over its last.
pub(super) fn fade(elapsed: u64, length: u64) -> f32 {
    let fade = FADE_MILLIS as f32;
    let into = 1.0 - elapsed as f32 / fade;
    let out = 1.0 - length.saturating_sub(elapsed) as f32 / fade;
    into.max(out).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOTS: [TourShot; 4] = [
        TourShot {
            from: [0.0, 0.0, 0.0],
            to: [100.0, 0.0, 0.0],
            at: [0.0, 1000.0, 0.0],
        },
        TourShot {
            from: [1.0, 0.0, 0.0],
            to: [1.0, 0.0, 0.0],
            at: [0.0, 1000.0, 0.0],
        },
        TourShot {
            from: [2.0, 0.0, 0.0],
            to: [2.0, 0.0, 0.0],
            at: [0.0, 1000.0, 0.0],
        },
        TourShot {
            from: [3.0, 0.0, 0.0],
            to: [3.0, 0.0, 0.0],
            at: [0.0, 1000.0, 0.0],
        },
    ];

    #[test]
    fn the_tour_opens_on_the_maps_view_and_shows_every_shot_once_a_round() {
        for seed in [1, 7, 12_345, u64::MAX] {
            let mut tour = Tour::new(&SHOTS, seed);
            assert_eq!(tour.current(), 0);
            let mut seen = vec![tour.current()];
            for round in 1..12 {
                tour.advance(round);
                seen.push(tour.current());
            }
            for round in seen.chunks(4).take(3) {
                let mut sorted = round.to_vec();
                sorted.sort_unstable();
                assert_eq!(sorted, [0, 1, 2, 3], "seed {seed}: {seen:?}");
            }
            // A new round never repeats the shot just shown.
            for pair in seen.windows(2) {
                assert_ne!(pair[0], pair[1], "seed {seed}: {seen:?}");
            }
        }
    }

    #[test]
    fn a_shot_glides_from_its_start_to_its_end_and_fades_at_both() {
        let mut tour = Tour::new(&SHOTS, 3);
        let (start, dark) = tour.sample(1_000);
        assert_eq!(start.origin, Vec3::ZERO);
        assert!((dark - 1.0).abs() < 1e-6, "it opens from black");
        let (middle, dark) = tour.sample(1_000 + SHOT_MILLIS / 2);
        assert!((middle.origin.x - 50.0).abs() < 1e-3);
        assert_eq!(dark, 0.0);
        // It looks at its point: straight along +y.
        assert!((middle.yaw - std::f32::consts::FRAC_PI_2).abs() < 0.06);
        let (_, dark) = tour.sample(1_000 + SHOT_MILLIS - FADE_MILLIS / 2);
        assert!((dark - 0.5).abs() < 0.01);
        // Then the next shot.
        let _ = tour.sample(1_000 + SHOT_MILLIS);
        assert_ne!(tour.current(), 0);
    }
}

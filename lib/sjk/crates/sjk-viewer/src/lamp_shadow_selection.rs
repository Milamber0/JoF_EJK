//! Continuous actor-shadow importance under a fixed rendering budget.
use super::SLOTS;
use crate::lamp_lights::LampSet;
use glam::Vec3;

/// Actor casters can affect receivers beyond a lamp's direct-light support.
pub(super) const EXTRA_RADIUS: f32 = 512.;

/// Rank actor-shadow slots with weights that vanish continuously at reassignment.
pub(super) fn select_with_candidates(
    lamps: &LampSet,
    eye: Vec3,
    candidates: Option<&super::candidates::Candidates>,
) -> [(usize, f32); SLOTS] {
    // Keep one extra competitor. A retiring source has zero weight when the next
    // source overtakes it, so reassignment cannot replace a visible shadow in one frame.
    let mut scores = [0.; SLOTS + 1];
    let mut ids = [usize::MAX; SLOTS + 1];
    let mut rank = |id: usize| {
        let lamp = &lamps.lamps[id];
        let distance = lamp.position.distance_squared(eye);
        let range = lamp.radius + EXTRA_RADIUS;
        let window = (1. - distance / (range * range)).max(0.);
        let score = lamp.power / (distance + 4096.) * window * window;
        if score <= 0. {
            return;
        }
        if let Some(at) = scores
            .iter()
            .zip(ids)
            .position(|(&s, i)| score > s || (score == s && id < i))
        {
            for i in (at + 1..=SLOTS).rev() {
                scores[i] = scores[i - 1];
                ids[i] = ids[i - 1];
            }
            scores[at] = score;
            ids[at] = id;
        }
    };
    if let Some(candidates) = candidates {
        candidates.visit(eye, &mut rank);
    } else {
        for id in 0..lamps.lamps.len() {
            rank(id);
        }
    }
    std::array::from_fn(|i| {
        let margin = ((scores[i] - scores[SLOTS]) / (scores[i] * 0.5).max(1e-8)).clamp(0., 1.);
        // Both the rank margin and absolute negligible-importance limit join smoothly.
        let strength = (scores[i] / 0.001).clamp(0., 1.);
        (ids[i], smooth(margin) * smooth(strength))
    })
}

fn smooth(x: f32) -> f32 {
    x * x * (3. - 2. * x)
}

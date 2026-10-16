//! Dynamic joints: the chains a rig model's manifest names (`dynamic <joint>`, the
//! joints under that one, such as a tail) move on their own. Each simulated joint is
//! a point pulled by a spring towards where the animation puts it, so it lags and
//! bounces when the body moves, held at its bone's length from its parent; a gentle
//! side-to-side wag, growing towards the tip, keeps it alive while the player stands,
//! and gravity, more towards the tip, makes it hang (a tail's rest pose sticks out
//! straight; the game it comes from lets physics pull it down).
//! Then each parent is turned to point at its simulated child. The state is the
//! actor's ([`super::Scratch`]); it all happens in the model's space, so the chain
//! answers the animation (steps, jumps, turns of the hips), not the player's
//! movement through the world.

use super::Joint;
use glam::{Affine3A, Mat3A, Quat, Vec3A};

/// The spring pulling a joint towards its animated place, per second squared.
const STIFFNESS: f32 = 70.0;
/// How fast the swing dies down, per second.
const DAMPING: f32 = 7.0;
/// The wag at the chain's tip, game units to each side, and how many a second.
pub(super) const WAG_UNITS: f32 = 2.5;
const WAG_HERTZ: f32 = 0.7;
/// The pull down on a chain's tip, game units per second squared, unless its
/// `dynamic` line gives one: twice the game's (`g_gravity` 800), which only levelled
/// the test model's tail (its rest pose rises 22 degrees); this hangs it about 29 degrees
/// below level, the tip some 12 units under its anchor. Less towards the anchor.
pub(super) const GRAVITY: f32 = 1600.0;
/// A frame step longer than this (a hitch, a teleport, the first frame) starts the
/// chain again at the animated pose.
const MAX_STEP: f32 = 0.1;

/// The simulated joints of a rig, parents before children.
#[derive(Clone, Debug, Default)]
pub(super) struct Chains {
    pub(super) joints: Vec<usize>,
    /// How far along its chain each one is, 0 at the anchor to 1 at the tip.
    along: Vec<f32>,
    /// Its chain's gravity, units per second squared.
    gravity: Vec<f32>,
}

impl Chains {
    /// The joints under each of `anchors` (not the anchors: they follow the
    /// animation and hold the chain), in `order`, each anchor with its chain's
    /// gravity.
    pub(super) fn under(joints: &[Joint], order: &[usize], anchors: &[(usize, f32)]) -> Self {
        let below = |mut index: usize| -> Option<(usize, f32)> {
            let mut depth = 0;
            while let Some(parent) = joints[index].parent {
                depth += 1;
                if let Some((_, gravity)) = anchors.iter().find(|(anchor, _)| *anchor == parent) {
                    return Some((depth, *gravity));
                }
                index = parent;
            }
            None
        };
        let found: Vec<(usize, usize, f32)> = order
            .iter()
            .filter_map(|&index| below(index).map(|(depth, gravity)| (index, depth, gravity)))
            .collect();
        let deepest = found.iter().map(|(_, depth, _)| *depth).max().unwrap_or(1) as f32;
        Self {
            joints: found.iter().map(|(index, _, _)| *index).collect(),
            along: found
                .iter()
                .map(|(_, depth, _)| *depth as f32 / deepest)
                .collect(),
            gravity: found.iter().map(|(_, _, gravity)| *gravity).collect(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.joints.is_empty()
    }
}

/// One actor's chain state.
#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    points: Vec<Vec3A>,
    velocities: Vec<Vec3A>,
    /// Seconds simulated, for the wag.
    time: f32,
    /// The animated pose before the chain moved it (scratch).
    animated: Vec<Affine3A>,
    aimed: Vec<bool>,
}

/// Move `chains` in `pose` by `step` seconds (`None`: start again at the animated
/// pose), wagging `wag` units at the tip.
pub(super) fn simulate(
    joints: &[Joint],
    chains: &Chains,
    pose: &mut [Affine3A],
    state: &mut State,
    step: Option<f32>,
    wag: f32,
) {
    let count = chains.joints.len();
    let step = step.filter(|step| *step > 0.0 && *step <= MAX_STEP);
    let restart = step.is_none() || state.points.len() != count;
    let step = step.unwrap_or(0.0);
    state.time += step;
    let phase = std::f32::consts::TAU * WAG_HERTZ * state.time;
    let target = |slot: usize, pose: &[Affine3A]| {
        let along = chains.along[slot];
        // Model space: +X is the player's side (-Y the front, +Z up). The tip trails
        // the base a little, so the wag runs down the chain.
        pose[chains.joints[slot]].translation + Vec3A::X * wag * along * (phase - along * 0.6).sin()
    };
    // Gravity on each point, more towards the tip.
    let weight = |slot: usize| Vec3A::NEG_Z * chains.gravity[slot] * chains.along[slot];
    if restart {
        // Already hanging: where the spring holds the weight.
        state.points = (0..count)
            .map(|slot| target(slot, pose) + weight(slot) / STIFFNESS)
            .collect();
        state.velocities = vec![Vec3A::ZERO; count];
    } else {
        for slot in 0..count {
            let pull = target(slot, pose) - state.points[slot];
            let velocity = &mut state.velocities[slot];
            *velocity += (pull * STIFFNESS - *velocity * DAMPING + weight(slot)) * step;
            state.points[slot] += *velocity * step;
        }
    }
    // Each point at its bone's length from its parent's (simulated or animated).
    for slot in 0..count {
        let index = chains.joints[slot];
        let Some(parent) = joints[index].parent else {
            continue;
        };
        let anchor = chains
            .joints
            .iter()
            .position(|&joint| joint == parent)
            .map_or(pose[parent].translation, |at| state.points[at]);
        let bone = pose[index].translation - pose[parent].translation;
        let towards = (state.points[slot] - anchor)
            .try_normalize()
            .or_else(|| bone.try_normalize())
            .unwrap_or(Vec3A::Z);
        state.points[slot] = anchor + towards * bone.length();
    }
    // Each parent turned to point at its simulated child; the children keep their
    // animated place under it.
    state.animated.clear();
    state.animated.extend_from_slice(pose);
    state.aimed.clear();
    state.aimed.resize(pose.len(), false);
    for slot in 0..count {
        let index = chains.joints[slot];
        let Some(parent) = joints[index].parent else {
            continue;
        };
        let local = state.animated[parent].inverse() * state.animated[index];
        if !state.aimed[parent] {
            let origin = pose[parent].translation;
            let now = ((pose[parent] * local).translation - origin).try_normalize();
            let wanted = (state.points[slot] - origin).try_normalize();
            if let (Some(now), Some(wanted)) = (now, wanted) {
                let turn = Mat3A::from_quat(Quat::from_rotation_arc(now.into(), wanted.into()));
                pose[parent].matrix3 = turn * pose[parent].matrix3;
            }
            state.aimed[parent] = true;
        }
        pose[index] = pose[parent] * local;
    }
}

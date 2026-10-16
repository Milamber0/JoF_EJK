//! The stage model's two animation tracks, the legs and the torso, as the game keeps
//! them, each blended between its own frames, and the takeover from one animation to
//! the next: the pose on screen when it begins, held still, blended into the new one
//! (as Ghoul2 blends from the frame it froze), eased in over [`HOVER_TAKEOVER`] or as a
//! saber move says. Nothing steps or jumps from one pose to the next (Creyon,
//! 13/10/2026: "smooth and fluent").

use crate::player_menu::force_pose::Takeover;
use glam::{Mat3, Quat, Vec3};
use sjk_model::{AnimationSequence, Gla, SplitAnimationSample};
use std::time::Instant;

pub(super) use crate::player_menu::force_pose::HOVER_TAKEOVER;

/// Steps of a fraction the stage skins at: the pose between two frames, and the share
/// a new animation has taken over.
pub(super) const STEPS: u8 = 32;

/// One half of the body's animation.
#[derive(Clone, Debug)]
pub(super) struct Track {
    pub(super) sequence: AnimationSequence,
    pub(super) started: Instant,
    /// How fast the sequence plays (1: the game's speed).
    pub(super) rate: f32,
}

impl Track {
    pub(super) fn new(sequence: AnimationSequence, now: Instant) -> Self {
        Self {
            sequence,
            started: now,
            rate: 1.0,
        }
    }

    /// Play `sequence` from its start at `now` at `rate`.
    pub(super) fn start(&mut self, sequence: AnimationSequence, now: Instant, rate: f32) {
        self.sequence = sequence;
        self.started = now;
        self.rate = rate;
    }

    /// Play on at `rate` from where the sequence is at `now`.
    pub(super) fn set_rate(&mut self, rate: f32, now: Instant) {
        let played = self.elapsed(now);
        self.rate = rate;
        if rate > 0.0 {
            self.started = now
                .checked_sub(std::time::Duration::from_secs_f32(played / rate))
                .unwrap_or(now);
        }
    }

    /// Seconds of the sequence played at `now`.
    pub(super) fn elapsed(&self, now: Instant) -> f32 {
        now.saturating_duration_since(self.started).as_secs_f32() * self.rate
    }

    /// The frame pair the track is between at `now` and the fraction between them.
    pub(super) fn sample(&self, now: Instant) -> ((usize, usize), f32) {
        let (current, next, fraction) =
            crate::animation_timing::animation_sample(&self.sequence, self.elapsed(now));
        ((current, next), fraction)
    }
}

/// The share a new animation has `seconds` into a takeover `how`: eased in and out, or
/// at an even pace as the game's.
pub(super) fn takeover(seconds: f32, how: Takeover) -> f32 {
    if how.seconds <= 0.0 {
        return 1.0;
    }
    let t = (seconds / how.seconds).clamp(0.0, 1.0);
    if how.eased {
        t * t * (3.0 - 2.0 * t)
    } else {
        t
    }
}

/// A fraction in [`STEPS`].
pub(super) fn step(fraction: f32) -> u8 {
    (fraction.clamp(0.0, 1.0) * f32::from(STEPS)).round() as u8
}

/// What the stage skins: each track's frames with their fraction, and the share the
/// new animation has taken over from the pose before it, in [`STEPS`], so a pose is
/// skinned again only when it changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PoseKey {
    lower: ((usize, usize), u8),
    upper: ((usize, usize), u8),
    weight: u8,
}

impl PoseKey {
    pub(super) fn new(
        lower: ((usize, usize), f32),
        upper: ((usize, usize), f32),
        weight: f32,
    ) -> Self {
        Self {
            lower: (lower.0, step(lower.1)),
            upper: (upper.0, step(upper.1)),
            weight: step(weight),
        }
    }

    /// The tracks' sample, fractions as stepped.
    pub(super) fn current(&self) -> SplitAnimationSample {
        let share = |steps: u8| f32::from(steps) / f32::from(STEPS);
        SplitAnimationSample {
            lower_frames: self.lower.0,
            lower_fraction: share(self.lower.1),
            upper_frames: self.upper.0,
            upper_fraction: share(self.upper.1),
        }
    }

    /// The share the new animation has of the pose, as stepped.
    pub(super) fn weight(&self) -> f32 {
        f32::from(self.weight) / f32::from(STEPS)
    }
}

/// How far two frames' bone matrices may differ, entry by entry, and still be one pose:
/// the turns' entries to about 3 degrees, the offsets to half a unit.
const SAME_TURN: f32 = 0.05;
const SAME_OFFSET: f32 = 0.5;

/// Whether frames `a` and `b` of `animation` hold the same pose, as a saber move's last
/// frame and the next move's first do (`saberMoveData` chains them end to start).
pub(super) fn poses_meet(animation: &Gla, a: usize, b: usize) -> bool {
    if a == b {
        return true;
    }
    (0..animation.bones.len()).all(|bone| {
        let (Some(first), Some(second)) = (
            animation.bone_matrix(a, bone),
            animation.bone_matrix(b, bone),
        ) else {
            return false;
        };
        first.iter().zip(&second).all(|(one, other)| {
            (0..3).all(|column| (one[column] - other[column]).abs() <= SAME_TURN)
                && (one[3] - other[3]).abs() <= SAME_OFFSET
        })
    })
}

/// One bone's local transform: its 3x3 part and its offset from the parent.
#[derive(Clone, Copy, Debug)]
struct Local {
    basis: Mat3,
    offset: Vec3,
}

fn local(matrix: [[f32; 4]; 3]) -> Local {
    let column = |index: usize| Vec3::new(matrix[0][index], matrix[1][index], matrix[2][index]);
    Local {
        basis: Mat3::from_cols(column(0), column(1), column(2)),
        offset: column(3),
    }
}

/// Between `a` and `b` by `t`, the bone turning about its joint, `pivot` (where the
/// joint stands in the model's bind pose: a GLA frame's bone matrix carries the model's
/// points, not the bone's own, so it turns them about that point).
///
/// The joint goes straight from where `a` puts it to where `b` does, as when the whole
/// matrices are blended entry by entry (Ghoul2's way, the in-game model's): the bone
/// stays on its parent. The turn about it is taken the short way at an even pace
/// ([`turn_between`]), as every GLA frame's is a plain turn (stored as a quaternion);
/// blended entries alone shrink a limb turning far between the two, and a swing's arm
/// and body folded in on themselves. Blending the turn while the matrix's offset went
/// its own straight way moved each joint off its parent between two poses far apart,
/// the farther from the model's middle the more: arms stretched out and swung through
/// the body (Creyon, 13/10/2026: "the movement of the models are broken"). A matrix
/// that is no plain turn (mirrored or scaled) gets its blended entries squared up to its
/// own axes' lengths and handedness instead.
fn blend(a: Local, b: Local, t: f32, pivot: Vec3) -> Local {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let basis = turn_between(a.basis, b.basis, t).unwrap_or_else(|| squared_up(a, b, t));
    let joint = (a.basis * pivot + a.offset).lerp(b.basis * pivot + b.offset, t);
    Local {
        basis,
        offset: joint - basis * pivot,
    }
}

/// The blended entries of `a` and `b` squared up to the axes' lengths and handedness the
/// two have.
fn squared_up(a: Local, b: Local, t: f32) -> Mat3 {
    let mixed = a.basis * (1.0 - t) + b.basis * t;
    let length =
        |axis: usize| a.basis.col(axis).length() * (1.0 - t) + b.basis.col(axis).length() * t;
    let x = mixed
        .x_axis
        .normalize_or(a.basis.x_axis.normalize_or(Vec3::X));
    let y = (mixed.y_axis - x * x.dot(mixed.y_axis)).normalize_or(x.any_orthonormal_vector());
    let mut z = x.cross(y);
    if z.dot(mixed.z_axis) < 0.0 {
        z = -z;
    }
    Mat3::from_cols(x * length(0), y * length(1), z * length(2))
}

/// How far from a plain turn a matrix may be and still be taken as one: a GLA bone's
/// quaternion is stored in 16-bit parts, so its matrix is a turn only to about 1e-4.
const TURN_TOLERANCE: f32 = 1e-2;

/// Whether `basis` is a plain turn: axes of length one, square to one another, and
/// right-handed.
fn is_turn(basis: Mat3) -> bool {
    let unit = |axis: Vec3| (axis.length_squared() - 1.0).abs() < TURN_TOLERANCE;
    unit(basis.x_axis)
        && unit(basis.y_axis)
        && unit(basis.z_axis)
        && basis.x_axis.dot(basis.y_axis).abs() < TURN_TOLERANCE
        && basis.y_axis.dot(basis.z_axis).abs() < TURN_TOLERANCE
        && basis.z_axis.dot(basis.x_axis).abs() < TURN_TOLERANCE
        && basis.determinant() > 0.0
}

/// The turn `t` of the way from `a` to `b` along the shortest way (a spherical blend of
/// their quaternions), or `None` when either is not a plain turn.
fn turn_between(a: Mat3, b: Mat3, t: f32) -> Option<Mat3> {
    if !is_turn(a) || !is_turn(b) {
        return None;
    }
    let from = Quat::from_mat3(&a).normalize();
    let to = Quat::from_mat3(&b).normalize();
    // `slerp` takes the shorter of the two ways round (it flips `to` when the two
    // quaternions point apart).
    Some(Mat3::from_quat(from.slerp(to, t).normalize()))
}

fn row_major(basis: Mat3, offset: Vec3) -> [[f32; 4]; 3] {
    std::array::from_fn(|row| {
        let r = basis.row(row);
        [r.x, r.y, r.z, offset[row]]
    })
}

/// The pose's bone matrices for `key`, the legs' track driving the bones outside
/// `upper_root`'s subtree and the torso's the rest, taking over from `from` (the bone
/// matrices on screen when the animation changed) by the key's weight, every
/// interpolation keeping the limbs their length ([`blend`]).
pub(super) fn pose_matrices(
    animation: &Gla,
    key: &PoseKey,
    upper_root: &str,
    from: Option<&[[[f32; 4]; 3]]>,
) -> Result<Vec<[[f32; 4]; 3]>, String> {
    let bones = &animation.bones;
    let root = bones
        .iter()
        .position(|bone| bone.name.eq_ignore_ascii_case(upper_root))
        .ok_or("no upper-body root bone")?;
    let upper = (0..bones.len())
        .map(|bone| {
            let mut at = Some(bone);
            let mut steps = 0;
            while let Some(index) = at {
                if index == root {
                    return true;
                }
                steps += 1;
                if steps > bones.len() {
                    return false;
                }
                at = bones[index].parent;
            }
            false
        })
        .collect::<Vec<_>>();
    // Each joint where the bind pose stands it ([`blend`]).
    let pivot = |bone: usize| {
        let base = bones[bone].base_pose;
        Vec3::new(base[0][3], base[1][3], base[2][3])
    };
    // A sample's pose, each bone between its track's two frames (close together, so
    // blended bone by bone in the parent's frame) and composed down the hierarchy.
    let compose = |sample: SplitAnimationSample| -> Result<Vec<Option<Local>>, String> {
        let mut world: Vec<Option<Local>> = vec![None; bones.len()];
        for start in 0..bones.len() {
            // Parents first: walk up to the first evaluated ancestor, then back down.
            let mut chain = Vec::new();
            let mut at = Some(start);
            while let Some(index) = at {
                if world[index].is_some() || chain.len() > bones.len() {
                    break;
                }
                chain.push(index);
                at = bones[index].parent;
            }
            for &bone in chain.iter().rev() {
                let (frames, fraction) = if upper[bone] {
                    (sample.upper_frames, sample.upper_fraction)
                } else {
                    (sample.lower_frames, sample.lower_fraction)
                };
                let at = |frame| {
                    animation
                        .bone_matrix(frame, bone)
                        .map(local)
                        .ok_or_else(|| format!("no bone {bone} in frame {frame}"))
                };
                let shown = blend(at(frames.0)?, at(frames.1)?, fraction, pivot(bone));
                world[bone] = Some(match bones[bone].parent.and_then(|parent| world[parent]) {
                    Some(parent) => Local {
                        basis: parent.basis * shown.basis,
                        offset: parent.basis * shown.offset + parent.offset,
                    },
                    None => shown,
                });
            }
        }
        Ok(world)
    };
    let mut world = compose(key.current())?;
    let weight = key.weight();
    if let Some(from) = from.filter(|from| weight < 1.0 && from.len() == bones.len()) {
        // A takeover blends the two whole poses bone by bone as they stand in the model,
        // each bone turning the short way from where the old pose holds it to where the
        // new one does. Blended in the parent's frame, a bone the two poses hold alike in
        // the model but reach differently (the game's swings start with the upper arm
        // turned a half turn about itself from the stance's, the forearm turned back)
        // swung the arm and saber round between them (Creyon, 13/10/2026: the saber
        // flickering into the second swing). It starts from the pose on screen, so a
        // takeover that begins while another is under way carries on from where that
        // one is rather than dropping it (Lightning's three-frame start snapped as its
        // hold took over before the start had, Creyon, 13/10/2026).
        for (bone, shown) in world.iter_mut().enumerate() {
            if let Some(new) = *shown {
                *shown = Some(blend(local(from[bone]), new, weight, pivot(bone)));
            }
        }
    }
    world
        .into_iter()
        .map(|bone| {
            bone.map(|shown| row_major(shown.basis, shown.offset))
                .ok_or_else(|| "bone hierarchy loops".to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sequence(name: &str, first_frame: usize) -> AnimationSequence {
        AnimationSequence {
            name: name.into(),
            first_frame,
            frame_count: 10,
            loop_frame: -1,
            frames_per_second: 20.0,
        }
    }

    /// A sequence plays from where it began at its speed.
    #[test]
    fn a_sequence_plays_at_its_speed() {
        let start = Instant::now();
        let mut track = Track::new(sequence("BOTH_STAND2", 0), start);
        track.start(sequence("BOTH_A3_T__B_", 100), start, 0.5);
        assert_eq!(
            track.sample(start + Duration::from_millis(200)).0,
            (102, 103)
        );
    }

    /// A sequence sped up or slowed down carries on from where it is.
    #[test]
    fn a_new_speed_carries_on_from_where_it_is() {
        let start = Instant::now();
        let mut track = Track::new(sequence("BOTH_RUN1", 0), start);
        let at = start + Duration::from_millis(300);
        let before = track.elapsed(at);
        track.set_rate(1.7, at);
        assert!((track.elapsed(at) - before).abs() < 1e-3);
        let later = at + Duration::from_millis(100);
        assert!((track.elapsed(later) - (before + 0.17)).abs() < 1e-3);
    }

    /// The takeover eases: slow at both ends, half way at half the blend.
    #[test]
    fn the_takeover_eases_in_and_out() {
        let how = HOVER_TAKEOVER;
        assert_eq!(takeover(0.0, how), 0.0);
        assert!((takeover(how.seconds / 2.0, how) - 0.5).abs() < 1e-6);
        assert!(takeover(how.seconds * 0.1, how) < 0.1);
        assert_eq!(takeover(how.seconds * 2.0, how), 1.0);
        // The game's takeover goes at an even pace.
        let game = crate::player_menu::force_pose::GAME_TAKEOVER;
        assert!((takeover(0.025, game) - 0.25).abs() < 1e-6);
        assert!((takeover(0.075, game) - 0.75).abs() < 1e-6);
    }

    /// Half way between a quarter turn and none is an eighth turn of full length: the
    /// limb keeps its size (blending entries alone would shrink it to about 0.71), and
    /// the ends are the bone's own matrices, a mirrored one included.
    #[test]
    fn a_blended_turn_keeps_its_length_and_handedness() {
        let none = local(row_major(Mat3::IDENTITY, Vec3::ZERO));
        let quarter = local(row_major(
            Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(0.0, 0.0, 2.0),
        ));
        let half = blend(none, quarter, 0.5, Vec3::ZERO);
        let arm = half.basis * Vec3::X;
        assert!((arm.length() - 1.0).abs() < 1e-5);
        assert!((arm.y - std::f32::consts::FRAC_PI_4.sin()).abs() < 1e-5);
        assert!((half.offset.z - 1.0).abs() < 1e-6);
        assert!((half.basis.determinant() - 1.0).abs() < 1e-5);
        // A mirrored bone stays mirrored between two of its poses.
        let mirror = Mat3::from_cols(-Vec3::X, Vec3::Y, Vec3::Z);
        let a = local(row_major(mirror, Vec3::ZERO));
        let b = local(row_major(Mat3::from_rotation_z(0.6) * mirror, Vec3::ZERO));
        let between = blend(a, b, 0.5, Vec3::ZERO);
        assert!((between.basis.determinant() + 1.0).abs() < 1e-4);
        // The ends are exact.
        assert_eq!(blend(a, b, 0.0, Vec3::ZERO).basis, a.basis);
        assert_eq!(blend(a, b, 1.0, Vec3::ZERO).basis, b.basis);
    }

    /// Between two poses far apart, a bone turns the short way at an even pace: a
    /// little short of a half turn is crossed through its middle, never folding back
    /// or flipping to the other side, and every step is the same size.
    #[test]
    fn a_far_turn_goes_the_short_way_at_an_even_pace() {
        let angle = 170_f32.to_radians();
        let none = local(row_major(Mat3::IDENTITY, Vec3::ZERO));
        let far = local(row_major(Mat3::from_rotation_z(angle), Vec3::ZERO));
        let heading = |t: f32| {
            let arm = blend(none, far, t, Vec3::ZERO).basis * Vec3::X;
            assert!((arm.length() - 1.0).abs() < 1e-4);
            arm.y.atan2(arm.x)
        };
        let mut last = 0.0;
        for step in 1..=10 {
            let t = step as f32 / 10.0;
            let now = heading(t);
            assert!(
                (now - angle * t).abs() < 1e-3,
                "at {t} the arm points {now}, not {}",
                angle * t
            );
            assert!(now > last);
            last = now;
        }
    }

    /// A bone turning about a joint away from the model's middle stays on it: the
    /// joint goes straight from one pose's place to the other's, and a point along the
    /// bone keeps its distance from the joint (no stretched arm).
    #[test]
    fn a_bone_turns_about_its_joint_and_stays_on_it() {
        // A shoulder 50 units up and 10 out, the arm turned by 160 degrees about it in
        // the second pose: the matrix carries the model's points about the shoulder.
        let shoulder = Vec3::new(10.0, 0.0, 50.0);
        let about = |turn: Mat3, moved: Vec3| Local {
            basis: turn,
            offset: shoulder + moved - turn * shoulder,
        };
        let a = about(Mat3::IDENTITY, Vec3::ZERO);
        let b = about(
            Mat3::from_rotation_y(160_f32.to_radians()),
            Vec3::new(0.0, 4.0, 0.0),
        );
        let hand = shoulder + Vec3::new(30.0, 0.0, 0.0);
        for step in 1..10 {
            let t = step as f32 / 10.0;
            let between = blend(a, b, t, shoulder);
            let joint = between.basis * shoulder + between.offset;
            let expected = shoulder + Vec3::new(0.0, 4.0 * t, 0.0);
            assert!(
                (joint - expected).length() < 1e-3,
                "joint at {joint} at {t}"
            );
            let reach = (between.basis * hand + between.offset - joint).length();
            assert!((reach - 30.0).abs() < 1e-3, "arm {reach} long at {t}");
        }
    }

    /// The same turn written with the quaternion's other sign (as two GLA frames may
    /// store it) blends the same way: the bone does not spin a whole turn round.
    #[test]
    fn a_turn_stored_with_either_sign_blends_alike() {
        let a = Mat3::from_rotation_x(0.3);
        let b = Mat3::from_quat(-Quat::from_rotation_x(0.9));
        let between = turn_between(a, b, 0.5).expect("plain turns");
        let expected = Mat3::from_rotation_x(0.6);
        assert!(between.abs_diff_eq(expected, 1e-5));
    }

    /// A GLA bone's matrix, its quaternion stored in 16-bit parts, counts as a turn; a
    /// mirrored or stretched one does not, and keeps the squared-up blend.
    #[test]
    fn only_plain_turns_take_the_short_way() {
        let stored = Quat::from_xyzw(0.5001, 0.4999, 0.5, 0.5);
        assert!(is_turn(Mat3::from_quat(stored)));
        assert!(!is_turn(Mat3::from_cols(-Vec3::X, Vec3::Y, Vec3::Z)));
        assert!(!is_turn(Mat3::from_diagonal(Vec3::new(1.0, 1.0, 1.5))));
    }

    #[test]
    fn keys_hold_the_takeover_share() {
        let sample = ((1, 2), 0.5);
        assert_eq!(PoseKey::new(sample, sample, 1.0).weight(), 1.0);
        assert!((PoseKey::new(sample, sample, 0.4).weight() - 0.40625).abs() < 1e-6);
        assert_ne!(
            PoseKey::new(sample, sample, 0.4),
            PoseKey::new(sample, sample, 1.0)
        );
    }
}

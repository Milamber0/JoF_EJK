//! Body fit for emotes played on stock models: limbs keep the clearance from the body
//! the rig's dance gives them, measured on each model's own mesh.
//!
//! A dance made on a slim rig and played on a wide stock model (a heavy alien, a big
//! coat) puts the hands and elbows where the slim body's surface was: inside the wide
//! one. Each skeleton's body is measured once from its mesh in its base pose (bind
//! pose for a rig): an ellipsoid around the vertices of each torso bone and each
//! thigh ([`PARTS`]), and the thickness of each forearm and shin ([`LIMBS`]). Every
//! frame, each limb's elbow (knee), mid point and wrist (ankle) is measured against
//! the body parts on both skeletons: how far its surface is from each part's surface.
//! Where the stock limb is nearer its body than the rig's is to its own (up to
//! [`KEEP`]: a stock limb inside its body, or nearer than a hand hovering over the
//! rig's), the point is pushed out along the part's radius until it is as near as the
//! rig's (and never inside, even where the rig's own limb sinks into its body: a slim
//! mesh hides that, a wide one would not). A rig limb touching its body ([`CONTACT`]) also draws the stock
//! limb back onto the surface, a little ([`PULL_MAX`]), so a hand on the hip stays on
//! a wider hip. The pushes become an elbow and a wrist target and a two-bone turn of
//! the upper and lower limb ([`fit_limb`]), so the limb keeps its lengths and the hand
//! (fingers, saber) follows.
//!
//! Each side's base pose sets its zero: a mesh whose limbs already lie inside its own
//! ellipsoids at rest (thick sleeves) is not pushed for that.
//!
//! Body parts and limbs are named by JKA bone ([`PARTS`], [`LIMBS`]): which bones are
//! the trunk, the thighs and the limbs is the skeleton's anatomy, read through
//! `_humanoid`'s names as the rig bone map and the ankles are.

use super::affine;
use glam::{Affine3A, Mat3A, Quat, Vec3A};
use sjk_model::{Gla, Glm};

/// The bones whose flesh the limbs keep clear of: the trunk and the thighs.
const PARTS: [&str; 6] = [
    "pelvis",
    "lower_lumbar",
    "upper_lumbar",
    "thoracic",
    "lfemurYZ",
    "rfemurYZ",
];

/// Bones that begin another region of the body: what hangs under them (the arms, the
/// shins and feet, the neck and head) is not the flesh of the trunk or thigh above.
const REGIONS: [&str; 5] = ["lhumerus", "rhumerus", "ltibia", "rtibia", "cervical"];

/// Each limb: its upper, lower and end bone, and the body part it grows from (a leg
/// is not pushed off its own thigh).
const LIMBS: [(&str, &str, &str, Option<&str>); 4] = [
    ("lhumerus", "lradius", "lhand", None),
    ("rhumerus", "rradius", "rhand", None),
    ("lfemurYZ", "ltibia", "ltalus", Some("lfemurYZ")),
    ("rfemurYZ", "rtibia", "rtalus", Some("rfemurYZ")),
];

/// The most clearance (game units, surface to surface) a stock limb is kept at: as
/// near as the rig's, up to this. Further out the stock limb only has to stay out, so
/// a stance (legs apart, arms out) is not changed for a wider body.
pub(super) const KEEP: f32 = 2.0;
/// A rig limb this near its body touches it: the stock limb is drawn onto its body
/// (fading out by twice this).
pub(super) const CONTACT: f32 = 1.5;
/// The most a touching limb is drawn in towards the stock body.
pub(super) const PULL_MAX: f32 = 4.0;
/// The share of a part's samples each ellipsoid axis spans, from the middle (loose
/// straps, holsters and capes beyond it do not widen the body).
const SPAN: f32 = 0.9;
/// The share of a limb's samples inside its thickness.
const LIMB_SPAN: f32 = 0.75;

/// One body part: an ellipsoid in its skeleton's base (bind) model space, moved by the
/// bone's posed transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Part {
    /// The JKA bone.
    pub(super) bone: usize,
    pub(super) center: Vec3A,
    pub(super) radii: Vec3A,
}

/// One limb of [`LIMBS`]: its bones and thickness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Limb {
    pub(super) upper: usize,
    pub(super) lower: usize,
    pub(super) end: usize,
    /// The part it grows from, skipped when measuring it.
    pub(super) own: Option<usize>,
    /// Its forearm's (shin's) radius.
    pub(super) radius: f32,
}

/// A skeleton's measured body.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Body {
    pub(super) parts: Vec<Part>,
    /// One per [`LIMBS`] entry; `None` for a limb the skeleton or mesh lacks.
    pub(super) limbs: Vec<Option<Limb>>,
}

impl Body {
    /// Measure a body from `samples`, each a mesh point in base (bind) model space and
    /// the JKA bone that moves it most. `segment` gives a limb's lower and end joint
    /// positions in the same space.
    pub(super) fn measure(
        gla: &Gla,
        samples: &[(usize, Vec3A)],
        segment: impl Fn(usize, usize) -> Option<(Vec3A, Vec3A)>,
    ) -> Self {
        let index = |name: &str| gla.bones.iter().position(|bone| bone.name == name);
        let part_bones: Vec<Option<usize>> = PARTS.iter().map(|name| index(name)).collect();
        let regions: Vec<Option<usize>> = REGIONS.iter().map(|name| index(name)).collect();
        // Each bone's part: itself, else its nearest ancestor that is one, unless
        // another region of the body begins on the way.
        let part_of: Vec<Option<usize>> = (0..gla.bones.len())
            .map(|mut bone| {
                for _ in 0..gla.bones.len() {
                    if part_bones.contains(&Some(bone)) {
                        return Some(bone);
                    }
                    if regions.contains(&Some(bone)) {
                        return None;
                    }
                    bone = gla.bones[bone].parent?;
                }
                None
            })
            .collect();
        let mut parts = Vec::new();
        for bone in part_bones.iter().flatten() {
            let points: Vec<Vec3A> = samples
                .iter()
                .filter(|(owner, _)| part_of.get(*owner).copied().flatten() == Some(*bone))
                .map(|(_, at)| *at)
                .collect();
            if points.len() < 8 {
                continue;
            }
            let axis = |pick: fn(Vec3A) -> f32| span(points.iter().map(|p| pick(*p)), SPAN);
            let (x0, x1) = axis(|p| p.x);
            let (y0, y1) = axis(|p| p.y);
            let (z0, z1) = axis(|p| p.z);
            let radii = Vec3A::new(x1 - x0, y1 - y0, z1 - z0) * 0.5;
            if radii.min_element() <= 0.1 {
                continue;
            }
            parts.push(Part {
                bone: *bone,
                center: Vec3A::new(x0 + x1, y0 + y1, z0 + z1) * 0.5,
                radii,
            });
        }
        let limbs = LIMBS
            .iter()
            .map(|(upper, lower, end, own)| {
                let (upper, lower, end) = (index(upper)?, index(lower)?, index(end)?);
                let (from, to) = segment(lower, end)?;
                let length = (to - from).length();
                if length <= f32::EPSILON {
                    return None;
                }
                let along = (to - from) / length;
                // The lower bone, and bones hung on it between its two ends (a twist
                // bone), not past its end (the fingers JKA hangs on the forearm).
                let member = |bone: usize| {
                    bone == lower
                        || (gla.bones[bone].parent == Some(lower) && bone != end && {
                            let at = affine(&gla.bones[bone].base_pose).translation;
                            let t = (at - from).dot(along) / length;
                            (0.0..0.95).contains(&t)
                        })
                };
                let distances = samples
                    .iter()
                    .filter(|(owner, _)| member(*owner))
                    .filter_map(|(_, at)| {
                        let t = (*at - from).dot(along).clamp(0.0, length);
                        let reach = (*at - from).dot(along) / length;
                        (0.0..=1.0)
                            .contains(&reach)
                            .then(|| (*at - (from + along * t)).length())
                    });
                let radius = percentile(distances, LIMB_SPAN)?;
                Some(Limb {
                    upper,
                    lower,
                    end,
                    own: own.and_then(index),
                    radius,
                })
            })
            .collect();
        Self { parts, limbs }
    }

    /// The body of a stock model's `mesh` (its first LOD's drawn surfaces) on `gla`.
    pub(super) fn of_glm(gla: &Gla, mesh: &Glm) -> Self {
        let mut samples = Vec::new();
        if let Some(lod) = mesh.lods.first() {
            for surface in &lod.surfaces {
                let drawn = mesh
                    .hierarchy
                    .get(surface.hierarchy_index)
                    .is_some_and(|h| {
                        h.flags & 0x3 == 0 && !h.name.starts_with('*') && !h.name.contains("_cap_")
                    });
                if !drawn {
                    continue;
                }
                for vertex in &surface.vertices {
                    let strongest = vertex
                        .weights
                        .iter()
                        .max_by(|a, b| a.weight.total_cmp(&b.weight))
                        .and_then(|w| surface.bone_references.get(w.bone_reference).copied());
                    if let Some(bone) = strongest.filter(|&bone| bone < gla.bones.len()) {
                        samples.push((bone, Vec3A::from_array(vertex.position)));
                    }
                }
            }
        }
        let base = |bone: usize| affine(&gla.bones[bone].base_pose).translation;
        Self::measure(gla, &samples, |lower, end| Some((base(lower), base(end))))
    }

    /// Whether there is anything to fit with.
    pub(super) fn is_empty(&self) -> bool {
        self.parts.is_empty() || self.limbs.iter().all(Option::is_none)
    }

    /// The part on `bone`.
    pub(super) fn part(&self, bone: usize) -> Option<&Part> {
        self.parts.iter().find(|part| part.bone == bone)
    }
}

/// The middle `share` of `values`: where it starts and ends (`(0, 0)` when empty).
fn span(values: impl Iterator<Item = f32>, share: f32) -> (f32, f32) {
    let mut sorted: Vec<f32> = values.collect();
    if sorted.is_empty() {
        return (0.0, 0.0);
    }
    sorted.sort_by(f32::total_cmp);
    let cut = ((1.0 - share) * 0.5 * sorted.len() as f32) as usize;
    let last = sorted.len() - 1;
    (sorted[cut.min(last)], sorted[last.saturating_sub(cut)])
}

fn percentile(values: impl Iterator<Item = f32>, share: f32) -> Option<f32> {
    let mut sorted: Vec<f32> = values.collect();
    if sorted.len() < 4 {
        return None;
    }
    sorted.sort_by(f32::total_cmp);
    let at = ((sorted.len() - 1) as f32 * share) as usize;
    Some(sorted[at])
}

/// How far `point` is outside `part` posed by `transform` (negative inside),
/// measured along the ray from its centre, and that ray's direction in model space.
pub(super) fn clearance(part: &Part, transform: &Affine3A, point: Vec3A) -> (f32, Vec3A) {
    let local = transform.inverse().transform_point3a(point) - part.center;
    let length = local.length();
    let scaled = (local / part.radii).length();
    let direction = transform.matrix3 * local;
    let direction = direction.try_normalize().unwrap_or(Vec3A::X);
    if scaled <= 1e-4 {
        return (-part.radii.min_element(), direction);
    }
    (length * (1.0 - 1.0 / scaled), direction)
}

/// How far to move a stock limb point along `direction` (a part's radius), given its
/// clearance from the part and the rig's from the same part (each less its limb's
/// thickness and its base pose's own).
pub(super) fn push(stock: f32, rig: f32, direction: Vec3A) -> Vec3A {
    let target = rig.clamp(0.0, KEEP);
    let gap = target - stock;
    let amount = if gap > 0.0 {
        gap
    } else {
        let weight = ((2.0 * CONTACT - rig) / CONTACT).clamp(0.0, 1.0);
        gap.max(-PULL_MAX) * weight
    };
    direction * amount
}

/// Turn a limb so its elbow (knee) heads for `elbow + elbow_push` and its wrist
/// (ankle) reaches `wrist + wrist_push`, the shoulder (hip) fixed and both bones'
/// lengths kept: the upper bone's turn about the shoulder, then the lower bone's about
/// the new elbow, as rotations to apply to each bone and everything under it.
pub(super) fn fit_limb(
    shoulder: Vec3A,
    elbow: Vec3A,
    wrist: Vec3A,
    elbow_push: Vec3A,
    wrist_push: Vec3A,
) -> Option<(Quat, Vec3A, Quat)> {
    let upper = (elbow - shoulder).length();
    let lower = (wrist - elbow).length();
    if upper <= f32::EPSILON || lower <= f32::EPSILON {
        return None;
    }
    let goal = wrist + wrist_push;
    let reach = goal - shoulder;
    let distance = reach
        .length()
        .clamp((upper - lower).abs() + 1e-3, upper + lower - 1e-3);
    let along = reach.try_normalize()?;
    let goal = shoulder + along * distance;
    // The elbow on the circle the two lengths allow, on the side the pushed elbow
    // points to (else the side it was on).
    let cos = ((upper * upper + distance * distance - lower * lower) / (2.0 * upper * distance))
        .clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let side = |toward: Vec3A| {
        let offset = toward - shoulder;
        (offset - along * offset.dot(along)).try_normalize()
    };
    let pole = side(elbow + elbow_push).or_else(|| side(elbow))?;
    let new_elbow = shoulder + (along * cos + pole * sin) * upper;
    let first = Quat::from_rotation_arc(
        (elbow - shoulder).normalize().into(),
        (new_elbow - shoulder).normalize().into(),
    );
    let carried_wrist = shoulder + Vec3A::from(first * glam::Vec3::from(wrist - shoulder));
    let second = Quat::from_rotation_arc(
        (carried_wrist - new_elbow).normalize().into(),
        (goal - new_elbow).normalize().into(),
    );
    Some((first, new_elbow, second))
}

/// `pose` turned by `rotation` about `pivot`.
pub(super) fn turned(pose: &Affine3A, rotation: Quat, pivot: Vec3A) -> Affine3A {
    let turn = Mat3A::from_quat(rotation);
    Affine3A {
        matrix3: turn * pose.matrix3,
        translation: pivot + turn * (pose.translation - pivot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gla(bones: &[(&str, Option<usize>, Vec3A)]) -> Gla {
        Gla {
            name: "body".into(),
            scale: 1.0,
            bones: bones
                .iter()
                .map(|(name, parent, at)| sjk_model::GlaBone {
                    name: (*name).to_owned(),
                    flags: 0,
                    parent: *parent,
                    base_pose: [
                        [1.0, 0.0, 0.0, at.x],
                        [0.0, 1.0, 0.0, at.y],
                        [0.0, 0.0, 1.0, at.z],
                    ],
                    inverse_base_pose: [[0.0; 4]; 3],
                    children: Vec::new(),
                })
                .collect(),
            frames: Vec::new(),
            compressed_bones: Vec::new(),
        }
    }

    #[test]
    fn a_body_is_measured_from_its_mesh_without_its_arms() {
        let gla = gla(&[
            ("pelvis", None, Vec3A::new(0.0, 0.0, 40.0)),
            ("lhumerus", Some(0), Vec3A::new(10.0, 0.0, 50.0)),
            ("lradius", Some(1), Vec3A::new(10.0, 0.0, 40.0)),
            ("lhand", Some(2), Vec3A::new(10.0, 0.0, 30.0)),
        ]);
        let mut samples = Vec::new();
        // A trunk 12 wide, 8 deep, 10 high round the pelvis, an arm far out with a
        // forearm 2 thick.
        for i in 0..=10 {
            for j in 0..=10 {
                let (u, v) = (i as f32 / 10.0 - 0.5, j as f32 / 10.0 - 0.5);
                samples.push((0, Vec3A::new(12.0 * u, 8.0 * v, 40.0 + 10.0 * u * v)));
                samples.push((0, Vec3A::new(12.0 * v, 8.0 * u, 45.0 * (1.0 + 0.2 * u))));
            }
            let t = i as f32 / 10.0;
            samples.push((1, Vec3A::new(30.0, 0.0, 50.0 - 10.0 * t)));
            samples.push((2, Vec3A::new(12.0, 0.0, 40.0 - 10.0 * t)));
            samples.push((2, Vec3A::new(8.0, 0.0, 40.0 - 10.0 * t)));
        }
        let base = |bone: usize| affine(&gla.bones[bone].base_pose).translation;
        let body = Body::measure(&gla, &samples, |lower, end| Some((base(lower), base(end))));
        let pelvis = body.part(0).expect("the trunk");
        // The arm's samples (30 out) do not widen the trunk.
        assert!(pelvis.radii.x < 7.0 && pelvis.radii.x > 4.0, "{pelvis:?}");
        let arm = body.limbs[0].expect("the left arm");
        assert!((arm.radius - 2.0).abs() < 1e-3, "{arm:?}");
        assert!(body.limbs[1].is_none());
    }

    #[test]
    fn clearance_reads_the_surface_along_the_radius() {
        let part = Part {
            bone: 0,
            center: Vec3A::ZERO,
            radii: Vec3A::new(10.0, 5.0, 20.0),
        };
        let (outside, direction) =
            clearance(&part, &Affine3A::IDENTITY, Vec3A::new(14.0, 0.0, 0.0));
        assert!((outside - 4.0).abs() < 1e-4 && (direction - Vec3A::X).length() < 1e-4);
        let (inside, _) = clearance(&part, &Affine3A::IDENTITY, Vec3A::new(0.0, 3.0, 0.0));
        assert!((inside + 2.0).abs() < 1e-4, "{inside}");
        // Moved with its bone.
        let moved = Affine3A::from_translation(glam::Vec3::new(0.0, 0.0, 100.0));
        let (outside, _) = clearance(&part, &moved, Vec3A::new(0.0, 0.0, 125.0));
        assert!((outside - 5.0).abs() < 1e-4);
    }

    #[test]
    fn a_limb_is_pushed_to_the_rigs_clearance_and_drawn_onto_a_touched_body() {
        // The rig's hand hovers 1 unit off its body; the stock hand is 2 inside.
        let moved = push(-2.0, 1.0, Vec3A::X);
        assert!((moved - Vec3A::X * 3.0).length() < 1e-4, "{moved:?}");
        // The rig's hand is far out: the stock hand only comes out to `KEEP`.
        let out = push(-2.0, 20.0, Vec3A::X);
        assert!((out - Vec3A::X * 4.0).length() < 1e-4, "{out:?}");
        assert_eq!(push(5.0, 20.0, Vec3A::X), Vec3A::ZERO);
        // The rig's own arm sinks 3 into its body: the stock arm still comes out.
        let out = push(-3.0, -3.0, Vec3A::X);
        assert!((out - Vec3A::X * 3.0).length() < 1e-4, "{out:?}");
        // The rig's hand touches its hip: the stock hand 6 out comes in 4.
        let drawn = push(6.0, 0.5, Vec3A::X);
        assert!((drawn + Vec3A::X * 4.0).length() < 1e-4, "{drawn:?}");
        // Half way out of touching: half the pull; not touching: left there.
        let half = push(6.0, 2.25, Vec3A::X);
        assert!((half + Vec3A::X * 2.0).length() < 1e-4, "{half:?}");
        assert_eq!(push(6.0, 3.0, Vec3A::X), Vec3A::ZERO);
    }

    #[test]
    fn a_fitted_limb_keeps_its_lengths_and_reaches_its_goal() {
        let shoulder = Vec3A::new(0.0, 0.0, 50.0);
        let elbow = Vec3A::new(0.0, 0.0, 40.0);
        let wrist = Vec3A::new(0.0, -10.0, 40.0);
        let (first, new_elbow, second) = fit_limb(
            shoulder,
            elbow,
            wrist,
            Vec3A::new(3.0, 0.0, 0.0),
            Vec3A::new(4.0, 0.0, 0.0),
        )
        .expect("a fit");
        assert!(((new_elbow - shoulder).length() - 10.0).abs() < 1e-3);
        let carried = shoulder + Vec3A::from(first * glam::Vec3::from(wrist - shoulder));
        let new_wrist = new_elbow + Vec3A::from(second * glam::Vec3::from(carried - new_elbow));
        assert!(((new_wrist - new_elbow).length() - 10.0).abs() < 1e-3);
        assert!(
            (new_wrist - Vec3A::new(4.0, -10.0, 40.0)).length() < 1e-2,
            "{new_wrist:?}"
        );
        // The elbow went out towards its push.
        assert!(new_elbow.x > 0.5, "{new_elbow:?}");
    }
}

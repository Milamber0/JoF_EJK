//! Rig clips (emotes) played on stock GLM models: the bone map in reverse.
//!
//! An emote made on a rig model names that model (its file's `rig`). A player
//! wearing a stock model (Kyle, the retail characters, community GLM models on
//! `_humanoid`) plays it here: the source rig is loaded without its meshes and
//! fitted to the stock model's own skeleton ([`super::source_rig`]), its clip is
//! posed on the rig every frame, and each JKA bone the rig's bone map drives takes
//! the model-space turn its mapped joint makes from its aligned bind pose. That is
//! [`RigModel::place`] read backwards: there a joint's rotation is the driver
//! bone's turn times the joint's aligned bind rotation, here the bone's turn is the
//! joint's rotation times the inverse of that aligned bind rotation.
//!
//! A mapped bone's position is carried by the bone of its joint's nearest mapped
//! ancestor in the rig (a finger by the hand, not by the forearm the GLA hangs it
//! on), so every limb keeps the stock model's lengths and stays joined. Bones the
//! map leaves out (clavicles, face, twist bones, the saber's hang tags) move rigidly
//! with the bone the skeleton's own animations keep them most rigid with
//! ([`rigidity`]): a hang tag with the hand, so the saber stays in it. The pelvis
//! also moves as the clip's pelvis
//! does, scaled by how long the stock model's legs are against the rig's, and
//! stands on the driver's current ground (its lower ankle), so a dance that bobs or
//! crouches does so on the floor. The result replaces the driver's bone matrices for
//! drawing and bolts (a saber stays in a dancing hand); the server, its animation
//! numbers and saber traces are untouched.
//!
//! See `docs/rig-models.md`, "Emotes on stock models".

use super::body::{Body, Limb, Part, clearance, fit_limb, push, turned};
use super::clips::blend;
use super::{ANKLES, ClipPlay, RigModel, affine, rows, source_rig};
use glam::{Affine3A, Mat3A, Vec3A};
use sjk_model::{Gla, Glm};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// Rounds of the body fit per limb and frame.
const FIT_ROUNDS: usize = 3;

/// The JKA bone the pelvis placement reads and writes.
const PELVIS: &str = "pelvis";

/// How the bones of one stock skeleton follow one source rig's joints.
#[derive(Clone, Debug)]
struct Reverse {
    /// Every bone once, in the order they are posed (what a bone reads is posed
    /// before it), and how.
    plan: Vec<(usize, Place)>,
    pelvis: Option<Pelvis>,
    /// Keeping the stock limbs as clear of the stock body as the rig's are of its
    /// own, when both bodies were measured ([`super::body`]).
    fit: Option<Fit>,
}

/// The body fit of one stock model playing one rig's clips.
#[derive(Clone, Debug)]
struct Fit {
    limbs: Vec<LimbFit>,
}

/// One limb of the fit.
#[derive(Clone, Debug)]
struct LimbFit {
    stock: Limb,
    /// The rig joints on its upper, lower and end bone, and the rig limb's radius.
    joints: [usize; 3],
    rig_radius: f32,
    /// The bones the upper and the lower bone carry, themselves included.
    upper_bones: Vec<usize>,
    lower_bones: Vec<usize>,
    parts: Vec<PartFit>,
}

/// One body part a limb keeps clear of, on both skeletons.
#[derive(Clone, Copy, Debug)]
struct PartFit {
    stock: Part,
    rig: Part,
    /// The rig joint moving the rig's part.
    joint: usize,
    /// The elbow's, mid point's and wrist's clearance in the base (bind) pose, each
    /// side's zero when negative.
    rest_stock: [f32; 3],
    rest_rig: [f32; 3],
}

/// How one stock bone is posed from the clip.
#[derive(Clone, Copy, Debug)]
enum Place {
    /// Turned as the source joint `joint` turns from its aligned bind pose
    /// (`inverse`: that rotation's inverse), its base position carried by the posed
    /// bone `carrier`; without one, placed as the pelvis (or where the driver has it).
    Mapped {
        joint: usize,
        inverse: Mat3A,
        carrier: Option<usize>,
    },
    /// Moved rigidly with the posed bone `partner`; without one, the driver's own.
    Follow { partner: Option<usize> },
}

/// Placing the stock pelvis from the clip's.
#[derive(Clone, Copy, Debug)]
struct Pelvis {
    bone: usize,
    /// The pelvis bone's base position.
    base: Vec3A,
    /// The source joint mapped to it, and that joint's bind position.
    joint: usize,
    bind: Vec3A,
    /// The stock legs' length over the rig's (pelvis height above the lower ankle).
    ratio: f32,
    /// The rig's lower ankle in its bind pose (a clip's ground) and the driver's
    /// ankle bones with their base positions, when both have ankles.
    ground: Option<Ground>,
}

#[derive(Clone, Copy, Debug)]
struct Ground {
    rig_ankle: f32,
    ankles: [(usize, Vec3A); 2],
}

/// One emote clip a stock model can play.
#[derive(Debug)]
struct StockClip {
    /// The clip's name, as emote files name it.
    name: String,
    /// The rig model it was made on, fitted to this skeleton, and the clip's index
    /// there.
    source: Arc<RigModel>,
    clip: usize,
    reverse: Arc<Reverse>,
}

/// The emote clips a stock skeleton can play, with what posing them needs.
#[derive(Debug)]
pub(crate) struct StockEmotes {
    /// Every GLA bone's base pose and its inverse, in the skeleton's model space.
    base: Vec<Affine3A>,
    inverse_base: Vec<Affine3A>,
    clips: Vec<StockClip>,
}

/// One actor's stock emote pose, kept between frames so posing allocates nothing.
#[derive(Clone, Debug, Default)]
pub(crate) struct StockScratch {
    /// The source rig's joints this frame.
    joints: Vec<Affine3A>,
    /// Every bone's model-space pose from the clip alone.
    posed: Vec<Affine3A>,
    /// This frame's bone matrices (posed times inverse base), one per GLA bone:
    /// what the GLM is skinned with in place of the driver's.
    pub(crate) matrices: Vec<[[f32; 4]; 3]>,
}

impl StockEmotes {
    /// The installed emotes whose clip comes from a rig model (their file's `rig`)
    /// that `gla` can play; `None` when there are none.
    pub(crate) fn load(vfs: &VirtualFileSystem, gla: &Gla, mesh: &Glm) -> Option<Self> {
        let catalogue = crate::emotes::lock().catalogue.clone();
        let catalogue = catalogue.unwrap_or_else(|| crate::emotes::load(vfs));
        let mut clips: Vec<StockClip> = Vec::new();
        let mut body: Option<Body> = None;
        let mut reverses: Vec<(String, Arc<Reverse>)> = Vec::new();
        for emote in catalogue {
            if emote.clip.is_empty()
                || emote.rig.is_empty()
                || clips.iter().any(|known| known.name == emote.clip)
            {
                continue;
            }
            let directory = format!("models/players/{}", emote.rig);
            let Some(source) = source_rig(vfs, &directory, gla) else {
                continue;
            };
            let Some(clip) = source.clips.iter().position(|c| c.name == emote.clip) else {
                continue;
            };
            let key = directory.to_ascii_lowercase();
            let reverse = match reverses.iter().find(|(known, _)| *known == key) {
                Some((_, reverse)) => reverse.clone(),
                None => {
                    let body = body.get_or_insert_with(|| Body::of_glm(gla, mesh));
                    let Some(reverse) = Reverse::of(&source, gla, Some(body)) else {
                        crate::log::progress(format_args!(
                            "stock emotes: {directory} does not fit the {} skeleton",
                            gla.name
                        ));
                        continue;
                    };
                    let reverse = Arc::new(reverse);
                    reverses.push((key, reverse.clone()));
                    reverse
                }
            };
            clips.push(StockClip {
                name: emote.clip.clone(),
                source,
                clip,
                reverse,
            });
        }
        if clips.is_empty() {
            return None;
        }
        Some(Self::with_clips(gla, clips))
    }

    fn with_clips(gla: &Gla, clips: Vec<StockClip>) -> Self {
        let base: Vec<Affine3A> = gla
            .bones
            .iter()
            .map(|bone| affine(&bone.base_pose))
            .collect();
        let inverse_base = base.iter().map(Affine3A::inverse).collect();
        Self {
            base,
            inverse_base,
            clips,
        }
    }

    /// The clip named `name` `seconds` after it started, if this skeleton can play
    /// it (see [`ClipPlay::new`]); its `clip` indexes this skeleton's clips.
    pub(crate) fn play(
        &self,
        name: &str,
        seconds: f32,
        remaining: Option<f32>,
        looping: bool,
    ) -> Option<ClipPlay> {
        let index = self.clips.iter().position(|clip| clip.name == name)?;
        let stock = &self.clips[index];
        let duration = stock.source.clips.get(stock.clip)?.duration;
        Some(ClipPlay::new(index, duration, seconds, remaining, looping))
    }

    /// Pose every bone from `play`'s clip, blended over the driver's evaluated
    /// `driver` matrices by its weight, into `scratch.matrices`. `false` (and nothing
    /// written) when the clip is unknown, its weight is zero or `driver` is not
    /// this skeleton's.
    pub(crate) fn pose(
        &self,
        driver: &[[[f32; 4]; 3]],
        play: ClipPlay,
        scratch: &mut StockScratch,
    ) -> bool {
        let Some(stock) = self.clips.get(play.clip) else {
            return false;
        };
        let Some(clip) = stock.source.clips.get(stock.clip) else {
            return false;
        };
        if play.weight <= 0.0 || driver.len() != self.base.len() {
            return false;
        }
        let StockScratch {
            joints,
            posed,
            matrices,
        } = scratch;
        stock.source.clip_pose(clip, play.time, joints);
        let reverse = &stock.reverse;
        let pelvis = reverse
            .pelvis
            .map(|pelvis| (pelvis.bone, pelvis.place(driver, joints)));
        posed.clear();
        posed.resize(self.base.len(), Affine3A::IDENTITY);
        matrices.clear();
        matrices.resize(self.base.len(), [[0.0; 4]; 3]);
        for &(bone, place) in &reverse.plan {
            let base = self.base[bone];
            let carried = |by: usize| posed[by] * self.inverse_base[by];
            let clip_pose = match place {
                Place::Mapped {
                    joint,
                    inverse,
                    carrier,
                } => {
                    let translation = match (pelvis, carrier) {
                        (Some((pelvis_bone, at)), _) if pelvis_bone == bone => at,
                        (_, Some(carrier)) => carried(carrier).transform_point3a(base.translation),
                        _ => affine(&driver[bone]).transform_point3a(base.translation),
                    };
                    Affine3A {
                        matrix3: joints[joint].matrix3 * inverse * base.matrix3,
                        translation,
                    }
                }
                Place::Follow {
                    partner: Some(partner),
                } => carried(partner) * base,
                Place::Follow { partner: None } => affine(&driver[bone]) * base,
            };
            posed[bone] = clip_pose;
        }
        if let Some(fit) = &reverse.fit {
            fit.apply(&stock.source, joints, posed, &self.inverse_base);
        }
        for (bone, clip_pose) in posed.iter().enumerate() {
            let base = self.base[bone];
            let shown = if play.weight >= 1.0 {
                *clip_pose
            } else {
                blend(&(affine(&driver[bone]) * base), clip_pose, play.weight)
            };
            matrices[bone] = rows(&(shown * self.inverse_base[bone]));
        }
        true
    }
}

impl Reverse {
    /// How `gla`'s bones follow `source` (fitted to `gla`): `None` when its bone map
    /// drives none of them or it was fitted to another skeleton.
    fn of(source: &RigModel, gla: &Gla, stock: Option<&Body>) -> Option<Self> {
        if source.anchors.len() != gla.bones.len() {
            return None;
        }
        let bones: Vec<Option<(usize, Mat3A)>> = (0..gla.bones.len())
            .map(|bone| {
                let anchor = source.anchors[bone].filter(|anchor| anchor.bone == bone)?;
                let driven = source.driven.get(anchor.joint).copied().flatten()?;
                Some((anchor.joint, driven.rotation.inverse()))
            })
            .collect();
        if bones.iter().all(Option::is_none) {
            return None;
        }
        let plan = plan(source, gla, &bones);
        let fit = stock.and_then(|stock| Fit::of(source, gla, stock, &bones));
        let bone_index = |name: &str| gla.bones.iter().position(|bone| bone.name == name);
        let base = |bone: usize| affine(&gla.bones[bone].base_pose).translation;
        let pelvis = bone_index(PELVIS).and_then(|bone| {
            let (joint, _) = bones[bone]?;
            let bind = source.joints[joint].bind.translation;
            let ankle_bones = ANKLES.map(bone_index);
            let ground = match ankle_bones {
                [Some(left), Some(right)] => {
                    let joint_of = |bone: usize| bones[bone].map(|(joint, _)| joint);
                    match (joint_of(left), joint_of(right)) {
                        (Some(l), Some(r)) => Some(Ground {
                            rig_ankle: source.joints[l]
                                .bind
                                .translation
                                .z
                                .min(source.joints[r].bind.translation.z),
                            ankles: [(left, base(left)), (right, base(right))],
                        }),
                        _ => None,
                    }
                }
                _ => None,
            };
            let ratio = ground
                .and_then(|ground| {
                    let stock = base(bone).z - ground.ankles[0].1.z.min(ground.ankles[1].1.z);
                    let rig = bind.z - ground.rig_ankle;
                    (stock > f32::EPSILON && rig > f32::EPSILON).then(|| stock / rig)
                })
                .unwrap_or(1.0);
            Some(Pelvis {
                bone,
                base: base(bone),
                joint,
                bind,
                ratio,
                ground,
            })
        });
        Some(Self { plan, pelvis, fit })
    }
}

impl Pelvis {
    /// Where the stock pelvis stands while the source's `joints` play: the driver's
    /// pelvis now plus the clip pelvis's move from its bind position, the height
    /// measured from the driver's lower ankle when both skeletons have ankles.
    fn place(&self, driver: &[[[f32; 4]; 3]], joints: &[Affine3A]) -> Vec3A {
        let clip = joints[self.joint].translation;
        let now = driver.get(self.bone).map_or(self.base, |matrix| {
            affine(matrix).transform_point3a(self.base)
        });
        let mut at = now + (clip - self.bind) * self.ratio;
        if let Some(ground) = self.ground {
            let ankle = ground
                .ankles
                .iter()
                .filter_map(|(bone, base)| {
                    driver
                        .get(*bone)
                        .map(|matrix| affine(matrix).transform_point3a(*base).z)
                })
                .fold(f32::MAX, f32::min);
            if ankle < f32::MAX {
                at.z = ankle + (clip.z - ground.rig_ankle) * self.ratio;
            }
        }
        at
    }
}

/// The order and way `gla`'s bones are posed, `mapped` giving each bone's source
/// joint and inverse aligned bind rotation. Mapped bones come first, the shallower
/// joints first; each is carried by the bone of its joint's nearest mapped ancestor
/// in the rig. The others follow, parents first, each with the posed bone it is
/// most rigid with ([`rigidity`]) among the mapped bones and its own parent; without
/// the skeleton's animations to tell, with its parent. A root bone keeps the
/// driver's matrix.
fn plan(source: &RigModel, gla: &Gla, mapped: &[Option<(usize, Mat3A)>]) -> Vec<(usize, Place)> {
    let count = gla.bones.len();
    let mut bone_of_joint = vec![None; source.joints.len()];
    for (bone, entry) in mapped.iter().enumerate() {
        if let Some((joint, _)) = entry {
            bone_of_joint[*joint] = Some(bone);
        }
    }
    let depth = |mut joint: usize| {
        let mut depth = 0;
        while let Some(parent) = source.joints[joint].parent {
            depth += 1;
            joint = parent;
        }
        depth
    };
    let mut first: Vec<(usize, usize, Mat3A)> = mapped
        .iter()
        .enumerate()
        .filter_map(|(bone, entry)| entry.map(|(joint, inverse)| (bone, joint, inverse)))
        .collect();
    first.sort_by_key(|&(bone, joint, _)| (depth(joint), bone));
    let mut plan: Vec<(usize, Place)> = first
        .iter()
        .map(|&(bone, joint, inverse)| {
            let mut ancestor = source.joints[joint].parent;
            let carrier = loop {
                let Some(at) = ancestor else { break None };
                match bone_of_joint[at] {
                    Some(carrier) if carrier != bone => break Some(carrier),
                    _ => ancestor = source.joints[at].parent,
                }
            };
            (
                bone,
                Place::Mapped {
                    joint,
                    inverse,
                    carrier,
                },
            )
        })
        .collect();
    let rigid = rigidity(gla);
    let parents: Vec<Option<usize>> = gla.bones.iter().map(|bone| bone.parent).collect();
    for bone in bone_order(&parents) {
        if mapped[bone].is_some() {
            continue;
        }
        // A root bone (`model_root`) keeps the driver's matrix: it carries no
        // vertices, and what hangs on it expects the driver's placement.
        let Some(parent) = parents[bone].filter(|&parent| parent < count) else {
            plan.push((bone, Place::Follow { partner: None }));
            continue;
        };
        let partner = match &rigid {
            Some(cost) => mapped
                .iter()
                .enumerate()
                .filter_map(|(candidate, entry)| entry.map(|_| candidate))
                .chain(Some(parent))
                .filter(|&candidate| candidate != bone)
                .min_by(|&a, &b| cost[bone * count + a].total_cmp(&cost[bone * count + b])),
            None => Some(parent),
        };
        plan.push((bone, Place::Follow { partner }));
    }
    plan
}

/// Frames of a skeleton's animations sampled to measure how rigidly bones move
/// together.
const RIGIDITY_SAMPLES: usize = 256;

/// How far (squared game units, summed over three points on it) each bone strays
/// from moving rigidly with each other one over `gla`'s own animations: entry
/// `bone * count + other`. A bone's base position and points 5 units along two axes
/// from it are carried by its matrix and read back in the other bone's base frame;
/// their variance over the sampled frames is the cost. `None` for a skeleton without
/// frames. Kept per skeleton for the session.
fn rigidity(gla: &Gla) -> Option<Arc<Vec<f32>>> {
    type Key = (String, usize, usize);
    type Costs = Option<Arc<Vec<f32>>>;
    static KNOWN: LazyLock<Mutex<HashMap<Key, Costs>>> = LazyLock::new(Mutex::default);
    let key = (gla.name.clone(), gla.bones.len(), gla.frames.len());
    let mut known = KNOWN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cost) = known.get(&key) {
        return cost.clone();
    }
    let count = gla.bones.len();
    let total = gla.frames.len();
    let probes: Vec<[Vec3A; 3]> = gla
        .bones
        .iter()
        .map(|bone| {
            let at = affine(&bone.base_pose).translation;
            [at, at + Vec3A::X * 5.0, at + Vec3A::Y * 5.0]
        })
        .collect();
    let mut sums = vec![[Vec3A::ZERO; 3]; count * count];
    let mut squares = vec![0.0f32; count * count];
    let mut samples = 0usize;
    for frame in (0..total).step_by((total / RIGIDITY_SAMPLES).max(1)) {
        let Ok(matrices) = gla.frame_matrices(frame) else {
            continue;
        };
        let matrices: Vec<Affine3A> = matrices.iter().map(affine).collect();
        let inverses: Vec<Affine3A> = matrices.iter().map(Affine3A::inverse).collect();
        for bone in 0..count {
            let posed = probes[bone].map(|probe| matrices[bone].transform_point3a(probe));
            for (other, inverse) in inverses.iter().enumerate() {
                let entry = bone * count + other;
                for (point, sum) in posed.iter().zip(sums[entry].iter_mut()) {
                    let read = inverse.transform_point3a(*point);
                    *sum += read;
                    squares[entry] += read.length_squared();
                }
            }
        }
        samples += 1;
    }
    let cost = (samples > 0).then(|| {
        let n = samples as f32;
        Arc::new(
            sums.iter()
                .zip(&squares)
                .map(|(sum, square)| {
                    square / n - sum.iter().map(|s| (*s / n).length_squared()).sum::<f32>()
                })
                .collect(),
        )
    });
    known.insert(key, cost.clone());
    cost
}

impl Fit {
    /// The fit of `stock`'s limbs against the rig `source`'s measured body: `None`
    /// when the rig was not measured or no limb has both bodies' parts.
    fn of(
        source: &RigModel,
        gla: &Gla,
        stock: &Body,
        mapped: &[Option<(usize, Mat3A)>],
    ) -> Option<Self> {
        let rig = source.body.as_ref()?;
        if stock.is_empty() || rig.is_empty() {
            return None;
        }
        let base = |bone: usize| affine(&gla.bones[bone].base_pose).translation;
        let under = |root: usize| -> Vec<usize> {
            (0..gla.bones.len())
                .filter(|&bone| {
                    let mut at = Some(bone);
                    for _ in 0..gla.bones.len() {
                        match at {
                            Some(found) if found == root => return true,
                            Some(found) => at = gla.bones[found].parent,
                            None => return false,
                        }
                    }
                    false
                })
                .collect()
        };
        let limbs: Vec<LimbFit> = stock
            .limbs
            .iter()
            .zip(&rig.limbs)
            .filter_map(|(stock_limb, rig_limb)| {
                let (stock_limb, rig_limb) = ((*stock_limb)?, (*rig_limb)?);
                let joint = |bone: usize| mapped.get(bone).copied().flatten().map(|(j, _)| j);
                let joints = [
                    joint(stock_limb.upper)?,
                    joint(stock_limb.lower)?,
                    joint(stock_limb.end)?,
                ];
                let three = |a: Vec3A, b: Vec3A| [a, (a + b) * 0.5, b];
                let stock_rest = three(base(stock_limb.lower), base(stock_limb.end));
                let bind = |j: usize| source.joints[j].bind.translation;
                let rig_rest = three(bind(joints[1]), bind(joints[2]));
                let parts: Vec<PartFit> = stock
                    .parts
                    .iter()
                    .filter(|part| Some(part.bone) != stock_limb.own)
                    .filter_map(|stock_part| {
                        let rig_part = *rig.part(stock_part.bone)?;
                        let joint = joint(stock_part.bone)?;
                        let rest = |part: &Part, points: &[Vec3A; 3], radius: f32| {
                            points
                                .map(|point| clearance(part, &Affine3A::IDENTITY, point).0 - radius)
                        };
                        Some(PartFit {
                            stock: *stock_part,
                            rig: rig_part,
                            joint,
                            rest_stock: rest(stock_part, &stock_rest, stock_limb.radius),
                            rest_rig: rest(&rig_part, &rig_rest, rig_limb.radius),
                        })
                    })
                    .collect();
                (!parts.is_empty()).then(|| LimbFit {
                    stock: stock_limb,
                    joints,
                    rig_radius: rig_limb.radius,
                    upper_bones: under(stock_limb.upper),
                    lower_bones: under(stock_limb.lower),
                    parts,
                })
            })
            .collect();
        (!limbs.is_empty()).then_some(Self { limbs })
    }

    /// Fit the limbs of `posed` (the clip's stock pose) to the clearance the rig
    /// `source`'s `joints` give its own.
    fn apply(
        &self,
        source: &RigModel,
        joints: &[Affine3A],
        posed: &mut [Affine3A],
        inverse_base: &[Affine3A],
    ) {
        for limb in &self.limbs {
            // A few rounds: a push out of one part can lead into another.
            for _ in 0..FIT_ROUNDS {
                if !self.fit_round(limb, source, joints, posed, inverse_base) {
                    break;
                }
            }
        }
    }

    /// One round of fitting `limb`; `false` when it needed no move.
    fn fit_round(
        &self,
        limb: &LimbFit,
        source: &RigModel,
        joints: &[Affine3A],
        posed: &mut [Affine3A],
        inverse_base: &[Affine3A],
    ) -> bool {
        let at = |bone: usize| posed[bone].translation;
        let (shoulder, elbow, wrist) = (
            at(limb.stock.upper),
            at(limb.stock.lower),
            at(limb.stock.end),
        );
        let stock_points = [elbow, (elbow + wrist) * 0.5, wrist];
        let rig_at = |joint: usize| joints[joint].translation;
        let (rig_elbow, rig_wrist) = (rig_at(limb.joints[1]), rig_at(limb.joints[2]));
        let rig_points = [rig_elbow, (rig_elbow + rig_wrist) * 0.5, rig_wrist];
        // Per point: the deepest push out of any part; without one, the pull onto the
        // part the rig's limb is nearest, if it touches it.
        let mut out = [(0.0f32, Vec3A::ZERO); 3];
        let mut onto = [(f32::MAX, Vec3A::ZERO); 3];
        for part in &limb.parts {
            let stock_transform = posed[part.stock.bone] * inverse_base[part.stock.bone];
            let rig_transform = joints[part.joint] * source.joints[part.joint].inverse_bind;
            for k in 0..3 {
                let (stock, direction) = clearance(&part.stock, &stock_transform, stock_points[k]);
                let (rig, _) = clearance(&part.rig, &rig_transform, rig_points[k]);
                let stock = stock - limb.stock.radius - part.rest_stock[k].min(0.0);
                let rig = rig - limb.rig_radius - part.rest_rig[k].min(0.0);
                let moved = push(stock, rig, direction);
                let amount = moved.dot(direction);
                if amount > out[k].0 {
                    out[k] = (amount, moved);
                } else if amount < 0.0 && rig < onto[k].0 {
                    onto[k] = (rig, moved);
                }
            }
        }
        let pushes: [Vec3A; 3] = std::array::from_fn(|k| {
            if out[k].0 > 0.0 {
                out[k].1
            } else if onto[k].0 < f32::MAX {
                onto[k].1
            } else {
                Vec3A::ZERO
            }
        });
        let elbow_push = pushes[0] + pushes[1] * 0.5;
        let wrist_push = pushes[2] + pushes[1] * 0.5;
        if elbow_push.length_squared() < 1e-4 && wrist_push.length_squared() < 1e-4 {
            return false;
        }
        let Some((first, new_elbow, second)) =
            fit_limb(shoulder, elbow, wrist, elbow_push, wrist_push)
        else {
            return false;
        };
        for &bone in &limb.upper_bones {
            posed[bone] = turned(&posed[bone], first, shoulder);
        }
        for &bone in &limb.lower_bones {
            posed[bone] = turned(&posed[bone], second, new_elbow);
        }
        true
    }
}

/// Bone indices with every parent before its children (a parent past the end, or a
/// cycle, makes the bone a root).
pub(super) fn bone_order(parents: &[Option<usize>]) -> Vec<usize> {
    let mut order = Vec::with_capacity(parents.len());
    let mut placed = vec![false; parents.len()];
    let mut chain = Vec::new();
    for start in 0..parents.len() {
        let mut bone = start;
        while !placed[bone] && !chain.contains(&bone) {
            chain.push(bone);
            match parents[bone] {
                Some(parent) if parent < parents.len() => bone = parent,
                _ => break,
            }
        }
        while let Some(bone) = chain.pop() {
            if !placed[bone] {
                placed[bone] = true;
                order.push(bone);
            }
        }
    }
    order
}

#[cfg(test)]
impl StockEmotes {
    /// A skeleton playing `source`'s clips (tests: no catalogue or file system).
    pub(super) fn of_source(gla: &Gla, source: Arc<RigModel>) -> Option<Self> {
        let reverse = Arc::new(Reverse::of(&source, gla, None)?);
        let clips = (0..source.clips.len())
            .map(|clip| StockClip {
                name: source.clips[clip].name.clone(),
                source: source.clone(),
                clip,
                reverse: reverse.clone(),
            })
            .collect();
        Some(Self::with_clips(gla, clips))
    }
}

#[cfg(test)]
#[path = "rig_stock_tests.rs"]
mod tests;

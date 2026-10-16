//! Checks of stock emotes and of rig models against the retail skeleton. They read
//! the game's `_humanoid.gla` and `animation.cfg` (never committed), so they are
//! ignored by default:
//!
//! ```sh
//! SJK_TEST_GLA=<_humanoid.gla> SJK_TEST_ANIMATIONS=<animation.cfg> \
//!   cargo test -p sjk-viewer --bins rig_stock -- --ignored --nocapture --test-threads=1
//! ```

use super::super::body::Body;
use super::super::clips::Clip;
use super::super::{Manifest, RigModel, Scratch, affine};
use super::*;
use glam::Vec3A;
use sjk_model::gltf::{GltfAnimation, GltfChannel, GltfJoint, GltfModel, GltfPath};
use sjk_model::{AnimationConfig, GlaBone, Glm};

fn translated(t: Vec3A) -> [[f32; 4]; 3] {
    [
        [1.0, 0.0, 0.0, t.x],
        [0.0, 1.0, 0.0, t.y],
        [0.0, 0.0, 1.0, t.z],
    ]
}

/// The retail skeleton and animation table, when the environment names them.
fn retail() -> Option<(Gla, AnimationConfig)> {
    let gla = std::env::var_os("SJK_TEST_GLA")?;
    let config = std::env::var_os("SJK_TEST_ANIMATIONS")?;
    let gla = Gla::parse(&std::fs::read(gla).ok()?).ok()?;
    let config = AnimationConfig::parse(&std::fs::read(config).ok()?).ok()?;
    Some((gla, config))
}

/// A mannequin-named rig standing on `gla`'s base pose: each joint where the JKA bone
/// its map entry names stands (spine and neck joints between them, third finger
/// joints past the second), its legs `legs` times as long below the hips.
fn mannequin_on(gla: &Gla, legs: f32) -> GltfModel {
    let at = |name: &str| {
        let bone = gla.bones.iter().find(|b| b.name == name).expect(name);
        affine(&bone.base_pose).translation
    };
    let mut joints: Vec<(String, Option<usize>, Vec3A)> = Vec::new();
    let mut add = |name: &str, parent: Option<&str>, position: Vec3A| {
        let parent = parent.map(|p| {
            joints
                .iter()
                .position(|(n, _, _)| n == p)
                .expect("parent first")
        });
        joints.push((name.to_owned(), parent, position));
    };
    let mid = |a: Vec3A, b: Vec3A| (a + b) * 0.5;
    add("pelvis", None, at("pelvis"));
    add("spine_01", Some("pelvis"), at("lower_lumbar"));
    add(
        "spine_02",
        Some("spine_01"),
        mid(at("lower_lumbar"), at("upper_lumbar")),
    );
    add("spine_03", Some("spine_02"), at("upper_lumbar"));
    add("spine_04", Some("spine_03"), at("thoracic"));
    add(
        "spine_05",
        Some("spine_04"),
        mid(at("thoracic"), at("cervical")),
    );
    add("neck_01", Some("spine_05"), at("cervical"));
    add(
        "neck_02",
        Some("neck_01"),
        mid(at("cervical"), at("cranium")),
    );
    add("head", Some("neck_02"), at("cranium"));
    for side in ["l", "r"] {
        let j = side;
        let hips = at("pelvis");
        let leg = |name: &str| hips + (at(name) - hips) * legs;
        add(
            &format!("clavicle_{side}"),
            Some("spine_05"),
            at(&format!("{j}clavical")),
        );
        add(
            &format!("upperarm_{side}"),
            Some(&format!("clavicle_{side}")),
            at(&format!("{j}humerus")),
        );
        add(
            &format!("lowerarm_{side}"),
            Some(&format!("upperarm_{side}")),
            at(&format!("{j}radius")),
        );
        add(
            &format!("hand_{side}"),
            Some(&format!("lowerarm_{side}")),
            at(&format!("{j}hand")),
        );
        for (finger, group, nudge) in [
            ("thumb", "d1", 0.0),
            ("index", "d2", -0.3),
            ("middle", "d2", 0.3),
            ("ring", "d4", -0.3),
            ("pinky", "d4", 0.3),
        ] {
            let offset = Vec3A::new(0.0, nudge, 0.0);
            let first = at(&format!("{j}_{group}_j1")) + offset;
            let second = at(&format!("{j}_{group}_j2")) + offset;
            add(
                &format!("{finger}_01_{side}"),
                Some(&format!("hand_{side}")),
                first,
            );
            add(
                &format!("{finger}_02_{side}"),
                Some(&format!("{finger}_01_{side}")),
                second,
            );
            add(
                &format!("{finger}_03_{side}"),
                Some(&format!("{finger}_02_{side}")),
                second + (second - first),
            );
        }
        add(
            &format!("thigh_{side}"),
            Some("pelvis"),
            leg(&format!("{j}femurYZ")),
        );
        add(
            &format!("calf_{side}"),
            Some(&format!("thigh_{side}")),
            leg(&format!("{j}tibia")),
        );
        add(
            &format!("foot_{side}"),
            Some(&format!("calf_{side}")),
            leg(&format!("{j}talus")),
        );
    }
    // Game axes back to glTF's (+Y up, +Z front), one unit a game unit.
    let gltf = |t: Vec3A| Vec3A::new(t.x, t.z, -t.y);
    GltfModel {
        joints: joints
            .iter()
            .map(|(name, parent, position)| {
                let local = match parent {
                    Some(parent) => gltf(*position - joints[*parent].2),
                    None => gltf(*position),
                };
                GltfJoint {
                    name: name.clone(),
                    parent: *parent,
                    bind: translated(gltf(*position)),
                    inverse_bind: translated(-gltf(*position)),
                    rest: translated(local),
                }
            })
            .collect(),
        ..Default::default()
    }
}

fn manifest() -> Manifest {
    Manifest::parse("scale 1").expect("the manifest")
}

/// `animation`'s frames, as bone matrices.
fn frames(gla: &Gla, config: &AnimationConfig, animation: &str) -> Vec<Vec<[[f32; 4]; 3]>> {
    let sequence = config.get(animation).expect(animation);
    (0..sequence.frame_count)
        .map(|frame| {
            gla.frame_matrices(sequence.first_frame + frame)
                .expect("the frame")
        })
        .collect()
}

/// `rig` posed by the driver's `frames`, made a clip named `name` (a frame a key).
fn clip_of(rig: &RigModel, frames: &[Vec<[[f32; 4]; 3]>], name: &str) -> Clip {
    let mut scratch = Scratch::default();
    let mut locals = Vec::new();
    let mut times = Vec::new();
    for (index, bones) in frames.iter().enumerate() {
        rig.pose_after(bones, None, &mut scratch, None);
        let pose = &scratch.pose;
        locals.push(
            rig.joints
                .iter()
                .enumerate()
                .map(|(joint, j)| match j.parent {
                    Some(parent) => pose[parent].inverse() * pose[joint],
                    None => j.above.inverse() * pose[joint],
                })
                .collect(),
        );
        times.push(index as f32 / 30.0);
    }
    Clip::from_frames(name, &times, &locals)
}

/// Every bone's posed position (its base position moved by its matrix).
fn positions(gla: &Gla, matrices: &[[[f32; 4]; 3]]) -> Vec<Vec3A> {
    gla.bones
        .iter()
        .zip(matrices)
        .map(|(bone, matrix)| affine(matrix).transform_point3a(affine(&bone.base_pose).translation))
        .collect()
}

/// A JKA animation played by a rig model made on the JKA skeleton, then as that rig's
/// clip on the stock skeleton, should come back as it was: every bone where the
/// animation puts it, measured from the pelvis.
#[test]
#[ignore = "reads the retail _humanoid.gla (SJK_TEST_GLA, SJK_TEST_ANIMATIONS)"]
fn retail_animations_come_back_through_a_stock_emote() {
    let Some((gla, config)) = retail() else {
        eprintln!("SJK_TEST_GLA and SJK_TEST_ANIMATIONS are not set");
        return;
    };
    let pelvis = gla.bones.iter().position(|b| b.name == "pelvis").unwrap();
    let mut worst_overall = 0.0f32;
    for animation in [
        "BOTH_STAND1",
        "BOTH_ROLL_F",
        "BOTH_A2_T__B_",
        "BOTH_FLIP_BACK1",
        "BOTH_WALL_RUN_RIGHT",
    ] {
        let Some(_) = config.get(animation) else {
            eprintln!("{animation}: not in animation.cfg");
            continue;
        };
        let frames = frames(&gla, &config, animation);
        let mut rig = RigModel::fit(mannequin_on(&gla, 1.0), "", &manifest(), &gla).unwrap();
        let clip = clip_of(&rig, &frames, animation);
        rig.clips.push(clip);
        let stock = StockEmotes::of_source(&gla, Arc::new(rig)).expect("the stock skeleton");
        let index = stock.clips.len() - 1;
        let mut scratch = StockScratch::default();
        let mut worst: Vec<f32> = vec![0.0; gla.bones.len()];
        for (frame, bones) in frames.iter().enumerate() {
            let play = ClipPlay {
                clip: index,
                time: frame as f32 / 30.0,
                weight: 1.0,
            };
            // The driver plays the same frame, so the clip's pelvis is the driver's.
            assert!(stock.pose(bones, play, &mut scratch));
            let expected = positions(&gla, bones);
            let got = positions(&gla, &scratch.matrices);
            for bone in 0..gla.bones.len() {
                let error =
                    ((got[bone] - got[pelvis]) - (expected[bone] - expected[pelvis])).length();
                worst[bone] = worst[bone].max(error);
            }
        }
        let mut ranked: Vec<(usize, f32)> = worst.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        eprintln!(
            "{animation}: worst bones {}",
            ranked
                .iter()
                .take(8)
                .map(|(bone, error)| format!("{} {error:.2}", gla.bones[*bone].name))
                .collect::<Vec<_>>()
                .join(", ")
        );
        // The bones the mesh is skinned with (not the root, the unweighted motion
        // bone or the hang tags, which some animations park away from the hand).
        let limbs = ranked
            .iter()
            .filter(|(bone, _)| {
                let name = &gla.bones[*bone].name;
                name != "model_root" && name != "Motion" && !name.contains("hang_tag")
            })
            .map(|(_, error)| *error)
            .fold(0.0f32, f32::max);
        worst_overall = worst_overall.max(limbs);
    }
    eprintln!("worst limb bone error from the pelvis: {worst_overall:.2} units");
    // What is left is the shoulder shrug JKA's animations add (its clavicle moves the
    // arm; the arm rides on the spine here), a few units; a finger left on the
    // forearm while the hand turns strayed more than 5.
    assert!(worst_overall < 4.0, "{worst_overall}");
}

/// A driver whose finger hangs on the forearm (as JKA's do), with a hand between.
fn hand_driver() -> Gla {
    let bone = |name: &str, parent, x: f32, z: f32| GlaBone {
        name: name.to_owned(),
        flags: 0,
        parent,
        base_pose: translated(Vec3A::new(x, 0.0, z)),
        inverse_base_pose: translated(Vec3A::new(-x, 0.0, -z)),
        children: Vec::new(),
    };
    Gla {
        name: "hand".into(),
        scale: 1.0,
        bones: vec![
            bone("model_root", None, 0.0, 0.0),
            bone("pelvis", Some(0), 0.0, 40.0),
            bone("rradius", Some(1), 5.0, 40.0),
            bone("rhand", Some(2), 10.0, 40.0),
            bone("r_d2_j1", Some(2), 14.0, 40.0),
        ],
        frames: Vec::new(),
        compressed_bones: Vec::new(),
    }
}

/// A rig with the finger under the hand, and a clip turning the hand a quarter turn
/// about the game's up axis.
fn hand_rig() -> GltfModel {
    let gltf = |x: f32, z: f32| Vec3A::new(x, z, 0.0);
    let joint = |name: &str, parent, at: Vec3A, local: Vec3A| GltfJoint {
        name: name.to_owned(),
        parent,
        bind: translated(at),
        inverse_bind: translated(-at),
        rest: translated(local),
    };
    let half = std::f32::consts::FRAC_1_SQRT_2;
    GltfModel {
        joints: vec![
            joint("pelvis", None, gltf(0.0, 40.0), gltf(0.0, 40.0)),
            joint("hand_r", Some(0), gltf(10.0, 40.0), gltf(10.0, 0.0)),
            joint("middle_01_r", Some(1), gltf(14.0, 40.0), gltf(4.0, 0.0)),
        ],
        animations: vec![GltfAnimation {
            name: "Wave".into(),
            channels: vec![GltfChannel {
                joint: 1,
                path: GltfPath::Rotation,
                times: vec![0.0, 1.0],
                values: vec![[0.0, 0.0, 0.0, 1.0], [0.0, half, 0.0, half]],
                step: false,
            }],
        }],
        ..Default::default()
    }
}

#[test]
fn a_stock_finger_stays_on_the_turning_hand() {
    let gla = hand_driver();
    let rig = RigModel::fit(hand_rig(), "", &manifest(), &gla).unwrap();
    let stock = StockEmotes::of_source(&gla, Arc::new(rig)).expect("the stock skeleton");
    let mut scratch = StockScratch::default();
    let driver = vec![translated(Vec3A::ZERO); 5];
    let play = stock.play("Wave", 1.0, None, false).expect("the clip");
    assert!(stock.pose(&driver, play, &mut scratch));
    let at = positions(&gla, &scratch.matrices);
    // The hand turns in place; the finger, hung on the forearm in the GLA, goes
    // round with the hand rather than staying where the forearm holds it.
    assert!(
        (at[3] - Vec3A::new(10.0, 0.0, 40.0)).length() < 1e-3,
        "{at:?}"
    );
    assert!(
        (at[4] - Vec3A::new(10.0, 4.0, 40.0)).length() < 1e-3,
        "{at:?}"
    );
}

/// The penetration of every limb point of `posed` into the stock body (each point's
/// clearance from each part, less the limb's thickness and the base pose's own),
/// deepest first, and how near the stock limbs stay to their body where the rig's
/// touch its own.
fn depths(fit: &Fit, posed: &[Affine3A], inverse_base: &[Affine3A]) -> (f32, f32) {
    let mut deepest = 0.0f32;
    let mut total = 0.0f32;
    for limb in &fit.limbs {
        let at = |bone: usize| posed[bone].translation;
        let (elbow, wrist) = (at(limb.stock.lower), at(limb.stock.end));
        let points = [elbow, (elbow + wrist) * 0.5, wrist];
        for part in &limb.parts {
            let transform = posed[part.stock.bone] * inverse_base[part.stock.bone];
            for (k, point) in points.iter().enumerate() {
                let (clear, _) = super::super::body::clearance(&part.stock, &transform, *point);
                let clear = clear - limb.stock.radius - part.rest_stock[k].min(0.0);
                deepest = deepest.max(-clear);
                total += (-clear).max(0.0);
            }
        }
    }
    (deepest, total)
}

/// A slim body's animations played on a wide model, with and without the body fit:
/// how deep the wide model's limbs sink into its body.
#[test]
#[ignore = "reads retail and community models (SJK_TEST_GLA, SJK_TEST_ANIMATIONS, SJK_TEST_SLIM, SJK_TEST_WIDE)"]
fn body_fit_keeps_wide_models_limbs_out_of_their_bodies() {
    let Some((gla, config)) = retail() else {
        eprintln!("SJK_TEST_GLA and SJK_TEST_ANIMATIONS are not set");
        return;
    };
    let read = |name: &str| -> Option<Glm> {
        Glm::parse(&std::fs::read(std::env::var_os(name)?).ok()?).ok()
    };
    let (Some(slim), Some(wide)) = (read("SJK_TEST_SLIM"), read("SJK_TEST_WIDE")) else {
        eprintln!("SJK_TEST_SLIM and SJK_TEST_WIDE name two GLM files");
        return;
    };
    let slim_body = Body::of_glm(&gla, &slim);
    let wide_body = Body::of_glm(&gla, &wide);
    for (name, body) in [("slim", &slim_body), ("wide", &wide_body)] {
        eprintln!(
            "{name}: parts {}, limb radii {:?}",
            body.parts
                .iter()
                .map(|p| format!(
                    "{} r=({:.1},{:.1},{:.1})",
                    gla.bones[p.bone].name, p.radii.x, p.radii.y, p.radii.z
                ))
                .collect::<Vec<_>>()
                .join(", "),
            body.limbs
                .iter()
                .map(|l| l.map(|l| (l.radius * 10.0).round() / 10.0))
                .collect::<Vec<_>>()
        );
    }
    let standing = frames(&gla, &config, "BOTH_STAND1");
    let mut before_all = (0.0f32, 0.0f32);
    let mut after_all = (0.0f32, 0.0f32);
    for animation in [
        "BOTH_STAND1IDLE1",
        "BOTH_ENGAGETAUNT",
        "BOTH_GESTURE1",
        "BOTH_STAND10_TALK1",
        "BOTH_CROUCH1IDLE",
        "BOTH_RUN1",
        "BOTH_ROLL_F",
        "BOTH_SIT2TOSTAND5",
    ] {
        if config.get(animation).is_none() {
            continue;
        }
        let clip_frames = frames(&gla, &config, animation);
        let mut rig = RigModel::fit(mannequin_on(&gla, 1.0), "", &manifest(), &gla).unwrap();
        rig.body = Some(slim_body.clone());
        let clip = clip_of(&rig, &clip_frames, animation);
        rig.clips.push(clip);
        let rig = Arc::new(rig);
        let fitted = Reverse::of(&rig, &gla, Some(&wide_body)).expect("the reverse");
        let fit = fitted.fit.as_ref().expect("a body fit");
        let plain = Reverse::of(&rig, &gla, None).expect("the reverse");
        let stock = |reverse: Reverse| StockEmotes {
            clips: vec![StockClip {
                name: animation.to_owned(),
                source: rig.clone(),
                clip: 0,
                reverse: Arc::new(reverse),
            }],
            ..StockEmotes::with_clips(&gla, Vec::new())
        };
        let (with_fit, without) = (stock(fitted.clone()), stock(plain));
        let mut before = (0.0f32, 0.0f32);
        let mut after = (0.0f32, 0.0f32);
        let mut scratch = StockScratch::default();
        // The frame where the unfitted limbs sink deepest, for the OBJ pictures.
        let mut worst = (0.0f32, 0usize);
        for frame in 0..clip_frames.len() {
            let play = ClipPlay {
                clip: 0,
                time: frame as f32 / 30.0,
                weight: 1.0,
            };
            for (emotes, sum) in [(&without, &mut before), (&with_fit, &mut after)] {
                assert!(emotes.pose(&standing[0], play, &mut scratch));
                let (deep, total) = depths(fit, &scratch.posed, &emotes.inverse_base);
                if std::ptr::eq(emotes, &without) && deep > worst.0 {
                    worst = (deep, frame);
                }
                sum.0 = sum.0.max(deep);
                sum.1 += total / clip_frames.len() as f32;
            }
        }
        eprintln!(
            "{animation}: deepest {:.1} -> {:.1} units, mean summed depth {:.1} -> {:.1}",
            before.0, after.0, before.1, after.1
        );
        if let Some(directory) = std::env::var_os("SJK_TEST_OBJ_DIR")
            && worst.0 > 0.0
        {
            let play = ClipPlay {
                clip: 0,
                time: worst.1 as f32 / 30.0,
                weight: 1.0,
            };
            for (emotes, label) in [(&without, "before"), (&with_fit, "after")] {
                assert!(emotes.pose(&standing[0], play, &mut scratch));
                let path =
                    std::path::Path::new(&directory).join(format!("{animation}_{label}.obj"));
                std::fs::write(path, obj(&wide, &scratch.matrices)).expect("the OBJ");
            }
        }
        before_all = (before_all.0.max(before.0), before_all.1 + before.1);
        after_all = (after_all.0.max(after.0), after_all.1 + after.1);
    }
    eprintln!(
        "all: deepest {:.1} -> {:.1}, summed depth {:.1} -> {:.1}",
        before_all.0, after_all.0, before_all.1, after_all.1
    );
    assert!(after_all.1 <= before_all.1);
}

/// `mesh`'s first LOD skinned with `matrices`, as an OBJ (drawn surfaces only).
fn obj(mesh: &Glm, matrices: &[[[f32; 4]; 3]]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let mut first = 1;
    let Some(lod) = mesh.lods.first() else {
        return out;
    };
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
            let at = Vec3A::from_array(vertex.position);
            let mut skinned = Vec3A::ZERO;
            for weight in &vertex.weights {
                if let Some(matrix) = surface
                    .bone_references
                    .get(weight.bone_reference)
                    .and_then(|bone| matrices.get(*bone))
                {
                    skinned += affine(matrix).transform_point3a(at) * weight.weight;
                }
            }
            let _ = writeln!(out, "v {} {} {}", skinned.x, skinned.y, skinned.z);
        }
        for triangle in &surface.triangles {
            let _ = writeln!(
                out,
                "f {} {} {}",
                first + triangle[0] as usize,
                first + triangle[1] as usize,
                first + triangle[2] as usize
            );
        }
        first += surface.vertices.len();
    }
    out
}

use super::*;
use sjk_model::GlaBone;
use sjk_model::gltf::{GltfAnimation, GltfChannel, GltfJoint, GltfPath, GltfPrimitive};

fn identity_rows() -> [[f32; 4]; 3] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ]
}

fn translated(x: f32, y: f32, z: f32) -> [[f32; 4]; 3] {
    [[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, z]]
}

/// A driver with a pelvis 40 units up and a cranium above it.
fn driver() -> Gla {
    let bone = |name: &str, parent, base| GlaBone {
        name: name.to_owned(),
        flags: 0,
        parent,
        base_pose: base,
        inverse_base_pose: identity_rows(),
        children: Vec::new(),
    };
    Gla {
        name: "_humanoid".into(),
        scale: 1.0,
        bones: vec![
            bone("model_root", None, identity_rows()),
            bone("pelvis", Some(0), translated(0.0, 0.0, 40.0)),
            bone("cranium", Some(1), translated(0.0, 0.0, 60.0)),
        ],
        frames: Vec::new(),
        compressed_bones: Vec::new(),
    }
}

/// A rig with a pelvis one metre up (glTF +Y) and a head half a metre above it,
/// one vertex on the head, and a clip "Nod" turning the head a quarter turn about
/// glTF +Y (up) over one second.
fn rig() -> GltfModel {
    let joint = |name: &str, parent, y: f32, rest: f32| GltfJoint {
        name: name.to_owned(),
        parent,
        bind: translated(0.0, y, 0.0),
        inverse_bind: translated(0.0, -y, 0.0),
        rest: translated(0.0, rest, 0.0),
    };
    let half = std::f32::consts::FRAC_1_SQRT_2;
    GltfModel {
        animations: vec![GltfAnimation {
            name: "Nod".into(),
            channels: vec![GltfChannel {
                joint: 1,
                path: GltfPath::Rotation,
                times: vec![0.0, 1.0],
                values: vec![[0.0, 0.0, 0.0, 1.0], [0.0, half, 0.0, half]],
                step: false,
            }],
        }],
        joints: vec![
            joint("pelvis", None, 1.0, 1.0),
            joint("head", Some(0), 1.5, 0.5),
        ],
        primitives: vec![GltfPrimitive {
            base_color_image: Some("textures/skin.png".into()),
            positions: vec![[0.0, 1.6, 0.1]; 3],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            texture_coordinates: vec![[0.0; 2]; 3],
            joints: vec![[1, 0, 0, 0]; 3],
            weights: vec![[1.0, 0.0, 0.0, 0.0]; 3],
            indices: vec![0, 1, 2],
            ..Default::default()
        }],
    }
}

fn manifest(text: &str) -> Manifest {
    Manifest::parse(text).expect("the manifest")
}

#[test]
fn a_manifest_reads_every_key_and_rejects_nonsense() {
    let parsed = manifest(
        "// wolf\nmodel wolf.gltf\ndriver models/players/jedi_hm/\npreset Mannequin\nscale 40\nmap tail pelvis\nmap ear_l cranium as head\n",
    );
    assert_eq!(parsed.model.as_deref(), Some("wolf.gltf"));
    assert_eq!(parsed.driver, "models/players/jedi_hm");
    assert_eq!(parsed.scale, Some(40.0));
    assert_eq!(parsed.map[1].align, Align::As("head".into()));
    assert!(Manifest::parse("scale -1").is_err());
    assert!(Manifest::parse("colour red").is_err());
    assert_eq!(manifest("").driver, DEFAULT_DRIVER);
}

#[test]
fn manifest_map_lines_replace_preset_entries() {
    let map = manifest("map head cervical").bone_map().expect("the map");
    let head: Vec<_> = map.iter().filter(|e| e.joint == "head").collect();
    assert_eq!(head.len(), 1);
    assert_eq!(head[0].bone, "cervical");
    assert!(manifest("preset nothing").bone_map().is_err());
}

#[test]
fn auto_scale_fits_the_pelvis_and_the_axes_turn_to_z_up() {
    let rig = RigModel::fit(rig(), "models/players/wolf", &manifest(""), &driver()).unwrap();
    // 1 m pelvis onto a 40-unit pelvis: 40 units a metre; glTF +Y becomes +Z,
    // glTF +Z (front) becomes -Y.
    let vertex = rig.surfaces[0].vertices[0].position;
    assert!((vertex[2] - 64.0).abs() < 1e-4, "{vertex:?}");
    assert!((vertex[1] + 4.0).abs() < 1e-4, "{vertex:?}");
    assert_eq!(
        rig.surfaces[0].shader,
        "models/players/wolf/textures/skin.png"
    );
}

#[test]
fn the_base_pose_gives_the_bind_pose() {
    let rig = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    let bones = vec![identity_rows(); 3];
    let mut scratch = Scratch::default();
    rig.pose(&bones, None, &mut scratch);
    let palette = &scratch.palette;
    for matrix in palette {
        for (row, expected) in matrix.iter().zip(identity_rows()) {
            for (a, b) in row.iter().zip(expected) {
                assert!((a - b).abs() < 1e-4, "{palette:?}");
            }
        }
    }
}

#[test]
fn a_turned_driver_bone_turns_its_joint_and_children() {
    let rig = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    // The cranium turned 90 degrees about Z around its base position (0, 0, 60).
    let turn = [
        [0.0, -1.0, 0.0, 60.0],
        [1.0, 0.0, 0.0, 60.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    let bones = vec![identity_rows(), identity_rows(), turn];
    let mut scratch = Scratch::default();
    rig.pose(&bones, None, &mut scratch);
    let vertex = Vec3A::from_array(rig.surfaces[0].vertices[0].position);
    let moved = affine(&scratch.palette[1]).transform_point3a(vertex);
    // The CPU path skins the same way.
    let (cpu, _) = skin(&rig.surfaces[0].vertices[0], &scratch.palette);
    assert!((Vec3A::from_array(cpu) - moved).length() < 1e-4);
    // Before: 4 units in front (-Y) of the head joint; after a quarter turn about +Z
    // the offset points along +X, and the head joint itself stays where it was.
    let head = rig.joints[1].bind.translation;
    let offset = moved - head;
    assert!(
        (offset.x - 4.0).abs() < 1e-3 && offset.y.abs() < 1e-3,
        "{offset:?}"
    );
}

#[test]
fn triangles_turn_to_the_game_winding() {
    let rig = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    // glTF fronts are counter-clockwise; the game's `cull front` keeps clockwise ones.
    assert_eq!(rig.surfaces[0].indices, vec![0, 2, 1]);
}

#[test]
fn a_bolt_moves_from_the_driver_bone_onto_the_rig_joint() {
    let mut driver = driver();
    driver.bones.push(GlaBone {
        name: "ceyebrow".into(),
        flags: 0,
        parent: Some(2),
        base_pose: translated(0.0, -3.0, 62.0),
        inverse_base_pose: identity_rows(),
        children: Vec::new(),
    });
    // 50 units a metre: the rig's head joint sits at 75, the driver's cranium at 60.
    let rig = RigModel::fit(rig(), "", &manifest("scale 50"), &driver).unwrap();
    // The cranium shifted 10 units along +X; its bolt 5 in front and 5 above it.
    let bones = vec![
        identity_rows(),
        identity_rows(),
        translated(10.0, 0.0, 0.0),
        translated(10.0, 0.0, 0.0),
    ];
    let bolt = translated(10.0, -5.0, 65.0);
    let moved = rig.retarget_bolt(2, &bones, None, bolt);
    let origin = Vec3A::new(moved[0][3], moved[1][3], moved[2][3]);
    assert!(
        (origin - Vec3A::new(0.0, -5.0, 80.0)).length() < 1e-4,
        "{origin:?}"
    );
    // The orientation is the driver's.
    assert_eq!(moved[0][..3], bolt[0][..3]);
    // An unmapped bone (the eyebrow) follows its mapped parent's joint.
    let point = rig.retarget_point(3, &bones, None, Vec3A::new(10.0, -5.0, 65.0));
    assert!((point - origin).length() < 1e-4, "{point:?}");
}

/// The head vertex skinned with `scratch`'s palette, minus the head joint.
fn head_offset(rig: &RigModel, scratch: &Scratch) -> Vec3A {
    let vertex = Vec3A::from_array(rig.surfaces[0].vertices[0].position);
    affine(&scratch.palette[1]).transform_point3a(vertex) - rig.joints[1].bind.translation
}

#[test]
fn a_clip_plays_on_the_rigs_own_skeleton() {
    let rig = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    assert_eq!(rig.clips.len(), 1);
    assert_eq!(rig.clips[0].duration, 1.0);
    let bones = vec![identity_rows(); 3];
    let mut scratch = Scratch::default();
    // At its first key the clip is the rest pose: the vertex stays 4 units in front
    // (-Y) and 4 above the head joint.
    let play = rig.play("Nod", 0.0, None, false).map(|play| ClipPlay {
        weight: 1.0,
        ..play
    });
    rig.pose(&bones, play, &mut scratch);
    let offset = head_offset(&rig, &scratch);
    assert!(
        (offset - Vec3A::new(0.0, -4.0, 4.0)).length() < 1e-4,
        "{offset:?}"
    );
    // At its end, a quarter turn about glTF +Y is one about the game's +Z.
    let play = rig.play("Nod", 1.0, None, false);
    assert_eq!(play.map(|play| play.weight), Some(1.0));
    rig.pose(&bones, play, &mut scratch);
    let offset = head_offset(&rig, &scratch);
    assert!(
        (offset - Vec3A::new(4.0, 0.0, 4.0)).length() < 1e-3,
        "{offset:?}"
    );
    // Half faded in, half way round.
    let half = ClipPlay {
        weight: 0.5,
        ..play.unwrap()
    };
    rig.pose(&bones, Some(half), &mut scratch);
    let offset = head_offset(&rig, &scratch);
    let side = 4.0 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (offset - Vec3A::new(side, -side, 4.0)).length() < 1e-3,
        "{offset:?}"
    );
    assert!(rig.play("Wave", 0.0, None, false).is_none());
}

#[test]
fn a_clip_fades_in_and_out_and_a_loop_wraps() {
    let play = ClipPlay::new(0, 2.0, 0.1, Some(5.0), false);
    assert!((play.weight - 0.4).abs() < 1e-6 && play.time == 0.1);
    let play = ClipPlay::new(0, 2.0, 3.0, Some(0.05), false);
    assert!((play.weight - 0.2).abs() < 1e-6 && play.time == 2.0);
    let play = ClipPlay::new(0, 2.0, 5.5, None, true);
    assert!(play.weight == 1.0 && (play.time - 1.5).abs() < 1e-6);
}

#[test]
fn a_bolt_follows_the_joint_a_clip_moves() {
    let rig = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    let bones = vec![identity_rows(); 3];
    let mut scratch = Scratch::default();
    let bolt = translated(0.0, -5.0, 65.0);
    rig.pose(&bones, None, &mut scratch);
    let still = rig.retarget_bolt(2, &bones, Some(&scratch), bolt);
    assert_eq!(still, rig.retarget_bolt(2, &bones, None, bolt));
    rig.pose(&bones, rig.play("Nod", 1.0, None, false), &mut scratch);
    let turned = rig.retarget_bolt(2, &bones, Some(&scratch), bolt);
    let origin = Vec3A::new(turned[0][3], turned[1][3], turned[2][3]);
    assert!(
        (origin - Vec3A::new(5.0, 0.0, 65.0)).length() < 1e-3,
        "{origin:?}"
    );
    // Its axes turn with the head: X now points along +Y.
    assert!((turned[1][0] - 1.0).abs() < 1e-3, "{turned:?}");
}

/// The lowest point of the driver (Kyle) and of a rig model posed from the same
/// frames, to see where the rig's feet land against the driver's.
#[test]
#[ignore = "reads the retail Kyle and an installed rig model (JKA_GAME_DATA, SJK_TEST_RIG)"]
fn feet_against_the_driver() {
    let (Some(data), Ok(name)) = (
        std::env::var_os("JKA_GAME_DATA").map(std::path::PathBuf::from),
        std::env::var("SJK_TEST_RIG"),
    ) else {
        println!("JKA_GAME_DATA or SJK_TEST_RIG is not set: skipped");
        return;
    };
    let mut vfs = VirtualFileSystem::new();
    let base = data.join("base");
    for entry in std::fs::read_dir(&base).unwrap().flatten() {
        let path = entry.path();
        let file = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        if file.starts_with("assets") && file.ends_with(".pk3") || file == format!("rig_{name}.pk3")
        {
            vfs.mount_pk3(&path).unwrap();
        }
    }
    vfs.mount_directory(&base).unwrap();
    let read = |path: &str| vfs.read(path).unwrap().unwrap().bytes;
    let gla = Gla::parse(&read("models/players/_humanoid/_humanoid.gla")).unwrap();
    let glm = sjk_model::Glm::parse(&read("models/players/kyle/model.glm")).unwrap();
    let kyle_skin = sjk_model::Skin::parse(&read("models/players/kyle/model_default.skin"));
    let config =
        sjk_model::AnimationConfig::parse(&read("models/players/_humanoid/animation.cfg")).unwrap();
    let directory = format!("models/players/{name}");
    let manifest = super::manifest(&vfs, &directory)
        .unwrap()
        .expect("a rig model");
    let rig = RigModel::load(&vfs, &directory, &manifest, &gla, &glm).unwrap();
    let mut scratch = Scratch::default();
    let identity = vec![
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0]
        ];
        gla.bones.len()
    ];
    let kyle_base = glm
        .skin_pose_matrices(&kyle_skin, 0, &identity)
        .unwrap()
        .into_iter()
        .filter(|s| s.shader.is_some() && !s.name.contains("_cap_") && !s.name.starts_with('*'))
        .flat_map(|s| s.vertices)
        .map(|v| v.position[2])
        .fold(f32::MAX, f32::min);
    rig.pose(&identity, None, &mut scratch);
    let rig_base = rig
        .surfaces
        .iter()
        .flat_map(|s| &s.vertices)
        .map(|v| skin(v, &scratch.palette).0[2])
        .fold(f32::MAX, f32::min);
    println!(
        "base pose: Kyle lowest {kyle_base:.2} (raw {:?}), rig lowest {rig_base:.2} ({:+.2})",
        super::sole(&glm),
        rig_base - kyle_base
    );
    for (animation, frame) in [
        ("BOTH_STAND1", 0),
        ("BOTH_STAND2", 0),
        ("BOTH_RUN1", 4),
        ("BOTH_CROUCH1IDLE", 0),
        ("BOTH_A2_T__B_", 0),
        ("BOTH_WALK1", 3),
    ] {
        let sequence = config.get(animation).unwrap();
        let frame = sequence.first_frame + frame;
        let kyle = glm
            .skin(&gla, &kyle_skin, frame, 0)
            .unwrap()
            .into_iter()
            .filter(|s| s.shader.is_some() && !s.name.contains("_cap_") && !s.name.starts_with('*'))
            .flat_map(|s| s.vertices)
            .map(|v| v.position[2])
            .fold(f32::MAX, f32::min);
        let matrices = gla.frame_matrices(frame).unwrap();
        rig.pose(&matrices, None, &mut scratch);
        let lowest = rig
            .surfaces
            .iter()
            .flat_map(|s| &s.vertices)
            .map(|v| skin(v, &scratch.palette).0[2])
            .fold(f32::MAX, f32::min);
        let talus = gla.bones.iter().position(|b| b.name == "ltalus").unwrap();
        let kyle_ankle = affine(&matrices[talus])
            .transform_point3a(affine(&gla.bones[talus].base_pose).translation)
            .z;
        let foot = rig.joints.len();
        println!(
            "{animation}: Kyle lowest {kyle:.2}, ankle {kyle_ankle:.2}; rig lowest {lowest:.2} ({:+.2}), {foot} joints",
            lowest - kyle
        );
        assert!(
            (lowest - kyle).abs() < 1.5,
            "{animation}: feet off by {}",
            lowest - kyle
        );
        // A clip of the rig's own over this frame stands on the same ground.
        if !rig.clips.is_empty() {
            let play = ClipPlay {
                clip: 0,
                time: 0.0,
                weight: 1.0,
            };
            rig.pose(&matrices, Some(play), &mut scratch);
            let dancing = rig
                .surfaces
                .iter()
                .flat_map(|s| &s.vertices)
                .map(|v| skin(v, &scratch.palette).0[2])
                .fold(f32::MAX, f32::min);
            println!(
                "  clip {}: lowest {dancing:.2} ({:+.2})",
                rig.clips[0].name,
                dancing - kyle
            );
        }
    }
}

#[test]
fn the_feet_land_on_the_drivers_ground_and_lift_raises_them() {
    // A driver whose soles are 2 units up in its base pose, its ankles at 10, and a
    // rig whose ankles stand 0.1 m (4 units at 40 a metre) above its soles at 0.
    let mut driver = driver();
    for (name, x) in [("ltalus", 5.0), ("rtalus", -5.0)] {
        driver.bones.push(GlaBone {
            name: name.into(),
            flags: 0,
            parent: Some(1),
            base_pose: translated(x, 0.0, 10.0),
            inverse_base_pose: identity_rows(),
            children: Vec::new(),
        });
    }
    let mut model = rig();
    let joint = |name: &str, x: f32| GltfJoint {
        name: name.to_owned(),
        parent: Some(0),
        bind: translated(x, 0.1, 0.0),
        inverse_bind: translated(-x, -0.1, 0.0),
        rest: translated(x, -0.9, 0.0),
    };
    model.joints.push(joint("foot_l", 0.1));
    model.joints.push(joint("foot_r", -0.1));
    // A sole vertex at 0 under the left foot.
    let primitive = &mut model.primitives[0];
    primitive.positions.push([0.1, 0.0, 0.0]);
    primitive.normals.push([0.0, 1.0, 0.0]);
    primitive.texture_coordinates.push([0.0; 2]);
    primitive.joints.push([2, 0, 0, 0]);
    primitive.weights.push([1.0, 0.0, 0.0, 0.0]);
    let fit = |text: &str| {
        RigModel::fit(model.clone(), "", &manifest(text), &driver)
            .unwrap()
            .with_feet(&driver, Some(2.0))
    };
    let rig = fit("map foot_l ltalus\nmap foot_r rtalus");
    let mut bones = vec![identity_rows(); 5];
    // The driver sinks 27 units, as a GLA frame does, and lifts its right foot.
    for bone in &mut bones[1..] {
        bone[2][3] = -27.0;
    }
    bones[4][2][3] = -20.0;
    let mut scratch = Scratch::default();
    rig.pose(&bones, None, &mut scratch);
    let lowest = |scratch: &Scratch| {
        rig.surfaces[0]
            .vertices
            .iter()
            .map(|v| skin(v, &scratch.palette).0[2])
            .fold(f32::MAX, f32::min)
    };
    // The driver's soles are at 2 - 27 = -25; so are the rig's, which the pelvis
    // alone puts at -27.
    assert!(
        (lowest(&scratch) + 25.0).abs() < 1e-3,
        "{}",
        lowest(&scratch)
    );
    let unplanted = RigModel::fit(model.clone(), "", &manifest(""), &driver).unwrap();
    let mut other = Scratch::default();
    unplanted.pose(&bones, None, &mut other);
    let low = unplanted.surfaces[0]
        .vertices
        .iter()
        .map(|v| skin(v, &other.palette).0[2])
        .fold(f32::MAX, f32::min);
    assert!((low + 27.0).abs() < 1e-3, "{low}");
    let lifted = fit("map foot_l ltalus\nmap foot_r rtalus\nlift 2");
    lifted.pose(&bones, None, &mut scratch);
    let low = lifted.surfaces[0]
        .vertices
        .iter()
        .map(|v| skin(v, &scratch.palette).0[2])
        .fold(f32::MAX, f32::min);
    assert!((low + 23.0).abs() < 1e-3, "{low}");
    assert!(Manifest::parse("lift up").is_err());
}

/// [`rig`]'s skeleton with a longer neck (the head 0.8 metres above the pelvis), a
/// tail the clip knows nothing of, and no clip of its own.
fn longer_rig() -> GltfModel {
    let mut model = rig();
    model.animations.clear();
    model.joints[1].bind = translated(0.0, 1.8, 0.0);
    model.joints[1].inverse_bind = translated(0.0, -1.8, 0.0);
    model.joints[1].rest = translated(0.0, 0.8, 0.0);
    model.joints.push(GltfJoint {
        name: "tail".into(),
        parent: Some(0),
        bind: translated(0.0, 0.9, 0.2),
        inverse_bind: translated(0.0, -0.9, -0.2),
        rest: translated(0.0, -0.1, 0.2),
    });
    model
}

#[test]
fn a_rig_with_the_same_joint_names_borrows_a_clip_and_keeps_its_lengths() {
    let source = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    let mut target = RigModel::fit(longer_rig(), "", &manifest(""), &driver()).unwrap();
    assert!(target.clips.is_empty());
    let clip = target
        .borrowed(&source, &source.clips[0])
        .expect("the same names");
    assert_eq!(clip.name, "Nod");
    assert_eq!(clip.duration, 1.0);
    target.clips.push(clip);
    let bones = vec![identity_rows(); 3];
    let mut scratch = Scratch::default();
    target.pose(&bones, target.play("Nod", 1.0, None, false), &mut scratch);
    // The head turns a quarter about the game's +Z, as the source's does, and stays
    // on its own longer neck (0.8 m above the pelvis at 40 units a metre).
    let head = scratch.pose[1];
    let turned = Mat3A::from_rotation_z(std::f32::consts::FRAC_PI_2);
    assert!(head.matrix3.abs_diff_eq(turned, 1e-4), "{head:?}");
    let expected = target.joints[1].bind.translation;
    assert!((head.translation - expected).length() < 1e-3, "{head:?}");
    assert!((expected.z - 72.0).abs() < 1e-3, "{expected:?}");
    // The tail, unknown to the clip, stays where the pelvis holds it.
    let tail = scratch.pose[2];
    assert!(tail.abs_diff_eq(target.joints[2].bind, 1e-4), "{tail:?}");
}

#[test]
fn a_rig_without_the_clips_joints_does_not_borrow_it() {
    let source = RigModel::fit(rig(), "", &manifest(""), &driver()).unwrap();
    let mut other = longer_rig();
    other.joints[1].name = "skull".into();
    let mut map = manifest("map skull cranium");
    map.preset = "mannequin".into();
    let target = RigModel::fit(other, "", &map, &driver()).unwrap();
    assert!(target.borrowed(&source, &source.clips[0]).is_none());
}

#[test]
fn an_emote_names_the_rig_its_clip_comes_from() {
    let def = crate::emotes::parse(
        "kane_dance",
        r#"{"name": "Dance", "clip": "kane_dance", "length_ms": 6800, "rig": "kane"}"#,
    )
    .unwrap();
    assert_eq!(def.rig, "kane");
    for bad in ["../kane", "a/b", r"a\b"] {
        let text = format!(r#"{{"name": "Dance", "length_ms": 6800, "rig": "{bad}"}}"#);
        assert!(crate::emotes::parse("kane_dance", &text).is_err(), "{bad}");
    }
}

/// [`rig`] with a three-joint tail behind the pelvis (glTF +Z is the front, so the
/// tail runs along -Z), moving on its own.
fn tailed() -> RigModel {
    tailed_with("dynamic tail_root")
}

/// [`tailed`] with the manifest `text`.
fn tailed_with(text: &str) -> RigModel {
    let mut model = rig();
    let joint = |name: &str, parent, z: f32, rest: f32| GltfJoint {
        name: name.to_owned(),
        parent: Some(parent),
        bind: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, z],
        ],
        inverse_bind: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, -1.0],
            [0.0, 0.0, 1.0, -z],
        ],
        rest: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, rest],
        ],
    };
    model.joints.push(joint("tail_root", 0, -0.1, -0.1));
    model.joints.push(joint("tail_1", 2, -0.4, -0.3));
    model.joints.push(joint("tail_2", 3, -0.7, -0.3));
    RigModel::fit(model, "", &manifest(text), &driver()).unwrap()
}

#[test]
fn a_dynamic_tail_lags_behind_the_body_then_catches_up() {
    let rig = tailed();
    let mut scratch = Scratch::default();
    let still = vec![identity_rows(); 3];
    rig.pose_after(&still, None, &mut scratch, None);
    let length = |pose: &[Affine3A]| (pose[4].translation - pose[3].translation).length();
    let bone = length(&scratch.pose);
    // The body steps 10 units sideways: on the next frame the tip has not followed.
    let mut moved = still.clone();
    moved[1] = translated(10.0, 0.0, 0.0);
    rig.pose_after(&moved, None, &mut scratch, Some(1.0 / 60.0));
    let root = scratch.pose[2].translation.x;
    let tip = scratch.pose[4].translation.x;
    assert!((root - 10.0).abs() < 1e-3, "{root}");
    assert!(tip < 5.0, "the tip lags: {tip}");
    assert!((length(&scratch.pose) - bone).abs() < 1e-3);
    // Three seconds later it is back behind the body, give or take the wag.
    for _ in 0..180 {
        rig.pose_after(&moved, None, &mut scratch, Some(1.0 / 60.0));
    }
    let tip = scratch.pose[4].translation.x;
    assert!((tip - 10.0).abs() < 2.0 * dynamics::WAG_UNITS, "{tip}");
    assert!((length(&scratch.pose) - bone).abs() < 1e-3);
}

#[test]
fn a_dynamic_tail_wags_while_the_body_stands_still() {
    let rig = tailed();
    let mut scratch = Scratch::default();
    let still = vec![identity_rows(); 3];
    rig.pose_after(&still, None, &mut scratch, None);
    let (mut left, mut right) = (0.0f32, 0.0f32);
    for _ in 0..300 {
        rig.pose_after(&still, None, &mut scratch, Some(1.0 / 60.0));
        let tip = scratch.pose[4].translation.x;
        left = left.min(tip);
        right = right.max(tip);
    }
    assert!(left < -0.5 && right > 0.5, "{left} {right}");
    assert!(left > -2.0 * dynamics::WAG_UNITS && right < 2.0 * dynamics::WAG_UNITS);
    // The body itself does not move.
    assert!(scratch.pose[0].abs_diff_eq(rig.joints[0].bind, 1e-4));
    // A long hitch starts the tail again where the animation puts it.
    rig.pose_after(&still, None, &mut scratch, Some(2.0));
    assert!(scratch.pose[3].translation.x.abs() < dynamics::WAG_UNITS);
}

#[test]
fn a_manifest_names_its_dynamic_joints() {
    let parsed = manifest("dynamic tail_root\ndynamic hair 0");
    assert_eq!(
        parsed.dynamic,
        [
            ("tail_root".to_owned(), None),
            ("hair".to_owned(), Some(0.0))
        ]
    );
    assert!(Manifest::parse("dynamic tail heavy").is_err());
    let rig = RigModel::fit(rig(), "", &manifest("dynamic nothing"), &driver()).unwrap();
    assert!(rig.chains.is_empty());
}

#[test]
fn gravity_hangs_a_tail_and_a_dynamic_line_can_turn_it_off() {
    let still = vec![identity_rows(); 3];
    let tip_drop = |text: &str| {
        let rig = tailed_with(text);
        let mut scratch = Scratch::default();
        rig.pose_after(&still, None, &mut scratch, None);
        for _ in 0..240 {
            rig.pose_after(&still, None, &mut scratch, Some(1.0 / 60.0));
        }
        rig.joints[4].bind.translation.z - scratch.pose[4].translation.z
    };
    // The game's gravity against the spring: the tip hangs several units lower.
    let hanging = tip_drop("dynamic tail_root");
    assert!(hanging > 5.0 && hanging < 30.0, "{hanging}");
    // `dynamic <joint> 0`: no gravity, the tip stays level with its rest.
    let level = tip_drop("dynamic tail_root 0");
    assert!(level.abs() < 1.0, "{level}");
}

/// An installed rig model's dynamic chains settled in its bind pose: how far each
/// chain's tip stands above (or below) its anchor, and at what angle from level, with
/// the manifest's gravity and with none. `SJK_TEST_PACK` names the pack holding it
/// (default `rig_<name>.pk3`).
#[test]
#[ignore = "reads the retail skeleton and an installed rig model (JKA_GAME_DATA, SJK_TEST_RIG)"]
fn dynamic_chains_of_an_installed_rig() {
    let (Some(data), Ok(name)) = (
        std::env::var_os("JKA_GAME_DATA").map(std::path::PathBuf::from),
        std::env::var("SJK_TEST_RIG"),
    ) else {
        println!("JKA_GAME_DATA or SJK_TEST_RIG is not set: skipped");
        return;
    };
    let pack = std::env::var("SJK_TEST_PACK").unwrap_or_else(|_| format!("rig_{name}.pk3"));
    let mut vfs = VirtualFileSystem::new();
    let base = data.join("base");
    for entry in std::fs::read_dir(&base).unwrap().flatten() {
        let path = entry.path();
        let file = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        if file.starts_with("assets") && file.ends_with(".pk3") || file == pack.to_ascii_lowercase()
        {
            vfs.mount_pk3(&path).unwrap();
        }
    }
    let read = |path: &str| vfs.read(path).unwrap().unwrap().bytes;
    let gla = Gla::parse(&read("models/players/_humanoid/_humanoid.gla")).unwrap();
    let glm = sjk_model::Glm::parse(&read("models/players/kyle/model.glm")).unwrap();
    let directory = format!("models/players/{name}");
    let manifest = super::manifest(&vfs, &directory)
        .unwrap()
        .expect("a rig model");
    let mut weightless = manifest.clone();
    for line in &mut weightless.dynamic {
        line.1 = Some(0.0);
    }
    let bones = vec![identity_rows(); gla.bones.len()];
    for (label, manifest) in [("manifest", &manifest), ("no gravity", &weightless)] {
        let rig = RigModel::load(&vfs, &directory, manifest, &gla, &glm).unwrap();
        let mut scratch = Scratch::default();
        rig.pose_after(&bones, None, &mut scratch, None);
        for _ in 0..600 {
            rig.pose_after(&bones, None, &mut scratch, Some(1.0 / 60.0));
        }
        let Some(&tip) = rig.chains.joints.last() else {
            println!("{name}: no dynamic chain");
            return;
        };
        let mut anchor = tip;
        while let Some(parent) = rig.joints[anchor].parent {
            anchor = parent;
            if !rig.chains.joints.contains(&anchor) {
                break;
            }
        }
        let offset = scratch.pose[tip].translation - scratch.pose[anchor].translation;
        let level = offset.truncate().length();
        println!(
            "{name} ({label}): tip {:.1} units above its anchor, {:.0} degrees from level, \
             {:.1} long",
            offset.z,
            offset.z.atan2(level).to_degrees(),
            offset.length()
        );
    }
}

/// The test rig fitted to the test driver, as a stock skeleton's emote source.
fn stock_emotes(model: GltfModel) -> StockEmotes {
    let source = Arc::new(RigModel::fit(model, "", &manifest(""), &driver()).unwrap());
    StockEmotes::of_source(&driver(), source).expect("the driver follows the rig")
}

fn full(play: Option<ClipPlay>) -> ClipPlay {
    ClipPlay {
        weight: 1.0,
        ..play.expect("the clip")
    }
}

#[test]
fn a_stock_model_plays_a_rig_clip_on_its_own_bones() {
    let stock = stock_emotes(rig());
    let driver_bones = vec![identity_rows(); 3];
    let mut scratch = StockScratch::default();
    // At its first key the clip is the rig's bind pose, which is the driver's base
    // pose: every bone matrix stays the identity.
    assert!(stock.pose(
        &driver_bones,
        full(stock.play("Nod", 0.0, None, false)),
        &mut scratch
    ));
    for matrix in &scratch.matrices {
        for (row, expected) in matrix.iter().zip(identity_rows()) {
            for (a, b) in row.iter().zip(expected) {
                assert!((a - b).abs() < 1e-4, "{:?}", scratch.matrices);
            }
        }
    }
    // At its end the head's quarter turn about the game's +Z turns the cranium
    // about its own base position (0, 0, 60); the pelvis stays.
    let play = stock.play("Nod", 1.0, None, false);
    assert_eq!(play.map(|play| play.weight), Some(1.0));
    assert!(stock.pose(&driver_bones, play.unwrap(), &mut scratch));
    let cranium = affine(&scratch.matrices[2]);
    let front = cranium.transform_point3a(Vec3A::new(0.0, -4.0, 64.0));
    assert!(
        (front - Vec3A::new(4.0, 0.0, 64.0)).length() < 1e-3,
        "{front:?}"
    );
    let pivot = cranium.transform_point3a(Vec3A::new(0.0, 0.0, 60.0));
    assert!(
        (pivot - Vec3A::new(0.0, 0.0, 60.0)).length() < 1e-3,
        "{pivot:?}"
    );
    let pelvis = affine(&scratch.matrices[1]).transform_point3a(Vec3A::new(1.0, 0.0, 40.0));
    assert!(
        (pelvis - Vec3A::new(1.0, 0.0, 40.0)).length() < 1e-3,
        "{pelvis:?}"
    );
    // Half faded in, half way round.
    let half = ClipPlay {
        weight: 0.5,
        ..play.unwrap()
    };
    assert!(stock.pose(&driver_bones, half, &mut scratch));
    let front = affine(&scratch.matrices[2]).transform_point3a(Vec3A::new(0.0, -4.0, 64.0));
    let side = 4.0 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (front - Vec3A::new(side, -side, 64.0)).length() < 1e-3,
        "{front:?}"
    );
    // Faded out it leaves the driver's matrices alone; an unknown clip plays nothing.
    let none = ClipPlay {
        weight: 0.0,
        ..play.unwrap()
    };
    assert!(!stock.pose(&driver_bones, none, &mut scratch));
    assert!(stock.play("Wave", 0.0, None, false).is_none());
    // Matrices of another skeleton are not posed.
    assert!(!stock.pose(&driver_bones[..2], play.unwrap(), &mut scratch));
}

#[test]
fn a_stock_pelvis_follows_the_clips_pelvis_and_carries_its_children() {
    // The rig crouches: its pelvis drops from 1 m to 0.5 m over a second.
    let mut model = rig();
    model.animations = vec![GltfAnimation {
        name: "Crouch".into(),
        channels: vec![GltfChannel {
            joint: 0,
            path: GltfPath::Translation,
            times: vec![0.0, 1.0],
            values: vec![[0.0, 1.0, 0.0, 0.0], [0.0, 0.5, 0.0, 0.0]],
            step: false,
        }],
    }];
    let stock = stock_emotes(model);
    let mut scratch = StockScratch::default();
    // The driver itself stands 3 units further front (-Y) than its base pose.
    let driver_bones = vec![translated(0.0, -3.0, 0.0); 3];
    assert!(stock.pose(
        &driver_bones,
        full(stock.play("Crouch", 1.0, None, false)),
        &mut scratch
    ));
    // Half a metre at 40 units a metre: 20 units down, from where the driver stands.
    let pelvis = affine(&scratch.matrices[1]).transform_point3a(Vec3A::new(0.0, 0.0, 40.0));
    assert!(
        (pelvis - Vec3A::new(0.0, -3.0, 20.0)).length() < 1e-3,
        "{pelvis:?}"
    );
    let cranium = affine(&scratch.matrices[2]).transform_point3a(Vec3A::new(0.0, 0.0, 60.0));
    assert!(
        (cranium - Vec3A::new(0.0, -3.0, 40.0)).length() < 1e-3,
        "{cranium:?}"
    );
    // The root bone, which the map leaves out, keeps the driver's matrix.
    assert_eq!(scratch.matrices[0], translated(0.0, -3.0, 0.0));
}

#[test]
fn stock_bones_are_posed_parents_first() {
    assert_eq!(stock::bone_order(&[Some(2), None, Some(1)]), vec![1, 2, 0]);
    // A cycle or a parent past the end still places every bone once.
    let order = stock::bone_order(&[Some(1), Some(0), Some(9)]);
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2]);
}

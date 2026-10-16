//! First-person view weapon: the `_hand.md3` tag rig and the gun on it.
//!
//! `CG_AddViewWeapon` (`cg_weapons.c:782-905`) places the hand rig at the
//! view origin with the bobbed view angles, then `CG_AddPlayerWeapon`
//! (`:420-620`) hangs the item view model on the rig's `tag_weapon` and the
//! barrels on `tag_barrel*`, all with `RF_DEPTHHACK`. Retail hand rigs carry
//! no surfaces — only 15 frames of tags — so the gun and barrels are the
//! static frame-0 meshes already in `object_meshes` and only the tags lerp
//! per frame (`R_LerpTag`, `tr_model.cpp:1792-1821`). The policy (models,
//! frame mapping, bob) lives in `sjk_client::view_weapon`; this module is
//! the renderer glue.

use super::*;
#[path = "view_weapon_offsets.rs"]
mod offsets;
use glam::Mat3;
use sjk_client::{
    LegacyViewBobSample, LegacyViewWeaponAnimations, LegacyViewWeaponFrames, LegacyViewWeaponPose,
    legacy_angles_to_quaternion, legacy_view_weapon_frames, legacy_view_weapon_pose,
    legacy_view_weapon_visible, legacy_view_weapon_visible_as, legacy_view_weapons,
};
use sjk_protocol::Snapshot;

const WEAPON_SLOTS: usize = 19; // WP_NUM_WEAPONS, codemp/game/bg_weapons.h:60

/// One weapon's rig and the object meshes it positions.
struct Entry {
    hand: Md3,
    tag_weapon: usize,
    gun_mesh: usize,
    /// `(hand tag index, object mesh index)` per barrel.
    barrels: Vec<(usize, usize)>,
    /// The gun's frame-0 `tag_flash`, where the muzzle effect plays.
    flash: Option<Md3Tag>,
}

/// Load-time rigs per weapon number; `submit` is allocation-free.
#[derive(Default)]
pub(crate) struct Catalog {
    entries: Vec<Option<Entry>>,
    animations: Option<LegacyViewWeaponAnimations>,
    /// `cg_drawGun 0`: `CG_AddViewWeapon` skips the gun model but keeps the
    /// muzzle flash (`codemp/cgame/cg_weapons.c:811-820`).
    visible: bool,
}

impl Catalog {
    /// Add the gun and barrel models to the map-load appearance set.
    pub(crate) fn extend_appearances(appearances: &mut BTreeSet<Appearance>) {
        for (_, model) in legacy_view_weapons() {
            for path in std::iter::once(model.gun).chain(model.barrels.iter().map(|b| b.0)) {
                appearances.insert(Appearance {
                    model: path.to_owned(),
                    variant: String::new(),
                });
            }
        }
    }

    /// Parse the hand rigs and resolve the meshes; weapons whose assets are
    /// missing stay empty and simply draw nothing.
    pub(crate) fn build(
        vfs: &VirtualFileSystem,
        meshes: &[StaticModelMesh],
        humanoid: Option<&AnimationConfig>,
    ) -> Self {
        let mesh_index = |path: &str| {
            meshes
                .iter()
                .position(|mesh| mesh.appearance.model.eq_ignore_ascii_case(path))
        };
        let mut entries = Vec::with_capacity(WEAPON_SLOTS);
        entries.resize_with(WEAPON_SLOTS, || None);
        for (weapon, model) in legacy_view_weapons() {
            let entry = (|| {
                let asset = vfs.read(model.hand).ok()??;
                let hand = Md3::parse(&asset.bytes).ok()?;
                let tag_weapon = hand.tag_index("tag_weapon")?;
                let gun_mesh = mesh_index(model.gun)?;
                let flash = vfs
                    .read(model.gun)
                    .ok()
                    .flatten()
                    .and_then(|asset| Md3::parse(&asset.bytes).ok())
                    .and_then(|gun| Some(gun.tags.first()?[gun.tag_index("tag_flash")?].clone()));
                let barrels = model
                    .barrels
                    .iter()
                    .filter_map(|(path, tag)| Some((hand.tag_index(tag)?, mesh_index(path)?)))
                    .collect();
                Some(Entry {
                    hand,
                    tag_weapon,
                    gun_mesh,
                    barrels,
                    flash,
                })
            })();
            entries[usize::from(weapon)] = entry;
        }
        Self {
            entries,
            animations: humanoid.map(LegacyViewWeaponAnimations::new),
            visible: true,
        }
    }

    /// Latch `cg_drawGun` (retail default 1).
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

/// Everything one frame needs, resolved from the snapshot up front.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Inputs {
    pub(crate) weapon: u8,
    pub(crate) frames: LegacyViewWeaponFrames,
    pub(crate) pose: LegacyViewWeaponPose,
    /// EternalJK's `cg_fovViewmodel` factor on the hand's forward axis.
    pub(crate) forward_scale: f32,
}

/// First-person view of the local player: eye origin and look angles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct View {
    pub(crate) origin: Vec3,
    /// Radians; pitch is positive looking up, as the viewer camera stores it.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// The local player's weapon state at the presented camera.
pub(crate) fn frame_inputs(
    state: &GpuState,
    snapshot: Option<&Snapshot>,
    camera: Option<first_person_view::Camera>,
    local_entity: Option<u64>,
    presentation_time: i64,
) -> Option<Inputs> {
    (|| {
        let view = camera.map_or(
            View {
                origin: state.camera_position,
                yaw: state.camera_yaw,
                pitch: state.camera_pitch,
            },
            |camera| camera.view(),
        );
        let predicted_weapon = state
            .live_session
            .is_some()
            .then(|| state.local_prediction.predicted_state())
            .flatten();
        inputs_with_weapon(
            &state.first_person_weapon,
            snapshot?,
            state.third_person || state.detached_camera,
            view,
            camera.map_or(0.0, |camera| camera.landing),
            local_torso_frame(&state.actor_meshes, local_entity, presentation_time),
            presentation_time,
            predicted_weapon,
        )
    })()
    .map(|mut inputs| {
        let (yaw, pitch) = camera.map_or((state.camera_yaw, state.camera_pitch), |camera| {
            (camera.yaw, camera.pitch)
        });
        let aspect = state.configuration.width as f32 / state.configuration.height.max(1) as f32;
        // `cg.refdef.fov_x` from the vertical FOV the last frame rendered with.
        let horizontal = 2.0
            * ((state.field_of_view.to_radians() * 0.5).tan() * aspect)
                .atan()
                .to_degrees();
        let fov = offsets::view_model_fov(state.console.as_ref(), horizontal, aspect);
        inputs.pose.origin = offsets::apply(
            inputs.pose.origin,
            yaw,
            pitch,
            state.console.as_ref(),
            fov.drop,
        );
        inputs.forward_scale = fov.forward_scale;
        inputs
    })
}

#[allow(clippy::too_many_arguments)]
fn inputs_with_weapon(
    catalog: &Catalog,
    snapshot: &Snapshot,
    third_person: bool,
    view: View,
    landing: f32,
    torso_frame: Option<(usize, f32)>,
    presentation_time: i64,
    predicted_weapon: Option<&sjk_client::pmove::MovementState>,
) -> Option<Inputs> {
    let weapon = match predicted_weapon {
        Some(state) => legacy_view_weapon_visible_as(snapshot, third_person, state.weapon)?,
        None => legacy_view_weapon_visible(snapshot, third_person)?,
    };
    let state = &snapshot.player;
    let frames = match (&catalog.animations, torso_frame) {
        (Some(animations), Some((clip, frame))) => animations.frames(clip, frame),
        _ => legacy_view_weapon_frames("", 0, 0.0),
    };
    // Quake angles: positive pitch looks down (`AngleVectors`, `q_math.c`).
    let view_angles = [-view.pitch.to_degrees(), view.yaw.to_degrees(), 0.0];
    Some(Inputs {
        weapon,
        frames,
        forward_scale: offsets::ViewModelFov::NONE.forward_scale,
        pose: legacy_view_weapon_pose(
            view.origin.to_array(),
            view_angles,
            predicted_weapon.map_or_else(
                || LegacyViewBobSample::new(state.bob_cycle(), state.velocity()),
                |state| LegacyViewBobSample::new(state.bob_cycle, state.velocity),
            ),
            landing,
            presentation_time,
        ),
    })
}

/// The local actor's posed torso animation and fractional frame, for `inputs`.
/// The animation comes from the same posed track as the frame (stock reads
/// `lower_lumbar`'s frame with the predicted `torsoAnim`), not from the older
/// server snapshot, whose animation can still be the previous one while the
/// predicted shot already plays; mixing the two put the hand on the wrong frames.
pub(crate) fn local_torso_frame(
    meshes: &[ActorMesh],
    local_entity: Option<u64>,
    presentation_time: i64,
) -> Option<(usize, f32)> {
    let mesh = meshes
        .iter()
        .find(|mesh| mesh.entity_id.map(|entity| entity.get()) == local_entity)?;
    let frame = mesh
        .animator
        .torso_frame(&mesh.preview.animation, presentation_time)?;
    Some((mesh.animator.torso_clip()?, frame))
}

/// Push the gun and barrel instances for `inputs`; returns how many.
pub(crate) fn submit(
    catalog: &Catalog,
    inputs: Inputs,
    object_groups: &mut [Vec<ActorInstance>],
) -> usize {
    let Some(Some(entry)) = catalog.entries.get(usize::from(inputs.weapon)) else {
        return 0;
    };
    if !catalog.visible {
        return 0;
    }
    let (hand_origin, hand_rotation) = hand_frame(inputs);
    let frames = inputs.frames;
    let front_lerp = 1.0 - frames.back_lerp;
    let mut submitted = 0;
    for (tag, mesh) in
        std::iter::once((entry.tag_weapon, entry.gun_mesh)).chain(entry.barrels.iter().copied())
    {
        let tag = entry.hand.lerp_tag(
            tag,
            frames.old_frame as usize,
            frames.frame as usize,
            front_lerp,
        );
        let placed = position_on_tag(hand_origin, hand_rotation, inputs.forward_scale, &tag);
        let mut instance = ActorInstance::new(
            placed.origin.to_array(),
            placed.rotation.to_array(),
            placed.scale.to_array(),
        );
        instance.depth_hack = 1.0;
        object_groups[mesh].push(instance);
        submitted += 1;
    }
    submitted
}

/// The muzzle socket of the gun placed by `inputs`: the gun's `tag_flash`
/// composed onto the hand's `tag_weapon` (`cg_weapons.c:576-579`, also
/// `cg.lastFPFlashPoint`). Tags are game-space, so no basis conversion.
pub(crate) fn flash_socket(catalog: &Catalog, inputs: Inputs) -> Option<muzzle_flash::Socket> {
    let Some(Some(entry)) = catalog.entries.get(usize::from(inputs.weapon)) else {
        return None;
    };
    let flash = entry.flash.as_ref()?;
    let (hand_origin, hand_rotation) = hand_frame(inputs);
    let frames = inputs.frames;
    let tag_weapon = entry.hand.lerp_tag(
        entry.tag_weapon,
        frames.old_frame as usize,
        frames.frame as usize,
        1.0 - frames.back_lerp,
    );
    Some(muzzle_socket(
        hand_origin,
        hand_rotation,
        inputs.forward_scale,
        &tag_weapon,
        flash,
    ))
}

/// The socket of `flash` on the gun that `tag_weapon` places on the hand.
/// The flash tag is a point of the gun mesh, so it goes through the same
/// transform the gun instance gets (rotation, then `scale` on the local axes):
/// socket and barrel stay together whatever the view-model FOV scale is.
fn muzzle_socket(
    hand_origin: Vec3,
    hand_rotation: Quat,
    forward_scale: f32,
    tag_weapon: &Md3Tag,
    flash: &Md3Tag,
) -> muzzle_flash::Socket {
    let gun = position_on_tag(hand_origin, hand_rotation, forward_scale, tag_weapon);
    let origin = gun.origin + gun.rotation * (gun.scale * Vec3::from_array(flash.origin));
    let direction = gun.rotation * (gun.scale * Vec3::from_array(flash.axes[0]));
    muzzle_flash::Socket {
        origin: origin.to_array(),
        direction: direction.normalize_or(Vec3::X).to_array(),
    }
}

fn hand_frame(inputs: Inputs) -> (Vec3, Quat) {
    (
        Vec3::from_array(inputs.pose.origin),
        Quat::from_array(legacy_angles_to_quaternion(inputs.pose.angles)),
    )
}

/// A child placed on a tag: the instance origin, rotation and local-axis scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TagPlacement {
    origin: Vec3,
    rotation: Quat,
    scale: Vec3,
}

/// `CG_PositionEntityOnTag` (`cg_ents.c:48-71`): the child origin is the
/// parent origin plus the tag origin along the parent axes, and the child
/// axes are the tag axes composed with the parent axes.
///
/// `forward_scale` is the length EternalJK gives the parent's forward axis
/// (`VectorScale( hand.axis[0], fracWeapFOV, ...)`, `cg_weapons.c:1040`). The
/// tag origin is scaled along it exactly. For the child's mesh the engine
/// squashes the child along the parent's forward axis, which is the tag's
/// `axes[i][0]` direction in the child's frame. An instance can only scale its
/// own axes, so `scale` squashes the child axis closest to that direction
/// (exact when the tag only rolls about the parent's forward axis, as most
/// retail hand rigs do; the blasters' tag is tilted about 13 degrees and is
/// approximated, see `docs/status.md`).
fn position_on_tag(
    parent_origin: Vec3,
    parent_rotation: Quat,
    forward_scale: f32,
    tag: &Md3Tag,
) -> TagPlacement {
    let [x, y, z] = tag.origin;
    let origin = parent_origin + parent_rotation * Vec3::new(x * forward_scale, y, z);
    let tag_rotation = Quat::from_mat3(&Mat3::from_cols(
        Vec3::from_array(tag.axes[0]),
        Vec3::from_array(tag.axes[1]),
        Vec3::from_array(tag.axes[2]),
    ));
    let forward_in_child = Vec3::new(tag.axes[0][0], tag.axes[1][0], tag.axes[2][0]);
    let mut scale = Vec3::ONE;
    scale[forward_in_child.abs().max_position()] = forward_scale;
    TagPlacement {
        origin,
        rotation: (parent_rotation * tag_rotation).normalize(),
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(origin: [f32; 3], axes: [[f32; 3]; 3]) -> Md3Tag {
        Md3Tag {
            name: String::new(),
            origin,
            axes,
        }
    }

    /// Retail `bowcaster_hand.md3` `tag_weapon` (frame 0): a quarter turn about
    /// the hand's forward axis, like most hand rigs.
    fn rolled_tag() -> Md3Tag {
        tag(
            [6.523, -3.035, -7.167],
            [
                [1.0, 0.002, -0.004],
                [0.004, 0.008, 1.0],
                [0.002, -1.0, 0.008],
            ],
        )
    }

    /// Retail `blaster_hand.md3` `tag_weapon` (frame 0): tilted, the hand's
    /// forward axis lies mostly along the gun's own -Y (its barrel).
    fn tilted_tag() -> Md3Tag {
        tag(
            [12.570, -5.071, -9.314],
            [
                [-0.215, -0.751, 0.624],
                [-0.973, 0.222, -0.068],
                [-0.088, -0.622, -0.778],
            ],
        )
    }

    /// Retail `blaster.md3` `tag_flash`, in the gun's frame.
    fn gun_flash() -> Md3Tag {
        tag(
            [-1.315, -11.746, -3.871],
            [
                [-0.188, -0.975, -0.120],
                [-0.845, 0.223, -0.486],
                [0.501, 0.010, -0.865],
            ],
        )
    }

    fn hand() -> (Vec3, Quat) {
        (
            Vec3::new(100.0, -40.0, 60.0),
            Quat::from_euler(glam::EulerRot::ZYX, 0.7, -0.3, 0.1),
        )
    }

    /// The vertex transform of an instance (`vertex_transform.wgsl`).
    fn on_instance(placed: &TagPlacement, point: Vec3) -> Vec3 {
        placed.origin + placed.rotation * (point * placed.scale)
    }

    /// `CG_PositionEntityOnTag` with EternalJK's scaled `hand.axis[0]`: the
    /// gun's point `point` in the world, from row-vector axes as the engine
    /// multiplies them (`cg_ents.c:48-66`).
    fn engine_point(hand: (Vec3, Quat), scale: f32, tag: &Md3Tag, point: Vec3) -> Vec3 {
        let (origin, rotation) = hand;
        let mut axes = [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z];
        axes[0] *= scale;
        let gun_origin =
            origin + axes[0] * tag.origin[0] + axes[1] * tag.origin[1] + axes[2] * tag.origin[2];
        let gun_axes = tag
            .axes
            .map(|row| axes[0] * row[0] + axes[1] * row[1] + axes[2] * row[2]);
        gun_origin + gun_axes[0] * point.x + gun_axes[1] * point.y + gun_axes[2] * point.z
    }

    #[test]
    fn unit_scale_is_the_plain_tag_composition() {
        for weapon in [rolled_tag(), tilted_tag()] {
            let (origin, rotation) = hand();
            let placed = position_on_tag(origin, rotation, 1.0, &weapon);
            assert_eq!(placed.scale, Vec3::ONE);
            let flash = gun_flash();
            let point = Vec3::from_array(flash.origin);
            let engine = engine_point(hand(), 1.0, &weapon, point);
            assert!(on_instance(&placed, point).distance(engine) < 0.01);
        }
    }

    #[test]
    fn rolled_tag_matches_the_engine_exactly() {
        // The hand's forward axis is the gun's own x here, so scaling the
        // gun's x is exactly what EternalJK's scaled `hand.axis[0]` does to the
        // mesh and to every tag on it.
        let weapon = rolled_tag();
        let (origin, rotation) = hand();
        let scale = 0.84;
        let placed = position_on_tag(origin, rotation, scale, &weapon);
        assert_eq!(placed.scale, Vec3::new(scale, 1.0, 1.0));
        let flash = gun_flash();
        for point in [Vec3::from_array(flash.origin), Vec3::new(3.0, -7.0, 2.0)] {
            let engine = engine_point(hand(), scale, &weapon, point);
            assert!(
                on_instance(&placed, point).distance(engine) < 0.05,
                "{point:?}"
            );
        }
    }

    #[test]
    fn muzzle_socket_stays_on_the_rendered_gun() {
        for (weapon, scale) in [
            (rolled_tag(), 0.84),
            (rolled_tag(), 1.0),
            (tilted_tag(), 0.84),
            (tilted_tag(), 1.2),
        ] {
            let (origin, rotation) = hand();
            let flash = gun_flash();
            let placed = position_on_tag(origin, rotation, scale, &weapon);
            let socket = muzzle_socket(origin, rotation, scale, &weapon, &flash);
            let rendered = on_instance(&placed, Vec3::from_array(flash.origin));
            assert!(
                Vec3::from_array(socket.origin).distance(rendered) < 1e-3,
                "scale {scale}"
            );
            // The direction is the flash's forward axis through the same
            // transform, so it still points along the barrel.
            let barrel = on_instance(&placed, Vec3::from_array(flash.axes[0]) * 10.0)
                - on_instance(&placed, Vec3::ZERO);
            assert!(Vec3::from_array(socket.direction).dot(barrel.normalize()) > 0.9999);
        }
    }

    #[test]
    fn tilted_tag_squashes_the_barrel_axis_and_stays_near_the_engine() {
        // The blaster's hand forward axis is the gun's -Y within 13 degrees;
        // the instance scales that axis, not x.
        let weapon = tilted_tag();
        let (origin, rotation) = hand();
        let scale = 0.84;
        let placed = position_on_tag(origin, rotation, scale, &weapon);
        assert_eq!(placed.scale, Vec3::new(1.0, scale, 1.0));
        let flash = gun_flash();
        let point = Vec3::from_array(flash.origin);
        let engine = engine_point(hand(), scale, &weapon, point);
        let error = on_instance(&placed, point).distance(engine);
        // About 0.45 units at the blaster's flash, 12 units out; scaling x
        // would be off by about 1.9.
        assert!(error < 0.5, "{error}");
        let mut x_scaled = placed;
        x_scaled.scale = Vec3::new(scale, 1.0, 1.0);
        assert!(on_instance(&x_scaled, point).distance(engine) > 1.5);
    }
}

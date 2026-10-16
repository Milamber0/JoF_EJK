//! Dismemberment, as EternalJK's client-limb case of `CG_General` (`cg_ents.c`).
//!
//! The server cuts the limb (`G_Dismember`, `g_combat.c`) and sends it as its own
//! entity: `weapon` `G2_MODEL_PART`, `modelGhoul2` the part, `modelindex` the owner.
//! cgame then duplicates the owner's Ghoul2 instance for the limb, roots it at the
//! limb surface with the limb's cap turned on and its pivot bone as origin, and on the
//! owner turns the limb (and everything below it) off and the stump's cap on. The
//! owner's weapon goes with a right arm, right hand or waist. Both cuts smoke
//! (`blaster/smoke_bolton`), and so does a moving limb every 0.4 s. `cg_dismember`
//! 0 shows none of it, 1 everything but heads and waists. A body copied from the owner
//! keeps the missing limbs; the player gets them back once alive again
//! (`CG_ReattachLimb`).

use super::*;
use sjk_model::GlmSurfaceHierarchy;
use std::cell::Cell;

/// `G2SURFACEFLAG_OFF`.
const OFF: u32 = 0x2;
/// `G2SURFACEFLAG_NODESCENDANTS`: neither the surface nor anything below it renders.
const NODESCENDANTS: u32 = 0x100;
const HEAD: u8 = 10;
const WAIST: u8 = 11;
const LARM: u8 = 12;
const RARM: u8 = 13;
const RHAND: u8 = 14;
const LLEG: u8 = 15;
/// `cgs.effects.mBlasterSmoke`.
pub(crate) const SMOKE: &str = "blaster/smoke_bolton";
/// `cg_dismember`, as EternalJK (`cg_xcvar.h`), which has it off by default;
/// SJK starts on 2 (every cut limb).
pub(crate) const CVAR: &str = "cg_dismember";
/// `g_dismember`, the server's chance to cut a limb, given to games this client hosts.
pub(crate) const SERVER_CVAR: &str = "g_dismember";

/// One actor's Ghoul2 surface overrides (`G2API_SetSurfaceOnOff`,
/// `G2API_SetRootSurface`) and the draws they leave visible.
#[derive(Clone, Debug, Default)]
pub(crate) struct Surfaces {
    /// The model's own flags, caps off.
    default_flags: Vec<u32>,
    flags: Vec<u32>,
    root: Option<usize>,
    /// The hierarchy surface each mesh draw comes from.
    draw_surfaces: Vec<usize>,
    /// Whether each mesh draw renders now.
    pub(crate) draw_visible: Vec<bool>,
    /// `centity_t::torsoBolt`: one bit per part cut from this actor.
    pub(crate) lost: u8,
    /// The weapon went with the arm, hand or waist.
    pub(crate) weapon_lost: bool,
    first_person_saber: bool,
}

/// Turn the model's cap surfaces into ordinary hidden surfaces so the actor mesh
/// carries them (Carcass flags caps `G2SURFACEFLAG_OFF`, which the skinning paths
/// skip), and return the surface state for a mesh whose draws come from
/// `draw_surfaces`, an index into the surfaces left after skipping.
pub(crate) fn reveal_caps(hierarchy: &mut [GlmSurfaceHierarchy]) -> Vec<u32> {
    hierarchy
        .iter_mut()
        .map(|surface| {
            if is_cap(&surface.name) {
                surface.flags &= !OFF;
                surface.flags | OFF
            } else {
                surface.flags
            }
        })
        .collect()
}

fn is_cap(name: &str) -> bool {
    name.to_ascii_lowercase().contains("_cap_")
}

impl Surfaces {
    /// `default_flags` from [`reveal_caps`]; `draw_skin_indices` the skinned-surface
    /// index each draw came from, counted over surfaces not flagged off.
    pub(crate) fn new(
        hierarchy: &[GlmSurfaceHierarchy],
        default_flags: Vec<u32>,
        draw_skin_indices: impl Iterator<Item = usize>,
    ) -> Self {
        let skinned: Vec<usize> = hierarchy
            .iter()
            .enumerate()
            .filter(|(_, surface)| surface.flags & OFF == 0)
            .map(|(index, _)| index)
            .collect();
        let draw_surfaces = draw_skin_indices
            .map(|index| skinned.get(index).copied().unwrap_or(usize::MAX))
            .collect();
        let mut surfaces = Self {
            flags: default_flags.clone(),
            default_flags,
            root: None,
            draw_surfaces,
            draw_visible: Vec::new(),
            lost: 0,
            weapon_lost: false,
            first_person_saber: false,
        };
        surfaces.refresh(hierarchy);
        surfaces
    }

    /// Back to the model's own surfaces (`CG_ReattachLimb` re-applying the skin).
    pub(crate) fn reset(&mut self, hierarchy: &[GlmSurfaceHierarchy]) {
        self.flags.clone_from(&self.default_flags);
        self.root = None;
        self.lost = 0;
        self.weapon_lost = false;
        self.refresh(hierarchy);
    }

    /// Take another actor's overrides (`CG_BodyQueueCopy` duplicating the instance).
    pub(crate) fn copy_from(&mut self, source: &Self, hierarchy: &[GlmSurfaceHierarchy]) {
        if !self.same_layout(source) {
            return;
        }
        self.flags.clone_from(&source.flags);
        self.root = source.root;
        self.lost = source.lost;
        self.weapon_lost = source.weapon_lost;
        self.refresh(hierarchy);
    }

    /// Whether `other` was built from the same surfaces and draws. A mesh rebuilt
    /// after a clientinfo change, or a Kyle stand-in carrying the requested
    /// appearance, can share an appearance with a mesh of another model; surface
    /// state is copied between meshes only when their layouts match.
    pub(crate) fn same_layout(&self, other: &Self) -> bool {
        self.flags.len() == other.flags.len() && self.draw_surfaces == other.draw_surfaces
    }

    /// `G2_SetSurfaceOnOff`: only the off and no-descendants bits change.
    fn set(&mut self, surface: usize, flags: u32) {
        if let Some(current) = self.flags.get_mut(surface) {
            *current = (*current & !(OFF | NODESCENDANTS)) | (flags & (OFF | NODESCENDANTS));
        }
    }

    /// rd-vanilla `RenderSurfaces`: from the root down, a surface with no flags
    /// renders, and a no-descendants surface stops the descent.
    fn rendered(&self, hierarchy: &[GlmSurfaceHierarchy]) -> Vec<bool> {
        let mut rendered = vec![false; hierarchy.len()];
        let mut pending: Vec<usize> = match self.root {
            Some(root) => vec![root],
            None => (0..hierarchy.len())
                .filter(|&index| hierarchy[index].parent.is_none())
                .collect(),
        };
        while let Some(index) = pending.pop() {
            let mut flags = self.flags.get(index).copied().unwrap_or(0) & (OFF | NODESCENDANTS);
            if self.first_person_saber
                && crate::actor_world_submission::first_person_saber::hide_surface(
                    &hierarchy[index].name,
                )
            {
                flags |= OFF | NODESCENDANTS;
            }
            if flags == 0 {
                rendered[index] = true;
            }
            if flags & NODESCENDANTS == 0 {
                pending.extend(hierarchy[index].children.iter().copied());
            }
        }
        rendered
    }

    fn refresh(&mut self, hierarchy: &[GlmSurfaceHierarchy]) {
        let rendered = self.rendered(hierarchy);
        self.draw_visible = self
            .draw_surfaces
            .iter()
            .map(|&surface| rendered.get(surface).copied().unwrap_or(true))
            .collect();
    }

    /// A hosting engine's Ghoul2 surface state (`crate::host`): its overrides of the
    /// model's own flags (`CGhoul2Info::mSlist`, as `G2_SetSurfaceOnOff` writes them) and
    /// its root surface (`mSurfaceRoot`), rendered as `RenderSurfaces` renders them.
    pub(crate) fn host_overrides(
        &mut self,
        overrides: &[(usize, u32)],
        root: Option<usize>,
        hierarchy: &[GlmSurfaceHierarchy],
    ) {
        let mut flags = self.default_flags.clone();
        for &(surface, value) in overrides {
            if let Some(current) = flags.get_mut(surface) {
                *current = (*current & !(OFF | NODESCENDANTS)) | (value & (OFF | NODESCENDANTS));
            }
        }
        if flags != self.flags || root != self.root {
            self.flags = flags;
            self.root = root;
            self.refresh(hierarchy);
        }
    }

    /// Apply the first-person head mask only when the view changes.
    pub(crate) fn first_person_saber(&mut self, enabled: bool, hierarchy: &[GlmSurfaceHierarchy]) {
        if self.first_person_saber != enabled {
            self.first_person_saber = enabled;
            self.refresh(hierarchy);
        }
    }

    /// `BG_GetRootSurfNameWithVariant` (`bg_g2_utils.c`): `root` where it is drawn, else
    /// its first drawn variant (`l_arma` .. `l_armh`), else `root`.
    fn variant(&self, hierarchy: &[GlmSurfaceHierarchy], root: &str) -> String {
        let rendered = self.rendered(hierarchy);
        let drawn = |name: &str| {
            find(hierarchy, name)
                .is_some_and(|index| rendered[index] && self.draw_surfaces.contains(&index))
        };
        if drawn(root) {
            return root.to_owned();
        }
        (b'a'..b'a' + 8)
            .map(|letter| format!("{root}{}", char::from(letter)))
            .find(|name| drawn(name))
            .unwrap_or_else(|| root.to_owned())
    }
}

fn find(hierarchy: &[GlmSurfaceHierarchy], name: &str) -> Option<usize> {
    hierarchy
        .iter()
        .position(|surface| surface.name.eq_ignore_ascii_case(name))
}

/// The surfaces and pivot of one cut (`CG_General`): the limb's root surface and cap,
/// the cap left on the owner, and the bone the limb turns about.
struct Cut {
    limb: String,
    limb_cap: String,
    stub_cap: String,
    pivot: &'static str,
}

fn cut(surfaces: &Surfaces, hierarchy: &[GlmSurfaceHierarchy], part: u8, humanoid: bool) -> Cut {
    let variant = |root: &str| surfaces.variant(hierarchy, root);
    let limb_with = |limb: String, limb_cap: &str, stub: String, stub_cap: &str, pivot| Cut {
        limb_cap: format!("{limb}_cap_{limb_cap}"),
        stub_cap: format!("{stub}_cap_{stub_cap}"),
        limb,
        pivot,
    };
    match part {
        HEAD => Cut {
            limb: "head".into(),
            limb_cap: "head_cap_torso".into(),
            stub_cap: "torso_cap_head".into(),
            pivot: "cranium",
        },
        WAIST => Cut {
            limb: "torso".into(),
            limb_cap: "torso_cap_hips".into(),
            stub_cap: "hips_cap_torso".into(),
            pivot: if humanoid { "thoracic" } else { "pelvis" },
        },
        LARM => limb_with(
            variant("l_arm"),
            "torso",
            variant("torso"),
            "l_arm",
            "lradius",
        ),
        RARM => limb_with(
            variant("r_arm"),
            "torso",
            variant("torso"),
            "r_arm",
            "rradius",
        ),
        RHAND => limb_with(
            variant("r_hand"),
            "r_arm",
            variant("r_arm"),
            "r_hand",
            "rhand",
        ),
        LLEG => limb_with(variant("l_leg"), "hips", variant("hips"), "l_leg", "ltibia"),
        // "umm... just default to the right leg, I guess (same as on server)".
        _ => limb_with(variant("r_leg"), "hips", variant("hips"), "r_leg", "rtibia"),
    }
}

/// A limb mesh's own state.
#[derive(Debug)]
pub(crate) struct Limb {
    /// The owner's animation when it was cut; the limb's copied animator ignores it,
    /// but the pose step needs a request to evaluate.
    pub(crate) state: sjk_runtime::AnimationState,
    pivot_bone: Option<usize>,
    /// The pivot bone in model space, from the latest pose (`G2API_SetNewOrigin`).
    pub(crate) pivot: [f32; 3],
    trail: Cell<i64>,
    previous_origin: Cell<Option<[f32; 3]>>,
}

impl Limb {
    /// Record the pivot bone's model-space position from `matrices` (skinning
    /// matrices, so the bone's own bind position is carried through them).
    pub(crate) fn update_pivot(&mut self, animation: &sjk_model::Gla, matrices: &[[[f32; 4]; 3]]) {
        let Some(bone) = self.pivot_bone else { return };
        let (Some(matrix), Some(base)) = (matrices.get(bone), animation.bones.get(bone)) else {
            return;
        };
        let origin = [
            base.base_pose[0][3],
            base.base_pose[1][3],
            base.base_pose[2][3],
        ];
        self.pivot = std::array::from_fn(|row| {
            matrix[row][0] * origin[0]
                + matrix[row][1] * origin[1]
                + matrix[row][2] * origin[2]
                + matrix[row][3]
        });
    }
}

fn entity_flags(snapshot: &sjk_protocol::Snapshot, local: bool, number: u16) -> Option<u32> {
    if local {
        return Some(snapshot.player.entity_flags());
    }
    snapshot
        .entities
        .iter()
        .find(|state| state.number() == number)
        .map(sjk_protocol::EntityState::e_flags)
}

/// `EF_DEAD`.
const EF_DEAD: u32 = 1 << 1;
/// `BOTH_RIGHTHANDCHOPPEDOFF`: a hand lost in a saber lock, while still alive.
const BOTH_RIGHTHANDCHOPPEDOFF: usize = 1254;
/// `ANIM_TOGGLEBIT`.
const ANIM_TOGGLEBIT: u16 = 2048;

/// What cutting one limb needs from the snapshot, read before the meshes change.
#[derive(Clone, Copy)]
struct LimbCut {
    limb_id: EntityId,
    part: u8,
    owner: u16,
    owner_flags: u32,
    /// The owner's torso animation, unless the owner is the local player.
    owner_torso: Option<usize>,
}

impl GpuState {
    /// Follow this frame's limb entities: cut new ones from their owners, free the
    /// limb meshes of limbs that are gone, and give living players their limbs back.
    pub(crate) fn update_limbs(&mut self, presentation_time: i64) {
        let setting = self
            .console
            .as_ref()
            .and_then(|console| console.integer_cvar(CVAR))
            .unwrap_or(0);
        // Nothing cut and no limb mesh in use: with cg_dismember 0 there is nothing to
        // do, otherwise one scan for new limb entities. The snapshot is borrowed, never
        // copied.
        let in_use = self.actor_meshes.iter().any(|mesh| {
            mesh.surfaces.lost != 0 || (mesh.limb.is_some() && mesh.entity_id.is_some())
        });
        if setting == 0 && !in_use {
            return;
        }
        let local = self
            .live_session
            .as_ref()
            .map(|session| session.latest_snapshot().player.client_num());
        let Self {
            live_session,
            demo_session,
            actor_meshes,
            ..
        } = self;
        let Some(snapshot) = crate::first_person_view::presented_snapshot(
            live_session.as_ref(),
            demo_session.as_ref(),
            presentation_time as i32,
        ) else {
            return;
        };
        if !in_use
            && !snapshot
                .entities
                .iter()
                .any(|state| sjk_client::legacy_limb(state).is_some())
        {
            return;
        }
        // Living players get their limbs back (`CG_ReattachLimb`).
        for mesh in actor_meshes.iter_mut() {
            if mesh.limb.is_some() || mesh.corpse_pool || mesh.surfaces.lost == 0 {
                continue;
            }
            let Some(number) = mesh
                .entity_id
                .and_then(|id| u16::try_from(id.get().saturating_sub(1)).ok())
            else {
                continue;
            };
            let flags = entity_flags(snapshot, local == Some(number), number);
            if flags.is_some_and(|flags| flags & EF_DEAD == 0) {
                mesh.surfaces.reset(&mesh.preview.mesh.hierarchy);
            }
        }
        // Limbs whose entity is gone return to the pool.
        for mesh in actor_meshes.iter_mut() {
            let (Some(_), Some(id)) = (mesh.limb.as_ref(), mesh.entity_id) else {
                continue;
            };
            let present = snapshot.entities.iter().any(|state| {
                u64::from(state.number()) + 1 == id.get()
                    && sjk_client::legacy_limb(state).is_some()
            });
            if !present || setting == 0 {
                mesh.entity_id = None;
            }
        }
        if setting == 0 {
            return;
        }
        // New limbs, one entity index at a time: what a cut needs is read from the
        // borrowed snapshot first, then the borrow ends before the meshes change.
        let count = snapshot.entities.len();
        for index in 0..count {
            let Some(request) = self.limb_cut(index, setting, local, presentation_time) else {
                continue;
            };
            if let Err(error) = self.cut_limb(request, presentation_time) {
                let number = request.limb_id.get() - 1;
                crate::log::progress(format_args!("limb {number}: {error}"));
            }
        }
    }

    /// The cut the limb entity at `index` of the presented snapshot asks for, if any.
    fn limb_cut(
        &self,
        index: usize,
        setting: i64,
        local: Option<u16>,
        presentation_time: i64,
    ) -> Option<LimbCut> {
        let snapshot = crate::first_person_view::presented_snapshot(
            self.live_session.as_ref(),
            self.demo_session.as_ref(),
            presentation_time as i32,
        )?;
        let state = snapshot.entities.get(index)?;
        let (part, owner) = sjk_client::legacy_limb(state)?;
        if setting < 2 && matches!(part, HEAD | WAIST) {
            return None;
        }
        let limb_id = EntityId::new(u64::from(state.number()) + 1);
        if self
            .actor_meshes
            .iter()
            .any(|mesh| mesh.limb.is_some() && mesh.entity_id == Some(limb_id))
        {
            return None;
        }
        let owner_local = local == Some(owner);
        let owner_flags = entity_flags(snapshot, owner_local, owner)?;
        let owner_torso = (!owner_local)
            .then(|| snapshot.entities.iter().find(|s| s.number() == owner))
            .flatten()
            .map(|owner_state| usize::from(owner_state.torso_animation() & !ANIM_TOGGLEBIT));
        Some(LimbCut {
            limb_id,
            part,
            owner,
            owner_flags,
            owner_torso,
        })
    }

    fn cut_limb(&mut self, request: LimbCut, presentation_time: i64) -> Result<(), Box<dyn Error>> {
        let LimbCut {
            limb_id,
            part,
            owner,
            owner_flags: flags,
            owner_torso,
        } = request;
        let owner_id = EntityId::new(u64::from(owner) + 1);
        let Some(owner_index) = self.actor_meshes.iter().position(|mesh| {
            mesh.limb.is_none() && !mesh.corpse_pool && mesh.entity_id == Some(owner_id)
        }) else {
            return Ok(());
        };
        let bit = 1u8 << (part - HEAD);
        let owner_mesh = &self.actor_meshes[owner_index];
        // A rig model draws its glTF, not the driver surfaces a cut hides and shows.
        if owner_mesh.preview.rig.is_some() {
            return Ok(());
        }
        if owner_mesh.surfaces.lost & bit != 0 {
            return Ok(());
        }
        // Only once the owner is dead and in a death animation (or has just lost a
        // hand in a saber lock).
        if flags & EF_DEAD == 0 {
            return Ok(());
        }
        if let Some(torso) = owner_torso
            && !sjk_client::legacy_death_animation(torso)
            && torso != BOTH_RIGHTHANDCHOPPEDOFF
        {
            return Ok(());
        }
        let world = self
            .demo_session
            .as_ref()
            .map_or(&self.live_world, crate::demo_playback::Session::world);
        let Some(animation_state) = world.entity(owner_id).and_then(|entity| entity.animation())
        else {
            return Ok(());
        };
        let origin_of = |id: EntityId| {
            world
                .entity(id)
                .map(|entity| Vec3::from_array(entity.sample(presentation_time).translation))
        };
        let (limb_origin, owner_origin) = (origin_of(limb_id), origin_of(owner_id));
        let hierarchy = &owner_mesh.preview.mesh.hierarchy;
        let names = cut(
            &owner_mesh.surfaces,
            hierarchy,
            part,
            owner_mesh.animator.humanoid(),
        );
        let (Some(limb_surface), stub_cap, limb_cap) = (
            find(hierarchy, &names.limb),
            find(hierarchy, &names.stub_cap),
            find(hierarchy, &names.limb_cap),
        ) else {
            return Ok(());
        };
        let pivot_bone = owner_mesh
            .preview
            .animation
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(names.pivot));
        let animator = owner_mesh.animator.detached_copy();
        let appearance = owner_mesh.appearance.clone();
        let mut limb_surfaces = owner_mesh.surfaces.clone();

        // The limb: a pooled limb mesh of this model, or a new upload.
        let limb_index = match self.actor_meshes.iter().position(|mesh| {
            mesh.limb.is_some() && mesh.entity_id.is_none() && mesh.appearance == appearance
        }) {
            Some(index) => index,
            None => {
                let preview = self.actor_meshes[owner_index].preview.clone();
                let mesh = self.upload_actor(preview, &appearance, limb_id, [None, None])?;
                self.actor_meshes.push(mesh);
                self.actor_groups.push(Vec::with_capacity(2));
                self.actor_meshes.len() - 1
            }
        };
        let limb_mesh = &mut self.actor_meshes[limb_index];
        if !limb_surfaces.same_layout(&limb_mesh.surfaces) {
            return Ok(());
        }
        limb_surfaces.root = Some(limb_surface);
        if let Some(cap) = limb_cap {
            limb_surfaces.set(cap, 0);
        }
        limb_surfaces.lost = 0;
        limb_surfaces.weapon_lost = true;
        limb_surfaces.refresh(&limb_mesh.preview.mesh.hierarchy);
        limb_mesh.surfaces = limb_surfaces;
        limb_mesh.entity_id = Some(limb_id);
        limb_mesh.animator = crate::actor_pose::evaluation::Slot::from_animator(animator);
        limb_mesh.disintegration = None;
        limb_mesh.limb = Some(Limb {
            state: animation_state,
            pivot_bone,
            pivot: [0.0; 3],
            trail: Cell::new(i64::MIN),
            previous_origin: Cell::new(None),
        });

        // The owner: limb off with everything below it, the stump's cap on.
        let owner_mesh = &mut self.actor_meshes[owner_index];
        owner_mesh.surfaces.set(limb_surface, NODESCENDANTS);
        if let Some(cap) = stub_cap {
            owner_mesh.surfaces.set(cap, 0);
        }
        owner_mesh.surfaces.lost |= bit;
        if matches!(part, RARM | RHAND | WAIST) {
            owner_mesh.surfaces.weapon_lost = true;
        }
        owner_mesh
            .surfaces
            .refresh(&owner_mesh.preview.mesh.hierarchy);

        // Smoke at the cut, on the limb and on the stump.
        if let Some(origin) = limb_origin {
            self.limb_smoke(origin, [0.0, 0.0, 1.0], presentation_time, part.into());
        }
        if let Some(origin) = owner_origin {
            self.limb_smoke(
                origin,
                [0.0, 0.0, 1.0],
                presentation_time,
                100 + u32::from(part),
            );
        }
        Ok(())
    }

    fn limb_smoke(&mut self, origin: Vec3, direction: [f32; 3], time: i64, salt: u32) {
        let Some(vfs) = self.vfs.clone() else { return };
        effect_runtime::spawn_effect(
            &mut self.particles,
            &mut self.effect_aux,
            &mut self.effects,
            &vfs,
            SMOKE,
            origin,
            Instant::now(),
            (time as u32).rotate_left(7) ^ salt,
            0,
            &mut None,
            combat_effects::rotation_from_direction(direction),
        );
    }
}

/// The world placement of a limb mesh: its entity's origin and angles, with the pivot
/// bone moved to that origin (`G2API_SetNewOrigin`, a translation only).
pub(crate) fn limb_instance(
    limb: &Limb,
    transform: &sjk_runtime::Transform,
) -> (glam::Vec3, glam::Quat) {
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let pivot = Vec3::from_array(limb.pivot) * Vec3::from_array(transform.scale);
    (
        Vec3::from_array(transform.translation) - rotation * pivot,
        rotation,
    )
}

/// A flying limb trails smoke from its cut every 0.4 s while it moves.
pub(crate) fn trail_due(limb: &Limb, origin: [f32; 3], now: i64) -> bool {
    let moved = limb
        .previous_origin
        .replace(Some(origin))
        .is_some_and(|previous| previous != origin);
    if !moved || limb.trail.get() >= now {
        return false;
    }
    limb.trail.set(now + 400);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(
        name: &str,
        flags: u32,
        parent: Option<usize>,
        children: &[usize],
    ) -> GlmSurfaceHierarchy {
        GlmSurfaceHierarchy {
            name: name.into(),
            flags,
            shader: String::new(),
            parent,
            children: children.to_vec(),
        }
    }

    /// hips > torso > (l_arm > l_hand, torso_cap_l_arm), l_arm > l_arm_cap_torso.
    fn model() -> Vec<GlmSurfaceHierarchy> {
        vec![
            surface("hips", 0, None, &[1]),
            surface("torso", 0, Some(0), &[2, 4]),
            surface("l_arm", 0, Some(1), &[3, 5]),
            surface("l_hand", 0, Some(2), &[]),
            surface("torso_cap_l_arm", OFF, Some(1), &[]),
            surface("l_arm_cap_torso", OFF, Some(2), &[]),
        ]
    }

    fn shown(surfaces: &Surfaces) -> Vec<bool> {
        surfaces.draw_visible.clone()
    }

    #[test]
    fn first_person_masks_the_head_subtree_and_restores_the_model_flags() {
        let hierarchy = vec![
            surface("torso", 0, None, &[1, 3]),
            surface("head", 0, Some(0), &[2]),
            surface("helmet", 0, Some(1), &[4]),
            surface("r_arm", 0, Some(0), &[]),
            surface("hair", 0, Some(2), &[]),
        ];
        let mut state = Surfaces::new(&hierarchy, vec![0; 5], 0..5);
        state.first_person_saber(true, &hierarchy);
        assert_eq!(shown(&state), [true, false, false, true, false]);
        state.first_person_saber(false, &hierarchy);
        assert_eq!(shown(&state), [true; 5]);
    }

    #[test]
    fn caps_are_carried_but_hidden_until_cut() {
        let mut hierarchy = model();
        let defaults = reveal_caps(&mut hierarchy);
        assert!(hierarchy.iter().all(|surface| surface.flags & OFF == 0));
        let surfaces = Surfaces::new(&hierarchy, defaults, 0..6);
        assert_eq!(shown(&surfaces), [true, true, true, true, false, false]);
    }

    #[test]
    fn a_cut_arm_leaves_the_stump_cap_and_the_limb_keeps_its_own() {
        let mut hierarchy = model();
        let defaults = reveal_caps(&mut hierarchy);
        let base = Surfaces::new(&hierarchy, defaults, 0..6);
        let names = cut(&base, &hierarchy, LARM, true);
        assert_eq!(
            (
                names.limb.as_str(),
                names.limb_cap.as_str(),
                names.stub_cap.as_str()
            ),
            ("l_arm", "l_arm_cap_torso", "torso_cap_l_arm")
        );
        let mut owner = base.clone();
        owner.set(2, NODESCENDANTS);
        owner.set(4, 0);
        owner.refresh(&hierarchy);
        assert_eq!(shown(&owner), [true, true, false, false, true, false]);
        let mut limb = base;
        limb.root = Some(2);
        limb.set(5, 0);
        limb.refresh(&hierarchy);
        assert_eq!(shown(&limb), [false, false, true, true, false, true]);
    }

    #[test]
    fn a_hidden_root_surface_is_found_by_its_variant() {
        let mut hierarchy = model();
        hierarchy[2].name = "l_arma".into();
        let defaults = reveal_caps(&mut hierarchy);
        let surfaces = Surfaces::new(&hierarchy, defaults, 0..6);
        assert_eq!(surfaces.variant(&hierarchy, "l_arm"), "l_arma");
    }

    #[test]
    fn reattaching_restores_the_model() {
        let mut hierarchy = model();
        let defaults = reveal_caps(&mut hierarchy);
        let mut surfaces = Surfaces::new(&hierarchy, defaults, 0..6);
        surfaces.set(2, NODESCENDANTS);
        surfaces.lost = 1 << 2;
        surfaces.reset(&hierarchy);
        assert_eq!(shown(&surfaces), [true, true, true, true, false, false]);
        assert_eq!(surfaces.lost, 0);
    }

    #[test]
    fn surface_state_is_copied_only_between_matching_layouts() {
        let mut hierarchy = model();
        let defaults = reveal_caps(&mut hierarchy);
        let mut source = Surfaces::new(&hierarchy, defaults.clone(), 0..6);
        source.set(2, NODESCENDANTS);
        source.lost = 1;
        // Same surfaces and draws: the cut is copied.
        let mut same = Surfaces::new(&hierarchy, defaults.clone(), 0..6);
        same.copy_from(&source, &hierarchy);
        assert_eq!(same.lost, 1);
        // Another model with as many surfaces but other draws: left whole.
        let mut other = Surfaces::new(&hierarchy, defaults, [0, 1, 3, 2, 4, 5].into_iter());
        other.copy_from(&source, &hierarchy);
        assert_eq!(other.lost, 0);
    }
}

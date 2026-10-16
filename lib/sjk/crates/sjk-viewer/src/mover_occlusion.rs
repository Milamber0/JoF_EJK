//! Movers (doors, lifts, `func_static` brushes) as occluders of the map's lamps.
//!
//! Lamp shadows come from a map-lifetime trace of the static world
//! (`lamp_visibility.rs`), which has no movers: a closed door let every lamp behind it
//! through. Each lamp a mover can reach gets a second tile in the same visibility
//! atlas, traced against the movers alone at their current pose; a receiver's lamp
//! visibility is the world tile's times the door tile's. A mover that changes pose
//! queues its lamps' door tiles again, traced on the GPU a few per frame
//! ([`gpu`]), so a lift moving all the time spreads its cost and the final pose is always
//! traced.
//!
//! Only movers the client knows block light: those in the current snapshot, at the pose
//! they are drawn with, and those of the map's baselines it has not seen yet, at their
//! spawn pose. A hidden or broken mover (not drawn) blocks nothing, and neither does one
//! missing from a snapshot that would hold it ([`Sight`]): the server removed it or hid it
//! with `SVF_NOCLIENT`. One left out because its place is out of the player's view keeps
//! its last pose.
use glam::{Quat, Vec3};
use std::collections::VecDeque;

#[path = "mover_occlusion_gpu.rs"]
pub(crate) mod gpu;

/// One mover's opaque, shadow-casting triangles in its model space.
pub(crate) struct Occluder {
    /// The mover catalog's mesh (`movers::Catalog::meshes`).
    pub(crate) mesh: usize,
    pub(crate) triangles: Vec<[Vec3; 3]>,
    /// Model-space bounds of `triangles`.
    pub(crate) lower: Vec3,
    pub(crate) upper: Vec3,
    /// The world box it can move through ([`reach`]).
    pub(crate) reach: (Vec3, Vec3),
    /// The lamps the static world lets see `reach` ([`seen_by`]), sorted; `None` for all.
    pub(crate) seen_by: Option<Vec<u32>>,
    /// Where the server would show it from ([`Sight`]).
    pub(crate) sight: Sight,
    /// It never stops ([`endless`]): no lamp gets a door tile for it.
    pub(crate) endless: bool,
}

impl Occluder {
    /// `None` without triangles.
    pub(crate) fn new(mesh: usize, triangles: Vec<[Vec3; 3]>, reach: (Vec3, Vec3)) -> Option<Self> {
        let mut lower = Vec3::splat(f32::INFINITY);
        let mut upper = Vec3::splat(f32::NEG_INFINITY);
        for point in triangles.iter().flatten() {
            lower = lower.min(*point);
            upper = upper.max(*point);
        }
        (!triangles.is_empty() && lower.is_finite() && upper.is_finite()).then_some(Self {
            mesh,
            triangles,
            lower,
            upper,
            reach,
            seen_by: None,
            sight: Sight::default(),
            endless: false,
        })
    }
}

/// The lamps whose reach touches `reach` and that see into it past the static world
/// (`blocked`): a ray from the lamp to one of 27 points spread through the box gets
/// through. A mover in another room needs no door tile for the lamp.
pub(crate) fn seen_by(
    lamps: &[crate::lamp_lights::Lamp],
    reach: (Vec3, Vec3),
    blocked: impl Fn(Vec3, Vec3) -> bool + Sync,
) -> Vec<u32> {
    let (lower, upper) = reach;
    // Just inside the box, off the walls a door slides against.
    let inset = ((upper - lower) * 0.5).min(Vec3::ONE);
    let (lower, upper) = (lower + inset, upper - inset);
    let points: Vec<Vec3> = (0..27)
        .map(|i| {
            let t = Vec3::new((i % 3) as f32, (i / 3 % 3) as f32, (i / 9) as f32) * 0.5;
            lower + (upper - lower) * t
        })
        .collect();
    let sees = |lamp: &crate::lamp_lights::Lamp| {
        touches(lamp.position, lamp.radius, reach.0, reach.1) && {
            let from = lamp.position + lamp.normal * 0.5;
            points.iter().any(|&point| !blocked(from, point))
        }
    };
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 4);
    let chunk = lamps.len().div_ceil(workers).max(1);
    let seen: Vec<Vec<u32>> = std::thread::scope(|scope| {
        let handles: Vec<_> = lamps
            .chunks(chunk)
            .enumerate()
            .map(|(part, chunk_lamps)| {
                let sees = &sees;
                scope.spawn(move || {
                    chunk_lamps
                        .iter()
                        .enumerate()
                        .filter(|(_, lamp)| sees(lamp))
                        .map(|(i, _)| (part * chunk + i) as u32)
                        .collect::<Vec<u32>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a lamp visibility worker"))
            .collect()
    });
    seen.concat()
}

/// The world box a brush model can move through, from its entity's spawn keys and its
/// model-space bounds. `func_door` and `func_button` slide between their spawn and
/// moved positions as `SP_func_door` computes them (`G_SetMovedir`, `lip`), `func_plat`
/// drops by its `height`; brushes that never move keep their spawn bounds. Anything
/// else (trains, rotating and bobbing movers, unknown classes) gets its bounds grown on
/// every side by its largest extent, which a train or a long swing can still leave.
pub(crate) fn reach(entity: Option<&sjk_entity::Entity>, lower: Vec3, upper: Vec3) -> (Vec3, Vec3) {
    let origin = entity
        .and_then(|e| e.vector("origin").ok().flatten())
        .map_or(Vec3::ZERO, Vec3::from_array);
    let (lower, upper) = (origin + lower, origin + upper);
    let size = upper - lower;
    let number = |key: &str, default: f32| {
        entity
            .and_then(|e| e.number(key).ok().flatten())
            .unwrap_or(default)
    };
    let slide = |travel: Vec3| (lower.min(lower + travel), upper.max(upper + travel));
    match entity.and_then(sjk_entity::Entity::classname) {
        Some("func_door" | "func_button") => {
            let angles = entity
                .and_then(|e| e.vector("angles").ok().flatten())
                .map(Vec3::from_array)
                .unwrap_or_else(|| Vec3::new(0., number("angle", 0.), 0.));
            let direction = if angles == Vec3::new(0., -1., 0.) {
                Vec3::Z
            } else if angles == Vec3::new(0., -2., 0.) {
                Vec3::NEG_Z
            } else {
                let (pitch, yaw) = (angles.x.to_radians(), angles.y.to_radians());
                Vec3::new(
                    pitch.cos() * yaw.cos(),
                    pitch.cos() * yaw.sin(),
                    -pitch.sin(),
                )
            };
            let lip = number(
                "lip",
                if entity.and_then(sjk_entity::Entity::classname) == Some("func_button") {
                    4.
                } else {
                    8.
                },
            );
            slide(direction * (direction.abs().dot(size) - lip))
        }
        Some("func_plat") => slide(Vec3::new(
            0.,
            0.,
            -number("height", size.z - number("lip", 8.)),
        )),
        Some("func_static" | "func_breakable" | "func_usable" | "func_glass" | "func_wall") => {
            (lower, upper)
        }
        _ => {
            let grow = Vec3::splat(size.max_element());
            (lower - grow, upper + grow)
        }
    }
}

/// Whether a mover moves for as long as the map runs: `func_bobbing`, `func_rotating`
/// and `func_pendulum`. Every pose change traces its door tiles again and re-bakes the
/// lamp cache around it, every frame without end (on `JoFTemple` 15 bobbing platforms
/// cost about 4 ms a frame at 4K), so it blocks no lamp; it still casts sun shadows.
pub(crate) fn endless(entity: Option<&sjk_entity::Entity>) -> bool {
    matches!(
        entity.and_then(sjk_entity::Entity::classname),
        Some("func_bobbing" | "func_rotating" | "func_pendulum")
    )
}

/// Whether a lamp's sphere of influence touches a box.
fn touches(position: Vec3, radius: f32, lower: Vec3, upper: Vec3) -> bool {
    position.clamp(lower, upper).distance_squared(position) < radius * radius
}

/// One door tile: the lamp it belongs to and the occluders that can shadow that lamp.
#[derive(Debug, PartialEq)]
pub(crate) struct Tile {
    pub(crate) lamp: u32,
    pub(crate) occluders: Vec<u32>,
}

/// The lamps that get a door tile and, per occluder, the tiles it can shadow.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Doors {
    pub(crate) tiles: Vec<Tile>,
    pub(crate) by_occluder: Vec<Vec<u32>>,
}

impl Doors {
    /// No door tiles for `occluders` occluders.
    pub(crate) fn none(occluders: usize) -> Self {
        Self {
            tiles: Vec::new(),
            by_occluder: vec![Vec::new(); occluders],
        }
    }

    /// Give a door tile to every lamp whose reach touches an occluder's, at most
    /// `capacity` of them: the most powerful when there are more.
    pub(crate) fn assign(
        lamps: &[crate::lamp_lights::Lamp],
        occluders: &[Occluder],
        capacity: usize,
    ) -> Self {
        let reaches: Vec<_> = occluders.iter().map(|occluder| occluder.reach).collect();
        let mut candidates: Vec<(u32, Vec<u32>)> = lamps
            .iter()
            .enumerate()
            .filter_map(|(lamp, light)| {
                let near: Vec<u32> = reaches
                    .iter()
                    .enumerate()
                    .filter(|(occluder, (lower, upper))| {
                        touches(light.position, light.radius, *lower, *upper)
                            && occluders[*occluder]
                                .seen_by
                                .as_ref()
                                .is_none_or(|seen| seen.binary_search(&(lamp as u32)).is_ok())
                    })
                    .map(|(occluder, _)| occluder as u32)
                    .collect();
                (!near.is_empty()).then_some((lamp as u32, near))
            })
            .collect();
        if candidates.len() > capacity {
            candidates.sort_by(|a, b| {
                lamps[b.0 as usize]
                    .power
                    .total_cmp(&lamps[a.0 as usize].power)
                    .then(a.0.cmp(&b.0))
            });
            candidates.truncate(capacity);
            candidates.sort_by_key(|candidate| candidate.0);
        }
        let mut by_occluder = vec![Vec::new(); occluders.len()];
        let tiles = candidates
            .into_iter()
            .enumerate()
            .map(|(tile, (lamp, occluders))| {
                for &occluder in &occluders {
                    by_occluder[occluder as usize].push(tile as u32);
                }
                Tile { lamp, occluders }
            })
            .collect();
        Self { tiles, by_occluder }
    }
}

/// Whether something inside the box `lower`..`upper` can stand between a lamp at
/// `lamp` and some point of the cell `cell_lower`..`cell_upper`: the cell meets the cone
/// from the lamp around the box's bounding sphere, beyond the sphere's near side.
/// Conservative: bounding spheres stand in for both boxes.
pub(crate) fn may_shadow(
    lamp: Vec3,
    lower: Vec3,
    upper: Vec3,
    cell_lower: Vec3,
    cell_upper: Vec3,
) -> bool {
    let centre = (lower + upper) * 0.5;
    let radius = (upper - lower).length() * 0.5;
    let to_box = centre - lamp;
    let distance = to_box.length();
    let cell_centre = (cell_lower + cell_upper) * 0.5;
    let cell_radius = (cell_upper - cell_lower).length() * 0.5;
    let to_cell = cell_centre - lamp;
    let reach = to_cell.length();
    if distance <= radius || reach <= cell_radius {
        return true;
    }
    if reach + cell_radius < distance - radius {
        return false;
    }
    let cone = (radius / distance).asin();
    let spread = (cell_radius / reach).asin();
    let angle = to_box.angle_between(to_cell);
    angle <= cone + spread
}

/// Per door tile, the lamp cache texels its movers can shadow: per layer, the union of
/// the texel rectangles (`[x0, y0, x1, y1)`, grown by two texels for the bake's filters)
/// of the surfaces in its lamp's reach that [`may_shadow`] finds behind them.
pub(crate) fn cache_regions(
    lamps: &[crate::lamp_lights::Lamp],
    doors: &Doors,
    occluders: &[Occluder],
    surfaces: &[Vec<crate::world_materials::lamp_cache::CachedSurface>],
    resolution: u32,
) -> Vec<Vec<(u32, [u32; 4])>> {
    doors
        .tiles
        .iter()
        .map(|tile| {
            let lamp = &lamps[tile.lamp as usize];
            let mut regions = Vec::new();
            for (layer, list) in surfaces.iter().enumerate() {
                let mut union: Option<[u32; 4]> = None;
                for surface in list {
                    if !touches(lamp.position, lamp.radius, surface.lower, surface.upper)
                        || !tile.occluders.iter().any(|&o| {
                            let (lower, upper) = occluders[o as usize].reach;
                            may_shadow(lamp.position, lower, upper, surface.lower, surface.upper)
                        })
                    {
                        continue;
                    }
                    let [x0, y0, x1, y1] = surface.texels;
                    let grown = [
                        x0.saturating_sub(2),
                        y0.saturating_sub(2),
                        (x1 + 2).min(resolution),
                        (y1 + 2).min(resolution),
                    ];
                    union = Some(union.map_or(grown, |u| merge(u, grown)));
                }
                if let Some(rect) = union {
                    regions.push((layer as u32, rect));
                }
            }
            regions
        })
        .collect()
}

/// The smallest rectangle holding both.
pub(crate) fn merge(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The clusters and areas a mover's reach box touches (`CM_BoxLeafnums`), to tell whether
/// a snapshot would hold it: `SV_AddEntitiesVisibleFromPoint` sends an entity when one of
/// its clusters is in the eye's PVS and one of its areas is connected. Built from the
/// whole reach box, which a mover's current place lies in. Only the first
/// [`SIGHT_CLUSTERS`] clusters are named and tested, so a mover touching more is taken as
/// out of view when none of those is visible: one the server dropped then keeps
/// blocking, rather than a mover it merely left out losing its shadow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Sight {
    clusters: [u32; SIGHT_CLUSTERS],
    cluster_count: u8,
    areas: [i32; SIGHT_AREAS],
    area_count: u8,
    /// The box touched more areas than are named: its area test passes.
    many_areas: bool,
}

const SIGHT_CLUSTERS: usize = 16;
const SIGHT_AREAS: usize = 4;

/// Where the server built the presented snapshot from (`SV_BuildClientSnapshot`: the
/// player's origin raised by its view height).
#[derive(Clone, Copy)]
pub(crate) struct Eye<'a> {
    /// The eye's cluster; `None` outside the map.
    pub(crate) cluster: Option<usize>,
    /// The map's PVS; `None` for a map without one, where every cluster sees every other.
    pub(crate) visibility: Option<&'a sjk_bsp::Visibility>,
}

impl Sight {
    pub(crate) fn new(bsp: &sjk_bsp::Bsp, lower: Vec3, upper: Vec3) -> Self {
        let mut leaves = [0usize; 256];
        let found = bsp.box_leaves(lower.to_array(), upper.to_array(), &mut leaves);
        let mut sight = Self::default();
        for &leaf in &leaves[..found.count] {
            let leaf = &bsp.leaves()[leaf];
            if let Ok(cluster) = u32::try_from(leaf.cluster) {
                sight.add_cluster(cluster);
            }
            if leaf.area >= 0 {
                sight.add_area(leaf.area);
            }
        }
        sight
    }

    fn add_cluster(&mut self, cluster: u32) {
        let count = usize::from(self.cluster_count);
        if self.clusters[..count].contains(&cluster) {
            return;
        }
        if count < SIGHT_CLUSTERS {
            self.clusters[count] = cluster;
            self.cluster_count += 1;
        }
    }

    fn add_area(&mut self, area: i32) {
        let count = usize::from(self.area_count);
        if self.areas[..count].contains(&area) {
            return;
        }
        if count == SIGHT_AREAS {
            self.many_areas = true;
        } else {
            self.areas[count] = area;
            self.area_count += 1;
        }
    }

    #[cfg(test)]
    pub(crate) fn of(clusters: &[u32], areas: &[i32]) -> Self {
        let mut sight = Self::default();
        clusters.iter().for_each(|&c| sight.add_cluster(c));
        areas.iter().for_each(|&a| sight.add_area(a));
        sight
    }

    /// Whether a snapshot taken where the eye sees the clusters `cluster_visible` accepts,
    /// with the areas `area_open` accepts connected, would hold the mover. A box in no
    /// cluster (in the void) is never in view.
    pub(crate) fn seen(
        &self,
        cluster_visible: impl Fn(usize) -> bool,
        area_open: impl Fn(i32) -> bool,
    ) -> bool {
        let clusters = &self.clusters[..usize::from(self.cluster_count)];
        let areas = &self.areas[..usize::from(self.area_count)];
        clusters.iter().any(|&c| cluster_visible(c as usize))
            && (self.many_areas || areas.is_empty() || areas.iter().any(|&a| area_open(a)))
    }
}

/// Where an occluder is and whether it blocks light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    pub(crate) origin: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) blocking: bool,
}

impl Pose {
    /// Unknown movers block nothing.
    pub(crate) const NONE: Self = Self {
        origin: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        blocking: false,
    };

    pub(crate) fn of(mover: &crate::movers::Presented) -> Self {
        Self {
            origin: Vec3::from_array(mover.origin),
            rotation: Quat::from_array(mover.rotation),
            blocking: mover.visible,
        }
    }

    /// A different pose for lighting: a hundredth of a unit, or about a hundredth of a
    /// degree, apart. A stopped mover evaluates to the same pose every frame.
    pub(crate) fn moved(&self, other: &Self) -> bool {
        self.blocking != other.blocking
            || (self.blocking
                && (self.origin.distance_squared(other.origin) > 1e-4
                    || turned(self.rotation, other.rotation)))
    }
}

/// Whether two rotations are more than about a hundredth of a degree apart, either sign
/// of the quaternion. Measured as the distance between the quaternions, which is zero for
/// equal ones: their dot product cannot tell, since `1 - 1e-8` is 1 in f32 and a
/// quaternion's dot with itself is one step below 1 at many angles (yaw 90 or 270 from
/// `legacy_angles_to_quaternion`), so a still mover looked as if it moved every frame.
fn turned(a: Quat, b: Quat) -> bool {
    let (a, b) = (glam::Vec4::from(a), glam::Vec4::from(b));
    // Unit quaternions θ apart are about θ/2 apart: (1e-4)² is θ near 0.0115°.
    (a - b).length_squared().min((a + b).length_squared()) > 1e-8
}

/// Door tiles waiting to be traced, oldest first, each at most once.
#[derive(Debug, Default)]
pub(crate) struct Queue {
    pending: VecDeque<u32>,
    queued: Vec<bool>,
}

impl Queue {
    pub(crate) fn new(tiles: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(tiles),
            queued: vec![false; tiles],
        }
    }

    pub(crate) fn push(&mut self, tile: u32) {
        if let Some(queued) = self.queued.get_mut(tile as usize)
            && !*queued
        {
            *queued = true;
            self.pending.push_back(tile);
        }
    }

    /// Move up to `budget` tiles into `batch`.
    pub(crate) fn take(&mut self, budget: usize, batch: &mut Vec<u32>) {
        batch.clear();
        while batch.len() < budget {
            let Some(tile) = self.pending.pop_front() else {
                break;
            };
            self.queued[tile as usize] = false;
            batch.push(tile);
        }
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }
}

/// Every occluder's current pose; queues the door tiles of those that change.
#[derive(Debug)]
pub(crate) struct Poses {
    pub(crate) current: Vec<Pose>,
    /// Occluders a snapshot or a baseline has placed.
    known: Vec<bool>,
    /// Counts pose changes, for the far sun cascade.
    pub(crate) generation: u64,
}

impl Poses {
    pub(crate) fn new(occluders: usize) -> Self {
        Self {
            current: vec![Pose::NONE; occluders],
            known: vec![false; occluders],
            generation: 0,
        }
    }

    /// Place `occluder` at `pose`, queueing its tiles when that changes its light.
    /// `current` keeps the pose last traced until the mover has moved past
    /// [`Pose::moved`]'s threshold from it, so motion slower than the threshold per
    /// frame still adds up to a trace.
    pub(crate) fn set(&mut self, occluder: usize, pose: Pose, doors: &Doors, queue: &mut Queue) {
        self.known[occluder] = true;
        if self.current[occluder].moved(&pose) {
            for &tile in &doors.by_occluder[occluder] {
                queue.push(tile);
            }
            self.generation = self.generation.wrapping_add(1);
            self.current[occluder] = pose;
        }
    }

    /// Place an occluder no snapshot has shown, at its spawn pose.
    pub(crate) fn set_unknown(
        &mut self,
        occluder: usize,
        pose: Pose,
        doors: &Doors,
        queue: &mut Queue,
    ) {
        if !self.known[occluder] {
            self.set(occluder, pose, doors, queue);
        }
    }

    pub(crate) fn any_unknown(&self) -> bool {
        self.known.iter().any(|known| !known)
    }
}

/// The CPU half of [`gpu::Runtime`]: which occluder each presented mover is, and where
/// every occluder stands.
pub(crate) struct Tracking {
    /// Per mover catalog mesh: its occluder.
    mesh_to_occluder: Vec<Option<u32>>,
    pub(crate) doors: Doors,
    pub(crate) poses: Poses,
    pub(crate) queue: Queue,
    /// Whether the baselines have placed the movers no snapshot has shown.
    baselines_placed: bool,
    /// Per occluder: where the server would show it from.
    sights: Vec<Sight>,
    /// Per occluder: the last [`Tracking::observe`] call whose snapshot held it.
    presented_at: Vec<u32>,
    observed: u32,
    /// Per occluder: an `EF_PERMANENT` baseline, drawn from the baseline and never sent in
    /// a snapshot.
    permanent: Vec<bool>,
}

impl Tracking {
    /// `doors` must name these occluders (`Doors::assign` over them, or [`Doors::none`]).
    pub(crate) fn new(occluders: &[Occluder], doors: Doors) -> Self {
        let meshes = occluders.iter().map(|o| o.mesh + 1).max().unwrap_or(0);
        let mut mesh_to_occluder = vec![None; meshes];
        for (index, occluder) in occluders.iter().enumerate() {
            mesh_to_occluder[occluder.mesh] = Some(index as u32);
        }
        let doors = if doors.by_occluder.len() == occluders.len() {
            doors
        } else {
            Doors::none(occluders.len())
        };
        Self {
            mesh_to_occluder,
            poses: Poses::new(occluders.len()),
            queue: Queue::new(doors.tiles.len()),
            doors,
            baselines_placed: false,
            sights: occluders.iter().map(|o| o.sight).collect(),
            presented_at: vec![0; occluders.len()],
            observed: 0,
            permanent: vec![false; occluders.len()],
        }
    }

    fn occluder_of(&self, mesh: Option<usize>) -> Option<usize> {
        mesh.and_then(|mesh| self.mesh_to_occluder.get(mesh).copied().flatten())
            .map(|occluder| occluder as usize)
    }

    /// Place the movers this frame presents, then the baselines' movers no snapshot has
    /// shown (once; each with whether it is `EF_PERMANENT`). With the presented movers
    /// coming from a snapshot, `in_view` tells whether that snapshot would hold a mover
    /// ([`Sight::seen`]): a blocking mover missing from it was removed or hidden by the
    /// server and stops blocking. Without a snapshot nothing is taken away.
    pub(crate) fn observe(
        &mut self,
        presented: &[crate::movers::Presented],
        baselines: Option<impl Iterator<Item = (crate::movers::Presented, bool)>>,
        mesh_of: impl Fn(usize) -> Option<usize>,
        in_view: Option<impl Fn(&Sight) -> bool>,
    ) {
        self.observed = self.observed.wrapping_add(1);
        for mover in presented {
            if let Some(occluder) = self.occluder_of(mesh_of(mover.model_index)) {
                self.poses
                    .set(occluder, Pose::of(mover), &self.doors, &mut self.queue);
                self.presented_at[occluder] = self.observed;
            }
        }
        if !self.baselines_placed
            && self.poses.any_unknown()
            && let Some(baselines) = baselines
        {
            for (mover, permanent) in baselines {
                if let Some(occluder) = self.occluder_of(mesh_of(mover.model_index)) {
                    self.permanent[occluder] |= permanent;
                    self.poses.set_unknown(
                        occluder,
                        Pose::of(&mover),
                        &self.doors,
                        &mut self.queue,
                    );
                }
            }
            self.baselines_placed = true;
        }
        let Some(in_view) = in_view else {
            return;
        };
        for occluder in 0..self.sights.len() {
            if self.poses.current[occluder].blocking
                && self.presented_at[occluder] != self.observed
                && !self.permanent[occluder]
                && in_view(&self.sights[occluder])
            {
                let removed = Pose {
                    blocking: false,
                    ..self.poses.current[occluder]
                };
                self.poses
                    .set(occluder, removed, &self.doors, &mut self.queue);
            }
        }
    }
}

#[cfg(test)]
#[path = "mover_occlusion_tests.rs"]
mod tests;

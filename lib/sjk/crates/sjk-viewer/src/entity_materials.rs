//! Allocation-free entity draw ordering for the shared Q3 stage runtime.
//!
//! rd-vanilla submits MD3 surfaces in `tr_mesh.cpp:386-416` and Ghoul2
//! surfaces in `tr_ghoul2.cpp:2460-2502` through `R_AddDrawSurf`, alongside
//! world surfaces. `R_SortDrawSurfs` (`tr_main.cpp:1134-1185`) orders by the
//! shader sort key and entity. SJK keeps separate world/entity traversal for
//! batching, but preserves the required ordering: all opaque work precedes
//! blended work, then entity blends are ordered by shader sort and entity
//! distance back-to-front. `RF_FORCE_ENT_ALPHA` entities are post-rendered
//! after every other surface (`tr_backend.cpp:755-761`), so their draws close
//! the blended list.

use super::{ActorDraw, ActorInstance, ActorMesh, StaticModelMesh};
use crate::world_materials::Runtime;
use glam::Vec3;
use std::ops::Range;

const MAX_ENTITY_DRAWS: usize = 16_384;
/// Surfaces of the outlined bodies: the card's player and every team-outlined one
/// (player models have a few dozen surfaces each).
const OUTLINE_DRAWS: usize = 2_048;

/// One extra entity draw of an existing mesh: a cgame custom shader replacing
/// every surface shader, or the surfaces' own shaders drawn with forced alpha.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OverrideInstance {
    pub(crate) mesh: OverrideMesh,
    /// Replacement shader; `None` keeps each surface's own.
    pub(crate) material: Option<usize>,
    pub(crate) instance: ActorInstance,
    pub(crate) no_depth: bool,
    /// rd-vanilla `RF_FORCE_ENT_ALPHA`: blend every stage by the instance's
    /// `entity_color` alpha, which `entity_control.y` must also select.
    pub(crate) forced_alpha: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverrideMesh {
    Actor(usize),
    Object(usize),
}

/// Buffer range created for one override instance.
#[derive(Clone, Debug)]
pub(crate) struct OverrideRange {
    mesh: OverrideMesh,
    material: Option<usize>,
    instances: Range<u32>,
    no_depth: bool,
    forced_alpha: bool,
}

/// One surface/instance-range submission consumed by `world_materials`.
#[derive(Clone, Debug)]
pub(crate) struct Draw {
    pub(crate) indices: Range<u32>,
    pub(crate) material: usize,
    pub(crate) instances: Range<u32>,
    pub(crate) no_depth: bool,
    /// Drawn with the stage's forced-alpha pipeline after all other blends.
    pub(crate) forced_alpha: bool,
    distance_squared: f32,
    /// Opaque, depth-tested and backed by a registered material.
    pub(crate) stage_major: bool,
}

/// How one mesh's surfaces are submitted.
#[derive(Clone, Copy)]
struct Submission {
    material: Option<usize>,
    no_depth: bool,
    forced_alpha: bool,
}

impl Submission {
    const PLAIN: Self = Self {
        material: None,
        no_depth: false,
        forced_alpha: false,
    };
}

/// Reused fixed-capacity opaque and blended entity draw lists.
pub(crate) struct Queue {
    opaque: Vec<Draw>,
    blended: Vec<Draw>,
    /// The player outlines' mask draws, kept out of every scene pass
    /// (`card_outline.rs`).
    outline: Vec<Draw>,
    dropped: usize,
}

impl Queue {
    pub(crate) fn new() -> Self {
        Self {
            opaque: Vec::with_capacity(MAX_ENTITY_DRAWS),
            blended: Vec::with_capacity(MAX_ENTITY_DRAWS),
            outline: Vec::with_capacity(OUTLINE_DRAWS),
            dropped: 0,
        }
    }

    /// Rebuild draw references without cloning meshes or growing storage. Overrides
    /// drawn with one of `outlines` (the player outlines' mask materials) go to
    /// [`Self::outline`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rebuild(
        &mut self,
        runtime: &Runtime,
        actors: &[ActorMesh],
        actor_ranges: &[Range<u32>],
        objects: &[StaticModelMesh],
        object_ranges: &[Range<u32>],
        overrides: &[OverrideRange],
        instances: &[ActorInstance],
        camera: Vec3,
        outlines: [usize; 3],
    ) {
        self.opaque.clear();
        self.blended.clear();
        self.outline.clear();
        self.dropped = 0;
        for (mesh, range) in actors.iter().zip(actor_ranges) {
            self.append_mesh(
                runtime,
                &mesh.draws,
                &mesh.surfaces.draw_visible,
                range,
                Submission::PLAIN,
                instances,
                camera,
            );
        }
        for (mesh, range) in objects.iter().zip(object_ranges) {
            self.append_mesh(
                runtime,
                &mesh.draws,
                &[],
                range,
                Submission::PLAIN,
                instances,
                camera,
            );
        }
        for entry in overrides {
            let draws = match entry.mesh {
                OverrideMesh::Actor(index) => actors
                    .get(index)
                    .map(|mesh| (mesh.draws.as_slice(), mesh.surfaces.draw_visible.as_slice())),
                OverrideMesh::Object(index) => objects
                    .get(index)
                    .map(|mesh| (mesh.draws.as_slice(), &[][..])),
            };
            let Some((draws, visible)) = draws else {
                continue;
            };
            if let Some(outline) = entry
                .material
                .filter(|material| outlines.contains(material))
            {
                for (index, surface) in draws.iter().enumerate() {
                    if visible.get(index).copied().unwrap_or(true)
                        && self.outline.len() < self.outline.capacity()
                    {
                        self.outline.push(Draw {
                            indices: surface.indices.clone(),
                            material: outline,
                            instances: entry.instances.clone(),
                            no_depth: false,
                            forced_alpha: false,
                            distance_squared: 0.0,
                            stage_major: false,
                        });
                    }
                }
                continue;
            }
            let submission = Submission {
                material: entry.material,
                no_depth: entry.no_depth,
                forced_alpha: entry.forced_alpha,
            };
            self.append_mesh(
                runtime,
                draws,
                visible,
                &entry.instances,
                submission,
                instances,
                camera,
            );
        }
        self.opaque.sort_unstable_by(|left, right| {
            let left_order = runtime.material_order(left.material);
            let right_order = runtime.material_order(right.material);
            left_order
                .0
                .total_cmp(&right_order.0)
                .then(left_order.1.cmp(&right_order.1))
                .then(left.material.cmp(&right.material))
        });
        self.blended.sort_unstable_by(|left, right| {
            left.forced_alpha
                .cmp(&right.forced_alpha)
                .then(
                    runtime
                        .material_order(left.material)
                        .0
                        .total_cmp(&runtime.material_order(right.material).0),
                )
                .then(right.distance_squared.total_cmp(&left.distance_squared))
                .then(left.material.cmp(&right.material))
        });
    }

    /// `visible` hides draws by index (dismembered surfaces); a draw past its end shows.
    #[allow(clippy::too_many_arguments)]
    fn append_mesh(
        &mut self,
        runtime: &Runtime,
        draws: &[ActorDraw],
        visible: &[bool],
        instances_range: &Range<u32>,
        submission: Submission,
        instances: &[ActorInstance],
        camera: Vec3,
    ) {
        if instances_range.is_empty() {
            return;
        }
        let Submission {
            material: override_material,
            no_depth,
            forced_alpha,
        } = submission;
        for (index, surface) in draws.iter().enumerate() {
            if !visible.get(index).copied().unwrap_or(true) {
                continue;
            }
            let material = override_material.unwrap_or(surface.material);
            let blended = runtime.material_blended(material);
            let stage_major = !no_depth && !forced_alpha && blended == Some(false);
            if forced_alpha || blended == Some(true) {
                for instance in instances_range.clone() {
                    let Some(value) = instances.get(instance as usize) else {
                        continue;
                    };
                    self.push(
                        Draw {
                            indices: surface.indices.clone(),
                            material,
                            instances: instance..instance + 1,
                            no_depth,
                            forced_alpha,
                            stage_major,
                            distance_squared: Vec3::from_array(value.position)
                                .distance_squared(camera),
                        },
                        true,
                    );
                }
            } else {
                self.push(
                    Draw {
                        indices: surface.indices.clone(),
                        material,
                        instances: instances_range.clone(),
                        no_depth,
                        forced_alpha: false,
                        stage_major,
                        distance_squared: 0.0,
                    },
                    false,
                );
            }
        }
    }

    fn push(&mut self, draw: Draw, blended: bool) {
        let target = if blended {
            &mut self.blended
        } else {
            &mut self.opaque
        };
        if target.len() == target.capacity() {
            self.dropped += 1;
        } else {
            target.push(draw);
        }
    }

    pub(crate) fn opaque(&self) -> &[Draw] {
        &self.opaque
    }

    pub(crate) fn blended(&self) -> &[Draw] {
        &self.blended
    }

    /// The player outlines' mask draws: each body's visible surfaces, one instance each,
    /// with its group's mask material.
    pub(crate) fn outline(&self) -> &[Draw] {
        &self.outline
    }
}

/// Append cgame override instances to the shared GPU instance stream while
/// retaining the mesh/material identity needed by the draw queue.
pub(crate) fn append_override_ranges(
    instances: &mut Vec<ActorInstance>,
    overrides: &[OverrideInstance],
    ranges: &mut Vec<OverrideRange>,
) {
    ranges.clear();
    for entry in overrides {
        if instances.len() >= crate::actor_instance::CAPACITY || ranges.len() == ranges.capacity() {
            break;
        }
        let start = u32::try_from(instances.len()).unwrap_or(u32::MAX);
        instances.push(entry.instance);
        ranges.push(OverrideRange {
            mesh: entry.mesh,
            material: entry.material,
            instances: start..start + 1,
            no_depth: entry.no_depth,
            forced_alpha: entry.forced_alpha,
        });
    }
}

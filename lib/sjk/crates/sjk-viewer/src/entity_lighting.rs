//! Per-entity light from the BSP light grid.
//!
//! Mirrors rd-vanilla `R_SetupEntityLighting` (`tr_light.cpp:304-412`):
//! sample the grid at the entity origin (`R_SetupEntityLightingGrid`,
//! `tr_light.cpp:134-264`, implemented generically in `sjk-bsp`), scale by
//! `r_ambientScale`/`r_directedScale` (`tr_init.cpp:1625-1626`, 0.6 and 1),
//! add the unconditional 32-unit minimum ambient (`tr_light.cpp:344-349`),
//! fold every dynamic light into the directed term and direction
//! (`tr_light.cpp:372-388`) and clamp ambient to `identityLightByte`.
//! Without a usable grid the reference lights entities with a flat 150/150
//! from the default sun direction (`tr_light.cpp:335-341`); that is
//! `EntityLight::FALLBACK`.
//!
//! SJK runs without overbright bits (windowed/modern presentation), so
//! `tr.identityLight == 1` and `identityLightByte == 255` throughout.
//! Light styles are treated as white: `CG_RunLightStyles`
//! (`cgame/cg_light.c:52-79`) emits 255 for every style that has no
//! `CS_LIGHT_STYLES` pattern, which is the case for the retail multiplayer
//! maps; animated patterns remain a follow-up shared with lightmap styles.

use crate::actor_instance::{ActorInstance, EntityLight};
use crate::dynamic_lights::{PointLight, PointLightList};
use crate::entity_materials::OverrideInstance;
use sjk_bsp::{Bsp, GridLight, LightGridLayout};

/// q3map default cell size (`R_LoadEntities`, `tr_bsp.cpp:1890-1892`).
const DEFAULT_GRID_SIZE: [f32; 3] = [64.0, 64.0, 128.0];
/// `r_ambientScale` / `r_directedScale` defaults (`tr_init.cpp:1625-1626`).
const AMBIENT_SCALE: f32 = 0.6;
const DIRECTED_SCALE: f32 = 1.0;
/// `tr_light.cpp:346-348` minimum ambient add, times `identityLight`.
const MINIMUM_AMBIENT: f32 = 32.0;
/// `DLIGHT_AT_RADIUS` / `DLIGHT_MINIMUM_RADIUS` (`tr_light.cpp:28-31`).
const DLIGHT_AT_RADIUS: f32 = 16.0;
const DLIGHT_MINIMUM_RADIUS: f32 = 16.0;
const IDENTITY_LIGHT_BYTE: f32 = 255.0;

/// Map-lifetime light-grid placement for the loaded world.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EntityLighting {
    layout: Option<LightGridLayout>,
}

impl EntityLighting {
    /// Share the validated map layout with the optional GPU sampler.
    pub(crate) fn layout(&self) -> Option<LightGridLayout> {
        self.layout
    }

    /// Resolve the grid layout from the world model and the worldspawn
    /// `gridsize` key (`R_LoadEntities`, `tr_bsp.cpp:1957-1960`).
    pub(crate) fn from_world(bsp: &Bsp) -> Self {
        let grid_size = sjk_entity::parse_entity_lump(bsp.entities())
            .ok()
            .and_then(|entities| {
                entities
                    .iter()
                    .find(|entity| entity.classname() == Some("worldspawn"))
                    .and_then(|world| world.vector("gridsize").ok().flatten())
            })
            .unwrap_or(DEFAULT_GRID_SIZE);
        Self {
            layout: bsp.render().light_grid_layout(grid_size),
        }
    }

    /// Light one entity origin.
    pub(crate) fn sample(&self, bsp: &Bsp, origin: [f32; 3], lights: &[PointLight]) -> EntityLight {
        let grid = match &self.layout {
            Some(layout) => {
                let mut grid = layout.sample(bsp.render(), origin, |_| [255.0; 3]);
                grid.ambient
                    .iter_mut()
                    .for_each(|value| *value *= AMBIENT_SCALE);
                grid.directed
                    .iter_mut()
                    .for_each(|value| *value *= DIRECTED_SCALE);
                grid
            }
            None => grid_less(),
        };
        finish(grid, origin, lights)
    }

    /// Light/fog every instance at its origin; return the number with nonzero fog.
    ///
    /// Deviation: cgame lights attached weapons from the owner's
    /// `lightingOrigin` (`RF_LIGHTING_ORIGIN`); this samples the hilt at its
    /// own position, which stays within one grid cell of the owner.
    pub(crate) fn apply(
        &self,
        bsp: &Bsp,
        lights: &[PointLight],
        groups: &mut [Vec<ActorInstance>],
        fogs: &crate::fog_volumes::Table,
    ) -> usize {
        let mut fogged = 0;
        for instance in groups.iter_mut().flat_map(|group| group.iter_mut()) {
            instance.set_light(self.sample(bsp, instance.position, lights));
            instance.fog_index = fogs.at_sphere(instance.position, 0.0);
            fogged += usize::from(instance.fog_index != 0);
        }
        fogged
    }
}

/// Light everything the entity stage pipeline draws this frame: skinned
/// actors, rigid objects (weapons, projectiles, emitter models, pickups) and
/// the pickup shader-override instances. Returns the fogged count; allocation-free.
pub(crate) fn apply_frame(
    lighting: &EntityLighting,
    bsp: &Bsp,
    lights: &PointLightList,
    actor_groups: &mut [Vec<ActorInstance>],
    object_groups: &mut [Vec<ActorInstance>],
    overrides: &mut [OverrideInstance],
    fogs: &crate::fog_volumes::Table,
) -> usize {
    let lights = lights.as_slice();
    let mut fogged = lighting.apply(bsp, lights, actor_groups, fogs);
    fogged += lighting.apply(bsp, lights, object_groups, fogs);
    for entry in overrides.iter_mut() {
        let light = lighting.sample(bsp, entry.instance.position, lights);
        entry.instance.set_light(light);
        entry.instance.fog_index = fogs.at_sphere(entry.instance.position, 0.0);
        fogged += usize::from(entry.instance.fog_index != 0);
    }
    fogged
}

/// `tr_light.cpp:335-341`: flat light for maps without a usable grid.
fn grid_less() -> GridLight {
    GridLight {
        ambient: [150.0; 3],
        directed: [150.0; 3],
        direction: EntityLight::FALLBACK.direction,
    }
}

/// `tr_light.cpp:343-412` after the grid sample: minimum add, dynamic
/// lights, ambient clamp and direction normalisation.
fn finish(mut grid: GridLight, origin: [f32; 3], lights: &[PointLight]) -> EntityLight {
    grid.ambient
        .iter_mut()
        .for_each(|value| *value += MINIMUM_AMBIENT);
    let magnitude = length(grid.directed);
    let mut direction = grid.direction.map(|value| value * magnitude);
    for light in lights {
        let mut towards = [0.0; 3];
        for axis in 0..3 {
            towards[axis] = light.origin[axis] - origin[axis];
        }
        let distance = length(towards);
        if distance > 0.0 {
            towards = towards.map(|value| value / distance);
        }
        let power = DLIGHT_AT_RADIUS * light.radius * light.radius;
        let clamped = distance.max(DLIGHT_MINIMUM_RADIUS);
        let strength = power / (clamped * clamped);
        for axis in 0..3 {
            grid.directed[axis] += strength * light.color[axis];
            direction[axis] += strength * towards[axis];
        }
    }
    let ambient = grid
        .ambient
        .map(|value| value.min(IDENTITY_LIGHT_BYTE) / 255.0);
    let directed = grid.directed.map(|value| value / 255.0);
    let magnitude = length(direction);
    let direction = if magnitude > 0.0 {
        direction.map(|value| value / magnitude)
    } else {
        EntityLight::FALLBACK.direction
    };
    EntityLight {
        ambient,
        directed,
        direction,
    }
}

fn length(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

//! Reflective floor discovery from material intent and planar geometry, at map load.
use super::*;
use std::collections::HashMap;

pub(super) struct Face {
    pub indices: Range<u32>,
    /// The flattened scene's material, whose maps (if any) the finish reads.
    pub material: usize,
    /// Index into `Floors::map_groups`, 0 the neutral group.
    pub maps: usize,
    pub clusters: Vec<usize>,
    pub center: Vec3,
    pub radius: f32,
    pub bounds: [[f32; 3]; 2],
}
pub(super) struct Plane {
    pub normal: Vec3,
    pub distance: f32,
    pub faces: Vec<Face>,
    /// Some face has material maps: the region keeps a wider margin.
    pub mapped: bool,
}

/// Authored environment mapping is explicit polished-surface intent, unlike colour.
pub(super) fn glossy(def: &sjk_shader::ShaderDefinition) -> bool {
    def.resolved_sort() == 3.
        && !def.emits_light
        && def.sky.is_none()
        && def.deforms.is_empty()
        && !def
            .stages
            .iter()
            .any(|s| s.glow || s.alpha_function.is_some())
        && def
            .stages
            .iter()
            .any(|s| s.texture_generator == sjk_shader::TextureGenerator::Environment)
}

pub(super) fn collect(scene: &FlattenedScene, shaders: &ShaderCatalog) -> Vec<Plane> {
    let mut planes: Vec<Plane> = Vec::new();
    let mut keys = HashMap::new();
    for draw in &scene.draws {
        if !draw.world_surface
            || !shaders
                .get(&scene.materials[draw.material].shader)
                .is_some_and(glossy)
        {
            continue;
        }
        let indices = &scene.indices[draw.indices.start as usize..draw.indices.end as usize];
        let Some(triangle) = indices.chunks_exact(3).find(|t| {
            let p = |i: u32| Vec3::from_array(scene.vertices[i as usize].position);
            (p(t[1]) - p(t[0]))
                .cross(p(t[2]) - p(t[0]))
                .length_squared()
                > 1e-6
        }) else {
            continue;
        };
        let p = |i: u32| Vec3::from_array(scene.vertices[i as usize].position);
        let mut normal = (p(triangle[1]) - p(triangle[0]))
            .cross(p(triangle[2]) - p(triangle[0]))
            .normalize();
        let authored = Vec3::from_array(scene.vertices[triangle[0] as usize].normal);
        if normal.dot(authored) < 0. {
            normal = -normal;
        }
        if normal.z < 0.7 {
            continue;
        }
        let distance = normal.dot(p(triangle[0]));
        if indices
            .iter()
            .any(|&i| (normal.dot(p(i)) - distance).abs() > 0.02)
        {
            continue;
        }
        let center = indices.iter().map(|&i| p(i)).sum::<Vec3>() / indices.len() as f32;
        let radius = indices
            .iter()
            .map(|&i| p(i).distance(center))
            .fold(0., f32::max);
        let key = (
            (normal * 10000.).round().as_ivec3(),
            (distance * 10.).round() as i32,
        );
        let index = *keys.entry(key).or_insert_with(|| {
            let i = planes.len();
            planes.push(Plane {
                normal,
                distance,
                faces: Vec::new(),
                mapped: false,
            });
            i
        });
        planes[index].faces.push(Face {
            indices: draw.indices.clone(),
            material: draw.material,
            maps: 0,
            clusters: draw.clusters.clone(),
            center,
            radius,
            bounds: draw.bounds,
        });
    }
    planes
}

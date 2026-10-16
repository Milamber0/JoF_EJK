//! The models world notes select ([`pick_model`]): any map entity naming an MD3, met
//! by the view ray on the triangles of its first frame, turned and scaled as the entity
//! says. Each MD3 is read once per map, on a press, never per frame ([`ModelCache`]).

use super::{
    Entity, FILL_TRIANGLES, HIGHLIGHT_EDGES, Model, ModelShader, REACH, box_edges, box_triangles,
    ray_box,
};
use glam::{Quat, Vec3};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::sync::Arc;

/// Two triangles of a model meeting at more than this angle (its cosine) keep their
/// shared edge in the outline; flatter edges are left out, so a smooth model's outline
/// stays within [`HIGHLIGHT_EDGES`].
const CREASE_COSINE: f32 = 0.87;

/// An MD3's first frame: each surface's name, shader and triangles in model space, and
/// the frame's bounds.
#[derive(Debug)]
pub(super) struct Mesh {
    pub(super) surfaces: Vec<MeshSurface>,
    pub(super) low: Vec3,
    pub(super) high: Vec3,
}

#[derive(Debug)]
pub(super) struct MeshSurface {
    pub(super) name: String,
    pub(super) shader: String,
    pub(super) triangles: Vec<[Vec3; 3]>,
}

impl Mesh {
    fn from_md3(model: &sjk_model::Md3) -> Self {
        let surfaces = model
            .surfaces
            .iter()
            .map(|surface| {
                let vertices = surface.frames.first().map_or(&[][..], Vec::as_slice);
                let at = |index: u32| {
                    vertices
                        .get(index as usize)
                        .map(|vertex| Vec3::from_array(vertex.position))
                };
                MeshSurface {
                    name: surface.name.clone(),
                    shader: surface.shaders.first().cloned().unwrap_or_default(),
                    triangles: surface
                        .triangles
                        .iter()
                        .filter_map(|[a, b, c]| Some([at(*a)?, at(*b)?, at(*c)?]))
                        .collect(),
                }
            })
            .collect::<Vec<_>>();
        let (low, high) = surfaces
            .iter()
            .flat_map(|surface| surface.triangles.iter().flatten())
            .fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(low, high), p| (low.min(*p), high.max(*p)),
            );
        Self {
            surfaces,
            low,
            high,
        }
    }
}

/// The MD3s world notes read, each once per map (on a press, never per frame).
#[derive(Default)]
pub(crate) struct ModelCache {
    pub(super) map: String,
    pub(super) meshes: HashMap<String, Option<Arc<Mesh>>>,
}

impl ModelCache {
    /// The first frame of `path`, or `None` when it is no readable MD3.
    fn mesh(&mut self, vfs: &VirtualFileSystem, map: &str, path: &str) -> Option<Arc<Mesh>> {
        if self.map != map {
            self.map = map.to_owned();
            self.meshes.clear();
        }
        self.meshes
            .entry(path.to_ascii_lowercase())
            .or_insert_with(|| {
                let asset = vfs.read(path).ok()??;
                let model = sjk_model::Md3::parse(&asset.bytes).ok()?;
                Some(Arc::new(Mesh::from_md3(&model)))
            })
            .clone()
    }
}

/// Where the ray from `origin` along `direction` meets the triangle, two-sided
/// (Moller and Trumbore), as a multiple of `direction`.
fn ray_triangle(origin: Vec3, direction: Vec3, [a, b, c]: &[Vec3; 3]) -> Option<f32> {
    let (ab, ac) = (*b - *a, *c - *a);
    let p = direction.cross(ac);
    let determinant = ab.dot(p);
    if determinant.abs() < 1e-9 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let s = origin - *a;
    let u = s.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = ac.dot(q) * inverse;
    (t > 0.0).then_some(t)
}

/// The model of the map's entities that the ray from `eye` along `direction` meets first
/// on its triangles: any entity naming an MD3 (`misc_model_static` props, which the
/// client draws from the map; breakables and other server entities, taken where the map
/// places them). Its shaders are the model's own; the caller adds their remaps.
pub(super) fn pick_model(
    entities: &[sjk_entity::Entity],
    cache: &mut ModelCache,
    vfs: &VirtualFileSystem,
    map: &str,
    eye: Vec3,
    direction: Vec3,
) -> Option<Model> {
    let mut best: Option<(
        f32,
        usize,
        crate::static_models::Spawn,
        Arc<Mesh>,
        usize,
        usize,
    )> = None;
    for (number, entity) in entities.iter().enumerate() {
        let Some(spawn) = crate::static_models::Spawn::placement(entity) else {
            continue;
        };
        if !spawn.model.to_ascii_lowercase().ends_with(".md3") {
            continue;
        }
        let Some(mesh) = cache.mesh(vfs, map, &spawn.model) else {
            continue;
        };
        // Into model space: world = origin + rotation (scale * local), so the ray's
        // parameter stays the world distance along `direction`.
        let rotation = Quat::from_array(sjk_client::legacy_angles_to_quaternion(spawn.angles));
        let scale = Vec3::from_array(spawn.scale);
        if scale.min_element().abs() < 1e-6 {
            continue;
        }
        let inverse = rotation.inverse();
        let origin = inverse * (eye - Vec3::from_array(spawn.origin)) / scale;
        let local = inverse * direction / scale;
        let Some(entry) = ray_box(origin, local, mesh.low, mesh.high) else {
            continue;
        };
        if entry > REACH || best.as_ref().is_some_and(|best| best.0 <= entry) {
            continue;
        }
        for (surface_index, surface) in mesh.surfaces.iter().enumerate() {
            for (triangle_index, triangle) in surface.triangles.iter().enumerate() {
                let Some(t) = ray_triangle(origin, local, triangle) else {
                    continue;
                };
                if t <= REACH && best.as_ref().is_none_or(|best| t < best.0) {
                    best = Some((
                        t,
                        number,
                        spawn.clone(),
                        mesh.clone(),
                        surface_index,
                        triangle_index,
                    ));
                }
            }
        }
    }
    let (distance, number, spawn, mesh, surface_index, triangle_index) = best?;
    let rotation = Quat::from_array(sjk_client::legacy_angles_to_quaternion(spawn.angles));
    let scale = Vec3::from_array(spawn.scale);
    let place = |point: Vec3| Vec3::from_array(spawn.origin) + rotation * (point * scale);
    let [a, b, c] = mesh.surfaces[surface_index].triangles[triangle_index].map(place);
    let mut normal = (b - a).cross(c - a).normalize_or_zero();
    if normal.dot(direction) > 0.0 {
        normal = -normal;
    }
    let entity = &entities[number];
    let text = |key: &str| entity.get(key).unwrap_or("").to_owned();
    let mut shaders: Vec<ModelShader> = Vec::new();
    for surface in &mesh.surfaces {
        if !shaders.iter().any(|known| known.shader == surface.shader) {
            shaders.push(ModelShader {
                surface: surface.name.clone(),
                shader: surface.shader.clone(),
                remapped: None,
            });
        }
    }
    // The hit surface's shader first: the one the player aimed at.
    let aimed = &mesh.surfaces[surface_index].shader;
    if let Some(at) = shaders.iter().position(|shader| &shader.shader == aimed) {
        shaders[..=at].rotate_right(1);
    }
    let triangles: Vec<[Vec3; 3]> = mesh
        .surfaces
        .iter()
        .flat_map(|surface| surface.triangles.iter())
        .map(|triangle| triangle.map(place))
        .collect();
    let boxed = |low, high| {
        let edges = box_edges(low, high)
            .into_iter()
            .map(|edge| edge.map(place))
            .collect();
        let faces = box_triangles(low, high)
            .into_iter()
            .map(|triangle| triangle.map(place))
            .collect();
        (edges, faces)
    };
    let (edges, triangles) = model_outline(&triangles, boxed, (mesh.low, mesh.high));
    Some(Model {
        entity: Entity {
            number,
            classname: text("classname"),
            targetname: text("targetname"),
            model: spawn.model.clone(),
            origin: text("origin"),
            shaders: Vec::new(),
        },
        angles: spawn.angles,
        scale: spawn.scale,
        skin: entity
            .get("skin")
            .or_else(|| entity.get("customSkin"))
            .unwrap_or("")
            .to_owned(),
        shaders,
        hit: eye + direction * distance,
        normal,
        distance,
        edges,
        triangles,
    })
}

/// An edge's two ends and the normals of the faces sharing it.
type SharedEdge = (Vec3, Vec3, Vec<Vec3>);

/// A model's outline and shade from its world-space `triangles`: the edges where its
/// faces fold (or end) and its triangles, when they keep within [`HIGHLIGHT_EDGES`] and
/// [`FILL_TRIANGLES`]; else the edges and faces of its bounds `low` to `high` in model
/// space, which `boxed` places in the world.
fn model_outline(
    triangles: &[[Vec3; 3]],
    boxed: impl Fn(Vec3, Vec3) -> (Vec<[Vec3; 2]>, Vec<[Vec3; 3]>),
    (low, high): (Vec3, Vec3),
) -> (Vec<[Vec3; 2]>, Vec<[Vec3; 3]>) {
    let key = |p: Vec3| p.to_array().map(f32::to_bits);
    // Each edge, either way round, with the normals of the faces that share it.
    let mut shared: HashMap<([u32; 3], [u32; 3]), SharedEdge> = HashMap::new();
    for triangle in triangles {
        let normal = (triangle[1] - triangle[0])
            .cross(triangle[2] - triangle[0])
            .normalize_or_zero();
        for index in 0..3 {
            let (a, b) = (triangle[index], triangle[(index + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            let id = if ka <= kb { (ka, kb) } else { (kb, ka) };
            shared
                .entry(id)
                .or_insert((a, b, Vec::new()))
                .2
                .push(normal);
        }
    }
    let edges: Vec<[Vec3; 2]> = shared
        .values()
        .filter(|(_, _, normals)| {
            normals.len() == 1
                || normals
                    .iter()
                    .any(|n| normals.iter().any(|m| n.dot(*m) < CREASE_COSINE))
        })
        .map(|(a, b, _)| [*a, *b])
        .collect();
    let (box_edges, box_triangles) = boxed(low, high);
    let edges = if edges.is_empty() || edges.len() > HIGHLIGHT_EDGES {
        box_edges
    } else {
        edges
    };
    let triangles = if triangles.len() > FILL_TRIANGLES {
        box_triangles
    } else {
        triangles.to_vec()
    };
    (edges, triangles)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A cube of side 16 round the origin, one surface, as an MD3's first frame.
    fn cube() -> Mesh {
        let (low, high) = (Vec3::splat(-8.0), Vec3::splat(8.0));
        Mesh {
            surfaces: vec![MeshSurface {
                name: "crate".into(),
                shader: "models/test/crate".into(),
                triangles: box_triangles(low, high),
            }],
            low,
            high,
        }
    }

    pub(in crate::world_notes) fn cube_map() -> (Vec<sjk_entity::Entity>, ModelCache) {
        let lump = concat!(
            "{\n\"classname\" \"worldspawn\"\n}\n",
            "{\n\"classname\" \"misc_model_static\"\n\"model\" \"models/test/crate.md3\"\n",
            "\"origin\" \"100 0 0\"\n\"angle\" \"45\"\n\"modelscale\" \"2\"\n}\n",
            "{\n\"classname\" \"misc_model_static\"\n\"model\" \"models/test/crate.md3\"\n",
            "\"origin\" \"300 0 0\"\n}\n",
        );
        let entities = sjk_entity::parse_entity_lump(lump.as_bytes()).expect("parses");
        let mut cache = ModelCache {
            map: "maps/test.bsp".into(),
            meshes: HashMap::new(),
        };
        cache
            .meshes
            .insert("models/test/crate.md3".into(), Some(Arc::new(cube())));
        (entities, cache)
    }

    #[test]
    fn rays_meet_triangles_from_either_side() {
        let triangle = [
            Vec3::new(5.0, -1.0, -1.0),
            Vec3::new(5.0, 1.0, -1.0),
            Vec3::new(5.0, 0.0, 1.0),
        ];
        assert_eq!(ray_triangle(Vec3::ZERO, Vec3::X, &triangle), Some(5.0));
        let back = [triangle[0], triangle[2], triangle[1]];
        assert_eq!(ray_triangle(Vec3::ZERO, Vec3::X, &back), Some(5.0));
        assert_eq!(ray_triangle(Vec3::ZERO, -Vec3::X, &triangle), None);
        assert_eq!(
            ray_triangle(Vec3::new(0.0, 3.0, 0.0), Vec3::X, &triangle),
            None
        );
    }

    /// The nearest placed model on the ray is picked on its triangles, turned and scaled
    /// as the map places it; one behind it or beside the ray is not.
    #[test]
    fn the_nearest_model_on_the_ray_is_picked() {
        let (entities, mut cache) = cube_map();
        let vfs = VirtualFileSystem::new();
        let map = "maps/test.bsp";
        let model = pick_model(&entities, &mut cache, &vfs, map, Vec3::ZERO, Vec3::X)
            .expect("the turned crate");
        assert_eq!(model.entity.number, 1);
        assert_eq!(model.entity.model, "models/test/crate.md3");
        // Turned 45 degrees and twice as big: its edge points at the eye, 16 * sqrt 2 off
        // its centre.
        let edge = 100.0 - 16.0 * std::f32::consts::SQRT_2;
        assert!((model.distance - edge).abs() < 0.01, "{}", model.distance);
        assert!(model.normal.dot(Vec3::X) < 0.0, "faces the eye");
        assert_eq!(model.shaders.len(), 1);
        assert_eq!(model.shaders[0].shader, "models/test/crate");
        // A cube's faces fold at every edge: 12 outline edges and 12 shaded triangles.
        assert_eq!((model.edges.len(), model.triangles.len()), (12, 12));
        assert!(pick_model(&entities, &mut cache, &vfs, map, Vec3::ZERO, Vec3::Y).is_none());
        // From beyond the first, the second crate is met first.
        let eye = Vec3::new(200.0, 0.0, 0.0);
        let model =
            pick_model(&entities, &mut cache, &vfs, map, eye, Vec3::X).expect("the plain crate");
        assert_eq!((model.entity.number, model.distance), (2, 92.0));
    }
}

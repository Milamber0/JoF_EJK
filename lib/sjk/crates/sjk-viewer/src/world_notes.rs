//! World notes: in-game annotations of surfaces and objects for whoever fixes them.
//!
//! `worldnote` selects the world surface under the crosshair, or the mover whose bounds
//! the view ray meets first, and names it in a centre print. A second `worldnote` within
//! [`CONFIRM_MS`] confirms the selection and opens a note line in the chat composer.
//! Enter appends the note with the selection to `notes.jsonl` (one JSON object per
//! line) and a readable line to `notes.txt` in the config directory, and takes a
//! JPEG screenshot of the same view named after the note. With the SJK identity on,
//! the note also goes to the hub for the SJK team (`PROTOCOL.md` in the hub, "World
//! notes"), followed by a smaller copy of the screenshot. Escape drops the note.
//!
//! While a selection waits or its note is written, its outline is drawn over the view
//! as flickering green dots along the surface's triangle edges (a mover's bounds),
//! projected each frame, at most [`HIGHLIGHT_DOTS`] of them, over a green scanline shade
//! of its triangles (thin horizontal strips with a wave running down them, at most
//! [`FILL_STRIPS`]; the HUD has no polygon, so the fill is rasterised into rectangles).
//!
//! A selection records what a fix needs: the map, the camera pose and a `setviewpos`
//! back to it, the hit point and normal, the BSP draw surface (index, shader, kind,
//! lightmap or vertex lighting, BSP material), the collision trace's surface flags, and
//! the nearest map entity (an inline model whose bounds hold the hit, or the closest
//! origin within [`ENTITY_RADIUS`]). When a remap (the map's, the server's or a local
//! one) draws another shader over the surface, the note names that shader too: what
//! the player saw is the remap's target, not the map's own shader. Every remap active
//! when the note is taken goes with it ([`Remap`]), with its source.
//!
//! A model drawn from the map's entities (an MD3 such as a `misc_model_static` prop,
//! which is not in the world's triangles) is selected when the view ray meets one of its
//! triangles before the world ([`pick_model`]): the note then names the model's path, its
//! entity and placement, and every surface's shader with its remap, and the outline
//! traces the model. While the note is written, Clear model in the dialog drops the model
//! for the surface behind it, and a click on the scene round the dialog selects what is
//! under the pointer instead ([`Notes::pick_at`]).

use crate::camera_uniform::CameraUniform;
use glam::Vec3;
use models::{ModelCache, pick_model};
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_vfs::VirtualFileSystem;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[path = "world_notes_models.rs"]
mod models;

/// How long a selection waits for its confirming press.
pub(crate) const CONFIRM_MS: u128 = 20_000;
/// Farthest a selection reaches.
const REACH: f32 = 16_384.0;
/// Entities whose origin lies this close to the hit are named when no inline model
/// holds it.
const ENTITY_RADIUS: f32 = 160.0;
/// `CONTENTS_SOLID | CONTENTS_PLAYERCLIP`-style mask of the collision trace: anything a
/// player could stand against.
const COLLISION_MASK: u32 = 1 | 0x10000;
/// JKA's standing view height: `setviewpos` places the origin, the eye sits above it.
const VIEW_HEIGHT: f32 = 36.0;
/// Most edges an outline keeps, and most dots it draws a frame (the HUD's shapes share
/// 16,384 vertices, six a dot).
const HIGHLIGHT_EDGES: usize = 512;
pub(crate) const HIGHLIGHT_DOTS: usize = 600;
/// Most triangles a shade keeps, and most strips it draws a frame.
const FILL_TRIANGLES: usize = 512;
pub(crate) const FILL_STRIPS: usize = 800;
/// Clip-space `w` a shaded triangle is cut at: what lies nearer the eye is not drawn.
const NEAR_W: f32 = 1.0;

/// What `worldnote` selected.
#[derive(Clone, Debug)]
pub(crate) struct Selection {
    map: String,
    pose: String,
    eye: Vec3,
    forward: Vec3,
    hit: Vec3,
    normal: Vec3,
    distance: f32,
    surface: Option<Surface>,
    entity: Option<Entity>,
    collision: Option<Collision>,
    /// The outline drawn while selected (`Notes::draw_highlight`).
    edges: Vec<[Vec3; 2]>,
    /// The triangles shaded while selected.
    triangles: Vec<[Vec3; 3]>,
    selected: Instant,
    /// The model the view ray met before the world, if any: the note is about it.
    model: Option<Model>,
    /// The shader remaps active when the selection was made.
    remaps: Vec<Remap>,
}

/// A shader remap active when a note was taken.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Remap {
    /// `map` (a worldspawn key), `server` or `local` (the console's `remapShader`).
    pub(crate) source: &'static str,
    /// The shader the map or model names.
    pub(crate) from: String,
    /// The shader drawn instead.
    pub(crate) to: String,
}

/// The shader a remap in `remaps` draws instead of `name`, when one draws another.
fn remap_of(remaps: &[Remap], name: &str) -> Option<String> {
    let name = sjk_client::shader_name(name)?;
    remaps
        .iter()
        .find(|remap| remap.from == name && remap.to != name)
        .map(|remap| remap.to.clone())
}

/// A model selected for a note: a map entity drawing an MD3, met on its triangles.
#[derive(Clone, Debug)]
struct Model {
    /// The entity, its `model` key the model's path.
    entity: Entity,
    /// Pitch, yaw and roll, and the scale on each axis.
    angles: [f32; 3],
    scale: [f32; 3],
    /// The entity's skin key, or empty.
    skin: String,
    shaders: Vec<ModelShader>,
    hit: Vec3,
    normal: Vec3,
    distance: f32,
    edges: Vec<[Vec3; 2]>,
    triangles: Vec<[Vec3; 3]>,
}

/// One surface of a selected model and the shader it draws.
#[derive(Clone, Debug)]
struct ModelShader {
    surface: String,
    shader: String,
    /// The shader a remap draws instead.
    remapped: Option<String>,
}

#[derive(Clone, Debug)]
struct Surface {
    index: usize,
    shader: String,
    /// The shader a remap draws instead of [`Surface::shader`].
    remapped: Option<String>,
    kind: &'static str,
    lightmap: i32,
    vertices: usize,
    material: u32,
}

#[derive(Clone, Debug)]
struct Entity {
    number: usize,
    classname: String,
    targetname: String,
    model: String,
    origin: String,
    /// The inline model's shaders, for a mover the ray met.
    shaders: Vec<String>,
}

#[derive(Clone, Debug)]
struct Collision {
    distance: f32,
    surface_flags: u32,
    content_flags: u32,
}

impl Selection {
    /// One line naming the selection, for the centre print and the note's subject.
    pub(crate) fn summary(&self) -> String {
        if let Some(model) = &self.model {
            let first = model.shaders.first();
            let more = model.shaders.len().saturating_sub(1);
            return format!(
                "{} {} ({}{}{}), {:.0} units",
                model.entity.classname,
                model.entity.model,
                first.map_or("no shader", |shader| shader.shader.as_str()),
                first
                    .and_then(|shader| shader.remapped.as_ref())
                    .map_or_else(String::new, |target| format!(" remapped to {target}")),
                match more {
                    0 => String::new(),
                    1 => ", 1 more shader".to_owned(),
                    more => format!(", {more} more shaders"),
                },
                model.distance
            );
        }
        let what = match (&self.surface, &self.entity) {
            (_, Some(entity)) if !entity.shaders.is_empty() => format!(
                "{} {} ({})",
                entity.classname,
                entity.model,
                entity.shaders.first().map_or("", String::as_str)
            ),
            (Some(surface), _) => format!(
                "{}{} (surface {}, {})",
                surface.shader,
                surface
                    .remapped
                    .as_ref()
                    .map_or_else(String::new, |target| format!(" remapped to {target}")),
                surface.index,
                lighting(surface.lightmap)
            ),
            (None, Some(entity)) => {
                let mut what = entity.classname.clone();
                if !entity.targetname.is_empty() {
                    what.push_str(&format!(" \"{}\"", entity.targetname));
                }
                if !entity.origin.is_empty() {
                    what.push_str(&format!(" at {}", entity.origin));
                }
                what
            }
            (None, None) => "nothing".to_owned(),
        };
        format!("{what}, {:.0} units", self.distance)
    }

    /// The selected model's path, for the dialog's Clear model.
    pub(crate) fn model_path(&self) -> Option<String> {
        self.model.as_ref().map(|model| model.entity.model.clone())
    }

    /// The aimed point, its normal and its distance: the model's when there is one.
    fn target(&self) -> (Vec3, Vec3, f32) {
        self.model
            .as_ref()
            .map_or((self.hit, self.normal, self.distance), |model| {
                (model.hit, model.normal, model.distance)
            })
    }

    /// The outline and the shade drawn while selected: the model's when there is one.
    fn outline(&self) -> (&[[Vec3; 2]], &[[Vec3; 3]]) {
        self.model.as_ref().map_or(
            (self.edges.as_slice(), self.triangles.as_slice()),
            |model| (model.edges.as_slice(), model.triangles.as_slice()),
        )
    }

    /// The shaders the note is about, as remaps name them: the surface's, or the
    /// model's surfaces'.
    fn noted_shaders(&self) -> Vec<String> {
        let names: Vec<&str> = match (&self.model, &self.surface, &self.entity) {
            (Some(model), _, _) => model.shaders.iter().map(|s| s.shader.as_str()).collect(),
            (None, Some(surface), _) => vec![surface.shader.as_str()],
            (None, None, Some(entity)) => entity.shaders.iter().map(String::as_str).collect(),
            (None, None, None) => Vec::new(),
        };
        names
            .into_iter()
            .filter_map(sjk_client::shader_name)
            .collect()
    }

    /// The note `text` about this selection as the hub takes it; the build and the
    /// server are the caller's to fill.
    fn world_note(&self, text: String) -> sjk_identity::WorldNote {
        use sjk_identity::report::{
            NOTE_MODEL_PATH_MAX, NOTE_MODEL_SHADERS_MAX, NOTE_REMAPS_MAX, NOTE_SHADER_MAX, field,
        };
        let (shader, remapped) = match (&self.model, &self.surface, &self.entity) {
            (Some(model), _, _) => model.shaders.first().map_or(("", None), |shader| {
                (shader.shader.as_str(), shader.remapped.as_deref())
            }),
            (None, Some(surface), _) => (surface.shader.as_str(), surface.remapped.as_deref()),
            (None, None, Some(entity)) => (entity.shaders.first().map_or("", String::as_str), None),
            (None, None, None) => ("", None),
        };
        let vector = |v: Vec3| [v.x, v.y, v.z];
        let (hit, normal, distance) = self.target();
        // The remaps of the noted shaders first, then the others, as many as the hub keeps.
        let noted = self.noted_shaders();
        let (mut remaps, others): (Vec<&Remap>, Vec<&Remap>) = self
            .remaps
            .iter()
            .partition(|remap| noted.contains(&remap.from));
        remaps.extend(others);
        let remaps = remaps
            .into_iter()
            .map(|remap| sjk_identity::NoteRemap {
                source: remap.source.to_owned(),
                from: field(&remap.from, NOTE_SHADER_MAX),
                to: field(&remap.to, NOTE_SHADER_MAX),
            })
            .filter(|remap| !remap.from.is_empty() && !remap.to.is_empty())
            .take(NOTE_REMAPS_MAX)
            .collect();
        let model = self.model.as_ref().map(|model| sjk_identity::NoteModel {
            path: field(&model.entity.model, NOTE_MODEL_PATH_MAX),
            classname: field(&model.entity.classname, 64),
            entity: u32::try_from(model.entity.number).ok(),
            origin: parse_vector(Some(&model.entity.origin)).map(vector),
            skin: field(&model.skin, NOTE_MODEL_PATH_MAX),
            shaders: model
                .shaders
                .iter()
                .take(NOTE_MODEL_SHADERS_MAX)
                .map(|shader| sjk_identity::NoteModelShader {
                    surface: field(&shader.surface, NOTE_SHADER_MAX),
                    shader: field(&shader.shader, NOTE_SHADER_MAX),
                    remapped: shader
                        .remapped
                        .as_deref()
                        .map_or_else(String::new, |name| field(name, NOTE_SHADER_MAX)),
                })
                .collect(),
        });
        let surface = self.surface.as_ref().filter(|_| self.model.is_none());
        sjk_identity::WorldNote {
            text,
            map: field(&self.map, 64),
            view: Some([
                self.eye.x.round(),
                self.eye.y.round(),
                (self.eye.z - VIEW_HEIGHT).round(),
                self.yaw().round(),
            ]),
            hit: Some(vector(hit)),
            normal: Some(vector(normal)),
            shader: field(shader, NOTE_SHADER_MAX),
            remapped: remapped.map_or_else(String::new, |name| field(name, NOTE_SHADER_MAX)),
            surface: surface.and_then(|s| u32::try_from(s.index).ok()),
            lighting: surface.map_or_else(String::new, |s| lighting(s.lightmap).to_owned()),
            distance: Some(distance.round()),
            entity: self
                .model
                .as_ref()
                .map(|model| &model.entity)
                .or(self.entity.as_ref())
                .map_or_else(String::new, |e| field(&e.classname, 64)),
            remaps,
            model,
            ..sjk_identity::WorldNote::default()
        }
    }

    fn yaw(&self) -> f32 {
        self.forward
            .y
            .atan2(self.forward.x)
            .to_degrees()
            .rem_euclid(360.0)
    }

    fn fresh(&self, now: Instant) -> bool {
        now.duration_since(self.selected).as_millis() <= CONFIRM_MS
    }

    /// `setviewpos` that puts the eye back where the selection was made.
    fn setviewpos(&self) -> String {
        let yaw = self.yaw();
        format!(
            "setviewpos {:.0} {:.0} {:.0} {:.0}",
            self.eye.x,
            self.eye.y,
            self.eye.z - VIEW_HEIGHT,
            yaw
        )
    }

    /// The note's line of `notes.jsonl`. With a model, `surface` is left out (the note is
    /// not about the world behind it) and `entity` and `model` name the model's entity.
    fn json(&self, note: &str, time: &str, screenshot: &str) -> serde_json::Value {
        let vector = |v: Vec3| serde_json::json!([round(v.x), round(v.y), round(v.z)]);
        let entity_json = |e: &Entity| {
            serde_json::json!({
                "number": e.number,
                "classname": e.classname,
                "targetname": e.targetname,
                "model": e.model,
                "origin": e.origin,
                "shaders": e.shaders,
            })
        };
        let (hit, normal, distance) = self.target();
        let surface = self.surface.as_ref().filter(|_| self.model.is_none());
        serde_json::json!({
            "time": time,
            "map": self.map,
            "note": note,
            "selection": self.summary(),
            "setviewpos": self.setviewpos(),
            "pose": self.pose,
            "hit": vector(hit),
            "normal": vector(normal),
            "distance": round(distance),
            "surface": surface.map(|s| serde_json::json!({
                "index": s.index,
                "shader": s.shader,
                "remapped_to": s.remapped,
                "kind": s.kind,
                "lightmap": s.lightmap,
                "lighting": lighting(s.lightmap),
                "vertices": s.vertices,
                "bsp_material": s.material,
            })),
            "entity": self
                .model
                .as_ref()
                .map(|model| &model.entity)
                .or(self.entity.as_ref())
                .map(entity_json),
            "model": self.model.as_ref().map(|model| serde_json::json!({
                "path": model.entity.model,
                "classname": model.entity.classname,
                "entity": model.entity.number,
                "targetname": model.entity.targetname,
                "origin": model.entity.origin,
                "angles": model.angles,
                "scale": model.scale,
                "skin": model.skin,
                "shaders": model.shaders.iter().map(|shader| serde_json::json!({
                    "surface": shader.surface,
                    "shader": shader.shader,
                    "remapped_to": shader.remapped,
                })).collect::<Vec<_>>(),
            })),
            "remaps": self.remaps.iter().map(|remap| serde_json::json!({
                "source": remap.source,
                "from": remap.from,
                "to": remap.to,
            })).collect::<Vec<_>>(),
            "collision": self.collision.as_ref().map(|c| serde_json::json!({
                "distance": round(c.distance),
                "surface_flags": format!("{:#x}", c.surface_flags),
                "content_flags": format!("{:#x}", c.content_flags),
            })),
            "screenshot": screenshot,
        })
    }
}

fn lighting(lightmap: i32) -> &'static str {
    match lightmap {
        0.. => "lightmapped",
        -3 => "vertex-lit",
        -2 => "white image",
        _ => "unlit",
    }
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 10.0).round() / 10.0
}

/// The selection under the ray from the eye of `camera` along `direction` (its view, or
/// through a point of the screen): the nearest draw surface of the world, unless a
/// mover's bounds come first. Models are [`pick_model`]'s.
pub(crate) fn pick(
    bsp: &Bsp,
    surfaces: &crate::decal_marks::DecalSurfaces,
    scratch: &mut TraceScratch,
    camera: &CameraUniform,
    map: &str,
    direction: Vec3,
) -> Option<Selection> {
    let eye = Vec3::from_array(camera.camera_position);
    let view = Vec3::from_array(camera.view_forward).normalize_or_zero();
    // `setviewpos` keeps the view's yaw; the ray may go elsewhere.
    let forward = if view == Vec3::ZERO { direction } else { view };
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }
    let world = surfaces.ray_hit(eye, direction, REACH);
    let entities = sjk_entity::parse_entity_lump(bsp.entities()).unwrap_or_default();
    // Movers are not in the world's triangles: the first inline model box the ray meets.
    let mut mover = None;
    for (number, entity) in entities.iter().enumerate() {
        let Some(index) = entity
            .get("model")
            .and_then(|model| model.strip_prefix('*'))
            .and_then(|index| index.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(model) = bsp.render().inline_model(index) else {
            continue;
        };
        let origin = parse_vector(entity.get("origin")).unwrap_or(Vec3::ZERO);
        let low = Vec3::from_array(model.minimums) + origin;
        let high = Vec3::from_array(model.maximums) + origin;
        let Some(distance) = ray_box(eye, direction, low, high) else {
            continue;
        };
        if world.as_ref().is_some_and(|hit| hit.distance < distance)
            || mover
                .as_ref()
                .is_some_and(|(_, _, nearest)| *nearest <= distance)
        {
            continue;
        }
        mover = Some((number, index, distance));
    }
    let trace = bsp.trace_box_with(
        scratch,
        eye.to_array(),
        (eye + direction * REACH).to_array(),
        Aabb::new([0.0; 3], [0.0; 3]).expect("point bounds are valid"),
        COLLISION_MASK,
    );
    let collision = (trace.fraction < 1.0).then_some(Collision {
        distance: trace.fraction * REACH,
        surface_flags: trace.surface_flags,
        content_flags: trace.content_flags,
    });
    let render = bsp.render();
    let shader_name = |index: usize| {
        bsp.shaders().get(index).map_or_else(
            || format!("shader {index}"),
            |s| s.name_lossy().into_owned(),
        )
    };
    let entity_of = |number: usize, shaders: Vec<String>| {
        let entity = &entities[number];
        let text = |key: &str| entity.get(key).unwrap_or("").to_owned();
        Entity {
            number,
            classname: text("classname"),
            targetname: text("targetname"),
            model: text("model"),
            origin: text("origin"),
            shaders,
        }
    };
    let selection =
        |hit: Vec3,
         normal: Vec3,
         distance: f32,
         surface,
         entity,
         (edges, triangles): (Vec<[Vec3; 2]>, Vec<[Vec3; 3]>)| Selection {
            map: map.to_owned(),
            pose: camera.viewpos(),
            eye,
            forward,
            hit,
            normal,
            distance,
            surface,
            entity,
            collision: collision.clone(),
            edges,
            triangles,
            selected: Instant::now(),
            model: None,
            remaps: Vec::new(),
        };
    if let Some((number, index, distance)) = mover {
        let shaders = render
            .inline_model(index)
            .map(|model| {
                let mut names: Vec<String> = model
                    .surfaces
                    .clone()
                    .filter_map(|surface| render.surfaces().get(surface))
                    .map(|surface| shader_name(surface.shader))
                    .collect();
                names.sort();
                names.dedup();
                names
            })
            .unwrap_or_default();
        let hit = eye + direction * distance;
        let outline = render
            .inline_model(index)
            .map_or_else(Default::default, |model| {
                let origin = parse_vector(entities[number].get("origin")).unwrap_or(Vec3::ZERO);
                let (low, high) = (
                    Vec3::from_array(model.minimums) + origin,
                    Vec3::from_array(model.maximums) + origin,
                );
                (box_edges(low, high), box_triangles(low, high))
            });
        return Some(selection(
            hit,
            -direction,
            distance,
            None,
            Some(entity_of(number, shaders)),
            outline,
        ));
    }
    let hit = world?;
    let point = eye + direction * hit.distance;
    let surface = render.surfaces().get(hit.surface).map(|surface| {
        let shader = bsp.shaders().get(surface.shader);
        Surface {
            index: hit.surface,
            shader: shader_name(surface.shader),
            remapped: None,
            kind: match surface.kind {
                sjk_bsp::SurfaceKind::Planar => "planar",
                sjk_bsp::SurfaceKind::Patch => "patch",
                sjk_bsp::SurfaceKind::TriangleSoup => "triangle soup",
                sjk_bsp::SurfaceKind::Flare => "flare",
            },
            lightmap: surface.lightmaps[0],
            vertices: surface.vertices.len(),
            material: shader.map_or(0, |s| s.surface_flags & 0x1f),
        }
    });
    let entity = nearest_entity(&entities, point).map(|number| entity_of(number, Vec::new()));
    let outline = (
        surfaces.surface_edges(hit.surface, HIGHLIGHT_EDGES),
        surfaces.surface_triangles(hit.surface, FILL_TRIANGLES),
    );
    Some(selection(
        point,
        hit.normal,
        hit.distance,
        surface,
        entity,
        outline,
    ))
}

/// The entity, other than the world, whose origin is nearest `point` within
/// [`ENTITY_RADIUS`].
fn nearest_entity(entities: &[sjk_entity::Entity], point: Vec3) -> Option<usize> {
    entities
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(number, entity)| {
            let origin = parse_vector(entity.get("origin"))?;
            let distance = origin.distance(point);
            (distance <= ENTITY_RADIUS).then_some((number, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(number, _)| number)
}

fn parse_vector(text: Option<&str>) -> Option<Vec3> {
    let mut parts = text?
        .split_whitespace()
        .map(|part| part.parse::<f32>().ok());
    Some(Vec3::new(parts.next()??, parts.next()??, parts.next()??))
}

/// The twelve edges of a box.
fn box_edges(low: Vec3, high: Vec3) -> Vec<[Vec3; 2]> {
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { low.x } else { high.x },
            if i & 2 == 0 { low.y } else { high.y },
            if i & 4 == 0 { low.z } else { high.z },
        )
    };
    (0..8)
        .flat_map(|i| [1, 2, 4].map(|bit| (i, i | bit)))
        .filter(|(from, to)| from != to)
        .map(|(from, to)| [corner(from), corner(to)])
        .collect()
}

/// The twelve triangles of a box's faces.
fn box_triangles(low: Vec3, high: Vec3) -> Vec<[Vec3; 3]> {
    let corner = |x: usize, y: usize, z: usize| {
        Vec3::new(
            if x == 0 { low.x } else { high.x },
            if y == 0 { low.y } else { high.y },
            if z == 0 { low.z } else { high.z },
        )
    };
    let mut triangles = Vec::with_capacity(12);
    for axis in 0..3 {
        for side in 0..2 {
            let at = |u: usize, v: usize| {
                let mut c = [0; 3];
                c[axis] = side;
                c[(axis + 1) % 3] = u;
                c[(axis + 2) % 3] = v;
                corner(c[0], c[1], c[2])
            };
            triangles.push([at(0, 0), at(1, 0), at(1, 1)]);
            triangles.push([at(0, 0), at(1, 1), at(0, 1)]);
        }
    }
    triangles
}

/// The part of `triangle` in front of the eye, on the screen of `view_projection`
/// (physical pixels): the triangle cut at clip-space `w` [`NEAR_W`] (Sutherland and
/// Hodgman against one plane), so a floor under the player keeps its visible part.
fn screen_polygon(
    view_projection: &glam::Mat4,
    viewport: [f32; 2],
    triangle: &[Vec3; 3],
) -> Vec<[f32; 2]> {
    let clip = triangle.map(|point| *view_projection * point.extend(1.0));
    let mut kept: Vec<glam::Vec4> = Vec::with_capacity(4);
    for index in 0..3 {
        let (a, b) = (clip[index], clip[(index + 1) % 3]);
        let (inside_a, inside_b) = (a.w >= NEAR_W, b.w >= NEAR_W);
        if inside_a {
            kept.push(a);
        }
        if inside_a != inside_b {
            let t = (NEAR_W - a.w) / (b.w - a.w);
            kept.push(a.lerp(b, t));
        }
    }
    kept.iter()
        .map(|clip| {
            [
                (clip.x / clip.w + 1.0) * 0.5 * viewport[0],
                (1.0 - clip.y / clip.w) * 0.5 * viewport[1],
            ]
        })
        .collect()
}

/// Where the convex `polygon` crosses the row at `y`: its left and right x.
fn row_span(polygon: &[[f32; 2]], y: f32) -> Option<(f32, f32)> {
    let mut span: Option<(f32, f32)> = None;
    for index in 0..polygon.len() {
        let (a, b) = (polygon[index], polygon[(index + 1) % polygon.len()]);
        if (a[1] <= y) == (b[1] <= y) {
            continue;
        }
        let x = a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        span = Some(span.map_or((x, x), |(low, high)| (low.min(x), high.max(x))));
    }
    span.filter(|(low, high)| high > low)
}

/// `point` on the screen of `view_projection` (physical pixels), if in front of the eye.
fn project(view_projection: &glam::Mat4, viewport: [f32; 2], point: Vec3) -> Option<[f32; 2]> {
    let clip = *view_projection * point.extend(1.0);
    if clip.w <= 0.5 {
        return None;
    }
    let (x, y) = (clip.x / clip.w, clip.y / clip.w);
    (x.abs() <= 1.2 && y.abs() <= 1.2)
        .then(|| [(x + 1.0) * 0.5 * viewport[0], (1.0 - y) * 0.5 * viewport[1]])
}

/// The direction from the eye of `camera` through `point` of a `viewport`-sized screen
/// (physical pixels), or `None` before the first frame.
fn screen_ray(camera: &CameraUniform, viewport: [f32; 2], point: [f32; 2]) -> Option<Vec3> {
    if viewport[0] <= 0.0 || viewport[1] <= 0.0 {
        return None;
    }
    let inverse = glam::Mat4::from_cols_array_2d(&camera.view_projection).inverse();
    let x = point[0] / viewport[0] * 2.0 - 1.0;
    let y = 1.0 - point[1] / viewport[1] * 2.0;
    // A point inside the frustum, whatever its depth convention.
    let far = inverse.project_point3(Vec3::new(x, y, 0.5));
    let eye = Vec3::from_array(camera.camera_position);
    let mut direction = (far - eye).normalize_or_zero();
    if direction.dot(Vec3::from_array(camera.view_forward)) < 0.0 {
        direction = -direction;
    }
    (direction != Vec3::ZERO && direction.is_finite()).then_some(direction)
}

/// Entry distance of the ray into the box, if it meets it ahead.
fn ray_box(origin: Vec3, direction: Vec3, low: Vec3, high: Vec3) -> Option<f32> {
    let inverse = direction.recip();
    let a = (low - origin) * inverse;
    let b = (high - origin) * inverse;
    let near = a.min(b).max_element();
    let far = a.max(b).min_element();
    (far >= near.max(0.0)).then_some(near.max(0.0))
}

/// The selection waiting for its confirming press, and the note being written.
pub(crate) struct Notes {
    selection: Option<Selection>,
    /// The confirmed selection the composer's note belongs to.
    writing: Option<Selection>,
    /// The last rendered main view.
    camera: Option<CameraUniform>,
    /// The selection's outline for this frame.
    highlight: sjk_ui::DrawList,
    /// The selection's shade for this frame, drawn under the outline.
    fill: sjk_ui::DrawList,
    started: Instant,
    /// A note went to the identity service and its outcome is not shown yet.
    sent_waiting: bool,
    /// The serial of the last outcome shown.
    sent_serial: u64,
    /// The models read for selections on this map.
    models: ModelCache,
    /// The last frame's size in physical pixels, for a click's ray.
    viewport: [f32; 2],
}

impl Default for Notes {
    fn default() -> Self {
        Self {
            selection: None,
            writing: None,
            camera: None,
            highlight: sjk_ui::DrawList::new(HIGHLIGHT_DOTS),
            fill: sjk_ui::DrawList::new(FILL_STRIPS),
            started: Instant::now(),
            sent_waiting: false,
            sent_serial: 0,
            models: ModelCache::default(),
            viewport: [0.0; 2],
        }
    }
}

/// What a selection is made from: the world, its models and the remaps drawn over them.
pub(crate) struct Scene<'a> {
    pub(crate) bsp: &'a Bsp,
    pub(crate) surfaces: &'a crate::decal_marks::DecalSurfaces,
    pub(crate) scratch: &'a mut TraceScratch,
    pub(crate) vfs: Option<&'a VirtualFileSystem>,
    pub(crate) map: &'a str,
    /// The remaps active now ([`Remap`]), in application order.
    pub(crate) remaps: Vec<Remap>,
    /// The entity markers on show (`r_showEntities`): they are picked before the world.
    pub(crate) markers: Vec<crate::entity_view::Marker>,
}

/// The selection's subject and its model's path, for the note dialog.
pub(crate) type Subject = (String, Option<String>);

/// What a `worldnote` press did.
pub(crate) enum Press {
    /// Selected (or reselected) something: show its summary.
    Selected(String),
    /// Confirmed the selection: open the composer for a note about it (its subject and
    /// its model's path).
    Confirmed(Subject),
    /// Nothing under the crosshair.
    Nothing,
}

impl Notes {
    /// Keep the final main view (`upload_scene_camera`).
    pub(crate) fn set_camera(&mut self, camera: CameraUniform) {
        self.camera = Some(camera);
    }

    /// A `worldnote` press.
    pub(crate) fn press(&mut self, scene: Scene<'_>) -> Press {
        let now = Instant::now();
        if let Some(selection) = self.selection.take().filter(|s| s.fresh(now)) {
            let subject = (selection.summary(), selection.model_path());
            self.writing = Some(selection);
            return Press::Confirmed(subject);
        }
        let Some(camera) = self.camera else {
            return Press::Nothing;
        };
        let forward = Vec3::from_array(camera.view_forward);
        match self.select(scene, &camera, forward) {
            Some(selection) => {
                let summary = selection.summary();
                self.selection = Some(selection);
                Press::Selected(summary)
            }
            None => Press::Nothing,
        }
    }

    /// The selection along `direction` from the eye of `camera`: the world's surface or
    /// mover, unless a model comes first, with the remaps drawn over their shaders.
    fn select(
        &mut self,
        scene: Scene<'_>,
        camera: &CameraUniform,
        direction: Vec3,
    ) -> Option<Selection> {
        if let Some(selection) = self.select_marker(&scene, camera, direction) {
            return Some(selection);
        }
        let world = pick(
            scene.bsp,
            scene.surfaces,
            scene.scratch,
            camera,
            scene.map,
            direction,
        );
        let eye = Vec3::from_array(camera.camera_position);
        let direction = direction.normalize_or_zero();
        let model = scene.vfs.and_then(|vfs| {
            let entities = sjk_entity::parse_entity_lump(scene.bsp.entities()).ok()?;
            pick_model(&entities, &mut self.models, vfs, scene.map, eye, direction)
        });
        let model = model.filter(|model| {
            world
                .as_ref()
                .is_none_or(|world| model.distance <= world.distance)
        });
        let mut selection = match (world, model) {
            (Some(mut world), model) => {
                world.model = model;
                world
            }
            (None, Some(model)) => Selection {
                map: scene.map.to_owned(),
                pose: camera.viewpos(),
                eye,
                forward: Vec3::from_array(camera.view_forward),
                hit: model.hit,
                normal: model.normal,
                distance: model.distance,
                surface: None,
                entity: None,
                collision: None,
                edges: Vec::new(),
                triangles: Vec::new(),
                selected: Instant::now(),
                model: Some(model),
                remaps: Vec::new(),
            },
            (None, None) => return None,
        };
        if let Some(surface) = &mut selection.surface {
            surface.remapped = remap_of(&scene.remaps, &surface.shader);
        }
        if let Some(model) = &mut selection.model {
            for shader in &mut model.shaders {
                shader.remapped = remap_of(&scene.remaps, &shader.shader);
            }
        }
        selection.remaps = scene.remaps;
        Some(selection)
    }

    /// The entity marker the view ray passes closest to, if the markers are on show:
    /// they draw through walls, so the one aimed at wins over whatever is behind it.
    fn select_marker(
        &self,
        scene: &Scene<'_>,
        camera: &CameraUniform,
        direction: Vec3,
    ) -> Option<Selection> {
        let eye = Vec3::from_array(camera.camera_position);
        let direction = direction.normalize_or_zero();
        let (marker, distance) = crate::entity_view::Marker::aim(&scene.markers, eye, direction)?;
        let entities = sjk_entity::parse_entity_lump(scene.bsp.entities()).ok()?;
        let source = entities.get(marker.number)?;
        let text = |key: &str| source.get(key).unwrap_or("").to_owned();
        let (low, high) = marker.bounds();
        Some(Selection {
            map: scene.map.to_owned(),
            pose: camera.viewpos(),
            eye,
            forward: Vec3::from_array(camera.view_forward),
            hit: marker.origin,
            normal: -direction,
            distance,
            surface: None,
            entity: Some(Entity {
                number: marker.number,
                classname: text("classname"),
                targetname: text("targetname"),
                model: text("model"),
                origin: text("origin"),
                shaders: Vec::new(),
            }),
            collision: None,
            edges: box_edges(low, high),
            triangles: box_triangles(low, high),
            selected: Instant::now(),
            model: None,
            remaps: scene.remaps.clone(),
        })
    }

    /// A click at `point` (physical pixels) on the scene round the note dialog: what is
    /// under the pointer becomes the note's selection. `None` when nothing is there, and
    /// the selection stays.
    pub(crate) fn pick_at(&mut self, scene: Scene<'_>, point: [f32; 2]) -> Option<Subject> {
        let camera = self.camera?;
        self.writing.as_ref()?;
        let direction = screen_ray(&camera, self.viewport, point)?;
        let selection = self.select(scene, &camera, direction)?;
        let subject = (selection.summary(), selection.model_path());
        self.writing = Some(selection);
        Some(subject)
    }

    /// Clear model in the note dialog: the note is about what is behind the model.
    pub(crate) fn clear_model(&mut self) -> Option<Subject> {
        let selection = self.writing.as_mut()?;
        selection.model.take()?;
        Some((selection.summary(), None))
    }

    /// The waiting selection's summary, for the world shots.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> Option<String> {
        self.selection.as_ref().map(Selection::summary)
    }

    /// Escape before the confirming press: forget the waiting selection. True when there
    /// was one to forget.
    pub(crate) fn cancel_selection(&mut self) -> bool {
        self.selection.take().is_some()
    }

    /// The composer closed without saving (Escape): forget the confirmed selection.
    pub(crate) fn composer_closed(&mut self) {
        self.writing = None;
    }

    /// Rebuild the outline of the waiting or confirmed selection for this frame.
    pub(crate) fn draw_highlight(&mut self, viewport: [f32; 2]) {
        self.viewport = viewport;
        self.highlight.clear();
        self.fill.clear();
        let now = Instant::now();
        if self.selection.as_ref().is_some_and(|s| !s.fresh(now)) {
            self.selection = None;
        }
        let (Some(selection), Some(camera)) = (
            self.writing.as_ref().or(self.selection.as_ref()),
            self.camera,
        ) else {
            return;
        };
        let view_projection = glam::Mat4::from_cols_array_2d(&camera.view_projection);
        let (edges, triangles) = selection.outline();
        // Space the dots by the outline's length on screen, so a big surface keeps
        // within the budget and a small one still reads as a line.
        let lengths: Vec<f32> = edges
            .iter()
            .map(|[a, b]| {
                match (
                    project(&view_projection, viewport, *a),
                    project(&view_projection, viewport, *b),
                ) {
                    (Some(a), Some(b)) => ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt(),
                    _ => 120.0,
                }
            })
            .collect();
        let total: f32 = lengths.iter().sum();
        let spacing = (total / HIGHLIGHT_DOTS as f32).max(7.0);
        let time = now.duration_since(self.started).as_secs_f32();
        Self::draw_fill(&mut self.fill, triangles, &view_projection, viewport, time);
        let size = (viewport[1] / 540.0).clamp(2.0, 4.0);
        let mut index = 0u32;
        for ([a, b], length) in edges.iter().zip(&lengths) {
            let steps = ((length / spacing).ceil() as usize).clamp(1, 64);
            for step in 0..=steps {
                let point = a.lerp(*b, step as f32 / steps as f32);
                index = index.wrapping_add(1);
                let Some([x, y]) = project(&view_projection, viewport, point) else {
                    continue;
                };
                // Each dot flickers on its own beat, brighter as a wave runs along.
                let seed = (index as f32 * 12.9898 + (time * 9.0).floor() * 78.233).sin();
                let flicker = (seed * 43_758.547).fract().abs();
                let wave = ((index as f32 * 0.21 - time * 6.0).sin() * 0.5 + 0.5).powi(4);
                let alpha = (0.35 + 0.4 * flicker + 0.25 * wave).min(1.0);
                if !self.highlight.push(sjk_ui::DrawCommand::SolidRect {
                    rect: sjk_ui::Rect::new(x - size * 0.5, y - size * 0.5, size, size),
                    color: sjk_ui::Color::new(0.25, 1.0, 0.45, alpha),
                }) {
                    return;
                }
            }
        }
    }

    /// Shade the selection's triangles in horizontal strips, spaced so the whole of it
    /// keeps within [`FILL_STRIPS`].
    fn draw_fill(
        fill: &mut sjk_ui::DrawList,
        triangles: &[[Vec3; 3]],
        view_projection: &glam::Mat4,
        viewport: [f32; 2],
        time: f32,
    ) {
        let polygons: Vec<Vec<[f32; 2]>> = triangles
            .iter()
            .map(|triangle| screen_polygon(view_projection, viewport, triangle))
            .filter(|polygon| polygon.len() >= 3)
            .collect();
        let rows = |polygon: &Vec<[f32; 2]>| {
            let (low, high) = polygon.iter().fold((f32::MAX, f32::MIN), |(low, high), p| {
                (low.min(p[1]), high.max(p[1]))
            });
            (low.max(0.0), high.min(viewport[1]))
        };
        let covered: f32 = polygons
            .iter()
            .map(|polygon| {
                let (low, high) = rows(polygon);
                (high - low).max(0.0)
            })
            .sum();
        let step = (covered / FILL_STRIPS as f32).max((viewport[1] / 360.0).max(3.0));
        let thickness = (step * 0.55).max(1.0);
        for polygon in &polygons {
            let (low, high) = rows(polygon);
            // Rows on one grid for the whole selection, so the strips line up.
            let mut y = (low / step).floor() * step;
            while y < high {
                if let Some((left, right)) = row_span(polygon, y + step * 0.5) {
                    let (left, right) = (left.max(0.0), right.min(viewport[0]));
                    if right > left {
                        let wave = ((y * 0.035 - time * 5.0).sin() * 0.5 + 0.5).powi(3);
                        if !fill.push(sjk_ui::DrawCommand::SolidRect {
                            rect: sjk_ui::Rect::new(left, y, right - left, thickness),
                            color: sjk_ui::Color::new(0.2, 1.0, 0.45, 0.13 + 0.17 * wave),
                        }) {
                            return;
                        }
                    }
                }
                y += step;
            }
        }
    }

    /// The shade to draw this frame, under [`Self::highlight`], if any.
    pub(crate) fn fill(&self) -> Option<&sjk_ui::DrawList> {
        (!self.fill.is_empty()).then_some(&self.fill)
    }

    /// The outline to draw this frame, if any.
    pub(crate) fn highlight(&self) -> Option<&sjk_ui::DrawList> {
        (!self.highlight.is_empty()).then_some(&self.highlight)
    }

    /// Save `note` about the confirmed selection into `directory`, returning the
    /// screenshot name to take (without extension; it lands in `screenshots`), a line
    /// for the console and the note for the hub (`None` when the hub would refuse its
    /// text).
    pub(crate) fn save(
        &mut self,
        note: &str,
        directory: &Path,
        screenshots: &Path,
    ) -> Option<(String, String, Option<sjk_identity::WorldNote>)> {
        let selection = self.writing.take()?;
        let note = note.trim();
        if note.is_empty() {
            return None;
        }
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let time = utc_text(seconds);
        let shot = format!("note_{seconds}");
        let screenshot = screenshots
            .join(format!("{shot}.jpg"))
            .display()
            .to_string();
        let json = selection.json(note, &time, &screenshot);
        let line = format!(
            "{time} | {} | {} | {} | {note}",
            selection.map,
            selection.setviewpos(),
            selection.summary()
        );
        let written = append(&directory.join("notes.jsonl"), &json.to_string())
            .and_then(|()| append(&directory.join("notes.txt"), &line));
        let hub = sjk_identity::report::note_text(note)
            .ok()
            .map(|text| selection.world_note(text));
        Some((
            shot,
            match written {
                Ok(()) => format!("note kept: {line}"),
                Err(error) => format!("^1note not kept in {}: {error}", directory.display()),
            },
            hub,
        ))
    }

    /// Once a frame: show the outcome of a note sent to the hub.
    fn poll_sent(&mut self) -> Option<String> {
        if !self.sent_waiting {
            return None;
        }
        let outcome = crate::player_identity::note_outcome()
            .filter(|outcome| outcome.serial != self.sent_serial)?;
        self.sent_serial = outcome.serial;
        self.sent_waiting = false;
        Some(if outcome.sent {
            format!(
                "Note sent to the SJK team ({}). Thank you!",
                outcome.message
            )
        } else {
            format!("Note kept on this PC, not sent: {}", outcome.message)
        })
    }
}

impl crate::GpuState {
    /// `worldnote`: select the world under the crosshair, or confirm the selection and
    /// open the note composer.
    pub(crate) fn world_note_press(&mut self) {
        let map = self.note_map();
        let remaps = self.note_remaps();
        let press = self.world_notes.press(Scene {
            bsp: &self.bsp,
            surfaces: &self.decal_surfaces,
            scratch: &mut self.trace_scratch,
            vfs: self.vfs.as_deref(),
            map: &map,
            remaps,
            markers: self.entity_view.pickable(),
        });
        let centre = |text: String| (sjk_client::ServerEventKind::CenterPrint, text);
        let (kind, text) = match press {
            Press::Selected(summary) => {
                crate::log::progress(format_args!("note selection: {summary}"));
                centre(format!(
                    "{summary}
Press World note again to write a note, Escape to drop it"
                ))
            }
            Press::Confirmed((subject, model)) => {
                self.gameplay_input.release_keys();
                self.text_dialog
                    .open(crate::text_dialog::Kind::Note { subject, model });
                self.sync_cursor_policy();
                return;
            }
            Press::Nothing => centre("Nothing under the crosshair to note".to_owned()),
        };
        self.chat.receive(kind, text, None, Instant::now());
    }

    /// A click on the scene round the note dialog at `point` (physical pixels): note
    /// what is under the pointer instead.
    pub(crate) fn world_note_pick_at(&mut self, point: [f32; 2]) {
        let map = self.note_map();
        let remaps = self.note_remaps();
        let picked = self.world_notes.pick_at(
            Scene {
                bsp: &self.bsp,
                surfaces: &self.decal_surfaces,
                scratch: &mut self.trace_scratch,
                vfs: self.vfs.as_deref(),
                map: &map,
                remaps,
                markers: self.entity_view.pickable(),
            },
            point,
        );
        if let Some((subject, model)) = picked {
            crate::log::progress(format_args!("note selection: {subject}"));
            self.text_dialog.set_note(subject, model);
        }
    }

    /// Clear model in the note dialog: the note is about what is behind the model.
    pub(crate) fn world_note_clear_model(&mut self) {
        if let Some((subject, model)) = self.world_notes.clear_model() {
            crate::log::progress(format_args!("note selection: {subject}"));
            self.text_dialog.set_note(subject, model);
        }
    }

    /// The map a note is about: the one explored, or the one loaded.
    fn note_map(&self) -> String {
        if self.resident.exploring() {
            self.resident.map.clone()
        } else {
            self.world_load_map.clone()
        }
    }

    /// The shader remaps drawn now, as `listRemaps` lists them, without those a later
    /// remap of the same shader hides or that restore a shader to itself.
    fn note_remaps(&mut self) -> Vec<Remap> {
        let mode = self.remap_mode();
        let server = self
            .live_session
            .as_ref()
            .map(|session| session.shader_remaps())
            .or_else(|| self.demo_session.as_ref().map(|demo| demo.shader_remaps()))
            .and_then(|remaps| remaps.table(mode));
        self.world_materials
            .remap_entries(server)
            .into_iter()
            .filter(|entry| entry.active && entry.from != entry.to)
            .map(|entry| Remap {
                source: entry.source,
                from: entry.from.to_owned(),
                to: entry.to.to_owned(),
            })
            .collect()
    }

    /// The note composer's Enter: keep the note with its selection and take the
    /// screenshot of the same view.
    pub(crate) fn save_world_note(&mut self, note: &str) {
        let Some(directory) = self
            .console
            .as_ref()
            .map(|console| console.config_directory().to_path_buf())
        else {
            return;
        };
        let Some((shot, line, hub)) =
            self.world_notes
                .save(note, &directory, self.screenshots.directory())
        else {
            return;
        };
        let tag = hub.and_then(|mut hub| {
            hub.build = crate::build_info::VERSION.to_owned();
            hub.server = self
                .live_session
                .as_ref()
                .filter(|session| !session.is_local())
                .map(|session| session.server().to_string())
                .unwrap_or_default();
            crate::player_identity::note(hub)
        });
        self.world_notes.sent_waiting |= tag.is_some();
        self.screenshots
            .request(crate::screenshot::Request::jpeg_named(&shot).for_note(tag));
        crate::log::progress(format_args!("{line}"));
        if let Some(console) = &mut self.console {
            console.push_log(line);
        }
    }
}

impl crate::GpuState {
    /// Once a frame: show the outcome of a note sent to the hub.
    pub(crate) fn poll_world_note(&mut self) {
        if let Some(message) = self.world_notes.poll_sent() {
            crate::log::progress(format_args!("{message}"));
            if let Some(console) = &mut self.console {
                console.push_log(message.clone());
            }
            self.chat.receive(
                sjk_client::ServerEventKind::CenterPrint,
                message,
                None,
                Instant::now(),
            );
        }
    }
}

fn append(path: &PathBuf, line: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// `dd/mm/yyyy hh:mm:ss UTC` of Unix `seconds`.
fn utc_text(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{day:02}/{month:02}/{year} {:02}:{:02}:{:02} UTC",
        rest / 3_600,
        rest / 60 % 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates_are_european() {
        assert_eq!(utc_text(0), "01/01/1970 00:00:00 UTC");
        // 07/10/2026 01:23:45 UTC.
        assert_eq!(utc_text(1_791_336_225), "07/10/2026 01:23:45 UTC");
    }

    #[test]
    fn boxes_have_twelve_edges_and_points_project_in_front_only() {
        let edges = box_edges(Vec3::ZERO, Vec3::ONE);
        assert_eq!(edges.len(), 12);
        assert!(edges.iter().all(|[a, b]| (*a - *b).length() == 1.0));
        let view = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::X, Vec3::Z);
        let projection =
            glam::camera::rh::proj::directx::perspective(1.2, 16.0 / 9.0, 2.0, 8_192.0);
        let view_projection = projection * view;
        let centre = project(
            &view_projection,
            [1920.0, 1080.0],
            Vec3::new(100.0, 0.0, 0.0),
        )
        .expect("in front");
        assert!((centre[0] - 960.0).abs() < 0.5 && (centre[1] - 540.0).abs() < 0.5);
        assert!(
            project(
                &view_projection,
                [1920.0, 1080.0],
                Vec3::new(-100.0, 0.0, 0.0)
            )
            .is_none()
        );
    }

    #[test]
    fn shades_are_cut_at_the_eye_and_rows_cross_polygons() {
        assert_eq!(box_triangles(Vec3::ZERO, Vec3::ONE).len(), 12);
        let view = glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::X, Vec3::Z);
        let projection =
            glam::camera::rh::proj::directx::perspective(1.2, 16.0 / 9.0, 2.0, 8_192.0);
        let view_projection = projection * view;
        let viewport = [1920.0, 1080.0];
        // A floor triangle reaching behind the eye keeps its part in front.
        let floor = [
            Vec3::new(-100.0, -50.0, -40.0),
            Vec3::new(200.0, -50.0, -40.0),
            Vec3::new(200.0, 50.0, -40.0),
        ];
        let polygon = screen_polygon(&view_projection, viewport, &floor);
        assert_eq!(polygon.len(), 4, "cut into a quad: {polygon:?}");
        assert!(polygon.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
        // Wholly behind: nothing.
        let behind = floor.map(|p| Vec3::new(-p.x.abs() - 10.0, p.y, p.z));
        assert!(screen_polygon(&view_projection, viewport, &behind).is_empty());
        // A row through a square spans its width; outside, nothing.
        let square = [[10.0, 10.0], [50.0, 10.0], [50.0, 50.0], [10.0, 50.0]];
        assert_eq!(row_span(&square, 30.0), Some((10.0, 50.0)));
        assert_eq!(row_span(&square, 60.0), None);
    }

    #[test]
    fn rays_enter_boxes_ahead_only() {
        let low = Vec3::new(10.0, -1.0, -1.0);
        let high = Vec3::new(12.0, 1.0, 1.0);
        assert_eq!(ray_box(Vec3::ZERO, Vec3::X, low, high), Some(10.0));
        assert_eq!(ray_box(Vec3::ZERO, -Vec3::X, low, high), None);
        assert_eq!(ray_box(Vec3::ZERO, Vec3::Y, low, high), None);
        // From inside, the box is met at once.
        assert_eq!(
            ray_box(Vec3::new(11.0, 0.0, 0.0), Vec3::X, low, high),
            Some(0.0)
        );
    }

    #[test]
    fn origins_parse_and_the_nearest_entity_is_named() {
        assert_eq!(
            parse_vector(Some("1 2.5 -3")),
            Some(Vec3::new(1.0, 2.5, -3.0))
        );
        assert_eq!(parse_vector(Some("1 2")), None);
        let entities = sjk_entity::parse_entity_lump(
            b"{\n\"classname\" \"worldspawn\"\n}\n\
              {\n\"classname\" \"light\"\n\"origin\" \"0 0 100\"\n}\n\
              {\n\"classname\" \"misc_model\"\n\"origin\" \"0 0 20\"\n}\n",
        )
        .expect("parses");
        assert_eq!(nearest_entity(&entities, Vec3::ZERO), Some(2));
        assert_eq!(
            nearest_entity(&entities, Vec3::new(0.0, 0.0, 1_000.0)),
            None
        );
    }

    #[test]
    fn a_note_needs_a_confirmed_selection_and_text() {
        let mut notes = Notes::default();
        let directory = std::env::temp_dir().join(format!("sjk-notes-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("directory");
        assert!(notes.save("hello", &directory, &directory).is_none());
        notes.writing = Some(Selection {
            map: "maps/mp/ffa3.bsp".into(),
            pose: "(0 0 0) : 90 0".into(),
            eye: Vec3::new(10.0, 20.0, 76.0),
            forward: Vec3::Y,
            hit: Vec3::new(10.0, 120.0, 76.0),
            normal: -Vec3::Y,
            distance: 100.0,
            surface: Some(Surface {
                index: 7,
                shader: "textures/imperial/basic_floor".into(),
                remapped: Some("textures/video/raven".into()),
                kind: "planar",
                lightmap: 2,
                vertices: 4,
                material: 3,
            }),
            entity: None,
            collision: None,
            edges: Vec::new(),
            triangles: Vec::new(),
            selected: Instant::now(),
            model: None,
            remaps: vec![
                Remap {
                    source: "local",
                    from: "textures/other/wall".into(),
                    to: "textures/other/wall_red".into(),
                },
                Remap {
                    source: "server",
                    from: "textures/imperial/basic_floor".into(),
                    to: "textures/video/raven".into(),
                },
            ],
        });
        let (shot, line, hub) = notes
            .save("  too   shiny  ", &directory, &directory)
            .expect("saved");
        let hub = hub.expect("the hub takes it");
        assert_eq!(hub.text, "too shiny");
        assert_eq!(hub.view, Some([10.0, 20.0, 40.0, 90.0]));
        assert_eq!(hub.shader, "textures/imperial/basic_floor");
        assert_eq!(
            (hub.surface, hub.lighting.as_str()),
            (Some(7), "lightmapped")
        );
        assert_eq!(hub.map, "maps/mp/ffa3.bsp");
        assert!(shot.starts_with("note_"));
        assert!(line.contains("setviewpos 10 20 40 90"), "{line}");
        // The remap's target is what the player saw; the hub has both, and the remaps,
        // the noted shader's first.
        assert_eq!(hub.remapped, "textures/video/raven");
        assert_eq!(
            hub.remaps
                .iter()
                .map(|r| (r.source.as_str(), r.from.as_str()))
                .collect::<Vec<_>>(),
            [
                ("server", "textures/imperial/basic_floor"),
                ("local", "textures/other/wall")
            ]
        );
        assert!(hub.model.is_none());
        assert!(line.contains(
            "textures/imperial/basic_floor remapped to textures/video/raven (surface 7, lightmapped)"
        ));
        let json = std::fs::read_to_string(directory.join("notes.jsonl")).expect("jsonl");
        let value: serde_json::Value = serde_json::from_str(json.trim()).expect("json");
        assert_eq!(value["note"], "too   shiny");
        assert_eq!(value["surface"]["shader"], "textures/imperial/basic_floor");
        assert_eq!(value["surface"]["remapped_to"], "textures/video/raven");
        assert_eq!(value["remaps"][1]["source"], "server");
        assert_eq!(value["remaps"][1]["to"], "textures/video/raven");
        assert!(value["model"].is_null());
        let shot_path = directory.join(format!("{shot}.jpg")).display().to_string();
        assert_eq!(value["screenshot"], shot_path);
        // Saving consumes the selection.
        assert!(notes.save("again", &directory, &directory).is_none());
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A note about a model names it, its shaders with their remaps, and leaves the
    /// surface behind it out; Clear model notes that surface again.
    #[test]
    fn a_note_about_a_model_names_it_and_clear_model_drops_it() {
        let (entities, mut cache) = models::tests::cube_map();
        let vfs = VirtualFileSystem::new();
        let mut model = pick_model(
            &entities,
            &mut cache,
            &vfs,
            "maps/test.bsp",
            Vec3::ZERO,
            Vec3::X,
        )
        .expect("the crate");
        let remaps = vec![Remap {
            source: "server",
            from: "models/test/crate".into(),
            to: "textures/video/raven".into(),
        }];
        model.shaders[0].remapped = remap_of(&remaps, &model.shaders[0].shader);
        assert_eq!(
            model.shaders[0].remapped.as_deref(),
            Some("textures/video/raven")
        );
        let mut notes = Notes {
            writing: Some(Selection {
                map: "maps/test.bsp".into(),
                pose: "(0 0 -36) : 0 0".into(),
                eye: Vec3::ZERO,
                forward: Vec3::X,
                hit: Vec3::new(400.0, 0.0, 0.0),
                normal: -Vec3::X,
                distance: 400.0,
                surface: Some(Surface {
                    index: 3,
                    shader: "textures/test/wall".into(),
                    remapped: None,
                    kind: "planar",
                    lightmap: 0,
                    vertices: 4,
                    material: 0,
                }),
                entity: None,
                collision: None,
                edges: Vec::new(),
                triangles: Vec::new(),
                selected: Instant::now(),
                model: Some(model),
                remaps,
            }),
            ..Notes::default()
        };
        let selection = notes.writing.as_ref().expect("writing");
        let summary = selection.summary();
        assert!(
            summary.starts_with(
                "misc_model_static models/test/crate.md3 \
                 (models/test/crate remapped to textures/video/raven)"
            ),
            "{summary}"
        );
        assert_eq!(
            selection.model_path().as_deref(),
            Some("models/test/crate.md3")
        );
        let hub = selection.world_note("crate too dark".into());
        assert_eq!(hub.shader, "models/test/crate");
        assert_eq!(hub.remapped, "textures/video/raven");
        assert_eq!((hub.surface, hub.lighting.as_str()), (None, ""));
        assert_eq!(hub.entity, "misc_model_static");
        let noted = hub.model.expect("the model");
        assert_eq!(noted.path, "models/test/crate.md3");
        assert_eq!(noted.entity, Some(1));
        assert_eq!(noted.origin, Some([100.0, 0.0, 0.0]));
        assert_eq!(noted.shaders[0].remapped, "textures/video/raven");
        let json = selection.json("crate too dark", "now", "shot.jpg");
        assert!(json["surface"].is_null());
        assert_eq!(json["entity"]["classname"], "misc_model_static");
        assert_eq!(json["model"]["path"], "models/test/crate.md3");
        assert_eq!(json["model"]["angles"], serde_json::json!([0.0, 45.0, 0.0]));
        assert_eq!(json["model"]["scale"], serde_json::json!([2.0, 2.0, 2.0]));
        assert_eq!(
            json["model"]["shaders"][0]["remapped_to"],
            "textures/video/raven"
        );
        assert!(json["distance"].as_f64().expect("distance") < 100.0);
        // Clear model: the surface behind it again, and no model to clear twice.
        let (subject, model) = notes.clear_model().expect("cleared");
        assert!(
            subject.starts_with("textures/test/wall (surface 3"),
            "{subject}"
        );
        assert_eq!(model, None);
        assert!(notes.clear_model().is_none());
    }

    /// A click's ray leaves the eye through the clicked pixel: the centre is the view.
    #[test]
    fn a_click_casts_a_ray_through_its_pixel() {
        let eye = Vec3::new(0.0, 0.0, 50.0);
        let view = glam::camera::rh::view::look_at_mat4(eye, eye + Vec3::X, Vec3::Z);
        let projection =
            glam::camera::rh::proj::directx::perspective(1.2, 16.0 / 9.0, 2.0, 8_192.0);
        let camera = CameraUniform {
            view_projection: (projection * view).to_cols_array_2d(),
            camera_position: eye.to_array(),
            view_forward: Vec3::X.to_array(),
            ..bytemuck::Zeroable::zeroed()
        };
        let viewport = [1920.0, 1080.0];
        let centre = screen_ray(&camera, viewport, [960.0, 540.0]).expect("a ray");
        assert!(centre.dot(Vec3::X) > 0.9999, "{centre:?}");
        let left = screen_ray(&camera, viewport, [100.0, 540.0]).expect("a ray");
        assert!(left.y > 0.1 && left.x > 0.0, "{left:?}");
        let low = screen_ray(&camera, viewport, [960.0, 1000.0]).expect("a ray");
        assert!(low.z < -0.1, "{low:?}");
        assert!(screen_ray(&camera, [0.0, 0.0], [1.0, 1.0]).is_none());
    }
}

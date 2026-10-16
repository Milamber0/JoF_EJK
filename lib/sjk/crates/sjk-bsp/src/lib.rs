//! Safe, owned loading and collision queries for Jedi Academy RBSP worlds.
//!
//! A [`Bsp`] is an ordinary immutable value: no loader changes a process-wide
//! "current map". That property is the basis for retaining and querying more
//! than one world at a time.

use std::error::Error;
use std::fmt;
use std::ops::Range;

mod collision;
mod error;
mod fog;
pub use error::BspError;
mod parse;
use parse::*;
mod validation;
pub use fog::{Fog, FogVolume};
use validation::*;
mod brush_winding;
mod leaf_links;
mod light_grid;
mod mark_fragments;
mod model_append;
mod patch_collision;
mod patch_facets;
mod patch_geometry;
mod patch_grid;
mod render_data;
mod trace_scratch;
mod write;

pub use collision::{Aabb, AabbError, CollisionTrace, trace_box_against_box};
pub use leaf_links::BoxLeaves;
pub use light_grid::{GridLight, LIGHT_STYLE_NONE, LightGridLayout};
pub use mark_fragments::{MAX_VERTICES_ON_POLY, MarkFragment, MarkFragments, MarkProjection};
pub use render_data::{
    DrawVertex, LightGridSample, Model, RenderData, Surface, SurfaceKind, Visibility,
};
pub use trace_scratch::TraceScratch;

pub const RBSP_MAGIC: [u8; 4] = *b"RBSP";
pub const RBSP_VERSION: i32 = 1;
pub const HEADER_LUMPS: usize = 18;
const HEADER_BYTES: usize = 8 + HEADER_LUMPS * 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum LumpKind {
    Entities = 0,
    Shaders = 1,
    Planes = 2,
    Nodes = 3,
    Leaves = 4,
    LeafSurfaces = 5,
    LeafBrushes = 6,
    Models = 7,
    Brushes = 8,
    BrushSides = 9,
    DrawVertices = 10,
    DrawIndices = 11,
    Fogs = 12,
    Surfaces = 13,
    Lightmaps = 14,
    LightGrid = 15,
    Visibility = 16,
    LightArray = 17,
}

impl LumpKind {
    const ALL: [Self; HEADER_LUMPS] = [
        Self::Entities,
        Self::Shaders,
        Self::Planes,
        Self::Nodes,
        Self::Leaves,
        Self::LeafSurfaces,
        Self::LeafBrushes,
        Self::Models,
        Self::Brushes,
        Self::BrushSides,
        Self::DrawVertices,
        Self::DrawIndices,
        Self::Fogs,
        Self::Surfaces,
        Self::Lightmaps,
        Self::LightGrid,
        Self::Visibility,
        Self::LightArray,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Shader {
    name: Box<[u8]>,
    pub surface_flags: u32,
    pub content_flags: u32,
}

impl Shader {
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn name_lossy(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: [f32; 3],
    pub distance: f32,
}

impl Plane {
    pub fn signed_distance(self, point: [f32; 3]) -> f32 {
        dot(self.normal, point) - self.distance
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeChild {
    Node(usize),
    Leaf(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub plane: usize,
    pub children: [NodeChild; 2],
    pub minimums: [i32; 3],
    pub maximums: [i32; 3],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Leaf {
    pub cluster: i32,
    pub area: i32,
    pub minimums: [i32; 3],
    pub maximums: [i32; 3],
    pub leaf_surfaces: Range<usize>,
    pub leaf_brushes: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrushSide {
    pub plane: usize,
    pub shader: usize,
    pub draw_surface: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Brush {
    pub sides: Range<usize>,
    pub shader: usize,
    pub content_flags: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bsp {
    entities: Box<[u8]>,
    shaders: Box<[Shader]>,
    planes: Box<[Plane]>,
    nodes: Box<[Node]>,
    leaves: Box<[Leaf]>,
    leaf_surfaces: Box<[usize]>,
    leaf_brushes: Box<[usize]>,
    brushes: Box<[Brush]>,
    brush_sides: Box<[BrushSide]>,
    render: RenderData,
    fogs: Box<[Fog]>,
    root_fog_count: usize,
    patches: Box<[Option<patch_collision::Patch>]>,
}

impl Bsp {
    /// Decode an owned map using actual file ranges, record layouts and cross references.
    /// Toolchain count ceilings are not validity constraints; all count-based allocations
    /// require complete source records and representable decoded layouts first.
    pub fn parse(data: &[u8]) -> Result<Self, BspError> {
        let lumps = parse_header(data)?;
        // The entity string has no load-time limit: the legacy engine only
        // bounds it at compile time, and retail t2_dpred exceeds that bound.
        let entities = lump_bytes(data, lumps[LumpKind::Entities as usize])?;

        let shaders = parse_shaders(data, lumps[LumpKind::Shaders as usize])?;
        let planes = parse_planes(data, lumps[LumpKind::Planes as usize])?;
        let raw_nodes = parse_nodes(data, lumps[LumpKind::Nodes as usize])?;
        let raw_leaves = parse_leaves(data, lumps[LumpKind::Leaves as usize])?;
        let leaf_surfaces = parse_indices(
            data,
            lumps[LumpKind::LeafSurfaces as usize],
            LumpKind::LeafSurfaces,
        )?;
        let leaf_brushes = parse_indices(
            data,
            lumps[LumpKind::LeafBrushes as usize],
            LumpKind::LeafBrushes,
        )?;
        let raw_brushes = parse_brushes(data, lumps[LumpKind::Brushes as usize])?;
        let brush_sides = parse_brush_sides(data, lumps[LumpKind::BrushSides as usize])?;

        require_nonempty(LumpKind::Shaders, shaders.len())?;
        require_nonempty(LumpKind::Planes, planes.len())?;
        require_nonempty(LumpKind::Nodes, raw_nodes.len())?;
        require_nonempty(LumpKind::Leaves, raw_leaves.len())?;

        let nodes = validate_nodes(raw_nodes, planes.len(), raw_leaves.len())?;
        validate_node_graph(&nodes)?;
        let leaves = validate_leaves(raw_leaves, leaf_surfaces.len(), leaf_brushes.len())?;
        validate_leaf_brushes(&leaf_brushes, raw_brushes.len())?;
        validate_brush_sides(&brush_sides, planes.len(), shaders.len())?;
        let brushes = validate_brushes(raw_brushes, brush_sides.len(), &shaders)?;
        let fogs = fog::parse(data, lumps[LumpKind::Fogs as usize], &brushes)?;
        let render = render_data::parse_render_data(data, &lumps, &shaders, brushes.len())?;
        validate_leaf_surfaces(&leaf_surfaces, render.surfaces().len())?;
        let patches = patch_collision::load(&render, &shaders)?;

        Ok(Self {
            entities: entities.to_vec().into_boxed_slice(),
            shaders: shaders.into_boxed_slice(),
            planes: planes.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
            leaves: leaves.into_boxed_slice(),
            leaf_surfaces: leaf_surfaces.into_boxed_slice(),
            leaf_brushes: leaf_brushes.into_boxed_slice(),
            brushes: brushes.into_boxed_slice(),
            brush_sides: brush_sides.into_boxed_slice(),
            render,
            root_fog_count: fogs.len(),
            fogs: fogs.into_boxed_slice(),
            patches,
        })
    }

    /// A map with nothing in it, spanning `minimums..maximums`: one leaf outside any
    /// visibility cluster, no surfaces, no brushes, an empty entity string. It stands in
    /// where a world's geometry comes from somewhere other than a BSP, so that every
    /// consumer of a map finds a valid, empty one: traces miss, no surface is drawn.
    pub fn empty(minimums: [f32; 3], maximums: [f32; 3]) -> Self {
        Self {
            entities: Box::default(),
            shaders: Box::default(),
            planes: Box::new([Plane {
                normal: [0.0, 0.0, 1.0],
                distance: 0.0,
            }]),
            nodes: Box::new([Node {
                plane: 0,
                children: [NodeChild::Leaf(0); 2],
                minimums: minimums.map(|value| value.floor() as i32),
                maximums: maximums.map(|value| value.ceil() as i32),
            }]),
            leaves: Box::new([Leaf {
                cluster: -1,
                area: 0,
                minimums: minimums.map(|value| value.floor() as i32),
                maximums: maximums.map(|value| value.ceil() as i32),
                leaf_surfaces: 0..0,
                leaf_brushes: 0..0,
            }]),
            leaf_surfaces: Box::default(),
            leaf_brushes: Box::default(),
            brushes: Box::default(),
            brush_sides: Box::default(),
            render: RenderData::empty(minimums, maximums),
            fogs: Box::default(),
            root_fog_count: 0,
            patches: Box::default(),
        }
    }

    /// Validated fog records, indexed by `Surface::fog` (zero based).
    pub fn fogs(&self) -> &[Fog] {
        &self.fogs
    }

    /// Number of fog records belonging to the root world. Appended asset
    /// volumes remain in model space and must not become world-global fog.
    pub fn root_fog_count(&self) -> usize {
        self.root_fog_count
    }

    pub fn entities(&self) -> &[u8] {
        &self.entities
    }

    pub fn shaders(&self) -> &[Shader] {
        &self.shaders
    }

    pub fn planes(&self) -> &[Plane] {
        &self.planes
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn leaves(&self) -> &[Leaf] {
        &self.leaves
    }

    pub fn brushes(&self) -> &[Brush] {
        &self.brushes
    }

    pub fn brush_sides(&self) -> &[BrushSide] {
        &self.brush_sides
    }

    pub fn render(&self) -> &RenderData {
        &self.render
    }

    pub fn leaf_surface_indices(&self, leaf_index: usize) -> Option<&[usize]> {
        let leaf = self.leaves.get(leaf_index)?;
        Some(&self.leaf_surfaces[leaf.leaf_surfaces.clone()])
    }

    pub fn leaf_at(&self, point: [f32; 3]) -> usize {
        let mut child = NodeChild::Node(0);
        loop {
            match child {
                NodeChild::Node(index) => {
                    let node = &self.nodes[index];
                    let side = usize::from(self.planes[node.plane].signed_distance(point) < 0.0);
                    child = node.children[side];
                }
                NodeChild::Leaf(index) => return index,
            }
        }
    }

    /// `CM_ModelContents` (`cm_test.cpp`): the union of the content flags of inline model
    /// `model`'s brushes and patches — what an entity made of that model is
    /// (`SV_SetBrushModel`). 0 for a model the map does not have.
    pub fn model_contents(&self, model: usize) -> u32 {
        let Some(model) = self.render.models().get(model) else {
            return 0;
        };
        let brushes = self
            .brushes
            .get(model.brushes.clone())
            .unwrap_or_default()
            .iter()
            .fold(0, |contents, brush| contents | brush.content_flags);
        model
            .surfaces
            .clone()
            .filter_map(|index| self.patches.get(index)?.as_ref())
            .fold(brushes, |contents, patch| contents | patch.contents())
    }

    /// Returns the union of matching brush-content flags at a point.
    pub fn point_contents(&self, point: [f32; 3], content_mask: u32) -> u32 {
        let leaf = &self.leaves[self.leaf_at(point)];
        let mut contents = 0_u32;
        for leaf_brush in leaf.leaf_brushes.clone() {
            let brush = &self.brushes[self.leaf_brushes[leaf_brush]];
            if brush.content_flags & content_mask == 0 {
                continue;
            }
            let inside = brush.sides.clone().all(|side_index| {
                let side = &self.brush_sides[side_index];
                self.planes[side.plane].signed_distance(point) <= 0.0
            });
            if inside {
                contents |= brush.content_flags;
            }
        }
        contents & content_mask
    }
}

#[derive(Clone, Copy, Debug)]
struct Lump {
    offset: usize,
    length: usize,
}

#[derive(Clone, Debug)]
struct RawNode {
    plane: i32,
    children: [i32; 2],
    minimums: [i32; 3],
    maximums: [i32; 3],
}

#[derive(Clone, Debug)]
struct RawLeaf {
    cluster: i32,
    area: i32,
    minimums: [i32; 3],
    maximums: [i32; 3],
    first_leaf_surface: i32,
    leaf_surface_count: i32,
    first_leaf_brush: i32,
    leaf_brush_count: i32,
}

#[derive(Clone, Debug)]
struct RawBrush {
    first_side: i32,
    side_count: i32,
    shader: i32,
}

fn read_i32(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .expect("validated record"),
    )
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .expect("validated record"),
    )
}

fn read_f32(data: &[u8], offset: usize) -> f32 {
    f32::from_bits(read_u32(data, offset))
}

fn read_i32x3(data: &[u8], offset: usize) -> [i32; 3] {
    [
        read_i32(data, offset),
        read_i32(data, offset + 4),
        read_i32(data, offset + 8),
    ]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

pub use write::{
    CollisionBrush, CollisionShader, box_brush, write_collision_map, write_collision_map_owned,
    write_collision_map_with_models,
};

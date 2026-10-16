//! Write a collision-only map: what a legacy game server needs of a world whose visuals
//! live elsewhere.
//!
//! The output is a valid RBSP with convex brushes, a spatial tree over them, one world
//! model and an entity string, and nothing to draw. Every leaf is in the single
//! visibility cluster, so a server treats all entities as mutually visible.
use crate::{HEADER_LUMPS, LumpKind, Plane, RBSP_MAGIC, RBSP_VERSION};

/// A convex solid: the half-spaces whose intersection it is, normals pointing out.
///
/// The first six planes must be the axial bounds in the order −X, +X, −Y, +Y, −Z, +Z
/// (`CM_BoundBrush` reads a brush's box from them); bevels and faces follow.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionBrush {
    pub planes: Vec<Plane>,
    /// Index into the shader list given to [`write_collision_map`]: its content flags
    /// are the brush's.
    pub shader: usize,
}

impl CollisionBrush {
    /// The box the six leading axial planes describe.
    pub fn bounds(&self) -> Option<[[f32; 3]; 2]> {
        let sides = self.planes.get(..6)?;
        Some([
            [-sides[0].distance, -sides[2].distance, -sides[4].distance],
            [sides[1].distance, sides[3].distance, sides[5].distance],
        ])
    }
}

/// Name and flags of one collision shader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollisionShader {
    pub name: String,
    pub surface_flags: u32,
    pub content_flags: u32,
}

/// Most brushes in a leaf before it is split, and the deepest the tree goes.
const LEAF_BRUSHES: usize = 16;
const MAX_DEPTH: usize = 40;

struct Tree {
    planes: Vec<Plane>,
    /// plane, children (negative: −(leaf + 1)), bounds.
    nodes: Vec<(usize, [i32; 2], [[f32; 3]; 2])>,
    /// bounds, range of `leaf_brushes`.
    leaves: Vec<([[f32; 3]; 2], std::ops::Range<usize>)>,
    leaf_brushes: Vec<u32>,
}

impl Tree {
    fn build(
        &mut self,
        boxes: &[[[f32; 3]; 2]],
        members: Vec<u32>,
        bounds: [[f32; 3]; 2],
        depth: usize,
        budget: usize,
    ) -> i32 {
        // The best of three median splits: the axis whose larger side is smallest. A brush
        // that straddles the plane goes to both sides, so a split only helps while it
        // still sends a good share of the brushes to one side alone.
        let mut best: Option<(usize, f32, usize)> = None;
        if members.len() > LEAF_BRUSHES && depth < MAX_DEPTH {
            let mut centres: Vec<f32> = Vec::with_capacity(members.len());
            for axis in 0..3 {
                centres.clear();
                centres.extend(members.iter().map(|&member| {
                    let extent = boxes[member as usize];
                    (extent[0][axis] + extent[1][axis]) * 0.5
                }));
                let middle = centres.len() / 2;
                let (_, &mut median, _) = centres.select_nth_unstable_by(middle, f32::total_cmp);
                if median <= bounds[0][axis] || median >= bounds[1][axis] {
                    continue;
                }
                let front = members
                    .iter()
                    .filter(|&&member| boxes[member as usize][1][axis] >= median)
                    .count();
                let back = members
                    .iter()
                    .filter(|&&member| boxes[member as usize][0][axis] <= median)
                    .count();
                let larger = front.max(back);
                if best.is_none_or(|(_, _, other)| larger < other) {
                    best = Some((axis, median, larger));
                }
            }
        }
        // Bound total leaf references, not just the larger child's size. Dense,
        // thin triangle prisms otherwise duplicate exponentially through splits.
        let Some((axis, middle, _)) = best
            .filter(|&(_, _, larger)| larger * 10 < members.len() * 9)
            .filter(|&(axis, middle, _)| {
                members
                    .iter()
                    .map(|&member| {
                        let bounds = boxes[member as usize];
                        usize::from(bounds[1][axis] >= middle)
                            + usize::from(bounds[0][axis] <= middle)
                    })
                    .sum::<usize>()
                    <= budget
            })
        else {
            let start = self.leaf_brushes.len();
            self.leaf_brushes.extend(members);
            self.leaves.push((bounds, start..self.leaf_brushes.len()));
            return -(self.leaves.len() as i32);
        };
        let front: Vec<u32> = members
            .iter()
            .copied()
            .filter(|&member| boxes[member as usize][1][axis] >= middle)
            .collect();
        let back: Vec<u32> = members
            .iter()
            .copied()
            .filter(|&member| boxes[member as usize][0][axis] <= middle)
            .collect();
        drop(members);
        let total = front.len() + back.len();
        let spare = budget - total;
        let front_budget = front.len() + spare * front.len() / total;
        let back_budget = budget - front_budget;
        let mut normal = [0.; 3];
        normal[axis] = 1.;
        let plane = self.planes.len();
        self.planes.push(Plane {
            normal,
            distance: middle,
        });
        let node = self.nodes.len();
        self.nodes.push((plane, [0, 0], bounds));
        let (mut front_bounds, mut back_bounds) = (bounds, bounds);
        front_bounds[0][axis] = middle;
        back_bounds[1][axis] = middle;
        let children = [
            self.build(boxes, front, front_bounds, depth + 1, front_budget),
            self.build(boxes, back, back_bounds, depth + 1, back_budget),
        ];
        self.nodes[node].1 = children;
        node as i32
    }
}

/// Serialise a collision-only map. `entities` is the entity string (worldspawn, spawn
/// points, ...); brushes without six leading axial planes are left out.
pub fn write_collision_map(
    entities: &str,
    shaders: &[CollisionShader],
    brushes: &[CollisionBrush],
) -> Vec<u8> {
    write_collision_brushes(entities, shaders, brushes, &[])
}

/// [`write_collision_map`] with inline models after the world: `models[i]` is model `*i+1`,
/// its brushes kept out of the world's tree and its bounds their box, as a map compiler
/// leaves a brush entity's.
pub fn write_collision_map_with_models(
    entities: &str,
    shaders: &[CollisionShader],
    brushes: &[CollisionBrush],
    models: &[Vec<CollisionBrush>],
) -> Vec<u8> {
    write_collision_brushes(entities, shaders, brushes, models)
}

/// Write the same collision-only RBSP while releasing each input brush after its
/// planes have been indexed. Large offline cooks avoid retaining two full copies.
pub fn write_collision_map_owned(
    entities: &str,
    shaders: &[CollisionShader],
    brushes: Vec<CollisionBrush>,
) -> Vec<u8> {
    write_collision_brushes(entities, shaders, brushes, &[])
}

fn write_collision_brushes<B: std::borrow::Borrow<CollisionBrush>>(
    entities: &str,
    shaders: &[CollisionShader],
    brushes: impl IntoIterator<Item = B>,
    models: &[Vec<CollisionBrush>],
) -> Vec<u8> {
    let mut boxes = Vec::new();
    // Brush planes first, shared where equal; the tree's planes after them.
    let mut planes = Vec::<Plane>::new();
    let mut known = std::collections::HashMap::<[u32; 4], u32>::new();
    let mut sides = Vec::<(u32, u32)>::new();
    let mut records = Vec::<(u32, u32, u32)>::new();
    for owned in brushes {
        let brush = owned.borrow();
        let Some(bounds) = brush.bounds().filter(|_| brush.shader < shaders.len()) else {
            continue;
        };
        boxes.push(bounds);
        let first = sides.len() as u32;
        for plane in &brush.planes {
            let key = [
                plane.normal[0].to_bits(),
                plane.normal[1].to_bits(),
                plane.normal[2].to_bits(),
                plane.distance.to_bits(),
            ];
            let index = *known.entry(key).or_insert_with(|| {
                planes.push(*plane);
                planes.len() as u32 - 1
            });
            sides.push((index, brush.shader as u32));
        }
        records.push((first, brush.planes.len() as u32, brush.shader as u32));
    }
    let world_brushes = records.len();
    // The inline models' brushes, after the world's and out of its tree.
    let mut model_ranges = Vec::with_capacity(models.len());
    for model in models {
        let first = records.len();
        let mut extent = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
        for brush in model {
            let Some(bounds) = brush.bounds().filter(|_| brush.shader < shaders.len()) else {
                continue;
            };
            for axis in 0..3 {
                extent[0][axis] = extent[0][axis].min(bounds[0][axis]);
                extent[1][axis] = extent[1][axis].max(bounds[1][axis]);
            }
            let first_side = sides.len() as u32;
            for plane in &brush.planes {
                let key = [
                    plane.normal[0].to_bits(),
                    plane.normal[1].to_bits(),
                    plane.normal[2].to_bits(),
                    plane.distance.to_bits(),
                ];
                let index = *known.entry(key).or_insert_with(|| {
                    planes.push(*plane);
                    planes.len() as u32 - 1
                });
                sides.push((index, brush.shader as u32));
            }
            records.push((first_side, brush.planes.len() as u32, brush.shader as u32));
        }
        model_ranges.push((extent, first, records.len() - first));
    }
    drop(known);
    let world = boxes.iter().fold(
        [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]],
        |mut all, extent| {
            for axis in 0..3 {
                all[0][axis] = all[0][axis].min(extent[0][axis]);
                all[1][axis] = all[1][axis].max(extent[1][axis]);
            }
            all
        },
    );
    let world = if boxes.is_empty() {
        [[-64.; 3], [64.; 3]]
    } else {
        world
    };
    let mut tree = Tree {
        planes,
        nodes: Vec::new(),
        leaves: Vec::new(),
        leaf_brushes: Vec::new(),
    };
    let root = tree.build(
        &boxes,
        (0..boxes.len() as u32).collect(),
        world,
        0,
        boxes.len().saturating_mul(4),
    );
    drop(boxes);
    if root < 0 {
        // The format needs a node: one plane above everything, both sides the only leaf.
        let plane = tree.planes.len();
        tree.planes.push(Plane {
            normal: [0., 0., 1.],
            distance: world[1][2] + 64.,
        });
        tree.nodes.push((plane, [root, root], world));
    }

    let mut lumps: [Vec<u8>; HEADER_LUMPS] = std::array::from_fn(|_| Vec::new());
    let put = |lump: &mut Vec<u8>, value: i32| lump.extend(value.to_le_bytes());
    let put_f = |lump: &mut Vec<u8>, value: f32| lump.extend(value.to_le_bytes());
    lumps[LumpKind::Entities as usize] = entities.bytes().chain([0]).collect();
    for shader in shaders {
        let lump = &mut lumps[LumpKind::Shaders as usize];
        let mut name = [0_u8; 64];
        let bytes = shader.name.as_bytes();
        name[..bytes.len().min(63)].copy_from_slice(&bytes[..bytes.len().min(63)]);
        lump.extend(name);
        lump.extend(shader.surface_flags.to_le_bytes());
        lump.extend(shader.content_flags.to_le_bytes());
    }
    for plane in &tree.planes {
        let lump = &mut lumps[LumpKind::Planes as usize];
        for value in plane.normal {
            put_f(lump, value);
        }
        put_f(lump, plane.distance);
    }
    let whole = |bounds: [[f32; 3]; 2]| {
        [
            bounds[0].map(|value| value.floor() as i32),
            bounds[1].map(|value| value.ceil() as i32),
        ]
    };
    for (plane, children, bounds) in &tree.nodes {
        let lump = &mut lumps[LumpKind::Nodes as usize];
        put(lump, *plane as i32);
        put(lump, children[0]);
        put(lump, children[1]);
        for corner in whole(*bounds) {
            for value in corner {
                put(lump, value);
            }
        }
    }
    for (bounds, range) in &tree.leaves {
        let lump = &mut lumps[LumpKind::Leaves as usize];
        put(lump, 0); // cluster
        put(lump, 0); // area
        for corner in whole(*bounds) {
            for value in corner {
                put(lump, value);
            }
        }
        put(lump, 0); // first leaf surface
        put(lump, 0); // leaf surface count
        put(lump, range.start as i32);
        put(lump, range.len() as i32);
    }
    for &brush in &tree.leaf_brushes {
        put(&mut lumps[LumpKind::LeafBrushes as usize], brush as i32);
    }
    drop(tree);
    {
        let lump = &mut lumps[LumpKind::Models as usize];
        for corner in world {
            for value in corner {
                put_f(lump, value);
            }
        }
        put(lump, 0); // first surface
        put(lump, 0); // surface count
        put(lump, 0); // first brush
        put(lump, world_brushes as i32);
        for (extent, first, count) in &model_ranges {
            for corner in extent {
                for value in corner {
                    put_f(lump, *value);
                }
            }
            put(lump, 0); // first surface
            put(lump, 0); // surface count
            put(lump, *first as i32);
            put(lump, *count as i32);
        }
    }
    for (first, count, shader) in &records {
        let lump = &mut lumps[LumpKind::Brushes as usize];
        put(lump, *first as i32);
        put(lump, *count as i32);
        put(lump, *shader as i32);
    }
    drop(records);
    for (plane, shader) in &sides {
        let lump = &mut lumps[LumpKind::BrushSides as usize];
        put(lump, *plane as i32);
        put(lump, *shader as i32);
        put(lump, -1); // draw surface
    }
    drop(sides);
    // One cluster that sees itself.
    {
        let lump = &mut lumps[LumpKind::Visibility as usize];
        put(lump, 1);
        put(lump, 1);
        lump.push(1);
    }

    let mut file = Vec::new();
    file.extend(RBSP_MAGIC);
    file.extend(RBSP_VERSION.to_le_bytes());
    let mut offset = 8 + HEADER_LUMPS * 8;
    for lump in &lumps {
        file.extend((offset as i32).to_le_bytes());
        file.extend((lump.len() as i32).to_le_bytes());
        offset += lump.len().next_multiple_of(4);
    }
    file.reserve(offset - file.len());
    for lump in lumps {
        file.extend(&lump);
        file.resize(file.len().next_multiple_of(4), 0);
    }
    file
}

/// The brush of an axis-aligned box: the six axial planes are all it needs.
pub fn box_brush(minimums: [f32; 3], maximums: [f32; 3], shader: usize) -> CollisionBrush {
    let axial = |axis: usize, sign: f32, distance: f32| {
        let mut normal = [0.; 3];
        normal[axis] = sign;
        Plane { normal, distance }
    };
    CollisionBrush {
        shader,
        planes: (0..3)
            .flat_map(|axis| {
                [
                    axial(axis, -1., -minimums[axis]),
                    axial(axis, 1., maximums[axis]),
                ]
            })
            .collect(),
    }
}

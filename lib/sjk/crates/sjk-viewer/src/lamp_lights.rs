//! Local lights of the real-time mode, from the map's emissive surfaces: q3map bakes
//! `q3map_surfacelight` faces as area lights and strips `light` entities from the BSP,
//! so the lamp faces are the map's lamps. Each connected patch of emissive triangles
//! becomes one Lambertian area light (position, normal, colour, power = radiance × area)
//! with a radius where its irradiance falls below a floor, filed in a world grid so a
//! pixel visits only the lamps of its cell.
use glam::{IVec3, Vec3};
#[path = "lamp_emitters.rs"]
mod emitters;
pub(crate) use emitters::collect as collect_patches;
#[path = "lamp_grid.rs"]
mod grid;
#[path = "lamp_grid_refine.rs"]
mod refine;
#[path = "lamp_grid_selection.rs"]
mod selection;
#[path = "lamp_visibility.rs"]
mod visibility;

/// One lamp: an area light seen from beyond its own size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lamp {
    pub(crate) position: Vec3,
    /// Zero denotes a two-sided area, or an omnidirectional glow when axes are zero.
    pub(crate) normal: Vec3,
    pub(crate) color: [f32; 3],
    /// Radiance times area, display units (a sunlit white surface is about one).
    pub(crate) power: f32,
    pub(crate) radius: f32,
    /// Half-axes of the finite emitting rectangle.
    pub(crate) axis_u: Vec3,
    pub(crate) axis_v: Vec3,
}

/// Every lamp of a map and the grid that finds them.
#[derive(Default)]
pub(crate) struct LampSet {
    pub(crate) lamps: Vec<Lamp>,
    pub(crate) cell: f32,
    pub(crate) origin: IVec3,
    pub(crate) counts: [u32; 3],
    /// Root cells, followed by optional children: list offset/count, or child-table
    /// offset and `refine::BRANCH | side` for a crowded root.
    pub(crate) cells: Vec<[u32; 2]>,
    pub(crate) cell_lamps: Vec<u32>,
    /// Shared node thresholds, sampled with smooth trilinear weights.
    pub(crate) thresholds: Vec<f32>,
}

/// Irradiance below which a lamp's reach ends.
const FLOOR: f32 = 0.004;
/// Scale from surface radiance × area to lamp power (calibrated against the bake).
pub(crate) const POWER_SCALE: f32 = 1.5;

/// One emissive material's contribution: its radiance (display units) and its triangles.
pub(crate) struct Emitter<'a> {
    /// A compact fixture shares one influence range across its luminous pieces.
    pub(crate) shared_reach: bool,
    pub(crate) omnidirectional: bool,
    pub(crate) texture: &'a crate::world_materials::emission::Texture,
    pub(crate) radiance: [f32; 3],
    pub(crate) ranges: Vec<std::ops::Range<u32>>,
}

impl LampSet {
    /// Extract all map-owned sources, then build their shared lookup exactly once.
    pub(crate) fn extract(
        vertices: &[([f32; 3], [f32; 3], [f32; 2])],
        indices: &[u32],
        emitters: &[Emitter],
        extra: Vec<Lamp>,
    ) -> Self {
        let mut lamps = emitters::collect(vertices, indices, emitters);
        lamps.extend(extra);
        grid::build(lamps)
    }
}

/// GPU form: a grid uniform and one storage buffer of vec4<u32> holding the lamps (five
/// vec4 of floats each, bit-cast), the cell table (two cells per vec4: offset, count) and
/// the cell lamp lists (four indices per vec4). One buffer keeps the compute stage of the
/// probe update within the eight storage buffers a baseline device allows.
pub(crate) struct Gpu {
    pub(crate) grid: wgpu::Buffer,
    pub(crate) data: wgpu::Buffer,
    visibility: wgpu::TextureView,
    /// Directional samples per edge of the traced atlas; `None` before tracing.
    resolution: Option<u32>,
    lamp_count: u32,
    /// Mover door tiles after the lamps' own in the atlas (`mover_occlusion.rs`).
    door_tiles: u32,
    doors: Option<crate::world_materials::mover_occlusion::Doors>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Grid {
    origin: [i32; 4],
    counts: [u32; 4],
    cell: [f32; 4],
    offsets: [u32; 4],
    selection: [u32; 4],
}

impl Gpu {
    /// Upload a set; an empty set is the neutral stand-in (zero counts, one dummy lamp).
    /// Lamps that `occluders` (the map's movers) can reach get door tiles, as many as the
    /// atlas holds without lowering its resolution.
    pub(crate) fn new(
        device: &wgpu::Device,
        set: &LampSet,
        occluders: &[crate::world_materials::mover_occlusion::Occluder],
    ) -> Self {
        use crate::world_materials::mover_occlusion::Doors;
        use wgpu::util::DeviceExt;
        let make = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let lamps: &[Lamp] = if set.lamps.is_empty() {
            &[Lamp {
                position: Vec3::ZERO,
                normal: Vec3::Z,
                color: [0.; 3],
                power: 0.,
                radius: 0.,
                axis_u: Vec3::ZERO,
                axis_v: Vec3::ZERO,
            }]
        } else {
            &set.lamps
        };
        let capacity = visibility::door_capacity(
            set.lamps.len() as u32,
            device.limits().max_texture_dimension_2d,
        );
        let doors = Doors::assign(&set.lamps, occluders, capacity as usize);
        // Per lamp: its door tile + 1, zero without one, in the first axis's spare word.
        let mut door_of = vec![0u32; lamps.len()];
        for (tile, door) in doors.tiles.iter().enumerate() {
            door_of[door.lamp as usize] = tile as u32 + 1;
        }
        let mut data: Vec<[u32; 4]> = Vec::new();
        for (l, door) in lamps.iter().zip(&door_of) {
            data.push([l.position.x, l.position.y, l.position.z, l.radius].map(f32::to_bits));
            data.push([l.normal.x, l.normal.y, l.normal.z, l.power].map(f32::to_bits));
            let area = 4. * l.axis_u.cross(l.axis_v).length();
            data.push(
                [
                    l.color[0],
                    l.color[1],
                    l.color[2],
                    if area > 1e-6 { 1. / area } else { 0. },
                ]
                .map(f32::to_bits),
            );
            let [x, y, z] = l.axis_u.to_array().map(f32::to_bits);
            data.push([x, y, z, *door]);
            data.push(l.axis_v.extend(0.).to_array().map(f32::to_bits));
        }
        let cells_offset = data.len() as u32;
        let cells: &[[u32; 2]] = if set.cells.is_empty() {
            &[[0, 0]]
        } else {
            &set.cells
        };
        for pair in cells.chunks(2) {
            let second = pair.get(1).copied().unwrap_or([0, 0]);
            data.push([pair[0][0], pair[0][1], second[0], second[1]]);
        }
        let lists_offset = data.len() as u32;
        let lists: &[u32] = if set.cell_lamps.is_empty() {
            &[0]
        } else {
            &set.cell_lamps
        };
        for quad in lists.chunks(4) {
            let mut entry = [0u32; 4];
            entry[..quad.len()].copy_from_slice(quad);
            data.push(entry);
        }
        let threshold_offset = data.len() as u32;
        for values in set.thresholds.chunks(4) {
            let mut entry = [0u32; 4];
            for (to, value) in entry.iter_mut().zip(values) {
                *to = value.to_bits();
            }
            data.push(entry);
        }
        let grid = Grid {
            origin: [set.origin.x, set.origin.y, set.origin.z, 0],
            counts: [
                set.counts[0],
                set.counts[1],
                set.counts[2],
                set.lamps.len() as u32,
            ],
            cell: [set.cell.max(1.), 0., 0., 0.],
            offsets: [cells_offset, lists_offset, 0, 0],
            selection: [
                threshold_offset,
                u32::from(!set.thresholds.is_empty()),
                0,
                doors.tiles.len() as u32,
            ],
        };
        Self {
            visibility: visibility::texture(device, [1, 1]),
            resolution: None,
            lamp_count: set.lamps.len() as u32,
            door_tiles: doors.tiles.len() as u32,
            doors: (!doors.tiles.is_empty()).then_some(doors),

            grid: make(
                "SJK lamp grid",
                bytemuck::bytes_of(&grid),
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            data: make(
                "SJK lamps",
                bytemuck::cast_slice(&data),
                wgpu::BufferUsages::STORAGE,
            ),
        }
    }

    /// The door tiles, for the mover occlusion runtime; `None` after the first call or
    /// without any.
    pub(crate) fn take_doors(&mut self) -> Option<crate::world_materials::mover_occlusion::Doors> {
        self.doors.take()
    }

    /// The visibility atlas, lamp tiles then door tiles.
    pub(crate) fn visibility_view(&self) -> &wgpu::TextureView {
        &self.visibility
    }

    /// Directional samples per tile edge, once the atlas is traced.
    pub(crate) fn visibility_resolution(&self) -> Option<u32> {
        self.resolution
    }

    /// Layout entries for the two lamp bindings at `base`, fragment and compute visible.
    pub(crate) fn layout_entries(base: u32) -> [wgpu::BindGroupLayoutEntry; 2] {
        let stages = wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT;
        let buffer = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding: base + binding,
            visibility: stages,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        [
            buffer(0, wgpu::BufferBindingType::Uniform),
            buffer(1, wgpu::BufferBindingType::Storage { read_only: true }),
        ]
    }

    /// Bind group entries at `base`, matching `layout_entries`.
    pub(crate) fn entries(&self, base: u32) -> [wgpu::BindGroupEntry<'_>; 2] {
        [
            wgpu::BindGroupEntry {
                binding: base,
                resource: self.grid.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: base + 1,
                resource: self.data.as_entire_binding(),
            },
        ]
    }
}

pub(crate) const SHADER: &str = include_str!("lamp_lights.wgsl");

/// The lamp module at `group` from `base`: bindings `base`, `base+1` for the lamps and,
/// when `shadowed`, `base+4..base+6` for the shadow maps and their table; unshadowed
/// (the probe update) visibility comes from the same static fixture cache.
pub(crate) fn source(group: u32, base: u32, shadowed: bool) -> String {
    let mut source = format!("{}{}", include_str!("lamp_area.wgsl"), SHADER);
    if !shadowed {
        let begin = source.find("// LAMP_SHADOWS_BEGIN").expect("marker");
        let end = source.find("// LAMP_SHADOWS_END").expect("marker");
        source.replace_range(begin..end, "// LAMP_SHADOWS_BEGIN\n");
        source = source.replace(
            "let shadow = lamp_shadow(lamp/5u, world, -to, normal);",
            "let shadow = lamp_static_visibility(lamp/5u, world, normal);",
        );
    }
    {
        source = source.replace(
            "// LAMP_SHADOWS_BEGIN",
            &format!(
                "// LAMP_SHADOWS_BEGIN\n{}{}",
                include_str!("lamp_visibility_common.wgsl"),
                include_str!("lamp_visibility_sample.wgsl")
            ),
        );
        source = source.replace(
            "@group(3) @binding(19)",
            &format!(
                "@group({group}) @binding({})",
                if shadowed { base - 5 } else { 6 }
            ),
        );
    }
    for (from, to) in [(24, 0), (25, 1), (28, 4), (29, 5), (30, 6)] {
        source = source.replace(
            &format!("@group(3) @binding({from})"),
            &format!("@group({group}) @binding({})", base + to),
        );
    }
    source
}

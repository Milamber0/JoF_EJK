use super::{
    BspError, Lump, LumpKind, Shader, lump_bytes, read_f32, read_i32, records, validate_index,
    validate_range,
};
use std::ops::Range;

const DRAW_VERTEX_BYTES: usize = 80;
const SURFACE_BYTES: usize = 148;
const LIGHTMAP_BYTES: usize = 128 * 128 * 3;
const LIGHT_GRID_SAMPLE_BYTES: usize = 30;

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub minimums: [f32; 3],
    pub maximums: [f32; 3],
    pub surfaces: Range<usize>,
    pub brushes: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawVertex {
    pub position: [f32; 3],
    pub texture_coordinates: [f32; 2],
    pub lightmap_coordinates: [[f32; 2]; 4],
    pub normal: [f32; 3],
    pub colors: [[u8; 4]; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceKind {
    Planar,
    Patch,
    TriangleSoup,
    Flare,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub shader: usize,
    pub fog: Option<usize>,
    pub kind: SurfaceKind,
    pub vertices: Range<usize>,
    pub indices: Range<usize>,
    pub lightmap_styles: [u8; 4],
    pub vertex_styles: [u8; 4],
    pub lightmaps: [i32; 4],
    pub lightmap_rectangles: [[i32; 4]; 4],
    pub lightmap_origin: [f32; 3],
    pub lightmap_vectors: [[f32; 3]; 3],
    pub patch_dimensions: Option<[usize; 2]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightGridSample {
    pub ambient: [[u8; 3]; 4],
    pub directed: [[u8; 3]; 4],
    pub styles: [u8; 4],
    pub latitude_longitude: [u8; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Visibility {
    pub cluster_count: usize,
    pub bytes_per_cluster: usize,
    data: Box<[u8]>,
}

impl Visibility {
    /// Return one validated PVS row, rejecting out-of-range or overflowing indices.
    pub fn cluster(&self, index: usize) -> Option<&[u8]> {
        if index >= self.cluster_count {
            return None;
        }
        let start = index.checked_mul(self.bytes_per_cluster)?;
        self.data
            .get(start..start.checked_add(self.bytes_per_cluster)?)
    }

    /// Test a PVS bit; absent rows, short rows and invalid cluster indices are not visible.
    pub fn is_cluster_visible(&self, from: usize, to: usize) -> bool {
        let Some(row) = self.cluster(from) else {
            return false;
        };
        if to >= self.cluster_count {
            return false;
        }
        row.get(to / 8)
            .is_some_and(|byte| byte & (1 << (to & 7)) != 0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderData {
    pub(super) models: Box<[Model]>,
    pub(super) vertices: Box<[DrawVertex]>,
    pub(super) indices: Box<[u32]>,
    pub(super) surfaces: Box<[Surface]>,
    pub(super) lightmap_pixels: Box<[u8]>,
    light_grid: Box<[LightGridSample]>,
    light_grid_array: Box<[u16]>,
    visibility: Option<Visibility>,
}

impl RenderData {
    /// Render data of [`crate::Bsp::empty`]: a world model with bounds and nothing to draw.
    pub(super) fn empty(minimums: [f32; 3], maximums: [f32; 3]) -> Self {
        Self {
            models: Box::new([Model {
                minimums,
                maximums,
                surfaces: 0..0,
                brushes: 0..0,
            }]),
            vertices: Box::default(),
            indices: Box::default(),
            surfaces: Box::default(),
            lightmap_pixels: Box::default(),
            light_grid: Box::default(),
            light_grid_array: Box::default(),
            visibility: None,
        }
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// Return a non-world model by its BSP model number.
    ///
    /// Model zero is the static world and is deliberately excluded. Consumers
    /// can instance the returned generic surface range without knowing why the
    /// asset partition exists (doors and elevators are game-adapter concepts).
    pub fn inline_model(&self, index: usize) -> Option<&Model> {
        (index != 0).then(|| self.models.get(index)).flatten()
    }

    pub fn vertices(&self) -> &[DrawVertex] {
        &self.vertices
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn surfaces(&self) -> &[Surface] {
        &self.surfaces
    }

    pub fn lightmap_count(&self) -> usize {
        self.lightmap_pixels.len() / LIGHTMAP_BYTES
    }

    pub fn lightmap(&self, index: usize) -> Option<&[u8]> {
        let start = index.checked_mul(LIGHTMAP_BYTES)?;
        self.lightmap_pixels.get(start..start + LIGHTMAP_BYTES)
    }

    pub fn light_grid(&self) -> &[LightGridSample] {
        &self.light_grid
    }

    pub fn light_grid_array(&self) -> &[u16] {
        &self.light_grid_array
    }

    pub fn visibility(&self) -> Option<&Visibility> {
        self.visibility.as_ref()
    }
}

#[path = "render_parse.rs"]
mod parse;
pub(super) use parse::parse_render_data;

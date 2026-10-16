//! Trilinear light-grid sampling for entity lighting.
//!
//! The grid layout mirrors rd-vanilla `R_LoadLightGrid`
//! (`tr_bsp.cpp:1806-1845`): the origin snaps the world model's minimums up
//! to the cell size, the bounds count cells up to the snapped maximums, and
//! `R_LoadLightGridArray` (`tr_bsp.cpp:1852-1877`) rejects a grid whose
//! indirection array does not match that layout. Sampling mirrors
//! `R_SetupEntityLightingGrid` (`tr_light.cpp:134-264`): eight neighbouring
//! cells are weighted by their trilinear factor, samples inside walls
//! (style slot zero unused) are skipped and renormalised, and each cell's
//! lat/long byte pair decodes to the incoming light direction.
//!
//! The sampler is engine-generic: the cell size (a compiler setting stored in
//! the map's entity lump) and the light-style colours are supplied by the
//! caller so that no game-specific defaults live in this crate.

use super::render_data::{LightGridSample, RenderData};

/// Style byte marking an unused style slot (`LS_LSNONE`).
pub const LIGHT_STYLE_NONE: u8 = 255;

/// Cell placement of a map's light grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightGridLayout {
    origin: [f32; 3],
    inverse_size: [f32; 3],
    bounds: [i64; 3],
}

/// Light gathered at one point, on the map's 0..=255 lighting scale.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridLight {
    /// Ambient light per channel.
    pub ambient: [f32; 3],
    /// Directed light per channel.
    pub directed: [f32; 3],
    /// Unit direction the directed light arrives from (zero when unknown).
    pub direction: [f32; 3],
}

impl RenderData {
    /// Describe the light-grid placement for the given cell size.
    ///
    /// Returns `None` when the map has no grid or when the indirection array
    /// does not cover the layout, matching the reference renderer's
    /// "light grid array mismatch" fallback to grid-less entity lighting.
    pub fn light_grid_layout(&self, cell_size: [f32; 3]) -> Option<LightGridLayout> {
        let world = self.models().first()?;
        if self.light_grid().is_empty() || cell_size.iter().any(|size| *size <= 0.0) {
            return None;
        }
        let mut origin = [0.0; 3];
        let mut inverse_size = [0.0; 3];
        let mut bounds = [0; 3];
        for axis in 0..3 {
            inverse_size[axis] = 1.0 / cell_size[axis];
            origin[axis] = cell_size[axis] * (world.minimums[axis] / cell_size[axis]).ceil();
            let maximum = cell_size[axis] * (world.maximums[axis] / cell_size[axis]).floor();
            bounds[axis] = ((maximum - origin[axis]) / cell_size[axis]) as i64 + 1;
        }
        let elements = bounds[0].checked_mul(bounds[1])?.checked_mul(bounds[2])?;
        if elements <= 0 || elements as usize != self.light_grid_array().len() {
            return None;
        }
        Some(LightGridLayout {
            origin,
            inverse_size,
            bounds,
        })
    }
}

impl LightGridLayout {
    /// Reciprocal cell size used to transform world positions into grid coordinates.
    pub fn inverse_cell_size(&self) -> [f32; 3] {
        self.inverse_size
    }

    /// Number of cells along each axis.
    pub fn bounds(&self) -> [i64; 3] {
        self.bounds
    }

    /// World-space position of cell `(0, 0, 0)`.
    pub fn origin(&self) -> [f32; 3] {
        self.origin
    }

    /// Sample the grid at `point`.
    ///
    /// `style_color` maps a light-style byte to its current RGB multiplier on
    /// the 0..=255 scale; a constant white keeps every style at full strength.
    pub fn sample(
        &self,
        render: &RenderData,
        point: [f32; 3],
        style_color: impl Fn(u8) -> [f32; 3],
    ) -> GridLight {
        let mut cell = [0i64; 3];
        let mut fraction = [0.0f32; 3];
        for axis in 0..3 {
            let value = (point[axis] - self.origin[axis]) * self.inverse_size[axis];
            let floor = value.floor();
            fraction[axis] = value - floor;
            cell[axis] = (floor as i64).clamp(0, self.bounds[axis] - 1);
        }
        let step = [1, self.bounds[0], self.bounds[0] * self.bounds[1]];
        let start = cell[0] * step[0] + cell[1] * step[1] + cell[2] * step[2];
        let array = render.light_grid_array();
        let samples = render.light_grid();

        let mut light = GridLight::default();
        let mut total_factor = 0.0f32;
        for corner in 0..8u32 {
            let mut factor = 1.0f32;
            let mut index = start;
            for axis in 0..3 {
                if corner & (1 << axis) != 0 {
                    factor *= fraction[axis];
                    index += step[axis];
                } else {
                    factor *= 1.0 - fraction[axis];
                }
            }
            let Some(sample) = usize::try_from(index)
                .ok()
                .and_then(|index| array.get(index))
                .and_then(|sample| samples.get(usize::from(*sample)))
            else {
                continue;
            };
            if sample.styles[0] == LIGHT_STYLE_NONE {
                continue;
            }
            total_factor += factor;
            accumulate_styles(&mut light, sample, factor, &style_color);
            let normal = decode_direction(sample.latitude_longitude);
            for axis in 0..3 {
                light.direction[axis] += factor * normal[axis];
            }
        }
        if total_factor > 0.0 && total_factor < 0.99 {
            let scale = 1.0 / total_factor;
            light.ambient.iter_mut().for_each(|value| *value *= scale);
            light.directed.iter_mut().for_each(|value| *value *= scale);
        }
        light.direction = normalize_or_zero(light.direction);
        light
    }
}

fn accumulate_styles(
    light: &mut GridLight,
    sample: &LightGridSample,
    factor: f32,
    style_color: &impl Fn(u8) -> [f32; 3],
) {
    for slot in 0..sample.styles.len() {
        let style = sample.styles[slot];
        if style == LIGHT_STYLE_NONE {
            break;
        }
        let color = style_color(style);
        for channel in 0..3 {
            let weight = factor * color[channel] / 255.0;
            light.ambient[channel] += weight * f32::from(sample.ambient[slot][channel]);
            light.directed[channel] += weight * f32::from(sample.directed[slot][channel]);
        }
    }
}

/// Decode the stored `[longitude, latitude]` bytes exactly as the reference:
/// x = cos(lat)·sin(long), y = sin(lat)·sin(long), z = cos(long), each byte
/// spanning one full turn.
fn decode_direction(latitude_longitude: [u8; 2]) -> [f32; 3] {
    let turn = std::f32::consts::TAU / 256.0;
    let longitude = f32::from(latitude_longitude[0]) * turn;
    let latitude = f32::from(latitude_longitude[1]) * turn;
    [
        latitude.cos() * longitude.sin(),
        latitude.sin() * longitude.sin(),
        longitude.cos(),
    ]
}

fn normalize_or_zero(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length <= 0.0 {
        return [0.0; 3];
    }
    vector.map(|value| value / length)
}

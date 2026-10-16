//! Load-time composition of independently authored BSP model assets.
//! The root world's tree, visibility, entities and light grid remain its own;
//! appended model zero becomes an ordinary instance, never a second world root.
use crate::{Bsp, BspError};
use std::ops::Range;

impl Bsp {
    /// Append all models of an independently parsed asset, including model zero.
    ///
    /// Returns their contiguous model-number range. Geometry stays in its local
    /// coordinate system; callers supply instance transforms to rendering and
    /// [`Self::trace_model_box`]. Materials, lightmaps, fog and collision references
    /// are rebased together. This allocates only at asset-load time and leaves
    /// root-world queries and visibility unchanged.
    pub fn append_model_asset(&mut self, mut asset: Bsp) -> Result<Range<usize>, BspError> {
        let first = self.render.models.len();
        let end = first
            .checked_add(asset.render.models.len())
            .ok_or(BspError::InternalValidatedRangeFailure)?;
        let shader = self.shaders.len();
        let plane = self.planes.len();
        let side = self.brush_sides.len();
        let brush = self.brushes.len();
        let surface = self.render.surfaces.len();
        let vertex = self.render.vertices.len();
        let index = self.render.indices.len();
        let lightmap = i32::try_from(self.render.lightmap_count())
            .map_err(|_| BspError::InternalValidatedRangeFailure)?;
        let fog = self.fogs.len();
        // Owned vectors determine sizes. Signed render references retain their
        // actual representation bound, not a toolchain model-count ceiling.
        checked_total(&self.shaders, &asset.shaders)?;
        checked_total(&self.planes, &asset.planes)?;
        checked_total(&self.brush_sides, &asset.brush_sides)?;
        checked_total(&self.brushes, &asset.brushes)?;
        checked_total(&self.fogs, &asset.fogs)?;
        checked_total(&self.patches, &asset.patches)?;
        checked_total(&self.render.models, &asset.render.models)?;
        checked_total(&self.render.vertices, &asset.render.vertices)?;
        checked_total(&self.render.indices, &asset.render.indices)?;
        let surfaces = checked_total(&self.render.surfaces, &asset.render.surfaces)?;
        let pixels = checked_total(&self.render.lightmap_pixels, &asset.render.lightmap_pixels)?;
        i32::try_from(surfaces).map_err(|_| BspError::InternalValidatedRangeFailure)?;
        i32::try_from(pixels / (128 * 128 * 3))
            .map_err(|_| BspError::InternalValidatedRangeFailure)?;
        for model in &mut asset.render.models {
            shift(&mut model.surfaces, surface);
            shift(&mut model.brushes, brush);
        }
        for face in &mut asset.render.surfaces {
            face.shader += shader;
            face.fog = face.fog.map(|i| i + fog);
            shift(&mut face.vertices, vertex);
            shift(&mut face.indices, index);
            for map in &mut face.lightmaps {
                if *map >= 0 {
                    *map += lightmap;
                }
            }
        }
        for item in &mut asset.brush_sides {
            item.plane += plane;
            item.shader += shader;
            if item.draw_surface >= 0 {
                item.draw_surface = item
                    .draw_surface
                    .checked_add(surface as i32)
                    .ok_or(BspError::InternalValidatedRangeFailure)?;
            }
        }
        for item in &mut asset.brushes {
            shift(&mut item.sides, side);
            item.shader += shader;
        }
        for volume in &mut asset.fogs {
            volume.brush = volume.brush.map(|i| i + brush);
        }
        // Indices are surface-relative, and patch planes/facets are owned;
        // neither must be rebased. No submap nodes join the root's BSP tree.
        append(&mut self.shaders, asset.shaders);
        append(&mut self.planes, asset.planes);
        append(&mut self.brush_sides, asset.brush_sides);
        append(&mut self.brushes, asset.brushes);
        append(&mut self.fogs, asset.fogs);
        append(&mut self.patches, asset.patches);
        append(&mut self.render.models, asset.render.models);
        append(&mut self.render.surfaces, asset.render.surfaces);
        append(&mut self.render.vertices, asset.render.vertices);
        append(&mut self.render.indices, asset.render.indices);
        append(
            &mut self.render.lightmap_pixels,
            asset.render.lightmap_pixels,
        );
        Ok(first..end)
    }
}

fn checked_total<T>(left: &[T], right: &[T]) -> Result<usize, BspError> {
    let count = left
        .len()
        .checked_add(right.len())
        .ok_or(BspError::InternalValidatedRangeFailure)?;
    count
        .checked_mul(std::mem::size_of::<T>())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or(BspError::InternalValidatedRangeFailure)?;
    Ok(count)
}

fn shift(range: &mut Range<usize>, offset: usize) {
    range.start += offset;
    range.end += offset;
}

fn append<T>(to: &mut Box<[T]>, from: Box<[T]>) {
    let mut values = std::mem::take(to).into_vec();
    values.extend(from.into_vec());
    *to = values.into_boxed_slice();
}

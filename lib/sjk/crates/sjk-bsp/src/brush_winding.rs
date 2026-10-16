//! Load-time polygons of the convex brush boundaries retained in a BSP.
use super::{
    Bsp, Plane,
    patch_geometry::{base_winding, chop, flipped},
};

impl Bsp {
    /// Reconstruct a brush side by clipping its plane against the remaining sides.
    /// `side` is an absolute index in [`Self::brush_sides`]. Bevel/redundant planes
    /// may produce an empty polygon. This does not change collision geometry.
    pub fn brush_side_polygon(&self, brush: usize, side: usize) -> Vec<[f32; 3]> {
        let Some(brush) = self.brushes.get(brush) else {
            return Vec::new();
        };
        if !brush.sides.contains(&side) {
            return Vec::new();
        }
        polygon(
            self.planes[self.brush_sides[side].plane],
            brush
                .sides
                .clone()
                .filter(|&i| i != side)
                .map(|i| self.planes[self.brush_sides[i].plane]),
        )
    }
}

fn polygon(face: Plane, boundaries: impl Iterator<Item = Plane>) -> Vec<[f32; 3]> {
    let mut points = base_winding(face);
    for plane in boundaries {
        // Redundant coplanar sides describe the same boundary, not an empty
        // brush. The collision winding helper otherwise discards all-on points.
        if (0..3).all(|i| (face.normal[i] - plane.normal[i]).abs() < 1e-6)
            && (face.distance - plane.distance).abs() < 1e-4
        {
            continue;
        }
        chop(&mut points, flipped(plane));
        if points.len() < 3 {
            return Vec::new();
        }
    }
    points
}

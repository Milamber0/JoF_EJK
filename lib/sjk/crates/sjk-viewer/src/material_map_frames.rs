//! Load-time vertex frames for material maps: a tangent with handedness and the
//! light grid's incoming direction for every vertex of the flattened world.
//!
//! The tangent follows the texture's +s axis and the handedness sign makes
//! `sign * cross(normal, tangent)` follow +t, the frame rend2 shades normal maps
//! in (`CalcNormal` in `glsl/lightall.glsl`; MikkTSpace in `tr_tangentspace.cpp`
//! produces the same axes). BSP planar faces carry one affine texture mapping, so
//! every vertex of a face gets that face's exact frame; patch meshes and
//! triangle soups average the frames of the triangles around a vertex.
//!
//! The light direction mirrors rend2's `R_CalcVertexLightDirs` (`tr_bsp.cpp`) and
//! `R_LightDirForPoint` (`tr_light.cpp`): the grid direction at the vertex when it
//! lies more than a little in front of the surface, the vertex normal otherwise.
//! Retail BSPs have no deluxemaps, so this is the only directional hint for
//! purely lightmapped surfaces.

use crate::GpuVertex;
use glam::Vec3;

/// rend2's `R_LightDirForPoint` threshold on `dot(gridDirection, normal)`.
const LIGHT_FACING: f32 = 0.2;

/// One unit tangent per vertex (xyz) with handedness in w (+1 or -1); zero where
/// no triangle gives the vertex a usable texture mapping.
pub(crate) fn tangents(vertices: &[GpuVertex], indices: &[u32]) -> Vec<[f32; 4]> {
    // Per vertex, the area-weighted tangent sums of right- and left-handed triangles.
    // A mirrored seam that shares vertices keeps the dominant handedness instead of
    // cancelling to nothing. (24 bytes per vertex, released before the frames are packed.)
    let mut sums = vec![[Vec3::ZERO; 2]; vertices.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|index| index as usize);
        if a >= vertices.len() || b >= vertices.len() || c >= vertices.len() {
            continue;
        }
        let Some((s, t, area)) = triangle_axes(&vertices[a], &vertices[b], &vertices[c]) else {
            continue;
        };
        for index in [a, b, c] {
            let normal = Vec3::from_array(vertices[index].normal);
            let side = usize::from(normal.cross(s).dot(t) < 0.0);
            sums[index][side] += s * area;
        }
    }
    vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let side =
                usize::from(sums[index][1].length_squared() > sums[index][0].length_squared());
            let normal = Vec3::from_array(vertex.normal).normalize_or_zero();
            let along = sums[index][side];
            // Gram-Schmidt against the vertex normal.
            let tangent = (along - normal * normal.dot(along)).normalize_or_zero();
            if tangent == Vec3::ZERO || normal == Vec3::ZERO {
                return [0.0; 4];
            }
            let handedness = if side == 1 { -1.0 } else { 1.0 };
            [tangent.x, tangent.y, tangent.z, handedness]
        })
        .collect()
}

/// Unit +s and +t directions of one triangle in world space and its area, or `None`
/// for degenerate geometry or texture coordinates.
fn triangle_axes(a: &GpuVertex, b: &GpuVertex, c: &GpuVertex) -> Option<(Vec3, Vec3, f32)> {
    let p = [a, b, c].map(|vertex| Vec3::from_array(vertex.position));
    let uv = [a, b, c].map(|vertex| vertex.texture_coordinates);
    let edge1 = p[1] - p[0];
    let edge2 = p[2] - p[0];
    let (s1, t1) = (uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]);
    let (s2, t2) = (uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]);
    let determinant = s1 * t2 - s2 * t1;
    let area = edge1.cross(edge2).length() * 0.5;
    if !determinant.is_finite() || determinant.abs() < 1e-12 || area <= 1e-8 {
        return None;
    }
    // `R_CalcTexDirs` (`tr_main.cpp`): the world-space derivatives of position by s and t.
    let s = ((edge1 * t2 - edge2 * t1) / determinant).try_normalize()?;
    let t = ((edge2 * s1 - edge1 * s2) / determinant).try_normalize()?;
    Some((s, t, area))
}

/// The unit direction light arrives from at `vertex`, from `grid` (the light grid's
/// direction at a point, if the map has a grid).
pub(crate) fn light_direction(
    vertex: &GpuVertex,
    grid: &impl Fn([f32; 3]) -> Option<[f32; 3]>,
) -> [f32; 3] {
    let normal = Vec3::from_array(vertex.normal).normalize_or_zero();
    grid(vertex.position)
        .map(Vec3::from_array)
        .and_then(Vec3::try_normalize)
        .filter(|direction| direction.dot(normal) > LIGHT_FACING)
        .unwrap_or(normal)
        .to_array()
}

/// `pack4x8snorm` as WGSL defines it: component i in bits 8i..8i+7.
pub(crate) fn pack_snorm4x8(value: [f32; 4]) -> u32 {
    value
        .iter()
        .enumerate()
        .fold(0, |packed, (index, component)| {
            let byte = (component.clamp(-1.0, 1.0) * 127.0).round() as i8 as u8;
            packed | u32::from(byte) << (8 * index)
        })
}

/// The GPU record of each vertex (8 bytes): packed tangent and handedness, packed
/// light direction from `grid`.
pub(crate) fn pack(
    vertices: &[GpuVertex],
    tangents: &[[f32; 4]],
    grid: impl Fn([f32; 3]) -> Option<[f32; 3]>,
) -> Vec<[u32; 2]> {
    vertices
        .iter()
        .zip(tangents)
        .map(|(vertex, tangent)| {
            let [x, y, z] = light_direction(vertex, &grid);
            [pack_snorm4x8(*tangent), pack_snorm4x8([x, y, z, 0.0])]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(position: [f32; 3], normal: [f32; 3], st: [f32; 2]) -> GpuVertex {
        GpuVertex {
            position,
            normal,
            color: [1.0; 4],
            texture_coordinates: st,
            lightmap_coordinates: [0.0; 2],
        }
    }

    fn assert_frame(tangent: [f32; 4], normal: [f32; 3]) {
        let t = Vec3::new(tangent[0], tangent[1], tangent[2]);
        let n = Vec3::from_array(normal).normalize();
        assert!((t.length() - 1.0).abs() < 1e-5, "unit tangent {t}");
        assert!(t.dot(n).abs() < 1e-5, "tangent {t} orthogonal to {n}");
        assert!(tangent[3] == 1.0 || tangent[3] == -1.0);
    }

    /// A floor quad facing +z, +s along +x and +t along -y, as a Q3 floor texture.
    fn floor(mirrored: bool) -> (Vec<GpuVertex>, Vec<u32>) {
        let u = |x: f32| if mirrored { -x / 64.0 } else { x / 64.0 };
        let vertices = [[0.0, 0.0], [64.0, 0.0], [64.0, 64.0], [0.0, 64.0]]
            .map(|[x, y]| vertex([x, y, 0.0], [0.0, 0.0, 1.0], [u(x), -y / 64.0]))
            .to_vec();
        (vertices, vec![0, 1, 2, 0, 2, 3])
    }

    #[test]
    fn planar_face_frame_follows_texture_axes() {
        let (vertices, indices) = floor(false);
        for tangent in tangents(&vertices, &indices) {
            assert_frame(tangent, [0.0, 0.0, 1.0]);
            assert!((tangent[0] - 1.0).abs() < 1e-5);
            // sign * cross(n, t) must point along +t, which is -y here.
            let bitangent =
                tangent[3] * Vec3::Z.cross(Vec3::new(tangent[0], tangent[1], tangent[2]));
            assert!((bitangent - Vec3::NEG_Y).length() < 1e-5, "{bitangent}");
        }
    }

    #[test]
    fn mirrored_texture_flips_handedness_not_bitangent() {
        let (plain, indices) = floor(false);
        let (mirrored, _) = floor(true);
        let plain = tangents(&plain, &indices);
        let mirrored = tangents(&mirrored, &indices);
        for (plain, mirrored) in plain.iter().zip(&mirrored) {
            assert_frame(*mirrored, [0.0, 0.0, 1.0]);
            assert!((mirrored[0] + 1.0).abs() < 1e-5, "tangent follows -x");
            assert_eq!(mirrored[3], -plain[3]);
            let bitangent =
                mirrored[3] * Vec3::Z.cross(Vec3::new(mirrored[0], mirrored[1], mirrored[2]));
            assert!((bitangent - Vec3::NEG_Y).length() < 1e-5);
        }
    }

    #[test]
    fn curved_patch_frames_are_smooth_and_orthogonal_to_vertex_normals() {
        // A half cylinder around the z axis, s around the arc and t up it, as a
        // tessellated patch with shared smooth normals.
        let (columns, rows) = (9, 3);
        let mut vertices = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                let angle = std::f32::consts::PI * column as f32 / (columns - 1) as f32;
                let normal = [angle.cos(), angle.sin(), 0.0];
                vertices.push(vertex(
                    [normal[0] * 128.0, normal[1] * 128.0, row as f32 * 64.0],
                    normal,
                    [column as f32 / 4.0, -(row as f32)],
                ));
            }
        }
        let mut indices = Vec::new();
        for row in 0..rows - 1 {
            for column in 0..columns - 1 {
                let at = |r: u32, c: u32| r * columns + c;
                let (r, c) = (row, column);
                indices.extend([at(r, c), at(r, c + 1), at(r + 1, c + 1)]);
                indices.extend([at(r, c), at(r + 1, c + 1), at(r + 1, c)]);
            }
        }
        let frames = tangents(&vertices, &indices);
        for (vertex, tangent) in vertices.iter().zip(&frames) {
            assert_frame(*tangent, vertex.normal);
            // Along the arc, the tangent is the circle's direction of increasing angle.
            let expected = Vec3::new(-vertex.normal[1], vertex.normal[0], 0.0);
            let t = Vec3::new(tangent[0], tangent[1], tangent[2]);
            assert!(t.dot(expected) > 0.99, "tangent {t} expected {expected}");
            assert_eq!(tangent[3], frames[0][3]);
        }
    }

    #[test]
    fn degenerate_mapping_leaves_no_frame() {
        let vertices = vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            vertex([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            vertex([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
        ];
        assert_eq!(tangents(&vertices, &[0, 1, 2]), vec![[0.0; 4]; 3]);
        // Out-of-range indices are ignored, never a panic.
        assert_eq!(tangents(&vertices, &[0, 1, 7]), vec![[0.0; 4]; 3]);
    }

    #[test]
    fn light_direction_follows_grid_only_in_front_of_the_surface() {
        let vertices = vec![
            vertex([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]),
            vertex([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0; 2]),
            vertex([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0; 2]),
        ];
        let grid = |position: [f32; 3]| match position[0] as i32 {
            0 => Some([0.0, 0.6, 0.8]),
            1 => Some([0.0, 0.99, 0.1]),
            _ => None,
        };
        let directions: Vec<_> = vertices.iter().map(|v| light_direction(v, &grid)).collect();
        assert_eq!(directions[0], [0.0, 0.6, 0.8]);
        assert_eq!(directions[1], [0.0, 0.0, 1.0]);
        assert_eq!(directions[2], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn snorm_packing_matches_wgsl_layout() {
        assert_eq!(pack_snorm4x8([1.0, -1.0, 0.0, 0.5]), 0x40_00_81_7f);
        assert_eq!(pack_snorm4x8([2.0, -2.0, 0.0, 0.0]), 0x00_00_81_7f);
        let vertex = GpuVertex {
            position: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            color: [1.0; 4],
            texture_coordinates: [0.0; 2],
            lightmap_coordinates: [0.0; 2],
        };
        let packed = pack(&[vertex], &[[0.0, 1.0, 0.0, -1.0]], |_| None);
        assert_eq!(packed, vec![[0x81_00_7f_00, 0x00_7f_00_00]]);
    }
}

//! Allocation-free updates of previously allocated skinned topology.

use crate::{Glm, GlmSurface, GlmVertex, ModelError, SkinnedSurface, SkinnedVertex};

impl Glm {
    /// Update positions/normals in an output originally made by `skin_pose_matrices`.
    ///
    /// The caller must retain the same model, skin and LOD. Topology and material names
    /// are unchanged; no allocation occurs. A mismatched output is rejected.
    pub fn reskin_pose_matrices(
        &self,
        lod: usize,
        matrices: &[[[f32; 4]; 3]],
        output: &mut [SkinnedSurface],
    ) -> Result<(), ModelError> {
        if matrices.len() != self.bone_count {
            return Err(ModelError::invalid(
                matrices.len(),
                "pose matrix count differs",
            ));
        }
        let source = self
            .lods
            .get(lod)
            .ok_or_else(|| ModelError::invalid(lod, "pose LOD is absent"))?;
        let visible = || {
            source
                .surfaces
                .iter()
                .zip(&self.hierarchy)
                .filter(|(_, hierarchy)| hierarchy.flags & 0x2 == 0)
        };
        if visible().count() != output.len()
            || visible()
                .zip(output.iter())
                .any(|((surface, hierarchy), target)| {
                    target.name != hierarchy.name || target.vertices.len() != surface.vertices.len()
                })
        {
            return Err(ModelError::invalid(lod, "retained pose topology differs"));
        }
        for ((surface, _), target) in visible().zip(output) {
            for (source_vertex, target_vertex) in surface.vertices.iter().zip(&mut target.vertices)
            {
                *target_vertex = vertex(surface, source_vertex, matrices);
            }
        }
        Ok(())
    }
}

/// Evaluate one vertex with the shared render/reskin arithmetic.
pub(super) fn vertex(
    surface: &GlmSurface,
    vertex: &GlmVertex,
    matrices: &[[[f32; 4]; 3]],
) -> SkinnedVertex {
    let mut position = [0.0; 3];
    let mut normal = [0.0; 3];
    for weight in &vertex.weights {
        let global_bone = surface.bone_references[weight.bone_reference];
        let matrix = matrices[global_bone];
        for axis in 0..3 {
            position[axis] += weight.weight
                * (matrix[axis][3]
                    + (0..3)
                        .map(|component| matrix[axis][component] * vertex.position[component])
                        .sum::<f32>());
            normal[axis] += weight.weight
                * (0..3)
                    .map(|component| matrix[axis][component] * vertex.normal[component])
                    .sum::<f32>();
        }
    }
    let length = normal.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length > f32::EPSILON {
        for component in &mut normal {
            *component /= length;
        }
    }
    SkinnedVertex {
        position,
        normal,
        texture_coordinates: vertex.texture_coordinates,
    }
}

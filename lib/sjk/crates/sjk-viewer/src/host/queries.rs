//! The world queries the engine asks its renderer: mark fragments, the light at a
//! point, and whether two points share a PVS — answered from the BSP SJK loaded.

use crate::GpuState;
use sjk_bsp::{MarkFragments, MarkProjection};

/// `R_MarkFragments`: clip the polygon `points`, swept along `projection`, against
/// the world surfaces that take marks. `emit` receives every fragment's points.
pub(crate) fn mark_fragments(
    gpu: &GpuState,
    points: &[[f32; 3]],
    projection: [f32; 3],
    max_points: usize,
    max_fragments: usize,
) -> MarkFragments {
    let mut fragments = MarkFragments::with_capacity(max_points, max_fragments);
    if points.len() >= 3 && max_points > 0 && max_fragments > 0 {
        let projection = MarkProjection::new(points, projection);
        crate::decal_marks::clip_into(&gpu.decal_surfaces, &gpu.bsp, &projection, &mut fragments);
    }
    fragments
}

/// `R_LightForPoint`: the light grid at `point`, as rd-vanilla scales it, in 0..1.
/// `None` when the map has no light grid.
pub(crate) fn light_for_point(gpu: &GpuState, point: [f32; 3]) -> Option<[[f32; 3]; 3]> {
    gpu.entity_lighting.layout()?;
    let light = gpu.entity_lighting.sample(&gpu.bsp, point, &[]);
    Some([light.ambient, light.directed, light.direction])
}

/// `R_inPVS`: whether `a` and `b` lie in clusters that see each other.
pub(crate) fn in_pvs(gpu: &GpuState, a: [f32; 3], b: [f32; 3]) -> bool {
    let cluster = |point| {
        let leaf = gpu.bsp.leaf_at(point);
        gpu.bsp.leaves().get(leaf).map_or(-1, |leaf| leaf.cluster)
    };
    let (from, to) = (cluster(a), cluster(b));
    if from < 0 || to < 0 {
        return false;
    }
    gpu.bsp
        .render()
        .visibility()
        .is_none_or(|vis| vis.is_cluster_visible(from as usize, to as usize))
}

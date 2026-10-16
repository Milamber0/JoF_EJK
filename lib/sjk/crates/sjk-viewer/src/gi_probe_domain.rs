//! BSP solid/void exclusion for the established world probe lattice.
use glam::{IVec3, Vec3};
/// Installation-time BSP exclusion mask; runtime probes remain format independent.
#[derive(Default)]
pub(crate) struct Domain {
    spacing: f32,
    origin: IVec3,
    counts: [u32; 3],
    dead: Vec<bool>,
}
impl Domain {
    /// Classify the fixed lattice against sealed BSP leaves and solid brushes.
    pub(crate) fn build(bsp: &sjk_bsp::Bsp, bounds: [Vec3; 2]) -> Self {
        let (spacing, origin, counts) = super::gi_probes::placement(bounds);
        let partitioned = bsp.leaves().iter().any(|l| l.cluster >= 0);
        let mut dead = Vec::new();
        for z in 0..counts[2] {
            for y in 0..counts[1] {
                for x in 0..counts[0] {
                    let p = (origin + IVec3::new(x as i32, y as i32, z as i32)).as_vec3() * spacing;
                    dead.push(
                        partitioned
                            && (bsp.leaves()[bsp.leaf_at(p.to_array())].cluster < 0
                                || bsp.point_contents(p.to_array(), 1) != 0),
                    );
                }
            }
        }
        Self {
            spacing,
            origin,
            counts,
            dead,
        }
    }
    /// Whether this lattice position lies in sealed void or a solid brush.
    pub(crate) fn blocked(&self, p: Vec3) -> bool {
        if self.dead.is_empty() {
            return false;
        }
        let i = (p / self.spacing).round().as_ivec3() - self.origin;
        if i.cmplt(IVec3::ZERO).any()
            || i.cmpge(IVec3::from_array(self.counts.map(|x| x as i32)))
                .any()
        {
            return false;
        }
        self.dead[i.x as usize
            + (i.y as usize + i.z as usize * self.counts[1] as usize) * self.counts[0] as usize]
    }
}

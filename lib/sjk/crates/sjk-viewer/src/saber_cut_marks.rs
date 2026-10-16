//! Clipped burn strips and a short cooling glow, sharing the bounded decal store.
use super::*;

impl DecalStore {
    /// Retain the burn and fade its orange heat over 2400 ms. No queued heap geometry.
    pub(crate) fn saber_cut(
        &mut self,
        world: &DecalSurfaces,
        bsp: &Bsp,
        endpoints: [Vec3; 2],
        normal: Vec3,
        now: Instant,
        materials: &[Arc<str>; 2],
        slots: [u8; 2],
    ) {
        if !self.marks_enabled {
            return;
        }
        let time = self.frame_time(now);
        let Self { rings, scratch, .. } = self;
        crate::decal_marks::project_strip(
            world,
            bsp,
            endpoints[0],
            endpoints[1],
            normal,
            0.65,
            scratch,
            |vertices| {
                for (i, color) in [[1.; 4], [1., 0.38, 0.025, 1.]].into_iter().enumerate() {
                    let poly = alloc(rings, if i == 0 { Ring::Normal } else { Ring::Fade }, time);
                    poly.shader = Arc::clone(&materials[i]);
                    poly.slot = slots[i];
                    poly.color = color;
                    poly.count = vertices.len() as u8;
                    poly.vertices[..vertices.len()].copy_from_slice(vertices);
                    if i == 1 {
                        poly.fade_time = time + 2400;
                        poly.fade_duration = 2400;
                    }
                }
            },
        );
    }
}

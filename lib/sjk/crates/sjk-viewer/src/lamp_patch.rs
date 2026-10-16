//! Flux-weighted area and shape of one local emitting patch.
use super::*;

/// Flux and second moments accumulated from luminous quadrature samples.
pub(super) struct Patch {
    pub(super) anchor: Vec3,
    pub(super) uv: Vec2,
    pub(super) normal: Vec3,
    pub(super) luminance: f32,
    gradient: [Vec3; 2],
    u: Vec3,
    v: Vec3,
    energy: Vec3,
    weight: f64,
    first: [f64; 2],
    second: [f64; 3],
}
impl Patch {
    pub(super) fn new(
        point: Vec3,
        uv: Vec2,
        normal: Vec3,
        luminance: f32,
        gradient: [Vec3; 2],
    ) -> Self {
        let u = normal
            .cross(if normal.z.abs() < 0.9 {
                Vec3::Z
            } else {
                Vec3::Y
            })
            .normalize();
        Self {
            anchor: point,
            uv,
            normal,
            luminance,
            gradient,
            u,
            v: normal.cross(u),
            energy: Vec3::ZERO,
            weight: 0.,
            first: [0.; 2],
            second: [0.; 3],
        }
    }
    pub(super) fn texcoord(&self, point: Vec3) -> Vec2 {
        let d = point - self.anchor;
        self.uv + Vec2::new(self.gradient[0].dot(d), self.gradient[1].dot(d))
    }
    pub(super) fn mean_uv(&self) -> Vec2 {
        self.texcoord(
            self.anchor
                + self.u * (self.first[0] / self.weight) as f32
                + self.v * (self.first[1] / self.weight) as f32,
        )
    }
    pub(super) fn merged_uv(&self, uv: Vec2, weight: f32) -> Vec2 {
        self.mean_uv().lerp(
            uv,
            (f64::from(weight) / (self.weight + f64::from(weight))) as f32,
        )
    }
    pub(super) fn add(&mut self, points: [Vec3; 3], radiance: [Vec3; 3], area: f32) {
        for (point, color) in points.into_iter().zip(radiance) {
            let energy = color * (area / 3.);
            let weight = f64::from(luma(energy));
            let d = point - self.anchor;
            let p = [f64::from(d.dot(self.u)), f64::from(d.dot(self.v))];
            self.weight += weight;
            self.energy += energy;
            for i in 0..2 {
                self.first[i] += weight * p[i];
            }
            for (k, (a, b)) in [(0, 0), (0, 1), (1, 1)].into_iter().enumerate() {
                self.second[k] += weight * p[a] * p[b];
            }
        }
    }
    pub(super) fn finish(self) -> Lamp {
        let mean = self.first.map(|x| x / self.weight);
        let xx = (self.second[0] / self.weight - mean[0] * mean[0]).max(0.);
        let xy = self.second[1] / self.weight - mean[0] * mean[1];
        let yy = (self.second[2] / self.weight - mean[1] * mean[1]).max(0.);
        let angle = 0.5 * (2. * xy).atan2(xx - yy);
        let (s, c) = angle.sin_cos();
        let spread = ((xx - yy).powi(2) + 4. * xy * xy).sqrt();
        let major = (3. * (xx + yy + spread) * 0.5).sqrt().max(0.125) as f32;
        let minor = (3. * (xx + yy - spread).max(0.) * 0.5).sqrt().max(0.125) as f32;
        let axis_u = (self.u * c as f32 + self.v * s as f32) * major;
        let axis_v = (-self.u * s as f32 + self.v * c as f32) * minor;
        let power = luma(self.energy) * POWER_SCALE;
        Lamp {
            position: self.anchor + self.u * mean[0] as f32 + self.v * mean[1] as f32,
            normal: self.normal,
            color: (self.energy / luma(self.energy)).to_array(),
            power,
            radius: (power / FLOOR).sqrt().clamp(32., 1024.) + major,
            axis_u,
            axis_v,
        }
    }
}

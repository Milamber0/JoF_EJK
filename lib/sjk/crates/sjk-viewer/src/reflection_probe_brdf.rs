//! The split-sum environment BRDF table reflection probes are shaded with, computed
//! once on the CPU as rend2's `R_CreateEnvBrdfLUT` (`tr_image.cpp`, after Karis's
//! "Real Shading in Unreal Engine 4" and knarkowicz/IntegrateDFG): GGX samples on a
//! Hammersley set with height-correlated Smith visibility. Texel (x, y) holds the
//! scale and bias applied to F0 at roughness (x + 0.5)/size and N·V (y + 0.5)/size.

/// Texels per side and GGX samples per texel. rend2 uses 128² and 1024 samples; the
/// table is smooth, so 64² at 256 samples is indistinguishable after filtering and
/// costs a few milliseconds instead of a large share of a second.
pub(crate) const SIZE: u32 = 64;
pub(crate) const SAMPLES: u32 = 256;

/// rend2's `GSmithCorrelated`: height-correlated Smith visibility, roughness perceptual.
fn smith_correlated(roughness: f32, n_v: f32, n_l: f32) -> f32 {
    let m = roughness * roughness;
    let m2 = m * m;
    let visible_v = n_l * (n_v * (n_v - n_v * m2) + m2).sqrt();
    let visible_l = n_v * (n_l * (n_l - n_l * m2) + m2).sqrt();
    0.5 / (visible_v + visible_l)
}

/// (scale, bias) of F0 for one roughness and N·V.
pub(crate) fn integrate(roughness: f32, n_v: f32, samples: u32) -> [f32; 2] {
    let view = [(1. - n_v * n_v).max(0.).sqrt(), 0., n_v];
    let m = roughness * roughness;
    let m2 = m * m;
    let (mut scale, mut bias) = (0f64, 0f64);
    for i in 0..samples {
        let e1 = i as f32 / samples as f32;
        let e2 = (f64::from(i.reverse_bits()) / 4_294_967_296.) as f32;
        let phi = std::f32::consts::TAU * e1;
        let cos_theta = ((1. - e2) / (1. + (m2 - 1.) * e2)).sqrt();
        let sin_theta = (1. - cos_theta * cos_theta).max(0.).sqrt();
        let half = [sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta];
        let v_h = view[0] * half[0] + view[1] * half[1] + view[2] * half[2];
        let l_z = 2. * v_h * half[2] - view[2];
        let (n_l, n_h, v_h) = (l_z.max(0.), half[2].max(0.), v_h.max(0.));
        if n_l > 0. && n_h > 0. {
            let weighted = n_l * smith_correlated(roughness, n_v, n_l) * (4. * v_h / n_h);
            let fresnel = (1. - v_h).powi(5);
            scale += f64::from(weighted * (1. - fresnel));
            bias += f64::from(weighted * fresnel);
        }
    }
    [
        (scale / f64::from(samples)) as f32,
        (bias / f64::from(samples)) as f32,
    ]
}

/// The whole table, rows by N·V, as RGBA half floats (scale, bias, 0, 1) for upload.
pub(crate) fn table() -> Vec<[u16; 4]> {
    let mut texels = Vec::with_capacity((SIZE * SIZE) as usize);
    for y in 0..SIZE {
        let n_v = (y as f32 + 0.5) / SIZE as f32;
        for x in 0..SIZE {
            let roughness = (x as f32 + 0.5) / SIZE as f32;
            let [scale, bias] = integrate(roughness, n_v, SAMPLES);
            texels.push([half(scale), half(bias), 0, half(1.)]);
        }
    }
    texels
}

/// IEEE 754 binary16 of `value` (round to nearest even), for the RGBA16F upload.
pub(crate) fn half(value: f32) -> u16 {
    half::f16::from_f32(value).to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_match_known_encodings() {
        assert_eq!(half(0.), 0);
        assert_eq!(half(1.), 0x3c00);
        assert_eq!(half(0.5), 0x3800);
        assert_eq!(half(-2.), 0xc000);
        assert_eq!(half(65504.), 0x7bff);
        assert_eq!(half(1e6), 0x7c00);
        assert_eq!(half(f32::INFINITY), 0x7c00);
        // The smallest normal and a subnormal.
        assert_eq!(half(6.103_515_6e-5), 0x0400);
        assert_eq!(half(5.960_464_5e-8), 0x0001);
        assert_eq!(half(1e-10), 0);
        // 1 + 2^-11 lies halfway between two halves: ties to even (1.0).
        assert_eq!(half(1. + 1. / 2048.), 0x3c00);
        assert_eq!(half(1. + 3. / 2048.), 0x3c02);
    }

    #[test]
    fn table_is_an_energy_bounded_scale_and_bias() {
        for &(roughness, n_v) in &[(0.05, 0.99), (0.5, 0.5), (1.0, 0.1), (0.3, 0.02)] {
            let [scale, bias] = integrate(roughness, n_v, SAMPLES);
            // Within rounding of the f32 sample sums.
            assert!((0.0..=1.001).contains(&scale), "{roughness} {n_v}: {scale}");
            assert!((0.0..=1.001).contains(&bias), "{roughness} {n_v}: {bias}");
            assert!(scale + bias <= 1.02, "{roughness} {n_v}: {}", scale + bias);
        }
        // A smooth surface seen head-on reflects nearly everything with F0 = 1 and
        // almost only F0 itself: Fresnel adds next to nothing.
        let [scale, bias] = integrate(0.05, 0.99, SAMPLES);
        assert!(scale + bias > 0.95, "{}", scale + bias);
        assert!(bias < 0.02, "{bias}");
        // Toward grazing the Fresnel term (bias) grows.
        let grazing = integrate(0.2, 0.1, SAMPLES)[1];
        assert!(grazing > integrate(0.2, 0.9, SAMPLES)[1] + 0.1, "{grazing}");
        // Rough surfaces lose energy to masking.
        let rough = integrate(1.0, 0.5, SAMPLES);
        assert!(rough[0] + rough[1] < integrate(0.1, 0.5, SAMPLES)[0] + 0.01);
    }

    #[test]
    fn table_covers_every_texel() {
        let table = table();
        assert_eq!(table.len(), (SIZE * SIZE) as usize);
        assert!(table.iter().all(|texel| texel[3] == 0x3c00));
    }
}

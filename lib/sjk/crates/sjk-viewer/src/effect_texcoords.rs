//! Fixed affine texture-coordinate transforms for effect shader stages.

use sjk_shader::TextureModification;

pub(crate) fn compile(modifications: &[TextureModification]) -> ([f32; 2], [f32; 2]) {
    let mut scale = [1.0_f32; 2];
    let mut scroll = [0.0_f32; 2];
    for modification in modifications {
        match (
            modification.kind.as_str(),
            modification.arguments.as_slice(),
        ) {
            ("scale", [x, y, ..]) => {
                scale[0] *= x;
                scale[1] *= y;
                scroll[0] *= x;
                scroll[1] *= y;
            }
            ("scroll", [x, y, ..]) => {
                scroll[0] += x;
                scroll[1] += y;
            }
            // `transform m00 m01 m10 m11 t0 t1` without a shear: a scale, which mirrors
            // when negative (`footstep_l` is `footstep_r` turned over, `-1 0 0 1 1 0`).
            // The whole-texture offset that brings a mirror back into 0..1 is the same
            // picture once the coordinates wrap (`fract`); a shear is left out.
            ("transform", [s, shear_s, shear_t, t, ..]) if *shear_s == 0.0 && *shear_t == 0.0 => {
                scale[0] *= s;
                scale[1] *= t;
                scroll[0] *= s;
                scroll[1] *= t;
            }
            _ => {}
        }
    }
    (scale, scroll)
}

pub(crate) fn sample(scale: [f32; 2], scroll: [f32; 2], time: f32) -> [f32; 4] {
    [
        scale[0],
        scale[1],
        (scroll[0] * time).rem_euclid(1.0),
        (scroll[1] * time).rem_euclid(1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modification(kind: &str, arguments: &[f32]) -> TextureModification {
        TextureModification {
            kind: kind.to_owned(),
            arguments: arguments.to_vec(),
            wave: None,
        }
    }

    #[test]
    fn a_mirroring_transform_turns_the_picture_over() {
        let (scale, scroll) =
            compile(&[modification("transform", &[-1.0, 0.0, 0.0, 1.0, 1.0, 0.0])]);
        assert_eq!(scale, [-1.0, 1.0]);
        assert_eq!(scroll, [0.0, 0.0]);
        // A shear is not a scale: left alone.
        let (scale, _) = compile(&[modification("transform", &[1.0, 0.5, 0.0, 1.0, 0.0, 0.0])]);
        assert_eq!(scale, [1.0, 1.0]);
    }
}

//! Immutable shader-deformation bytecode for the shared world/model vertex path.
use super::{GpuStage, wave_code, wave_parameters};
use sjk_shader::Deform;

pub(crate) fn compile(stage: &mut GpuStage, deforms: &[Deform]) {
    for (i, deform) in deforms.iter().take(3).enumerate() {
        match deform {
            Deform::Wave { spread, wave } => {
                stage.deform_a[i] = [1.0, wave_code(&wave.function), *spread, 0.0];
                stage.deform_b[i] = wave_parameters(Some(wave));
            }
            Deform::Bulge {
                width,
                height,
                speed,
            } => {
                stage.deform_a[i][0] = 2.0;
                stage.deform_b[i] = [*width, *height, *speed, 0.0];
            }
            Deform::Move { direction, wave } => {
                stage.deform_a[i] = [3.0, wave_code(&wave.function), 0.0, 0.0];
                stage.deform_b[i] = wave_parameters(Some(wave));
                stage.deform_c[i] = [direction[0], direction[1], direction[2], 0.0];
            }
            Deform::Normals {
                amplitude,
                frequency,
            } => {
                stage.deform_a[i][0] = 4.0;
                stage.deform_b[i] = [*amplitude, *frequency, 0.0, 0.0];
            }
            Deform::AutoSprite => stage.deform_a[i][0] = 5.0,
            Deform::AutoSprite2 => stage.deform_a[i][0] = 6.0,
            Deform::ProjectionShadow => stage.deform_a[i][0] = 7.0,
            Deform::Text(slot) => stage.deform_a[i] = [8.0, f32::from(*slot), 0.0, 0.0],
        }
    }
}

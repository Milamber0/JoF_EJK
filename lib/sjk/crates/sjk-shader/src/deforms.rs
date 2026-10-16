//! Ordered `deformVertexes` declarations (codemp/rd-vanilla/tr_shader.cpp).
use crate::{
    ShaderError, WaveForm,
    parse::{parse_number, token},
};
use sjk_vfs::VirtualPath;

/// A shader's geometry operation, applied before texture coordinate generation.
#[derive(Clone, Debug, PartialEq)]
pub enum Deform {
    /// Move along the normal using a spatially phased waveform.
    Wave { spread: f32, wave: WaveForm },
    /// Translate along a vector using a waveform.
    Move { direction: [f32; 3], wave: WaveForm },
    /// Expand along normals, optionally modulated by texture S and time.
    Bulge { width: f32, height: f32, speed: f32 },
    /// Perturb normals using four-dimensional noise.
    Normals { amplitude: f32, frequency: f32 },
    /// Replace independent quads with camera-facing squares.
    AutoSprite,
    /// Rotate independent quads around their long axis.
    AutoSprite2,
    /// Project a model onto its shadow plane.
    ProjectionShadow,
    /// Replace a quad with text from the corresponding refdef text slot.
    Text(u8),
}

pub(super) fn parse(
    tokens: &[String],
    at: &mut usize,
    source: &VirtualPath,
) -> Result<Option<Deform>, ShaderError> {
    let kind = token(tokens, *at, source)?.to_ascii_lowercase();
    *at += 1;
    let value = match kind.as_str() {
        "wave" => {
            let divisor = parse_number(tokens, at, source)?;
            Deform::Wave {
                spread: if divisor == 0.0 { 100.0 } else { 1.0 / divisor },
                wave: waveform(tokens, at, source)?,
            }
        }
        "move" => {
            let mut direction = [0.0; 3];
            for component in &mut direction {
                *component = parse_number(tokens, at, source)?;
            }
            Deform::Move {
                direction,
                wave: waveform(tokens, at, source)?,
            }
        }
        "bulge" => Deform::Bulge {
            width: parse_number(tokens, at, source)?,
            height: parse_number(tokens, at, source)?,
            speed: parse_number(tokens, at, source)?,
        },
        "normal" => Deform::Normals {
            amplitude: parse_number(tokens, at, source)?,
            frequency: parse_number(tokens, at, source)?,
        },
        "autosprite" => Deform::AutoSprite,
        "autosprite2" => Deform::AutoSprite2,
        "projectionshadow" => Deform::ProjectionShadow,
        _ if kind.starts_with("text") => {
            let slot = kind[4..].parse::<u8>().ok().filter(|i| *i < 8).unwrap_or(0);
            Deform::Text(slot)
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn waveform(
    tokens: &[String],
    at: &mut usize,
    source: &VirtualPath,
) -> Result<WaveForm, ShaderError> {
    let function = token(tokens, *at, source)?.to_ascii_lowercase();
    *at += 1;
    Ok(WaveForm {
        function,
        base: parse_number(tokens, at, source)?,
        amplitude: parse_number(tokens, at, source)?,
        phase: parse_number(tokens, at, source)?,
        frequency: parse_number(tokens, at, source)?,
    })
}

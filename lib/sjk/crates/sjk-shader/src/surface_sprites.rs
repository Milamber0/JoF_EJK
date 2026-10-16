//! Raven surface-sprite declarations (`codemp/rd-vanilla/tr_shader.cpp`).
use super::*;

/// Generated surface geometry, independent of the underlying face's stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteKind {
    Vertical,
    Oriented,
    Effect,
    Flattened,
}

/// Authored sprite-facing mode; interpretation depends on sprite kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpriteFacing {
    #[default]
    Normal,
    Down,
    Any,
    Up,
}

/// Parameters and stock defaults for `surfaceSprites` and `ss*` directives.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSprites {
    pub kind: SpriteKind,
    pub width: f32,
    pub height: f32,
    pub density: f32,
    pub fade_distance: f32,
    pub fade_max: f32,
    pub fade_scale: f32,
    pub variance: [f32; 2],
    pub facing: SpriteFacing,
    pub wind: f32,
    pub wind_idle: f32,
    pub vertical_skew: f32,
    pub effect_duration: f32,
    pub effect_grow: [f32; 2],
    pub effect_alpha: [f32; 2],
    pub weather: bool,
}

pub(super) fn parse(
    tokens: &[String],
    cursor: &mut usize,
    source: &VirtualPath,
) -> Result<Option<SurfaceSprites>, ShaderError> {
    let kind = match token(tokens, *cursor, source)?
        .to_ascii_lowercase()
        .as_str()
    {
        "vertical" => Some(SpriteKind::Vertical),
        "oriented" => Some(SpriteKind::Oriented),
        "effect" => Some(SpriteKind::Effect),
        "flattened" => Some(SpriteKind::Flattened),
        _ => None,
    };
    *cursor += 1;
    let mut values = [0.0; 4];
    for v in &mut values {
        *v = parse_number(tokens, cursor, source)?;
    }
    let [width, height, density, fade_distance] = values;
    let Some(kind) = kind else {
        return Ok(None);
    };
    if !values.iter().all(|v| v.is_finite())
        || width <= 0.0
        || height <= 0.0
        || density <= 0.0
        || fade_distance < 32.0
    {
        return Ok(None);
    }
    Ok(Some(SurfaceSprites {
        kind,
        width,
        height,
        density,
        fade_distance,
        fade_max: fade_distance * 1.33,
        fade_scale: 0.0,
        variance: [0.0; 2],
        facing: SpriteFacing::Normal,
        wind: 0.0,
        wind_idle: 0.0,
        vertical_skew: 0.0,
        effect_duration: 1000.0,
        effect_grow: [0.0; 2],
        effect_alpha: [1.0, 0.0],
        weather: false,
    }))
}

pub(super) fn optional(
    name: &str,
    tokens: &[String],
    cursor: &mut usize,
    source: &VirtualPath,
    sprite: &mut Option<SurfaceSprites>,
) -> Result<(), ShaderError> {
    let count = match name {
        "ssvariance" | "ssfxgrow" | "ssfxalpharange" => 2,
        "sshangdown" | "ssanyangle" | "ssfaceup" | "ssfxweather" => 0,
        _ => 1,
    };
    let mut v = [0.0; 2];
    for value in v.iter_mut().take(count) {
        *value = parse_number(tokens, cursor, source)?;
    }
    let Some(s) = sprite else {
        return Ok(());
    };
    if !v.iter().all(|v| v.is_finite()) {
        return Ok(());
    }
    match name {
        "ssfademax" if v[0] > s.fade_distance => s.fade_max = v[0],
        "ssfadescale" => s.fade_scale = v[0],
        "ssvariance" if v.iter().all(|v| *v >= 0.0) => s.variance = v,
        "sshangdown" if s.facing == SpriteFacing::Normal => s.facing = SpriteFacing::Down,
        "ssanyangle" if s.facing == SpriteFacing::Normal => s.facing = SpriteFacing::Any,
        "ssfaceup" if s.facing == SpriteFacing::Normal => s.facing = SpriteFacing::Up,
        "sswind" if v[0] >= 0.0 => {
            s.wind = v[0];
            if s.wind_idle <= 0.0 {
                s.wind_idle = v[0];
            }
        }
        "sswindidle" if v[0] >= 0.0 => s.wind_idle = v[0],
        "ssvertskew" if v[0] >= 0.0 => s.vertical_skew = v[0],
        "ssfxduration" if v[0] > 0.0 => s.effect_duration = v[0],
        "ssfxgrow" if v.iter().all(|v| *v >= 0.0) => s.effect_grow = v,
        "ssfxalpharange" if v.iter().all(|v| (0.0..=1.0).contains(v)) => s.effect_alpha = v,
        "ssfxweather" => s.weather = true,
        _ => {}
    }
    Ok(())
}

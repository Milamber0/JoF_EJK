//! Presentation-only light requests from the same transformed blades used for drawing.
use crate::dynamic_lights::{PointLight, PointLightList};
use crate::saber::{Blade, Color};
use crate::saber_rgb::BladeColor;
use glam::Vec3;

/// Keep CG_DoSaber/CG_DoSaberLight source selection with a broader emissive light.
/// Flicker uses a deterministic presentation-time seed, not the simulation RNG; a blade
/// skin's light also flickers in brightness, as its file says
/// ([`crate::saber_skins::LightFlicker`]), and turns its hue with the blade's when the
/// skin's does ([`crate::saber_skins::SkinColor::light_at`]).
pub(crate) fn append(
    output: &mut PointLightList,
    blades: &[Option<Blade>; 8],
    color: BladeColor,
    blade_count: u8,
    disabled: bool,
    time: i64,
    seed: u64,
) {
    if disabled {
        return;
    }
    // A skin's light flickers (and turns its hue) as its file says; the stock colours
    // hold steady.
    let (light, gain) = match color {
        BladeColor::Skin(skin) => (skin.light_at(time), 1.8 * skin.flicker.at(time, seed)),
        _ => (rgb(color), 1.8),
    };
    let color = light.map(|c| c * gain);
    let jitter = ((time as u64)
        .wrapping_mul(1664525)
        .wrapping_add(seed.wrapping_mul(1013904223))
        & 0xffff) as f32
        / 65535.0;
    // codemp/cgame/cg_players.c:6531 selects individual lights for one/two blades.
    if blade_count < 3 {
        for blade in blades.iter().flatten().filter(|b| b.length >= 0.5) {
            output.push_radiant(PointLight {
                origin: (Vec3::from_array(blade.base)
                    + Vec3::from_array(blade.direction) * (blade.length * 0.5))
                    .to_array(),
                radius: blade.length * 2.4 + jitter * 3.0,
                color,
            });
        }
        return;
    }
    // CG_DoSaberLight, cg_players.c:5275-5355. Bounded eight-tip calculation,
    // not a sweep over all actors. Current presentation uses one colour per hilt.
    let mut tips = [Vec3::ZERO; 8];
    let mut count = 0;
    let mut radius = 0.0_f32;
    let mut midpoint = Vec3::ZERO;
    for blade in blades.iter().flatten().filter(|b| b.length >= 0.5) {
        let base = Vec3::from_array(blade.base);
        let direction = Vec3::from_array(blade.direction);
        tips[count] = base + direction * blade.length;
        midpoint = base + direction * (blade.length * 0.5);
        radius = radius.max(blade.length * 2.4);
        count += 1;
    }
    if count == 0 {
        return;
    }
    if count > 1 {
        midpoint = tips[..count].iter().copied().sum::<Vec3>() / count as f32;
        for a in &tips[..count] {
            for b in &tips[..count] {
                radius = radius.max(a.distance(*b));
            }
        }
    }
    output.push_radiant(PointLight {
        origin: midpoint.to_array(),
        radius: radius + jitter * 8.0,
        color,
    });
}

pub(crate) fn rgb(color: BladeColor) -> [f32; 3] {
    // CG_RGBForSaberColor, cg_players.c:5246-5273; not the white retail sprite tint.
    match color {
        BladeColor::Retail(Color::Red) => [1.0, 0.2, 0.2],
        BladeColor::Retail(Color::Orange) => [1.0, 0.5, 0.1],
        BladeColor::Retail(Color::Yellow) => [1.0, 1.0, 0.2],
        BladeColor::Retail(Color::Green) => [0.2, 1.0, 0.2],
        BladeColor::Retail(Color::Blue) => [0.2, 0.4, 1.0],
        BladeColor::Retail(Color::Purple) => [0.9, 0.2, 1.0],
        BladeColor::Rgb(rgb) => rgb.map(|v| f32::from(v) / 255.0),
        BladeColor::Skin(skin) => skin.light_color(),
    }
}

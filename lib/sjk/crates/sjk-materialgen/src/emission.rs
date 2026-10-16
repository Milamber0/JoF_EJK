//! Emission maps: which world textures give light, and which of their texels do.
//!
//! A light panel, a lamp or a computer screen painted into an ordinary lightmapped
//! texture shows only as bright as the light that falls on it. The client adds an
//! `<texture>_e` image unlit on top of such a surface (and, in real-time lighting,
//! turns it into a light source), so the generator writes one for textures that
//! evidently emit.
//!
//! # Which textures
//!
//! A texture needs evidence ([`Evidence`]), strongest first:
//!
//! 1. the overrides file (`emission=on`, a strength, or `emission=off` to veto);
//! 2. `q3map_surfacelight` on a shader that draws it: the map compiler lit the room
//!    from this surface (`q3map_lightRGB` only colours such a light, and
//!    `q3map_lightImage` only averages an image for its colour, so neither is
//!    evidence on its own);
//! 3. an authored glow image next to it (`<texture>_glow`, `<texture>glow`,
//!    `<texture>_glw`), the black-backed overlay retail shaders add with
//!    `blendFunc add` and `glow`;
//! 4. a fixture or screen keyword in its file name ([`KEYWORDS`], minus
//!    [`NOT_KEYWORDS`] and switched-off names);
//! 5. the BSP material `computer` (`MATERIAL_COMPUTER`);
//! 6. a control in its file name ([`PANEL_LIGHT_KEYWORDS`]: switches, door locks,
//!    keypads, control panels), whose small coloured indicator lights emit.
//!
//! A shader that already shows light on top of its diffuse pair (an additive,
//! glowing or `GL_DST_COLOR GL_ONE` stage) needs none: its texture gets no emission
//! map unless another shader draws it plainly, and the client ignores an emission
//! map on such a shader, so the light is never shown twice. A declared light fixture
//! (`q3map_surfacelight`) whose overlay does not `glow` is the exception: an additive
//! overlay at the paint's brightness, or one that only brightens what the lightmap lit
//! (`GL_DST_COLOR GL_ONE`), does not read as a lamp in a dark room or bloom, and Sol's
//! world notes asked for exactly those fixtures to emit (07/10/2026).
//!
//! # Which texels
//!
//! With an authored glow image, the emission is that image: it is what the glowing
//! shader adds. Otherwise the texture's own luminous texels are found: near-white
//! texels, or saturated ones (screen glyphs, coloured bulbs), clearly brighter than
//! most of the texture. Strong evidence (overrides, surface lights) takes any bright
//! texel at a lower threshold; weak evidence (keywords, `computer`) needs near-white
//! or saturated texels; panel lights only saturated ones, and at most
//! [`MAX_PANEL_LIGHT_COVERAGE`] of the texture (more bright colour is paint, not
//! indicator lights). A texture whose median texel is bright is a light panel: all
//! of its bright texels emit. No luminous texel, no map. The emitted colour is the
//! texel's colour, lifted so the brightest emitting part reaches about full
//! brightness (at most 3x).
//!
//! # Layout
//!
//! `<texture>_e.png` is RGB: the emitted colour, sRGB-encoded like the diffuse
//! image; black emits nothing. A colour rather than a mask times the diffuse
//! colour keeps the image self-contained (glTF's `emissiveTexture`, and the
//! `_e` images some community packs already ship): it can differ from the paint
//! (a screen's glow), the client's light extraction reads one image, and a pack
//! still works when another pack replaces the diffuse image.

use crate::filters::{Plane, percentile};
use image::{Rgb, RgbImage, RgbaImage};
use sjk_shader::{ShaderDefinition, StageBlend, TextureGenerator};

/// The emission map's name after the diffuse image's path without extension.
pub const SUFFIX: &str = "_e";

/// Names of authored glow overlays next to a texture, in lookup order.
pub const GLOW_SUFFIXES: [&str; 3] = ["_glow", "glow", "_glw"];

/// File-name keywords of light fixtures and screens (lower case, substrings).
pub const KEYWORDS: &[&str] = &[
    "light", "lamp", "bulb", "neon", "screen", "monitor", "display", "console", "computer",
    "comp_", "holo", "glow",
];

/// Names containing a keyword without naming a light.
pub const NOT_KEYWORDS: &[&str] = &[
    "lightning",
    "highlight",
    "flight",
    "lightgr",
    "slight",
    "delight",
    "lightsab",
    "clamp",
    "switch",
];

/// File-name keywords of controls whose painted indicator lights glow: switches, door
/// locks, keypads and control panels. Weaker than [`KEYWORDS`]: only small, saturated,
/// clearly bright spots emit.
pub const PANEL_LIGHT_KEYWORDS: &[&str] = &[
    "switch", "control", "onoff", "keypad", "keyport", "terminal", "button", "comm_", "locked",
];

/// Largest share of a texture that may emit on [`Evidence::PanelLights`].
pub const MAX_PANEL_LIGHT_COVERAGE: f32 = 0.15;

/// Name parts of a fixture that is switched off or broken.
pub const UNLIT: &[&str] = &["off", "broken", "dead", "unlit", "dark"];

/// Overrides strength limit (`emission=<strength>`).
pub const MAX_STRENGTH: f32 = 4.0;

/// Why a texture is taken to emit light.
#[derive(Clone, Debug, PartialEq)]
pub enum Evidence {
    /// The overrides file asks for it.
    Override,
    /// A plain shader drawing it declares `q3map_surfacelight` (the largest value).
    SurfaceLight(f32),
    /// An authored glow image next to it (its path).
    GlowImage(String),
    /// A keyword in its file name.
    Keyword(&'static str),
    /// A control's name: its indicator lights.
    PanelLights(&'static str),
    /// BSP material `computer`.
    Computer,
}

impl Evidence {
    /// Strong evidence: the texture is a light, whatever its texels look like.
    pub fn strong(&self) -> bool {
        matches!(self, Self::Override | Self::SurfaceLight(_))
    }

    /// Short description for listings and the manifest.
    pub fn describe(&self) -> String {
        match self {
            Self::Override => "overrides".to_owned(),
            Self::SurfaceLight(value) => format!("q3map_surfacelight {value}"),
            Self::GlowImage(path) => format!("glow image {path}"),
            Self::Keyword(word) => format!("keyword \"{word}\""),
            Self::PanelLights(word) => format!("panel lights (\"{word}\")"),
            Self::Computer => "bsp material computer".to_owned(),
        }
    }
}

/// What one shader drawing a texture says about its light.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShaderLight {
    /// `q3map_surfacelight`, 0 without.
    pub surface_light: f32,
    /// The shader already shows its light ([`shader_shows_light`]): it needs no
    /// emission map.
    pub glows: bool,
}

impl ShaderLight {
    pub fn of(definition: &ShaderDefinition) -> Self {
        Self {
            surface_light: if definition.surface_light.is_finite() {
                definition.surface_light.max(0.0)
            } else {
                0.0
            },
            glows: shader_shows_light(definition),
        }
    }
}

/// Whether a shader already shows its light over the diffuse pair: a `glow` stage, or
/// an additive or `GL_DST_COLOR GL_ONE` overlay ([`shows_light`]) on a shader that
/// declares no `q3map_surfacelight`. A declared fixture without a `glow` stage takes
/// an emission map. Kept in step with the client's `material_maps::shows_light`.
pub fn shader_shows_light(definition: &ShaderDefinition) -> bool {
    let fixture = definition.surface_light.is_finite() && definition.surface_light > 0.0;
    definition.stages.iter().any(|stage| {
        stage.texture_generator == TextureGenerator::Base && stage.glow
            || !fixture && shows_light(stage)
    })
}

/// A texture stage that adds light of its own: glowing, additive, or brightening the
/// destination (`GL_DST_COLOR GL_ONE`). Environment and lightmap stages do not.
pub fn shows_light(stage: &sjk_shader::ShaderStage) -> bool {
    stage.texture_generator == TextureGenerator::Base
        && (stage.glow
            || stage.blend == StageBlend::Add
            || matches!(&stage.blend, StageBlend::Custom { destination, .. }
                if destination.eq_ignore_ascii_case("gl_one")))
}

/// The decision to write an emission map, before the texels are looked at.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub evidence: Evidence,
    /// Multiplier on the emitted colour (overrides), 1 by default.
    pub strength: f32,
    /// Authored glow image whose colour is the emission.
    pub glow_image: Option<String>,
}

/// The keyword of `base` (the image path without extension), if its file name has one.
pub fn keyword(base: &str) -> Option<&'static str> {
    let name = lit_name(base)?;
    if NOT_KEYWORDS.iter().any(|word| name.contains(word)) {
        return None;
    }
    KEYWORDS.iter().copied().find(|word| name.contains(word))
}

/// The control keyword of `base` ([`PANEL_LIGHT_KEYWORDS`]), if its file name has one.
pub fn panel_light_keyword(base: &str) -> Option<&'static str> {
    let name = lit_name(base)?;
    PANEL_LIGHT_KEYWORDS
        .iter()
        .copied()
        .find(|word| name.contains(word))
}

/// The lower-case file name of `base`, unless it names something switched off or broken.
fn lit_name(base: &str) -> Option<String> {
    let name = base.rsplit('/').next().unwrap_or(base).to_ascii_lowercase();
    let digitless = |part: &str| {
        part.trim_end_matches(|c: char| c.is_ascii_digit())
            .to_owned()
    };
    let unlit = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| UNLIT.contains(&digitless(part).as_str()))
        // `onoff` is a switch's name, not a switched-off fixture.
        || (digitless(&name).ends_with("off") && !digitless(&name).ends_with("onoff"));
    (!unlit).then_some(name)
}

/// Decide whether a texture gets an emission map. `uses` are the shaders drawing it,
/// `computer` whether a BSP gives one of them `MATERIAL_COMPUTER`, `glow_image` an
/// authored glow image next to it and `strength` the overrides file's setting.
/// `Err(Some(note))` explains a refusal worth listing; `Err(None)` means no evidence.
pub fn decide(
    base: &str,
    uses: &[ShaderLight],
    computer: bool,
    glow_image: Option<&str>,
    strength: Option<f32>,
) -> Result<Plan, Option<&'static str>> {
    if strength == Some(0.0) {
        return Err(Some("emission=off in the overrides"));
    }
    if !uses.is_empty() && uses.iter().all(|light| light.glows) {
        return Err(Some("its shaders already show their light"));
    }
    let plan = |evidence| Plan {
        evidence,
        strength: strength.unwrap_or(1.0),
        glow_image: glow_image.map(str::to_owned),
    };
    if strength.is_some() {
        return Ok(plan(Evidence::Override));
    }
    let surface_light = uses
        .iter()
        .filter(|light| !light.glows)
        .map(|light| light.surface_light)
        .fold(0.0_f32, f32::max);
    if surface_light > 0.0 {
        return Ok(plan(Evidence::SurfaceLight(surface_light)));
    }
    if let Some(path) = glow_image {
        return Ok(plan(Evidence::GlowImage(path.to_owned())));
    }
    if let Some(word) = keyword(base) {
        return Ok(plan(Evidence::Keyword(word)));
    }
    if computer {
        return Ok(plan(Evidence::Computer));
    }
    if let Some(word) = panel_light_keyword(base) {
        return Ok(plan(Evidence::PanelLights(word)));
    }
    Err(None)
}

/// A generated emission map.
#[derive(Clone, Debug)]
pub struct Emission {
    pub image: RgbImage,
    /// Fraction of the texture that emits (mask-weighted).
    pub coverage: f32,
    /// Lift applied to the emitting texels' colour.
    pub gain: f32,
}

/// Least emitting fraction worth an image.
pub const MIN_COVERAGE: f32 = 0.001;
/// Median brightness (max channel) from which a texture counts as a bright panel.
pub const BRIGHT_PANEL: f32 = 0.8;
/// Most the emitting texels' colour is lifted.
pub const MAX_GAIN: f32 = 3.0;

/// Write the emission map of `source` (the diffuse image, at output size) for `plan`;
/// `glow` is the decoded authored glow image when the plan names one. The error says
/// why no map was written.
pub fn generate(
    source: &RgbaImage,
    glow: Option<&RgbaImage>,
    plan: &Plan,
) -> Result<Emission, &'static str> {
    let (width, height) = source.dimensions();
    let strength = plan.strength.clamp(0.0, MAX_STRENGTH);
    if let Some(glow) = glow {
        let mut emitting = 0usize;
        let image = RgbImage::from_fn(width, height, |x, y| {
            let colour = bilinear(
                glow,
                (x as f32 + 0.5) / width as f32,
                (y as f32 + 0.5) / height as f32,
            );
            let pixel = colour.map(|c| encode(decode(c) * strength));
            if pixel.iter().any(|c| *c > 5) {
                emitting += 1;
            }
            Rgb(pixel)
        });
        let coverage = emitting as f32 / (width as f32 * height as f32).max(1.0);
        if coverage < MIN_COVERAGE {
            return Err("the glow image is black");
        }
        return Ok(Emission {
            image,
            coverage,
            gain: 1.0,
        });
    }
    let (w, h) = (width as usize, height as usize);
    let texel = |x: usize, y: usize| source.get_pixel(x as u32, y as u32).0;
    let value = Plane::from_fn(w, h, |x, y| {
        let [r, g, b, _] = texel(x, y);
        f32::from(r.max(g).max(b)) / 255.0
    });
    let median = percentile(&value, None, 0.5);
    let strong = plan.evidence.strong();
    let indicators = matches!(plan.evidence, Evidence::PanelLights(_));
    // Lit parts stand out from the rest of the texture; a texture that is mostly bright
    // is a light panel, all of whose bright texels emit (a low, absolute threshold, so
    // texture noise on the panel does not speckle its light).
    let panel = median >= BRIGHT_PANEL;
    let low = match (strong, panel) {
        (_, true) => 0.55,
        (true, false) => (median + 0.1).clamp(0.45, 0.85),
        (false, false) => (median + 0.2).clamp(0.6, 0.9),
    };
    let white_low = if panel { low } else { low.max(0.75) };
    let mask = Plane::from_fn(w, h, |x, y| {
        let [r, g, b, _] = texel(x, y);
        let max = f32::from(r.max(g).max(b)) / 255.0;
        let min = f32::from(r.min(g).min(b)) / 255.0;
        let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };
        let bright = smoothstep(low, low + 0.15, max);
        if strong {
            return bright;
        }
        // Near-white texels must be clearly bright; saturated ones (glyphs, coloured
        // bulbs) need only stand out. Pale, half-saturated paint is neither.
        let white = smoothstep(white_low, white_low + 0.15, max)
            * (1.0 - smoothstep(0.25, 0.45, saturation));
        let colour = bright * smoothstep(0.35, 0.55, saturation);
        // Indicator lights are coloured: white on a control is paint or a label.
        if indicators {
            colour
        } else {
            white.max(colour)
        }
    });
    let coverage = mask.data.iter().sum::<f32>() / mask.data.len().max(1) as f32;
    if coverage < MIN_COVERAGE {
        return Err("no luminous texels");
    }
    if indicators && coverage > MAX_PANEL_LIGHT_COVERAGE {
        return Err("too much bright colour for indicator lights");
    }
    let linear = |x: usize, y: usize| {
        let [r, g, b, _] = texel(x, y);
        [r, g, b].map(|c| decode(f32::from(c) / 255.0))
    };
    let luminance = Plane::from_fn(w, h, |x, y| {
        let [r, g, b] = linear(x, y);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    });
    let peak = percentile(&luminance, Some(&mask), 0.95);
    let gain = (1.0 / peak.max(1e-3)).clamp(1.0, MAX_GAIN);
    let image = RgbImage::from_fn(width, height, |x, y| {
        let (x, y) = (x as usize, y as usize);
        let weight = mask.at(x, y) * gain * strength;
        Rgb(linear(x, y).map(|c| encode(c * weight)))
    });
    Ok(Emission {
        image,
        coverage,
        gain,
    })
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// sRGB 0–1 to linear.
fn decode(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear to an sRGB byte, clamped.
fn encode(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let srgb = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

/// `image` at normalised (`u`, `v`), bilinear and wrapping like a tiling texture, as
/// 0–1 sRGB values.
fn bilinear(image: &RgbaImage, u: f32, v: f32) -> [f32; 3] {
    let (width, height) = (image.width() as i64, image.height() as i64);
    let x = u * width as f32 - 0.5;
    let y = v * height as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |dx: i64, dy: i64| {
        let px = (x0 as i64 + dx).rem_euclid(width) as u32;
        let py = (y0 as i64 + dy).rem_euclid(height) as u32;
        image.get_pixel(px, py).0
    };
    std::array::from_fn(|c| {
        let sample = |p: [u8; 4]| f32::from(p[c]) / 255.0;
        let top = sample(at(0, 0)) * (1.0 - fx) + sample(at(1, 0)) * fx;
        let bottom = sample(at(0, 1)) * (1.0 - fx) + sample(at(1, 1)) * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn plan(evidence: Evidence) -> Plan {
        Plan {
            evidence,
            strength: 1.0,
            glow_image: None,
        }
    }

    /// A 64x64 dark housing (grey 60) with a 16x8 lamp of `lamp` at (24, 28).
    fn housing(lamp: [u8; 3]) -> RgbaImage {
        RgbaImage::from_fn(64, 64, |x, y| {
            if (24..40).contains(&x) && (28..36).contains(&y) {
                Rgba([lamp[0], lamp[1], lamp[2], 255])
            } else {
                Rgba([60, 62, 58, 255])
            }
        })
    }

    #[test]
    fn keywords_name_fixtures_and_screens_not_lookalikes() {
        assert_eq!(keyword("textures/kejim/lightpanel"), Some("light"));
        assert_eq!(keyword("textures/taspir/lamp2_bulb"), Some("lamp"));
        assert_eq!(keyword("textures/doomgiver/comp_panel3"), Some("comp_"));
        assert_eq!(keyword("textures/danger/monitor02"), Some("monitor"));
        assert_eq!(keyword("textures/rooftop/neon_tall"), Some("neon"));
        // Lookalikes and directories do not count.
        assert_eq!(keyword("textures/cairn/lightning3"), None);
        assert_eq!(keyword("textures/x/highlight_trim"), None);
        assert_eq!(keyword("textures/rail/compartments"), None);
        assert_eq!(keyword("textures/lights/wall"), None);
        assert_eq!(keyword("textures/cairn/basic_lightgray"), None);
        assert_eq!(keyword("models/weapons2/saber/w_lightsab1"), None);
        assert_eq!(keyword("textures/wedge/trim_clamp"), None);
        assert_eq!(keyword("textures/sith_council/light_switch"), None);
        // Switched off or broken fixtures.
        assert_eq!(keyword("textures/x/light_off"), None);
        assert_eq!(keyword("textures/x/lightsoff"), None);
        assert_eq!(keyword("textures/x/screen_broken2"), None);
        assert_eq!(keyword("textures/x/wall"), None);
    }

    #[test]
    fn controls_have_panel_lights_unless_switched_off() {
        assert_eq!(
            panel_light_keyword("textures/kejim/switch3"),
            Some("switch")
        );
        assert_eq!(
            panel_light_keyword("textures/imperial/switch_door_unlocked"),
            Some("switch")
        );
        assert_eq!(
            panel_light_keyword("textures/x/door_1new_onoff"),
            Some("onoff")
        );
        assert_eq!(
            panel_light_keyword("textures/x/h_control_metal"),
            Some("control")
        );
        assert_eq!(panel_light_keyword("textures/kejim/lift_off"), None);
        assert_eq!(panel_light_keyword("textures/x/switch_broken"), None);
        assert_eq!(panel_light_keyword("textures/x/wall_blocks"), None);
        // A light name is the stronger evidence; a plain wall none.
        assert_eq!(
            decide("textures/x/light_switch", &[], false, None, None).map(|p| p.evidence),
            Ok(Evidence::PanelLights("switch"))
        );
        assert_eq!(
            decide("textures/x/keypad_light", &[], false, None, None).map(|p| p.evidence),
            Ok(Evidence::Keyword("light"))
        );
    }

    #[test]
    fn panel_lights_are_small_coloured_spots() {
        let panel = plan(Evidence::PanelLights("switch"));
        // A red indicator on a grey panel emits, only there.
        let emission = generate(&housing([230, 30, 20]), None, &panel).expect("red light");
        assert!(emission.image.get_pixel(30, 30).0[0] > 100);
        assert_eq!(emission.image.get_pixel(4, 4).0, [0, 0, 0]);
        // A white label is paint, not a light.
        assert_eq!(
            generate(&housing([250, 250, 250]), None, &panel).map(|e| e.coverage),
            Err("no luminous texels")
        );
        // A panel painted bright red all over is not an indicator.
        let red = RgbaImage::from_fn(64, 64, |x, _| {
            if x < 32 {
                Rgba([230, 30, 20, 255])
            } else {
                Rgba([60, 62, 58, 255])
            }
        });
        assert_eq!(
            generate(&red, None, &panel).map(|e| e.coverage),
            Err("too much bright colour for indicator lights")
        );
    }

    #[test]
    fn evidence_order_and_vetoes() {
        let plain = ShaderLight::default();
        let lit = ShaderLight {
            surface_light: 3000.0,
            glows: false,
        };
        let glowing = ShaderLight {
            surface_light: 1500.0,
            glows: true,
        };
        let wall = "textures/a/wall";
        assert_eq!(decide(wall, &[plain], false, None, None), Err(None));
        assert_eq!(
            decide(wall, &[plain, lit], false, None, None).map(|p| p.evidence),
            Ok(Evidence::SurfaceLight(3000.0))
        );
        // A surface light whose shader already glows is not evidence for the plain use.
        assert_eq!(
            decide(wall, &[plain, glowing], false, None, None),
            Err(None)
        );
        assert_eq!(
            decide(wall, &[glowing], false, None, Some(1.0)),
            Err(Some("its shaders already show their light"))
        );
        let glow = decide(
            wall,
            &[plain],
            false,
            Some("textures/a/wall_glow.jpg"),
            None,
        )
        .expect("glow image");
        assert_eq!(
            glow.evidence,
            Evidence::GlowImage("textures/a/wall_glow.jpg".into())
        );
        assert_eq!(glow.glow_image.as_deref(), Some("textures/a/wall_glow.jpg"));
        assert_eq!(
            decide("textures/a/lightpanel", &[plain], false, None, None).map(|p| p.evidence),
            Ok(Evidence::Keyword("light"))
        );
        assert_eq!(
            decide(wall, &[plain], true, None, None).map(|p| p.evidence),
            Ok(Evidence::Computer)
        );
        // The overrides come first, both ways.
        let forced = decide(wall, &[plain], false, None, Some(2.0)).expect("forced");
        assert_eq!(
            (forced.evidence, forced.strength),
            (Evidence::Override, 2.0)
        );
        assert_eq!(
            decide("textures/a/lightpanel", &[lit], false, None, Some(0.0)),
            Err(Some("emission=off in the overrides"))
        );
    }

    #[test]
    fn shader_light_sees_overlays_but_not_reflections() {
        let shader = |script: &str| {
            ShaderLight::of(
                &sjk_shader::parse_shader_script(script.as_bytes(), "s.shader").expect("parses")[0],
            )
        };
        let plain =
            shader("t/a { q3map_surfacelight 500 { map $lightmap } { map t/a blendFunc filter } }");
        assert_eq!((plain.surface_light, plain.glows), (500.0, false));
        for overlay in [
            "{ map t/a_glow blendFunc add }",
            "{ map t/a_glow blendFunc GL_ONE GL_ONE glow }",
            "{ map t/a_blend blendFunc GL_DST_COLOR GL_ONE }",
            "{ map t/a_glow blendFunc blend glow }",
        ] {
            let script =
                format!("t/a {{ {{ map $lightmap }} {{ map t/a blendFunc filter }} {overlay} }}");
            assert!(shader(&script).glows, "{overlay}");
        }
        let chrome = shader(
            "t/a { { map $lightmap } { map t/a blendFunc filter } \
             { map t/env tcGen environment blendFunc add } }",
        );
        assert!(!chrome.glows);
        // A declared fixture whose overlay does not glow still takes a map (retail's
        // `mp/s_ylight_red`, `mp/s_squareslight_y`); one with a `glow` stage does not.
        for overlay in [
            "{ map t/a_blend blendFunc GL_ONE GL_ONE rgbGen wave sin 0.5 0.05 0 10 }",
            "{ map t/a_blend blendFunc GL_DST_COLOR GL_ONE }",
        ] {
            let script = format!(
                "t/a {{ q3map_surfacelight 3000 {{ map $lightmap }} \
                 {{ map t/a blendFunc filter }} {overlay} }}"
            );
            assert!(!shader(&script).glows, "{overlay}");
        }
        let glowing_fixture = shader(
            "t/a { q3map_surfacelight 3000 { map $lightmap } { map t/a blendFunc filter } \
             { map t/a_glw blendFunc GL_ONE GL_ONE glow } }",
        );
        assert!(glowing_fixture.glows);
    }

    #[test]
    fn a_white_lamp_in_dark_housing_emits_only_the_lamp() {
        let emission = generate(
            &housing([250, 248, 240]),
            None,
            &plan(Evidence::Keyword("lamp")),
        )
        .expect("emits");
        let lamp_fraction = (16.0 * 8.0) / (64.0 * 64.0);
        assert!(
            (emission.coverage - lamp_fraction).abs() < 0.01,
            "{}",
            emission.coverage
        );
        assert_eq!(emission.image.get_pixel(2, 2).0, [0, 0, 0]);
        let lit = emission.image.get_pixel(30, 30).0;
        assert!(lit.iter().all(|c| *c >= 240), "{lit:?}");
    }

    #[test]
    fn screen_glyphs_emit_and_pale_paint_does_not() {
        // A dark screen with saturated green glyphs in every fourth column.
        let screen = RgbaImage::from_fn(32, 32, |x, _| {
            if x % 4 == 0 {
                Rgba([40, 230, 60, 255])
            } else {
                Rgba([10, 20, 15, 255])
            }
        });
        let emission =
            generate(&screen, None, &plan(Evidence::Keyword("screen"))).expect("glyphs emit");
        assert!((emission.coverage - 0.25).abs() < 0.01);
        let glyph = emission.image.get_pixel(0, 5).0;
        assert!(glyph[1] > 200 && glyph[0] < glyph[1], "{glyph:?}");
        assert_eq!(emission.image.get_pixel(1, 5).0, [0, 0, 0]);
        // A grey metal track with pale highlights (light grey, low saturation, 0.7) is
        // not a light, even with a keyword.
        let track = housing([180, 175, 170]);
        assert_eq!(
            generate(&track, None, &plan(Evidence::Keyword("light"))).map(|e| e.coverage),
            Err("no luminous texels")
        );
    }

    #[test]
    fn bright_panels_emit_whole_and_pale_walls_only_their_white() {
        // Beige paint (median 0.78) with a near-white stripe: only the stripe emits.
        let paint = RgbaImage::from_fn(16, 16, |x, _| {
            if x < 10 {
                Rgba([200, 190, 170, 255])
            } else {
                Rgba([250, 250, 245, 255])
            }
        });
        let stripe = generate(&paint, None, &plan(Evidence::Keyword("light"))).expect("stripe");
        assert!(stripe.coverage > 0.05 && stripe.coverage < 6.0 / 16.0);
        assert_eq!(stripe.image.get_pixel(2, 2).0, [0, 0, 0]);
        // A uniformly bright panel is a panel light under either evidence.
        let white = RgbaImage::from_pixel(16, 16, Rgba([235, 235, 235, 255]));
        for evidence in [Evidence::Keyword("light"), Evidence::SurfaceLight(3000.0)] {
            let panel = generate(&white, None, &plan(evidence)).expect("panel");
            assert!(panel.coverage > 0.9, "{}", panel.coverage);
        }
        // Texture noise on a bright panel does not speckle its light.
        let noisy = RgbaImage::from_fn(32, 32, |x, y| {
            let v = 205 + ((x * 7 + y * 13) % 41) as u8;
            Rgba([v, v, v.saturating_add(5), 255])
        });
        let panel = generate(&noisy, None, &plan(Evidence::Keyword("light"))).expect("panel");
        assert!(panel.coverage > 0.99, "{}", panel.coverage);
        // A dark wall with a surface light and nothing bright has nothing to show.
        let dark = RgbaImage::from_pixel(16, 16, Rgba([40, 40, 40, 255]));
        assert_eq!(
            generate(&dark, None, &plan(Evidence::SurfaceLight(300.0))).map(|e| e.coverage),
            Err("no luminous texels")
        );
    }

    #[test]
    fn strong_evidence_takes_dimmer_lamps_than_names_do() {
        // A grey lamp (0.59) in grey housing (0.24): a surface light, not a name.
        let dim = housing([150, 150, 150]);
        assert!(generate(&dim, None, &plan(Evidence::SurfaceLight(500.0))).is_ok());
        assert_eq!(
            generate(&dim, None, &plan(Evidence::Keyword("lamp"))).map(|e| e.coverage),
            Err("no luminous texels")
        );
    }

    #[test]
    fn dim_lamps_are_lifted_and_strength_scales() {
        let dim = housing([150, 150, 150]);
        // Strong evidence finds the dim lamp (0.59 against a 0.24 median) and lifts it.
        let emission = generate(&dim, None, &plan(Evidence::Override)).expect("lamp");
        assert!(emission.gain > 1.0 && emission.gain <= MAX_GAIN);
        let lifted = emission.image.get_pixel(30, 30).0[0];
        assert!(lifted > 150, "{lifted}");
        let half = generate(
            &dim,
            None,
            &Plan {
                strength: 0.5,
                ..plan(Evidence::Override)
            },
        )
        .expect("lamp");
        assert!(half.image.get_pixel(30, 30).0[0] < lifted);
    }

    #[test]
    fn glow_images_are_the_emission_at_the_diffuse_size() {
        let glow = RgbaImage::from_fn(8, 8, |x, y| {
            if x >= 4 && y >= 4 {
                Rgba([255, 128, 0, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let source = RgbaImage::from_pixel(16, 16, Rgba([90, 90, 90, 255]));
        let emission = generate(
            &source,
            Some(&glow),
            &plan(Evidence::GlowImage("t/a_glow".into())),
        )
        .expect("glow");
        assert_eq!(emission.image.dimensions(), (16, 16));
        assert_eq!(emission.image.get_pixel(12, 12).0, [255, 128, 0]);
        assert_eq!(emission.image.get_pixel(3, 3).0, [0, 0, 0]);
        // Bilinear edges (and the wrap at the left and top) add a partial border.
        assert!(
            emission.coverage > 0.2 && emission.coverage < 0.45,
            "{}",
            emission.coverage
        );
        let black = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]));
        assert_eq!(
            generate(&source, Some(&black), &plan(Evidence::Override)).map(|e| e.coverage),
            Err("the glow image is black")
        );
    }

    #[test]
    fn colour_conversion_round_trips() {
        for byte in [0u8, 1, 10, 64, 128, 200, 255] {
            assert_eq!(encode(decode(f32::from(byte) / 255.0)), byte);
        }
    }
}

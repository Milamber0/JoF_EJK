//! Finding a stage's material maps and converting them into the two layouts the
//! material program reads.
//!
//! Lookup follows rend2's `ParseStage` keywords and its automatic search next to
//! the diffuse image in `CollapseStagesToGLSL` (`codemp/rd-rend2/tr_shader.cpp`):
//! `<diffuse>_nh` (normal plus height) before `<diffuse>_n`, and `_specGloss`
//! before the packed `_rmo` and `_orm`. ioquake3's rend2, which OpenJK's grew
//! from, named specular maps `_s`; older packs use it, so it is tried after
//! `_specGloss` with the same meaning. SJK's emission map is `<diffuse>_e`, an
//! sRGB colour like the diffuse image (rend2 has no emission map or keyword).
//!
//! Conversion happens once at load. Normal maps keep RGB and turn their height
//! into depth (`255 - alpha`), as rend2 does on upload. Gloss maps get rend2's
//! SDR colour ratio (`R_BuildSDRSpecGlossImage`, `tr_image.cpp`). Packed maps are
//! reordered to occlusion, roughness, metalness, specular — the order rend2's
//! texture swizzle produces (`R_LoadPackedMaterialImage`) — so one shader path
//! reads every packed layout.

use super::{MapImage, Settings};
use crate::decoded_image_cache::cached_decoded_image;
use image::RgbaImage;
use sjk_shader::{
    DEFAULT_NORMAL_SCALE, SPEC_GLOSS_SCALE, ShaderCatalog, ShaderStage, SpecularLayout,
    packed_specular_scale,
};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

/// Specular layout codes of the material program (`MaterialMapParams.control.y`).
pub(super) const SPECULAR_GLOSS: u32 = 1;
pub(super) const SPECULAR_PACKED: u32 = 2;

/// Automatic normal-map names, in rend2's order, and whether alpha holds height.
const NORMAL_SUFFIXES: [(&str, bool); 2] = [("_nh", true), ("_n", false)];
/// Automatic specular names: rend2's, with ioquake3's `_s` after `_specGloss`.
const SPECULAR_SUFFIXES: [(&str, SpecularLayout); 4] = [
    ("_specGloss", SpecularLayout::SpecGloss),
    ("_s", SpecularLayout::SpecGloss),
    ("_rmo", SpecularLayout::Rmo),
    ("_orm", SpecularLayout::Orm),
];

/// The emission map's name after the diffuse image's path (`sjk-materialgen` writes it).
pub(super) const EMISSION_SUFFIX: &str = "_e";

/// What one stage's lookup found, with rend2's scales for how each map was found.
pub(super) struct Found {
    pub(super) normal: Option<MapImage>,
    /// The normal map's alpha is height (now stored as depth).
    pub(super) height: bool,
    pub(super) normal_scale: [f32; 4],
    /// The converted map and its layout code.
    pub(super) specular: Option<(MapImage, u32)>,
    pub(super) specular_scale: [f32; 4],
    pub(super) parallax_bias: f32,
    /// The emission map: the emitted colour, sRGB, alpha unused.
    pub(super) emission: Option<MapImage>,
}

/// Resolve the maps of `diffuse`, the stage whose texture they belong to.
pub(super) fn find(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    settings: Settings,
    diffuse: &ShaderStage,
    implicit_name: &str,
    cache: &mut HashMap<String, Arc<RgbaImage>>,
) -> Result<Found, Box<dyn Error>> {
    let material = &diffuse.material;
    let base = diffuse_base(vfs, shaders, diffuse, implicit_name)?;
    let exists = |name: &str| -> Result<Option<String>, Box<dyn Error>> {
        Ok(shaders
            .resolve_stage_image(vfs, name)?
            .map(|path| path.as_str().to_owned()))
    };
    let mut found = Found {
        normal: None,
        height: false,
        normal_scale: material.normal_scale,
        specular: None,
        specular_scale: material.specular_scale,
        parallax_bias: material.parallax_bias,
        emission: None,
    };
    if settings.normal {
        let choice = match (&material.normal_map, &base) {
            (Some(name), _) => exists(name)?.map(|path| (path, material.normal_height, false)),
            (None, Some(base)) => {
                auto(&NORMAL_SUFFIXES, base, &exists)?.map(|(path, height)| (path, height, true))
            }
            (None, None) => None,
        };
        if let Some((path, height, automatic)) = choice {
            if let Some(image) = load(vfs, &path, "normal", cache, |pixels| {
                convert_normal(pixels, height)
            })? {
                found.normal = Some(image);
                found.height = height;
                if automatic {
                    found.normal_scale = DEFAULT_NORMAL_SCALE;
                }
            }
        }
    }
    if settings.specular {
        let choice = match (&material.specular_map, &base) {
            (Some(name), _) if is_white(name) => Some((None, material.specular_layout, false)),
            (Some(name), _) => {
                exists(name)?.map(|path| (Some(path), material.specular_layout, false))
            }
            (None, Some(base)) => auto(&SPECULAR_SUFFIXES, base, &exists)?
                .map(|(path, layout)| (Some(path), layout, true)),
            (None, None) => None,
        };
        if let Some((path, layout, automatic)) = choice {
            let code = if layout == SpecularLayout::SpecGloss {
                SPECULAR_GLOSS
            } else {
                SPECULAR_PACKED
            };
            let image = match path {
                None => Some(MapImage {
                    key: "material:$whiteimage".into(),
                    pixels: Arc::new(RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]))),
                }),
                Some(path) => load(vfs, &path, layout_name(layout), cache, |pixels| {
                    convert_specular(pixels, layout)
                })?,
            };
            if let Some(image) = image {
                found.specular = Some((image, code));
                if automatic {
                    found.specular_scale = if code == SPECULAR_GLOSS {
                        SPEC_GLOSS_SCALE
                    } else {
                        packed_specular_scale()
                    };
                }
            }
        }
    }
    if settings.emission
        && let Some(base) = &base
        && let Some(path) = exists(&format!("{base}{EMISSION_SUFFIX}"))?
    {
        found.emission = load(vfs, &path, "emission", cache, convert_emission)?;
    }
    Ok(found)
}

/// The emitted colour stays as authored (sRGB); alpha means nothing.
pub(super) fn convert_emission(source: &RgbaImage) -> RgbaImage {
    let mut pixels = source.clone();
    for pixel in pixels.pixels_mut() {
        pixel.0[3] = 255;
    }
    pixels
}

fn is_white(name: &str) -> bool {
    name.eq_ignore_ascii_case("$whiteimage") || name.eq_ignore_ascii_case("*white")
}

/// The first suffix of `suffixes` whose image exists next to `base`.
fn auto<T: Copy>(
    suffixes: &[(&str, T)],
    base: &str,
    exists: &impl Fn(&str) -> Result<Option<String>, Box<dyn Error>>,
) -> Result<Option<(String, T)>, Box<dyn Error>> {
    for (suffix, value) in suffixes {
        if let Some(path) = exists(&format!("{base}{suffix}"))? {
            return Ok(Some((path, *value)));
        }
    }
    Ok(None)
}

/// The diffuse image's resolved path without its extension, as rend2 strips
/// `diffuseImg->imgName`. Generated images (`$whiteimage`) have no maps.
fn diffuse_base(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    diffuse: &ShaderStage,
    implicit_name: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let path = match diffuse.images.first() {
        None => shaders.resolve_image(vfs, implicit_name)?,
        Some(name) if name.starts_with('$') || name.starts_with('*') => return Ok(None),
        Some(name) => shaders.resolve_stage_image(vfs, name)?,
    };
    Ok(path.map(|path| {
        let path = path.as_str();
        path.rsplit_once('.')
            .filter(|(_, extension)| !extension.contains('/'))
            .map_or(path, |(stem, _)| stem)
            .to_owned()
    }))
}

fn layout_name(layout: SpecularLayout) -> &'static str {
    match layout {
        SpecularLayout::None | SpecularLayout::SpecGloss => "gloss",
        SpecularLayout::Rmo => "rmo",
        SpecularLayout::Moxr => "moxr",
        SpecularLayout::Orm => "orm",
    }
}

/// Decode and convert one map once per load worker; the key names the conversion.
fn load(
    vfs: &VirtualFileSystem,
    path: &str,
    kind: &str,
    cache: &mut HashMap<String, Arc<RgbaImage>>,
    convert: impl FnOnce(&RgbaImage) -> RgbaImage,
) -> Result<Option<MapImage>, Box<dyn Error>> {
    let key = format!("material:{kind}:{}", path.to_ascii_lowercase());
    if let Some(pixels) = cache.get(&key) {
        return Ok(Some(MapImage {
            key,
            pixels: Arc::clone(pixels),
        }));
    }
    let Some(decoded) = cached_decoded_image(vfs, path)? else {
        return Ok(None);
    };
    let pixels = Arc::new(convert(&decoded));
    cache.insert(key.clone(), Arc::clone(&pixels));
    Ok(Some(MapImage { key, pixels }))
}

/// RGB stays the encoded normal; alpha becomes depth (`255 - height`) for parallax,
/// or opaque when the map carries no height.
pub(super) fn convert_normal(source: &RgbaImage, height: bool) -> RgbaImage {
    let mut pixels = source.clone();
    for pixel in pixels.pixels_mut() {
        pixel.0[3] = if height { 255 - pixel.0[3] } else { 255 };
    }
    pixels
}

/// Gloss maps get rend2's SDR ratio; packed maps are reordered to occlusion,
/// roughness, metalness, specular.
pub(super) fn convert_specular(source: &RgbaImage, layout: SpecularLayout) -> RgbaImage {
    let mut pixels = source.clone();
    for pixel in pixels.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        pixel.0 = match layout {
            SpecularLayout::None | SpecularLayout::SpecGloss => sdr_spec_gloss([r, g, b, a]),
            SpecularLayout::Rmo => [b, r, g, 255],
            SpecularLayout::Moxr => [g, a, r, 255],
            SpecularLayout::Orm => [r, g, b, a],
        };
    }
    pixels
}

/// rend2 `R_BuildSDRSpecGlossImage`: scale the colour by the ratio of its linear to
/// its encoded sum. rend2 counts green twice and blue never; packs were tuned to it.
fn sdr_spec_gloss([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let color = [r, g, b].map(|channel| f32::from(channel) / 255.0);
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let sum = color[0] + color[1] + color[2];
    let ratio = if sum > 0.0 {
        (linear(color[0]) + linear(color[1]) + linear(color[1])) / sum
    } else {
        0.0
    };
    let byte = |c: f32| (c * ratio * 255.0).clamp(0.0, 255.0) as u8;
    [byte(color[0]), byte(color[1]), byte(color[2]), a]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_shader::parse_shader_script;

    /// A synthetic 2x2 image encoded in the format its name's extension asks for.
    fn encoded(name: &str, color: [u8; 4]) -> Vec<u8> {
        let format = image::ImageFormat::from_path(name).expect("image extension");
        let pixels =
            image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, image::Rgba(color)));
        // JPEG has no alpha channel.
        let pixels = if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(pixels.to_rgb8())
        } else {
            pixels
        };
        let mut bytes = Vec::new();
        pixels
            .write_to(&mut std::io::Cursor::new(&mut bytes), format)
            .expect("encode synthetic image");
        bytes
    }

    fn vfs(files: &[&str]) -> VirtualFileSystem {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "synthetic",
            files
                .iter()
                .map(|name| (name.to_string(), encoded(name, [128, 128, 255, 200]))),
        )
        .expect("mount synthetic images");
        vfs
    }

    fn stage(script: &str) -> (ShaderCatalog, ShaderStage) {
        let definitions =
            parse_shader_script(script.as_bytes(), "scripts/t.shader").expect("script parses");
        let stage = definitions[0].stages[0].clone();
        (ShaderCatalog::default(), stage)
    }

    const ALL: Settings = Settings {
        normal: true,
        specular: true,
        parallax: true,
        reflections: 0,
        emission: true,
    };

    fn found(files: &[&str], script: &str, settings: Settings) -> Found {
        let vfs = vfs(files);
        let (shaders, stage) = stage(script);
        find(
            &vfs,
            &shaders,
            settings,
            &stage,
            "textures/a/floor",
            &mut HashMap::new(),
        )
        .expect("lookup succeeds")
    }

    const PLAIN: &str = "textures/a { { map textures/a/floor.tga blendFunc filter } }";

    #[test]
    fn automatic_lookup_prefers_height_maps_and_gloss_maps() {
        let result = found(
            &[
                "textures/a/floor.jpg",
                "textures/a/floor_n.png",
                "textures/a/floor_nh.png",
                "textures/a/floor_s.png",
                "textures/a/floor_specGloss.png",
                "textures/a/floor_rmo.png",
            ],
            PLAIN,
            ALL,
        );
        let normal = result.normal.expect("normal map");
        assert!(normal.key.ends_with("floor_nh.png"), "{}", normal.key);
        assert!(result.height);
        assert_eq!(result.normal_scale, DEFAULT_NORMAL_SCALE);
        // Height 200 becomes depth 55.
        assert_eq!(normal.pixels.get_pixel(0, 0).0, [128, 128, 255, 55]);
        let (specular, code) = result.specular.expect("specular map");
        assert!(
            specular.key.ends_with("floor_specgloss.png"),
            "{}",
            specular.key
        );
        assert_eq!(code, SPECULAR_GLOSS);
        assert_eq!(result.specular_scale, SPEC_GLOSS_SCALE);
    }

    #[test]
    fn plain_normal_map_and_legacy_and_packed_specular_names() {
        let result = found(
            &[
                "textures/a/floor.tga",
                "textures/a/floor_n.tga",
                "textures/a/floor_s.jpg",
            ],
            PLAIN,
            ALL,
        );
        assert!(!result.height);
        assert_eq!(
            result.normal.expect("normal").pixels.get_pixel(1, 1).0[3],
            255
        );
        assert_eq!(result.specular.expect("specular").1, SPECULAR_GLOSS);

        let result = found(
            &["textures/a/floor.tga", "textures/a/floor_orm.png"],
            PLAIN,
            ALL,
        );
        assert!(result.normal.is_none());
        let (image, code) = result.specular.expect("packed map");
        assert_eq!(code, SPECULAR_PACKED);
        assert_eq!(result.specular_scale, packed_specular_scale());
        assert_eq!(image.pixels.get_pixel(0, 0).0, [128, 128, 255, 200]);
    }

    #[test]
    fn settings_gate_each_kind() {
        let files = [
            "textures/a/floor.tga",
            "textures/a/floor_n.tga",
            "textures/a/floor_rmo.tga",
        ];
        let result = found(
            &files,
            PLAIN,
            Settings {
                normal: false,
                ..ALL
            },
        );
        assert!(result.normal.is_none() && result.specular.is_some());
        let result = found(
            &files,
            PLAIN,
            Settings {
                specular: false,
                ..ALL
            },
        );
        assert!(result.normal.is_some() && result.specular.is_none());
    }

    #[test]
    fn explicit_keywords_override_the_search_and_keep_their_scales() {
        let result = found(
            &[
                "textures/a/floor.tga",
                "textures/a/floor_n.tga",
                "textures/other/bumps.png",
            ],
            "textures/a { { map textures/a/floor.tga normalMap textures/other/bumps \
             normalScale 2 specMap $whiteimage specularReflectance 0.25 } }",
            ALL,
        );
        assert!(result.normal.expect("normal").key.ends_with("bumps.png"));
        assert_eq!(result.normal_scale, [2.0, 2.0, 1.0, 0.05]);
        let (white, code) = result.specular.expect("white gloss map");
        assert_eq!(code, SPECULAR_GLOSS);
        assert_eq!(white.pixels.dimensions(), (1, 1));
        assert_eq!(result.specular_scale, [0.25, 0.25, 0.25, 0.0]);
    }

    #[test]
    fn emission_maps_are_found_next_to_the_diffuse_image_alone() {
        let files = [
            "textures/a/floor.jpg",
            "textures/a/floor_e.png",
            "textures/a/floor_n.png",
        ];
        let result = found(&files, PLAIN, ALL);
        let emission = result.emission.expect("emission map");
        assert!(emission.key.ends_with("floor_e.png"), "{}", emission.key);
        assert!(emission.key.starts_with("material:emission:"));
        // The colour is kept as authored; alpha means nothing.
        assert_eq!(emission.pixels.get_pixel(0, 0).0, [128, 128, 255, 255]);
        // Emission maps need only their own control.
        let alone = found(
            &files,
            PLAIN,
            Settings {
                emission: true,
                ..Default::default()
            },
        );
        assert!(alone.emission.is_some() && alone.normal.is_none() && alone.specular.is_none());
        let off = found(
            &files,
            PLAIN,
            Settings {
                emission: false,
                ..ALL
            },
        );
        assert!(off.emission.is_none() && off.normal.is_some());
        // No `_e` image, no emission map; generated diffuse images have none either.
        assert!(
            found(&["textures/a/floor.jpg"], PLAIN, ALL)
                .emission
                .is_none()
        );
        assert!(
            found(
                &["textures/a/floor_e.png"],
                "textures/a { { map $whiteimage } }",
                ALL
            )
            .emission
            .is_none()
        );
    }

    #[test]
    fn missing_maps_and_generated_diffuse_images_find_nothing() {
        let result = found(&["textures/a/floor.tga"], PLAIN, ALL);
        assert!(result.normal.is_none() && result.specular.is_none());
        let result = found(
            &["textures/a/floor_n.tga"],
            "textures/a { { map $whiteimage } }",
            ALL,
        );
        assert!(result.normal.is_none());
    }

    #[test]
    fn implicit_stage_images_use_the_material_name() {
        let vfs = vfs(&["textures/a/floor.jpg", "textures/a/floor_n.jpg"]);
        let (shaders, mut stage) = stage(PLAIN);
        stage.images.clear();
        let result = find(
            &vfs,
            &shaders,
            ALL,
            &stage,
            "textures/a/floor",
            &mut HashMap::new(),
        )
        .expect("lookup");
        assert!(result.normal.expect("normal").key.ends_with("floor_n.jpg"));
    }

    #[test]
    fn packed_layouts_reorder_to_occlusion_roughness_metalness_specular() {
        let source = RgbaImage::from_pixel(1, 1, image::Rgba([10, 20, 30, 40]));
        let at = |layout| convert_specular(&source, layout).get_pixel(0, 0).0;
        // RMO: roughness, metalness, occlusion.
        assert_eq!(at(SpecularLayout::Rmo), [30, 10, 20, 255]);
        // MOXR: metalness, occlusion, unused, roughness.
        assert_eq!(at(SpecularLayout::Moxr), [20, 40, 10, 255]);
        assert_eq!(at(SpecularLayout::Orm), [10, 20, 30, 40]);
    }

    #[test]
    fn gloss_maps_get_rend2_sdr_ratio() {
        assert_eq!(sdr_spec_gloss([255, 255, 255, 7]), [255, 255, 255, 7]);
        assert_eq!(sdr_spec_gloss([0, 0, 0, 9]), [0, 0, 0, 9]);
        // Grey keeps its hue and drops to its linear value: 128 -> about 55.
        let [r, g, b, _] = sdr_spec_gloss([128, 128, 128, 255]);
        assert!(r == g && g == b && (54..=56).contains(&r), "{r}");
    }
}

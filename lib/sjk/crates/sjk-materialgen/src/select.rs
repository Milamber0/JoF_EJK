//! Which textures get maps: the diffuse images of shaders that installed maps
//! actually draw on lightmapped or vertex-lit surfaces, minus everything that must
//! not get maps ([`SkipReason`]).
//!
//! A shader qualifies the way the client's material maps select stages (and rend2's
//! `CollapseStagesToLightall`): its lightmap and diffuse stages must collapse
//! into one opaque pass (rd-vanilla `CollapseMultitexture` rules) with plain
//! colour generators. The maps belong to that diffuse stage's image. A
//! surface without a shader script is the implicit lightmapped default shader
//! of its texture, which qualifies too. On vertex-lit surfaces, a shader without a
//! lightmap stage qualifies by its first stage when that is opaque `rgbGen vertex`
//! paint ([`vertex_diffuse_stage`]), as the client maps it.

use crate::classes::{ClassSource, MaterialClass, bsp, classify, polished, wants_height};
use crate::emission::{self, Plan, ShaderLight};
use crate::overrides::Overrides;
use sjk_bsp::{Bsp, SurfaceKind};
use sjk_shader::{ShaderCatalog, ShaderDefinition, ShaderStage, StageBlend, TextureGenerator};
use sjk_vfs::VirtualFileSystem;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

/// `SURF_SKY`, `SURF_NODRAW` (OpenJK `surfaceflags.h`).
const SURF_SKY: u32 = 0x0000_2000;
const SURF_NODRAW: u32 = 0x0020_0000;
/// `CONTENTS_LAVA | CONTENTS_WATER | CONTENTS_SLIME`.
const CONTENTS_LIQUID: u32 = 0x0000_0002 | 0x0000_0004 | 0x0002_0000;
/// `CONTENTS_FOG`.
const CONTENTS_FOG: u32 = 0x0000_0008;

/// rend2's automatic normal-map names, in its lookup order.
pub const NORMAL_SUFFIXES: [&str; 2] = ["_nh", "_n"];
/// rend2's automatic specular names (with ioquake3's `_s`), in lookup order.
pub const SPECULAR_SUFFIXES: [&str; 4] = ["_specGloss", "_s", "_rmo", "_orm"];

/// Path prefixes that never get maps, with the reason.
pub const SKIPPED_PREFIXES: &[(&str, SkipReason)] = &[
    ("textures/system/", SkipReason::System),
    ("textures/common/", SkipReason::System),
    ("textures/skies/", SkipReason::Sky),
    ("textures/sky", SkipReason::Sky),
    ("textures/fogs/", SkipReason::Fog),
    ("textures/fog/", SkipReason::Fog),
    ("gfx/", SkipReason::Interface),
    ("ui/", SkipReason::Interface),
    ("menu/", SkipReason::Interface),
    ("fonts/", SkipReason::Interface),
    ("levelshots/", SkipReason::Interface),
    ("2d/", SkipReason::Interface),
    ("maps/", SkipReason::Lightmap),
];

/// Why a shader or texture gets no maps.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SkipReason {
    /// Sky shader or sky surface.
    Sky,
    /// Fog volume.
    Fog,
    /// Water, lava or slime.
    Liquid,
    /// `nodraw`, stage-less, clip, trigger or other editor/system shader.
    System,
    /// Interface, font or 2D image.
    Interface,
    /// A lightmap or other generated image (`$lightmap`, `*white`).
    Lightmap,
    /// Drawn without a lightmap, and its first stage is not opaque vertex-lit paint.
    VertexLit,
    /// `deformVertexes`: moving geometry.
    Deformed,
    /// Only blended or additive stages: an effect, no opaque base.
    BlendOnly,
    /// No lightmap stage: fullbright or vertex-coloured shader.
    NoLightmapStage,
    /// Lightmap and diffuse stages do not collapse into one opaque pass.
    NotCollapsible,
    /// The diffuse stage has non-plain colour generators or surface sprites.
    Effect,
    /// The diffuse stage glows: an emissive image.
    Emissive,
    /// Animated (`animMap`) or rotating, stretching or turbulent coordinates.
    Animated,
    /// Environment-mapped diffuse stage.
    EnvironmentMapped,
    /// Alpha-tested and not of a class where maps are clearly safe (foliage).
    AlphaTested,
    /// The image is not in the game data.
    MissingImage,
    /// Normal and specular maps already exist in the game data.
    HasMaps,
}

impl SkipReason {
    /// Short description for listings and the manifest.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Sky => "sky",
            Self::Fog => "fog",
            Self::Liquid => "water, lava or slime",
            Self::System => "nodraw/clip/trigger/system",
            Self::Interface => "interface or 2D image",
            Self::Lightmap => "lightmap or generated image",
            Self::VertexLit => "vertex-lit, but not opaque rgbGen vertex paint",
            Self::Deformed => "deformVertexes",
            Self::BlendOnly => "blended or additive effect",
            Self::NoLightmapStage => "no lightmap stage",
            Self::NotCollapsible => "lightmap and diffuse stages do not collapse",
            Self::Effect => "colour generators or surface sprites",
            Self::Emissive => "glowing diffuse stage",
            Self::Animated => "animated texture coordinates or frames",
            Self::EnvironmentMapped => "environment-mapped",
            Self::AlphaTested => "alpha-tested foliage or similar",
            Self::MissingImage => "image not found",
            Self::HasMaps => "already has rend2 maps",
        }
    }
}

/// A texture that gets maps.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// The diffuse image's VFS path, with extension.
    pub image: String,
    /// The path without extension, the base of the map names.
    pub base: String,
    /// The class as generated: the table's row, made glossier for polished shaders,
    /// with height decided and the overrides applied.
    pub class: MaterialClass,
    pub class_source: ClassSource,
    /// Some shader using it has a `tcGen environment` stage (`classes::polished`).
    pub polished: bool,
    /// Lines of the overrides file that applied.
    pub overrides: Vec<usize>,
    /// Some shader using it alpha-tests the diffuse stage.
    pub alpha_tested: bool,
    /// Write a normal map (none exists yet).
    pub normal: bool,
    /// Write a packed `_rmo` map (no specular map exists yet).
    pub packed: bool,
    /// Write an emission map (`_e`) when its texels allow ([`crate::emission`]).
    pub emission: Option<Plan>,
    /// Why a texture with some sign of light gets no emission map.
    pub emission_note: Option<&'static str>,
    /// Existing maps found next to the image, by name.
    pub existing: Vec<String>,
    pub shaders: BTreeSet<String>,
    pub maps: BTreeSet<String>,
    /// Lightmapped triangles drawn with it (patches count their control points).
    pub triangles: u64,
}

/// A shader or texture that gets no maps.
#[derive(Clone, Debug)]
pub struct Skipped {
    /// Shader name, or the image path for whole-texture reasons.
    pub name: String,
    pub reason: SkipReason,
    pub maps: BTreeSet<String>,
    pub triangles: u64,
}

/// Result of [`select`].
#[derive(Clone, Debug, Default)]
pub struct Selection {
    /// Map names read, e.g. `mp/ffa3`.
    pub maps: Vec<String>,
    /// Most-used first, then by path.
    pub candidates: Vec<Candidate>,
    /// Sorted by reason, then name.
    pub skipped: Vec<Skipped>,
}

/// How the installed maps use one shader.
#[derive(Clone, Debug, Default)]
pub struct ShaderUse {
    pub surface_flags: u32,
    pub content_flags: u32,
    pub lightmapped_triangles: u64,
    pub other_triangles: u64,
    pub maps: BTreeSet<String>,
}

/// BSP names of all installed maps (`maps/**.bsp`), without prefix and extension.
pub fn installed_maps(vfs: &VirtualFileSystem) -> Vec<String> {
    vfs.paths()
        .iter()
        .filter_map(|path| {
            path.as_str()
                .strip_prefix("maps/")
                .and_then(|name| name.strip_suffix(".bsp"))
                .map(str::to_owned)
        })
        .collect()
}

/// Maps read by [`shader_uses`] and those that could not be parsed.
#[derive(Clone, Debug, Default)]
pub struct MapUses {
    pub uses: BTreeMap<String, ShaderUse>,
    /// Maps whose shaders were read.
    pub read: Vec<String>,
    /// Maps that failed to parse, with the error; the client cannot load them either.
    pub unreadable: Vec<(String, String)>,
}

/// Read the shader lumps and surfaces of `maps`. A map that is not installed is
/// an error; one that does not parse is reported and left out.
pub fn shader_uses(vfs: &VirtualFileSystem, maps: &[String]) -> Result<MapUses, Box<dyn Error>> {
    let mut result = MapUses::default();
    for map in maps {
        let path = format!("maps/{map}.bsp");
        let asset = vfs
            .read(&path)?
            .ok_or_else(|| format!("map {map} not found ({path})"))?;
        match Bsp::parse(&asset.bytes) {
            Ok(bsp) => {
                add_bsp(&mut result.uses, map, &bsp);
                result.read.push(map.clone());
            }
            Err(error) => result.unreadable.push((map.clone(), error.to_string())),
        }
    }
    Ok(result)
}

fn add_bsp(uses: &mut BTreeMap<String, ShaderUse>, map: &str, bsp: &Bsp) {
    for surface in bsp.render().surfaces() {
        if surface.kind == SurfaceKind::Flare {
            continue;
        }
        let Some(shader) = bsp.shaders().get(surface.shader) else {
            continue;
        };
        let name = shader
            .name_lossy()
            .trim_end_matches('\0')
            .replace('\\', "/")
            .to_ascii_lowercase();
        let entry = uses.entry(name).or_default();
        entry.surface_flags = shader.surface_flags;
        entry.content_flags = shader.content_flags;
        entry.maps.insert(map.to_owned());
        let triangles = if surface.indices.is_empty() {
            surface.vertices.len() as u64
        } else {
            surface.indices.len() as u64 / 3
        };
        if surface.lightmaps[0] >= 0 {
            entry.lightmapped_triangles += triangles;
        } else {
            entry.other_triangles += triangles;
        }
    }
}

/// What a qualifying shader contributes.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderChoice {
    /// Resolved diffuse image path.
    pub image: String,
    pub alpha_tested: bool,
    /// The shader has a `tcGen environment` stage: stock polish.
    pub polished: bool,
    /// Its surface light and whether it already shows light over the diffuse pair.
    pub light: ShaderLight,
}

/// Decide one shader: its diffuse image, or why it gets no maps.
pub fn evaluate_shader(
    vfs: &VirtualFileSystem,
    catalog: &ShaderCatalog,
    name: &str,
    shader_use: &ShaderUse,
) -> Result<Result<ShaderChoice, SkipReason>, Box<dyn Error>> {
    if name.starts_with(['$', '*']) {
        return Ok(Err(SkipReason::Lightmap));
    }
    if let Some(reason) = skipped_path(name) {
        return Ok(Err(reason));
    }
    if shader_use.surface_flags & SURF_SKY != 0 {
        return Ok(Err(SkipReason::Sky));
    }
    if shader_use.surface_flags & SURF_NODRAW != 0 {
        return Ok(Err(SkipReason::System));
    }
    if shader_use.content_flags & CONTENTS_FOG != 0 {
        return Ok(Err(SkipReason::Fog));
    }
    if shader_use.content_flags & CONTENTS_LIQUID != 0 {
        return Ok(Err(SkipReason::Liquid));
    }
    let (image, alpha_tested, polished, light) = match catalog.get(name) {
        Some(definition) => {
            let stage = match diffuse_stage(definition) {
                Ok(stage) => stage,
                // Vertex-lit paint: the client maps an opaque `rgbGen vertex` stage on a
                // vertex-lit surface, its vertex colours standing in for the lightmap.
                Err(SkipReason::NoLightmapStage) if shader_use.other_triangles > 0 => {
                    match vertex_diffuse_stage(definition) {
                        Ok(stage) => stage,
                        Err(reason) => return Ok(Err(reason)),
                    }
                }
                Err(reason) => return Ok(Err(reason)),
            };
            let image = catalog.resolve_stage_image(vfs, &stage.images[0])?;
            let polished = definition
                .stages
                .iter()
                .any(|stage| stage.texture_generator == TextureGenerator::Environment);
            (
                image,
                stage.alpha_function.is_some(),
                polished,
                ShaderLight::of(definition),
            )
        }
        None => (
            catalog.resolve_image(vfs, name)?,
            false,
            false,
            ShaderLight::default(),
        ),
    };
    let Some(image) = image else {
        return Ok(Err(SkipReason::MissingImage));
    };
    let image = image.as_str().to_owned();
    if let Some(reason) = skipped_path(&image) {
        return Ok(Err(reason));
    }
    Ok(Ok(ShaderChoice {
        image,
        alpha_tested,
        polished,
        light,
    }))
}

/// Triangles a shader draws on lightmapped and vertex-lit surfaces.
fn drawn(shader_use: &ShaderUse) -> u64 {
    shader_use.lightmapped_triangles + shader_use.other_triangles
}

fn skipped_path(path: &str) -> Option<SkipReason> {
    let path = path.to_ascii_lowercase();
    SKIPPED_PREFIXES
        .iter()
        .find(|(prefix, _)| path.starts_with(prefix))
        .map(|(_, reason)| *reason)
}

/// The diffuse stage of the first lightmap/diffuse pair that collapses into
/// one opaque pass, checked as the client's material maps check it.
pub fn diffuse_stage(definition: &ShaderDefinition) -> Result<&ShaderStage, SkipReason> {
    if definition.sky.is_some() {
        return Err(SkipReason::Sky);
    }
    if definition.fog.is_some() || definition.fog_contents {
        return Err(SkipReason::Fog);
    }
    if !definition.has_color_pass() {
        return Err(SkipReason::System);
    }
    if !definition.deforms.is_empty() {
        return Err(SkipReason::Deformed);
    }
    let stages = &definition.stages;
    let mut index = 0;
    while index < stages.len() {
        let Some(second) = stages.get(index + 1) else {
            break;
        };
        let first = &stages[index];
        if !collapses_opaque(first, second) {
            index += 1;
            continue;
        }
        index += 2;
        let diffuse = match (first.texture_generator, second.texture_generator) {
            (TextureGenerator::Lightmap, TextureGenerator::Lightmap) => continue,
            (TextureGenerator::Lightmap, _) => second,
            (_, TextureGenerator::Lightmap) => first,
            _ => continue,
        };
        return check_diffuse(diffuse, false);
    }
    let lightmap = stages
        .iter()
        .any(|stage| stage.texture_generator == TextureGenerator::Lightmap);
    Err(
        if stages
            .iter()
            .all(|stage| stage.blend != StageBlend::Replace)
        {
            SkipReason::BlendOnly
        } else if !lightmap {
            SkipReason::NoLightmapStage
        } else if stages.iter().any(|stage| stage.alpha_function.is_some()) {
            SkipReason::AlphaTested
        } else {
            SkipReason::NotCollapsible
        },
    )
}

/// The diffuse stage of a shader drawn on vertex-lit surfaces without a lightmap stage:
/// its first stage, when that is opaque texture paint lit by its vertex colours
/// (`rgbGen vertex` or `exactVertex`), as the client maps it. Later stages (detail
/// textures, blended terrain layers) keep their shading.
pub fn vertex_diffuse_stage(definition: &ShaderDefinition) -> Result<&ShaderStage, SkipReason> {
    let Some(first) = definition.stages.first() else {
        return Err(SkipReason::System);
    };
    let vertex = first
        .rgb_generator
        .as_deref()
        .is_some_and(|g| g.eq_ignore_ascii_case("vertex") || g.eq_ignore_ascii_case("exactvertex"));
    if first.blend != StageBlend::Replace
        || first.texture_generator != TextureGenerator::Base
        || !vertex
    {
        return Err(SkipReason::VertexLit);
    }
    check_diffuse(first, true)
}

/// rd-vanilla `CollapseMultitexture` (`tr_shader.cpp`) for the one rule that
/// yields an opaque modulated pass: a replacing stage followed by a filter.
fn collapses_opaque(first: &ShaderStage, second: &ShaderStage) -> bool {
    let filter = match &second.blend {
        StageBlend::Filter => true,
        StageBlend::Custom {
            source,
            destination,
        } => {
            source.eq_ignore_ascii_case("gl_zero")
                && destination.eq_ignore_ascii_case("gl_src_color")
        }
        _ => false,
    };
    first.blend == StageBlend::Replace
        && filter
        && first.depth_function == second.depth_function
        && first.alpha_function == second.alpha_function
        && first.rgb_generator == second.rgb_generator
        && first.alpha_generator == second.alpha_generator
        && first.rgb_wave == second.rgb_wave
        && first.alpha_wave == second.alpha_wave
        && first.rgb_constant == second.rgb_constant
        && first.alpha_constant == second.alpha_constant
}

/// The diffuse stage's own conditions; `vertex` admits the vertex-light generators.
fn check_diffuse(stage: &ShaderStage, vertex: bool) -> Result<&ShaderStage, SkipReason> {
    if stage.texture_generator == TextureGenerator::Environment {
        return Err(SkipReason::EnvironmentMapped);
    }
    let plain = |generator: Option<&str>| {
        generator.is_none_or(|g| {
            g.eq_ignore_ascii_case("identity")
                || g.eq_ignore_ascii_case("identitylighting")
                || (vertex
                    && (g.eq_ignore_ascii_case("vertex") || g.eq_ignore_ascii_case("exactvertex")))
        })
    };
    let plain_alpha = stage.alpha_generator.as_deref().is_none_or(|g| {
        !g.eq_ignore_ascii_case("lightingspecular") && !g.eq_ignore_ascii_case("portal")
    });
    if !plain(stage.rgb_generator.as_deref())
        || stage.rgb_wave.is_some()
        || stage.rgb_constant.is_some()
        || !plain_alpha
        || stage.surface_sprites.is_some()
    {
        return Err(SkipReason::Effect);
    }
    if stage.glow {
        return Err(SkipReason::Emissive);
    }
    let moving = stage
        .texture_modifications
        .iter()
        .any(|m| matches!(m.kind.as_str(), "rotate" | "stretch" | "turb"));
    if stage.images.len() != 1 || moving {
        return Err(SkipReason::Animated);
    }
    if stage.images[0].starts_with(['$', '*']) {
        return Err(SkipReason::Lightmap);
    }
    Ok(stage)
}

/// Select the textures of `uses` that get maps, tuned by `overrides`.
pub fn select(
    vfs: &VirtualFileSystem,
    catalog: &ShaderCatalog,
    maps: Vec<String>,
    uses: &BTreeMap<String, ShaderUse>,
    overrides: &Overrides,
) -> Result<Selection, Box<dyn Error>> {
    let mut skipped = Vec::new();
    // image -> the shaders drawing it, with their use and choice
    let mut by_image: BTreeMap<String, Vec<(&str, &ShaderUse, ShaderChoice)>> = BTreeMap::new();
    for (name, shader_use) in uses {
        match evaluate_shader(vfs, catalog, name, shader_use)? {
            Ok(choice) => by_image
                .entry(choice.image.clone())
                .or_default()
                .push((name, shader_use, choice)),
            Err(reason) => skipped.push(Skipped {
                name: name.clone(),
                reason,
                maps: shader_use.maps.clone(),
                triangles: shader_use.lightmapped_triangles + shader_use.other_triangles,
            }),
        }
    }
    let mut candidates = Vec::new();
    for (image, shaders) in by_image {
        // The class follows the shader drawing most of it (then the first name).
        let (_, primary, _) = shaders
            .iter()
            .max_by(|a, b| drawn(a.1).cmp(&drawn(b.1)).then_with(|| b.0.cmp(a.0)))
            .expect("every image has a shader");
        let (table_class, class_source) = classify(&image, primary.surface_flags);
        let alpha_tested = shaders.iter().any(|(_, _, choice)| choice.alpha_tested);
        let polish = shaders.iter().any(|(_, _, choice)| choice.polished);
        let base = strip_extension(&image).to_owned();
        let mut class = if polish {
            polished(table_class)
        } else {
            table_class.clone()
        };
        class.parallax = wants_height(&class, &base);
        let (class, applied) = overrides.apply(&base, class);
        let triangles = shaders
            .iter()
            .map(|(_, shader_use, _)| drawn(shader_use))
            .sum();
        let maps_used: BTreeSet<String> = shaders
            .iter()
            .flat_map(|(_, shader_use, _)| shader_use.maps.iter().cloned())
            .collect();
        let skip = |reason| Skipped {
            name: image.clone(),
            reason,
            maps: maps_used.clone(),
            triangles,
        };
        if alpha_tested && !class.alpha_test_safe {
            skipped.push(skip(SkipReason::AlphaTested));
            continue;
        }
        let existing_with = |suffixes: &[&str]| -> Result<Vec<String>, Box<dyn Error>> {
            let mut found = Vec::new();
            for suffix in suffixes {
                if let Some(path) = catalog.resolve_stage_image(vfs, &format!("{base}{suffix}"))? {
                    found.push(path.as_str().to_owned());
                }
            }
            Ok(found)
        };
        let normals = existing_with(&NORMAL_SUFFIXES)?;
        let speculars = existing_with(&SPECULAR_SUFFIXES)?;
        let emissions = existing_with(&[emission::SUFFIX])?;
        let lights: Vec<ShaderLight> = shaders.iter().map(|(_, _, choice)| choice.light).collect();
        let computer = shaders
            .iter()
            .any(|(_, shader_use, _)| shader_use.surface_flags & bsp::MASK == bsp::COMPUTER);
        let glow_image = existing_with(&emission::GLOW_SUFFIXES)?.into_iter().next();
        let decision = emission::decide(
            &base,
            &lights,
            computer,
            glow_image.as_deref(),
            overrides.emission(&base),
        );
        let (emission, emission_note) = match decision {
            Ok(_) if !emissions.is_empty() => (None, Some("already has an emission map")),
            Ok(plan) => (Some(plan), None),
            Err(note) => (None, note),
        };
        // Textures with every rend2 map may still need an emission map.
        if !normals.is_empty() && !speculars.is_empty() && emission.is_none() {
            skipped.push(skip(SkipReason::HasMaps));
            continue;
        }
        candidates.push(Candidate {
            normal: normals.is_empty(),
            packed: speculars.is_empty(),
            emission,
            emission_note,
            existing: normals
                .into_iter()
                .chain(speculars)
                .chain(emissions)
                .collect(),
            shaders: shaders
                .iter()
                .map(|(name, _, _)| (*name).to_owned())
                .collect(),
            maps: maps_used,
            image,
            base,
            class,
            class_source,
            polished: polish,
            overrides: applied,
            alpha_tested,
            triangles,
        });
    }
    candidates.sort_by(|a, b| {
        b.triangles
            .cmp(&a.triangles)
            .then_with(|| a.image.cmp(&b.image))
    });
    skipped.sort_by(|a, b| a.reason.cmp(&b.reason).then_with(|| a.name.cmp(&b.name)));
    Ok(Selection {
        maps,
        candidates,
        skipped,
    })
}

/// `textures/a/b.tga` → `textures/a/b`.
pub fn strip_extension(path: &str) -> &str {
    path.rsplit_once('.')
        .filter(|(_, extension)| !extension.contains('/'))
        .map_or(path, |(stem, _)| stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_shader::parse_shader_script;

    fn shader(script: &str) -> ShaderDefinition {
        parse_shader_script(script.as_bytes(), "shaders/test.shader")
            .expect("script parses")
            .remove(0)
    }

    fn reason(script: &str) -> SkipReason {
        diffuse_stage(&shader(script)).expect_err("skipped")
    }

    #[test]
    fn vertex_lit_paint_is_its_first_stage() {
        for script in [
            "textures/d/sand { q3map_nolightmap { map textures/d/sand rgbGen vertex } }",
            "textures/d/rock { { map textures/d/rock rgbGen exactVertex } \
             { map textures/common/detail9 blendFunc GL_DST_COLOR GL_ONE tcMod scale 10 10 } }",
        ] {
            let definition = shader(script);
            // Not a lightmap pair, but vertex-lit paint.
            assert_eq!(
                diffuse_stage(&definition).err(),
                Some(SkipReason::NoLightmapStage)
            );
            let stage = vertex_diffuse_stage(&definition).expect(script);
            assert!(stage.images[0].starts_with("textures/d/"), "{script}");
        }
        for (script, why) in [
            (
                "textures/d/a { { map textures/d/a } }",
                SkipReason::VertexLit,
            ),
            (
                "textures/d/a { { map textures/d/a rgbGen vertex blendFunc blend } }",
                SkipReason::VertexLit,
            ),
            (
                "textures/d/a { { map textures/d/a rgbGen vertex tcMod rotate 10 } }",
                SkipReason::Animated,
            ),
            (
                "textures/d/a { { map textures/d/a rgbGen vertex glow } }",
                SkipReason::Emissive,
            ),
        ] {
            assert_eq!(
                vertex_diffuse_stage(&shader(script)).err(),
                Some(why),
                "{script}"
            );
        }
    }

    #[test]
    fn lightmapped_wall_qualifies_in_either_order() {
        let first = shader(
            "textures/a/wall { { map $lightmap } { map textures/a/wall blendFunc GL_DST_COLOR GL_ZERO } }",
        );
        assert_eq!(
            diffuse_stage(&first).expect("eligible").images[0],
            "textures/a/wall"
        );
        let second = shader(
            "textures/a/wall { { map textures/a/wall } { map $lightmap blendFunc filter } }",
        );
        assert_eq!(
            diffuse_stage(&second).expect("eligible").images[0],
            "textures/a/wall"
        );
        // Extra overlay stages after the base pair do not matter.
        let overlay = shader(
            "textures/a/lamp { { map $lightmap } { map textures/a/lamp blendFunc filter } \
             { map textures/a/lamp_glow blendFunc add glow } }",
        );
        assert!(diffuse_stage(&overlay).is_ok());
    }

    #[test]
    fn skip_rules_on_shader_scripts() {
        assert_eq!(
            reason("textures/s/sky { skyParms textures/s/box 512 - { map $lightmap } }"),
            SkipReason::Sky
        );
        assert_eq!(
            reason("textures/f/fog { surfaceparm fog fogparms ( 1 1 1 ) 512 }"),
            SkipReason::Fog
        );
        assert_eq!(
            reason("textures/c/nodraw { surfaceparm nodraw }"),
            SkipReason::System
        );
        assert_eq!(
            reason(
                "textures/a/flag { deformVertexes wave 100 sin 0 3 0 0.5 { map $lightmap } \
                 { map textures/a/flag blendFunc filter } }"
            ),
            SkipReason::Deformed
        );
        assert_eq!(
            reason("textures/e/beam { { map textures/e/beam blendFunc add } }"),
            SkipReason::BlendOnly
        );
        assert_eq!(
            reason(
                "textures/e/glass { { map textures/e/glass blendFunc blend } { map $lightmap blendFunc filter } }"
            ),
            SkipReason::BlendOnly
        );
        assert_eq!(
            reason("textures/a/bright { { map textures/a/bright } }"),
            SkipReason::NoLightmapStage
        );
        // Foliage: alpha test on the texture only, depthFunc equal on the lightmap.
        assert_eq!(
            reason(
                "textures/p/leaves { cull none { map textures/p/leaves alphaFunc GE128 depthWrite } \
                 { map $lightmap blendFunc filter depthFunc equal } }"
            ),
            SkipReason::AlphaTested
        );
        assert_eq!(
            reason(
                "textures/a/pulse { { map $lightmap rgbGen wave sin 0 1 0 1 } \
                 { map textures/a/pulse blendFunc filter rgbGen wave sin 0 1 0 1 } }"
            ),
            SkipReason::Effect
        );
        assert_eq!(
            reason(
                "textures/a/glow { { map $lightmap } { map textures/a/glow blendFunc filter glow } }"
            ),
            SkipReason::Emissive
        );
        assert_eq!(
            reason(
                "textures/a/fan { { map $lightmap } { map textures/a/fan blendFunc filter tcMod rotate 30 } }"
            ),
            SkipReason::Animated
        );
        assert_eq!(
            reason(
                "textures/a/anim { { map $lightmap } { animMap 2 textures/a/f1 textures/a/f2 blendFunc filter } }"
            ),
            SkipReason::Animated
        );
        assert_eq!(
            reason(
                "textures/a/env { { map $lightmap } { map textures/a/env blendFunc filter tcGen environment } }"
            ),
            SkipReason::EnvironmentMapped
        );
    }

    #[test]
    fn alpha_tested_pairs_that_collapse_are_flagged() {
        let grate = shader(
            "textures/m/grate { { map textures/m/grate alphaFunc GE128 } \
             { map $lightmap blendFunc filter alphaFunc GE128 } }",
        );
        assert!(
            diffuse_stage(&grate)
                .expect("collapses")
                .alpha_function
                .is_some()
        );
    }

    fn library() -> (VirtualFileSystem, ShaderCatalog) {
        let script = "textures/a/wall { surfaceparm metalsteps { map $lightmap } \
                      { map textures/a/wall blendFunc filter } \
                      { map textures/a/env blendFunc add tcGen environment } }\n\
                      textures/a/sky { skyParms - 512 - surfaceparm sky }\n\
                      textures/p/fern { { map textures/p/fern alphaFunc GE128 } \
                      { map $lightmap blendFunc filter alphaFunc GE128 } }\n\
                      textures/m/grate { { map textures/m/grate alphaFunc GE128 } \
                      { map $lightmap blendFunc filter alphaFunc GE128 } }";
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "test",
            [
                ("shaders/test.shader", script.as_bytes().to_vec()),
                ("textures/a/wall.jpg", vec![0]),
                ("textures/a/rock.tga", vec![0]),
                ("textures/a/done.tga", vec![0]),
                ("textures/a/done_n.png", vec![0]),
                ("textures/a/done_specGloss.tga", vec![0]),
                ("textures/a/half.tga", vec![0]),
                ("textures/a/half_nh.tga", vec![0]),
                ("textures/p/fern.tga", vec![0]),
                ("textures/m/grate.tga", vec![0]),
            ],
        )
        .expect("memory mount");
        let catalog = ShaderCatalog::load(&vfs).expect("catalog");
        (vfs, catalog)
    }

    fn lit(maps: &[&str], triangles: u64) -> ShaderUse {
        ShaderUse {
            lightmapped_triangles: triangles,
            maps: maps.iter().map(|map| (*map).to_owned()).collect(),
            ..ShaderUse::default()
        }
    }

    #[test]
    fn selection_resolves_images_and_existing_maps() {
        let (vfs, catalog) = library();
        let mut uses = BTreeMap::new();
        uses.insert("textures/a/wall".to_owned(), lit(&["mp/a"], 10));
        uses.insert("textures/a/rock".to_owned(), lit(&["mp/a", "mp/b"], 50));
        uses.insert("textures/a/done".to_owned(), lit(&["mp/a"], 5));
        uses.insert("textures/a/half".to_owned(), lit(&["mp/a"], 5));
        uses.insert("textures/a/sky".to_owned(), lit(&["mp/a"], 5));
        uses.insert("textures/p/fern".to_owned(), lit(&["mp/a"], 5));
        uses.insert("textures/m/grate".to_owned(), lit(&["mp/a"], 5));
        uses.insert("textures/a/missing".to_owned(), lit(&["mp/a"], 5));
        let mut vertex_lit = lit(&["mp/a"], 0);
        vertex_lit.other_triangles = 9;
        uses.insert("textures/a/rock2".to_owned(), vertex_lit);
        let mut walls = uses["textures/a/wall"].clone();
        walls.surface_flags = crate::classes::bsp::SURF_METALSTEPS;
        uses.insert("textures/a/wall".to_owned(), walls);

        let overrides =
            Overrides::parse("textures/a/half roughness=0.05 height=on").expect("overrides");
        let selection =
            select(&vfs, &catalog, vec!["mp/a".into()], &uses, &overrides).expect("selects");
        let images: Vec<&str> = selection
            .candidates
            .iter()
            .map(|c| c.image.as_str())
            .collect();
        // Most used first; the unscripted rock resolves through its implicit shader.
        assert_eq!(
            images,
            [
                "textures/a/rock.tga",
                "textures/a/wall.jpg",
                "textures/a/half.tga",
                "textures/m/grate.tga"
            ]
        );
        let rock = &selection.candidates[0];
        assert_eq!(rock.class.name, "stone");
        assert_eq!(rock.base, "textures/a/rock");
        assert_eq!(rock.maps.len(), 2);
        let wall = &selection.candidates[1];
        assert_eq!(
            (wall.class.name, wall.class_source),
            ("metal", ClassSource::MetalSteps)
        );
        // Its environment stage marks it polished; metal gets no height, stone does.
        assert!(wall.polished && wall.class.roughness == crate::classes::POLISHED_ROUGHNESS);
        assert!(!wall.class.parallax && !rock.polished && rock.class.parallax);
        let grate = &selection.candidates[3];
        assert!(grate.alpha_tested && grate.class.alpha_test_safe);
        // An existing normal map leaves only the packed map to generate.
        let half = &selection.candidates[2];
        assert!(!half.normal && half.packed);
        assert_eq!(half.existing, ["textures/a/half_nh.tga"]);
        // The overrides file's line 1 applied to it alone.
        assert_eq!((half.class.roughness, half.class.parallax), (0.05, true));
        assert_eq!(half.overrides, [1]);
        assert!(rock.overrides.is_empty());

        let reasons: BTreeMap<&str, SkipReason> = selection
            .skipped
            .iter()
            .map(|s| (s.name.as_str(), s.reason))
            .collect();
        assert_eq!(reasons["textures/a/sky"], SkipReason::Sky);
        // Vertex-lit only, unscripted: the implicit vertex paint qualifies, but it has
        // no image here.
        assert_eq!(reasons["textures/a/rock2"], SkipReason::MissingImage);
        assert_eq!(reasons["textures/a/missing"], SkipReason::MissingImage);
        assert_eq!(reasons["textures/a/done.tga"], SkipReason::HasMaps);
        assert_eq!(reasons["textures/p/fern.tga"], SkipReason::AlphaTested);
    }

    #[test]
    fn emission_plans_follow_the_evidence() {
        let script = "textures/e/panel { q3map_surfacelight 2000 { map $lightmap } \
                      { map textures/e/panel blendFunc filter } }\n\
                      textures/e/strip { q3map_surfacelight 2000 { map $lightmap } \
                      { map textures/e/strip blendFunc filter } \
                      { map textures/e/strip_glow blendFunc add glow } }";
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "test",
            [
                ("shaders/e.shader", script.as_bytes().to_vec()),
                ("textures/e/panel.jpg", vec![0]),
                ("textures/e/strip.jpg", vec![0]),
                ("textures/e/strip_glow.jpg", vec![0]),
                ("textures/e/wall.jpg", vec![0]),
                ("textures/e/wall_glow.jpg", vec![0]),
                ("textures/e/lamp1.jpg", vec![0]),
                ("textures/e/door.jpg", vec![0]),
                ("textures/e/terminal.jpg", vec![0]),
                ("textures/e/screen1.jpg", vec![0]),
                ("textures/e/screen1_e.png", vec![0]),
                ("textures/e/lightbox.jpg", vec![0]),
                ("textures/e/lightbox_n.png", vec![0]),
                ("textures/e/lightbox_rmo.png", vec![0]),
                ("textures/e/plainbox.jpg", vec![0]),
                ("textures/e/plainbox_n.png", vec![0]),
                ("textures/e/plainbox_rmo.png", vec![0]),
            ],
        )
        .expect("memory mount");
        let catalog = ShaderCatalog::load(&vfs).expect("catalog");
        let mut uses = BTreeMap::new();
        for name in [
            "panel", "strip", "wall", "lamp1", "door", "screen1", "lightbox", "plainbox",
        ] {
            uses.insert(format!("textures/e/{name}"), lit(&["mp/e"], 4));
        }
        let mut terminal = lit(&["mp/e"], 4);
        terminal.surface_flags = crate::classes::bsp::COMPUTER;
        uses.insert("textures/e/terminal".to_owned(), terminal);
        let overrides = Overrides::parse("textures/e/door emission=off").expect("overrides");
        let selection =
            select(&vfs, &catalog, vec!["mp/e".into()], &uses, &overrides).expect("selects");
        let find = |name: &str| {
            selection
                .candidates
                .iter()
                .find(|c| c.base == format!("textures/e/{name}"))
                .unwrap_or_else(|| panic!("{name} selected"))
        };
        let evidence = |name: &str| find(name).emission.as_ref().map(|p| p.evidence.clone());
        assert_eq!(
            evidence("panel"),
            Some(emission::Evidence::SurfaceLight(2000.0))
        );
        // The strip's shader shows its glow already.
        assert_eq!(evidence("strip"), None);
        assert_eq!(
            find("strip").emission_note,
            Some("its shaders already show their light")
        );
        let wall = find("wall").emission.as_ref().expect("wall glows");
        assert_eq!(wall.glow_image.as_deref(), Some("textures/e/wall_glow.jpg"));
        assert_eq!(evidence("lamp1"), Some(emission::Evidence::Keyword("lamp")));
        assert_eq!(evidence("terminal"), Some(emission::Evidence::Computer));
        assert_eq!(evidence("door"), None);
        assert_eq!(
            find("door").emission_note,
            Some("emission=off in the overrides")
        );
        assert_eq!(evidence("screen1"), None);
        assert_eq!(
            find("screen1").emission_note,
            Some("already has an emission map")
        );
        // Every rend2 map exists: only the emission map is left to write, or nothing.
        let lightbox = find("lightbox");
        assert!(!lightbox.normal && !lightbox.packed && lightbox.emission.is_some());
        assert!(
            selection
                .skipped
                .iter()
                .any(|s| s.name == "textures/e/plainbox.jpg" && s.reason == SkipReason::HasMaps)
        );
    }

    #[test]
    fn bsp_flags_and_paths_skip_shaders() {
        let (vfs, catalog) = library();
        let check = |name: &str, surface_flags: u32, content_flags: u32| {
            let shader_use = ShaderUse {
                surface_flags,
                content_flags,
                ..lit(&["mp/a"], 1)
            };
            evaluate_shader(&vfs, &catalog, name, &shader_use)
                .expect("evaluates")
                .expect_err("skipped")
        };
        assert_eq!(check("textures/a/rock", SURF_SKY, 0), SkipReason::Sky);
        assert_eq!(check("textures/a/rock", SURF_NODRAW, 0), SkipReason::System);
        assert_eq!(check("textures/a/rock", 0, CONTENTS_FOG), SkipReason::Fog);
        assert_eq!(check("textures/a/rock", 0, 0x4), SkipReason::Liquid);
        assert_eq!(check("textures/system/clip", 0, 0), SkipReason::System);
        assert_eq!(check("textures/common/caulk", 0, 0), SkipReason::System);
        assert_eq!(check("gfx/2d/crosshair", 0, 0), SkipReason::Interface);
        assert_eq!(check("$lightmap", 0, 0), SkipReason::Lightmap);
        assert_eq!(check("maps/mp/ffa3/lm_0000", 0, 0), SkipReason::Lightmap);
    }

    #[test]
    fn extension_stripping() {
        assert_eq!(strip_extension("textures/a/b.tga"), "textures/a/b");
        assert_eq!(strip_extension("textures/a.b/c"), "textures/a.b/c");
    }
}

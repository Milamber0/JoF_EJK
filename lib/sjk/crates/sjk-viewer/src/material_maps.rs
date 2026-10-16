//! Optional material maps for world stages in the convention of OpenJK's rend2
//! renderer (`codemp/rd-rend2`): normal maps (with height in alpha for parallax)
//! and specular or packed roughness/metalness/occlusion maps, named by stage
//! keywords or found next to the diffuse image (`_nh`, `_n`, `_specGloss`,
//! `_rmo`, `_orm`), so existing rend2 texture packs apply unchanged. SJK adds
//! emission maps (`_e`; rend2 has none): an emitted colour added unlit
//! ([`Settings::emission`]).
//!
//! The controls are `r_normalMapping`, `r_specularMapping`, `r_parallaxMapping`
//! and `r_emissiveMaps`, sampled at startup like rend2's latched cvars. SJK turns
//! them all on by default (Sol's choice; rend2 defaults the first three
//! off); they act only where a pack supplies maps. `r_parallaxStrength`
//! (`world_lighting_mode.rs`, live, default 0.1) scales the parallax depth and
//! `r_parallaxNearDistance` (live, default 24 units) holds it back near the camera
//! (`material_map_parallax_tests.rs` models both limits). Off, no
//! image is looked up, no layout, buffer or program exists and every stage compiles
//! exactly as before. On without maps, a map load only looks the map names up in the
//! file index; no layout, buffer or program is created either. On, a stage with maps
//! compiles to its own pipeline key ([`PIPELINE_BIT`]) whose program is the ordinary
//! stage program plus the material hooks (`material_map_program.rs`); stages without
//! maps keep their pipelines, bind groups and stage-table records.
//!
//! They cover lightmapped world surfaces (static and inline movers) whose lightmap
//! and diffuse stages collapse into one pass, and vertex-lit world surfaces whose
//! first stage is opaque `rgbGen vertex` paint (their vertex colours are the baked
//! light). Models, the layers blended over vertex-lit paint and uncollapsed stacks
//! keep their authored shading.

#[path = "material_map_frames.rs"]
pub(crate) mod frames;
#[path = "material_map_gpu.rs"]
pub(super) mod gpu;
#[path = "material_map_images.rs"]
mod images;
#[cfg(test)]
#[path = "material_map_parallax_tests.rs"]
mod parallax_tests;
#[path = "material_map_program.rs"]
pub(super) mod program;
#[path = "reflection_probes.rs"]
pub(crate) mod reflections;

use crate::world_stage::{CollapseOperator, CompiledStage};
use image::RgbaImage;
use sjk_shader::{ShaderCatalog, StageBlend, TextureGenerator};
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// [`crate::world_stage::PipelineKey::geometry`] bit of a material-mapped stage. It was
/// 16, which [`crate::world_stage::FORCED_ALPHA`] also is: an alpha-tested stage drawn
/// with forced alpha (a fading afterimage, a duel ghost) then read as mapped, took the
/// material program's layout with the plain stage group, and wgpu rejected the draw.
pub(crate) const PIPELINE_BIT: u8 = 32;

/// Geometry bits without [`PIPELINE_BIT`]: material maps change shading only, so a
/// mapped stage still casts sun shadows, fills the light buffer and receives SSAO.
pub(crate) const fn without_maps(geometry: u8) -> u8 {
    geometry & !PIPELINE_BIT
}

/// First bit of `r_materialMapsDebug` in the scene lighting-mode word
/// (`world_lighting_mode.rs`); the material program reads two bits from here.
pub(crate) const DEBUG_SHIFT: u32 = 8;

/// The controls in the order of their [`LATCH`] bits.
const CONTROLS: [&str; 4] = [
    "r_normalMapping",
    "r_specularMapping",
    "r_parallaxMapping",
    "r_emissiveMaps",
];
/// [`LATCH`] has been written: the viewer sampled its controls.
const LATCHED: u8 = 0x80;
/// The control values the running viewer sampled (bit `i` for `CONTROLS[i]`, plus
/// [`LATCHED`]). Before the sample, changes are the startup configuration being
/// applied, not changes that need a restart.
static LATCH: AtomicU8 = AtomicU8::new(0);

/// Whether a material-map control is on when nothing sets it: on in SJK.
const DEFAULT_ON: bool = true;

/// Startup policy under rend2's names. [`Settings::default`] is everything off; the
/// controls' defaults are [`DEFAULT_ON`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Settings {
    /// `r_normalMapping`: normal maps (and their height, for parallax).
    pub(crate) normal: bool,
    /// `r_specularMapping`: specular and packed material maps.
    pub(crate) specular: bool,
    /// `r_parallaxMapping`: parallax from the height in a normal map's alpha.
    pub(crate) parallax: bool,
    /// `r_emissiveMaps`: emission maps (`<diffuse>_e`), added unlit; SJK's, on by default.
    pub(crate) emission: bool,
    /// `r_cubeMapping` with `r_cubeMapSize`: the face size of the reflection probes that
    /// specular-mapped surfaces reflect ([`reflections`]), 0 without them.
    pub(crate) reflections: u32,
}

impl Settings {
    /// Read the registered values once, at context creation (or a graphics reload), and
    /// remember them so a later change can tell whether it needs a reload.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>) -> Self {
        let [normal, specular, parallax, emission] = controls(console);
        if console.is_some() {
            let bits = [normal, specular, parallax, emission]
                .iter()
                .enumerate()
                .fold(LATCHED, |bits, (index, on)| bits | (u8::from(*on) << index));
            LATCH.store(bits, Ordering::Relaxed);
        }
        Self::from(
            normal,
            specular,
            parallax,
            emission,
            reflections::sample(console),
        )
    }

    /// What [`Self::sample`] would read now, without remembering it.
    pub(crate) fn read(console: Option<&crate::console::ViewerConsole>) -> Self {
        let [normal, specular, parallax, emission] = controls(console);
        Self::from(
            normal,
            specular,
            parallax,
            emission,
            reflections::read(console),
        )
    }

    fn from(
        normal: bool,
        specular: bool,
        parallax: bool,
        emission: bool,
        reflections: u32,
    ) -> Self {
        Self {
            normal,
            specular,
            // Parallax reads the normal map's height: nothing to do without normal maps.
            parallax: normal && parallax,
            // Probes are reflected through specular maps only.
            reflections: if specular { reflections } else { 0 },
            emission,
        }
    }

    /// Whether any material map may be looked up.
    pub(crate) fn enabled(self) -> bool {
        self.normal || self.specular || self.emission
    }

    /// Whether normal or specular maps may be looked up (rend2's kinds): only those need
    /// vertex frames and the light's direction.
    pub(crate) fn shading(self) -> bool {
        self.normal || self.specular
    }
}

/// The [`CONTROLS`] as the console holds them.
fn controls(console: Option<&crate::console::ViewerConsole>) -> [bool; 4] {
    CONTROLS.map(|name| {
        console
            .and_then(|c| c.integer_cvar(name))
            .map_or(DEFAULT_ON, |value| value != 0)
    })
}

/// Whether setting control `index` to `on` differs from what the viewer runs with
/// (`latch`, the [`LATCH`] word). Nothing differs before the startup sample: the
/// configuration file setting a control is what the sample then reads.
fn restart_needed(latch: u8, index: usize, on: bool) -> bool {
    latch & LATCHED != 0 && (latch >> index & 1 != 0) != on
}

/// The enabled kinds for the load log, as `normal+specular+parallax` (or `off`).
impl std::fmt::Display for Settings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kinds = [
            (self.normal, "normal"),
            (self.specular, "specular"),
            (self.parallax, "parallax"),
            (self.reflections > 0, "reflections"),
            (self.emission, "emission"),
        ];
        let mut first = true;
        for (_, name) in kinds.iter().filter(|(on, _)| *on) {
            formatter.write_str(if first { "" } else { "+" })?;
            formatter.write_str(name)?;
            first = false;
        }
        if first {
            formatter.write_str("off")?;
        }
        Ok(())
    }
}

/// Register the rend2-named controls and SJK's emission controls; a change of the
/// latched ones asks for a restart, like rend2's latch.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    for (index, (name, default, help)) in [
        (
            "r_normalMapping",
            i64::from(DEFAULT_ON),
            "Normal maps on world surfaces (rend2 _n/_nh images and normalMap keywords); \
             vid_restart applies it",
        ),
        (
            "r_specularMapping",
            i64::from(DEFAULT_ON),
            "Specular/roughness maps on world surfaces (rend2 _specGloss/_rmo/_orm images and \
             keywords); vid_restart applies it",
        ),
        (
            // Off in SJK for a while on 07/10/2026: the full depth of generated height (a
            // guess from paint) swam. On again with `r_parallaxStrength` 0.1 (Sol's choice).
            "r_parallaxMapping",
            i64::from(DEFAULT_ON),
            "Parallax from the height in a normal map's alpha (_nh images, normalHeightMap); \
             needs r_normalMapping; depth is r_parallaxStrength; vid_restart applies it",
        ),
        (
            "r_emissiveMaps",
            i64::from(DEFAULT_ON),
            "Emission maps on world surfaces (<texture>_e images): light-emitting texels glow \
             unlit; vid_restart applies it",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        debug_assert_eq!(name, CONTROLS[index]);
        cvars.register(CvarDefinition::new(name, default, CvarFlags::ARCHIVE, help))?;
        cvars.on_change(name, move |change| {
            let on = matches!(change.current, CvarValue::Integer(value) if value != 0);
            if restart_needed(LATCH.load(Ordering::Relaxed), index, on) {
                crate::log::progress(format_args!(
                    "{name} changed: {}",
                    crate::graphics_reload::APPLY
                ))
            }
            crate::graphics_reload::notice();
        })?;
    }
    lights::register(cvars)?;
    reflections::register(cvars)
}

/// `r_emissiveLights`: emission-mapped surfaces as light sources of real-time lighting.
pub(crate) mod lights {
    use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// The cvar's name.
    pub(crate) const CVAR: &str = "r_emissiveLights";
    /// Radiance of white emission (before the multiplier): what an explicit glow stage of
    /// an undeclared fixture and self-lit fixture paint get (`world_emission.rs`), since
    /// an emission map shows the same kind of light. An artistic convention, not a
    /// recovered compiler intensity.
    pub(crate) const RADIANCE: f32 = 16.;
    /// Most lamps the emission maps of one map may add, the brightest kept: screens and
    /// small inserts must not multiply the per-pixel lamp work.
    pub(crate) const MAX_LAMPS: usize = 1024;
    /// Most the light power may be multiplied.
    pub(crate) const MAX: f32 = 4.0;
    /// The current multiplier's bits (1.0 until registered), read when a map loads.
    static GAIN: AtomicU32 = AtomicU32::new(0x3f80_0000);

    /// The multiplier a cvar value asks for: 0 (off) to [`MAX`], 1 for a non-number.
    pub(crate) fn gain_of(value: &CvarValue) -> f32 {
        let gain = match value {
            CvarValue::Float(value) => *value as f32,
            CvarValue::Integer(value) => *value as f32,
            CvarValue::Bool(on) => f32::from(u8::from(*on)),
            CvarValue::Text(_) => 1.0,
        };
        if gain.is_finite() {
            gain.clamp(0.0, MAX)
        } else {
            1.0
        }
    }

    /// The multiplier for the map being loaded.
    pub(crate) fn gain() -> f32 {
        f32::from_bits(GAIN.load(Ordering::Relaxed))
    }

    /// Whether a material's emission map becomes a light: lights are on (`gain`) and the
    /// material gives none yet (`existing`, from a declared surface light, an inferred
    /// fixture or self-lit paint), so no surface is counted twice.
    pub(crate) fn applies(existing: [f32; 3], gain: f32) -> bool {
        gain > 0. && existing.iter().all(|c| *c <= 0.)
    }

    /// Keep the [`MAX_LAMPS`] most powerful lamps.
    pub(crate) fn keep_brightest(lamps: &mut Vec<crate::lamp_lights::Lamp>) {
        if lamps.len() > MAX_LAMPS {
            lamps.sort_by(|a, b| b.power.total_cmp(&a.power));
            lamps.truncate(MAX_LAMPS);
        }
    }

    pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
        cvars.register(CvarDefinition::new(
            CVAR,
            1.0_f64,
            CvarFlags::ARCHIVE,
            "Emission-mapped surfaces light their surroundings in real-time lighting \
             (r_dayNight 1): 0 off, 1 default, up to 4; applies when a map loads",
        ))?;
        let set = |value: &CvarValue| GAIN.store(gain_of(value).to_bits(), Ordering::Relaxed);
        set(&cvars.get(CVAR).expect("registered").value);
        cvars.on_change(CVAR, move |change| set(&change.current))
    }
}

/// The `sjk-materialgen` tuning this client expects (`package::GENERATION` there):
/// 2 tuned metal for reflection probes and marked polished shaders, 3 added emission
/// maps, 4 turned relief the right way up and took parallax and grain off metal, 5 added
/// vertex-lit paint and the indicator lights of controls, 6 lets declared light fixtures
/// whose overlay does not glow emit ([`shows_light`]), 7 classes crates as painted metal
/// and concrete as concrete, reads the overrides from the client's generated folder and
/// takes per-texture relief strength.
pub(crate) const GENERATION: u32 = 7;
/// Where the generator's manifest sits in its pk3.
const MANIFEST: &str = "sjk-materialgen/manifest.json";
/// The older-pack note was printed: once per run is enough.
static GENERATION_REPORTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The generation of the mounted generated pack: `None` without one, 1 for a manifest
/// written before generations were recorded.
pub(crate) fn pack_generation(vfs: &VirtualFileSystem) -> Option<u32> {
    let asset = vfs.read(MANIFEST).ok()??;
    let manifest: serde_json::Value = serde_json::from_slice(&asset.bytes).ok()?;
    Some(
        manifest
            .get("generation")
            .and_then(serde_json::Value::as_u64)
            .map_or(1, |generation| generation as u32),
    )
}

/// Note once when the mounted generated pack predates [`GENERATION`].
pub(crate) fn report_pack_generation(vfs: &VirtualFileSystem) {
    let Some(generation) = pack_generation(vfs).filter(|generation| *generation < GENERATION)
    else {
        return;
    };
    if !GENERATION_REPORTED.swap(true, Ordering::Relaxed) {
        crate::log::progress(format_args!(
            "material maps: the generated pack is generation {generation} of sjk-materialgen, \
             this client expects {GENERATION} (vertex-lit terrain, relief the right way up, \
             smoother metal, emission maps, your overrides); regenerate it (docs/rendering.md, Generating material \
             maps)"
        ));
    }
}

/// Which bundle of a collapsed hardware stage holds the diffuse texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Bundle {
    Primary,
    Secondary,
}

/// The diffuse bundle of a stage that can take material maps: a lightmapped
/// world surface's lightmap and diffuse texture collapsed into one opaque pass
/// with plain colour generators, or a vertex-lit world surface's opaque texture lit
/// by its vertex colours (`rgbGen vertex`/`exactVertex`). The maps then replace the
/// lightmap's or the vertex light's response (rend2's `CollapseStagesToLightall`
/// makes the same choice). Everything else, including deforming and sprite stages
/// and the blended layers over a vertex-lit base, keeps its authored shading.
pub(super) fn diffuse_bundle(stage: &CompiledStage, lightmap: i32) -> Option<Bundle> {
    let Some(secondary) = stage.secondary.as_ref() else {
        let vertex_lit = lightmap == crate::world_stage::LIGHTMAP_BY_VERTEX
            && stage.combine == CollapseOperator::None
            && stage.output_blend == StageBlend::Replace
            && stage.primary.texture_generator == TextureGenerator::Base
            && stage.primary.rgb_generator.as_deref().is_some_and(|g| {
                g.eq_ignore_ascii_case("vertex") || g.eq_ignore_ascii_case("exactVertex")
            });
        return (vertex_lit && plain_paint(&stage.primary, true)).then_some(Bundle::Primary);
    };
    if lightmap < 0
        || stage.combine != CollapseOperator::Modulate
        || stage.output_blend != StageBlend::Replace
    {
        return None;
    }
    let bundle = match (stage.primary.texture_generator, secondary.texture_generator) {
        (TextureGenerator::Base, TextureGenerator::Lightmap) => Bundle::Primary,
        (TextureGenerator::Lightmap, TextureGenerator::Base) => Bundle::Secondary,
        _ => return None,
    };
    let diffuse = match bundle {
        Bundle::Primary => &stage.primary,
        Bundle::Secondary => secondary,
    };
    // Collapsed bundles share their generators (`collapse_pair`), so one check covers both.
    plain_paint(diffuse, false).then_some(bundle)
}

/// A diffuse bundle with plain colour: no waves, constants, specular or portal alpha,
/// or surface sprites; `vertex` admits the vertex-light generators.
fn plain_paint(diffuse: &sjk_shader::ShaderStage, vertex: bool) -> bool {
    let plain = diffuse.rgb_generator.as_deref().is_none_or(|g| {
        g.eq_ignore_ascii_case("identity")
            || g.eq_ignore_ascii_case("identityLighting")
            || (vertex
                && (g.eq_ignore_ascii_case("vertex") || g.eq_ignore_ascii_case("exactVertex")))
    });
    let plain_alpha = diffuse.alpha_generator.as_deref().is_none_or(|g| {
        !g.eq_ignore_ascii_case("lightingSpecular") && !g.eq_ignore_ascii_case("portal")
    });
    plain
        && diffuse.rgb_wave.is_none()
        && diffuse.rgb_constant.is_none()
        && plain_alpha
        && diffuse.surface_sprites.is_none()
}

/// One decoded map in the layout the material program reads, with its cache key.
#[derive(Clone, Debug)]
pub(super) struct MapImage {
    pub(super) key: String,
    pub(super) pixels: Arc<RgbaImage>,
}

/// Uniform parameters of one material-mapped stage (`MaterialMapParams` in WGSL).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Params {
    /// rend2 `normalScale`: x/y strength, z unused, w parallax depth.
    pub(super) normal_scale: [f32; 4],
    /// rend2 `specularScale`.
    pub(super) specular_scale: [f32; 4],
    /// x: [`FLAG_NORMAL`] and friends; y: specular layout (0 none, 1 spec/gloss,
    /// 2 occlusion-roughness-metalness-specular); z: parallax bias; w unused.
    pub(super) control: [f32; 4],
}

/// A material-mapped stage's maps for the floor mirrors' finish
/// (`floor_reflection.wgsl`): the uploaded views and the stage's [`Params`] words.
#[derive(Clone, Debug)]
pub(crate) struct FloorMaps {
    pub(crate) normal: wgpu::TextureView,
    pub(crate) specular: wgpu::TextureView,
    pub(crate) params: [f32; 12],
    pub(crate) clamp: bool,
}

/// The stage has a normal map.
pub(super) const FLAG_NORMAL: u32 = 1;
/// The normal map's alpha is a depth map and parallax is enabled.
pub(super) const FLAG_PARALLAX: u32 = 2;
/// The diffuse texture is the secondary bundle.
pub(super) const FLAG_SECONDARY: u32 = 4;
/// Two-sided material: shade the side facing the viewer.
pub(super) const FLAG_TWO_SIDED: u32 = 8;
/// The stage has an emission map.
pub(super) const FLAG_EMISSION: u32 = 16;
/// The stage is drawn into the dynamic glow image only for its emission map (it has no
/// authored `glow`): the glow program writes the emission, not the lit colour.
pub(super) const FLAG_EMISSION_GLOW: u32 = 32;

/// The maps of one stage, decoded on a load worker and uploaded with its bind group.
#[derive(Clone, Debug)]
pub(super) struct StageMaps {
    pub(super) normal: Option<MapImage>,
    pub(super) specular: Option<MapImage>,
    /// The emission map (sRGB colour, uploaded as such).
    pub(super) emission: Option<MapImage>,
    pub(super) params: Params,
    /// Clamp the maps like the diffuse texture.
    pub(super) clamp: bool,
}

/// How many stages of a map found which maps, for the load log.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Counts {
    pub(super) stages: usize,
    pub(super) normal: usize,
    pub(super) parallax: usize,
    pub(super) specular: usize,
    pub(super) emission: usize,
}

impl Counts {
    pub(super) fn of<'a>(maps: impl Iterator<Item = &'a StageMaps>) -> Self {
        maps.fold(Self::default(), |found, maps| {
            let flags = maps.params.control[0] as u32;
            Self {
                stages: found.stages + 1,
                normal: found.normal + usize::from(maps.normal.is_some()),
                parallax: found.parallax + usize::from(flags & FLAG_PARALLAX != 0),
                specular: found.specular + usize::from(maps.specular.is_some()),
                emission: found.emission + usize::from(maps.emission.is_some()),
            }
        })
    }
}

/// Find, decode and convert the maps of one hardware stage. `None` when the stage
/// cannot take maps or none exist; the stage then compiles exactly as without maps.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    settings: Settings,
    stage: &CompiledStage,
    lightmap: i32,
    two_sided: bool,
    implicit_name: &str,
    cache: &mut HashMap<String, Arc<RgbaImage>>,
) -> Result<Option<StageMaps>, Box<dyn Error>> {
    if !settings.enabled() {
        return Ok(None);
    }
    let Some(bundle) = diffuse_bundle(stage, lightmap) else {
        return Ok(None);
    };
    let diffuse = match bundle {
        Bundle::Primary => &stage.primary,
        Bundle::Secondary => stage.secondary.as_ref().expect("collapsed stage"),
    };
    let found = images::find(vfs, shaders, settings, diffuse, implicit_name, cache)?;
    if found.normal.is_none() && found.specular.is_none() && found.emission.is_none() {
        return Ok(None);
    }
    let mut flags = 0;
    if found.normal.is_some() {
        flags |= FLAG_NORMAL;
        if settings.parallax && found.height {
            flags |= FLAG_PARALLAX;
        }
    }
    if bundle == Bundle::Secondary {
        flags |= FLAG_SECONDARY;
    }
    if two_sided {
        flags |= FLAG_TWO_SIDED;
    }
    if found.emission.is_some() {
        flags |= FLAG_EMISSION;
        if !stage.glow {
            flags |= FLAG_EMISSION_GLOW;
        }
    }
    Ok(Some(StageMaps {
        params: Params {
            normal_scale: found.normal_scale,
            specular_scale: found.specular_scale,
            control: [
                flags as f32,
                found
                    .specular
                    .as_ref()
                    .map_or(0.0, |(_, kind)| *kind as f32),
                found.parallax_bias,
                0.0,
            ],
        },
        normal: found.normal,
        specular: found.specular.map(|(image, _)| image),
        emission: found.emission,
        clamp: diffuse.clamp,
    }))
}

impl StageMaps {
    /// The stage glows only because of its emission map ([`FLAG_EMISSION_GLOW`]).
    pub(super) fn emission_glow(&self) -> bool {
        self.params.control[0] as u32 & FLAG_EMISSION_GLOW != 0
    }
}

/// Whether a shader already shows light of its own on top of its paint: a glowing
/// texture stage, or an additive or destination-brightening (`GL_DST_COLOR GL_ONE`)
/// one on a shader that declares no `q3map_surfacelight`. Such a shader takes no
/// emission map, so its light is never shown twice. A declared light fixture whose
/// overlay does not glow takes one: the overlay alone neither reads as a lamp in a
/// dark room nor blooms (Sol's world notes, 07/10/2026). The generator decides the
/// same way (`sjk-materialgen`'s `emission::shader_shows_light`).
pub(super) fn shows_light(definition: &sjk_shader::ShaderDefinition) -> bool {
    let fixture = definition.surface_light.is_finite() && definition.surface_light > 0.0;
    definition.stages.iter().any(|stage| {
        stage.texture_generator == TextureGenerator::Base
            && (stage.glow
                || !fixture
                    && (stage.blend == StageBlend::Add
                        || matches!(&stage.blend, StageBlend::Custom { destination, .. }
                            if destination.eq_ignore_ascii_case("gl_one"))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_stage::collapse_multitexture;

    fn stages(script: &str) -> Vec<CompiledStage> {
        let definitions = sjk_shader::parse_shader_script(script.as_bytes(), "scripts/t.shader")
            .expect("script parses");
        collapse_multitexture(&definitions[0].stages)
    }

    #[test]
    fn lightmapped_diffuse_pairs_take_maps() {
        let compiled =
            stages("textures/a {\n{ map $lightmap }\n{ map textures/a/floor blendFunc filter }\n}");
        assert_eq!(compiled.len(), 1);
        assert_eq!(diffuse_bundle(&compiled[0], 0), Some(Bundle::Primary));
        // A lightmap pair on a vertex-lit or model surface has no lightmap to redistribute.
        assert_eq!(diffuse_bundle(&compiled[0], -3), None);
        assert_eq!(diffuse_bundle(&compiled[0], -1), None);
        let reversed =
            stages("textures/a {\n{ map textures/a/floor }\n{ map $lightmap blendFunc filter }\n}");
        assert_eq!(diffuse_bundle(&reversed[0], 2), Some(Bundle::Primary));
    }

    #[test]
    fn vertex_lit_paint_takes_maps() {
        // desert/sandfloor2_phong and t_rockwall1_clip: the vertex colours are the light.
        for script in [
            "textures/a {\nq3map_nolightmap\n{ map textures/a/sand rgbGen vertex }\n}",
            "textures/a {\n{ map textures/a/rock rgbGen vertex }\n\
             { map textures/common/detail9 blendFunc GL_DST_COLOR GL_ONE tcMod scale 10 10 }\n}",
            "textures/a {\n{ map textures/a/rock rgbGen exactVertex alphaGen vertex }\n}",
        ] {
            let compiled = stages(script);
            assert_eq!(
                diffuse_bundle(&compiled[0], -3),
                Some(Bundle::Primary),
                "{script}"
            );
            // Only on vertex-lit surfaces; the overlays keep their shading.
            assert_eq!(diffuse_bundle(&compiled[0], -1), None, "{script}");
            assert!(
                compiled[1..]
                    .iter()
                    .all(|s| diffuse_bundle(s, -3).is_none())
            );
        }
        // Unlit, waving or blended paint is not vertex-lit paint.
        for script in [
            "textures/a { { map textures/a/sand } }",
            "textures/a { { map textures/a/sand rgbGen wave sin 0 1 0 1 } }",
            "textures/a { { map textures/a/sand rgbGen vertex blendFunc blend } }",
        ] {
            assert_eq!(diffuse_bundle(&stages(script)[0], -3), None, "{script}");
        }
        // An unscripted vertex-lit texture's implicit stage qualifies.
        let (implicit, _, _) = crate::world_stage::material_stages(None, -3);
        let compiled = collapse_multitexture(&implicit);
        assert_eq!(diffuse_bundle(&compiled[0], -3), Some(Bundle::Primary));
    }

    #[test]
    fn unscripted_textures_take_maps() {
        // A texture without a shader script draws as an implicit lightmap + texture pair
        // whose diffuse stage names no image: the material name is the image.
        let (stages, _, _) = crate::world_stage::material_stages(None, 3);
        let compiled = collapse_multitexture(&stages);
        assert_eq!(compiled.len(), 1);
        assert!(compiled[0].primary.images.is_empty());
        assert_eq!(diffuse_bundle(&compiled[0], 3), Some(Bundle::Primary));
    }

    #[test]
    fn effect_stages_keep_authored_shading() {
        for script in [
            // A single texture without a lightmap.
            "textures/a { { map textures/a/floor } }",
            // Additive glow over the lightmap.
            "textures/a {\n{ map $lightmap }\n{ map textures/a/glow blendFunc add }\n}",
            // Environment mapping.
            "textures/a {\n{ map $lightmap }\n{ map textures/a/env tcGen environment \
             blendFunc filter }\n}",
            // Animated colour.
            "textures/a {\n{ map $lightmap rgbGen wave sin 0 1 0 1 }\n\
             { map textures/a/floor blendFunc filter rgbGen wave sin 0 1 0 1 }\n}",
        ] {
            let compiled = stages(script);
            assert!(
                compiled
                    .iter()
                    .all(|stage| diffuse_bundle(stage, 0).is_none()),
                "{script}"
            );
        }
    }

    #[test]
    fn settings_need_normal_maps_for_parallax() {
        // Unset controls take SJK's defaults: every kind on, probes at their default size.
        assert_eq!(
            Settings::sample(None),
            Settings {
                normal: true,
                specular: true,
                parallax: true,
                emission: true,
                reflections: reflections::DEFAULT_SIZE,
            }
        );
        assert!(!Settings::default().enabled());
        assert!(
            Settings {
                specular: true,
                ..Default::default()
            }
            .enabled()
        );
    }

    #[test]
    fn counts_name_the_maps_each_stage_found() {
        let image = MapImage {
            key: "material:normal:t".into(),
            pixels: Arc::new(RgbaImage::new(1, 1)),
        };
        let stage = |normal: bool, specular: bool, flags: u32| StageMaps {
            normal: normal.then(|| image.clone()),
            specular: specular.then(|| image.clone()),
            emission: (flags & FLAG_EMISSION != 0).then(|| image.clone()),
            params: Params {
                control: [flags as f32, 0.0, 0.0, 0.0],
                ..Default::default()
            },
            clamp: false,
        };
        let stages = [
            stage(true, true, FLAG_NORMAL | FLAG_PARALLAX),
            stage(true, false, FLAG_NORMAL),
            stage(false, true, 0),
            stage(false, false, FLAG_EMISSION | FLAG_EMISSION_GLOW),
        ];
        assert_eq!(
            Counts::of(stages.iter()),
            Counts {
                stages: 4,
                normal: 2,
                parallax: 1,
                specular: 2,
                emission: 1,
            }
        );
        assert!(stages[3].emission_glow() && !stages[0].emission_glow());
    }

    #[test]
    fn settings_name_their_kinds_for_the_log() {
        assert_eq!(Settings::default().to_string(), "off");
        let all = Settings {
            normal: true,
            specular: true,
            parallax: true,
            reflections: 0,
            emission: false,
        };
        assert_eq!(all.to_string(), "normal+specular+parallax");
        let emission = Settings {
            emission: true,
            ..Default::default()
        };
        assert_eq!(emission.to_string(), "emission");
        // Emission alone looks maps up but needs no frames or light directions.
        assert!(emission.enabled() && !emission.shading());
        let reflecting = Settings {
            specular: true,
            reflections: 128,
            ..Default::default()
        };
        assert_eq!(reflecting.to_string(), "specular+reflections");
        let specular = Settings {
            specular: true,
            ..Default::default()
        };
        assert_eq!(specular.to_string(), "specular");
    }

    #[test]
    fn pipeline_bit_only_separates_shading() {
        // Distinct from deforms (1), sprites (2), live emission (4), polygon offset (8)
        // and forced entity alpha (16).
        assert_eq!(
            PIPELINE_BIT
                & (1 | 2
                    | 4
                    | crate::world_stage::POLYGON_OFFSET
                    | crate::world_stage::FORCED_ALPHA),
            0
        );
        // A forced-alpha key of a plain stage is not taken for a mapped one.
        let forced = super::super::forced_alpha::key(
            crate::world_stage::PipelineKey {
                geometry: 0,
                source: wgpu::BlendFactor::One,
                destination: wgpu::BlendFactor::Zero,
                depth_write: true,
                depth: wgpu::CompareFunction::LessEqual,
                cull: None,
            },
            true,
        );
        assert_eq!(forced.geometry & PIPELINE_BIT, 0);
        let stage = &stages(
            "textures/a {
{ map $lightmap }
{ map textures/a/b blendFunc filter }
}",
        )[0];
        let plain = crate::world_stage::hardware_pipeline_key(stage, sjk_shader::ShaderCull::Front);
        let mut mapped = plain;
        mapped.geometry |= PIPELINE_BIT;
        // Mapped and plain stages never share a pipeline, yet a mapped stage still casts
        // shadows, fills the light buffer and receives SSAO like the plain one.
        assert_ne!(plain, mapped);
        assert_eq!(without_maps(mapped.geometry), plain.geometry);
        assert!(crate::world_materials::ssao::eligible(
            mapped,
            &crate::world_stage::compile_hardware_stage(stage)
        ));
    }

    #[test]
    fn only_changes_after_the_startup_sample_ask_for_a_restart() {
        // The configuration file sets the controls before the viewer samples them.
        assert!(!restart_needed(0, 0, true));
        assert!(!restart_needed(0, 2, false));
        // Sampled with normal maps on and the others off.
        let latch = LATCHED | 1;
        assert!(!restart_needed(latch, 0, true));
        assert!(restart_needed(latch, 0, false));
        assert!(restart_needed(latch, 1, true));
        assert!(!restart_needed(latch, 1, false));
        // Parallax latches its own value, not the effective one: a restart with it
        // on is what the change asks for, even while normal maps are off.
        assert!(restart_needed(LATCHED, 2, true));
    }

    #[test]
    fn emission_maps_default_on_and_their_lights_follow_the_cvar() {
        let mut cvars = CvarRegistry::new();
        register(&mut cvars).expect("registers");
        assert_eq!(
            cvars.get("r_emissiveMaps").expect("registered").value,
            CvarValue::Integer(1)
        );
        // SJK turns normal, specular and parallax maps on by default too (parallax at a
        // tenth of its depth, `r_parallaxStrength`).
        for name in CONTROLS.iter().take(3) {
            assert_eq!(
                cvars.get(name).expect("registered").value,
                CvarValue::Integer(1)
            );
        }
        // The light multiplier is read when a map loads; it stays within 0..4.
        assert_eq!(lights::gain_of(&CvarValue::Float(2.5)), 2.5);
        assert_eq!(lights::gain_of(&CvarValue::Integer(0)), 0.0);
        assert_eq!(lights::gain_of(&CvarValue::Float(9.0)), lights::MAX);
        assert_eq!(lights::gain_of(&CvarValue::Float(-1.0)), 0.0);
        assert_eq!(lights::gain_of(&CvarValue::Float(f64::NAN)), 1.0);
        assert_eq!(lights::gain_of(&CvarValue::Text("x".into())), 1.0);
        // Only a material without light of its own gets one from its emission map.
        assert!(lights::applies([0.; 3], 1.));
        assert!(!lights::applies([0.; 3], 0.));
        assert!(!lights::applies([0.2, 0., 0.], 1.));
        // Emission latches like the rend2 controls.
        assert!(restart_needed(LATCHED | 8, 3, false));
        assert!(!restart_needed(LATCHED | 8, 3, true));
    }

    #[test]
    fn emission_map_lights_keep_the_brightest_up_to_the_cap() {
        let lamp = |power: f32| crate::lamp_lights::Lamp {
            position: glam::Vec3::ZERO,
            normal: glam::Vec3::Z,
            color: [1.; 3],
            power,
            radius: 64.,
            axis_u: glam::Vec3::X,
            axis_v: glam::Vec3::Y,
        };
        let mut few: Vec<_> = [3., 1., 2.].map(lamp).to_vec();
        lights::keep_brightest(&mut few);
        // Under the cap the extraction order stays.
        assert_eq!(
            few.iter().map(|l| l.power).collect::<Vec<_>>(),
            [3., 1., 2.]
        );
        let mut many: Vec<_> = (0..lights::MAX_LAMPS + 10)
            .map(|i| lamp(i as f32))
            .collect();
        lights::keep_brightest(&mut many);
        assert_eq!(many.len(), lights::MAX_LAMPS);
        assert_eq!(many[0].power, (lights::MAX_LAMPS + 9) as f32);
        assert!(many.iter().all(|l| l.power >= 10.));
    }

    #[test]
    fn emission_map_fields_keep_their_mean_and_mask() {
        // Left half white, right half black: half the area emits.
        let image = Arc::new(RgbaImage::from_fn(8, 8, |x, _| {
            image::Rgba(if x < 4 { [255; 4] } else { [0, 0, 0, 255] })
        }));
        let stage =
            &stages("textures/a {\n{ map $lightmap }\n{ map textures/a/b blendFunc filter }\n}")[0];
        // The diffuse bundle, as `compile_material` picks it.
        assert_eq!(diffuse_bundle(stage, 0), Some(Bundle::Primary));
        let field = crate::world_materials::emission::Texture::from_map(
            &stage.primary,
            image,
            lights::RADIANCE,
        );
        let mean = field.mean();
        assert!((mean.x - lights::RADIANCE / 2.).abs() < 1e-3, "{mean}");
        // Relative to the mean: twice it on the white half, nothing on the black half.
        let lit = field.sample(glam::Vec2::new(0.25, 0.5), 0);
        let dark = field.sample(glam::Vec2::new(0.75, 0.5), 0);
        assert!((lit.x - 2.).abs() < 0.05 && dark.x < 0.05, "{lit} {dark}");
    }

    #[test]
    fn shaders_that_show_light_take_no_emission_maps() {
        let shows = |script: &str| {
            shows_light(
                &sjk_shader::parse_shader_script(script.as_bytes(), "scripts/t.shader")
                    .expect("parses")[0],
            )
        };
        assert!(!shows(
            "t/a { q3map_surfacelight 900 { map $lightmap } { map t/a blendFunc filter } }"
        ));
        assert!(shows(
            "t/a { { map $lightmap } { map t/a blendFunc filter } { map t/g blendFunc add } }"
        ));
        assert!(shows(
            "t/a { { map $lightmap } { map t/a blendFunc filter } \
             { map t/g blendFunc GL_DST_COLOR GL_ONE } }"
        ));
        assert!(shows(
            "t/a { { map $lightmap } { map t/a blendFunc filter } { map t/g blendFunc blend glow } }"
        ));
        // A declared fixture shows its light only through a `glow` stage.
        assert!(!shows(
            "t/a { q3map_surfacelight 3000 { map $lightmap } { map t/a blendFunc filter } \
             { map t/g blendFunc GL_DST_COLOR GL_ONE } }"
        ));
        assert!(!shows(
            "t/a { q3map_surfacelight 750 { map $lightmap } { map t/a blendFunc filter } \
             { map t/g blendFunc GL_ONE GL_ONE } }"
        ));
        assert!(shows(
            "t/a { q3map_surfacelight 3000 { map $lightmap } { map t/a blendFunc filter } \
             { map t/g blendFunc GL_ONE GL_ONE glow } }"
        ));
        // Reflections are not light.
        assert!(!shows(
            "t/a { { map $lightmap } { map t/a blendFunc filter } \
             { map t/env tcGen environment blendFunc add } }"
        ));
    }

    #[test]
    fn generated_packs_report_their_generation() {
        let mut vfs = VirtualFileSystem::new();
        assert_eq!(pack_generation(&vfs), None);
        vfs.mount_memory("old", [(MANIFEST, br#"{"version": "0.1.0"}"#.to_vec())])
            .expect("mounts");
        assert_eq!(pack_generation(&vfs), Some(1));
        vfs.mount_memory(
            "new",
            [(
                MANIFEST,
                format!(r#"{{"generation": {GENERATION}}}"#).into_bytes(),
            )],
        )
        .expect("mounts");
        assert_eq!(pack_generation(&vfs), Some(GENERATION));
    }

    #[test]
    fn params_match_the_wgsl_uniform_size() {
        assert_eq!(std::mem::size_of::<Params>(), 48);
    }
}

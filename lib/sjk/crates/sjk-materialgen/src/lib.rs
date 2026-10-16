//! `sjk-materialgen`: a local generator of material maps for the world
//! textures of an installed Jedi Academy, in the convention of OpenJK's rend2
//! renderer that SJK's optional material maps read (`r_normalMapping`,
//! `r_specularMapping`, `r_parallaxMapping`).
//!
//! # What it does
//!
//! 1. Mounts the game data like the client ([`mount`]): `GameData/base`, an
//!    optional `fs_game` directory and `SJK_CONTENT`, loose files and pk3s in
//!    Quake 3 order, case-insensitive. Its own earlier output is left out.
//! 2. Reads the shader lumps and surfaces of the installed maps (or `--maps`)
//!    and the shader scripts, and picks the diffuse images of shaders drawn on
//!    lightmapped surfaces whose lightmap and diffuse stages collapse into one
//!    opaque pass: the stages the renderer gives maps to ([`select`]). Skies,
//!    fog, liquids, nodraw/clip/system shaders, interface images, lightmaps,
//!    blend-only effects, deforms, glowing, animated or environment-mapped
//!    stages, alpha-tested foliage and textures that already have rend2 maps
//!    are skipped, each with a recorded reason.
//! 3. Generates, per texture and deterministically ([`generate`]), a
//!    tangent-space normal map from a height estimated from the albedo
//!    (multi-scale high-pass that drops baked lighting gradients, Scharr
//!    gradients, wrap-around filtering so tiling textures stay seamless), and a
//!    packed roughness/metalness/occlusion map from the material class
//!    ([`classes`], the one table to tune) and local image statistics.
//! 4. Writes one pk3 ([`package`]) of PNGs at the names rend2's automatic
//!    lookup tries next to the diffuse image: `<texture>_nh.png` (normal RGB,
//!    height in alpha) for parallax-worthy classes, `<texture>_n.png`
//!    otherwise, and `<texture>_rmo.png`, plus `sjk-materialgen/manifest.json`.
//! 5. Writes `<texture>_e.png`, an emission map, for textures that evidently give
//!    light (surface lights, authored glow images, fixture and screen names) and
//!    only for their luminous texels ([`emission`]); shaders that already show
//!    their light through a glowing or additive stage get none.
//!
//! # Why `_rmo`
//!
//! rend2 reads two specular conventions: `_specGloss` (specular colour and
//! gloss, which the client passes through rend2's SDR colour-ratio conversion) and the
//! packed `_rmo`/`_orm` (roughness, metalness, occlusion). The heuristics
//! produce roughness and metalness directly, and the packed path uses the
//! albedo as the metal colour and a 0.04 dielectric reflectance on its own, so
//! `_rmo` needs no colour guesswork and no conversion to undo. Its red,
//! green and blue are roughness, metalness and occlusion; the client reorders them to
//! occlusion, roughness, metalness at load, as rend2's swizzle does.
//! In that path metal loses its diffuse light; the client's reflection probes
//! (`r_cubeMapping`) reflect the room back into it, so the metal class is
//! mostly metallic (0.8) and brushed (roughness 0.3). Without probes such
//! metal reads darker than its retail look.
//!
//! # Tuning
//!
//! Shaders with a `tcGen environment` stage are stock polish and get a glossier
//! class ([`classes::polished`]). A text file of per-texture rules
//! ([`overrides`], `--overrides`) sets the class, roughness, metalness or
//! height of textures the heuristics get wrong. The manifest records the
//! generation of the tuning ([`package::GENERATION`]); the client reports a pack
//! from an older one, which should be regenerated.
//!
//! # Using the output
//!
//! The default output is `generated/zzz_sjk_materials.pk3` in the client's folder
//! (`GameData/SJK` when the client keeps its `config.cfg` there, else the per-user
//! SJK folder, `%APPDATA%\SJK` on Windows), never elsewhere in the game folder. The
//! client mounts the packs of both `generated` folders by itself, above every game,
//! server and downloaded pack, and then leaves out older generated packs (known by
//! their manifest) copied into a game directory. Output written elsewhere needs
//! `SJK_CONTENT` set to its directory. Then set `r_normalMapping 1`, `r_specularMapping 1`
//! and optionally `r_parallaxMapping 1` and restart. Emission maps need only
//! `r_emissiveMaps`, which is on by default.
//!
//! # Retail data
//!
//! The generated images are derived from retail textures. They stay on the
//! player's machine and must never be shared or committed; the tests use
//! synthetic images only.

pub mod classes;
pub mod cli;
pub mod emission;
pub mod filters;
pub mod generate;
pub mod mount;
pub mod overrides;
pub mod overrides_file;
pub mod package;
pub mod run;
pub mod select;

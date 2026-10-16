//! rd-vanilla `CollapseMultitexture` rules on hand-written shader texts.
use super::*;
use sjk_shader::{StageColour, parse_shader_script};

fn stages(body: &str) -> Vec<ShaderStage> {
    let script = format!("textures/test/collapse\n{{\n{body}\n}}\n");
    let mut definitions =
        parse_shader_script(script.as_bytes(), "shaders/collapse_test.shader").unwrap();
    definitions.remove(0).stages
}

fn compile(body: &str) -> Vec<CompiledStage> {
    collapse_multitexture(&stages(body))
}

/// The operator and output blend of the first pass, if its first two stages merged.
fn merged(body: &str) -> Option<(CollapseOperator, StageBlend)> {
    let compiled = compile(body);
    compiled[0]
        .secondary
        .is_some()
        .then(|| (compiled[0].combine, compiled[0].output_blend.clone()))
}

fn pair(first: &str, second: &str) -> Option<(CollapseOperator, StageBlend)> {
    merged(&format!("{{\n{first}\n}}\n{{\n{second}\n}}"))
}

const MODULATE_SOURCE: &str = "blendFunc GL_DST_COLOR GL_ZERO";
const MODULATE_DESTINATION: &str = "blendFunc GL_ZERO GL_SRC_COLOR";

#[test]
fn unset_rgbgen_equals_explicit_identity_for_a_lightmap_pair() {
    // The layout of the glossyBase family from issue #69: the lightmap stage
    // leaves rgbGen unset, the texture stage spells out `rgbGen identity`.
    let compiled = compile(
        "{ map $lightmap tcGen lightmap }
         { map textures/test/base blendFunc GL_DST_COLOR GL_ZERO rgbGen identity }
         { map textures/test/env blendFunc GL_ONE GL_ONE_MINUS_SRC_COLOR rgbGen identity tcGen environment }",
    );
    assert_eq!(compiled.len(), 2);
    let pass = &compiled[0];
    assert_eq!(pass.combine, CollapseOperator::Modulate);
    assert_eq!(pass.output_blend, StageBlend::Replace);
    assert!(pass.output_depth_write);
    assert_eq!(pass.primary.images, ["textures/test/base"]);
    let secondary = pass.secondary.as_ref().unwrap();
    assert_eq!(secondary.texture_generator, TextureGenerator::Lightmap);
    // The collapsed pass keeps stage 0's colour after the bundle swap.
    assert_eq!(pass.primary.rgb_generator, None);
    assert!(compiled[1].secondary.is_none());
}

#[test]
fn all_eight_blend_pairs_collapse() {
    use CollapseOperator::{Add, Modulate};
    let map = "map textures/test/a";
    let identity = "rgbGen identity";
    let cases = [
        ("", MODULATE_DESTINATION, (Modulate, StageBlend::Replace)),
        ("", MODULATE_SOURCE, (Modulate, StageBlend::Replace)),
        ("", "blendFunc filter", (Modulate, StageBlend::Replace)),
        (
            MODULATE_SOURCE,
            MODULATE_SOURCE,
            (Modulate, StageBlend::Filter),
        ),
        (
            MODULATE_DESTINATION,
            MODULATE_SOURCE,
            (Modulate, StageBlend::Filter),
        ),
        (
            MODULATE_SOURCE,
            MODULATE_DESTINATION,
            (Modulate, StageBlend::Filter),
        ),
        (
            MODULATE_DESTINATION,
            MODULATE_DESTINATION,
            (Modulate, StageBlend::Filter),
        ),
        ("", "blendFunc add", (Add, StageBlend::Replace)),
        (
            "blendFunc GL_ONE GL_ONE",
            "blendFunc add",
            (Add, StageBlend::Add),
        ),
    ];
    for (first, second, expected) in cases {
        assert_eq!(
            pair(
                &format!("{map} {first} {identity}"),
                &format!("{map} {second} {identity}")
            ),
            Some(expected),
            "{first:?} then {second:?}"
        );
    }
}

#[test]
fn other_blend_pairs_stay_separate() {
    let map = "map textures/test/a rgbGen identity";
    for (first, second) in [
        ("", "blendFunc blend"),
        ("", "blendFunc GL_ONE GL_ONE_MINUS_SRC_COLOR"),
        ("", "blendFunc GL_DST_COLOR GL_SRC_COLOR"),
        ("", "blendFunc GL_DST_COLOR GL_ONE"),
        ("blendFunc add", "blendFunc filter"),
        ("blendFunc filter", "blendFunc add"),
        ("blendFunc blend", "blendFunc filter"),
        ("blendFunc filter", ""),
        ("blendFunc add", ""),
        ("", ""),
    ] {
        assert_eq!(
            pair(&format!("{map} {first}"), &format!("{map} {second}")),
            None,
            "{first:?} then {second:?}"
        );
    }
}

#[test]
fn unset_rgbgen_defaults_by_blend_source() {
    let colour =
        |stage: &str| stages(&format!("{{ map textures/test/a {stage} }}"))[0].resolved_colour;
    let rgb = |stage: &str| colour(stage).rgb;
    assert_eq!(rgb(""), RgbGen::Identity);
    assert_eq!(rgb("blendFunc filter"), RgbGen::Identity);
    assert_eq!(rgb(MODULATE_SOURCE), RgbGen::Identity);
    assert_eq!(rgb(MODULATE_DESTINATION), RgbGen::Identity);
    assert_eq!(rgb("blendFunc add"), RgbGen::IdentityLighting);
    assert_eq!(rgb("blendFunc blend"), RgbGen::IdentityLighting);
    // GL_ONE GL_ZERO renders like no blend but still defaults to identityLighting.
    assert_eq!(rgb("blendFunc GL_ONE GL_ZERO"), RgbGen::IdentityLighting);
    assert_eq!(
        rgb("blendFunc GL_SRC_ALPHA GL_ONE"),
        RgbGen::IdentityLighting
    );
    // The last blendFunc decides; an unknown rgbGen name leaves the default.
    assert_eq!(rgb("blendFunc add blendFunc filter"), RgbGen::Identity);
    assert_eq!(rgb("rgbGen sparkle"), RgbGen::Identity);
    assert_eq!(colour(""), StageColour::IDENTITY);
    assert_eq!(colour("alphaGen identity"), StageColour::IDENTITY);
    assert_eq!(colour("rgbGen lightingDiffuse").alpha, AlphaGen::Skip);
    assert_eq!(colour("blendFunc add").alpha, AlphaGen::Identity);
}

#[test]
fn defaulted_generators_decide_the_collapse() {
    use CollapseOperator::{Add, Modulate};
    let a = "map textures/test/a";
    // Unset and explicit identity agree in both orders.
    assert!(
        pair(
            &format!("{a} rgbGen identity"),
            &format!("{a} blendFunc filter")
        )
        .is_some()
    );
    assert!(pair(a, &format!("{a} blendFunc filter rgbGen identity")).is_some());
    assert!(pair(a, &format!("{a} blendFunc filter alphaGen identity")).is_some());
    // An unset add stage is identityLighting, so neither add pair merges unset.
    assert_eq!(pair(a, &format!("{a} blendFunc add")), None);
    assert_eq!(
        pair(&format!("{a} blendFunc add"), &format!("{a} blendFunc add")),
        None
    );
    assert_eq!(
        pair(a, &format!("{a} blendFunc add rgbGen identity")),
        Some((Add, StageBlend::Replace))
    );
    // GL_ONE GL_ZERO defaults to identityLighting, unlike an unblended stage.
    assert_eq!(
        pair(
            &format!("{a} blendFunc GL_ONE GL_ZERO"),
            &format!("{a} blendFunc filter")
        ),
        None
    );
    assert_eq!(
        pair(
            &format!("{a} blendFunc GL_ONE GL_ZERO"),
            &format!("{a} blendFunc filter rgbGen identityLighting")
        ),
        Some((Modulate, StageBlend::Replace))
    );
}

#[test]
fn add_collapse_needs_identity_colour() {
    let a = "map textures/test/a";
    for generator in [
        "identityLighting",
        "vertex",
        "exactVertex",
        "entity",
        "const ( 1 1 1 )",
    ] {
        let rgb = format!("rgbGen {generator}");
        assert_eq!(
            pair(&format!("{a} {rgb}"), &format!("{a} blendFunc add {rgb}")),
            None,
            "{generator}"
        );
        assert!(
            pair(
                &format!("{a} {rgb}"),
                &format!("{a} blendFunc filter {rgb}")
            )
            .is_some(),
            "{generator}"
        );
    }
}

#[test]
fn colour_generators_must_match() {
    let a = "map textures/test/a";
    let filter = "blendFunc filter";
    assert_eq!(
        pair(&format!("{a} rgbGen vertex"), &format!("{a} {filter}")),
        None
    );
    assert_eq!(
        pair(
            &format!("{a} rgbGen vertex"),
            &format!("{a} {filter} rgbGen exactVertex")
        ),
        None
    );
    assert_eq!(
        pair(&format!("{a} alphaGen vertex"), &format!("{a} {filter}")),
        None
    );
    // `rgbGen vertex` implies vertex alpha unless a later alphaGen overrides it.
    assert!(
        pair(
            &format!("{a} rgbGen vertex"),
            &format!("{a} {filter} rgbGen vertex alphaGen vertex")
        )
        .is_some()
    );
    assert!(
        pair(
            &format!("{a} rgbGen vertex"),
            &format!("{a} {filter} alphaGen identity rgbGen vertex")
        )
        .is_some()
    );
    assert_eq!(
        pair(
            &format!("{a} rgbGen vertex"),
            &format!("{a} {filter} rgbGen vertex alphaGen identity")
        ),
        None
    );
}

#[test]
fn waveforms_compare_only_for_wave_generators() {
    let a = "map textures/test/a";
    let f = "blendFunc filter";
    let wave = "rgbGen wave sin 0.5 0.5 0 1";
    assert!(pair(&format!("{a} {wave}"), &format!("{a} {f} {wave}")).is_some());
    assert_eq!(
        pair(
            &format!("{a} {wave}"),
            &format!("{a} {f} rgbGen wave sin 0.5 0.5 0 2")
        ),
        None
    );
    // An unknown function is sine in rd-vanilla.
    assert!(
        pair(
            &format!("{a} {wave}"),
            &format!("{a} {f} rgbGen wave wobble 0.5 0.5 0 1")
        )
        .is_some()
    );
    let alpha = "alphaGen wave square 0 1 0 1";
    assert!(pair(&format!("{a} {alpha}"), &format!("{a} {f} {alpha}")).is_some());
    assert_eq!(
        pair(
            &format!("{a} {alpha}"),
            &format!("{a} {f} alphaGen wave triangle 0 1 0 1")
        ),
        None
    );
}

#[test]
fn constant_colours_are_not_compared_and_stage_zero_keeps_its_colour() {
    let compiled = compile(
        "{ map $lightmap rgbGen const ( 1 0 0 ) }
         { map textures/test/a blendFunc filter rgbGen const ( 0 1 0 ) }",
    );
    assert_eq!(compiled.len(), 1);
    assert_eq!(compiled[0].primary.images, ["textures/test/a"]);
    assert_eq!(compiled[0].primary.rgb_constant, Some([1.0, 0.0, 0.0]));
}

#[test]
fn state_other_than_blend_and_depth_write_must_match() {
    let a = "map textures/test/a rgbGen identity";
    let f = "blendFunc filter";
    assert_eq!(
        pair(&format!("{a} alphaFunc GE128"), &format!("{a} {f}")),
        None
    );
    assert_eq!(
        pair(
            &format!("{a} alphaFunc GE128"),
            &format!("{a} {f} alphaFunc GT0")
        ),
        None
    );
    assert!(
        pair(
            &format!("{a} alphaFunc GE128"),
            &format!("{a} {f} alphaFunc ge128")
        )
        .is_some()
    );
    // An invalid alphaFunc name leaves the test off.
    assert!(pair(&format!("{a} alphaFunc GE100"), &format!("{a} {f}")).is_some());
    assert_eq!(
        pair(&format!("{a} depthFunc equal"), &format!("{a} {f}")),
        None
    );
    assert!(
        pair(
            &format!("{a} depthFunc equal"),
            &format!("{a} {f} depthFunc equal")
        )
        .is_some()
    );
    // Depth-write is masked out; the pass keeps stage 0's.
    let compiled = compile(&format!("{{ {a} {f} }} {{ {a} {f} depthWrite }}"));
    assert!(compiled[0].secondary.is_some());
    assert!(!compiled[0].output_depth_write);
}

#[test]
fn texture_coordinates_are_not_compared() {
    let compiled = compile(
        "{ map textures/test/a tcMod scroll 0.1 0 tcMod scale 2 2 }
         { map textures/test/b blendFunc filter tcGen environment tcMod rotate 10 }",
    );
    assert_eq!(compiled.len(), 1);
    let secondary = compiled[0].secondary.as_ref().unwrap();
    assert_eq!(compiled[0].primary.texture_modifications.len(), 2);
    assert_eq!(secondary.texture_generator, TextureGenerator::Environment);
    assert_eq!(secondary.texture_modifications.len(), 1);
}

#[test]
fn only_the_first_two_stages_collapse() {
    // A leading stage that cannot merge keeps a later lightmap pair apart.
    let compiled = compile(
        "{ map textures/test/glow blendFunc add }
         { map $lightmap blendFunc filter }
         { map textures/test/a blendFunc filter }",
    );
    assert_eq!(compiled.len(), 3);
    assert!(compiled.iter().all(|pass| pass.secondary.is_none()));
    // After a merge, later stages are not paired again.
    let compiled = compile(
        "{ map $lightmap }
         { map textures/test/a blendFunc filter }
         { map textures/test/b blendFunc filter }
         { map textures/test/c blendFunc filter }",
    );
    assert_eq!(compiled.len(), 3);
    assert!(compiled[0].secondary.is_some());
    assert!(compiled[1..].iter().all(|pass| pass.secondary.is_none()));
}

#[test]
fn lightmap_moves_to_the_second_bundle() {
    let compiled = compile("{ map textures/test/a } { map $lightmap blendFunc filter }");
    assert_eq!(compiled[0].primary.images, ["textures/test/a"]);
    assert_eq!(
        compiled[0].secondary.as_ref().unwrap().texture_generator,
        TextureGenerator::Lightmap
    );
}

#[test]
fn unscripted_lightmapped_surfaces_collapse() {
    let (stages, _, _) = material_stages(None, 0);
    let compiled = collapse_multitexture(&stages);
    assert_eq!(compiled.len(), 1);
    assert_eq!(compiled[0].combine, CollapseOperator::Modulate);
    assert_eq!(compiled[0].output_blend, StageBlend::Replace);
}

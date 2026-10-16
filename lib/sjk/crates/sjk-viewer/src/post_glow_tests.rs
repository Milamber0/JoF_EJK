//! Dynamic glow settings, blur sizes and kernels, and the programs that run them.
use super::*;
use wgpu::naga;

fn registry() -> (CvarRegistry, Settings) {
    let mut cvars = CvarRegistry::new();
    let settings = Settings::bind(&mut cvars).unwrap();
    (cvars, settings)
}

#[test]
fn cvars_register_archived_with_stock_names_and_defaults() {
    let (cvars, settings) = registry();
    for (name, value) in [
        ("r_DynamicGlow", CvarValue::Integer(1)),
        ("r_DynamicGlowPasses", CvarValue::Integer(5)),
        ("r_DynamicGlowDelta", CvarValue::Float(0.8)),
        ("r_DynamicGlowIntensity", CvarValue::Float(1.13)),
        ("r_DynamicGlowSoft", CvarValue::Integer(1)),
        ("r_DynamicGlowWidth", CvarValue::Integer(0)),
        ("r_DynamicGlowHeight", CvarValue::Integer(0)),
        ("r_DynamicGlowScale", CvarValue::Float(0.25)),
        ("r_dynamicGlowStyle", CvarValue::Integer(1)),
    ] {
        // The retail menu writes `r_dynamicglow`: lookups ignore case.
        let cvar = cvars.get(&name.to_ascii_lowercase()).unwrap();
        assert_eq!(cvar.value, value, "{name}");
        assert!(cvar.flags.contains(CvarFlags::ARCHIVE), "{name}");
    }
    assert_eq!(settings.policy(), Policy::default());
    assert_eq!(settings.live(), Live::default());
}

#[test]
fn edits_reach_the_policy_and_the_frame_values() {
    let (mut cvars, settings) = registry();
    cvars.set_text("r_dynamicglow", "2").unwrap();
    cvars.set_text("r_DynamicGlowPasses", "7").unwrap();
    cvars.set_text("r_DynamicGlowDelta", "1.5").unwrap();
    cvars.set_text("r_DynamicGlowIntensity", "1.4").unwrap();
    cvars.set_text("r_DynamicGlowSoft", "0").unwrap();
    cvars.set_text("r_DynamicGlowWidth", "320").unwrap();
    cvars.set_text("r_DynamicGlowHeight", "200").unwrap();
    cvars.set_text("r_DynamicGlowScale", "0.5").unwrap();
    cvars.set_text("r_dynamicGlowStyle", "0").unwrap();
    assert_eq!(
        settings.live(),
        Live {
            mode: Mode::Sabers,
            passes: 7,
            delta: 1.5,
            intensity: 1.4,
            soft: false,
        }
    );
    assert_eq!(
        settings.policy(),
        Policy {
            enabled: true,
            style: Style::Retail,
            width: 320,
            height: 200,
            scale: 0.5,
        }
    );
    cvars.set_text("r_DynamicGlow", "0").unwrap();
    assert!(!settings.policy().enabled);
    // Pass counts stay within what one frame records.
    cvars.set_text("r_DynamicGlowPasses", "1000").unwrap();
    assert_eq!(settings.live().passes, MAX_PASSES);
    cvars.set_text("r_DynamicGlowPasses", "-3").unwrap();
    assert_eq!(settings.live().passes, 1);
    // Settings shared with a world install see the same edits.
    let shared = settings.clone();
    cvars.set_text("r_DynamicGlow", "3").unwrap();
    assert_eq!(shared.live().mode, Mode::GlowOnly);
}

#[test]
fn modes_follow_stock_and_jof() {
    assert_eq!(Mode::from_cvar(0), Mode::Off);
    assert_eq!(Mode::from_cvar(1), Mode::On);
    assert_eq!(Mode::from_cvar(2), Mode::Sabers);
    assert_eq!(Mode::from_cvar(3), Mode::GlowOnly);
    // Any other nonzero value is on, as `r_DynamicGlow->integer` tests are.
    assert_eq!(Mode::from_cvar(4), Mode::On);
    assert_eq!(Mode::from_cvar(-1), Mode::On);
}

#[test]
fn retail_blur_size_follows_scale_or_both_overrides() {
    let frame = [1920, 1080];
    assert_eq!(retail_size(frame, Policy::default()), [480, 270]);
    let sized = |width, height, scale| Policy {
        width,
        height,
        scale,
        ..Policy::default()
    };
    assert_eq!(retail_size(frame, sized(320, 200, 0.25)), [320, 200]);
    // One override alone is ignored (`tr_image.cpp:1559`).
    assert_eq!(retail_size(frame, sized(320, 0, 0.5)), [960, 540]);
    // Never larger than the frame, never empty.
    assert_eq!(retail_size(frame, sized(4000, 3000, 0.25)), frame);
    assert_eq!(retail_size(frame, sized(0, 0, 0.0)), [1, 1]);
    assert_eq!(retail_size(frame, sized(0, 0, f32::NAN)), [480, 270]);
}

#[test]
fn vulkan_pyramid_halves_four_times() {
    assert_eq!(
        vulkan_levels([3840, 2160]),
        [[1920, 1080], [960, 540], [480, 270], [240, 135]]
    );
    assert_eq!(
        vulkan_levels([1919, 1079]),
        [[959, 539], [479, 269], [239, 134], [119, 67]]
    );
    assert_eq!(vulkan_levels([3, 1]), [[1, 1]; 4]);
}

#[test]
fn retail_kernel_spreads_by_delta_and_gains_intensity_per_pass() {
    let offsets: Vec<f32> = (0..5).map(|pass| retail_offset(pass, 0.8)).collect();
    for (offset, expected) in offsets.iter().zip([0.1, 0.9, 1.7, 2.5, 3.3]) {
        assert!((offset - expected).abs() < 1e-6, "{offset} {expected}");
    }
    // Four taps of Intensity / 4: a flat image gains Intensity per pass before clamping.
    assert!((4.0 * retail_weight(1.13) - 1.13).abs() < 1e-6);
    assert!((retail_weight(1.0) - 0.25).abs() < 1e-6);
}

#[test]
fn vulkan_kernel_matches_blur_frag_and_the_level_factor() {
    let [center, side] = vulkan_weights();
    assert!((center - (0.375 + 0.15)).abs() < 1e-6);
    assert!((side - (0.3125 + 0.15)).abs() < 1e-6);
    // Each one-dimensional pass gains 1.45: the correction brightens every level.
    assert!((center + 2.0 * side - 1.45).abs() < 1e-6);
    assert!((vulkan_factor(1.13) - 0.13).abs() < 1e-6);
    assert!((vulkan_factor(0.5) - 0.01).abs() < 1e-6);
    assert!((vulkan_factor(9.0) - 4.0).abs() < 1e-6);
}

#[test]
fn stage_programs_draw_display_values_into_the_glow_image() {
    use wgpu::TextureFormat::*;
    assert_eq!(world_format(Bgra8UnormSrgb), Rgba8UnormSrgb);
    assert_eq!(world_format(Rgba16Float), Rgba8UnormSrgb);
    assert_eq!(world_format(Bgra8Unorm), Rgba8Unorm);
}

fn validate(label: &str, source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(source)));
    module
}

fn entry_points(module: &naga::Module) -> Vec<&str> {
    module
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect()
}

#[test]
fn blur_program_validates_with_every_pass() {
    let module = validate("post_glow", include_str!("post_glow.wgsl"));
    let entries = entry_points(&module);
    for entry in ["vs_main", "retail", "vulkan_blur", "vulkan_combine"] {
        assert!(entries.contains(&entry), "{entry}");
    }
    // The parameter block's pass array matches the uploaded one.
    assert!(
        include_str!("post_glow.wgsl")
            .contains(&format!("passes: array<vec4<f32>, {}>", LEVELS * 2 + 1))
    );
}

#[test]
fn resolve_program_validates_with_the_glow_composite() {
    let source = concat!(include_str!("post_aa.wgsl"), include_str!("post_hdr.wgsl"));
    validate("post_aa", source);
    // Glow joins after the effect layer and before the r_gamma ramp.
    let output = &source[source.find("fn output_color").unwrap()..];
    let glow = output.find("with_glow(with_effects(").unwrap();
    assert!(glow < output.find("R_SetColorMappings").unwrap());
    // Its bindings stay clear of the resolve's others.
    for binding in [
        "@binding(8) var glow_image",
        "@binding(9) var<uniform> glow_controls",
    ] {
        assert!(source.contains(binding), "{binding}");
    }
}

#[test]
fn saber_glow_draws_the_glow_capsule_without_the_core() {
    let source = include_str!("saber.wgsl");
    let module = validate("saber", source);
    assert!(entry_points(&module).contains(&"fragment_glow"));
    // A core instance carries zero hilt radius (`Instance::pair_flickering`).
    let glow = &source[source.find("fn fragment_glow").unwrap()..];
    let body = &glow[..glow.find("\n}").unwrap()];
    assert!(body.contains("if input.hilt <= 0.0 { discard; }"));
    assert!(body.contains("glow_capsule(input)"));
}

#[test]
fn retail_saber_shaders_glow_on_the_blade_and_not_the_core() {
    // assets1.pk3 sabers.shader: the six `*_glow` sprites carry `glow`, the `*_line`
    // cores do not, and the blur trails glow.
    let script = "gfx/effects/sabers/blue_glow\n{\n cull twosided\n {\n map gfx/effects/sabers/blue_glow2\n blendFunc GL_ONE GL_ONE\n glow\n rgbGen vertex\n }\n}\n\
        gfx/effects/sabers/blue_line\n{\n cull twosided\n {\n map gfx/effects/sabers/blue_line\n blendFunc GL_ONE GL_ONE\n rgbGen vertex\n }\n}\n\
        gfx/effects/sabers/saberBlur\n{\n cull twosided\n {\n clampmap gfx/effects/sabers/blurglow\n blendFunc GL_ONE GL_ONE\n glow\n rgbGen vertex\n }\n}\n";
    let definitions =
        sjk_shader::parse_shader_script(script.as_bytes(), "shaders/sabers.shader").unwrap();
    let glow: Vec<bool> = definitions
        .iter()
        .map(|definition| definition.stages.iter().any(|stage| stage.glow))
        .collect();
    assert_eq!(glow, [true, false, true]);
}

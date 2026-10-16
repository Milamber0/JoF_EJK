use super::*;

/// A made-up blade skin for the tests (no real skin's values): a teal blade.
pub(crate) const SAMPLE: &str = r#"{
  "version": 1,
  "glow_profile": {"width": 0.4, "peak": 0.5, "tail_width": 0.9, "tail_peak": 0.05},
  "core_profile": {"width": 0.3, "fringe_width": 0.6},
  "core": {
    "white": [0.9, 1.0, 1.0],
    "white_flare": 0.5,
    "fringe_cool": [0.1, 0.5, 0.6],
    "fringe_hot": [0.3, 0.9, 0.8],
    "fringe_heat": {"base": 0.25, "grain": 0.5},
    "fringe_brightness": {"base": 0.8, "grain": 0.4, "flare": 0.9},
    "breathe": {"amount": 0.05, "rate": 8.0, "along": 0.5, "seed": 3.0}
  },
  "corona": {
    "rim_cool": [0.0, 0.2, 0.4],
    "rim_hot": [0.1, 0.5, 0.7],
    "inner": [0.4, 0.9, 0.9],
    "inner_width": 0.7,
    "rim_heat": {"grain": 1.0, "flare": 0.5},
    "inner_mix": {"base": 0.5, "grain": 0.4, "flare": 0.2},
    "brightness": {"base": 0.6, "grain": 0.5, "flare": 1.0},
    "reach": 1.5,
    "swell": {"grain": 0.1, "flare": 0.3}
  },
  "granulation": {
    "octaves": [
      {"scale": 0.3, "speed": 1.5, "evolve": 0.4, "offset": 50.0, "weight": 0.7},
      {"scale": 0.9, "speed": 3.0, "evolve": 0.8, "offset": 90.0, "weight": 0.3}
    ],
    "low": 0.2,
    "high": 0.8
  },
  "flares": {
    "count": 2, "rate": 0.5, "rate_step": 0.1, "rate_seed": 0.05,
    "phase_seed": 5.0, "phase_step": 0.4, "threshold": 0.5,
    "overshoot": 4.0, "size": 3.0, "size_jitter": 1.0
  },
  "shimmer": [{"amount": 0.04, "rate": 8.0, "along": 0.6, "seed": 2.0}],
  "tongues": {
    "along": 0.4, "offset": 11.0, "out": 2.0, "speed": 3.0,
    "edge_low": 0.3, "edge_high": 0.8, "low": 0.4, "range": 1.2
  },
  "trail": [0.2, 0.8, 0.9],
  "light": {
    "color": [0.2, 0.7, 0.9],
    "flicker": {"amount": 0.1, "waves": [{"rate": 6.0, "weight": 0.5, "phase": 1.0}]}
  },
  "sounds": {
    "on": "sound/test/blade/on.wav",
    "off": "sound/test/blade/off.wav",
    "hum": "sound/test/blade/hum.wav",
    "swings": ["sound/test/blade/s1.wav", "sound/test/blade/s2.wav", "sound/test/blade/s3.wav"]
  }
}"#;

/// The made-up sections of the optional effects: a rounded tip of 2.5 half-widths,
/// lightning arcs, motes and a turning hue (made-up values, no real skin's).
pub(crate) const EFFECTS: &str = r#""arcs": {
    "count": 3, "color": [0.7, 0.8, 1.0], "brightness": 2.0, "width": 0.15, "halo": 0.5,
    "reach": 1.1, "jag": 0.4, "kinks": 0.6, "span": {"low": 0.2, "high": 0.5},
    "rate": 6.0, "jitter": 20.0, "threshold": 0.3, "tip": 0.25, "crawl": 12.0, "decay": 1.5
  },
  "motes": {
    "color": [0.9, 0.9, 1.0], "brightness": 1.5, "density": 0.3, "cells": 0.8,
    "rings": 2.5, "size": 0.2, "stretch": 2.0, "drift": {"along": -2.0, "out": 1.5},
    "twinkle": 4.0, "inner": 0.4, "outer": 1.7, "focus": 0.6
  },
  "hue": {"rate": 0.1, "along": 0.02, "out": 0.2},
  "trail": [0.2, 0.8, 0.9],"#;

/// [`SAMPLE`] with [`EFFECTS`] and `core.tip` 2.5.
pub(crate) fn sample_with_effects() -> String {
    sample_with(r#""trail": [0.2, 0.8, 0.9],"#, EFFECTS).replacen(
        r#""breathe": {"amount": 0.05, "rate": 8.0, "along": 0.5, "seed": 3.0}"#,
        r#""breathe": {"amount": 0.05, "rate": 8.0, "along": 0.5, "seed": 3.0}, "tip": 2.5"#,
        1,
    )
}

/// The sample with `field`'s text (as written in [`SAMPLE`]) replaced by `with`.
pub(crate) fn sample_with(field: &str, with: &str) -> String {
    assert!(SAMPLE.contains(field), "{field}");
    SAMPLE.replacen(field, with, 1)
}

#[test]
fn the_sample_reads_with_every_value() {
    let def = parse("saber_test", SAMPLE).unwrap();
    assert_eq!(def.version, 1);
    assert_eq!(def.glow_profile.width, 0.4);
    assert_eq!(def.core.white, [0.9, 1.0, 1.0]);
    assert_eq!(def.corona.reach, 1.5);
    assert_eq!(def.granulation.octaves.len(), 2);
    assert_eq!(def.flares.count, 2);
    assert_eq!(def.shimmer.len(), 1);
    assert_eq!(def.light.flicker.waves[0].rate, 6.0);
    assert_eq!(def.sounds.swings[2], "sound/test/blade/s3.wav");
    assert_eq!(def.glow_image, None);
}

#[test]
fn absent_profiles_are_the_neutral_pairs() {
    let text = sample_with(
        r#""glow_profile": {"width": 0.4, "peak": 0.5, "tail_width": 0.9, "tail_peak": 0.05},
  "core_profile": {"width": 0.3, "fringe_width": 0.6},"#,
        r#""glow_image": "gfx/test/glow.png", "core_image": "gfx/test/core.png","#,
    );
    let def = parse("saber_test", &text).unwrap();
    assert_eq!(def.glow_profile, GlowProfile::NEUTRAL);
    assert_eq!(def.core_profile, CoreProfile::NEUTRAL);
    assert_eq!(def.glow_image.as_deref(), Some("gfx/test/glow.png"));
}

#[test]
fn unknown_or_missing_fields_and_bad_values_are_refused_by_name() {
    let refused = |text: &str, says: &str| {
        let why = parse("saber_test", text).unwrap_err();
        assert!(why.contains(says), "{why:?} should say {says:?}");
    };
    refused(
        &sample_with(r#""version": 1,"#, r#""version": 1, "colour": 3,"#),
        "unknown field `colour`",
    );
    refused(
        &sample_with(
            r#""white_flare": 0.5,"#,
            r#""white_flare": 0.5, "glint": 1,"#,
        ),
        "unknown field `glint`",
    );
    refused(
        &sample_with(r#""trail": [0.2, 0.8, 0.9],"#, ""),
        "missing field `trail`",
    );
    refused(
        &sample_with(r#""version": 1"#, r#""version": 2"#),
        "version must be 1",
    );
    refused(
        &sample_with(r#""reach": 1.5"#, r#""reach": 3.0"#),
        "corona.reach must be 1 to 2",
    );
    refused(
        &sample_with(r#""count": 2"#, r#""count": 9"#),
        "flares.count must be 0 to 8",
    );
    refused(
        &sample_with(r#""trail": [0.2, 0.8, 0.9]"#, r#""trail": [0.2, 1.8, 0.9]"#),
        "trail green must be 0 to 1",
    );
    refused(
        &sample_with(r#""low": 0.2"#, r#""low": 0.9"#),
        "granulation.low must be below",
    );
    refused(
        &sample_with(r#""edge_low": 0.3"#, r#""edge_low": 0.9"#),
        "tongues.edge_low must be below",
    );
    refused(
        &sample_with(r#""on": "sound/test/blade/on.wav""#, r#""on": "../on.wav""#),
        "sounds.on must be a game path",
    );
    refused(
        &sample_with(
            r#""on": "sound/test/blade/on.wav""#,
            r#""on": "/sound/on.wav""#,
        ),
        "sounds.on must be a game path",
    );
    refused(
        &sample_with(
            r#""shimmer": [{"amount": 0.04, "rate": 8.0, "along": 0.6, "seed": 2.0}]"#,
            r#""shimmer": [{"amount": 0.04, "rate": 8.0, "along": 0.6, "seed": 2.0},
              {"amount": 0.04, "rate": 8.0, "along": 0.6, "seed": 2.0},
              {"amount": 0.04, "rate": 8.0, "along": 0.6, "seed": 2.0}]"#,
        ),
        "shimmer must hold at most 2",
    );
    refused(
        &sample_with(r#", "sound/test/blade/s3.wav"]"#, "]"),
        "invalid length 2",
    );
    refused("3", "invalid type");
    refused("not json", "expected");
    assert!(
        parse("Saber Test", SAMPLE)
            .unwrap_err()
            .contains("not an unlock id")
    );
}

#[test]
fn the_effects_are_optional_and_absent_ones_draw_nothing() {
    // Without them (as the Sun's file was written): the default tip and no effects.
    let plain = parse("saber_test", SAMPLE).unwrap();
    assert_eq!(plain.core.tip, DEFAULT_TIP);
    assert_eq!((plain.arcs, plain.motes, plain.hue), (None, None, None));
    let def = parse("saber_test", &sample_with_effects()).unwrap();
    assert_eq!(def.core.tip, 2.5);
    let arcs = def.arcs.unwrap();
    assert_eq!((arcs.count, arcs.span.high, arcs.decay), (3, 0.5, 1.5));
    let motes = def.motes.unwrap();
    assert_eq!((motes.drift.along, motes.focus), (-2.0, 0.6));
    assert_eq!(def.hue.unwrap().out, 0.2);
}

#[test]
fn the_effects_are_checked_strictly_by_name() {
    let effects = sample_with_effects();
    let refused = |from: &str, to: &str, says: &str| {
        assert!(effects.contains(from), "{from}");
        let why = parse("saber_test", &effects.replacen(from, to, 1)).unwrap_err();
        assert!(why.contains(says), "{why:?} should say {says:?}");
    };
    refused(
        r#""tip": 2.5"#,
        r#""tip": 0.1"#,
        "core.tip must be 0.5 to 8",
    );
    refused(
        r#""count": 3"#,
        r#""count": 5"#,
        "arcs.count must be 0 to 4",
    );
    refused(
        r#""width": 0.15"#,
        r#""width": 0.0"#,
        "arcs.width must be 0.02 to 1",
    );
    refused(
        r#""span": {"low": 0.2, "high": 0.5}"#,
        r#""span": {"low": 0.6, "high": 0.5}"#,
        "arcs.span.low must be at most arcs.span.high",
    );
    refused(
        r#""rate": 6.0"#,
        r#""rate": 0.0"#,
        "arcs.rate must be 0.05 to 60",
    );
    refused(
        r#""decay": 1.5"#,
        r#""decay": 9.0"#,
        "arcs.decay must be 0 to 8",
    );
    refused(r#", "decay": 1.5"#, "", "missing field `decay`");
    refused(
        r#""jitter": 20.0"#,
        r#""jiter": 20.0"#,
        "unknown field `jiter`",
    );
    refused(
        r#""density": 0.3"#,
        r#""density": 1.3"#,
        "motes.density must be 0 to 1",
    );
    refused(
        r#""stretch": 2.0"#,
        r#""stretch": 3.0"#,
        "motes.size × motes.stretch must be at most 0.5",
    );
    refused(
        r#""outer": 1.7"#,
        r#""outer": 0.3"#,
        "motes.inner must be below",
    );
    refused(
        r#""drift": {"along": -2.0, "out": 1.5}"#,
        r#""drift": {"along": -2.0, "out": 11.0}"#,
        "motes.drift.out must be -10 to 10",
    );
    refused(
        r#""out": 0.2}"#,
        r#""out": 5.0}"#,
        "hue.out must be -4 to 4",
    );
    refused(
        r#""hue": {"rate": 0.1, "along": 0.02, "out": 0.2}"#,
        r#""hue": {"rate": 0.1, "along": 0.02}"#,
        "missing field `out`",
    );
}

#[test]
fn the_folder_loads_by_id_and_names_the_bad_file() {
    let mut vfs = sjk_vfs::VirtualFileSystem::new();
    vfs.mount_memory(
        "pack",
        [
            (
                "skins/blades/saber_test.bladeskin",
                SAMPLE.as_bytes().to_vec(),
            ),
            ("skins/blades/saber_bad.bladeskin", b"{}".to_vec()),
            ("skins/blades/readme.txt", b"no".to_vec()),
        ],
    )
    .unwrap();
    let skins = load(&vfs);
    assert_eq!(skins.len(), 1);
    assert_eq!(skins[0].0, "saber_test");
}

/// A made-up additions file (no real skin's values): a glint, wisps, haze, an echo and
/// options switching them and the sample's arcs and motes.
pub(crate) const ADDITIONS: &str = r#"{
  "star": {"color": [1.0, 0.9, 0.7], "brightness": 1.5, "rays": 6, "length": 8.0,
    "width": 0.05, "spin": 0.1, "twinkle": 2.0, "depth": 0.3, "halo": 0.6},
  "wisps": {"color": [0.6, 0.8, 0.9], "brightness": 0.7, "rise": 9.0, "speed": 5.0,
    "scale": 0.2, "curl": 0.8, "density": 0.5},
  "haze": {"strength": 0.3, "scale": 0.4, "speed": 8.0, "reach": 3.0},
  "echo": {"fade": 0.4, "sway": 2.0, "rate": 0.3, "lean": 0.05},
  "options": [
    {"id": "glint", "name": "Tip glint", "sections": ["star"]},
    {"id": "haze", "name": "Heat haze", "sections": ["haze"]},
    {"id": "smoke", "name": "Wisps and echo", "sections": ["wisps", "echo"]},
    {"id": "storm", "name": "Arcs", "sections": ["arcs", "motes"]}
  ]
}"#;

/// [`sample_with_effects`] with [`ADDITIONS`] laid over it.
pub(crate) fn sample_with_additions() -> String {
    extras::merged(&sample_with_effects(), ADDITIONS).unwrap()
}

#[test]
fn the_additions_file_lays_the_third_set_and_the_options_over_the_skin() {
    let def = parse("saber_test", &sample_with_additions()).unwrap();
    let star = def.star.unwrap();
    assert_eq!((star.rays, star.length, star.halo), (6, 8.0, 0.6));
    assert_eq!(def.wisps.unwrap().rise, 9.0);
    assert_eq!(def.haze.unwrap().reach, 3.0);
    assert_eq!(def.echo.unwrap().sway, 2.0);
    let ids: Vec<&str> = def.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["glint", "haze", "smoke", "storm"]);
    // The skin file's own fields stay; one in both is the addition's.
    assert_eq!(def.trail, [0.2, 0.8, 0.9]);
    let over = extras::merged(&sample_with_effects(), r#"{"trail": [1.0, 0.0, 0.0]}"#).unwrap();
    assert_eq!(parse("saber_test", &over).unwrap().trail, [1.0, 0.0, 0.0]);
    // Either file not an object is refused.
    assert!(extras::merged(&sample_with_effects(), "[1]").is_err());
    assert!(extras::merged("[1]", ADDITIONS).is_err());
    // Without it, none of the third set and no options.
    let plain = parse("saber_test", SAMPLE).unwrap();
    assert!(plain.star.is_none() && plain.wisps.is_none() && plain.haze.is_none());
    assert!(plain.echo.is_none() && plain.options.is_empty());
}

#[test]
fn options_switch_their_sections_and_the_room_follows() {
    use extras::{Section, option_bit};
    let def = parse("saber_test", &sample_with_additions()).unwrap();
    assert_eq!(def.off_mask(0), 0);
    assert_eq!(def.off_mask(option_bit("glint")), Section::Star.bit());
    assert_eq!(
        def.off_mask(option_bit("smoke") | option_bit("storm")),
        Section::Wisps.bit() | Section::Echo.bit() | Section::Arcs.bit() | Section::Motes.bit()
    );
    // An id the file does not offer switches nothing.
    assert_eq!(def.off_mask(option_bit("wings")), 0);
    // The quad's room: the wisps' rise, the haze's reach, whichever is on and further.
    assert_eq!(def.room(0), 9.0);
    assert_eq!(def.room(Section::Wisps.bit()), 3.0);
    assert_eq!(def.room(Section::Wisps.bit() | Section::Haze.bit()), 0.0);
    // Every section's bit is its place in the list (saber.wgsl's OFF_* constants).
    let shader = include_str!("saber.wgsl");
    for (index, section) in Section::ALL.into_iter().enumerate() {
        assert_eq!(section.bit(), 1 << index);
        assert_eq!(Section::named(section.name()), Some(section));
        let constant = format!(
            "const OFF_{}: u32 = {}u;",
            section.name().to_ascii_uppercase(),
            section.bit()
        );
        // Afterimages and the echo are the CPU's (no instance is made for them).
        if !matches!(section, Section::Ghosts | Section::Echo) {
            assert!(shader.contains(&constant), "saber.wgsl lost {constant}");
        }
    }
}

#[test]
fn the_third_set_and_the_options_are_checked_strictly_by_name() {
    let refused = |old: &str, new: &str, why: &str| {
        assert!(ADDITIONS.contains(old), "{old}");
        let text =
            extras::merged(&sample_with_effects(), &ADDITIONS.replacen(old, new, 1)).unwrap();
        let error = parse("saber_test", &text).unwrap_err();
        assert!(error.contains(why), "{new}: {error}");
    };
    refused(r#""rays": 6"#, r#""rays": 1"#, "star.rays must be 2 to 8");
    refused(r#""length": 8.0"#, r#""length": 40.0"#, "star.length");
    refused(r#""rise": 9.0"#, r#""rise": 30.0"#, "wisps.rise");
    refused(r#""reach": 3.0"#, r#""reach": 0.1"#, "haze.reach");
    refused(r#""lean": 0.05"#, r#""lean": 1.0"#, "echo.lean");
    refused(r#""strength": 0.3, "#, "", "missing field `strength`");
    refused(r#""id": "glint""#, r#""id": "Glint""#, "options[0].id");
    refused(r#""id": "haze""#, r#""id": "glint""#, "given twice");
    refused(r#""name": "Tip glint""#, r#""name": """#, "options[0].name");
    refused(
        r#"["star"]"#,
        r#"["plumes"]"#,
        r#"options[0].sections: "plumes" is not a section"#,
    );
    refused(r#"["star"]"#, r#"["glyphs"]"#, "the file has no glyphs");
    refused(
        r#"["haze"]"#,
        r#"["star"]"#,
        "star is already another option's",
    );
    refused(r#"["star"]"#, "[]", "must name 1 to 4 sections");
    // Two ids on one bit (souls and wisps) cannot be told apart in a look.
    let text = ADDITIONS
        .replacen(r#""id": "glint""#, r#""id": "souls""#, 1)
        .replacen(r#""id": "haze""#, r#""id": "wisps""#, 1);
    let text = extras::merged(&sample_with_effects(), &text).unwrap();
    assert!(
        parse("saber_test", &text)
            .unwrap_err()
            .contains("shares its bit")
    );
    // At most six options.
    let many: Vec<String> = (0..7)
        .map(|n| format!(r#"{{"id": "o{n}", "name": "O", "sections": ["arcs"]}}"#))
        .collect();
    let text = format!(r#"{{"options": [{}]}}"#, many.join(","));
    let text = extras::merged(&sample_with_effects(), &text).unwrap();
    assert!(
        parse("saber_test", &text)
            .unwrap_err()
            .contains("at most 6")
    );
}

/// A made-up later additions file (no real skin's values): the glint's orbit.
pub(crate) const LATER_ADDITIONS: &str = r#"{
  "orbit": {"at": 10.0, "count": 2, "radius": 4.5, "rate": 0.4, "tilt": 0.3}
}"#;

#[test]
fn the_orbit_needs_a_star_and_is_checked_strictly() {
    let orbit = |later: &str| {
        let text = extras::merged(&sample_with_additions(), later).unwrap();
        parse("saber_test", &text)
    };
    let def = orbit(LATER_ADDITIONS).unwrap();
    let o = def.orbit.unwrap();
    assert_eq!(
        (o.at, o.count, o.radius, o.rate, o.tilt),
        (10.0, 2, 4.5, 0.4, 0.3)
    );
    // The rest of the file is as the first additions left it.
    assert_eq!(def.star.unwrap().length, 8.0);
    let refused = |old: &str, new: &str, why: &str| {
        assert!(LATER_ADDITIONS.contains(old), "{old}");
        let error = orbit(&LATER_ADDITIONS.replacen(old, new, 1)).unwrap_err();
        assert!(error.contains(why), "{new}: {error}");
    };
    refused(
        r#""count": 2"#,
        r#""count": 0"#,
        "orbit.count must be 1 to 4",
    );
    refused(
        r#""count": 2"#,
        r#""count": 5"#,
        "orbit.count must be 1 to 4",
    );
    refused(r#""at": 10.0"#, r#""at": 50.0"#, "orbit.at");
    refused(r#""radius": 4.5"#, r#""radius": 20.0"#, "orbit.radius");
    refused(r#""rate": 0.4"#, r#""rate": 5.0"#, "orbit.rate");
    refused(r#""tilt": 0.3"#, r#""tilt": 1.5"#, "orbit.tilt");
    refused(r#", "tilt": 0.3"#, "", "missing field `tilt`");
    refused(
        r#""tilt": 0.3"#,
        r#""tilt": 0.3, "wobble": 1"#,
        "unknown field `wobble`",
    );
    // Nothing to circle without a star.
    let text = extras::merged(&sample_with_effects(), LATER_ADDITIONS).unwrap();
    assert!(
        parse("saber_test", &text)
            .unwrap_err()
            .contains("orbit needs a star")
    );
}

/// A made-up later additions file with black holes instead of the glint (no real skin's
/// values): the first file's star goes, the holes circle.
pub(crate) const HOLES: &str = r#"{
  "star": null,
  "holes": {"color": [0.8, 0.6, 1.0], "brightness": 2.0, "size": 1.2, "lens": 1.0,
    "spin": 0.5},
  "orbit": {"at": 12.0, "count": 2, "radius": 5.0, "rate": 0.3, "tilt": 0.25},
  "options": [{"id": "holes", "name": "Black holes", "sections": ["holes"]}]
}"#;

#[test]
fn black_holes_take_the_glints_place_and_are_checked_strictly() {
    let holes = |later: &str| {
        let text = extras::merged(&sample_with_additions(), later).unwrap();
        parse("saber_test", &text)
    };
    let def = holes(HOLES).unwrap();
    let h = def.holes.unwrap();
    assert_eq!((h.size, h.lens, h.spin), (1.2, 1.0, 0.5));
    // The quad reaches the swirl and the lens: 1.2 x (2.5 + 2 x 1).
    assert!((h.reach() - 5.4).abs() < 1e-6);
    assert!(def.star.is_none() && def.orbit.unwrap().count == 2);
    assert!(def.has(extras::Section::Holes));
    let refused = |old: &str, new: &str, why: &str| {
        assert!(HOLES.contains(old), "{old}");
        let error = holes(&HOLES.replacen(old, new, 1)).unwrap_err();
        assert!(error.contains(why), "{new}: {error}");
    };
    refused(r#""size": 1.2"#, r#""size": 0.1"#, "holes.size");
    refused(r#""lens": 1.0"#, r#""lens": 4.0"#, "holes.lens");
    refused(r#""spin": 0.5"#, r#""spin": 9.0"#, "holes.spin");
    refused(
        r#""brightness": 2.0"#,
        r#""brightness": 20.0"#,
        "holes.brightness",
    );
    refused(r#""lens": 1.0,"#, "", "missing field `lens`");
    // A star and holes at once: which is drawn in the glints' places?
    refused(
        r#""star": null,"#,
        "",
        "star and holes may not both be given",
    );
    // Holes without an orbit sit at the tip; an orbit with neither has nothing to circle.
    assert!(holes(&HOLES.replacen(r#""orbit""#, r#""x_orbit""#, 1)).is_err());
    let at_tip = HOLES.replace(
        r#""orbit": {"at": 12.0, "count": 2, "radius": 5.0, "rate": 0.3, "tilt": 0.25},"#,
        "",
    );
    assert!(holes(&at_tip).unwrap().orbit.is_none());
    let text = extras::merged(&sample_with_effects(), LATER_ADDITIONS).unwrap();
    assert!(
        parse("saber_test", &text)
            .unwrap_err()
            .contains("orbit needs a star or holes")
    );
}

#[test]
fn the_later_additions_file_is_laid_over_the_first() {
    let mut vfs = sjk_vfs::VirtualFileSystem::new();
    let later = r#"{"orbit": {"at": 10.0, "count": 2, "radius": 4.5, "rate": 0.4, "tilt": 0.3},
        "trail": [1.0, 0.0, 0.0]}"#;
    vfs.mount_memory(
        "pack",
        [
            (
                "skins/blades/saber_test.bladeskin",
                sample_with_effects().into_bytes(),
            ),
            (
                "skins/blades/saber_test.bladeextra",
                ADDITIONS.as_bytes().to_vec(),
            ),
            (
                "skins/blades/saber_test.bladeextra2",
                later.as_bytes().to_vec(),
            ),
            // The later file without the first one's star is refused, as a whole.
            (
                "skins/blades/saber_lone.bladeskin",
                sample_with_effects().into_bytes(),
            ),
            (
                "skins/blades/saber_lone.bladeextra2",
                LATER_ADDITIONS.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap();
    let skins = load(&vfs);
    let ids: Vec<&str> = skins.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["saber_test"]);
    let def = &skins[0].1;
    assert_eq!(def.orbit.unwrap().count, 2);
    assert!(def.star.is_some() && def.options.len() == 4);
    assert_eq!(def.trail, [1.0, 0.0, 0.0], "the later file's field wins");
}

#[test]
fn the_loader_lays_an_additions_file_over_its_skin_and_older_names_are_left() {
    let mut vfs = sjk_vfs::VirtualFileSystem::new();
    vfs.mount_memory(
        "pack",
        [
            (
                "skins/blades/saber_test.bladeskin",
                sample_with_effects().into_bytes(),
            ),
            (
                "skins/blades/saber_test.bladeextra",
                ADDITIONS.as_bytes().to_vec(),
            ),
            (
                "skins/blades/saber_plain.bladeskin",
                SAMPLE.as_bytes().to_vec(),
            ),
            // An additions file alone is no skin.
            (
                "skins/blades/saber_lone.bladeextra",
                ADDITIONS.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap();
    let skins = load(&vfs);
    let ids: Vec<&str> = skins.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["saber_plain", "saber_test"]);
    assert!(skins[1].1.star.is_some() && skins[1].1.options.len() == 4);
    assert!(skins[0].1.star.is_none());
}

/// A third additions file with every section of the fifth set.
pub(crate) const LAB_ADDITIONS: &str = r#"{
    "bands": {"color": [1.0, 0.1, 0.15], "brightness": 1.2, "spacing": 4.0, "width": 0.45,
        "twist": 0.6, "speed": 2.0, "mix": 1.0, "pinch": 0.3},
    "segments": {"count": 6, "rate": 9.0, "threshold": 0.93, "depth": 0.88, "gap": 0.6,
        "hum": 0.07},
    "film": {"amount": 0.85, "scale": 0.2, "speed": 1.0, "saturation": 0.6, "clear": 0.7},
    "blobs": {"color": [1.0, 0.5, 0.15], "brightness": 2.0, "count": 6, "size": 0.7,
        "speed": 4.0, "wobble": 0.4, "pools": 1.2},
    "front": {"color": [1.0, 0.1, 0.1], "amount": 0.9, "period": 8.0, "duty": 0.6,
        "hold": 0.3, "reach": 0.7, "soft": 3.0, "jag": 0.5, "veins": 1.0},
    "options": [{"id": "rings", "name": "Rings", "sections": ["bands"]},
        {"id": "overheat", "name": "Overheat", "sections": ["front", "segments"]}]
}"#;

#[test]
fn the_third_additions_file_brings_the_fifth_set_and_its_lanes() {
    use extras::{Section, option_bit};
    let mut vfs = sjk_vfs::VirtualFileSystem::new();
    vfs.mount_memory(
        "pack",
        [
            (
                "skins/blades/saber_test.bladeskin",
                sample_with_effects().into_bytes(),
            ),
            (
                "skins/blades/saber_test.bladeextra3",
                LAB_ADDITIONS.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap();
    let skins = load(&vfs);
    assert_eq!(skins.len(), 1);
    let def = &skins[0].1;
    assert_eq!(def.segments.unwrap().count, 6);
    assert_eq!(def.blobs.unwrap().pools, 1.2);
    assert_eq!(def.front.unwrap().reach, 0.7);
    assert!(def.bands.is_some() && def.film.is_some());
    assert_eq!(
        def.off_mask(option_bit("overheat")),
        Section::Front.bit() | Section::Segments.bit()
    );
    let one = crate::saber_skins::SkinUniform::of(def);
    let lanes: &[[f32; 4]] = bytemuck::cast_slice(std::slice::from_ref(&one));
    // The thirteen lanes before the sixth and seventh sets' twenty are the fifth set's.
    let lab = &lanes[lanes.len() - 33..lanes.len() - 20];
    assert_eq!(lab[0], [1.0, 0.1, 0.15, 1.2]);
    assert_eq!(lab[1], [4.0, 0.45, 0.6, 2.0]);
    assert_eq!(lab[2], [1.0, 0.3, 0.0, 0.0]);
    assert_eq!(lab[3], [6.0, 9.0, 0.93, 0.88]);
    assert_eq!(lab[4], [0.6, 0.07, 0.0, 0.0]);
    assert_eq!(lab[5], [0.85, 0.2, 1.0, 0.6]);
    assert_eq!(lab[6], [0.7, 0.0, 0.0, 0.0]);
    assert_eq!(lab[7], [1.0, 0.5, 0.15, 2.0]);
    assert_eq!(lab[8], [6.0, 0.7, 4.0, 0.4]);
    assert_eq!(lab[9], [1.2, 0.0, 0.0, 0.0]);
    assert_eq!(lab[10], [1.0, 0.1, 0.1, 0.9]);
    assert_eq!(lab[11], [8.0, 0.6, 0.3, 0.7]);
    assert_eq!(lab[12], [3.0, 0.5, 1.0, 0.0]);
    // Without the file, none of it.
    let plain =
        crate::saber_skins::SkinUniform::of(&parse("saber_test", &sample_with_effects()).unwrap());
    let lanes: &[[f32; 4]] = bytemuck::cast_slice(std::slice::from_ref(&plain));
    assert!(
        lanes[lanes.len() - 33..]
            .iter()
            .all(|lane| *lane == [0.0; 4])
    );
}

#[test]
fn the_fifth_set_is_checked_strictly_by_name() {
    let refused = |old: &str, new: &str, why: &str| {
        assert!(LAB_ADDITIONS.contains(old), "{old}");
        let text =
            extras::merged(&sample_with_effects(), &LAB_ADDITIONS.replacen(old, new, 1)).unwrap();
        let error = parse("saber_test", &text).unwrap_err();
        assert!(error.contains(why), "{new}: {error}");
    };
    refused(r#""spacing": 4.0"#, r#""spacing": 0.0"#, "bands.spacing");
    refused(r#""pinch": 0.3"#, r#""pinch": 0.95"#, "bands.pinch");
    refused(
        r#""count": 6, "rate""#,
        r#""count": 1, "rate""#,
        "segments.count must be 2 to 12",
    );
    refused(r#""clear": 0.7"#, r#""clear": 1.5"#, "film.clear");
    refused(
        r#""count": 6, "size""#,
        r#""count": 9, "size""#,
        "blobs.count must be 1 to 8",
    );
    refused(r#""period": 8.0"#, r#""period": 0.1"#, "front.period");
    refused(r#""veins": 1.0"#, r#""veins": 1.0, "glow": 1"#, "glow");
}

/// A fourth additions file with every section of the sixth set.
pub(crate) const FIGURES_ADDITIONS: &str = r#"{
    "sketch": {"color": [0.93, 0.95, 1.0], "brightness": 2.0, "rate": 8.0, "wobble": 0.3,
        "hatch": 1.2, "glow": 0.3},
    "hourglass": {"color": [1.0, 0.78, 0.35], "brightness": 1.5, "period": 8.0,
        "waist": 0.25, "glass": 0.8},
    "serpent": {"color": [1.0, 0.78, 0.25], "brightness": 1.6, "coils": 5.0,
        "amplitude": 0.8, "speed": 0.35, "thickness": 0.3},
    "strands": {"count": 28, "spread": 1.2, "sway": 0.4, "brightness": 1.5, "hue": 0.45,
        "saturation": 0.65},
    "butterflies": {"color": [1.0, 0.55, 0.12], "brightness": 1.8, "count": 7, "size": 2.5,
        "flap": 9.0, "flight": 0.25, "roam": 10.0},
    "wings": {"color": [1.0, 0.45, 0.08], "brightness": 1.6, "span": 12.0, "rate": 0.25,
        "feathers": 6},
    "copies": {"count": 2, "angle": 0.04, "fade": 0.4, "period": 5.0, "flicker": 0.3},
    "options": [{"id": "flight", "name": "Flight", "sections": ["butterflies", "wings"]}]
}"#;

#[test]
fn the_fourth_additions_file_brings_the_sixth_set_its_lanes_and_room() {
    use extras::{Section, option_bit};
    let mut vfs = sjk_vfs::VirtualFileSystem::new();
    vfs.mount_memory(
        "pack",
        [
            (
                "skins/blades/saber_test.bladeskin",
                sample_with_effects().into_bytes(),
            ),
            (
                "skins/blades/saber_test.bladeextra4",
                FIGURES_ADDITIONS.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap();
    let skins = load(&vfs);
    assert_eq!(skins.len(), 1);
    let def = &skins[0].1;
    assert_eq!(def.strands.unwrap().count, 28);
    assert_eq!(def.wings.unwrap().feathers, 6);
    assert!(def.sketch.is_some() && def.hourglass.is_some() && def.serpent.is_some());
    assert!(def.copies.is_some());
    // The quad's room: the butterflies' flights (roam and size) or the wings' span.
    assert_eq!(def.room(0), 12.5);
    let flight = def.off_mask(option_bit("flight"));
    assert_eq!(flight, Section::Butterflies.bit() | Section::Wings.bit());
    assert_eq!(def.room(flight), 0.0);
    let one = crate::saber_skins::SkinUniform::of(def);
    let lanes: &[[f32; 4]] = bytemuck::cast_slice(std::slice::from_ref(&one));
    let figures = &lanes[lanes.len() - 20..lanes.len() - 5];
    assert_eq!(figures[0], [0.93, 0.95, 1.0, 2.0]);
    assert_eq!(figures[1], [8.0, 0.3, 1.2, 0.3]);
    assert_eq!(figures[3], [8.0, 0.25, 0.8, 0.0]);
    assert_eq!(figures[5], [5.0, 0.8, 0.35, 0.3]);
    assert_eq!(figures[6], [28.0, 1.2, 0.4, 1.5]);
    assert_eq!(figures[7], [0.45, 0.65, 0.0, 0.0]);
    assert_eq!(figures[9], [7.0, 2.5, 9.0, 0.25]);
    assert_eq!(figures[10], [10.0, 0.0, 0.0, 0.0]);
    assert_eq!(figures[12], [12.0, 0.25, 6.0, 0.0]);
    assert_eq!(figures[13], [2.0, 0.04, 0.4, 5.0]);
    assert_eq!(figures[14], [0.3, 0.0, 0.0, 0.0]);
}

#[test]
fn the_sixth_set_is_checked_strictly_by_name() {
    let refused = |old: &str, new: &str, why: &str| {
        assert!(FIGURES_ADDITIONS.contains(old), "{old}");
        let text = extras::merged(
            &sample_with_effects(),
            &FIGURES_ADDITIONS.replacen(old, new, 1),
        )
        .unwrap();
        let error = parse("saber_test", &text).unwrap_err();
        assert!(error.contains(why), "{new}: {error}");
    };
    refused(r#""rate": 8.0"#, r#""rate": 0.5"#, "sketch.rate");
    refused(r#""waist": 0.25"#, r#""waist": 0.0"#, "hourglass.waist");
    refused(r#""coils": 5.0"#, r#""coils": 20.0"#, "serpent.coils");
    refused(
        r#""count": 28"#,
        r#""count": 40"#,
        "strands.count must be 2 to 32",
    );
    refused(r#""roam": 10.0"#, r#""roam": 30.0"#, "butterflies.roam");
    refused(
        r#""feathers": 6"#,
        r#""feathers": 2"#,
        "wings.feathers must be 3 to 9",
    );
    refused(r#""angle": 0.04"#, r#""angle": 0.3"#, "copies.angle");
}

/// A fifth additions file with both sections of the seventh set.
pub(crate) const SOLIDS_ADDITIONS: &str = r#"{
    "ink": {"color": [0.02, 0.02, 0.03], "body": 0.95, "outline": 1.0, "width": 0.9,
        "edge": 0.2, "point": 6.0, "gloss": 0.35, "halftone": 1.5},
    "chrome": {"color": [0.9, 0.93, 1.0], "width": 0.9, "ripple": 0.12, "beads": 3.0,
        "speed": 6.0},
    "options": [{"id": "mirror", "name": "Mirror", "sections": ["chrome"]},
        {"id": "ink", "name": "Ink", "sections": ["ink"]}]
}"#;

#[test]
fn the_fifth_additions_file_brings_the_seventh_set_and_its_lanes() {
    use extras::{Section, option_bit};
    let mut vfs = sjk_vfs::VirtualFileSystem::new();
    vfs.mount_memory(
        "pack",
        [
            (
                "skins/blades/saber_test.bladeskin",
                sample_with_effects().into_bytes(),
            ),
            (
                "skins/blades/saber_test.bladeextra5",
                SOLIDS_ADDITIONS.as_bytes().to_vec(),
            ),
        ],
    )
    .unwrap();
    let skins = load(&vfs);
    assert_eq!(skins.len(), 1);
    let def = &skins[0].1;
    assert!(def.solids().any());
    assert_eq!(def.ink.unwrap().point, 6.0);
    assert_eq!(def.chrome.unwrap().beads, 3.0);
    // The last two sections take the mask's last two bits.
    assert_eq!(def.off_mask(option_bit("mirror")), Section::Chrome.bit());
    assert_eq!(Section::Chrome.bit(), 1 << 31);
    let one = crate::saber_skins::SkinUniform::of(def);
    let lanes: &[[f32; 4]] = bytemuck::cast_slice(std::slice::from_ref(&one));
    let solids = &lanes[lanes.len() - 5..];
    assert_eq!(solids[0], [0.02, 0.02, 0.03, 0.95]);
    assert_eq!(solids[1], [1.0, 0.9, 0.2, 6.0]);
    assert_eq!(solids[2], [0.35, 1.5, 0.0, 0.0]);
    assert_eq!(solids[3], [0.9, 0.93, 1.0, 0.9]);
    assert_eq!(solids[4], [0.12, 3.0, 6.0, 0.0]);
    // A halftone alone prints the glow but draws nothing in the solid pass.
    let print = r#"{"ink": {"color": [0.0, 0.0, 0.0], "body": 0.0, "outline": 0.0,
        "width": 0.7, "edge": 0.2, "point": 0.0, "gloss": 0.0, "halftone": 2.0}}"#;
    let text = extras::merged(&sample_with_effects(), print).unwrap();
    assert!(!parse("saber_test", &text).unwrap().solids().any());
}

#[test]
fn the_seventh_set_is_checked_strictly_by_name() {
    let refused = |old: &str, new: &str, why: &str| {
        assert!(SOLIDS_ADDITIONS.contains(old), "{old}");
        let text = extras::merged(
            &sample_with_effects(),
            &SOLIDS_ADDITIONS.replacen(old, new, 1),
        )
        .unwrap();
        let error = parse("saber_test", &text).unwrap_err();
        assert!(error.contains(why), "{new}: {error}");
    };
    refused(r#""body": 0.95"#, r#""body": 1.5"#, "ink.body");
    refused(r#""edge": 0.2"#, r#""edge": 0.0"#, "ink.edge");
    refused(
        r#""color": [0.02, 0.02, 0.03]"#,
        r#""color": [2.0, 0.0, 0.0]"#,
        "ink.color",
    );
    refused(r#""ripple": 0.12"#, r#""ripple": 0.9"#, "chrome.ripple");
    let nothing = SOLIDS_ADDITIONS.replacen(r#""halftone": 1.5"#, r#""halftone": 0.0"#, 1);
    let text = extras::merged(
        &sample_with_effects(),
        &nothing.replacen(
            r#""body": 0.95, "outline": 1.0"#,
            r#""body": 0.0, "outline": 0.0"#,
            1,
        ),
    )
    .unwrap();
    assert!(
        parse("saber_test", &text)
            .unwrap_err()
            .contains("ink needs a body, an outline or a halftone")
    );
}

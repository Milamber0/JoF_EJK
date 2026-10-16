//! Material classes: the one table that decides how strong a texture's normals
//! are, whether it gets parallax height, and its roughness, metalness and
//! occlusion. Tune generation here.
//!
//! A texture's class comes from, in order:
//!
//! 1. the map compiler's material id in the BSP shader lump (`surfaceFlags &
//!    MATERIAL_MASK`, written from the shader's `material`/`q3map_material`
//!    keyword; ids from OpenJK `codemp/game/surfaceflags.h`);
//! 2. a material keyword ([`MATERIAL_KEYWORDS`]: concrete, cement), then the first
//!    class, in table order, with a keyword contained in the texture's path below
//!    `textures/` (directories and file name, lower case), except inside a word that
//!    only looks like it ([`NOT_CLASS_KEYWORDS`]: `background` is not ground);
//! 3. `surfaceparm metalsteps` (`SURF_METALSTEPS`) for metal;
//! 4. the texture set's class ([`SET_CLASSES`]: the directory below `textures/`);
//! 5. [`GENERIC`].
//!
//! A shader with a `tcGen environment` stage asks for a polished surface the stock way:
//! its texture's class is made glossier ([`polished`]). A per-texture overrides file
//! ([`crate::overrides`]) has the last word.

/// One row of [`CLASSES`]. All values are in the units the generator writes:
/// normal strength multiplies the height slope, the rest are 0–1 map values.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialClass {
    /// Short name used in the manifest and the dry-run listing.
    pub name: &'static str,
    /// BSP material ids (`MATERIAL_*`) that select this class.
    pub bsp_materials: &'static [u32],
    /// Lower-case substrings of the texture path that select this class.
    pub keywords: &'static [&'static str],
    /// Slope of the normal map per unit of normalised height at 256 texels.
    pub normal_strength: f32,
    /// Write height for parallax (`_nh`); otherwise a plain normal map (`_n`).
    pub parallax: bool,
    /// Path keywords that give this class height anyway (none at present).
    pub height_keywords: &'static [&'static str],
    /// Base roughness (rend2 packed roughness: 0 mirror, 1 matte).
    pub roughness: f32,
    /// How far local variation, brightness and cavities move the roughness.
    pub roughness_variation: f32,
    /// Metalness of bright, unsaturated texels; painted and dark texels get less.
    pub metalness: f32,
    /// Ambient darkening of cavities (occlusion channel), 0 for none.
    pub occlusion: f32,
    /// Alpha-tested stages of this class may get maps (grates, not foliage).
    pub alpha_test_safe: bool,
    /// Weight of the two finest height bands (1 and 2 texels at 256 texels): 1 keeps
    /// them. Smooth classes take less, so the grain of a scan or an upscaler does not
    /// ripple their normals and swim in their reflections.
    pub fine_detail: f32,
    /// Which way up the luminance height is.
    pub relief: Relief,
}

/// Which way up a texture's luminance height is. Luminance is a guess at height: a dark
/// stud on a lighter plate or an inset panel painted lighter than its frame comes out
/// upside down, and reads pushed in instead of raised.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Relief {
    /// Bright is high, unless the paint's own top light clearly says otherwise
    /// ([`crate::generate::painted_relief`]).
    Auto,
    /// Bright is high.
    Keep,
    /// Dark is high.
    Inverted,
}

/// BSP material ids of OpenJK `surfaceflags.h`.
pub mod bsp {
    pub const NONE: u32 = 0;
    pub const SOLID_WOOD: u32 = 1;
    pub const HOLLOW_WOOD: u32 = 2;
    pub const SOLID_METAL: u32 = 3;
    pub const HOLLOW_METAL: u32 = 4;
    pub const SHORT_GRASS: u32 = 5;
    pub const LONG_GRASS: u32 = 6;
    pub const DIRT: u32 = 7;
    pub const SAND: u32 = 8;
    pub const GRAVEL: u32 = 9;
    pub const GLASS: u32 = 10;
    pub const CONCRETE: u32 = 11;
    pub const MARBLE: u32 = 12;
    pub const WATER: u32 = 13;
    pub const SNOW: u32 = 14;
    pub const ICE: u32 = 15;
    pub const FLESH: u32 = 16;
    pub const MUD: u32 = 17;
    pub const BP_GLASS: u32 = 18;
    pub const DRY_LEAVES: u32 = 19;
    pub const GREEN_LEAVES: u32 = 20;
    pub const FABRIC: u32 = 21;
    pub const CANVAS: u32 = 22;
    pub const ROCK: u32 = 23;
    pub const RUBBER: u32 = 24;
    pub const PLASTIC: u32 = 25;
    pub const TILES: u32 = 26;
    pub const CARPET: u32 = 27;
    pub const PLASTER: u32 = 28;
    pub const SHATTER_GLASS: u32 = 29;
    pub const ARMOR: u32 = 30;
    pub const COMPUTER: u32 = 31;
    /// `MATERIAL_MASK`: the material id's bits in `surfaceFlags`.
    pub const MASK: u32 = 0x1f;
    /// `SURF_METALSTEPS`.
    pub const SURF_METALSTEPS: u32 = 0x0000_8000;
}

/// The classes, in keyword-matching order: more specific rows first.
pub const CLASSES: &[MaterialClass] = &[
    MaterialClass {
        name: "foliage",
        bsp_materials: &[bsp::DRY_LEAVES, bsp::GREEN_LEAVES, bsp::LONG_GRASS],
        keywords: &[
            "leaf", "leaves", "foliage", "plant", "bush", "vine", "ivy", "fern", "hedge", "branch",
            "frond",
        ],
        normal_strength: 1.5,
        parallax: false,
        height_keywords: &[],
        roughness: 0.8,
        roughness_variation: 0.15,
        metalness: 0.0,
        occlusion: 0.3,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "glass",
        bsp_materials: &[bsp::GLASS, bsp::BP_GLASS, bsp::SHATTER_GLASS, bsp::ICE],
        keywords: &["glass", "window"],
        normal_strength: 0.5,
        parallax: false,
        height_keywords: &[],
        roughness: 0.1,
        roughness_variation: 0.1,
        metalness: 0.0,
        occlusion: 0.0,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "electronics",
        bsp_materials: &[bsp::COMPUTER, bsp::PLASTIC, bsp::RUBBER],
        keywords: &[
            "computer",
            "console",
            "monitor",
            "screen",
            "display",
            "panel_light",
            "button",
            "keypad",
            "keyport",
            "plastic",
            "control",
            "switch",
            "onoff",
            "terminal",
            "comm_",
        ],
        normal_strength: 1.5,
        parallax: false,
        height_keywords: &[],
        roughness: 0.45,
        roughness_variation: 0.25,
        metalness: 0.1,
        occlusion: 0.3,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "lights",
        bsp_materials: &[],
        keywords: &["light", "lamp", "neon", "glow"],
        normal_strength: 1.0,
        parallax: false,
        height_keywords: &[],
        roughness: 0.4,
        roughness_variation: 0.2,
        metalness: 0.0,
        occlusion: 0.2,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "metal",
        bsp_materials: &[bsp::SOLID_METAL, bsp::HOLLOW_METAL, bsp::ARMOR],
        keywords: &[
            "metal",
            "steel",
            "grate",
            "grating",
            "grill",
            "pipe",
            "girder",
            "rust",
            "chrome",
            "mtl",
            "vent",
            "rivet",
            "hatch",
            "duct",
            "catwalk",
            "railing",
            "beam",
            "brace",
            "casing",
            "hull",
            "tank",
            "barrel",
            "blastdoor",
            "blastshield",
            "hangar",
            "elevator",
            "turbolift",
        ],
        normal_strength: 2.0,
        // No parallax height: seams and rivets read as well in the normal map, and a height
        // guessed from paint made metal panels swim (Sol, 06/10/2026).
        parallax: false,
        height_keywords: &[],
        // Brushed rather than polished (0.3 +- 0.3 over the texture), and mostly
        // metallic. With reflection probes the client now reflects the room into metal,
        // which loses its diffuse share in rend2's packed path; 0.8 rather than 1 keeps a
        // fifth of the diffuse light for painted or grimy parts the heuristics call
        // metal, and the generator still lowers metalness on dark and saturated texels.
        roughness: 0.3,
        roughness_variation: 0.3,
        metalness: 0.8,
        occlusion: 0.4,
        alpha_test_safe: true,
        fine_detail: 0.25,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "tiles",
        bsp_materials: &[bsp::TILES, bsp::MARBLE],
        keywords: &["tile", "marble", "mosaic", "polished"],
        normal_strength: 2.5,
        parallax: true,
        height_keywords: &[],
        roughness: 0.35,
        roughness_variation: 0.35,
        metalness: 0.0,
        occlusion: 0.5,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "stone",
        bsp_materials: &[bsp::ROCK, bsp::CONCRETE, bsp::GRAVEL],
        keywords: &[
            "rock", "stone", "brick", "cobble", "cliff", "boulder", "slate", "granite", "cave",
            "canyon", "concrete", "cement", "gravel", "pillar", "column", "masonry", "ruin",
        ],
        normal_strength: 3.0,
        parallax: true,
        height_keywords: &[],
        roughness: 0.85,
        roughness_variation: 0.2,
        metalness: 0.0,
        occlusion: 0.6,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "wood",
        bsp_materials: &[bsp::SOLID_WOOD, bsp::HOLLOW_WOOD],
        // Not "crate": Star Wars crates are metal containers (`cairn/c_crate`,
        // `impdetention/crate7`, `factory/newcrate*`), painted panels (Sol, 12/10/2026).
        keywords: &["wood", "plank", "bark", "timber", "logs"],
        normal_strength: 1.8,
        parallax: false,
        height_keywords: &[],
        roughness: 0.7,
        roughness_variation: 0.25,
        metalness: 0.0,
        occlusion: 0.4,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "ground",
        bsp_materials: &[bsp::DIRT, bsp::SAND, bsp::MUD, bsp::SNOW, bsp::SHORT_GRASS],
        keywords: &[
            "dirt", "sand", "mud", "snow", "ground", "terrain", "grass", "soil",
        ],
        normal_strength: 2.0,
        parallax: true,
        height_keywords: &[],
        roughness: 0.9,
        roughness_variation: 0.1,
        metalness: 0.0,
        occlusion: 0.4,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "fabric",
        bsp_materials: &[bsp::FABRIC, bsp::CANVAS, bsp::CARPET, bsp::FLESH],
        keywords: &[
            "cloth", "fabric", "carpet", "banner", "rug", "canvas", "tapestry", "curtain", "flag",
        ],
        normal_strength: 1.0,
        parallax: false,
        height_keywords: &[],
        roughness: 0.95,
        roughness_variation: 0.05,
        metalness: 0.0,
        occlusion: 0.3,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    MaterialClass {
        name: "plaster",
        bsp_materials: &[bsp::PLASTER],
        keywords: &["plaster", "stucco", "drywall", "adobe", "clay"],
        normal_strength: 1.2,
        parallax: false,
        height_keywords: &[],
        roughness: 0.9,
        roughness_variation: 0.1,
        metalness: 0.0,
        occlusion: 0.3,
        alpha_test_safe: false,
        fine_detail: 1.0,
        relief: Relief::Auto,
    },
    // Painted metal of the Star Wars interiors: walls, doors, trims and panels that are
    // neither bare metal nor stone. Last, so every other class's words come first, and
    // the default of the sci-fi texture sets ([`SET_CLASSES`]).
    MaterialClass {
        name: "panel",
        bsp_materials: &[],
        keywords: &[
            "panel", "plating", "bulkhead", "trim", "door", "plate", "lift", "gate", "walkway",
            "crate",
        ],
        normal_strength: 1.5,
        parallax: false,
        height_keywords: &[],
        roughness: 0.5,
        roughness_variation: 0.25,
        metalness: 0.35,
        occlusion: 0.4,
        alpha_test_safe: false,
        fine_detail: 0.4,
        relief: Relief::Auto,
    },
];

/// The class of textures in a texture set (the directory below `textures/`) when neither
/// the BSP material, a keyword nor `metalsteps` says anything: the Imperial, Rebel and
/// industrial sets are painted metal, the ancient ones stone.
pub const SET_CLASSES: &[(&str, &str)] = &[
    ("bespin", "panel"),
    ("bounty", "panel"),
    ("byss", "panel"),
    ("cairn", "panel"),
    ("doomgiver", "panel"),
    ("factory", "panel"),
    ("h_evil", "panel"),
    ("hoth", "panel"),
    ("imp_mine", "panel"),
    ("impdetention", "panel"),
    ("impgarrison", "panel"),
    ("imperial", "panel"),
    ("kejim", "panel"),
    ("rail", "panel"),
    ("taspir", "panel"),
    ("vjun", "panel"),
    ("wedge", "panel"),
    ("korriban", "stone"),
    ("rift", "stone"),
    ("rocky_ruins", "stone"),
    ("yavin", "stone"),
];

/// Words that contain a class keyword without naming that material: (word, keyword it
/// hides). A path keyword inside one of these words does not count, so
/// `taspir/background` (a rock face) is not `ground` and `nightmare/bricks_rustic` not
/// rusted metal. Found by listing every keyword match inside a longer word over Sol's
/// installed maps (13/10/2026).
pub const NOT_CLASS_KEYWORDS: &[(&str, &str)] = &[
    // taspir/background, faru/background: rock walls (Sol, 12/10/2026: "make it 3d").
    ("background", "ground"),
    // models/items/rockets, bounty/rockets: weapons, not rock.
    ("rocket", "rock"),
    // nightmare/bricks_rustic: rustic bricks, not rust.
    ("rustic", "rust"),
];

/// Keywords that name what a surface is made of, consulted before the table's order,
/// with the class they give: a part word earlier in the table (a `vent` in a concrete
/// wall) must not make cast concrete bare metal. `desert/concrete_wall_tall_vent` was
/// polished metal between plain concrete walls (Sol, 12/10/2026: "this texture looks
/// out of place compared to the ones around").
pub const MATERIAL_KEYWORDS: &[(&str, &str)] = &[("concrete", "stone"), ("cement", "stone")];

/// Whether `keyword` occurs in `searched` other than inside a [`NOT_CLASS_KEYWORDS`] word.
fn names(searched: &str, keyword: &str) -> bool {
    let mut rest = searched;
    while let Some(at) = rest.find(keyword) {
        let hidden = NOT_CLASS_KEYWORDS.iter().any(|(word, hides)| {
            *hides == keyword
                && word
                    .find(keyword)
                    .is_some_and(|offset| at >= offset && rest[at - offset..].starts_with(word))
        });
        if !hidden {
            return true;
        }
        rest = &rest[at + keyword.len()..];
    }
    false
}

/// The class of textures nothing else describes: moderate bumps, matte.
pub const GENERIC: MaterialClass = MaterialClass {
    name: "generic",
    bsp_materials: &[bsp::NONE, bsp::WATER],
    keywords: &[],
    normal_strength: 1.5,
    parallax: false,
    height_keywords: &[],
    roughness: 0.75,
    roughness_variation: 0.25,
    metalness: 0.0,
    occlusion: 0.35,
    alpha_test_safe: false,
    fine_detail: 1.0,
    relief: Relief::Auto,
};

/// Base roughness of a polished surface (a `tcGen environment` stage): clear
/// reflections without a mirror finish.
pub const POLISHED_ROUGHNESS: f32 = 0.25;

/// `class` for a texture its shader marks as polished: roughness at most
/// [`POLISHED_ROUGHNESS`], half the variation, so its reflections stay clear.
pub fn polished(class: &MaterialClass) -> MaterialClass {
    MaterialClass {
        roughness: class.roughness.min(POLISHED_ROUGHNESS),
        roughness_variation: class.roughness_variation * 0.5,
        ..class.clone()
    }
}

/// Whether textures of `class` at `path` get height: the class always does, or the
/// path names one of its height keywords.
pub fn wants_height(class: &MaterialClass, path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let searched = lower.strip_prefix("textures/").unwrap_or(&lower);
    class.parallax
        || class
            .height_keywords
            .iter()
            .any(|word| searched.contains(word))
}

/// Which rule chose a class, for the manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassSource {
    /// The BSP material id.
    BspMaterial(u32),
    /// A path keyword.
    Keyword(&'static str),
    /// `surfaceparm metalsteps`.
    MetalSteps,
    /// The texture set's class.
    Set(&'static str),
    /// Nothing matched.
    Default,
}

impl ClassSource {
    /// Human-readable description used in listings and the manifest.
    pub fn describe(self) -> String {
        match self {
            Self::BspMaterial(id) => format!("bsp material {id}"),
            Self::Keyword(word) => format!("keyword \"{word}\""),
            Self::MetalSteps => "surfaceparm metalsteps".to_owned(),
            Self::Set(set) => format!("texture set \"{set}\""),
            Self::Default => "default".to_owned(),
        }
    }
}

/// Choose the class of a texture. `image_path` is the diffuse image's VFS path;
/// `surface_flags` the BSP shader lump's flags of the shader that uses it.
pub fn classify(image_path: &str, surface_flags: u32) -> (&'static MaterialClass, ClassSource) {
    let material = surface_flags & bsp::MASK;
    if material != bsp::NONE
        && let Some(class) = CLASSES
            .iter()
            .find(|class| class.bsp_materials.contains(&material))
    {
        return (class, ClassSource::BspMaterial(material));
    }
    let lower = image_path.to_ascii_lowercase();
    let searched = lower.strip_prefix("textures/").unwrap_or(&lower);
    if let Some((word, class)) = MATERIAL_KEYWORDS
        .iter()
        .find(|(word, _)| names(searched, word))
    {
        let class = by_name(class).expect("material keywords name table classes");
        return (class, ClassSource::Keyword(word));
    }
    for class in CLASSES {
        if let Some(word) = class.keywords.iter().find(|word| names(searched, word)) {
            return (class, ClassSource::Keyword(word));
        }
    }
    if surface_flags & bsp::SURF_METALSTEPS != 0 {
        let metal = by_name("metal").expect("the table has a metal class");
        return (metal, ClassSource::MetalSteps);
    }
    let set = searched.split('/').next().unwrap_or("");
    if let Some((set, class)) = SET_CLASSES.iter().find(|(name, _)| *name == set) {
        let class = by_name(class).expect("set classes name table classes");
        return (class, ClassSource::Set(set));
    }
    (&GENERIC, ClassSource::Default)
}

/// Look a class up by its name.
pub fn by_name(name: &str) -> Option<&'static MaterialClass> {
    CLASSES
        .iter()
        .chain(std::iter::once(&GENERIC))
        .find(|class| class.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bsp_material_wins_over_keywords() {
        let (class, source) = classify("textures/a/metal_wall", bsp::ROCK);
        assert_eq!(class.name, "stone");
        assert_eq!(source, ClassSource::BspMaterial(bsp::ROCK));
    }

    #[test]
    fn keywords_follow_table_order() {
        assert_eq!(classify("textures/x/rockwall", 0).0.name, "stone");
        assert_eq!(classify("textures/x/metal_floor", 0).0.name, "metal");
        // "glass" comes before "metal" in the table.
        assert_eq!(classify("textures/x/metal_glass", 0).0.name, "glass");
        // Only the part below textures/ is searched.
        assert_eq!(classify("textures/x/plain", 0).0.name, "generic");
    }

    #[test]
    fn texture_sets_classify_what_no_word_names() {
        let (class, source) = classify("textures/imperial/basic_wall2", 0);
        assert_eq!(
            (class.name, source),
            ("panel", ClassSource::Set("imperial"))
        );
        assert_eq!(classify("textures/korriban/wall02", 0).0.name, "stone");
        // Words and BSP materials still come first.
        assert_eq!(classify("textures/imperial/metal_grate", 0).0.name, "metal");
        assert_eq!(
            classify("textures/imperial/control_onoff", 0).0.name,
            "electronics"
        );
        assert_eq!(
            classify("textures/imperial/floor", bsp::TILES).0.name,
            "tiles"
        );
        assert_eq!(classify("textures/mp/door_trim", 0).0.name, "panel");
        assert_eq!(classify("textures/yavin/trim_stone01", 0).0.name, "stone");
        for (_, class) in SET_CLASSES {
            assert!(by_name(class).is_some(), "{class}");
        }
    }

    #[test]
    fn keywords_inside_other_words_do_not_count() {
        // Sol's notes, 12/10/2026: a rock face and metal crates.
        assert_eq!(
            classify("textures/taspir/background", 0),
            (by_name("panel").unwrap(), ClassSource::Set("taspir"))
        );
        assert_eq!(classify("textures/x/rockets", 0).0.name, "generic");
        assert_eq!(classify("textures/x/bricks_rustic", 0).0.name, "stone");
        // The word itself still counts, also next to a hiding word.
        assert_eq!(classify("textures/x/ground", 0).0.name, "ground");
        assert_eq!(classify("textures/x/background_ground", 0).0.name, "ground");
        assert_eq!(classify("textures/x/rock_rocket", 0).0.name, "stone");
        assert_eq!(classify("textures/x/rust_plate", 0).0.name, "metal");
        assert_eq!(classify("textures/cairn/c_crate2", 0).0.name, "panel");
        assert_eq!(classify("textures/x/woodcrate", 0).0.name, "wood");
        for (word, keyword) in NOT_CLASS_KEYWORDS {
            assert!(word.contains(keyword), "{word}");
        }
    }

    #[test]
    fn materials_come_before_parts() {
        let (class, source) = classify("textures/desert/concrete_wall_tall_vent", 0);
        assert_eq!(
            (class.name, source),
            ("stone", ClassSource::Keyword("concrete"))
        );
        assert_eq!(
            classify("textures/desert/concrete_floor_light", 0).0.name,
            "stone"
        );
        // The BSP material still comes first.
        assert_eq!(
            classify("textures/x/concrete_vent", bsp::SOLID_METAL)
                .0
                .name,
            "metal"
        );
        for (_, class) in MATERIAL_KEYWORDS {
            assert!(by_name(class).is_some(), "{class}");
        }
    }

    #[test]
    fn metal_steps_and_default() {
        let (class, source) = classify("textures/x/plain", bsp::SURF_METALSTEPS);
        assert_eq!((class.name, source), ("metal", ClassSource::MetalSteps));
        let (class, source) = classify("textures/x/plain", 0);
        assert_eq!((class.name, source), ("generic", ClassSource::Default));
    }

    #[test]
    fn every_bsp_material_has_one_class() {
        for id in 0..32 {
            let classes = CLASSES
                .iter()
                .chain(std::iter::once(&GENERIC))
                .filter(|class| class.bsp_materials.contains(&id))
                .count();
            assert_eq!(classes, 1, "material id {id}");
        }
    }

    #[test]
    fn metal_is_tuned_for_reflection_probes() {
        let metal = by_name("metal").expect("metal class");
        assert_eq!((metal.metalness, metal.roughness), (0.8, 0.3));
        // The roughest texel stays below the dielectric classes' base roughness.
        assert!(metal.roughness + metal.roughness_variation <= 0.7);
        // Metal never gets parallax height: a height guessed from paint made it swim.
        assert!(!wants_height(metal, "textures/imperial/metal_panel2"));
        assert!(!wants_height(metal, "textures/x/floor_plate"));
        assert!(metal.fine_detail < 1.0);
        assert!(wants_height(
            by_name("stone").unwrap(),
            "textures/x/anything"
        ));
    }

    #[test]
    fn polished_shaders_get_glossier() {
        let stone = by_name("stone").expect("stone class");
        let shiny = polished(stone);
        assert_eq!(shiny.roughness, POLISHED_ROUGHNESS);
        assert_eq!(shiny.roughness_variation, stone.roughness_variation * 0.5);
        assert_eq!((shiny.name, shiny.metalness), (stone.name, stone.metalness));
        // Already glossier classes keep their roughness.
        let glass = by_name("glass").expect("glass class");
        assert_eq!(polished(glass).roughness, glass.roughness);
    }

    #[test]
    fn table_values_are_in_range() {
        for class in CLASSES.iter().chain(std::iter::once(&GENERIC)) {
            assert!(class.normal_strength > 0.0, "{}", class.name);
            assert!((0.0..=1.0).contains(&class.fine_detail), "{}", class.name);
            for value in [
                class.roughness,
                class.roughness_variation,
                class.metalness,
                class.occlusion,
            ] {
                assert!((0.0..=1.0).contains(&value), "{}", class.name);
            }
        }
        assert!(by_name("generic").is_some());
    }
}

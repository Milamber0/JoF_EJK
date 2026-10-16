//! Per-texture overrides of the class table: a small text file the player edits when
//! the heuristics pick the wrong material for a texture.
//!
//! One rule per line: a path pattern, then `key=value` settings separated by spaces.
//! `#` starts a comment; blank lines are ignored.
//!
//! ```text
//! # pattern                      settings
//! textures/mp/floor*             class=tiles roughness=0.2
//! textures/kor_*/*metal*         metalness=0.9 height=on
//! */plain_wall                   class=plaster height=off
//! textures/kejim/lightpanel*      emission=on
//! textures/x/lightgreen_wall      emission=off
//! textures/desert/s_floor1        relief=inverted
//! textures/taspir/trim_walk       strength=2.5
//! ```
//!
//! The pattern is matched against the diffuse image's path without extension
//! (`textures/mp/floor1`), ignoring case; `*` matches any run of characters
//! (slashes included) and `?` one character. Keys:
//!
//! - `class`: a class name from [`crate::classes`] (`metal`, `stone`, `tiles`, ...); the
//!   texture takes that class's whole row.
//! - `roughness`, `metalness`: the class's base roughness and metalness, 0–1.
//! - `strength`: multiplies the class's normal strength (the slope of the generated
//!   relief), from [`MIN_STRENGTH`] to [`MAX_STRENGTH`]: more than 1 for a texture whose
//!   shapes read flat, less for one whose relief is overdone. Like `--strength`, but for
//!   the matching textures only; both multiply.
//! - `height`: `on` writes `_nh` (height for parallax), `off` a plain `_n`.
//! - `emission`: `on` writes an emission map (`_e`) whatever the evidence, `off`
//!   never writes one, and a number from 0 to 4 is `on` with that strength (the
//!   emitted colour's multiplier; 0 is `off`). See [`crate::emission`].
//! - `relief`: `inverted` makes the dark parts high (a texture whose relief reads
//!   pushed in), `normal` keeps bright high, `auto` lets the painted shading decide
//!   ([`crate::classes::Relief`]).
//!
//! Every matching rule applies, in file order, so a broad rule can come first and a
//! narrower one refine it; for `emission` the last matching rule that sets it wins.
//! The manifest lists the line numbers that applied.

use crate::classes::{MaterialClass, Relief, by_name};

/// Smallest and largest `strength=` multiplier.
pub const MIN_STRENGTH: f32 = 0.1;
pub const MAX_STRENGTH: f32 = 4.0;

/// One parsed rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// Lower-case glob.
    pub pattern: String,
    pub class: Option<&'static MaterialClass>,
    pub roughness: Option<f32>,
    pub metalness: Option<f32>,
    /// Multiplier on the class's normal strength.
    pub strength: Option<f32>,
    pub height: Option<bool>,
    pub relief: Option<Relief>,
    /// Emission strength: 0 off, 1 on (`on`), up to [`crate::emission::MAX_STRENGTH`].
    pub emission: Option<f32>,
    /// 1-based line in the file.
    pub line: usize,
}

/// The rules of one file, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    pub rules: Vec<Rule>,
}

impl Overrides {
    /// Parse a whole file; the error names the line and what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut rules = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let content = line.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            let mut words = content.split_whitespace();
            let pattern = words.next().expect("non-empty").to_ascii_lowercase();
            let mut rule = Rule {
                pattern,
                class: None,
                roughness: None,
                metalness: None,
                strength: None,
                height: None,
                relief: None,
                emission: None,
                line: line_number,
            };
            let mut any = false;
            for word in words {
                let (key, value) = word.split_once('=').ok_or_else(|| {
                    format!("line {line_number}: expected key=value, got {word:?}")
                })?;
                let unit = |value: &str| {
                    value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| (0.0..=1.0).contains(v))
                        .ok_or_else(|| {
                            format!("line {line_number}: {key} needs a number from 0 to 1")
                        })
                };
                match key.to_ascii_lowercase().as_str() {
                    "class" => {
                        rule.class =
                            Some(by_name(&value.to_ascii_lowercase()).ok_or_else(|| {
                                format!("line {line_number}: unknown class {value:?}")
                            })?)
                    }
                    "roughness" => rule.roughness = Some(unit(value)?),
                    "metalness" => rule.metalness = Some(unit(value)?),
                    "strength" => {
                        rule.strength = Some(
                            value
                                .parse::<f32>()
                                .ok()
                                .filter(|v| (MIN_STRENGTH..=MAX_STRENGTH).contains(v))
                                .ok_or_else(|| {
                                    format!(
                                        "line {line_number}: strength needs a number from \
                                         {MIN_STRENGTH} to {MAX_STRENGTH}"
                                    )
                                })?,
                        )
                    }
                    "height" => {
                        rule.height = Some(match value.to_ascii_lowercase().as_str() {
                            "on" | "1" | "yes" | "true" => true,
                            "off" | "0" | "no" | "false" => false,
                            _ => {
                                return Err(format!("line {line_number}: height needs on or off"));
                            }
                        })
                    }
                    "relief" => {
                        rule.relief = Some(match value.to_ascii_lowercase().as_str() {
                            "inverted" | "invert" | "in" => Relief::Inverted,
                            "normal" | "keep" | "out" => Relief::Keep,
                            "auto" => Relief::Auto,
                            _ => {
                                return Err(format!(
                                    "line {line_number}: relief needs inverted, normal or auto"
                                ));
                            }
                        })
                    }
                    "emission" => {
                        rule.emission = Some(match value.to_ascii_lowercase().as_str() {
                            "on" | "yes" | "true" => 1.0,
                            "off" | "no" | "false" => 0.0,
                            number => number
                                .parse::<f32>()
                                .ok()
                                .filter(|v| (0.0..=crate::emission::MAX_STRENGTH).contains(v))
                                .ok_or_else(|| {
                                    format!(
                                        "line {line_number}: emission needs on, off or a strength from 0 to 4"
                                    )
                                })?,
                        })
                    }
                    other => return Err(format!("line {line_number}: unknown key {other:?}")),
                }
                any = true;
            }
            if !any {
                return Err(format!("line {line_number}: a pattern without settings"));
            }
            rules.push(rule);
        }
        Ok(Self { rules })
    }

    /// `class` with every rule matching `base` (the image path without extension)
    /// applied, and the lines that applied.
    pub fn apply(&self, base: &str, mut class: MaterialClass) -> (MaterialClass, Vec<usize>) {
        let path = base.to_ascii_lowercase();
        let mut lines = Vec::new();
        for rule in self.rules.iter().filter(|rule| glob(&rule.pattern, &path)) {
            if let Some(replacement) = rule.class {
                class = replacement.clone();
            }
            if let Some(roughness) = rule.roughness {
                class.roughness = roughness;
            }
            if let Some(metalness) = rule.metalness {
                class.metalness = metalness;
            }
            if let Some(strength) = rule.strength {
                class.normal_strength *= strength;
            }
            if let Some(height) = rule.height {
                class.parallax = height;
                class.height_keywords = &[];
            }
            if let Some(relief) = rule.relief {
                class.relief = relief;
            }
            lines.push(rule.line);
        }
        (class, lines)
    }

    /// The emission setting for `base` (the image path without extension): the last
    /// matching rule that sets one, `None` when no rule does.
    pub fn emission(&self, base: &str) -> Option<f32> {
        let path = base.to_ascii_lowercase();
        self.rules
            .iter()
            .filter(|rule| glob(&rule.pattern, &path))
            .filter_map(|rule| rule.emission)
            .next_back()
    }
}

/// `*` and `?` glob over bytes (paths are ASCII in practice).
pub fn glob(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_whole_paths() {
        assert!(glob("textures/mp/floor*", "textures/mp/floor1"));
        assert!(glob("*/floor?", "textures/mp/floor1"));
        assert!(glob("*metal*", "textures/kor/old_metal_wall"));
        assert!(!glob("textures/mp/floor", "textures/mp/floor1"));
        assert!(!glob("*/wall", "textures/mp/floor"));
        assert!(glob("*", ""));
        assert!(glob("a*b*c", "a__b__c"));
        assert!(!glob("a*b*c", "a__c__b"));
    }

    #[test]
    fn rules_parse_with_comments_and_report_bad_lines() {
        let overrides = Overrides::parse(
            "# metal floors\n\
             textures/MP/floor*  class=tiles roughness=0.2   # shiny\n\
             \n\
             *metal* metalness=0.9 height=on relief=inverted\n",
        )
        .expect("parses");
        assert_eq!(overrides.rules.len(), 2);
        let first = &overrides.rules[0];
        assert_eq!(first.pattern, "textures/mp/floor*");
        assert_eq!(first.class.map(|c| c.name), Some("tiles"));
        assert_eq!((first.roughness, first.line), (Some(0.2), 2));
        assert_eq!(overrides.rules[1].height, Some(true));
        assert_eq!(overrides.rules[1].relief, Some(Relief::Inverted));
        for (bad, message) in [
            ("x class=chrome", "unknown class"),
            ("x roughness=2", "from 0 to 1"),
            ("x strength=0", "strength needs a number"),
            ("x strength=5", "strength needs a number"),
            ("x height=maybe", "on or off"),
            ("x relief=up", "inverted, normal or auto"),
            ("x shiny", "key=value"),
            ("x colour=red", "unknown key"),
            ("x emission=bright", "emission needs on, off"),
            ("x emission=5", "emission needs on, off"),
            ("x", "without settings"),
        ] {
            let error = Overrides::parse(bad).expect_err(bad);
            assert!(
                error.contains(message) && error.starts_with("line 1"),
                "{error}"
            );
        }
    }

    #[test]
    fn matching_rules_apply_in_order() {
        let overrides = Overrides::parse(
            "textures/* roughness=0.6\n\
             textures/mp/floor* class=metal\n\
             textures/mp/floor1 roughness=0.1 height=off relief=normal\n",
        )
        .expect("parses");
        let generic = crate::classes::GENERIC.clone();
        let (class, lines) = overrides.apply("Textures/MP/Floor1", generic.clone());
        assert_eq!(class.name, "metal");
        assert_eq!(class.roughness, 0.1);
        assert!(!class.parallax && class.height_keywords.is_empty());
        assert_eq!(class.relief, Relief::Keep);
        assert_eq!(lines, vec![1, 2, 3]);
        let (class, lines) = overrides.apply("textures/mp/wall", generic);
        assert_eq!(
            (class.name, class.roughness, lines),
            ("generic", 0.6, vec![1])
        );
    }

    #[test]
    fn strength_rules_multiply_the_class_strength() {
        let overrides = Overrides::parse(
            "textures/taspir/* strength=2\n\
             textures/taspir/trim_walk class=panel strength=1.5\n",
        )
        .expect("parses");
        let generic = crate::classes::GENERIC.clone();
        let (class, _) = overrides.apply("textures/taspir/t_wall2", generic.clone());
        assert_eq!(class.normal_strength, generic.normal_strength * 2.0);
        // A class rule replaces the whole row (earlier strengths included); the strength
        // on its own line multiplies the new row.
        let panel = by_name("panel").unwrap();
        let (class, lines) = overrides.apply("textures/taspir/trim_walk", generic);
        assert_eq!(class.normal_strength, panel.normal_strength * 1.5);
        assert_eq!(lines, vec![1, 2]);
    }

    #[test]
    fn emission_rules_switch_and_scale_and_the_last_wins() {
        let overrides = Overrides::parse(
            "textures/kejim/light* emission=on\n\
             textures/kejim/lightpanel2 emission=off\n\
             textures/kejim/lightstrip emission=2.5 roughness=0.3\n\
             textures/* roughness=0.5\n",
        )
        .expect("parses");
        assert_eq!(overrides.emission("textures/kejim/lightpanel"), Some(1.0));
        assert_eq!(overrides.emission("Textures/Kejim/LightPanel2"), Some(0.0));
        assert_eq!(overrides.emission("textures/kejim/lightstrip"), Some(2.5));
        assert_eq!(overrides.emission("textures/kejim/wall"), None);
        // An emission-only rule changes no class value but is recorded as applied.
        let (class, lines) = overrides.apply(
            "textures/kejim/lightpanel2",
            crate::classes::GENERIC.clone(),
        );
        assert_eq!(lines, vec![1, 2, 4]);
        assert_eq!(class.roughness, 0.5);
        assert_eq!(
            Overrides::parse("x emission=0").expect("parses").rules[0].emission,
            Some(0.0)
        );
    }
}

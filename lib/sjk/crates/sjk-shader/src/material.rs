//! Optional material-map keywords in the convention of OpenJK's rend2 renderer.
//!
//! rend2 (`codemp/rd-rend2/tr_shader.cpp`, `ParseStage`) extends a stage with a
//! normal map, a specular or packed material map and their scales. The values
//! recorded here follow that parser in order, including its overrides: a
//! `normalMap` keyword resets the normal scale to the defaults, a `specMap`
//! keyword resets the specular scale, and a packed map (`rmoMap` and its
//! relatives) replaces the specular scale after the stage is parsed
//! (`R_LoadPackedMaterialImage`, `tr_image.cpp`). Renderers without material
//! maps ignore all of it; vanilla stage semantics are unchanged.

/// How a specular map's channels are laid out (`specularType_t` in rend2).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpecularLayout {
    /// No specular map.
    #[default]
    None,
    /// RGB specular colour and gloss in alpha (`specMap`, `specularMap`, `_specGloss`).
    SpecGloss,
    /// Roughness, metalness and occlusion in red, green and blue (`rmoMap`, `_rmo`).
    Rmo,
    /// Metalness, occlusion, unused and roughness (`moxrMap`).
    Moxr,
    /// Occlusion, roughness and metalness in red, green and blue (`ormMap`, `_orm`).
    Orm,
}

/// rend2's base normal scale and parallax depth (`r_baseNormalX`, `r_baseNormalY`,
/// `r_baseParallax` defaults, `tr_init.cpp`), set whenever a normal map is assigned.
pub const DEFAULT_NORMAL_SCALE: [f32; 4] = [1.0, 1.0, 1.0, 0.05];

/// rend2's initial specular scale of every stage (`r_baseSpecular` 0.04, gloss
/// term 0.99), from `ParseShader`'s stage reset in `tr_shader.cpp`.
pub const INITIAL_SPECULAR_SCALE: [f32; 4] = [0.04, 0.04, 0.04, 0.99];

/// Specular scale of a stage given a gloss map (`specMap` keyword or `_specGloss` image).
pub const SPEC_GLOSS_SCALE: [f32; 4] = [1.0, 1.0, 1.0, 0.0];

/// Material-map data of one stage. Default means no maps and rend2's initial scales.
#[derive(Clone, Debug, PartialEq)]
pub struct StageMaterial {
    /// Explicit normal map image (`normalMap` or `normalHeightMap`).
    pub normal_map: Option<String>,
    /// The normal map carries height in alpha (`normalHeightMap`, or a `_nh` image).
    pub normal_height: bool,
    /// Explicit specular or packed material image; `$whiteimage` is kept verbatim.
    pub specular_map: Option<String>,
    /// Channel layout of [`Self::specular_map`].
    pub specular_layout: SpecularLayout,
    /// rend2 `normalScale`: x/y strength, z unused (1), w parallax depth.
    pub normal_scale: [f32; 4],
    /// rend2 `specularScale`: RGB (or metalness, specular, unused) and a gloss term.
    pub specular_scale: [f32; 4],
    /// rend2 `parallaxBias`.
    pub parallax_bias: f32,
}

impl Default for StageMaterial {
    fn default() -> Self {
        Self {
            normal_map: None,
            normal_height: false,
            specular_map: None,
            specular_layout: SpecularLayout::None,
            normal_scale: [0.0; 4],
            specular_scale: INITIAL_SPECULAR_SCALE,
            parallax_bias: 0.0,
        }
    }
}

impl StageMaterial {
    /// Whether the stage names any material map explicitly.
    pub fn has_explicit_maps(&self) -> bool {
        self.normal_map.is_some() || self.specular_map.is_some()
    }

    /// Whether `directive` (lower case) is a material-map keyword of a stage.
    pub fn is_directive(directive: &str) -> bool {
        matches!(
            directive,
            "normalmap"
                | "normalheightmap"
                | "specmap"
                | "specularmap"
                | "rmomap"
                | "rmosmap"
                | "moxrmap"
                | "mosrmap"
                | "ormmap"
                | "ormsmap"
                | "specularreflectance"
                | "specularexponent"
                | "gloss"
                | "roughness"
                | "parallaxdepth"
                | "parallaxbias"
                | "normalscale"
                | "specularscale"
        )
    }

    /// Apply one keyword (lower case) with the numeric or image arguments that follow
    /// it. `next` returns the token at an offset from the keyword's first argument
    /// without consuming it; the result is the number of arguments taken.
    ///
    /// Arguments are optional where rend2's are: a missing image leaves the stage
    /// unchanged, missing numbers keep earlier values. Scales take numbers only, so
    /// an unrelated directive after them is never swallowed.
    pub fn apply(&mut self, directive: &str, next: impl Fn(usize) -> Option<String>) -> usize {
        let number = |offset: usize| next(offset).and_then(|value| value.parse::<f32>().ok());
        let image = || next(0).filter(|value| value != "}");
        match directive {
            "normalmap" | "normalheightmap" => {
                let Some(name) = image() else { return 0 };
                self.normal_map = Some(name);
                self.normal_height = directive == "normalheightmap";
                self.normal_scale = DEFAULT_NORMAL_SCALE;
                1
            }
            "specmap" | "specularmap" => {
                let Some(name) = image() else { return 0 };
                self.specular_map = Some(name);
                self.specular_layout = SpecularLayout::SpecGloss;
                self.specular_scale = SPEC_GLOSS_SCALE;
                1
            }
            // rend2 picks the alpha-carrying variants by comparing the image name, not
            // the keyword, with "rmosMap" (and so on): in practice every packed keyword
            // loads the three-channel layout. Packs were authored against that.
            "rmomap" | "rmosmap" | "moxrmap" | "mosrmap" | "ormmap" | "ormsmap" => {
                let Some(name) = image() else { return 0 };
                self.specular_layout = match directive {
                    "rmomap" | "rmosmap" => SpecularLayout::Rmo,
                    "moxrmap" | "mosrmap" => SpecularLayout::Moxr,
                    _ => SpecularLayout::Orm,
                };
                self.specular_map = Some(name);
                1
            }
            "specularreflectance" => {
                let Some(value) = number(0) else { return 0 };
                let value = value.clamp(0.0, 1.0);
                self.specular_scale[..3].fill(value);
                1
            }
            "specularexponent" => {
                let Some(value) = number(0) else { return 0 };
                let exponent = value.clamp(1.0, 8192.0);
                self.specular_scale[3] = 1.0 - exponent.ln() / 8192_f32.ln();
                1
            }
            "gloss" => {
                let Some(value) = number(0) else { return 0 };
                self.specular_scale[3] = 1.0 - value;
                1
            }
            "roughness" => {
                let Some(value) = number(0) else { return 0 };
                self.specular_scale[3] = value;
                1
            }
            "parallaxdepth" => {
                let Some(value) = number(0) else { return 0 };
                self.normal_scale[3] = value;
                1
            }
            "parallaxbias" => {
                let Some(value) = number(0) else { return 0 };
                self.parallax_bias = value;
                1
            }
            "normalscale" => {
                let Some(x) = number(0) else { return 0 };
                self.normal_scale[0] = x;
                let Some(y) = number(1) else {
                    self.normal_scale[1] = x;
                    return 1;
                };
                self.normal_scale[1] = y;
                let Some(height) = number(2) else { return 2 };
                self.normal_scale[3] = height;
                3
            }
            "specularscale" => {
                let Some(first) = number(0) else { return 0 };
                self.specular_scale[0] = first;
                let Some(second) = number(1) else { return 1 };
                self.specular_scale[1] = second;
                let Some(third) = number(2) else {
                    // Two values: RGB, then gloss.
                    self.specular_scale[3] = 1.0 - second;
                    self.specular_scale[1] = first;
                    self.specular_scale[2] = first;
                    return 2;
                };
                self.specular_scale[2] = third;
                let Some(gloss) = number(3) else { return 3 };
                self.specular_scale[3] = 1.0 - gloss;
                4
            }
            _ => 0,
        }
    }

    /// Finish a parsed stage: rend2 loads an explicit packed map after the stage and
    /// replaces its specular scale then (`R_LoadPackedMaterialImage`): metalness,
    /// occlusion and roughness unscaled, base specular halved (0.04 against the
    /// shader's 0.08).
    pub fn finish(&mut self) {
        if self.specular_map.is_some()
            && !matches!(
                self.specular_layout,
                SpecularLayout::None | SpecularLayout::SpecGloss
            )
        {
            self.specular_scale = packed_specular_scale();
        }
    }
}

/// The specular scale rend2 gives a packed material map (`R_LoadPackedMaterialImage`).
pub fn packed_specular_scale() -> [f32; 4] {
    [1.0, 0.5, 1.0, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_shader_script;

    fn stage(script: &str) -> crate::ShaderStage {
        let definitions =
            parse_shader_script(script.as_bytes(), "scripts/test.shader").expect("script parses");
        definitions[0].stages[0].clone()
    }

    #[test]
    fn vanilla_stage_has_no_maps_and_rend2_initial_scales() {
        let stage = stage("textures/a { { map textures/a/b blendFunc filter } }");
        assert_eq!(stage.material, StageMaterial::default());
        assert!(!stage.material.has_explicit_maps());
        assert_eq!(stage.material.specular_scale, INITIAL_SPECULAR_SCALE);
    }

    #[test]
    fn normal_and_specular_keywords_record_images_and_reset_scales() {
        let stage = stage(
            "textures/a {\n{\nmap textures/a/b\nnormalScale 3\nnormalMap textures/a/b_n\n\
             specMap textures/a/b_spec\nspecularReflectance 0.5\ngloss 0.25\nparallaxBias 0.1\n}\n}",
        );
        let material = &stage.material;
        assert_eq!(material.normal_map.as_deref(), Some("textures/a/b_n"));
        assert!(!material.normal_height);
        // normalMap after normalScale resets to the defaults, as in rend2.
        assert_eq!(material.normal_scale, DEFAULT_NORMAL_SCALE);
        assert_eq!(material.specular_map.as_deref(), Some("textures/a/b_spec"));
        assert_eq!(material.specular_layout, SpecularLayout::SpecGloss);
        assert_eq!(material.specular_scale, [0.5, 0.5, 0.5, 0.75]);
        assert_eq!(material.parallax_bias, 0.1);
        // Material images never become diffuse or stage images.
        assert_eq!(stage.images, vec!["textures/a/b".to_string()]);
    }

    #[test]
    fn height_map_keyword_and_parallax_depth() {
        let stage = stage(
            "textures/a { { map textures/a/b normalHeightMap textures/a/b_nh parallaxDepth 0.1 } }",
        );
        assert!(stage.material.normal_height);
        assert_eq!(stage.material.normal_scale, [1.0, 1.0, 1.0, 0.1]);
    }

    #[test]
    fn normal_scale_takes_one_to_three_numbers() {
        let mut material = StageMaterial::default();
        let tokens = ["2", "map"];
        let taken = material.apply("normalscale", |i| tokens.get(i).map(|t| t.to_string()));
        assert_eq!(taken, 1);
        assert_eq!(material.normal_scale, [2.0, 2.0, 0.0, 0.0]);
        let tokens = ["2", "3", "0.2", "4"];
        let taken = material.apply("normalscale", |i| tokens.get(i).map(|t| t.to_string()));
        assert_eq!(taken, 3);
        assert_eq!(material.normal_scale, [2.0, 3.0, 0.0, 0.2]);
    }

    #[test]
    fn specular_scale_forms() {
        let mut material = StageMaterial::default();
        let tokens = ["0.5", "0.25", "}"];
        assert_eq!(
            material.apply("specularscale", |i| tokens.get(i).map(|t| t.to_string())),
            2
        );
        assert_eq!(material.specular_scale, [0.5, 0.5, 0.5, 0.75]);
        let tokens = ["0.1", "0.2", "0.3", "0.4"];
        assert_eq!(
            material.apply("specularscale", |i| tokens.get(i).map(|t| t.to_string())),
            4
        );
        assert_eq!(material.specular_scale, [0.1, 0.2, 0.3, 0.6]);
    }

    #[test]
    fn specular_exponent_maps_to_gloss_term() {
        let mut material = StageMaterial::default();
        let tokens = ["8192"];
        material.apply("specularexponent", |i| tokens.get(i).map(|t| t.to_string()));
        assert!(material.specular_scale[3].abs() < 1e-6);
        let tokens = ["1"];
        material.apply("specularexponent", |i| tokens.get(i).map(|t| t.to_string()));
        assert!((material.specular_scale[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn packed_maps_use_three_channel_layouts_and_replace_the_scale() {
        for (keyword, layout) in [
            ("rmoMap", SpecularLayout::Rmo),
            ("rmosMap", SpecularLayout::Rmo),
            ("moxrMap", SpecularLayout::Moxr),
            ("mosrMap", SpecularLayout::Moxr),
            ("ormMap", SpecularLayout::Orm),
            ("ormsMap", SpecularLayout::Orm),
        ] {
            let stage = stage(&format!(
                "textures/a {{ {{ map textures/a/b {keyword} textures/a/b_x roughness 0.3 }} }}"
            ));
            assert_eq!(stage.material.specular_layout, layout, "{keyword}");
            assert_eq!(
                stage.material.specular_map.as_deref(),
                Some("textures/a/b_x")
            );
            assert_eq!(stage.material.specular_scale, packed_specular_scale());
        }
    }

    #[test]
    fn keywords_do_not_swallow_following_directives() {
        let stage = stage(
            "textures/a { { map textures/a/b roughness blendFunc GL_ONE GL_ONE normalScale \
             rgbGen identity } }",
        );
        assert_eq!(stage.blend, crate::StageBlend::Add);
        assert_eq!(stage.rgb_generator.as_deref(), Some("identity"));
        assert_eq!(stage.material.specular_scale, INITIAL_SPECULAR_SCALE);
    }

    #[test]
    fn anim_map_frames_stop_at_material_keywords() {
        let stage = stage(
            "textures/a { { animMap 5 textures/a/1 textures/a/2 normalMap textures/a/1_n } }",
        );
        assert_eq!(stage.images.len(), 2);
        assert_eq!(stage.material.normal_map.as_deref(), Some("textures/a/1_n"));
    }
}

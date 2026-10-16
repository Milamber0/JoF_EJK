//! rd-vanilla's view of a stage's colour generators.
//!
//! [`ShaderStage::rgb_generator`] and [`ShaderStage::alpha_generator`] keep the
//! authored text, and an unset generator stays `None`. rd-vanilla resolves both
//! while parsing (`ParseStage`, `codemp/rd-vanilla/tr_shader.cpp:1399-1716`):
//! unknown names are ignored, `rgbGen vertex` also selects vertex alpha, an unset
//! rgbGen becomes identity or identityLighting depending on the blend source,
//! and identity alpha is skipped for identity or lightingDiffuse colour. Code
//! that must reproduce a decision rd-vanilla takes on those resolved values,
//! such as multitexture collapsing, compares [`StageColour`] instead.

/// `colorGen_t` values a shader script can select (`tr_local.h`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RgbGen {
    Identity,
    IdentityLighting,
    Entity,
    OneMinusEntity,
    Vertex,
    ExactVertex,
    OneMinusVertex,
    Waveform,
    LightingDiffuse,
    LightingDiffuseEntity,
    Const,
}

/// `alphaGen_t` values a shader script can select, plus the parser's `Skip`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlphaGen {
    Identity,
    /// `AGEN_SKIP`: identity alpha that rd-vanilla does not compute.
    Skip,
    Entity,
    OneMinusEntity,
    Vertex,
    OneMinusVertex,
    LightingSpecular,
    Waveform,
    Portal,
    Const,
    Dot,
    OneMinusDot,
}

/// A stage's colour generators after rd-vanilla `ParseStage` defaults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageColour {
    pub rgb: RgbGen,
    pub alpha: AlphaGen,
}

impl StageColour {
    /// A stage that declares `rgbGen identity` (its identity alpha is skipped).
    pub const IDENTITY: Self = Self {
        rgb: RgbGen::Identity,
        alpha: AlphaGen::Skip,
    };
}

/// Tracks the directives of one stage in source order, as `ParseStage` does.
#[derive(Debug, Default)]
pub(crate) struct StageColourParser {
    rgb: Option<RgbGen>,
    alpha: Option<AlphaGen>,
    lighting_source: bool,
}

impl StageColourParser {
    /// `blendFunc`: rd-vanilla keeps the source factor of the last one.
    /// `add` and `blend` use GL_ONE and GL_SRC_ALPHA, `filter` GL_DST_COLOR, and
    /// an unknown source name is replaced by GL_ONE (`NameToSrcBlendMode`).
    pub(crate) fn blend(&mut self, first: &str) {
        let first = first.to_ascii_lowercase();
        self.lighting_source = match first.as_str() {
            "add" | "blend" | "gl_one" | "gl_src_alpha" => true,
            "filter"
            | "gl_zero"
            | "gl_dst_color"
            | "gl_one_minus_dst_color"
            | "gl_one_minus_src_alpha"
            | "gl_dst_alpha"
            | "gl_one_minus_dst_alpha"
            | "gl_src_alpha_saturate" => false,
            _ => true,
        };
    }

    /// `rgbGen <name>`; an unknown name leaves the previous value.
    pub(crate) fn rgb(&mut self, name: &str) {
        let generator = match name.to_ascii_lowercase().as_str() {
            "wave" => RgbGen::Waveform,
            "const" => RgbGen::Const,
            "identity" => RgbGen::Identity,
            "identitylighting" => RgbGen::IdentityLighting,
            "entity" => RgbGen::Entity,
            "oneminusentity" => RgbGen::OneMinusEntity,
            "vertex" => RgbGen::Vertex,
            "exactvertex" => RgbGen::ExactVertex,
            "lightingdiffuse" => RgbGen::LightingDiffuse,
            "lightingdiffuseentity" => RgbGen::LightingDiffuseEntity,
            "oneminusvertex" => RgbGen::OneMinusVertex,
            _ => return,
        };
        // `AGEN_IDENTITY` is zero, so an explicit `alphaGen identity` before
        // `rgbGen vertex` is replaced as well.
        if generator == RgbGen::Vertex && matches!(self.alpha, None | Some(AlphaGen::Identity)) {
            self.alpha = Some(AlphaGen::Vertex);
        }
        self.rgb = Some(generator);
    }

    /// `alphaGen <name>`; an unknown name leaves the previous value.
    pub(crate) fn alpha(&mut self, name: &str) {
        self.alpha = Some(match name.to_ascii_lowercase().as_str() {
            "wave" => AlphaGen::Waveform,
            "const" => AlphaGen::Const,
            "identity" => AlphaGen::Identity,
            "entity" => AlphaGen::Entity,
            "oneminusentity" => AlphaGen::OneMinusEntity,
            "vertex" => AlphaGen::Vertex,
            "lightingspecular" => AlphaGen::LightingSpecular,
            "oneminusvertex" => AlphaGen::OneMinusVertex,
            "dot" => AlphaGen::Dot,
            "oneminusdot" => AlphaGen::OneMinusDot,
            "portal" => AlphaGen::Portal,
            _ => return,
        });
    }

    /// The defaults at the end of `ParseStage` (`tr_shader.cpp:1685-1716`).
    pub(crate) fn finish(&self) -> StageColour {
        let rgb = self.rgb.unwrap_or(if self.lighting_source {
            RgbGen::IdentityLighting
        } else {
            RgbGen::Identity
        });
        let mut alpha = self.alpha.unwrap_or(AlphaGen::Identity);
        if alpha == AlphaGen::Identity && matches!(rgb, RgbGen::Identity | RgbGen::LightingDiffuse)
        {
            alpha = AlphaGen::Skip;
        }
        StageColour { rgb, alpha }
    }
}

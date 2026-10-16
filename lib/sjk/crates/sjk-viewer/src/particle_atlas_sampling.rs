//! Sample only the particle shader stages consumed by the current draw.
use crate::*;

impl ParticleLayerSample {
    /// The effect's colour `rgb` as this stage draws it, before the stage's own
    /// `rgbGen` wave. rd-vanilla colours a stage with the effect only through `rgbGen
    /// vertex` (`exactVertex`, `entity`); SJK keeps the effect's colour on its other
    /// additive and blended stages, but not on one that multiplies the scene
    /// (`GL_DST_COLOR` with `GL_ZERO` or `GL_SRC_COLOR`). Its neutral texels, white or
    /// mid-grey, leave the scene as it is only at full colour: the repeater's alt-fire
    /// burst (`effects/repeater/concussion.efx`) fades its `rgbGen identity` shock balls,
    /// and SJK drew them as dark squares (Sol, 11/10/2026).
    pub(crate) fn tint(&self, rgb: [f32; 3]) -> [f32; 3] {
        let multiplies = matches!(
            self.blend,
            ParticleBlend::Filter | ParticleBlend::TwiceModulate
        );
        if multiplies && !self.tinted {
            [1.0; 3]
        } else {
            rgb
        }
    }
}

impl ParticleAtlas {
    /// Sample the first authored stage, or the atlas fallback for an unknown shader.
    pub(crate) fn first_layer(&self, shader: &str, age_seconds: f32) -> ParticleLayerSample {
        self.stages(shader)
            .and_then(|animations| animations.first())
            .map(|animation| self.sample_animation(animation, age_seconds))
            .unwrap_or(ParticleLayerSample {
                uv_rect: self.fallback,
                blend: ParticleBlend::Add,
                rgb: 1.0,
                alpha: 1.0,
                uv_transform: [1.0, 1.0, 0.0, 0.0],
                glow: false,
                tinted: true,
            })
    }

    /// Evaluate one stage's original frame, waveform and texture-coordinate rules.
    pub(crate) fn sample_animation(
        &self,
        animation: &ParticleAtlasAnimation,
        age_seconds: f32,
    ) -> ParticleLayerSample {
        let age_seconds = age_seconds - animation.time_offset;
        if animation.frames.len() == 1 || animation.frequency <= 0.0 {
            return ParticleLayerSample {
                uv_rect: animation.frames.first().copied().unwrap_or(self.fallback),
                blend: animation.blend,
                rgb: effect_wave::evaluate(animation.rgb_wave.as_ref(), age_seconds),
                alpha: effect_wave::evaluate(animation.alpha_wave.as_ref(), age_seconds),
                uv_transform: effect_texcoords::sample(
                    animation.tc_scale,
                    animation.tc_scroll,
                    age_seconds,
                ),
                glow: animation.glow,
                tinted: animation.tinted,
            };
        }
        let raw = (age_seconds.max(0.0) * animation.frequency).floor() as usize;
        let frame = if animation.one_shot {
            raw.min(animation.frames.len() - 1)
        } else {
            raw % animation.frames.len()
        };
        ParticleLayerSample {
            uv_rect: animation.frames[frame],
            blend: animation.blend,
            rgb: effect_wave::evaluate(animation.rgb_wave.as_ref(), age_seconds),
            alpha: effect_wave::evaluate(animation.alpha_wave.as_ref(), age_seconds),
            uv_transform: effect_texcoords::sample(
                animation.tc_scale,
                animation.tc_scroll,
                age_seconds,
            ),
            glow: animation.glow,
            tinted: animation.tinted,
        }
    }

    /// Retain a borrowed stage selection without filling an eight-stage scratch array.
    pub(crate) fn layers_for(
        &self,
        shader: &str,
        age_seconds: f32,
    ) -> effect_runtime::ParticleLayerSamples<'_> {
        effect_runtime::ParticleLayerSamples::new(
            self,
            self.stages(shader).map_or(&[], Vec::as_slice),
            age_seconds,
        )
    }

    /// A shader's stages. Shader names are case-insensitive (the atlas keys them in
    /// lower case); a mixed-case name such as `gfx/effects/saberFlare` used to miss
    /// and draw the fallback spark, scaled to the clash flare's full-screen size.
    /// A mixed-case name is lowered into a reused per-thread buffer, so the lookup
    /// does not allocate after the first one.
    fn stages(&self, shader: &str) -> Option<&Vec<ParticleAtlasAnimation>> {
        if !shader.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return self.animations.get(shader);
        }
        thread_local! {
            static LOWER: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
        }
        LOWER.with_borrow_mut(|lower| {
            lower.clear();
            lower.extend(shader.chars().map(|c| c.to_ascii_lowercase()));
            self.animations.get(lower.as_str())
        })
    }
}

#[cfg(test)]
mod tint_tests {
    use super::*;

    fn layer(blend: ParticleBlend, tinted: bool) -> ParticleLayerSample {
        ParticleLayerSample {
            uv_rect: [0.0; 4],
            blend,
            rgb: 1.0,
            alpha: 1.0,
            uv_transform: [1.0, 1.0, 0.0, 0.0],
            glow: false,
            tinted,
        }
    }

    #[test]
    fn a_multiplying_stage_keeps_its_neutral_texels_neutral() {
        // `concussion.efx` fades its shock balls' colour towards black.
        let faded = [0.1; 3];
        assert_eq!(
            layer(ParticleBlend::TwiceModulate, false).tint(faded),
            [1.0; 3]
        );
        assert_eq!(layer(ParticleBlend::Filter, false).tint(faded), [1.0; 3]);
        // `rgbGen vertex` asks for the colour; other blends keep fading as before.
        assert_eq!(layer(ParticleBlend::TwiceModulate, true).tint(faded), faded);
        assert_eq!(layer(ParticleBlend::Add, false).tint(faded), faded);
        assert_eq!(layer(ParticleBlend::Alpha, false).tint(faded), faded);
    }
}

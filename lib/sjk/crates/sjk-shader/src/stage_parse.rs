//! Stage directive parsing.
use super::*;

pub(super) fn parse_stage(
    tokens: &[String],
    cursor: &mut usize,
    source: &VirtualPath,
    definition: &mut ShaderDefinition,
) -> Result<(), ShaderError> {
    let mut stage = ShaderStage {
        surface_sprites: None,
        portal_range: None,
        images: Vec::new(),
        video: None,
        animation_frequency: None,
        one_shot: false,
        clamp: false,
        blend: StageBlend::Replace,
        glow: false,
        alpha_function: None,
        rgb_generator: None,
        alpha_generator: None,
        resolved_colour: StageColour::IDENTITY,
        rgb_wave: None,
        alpha_wave: None,
        texture_modifications: Vec::new(),
        rgb_constant: None,
        alpha_constant: None,
        texture_generator: TextureGenerator::Base,
        depth_write: true,
        depth_function: DepthFunction::LessEqual,
        material: StageMaterial::default(),
    };
    let mut depth_write_explicit = false;
    let mut colour = stage_colour::StageColourParser::default();
    while token(tokens, *cursor, source)? != "}" {
        let directive = tokens[*cursor].to_ascii_lowercase();
        *cursor += 1;
        match directive.as_str() {
            "surfacesprites" => {
                stage.surface_sprites = surface_sprites::parse(tokens, cursor, source)?
            }
            "ssfademax" | "ssfadescale" | "ssvariance" | "sshangdown" | "ssanyangle"
            | "ssfaceup" | "sswind" | "sswindidle" | "ssvertskew" | "ssfxduration" | "ssfxgrow"
            | "ssfxalpharange" | "ssfxweather" => surface_sprites::optional(
                &directive,
                tokens,
                cursor,
                source,
                &mut stage.surface_sprites,
            )?,
            "map" => {
                let image = token(tokens, *cursor, source)?.to_owned();
                if image.eq_ignore_ascii_case("$lightmap") {
                    stage.texture_generator = TextureGenerator::Lightmap;
                }
                stage.images.push(image);
                *cursor += 1;
            }
            "clampmap" => {
                stage.clamp = true;
                stage
                    .images
                    .push(token(tokens, *cursor, source)?.to_owned());
                *cursor += 1;
            }
            "animmap" | "oneshotanimmap" => {
                stage.one_shot = directive == "oneshotanimmap";
                stage.animation_frequency = token(tokens, *cursor, source)?.parse::<f32>().ok();
                *cursor += 1;
                while token(tokens, *cursor, source)? != "}"
                    && !is_stage_directive(token(tokens, *cursor, source)?)
                {
                    stage
                        .images
                        .push(token(tokens, *cursor, source)?.to_owned());
                    *cursor += 1;
                }
            }
            "videomap" => {
                stage.video = Some(token(tokens, *cursor, source)?.to_owned());
                *cursor += 1;
            }
            "glow" => stage.glow = true,
            "blendfunc" => {
                let first = token(tokens, *cursor, source)?.to_ascii_lowercase();
                *cursor += 1;
                colour.blend(&first);
                stage.blend = match first.as_str() {
                    "add" => StageBlend::Add,
                    "filter" => StageBlend::Filter,
                    "blend" => StageBlend::Alpha,
                    _ if first.starts_with("gl_") => {
                        let second = token(tokens, *cursor, source)?.to_ascii_lowercase();
                        *cursor += 1;
                        match (first.as_str(), second.as_str()) {
                            // rd-vanilla `tr_shader.cpp:1700-1707`: GL_ONE GL_ZERO
                            // is an opaque stage that keeps writing depth.
                            ("gl_one", "gl_zero") => StageBlend::Replace,
                            ("gl_one", "gl_one") => StageBlend::Add,
                            ("gl_dst_color", "gl_zero") | ("gl_zero", "gl_src_color") => {
                                StageBlend::Filter
                            }
                            ("gl_src_alpha", "gl_one_minus_src_alpha") => StageBlend::Alpha,
                            _ => StageBlend::Custom {
                                source: first,
                                destination: second,
                            },
                        }
                    }
                    _ => StageBlend::Replace,
                };
                if !depth_write_explicit {
                    stage.depth_write = stage.blend == StageBlend::Replace;
                }
            }
            "alphafunc" => {
                stage.alpha_function = Some(token(tokens, *cursor, source)?.to_ascii_lowercase());
                *cursor += 1;
            }
            "rgbgen" => {
                let (generator, wave) = parse_generator(tokens, cursor, source)?;
                colour.rgb(&generator);
                if generator == "const" {
                    stage.rgb_constant = Some(parse_parenthesized_vec3(tokens, cursor, source)?);
                }
                stage.rgb_generator = Some(generator);
                stage.rgb_wave = wave;
            }
            "alphagen" => {
                let (generator, wave) = parse_generator(tokens, cursor, source)?;
                colour.alpha(&generator);
                if generator == "portal" {
                    let range = tokens
                        .get(*cursor)
                        .and_then(|v| v.parse::<f32>().ok())
                        .filter(|v| v.is_finite() && *v > 0.0);
                    if range.is_some() {
                        *cursor += 1;
                    }
                    stage.portal_range = Some(range.unwrap_or(256.0));
                    definition.portal_range = stage.portal_range;
                }
                if generator == "const" {
                    stage.alpha_constant = Some(parse_number(tokens, cursor, source)?);
                }
                stage.alpha_generator = Some(generator);
                stage.alpha_wave = wave;
            }
            "tcgen" => {
                stage.texture_generator = match token(tokens, *cursor, source)?
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "lightmap" => TextureGenerator::Lightmap,
                    "environment" | "environmentmapped" => TextureGenerator::Environment,
                    _ => TextureGenerator::Base,
                };
                *cursor += 1;
            }
            "depthwrite" => {
                stage.depth_write = true;
                depth_write_explicit = true;
            }
            "depthfunc" => {
                stage.depth_function = match token(tokens, *cursor, source)?
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "equal" => DepthFunction::Equal,
                    "disable" | "none" => DepthFunction::Disabled,
                    _ => DepthFunction::LessEqual,
                };
                *cursor += 1;
            }
            "tcmod" => {
                let kind = token(tokens, *cursor, source)?.to_ascii_lowercase();
                *cursor += 1;
                if kind == "stretch" {
                    let function = token(tokens, *cursor, source)?.to_ascii_lowercase();
                    *cursor += 1;
                    let mut values = [0.0; 4];
                    for value in &mut values {
                        *value = parse_number(tokens, cursor, source)?;
                    }
                    stage.texture_modifications.push(TextureModification {
                        kind,
                        arguments: Vec::new(),
                        wave: Some(WaveForm {
                            function,
                            base: values[0],
                            amplitude: values[1],
                            phase: values[2],
                            frequency: values[3],
                        }),
                    });
                    continue;
                }
                let argument_count = match kind.as_str() {
                    "scroll" | "scale" => 2,
                    "rotate" => 1,
                    "transform" => 6,
                    "turb" => 4,
                    _ => 0,
                };
                let mut arguments = Vec::with_capacity(argument_count);
                for _ in 0..argument_count {
                    let argument = token(tokens, *cursor, source)?;
                    let Ok(value) = argument.parse::<f32>() else {
                        break;
                    };
                    arguments.push(value);
                    *cursor += 1;
                }
                stage.texture_modifications.push(TextureModification {
                    kind,
                    arguments,
                    wave: None,
                });
            }
            _ if StageMaterial::is_directive(&directive) => {
                let start = *cursor;
                *cursor += stage
                    .material
                    .apply(&directive, |offset| tokens.get(start + offset).cloned());
            }
            _ => {}
        }
    }
    *cursor += 1;
    stage.material.finish();
    stage.resolved_colour = colour.finish();
    definition.stage_images.extend(stage.images.iter().cloned());
    if stage.glow || stage.blend == StageBlend::Add {
        definition
            .emissive_images
            .extend(stage.images.iter().cloned());
    } else {
        definition
            .diffuse_images
            .extend(stage.images.iter().cloned());
    }
    definition.stages.push(stage);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::parse_shader_script;

    #[test]
    fn video_map_names_the_stage_video_and_ends_at_the_next_directive() {
        let definitions = parse_shader_script(
            b"textures/video/raven\n{\n{\nvideoMap video/openinglogos\nblendFunc add\n}\n}",
            "scripts/cinematics.shader",
        )
        .expect("script parses");
        let stage = &definitions[0].stages[0];
        assert_eq!(stage.video.as_deref(), Some("video/openinglogos"));
        assert!(stage.images.is_empty());
        assert_eq!(stage.blend, crate::StageBlend::Add);
    }
}

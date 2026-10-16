//! Shader script parsing and token helpers.
use super::*;

pub(super) fn parse_script(
    bytes: &[u8],
    source: &VirtualPath,
) -> Result<Vec<ShaderDefinition>, ShaderError> {
    let text = String::from_utf8_lossy(bytes);
    let tokens = tokenize(&text, source)?;
    parse_tokens(&tokens, source)
}

pub(super) fn parse_tokens(
    tokens: &[String],
    source: &VirtualPath,
) -> Result<Vec<ShaderDefinition>, ShaderError> {
    let mut cursor = 0;
    let mut definitions = Vec::new();
    while cursor < tokens.len() {
        let name = tokens[cursor].clone();
        cursor += 1;
        expect(&tokens, &mut cursor, "{", source)?;
        let mut definition = ShaderDefinition {
            name,
            emits_light: false,
            surface_light: 0.0,
            light_image: None,
            editor_image: None,
            stage_images: Vec::new(),
            diffuse_images: Vec::new(),
            emissive_images: Vec::new(),
            deforms: Vec::new(),
            portal_range: None,
            stages: Vec::new(),
            sort: None,
            cull: ShaderCull::Front,
            sky: None,
            sun: None,
            fog: None,
            fog_contents: false,
            no_gl_fog: false,
            polygon_offset: false,
        };
        while token(&tokens, cursor, source)? != "}" {
            if tokens[cursor] == "{" {
                cursor += 1;
                parse_stage(&tokens, &mut cursor, source, &mut definition)?;
                continue;
            }
            let directive = tokens[cursor].to_ascii_lowercase();
            cursor += 1;
            match directive.as_str() {
                "portal" => definition.sort = Some(1.0),
                "q3map_surfacelight" => {
                    // Keep malformed/nonzero hints conservative for ambient-light consumers.
                    let value = token(&tokens, cursor, source)?.parse::<f32>();
                    definition.emits_light = value.as_ref().map_or(true, |v| *v != 0.0);
                    definition.surface_light = value.map_or(0.0, |v| v.max(0.0));
                    cursor += 1;
                }
                "q3map_lightimage" => {
                    definition.light_image = Some(token(&tokens, cursor, source)?.to_owned());
                    cursor += 1;
                }
                "deformvertexes" => {
                    if let Some(deform) = super::deforms::parse(&tokens, &mut cursor, source)? {
                        if definition.deforms.len() < 3 {
                            definition.deforms.push(deform);
                        }
                    }
                }
                "qer_editorimage" => {
                    definition.editor_image = Some(token(&tokens, cursor, source)?.to_owned());
                    cursor += 1;
                }
                "polygonoffset" => definition.polygon_offset = true,
                "sort" => {
                    definition.sort = Some(parse_sort(token(&tokens, cursor, source)?));
                    cursor += 1;
                }
                "cull" => {
                    definition.cull = match token(&tokens, cursor, source)?
                        .to_ascii_lowercase()
                        .as_str()
                    {
                        "none" | "twosided" | "disable" => ShaderCull::TwoSided,
                        "back" | "backside" | "backsided" => ShaderCull::Back,
                        _ => ShaderCull::Front,
                    };
                    cursor += 1;
                }
                "skyparms" => {
                    let outer = token(&tokens, cursor, source)?.to_owned();
                    cursor += 1;
                    let height = token(&tokens, cursor, source)?
                        .parse::<f32>()
                        .unwrap_or(0.0);
                    cursor += 1;
                    // OpenJK tr_shader.cpp:1932-1936 ignores the inner box;
                    // an omitted final argument must not consume the closing brace.
                    let inner = if token(&tokens, cursor, source)? == "}" {
                        None
                    } else {
                        let inner = box_name(tokens[cursor].clone());
                        cursor += 1;
                        inner
                    };
                    definition.sky = Some(SkyParms {
                        outer_box: box_name(outer),
                        cloud_height: if height == 0.0 { 512.0 } else { height },
                        inner_box: inner,
                    });
                }
                "surfaceparm" => {
                    definition.fog_contents |=
                        token(&tokens, cursor, source)?.eq_ignore_ascii_case("fog");
                    cursor += 1;
                }
                "noglfog" => definition.no_gl_fog = true,
                "fogparms" => {
                    // rd-vanilla tr_shader.cpp:2271-2288: `( r g b ) depth`;
                    // any trailing gradient values are ignored.
                    let mut values = [0.0; 4];
                    expect(&tokens, &mut cursor, "(", source)?;
                    for value in &mut values[..3] {
                        *value = token(&tokens, cursor, source)?.parse().unwrap_or(0.0);
                        cursor += 1;
                    }
                    expect(&tokens, &mut cursor, ")", source)?;
                    values[3] = token(&tokens, cursor, source)?.parse().unwrap_or(0.0);
                    cursor += 1;
                    definition.fog = Some(FogParms {
                        color: [values[0], values[1], values[2]],
                        depth_for_opaque: values[3],
                    });
                }
                "sun" | "q3map_sun" | "q3map_sunext" => {
                    let mut values = [0.0; 6];
                    for value in &mut values {
                        *value = token(&tokens, cursor, source)?
                            .parse::<f32>()
                            .unwrap_or(0.0);
                        cursor += 1;
                    }
                    let degrees = values[4].to_radians();
                    let elevation = values[5].to_radians();
                    definition.sun = Some(SunParms {
                        color: [values[0], values[1], values[2]],
                        intensity: values[3],
                        direction: [
                            degrees.cos() * elevation.cos(),
                            degrees.sin() * elevation.cos(),
                            elevation.sin(),
                        ],
                    });
                }
                _ => {}
            }
            // Unknown outer directives and their arguments are harmless
            // tokens until the next stage/opening or closing brace.
        }
        cursor += 1;
        if definition.sky.is_some() && definition.sort.is_none() {
            definition.sort = Some(2.0);
        }
        if definition.sort.is_none() && !definition.has_color_pass() {
            // rd-vanilla tr_shader.cpp:3184-3187, SS_FOG in tr_local.h:183.
            definition.sort = Some(12.0);
        }
        definitions.push(definition);
    }
    Ok(definitions)
}

pub(super) fn box_name(value: String) -> Option<String> {
    (value != "-").then_some(value)
}

/// Parse a standalone Quake 3 shader script. This is primarily useful for
/// tools and deterministic material-compiler fixtures; catalog loading uses
/// the same parser internally.
pub fn parse_shader_script(
    bytes: &[u8],
    source_name: &str,
) -> Result<Vec<ShaderDefinition>, ShaderError> {
    let source = VirtualPath::new(source_name).map_err(VfsError::from)?;
    parse_script(bytes, &source)
}

pub(super) fn parse_number(
    tokens: &[String],
    cursor: &mut usize,
    source: &VirtualPath,
) -> Result<f32, ShaderError> {
    let value = token(tokens, *cursor, source)?
        .parse::<f32>()
        .map_err(|_| ShaderError::Syntax {
            source: source.clone(),
            offset: *cursor,
            message: "shader directive requires a numeric argument",
        })?;
    *cursor += 1;
    Ok(value)
}

pub(super) fn parse_parenthesized_vec3(
    tokens: &[String],
    cursor: &mut usize,
    source: &VirtualPath,
) -> Result<[f32; 3], ShaderError> {
    let parenthesized = token(tokens, *cursor, source)? == "(";
    if parenthesized {
        *cursor += 1;
    }
    let result = [
        parse_number(tokens, cursor, source)?,
        parse_number(tokens, cursor, source)?,
        parse_number(tokens, cursor, source)?,
    ];
    if parenthesized && token(tokens, *cursor, source)? == ")" {
        *cursor += 1;
    }
    Ok(result)
}

pub(super) fn parse_sort(value: &str) -> f32 {
    match value.to_ascii_lowercase().as_str() {
        "portal" => 1.0,
        "sky" | "environment" => 2.0,
        "opaque" => 3.0,
        "decal" => 4.0,
        "seethrough" => 5.0,
        "banner" => 6.0,
        "inside" => 7.0,
        "mid_inside" => 8.0,
        "middle" => 9.0,
        "mid_outside" => 10.0,
        "outside" => 11.0,
        "underwater" => 13.0,
        "additive" => 15.0,
        "nearest" => 21.0,
        _ => value.parse().unwrap_or(3.0),
    }
}

pub(super) fn parse_generator(
    tokens: &[String],
    cursor: &mut usize,
    source: &VirtualPath,
) -> Result<(String, Option<WaveForm>), ShaderError> {
    let generator = token(tokens, *cursor, source)?.to_ascii_lowercase();
    *cursor += 1;
    if generator != "wave" {
        return Ok((generator, None));
    }
    let function = token(tokens, *cursor, source)?.to_ascii_lowercase();
    *cursor += 1;
    let mut values = [0.0; 4];
    for value in &mut values {
        *value = token(tokens, *cursor, source)?
            .parse::<f32>()
            .map_err(|_| ShaderError::Syntax {
                source: source.clone(),
                offset: *cursor,
                message: "wave generator requires four numeric arguments",
            })?;
        *cursor += 1;
    }
    Ok((
        generator,
        Some(WaveForm {
            function,
            base: values[0],
            amplitude: values[1],
            phase: values[2],
            frequency: values[3],
        }),
    ))
}

pub(super) fn is_stage_directive(token: &str) -> bool {
    let token = token.to_ascii_lowercase();
    StageMaterial::is_directive(&token)
        || matches!(
            token.as_str(),
            "map"
                | "clampmap"
                | "animmap"
                | "oneshotanimmap"
                | "videomap"
                | "blendfunc"
                | "alphafunc"
                | "rgbgen"
                | "alphagen"
                | "tcgen"
                | "tcmod"
                | "depthfunc"
                | "depthwrite"
                | "detail"
                | "glow"
                | "surfacesprites"
                | "ssfademax"
                | "ssfadescale"
                | "ssvariance"
                | "sshangdown"
                | "ssanyangle"
                | "ssfaceup"
                | "sswind"
                | "sswindidle"
                | "ssvertskew"
                | "ssfxduration"
                | "ssfxgrow"
                | "ssfxalpharange"
                | "ssfxweather"
        )
}

pub(super) fn tokenize(text: &str, source: &VirtualPath) -> Result<Vec<String>, ShaderError> {
    let (tokens, _, error) = lex(text, source);
    match error {
        Some(error) => Err(error),
        None => Ok(tokens),
    }
}

pub(super) fn lex(
    text: &str,
    source: &VirtualPath,
) -> (Vec<String>, Vec<bool>, Option<ShaderError>) {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    let mut tokens = Vec::new();
    let mut heads = Vec::new();
    let mut depth = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
            continue;
        }
        // ScanAndLoadShaderFiles accepts deprecated top-level hash comments.
        if bytes[cursor..].starts_with(b"//") || (depth == 0 && bytes[cursor] == b'#') {
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            let start = cursor;
            cursor += 2;
            while cursor + 1 < bytes.len() && !bytes[cursor..].starts_with(b"*/") {
                cursor += 1;
            }
            if cursor + 1 >= bytes.len() {
                return (
                    tokens,
                    heads,
                    Some(ShaderError::Syntax {
                        source: source.clone(),
                        offset: start,
                        message: "unterminated block comment",
                    }),
                );
            }
            cursor += 2;
            continue;
        }
        if matches!(bytes[cursor], b'{' | b'}') {
            heads.push(false);
            if bytes[cursor] == b'{' {
                depth += 1;
            } else {
                depth = depth.saturating_sub(1);
            }
            tokens.push(char::from(bytes[cursor]).to_string());
            cursor += 1;
            continue;
        }
        if bytes[cursor] == b'"' {
            let start = cursor;
            cursor += 1;
            let content_start = cursor;
            while cursor < bytes.len() && bytes[cursor] != b'"' {
                cursor += 1;
            }
            if cursor == bytes.len() {
                return (
                    tokens,
                    heads,
                    Some(ShaderError::Syntax {
                        source: source.clone(),
                        offset: start,
                        message: "unterminated quoted token",
                    }),
                );
            }
            heads.push(line_head(text, start));
            tokens.push(text[content_start..cursor].to_owned());
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'{' | b'}')
        {
            cursor += 1;
        }
        heads.push(line_head(text, start));
        tokens.push(text[start..cursor].to_owned());
    }
    (tokens, heads, None)
}

fn line_head(text: &str, offset: usize) -> bool {
    text[..offset]
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .trim()
        .is_empty()
}

pub(super) fn expect(
    tokens: &[String],
    cursor: &mut usize,
    expected: &str,
    source: &VirtualPath,
) -> Result<(), ShaderError> {
    let actual = token(tokens, *cursor, source)?;
    if actual != expected {
        return Err(ShaderError::Syntax {
            source: source.clone(),
            offset: *cursor,
            message: "expected opening brace after shader name",
        });
    }
    *cursor += 1;
    Ok(())
}

pub(super) fn token<'a>(
    tokens: &'a [String],
    cursor: usize,
    source: &VirtualPath,
) -> Result<&'a str, ShaderError> {
    tokens
        .get(cursor)
        .map(String::as_str)
        .ok_or_else(|| ShaderError::Syntax {
            source: source.clone(),
            offset: cursor,
            message: "unexpected end of shader script",
        })
}

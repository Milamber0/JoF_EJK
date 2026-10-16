//! Material-level fault isolation for community scripts.

use crate::{ShaderDefinition, ShaderError, parse};
use sjk_vfs::VirtualPath;

pub(super) fn parse_recovering(
    bytes: &[u8],
    source: &VirtualPath,
    mut warning: impl FnMut(ShaderError),
) -> Vec<ShaderDefinition> {
    let text = String::from_utf8_lossy(bytes);
    let (tokens, heads, mut lexical_error) = parse::lex(&text, source);
    let mut definitions = Vec::new();
    let mut start = 0;
    while start < tokens.len() {
        let mut end = start + 1;
        let mut depth = 0usize;
        let mut closed = false;
        let mut recovery_anchor = None;
        if tokens.get(end).is_some_and(|token| token == "{") {
            depth = 1;
            end += 1;
            while end < tokens.len() {
                // Only use this heuristic if brace balancing fails. Valid
                // flag-only directives may also precede a stage opening brace.
                if heads[end] && definition_start(&tokens, end) && recovery_anchor.is_none() {
                    recovery_anchor = Some(end);
                }
                match tokens[end].as_str() {
                    "{" => depth += 1,
                    "}" => depth -= 1,
                    _ => {}
                }
                end += 1;
                if depth == 0 {
                    closed = true;
                    break;
                }
            }
            if !closed {
                if let Some(anchor) = recovery_anchor {
                    end = anchor;
                }
            }
        } else {
            while end < tokens.len() && !definition_start(&tokens, end) {
                end += 1;
            }
        }
        let result = if closed {
            parse::parse_tokens(&tokens[start..end], source)
        } else {
            let error = if end == tokens.len() {
                lexical_error.take()
            } else {
                None
            };
            Err(error.unwrap_or_else(|| ShaderError::Syntax {
                source: source.clone(),
                offset: start,
                message: if depth > 0 {
                    "missing closing brace before next definition or end of script"
                } else {
                    "expected opening brace after shader name"
                },
            }))
        };
        match result {
            Ok(mut parsed) => definitions.append(&mut parsed),
            Err(error) => warning(ShaderError::SkippedShader {
                name: tokens[start].clone(),
                error: Box::new(error),
            }),
        }
        start = end;
    }
    if let Some(error) = lexical_error {
        warning(error);
    }
    definitions
}

fn definition_start(tokens: &[String], index: usize) -> bool {
    if !tokens.get(index + 1).is_some_and(|token| token == "{") {
        return false;
    }
    let name = tokens[index].as_str();
    // Tool hints and renderer flags can stand alone immediately before a stage.
    // They are not material names, even in a script with a missing closing brace.
    let lower = name.to_ascii_lowercase();
    !lower.starts_with("q3map_")
        && !lower.starts_with("qer_")
        && !parse::is_stage_directive(&lower)
        && !matches!(
            lower.as_str(),
            "{" | "}"
                | "portal"
                | "notc"
                | "nomipmaps"
                | "nopicmip"
                | "polygonoffset"
                | "entitymergable"
                | "noglfog"
                | "skyparms"
                | "surfaceparm"
                | "fogparms"
                | "sort"
                | "cull"
                | "sun"
                | "deformvertexes"
        )
}

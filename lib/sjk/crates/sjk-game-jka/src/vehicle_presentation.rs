//! Client-only vehicle weapon assets from `vehWeaponFields` in multiplayer
//! `bg_vehicleLoad.c`. These names do not register server configstrings.

use crate::text_parse::TextParser;

/// Authored projectile presentation. Empty fields retain stock's null handles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponPresentation {
    /// EFX trail emitted each presented frame.
    pub shot_effect: Option<String>,
    /// Optional rigid projectile model.
    pub model: Option<String>,
    /// Optional flight loop.
    pub loop_sound: Option<String>,
}

pub(crate) fn parse(text: &[u8], name: &[u8]) -> WeaponPresentation {
    let mut parser = TextParser::new(text);
    let mut result = WeaponPresentation::default();
    if !crate::vehicle_parms::find_block(&mut parser, name) {
        return result;
    }
    loop {
        parser.skip_rest_of_line();
        let key = parser.parse_ext(true);
        if key.is_empty() || key == b"}" {
            break;
        }
        let field = if key.eq_ignore_ascii_case(b"shotFX") {
            Some(&mut result.shot_effect)
        } else if key.eq_ignore_ascii_case(b"model") {
            Some(&mut result.model)
        } else if key.eq_ignore_ascii_case(b"loopSound") {
            Some(&mut result.loop_sound)
        } else {
            None
        };
        let value = parser.parse_ext(true);
        if let Some(field) = field {
            *field = std::str::from_utf8(value)
                .ok()
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
        }
    }
    result
}

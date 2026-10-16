//! An NPC's sabers (`NPC_ParseParms`, `NPC_stats.c:2647-3505`): `saber` and `saber2` by
//! name (read through [`SaberParms::parse`], `WP_SaberParseParms`), then colours, lengths
//! and radii for a hand's every blade or one of them, and the starting style.
//!
//! The per-blade keys are `saber` or `saber2`, then `Color`, `Length` or `Radius`, then
//! nothing (every blade) or a blade from 2 to 8; `saberColor1` and the like are not keys.

use crate::npc_parms::NpcRefusalReason;
use crate::npc_parms_keys::{Reader, float_or_skip, int_or_skip};
use crate::saber_definition::{MAX_BLADES, SFL_TWO_HANDED, SaberDefinition, SaberParseHost};
use crate::text_parse::TextParser;

/// What a per-blade key sets.
#[derive(Clone, Copy)]
enum BladeField {
    Color,
    Length,
    Radius,
}

/// A per-blade key: the hand, the field, and one blade or all of them.
fn blade_key(key: &[u8]) -> Option<(usize, BladeField, Option<usize>)> {
    let rest = key.strip_prefix(b"saber")?;
    let (hand, rest) = match rest.strip_prefix(b"2") {
        Some(rest) => (1, rest),
        None => (0, rest),
    };
    let (field, suffix) = [
        (&b"color"[..], BladeField::Color),
        (b"length", BladeField::Length),
        (b"radius", BladeField::Radius),
    ]
    .into_iter()
    .find_map(|(name, field)| rest.strip_prefix(name).map(|suffix| (field, suffix)))?;
    let blade = match suffix {
        [] => None,
        [digit @ b'2'..=b'8'] => Some(usize::from(digit - b'1')),
        _ => return None,
    };
    Some((hand, field, blade))
}

/// `@` and a saber's name: the model configstring a saber is sent to clients by.
fn saber_model(name: &[u8]) -> Vec<u8> {
    [b"@".as_slice(), name].concat()
}

/// A saber key, if `key` is one.
pub(crate) fn saber_key(
    reader: &mut Reader<'_>,
    key: &[u8],
    parser: &mut TextParser<'_>,
    host: &mut impl SaberParseHost,
) -> Result<bool, NpcRefusalReason> {
    let npc = &mut *reader.npc;
    let parse = |name: &[u8], host: &mut _| {
        reader
            .sabers
            .parse(name, host)
            .map(|(_, saber)| saber)
            .map_err(NpcRefusalReason::Saber)
    };
    match key {
        b"saber" => {
            let name = parser.parse_string();
            npc.sabers[0] = parse(name, host)?;
            npc.registered_models.push(saber_model(name));
            npc.saber_models[0] = Some(saber_model(name));
        }
        b"saber2" => {
            let name = parser.parse_string();
            // "can't use a second saber if first one is a two-handed saber...?"
            if npc.sabers[0].flags & SFL_TWO_HANDED == 0 {
                npc.sabers[1] = parse(name, host)?;
                if npc.sabers[1].flags & SFL_TWO_HANDED != 0 {
                    // "tsk tsk, can't use a twoHanded saber as second saber"
                    npc.sabers[1] = SaberDefinition::removed(host);
                } else {
                    npc.registered_models.push(saber_model(name));
                    npc.saber_models[1] = Some(saber_model(name));
                }
            }
        }
        b"saberstyle" => {
            if let Some(style) = int_or_skip(parser) {
                npc.saber_style = Some(style.clamp(0, 5));
            }
        }
        _ => {
            let Some((hand, field, one)) = blade_key(key) else {
                return Ok(false);
            };
            let blades = one.map_or(0..MAX_BLADES, |blade| blade..blade + 1);
            match field {
                BladeField::Color => {
                    let color = crate::saber_keywords::color(parser.parse_string(), host);
                    for blade in blades {
                        npc.sabers[hand].blades[blade].color = color;
                        // Only the whole hand's colour is sent, rewritten for each blade.
                        if one.is_none() {
                            npc.bolt_to_player = bolt_colour(npc.bolt_to_player, hand, color);
                        }
                    }
                }
                BladeField::Length => {
                    if let Some(length) = float_or_skip(parser) {
                        blades.for_each(|blade| {
                            npc.sabers[hand].blades[blade].length_max = length.max(4.0)
                        });
                    }
                }
                BladeField::Radius => {
                    if let Some(radius) = float_or_skip(parser) {
                        blades.for_each(|blade| {
                            npc.sabers[hand].blades[blade].radius = radius.max(0.25)
                        });
                    }
                }
            }
        }
    }
    Ok(true)
}

/// `s.boltToPlayer` with a hand's colour (`NPC_stats.c:2715-2724`, `2821-2830`): the
/// first hand's colour plus one in bits 0-2, the second's in bits 3-5, the other hand's
/// bits kept. The reference writes it once per blade, to the same end.
fn bolt_colour(bolt: i32, hand: usize, color: i32) -> i32 {
    if hand == 0 {
        (bolt & 0x38) + (color + 1)
    } else {
        (bolt & 0x7) + ((color + 1) << 3)
    }
}

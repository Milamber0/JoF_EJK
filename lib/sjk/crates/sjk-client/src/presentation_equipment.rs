//! Legacy held equipment and clientinfo colour projection.

use sjk_protocol::GameState;
use sjk_runtime::{HeldEquipment, HeldItemKind};

/// Snapshot-derived equipment, including primary detachment from the hand.
pub(super) fn legacy_equipment(
    game_state: &GameState,
    client_num: u16,
    weapon: u8,
    saber_holstered: u8,
    saber_move: u32,
    saber_in_flight: bool,
    alive: bool,
) -> Option<HeldEquipment> {
    let kind = legacy_held_item_kind(weapon)?;
    Some(HeldEquipment {
        kind,
        weapon,
        primary_in_flight: kind == HeldItemKind::EnergyBlade && saber_in_flight,
        active: equipment_active(kind, saber_holstered, alive),
        color: client_saber_color(game_state, client_num),
        secondary_active: secondary_equipment_active(kind, saber_holstered, alive),
        secondary_color: client_saber_color_at(game_state, client_num, 1),
        trail_duration_millis: trail_duration(kind, saber_move),
    })
}

/// Translate the codemp weapon enumeration into a portable held-item category.
pub(super) fn legacy_held_item_kind(weapon: u8) -> Option<HeldItemKind> {
    match weapon {
        1 | 2 => Some(HeldItemKind::Melee),
        3 => Some(HeldItemKind::EnergyBlade),
        4.. => Some(HeldItemKind::Ranged),
        _ => None,
    }
}

/// Secondary blades follow the authoritative partial-holster state.
pub(super) fn secondary_equipment_active(
    kind: HeldItemKind,
    saber_holstered: u8,
    alive: bool,
) -> bool {
    kind != HeldItemKind::EnergyBlade || (alive && saber_holstered == 0)
}

/// Authored trail duration for the selected legacy saber move.
pub(super) fn trail_duration(kind: HeldItemKind, saber_move: u32) -> u16 {
    if kind == HeldItemKind::EnergyBlade {
        crate::legacy_saber_trail_length(saber_move)
    } else {
        0
    }
}

/// Primary blade activity independent of whether its hilt is held or flying.
pub(super) fn equipment_active(kind: HeldItemKind, saber_holstered: u8, alive: bool) -> bool {
    // CG_Player keeps the primary saber blade live for partial holster state 1
    // (dual/staff secondary blade off), and turns it fully off at state 2.
    // Single-saber clients ordinarily transition 0 <-> 2.
    kind != HeldItemKind::EnergyBlade || (alive && saber_holstered < 2)
}

/// Primary blade colour from legacy clientinfo.
fn client_saber_color(game_state: &GameState, client_num: u16) -> [u8; 3] {
    client_saber_color_at(game_state, client_num, 0)
}

/// Blade colour of the selected primary or secondary clientinfo slot.
fn client_saber_color_at(game_state: &GameState, client_num: u16, saber: usize) -> [u8; 3] {
    const CS_PLAYERS: usize = 1_131;
    game_state
        .config_string(CS_PLAYERS + usize::from(client_num))
        .map_or(legacy_blade_rgb(4), |config| {
            saber_color_from_config(config, saber)
        })
}

/// Blade colour of saber 0/1 from a player configstring: `c1`/`c2` carry the
/// `saber_colors_t` index; JA+/TaystJK servers add the packed custom tint as
/// `c3`/`c4` (TaystJK `codemp/game/g_client.c:2717-2718`) which applies to
/// any index past purple.  Without those keys such an index rolls over to a
/// stock colour like `ClampSaberColor` (`codemp/cgame/cg_players.c:6066`).
pub(super) fn saber_color_from_config(config: &[u8], saber: usize) -> [u8; 3] {
    const KEYS: [(&[u8], &[u8]); 2] = [(b"c1", b"c3"), (b"c2", b"c4")];
    const NUM_SABER_COLORS: i32 = 12;
    const SABER_RGB: i32 = 6;
    let (index_key, rgb_key) = KEYS[saber];
    // `atoi`, as `CG_NewClientInfo` reads it: JoF EJK appends the worn hat or
    // cape to the digits (`8santahat`), which leaves the colour untouched.
    let color = legacy_info_value(config, index_key)
        .map_or(4, legacy_atoi)
        .rem_euclid(NUM_SABER_COLORS);
    if color < SABER_RGB {
        return legacy_blade_rgb(color);
    }
    // The key is present but empty for a client that never set a tint;
    // `atoi` reads that as 0, which unpacks to red.
    match legacy_info_value(config, rgb_key) {
        Some(packed) => crate::unpack_saber_rgb(legacy_atoi(packed) as u32),
        None => legacy_blade_rgb(color - SABER_RGB),
    }
}

/// Stock blade tints of `saber_colors_t` 0..=5 (`cg_players.c:6371-6394`).
pub(super) fn legacy_blade_rgb(color: i32) -> [u8; 3] {
    match color {
        0 => [255, 51, 51],
        1 => [255, 128, 26],
        2 => [255, 255, 51],
        3 => [51, 255, 51],
        4 => [51, 102, 255],
        _ => [230, 51, 255],
    }
}

/// Raw value of `wanted` in a legacy info string, if the key is present.
fn legacy_info_value<'a>(config: &'a [u8], wanted: &[u8]) -> Option<&'a [u8]> {
    let config = config.strip_prefix(b"\\").unwrap_or(config);
    let mut fields = config.split(|byte| *byte == b'\\');
    while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
        if key.eq_ignore_ascii_case(wanted) {
            return Some(value);
        }
    }
    None
}

/// C `atoi`: the leading decimal digits (with sign), 0 when there are none.
pub(super) fn legacy_atoi(value: &[u8]) -> i32 {
    let (negative, digits) = match value.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, value),
    };
    let magnitude = digits
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0i32, |total, byte| {
            total.wrapping_mul(10).wrapping_add(i32::from(byte - b'0'))
        });
    if negative {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worn_cosmetic_leaves_the_saber_colour_alone() {
        // JoF EJK's `c1 "4santahat"`: blue wearing a Santa hat.
        let config = br"n\Sol\c1\4santahat\c2\0royalcape";
        assert_eq!(saber_color_from_config(config, 0), legacy_blade_rgb(4));
        assert_eq!(saber_color_from_config(config, 1), legacy_blade_rgb(0));
        // Past purple without a packed tint, the colour rolls over as before.
        let config = br"c1\8santahat";
        assert_eq!(saber_color_from_config(config, 0), legacy_blade_rgb(2));
        assert_eq!(saber_color_from_config(br"n\x", 0), legacy_blade_rgb(4));
    }
}

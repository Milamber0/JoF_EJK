//! Read-only BaseJKA information projections for client presentation.

use sjk_protocol::PlayerState;

use crate::pmove::MovementState;
use crate::weapon_data::legacy_weapon_data;

/// Authoritative values consumed by a legacy-compatible multiplayer HUD.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HudDataSource {
    /// Current signed health value (`STAT_HEALTH`).
    pub health: i32,
    /// Current signed armor value (`STAT_ARMOR`).
    pub armor: i32,
    /// Current force-power reserve.
    pub force: u8,
    /// Current `weapon_t` value.
    pub weapon: u8,
    /// Current weapon's shared ammo-pool count, or no displayable ammo.
    pub ammo: Option<i32>,
    /// Active saber offense style while `weapon == WP_SABER`.
    pub saber_style: Option<u8>,
}

/// Compatibility alias retained for callers written before the widget HUD.
pub type LegacyHudValues = HudDataSource;

/// Project protocol-26 player state into the values codemp draws in its HUD.
///
/// Ammo follows `codemp/game/bg_weapons.c::weaponData[].ammoIndex`; codemp's
/// `CG_DrawHUD` substitutes saber style for saber ammo (the draw level,
/// `fd.saberDrawAnimLevel`, `cg_draw.c:810`/`:1172`), while `CG_DrawAmmo`
/// renders infinite/no-ammo weapons as `--` and suppresses a negative count.
/// This is intentionally a compatibility adapter rather than an engine-wide
/// weapon model.
pub fn legacy_hud_values(player: &PlayerState) -> LegacyHudValues {
    let weapon = player.weapon();
    let ammo = weapon_ammo_index(weapon)
        .and_then(|index| player.ammo.get(index).copied())
        .map(|value| value as i32)
        .filter(|value| *value >= 0);
    HudDataSource {
        health: player.health(),
        armor: player.armor(),
        force: player.force_power(),
        weapon,
        ammo,
        saber_style: (weapon == 3).then(|| player.saber_draw_style()),
    }
}

/// Project protocol state into the read-only data view consumed by HUD widgets.
///
/// This deliberately returns the same compatibility projection as
/// [`legacy_hud_values`], keeping raw `PlayerState` out of the UI framework.
pub fn legacy_hud_data(player: &PlayerState) -> HudDataSource {
    legacy_hud_values(player)
}

/// Project the locally predicted weapon and ammo over authoritative HUD data.
///
/// This is the `cg.predictedPlayerState` source used by `CG_DrawAmmo`; health,
/// armor, force, and saber style remain snapshot values in this bounded port.
pub fn legacy_predicted_hud_data(
    player: &PlayerState,
    predicted: Option<&MovementState>,
) -> HudDataSource {
    let mut values = legacy_hud_data(player);
    let Some(predicted) = predicted else {
        return values;
    };
    values.weapon = predicted.weapon;
    values.ammo = legacy_weapon_data(predicted.weapon)
        .map(|data| predicted.ammo[data.ammo_index])
        .filter(|ammo| *ammo >= 0 && matches!(predicted.weapon, 4..=16));
    values.saber_style = (predicted.weapon == 3).then(|| player.saber_draw_style());
    values
}

/// User-facing names for codemp's `saber_styles_t` values.
pub fn legacy_saber_style_name(style: u8) -> &'static str {
    match style {
        1 => "Fast",
        2 => "Medium",
        3 => "Strong",
        4 => "Desann",
        5 => "Tavion",
        6 => "Dual",
        7 => "Staff",
        _ => "Unknown",
    }
}

// OpenJK codemp/game/bg_weapons.c, weaponData[WP_NUM_WEAPONS].ammoIndex.
fn weapon_ammo_index(weapon: u8) -> Option<usize> {
    const INDICES: [usize; 19] = [0, 0, 0, 0, 2, 2, 3, 3, 4, 3, 4, 5, 7, 8, 9, 4, 2, 0, 0];
    (matches!(weapon, 4..=16))
        .then(|| INDICES.get(usize::from(weapon)).copied())
        .flatten()
}

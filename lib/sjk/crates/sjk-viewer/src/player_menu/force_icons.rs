//! Art of the Force page, read from the player's game data and never
//! bundled: each power's holocron icon (the legacy UI's `HolocronIcons` in
//! `ui_shared.c`), and the two Force Enlightenment pickup icons
//! (`item_force_enlighten_light` and `_dark` in `bg_misc.c`) as the side
//! emblems. Anything absent is simply not drawn; the page reads the same as
//! text alone.

use super::icons::{FORCE_CELLS, IconLoader, IconRequest};
use sjk_client::{FORCE_POWER_COUNT, ForceSide};
use sjk_ui::TextureId;

/// Shader names in `forcePowers_t` order (`HolocronIcons`, also the HUD
/// Force wheel's `forcePowerIcons`).
pub(crate) const POWER_ICONS: [&str; FORCE_POWER_COUNT] = [
    "gfx/mp/f_icon_lt_heal",
    "gfx/mp/f_icon_levitation",
    "gfx/mp/f_icon_speed",
    "gfx/mp/f_icon_push",
    "gfx/mp/f_icon_pull",
    "gfx/mp/f_icon_lt_telepathy",
    "gfx/mp/f_icon_dk_grip",
    "gfx/mp/f_icon_dk_l1",
    "gfx/mp/f_icon_dk_rage",
    "gfx/mp/f_icon_lt_protect",
    "gfx/mp/f_icon_lt_absorb",
    "gfx/mp/f_icon_lt_healother",
    "gfx/mp/f_icon_dk_forceother",
    "gfx/mp/f_icon_dk_drain",
    "gfx/mp/f_icon_sight",
    "gfx/mp/f_icon_saber_attack",
    "gfx/mp/f_icon_saber_defend",
    "gfx/mp/f_icon_saber_throw",
];

/// Light and dark side emblems.
const SIDE_ICONS: [&str; 2] = ["gfx/hud/mpi_jlight", "gfx/hud/mpi_dklight"];

const _: () = assert!(POWER_ICONS.len() + SIDE_ICONS.len() <= FORCE_CELLS);

/// Image extensions tried for an extensionless shader name, as the
/// renderer falls back when no shader script defines it.
const EXTENSIONS: [&str; 3] = ["tga", "png", "jpg"];

/// Atlas cell of power `index`'s icon.
pub(super) fn power_texture(index: usize) -> TextureId {
    IconLoader::force_texture(index)
}

/// Atlas cell of `side`'s emblem.
pub(super) fn side_texture(side: ForceSide) -> TextureId {
    let slot = match side {
        ForceSide::Light => 0,
        ForceSide::Dark => 1,
    };
    IconLoader::force_texture(POWER_ICONS.len() + slot)
}

/// Every power icon and side emblem, with the paths to try for each.
pub(super) fn requests() -> Vec<IconRequest> {
    let sides = [ForceSide::Light, ForceSide::Dark];
    POWER_ICONS
        .iter()
        .enumerate()
        .map(|(index, name)| (power_texture(index), *name))
        .chain(
            sides
                .into_iter()
                .zip(SIDE_ICONS)
                .map(|(side, name)| (side_texture(side), name)),
        )
        .map(|(texture, name)| {
            let paths = EXTENSIONS.map(|extension| format!("{name}.{extension}"));
            (texture, paths.to_vec())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn force_art_takes_distinct_cells_after_the_model_icons() {
        let requests = requests();
        assert_eq!(requests.len(), POWER_ICONS.len() + SIDE_ICONS.len());
        let cells: HashSet<u32> = requests.iter().map(|(texture, _)| texture.0).collect();
        assert_eq!(cells.len(), requests.len());
        let first = crate::ui_renderer::FORCE_ICON_FIRST;
        let end = first + FORCE_CELLS as u32;
        assert!(cells.iter().all(|cell| (first..end).contains(cell)));
        // Past every model icon cell and every HUD cell.
        let model_end = super::super::icons::MODEL_ICONS as u32 + 1;
        assert!(first >= model_end + 96);
        assert!(end <= crate::ui_renderer::ATLAS_CELLS);
        assert_eq!(requests[0].1[0], "gfx/mp/f_icon_lt_heal.tga");
        assert_eq!(side_texture(ForceSide::Dark).0, first + 19);
    }
}

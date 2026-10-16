//! The tiers' pictures in the UI's icon atlas: `gfx/sjk/holocron_<tier>.png`, bundled
//! with the client and mounted below all game data (the list in [`crate::illuminate`]),
//! so a PK3 with the same paths replaces them. They are uploaded once per installed world
//! with the HUD's other pictures (`hud/icons.rs`) into the cells
//! [`crate::ui_renderer::HOLOCRON_ICON_FIRST`] on, one per tier.
//!
//! A tier whose picture is missing has no texture and draws as a gem
//! ([`super::gem`]): nothing shows a blank or a coloured placeholder box.

use super::{COUNT, TIERS};
use sjk_shader::ShaderCatalog;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;
use std::sync::atomic::{AtomicU8, Ordering};

/// Which tiers' pictures were uploaded (bit `index`): the pages the console draws, which
/// do not hold the HUD, ask [`is_loaded`] to know whether a tier draws its picture or its
/// gem.
static LOADED: AtomicU8 = AtomicU8::new(0);

/// Whether the picture of tier `index` is in the atlas (else it draws as a gem).
pub(crate) fn is_loaded(index: usize) -> bool {
    LOADED.load(Ordering::Relaxed) & (1 << index) != 0
}

/// The icon cell of tier `index`.
pub(crate) const fn texture(index: usize) -> TextureId {
    TextureId(crate::ui_renderer::HOLOCRON_ICON_FIRST + index as u32)
}

/// Upload the tiers' pictures once per installed world; absent ones stay `None`.
pub(crate) fn load(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    mut upload: impl FnMut(TextureId, &[u8]),
) -> [Option<TextureId>; COUNT] {
    let loaded: [Option<TextureId>; COUNT] = std::array::from_fn(|index| {
        let pixels = crate::hud::icons::assets::decode(vfs, shaders, &TIERS[index].icon_path())?;
        let id = texture(index);
        upload(id, pixels.as_raw());
        Some(id)
    });
    let mask = loaded
        .iter()
        .enumerate()
        .filter(|(_, id)| id.is_some())
        .fold(0_u8, |mask, (index, _)| mask | 1 << index);
    LOADED.store(mask, Ordering::Relaxed);
    loaded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tier_has_its_own_cell_inside_the_atlas() {
        let cells: Vec<u32> = (0..COUNT).map(|index| texture(index).0).collect();
        for pair in cells.windows(2) {
            assert_eq!(pair[1], pair[0] + 1);
        }
        assert!(cells[COUNT - 1] < crate::ui_renderer::ATLAS_CELLS);
        assert!(cells[0] >= crate::ui_renderer::VEHICLE_CROSSHAIR_ICON);
    }
}

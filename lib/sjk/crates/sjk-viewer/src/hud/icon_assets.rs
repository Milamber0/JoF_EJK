//! Map-lifetime icon registration through the shared shader and decoded-image caches.
use crate::{decoded_image_cache::cached_decoded_image, ui_renderer};
use sjk_shader::ShaderCatalog;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;

/// Existing item catalogue, then 43 cause-of-death cells, then two CTY flag variants,
/// then the weapon selection row's art: each weapon's `_na` (no ammo) icon and the
/// staff and dual saber icons, then the team logos floated over allies.
pub(super) const COUNT: usize = 120;
/// First of the cause-of-death cells, by `meansOfDeath_t`: an icon pack's
/// `hud/mod/*`, else the weapon's own picture ([`MOD_ITEMS`]); the kill feed's icons.
pub(super) const MEANS_FIRST: usize = 51;
/// Cause-of-death cells, `MOD_UNKNOWN` to `MOD_TEAM_CHANGE`.
pub(super) const MEANS_COUNT: usize = 43;
/// First of the 19 `_na` weapon icons, by weapon (`cgs.media.weaponIcons_NA`).
pub(super) const EMPTY_WEAPONS: usize = 96;
/// `cgs.media.weaponIconsStaff` and `weaponIconsAkimbo` (JoF EJK `cg_main.c`).
pub(super) const SABER_STAFF: usize = 115;
pub(super) const SABER_DUAL: usize = 116;
/// First of the team logos' cells, in [`super::team_logo::Logo`]'s order.
pub(super) const TEAM_LOGO_FIRST: usize = 117;

/// Stock weapon ordinals -> existing bg_itemlist entries (bg_misc.c:1241-1619).
pub(crate) const WEAPONS: [usize; 19] = [
    0, 19, 20, 21, 22, 25, 26, 27, 28, 29, 30, 31, 35, 36, 37, 23, 24, 38, 39,
];
/// Stock timed powerup slots -> BG_FindItemForPowerup (bg_misc.c:1999-2011).
pub(crate) const POWERS: [usize; 16] = [0, 0, 0, 0, 46, 47, 48, 0, 0, 0, 0, 0, 15, 16, 17, 18];

const MOD_NAMES: [&str; 43] = [
    "generic",
    "stun",
    "melee",
    "saber",
    "pistol",
    "pistol",
    "rifle",
    "rifle_alt",
    "disruptor",
    "disruptor",
    "disruptor_alt",
    "bowcaster",
    "repeater",
    "repeater_alt",
    "repeater_alt",
    "demp2",
    "demp2_alt",
    "flechette",
    "flechette_alt",
    "merrsonn",
    "merrsonn_splash",
    "merrsonn_alt",
    "merrsonn_alt",
    "thermal",
    "thermal_alt",
    "mine",
    "mine_alt",
    "detpack",
    "swoop",
    "concussion",
    "concussion_alt",
    "force",
    "portable_turret",
    "water",
    "ooze",
    "lava",
    "crushed",
    "telefrag",
    "fall",
    "generic",
    "generic",
    "generic",
    "generic",
];
/// Each cause of death's fallback, the bg_itemlist entry of the weapon (or item)
/// that deals it: the actual weapon icon, never synthetic substitute art.
pub(crate) const MOD_ITEMS: [usize; 43] = [
    0, 19, 20, 21, 22, 22, 25, 25, 26, 26, 26, 27, 28, 28, 28, 29, 29, 30, 30, 31, 31, 31, 31, 35,
    35, 36, 36, 37, 0, 23, 23, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Resolve the shader's selected image with the world renderer's exact decoder/cache.
pub(crate) fn decode(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    name: &str,
) -> Option<image::RgbaImage> {
    let path = shaders.resolve_image(vfs, name).ok()??;
    let image = cached_decoded_image(vfs, path.as_str()).ok()??;
    Some(image::imageops::resize(
        &*image,
        ui_renderer::ICON_SIZE,
        ui_renderer::ICON_SIZE,
        image::imageops::FilterType::Triangle,
    ))
}

/// Load once per installed world; unresolved slots stay absent, never coloured quads.
pub(super) fn load(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    mut upload: impl FnMut(TextureId, &[u8]),
) -> [Option<TextureId>; COUNT] {
    let items = &crate::pickups::simple::ICONS;
    // Collected and reported as one line: several causes of death legitimately
    // have no art (falling, crushing, lava), and one line per slot buried the
    // rest of the world-load log under 48 warnings.
    let mut absent: Vec<String> = Vec::new();
    let table = std::array::from_fn(|slot| {
        let name = match slot {
            0..=50 => items[slot].to_owned(),
            MEANS_FIRST..=93 => format!("hud/mod/{}", MOD_NAMES[slot - MEANS_FIRST]),
            94 => "gfx/hud/mpi_rflag_ys".to_owned(),
            95 => "gfx/hud/mpi_bflag_ys".to_owned(),
            EMPTY_WEAPONS..SABER_STAFF => {
                let icon = items[WEAPONS[slot - EMPTY_WEAPONS]];
                if icon.is_empty() {
                    String::new()
                } else {
                    format!("{icon}_na")
                }
            }
            SABER_STAFF => "gfx/hud/w_icon_saberstaff".to_owned(),
            SABER_DUAL => "gfx/hud/w_icon_duallightsaber".to_owned(),
            _ => super::team_logo::Logo::ALL[slot - TEAM_LOGO_FIRST]
                .shader()
                .to_owned(),
        };
        if name.is_empty() {
            return None;
        }
        let pixels = decode(vfs, shaders, &name).or_else(|| {
            let item = *MOD_ITEMS.get(slot.wrapping_sub(MEANS_FIRST))?;
            (item != 0)
                .then(|| decode(vfs, shaders, items[item]))
                .flatten()
        });
        let Some(pixels) = pixels else {
            if !absent.contains(&name) {
                absent.push(name);
            }
            return None;
        };
        let id = TextureId(ui_renderer::ICON_CELLS + slot as u32);
        upload(id, pixels.as_raw());
        Some(id)
    });
    if !absent.is_empty() {
        crate::log::progress(format_args!(
            "HUD icons absent, drawn as nothing: {}",
            absent.join(", ")
        ));
    }
    table
}

/// TaystJK hud_shared.c:31-75; the method-of-death colour, not name colour.
pub(crate) fn tint(method: usize) -> [f32; 3] {
    match method {
        1 | 4 | 5 => [0.0, 1.0, 1.0],
        2 => [1.0, 1.0, 0.0],
        3 => [0.5, 0.0, 1.0],
        6 | 7 | 11 => [0.0, 1.0, 0.5],
        8..=10 | 28 | 34 | 40 => [0.0, 1.0, 0.0],
        12..=14 | 33 => [0.0, 0.0, 1.0],
        15 | 16 => [0.5, 1.0, 0.0],
        17 | 18 => [1.0, 0.5, 0.0],
        19..=22 | 35 => [1.0, 0.0, 0.0],
        23 | 24 | 27 | 31 => [1.0, 0.4, 0.0],
        25 | 26 => [1.0, 1.0, 0.4],
        29 | 30 => [1.0, 0.0, 1.0],
        36 => [0.0; 3],
        37 => [0.4, 0.2, 0.3],
        38 | 41 => [1.0, 1.0, 0.6],
        39 => [0.5, 0.2, 0.7],
        _ => [1.0; 3],
    }
}

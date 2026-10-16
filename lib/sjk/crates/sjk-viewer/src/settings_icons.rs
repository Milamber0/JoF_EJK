//! The settings menu's icons: grey metal discs in the quick wheels' style, generated
//! by Sol as one board and cut by `scripts/settings_icons.py`; uploaded into the UI
//! atlas at start (`ui_renderer::settings_icon`). The classic Settings lists show
//! one beside each group ([`crate::menu::classic::layout::Entry::icon`]); the rest
//! are spares for the screens still to use them.

/// Every icon, in board order.
pub(crate) const ICONS: [(&str, &[u8]); 32] = [
    (
        "settings",
        include_bytes!("../assets/settings/settings.png"),
    ),
    (
        "key_bindings",
        include_bytes!("../assets/settings/key_bindings.png"),
    ),
    ("options", include_bytes!("../assets/settings/options.png")),
    (
        "first_setup",
        include_bytes!("../assets/settings/first_setup.png"),
    ),
    (
        "graphics",
        include_bytes!("../assets/settings/graphics.png"),
    ),
    ("sound", include_bytes!("../assets/settings/sound.png")),
    (
        "gameplay",
        include_bytes!("../assets/settings/gameplay.png"),
    ),
    ("network", include_bytes!("../assets/settings/network.png")),
    (
        "movement",
        include_bytes!("../assets/settings/movement.png"),
    ),
    (
        "interaction",
        include_bytes!("../assets/settings/interaction.png"),
    ),
    ("weapons", include_bytes!("../assets/settings/weapons.png")),
    (
        "force_powers",
        include_bytes!("../assets/settings/force_powers.png"),
    ),
    (
        "mouse_joystick",
        include_bytes!("../assets/settings/mouse_joystick.png"),
    ),
    (
        "other_controls",
        include_bytes!("../assets/settings/other_controls.png"),
    ),
    (
        "game_options",
        include_bytes!("../assets/settings/game_options.png"),
    ),
    (
        "interface",
        include_bytes!("../assets/settings/interface.png"),
    ),
    ("hud", include_bytes!("../assets/settings/hud.png")),
    (
        "scoreboard",
        include_bytes!("../assets/settings/scoreboard.png"),
    ),
    ("video", include_bytes!("../assets/settings/video.png")),
    ("image", include_bytes!("../assets/settings/image.png")),
    (
        "lighting",
        include_bytes!("../assets/settings/lighting.png"),
    ),
    ("shadows", include_bytes!("../assets/settings/shadows.png")),
    ("weather", include_bytes!("../assets/settings/weather.png")),
    ("text", include_bytes!("../assets/settings/text.png")),
    ("camera", include_bytes!("../assets/settings/camera.png")),
    (
        "crosshair",
        include_bytes!("../assets/settings/crosshair.png"),
    ),
    (
        "nameplates",
        include_bytes!("../assets/settings/nameplates.png"),
    ),
    (
        "identity",
        include_bytes!("../assets/settings/identity.png"),
    ),
    (
        "changelog",
        include_bytes!("../assets/settings/changelog.png"),
    ),
    ("credits", include_bytes!("../assets/settings/credits.png")),
    ("update", include_bytes!("../assets/settings/update.png")),
    ("import", include_bytes!("../assets/settings/import.png")),
];

/// The atlas texture of icon `name`, if there is one by that name: a settings
/// icon, else a quick wheel icon (Settings' Quick wheel category wears the
/// wheel's own, [`crate::quick_wheel::catalog::WHEEL_ICON`]).
pub(crate) fn texture(name: &str) -> Option<sjk_ui::TextureId> {
    ICONS
        .iter()
        .position(|(icon, _)| *icon == name)
        .map(crate::ui_renderer::settings_icon)
        .or_else(|| {
            crate::quick_wheel::catalog::icon_index(name).map(crate::ui_renderer::wheel_icon)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_one_atlas_cell_and_has_a_cell() {
        assert!(ICONS.len() <= crate::ui_renderer::SETTINGS_ICON_CELLS);
        for (name, bytes) in ICONS {
            let icon = image::load_from_memory(bytes).expect(name);
            let size = crate::ui_renderer::ICON_SIZE;
            assert_eq!((icon.width(), icon.height()), (size, size), "{name}");
        }
    }
}

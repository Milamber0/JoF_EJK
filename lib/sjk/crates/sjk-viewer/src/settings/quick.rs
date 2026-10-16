//! The FIRST SETUP tab of Settings: the few settings worth choosing once on a first
//! start (first, bringing the setup from another client and the gameplay FPS it
//! moved at; then the renderer and its quality, the menu and
//! camera styles, display, aim, sound, the HUD, nameplates, the Force shells and
//! the network opt-ins). The rows are the catalogue's own, looked up by cvar, so a
//! change here is the same change the other tabs make; the import row and the
//! last row, whether the screen opens at start, are the tab's own.

use super::catalog::*;
use std::sync::OnceLock;

/// Set to 1 by the "Don't show at start" row: the screen no longer opens at start.
pub(crate) const HIDE_CVAR: &str = "ui_hideFirstSetup";

/// The tab's own last row.
const HIDE_ROW: Setting = Setting {
    label: "Don't show at start",
    cvar: HIDE_CVAR,
    kind: ValueKind::Bool,
};

/// The tab's own first row: the setup from another client's `.cfg`, from the
/// configs found on this computer or a file.
const IMPORT_ROW: Setting = Setting {
    label: "Import from a client",
    cvar: super::catalog::IMPORT_ROW,
    kind: ValueKind::ImportPage,
};

/// The tab's cvars, in order, after the import row.
const CVARS: &[&str] = &[
    crate::gameplay_fps::CVAR,
    crate::graphics_quality::RENDERER_ROW,
    crate::graphics_quality::ROW_NAME,
    crate::menu::style::CVAR,
    crate::camera::STYLE_CVAR,
    "r_resolution",
    "r_fullscreen",
    "r_vsync",
    "cg_fov",
    "sensitivity",
    "m_invert",
    "cl_run",
    "s_volume",
    "s_musicVolume",
    crate::menu_hud::STYLE_CVAR,
    "cg_hudScale",
    "cg_crosshair",
    "cg_nameplate",
    "cg_nameplateRange",
    "cg_nameplateBars",
    "cg_nameplateForce",
    "cg_nameplateIcons",
    "cg_nameplateShare",
    "cg_auraShell",
    "cg_spProtAbsColor",
    "cl_autoUpdate",
    "cl_identity",
];

/// The tab's settings, looked up in the other tabs once.
pub(super) fn rows() -> &'static [Setting] {
    static ROWS: OnceLock<Vec<Setting>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let found = CVARS.iter().filter_map(|cvar| {
            // Not the QUICK tab itself, which is what is being built.
            (0..QUICK_TAB)
                .flat_map(super::settings)
                .find(|setting| setting.cvar.eq_ignore_ascii_case(cvar))
                .copied()
        });
        [IMPORT_ROW]
            .into_iter()
            .chain(found)
            .chain([HIDE_ROW])
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_cvar_is_a_setting() {
        assert_eq!(rows().len(), CVARS.len() + 2);
        assert_eq!(rows().last().map(|row| row.cvar), Some(HIDE_CVAR));
    }

    #[test]
    fn the_import_and_gameplay_fps_come_first_then_graphics_and_the_styles() {
        let cvars: Vec<_> = rows().iter().take(6).map(|row| row.cvar).collect();
        assert_eq!(
            cvars,
            [
                super::super::catalog::IMPORT_ROW,
                crate::gameplay_fps::CVAR,
                crate::graphics_quality::RENDERER_ROW,
                crate::graphics_quality::ROW_NAME,
                crate::menu::style::CVAR,
                crate::camera::STYLE_CVAR
            ]
        );
        assert_eq!(
            super::super::Group::Quick.lines()[..9],
            [
                super::super::Line::Heading("From another client"),
                super::super::Line::Row(0),
                super::super::Line::Row(1),
                super::super::Line::Heading("Graphics"),
                super::super::Line::Row(2),
                super::super::Line::Row(3),
                super::super::Line::Heading("Styles"),
                super::super::Line::Row(4),
                super::super::Line::Row(5),
            ]
        );
    }

    #[test]
    fn the_tab_is_last_so_the_other_tabs_keep_their_numbers() {
        assert_eq!(TABS[QUICK_TAB], FIRST_SETUP_CAPTION);
        assert_eq!(QUICK_TAB, TABS.len() - 1);
    }
}

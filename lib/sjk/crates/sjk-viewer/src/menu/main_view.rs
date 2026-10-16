//! The main menu's first page in either style, and the version line the
//! menus and console show.

use super::ClientMenu;
use crate::{TextVertex, UiFont};

/// Build version ([`crate::build_info::VERSION`]), bottom right of the main
/// menus; the console shows it too.
pub(crate) const VERSION_LINE: &str = concat!("SJK ", env!("SJK_BUILD_VERSION"));

/// The version line the menus show: the version, and a newer release the update
/// check found.
pub(crate) fn version_line() -> std::borrow::Cow<'static, str> {
    match crate::update::available_version() {
        Some(version) => format!("{VERSION_LINE}   /   update {version} available").into(),
        None => VERSION_LINE.into(),
    }
}

impl ClientMenu {
    pub(super) fn append_main(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        match self.menu_style {
            super::MenuStyle::Classic => {
                let reveal = self.screen_reveal();
                super::classic::view::build(
                    &mut self.ui,
                    viewport,
                    &self.classic,
                    reveal,
                    self.art,
                );
                self.ui.append_text(vertices, font, viewport);
            }
            // Drawn with the player's details by `append_sjk_home`; here, from
            // a caller without the console, in Inter.
            super::MenuStyle::Sjk => {
                let target = super::sjk::TextTarget::Inter(vertices, font);
                self.append_sjk_home(target, None, viewport);
            }
        }
    }
}

//! The character page's model grid: one icon tile per listed catalogue
//! entry (the team's characters first, then the species; see
//! `team_filter`), scrolled by rows when more tiles are listed than fit. The
//! classic pages and the SJK UI's Character draw it; this holds its tokens,
//! scroll and tile names.

use super::*;
use sjk_ui::{Color, FontWeight, Rect, TextAlign};

/// First tile token; the `i`th tile on screen answers to `TILE_BASE + i`
/// (counted from the first visible one, so a long list never runs into
/// other tokens).
pub(super) const TILE_BASE: u16 = 100;
/// Tile tokens there are room for, from [`TILE_BASE`].
pub(super) const MAX_VISIBLE_TILES: u16 = 200;
/// Token of the wheel target covering the grid.
pub(super) const GRID_SCROLL_TOKEN: u16 = 902;
/// Tiles per grid row.
pub(super) const COLUMNS: usize = 8;

impl PlayerMenu {
    /// Scroll the grid by `rows` (negative = up) within its bounds.
    pub(super) fn scroll_grid(&mut self, rows: i32) {
        if self.is_sjk() {
            self.scroll_creator(rows);
            return;
        }
        let target = (self.grid_scroll as i32 + rows).max(0) as usize;
        self.grid_scroll = target.min(self.grid_max_scroll);
    }

    /// Tiles per row of the grid on show.
    fn grid_columns(&self) -> usize {
        if self.classic_style {
            super::classic::GRID_COLUMNS
        } else {
            COLUMNS
        }
    }

    /// Slot in [`Self::tiles`] of the `local`th tile on screen.
    pub(super) fn visible_slot(&self, local: usize) -> usize {
        self.grid_scroll * self.grid_columns() + local
    }

    /// The model's name in two lines (model, then skin) across a tile whose
    /// icon cannot be shown.
    pub(super) fn tile_name(
        &mut self,
        absolute: usize,
        rect: Rect,
        size: f32,
        color: Color,
        tracking: f32,
    ) {
        let Some(name) = catalog_of(&self.loader).and_then(|catalog| entry_name(catalog, absolute))
        else {
            return;
        };
        let (model, skin) = name.split_once('/').unwrap_or((name, ""));
        let line = size * 1.25;
        let top = rect.y + (rect.height - line * 2.0) * 0.5;
        for (index, text) in [model, skin].into_iter().enumerate() {
            self.canvas.text_aligned(
                text,
                Rect::new(rect.x, top + index as f32 * line, rect.width, line),
                size,
                color,
                FontWeight::Regular,
                tracking,
                TextAlign::Center,
            );
        }
    }
}

/// What the grid calls catalogue entry `absolute`: a character's `model/skin`
/// or a species' model.
pub(super) fn entry_name(
    catalog: &sjk_client::LegacyAssetCatalog,
    absolute: usize,
) -> Option<&str> {
    match catalog.characters.get(absolute) {
        Some(character) => Some(&character.cvar_value),
        None => catalog
            .species
            .get(absolute - catalog.characters.len())
            .map(|species| species.model.as_str()),
    }
}

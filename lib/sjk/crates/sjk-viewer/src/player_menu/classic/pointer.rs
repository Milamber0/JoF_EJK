//! Pointer routing on the classic profile pages: hovering an entry or one of
//! its cells focuses it, a click activates the entry or picks the cell, and
//! the wheel scrolls the list under it. On the Force page a click on a star
//! sets the power to that level (on its own top star, one below), and the
//! right button lowers a power a level, as retail's did. A click or a drag on
//! a colour slider's bar sets it from the pointer, and the wheel steps it.

use super::cosmetics_page::{self, list_of};
use super::force_page::star_of;
use super::saber_rgb;
use super::view::{
    BLADE_BASE, CAPES_SCROLL, HATS_SCROLL, HILT_BASE, HILTS_SCROLL, PART_BASE, PARTS_SCROLL,
    TEMPLATE_BASE, TEMPLATES_SCROLL, TINT_BASE, TINTS_SCROLL,
};
use super::{BLADE_SWATCHES, Item};
use crate::console::ViewerConsole;
use crate::player_menu::grid::{GRID_SCROLL_TOKEN, MAX_VISIBLE_TILES, TILE_BASE};
use crate::player_menu::saber::{SaberStyle, allowed};
use crate::player_menu::{PlayerMenu, PlayerMenuResult, catalog_of};
use sjk_client::CosmeticSlot;
use sjk_ui::{InputEvent, PointerButton, UiEventKind};

/// What a pointer token names on a classic page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Entry(usize),
    Tile(usize),
    Part(usize),
    Tint(usize),
    Hilt(bool, usize),
    Blade(bool, usize),
    /// A Force power's level star.
    Star(usize, u8),
    /// A row of the hat or cape list.
    Cosmetic(CosmeticSlot, usize),
    /// A row of the Force template list.
    Template(usize),
    /// A custom colour slider's bar.
    Channel(u8),
    Scroll(u16),
}

fn target(token: u16) -> Option<Target> {
    if let Some((slot, row)) = cosmetics_page::row_of(token) {
        return Some(Target::Cosmetic(slot, row));
    }
    if let Some((power, level)) = star_of(token) {
        return Some(Target::Star(power, level));
    }
    if let Some(index) = saber_rgb::of_token(token) {
        return Some(Target::Channel(index));
    }
    if let Some(row) = token
        .checked_sub(TEMPLATE_BASE)
        .map(usize::from)
        .filter(|row| *row < super::force_page::MAX_TEMPLATE_ROWS)
    {
        return Some(Target::Template(row));
    }
    Some(match token {
        GRID_SCROLL_TOKEN | PARTS_SCROLL | TINTS_SCROLL | HATS_SCROLL | CAPES_SCROLL
        | TEMPLATES_SCROLL => Target::Scroll(token),
        token if HILTS_SCROLL.contains(&token) => Target::Scroll(token),
        token if token >= BLADE_BASE[1] && token < BLADE_BASE[1] + 6 => {
            Target::Blade(true, usize::from(token - BLADE_BASE[1]))
        }
        token if token >= BLADE_BASE[0] && token < BLADE_BASE[0] + 6 => {
            Target::Blade(false, usize::from(token - BLADE_BASE[0]))
        }
        token if token >= HILT_BASE[1] && token < HILT_BASE[1] + 100 => {
            Target::Hilt(true, usize::from(token - HILT_BASE[1]))
        }
        token if token >= HILT_BASE[0] && token < HILT_BASE[0] + 100 => {
            Target::Hilt(false, usize::from(token - HILT_BASE[0]))
        }
        token if token >= TINT_BASE && token < TINT_BASE + 40 => {
            Target::Tint(usize::from(token - TINT_BASE))
        }
        token if token >= PART_BASE && token < PART_BASE + 60 => {
            Target::Part(usize::from(token - PART_BASE))
        }
        token if token >= TILE_BASE && token < TILE_BASE + MAX_VISIBLE_TILES => {
            Target::Tile(usize::from(token - TILE_BASE))
        }
        token if token < 64 => Target::Entry(usize::from(token)),
        _ => return None,
    })
}

impl PlayerMenu {
    pub(in crate::player_menu) fn classic_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        let secondary = matches!(
            event,
            InputEvent::PointerRelease {
                button: PointerButton::Secondary,
                ..
            }
        );
        let Some(event) = self.canvas.pointer(event) else {
            return PlayerMenuResult::None;
        };
        let Some(target) = event.token.and_then(target) else {
            return PlayerMenuResult::None;
        };
        if event.kind == UiEventKind::Wheel {
            let rows = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            self.classic_scroll(target, rows, console);
            return PlayerMenuResult::None;
        }
        // The item a cell belongs to takes focus while it is hovered.
        let owner = match target {
            Target::Entry(index) => Some(index),
            Target::Tile(_) => self.focus_of(Item::Models),
            Target::Part(_) => self.focus_of(Item::Parts),
            Target::Tint(_) => self.focus_of(Item::Tints),
            Target::Hilt(second, _) => {
                self.focus_of(if second { Item::Hilts2 } else { Item::Hilts })
            }
            Target::Blade(second, _) => {
                self.focus_of(if second { Item::Blades2 } else { Item::Blades })
            }
            Target::Star(power, _) => self.focus_of(Item::Power(power as u8)),
            Target::Cosmetic(slot, _) => self.focus_of(list_of(slot).0),
            Target::Template(_) => self.focus_of(Item::Templates),
            Target::Channel(index) => self.focus_of(Item::Channel(index)),
            Target::Scroll(_) => None,
        };
        if matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover) {
            let typing = self.name_editing || self.force_templates.editing || self.search_editing;
            if let Some(index) = owner.filter(|_| !typing) {
                self.classic.focus = index;
            }
            if let Target::Cosmetic(slot, row) = target {
                self.cosmetics.cursor[slot.index()] = row;
            }
            return PlayerMenuResult::None;
        }
        // The right button (a click that is not an activation) lowers a power.
        if secondary && event.kind == UiEventKind::Click {
            let power = match target {
                Target::Star(power, _) => Some(power),
                Target::Entry(index) => self
                    .classic_items()
                    .get(index)
                    .and_then(|item| item.power()),
                _ => None,
            };
            if let Some(power) = power {
                if let Some(index) = owner {
                    self.classic.focus = index;
                }
                self.force.step(power, false);
            }
            return PlayerMenuResult::None;
        }
        // A click or a drag along a colour slider sets it from the pointer.
        if let (Target::Channel(index), UiEventKind::Activate | UiEventKind::Drag) =
            (target, event.kind)
        {
            let token = saber_rgb::RGB_BASE + u16::from(index);
            if let (Some(position), Some(bar)) = (event.position, self.canvas.rect_for(token)) {
                if let Some(focus) = owner {
                    self.classic.focus = focus;
                }
                let (second, channel) = saber_rgb::channel(index);
                self.saber
                    .set_channel(second, channel, saber_rgb::value_at(bar, position.x));
                self.saber.apply(console);
            }
            return PlayerMenuResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return PlayerMenuResult::None;
        }
        if self.name_editing {
            // A click elsewhere commits the name as Enter would.
            self.name_editing = false;
            self.apply(console);
        }
        // A click elsewhere ends typing a template name or the search,
        // keeping it.
        self.force_templates.editing = false;
        self.search_editing = false;
        if let Some(index) = owner {
            self.classic.focus = index;
        }
        match target {
            Target::Entry(_) => self.classic_activate(console),
            Target::Tile(local) => {
                let slot = self.visible_slot(local);
                self.pick_tile(console, slot);
                PlayerMenuResult::None
            }
            Target::Part(index) => {
                self.set_variant(self.classic.part_axis, index);
                self.apply(console);
                PlayerMenuResult::None
            }
            Target::Tint(index) => {
                self.set_variant(3, index);
                self.apply(console);
                PlayerMenuResult::None
            }
            Target::Hilt(second, index) => {
                if let Some(catalog) = catalog_of(&self.loader) {
                    self.saber.select(catalog, index, second);
                    self.saber.apply(console);
                }
                PlayerMenuResult::None
            }
            Target::Blade(second, k) => {
                if let Some(index) = BLADE_SWATCHES.get(k) {
                    self.saber.select_color(second, *index);
                    self.saber.apply(console);
                }
                PlayerMenuResult::None
            }
            Target::Star(power, level) => {
                let current = self.force.allocation().levels[power];
                let wanted = if level == current { level - 1 } else { level };
                self.force.set_level(power, wanted);
                PlayerMenuResult::None
            }
            Target::Cosmetic(slot, row) => {
                self.cosmetics.toggle(console, slot, row);
                PlayerMenuResult::None
            }
            Target::Template(row) => {
                self.load_template_row(row);
                PlayerMenuResult::None
            }
            Target::Channel(_) | Target::Scroll(_) => PlayerMenuResult::None,
        }
    }

    fn focus_of(&self, item: Item) -> Option<usize> {
        self.classic_items().iter().position(|entry| *entry == item)
    }

    /// The wheel over a list: the head grid scrolls by rows; the part,
    /// tint and hilt lists step their choice (they follow it), written at
    /// once like the arrow keys.
    fn classic_scroll(&mut self, target: Target, rows: i32, console: &mut ViewerConsole) {
        if rows == 0 {
            return;
        }
        match target {
            Target::Scroll(GRID_SCROLL_TOKEN) | Target::Tile(_) => self.scroll_grid(rows),
            Target::Scroll(PARTS_SCROLL) | Target::Part(_) => {
                self.cycle_variant(self.classic.part_axis, rows as isize);
                self.apply(console);
            }
            Target::Scroll(TINTS_SCROLL) | Target::Tint(_) => {
                self.cycle_variant(3, rows as isize);
                self.apply(console);
            }
            Target::Scroll(token) if HILTS_SCROLL.contains(&token) => {
                self.scroll_hilts(token == HILTS_SCROLL[1], rows, console);
            }
            Target::Hilt(second, _) => self.scroll_hilts(second, rows, console),
            // The cosmetic lists scroll their view; wearing takes a click.
            Target::Scroll(HATS_SCROLL) => self.scroll_cosmetics(CosmeticSlot::Hat, rows),
            Target::Scroll(CAPES_SCROLL) => self.scroll_cosmetics(CosmeticSlot::Cape, rows),
            Target::Cosmetic(slot, _) => self.scroll_cosmetics(slot, rows),
            // The template list scrolls its view; loading takes a click.
            Target::Scroll(TEMPLATES_SCROLL) | Target::Template(_) => {
                let state = &mut self.force_templates;
                state.scroll = state.scroll.saturating_add_signed(rows as isize);
            }
            // Over a colour slider the wheel steps it, as the arrows do.
            Target::Channel(index) => {
                let catalog = catalog_of(&self.loader);
                self.saber
                    .adjust(saber_rgb::row(index), rows as isize, catalog);
                self.saber.apply(console);
            }
            _ => {}
        }
    }

    /// Wheel over a cosmetic list moves its cursor, which the view keeps
    /// in sight.
    fn scroll_cosmetics(&mut self, slot: CosmeticSlot, rows: i32) {
        self.cosmetics.move_cursor(slot, rows as isize);
    }

    /// Wheel over a hilt list steps the chosen hilt, as the list follows
    /// the choice.
    fn scroll_hilts(&mut self, second: bool, rows: i32, console: &mut ViewerConsole) {
        let style = if second {
            SaberStyle::Single
        } else {
            self.saber.style()
        };
        let Some(catalog) = catalog_of(&self.loader) else {
            return;
        };
        let count = catalog
            .saber_hilts
            .iter()
            .filter(|hilt| allowed(hilt, style))
            .count();
        if count == 0 {
            return;
        }
        let current = catalog
            .saber_hilts
            .iter()
            .filter(|hilt| allowed(hilt, style))
            .position(|hilt| hilt.name.eq_ignore_ascii_case(self.saber.hilt(second)))
            .unwrap_or(0);
        let next = (current as i32 + rows).clamp(0, count as i32 - 1) as usize;
        self.saber.select(catalog, next, second);
        self.saber.apply(console);
    }
}

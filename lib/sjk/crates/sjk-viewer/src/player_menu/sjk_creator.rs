//! The SJK UI Character page's creator (Sol's request, 12/10/2026: a better character
//! creator and hat and cape picker than arrows with text; `docs/sjk-ui.md`, Character).
//!
//! One strip of categories, Model, then a species' Head, Torso, Legs and Skin, and Hat
//! and Cape, over one grid of tiles for the category on show: the models' icons, a
//! species' part pictures, the skin colours as swatches, the hats and capes as tiles
//! of their own. The models list the retail game's own under a "Jedi Academy" heading
//! and the community's after them under "Community" (the rule in `AGENTS.md`); the
//! search filters whatever is on show. A model, part or colour chosen shows on the
//! stage and waits for Apply ([`PlayerMenu::apply_look`]); a hat or cape is worn at
//! once, as before.

use super::grid::{COLUMNS, GRID_SCROLL_TOKEN, MAX_VISIBLE_TILES, TILE_BASE};
use super::part_icons::Part;
use super::team_filter::{TeamSkin, search_matches};
use super::*;
use crate::menu::sjk::{Frame, color, kit, text};
use crate::menu_widgets::TextFamily;
use sjk_client::CosmeticSlot;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign, UiEventKind};

/// The strip's segments answer to `CATEGORY_BASE` and the six after it.
pub(super) const CATEGORY_BASE: u16 = 2_410;
/// The team's segments (Default, Red, Blue) answer to `TEAM_BASE` and the two after it.
pub(super) const TEAM_BASE: u16 = 2_420;
/// The line over the tiles (the title, the team), a section's heading and the gap
/// between tiles.
pub(super) const TITLE: f32 = 40.0;
const HEADING: f32 = 32.0;
const GAP: f32 = 8.0;
/// A hat's or cape's tile: four to a line, this high.
const PIECE_COLUMNS: usize = 4;
const PIECE_HEIGHT: f32 = 84.0;

/// What the grid lists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Category {
    #[default]
    Model,
    Head,
    Torso,
    Legs,
    Skin,
    Hat,
    Cape,
}

impl Category {
    pub(super) const ALL: [Self; 7] = [
        Self::Model,
        Self::Head,
        Self::Torso,
        Self::Legs,
        Self::Skin,
        Self::Hat,
        Self::Cape,
    ];
    /// Every model, which has no parts to pick.
    const PLAIN: [Self; 3] = [Self::Model, Self::Hat, Self::Cape];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Head => "Head",
            Self::Torso => "Torso",
            Self::Legs => "Legs",
            Self::Skin => "Skin",
            Self::Hat => "Hat",
            Self::Cape => "Cape",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Model => "Models",
            Self::Head => "Heads",
            Self::Torso => "Torsos",
            Self::Legs => "Legs",
            Self::Skin => "Skin colours",
            Self::Hat => "Hats",
            Self::Cape => "Capes",
        }
    }

    /// The slot a hat or cape is worn in.
    pub(super) fn slot(self) -> Option<CosmeticSlot> {
        match self {
            Self::Hat => Some(CosmeticSlot::Hat),
            Self::Cape => Some(CosmeticSlot::Cape),
            _ => None,
        }
    }

    pub(super) fn part(self) -> Option<Part> {
        match self {
            Self::Head => Some(Part::Head),
            Self::Torso => Some(Part::Torso),
            Self::Legs => Some(Part::Legs),
            _ => None,
        }
    }

    /// The species axis a part or colour is chosen on (0 heads, 1 torsos, 2 legs, 3
    /// skin colours).
    fn axis(self) -> Option<usize> {
        match self {
            Self::Head => Some(0),
            Self::Torso => Some(1),
            Self::Legs => Some(2),
            Self::Skin => Some(3),
            _ => None,
        }
    }

    /// Whether only a species has these (a model has no parts).
    fn needs_species(self) -> bool {
        self.axis().is_some()
    }
}

/// One line of the grid: a section's heading or a row of tiles (`count` of them from
/// item `first`).
#[derive(Clone, Copy, Debug)]
enum Line {
    Heading(&'static str, usize),
    Tiles { first: usize, count: usize },
}

/// The creator's state: the category on show, what it lists and where the grid is
/// scrolled.
#[derive(Debug, Default)]
pub(super) struct Creator {
    category: Category,
    /// What the category lists, after the search: catalogue entries for models, part
    /// or colour indices for the rest, `0` None then piece `i` as `i + 1` for hats and
    /// capes.
    items: Vec<usize>,
    /// How many of `items`, from the first, are the retail game's (models only).
    retail: usize,
    /// How many after those are high quality (rig) models.
    rig: usize,
    lines: Vec<Line>,
    first_line: usize,
    max_first: usize,
    /// The item the pointer or the keys are on.
    cursor: usize,
    /// Scroll to the cursor on the next frame.
    follow: bool,
    /// Put the cursor on what is chosen on the next frame (after opening, a category
    /// or a model change).
    to_chosen: bool,
    /// Item of the first tile drawn last frame: tile `i` on screen is this plus `i`.
    first_shown: usize,
    /// Tiles per line of the last frame.
    columns: usize,
}

impl Creator {
    /// As the screen opens: Model, scrolled to what is chosen.
    pub(super) fn reset(&mut self) {
        self.category = Category::Model;
        self.first_line = 0;
        self.cursor = 0;
        self.follow = true;
        self.to_chosen = true;
    }

    /// Choose what to show again, as the catalogue or the model changed.
    pub(super) fn show_chosen(&mut self) {
        self.to_chosen = true;
    }

    /// The first line on show, back at the top (a new search).
    pub(super) fn back_to_top(&mut self) {
        self.first_line = 0;
        self.cursor = 0;
        self.follow = true;
    }

    /// Scroll by `lines` (negative up) within what the last frame allowed.
    pub(super) fn scroll(&mut self, lines: i32) {
        let target = (self.first_line as i32 + lines).max(0) as usize;
        self.first_line = target.min(self.max_first);
    }
}

impl PlayerMenu {
    /// The category on show: a species' parts only while a species is the model.
    pub(super) fn category(&self) -> Category {
        let category = self.creator.category;
        if category.needs_species() && !matches!(self.choice, Some(Choice::Species(_))) {
            Category::Model
        } else {
            category
        }
    }

    /// The categories the strip offers.
    pub(super) fn categories(&self) -> &'static [Category] {
        if matches!(self.choice, Some(Choice::Species(_))) {
            &Category::ALL
        } else {
            &Category::PLAIN
        }
    }

    /// Show `category`, scrolled to what is chosen in it, the model turned to the
    /// side the piece is worn on.
    pub(super) fn set_category(&mut self, category: Category) {
        self.creator.category = category;
        self.creator.first_line = 0;
        self.creator.to_chosen = true;
        self.turn.face(if category == Category::Cape {
            180.0
        } else {
            0.0
        });
    }

    /// Step the strip by `direction`.
    pub(super) fn step_category(&mut self, direction: isize) {
        let categories = self.categories();
        let at = categories
            .iter()
            .position(|category| *category == self.category())
            .unwrap_or(0);
        self.set_category(categories[super::controller::wrap(at, direction, categories.len())]);
    }

    /// The row the grid is, for the selection.
    pub(super) fn grid_row(&self) -> usize {
        self.character_rows()
            .iter()
            .position(|row| *row == rows::CharacterRow::Grid)
            .unwrap_or(0)
    }

    /// The row of Apply, for the selection.
    pub(super) fn apply_row(&self) -> usize {
        self.character_rows()
            .iter()
            .position(|row| *row == rows::CharacterRow::Apply)
            .unwrap_or(0)
    }

    /// Refill the items of the category on show for the search.
    fn rebuild_items(&mut self) {
        let category = self.category();
        let mut items = std::mem::take(&mut self.creator.items);
        items.clear();
        let mut retail = 0;
        let mut rig = 0;
        let search = self.search.as_str();
        let species = match self.choice {
            Some(Choice::Species(index)) => {
                catalog_of(&self.loader).and_then(|catalog| catalog.species.get(index))
            }
            _ => None,
        };
        match category {
            Category::Model => {
                items.extend_from_slice(&self.tiles);
                retail = self.retail_tiles;
                rig = self.rig_tiles;
            }
            Category::Head | Category::Torso | Category::Legs => {
                if let Some(species) = species {
                    let parts = match category {
                        Category::Head => &species.heads,
                        Category::Torso => &species.torsos,
                        _ => &species.legs,
                    };
                    items.extend(
                        parts
                            .iter()
                            .enumerate()
                            .filter(|(_, part)| search_matches(search, part))
                            .map(|(index, _)| index),
                    );
                }
            }
            Category::Skin => {
                if let Some(species) = species {
                    items.extend(0..species.colors.len());
                }
            }
            Category::Hat | Category::Cape => {
                let slot = category.slot().unwrap_or(CosmeticSlot::Hat);
                items.push(0);
                items.extend(
                    (0..self.cosmetics.count(slot))
                        .filter(|index| {
                            self.cosmetics
                                .display(slot, *index)
                                .is_some_and(|name| search_matches(search, name))
                        })
                        .map(|index| index + 1),
                );
            }
        }
        self.creator.items = items;
        self.creator.retail = retail;
        self.creator.rig = rig;
    }

    /// Where the chosen item stands in the items on show.
    fn chosen_position(&self) -> Option<usize> {
        let category = self.category();
        let item = match category {
            Category::Model => self.choice_index(),
            Category::Hat | Category::Cape => category
                .slot()
                .and_then(|slot| self.cosmetics.worn_position(slot))
                .map_or(0, |index| index + 1),
            _ => self.variants[category.axis()?],
        };
        self.creator.items.iter().position(|each| *each == item)
    }

    /// Choose the item at `position` of the grid: a model, part or colour is the
    /// draft's, a hat or cape is worn.
    pub(super) fn creator_pick(&mut self, console: &mut ViewerConsole, position: usize) {
        let Some(&item) = self.creator.items.get(position) else {
            return;
        };
        let category = self.category();
        self.creator.cursor = position;
        if let Some(slot) = category.slot() {
            self.cosmetics.wear(console, slot, item.checked_sub(1));
            return;
        }
        match category.axis() {
            Some(axis) => self.set_variant(axis, item),
            None => self.select_choice(item),
        }
        // Shown, not worn yet: the Apply row has the keys, so Enter wears it.
        self.selected = self.apply_row();
    }

    /// Choose team `index` of Default, Red and Blue.
    fn pick_team(&mut self, index: usize) {
        let target = TeamSkin::ALL[index.min(TeamSkin::ALL.len() - 1)];
        for _ in 0..TeamSkin::ALL.len() {
            if self.team == target {
                break;
            }
            self.cycle_team(1);
        }
        self.creator.show_chosen();
    }

    /// Up or Down on the grid: the cursor moves a line, and the keys carry on to the
    /// next row only when it cannot; whether it moved.
    pub(super) fn grid_step(&mut self, direction: isize) -> bool {
        if !self.is_sjk()
            || self.page != ProfilePage::Character
            || self.selected != self.grid_row()
            || self.creator.columns == 0
        {
            return false;
        }
        let next = self
            .creator
            .cursor
            .checked_add_signed(direction * self.creator.columns as isize)
            .filter(|next| *next < self.creator.items.len());
        match next {
            Some(next) => {
                self.creator.cursor = next;
                self.creator.follow = true;
                true
            }
            None => false,
        }
    }

    /// Left or Right on the grid: the cursor moves a tile.
    pub(super) fn grid_side(&mut self, direction: isize) {
        let last = self.creator.items.len().saturating_sub(1);
        self.creator.cursor = self
            .creator
            .cursor
            .saturating_add_signed(direction)
            .min(last);
        self.creator.follow = true;
    }

    /// A click or hover on tile `local` of those on screen.
    pub(super) fn creator_tile(
        &mut self,
        kind: UiEventKind,
        local: usize,
        console: &mut ViewerConsole,
    ) {
        let position = self.creator.first_shown + local;
        match kind {
            UiEventKind::HoverEnter | UiEventKind::Hover
                if !self.name_editing && !self.search_editing =>
            {
                self.selected = self.grid_row();
                self.creator.cursor = position.min(self.creator.items.len().saturating_sub(1));
            }
            UiEventKind::Activate => {
                if self.name_editing {
                    self.name_editing = false;
                    self.apply(console);
                }
                self.search_editing = false;
                self.creator_pick(console, position);
            }
            _ => {}
        }
    }

    /// The pointer on the strip or the team; `None` leaves it to the screen.
    pub(super) fn creator_pointer(
        &mut self,
        kind: UiEventKind,
        token: u16,
    ) -> Option<PlayerMenuResult> {
        if kind != UiEventKind::Activate {
            return None;
        }
        if let Some(category) = token
            .checked_sub(CATEGORY_BASE)
            .and_then(|index| self.categories().get(usize::from(index)))
            .copied()
        {
            self.set_category(category);
        } else if let Some(index) = token
            .checked_sub(TEAM_BASE)
            .filter(|index| usize::from(*index) < TeamSkin::ALL.len())
        {
            self.pick_team(usize::from(index));
        } else {
            return None;
        }
        Some(PlayerMenuResult::None)
    }

    /// The strip of categories, `top` from the column's `x` (frame pixels): the
    /// segments, the one on show lit.
    pub(super) fn sjk_category_strip(&mut self, frame: &Frame, x: f32, top: f32) {
        let categories = self.categories();
        let mut labels = [""; 7];
        for (label, category) in labels.iter_mut().zip(categories) {
            *label = category.label();
        }
        let current = categories
            .iter()
            .position(|category| *category == self.category())
            .unwrap_or(0);
        let focused = self
            .character_rows()
            .get(self.selected)
            .is_some_and(|row| *row == rows::CharacterRow::Category);
        kit::segments(
            &mut self.canvas,
            frame,
            x + kit::segments_width(&labels[..categories.len()]),
            top + 19.0,
            &labels[..categories.len()],
            current,
            focused,
            CATEGORY_BASE,
        );
    }

    /// The title line and the grid under it, in the column from `x`, `width` wide,
    /// from `top` to `bottom` (frame pixels).
    pub(super) fn sjk_creator_grid(
        &mut self,
        frame: &Frame,
        x: f32,
        width: f32,
        top: f32,
        bottom: f32,
    ) {
        let s = frame.s;
        let category = self.category();
        self.rebuild_items();
        if std::mem::take(&mut self.creator.to_chosen) {
            self.creator.cursor = self.chosen_position().unwrap_or(0);
            self.creator.follow = true;
        }
        let count = self.creator.items.len();
        self.creator.cursor = self.creator.cursor.min(count.saturating_sub(1));
        // The title, and for models the team they are listed for.
        let found = if self.search.is_empty() { "" } else { " found" };
        text(
            &mut self.canvas,
            TextFamily::Display,
            format_args!("{}  ·  {count}{found}", category.title()),
            frame.rect(x + 22.0, top + 6.0, 330.0, 28.0),
            22.0 * s,
            color::HOLO,
            FontWeight::Regular,
            TextAlign::Start,
        );
        if category == Category::Model {
            let focused = self
                .character_rows()
                .get(self.selected)
                .is_some_and(|row| *row == rows::CharacterRow::Team);
            kit::segments(
                &mut self.canvas,
                frame,
                x + width,
                top + TITLE * 0.5 - 1.0,
                &["Default", "Red", "Blue"],
                self.team.index(),
                focused,
                TEAM_BASE,
            );
        }
        // Under the hats and capes: why there are none, or that JoF has more.
        let note = match category.slot() {
            Some(_) if self.cosmetics.catalog().is_none() => {
                Some("Looking for the installed pieces...")
            }
            Some(slot) if self.cosmetics.count(slot) == 0 => {
                Some("None installed: JoF's cosmetics pack brings them.")
            }
            Some(slot) if self.cosmetics.has_missing(slot) => {
                Some("JoF's cosmetics pack has more than are installed.")
            }
            _ => None,
        };
        let bottom = if note.is_some() {
            bottom - 26.0
        } else {
            bottom
        };
        if let Some(note) = note {
            text(
                &mut self.canvas,
                TextFamily::Body,
                format_args!("{note}"),
                frame.rect(x + 4.0, bottom + 4.0, width, 22.0),
                16.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let grid_top = top + TITLE;
        if count == 0 {
            let line = self.catalogue_line().map(str::to_owned).unwrap_or_else(|| {
                match category {
                    Category::Model => "No model matches.",
                    Category::Hat | Category::Cape => "Nothing matches.",
                    _ => "No part matches.",
                }
                .to_owned()
            });
            text(
                &mut self.canvas,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(x + 22.0, grid_top + 8.0, width - 44.0, 24.0),
                17.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            self.creator.columns = 0;
            self.creator.max_first = 0;
            self.creator.lines.clear();
            return;
        }
        // The lines: a heading and rows of tiles, per section.
        let (columns, tile_width, tile_height) = match category {
            Category::Hat | Category::Cape => (
                PIECE_COLUMNS,
                (width - GAP * (PIECE_COLUMNS - 1) as f32) / PIECE_COLUMNS as f32,
                PIECE_HEIGHT,
            ),
            _ => {
                let tile = (width - GAP * (COLUMNS - 1) as f32) / COLUMNS as f32;
                (COLUMNS, tile, tile)
            }
        };
        let rig = self.creator.rig;
        let sections = category == Category::Model
            && (rig > 0
                || catalog_of(&self.loader)
                    .is_some_and(|catalog| catalog.retail_characters + catalog.retail_species > 0));
        let retail = self.creator.retail;
        let mut lines = std::mem::take(&mut self.creator.lines);
        lines.clear();
        let rows_of = |lines: &mut Vec<Line>, from: usize, to: usize| {
            for first in (from..to).step_by(columns) {
                lines.push(Line::Tiles {
                    first,
                    count: (to - first).min(columns),
                });
            }
        };
        if sections {
            if retail > 0 {
                lines.push(Line::Heading("Jedi Academy", retail));
                rows_of(&mut lines, 0, retail);
            }
            // High quality models (SJK rig models) only when one is installed.
            if rig > 0 {
                lines.push(Line::Heading("High quality", rig));
                rows_of(&mut lines, retail, retail + rig);
            }
            if retail + rig < count {
                lines.push(Line::Heading("Community", count - retail - rig));
                rows_of(&mut lines, retail + rig, count);
            }
        } else {
            rows_of(&mut lines, 0, count);
        }
        let step = tile_height + GAP;
        let height_of = |line: &Line| match line {
            Line::Heading(..) => HEADING,
            Line::Tiles { .. } => step,
        };
        let room = bottom - grid_top;
        // The first line the last one still fits under.
        let mut used = 0.0;
        let mut max_first = lines.len();
        for (index, line) in lines.iter().enumerate().rev() {
            used += height_of(line);
            if used - GAP > room {
                break;
            }
            max_first = index;
        }
        let max_first = max_first.min(lines.len() - 1);
        let fits = |first: usize| {
            let mut used = 0.0;
            let mut shown = 0;
            for line in &lines[first..] {
                used += height_of(line);
                if used - GAP > room && shown > 0 {
                    break;
                }
                shown += 1;
            }
            shown
        };
        let mut first = self.creator.first_line.min(max_first);
        if std::mem::take(&mut self.creator.follow) {
            let at = lines.iter().position(|line| {
                matches!(line, Line::Tiles { first, count }
                    if (*first..*first + *count).contains(&self.creator.cursor))
            });
            if let Some(at) = at {
                if at < first {
                    first = if at > 0 && matches!(lines[at - 1], Line::Heading(..)) {
                        at - 1
                    } else {
                        at
                    };
                }
                while at >= first + fits(first) && first < max_first {
                    first += 1;
                }
            }
        }
        let shown = fits(first);
        self.creator.first_line = first;
        self.creator.max_first = max_first;
        self.creator.columns = columns;
        let area_height: f32 = lines[first..first + shown]
            .iter()
            .map(height_of)
            .sum::<f32>()
            - GAP;
        self.canvas.scroll_region(
            GRID_SCROLL_TOKEN,
            frame.rect(x, grid_top, width, area_height.max(1.0)),
        );
        let focused = self
            .character_rows()
            .get(self.selected)
            .is_some_and(|row| *row == rows::CharacterRow::Grid);
        let chosen = self.chosen_position();
        let mut y = grid_top;
        let mut local = 0_usize;
        self.creator.first_shown = lines[first..]
            .iter()
            .find_map(|line| match line {
                Line::Tiles { first, .. } => Some(*first),
                Line::Heading(..) => None,
            })
            .unwrap_or(0);
        for line in &lines[first..first + shown] {
            match *line {
                Line::Heading(name, members) => {
                    text(
                        &mut self.canvas,
                        TextFamily::Display,
                        format_args!("{name}  ·  {members}"),
                        frame.rect(x + 4.0, y + 4.0, 320.0, 24.0),
                        18.0 * s,
                        color::GOLD,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::SolidRect {
                        rect: frame.rect(x + 210.0, y + 16.0, width - 210.0, 1.0),
                        color: color::alpha(color::GOLD, 0.35),
                    });
                    y += HEADING;
                }
                Line::Tiles { first: from, count } => {
                    for column in 0..count {
                        let position = from + column;
                        if local >= usize::from(MAX_VISIBLE_TILES) {
                            break;
                        }
                        let token = TILE_BASE + local as u16;
                        let rect = [
                            x + column as f32 * (tile_width + GAP),
                            y,
                            tile_width,
                            tile_height,
                        ];
                        let lit = self.canvas.token_hovered(token)
                            || (focused && position == self.creator.cursor);
                        self.sjk_tile(
                            frame,
                            category,
                            position,
                            rect,
                            chosen == Some(position),
                            lit,
                        );
                        self.canvas
                            .hit_region(token, frame.rect(rect[0], rect[1], rect[2], rect[3]));
                        local += 1;
                    }
                    y += step;
                }
            }
        }
        if lines.len() > shown {
            self.canvas.scrollbar(
                GRID_SCROLL_TOKEN,
                frame.rect(x + width + 12.0, grid_top, 4.0, area_height.max(1.0)),
                first,
                shown,
                lines.len(),
            );
        }
        self.creator.lines = lines;
    }

    /// One tile of the grid: its picture, then the ring of what is chosen or lit.
    fn sjk_tile(
        &mut self,
        frame: &Frame,
        category: Category,
        position: usize,
        area: [f32; 4],
        chosen: bool,
        lit: bool,
    ) {
        let s = frame.s;
        let Some(&item) = self.creator.items.get(position) else {
            return;
        };
        let rect = frame.rect(area[0], area[1], area[2], area[3]);
        let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius: 8.0 * s,
            color: color::alpha(color::SPACE, if lit { 0.75 } else { 0.6 }),
        });
        let species = match self.choice {
            Some(Choice::Species(index)) => Some(index),
            _ => None,
        };
        let bright = if chosen || lit { 1.0 } else { 0.8 };
        match category {
            Category::Model => match self.icons.icon(item) {
                Some(texture) => {
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                        rect,
                        texture,
                        color: Color::new(1.0, 1.0, 1.0, bright),
                    });
                }
                None if self.icons.failed(item) => {
                    self.tile_name(item, rect, 11.0 * s, color::TEXT, 0.2 * s);
                }
                None => {}
            },
            Category::Head | Category::Torso | Category::Legs => {
                let part = category.part().unwrap_or(Part::Head);
                let picture = species.and_then(|species| self.part_icons.part(species, part, item));
                match picture {
                    Some(texture) => {
                        let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                            rect,
                            texture,
                            color: Color::new(1.0, 1.0, 1.0, bright),
                        });
                    }
                    None => {
                        let name = catalog_of(&self.loader)
                            .zip(species)
                            .and_then(|(catalog, index)| catalog.species.get(index))
                            .and_then(|species| match part {
                                Part::Head => species.heads.get(item),
                                Part::Torso => species.torsos.get(item),
                                Part::Legs => species.legs.get(item),
                            })
                            .map_or("", |name| {
                                name.split_once('_').map_or(name.as_str(), |(_, rest)| rest)
                            });
                        self.canvas.text_aligned(
                            name,
                            rect,
                            12.0 * s,
                            color::TEXT,
                            FontWeight::Regular,
                            0.2 * s,
                            TextAlign::Center,
                        );
                    }
                }
            }
            Category::Skin => {
                let rgb = catalog_of(&self.loader)
                    .zip(species)
                    .and_then(|(catalog, index)| catalog.species.get(index))
                    .and_then(|species| species.colors.get(item))
                    .map_or([128; 3], |colour| colour.rgb);
                let inset = 6.0 * s;
                let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: sjk_ui::Rect::new(
                        rect.x + inset,
                        rect.y + inset,
                        rect.width - 2.0 * inset,
                        rect.height - 2.0 * inset,
                    ),
                    radius: 6.0 * s,
                    color: super::sjk_view::rgb_color(rgb),
                });
            }
            Category::Hat | Category::Cape => {
                let slot = category.slot().unwrap_or(CosmeticSlot::Hat);
                let name = match item.checked_sub(1) {
                    None => "None",
                    Some(index) => self.cosmetics.display(slot, index).unwrap_or("?"),
                };
                self.canvas.text_aligned(
                    name,
                    rect,
                    18.0 * s,
                    if chosen {
                        color::GOLD_BRIGHT
                    } else {
                        color::alpha(color::TEXT, if lit { 1.0 } else { 0.85 })
                    },
                    FontWeight::Regular,
                    0.0,
                    TextAlign::Center,
                );
            }
        }
        if chosen || lit {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: 8.0 * s,
                width: if chosen { 2.5 * s } else { 1.5 * s },
                color: if chosen {
                    color::GOLD_BRIGHT
                } else {
                    Color::new(1.0, 1.0, 1.0, 0.6)
                },
            });
        }
    }

    /// Scroll the grid by `lines` (negative up).
    pub(super) fn scroll_creator(&mut self, lines: i32) {
        self.creator.scroll(lines);
    }

    /// The tile the cursor is on.
    pub(super) fn creator_cursor(&self) -> usize {
        self.creator.cursor
    }
}

#[cfg(test)]
impl PlayerMenu {
    /// For the world shots: the first species as the model (or the model as it is) and
    /// the strip's category `index`.
    pub(crate) fn creator_for_shot(&mut self, species: bool, index: usize) {
        let first_species = catalog_of(&self.loader)
            .filter(|catalog| !catalog.species.is_empty())
            .map(|catalog| catalog.characters.len());
        if let Some(absolute) = first_species.filter(|_| species) {
            self.select_choice(absolute);
            self.creator.show_chosen();
        }
        let categories = self.categories();
        self.set_category(categories[index.min(categories.len() - 1)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::sjk::TextTarget;

    fn draw(menu: &mut PlayerMenu) {
        let font = crate::text::load_modern(1.0, None).unwrap().font;
        let mut vertices = Vec::new();
        menu.append_sjk(
            TextTarget::Inter(&mut vertices, &font),
            [1920.0, 1080.0],
            1.0,
        );
    }

    fn centre(menu: &PlayerMenu, token: u16) -> sjk_ui::Vec2 {
        let rect = menu.canvas.rect_for(token).expect("the target");
        sjk_ui::Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    fn click(menu: &mut PlayerMenu, console: &mut ViewerConsole, position: sjk_ui::Vec2) {
        let button = sjk_ui::PointerButton::Primary;
        for event in [
            sjk_ui::InputEvent::PointerMove(position),
            sjk_ui::InputEvent::PointerPress { position, button },
            sjk_ui::InputEvent::PointerRelease { position, button },
        ] {
            let _ = menu.handle_pointer(event, console);
        }
    }

    /// The SJK UI's Character page with two hats and a cape installed.
    fn page() -> (PlayerMenu, ViewerConsole, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut vfs = sjk_vfs::VirtualFileSystem::new();
        vfs.mount_memory(
            "cosmetics",
            [
                ("models/cosmetics/hats/tophat.md3", b"x".to_vec()),
                ("models/cosmetics/hats/santahat.md3", b"x".to_vec()),
                ("models/cosmetics/capes/royalcape.md3", b"x".to_vec()),
            ],
        )
        .unwrap();
        let mut menu = PlayerMenu::new();
        menu.set_sjk(true);
        menu.cosmetics
            .open(&console, Some(&std::sync::Arc::new(vfs)));
        menu.return_target = ReturnTarget::InGame;
        menu.set_page(ProfilePage::Character);
        draw(&mut menu);
        (menu, console, directory)
    }

    /// A plain model offers Model, Hat and Cape; the strip's segments choose, and a
    /// click on a tile of the Hat grid wears that hat (None first, then the hats).
    #[test]
    fn the_strip_chooses_the_grid_and_a_tile_wears_a_hat() {
        let (mut menu, mut console, _directory) = page();
        assert_eq!(menu.category(), Category::Model);
        assert_eq!(menu.categories(), &Category::PLAIN);
        assert!(menu.canvas.rect_for(CATEGORY_BASE + 2).is_some());
        assert!(menu.canvas.rect_for(CATEGORY_BASE + 3).is_none());
        let at = centre(&menu, CATEGORY_BASE + 1);
        click(&mut menu, &mut console, at);
        assert_eq!(menu.category(), Category::Hat);
        draw(&mut menu);
        assert!(!menu.canvas.overflowed());
        assert_eq!(menu.creator.items, [0, 1, 2]);
        let at = centre(&menu, TILE_BASE + 2);
        click(&mut menu, &mut console, at);
        assert!(
            console
                .text_value("color1")
                .is_some_and(|value| value.ends_with("tophat")),
            "{:?}",
            console.text_value("color1")
        );
        draw(&mut menu);
        let at = centre(&menu, TILE_BASE);
        click(&mut menu, &mut console, at);
        assert_eq!(
            console
                .text_value("color1")
                .map(|value| sjk_client::split_color_value(value).1),
            Some(None)
        );
        // A hat is worn at once; nothing waits for Apply.
        assert!(!menu.look_pending());
    }

    /// The keys: Right on the strip shows the capes with the model's back to the camera,
    /// Enter goes down to the grid, the arrows choose a tile, Enter wears it.
    #[test]
    fn the_keys_walk_the_strip_then_the_tiles() {
        let (mut menu, mut console, _directory) = page();
        menu.selected = menu
            .character_rows()
            .iter()
            .position(|row| *row == rows::CharacterRow::Category)
            .unwrap();
        menu.adjust(&mut console, 1);
        menu.adjust(&mut console, 1);
        assert_eq!(menu.category(), Category::Cape);
        for frame in 0..90 {
            menu.turn.advance(f64::from(frame) / 60.0);
        }
        assert!((menu.model_turn().abs() - 180.0).abs() < 0.5);
        draw(&mut menu);
        menu.activate(&mut console);
        assert_eq!(menu.selected, menu.grid_row());
        menu.adjust(&mut console, 1);
        menu.activate(&mut console);
        assert!(
            console
                .text_value("color2")
                .is_some_and(|value| value.ends_with("royalcape"))
        );
        // Up at the first line leaves the grid for the row above.
        assert!(!menu.grid_step(-1));
        menu.step_category(-1);
        assert_eq!(menu.category(), Category::Hat);
        for frame in 90..180 {
            menu.turn.advance(f64::from(frame) / 60.0);
        }
        assert!(menu.model_turn().abs() < 0.5, "{}", menu.model_turn());
    }

    /// The search filters the grid on show, by name.
    #[test]
    fn the_search_filters_the_hats() {
        let (mut menu, _console, _directory) = page();
        menu.set_category(Category::Hat);
        menu.set_search("santa");
        draw(&mut menu);
        // None stays, then the one hat the words name.
        assert_eq!(menu.creator.items, [0, 1]);
    }

    // The tokens stay apart from the tiles' and the screen's others.
    const _: () = assert!(CATEGORY_BASE + 7 <= TEAM_BASE);
    const _: () = assert!(TEAM_BASE + 3 <= 2_500);
    const _: () = assert!(TILE_BASE + MAX_VISIBLE_TILES < CATEGORY_BASE);
}

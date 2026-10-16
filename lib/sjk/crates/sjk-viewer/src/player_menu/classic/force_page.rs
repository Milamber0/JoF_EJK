//! Drawing of the classic Force page. Retail's `ingame_playerforce` window
//! gives the frame, the title band, the blue and red side bars
//! (`menu_blendbox`, `menu_blendboxr`), the gold mastery line and the level
//! stars numbered with what each level costs (`UI_DrawForceStars`). SJK adds
//! the side cards with their emblems, a points meter that previews a
//! hovered star's cost, a holocron icon on every power, two columns, and a
//! panel describing the focused power. Retail's templates column (the
//! `FEEDER_FORCECFG` list, the name field and Save) keeps its place on the
//! left.

use super::layout::{self, Item, force};
use super::view::{FRAME, LABEL, STAR_BASE, TEMPLATE_BASE, TEMPLATES_SCROLL, VALUE};
use super::{ClassicPage, Frame};
use crate::menu::art::ArtPiece;
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{DISABLED, FOCUS, GOLD, glow, ink};
use crate::player_menu::PlayerMenu;
use crate::player_menu::force::NextLevel;
use crate::player_menu::force_icons::{power_texture, side_texture};
use sjk_client::{ForcePower, ForceSide};
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// Retail row colours: neutral and saber powers, the light side's, the
/// dark side's (`forecolor` of the `setfp_*` items), and their hover tints.
const NEUTRAL_TEXT: Color = Color::new(0.8, 0.8, 0.8, 1.0);
const LIGHT_TEXT: Color = Color::new(0.5, 0.5, 1.0, 1.0);
const DARK_TEXT: Color = Color::new(1.0, 0.2, 0.2, 1.0);
const LIGHT_HOVER: Color = Color::new(0.75, 0.75, 1.0, 1.0);
const DARK_HOVER: Color = Color::new(1.0, 0.55, 0.5, 1.0);
/// Side card tints.
const LIGHT_TINT: Color = Color::new(0.36, 0.66, 1.0, 1.0);
const DARK_TINT: Color = Color::new(1.0, 0.33, 0.28, 1.0);
/// A level the points left cannot pay for.
const SHORT: Color = Color::new(1.0, 0.4, 0.3, 1.0);
/// A power the server the client plays on turns off, holds at a level or
/// does not use: still bought here, so not greyed out.
const SERVER_LIMIT: Color = Color::new(1.0, 0.62, 0.25, 1.0);
/// Stars of a power that cannot be bought here (a team power outside team
/// games, Saber Defend or Throw without Attack). Retail's `grColor` (0.2) all
/// but vanished on the window, hiding the costs; this grey still reads as
/// unavailable.
const UNUSABLE: Color = Color::new(0.55, 0.55, 0.55, 1.0);
/// Holocron opacity of such a power; one that can be bought is drawn whole,
/// bought or not, so its picture never fades into the window.
const UNUSABLE_ICON: f32 = 0.55;
/// The template list (`fcflist`: `backcolor 0 0 .5 .25`, `bordercolor .5
/// .5 .5`, the chosen row `outlinecolor .25 .464 .578 .5`).
const TEMPLATE_BACK: Color = Color::new(0.0, 0.0, 0.5, 0.25);
const TEMPLATE_BORDER: Color = Color::new(0.5, 0.5, 0.5, 1.0);
const TEMPLATE_CHOSEN: Color = Color::new(0.25, 0.464, 0.578, 0.5);
/// A save that worked.
const SAVED: Color = Color::new(0.45, 0.9, 0.45, 1.0);
/// Most template rows a token range names.
pub(super) const MAX_TEMPLATE_ROWS: usize = 128;

/// The page's power names (`MENUS_FORCE_*`), in `forcePowers_t` order.
pub(super) const POWER_LABELS: [&str; 18] = [
    "Heal",
    "Jump",
    "Speed",
    "Push",
    "Pull",
    "Mind Trick",
    "Grip",
    "Lightning",
    "Dark Rage",
    "Protect",
    "Absorb",
    "Team Heal",
    "Team Energize",
    "Drain",
    "Sight",
    "Saber Attack",
    "Saber Defend",
    "Saber Throw",
];

/// Force mastery names (`forceMasteryLevels`, `mp_ingame.str`).
pub(super) const MASTERY: [&str; 8] = [
    "Uninitiated",
    "Initiate",
    "Padawan",
    "Jedi",
    "Jedi Adept",
    "Jedi Guardian",
    "Jedi Knight",
    "Jedi Master",
];

pub(crate) fn mastery(rank: u8) -> &'static str {
    MASTERY[usize::from(rank).min(MASTERY.len() - 1)]
}

/// The pointer token of level `level` (1 to 3) of power `power`.
pub(super) fn star_token(power: usize, level: u8) -> u16 {
    STAR_BASE + (power * 3) as u16 + u16::from(level.saturating_sub(1))
}

/// Power and level a star token names.
pub(super) fn star_of(token: u16) -> Option<(usize, u8)> {
    let offset = token.checked_sub(STAR_BASE)?;
    let power = usize::from(offset / 3);
    (power < POWER_LABELS.len()).then_some((power, (offset % 3) as u8 + 1))
}

/// A level's price as the detail panel prints it: points, or "free".
struct Cost(Option<u8>);

impl std::fmt::Display for Cost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(points) => write!(formatter, "{points}"),
            None => formatter.write_str("free"),
        }
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, alpha)
}

impl PlayerMenu {
    /// Window, title, mastery line, points meter and column headings.
    pub(super) fn force_page(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Force;
        let at = |canvas| layout::place(page, frame, canvas);
        if frame == Frame::Full {
            for (piece, canvas) in [
                (ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]),
                (ArtPiece::SideLeft, [0.0, 0.0, 160.0, 480.0]),
                (ArtPiece::SideRight, [480.0, 0.0, 160.0, 480.0]),
            ] {
                self.piece(place, piece, canvas);
            }
        }
        self.window_box(place, page, frame);
        self.band_title(
            place,
            at([20.0, 5.0, 560.0, 28.0]),
            "Choose your Force Training",
            15.0,
        );
        let allocation = self.force.allocation();
        let rank = allocation.rank;
        let side = allocation.side;
        // On a server below Jedi Master, the rank is its highest.
        let limit = match self.force.server() {
            Some(server) if server.max_rank < 7 => " (this server's highest)",
            _ => "",
        };
        self.label_fmt(
            place,
            at([15.0, 36.0, 570.0, 16.0]),
            format_args!("Force Mastery: {}{limit}", mastery(rank)),
            14.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        self.points_meter(place, frame);
        let (side_text, side_band) = match side {
            ForceSide::Light => ("Light Side", ArtPiece::BlendBox),
            ForceSide::Dark => ("Dark Side", ArtPiece::BlendBoxRed),
        };
        self.column_heading(place, at(force::TEMPLATES_HEAD), "Force Templates", None);
        self.template_note(place, at(force::TEMPLATE_NOTE));
        self.column_heading(place, at(force::NEUTRAL_HEAD), "Neutral", None);
        self.column_heading(place, at(force::SIDE_HEAD), side_text, Some(side_band));
        self.column_heading(place, at(force::SABER_HEAD), "Lightsaber", None);
    }

    /// "Points Remaining", and a bar of the points left that previews what
    /// a hovered star would take from it.
    fn points_meter(&mut self, place: &Placement, frame: Frame) {
        let page = ClassicPage::Force;
        let at = |canvas| layout::place(page, frame, canvas);
        let remaining = self.force.remaining_points();
        let total = self.force.total_points().max(1);
        let dirty = self.force.is_dirty();
        let [x, y, w, h] = at(force::POINTS);
        self.label(
            place,
            [x, y, w * 0.5, h],
            "Points Remaining:",
            12.0,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if dirty {
            self.label(
                place,
                [x, y, w, h],
                "not applied",
                11.0,
                with_alpha(GOLD, 0.8),
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
        self.label_fmt(
            place,
            [x + w * 0.5, y, w * 0.5, h],
            format_args!("{remaining} of {total}"),
            13.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::End,
        );
        let meter = at(force::METER);
        self.fill(place, meter, ink(0.7));
        let [mx, my, mw, mh] = meter;
        let fraction = |points: u16| mw * f32::from(points.min(total)) / f32::from(total);
        let left = fraction(remaining);
        self.fill(place, [mx, my, left, mh], with_alpha(GOLD, 0.85));
        // A hovered star beyond the power's level shows its price on the bar.
        if let Some((power, level)) = self.hovered_star() {
            let cost = self.force.cost_to(power, level);
            if cost > 0 {
                if cost <= remaining {
                    let width = fraction(cost);
                    self.fill(
                        place,
                        [mx + left - width, my, width, mh],
                        Color::new(1.0, 1.0, 1.0, 0.85),
                    );
                } else {
                    self.fill(place, [mx, my, left, mh], with_alpha(SHORT, 0.9));
                }
            }
        }
        self.border(place, meter, FRAME, 1.0);
    }

    /// The star under the pointer, when one is.
    fn hovered_star(&self) -> Option<(usize, u8)> {
        (0..POWER_LABELS.len())
            .flat_map(|power| (1..=3).map(move |level| (power, level)))
            .find(|(power, level)| self.canvas.token_hovered(star_token(*power, *level)))
    }

    /// A column heading over a thin rule, on a side bar for the side's.
    fn column_heading(
        &mut self,
        place: &Placement,
        canvas: [f32; 4],
        text: &str,
        band: Option<ArtPiece>,
    ) {
        let [x, y, w, h] = canvas;
        if let Some(band) = band {
            self.piece(place, band, canvas);
        }
        self.label(
            place,
            [x + 4.0, y, w - 8.0, h],
            text,
            12.0,
            LABEL,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        self.fill(place, [x, y + h, w, 1.0], with_alpha(FRAME, 0.9));
    }

    /// One entry of the Force page.
    pub(super) fn force_entry(
        &mut self,
        place: &Placement,
        item: Item,
        canvas: [f32; 4],
        active: bool,
    ) {
        match item {
            Item::SideLight => self.side_card(place, canvas, ForceSide::Light, active),
            Item::SideDark => self.side_card(place, canvas, ForceSide::Dark, active),
            Item::Power(index) => self.power_row(place, usize::from(index), canvas, active),
            Item::Templates => self.template_list(place, canvas, active),
            Item::TemplateName => self.template_name(place, canvas, active),
            Item::TemplateSave => {
                if active {
                    glow(
                        &mut self.canvas,
                        place.rect(canvas),
                        place.scale,
                        self.classic.art,
                    );
                }
                self.label(
                    place,
                    canvas,
                    item.label(),
                    14.0,
                    if active { FOCUS } else { GOLD },
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            Item::ForceReset | Item::ForceDiscard | Item::ForceApply => {
                let enabled = item == Item::ForceReset || self.force.is_dirty();
                if active && enabled {
                    glow(
                        &mut self.canvas,
                        place.rect(canvas),
                        place.scale,
                        self.classic.art,
                    );
                }
                let color = match (enabled, active) {
                    (false, _) => DISABLED,
                    (true, true) => FOCUS,
                    (true, false) => GOLD,
                };
                self.label(
                    place,
                    canvas,
                    item.label(),
                    15.0,
                    color,
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            _ => {}
        }
    }

    /// A side card: the side's bar and emblem, lit when chosen.
    fn side_card(&mut self, place: &Placement, canvas: [f32; 4], side: ForceSide, active: bool) {
        let chosen = self.force.allocation().side == side;
        let (text, band, tint) = match side {
            ForceSide::Light => ("Light Side", ArtPiece::BlendBox, LIGHT_TINT),
            ForceSide::Dark => ("Dark Side", ArtPiece::BlendBoxRed, DARK_TINT),
        };
        let [x, y, w, h] = canvas;
        if chosen {
            self.fill(place, canvas, with_alpha(tint, 0.14));
            self.piece(place, band, canvas);
        } else {
            self.fill(place, canvas, ink(0.55));
        }
        let border = if active {
            FOCUS
        } else if chosen {
            with_alpha(tint, 0.9)
        } else {
            with_alpha(FRAME, 0.8)
        };
        self.border(place, canvas, border, if chosen { 2.0 } else { 1.0 });
        let emblem = side_texture(side);
        let mut text_x = x + 10.0;
        if self.force_icons.is_texture_ready(emblem) {
            let size = h - 6.0;
            let shade = if chosen || active { 1.0 } else { 0.35 };
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect([x + 3.0, y + 3.0, size, size]),
                texture: emblem,
                color: Color::new(shade, shade, shade, 1.0),
            });
            text_x = x + size + 10.0;
        }
        let color = match (chosen, active) {
            (_, true) | (true, _) => FOCUS,
            _ => with_alpha(VALUE, 0.8),
        };
        self.label(
            place,
            [text_x, y, x + w - text_x - 4.0, h],
            text,
            15.0,
            color,
            FontWeight::Semibold,
            TextAlign::Start,
        );
    }

    /// One power: its holocron, its name in the column's colour, and three
    /// stars numbered with each level's cost.
    fn power_row(&mut self, place: &Placement, index: usize, canvas: [f32; 4], active: bool) {
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return;
        };
        let level = self.force.allocation().levels[index];
        let next = self.force.next_level(index);
        let usable = level > 0 || !matches!(next, NextLevel::OtherSide | NextLevel::NeedsOffense);
        let limited = self.force.server_limit(index).is_some();
        let [x, y, _, h] = canvas;
        if active {
            self.piece(place, ArtPiece::BlendBox2, canvas);
        }
        let icon = power_texture(index);
        if self.force_icons.is_texture_ready(icon) {
            let alpha = if usable { 1.0 } else { UNUSABLE_ICON };
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect([x + 1.0, y + 1.0, h - 2.0, h - 2.0]),
                texture: icon,
                color: Color::new(1.0, 1.0, 1.0, alpha),
            });
        }
        let (rest, hover) = match power.side() {
            Some(ForceSide::Light) => (LIGHT_TEXT, LIGHT_HOVER),
            Some(ForceSide::Dark) => (DARK_TEXT, DARK_HOVER),
            None => (NEUTRAL_TEXT, FOCUS),
        };
        let color = match (usable, active) {
            (false, _) => DISABLED,
            (true, _) if limited => SERVER_LIMIT,
            (true, true) => hover,
            (true, false) => rest,
        };
        let first_star = force::star(canvas, 1);
        self.label(
            place,
            [x + h + 4.0, y, first_star[0] - x - h - 6.0, h],
            POWER_LABELS[index],
            12.0,
            color,
            if level > 0 {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            TextAlign::Start,
        );
        let hovered = self
            .hovered_star()
            .filter(|(star_power, _)| *star_power == index)
            .map(|(_, star_level)| star_level);
        let affordable = hovered.is_some_and(|target| {
            self.force.cost_to(index, target) <= self.force.remaining_points()
        });
        for star_level in 1..=3 {
            let rect = force::star(canvas, star_level);
            let bought = star_level <= level;
            let preview = !bought && hovered.is_some_and(|target| star_level <= target);
            let cost = power.level_cost(star_level);
            let piece = ArtPiece::force_level(cost, bought || preview);
            let tint = match (usable, preview) {
                (false, _) => UNUSABLE,
                (true, true) if affordable => Color::new(1.0, 1.0, 1.0, 0.6),
                (true, true) => SHORT,
                (true, false) => Color::new(1.0, 1.0, 1.0, 1.0),
            };
            if self.classic.art.has(piece) {
                let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: place.rect(rect),
                    texture: piece.texture(),
                    color: tint,
                });
            } else {
                // Without the retail stars: a filled or hollow chip with the cost.
                if bought || preview {
                    self.fill(place, rect, with_alpha(GOLD, tint.a * 0.9));
                }
                self.border(place, rect, with_alpha(VALUE, tint.a), 1.0);
                self.label_fmt(
                    place,
                    rect,
                    format_args!("{cost}"),
                    10.0,
                    if bought { ink(1.0) } else { VALUE },
                    FontWeight::Semibold,
                    TextAlign::Center,
                );
            }
            self.canvas
                .hit_region(star_token(index, star_level), place.rect(rect));
        }
    }

    /// The side's templates, 16-unit rows: the player's own tagged, the one
    /// the draft holds filled.
    fn template_list(&mut self, place: &Placement, canvas: [f32; 4], active: bool) {
        self.fill(place, canvas, TEMPLATE_BACK);
        self.border(
            place,
            canvas,
            if active { FOCUS } else { TEMPLATE_BORDER },
            1.0,
        );
        self.canvas
            .scroll_region(TEMPLATES_SCROLL, place.rect(canvas));
        let [x, y, w, h] = canvas;
        let rows = (h / force::TEMPLATE_ROW).floor() as usize;
        let side = self.force.allocation().side;
        let count = self
            .force_templates
            .list
            .of(side)
            .len()
            .min(MAX_TEMPLATE_ROWS);
        if count == 0 {
            self.label(
                place,
                [x + 4.0, y + 6.0, w - 8.0, force::TEMPLATE_ROW],
                "No templates for this side.",
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Center,
            );
            return;
        }
        let chosen = self.template_row();
        let state = &mut self.force_templates;
        if let Some(row) = chosen {
            if row < state.scroll {
                state.scroll = row;
            } else if row >= state.scroll + rows {
                state.scroll = row + 1 - rows;
            }
        }
        state.scroll = state.scroll.min(count.saturating_sub(rows));
        let first = state.scroll;
        for row in first..(first + rows).min(count) {
            let cell = [
                x + 1.0,
                y + 1.0 + (row - first) as f32 * force::TEMPLATE_ROW,
                w - 2.0,
                force::TEMPLATE_ROW,
            ];
            let token = TEMPLATE_BASE + row as u16;
            let hovered = self.canvas.token_hovered(token);
            if chosen == Some(row) {
                self.fill(place, cell, TEMPLATE_CHOSEN);
            }
            if hovered {
                self.piece(place, ArtPiece::BlendBox2, cell);
            }
            let Some((name, own)) = self
                .force_templates
                .list
                .of(side)
                .get(row)
                .map(|template| (template.name.clone(), template.own))
            else {
                continue;
            };
            let [cx, cy, cw, ch] = cell;
            let color = if hovered || chosen == Some(row) {
                FOCUS
            } else {
                VALUE
            };
            let tag = if own { 34.0 } else { 0.0 };
            self.label(
                place,
                [cx + 4.0, cy, cw - 8.0 - tag, ch],
                &name,
                11.0,
                color,
                FontWeight::Regular,
                TextAlign::Start,
            );
            if own {
                self.label(
                    place,
                    [cx + cw - tag - 4.0, cy, tag, ch],
                    "yours",
                    9.0,
                    GOLD,
                    FontWeight::Semibold,
                    TextAlign::End,
                );
            }
            self.canvas.hit_region(token, place.rect(cell));
        }
        if count > rows {
            let track = place.rect([x + w - 5.0, y + 2.0, 3.0, h - 4.0]);
            self.canvas
                .scrollbar(TEMPLATES_SCROLL, track, first, rows, count);
        }
    }

    /// The name field (`ui_SaveFCF`), underlined while it takes typing.
    fn template_name(&mut self, place: &Placement, canvas: [f32; 4], active: bool) {
        let s = place.scale;
        let editing = self.force_templates.editing;
        if active && !editing {
            glow(&mut self.canvas, place.rect(canvas), s, self.classic.art);
        }
        let name = self.force_templates.name.clone();
        let color = if active || editing { FOCUS } else { VALUE };
        if name.is_empty() && !editing {
            self.label(
                place,
                canvas,
                "Name: (type a name)",
                12.0,
                with_alpha(color, 0.7),
                FontWeight::Regular,
                TextAlign::Start,
            );
        } else {
            self.label_fmt(
                place,
                canvas,
                format_args!("Name: {name}"),
                12.0,
                color,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if editing {
            let accent = self.canvas.theme().accent;
            self.canvas.edit_underline(place.rect(canvas), accent, s);
        }
    }

    /// How the last save went, under Save.
    fn template_note(&mut self, place: &Placement, canvas: [f32; 4]) {
        let Some((text, saved)) = self.force_templates.note.clone() else {
            return;
        };
        let [x, y, w, _] = canvas;
        self.label(
            place,
            [x, y, w, 14.0],
            &text,
            10.0,
            if saved { SAVED } else { SHORT },
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The panel under the side column: the hovered or focused power's
    /// holocron, level and costs, or the profile at a glance.
    pub(super) fn force_detail(&mut self, place: &Placement, frame: Frame, item: Option<Item>) {
        let panel = layout::place(ClassicPage::Force, frame, force::DETAIL);
        self.fill(place, panel, ink(0.45));
        self.border(place, panel, with_alpha(FRAME, 0.9), 1.0);
        let [x, y, w, _] = panel;
        let star = self.hovered_star().map(|(power, _)| power);
        match star.or_else(|| item.and_then(Item::power)) {
            Some(index) => self.power_detail(place, index, [x, y, w]),
            None => self.side_detail(place, [x, y, w]),
        }
    }

    fn power_detail(&mut self, place: &Placement, index: usize, [x, y, w]: [f32; 3]) {
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return;
        };
        let level = self.force.allocation().levels[index];
        let icon = power_texture(index);
        let text_x = if self.force_icons.is_texture_ready(icon) {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect([x + 5.0, y + 5.0, 44.0, 44.0]),
                texture: icon,
                color: FOCUS,
            });
            x + 56.0
        } else {
            x + 8.0
        };
        let width = x + w - text_x - 4.0;
        self.label(
            place,
            [text_x, y + 5.0, width, 16.0],
            POWER_LABELS[index],
            14.0,
            GOLD,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        self.label_fmt(
            place,
            [text_x, y + 21.0, width, 14.0],
            format_args!("Level {level} of 3"),
            12.0,
            VALUE,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let free_saber = self.force.free_saber();
        let status = [text_x, y + 36.0, width, 14.0];
        let next = self.force.next_level(index);
        let (fixed, color) = match next {
            NextLevel::Mastered => (Some("Mastered"), GOLD),
            NextLevel::Free => (Some("Next level: free"), LABEL),
            NextLevel::Costs(_) => (None, LABEL),
            NextLevel::Short(_) => (None, SHORT),
            NextLevel::OtherSide => (Some("The other side's power"), DISABLED),
            NextLevel::NeedsOffense => (Some("Requires Saber Attack 1"), SHORT),
        };
        match (fixed, next) {
            (Some(text), _) => self.label(
                place,
                status,
                text,
                12.0,
                color,
                FontWeight::Semibold,
                TextAlign::Start,
            ),
            (None, NextLevel::Costs(cost) | NextLevel::Short(cost)) => self.label_fmt(
                place,
                status,
                format_args!("Next level: {cost} points"),
                12.0,
                color,
                FontWeight::Semibold,
                TextAlign::Start,
            ),
            _ => {}
        }
        let cost = |level: u8| {
            Cost((!power.level_is_free(level, free_saber)).then(|| power.level_cost(level)))
        };
        let spent: u16 = (1..=level)
            .filter(|level| !power.level_is_free(*level, free_saber))
            .map(|level| u16::from(power.level_cost(level)))
            .sum();
        self.label_fmt(
            place,
            [x + 6.0, y + 54.0, w - 12.0, 13.0],
            format_args!("Levels cost {} / {} / {}", cost(1), cost(2), cost(3)),
            11.0,
            VALUE,
            FontWeight::Regular,
            TextAlign::Start,
        );
        // What the server does with it, in place of the spending, when it limits it.
        match self.force.server_limit(index) {
            Some(limit) => self.label(
                place,
                [x + 6.0, y + 68.0, w - 12.0, 13.0],
                limit.describe(),
                11.0,
                SERVER_LIMIT,
                FontWeight::Semibold,
                TextAlign::Start,
            ),
            None => self.label_fmt(
                place,
                [x + 6.0, y + 68.0, w - 12.0, 13.0],
                format_args!("Spent on it: {spent} points"),
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            ),
        }
    }

    /// The profile at a glance: the side's emblem, mastery and spending.
    fn side_detail(&mut self, place: &Placement, [x, y, w]: [f32; 3]) {
        let allocation = self.force.allocation();
        let side = allocation.side;
        let rank = allocation.rank;
        let known = allocation.levels.iter().filter(|level| **level > 0).count();
        let total = self.force.total_points();
        let used = total.saturating_sub(self.force.remaining_points());
        let emblem = side_texture(side);
        let text_x = if self.force_icons.is_texture_ready(emblem) {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: place.rect([x + 5.0, y + 5.0, 44.0, 44.0]),
                texture: emblem,
                color: FOCUS,
            });
            x + 56.0
        } else {
            x + 8.0
        };
        let width = x + w - text_x - 4.0;
        let (name, tint) = match side {
            ForceSide::Light => ("Light Side", LIGHT_TINT),
            ForceSide::Dark => ("Dark Side", DARK_TINT),
        };
        self.label(
            place,
            [text_x, y + 5.0, width, 16.0],
            name,
            14.0,
            tint,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        self.label(
            place,
            [text_x, y + 21.0, width, 14.0],
            mastery(rank),
            12.0,
            GOLD,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.label_fmt(
            place,
            [text_x, y + 36.0, width, 14.0],
            format_args!("{known} powers known"),
            12.0,
            VALUE,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let Some(server) = self.force.server() else {
            self.label_fmt(
                place,
                [x + 6.0, y + 61.0, w - 12.0, 13.0],
                format_args!("{used} of {total} points spent"),
                11.0,
                VALUE,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        };
        self.label_fmt(
            place,
            [x + 6.0, y + 54.0, w - 12.0, 13.0],
            format_args!("{used} of {total} points spent"),
            11.0,
            VALUE,
            FontWeight::Regular,
            TextAlign::Start,
        );
        // The server's line: how many powers it turns off (their names are
        // drawn in its colour), or that it allows them all.
        let off = (0..POWER_LABELS.len())
            .filter(|index| server.disabled_mask & (1 << index) != 0)
            .count();
        let at = [x + 6.0, y + 68.0, w - 12.0, 13.0];
        let (weight, align) = (FontWeight::Semibold, TextAlign::Start);
        match off {
            0 => self.label(
                place,
                at,
                "This server allows every power",
                11.0,
                VALUE,
                weight,
                align,
            ),
            _ => self.label_fmt(
                place,
                at,
                format_args!(
                    "This server turns {off} power{} off",
                    if off == 1 { "" } else { "s" }
                ),
                11.0,
                SERVER_LIMIT,
                weight,
                align,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_tokens_round_trip_and_stay_in_their_range() {
        for power in 0..POWER_LABELS.len() {
            for level in 1..=3 {
                assert_eq!(star_of(star_token(power, level)), Some((power, level)));
            }
        }
        assert_eq!(star_of(STAR_BASE - 1), None);
        assert_eq!(star_of(star_token(17, 3) + 1), None);
    }

    #[test]
    fn mastery_names_cover_every_rank() {
        assert_eq!(mastery(0), "Uninitiated");
        assert_eq!(mastery(7), "Jedi Master");
        assert_eq!(mastery(9), "Jedi Master");
    }
}

#[cfg(test)]
mod drawing_tests {
    use super::super::layout::{DARK_POWERS, LIGHT_POWERS, NEUTRAL_POWERS, SABER_POWERS};
    use super::super::{ClassicPage, Frame};
    use super::*;
    use crate::menu::art::ArtSet;
    use crate::player_menu::ReturnTarget;
    use sjk_ui::TextureId;

    /// An owner's profile: dark side, every point spent, Dark Rage and Team
    /// Energize still at level 0, in a free-for-all (no team powers).
    const OWNER: &str = "7-2-031330310000030333";

    /// The Force page drawn on `frame` for `side`, focused on `focus`, with all
    /// retail art and every Force icon uploaded, `templates` of the player's own
    /// templates listed on each side and the list scrolled to `scroll`. Drawing
    /// panics in a debug build if the canvas runs out of storage.
    fn drawn_with(
        frame: Frame,
        dark: bool,
        focus: u8,
        templates: usize,
        scroll: usize,
    ) -> PlayerMenu {
        let mut menu = PlayerMenu::new();
        menu.return_target = match frame {
            Frame::Full => ReturnTarget::MainMenu,
            Frame::InGame => ReturnTarget::InGame,
        };
        menu.classic_style = true;
        menu.classic.art = ArtPiece::ALL
            .iter()
            .fold(ArtSet::default(), |set, piece| set.with(*piece));
        for (texture, _) in crate::player_menu::force_icons::requests() {
            menu.force_icons.mark_ready(texture);
        }
        menu.show_classic(ClassicPage::Force);
        for side in [ForceSide::Light, ForceSide::Dark] {
            menu.force_templates.list.fill(side, templates);
        }
        menu.force.load_template("owner", OWNER);
        menu.choose_side(if dark {
            ForceSide::Dark
        } else {
            ForceSide::Light
        });
        menu.force_templates.scroll = scroll;
        menu.snapshot_focus_power(focus);
        let font = crate::text::load_modern(1.0, None).unwrap().font;
        let mut vertices = Vec::new();
        menu.append(&mut vertices, &font, [1920.0, 1080.0], 1.0);
        menu
    }

    fn drawn(frame: Frame, dark: bool, focus: u8) -> PlayerMenu {
        drawn_with(frame, dark, focus, 0, 0)
    }

    /// Colours of the textured quads drawing `texture`, in order.
    fn quads(menu: &PlayerMenu, texture: TextureId) -> Vec<Color> {
        menu.canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::TexturedQuad {
                    texture: drawn,
                    color,
                    ..
                } if *drawn == texture => Some(*color),
                _ => None,
            })
            .collect()
    }

    fn star_textures() -> Vec<TextureId> {
        (0..=8)
            .flat_map(|cost| [true, false].map(|bought| ArtPiece::force_level(cost, bought)))
            .map(ArtPiece::texture)
            .collect()
    }

    /// No templates, then 60 of the player's own (each row with its tag): the
    /// list at the top, in the middle and scrolled past its end.
    const LISTS: [(usize, usize); 4] = [(0, 0), (60, 0), (60, 23), (60, 100)];

    #[test]
    fn every_power_row_draws_a_readable_holocron_and_three_readable_stars() {
        let stars = star_textures();
        for frame in [Frame::Full, Frame::InGame] {
            for ((dark, side), (templates, scroll)) in [(false, LIGHT_POWERS), (true, DARK_POWERS)]
                .into_iter()
                .flat_map(|column| LISTS.map(|list| (column, list)))
            {
                let menu = drawn_with(frame, dark, side[0], templates, scroll);
                let shown = NEUTRAL_POWERS.iter().chain(&SABER_POWERS).chain(&side);
                for &power in shown {
                    let index = usize::from(power);
                    let holocron = quads(&menu, power_texture(index));
                    let row = holocron.first().copied();
                    let alpha = row.map_or(0.0, |color| color.a);
                    let usable = !matches!(
                        menu.force.next_level(index),
                        NextLevel::OtherSide | NextLevel::NeedsOffense
                    ) || menu.force.allocation().levels[index] > 0;
                    let wanted = if usable { 1.0 } else { UNUSABLE_ICON };
                    assert_eq!(alpha, wanted, "{frame:?} {} holocron", POWER_LABELS[index]);
                    let registered = menu.canvas.widget_tokens();
                    for level in 1..=3 {
                        assert!(
                            registered.contains(&star_token(index, level)),
                            "{frame:?} {} star {level} cannot be pointed at",
                            POWER_LABELS[index]
                        );
                    }
                }
                let drawn_stars: Vec<Color> = menu
                    .canvas
                    .draw_list()
                    .commands()
                    .iter()
                    .filter_map(|command| match command {
                        DrawCommand::TexturedQuad { texture, color, .. }
                            if stars.contains(texture) =>
                        {
                            Some(*color)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(drawn_stars.len(), 3 * 13, "{frame:?} dark {dark}");
                assert!(
                    drawn_stars
                        .iter()
                        .all(|color| color.r >= 0.5 && color.a >= 0.5),
                    "{frame:?} dark {dark}: a star fades into the window"
                );
                let list = menu.canvas.draw_list();
                assert!(list.len() < list.limit(), "the draw list is full");
                // Only the visible template rows take pointer areas, so the page
                // keeps its headroom whatever the list holds.
                let areas = menu.canvas.widget_count();
                assert!(
                    areas <= 84,
                    "{frame:?} {templates} templates: {areas} areas"
                );
                let (text, slots) = menu.canvas.text_budget();
                assert!(text * 2 <= slots, "{frame:?}: {text} of {slots} text runs");
            }
        }
    }

    /// Holocrons come from the icon atlas and stars from the retail art, so
    /// each row switches textures; the old limit of 48 switches dropped the
    /// pictures of the last rows (Dark Rage, Team Energize) and, with a hover
    /// glow, a Lightning star. The page keeps well under the renderer's limit,
    /// with room for the HUD and console drawn in the same frame.
    #[test]
    fn the_page_fits_the_renderers_texture_switches() {
        for frame in [Frame::Full, Frame::InGame] {
            for dark in [false, true] {
                let menu = drawn_with(frame, dark, 0, 60, 0);
                let switches = crate::ui_renderer::texture_switches(&[menu.canvas.draw_list()]);
                assert!(
                    switches > 48,
                    "{frame:?}: {switches} switches (the old limit held)"
                );
                assert!(
                    switches * 3 <= crate::ui_renderer::art::MAX_RUNS,
                    "{frame:?} dark {dark}: {switches} texture switches"
                );
            }
        }
    }

    #[test]
    fn the_detail_panel_shows_the_focused_powers_holocron() {
        // Dark Rage (8) and Team Energize (12), the dark column's last rows.
        for power in [8_u8, 12] {
            let menu = drawn(Frame::InGame, true, power);
            let holocron = quads(&menu, power_texture(usize::from(power)));
            assert_eq!(
                holocron.len(),
                2,
                "{} row and panel",
                POWER_LABELS[power as usize]
            );
            assert_eq!(holocron[1], FOCUS);
        }
    }
}

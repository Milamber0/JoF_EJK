//! The Collection's Shaders tab: the saber's looks as a rack, a row to each, or as a grid
//! of cards ([`SHADER_VIEW_CVAR`], switched by V or the two marks by the kinds): the
//! stock blade first in the player's colour, then every blade skin
//! (`docs/unlockables.md`), the rarest tier first, each framed in its tier's colour,
//! alive when owned, grey and still under a padlock when not. The one chosen is the
//! blade the player's model holds beside the page ([`Backstage`]), a locked one too, as
//! a preview only this screen draws; the words beside the model say what it is, since
//! when it is the player's or how to get it, and Equip or Unequip. Body shaders, for the
//! whole character, are to come: their kind is shown, not yet offered.

use super::view::{LEAD_Y, LEFT_X, STAGE_TEXT_WIDTH, STAGE_TEXT_X, left_lines};
use super::*;
use crate::menu::sjk::{Frame, color, kit, text};
use crate::menu_widgets::TextFamily;
use crate::rarity_fx;
use crate::unlockables::Unlockable;
use sjk_ui::{DrawCommand, FontWeight, TextAlign};

/// The rack: where its first row starts, a row's room and a swatch's size.
const RACK_TOP: f32 = 324.0;
const ROW_STEP: f32 = 106.0;
const ROW_HEIGHT: f32 = 96.0;
const SWATCH: [f32; 2] = [380.0, 84.0];
const RACK_WIDTH: f32 = 860.0;

/// The chroma mark's side beside a rack row's name and the detail's kind line, and the
/// room it keeps from the words after it (frame pixels).
const MARK: f32 = 24.0;
const MARK_GAP: f32 = 10.0;
/// The grid's cards: a card's size and the room between them (frame pixels).
const CARD: [f32; 2] = [164.0, 196.0];
const CARD_GAP: [f32; 2] = [10.0, 14.0];
/// How far the chosen card lifts (frame pixels), and its outline's colour: white, so it
/// never reads as a tier (Mythical's frame is gold).
const CHOSEN_LIFT: f32 = 5.0;
const CHOSEN_OUTLINE: sjk_ui::Color = sjk_ui::Color::new(1.0, 1.0, 1.0, 1.0);
/// The view switches' side and where the first stands, left of the kinds.
const SWITCH: f32 = 36.0;
const SWITCH_X: f32 = LEFT_X + 466.0;

/// The detail's state pill: its height, its least width and its gap from the name.
const TAG_HEIGHT: f32 = 26.0;
const TAG_MIN_WIDTH: f32 = 80.0;
const TAG_GAP: f32 = 18.0;
/// The detail's tier pill's height.
const PILL_HEIGHT: f32 = 30.0;

/// Where a rack row's words go (frame pixels, `[x, y, width, height]`), the row's top
/// at `y`: the chroma mark (a chroma only) and the name on the first line, the name
/// ending short of the row's right edge, and the state under them.
fn rack_words(y: f32, chroma: bool) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let x = LEFT_X + SWATCH[0] + 34.0;
    let right = LEFT_X - 12.0 + RACK_WIDTH - 16.0;
    let mark = [x, y + 23.0, MARK, MARK];
    let name_x = if chroma { x + MARK + MARK_GAP } else { x };
    let name = [name_x, y + 18.0, right - name_x, 34.0];
    let status = [x, y + 56.0, right - x, 22.0];
    (mark, name, status)
}

/// The detail's name and state pill on the line at `y` of the column from `x`, `width`
/// wide: the pill at the column's right end, the name ellipsised short of it.
fn title_rects(x: f32, y: f32, width: f32, tag_width: f32) -> ([f32; 4], [f32; 4]) {
    let tag_width = tag_width.max(TAG_MIN_WIDTH);
    let tag = [x + width - tag_width, y + 18.0, tag_width, TAG_HEIGHT];
    let name = [x, y, (tag[0] - TAG_GAP - x).max(0.0), 62.0];
    (name, tag)
}

/// The first row the rack shows, from the one it showed first: moved just enough that
/// the chosen row is in view, never past either end.
fn rack_window(first: usize, chosen: usize, rows: usize) -> usize {
    window(first, chosen, rows, RACK_SHOWN)
}

/// The first of `rows` a window of `shown` shows, from the one it showed first: moved
/// just enough that `chosen` is in view, never past either end.
fn window(first: usize, chosen: usize, rows: usize, shown: usize) -> usize {
    let first = if chosen < first {
        chosen
    } else if chosen >= first + shown {
        chosen + 1 - shown
    } else {
        first
    };
    first.min(rows.saturating_sub(shown))
}

/// Card `index`'s rectangle in the grid (frame pixels) when its first line shown is
/// `first_line`.
fn card_rect(index: usize, first_line: usize) -> [f32; 4] {
    let line = (index / GRID_COLUMNS) as f32 - first_line as f32;
    let column = (index % GRID_COLUMNS) as f32;
    [
        LEFT_X - 12.0 + column * (CARD[0] + CARD_GAP[0]),
        RACK_TOP + line * (CARD[1] + CARD_GAP[1]),
        CARD[0],
        CARD[1],
    ]
}

/// A shader's name on a card: without the " blade" every one ends in.
fn card_name(name: &str) -> &str {
    name.strip_suffix(" blade").unwrap_or(name)
}

/// The owned line of a shader the player holds: since when, and from the team or the
/// medal that brings it (a medal the client does not know reads as the team).
fn owned_line(grant: &sjk_identity::Unlock) -> String {
    if crate::debug_collectibles::added(grant) {
        return crate::debug_collectibles::NOTE.to_owned();
    }
    let date = crate::medals::date_text(grant.granted);
    let from = grant
        .medal
        .as_deref()
        .and_then(crate::medals::Medal::from_id)
        .map_or_else(
            || "from the SJK team".to_owned(),
            |medal| format!("with your {} medal", medal.name()),
        );
    let held = if grant.count > 1 {
        format!(", {} held", grant.count)
    } else {
        String::new()
    };
    if date.is_empty() {
        format!("Yours, {from}{held}")
    } else {
        format!("Yours since {date}, {from}{held}")
    }
}

/// The filter's chips and search field: where they stand and how tall they are.
const FILTER_Y: f32 = 292.0;
const FILTER_HEIGHT: f32 = 28.0;

/// The chip of tier `index` of [`FILTER_TIERS`]: its token, name, colour, whether it is
/// the one on and its width.
fn chip_of(
    index: usize,
    filter: &ShaderFilter,
    width: f32,
) -> (u16, String, sjk_ui::Color, bool, f32) {
    let tier = FILTER_TIERS[index];
    (
        FILTER_TIER_BASE + 1 + index as u16,
        tier.label().to_owned(),
        tier.colour(),
        filter.tier == Some(tier),
        width,
    )
}

/// What a row shows: the stock blade, or a blade skin.
#[derive(Clone, Copy)]
struct Row {
    skin: Option<&'static Unlockable>,
    owned: bool,
    worn: bool,
    /// How many the player holds (holocrons may drop the same shader again): 1 for the
    /// stock blade and a shader held once, 0 when locked.
    count: u32,
}

/// `word` with how many are held after it when there are several: "Yours x2".
fn held_words(word: &str, count: u32) -> String {
    if count > 1 {
        format!("{word} x{count}")
    } else {
        word.to_owned()
    }
}

impl Row {
    fn name(self) -> &'static str {
        self.skin.map_or("Stock blade", |skin| skin.name)
    }

    /// Its tier, `None` for the stock blade.
    fn tier(self) -> Option<unlockables::Rarity> {
        self.skin.map(|skin| skin.tier)
    }

    /// Its frame's colour: its tier's, quieter when locked; the stock blade's is the
    /// page's own.
    fn frame_colour(self) -> sjk_ui::Color {
        match self.tier() {
            Some(tier) => color::alpha(tier.colour(), if self.owned { 0.9 } else { 0.45 }),
            None => color::alpha(color::HOLO, 0.3),
        }
    }

    /// Its state, as the rack says it.
    fn status(self) -> String {
        match (self.skin, self.worn, self.owned) {
            (None, true, _) => "Worn, in your colour".to_owned(),
            (None, false, _) => "In your colour".to_owned(),
            (Some(_), true, _) => held_words("Worn", self.count),
            (Some(_), false, true) => held_words("Yours", self.count),
            (Some(_), false, false) => "Locked".to_owned(),
        }
    }
}

impl Panel {
    /// The rows, and the lead's words.
    fn rows(inputs: &Inputs<'_>, filter: &ShaderFilter) -> Vec<Row> {
        let worn = inputs
            .holdings
            .worn_blade_skin(inputs.setting)
            .map(|skin| skin.id);
        std::iter::once(Row {
            skin: None,
            owned: true,
            worn: worn.is_none(),
            count: 1,
        })
        .chain(
            unlockables::blade_skins_by_tier()
                .iter()
                .filter(|skin| filter.passes(skin, inputs.holdings.unlock(skin.id).is_some()))
                .map(|&skin| Row {
                    skin: Some(skin),
                    owned: inputs.holdings.unlock(skin.id).is_some(),
                    worn: worn == Some(skin.id),
                    count: inputs
                        .holdings
                        .unlock(skin.id)
                        .map_or(0, |unlock| unlock.count),
                }),
        )
        .collect()
    }

    /// The Shaders tab, swatches alive at `seconds`.
    pub(super) fn shaders(&mut self, frame: &Frame, inputs: &Inputs<'_>, seconds: f32) {
        let s = frame.s;
        self.grid = inputs.grid;
        let rows = Self::rows(inputs, &self.shader_filter);
        self.shader_list = rows.iter().filter_map(|row| row.skin).collect();
        self.shader_count = rows.len();
        if self.shader >= rows.len() {
            self.shader = rows.iter().position(|row| row.worn).unwrap_or(0);
        }
        let owned = unlockables::blade_skins()
            .filter(|skin| inputs.holdings.unlock(skin.id).is_some())
            .count();
        let headline = if inputs.holdings.reason().is_none() {
            format!("{owned} of {} owned", unlockables::ALL.len())
        } else {
            "Saber shaders".to_owned()
        };
        let shown_words;
        let line = match inputs.holdings {
            unlockables::Holdings::Known(_) if self.shader_filter.active() => {
                shown_words = format!(
                    "{} of {} shaders shown: T tier, O owned, / search.",
                    rows.len() - 1,
                    unlockables::ALL.len()
                );
                shown_words.as_str()
            }
            unlockables::Holdings::IdentityOff => {
                "Shaders are kept on the SJK hub: switch the identity on in Settings, Network."
            }
            unlockables::Holdings::NoHub => {
                "Shaders are kept on the SJK hub: no hub is set (cl_hubUrl)."
            }
            unlockables::Holdings::Waiting => "Waiting for the SJK hub to say which are yours.",
            unlockables::Holdings::Known(held)
                if held.iter().any(crate::debug_collectibles::added) =>
            {
                "Debug collectibles is on: every shader is shown as yours, on this PC only."
            }
            unlockables::Holdings::Known(_) => {
                "How your blade looks and sounds, for every SJK player on your server."
            }
        };
        self.lead(frame, &headline, line);
        self.filter_bar(frame);
        self.kinds(frame, owned);
        self.view_switches(frame);
        let unlocks = matches!(inputs.holdings, unlockables::Holdings::Known(_));
        // The rack (or the grid) scrolls just enough to keep the chosen one in view.
        self.shader_first = rack_window(self.shader_first, self.shader, rows.len());
        self.grid_first = window(
            self.grid_first,
            self.shader / GRID_COLUMNS,
            rows.len().div_ceil(GRID_COLUMNS),
            GRID_LINES_SHOWN,
        );
        let first = self.shader_first;
        for (index, row) in rows.iter().enumerate() {
            self.shader_rows[index] = ShaderRow {
                wear: match row.skin {
                    // The stock blade, worn by taking the skin off.
                    None => (!row.worn).then_some(""),
                    Some(skin) if row.owned && unlocks => Some(if row.worn { "" } else { skin.id }),
                    Some(_) => None,
                },
                worn: row.worn,
            };
        }
        if self.grid {
            let first_line = self.grid_first;
            let shown = first_line * GRID_COLUMNS..(first_line + GRID_LINES_SHOWN) * GRID_COLUMNS;
            for (index, row) in rows.iter().enumerate() {
                if shown.contains(&index) {
                    let rect = card_rect(index, first_line);
                    self.grid_card(frame, *row, index, rect, inputs, seconds);
                }
            }
            self.grid_scroll_bar(frame, first_line, rows.len().div_ceil(GRID_COLUMNS));
        } else {
            for (index, row) in rows.iter().enumerate() {
                if (first..first + RACK_SHOWN).contains(&index) {
                    let y = RACK_TOP + (index - first) as f32 * ROW_STEP;
                    self.rack_row(frame, *row, index, y, inputs, seconds);
                }
            }
            self.rack_scroll_bar(frame, first, rows.len());
        }
        if let Some(row) = rows.get(self.shader).copied() {
            self.shader_beside_model(frame, row, inputs, seconds);
        }
        let _ = s;
    }

    /// The filter under the headline: a chip for All and one for each tier (in its tier's
    /// colour), the owned-only chip and the search field.
    fn filter_bar(&mut self, frame: &Frame) {
        let s = frame.s;
        let y = FILTER_Y;
        let mut left = LEFT_X;
        let chips: [(u16, String, sjk_ui::Color, bool, f32); 7] = [
            (
                FILTER_TIER_BASE,
                "All".to_owned(),
                color::HOLO,
                self.shader_filter.tier.is_none(),
                56.0,
            ),
            chip_of(0, &self.shader_filter, 90.0),
            chip_of(1, &self.shader_filter, 104.0),
            chip_of(2, &self.shader_filter, 70.0),
            chip_of(3, &self.shader_filter, 100.0),
            chip_of(4, &self.shader_filter, 96.0),
            (
                FILTER_OWNED_TOKEN,
                "Owned".to_owned(),
                color::GOLD,
                self.shader_filter.owned,
                84.0,
            ),
        ];
        for (token, label, colour, lit, width) in chips {
            let rect = frame.rect(left, y, width, FILTER_HEIGHT);
            let hovered = self.ui.token_hovered(token);
            if lit || hovered {
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect,
                    radius: FILTER_HEIGHT * 0.5 * s,
                    color: color::alpha(colour, if lit { 0.2 } else { 0.09 }),
                });
            }
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: FILTER_HEIGHT * 0.5 * s,
                width: 1.2 * s,
                color: color::alpha(colour, if lit { 1.0 } else { 0.4 }),
            });
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{label}"),
                frame.rect(left, y + 4.0, width, FILTER_HEIGHT - 8.0),
                14.0 * s,
                if lit { color::TEXT } else { color::MUTED },
                FontWeight::Semibold,
                TextAlign::Center,
            );
            self.ui.hit_region(token, rect);
            left += width + 8.0;
        }
        // The search field takes what is left of the rack's width.
        let width = (LEFT_X - 12.0 + RACK_WIDTH - left).max(120.0);
        let rect = frame.rect(left, y, width, FILTER_HEIGHT);
        let focused = self.search_focus;
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius: 8.0 * s,
            color: color::alpha(color::SPACE, if focused { 0.8 } else { 0.5 }),
        });
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 8.0 * s,
            width: 1.2 * s,
            color: if focused {
                color::GOLD
            } else {
                color::alpha(color::HOLO, 0.3)
            },
        });
        let caret = (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let shown = if focused {
            format!(
                "{}{}",
                self.shader_filter.query,
                if caret { "|" } else { "" }
            )
        } else if self.shader_filter.query.is_empty() {
            "Search  /".to_owned()
        } else {
            self.shader_filter.query.clone()
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{shown}"),
            frame.rect(left + 12.0, y + 4.0, width - 24.0, FILTER_HEIGHT - 8.0),
            14.0 * s,
            if focused || !self.shader_filter.query.is_empty() {
                color::TEXT
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui.hit_region(SEARCH_TOKEN, rect);
    }

    /// The rack's scroll bar at its right, when it holds more rows than it shows.
    fn rack_scroll_bar(&mut self, frame: &Frame, first: usize, rows: usize) {
        if rows <= RACK_SHOWN {
            return;
        }
        let s = frame.s;
        let x = LEFT_X - 12.0 + RACK_WIDTH + 8.0;
        let height = RACK_SHOWN as f32 * ROW_STEP - (ROW_STEP - ROW_HEIGHT);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, RACK_TOP, 4.0, height),
            radius: 2.0 * s,
            color: color::alpha(color::HOLO, 0.14),
        });
        let thumb = height * RACK_SHOWN as f32 / rows as f32;
        let top = RACK_TOP + (height - thumb) * first as f32 / (rows - RACK_SHOWN) as f32;
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, top, 4.0, thumb),
            radius: 2.0 * s,
            color: color::GOLD,
        });
    }

    /// The grid's scroll bar at its right, when it holds more lines than it shows.
    fn grid_scroll_bar(&mut self, frame: &Frame, first: usize, lines: usize) {
        if lines <= GRID_LINES_SHOWN {
            return;
        }
        let s = frame.s;
        let x = LEFT_X - 12.0 + RACK_WIDTH + 8.0;
        let height = GRID_LINES_SHOWN as f32 * (CARD[1] + CARD_GAP[1]) - CARD_GAP[1];
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, RACK_TOP, 4.0, height),
            radius: 2.0 * s,
            color: color::alpha(color::HOLO, 0.14),
        });
        let thumb = height * GRID_LINES_SHOWN as f32 / lines as f32;
        let top = RACK_TOP + (height - thumb) * first as f32 / (lines - GRID_LINES_SHOWN) as f32;
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, top, 4.0, thumb),
            radius: 2.0 * s,
            color: color::GOLD,
        });
    }

    /// The two view switches left of the kinds: the rack (three bars) and the grid
    /// (four squares), the one on show lit.
    fn view_switches(&mut self, frame: &Frame) {
        let s = frame.s;
        let y = LEAD_Y - 2.0;
        for (index, (token, grid)) in [(LIST_VIEW_TOKEN, false), (GRID_VIEW_TOKEN, true)]
            .into_iter()
            .enumerate()
        {
            let x = SWITCH_X + index as f32 * (SWITCH + 8.0);
            let lit = self.grid == grid;
            let hovered = self.ui.token_hovered(token);
            let rect = frame.rect(x, y, SWITCH, SWITCH);
            if lit || hovered {
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect,
                    radius: 9.0 * s,
                    color: color::alpha(color::GOLD, if lit { 0.16 } else { 0.08 }),
                });
            }
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: 9.0 * s,
                width: 1.2 * s,
                color: if lit {
                    color::GOLD
                } else {
                    color::alpha(color::HOLO, 0.3)
                },
            });
            let ink = if lit {
                color::GOLD_BRIGHT
            } else {
                color::QUIET
            };
            let marks: &[[f32; 4]] = if grid {
                &[
                    [10.0, 10.0, 7.0, 7.0],
                    [19.0, 10.0, 7.0, 7.0],
                    [10.0, 19.0, 7.0, 7.0],
                    [19.0, 19.0, 7.0, 7.0],
                ]
            } else {
                &[
                    [10.0, 10.0, 16.0, 3.0],
                    [10.0, 16.5, 16.0, 3.0],
                    [10.0, 23.0, 16.0, 3.0],
                ]
            };
            for [mx, my, width, height] in marks {
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(x + mx, y + my, *width, *height),
                    radius: 1.5 * s,
                    color: ink,
                });
            }
            self.ui.hit_region(token, rect);
        }
    }

    /// Card `index` of the grid in `rect` (frame pixels): its tier's frame and a band
    /// of its colour along the top, the blade alive (or grey and locked), the chroma
    /// mark, its name, its tier and its state.
    fn grid_card(
        &mut self,
        frame: &Frame,
        row: Row,
        index: usize,
        rect: [f32; 4],
        inputs: &Inputs<'_>,
        seconds: f32,
    ) {
        let s = frame.s;
        let [x, y, width, height] = rect;
        let chosen = index == self.shader;
        // The chosen card lifts off the grid, a shadow under it and a white outline
        // round it: its own mark, apart from any tier's colour (Mythical's is gold).
        let y = if chosen { y - CHOSEN_LIFT } else { y };
        let token = SHADER_BASE + index as u16;
        let hovered = self.ui.token_hovered(token);
        let area = frame.rect(x, y, width, height);
        if chosen {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x + 2.0, y + CHOSEN_LIFT + 4.0, width - 4.0, height),
                radius: 12.0 * s,
                color: color::alpha(color::SPACE, 0.55),
            });
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: area,
            radius: 12.0 * s,
            color: color::alpha(color::SPACE, if chosen || hovered { 0.78 } else { 0.6 }),
        });
        if let Some(tier) = row.tier() {
            // The tier's band along the top, and its glow under it.
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x + 12.0, y, width - 24.0, 4.0),
                radius: 2.0 * s,
                color: color::alpha(tier.colour(), if row.owned { 1.0 } else { 0.5 }),
            });
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x + 4.0, y + 4.0, width - 8.0, 30.0),
                radius: 10.0 * s,
                color: color::alpha(tier.colour(), if row.owned { 0.1 } else { 0.05 }),
            });
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect: area,
            radius: 12.0 * s,
            width: if chosen { 1.8 } else { 1.2 } * s,
            color: row.frame_colour(),
        });
        if chosen {
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect: frame.rect(x - 3.0, y - 3.0, width + 6.0, height + 6.0),
                radius: 15.0 * s,
                width: 2.0 * s,
                color: color::alpha(CHOSEN_OUTLINE, 0.95),
            });
        }
        let swatch_rect = [x + 8.0, y + 22.0, width - 16.0, 52.0];
        match row.skin {
            None => swatch::small_stock(self.ui.draw_list_mut(), frame, swatch_rect, inputs.stock),
            Some(skin) => {
                let look = self.skins.get(skin.id);
                let _ = swatch::small_swatch(
                    self.ui.draw_list_mut(),
                    frame,
                    swatch_rect,
                    look,
                    row.owned,
                    seconds,
                    swatch::chroma_turn(look, swatch::rgb_bytes(inputs.stock)),
                );
            }
        }
        let chroma = row.skin.is_some_and(|skin| skin.chroma);
        if chroma {
            swatch::chroma_mark(
                self.ui.draw_list_mut(),
                frame,
                x + width - 30.0,
                y + 89.0,
                20.0,
                if row.owned { 1.0 } else { 0.55 },
            );
        }
        let name_width = width - 20.0 - if chroma { 26.0 } else { 0.0 };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", card_name(row.name())),
            frame.rect(x + 10.0, y + 86.0, name_width, 26.0),
            21.0 * s,
            match (chosen, row.owned) {
                (true, _) => CHOSEN_OUTLINE,
                (false, true) => color::TEXT,
                (false, false) => color::MUTED,
            },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let (tier_text, tier_colour) = match row.tier() {
            Some(tier) => (
                tier.label(),
                color::alpha(tier.colour(), if row.owned { 1.0 } else { 0.6 }),
            ),
            None => ("Your colour", color::QUIET),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{tier_text}"),
            frame.rect(x + 10.0, y + 122.0, width - 20.0, 20.0),
            14.0 * s,
            tier_colour,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let state = match (row.skin, row.worn, row.owned) {
            (_, true, _) => held_words("Worn", row.count),
            (None, false, _) => String::new(),
            (Some(_), false, true) => held_words("Yours", row.count),
            (Some(_), false, false) => "Locked".to_owned(),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{state}"),
            frame.rect(x + 10.0, y + 152.0, width - 20.0, 20.0),
            14.0 * s,
            if row.worn || (row.owned && row.skin.is_some()) {
                color::GOLD
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        if let Some(tier) = row.tier() {
            rarity_fx::draw(
                self.ui.draw_list_mut(),
                rarity_fx::Item::of(tier, row.owned, rarity_fx::Size::Full),
                area,
                12.0 * s,
                s,
                seconds,
            );
        }
        self.ui.hit_region(token, area);
    }

    /// The kinds of shader along the lead: Saber, and Body to come.
    fn kinds(&mut self, frame: &Frame, owned: usize) {
        let s = frame.s;
        let x = LEFT_X + 560.0;
        let y = LEAD_Y - 2.0;
        let pills: [(&str, String, bool); 2] = [
            ("Saber", format!("{owned}/{}", unlockables::ALL.len()), true),
            ("Body", "soon".to_owned(), false),
        ];
        let mut left = x;
        for (name, count, lit) in pills {
            let width = 132.0;
            let rect = frame.rect(left, y, width, 36.0);
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: 18.0 * s,
                width: 1.2 * s,
                color: if lit {
                    color::GOLD
                } else {
                    color::alpha(color::HOLO, 0.25)
                },
            });
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{name}"),
                frame.rect(left + 18.0, y + 5.0, 70.0, 26.0),
                19.0 * s,
                if lit {
                    color::GOLD_BRIGHT
                } else {
                    color::QUIET
                },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{count}"),
                frame.rect(left + 70.0, y + 7.0, width - 84.0, 22.0),
                14.0 * s,
                if lit { color::GOLD } else { color::QUIET },
                FontWeight::Regular,
                TextAlign::End,
            );
            left += width + 12.0;
        }
    }

    /// Row `index` of the rack at `y`.
    fn rack_row(
        &mut self,
        frame: &Frame,
        row: Row,
        index: usize,
        y: f32,
        inputs: &Inputs<'_>,
        seconds: f32,
    ) {
        let s = frame.s;
        let chosen = index == self.shader;
        let token = SHADER_BASE + index as u16;
        let hovered = self.ui.token_hovered(token);
        if chosen || hovered {
            kit::band(
                &mut self.ui,
                frame,
                [LEFT_X - 12.0, y, RACK_WIDTH, ROW_HEIGHT],
            );
        }
        if chosen {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(LEFT_X - 12.0, y + 14.0, 3.0, ROW_HEIGHT - 28.0),
                radius: 1.5 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        let rect = [
            LEFT_X + 6.0,
            y + (ROW_HEIGHT - SWATCH[1]) * 0.5,
            SWATCH[0],
            SWATCH[1],
        ];
        match row.skin {
            None => swatch::small_stock(self.ui.draw_list_mut(), frame, rect, inputs.stock),
            Some(skin) => {
                let look = self.skins.get(skin.id);
                let _ = swatch::small_swatch(
                    self.ui.draw_list_mut(),
                    frame,
                    rect,
                    look,
                    row.owned,
                    seconds,
                    swatch::chroma_turn(look, swatch::rgb_bytes(inputs.stock)),
                );
            }
        }
        // The swatch framed in its tier's colour.
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect: frame.rect(rect[0], rect[1], rect[2], rect[3]),
            radius: 10.0 * s,
            width: 1.4 * s,
            color: row.frame_colour(),
        });
        if let Some(tier) = row.tier() {
            rarity_fx::draw(
                self.ui.draw_list_mut(),
                rarity_fx::Item::of(tier, row.owned, rarity_fx::Size::Small),
                frame.rect(rect[0], rect[1], rect[2], rect[3]),
                10.0 * s,
                s,
                seconds,
            );
        }
        let chroma = row.skin.is_some_and(|skin| skin.chroma);
        let (mark, name, status) = rack_words(y, chroma);
        if let Some(tier) = row.tier() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{} {}", tier.symbol(), tier.label()),
                frame.rect(status[0], status[1], status[2], status[3]),
                15.0 * s,
                color::alpha(tier.colour(), if row.owned { 1.0 } else { 0.6 }),
                FontWeight::Semibold,
                TextAlign::End,
            );
        }
        if chroma {
            swatch::chroma_mark(
                self.ui.draw_list_mut(),
                frame,
                mark[0],
                mark[1],
                mark[2],
                if row.owned { 1.0 } else { 0.55 },
            );
        }
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", row.name()),
            frame.rect(name[0], name[1], name[2], name[3]),
            28.0 * s,
            match (chosen, row.owned) {
                (true, _) => color::GOLD_BRIGHT,
                (false, true) => color::TEXT,
                (false, false) => color::MUTED,
            },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", row.status()),
            frame.rect(status[0], status[1], status[2], status[3]),
            15.0 * s,
            if row.worn || (row.owned && row.skin.is_some()) {
                color::GOLD
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.ui
            .hit_region(token, frame.rect(LEFT_X - 12.0, y, RACK_WIDTH, ROW_HEIGHT));
    }

    /// What the chosen row is, beside the model: its kind, name and state, what it is,
    /// since when it is the player's (with the team's note) or how to get it, and Equip
    /// or Unequip. Without a model it shows the swatch, larger.
    fn shader_beside_model(&mut self, frame: &Frame, row: Row, inputs: &Inputs<'_>, seconds: f32) {
        let s = frame.s;
        let x = STAGE_TEXT_X;
        let width = STAGE_TEXT_WIDTH;
        if self.backstage == Backstage::None {
            let rect = [x, 300.0, width, 118.0];
            match row.skin {
                None => swatch::small_stock(self.ui.draw_list_mut(), frame, rect, inputs.stock),
                Some(skin) => {
                    let look = self.skins.get(skin.id);
                    let _ = swatch::small_swatch(
                        self.ui.draw_list_mut(),
                        frame,
                        rect,
                        look,
                        row.owned,
                        seconds,
                        swatch::chroma_turn(look, swatch::rgb_bytes(inputs.stock)),
                    );
                }
            }
        } else if !row.owned {
            let rect = frame.rect(x, 232.0, 360.0, 34.0);
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect,
                radius: 17.0 * s,
                color: color::alpha(color::SPACE, 0.6),
            });
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: 17.0 * s,
                width: 1.2 * s,
                color: color::alpha(color::HOLO, 0.6),
            });
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("Preview on your saber: not yours yet"),
                frame.rect(x, 238.0, 360.0, 22.0),
                15.0 * s,
                color::HOLO,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
        let mut y = 618.0;
        // Its tier first, a pill in the tier's colour, alive as its tier is.
        if let Some(tier) = row.tier() {
            let label = tier.label();
            let pill_width = crate::text::display_width(label, 17.0) + 34.0;
            let rect = frame.rect(x, y - 44.0, pill_width, PILL_HEIGHT);
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect,
                radius: PILL_HEIGHT * 0.5 * s,
                color: color::alpha(tier.colour(), 0.16),
            });
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: PILL_HEIGHT * 0.5 * s,
                width: 1.2 * s,
                color: tier.colour(),
            });
            rarity_fx::draw(
                self.ui.draw_list_mut(),
                rarity_fx::Item::of(tier, row.owned, rarity_fx::Size::Small),
                rect,
                PILL_HEIGHT * 0.5 * s,
                s,
                seconds,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{label}"),
                frame.rect(x, y - 41.0, pill_width, 24.0),
                17.0 * s,
                tier.colour(),
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
        // A chroma says so first, with its mark: it takes the player's saber colour.
        let chroma = row.skin.is_some_and(|skin| skin.chroma);
        let kind_x = if chroma {
            swatch::chroma_mark(self.ui.draw_list_mut(), frame, x, y - 1.0, 22.0, 1.0);
            x + 22.0 + MARK_GAP
        } else {
            x
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "{}",
                match (row.skin.is_some(), chroma) {
                    (true, true) => "Chroma saber shader: takes your saber colour",
                    (true, false) => "Saber shader",
                    (false, _) => "Your saber",
                }
            ),
            frame.rect(kind_x, y, x + width - kind_x, 22.0),
            16.0 * s,
            color::HOLO,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if let Some(skin) = row.skin {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", crate::editions::name(skin.edition)),
                frame.rect(x, y, width, 22.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Semibold,
                TextAlign::End,
            );
        }
        y += 26.0;
        let name = row.name();
        let lit = row.owned && row.skin.is_some();
        let tag = match (row.skin, row.worn, row.owned) {
            (None, _, _) => None,
            (Some(_), true, _) => Some(held_words("Worn", row.count)),
            (Some(_), false, true) => Some(held_words("Yours", row.count)),
            (Some(_), false, false) => Some("Locked".to_owned()),
        };
        // The name never runs under the state: it ends short of the pill, ellipsised,
        // whatever face and size the text is drawn in.
        let (name_rect, tag_rect) = match tag {
            Some(tag) => {
                let (name, pill) =
                    title_rects(x, y, width, crate::text::display_width(&tag, 15.0) + 26.0);
                (name, Some((tag, pill)))
            }
            None => ([x, y, width, 62.0], None),
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{name}"),
            frame.rect(name_rect[0], name_rect[1], name_rect[2], name_rect[3]),
            56.0 * s,
            if lit { color::GOLD_BRIGHT } else { color::TEXT },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if let Some((tag, [tag_x, tag_y, tag_width, tag_height])) = tag_rect {
            let rect = frame.rect(tag_x, tag_y, tag_width, tag_height);
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect,
                radius: 13.0 * s,
                width: 1.2 * s,
                color: if lit { color::GOLD } else { color::QUIET },
            });
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{tag}"),
                frame.rect(tag_x, tag_y + 2.0, tag_width, 22.0),
                15.0 * s,
                if lit { color::GOLD } else { color::QUIET },
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
        y += 72.0;
        // Its saber smith signs it at the right, beside the description (which wraps
        // well short of it), so the card keeps its height.
        if let Some(skin) = row.skin {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("Saber smith: {}", skin.smith),
                frame.rect(x, y, width, 24.0),
                16.0 * s,
                color::HOLO,
                FontWeight::Semibold,
                TextAlign::End,
            );
        }
        let description = row.skin.map_or(
            "Your blade in the colour you pick on Profile, Saber.",
            |skin| skin.description,
        );
        y = left_lines(
            &mut self.ui,
            frame,
            description,
            [x, y, width],
            (52, 3),
            18.0,
            color::TEXT,
            TextAlign::Start,
        );
        y += 6.0;
        let grant = row.skin.and_then(|skin| inputs.holdings.unlock(skin.id));
        let (line, colour) = match (row.skin, grant) {
            (None, _) if row.worn => ("What you hold now: no shader on.".to_owned(), color::MUTED),
            (None, _) => ("Wearing it takes your shader off.".to_owned(), color::MUTED),
            (Some(_), Some(grant)) => (owned_line(grant), color::GOLD),
            (Some(skin), None) => (format!("How to get it: {}", skin.how_to_get), color::MUTED),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(x, y, width, 24.0),
            16.0 * s,
            colour,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        y += 26.0;
        if let Some(note) = grant
            .map(|grant| grant.note.as_str())
            .filter(|note| !note.is_empty() && *note != crate::debug_collectibles::NOTE)
        {
            let quoted = format!("\u{201c}{note}\u{201d}");
            y = left_lines(
                &mut self.ui,
                frame,
                &quoted,
                [x, y, width],
                (56, 1),
                16.0,
                color::MUTED,
                TextAlign::Start,
            );
        }
        let wear = self.shader_rows.get(self.shader).and_then(|row| row.wear);
        if let Some(wear) = wear {
            let label = if wear.is_empty() && row.skin.is_some() {
                "Unequip"
            } else if row.skin.is_none() {
                "Wear the stock blade"
            } else {
                "Equip"
            };
            let button_width = if row.skin.is_none() { 250.0 } else { 160.0 };
            let button_y = (y + 10.0).min(946.0);
            kit::button(
                &mut self.ui,
                frame,
                [x, button_y, button_width, 44.0],
                label,
                !(wear.is_empty() && row.skin.is_some()),
                true,
                false,
                WEAR_TOKEN,
            );
            y = button_y + 44.0;
        }
        self.shader_options.clear();
        if let Some(skin) = row.skin.filter(|_| row.owned) {
            self.shader_parts(frame, skin.id, y + 16.0, inputs);
        }
    }

    /// The chosen shader's parts its wearer may switch off, under its words from `y`:
    /// a tick each in two columns (three past four), named as its file names them,
    /// ticked while on; a click or its number key switches it, on the model at once and,
    /// worn, for every SJK player.
    fn shader_parts(&mut self, frame: &Frame, skin: &'static str, y: f32, inputs: &Inputs<'_>) {
        let Some(look) = self.skins.get(skin) else {
            return;
        };
        let options = look.options();
        if options.is_empty() {
            return;
        }
        let s = frame.s;
        let x = STAGE_TEXT_X;
        let columns = if options.len() > 4 { 3 } else { 2 };
        let column = STAGE_TEXT_WIDTH / columns as f32;
        // Never under the keys' line: the rows close up when room is short.
        let rows = options.len().div_ceil(columns);
        let step = ((view::KEYS_Y - 22.0 - (y + 28.0)) / rows as f32).clamp(28.0, 38.0);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Parts"),
            frame.rect(x, y, STAGE_TEXT_WIDTH, 22.0),
            16.0 * s,
            color::HOLO,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let shown: Vec<(&'static str, String, bool, String)> = options
            .iter()
            .map(|option| {
                let on = crate::saber_skin_options::is_on(inputs.options, skin, &option.id);
                (skin, option.id.clone(), on, option.name.clone())
            })
            .collect();
        for (index, (_, _, on, name)) in shown.iter().enumerate() {
            let token = OPTION_BASE + index as u16;
            let left = x + (index % columns) as f32 * column;
            let middle = y + 28.0 + (index / columns) as f32 * step + step * 0.5;
            let hovered = self.ui.token_hovered(token);
            let after = kit::tick(&mut self.ui, frame, left, middle, *on, hovered);
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{name}"),
                frame.rect(
                    after + 10.0,
                    middle - 11.0,
                    column - kit::TICK_BOX - 18.0,
                    22.0,
                ),
                16.0 * s,
                if *on { color::TEXT } else { color::MUTED },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            self.ui.hit_region(
                token,
                frame.rect(left - 4.0, middle - step * 0.5, column - 8.0, step),
            );
        }
        self.shader_options = shown
            .into_iter()
            .map(|(skin, option, on, _)| (skin, option, on))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::sjk::Frame;

    #[test]
    fn the_owned_line_names_the_medal_that_brings_a_shader() {
        let grant = |medal: Option<&str>| sjk_identity::Unlock {
            id: "saber_glitch".to_owned(),
            granted: 0,
            note: String::new(),
            count: 1,
            medal: medal.map(str::to_owned),
        };
        assert_eq!(owned_line(&grant(None)), "Yours, from the SJK team");
        assert_eq!(
            owned_line(&grant(Some("bug_hunter"))),
            "Yours, with your Bug Hunter medal"
        );
        assert_eq!(
            owned_line(&grant(Some("from_the_future"))),
            "Yours, from the SJK team"
        );
        let dated = sjk_identity::Unlock {
            granted: 1_791_000_000,
            ..grant(Some("early_tester"))
        };
        assert!(owned_line(&dated).starts_with("Yours since "));
        let twice = sjk_identity::Unlock {
            count: 3,
            ..grant(None)
        };
        assert_eq!(owned_line(&twice), "Yours, from the SJK team, 3 held");
        assert_eq!(held_words("Yours", 1), "Yours");
        assert_eq!(held_words("Worn", 2), "Worn x2");
        assert!(owned_line(&dated).ends_with(", with your Early Tester medal"));
    }

    /// Whether two window rectangles share any area.
    fn overlap(a: sjk_ui::Rect, b: sjk_ui::Rect) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    #[test]
    fn names_never_run_under_the_state_or_the_chroma_mark() {
        let longest = unlockables::blade_skins()
            .map(|skin| skin.name)
            .max_by(|a, b| {
                crate::text::display_width(a, 56.0).total_cmp(&crate::text::display_width(b, 56.0))
            })
            .unwrap();
        for viewport in [
            [1920.0, 1080.0],
            [3840.0, 2160.0],
            [1440.0, 1080.0],
            [2560.0, 1080.0],
        ] {
            let frame = Frame::new(viewport);
            let window = |r: [f32; 4]| frame.rect(r[0], r[1], r[2], r[3]);
            for chroma in [false, true] {
                let (mark, name, status) = rack_words(RACK_TOP, chroma);
                let (mark, name, status) = (window(mark), window(name), window(status));
                assert!(!overlap(name, status), "{viewport:?}");
                if chroma {
                    assert!(
                        !overlap(mark, name) && !overlap(mark, status),
                        "{viewport:?}"
                    );
                }
                // Inside the rack, with room for the longest name at its size.
                let rack_right = frame.rect(LEFT_X - 12.0 + RACK_WIDTH, 0.0, 0.0, 0.0).x;
                assert!(name.x + name.width <= rack_right);
                assert!(name.width >= crate::text::display_width(longest, 28.0) * frame.s);
            }
            for tag in ["Worn", "Yours", "Locked"] {
                let (name, pill) = title_rects(
                    STAGE_TEXT_X,
                    644.0,
                    STAGE_TEXT_WIDTH,
                    crate::text::display_width(tag, 15.0) + 26.0,
                );
                let (name, pill) = (window(name), window(pill));
                assert!(!overlap(name, pill), "{viewport:?} {tag}");
                assert!(
                    pill.x + pill.width
                        <= window([STAGE_TEXT_X + STAGE_TEXT_WIDTH, 0.0, 0.0, 0.0]).x + 0.01
                );
                assert!(
                    name.width > 200.0 * frame.s,
                    "{viewport:?}: room for a name"
                );
            }
        }
    }

    #[test]
    fn the_rack_scrolls_only_to_keep_the_chosen_row_in_view() {
        let rows = SHADER_ROWS;
        assert!(rows > RACK_SHOWN, "more shaders than the rack shows");
        // Within the window the rack holds still (a pointer moving over the rows
        // chooses them without scrolling them away).
        assert_eq!(rack_window(0, RACK_SHOWN - 1, rows), 0);
        assert_eq!(rack_window(3, 4, rows), 3);
        // Past either end it moves just enough.
        assert_eq!(rack_window(0, RACK_SHOWN, rows), 1);
        assert_eq!(rack_window(5, 2, rows), 2);
        assert_eq!(rack_window(0, rows - 1, rows), rows - RACK_SHOWN);
        assert_eq!(rack_window(40, 0, rows), 0);
    }

    #[test]
    fn the_wheel_scrolls_the_rack_and_keeps_the_chosen_row_shown() {
        let mut panel = Panel::new();
        panel.open(Tab::Shaders, true, true, ReturnTarget::MainMenu);
        // As the first frame leaves it, the stock blade worn.
        panel.shader = 0;
        let wheel = |panel: &mut Panel, y: f32| {
            let _ = panel.handle_pointer(
                InputEvent::PointerWheel {
                    position: sjk_ui::Vec2::new(500.0, 500.0),
                    delta: sjk_ui::Vec2::new(0.0, y),
                },
                false,
            );
        };
        for _ in 0..3 {
            wheel(&mut panel, -1.0);
        }
        assert_eq!(panel.shader_first, 3);
        assert_eq!(panel.shader, 3, "the chosen row follows into view");
        for _ in 0..60 {
            wheel(&mut panel, -1.0);
        }
        assert_eq!(panel.shader_first, SHADER_ROWS - RACK_SHOWN);
        wheel(&mut panel, 1.0);
        assert_eq!(panel.shader_first, SHADER_ROWS - RACK_SHOWN - 1);
    }

    /// Every row and the words beside the model fit the canvas over the keys, for an
    /// owned and worn skin, a locked one and the stock blade, with and without a model,
    /// at 1080 lines, 4K, 4:3 and 21:9.
    #[test]
    fn the_rack_and_its_words_fit_over_the_keys() {
        let sun = [sjk_identity::Unlock {
            id: "saber_sun".into(),
            granted: 1_791_336_225,
            note: "a long note from the team ".repeat(8),
            count: 1,
            medal: None,
        }];
        let font = crate::text::load_modern(1.0, None).expect("Inter").font;
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [2_560.0, 1_080.0],
        ] {
            for backstage in [Backstage::None, Backstage::World { head: None }] {
                for row in 0..SHADER_ROWS {
                    let inputs = super::super::tests::inputs(
                        unlockables::Holdings::Known(&sun),
                        "saber_sun",
                    );
                    let mut panel = Panel::new();
                    panel.open(Tab::Shaders, true, true, ReturnTarget::MainMenu);
                    panel.set_backstage(backstage);
                    panel.shader = row;
                    panel.build(&inputs, &font, viewport);
                    assert!(!panel.ui.overflowed(), "{viewport:?}");
                    let frame = Frame::new(viewport);
                    let keys = frame.point(0.0, super::super::view::KEYS_Y)[1];
                    for command in panel.ui.draw_list().commands() {
                        if let DrawCommand::Text { rect, .. } = command {
                            assert!(rect.bottom() <= keys || rect.y >= keys, "{row} {rect:?}");
                            assert!(rect.right() <= viewport[0] + 1.0, "{rect:?}");
                        }
                    }
                    // The rows in view answer the pointer, above the keys; the rest
                    // are scrolled off.
                    let first = panel.shader_first;
                    assert!((first..first + RACK_SHOWN).contains(&row), "{row}");
                    for index in 0..SHADER_ROWS {
                        let area = panel.ui.rect_for(SHADER_BASE + index as u16);
                        let shown = (first..first + RACK_SHOWN).contains(&index);
                        assert_eq!(area.is_some(), shown, "{index}");
                        if let Some(area) = area {
                            assert!(area.bottom() <= keys, "{index} {viewport:?}");
                        }
                    }
                }
            }
        }
    }
}

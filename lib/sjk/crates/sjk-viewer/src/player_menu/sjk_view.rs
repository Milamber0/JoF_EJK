//! The player screen in the SJK UI (`docs/sjk-ui.md`, Character): the
//! player's name as the screen's title, the three pages as tabs under it,
//! their rows in a column on the left drawn with the SJK UI's kit, and the
//! model standing on the menu map's stage on the right, a caption beside it as
//! in a gallery. It is the player screen's shared state with a view of its own:
//! the rows, the grid's tiles, the tabs and the back key answer to the
//! screen's tokens, so its keys and pointer work unchanged. Each row registers its
//! control first and then the whole row, so a token's rectangle is its
//! control's ([`MenuCanvas::rect_for`]); a click on a row outside its control
//! only chooses the row (`pointer.rs`).
//!
//! Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]).

use super::force::{GT_TEAM, NextLevel, POWER_NAMES, POWER_NOTES};
use super::force_icons::{power_texture, side_texture};
use super::rows::{
    CharacterRow, FORCE_APPLY_ROW, FORCE_DISCARD_ROW, FORCE_POWER_ROW, FORCE_RESET_ROW,
    FORCE_SIDE_ROW, SaberRow, saber_color,
};
use super::saber::{PALETTE, SaberStyle};
use super::*;
use crate::menu::sjk::{
    Frame, TextTarget, color, fade, fade_across, key_hint, key_hint_width, kit, text, wrap,
};
use crate::menu_widgets::{BACK_TOKEN, TAB_BASE, TextFamily};
use sjk_client::{ForcePower, ForceSide, LegacyCatalogStatus};
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The pages' names on their tabs.
const TABS: [&str; 3] = ["Character", "Saber", "Force"];

/// The top bar's middle line; the tabs stand under it ([`crate::profile_hub::tabs`]).
const BAR_Y: f32 = 87.0;
/// The form's column, its rows' top and height, where a row's name starts
/// and where its control ends, and a control's usual width.
const COLUMN_X: f32 = 96.0;
const COLUMN_WIDTH: f32 = 664.0;
const ROWS_TOP: f32 = 214.0;
const ROW: f32 = 52.0;
const LABEL_X: f32 = COLUMN_X + 22.0;
const CONTROL_RIGHT: f32 = COLUMN_X + COLUMN_WIDTH - 10.0;
const CONTROL_WIDTH: f32 = 330.0;
/// A control's height inside its row.
const CONTROL: f32 = 38.0;
/// The rows end above this; the keys' line below it.
const BOTTOM: f32 = 960.0;
/// The Character page's Apply button: its height, and the room it takes under the rows.
const APPLY_HEIGHT: f32 = 46.0;
const APPLY_LINE: f32 = APPLY_HEIGHT + 12.0;
/// The category strip's height.
const STRIP: f32 = 46.0;
const KEYS_Y: f32 = 992.0;
/// A slider's track and its number.
const TRACK_WIDTH: f32 = 230.0;
const NUMBER_WIDTH: f32 = 46.0;
/// The Force page: the side cards' height, a power cell's, the gap between
/// the two columns.
const SIDE_HEIGHT: f32 = 64.0;
const CELL: f32 = 44.0;
const GUTTER: f32 = 16.0;
/// The points meter's line, a group's sub-heading and the gap after a group.
const METER_Y: f32 = 250.0;
const GROUP_HEADING: f32 = 34.0;
const GROUP_GAP: f32 = 14.0;
/// A level's round mark (classic+'s `forcecircle`/`forcestar`) and the gap
/// between two on their channel.
const DISC: f32 = 30.0;
const DISC_GAP: f32 = 12.0;
/// How long the spark takes along a hovered level's channel, and how long
/// the ring a bought level sends out lasts.
const SPARK_SECONDS: f64 = 0.9;
const BURST_SECONDS: f32 = 0.55;
/// The box at the bottom right showing the power under the pointer.
const POWER_BOX: Area = [1_384.0, 640.0, 440.0, 300.0];
/// Opened from a game, where the menu map's stage is not: where the model's
/// live preview stands, right of the form: square, so a raised blade stays in
/// it (the preview camera frames the body's height and widens with the
/// area), and the line its feet are on.
const MODEL_AREA: Area = [780.0, 110.0, 860.0, 840.0];
/// The room the preview camera leaves round the body (retail's framing is 1).
const MODEL_ROOM: f32 = 1.35;
/// Where the feet fall in the area: the body's 64 units from 24 below the
/// framed middle's origin, in a frame `MODEL_ROOM` times retail's 80.
const MODEL_FEET_Y: f32 = MODEL_AREA[1] + MODEL_AREA[3] * (0.5 + 32.0 / (80.0 * MODEL_ROOM));
/// Level `l` (1 to 3) of power `p` answers to `LEVEL_BASE + p * 3 + l - 1`.
const LEVEL_BASE: u16 = 1_000;
/// The saber styles as the Style row's buttons offer them.
const STYLES: [SaberStyle; 3] = [SaberStyle::Single, SaberStyle::Staff, SaberStyle::Dual];
/// Style button `i` answers to `STYLE_BASE + i`.
const STYLE_BASE: u16 = 1_080;
/// The hilt lists' wheel areas: the first saber's, then the second's.
const HILT_SCROLL: u16 = 1_090;
/// Hilt `i` of the first saber's list answers to `HILT_BASE + i`, of the
/// second's to `HILT_BASE + HILT_STRIDE + i`.
const HILT_BASE: u16 = 1_100;
const HILT_STRIDE: u16 = 300;
/// A hilt list's sub-heading, its rows' height and how many show: six of two
/// columns for Single and Staff, four for each of Dual's two lists side by side,
/// which leave room for both blades.
const HILT_HEAD: f32 = 32.0;
const HILT_ROW: f32 = 34.0;
const HILT_ROWS: usize = 6;
const DUAL_HILT_ROWS: usize = 4;
/// The hilt search's field, at the right of the lists' heading line: beside one
/// list's heading, and beside Dual's "Left hand".
const SEARCH_WIDTH: f32 = 240.0;
const DUAL_SEARCH_WIDTH: f32 = 170.0;
/// The blade choice's swatches (the Collection's, shrunk) and the gap between
/// them, from `SKIN_X` so that the ones shown (at most `SKIN_SHOWN` of the stock
/// blade and every blade skin owned, scrolled to keep the chosen one in view)
/// end at the controls' edge.
const SKIN_HEIGHT: f32 = 44.0;
const SKIN_WIDTH: f32 = SKIN_HEIGHT * 400.0 / 224.0;
const SKIN_GAP: f32 = 6.0;
/// A chroma's mark in its blade swatch's corner.
const CHROMA_MARK: f32 = 14.0;
const SKIN_SHOWN: usize = 6;
const SKIN_X: f32 =
    CONTROL_RIGHT - SKIN_SHOWN as f32 * SKIN_WIDTH - (SKIN_SHOWN - 1) as f32 * SKIN_GAP;
/// Blade choice `i` (0 the stock blade) answers to `SKIN_BASE + i`.
const SKIN_BASE: u16 = 7_300;
/// The blade row's wheel area and its arrows, which browse it a row's width at a time.
const SKIN_SCROLL: u16 = 7_597;
const SKIN_PREV: u16 = 7_598;
const SKIN_NEXT: u16 = 7_599;

/// The first of the blade choices the row shows: the chosen one kept third from
/// the left where it can be, the window never past either end.
fn skin_window(chosen: usize, count: usize) -> usize {
    chosen
        .saturating_sub(2)
        .min(count.saturating_sub(SKIN_SHOWN))
}

/// Hilt lines a list shows for `style`.
fn hilt_rows(style: SaberStyle) -> usize {
    if style == SaberStyle::Dual {
        DUAL_HILT_ROWS
    } else {
        HILT_ROWS
    }
}

/// A hilt list's lines when the retail game's hilts are told from the community's
/// (the rule in `AGENTS.md`): the retail ones under a "Jedi Academy" line, the
/// community's under "Community", each section's hilts `columns` to a line.
#[derive(Clone, Copy, Debug)]
struct HiltLines {
    /// Hilts listed, and how many of them (the first) are the retail game's.
    total: usize,
    retail: usize,
    columns: usize,
    /// Whether the headings show: the game's retail hilts are known.
    headed: bool,
}

impl HiltLines {
    fn retail_lines(self) -> usize {
        self.retail.div_ceil(self.columns)
    }

    /// Lines of the retail section: its heading and its rows.
    fn retail_span(self) -> usize {
        if self.retail == 0 {
            0
        } else {
            usize::from(self.headed) + self.retail_lines()
        }
    }

    /// Lines in all.
    fn lines(self) -> usize {
        let community = self.total - self.retail;
        self.retail_span()
            + if community == 0 {
                0
            } else {
                usize::from(self.headed) + community.div_ceil(self.columns)
            }
    }

    /// Line and column of the `at`th hilt listed.
    fn place(self, at: usize) -> (usize, usize) {
        if at < self.retail {
            (
                usize::from(self.headed) + at / self.columns,
                at % self.columns,
            )
        } else {
            let k = at - self.retail;
            (
                self.retail_span() + usize::from(self.headed) + k / self.columns,
                k % self.columns,
            )
        }
    }

    /// The heading standing on `line`, with the hilts under it.
    fn heading(self, line: usize) -> Option<(&'static str, usize)> {
        if !self.headed {
            return None;
        }
        if self.retail > 0 && line == 0 {
            Some(("Jedi Academy", self.retail))
        } else if self.retail < self.total && line == self.retail_span() {
            Some(("Community", self.total - self.retail))
        } else {
            None
        }
    }
}

/// The pointer token of hilt `index` in the first saber's list, or the
/// second's.
fn hilt_token(second: bool, index: usize) -> u16 {
    HILT_BASE
        + if second { HILT_STRIDE } else { 0 }
        + index.min(usize::from(HILT_STRIDE) - 1) as u16
}

/// Which list (the second saber's or not) and which hilt a hilt token names.
fn hilt_of(token: u16) -> Option<(bool, usize)> {
    let offset = token.checked_sub(HILT_BASE)?;
    match offset / HILT_STRIDE {
        0 => Some((false, usize::from(offset))),
        1 => Some((true, usize::from(offset - HILT_STRIDE))),
        _ => None,
    }
}

/// What a hilt list keeps between frames: its first line on show, and the
/// hilt it last showed chosen, so a newly chosen one is brought into view.
#[derive(Debug, Default)]
pub(super) struct HiltList {
    first: usize,
    chosen: String,
    /// The columns it was laid out in (two for Single and Staff, one each
    /// for Dual's): a change lays it out afresh.
    columns: usize,
    /// The hilt search it was laid out for: a change shows the list from its
    /// top (or round the chosen hilt, when the search keeps it).
    query: String,
}

/// The pointer token of level `level` (1 to 3) of power `power`.
pub(super) fn level_token(power: usize, level: u8) -> u16 {
    LEVEL_BASE + (power * 3) as u16 + u16::from(level.saturating_sub(1))
}

/// The power and level a level token names.
pub(super) fn level_of(token: u16) -> Option<(usize, u8)> {
    let offset = token.checked_sub(LEVEL_BASE)?;
    let power = usize::from(offset / 3);
    (power < POWER_NAMES.len()).then_some((power, (offset % 3) as u8 + 1))
}

/// The colour of power `index`'s group: Neutral silver, the light side's
/// blue, the dark side's red (classic+'s row colours, lifted for the navy),
/// Lightsaber green (its holocrons').
fn power_tint(index: usize) -> Color {
    match ForcePower::ALL.get(index).and_then(|power| power.side()) {
        Some(ForceSide::Light) => Color::new(0.42, 0.66, 1.0, 1.0),
        Some(ForceSide::Dark) => Color::new(1.0, 0.36, 0.3, 1.0),
        None if index >= ForcePower::SaberOffense as usize => Color::new(0.42, 0.92, 0.58, 1.0),
        None => Color::new(0.84, 0.88, 0.96, 1.0),
    }
}

/// `colour` a third of the way to white, for text on the dark.
fn lighter(colour: Color) -> Color {
    let mix = |channel: f32| channel + (1.0 - channel) * 0.35;
    Color::new(mix(colour.r), mix(colour.g), mix(colour.b), colour.a)
}

/// One of the Force page's groups: its sub-heading, its powers in classic+'s
/// order, and where it sits (column 0 or 1, the first or second block).
struct ForceGroup {
    name: &'static str,
    powers: &'static [u8],
    column: usize,
    line: usize,
}

/// The Force page's groups as classic+ groups them: Neutral, the side's own
/// five (the other side's are left out, as there), Lightsaber.
fn force_groups(side: ForceSide) -> [ForceGroup; 3] {
    use super::classic::layout::{DARK_POWERS, LIGHT_POWERS, NEUTRAL_POWERS, SABER_POWERS};
    let (name, powers): (&str, &'static [u8]) = match side {
        ForceSide::Light => ("Light side", &LIGHT_POWERS),
        ForceSide::Dark => ("Dark side", &DARK_POWERS),
    };
    [
        ForceGroup {
            name: "Neutral",
            powers: &NEUTRAL_POWERS,
            column: 0,
            line: 0,
        },
        ForceGroup {
            name,
            powers,
            column: 1,
            line: 0,
        },
        ForceGroup {
            name: "Lightsaber",
            powers: &SABER_POWERS,
            column: 0,
            line: 1,
        },
    ]
}

/// The powers a `g_forcePowerDisable` mask turns off, by name, in
/// `forcePowers_t` order ("Heal, Grip"); Jump and the saber skills it holds
/// at a level instead are named with it ("Jump 1").
fn server_off_list(mask: u32) -> String {
    use std::fmt::Write as _;
    let mut list = String::new();
    for (index, name) in POWER_NAMES.iter().enumerate() {
        if mask & (1 << index) == 0 {
            continue;
        }
        if !list.is_empty() {
            list.push_str(", ");
        }
        let _ = write!(list, "{}", SentenceCase(name));
        match index {
            1 => list.push_str(" 1"),
            15 | 16 => list.push_str(" 3"),
            _ => {}
        }
    }
    list
}

/// The Force page's rows in the order the keys go through them: the sides,
/// the groups' powers (Neutral, the side's, Lightsaber), then the actions.
pub(super) fn force_key_order(side: ForceSide) -> Vec<usize> {
    let mut order = vec![FORCE_SIDE_ROW];
    for group in force_groups(side) {
        order.extend(
            group
                .powers
                .iter()
                .map(|power| FORCE_POWER_ROW + usize::from(*power)),
        );
    }
    order.extend([FORCE_RESET_ROW, FORCE_DISCARD_ROW, FORCE_APPLY_ROW]);
    order
}

/// A rectangle of the frame as the kit takes it.
type Area = [f32; 4];

impl PlayerMenu {
    /// Draw the screen in the SJK UI at `reveal` opacity.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2], reveal: f32) {
        let frame = Frame::new(viewport);
        self.canvas.begin_transparent(viewport);
        self.canvas.push_opacity(reveal);
        let in_game = self.return_target == ReturnTarget::InGame;
        if in_game {
            // Over a match: dim it all, then the model's live preview on a
            // soft shadow where the menu map's stage would hold it.
            let _ = self.canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect: sjk_ui::Rect::new(0.0, 0.0, viewport[0], viewport[1]),
                color: color::alpha(color::SPACE, 0.5),
            });
        }
        scrims(&mut self.canvas, viewport, &frame);
        if in_game {
            self.sjk_preview(&frame);
        }
        self.sjk_top(&frame);
        self.turn.advance(crate::menu::art::motion::seconds());
        if self.model_turns() {
            // First, so every control over the model keeps the pointer.
            self.outfit_turn_area(&frame, MODEL_AREA);
        }
        match self.page {
            ProfilePage::Character => self.sjk_character(&frame),
            ProfilePage::Saber => self.sjk_saber(&frame),
            ProfilePage::Force => self.sjk_force(&frame),
        }
        self.sjk_caption(&frame);
        // Under the model's feet: it turns when dragged (`sjk_outfit.rs`).
        self.sjk_turn_hint(
            &frame,
            MODEL_AREA[0] + MODEL_AREA[2] * 0.5,
            MODEL_FEET_Y + 26.0,
        );
        self.sjk_keys(&frame);
        self.canvas.pop_opacity();
        self.canvas.finish(self.selected as u16);
        target.append(&self.canvas, viewport);
    }

    /// The SJK UI's own pointer targets, before the screen's shared ones:
    /// - a level cell chooses its power's row when hovered and, clicked, sets
    ///   the power to that level (its own level steps it down one, as
    ///   classic+'s stars do);
    /// - a style button chooses the style;
    /// - a hilt chooses that hilt, the wheel over a list scrolls it, and a
    ///   click on a list beside its hilts only chooses its row.
    ///
    /// `None` leaves the event to the shared handling.
    pub(super) fn sjk_pointer(
        &mut self,
        kind: sjk_ui::UiEventKind,
        token: u16,
        wheel: f32,
        console: &mut ViewerConsole,
    ) -> Option<PlayerMenuResult> {
        use sjk_ui::UiEventKind;
        let hover = matches!(kind, UiEventKind::HoverEnter | UiEventKind::Hover);
        let activate = kind == UiEventKind::Activate;
        match self.page {
            ProfilePage::Force => {
                let (power, level) = level_of(token)?;
                let row = FORCE_POWER_ROW + power;
                if hover {
                    self.selected = row;
                } else if activate {
                    self.selected = row;
                    // A click raises to the level; a right click removes one
                    // ([`Self::sjk_remove_level`]).
                    if level > self.force.allocation().levels[power]
                        && self.force.set_level(power, level)
                    {
                        self.note_level_bought(power);
                    }
                } else {
                    return None;
                }
            }
            ProfilePage::Saber => {
                let hilt_row = |second: bool| {
                    self.saber_row(if second {
                        SaberRow::SecondHilt
                    } else {
                        SaberRow::Hilt
                    })
                };
                let skin_row = self.saber_row(SaberRow::Skin);
                let browse = match token {
                    SKIN_PREV if activate => -(SKIN_SHOWN as isize),
                    SKIN_NEXT if activate => SKIN_SHOWN as isize,
                    _ if kind == UiEventKind::Wheel
                        && (token == SKIN_SCROLL
                            || token == SKIN_PREV
                            || token == SKIN_NEXT
                            || token
                                .checked_sub(SKIN_BASE)
                                .is_some_and(|c| usize::from(c) < self.blade_choice.count())) =>
                    {
                        if wheel > 0.0 {
                            -1
                        } else {
                            1
                        }
                    }
                    _ => 0,
                };
                if browse != 0 {
                    self.selected = skin_row?;
                    self.blade_choice
                        .browse(self.blade_first, browse, SKIN_SHOWN);
                } else if [SKIN_SCROLL, SKIN_PREV, SKIN_NEXT].contains(&token) {
                    if hover {
                        self.selected = skin_row?;
                    } else {
                        return None;
                    }
                } else if let Some(choice) = token
                    .checked_sub(SKIN_BASE)
                    .map(usize::from)
                    .filter(|choice| *choice < self.blade_choice.count())
                {
                    let row = skin_row?;
                    if hover {
                        self.selected = row;
                    } else if activate {
                        self.selected = row;
                        self.blade_choice.pick(console, choice);
                    } else {
                        return None;
                    }
                } else if let Some(style) = token
                    .checked_sub(STYLE_BASE)
                    .and_then(|index| STYLES.get(usize::from(index)))
                {
                    if hover {
                        self.selected = 0;
                    } else if activate {
                        self.selected = 0;
                        self.saber.set_style(*style, catalog_of(&self.loader));
                        self.saber.apply(console);
                    } else {
                        return None;
                    }
                } else if let Some(list) = token
                    .checked_sub(HILT_SCROLL)
                    .filter(|list| *list < 2)
                    .map(usize::from)
                    .or_else(|| hilt_of(token).map(|(second, _)| usize::from(second)))
                    .filter(|_| kind == UiEventKind::Wheel)
                {
                    let state = &mut self.sjk_hilts[list];
                    state.first = if wheel > 0.0 {
                        state.first.saturating_sub(1)
                    } else {
                        state.first + 1
                    };
                } else if let Some((second, index)) = hilt_of(token) {
                    let row = hilt_row(second)?;
                    if hover {
                        self.selected = row;
                    } else if activate {
                        self.selected = row;
                        if let Some(catalog) = catalog_of(&self.loader) {
                            self.saber.select(catalog, index, second);
                            self.saber.apply(console);
                        }
                    } else {
                        return None;
                    }
                } else if activate
                    && [hilt_row(false), hilt_row(true), skin_row]
                        .contains(&Some(usize::from(token)))
                {
                    self.selected = usize::from(token);
                } else {
                    return None;
                }
            }
            ProfilePage::Character => return self.creator_pointer(kind, token),
        }
        Some(PlayerMenuResult::None)
    }

    /// A right click on `token`, when it is a level mark of the Force page: the
    /// level goes, with the ones above it, so the power stands one below it. A
    /// mark above the power's level has nothing to remove.
    pub(super) fn sjk_remove_level(&mut self, token: u16) {
        if self.page != ProfilePage::Force {
            return;
        }
        let Some((power, level)) = level_of(token) else {
            return;
        };
        self.selected = FORCE_POWER_ROW + power;
        if level <= self.force.allocation().levels[power] {
            self.force.set_level(power, level - 1);
        }
    }

    /// Where `row` is on the Saber page, when the page has it.
    fn saber_row(&self, row: SaberRow) -> Option<usize> {
        self.saber_rows().iter().position(|each| *each == row)
    }

    /// The rows of the Saber and Force pages in the order they show them
    /// (`None` on the Character page, whose rows go in their own order): the
    /// Saber page's hilt lists before its blades (Dual's two side by side),
    /// the Force page's groups ([`force_key_order`]).
    fn sjk_key_order(&self) -> Option<Vec<usize>> {
        match self.page {
            ProfilePage::Character => None,
            ProfilePage::Force => Some(force_key_order(self.force.allocation().side)),
            ProfilePage::Saber => {
                let rows = self.saber_rows();
                let first = |row: &SaberRow| {
                    !matches!(
                        row,
                        SaberRow::Style | SaberRow::Search | SaberRow::Hilt | SaberRow::SecondHilt
                    )
                };
                Some(
                    rows.iter()
                        .enumerate()
                        .filter(|(_, row)| !first(row))
                        .chain(rows.iter().enumerate().filter(|(_, row)| first(row)))
                        .map(|(index, _)| index)
                        .collect(),
                )
            }
        }
    }

    /// Up (`direction` -1) or Down on the Saber or Force page: the next row
    /// in the order the page shows them; false on the Character page.
    pub(super) fn sjk_step(&mut self, direction: isize) -> bool {
        let Some(order) = self.sjk_key_order() else {
            return false;
        };
        let next = match order.iter().position(|row| *row == self.selected) {
            Some(at) => super::controller::wrap(at, direction, order.len()),
            None if direction < 0 => order.len() - 1,
            None => 0,
        };
        self.selected = order[next];
        true
    }

    /// Opened from a game, the live model this view wants
    /// ([`crate::menu_stage::preview`]): standing right of the form, holding
    /// the saber draft lit in its style's stance, as the stage model does on
    /// the menu map. On the menu map the stage holds it instead.
    pub(super) fn sjk_model_preview(&self) -> Option<ModelPreview> {
        (self.return_target == ReturnTarget::InGame).then_some(ModelPreview {
            area: PreviewArea::Sjk(MODEL_AREA),
            stance: "BOTH_STAND2",
            sabers: true,
            // The Force page's model shows the power under the pointer, and only while
            // the pointer is on it.
            pose: self
                .sjk_pointer_power()
                .and_then(|(power, _)| super::force_pose::gesture(power, self.saber.style())),
            shell: self
                .sjk_pointer_power()
                .and_then(|(power, _)| super::force_pose::shell(power)),
            effect: self.sjk_pointer_power().and_then(|(power, level)| {
                super::force_pose::effect(power, level, self.saber.style())
            }),
            showcase: false,
            room: MODEL_ROOM,
            // Still, as the stage model stands, a little to its right; the camera goes
            // round the model the other way to turn it as the pointer does
            // ([`Self::model_turn`]).
            angle: Some(-28.0 - self.model_turn()),
        })
    }

    /// The live preview on its shadow, once the renderer has drawn a frame.
    fn sjk_preview(&mut self, frame: &Frame) {
        let s = frame.s;
        let [x, y, width, height] = MODEL_AREA;
        let shadow = [x + width * 0.34, MODEL_FEET_Y - 12.0, width * 0.32, 24.0];
        let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(shadow[0], shadow[1], shadow[2], shadow[3]),
            radius: 14.0 * s,
            color: color::alpha(color::SPACE, 0.55),
        });
        let _ = self.canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x + width * 0.25, MODEL_FEET_Y, width * 0.5, 1.5),
            color: color::alpha(color::GOLD, 0.35),
        });
        if self.preview_ready {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x, y, width, height),
                texture: crate::ui_renderer::PREVIEW_TEXTURE,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
    }

    /// Whether a click at `point` on row token `token` lands outside the
    /// row's control, so it only chooses the row. A Force power's row has
    /// none: its level marks are targets of their own.
    pub(super) fn sjk_beside_control(&self, token: u16, point: sjk_ui::Vec2) -> bool {
        if self.page == ProfilePage::Force
            && (FORCE_POWER_ROW..FORCE_RESET_ROW).contains(&usize::from(token))
        {
            return true;
        }
        let Some(rect) = self.canvas.rect_for(token) else {
            return false;
        };
        !(point.x >= rect.x
            && point.x <= rect.right()
            && point.y >= rect.y
            && point.y <= rect.bottom())
    }

    /// The way back, the player's name as the title, the pages' tabs.
    fn sjk_top(&mut self, frame: &Frame) {
        let s = frame.s;
        let [x, y] = frame.point(COLUMN_X, BAR_Y - 12.0);
        let back = match self.return_target {
            ReturnTarget::MainMenu => "Main menu",
            ReturnTarget::InGame => "Game menu",
        };
        let end = key_hint(&mut self.canvas, &["Esc"], back, x, y, s);
        self.canvas
            .hit_region(BACK_TOKEN, sjk_ui::Rect::new(x, y, end - x, 24.0 * s));
        let title_x = (end - frame.origin[0]) / s + 22.0;
        let name = crate::profile_hub::title(&self.draft.name);
        text(
            &mut self.canvas,
            TextFamily::Display,
            format_args!("{name}"),
            frame.rect(title_x, BAR_Y - 30.0, 1_000.0, 60.0),
            48.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if self.hub {
            // The Profile screen's row of tabs, this page lit.
            let tab = crate::profile_hub::Tab::of_player_page(self.page.index());
            crate::profile_hub::row(&mut self.canvas, frame, tab);
        } else {
            crate::profile_hub::tabs(&mut self.canvas, frame, &TABS, self.page.index(), TAB_BASE);
        }
    }

    /// A row's band when it is the focused one, and its name; returns whether
    /// it is focused.
    fn sjk_row(&mut self, frame: &Frame, index: usize, area: Area, label: &str) -> bool {
        let focused = index == self.selected;
        if focused {
            kit::band(&mut self.canvas, frame, area);
        }
        let [x, y, _, height] = area;
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{label}"),
            frame.rect(x + 22.0, y + (height - 26.0) * 0.5, 220.0, 26.0),
            19.0 * frame.s,
            if focused {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                color::alpha(color::TEXT, 0.88)
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        focused
    }

    /// Register row `index`'s control `control`, then the whole row `area`.
    fn sjk_targets(&mut self, frame: &Frame, index: usize, control: Area, area: Area) {
        let token = index as u16;
        self.canvas.hit_region(
            token,
            frame.rect(control[0], control[1], control[2], control[3]),
        );
        self.canvas
            .hit_region(token, frame.rect(area[0], area[1], area[2], area[3]));
    }

    /// The character page: the name and the search, the strip of categories, the
    /// tiles of the one on show ([`Self::sjk_creator_grid`]) and the Apply button.
    fn sjk_character(&mut self, frame: &Frame) {
        let rows = self.character_rows();
        for (index, row) in rows.iter().enumerate().take(2) {
            let top = ROWS_TOP + index as f32 * ROW;
            let area = [COLUMN_X, top, COLUMN_WIDTH, ROW];
            let focused = self.sjk_row(frame, index, area, row.label());
            let control = [
                CONTROL_RIGHT - CONTROL_WIDTH,
                top + (ROW - CONTROL) * 0.5,
                CONTROL_WIDTH,
                CONTROL,
            ];
            match row {
                CharacterRow::Name => {
                    let caret = if self.name_editing { "_" } else { "" };
                    kit::field(
                        &mut self.canvas,
                        frame,
                        control,
                        format_args!("{}{caret}", self.draft.name),
                        focused || self.name_editing,
                        false,
                    );
                }
                _ => {
                    if self.search.is_empty() && !self.search_editing {
                        kit::field(
                            &mut self.canvas,
                            frame,
                            control,
                            format_args!("Type a name to filter"),
                            focused,
                            false,
                        );
                    } else {
                        let caret = if self.search_editing { "_" } else { "" };
                        kit::field(
                            &mut self.canvas,
                            frame,
                            control,
                            format_args!("{}{caret}", self.search),
                            focused || self.search_editing,
                            false,
                        );
                    }
                }
            }
            self.sjk_targets(frame, index, control, area);
        }
        // The strip: its row is chosen by a click beside its segments.
        let strip_top = ROWS_TOP + 2.0 * ROW + 6.0;
        let strip = [COLUMN_X, strip_top, COLUMN_WIDTH, STRIP];
        if rows.get(self.selected) == Some(&CharacterRow::Category) {
            kit::band(&mut self.canvas, frame, strip);
        }
        self.canvas
            .hit_region(2, frame.rect(strip[0], strip[1], strip[2], strip[3]));
        self.sjk_category_strip(frame, COLUMN_X, strip_top);
        let grid_top = strip_top + STRIP + 6.0;
        self.sjk_creator_grid(frame, COLUMN_X, COLUMN_WIDTH, grid_top, BOTTOM - APPLY_LINE);
        // The model chosen shows on the stage and is worn on Apply.
        let pending = self.look_pending();
        let apply = self.apply_row();
        kit::button(
            &mut self.canvas,
            frame,
            [COLUMN_X, BOTTOM - APPLY_HEIGHT, COLUMN_WIDTH, APPLY_HEIGHT],
            if pending { "Apply" } else { "Applied" },
            true,
            pending,
            self.selected == apply,
            apply as u16,
        );
    }

    /// What the catalogue's state says while it is not ready.
    pub(super) fn catalogue_line(&self) -> Option<&str> {
        match self.status() {
            LegacyCatalogStatus::Ready => None,
            LegacyCatalogStatus::Idle | LegacyCatalogStatus::Loading => {
                Some("Reading the character catalogue...")
            }
            LegacyCatalogStatus::Failed => Some(
                self.loader
                    .as_ref()
                    .and_then(LegacyAssetCatalogLoader::error)
                    .unwrap_or("The character catalogue is unavailable."),
            ),
        }
    }

    /// The saber page: the style as three buttons, the hilts as a list (two
    /// side by side for Dual: the right hand's and the left's), then each
    /// blade's colour and its channels.
    fn sjk_saber(&mut self, frame: &Frame) {
        let rows = self.saber_rows();
        let style = self.saber.style();
        // The style: the row, then its three buttons over it.
        let style_area = [COLUMN_X, ROWS_TOP, COLUMN_WIDTH, ROW];
        let focused = self.sjk_row(frame, 0, style_area, SaberRow::Style.label());
        let middle = ROWS_TOP + ROW * 0.5;
        let control = [
            CONTROL_RIGHT - CONTROL_WIDTH,
            middle - CONTROL * 0.5,
            CONTROL_WIDTH,
            CONTROL,
        ];
        self.sjk_targets(frame, 0, control, style_area);
        let current = STYLES.iter().position(|each| *each == style).unwrap_or(0);
        kit::segments(
            &mut self.canvas,
            frame,
            CONTROL_RIGHT,
            middle,
            &STYLES.map(SaberStyle::label),
            current,
            focused,
            STYLE_BASE,
        );
        // The hilts, the search in their heading line (beside the second list's
        // for Dual).
        let lists_top = ROWS_TOP + ROW + 8.0;
        let half = (COLUMN_WIDTH - GUTTER) * 0.5;
        let height = HILT_HEAD + hilt_rows(style) as f32 * HILT_ROW + 12.0;
        if style == SaberStyle::Dual {
            for (second, x, heading, search) in [
                (false, COLUMN_X, "Right hand", None),
                (
                    true,
                    COLUMN_X + half + GUTTER,
                    "Left hand",
                    Some(DUAL_SEARCH_WIDTH),
                ),
            ] {
                let area = [x, lists_top, half, height];
                self.sjk_hilt_list(frame, second, area, 1, heading, search);
            }
        } else {
            let heading = if style == SaberStyle::Staff {
                "Staff"
            } else {
                "Hilt"
            };
            self.sjk_hilt_list(
                frame,
                false,
                [COLUMN_X, lists_top, COLUMN_WIDTH, height],
                2,
                heading,
                Some(SEARCH_WIDTH),
            );
        }
        // The blade, then each blade's colour.
        let mut top = lists_top + height + 10.0;
        for (index, row) in rows.iter().enumerate() {
            if matches!(
                row,
                SaberRow::Style | SaberRow::Search | SaberRow::Hilt | SaberRow::SecondHilt
            ) {
                continue;
            }
            if *row == SaberRow::Skin {
                self.sjk_blade_choice(frame, index, top);
                top += ROW;
                continue;
            }
            let area = [COLUMN_X, top, COLUMN_WIDTH, ROW];
            let focused = self.sjk_row(frame, index, area, row.label());
            let middle = top + ROW * 0.5;
            let control = [
                CONTROL_RIGHT - CONTROL_WIDTH,
                middle - CONTROL * 0.5,
                CONTROL_WIDTH,
                CONTROL,
            ];
            top += ROW;
            if row.is_blade() {
                let custom = self.saber.custom_rgb(row.second());
                let chips = PALETTE.map(|index| saber_color(index, custom));
                let active = usize::from(self.saber.color(row.second()));
                kit::chips(&mut self.canvas, frame, control, &chips, active, focused);
                self.sjk_targets(frame, index, control, area);
                continue;
            }
            let Some(channel) = row.channel() else {
                continue;
            };
            let value = self.saber.channel(row.second(), channel);
            let ratio = f32::from(value) / 255.0;
            let track_x = CONTROL_RIGHT - NUMBER_WIDTH - 18.0 - TRACK_WIDTH;
            kit::slider(
                &mut self.canvas,
                frame,
                track_x,
                middle,
                TRACK_WIDTH,
                ratio,
                focused,
            );
            let number = frame.rect(
                CONTROL_RIGHT - NUMBER_WIDTH - 6.0,
                middle - 15.0,
                NUMBER_WIDTH + 6.0,
                30.0,
            );
            match self.numeric.as_ref().filter(|edit| edit.row == index) {
                Some(edit) => {
                    self.canvas.set_family(TextFamily::Display);
                    edit.draw(&mut self.canvas, number, 1.4 * frame.s);
                    self.canvas.set_family(TextFamily::Body);
                }
                None => text(
                    &mut self.canvas,
                    TextFamily::Display,
                    format_args!("{value}"),
                    number,
                    21.0 * frame.s,
                    if focused {
                        color::TEXT
                    } else {
                        color::alpha(color::TEXT, 0.9)
                    },
                    FontWeight::Regular,
                    TextAlign::End,
                ),
            }
            // The track takes the pointer a little beyond its ends.
            let track = [track_x - 10.0, middle - 16.0, TRACK_WIDTH + 20.0, 32.0];
            self.sjk_targets(frame, index, track, area);
            // After the row: the region registered last takes the pointer
            // where they overlap.
            self.canvas.hit_region(
                crate::menu_widgets::numeric::VALUE_BASE + index as u16,
                number,
            );
        }
    }

    /// One hilt list on `area` under its `heading`: the hilts the search lists
    /// ([`super::saber::SaberMenu::listed`]) in `columns` columns, row after row,
    /// the chosen one gold with a dot, scrolled to keep a newly chosen one in
    /// view; the wheel scrolls it, a click chooses. With `search` (its width) the
    /// hilt search's field stands at the right of its heading line.
    fn sjk_hilt_list(
        &mut self,
        frame: &Frame,
        second: bool,
        area: Area,
        columns: usize,
        heading: &str,
        search: Option<f32>,
    ) {
        let s = frame.s;
        let row = self
            .saber_row(if second {
                SaberRow::SecondHilt
            } else {
                SaberRow::Hilt
            })
            .unwrap_or(1);
        let shown_lines = hilt_rows(self.saber.style());
        // How many hilts the search lists, and where the chosen one is among them.
        let (count, chosen, retail, headed) =
            catalog_of(&self.loader).map_or((0, None, 0, false), |catalog| {
                let mut count = 0;
                let mut chosen = None;
                let mut retail = 0;
                for (index, hilt) in self.saber.listed(catalog, second) {
                    if hilt.name.eq_ignore_ascii_case(self.saber.hilt(second)) {
                        chosen = Some(count);
                    }
                    if index < catalog.retail_saber_hilts {
                        retail += 1;
                    }
                    count += 1;
                }
                (count, chosen, retail, catalog.retail_saber_hilts > 0)
            });
        let [x, y, width, height] = area;
        if self.selected == row {
            kit::band(&mut self.canvas, frame, area);
        }
        // The whole list first: the hilts, registered after, take the pointer.
        self.canvas
            .hit_region(row as u16, frame.rect(x, y, width, height));
        let list = usize::from(second);
        self.canvas
            .scroll_region(HILT_SCROLL + list as u16, frame.rect(x, y, width, height));
        let field_x = search.map(|field| x + width - 12.0 - field);
        // The heading's rule runs to the field, or is left out where only a stub
        // of it would show (beside Dual's "Left hand"; the kit puts the rule 8.5
        // pixels a character and 22 after the label).
        let rule_width = field_x.map_or(width - 34.0, |field_x| {
            let room = field_x - 16.0 - (x + 22.0);
            let label = 8.5 * heading.chars().count() as f32 + 22.0;
            if room - label < 24.0 { 0.0 } else { room }
        });
        kit::heading(
            &mut self.canvas,
            frame,
            x + 22.0,
            y + HILT_HEAD * 0.5,
            rule_width,
            heading,
        );
        if let (Some(field_x), Some(field), Some(search_row)) =
            (field_x, search, self.saber_row(SaberRow::Search))
        {
            let control = [field_x, y + 1.0, field, HILT_HEAD - 2.0];
            self.sjk_hilt_search(frame, search_row, control, count);
        }
        let sections = HiltLines {
            total: count,
            retail,
            columns,
            headed,
        };
        let lines = sections.lines();
        let state = &mut self.sjk_hilts[list];
        let query = self.saber.search();
        let searched = state.query != query;
        if searched {
            state.query.clear();
            state.query.push_str(query);
            state.first = 0;
        }
        // A hilt chosen since the last frame (by the keys, say) comes into
        // view; the first time the list shows, in its middle.
        if searched
            || state.columns != columns
            || !state.chosen.eq_ignore_ascii_case(self.saber.hilt(second))
        {
            let first_show = !searched && (state.chosen.is_empty() || state.columns != columns);
            state.columns = columns;
            state.chosen.clear();
            state.chosen.push_str(self.saber.hilt(second));
            if let Some(line) = chosen.map(|index| sections.place(index).0) {
                if first_show {
                    state.first = line.saturating_sub(shown_lines / 2);
                } else if line < state.first {
                    state.first = line;
                } else if line >= state.first + shown_lines {
                    state.first = line + 1 - shown_lines;
                }
            }
        }
        state.first = state.first.min(lines.saturating_sub(shown_lines));
        let first = state.first;
        if count == 0 {
            let empty = frame.rect(x + 22.0, y + HILT_HEAD + 6.0, width - 44.0, HILT_ROW);
            if query.trim().is_empty() {
                text(
                    &mut self.canvas,
                    TextFamily::Body,
                    format_args!("No hilts found"),
                    empty,
                    17.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            } else {
                text(
                    &mut self.canvas,
                    TextFamily::Body,
                    format_args!("No hilt matches \"{}\"", query.trim()),
                    empty,
                    17.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
        }
        let cell_width = (width - 24.0) / columns as f32;
        if let Some(catalog) = catalog_of(&self.loader) {
            // The sections' headings, where they stand among the lines on show.
            for line in first..(first + shown_lines).min(lines) {
                if let Some((name, members)) = sections.heading(line) {
                    let top = y + HILT_HEAD + 6.0 + (line - first) as f32 * HILT_ROW;
                    text(
                        &mut self.canvas,
                        TextFamily::Display,
                        format_args!("{name}  ·  {members}"),
                        frame.rect(x + 16.0, top, width - 32.0, HILT_ROW - 2.0),
                        16.0 * s,
                        color::GOLD,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
            }
            let listed = self.saber.listed(catalog, second).enumerate();
            for (at, (index, hilt)) in listed {
                let (line, column) = sections.place(at);
                if line < first || line >= first + shown_lines {
                    continue;
                }
                let cell = [
                    x + 12.0 + column as f32 * cell_width,
                    y + HILT_HEAD + 6.0 + (line - first) as f32 * HILT_ROW,
                    cell_width - 6.0,
                    HILT_ROW - 2.0,
                ];
                let rect = frame.rect(cell[0], cell[1], cell[2], cell[3]);
                let token = hilt_token(second, index);
                let picked = chosen == Some(at);
                let hovered = self.canvas.token_hovered(token);
                if picked || hovered {
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect,
                        radius: 8.0 * s,
                        color: color::alpha(
                            if picked { color::GOLD } else { color::HOLO },
                            if picked { 0.16 } else { 0.1 },
                        ),
                    });
                }
                if picked {
                    kit::changed_dot(
                        &mut self.canvas,
                        frame,
                        cell[0] + cell[2] - 16.0,
                        cell[1] + cell[3] * 0.5,
                    );
                }
                text(
                    &mut self.canvas,
                    TextFamily::Body,
                    format_args!("{}", hilt.display_name),
                    frame.rect(cell[0] + 12.0, cell[1], cell[2] - 40.0, cell[3]),
                    17.0 * s,
                    match (picked, hovered) {
                        (true, _) => color::GOLD_BRIGHT,
                        (false, true) => color::TEXT,
                        (false, false) => color::alpha(color::TEXT, 0.82),
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                self.canvas.hit_region(token, rect);
            }
        }
        if lines > shown_lines {
            self.canvas.scrollbar(
                HILT_SCROLL + list as u16,
                frame.rect(
                    x + width - 8.0,
                    y + HILT_HEAD + 6.0,
                    3.0,
                    shown_lines as f32 * HILT_ROW,
                ),
                first,
                shown_lines,
                lines,
            );
        }
    }

    /// The hilt search's field on `control`, as the Character page's model
    /// search: its words with the caret while typed and how many hilts it
    /// lists (`found`), or what to type.
    fn sjk_hilt_search(&mut self, frame: &Frame, row: usize, control: Area, found: usize) {
        let focused = self.selected == row;
        let editing = self.search_editing;
        let search = self.saber.search();
        if search.is_empty() && !editing {
            kit::field(
                &mut self.canvas,
                frame,
                control,
                format_args!("Search hilts"),
                focused,
                false,
            );
        } else {
            let caret = if editing { "_" } else { "" };
            kit::field(
                &mut self.canvas,
                frame,
                control,
                format_args!("{search}{caret}   {found} found"),
                focused || editing,
                false,
            );
        }
        let [x, y, width, height] = control;
        self.canvas
            .hit_region(row as u16, frame.rect(x, y, width, height));
    }

    /// The blade row at `top`: the stock blade in the first blade's colour, then
    /// each blade skin the player owns, as the Collection's swatches shrunk, the
    /// one worn ringed gold; under the row's name the name of the one under the
    /// pointer (or worn); with no skin to offer, a line saying why.
    fn sjk_blade_choice(&mut self, frame: &Frame, index: usize, top: f32) {
        use crate::console::collection_panel::swatch;
        let s = frame.s;
        let area = [COLUMN_X, top, COLUMN_WIDTH, ROW];
        let focused = index == self.selected;
        if focused {
            kit::band(&mut self.canvas, frame, area);
        }
        let count = self.blade_choice.count();
        let chosen = self.blade_choice.chosen();
        let first = self
            .blade_choice
            .first(skin_window(chosen, count), SKIN_SHOWN);
        let shown = count.min(SKIN_SHOWN);
        let y = top + (ROW - SKIN_HEIGHT) * 0.5;
        let span = shown as f32 * (SKIN_WIDTH + SKIN_GAP) - SKIN_GAP;
        self.sjk_targets(frame, index, [SKIN_X, y, span, SKIN_HEIGHT], area);
        // The wheel over the row browses it, as do the arrows at its ends.
        self.canvas
            .scroll_region(SKIN_SCROLL, frame.rect(SKIN_X, y, span, SKIN_HEIGHT));
        self.blade_first = first;
        let seconds = crate::menu::art::motion::seconds() as f32;
        let colour = saber_color(self.saber.color(false), self.saber.custom_rgb(false));
        let mut named = chosen;
        // More blades on either side than the row shows: a chevron there.
        for (more, x, mark, token) in [
            (first > 0, SKIN_X - 14.0, "‹", SKIN_PREV),
            (first + shown < count, SKIN_X + span + 3.0, "›", SKIN_NEXT),
        ] {
            if more {
                let hovered = self.canvas.token_hovered(token);
                text(
                    &mut self.canvas,
                    TextFamily::Body,
                    format_args!("{mark}"),
                    frame.rect(x, y, 11.0, SKIN_HEIGHT),
                    if hovered { 26.0 } else { 22.0 } * s,
                    if hovered {
                        Color::new(1.0, 1.0, 1.0, 1.0)
                    } else {
                        color::GOLD_BRIGHT
                    },
                    FontWeight::Regular,
                    TextAlign::Center,
                );
                self.canvas
                    .hit_region(token, frame.rect(x - 4.0, y, 19.0, SKIN_HEIGHT));
            }
        }
        for choice in first..first + shown {
            let x = SKIN_X + (choice - first) as f32 * (SKIN_WIDTH + SKIN_GAP);
            let rect = [x, y, SKIN_WIDTH, SKIN_HEIGHT];
            let token = SKIN_BASE + choice as u16;
            let hovered = self.canvas.token_hovered(token);
            if hovered {
                named = choice;
            }
            match self.blade_choice.get(choice).flatten() {
                None => swatch::small_stock(self.canvas.draw_list_mut(), frame, rect, colour),
                Some(skin) => {
                    let look = self.blade_choice.look(skin.id);
                    let _ = swatch::small_blade(
                        self.canvas.draw_list_mut(),
                        frame,
                        rect,
                        look,
                        seconds,
                        swatch::chroma_turn(look, swatch::rgb_bytes(colour)),
                    );
                    // A thin edge in the shader's tier colour.
                    let _ = self.canvas.draw_list_mut().push(DrawCommand::Border {
                        rect: frame.rect(x, y, SKIN_WIDTH, SKIN_HEIGHT),
                        radius: 4.0 * s,
                        width: 1.2 * s,
                        color: color::alpha(skin.tier.colour(), 0.85),
                    });
                    // Its tier's effects, small (the row holds owned shaders only).
                    crate::rarity_fx::draw(
                        self.canvas.draw_list_mut(),
                        crate::rarity_fx::Item::of(skin.tier, true, crate::rarity_fx::Size::Small),
                        frame.rect(x, y, SKIN_WIDTH, SKIN_HEIGHT),
                        4.0 * s,
                        s,
                        seconds,
                    );
                    // A chroma's mark in the swatch's top right corner, over the glass.
                    if skin.chroma {
                        swatch::chroma_mark(
                            self.canvas.draw_list_mut(),
                            frame,
                            x + SKIN_WIDTH - CHROMA_MARK - 3.0,
                            y + 3.0,
                            CHROMA_MARK,
                            1.0,
                        );
                    }
                }
            }
            if choice == chosen || hovered {
                let _ = self.canvas.draw_list_mut().push(DrawCommand::Border {
                    rect: frame.rect(x - 2.0, y - 2.0, SKIN_WIDTH + 4.0, SKIN_HEIGHT + 4.0),
                    radius: 6.0 * s,
                    width: if choice == chosen { 2.5 * s } else { 1.5 * s },
                    color: if choice == chosen {
                        color::GOLD_BRIGHT
                    } else {
                        Color::new(1.0, 1.0, 1.0, 0.6)
                    },
                });
            }
            self.canvas
                .hit_region(token, frame.rect(x, y, SKIN_WIDTH, SKIN_HEIGHT));
        }
        // The row's name, and under it the blade's.
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{}", SaberRow::Skin.label()),
            frame.rect(LABEL_X, top + 4.0, SKIN_X - LABEL_X - 16.0, 24.0),
            19.0 * s,
            if focused {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                color::alpha(color::TEXT, 0.88)
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let skin = self.blade_choice.get(named).flatten();
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{}", skin.map_or("Stock blade", |skin| skin.name)),
            frame.rect(LABEL_X, top + 28.0, SKIN_X - LABEL_X - 16.0, 20.0),
            15.0 * s,
            if skin.is_some() {
                color::GOLD_BRIGHT
            } else {
                color::MUTED
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        if let Some(hint) = self.blade_choice.hint() {
            let x = SKIN_X + SKIN_WIDTH + 16.0;
            text(
                &mut self.canvas,
                TextFamily::Body,
                format_args!("{hint}"),
                frame.rect(x, top + (ROW - 22.0) * 0.5, CONTROL_RIGHT - x, 22.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// The Force page: mastery and points over their meter, the two sides,
    /// the powers grouped as classic+ groups them (Neutral and Lightsaber on
    /// the left, the side's five on the right), each with its three levels
    /// priced, then Start over, Discard and Apply.
    fn sjk_force(&mut self, frame: &Frame) {
        let s = frame.s;
        let remaining = self.force.remaining_points();
        let total = self.force.total_points();
        let rank = self.force.allocation().rank;
        let dirty = self.force.is_dirty();
        let hovered = self.sjk_hovered_level();
        text(
            &mut self.canvas,
            TextFamily::Display,
            format_args!("{}", super::classic::force_page::mastery(rank)),
            frame.rect(COLUMN_X, ROWS_TOP - 6.0, 400.0, 32.0),
            26.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!(
                "{remaining} of {total} points left{}",
                if dirty { ", not applied yet" } else { "" }
            ),
            frame.rect(CONTROL_RIGHT - 420.0, ROWS_TOP - 4.0, 420.0, 28.0),
            17.0 * s,
            if dirty {
                color::GOLD_BRIGHT
            } else {
                color::MUTED
            },
            FontWeight::Regular,
            TextAlign::End,
        );
        self.sjk_points_meter(frame, remaining, total, hovered);
        // The two sides.
        let side_top = METER_Y + 22.0;
        let picker = [COLUMN_X, side_top, COLUMN_WIDTH, SIDE_HEIGHT];
        if self.selected == FORCE_SIDE_ROW {
            kit::band(
                &mut self.canvas,
                frame,
                [
                    COLUMN_X - 6.0,
                    side_top - 6.0,
                    COLUMN_WIDTH + 12.0,
                    SIDE_HEIGHT + 12.0,
                ],
            );
        }
        let chosen = self.force.allocation().side;
        let half = (COLUMN_WIDTH - GUTTER) * 0.5;
        for (index, (side, label, tint)) in [
            (ForceSide::Light, "Light side", color::HOLO),
            (ForceSide::Dark, "Dark side", color::EMBER),
        ]
        .into_iter()
        .enumerate()
        {
            let x = COLUMN_X + index as f32 * (half + GUTTER);
            let card = frame.rect(x, side_top, half, SIDE_HEIGHT);
            let picked = side == chosen;
            let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: card,
                radius: 12.0 * s,
                color: if picked {
                    color::alpha(tint, 0.16)
                } else {
                    color::alpha(color::SPACE, 0.5)
                },
            });
            let _ = self.canvas.draw_list_mut().push(DrawCommand::Border {
                rect: card,
                radius: 12.0 * s,
                width: if picked { 2.0 * s } else { 1.5 * s },
                color: color::alpha(tint, if picked { 0.85 } else { 0.25 }),
            });
            let emblem = side_texture(side);
            let mut label_x = x + 22.0;
            if self.force_icons.is_texture_ready(emblem) {
                let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: frame.rect(
                        x + 12.0,
                        side_top + 10.0,
                        SIDE_HEIGHT - 20.0,
                        SIDE_HEIGHT - 20.0,
                    ),
                    texture: emblem,
                    color: Color::new(1.0, 1.0, 1.0, if picked { 1.0 } else { 0.4 }),
                });
                label_x = x + SIDE_HEIGHT + 4.0;
            }
            text(
                &mut self.canvas,
                TextFamily::Display,
                format_args!("{label}"),
                frame.rect(
                    label_x,
                    side_top + (SIDE_HEIGHT - 32.0) * 0.5,
                    half - (label_x - x) - 12.0,
                    32.0,
                ),
                26.0 * s,
                if picked { color::TEXT } else { color::MUTED },
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        self.canvas.hit_region(
            FORCE_SIDE_ROW as u16,
            frame.rect(picker[0], picker[1], picker[2], picker[3]),
        );
        // The powers, in classic+'s groups.
        let groups_top = side_top + SIDE_HEIGHT + 22.0;
        for group in force_groups(chosen) {
            let x = COLUMN_X + group.column as f32 * (half + GUTTER);
            let top = groups_top + group.line as f32 * (GROUP_HEADING + 5.0 * CELL + GROUP_GAP);
            kit::heading_in(
                &mut self.canvas,
                frame,
                x,
                top + GROUP_HEADING * 0.5,
                half,
                group.name,
                power_tint(usize::from(group.powers[0])),
            );
            for (slot, power) in group.powers.iter().enumerate() {
                let row_top = top + GROUP_HEADING + slot as f32 * CELL;
                self.sjk_power(
                    frame,
                    usize::from(*power),
                    [x, row_top, half, CELL],
                    hovered,
                );
            }
        }
        // Under the side's group, the server's rules.
        self.sjk_server_rules(
            frame,
            COLUMN_X + half + GUTTER,
            groups_top + GROUP_HEADING + 5.0 * CELL + GROUP_GAP,
            half,
        );
        // The actions, under the Lightsaber group in the left column: Apply
        // across it, Start over and Discard side by side below. The right
        // column's server panel, which grows with its lines, keeps its room.
        let actions_top = groups_top
            + (GROUP_HEADING + 5.0 * CELL + GROUP_GAP)
            + GROUP_HEADING
            + 3.0 * CELL
            + 24.0;
        let small = (half - 12.0) / 2.0;
        for (row, label, enabled, primary, area) in [
            (
                FORCE_APPLY_ROW,
                if dirty { "Apply" } else { "Applied" },
                dirty,
                true,
                [COLUMN_X, actions_top, half, 48.0],
            ),
            (
                FORCE_RESET_ROW,
                "Start over",
                true,
                false,
                [COLUMN_X, actions_top + 58.0, small, 44.0],
            ),
            (
                FORCE_DISCARD_ROW,
                "Discard",
                dirty,
                false,
                [COLUMN_X + small + 12.0, actions_top + 58.0, small, 44.0],
            ),
        ] {
            kit::button(
                &mut self.canvas,
                frame,
                area,
                label,
                primary,
                enabled,
                self.selected == row,
                row as u16,
            );
        }
    }

    /// What the server the client plays on allows, in the column from `x`,
    /// `top`, `width` wide: its highest rank, free saber skills, the powers it
    /// turns off, whether team powers work; off a server, that one will say.
    fn sjk_server_rules(&mut self, frame: &Frame, x: f32, top: f32, width: f32) {
        let s = frame.s;
        kit::heading_in(
            &mut self.canvas,
            frame,
            x,
            top + GROUP_HEADING * 0.5,
            width,
            "This server",
            color::GOLD,
        );
        let mut y = top + GROUP_HEADING + 4.0;
        // Each line wrapped to the column, 24 pixels a line, moving `y` on.
        let line = |canvas: &mut crate::menu_widgets::MenuCanvas,
                    y: &mut f32,
                    words: &str,
                    colour: Color| {
            for part in wrap(words, 42) {
                text(
                    canvas,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(x + 14.0, *y, width - 28.0, 24.0),
                    16.0 * s,
                    colour,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                *y += 24.0;
            }
        };
        let Some(server) = self.force.server() else {
            line(
                &mut self.canvas,
                &mut y,
                "Not on a server. A server can lower the highest rank and turn powers \
                 off; its rules show here once you join.",
                color::MUTED,
            );
            return;
        };
        let rank = server.max_rank.min(7);
        line(
            &mut self.canvas,
            &mut y,
            &format!(
                "Highest rank: {} ({} points)",
                super::classic::force_page::mastery(rank),
                sjk_client::mastery_points(rank)
            ),
            color::TEXT,
        );
        if server.free_saber {
            line(
                &mut self.canvas,
                &mut y,
                "Saber offense and defense 1 are free",
                color::TEXT,
            );
        }
        let off = server.disabled_mask & ((1 << POWER_NAMES.len()) - 1);
        match off.count_ones() {
            0 => line(
                &mut self.canvas,
                &mut y,
                "Every power is allowed",
                color::TEXT,
            ),
            // A long list would run past the page: the rows say which.
            count @ 9.. => line(
                &mut self.canvas,
                &mut y,
                &format!("{count} powers off, marked on their rows"),
                color::EMBER,
            ),
            _ => line(
                &mut self.canvas,
                &mut y,
                &format!("Off: {}", server_off_list(off)),
                color::EMBER,
            ),
        }
        if server.gametype < GT_TEAM {
            line(
                &mut self.canvas,
                &mut y,
                "Team powers: team games only",
                color::MUTED,
            );
        }
        y += 8.0;
        line(
            &mut self.canvas,
            &mut y,
            "Powers it turns off stay yours to pick, for full Force duels. \
             Applied in play, they change when you respawn.",
            color::QUIET,
        );
        debug_assert!(y <= BOTTOM, "the server's rules run to {y}");
    }

    /// The points left as a gold bar under the page's first line; a hovered
    /// level shows what it would take from it (white), or the whole bar turns
    /// ember when there are not enough.
    fn sjk_points_meter(
        &mut self,
        frame: &Frame,
        remaining: u16,
        total: u16,
        hovered: Option<(usize, u8)>,
    ) {
        let s = frame.s;
        let total = total.max(1);
        let share = |points: u16| COLUMN_WIDTH * f32::from(points.min(total)) / f32::from(total);
        let bar = |canvas: &mut crate::menu_widgets::MenuCanvas, x: f32, width: f32, colour| {
            if width > 0.0 {
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(x, METER_Y, width, 5.0),
                    radius: 2.5 * s,
                    color: colour,
                });
            }
        };
        bar(
            &mut self.canvas,
            COLUMN_X,
            COLUMN_WIDTH,
            color::alpha(color::HOLO, 0.16),
        );
        let left = share(remaining);
        let cost = hovered.map_or(0, |(power, level)| self.force.cost_to(power, level));
        if cost > remaining {
            bar(
                &mut self.canvas,
                COLUMN_X,
                left,
                color::alpha(color::EMBER, 0.9),
            );
        } else {
            bar(&mut self.canvas, COLUMN_X, left, color::GOLD);
            let taken = share(cost);
            bar(
                &mut self.canvas,
                COLUMN_X + left - taken,
                taken,
                Color::new(1.0, 1.0, 1.0, 0.85),
            );
        }
    }

    /// The level cell under the pointer: its power and level.
    fn sjk_hovered_level(&self) -> Option<(usize, u8)> {
        (0..POWER_NAMES.len())
            .flat_map(|power| (1..=3).map(move |level| (power, level)))
            .find(|(power, level)| self.canvas.token_hovered(level_token(*power, *level)))
    }

    /// Power `index` on `area`: its holocron and name, then its three levels
    /// as classic+ draws them, round marks numbered with what each level costs
    /// (a ring until bought, a disc once bought), in the colour of the power's
    /// group ([`power_tint`]) on a channel lit up to its level. The bought discs
    /// glow; hovering a level charges the channel up to it (a spark runs along
    /// it), and a level just bought sends a ring out ([`Self::note_level_bought`]).
    fn sjk_power(&mut self, frame: &Frame, index: usize, area: Area, hovered: Option<(usize, u8)>) {
        let s = frame.s;
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return;
        };
        let [x, top, width, height] = area;
        let row = FORCE_POWER_ROW + index;
        let focused = self.selected == row;
        let level = self.force.allocation().levels[index];
        let usable = level > 0
            || !matches!(
                self.force.next_level(index),
                NextLevel::OtherSide | NextLevel::NeedsOffense
            );
        let limit = self.force.server_limit(index);
        if focused {
            kit::band(&mut self.canvas, frame, area);
        }
        // The whole row first: the marks, registered after, take the pointer.
        self.canvas
            .hit_region(row as u16, frame.rect(x, top, width, height));
        let icon = power_texture(index);
        let mut name_x = x + 14.0;
        if self.force_icons.is_texture_ready(icon) {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x + 10.0, top + 6.0, height - 12.0, height - 12.0),
                texture: icon,
                color: Color::new(1.0, 1.0, 1.0, if usable { 0.95 } else { 0.3 }),
            });
            name_x = x + height + 4.0;
        }
        let marks_x = x + width - 10.0 - 3.0 * DISC - 2.0 * DISC_GAP;
        // A power the server limits keeps its name, raised over a note saying so.
        let name_rect = match limit {
            Some(_) => frame.rect(name_x, top + 3.0, marks_x - name_x - 6.0, 24.0),
            None => frame.rect(name_x, top, marks_x - name_x - 6.0, height),
        };
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{}", SentenceCase(POWER_NAMES[index])),
            name_rect,
            17.0 * s,
            match (usable, focused) {
                (false, _) => color::QUIET,
                (true, true) => Color::new(1.0, 1.0, 1.0, 1.0),
                (true, false) => color::alpha(color::TEXT, 0.88),
            },
            if level > 0 {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            TextAlign::Start,
        );
        if let Some(limit) = limit {
            text(
                &mut self.canvas,
                TextFamily::Body,
                format_args!("{}", limit.tag()),
                frame.rect(name_x, top + 24.0, marks_x - name_x - 6.0, 18.0),
                13.0 * s,
                color::alpha(color::EMBER, if usable { 0.95 } else { 0.6 }),
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let tint = power_tint(index);
        let target = hovered
            .filter(|(hovered_power, _)| *hovered_power == index && usable)
            .map(|(_, level)| level);
        let affordable = target.is_some_and(|target| {
            self.force.cost_to(index, target) <= self.force.remaining_points()
        });
        let free_saber = self.force.free_saber();
        let middle = top + height * 0.5;
        let centre = |mark: u8| marks_x + DISC * 0.5 + f32::from(mark - 1) * (DISC + DISC_GAP);
        let now = crate::menu::art::motion::seconds();
        let canvas = &mut self.canvas;
        let dot = |canvas: &mut crate::menu_widgets::MenuCanvas, cx: f32, size: f32, colour| {
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(cx - size * 0.5, middle - size * 0.5, size, size),
                radius: size * 0.5 * s,
                color: colour,
            });
        };
        let channel = |canvas: &mut crate::menu_widgets::MenuCanvas, from: f32, to: f32, colour| {
            if to > from {
                let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(from, middle - 1.5, to - from, 3.0),
                    radius: 1.5 * s,
                    color: colour,
                });
            }
        };
        // The channel: dim from the first mark to the last, lit to the level.
        channel(
            canvas,
            centre(1),
            centre(3),
            color::alpha(if usable { tint } else { color::HOLO }, 0.16),
        );
        if level > 1 {
            channel(canvas, centre(1), centre(level), color::alpha(tint, 0.8));
        }
        // A hovered level beyond the power's charges the channel up to it,
        // a spark running along it (drawn over the marks).
        let charge = if affordable { tint } else { color::EMBER };
        let mut spark = None;
        if let Some(target) = target.filter(|target| *target > level) {
            let from = centre(level.max(1));
            let to = centre(target);
            channel(canvas, from, to, color::alpha(charge, 0.45));
            if to > from {
                let phase = (now.rem_euclid(SPARK_SECONDS) / SPARK_SECONDS) as f32;
                spark = Some(from + (to - from) * phase);
            }
        }
        // A slow breath for the hovered marks, between 0 and 1.
        let breath = (0.5 + 0.5 * (now * std::f64::consts::TAU / 1.6).sin()) as f32;
        for mark in 1..=3_u8 {
            let cx = centre(mark);
            let rect = frame.rect(cx - DISC * 0.5, middle - DISC * 0.5, DISC, DISC);
            let bought = mark <= level;
            let preview = !bought && target.is_some_and(|target| mark <= target);
            let ring = |canvas: &mut crate::menu_widgets::MenuCanvas, colour, width: f32| {
                let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
                    center: frame.point(cx, middle),
                    radius: (DISC * 0.5 - 1.2) * s,
                    width: width * s,
                    start: 0.0,
                    sweep: std::f32::consts::TAU,
                    color: colour,
                    knockout: None,
                });
            };
            let ink = match (usable, bought, preview) {
                (true, true, _) => {
                    // A bought level: a glowing disc.
                    dot(canvas, cx, DISC + 16.0, color::alpha(tint, 0.12));
                    dot(canvas, cx, DISC + 7.0, color::alpha(tint, 0.22));
                    dot(canvas, cx, DISC, tint);
                    color::SPACE
                }
                (false, true, _) => {
                    dot(canvas, cx, DISC, color::alpha(tint, 0.35));
                    color::SPACE
                }
                (true, false, true) if affordable => {
                    dot(
                        canvas,
                        cx,
                        DISC + 10.0,
                        color::alpha(tint, 0.1 + 0.12 * breath),
                    );
                    dot(canvas, cx, DISC, color::alpha(color::SPACE, 0.85));
                    dot(canvas, cx, DISC, color::alpha(tint, 0.22 + 0.18 * breath));
                    ring(canvas, tint, 2.2);
                    color::TEXT
                }
                (true, false, true) => {
                    dot(canvas, cx, DISC, color::alpha(color::SPACE, 0.85));
                    ring(canvas, color::EMBER, 2.0);
                    color::EMBER
                }
                (true, false, false) => {
                    dot(canvas, cx, DISC, color::alpha(color::SPACE, 0.85));
                    ring(
                        canvas,
                        color::alpha(tint, if focused { 0.85 } else { 0.55 }),
                        2.0,
                    );
                    lighter(tint)
                }
                (false, false, _) => {
                    dot(canvas, cx, DISC, color::alpha(color::SPACE, 0.85));
                    ring(canvas, color::alpha(color::HOLO, 0.16), 1.6);
                    color::alpha(color::QUIET, 0.6)
                }
            };
            let cost = if power.level_is_free(mark, free_saber) {
                0
            } else {
                power.level_cost(mark)
            };
            text(
                canvas,
                TextFamily::Display,
                format_args!("{cost}"),
                rect,
                19.0 * s,
                ink,
                FontWeight::Semibold,
                TextAlign::Center,
            );
            canvas.hit_region(level_token(index, mark), rect);
        }
        if let Some(spark) = spark {
            dot(canvas, spark, 14.0, color::alpha(charge, 0.3));
            dot(canvas, spark, 6.0, Color::new(1.0, 1.0, 1.0, 0.95));
        }
        // A level just bought sends a ring out from its mark.
        if let Some((_, mark, at)) = self.sjk_burst.filter(|(burst, ..)| *burst == index) {
            let age = (now - at) as f32;
            if (0.0..BURST_SECONDS).contains(&age) && mark >= 1 {
                let grown = age / BURST_SECONDS;
                let _ = self.canvas.draw_list_mut().push(DrawCommand::Arc {
                    center: frame.point(centre(mark.min(3)), middle),
                    radius: (DISC * 0.5 + grown * 20.0) * s,
                    width: (3.0 - 2.0 * grown) * s,
                    start: 0.0,
                    sweep: std::f32::consts::TAU,
                    color: color::alpha(tint, 0.9 * (1.0 - grown)),
                    knockout: None,
                });
            }
        }
    }

    /// Note that power `power` just rose a level, for the ring its mark sends
    /// out (on the Force page in the SJK UI).
    pub(super) fn note_level_bought(&mut self, power: usize) {
        let level = self.force.allocation().levels[power];
        self.sjk_burst = Some((power, level, crate::menu::art::motion::seconds()));
    }

    /// The power under the pointer on the Force page (its row or one of its level
    /// marks) and its drafted level; none once the pointer leaves it.
    fn sjk_pointer_power(&self) -> Option<(sjk_client::ForcePower, u8)> {
        if self.page != ProfilePage::Force {
            return None;
        }
        let index = (0..POWER_NAMES.len())
            .find(|index| self.canvas.token_hovered((FORCE_POWER_ROW + index) as u16))
            .or_else(|| self.sjk_hovered_level().map(|(power, _)| power))?;
        let power = sjk_client::ForcePower::ALL.get(index).copied()?;
        Some((power, self.force.allocation().levels[index]))
    }

    /// The power the box at the bottom right shows: the one whose row has the
    /// keyboard (the pointer gives it a row it hovers).
    fn sjk_box_power(&self) -> Option<usize> {
        (self.page == ProfilePage::Force)
            .then(|| self.selected.checked_sub(FORCE_POWER_ROW))
            .flatten()
            .filter(|index| *index < POWER_NAMES.len())
    }

    /// The box at the bottom right on the Force page: the power under the
    /// pointer (or the keyboard) in big, its holocron, group, level, what it
    /// does, its levels' prices and what the next one takes.
    fn sjk_force_box(&mut self, frame: &Frame, index: usize) {
        let s = frame.s;
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return;
        };
        let [x, y, width, height] = POWER_BOX;
        let rect = frame.rect(x, y, width, height);
        let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x + 4.0, y + 8.0, width, height),
            radius: 18.0 * s,
            color: color::alpha(color::SPACE, 0.5),
        });
        let _ = self.canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius: 18.0 * s,
            color: Color::new(0.04, 0.06, 0.12, 0.92),
        });
        let tint = power_tint(index);
        let _ = self.canvas.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 18.0 * s,
            width: 1.5 * s,
            color: color::alpha(tint, 0.5),
        });
        let icon = power_texture(index);
        let level = self.force.allocation().levels[index];
        let text_x = if self.force_icons.is_texture_ready(icon) {
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x + 22.0, y + 22.0, 112.0, 112.0),
                texture: icon,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
            x + 152.0
        } else {
            x + 24.0
        };
        let line = width - (text_x - x) - 20.0;
        text(
            &mut self.canvas,
            TextFamily::Display,
            format_args!("{}", SentenceCase(POWER_NAMES[index])),
            frame.rect(text_x, y + 22.0, line, 46.0),
            40.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let group = match power.side() {
            Some(ForceSide::Light) => "Light side",
            Some(ForceSide::Dark) => "Dark side",
            None if index >= 15 => "Lightsaber",
            None => "Neutral",
        };
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{group}"),
            frame.rect(text_x, y + 70.0, line, 24.0),
            16.0 * s,
            tint,
            FontWeight::Regular,
            TextAlign::Start,
        );
        kit::pips(
            &mut self.canvas,
            frame,
            text_x + 6.0,
            y + 116.0,
            level,
            3,
            true,
        );
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("Level {level} of 3"),
            frame.rect(text_x + 62.0, y + 104.0, line - 62.0, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        for (number, part) in wrap(POWER_NOTES[index], 46).take(3).enumerate() {
            text(
                &mut self.canvas,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(
                    x + 24.0,
                    y + 150.0 + number as f32 * 26.0,
                    width - 48.0,
                    26.0,
                ),
                17.0 * s,
                color::alpha(color::TEXT, 0.9),
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let (status, status_colour) = match self.force.next_level(index) {
            NextLevel::Mastered => ("Mastered".to_owned(), color::GOLD_BRIGHT),
            NextLevel::Free => ("Next level: free".to_owned(), color::GOLD_BRIGHT),
            NextLevel::Costs(cost) => (format!("Next level: {cost} points"), color::GOLD_BRIGHT),
            NextLevel::Short(cost) => (
                format!("Next level: {cost} points, too few left"),
                color::EMBER,
            ),
            NextLevel::OtherSide => ("The other side's power".to_owned(), color::QUIET),
            NextLevel::NeedsOffense => ("Needs Saber offense 1".to_owned(), color::EMBER),
        };
        if let Some(limit) = self.force.server_limit(index) {
            text(
                &mut self.canvas,
                TextFamily::Body,
                format_args!("{}", limit.describe()),
                frame.rect(x + 24.0, y + height - 72.0, width - 48.0, 26.0),
                17.0 * s,
                color::EMBER,
                FontWeight::Semibold,
                TextAlign::Start,
            );
        }
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{status}"),
            frame.rect(x + 24.0, y + height - 44.0, width - 48.0, 26.0),
            17.0 * s,
            status_colour,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let free_saber = self.force.free_saber();
        let price = |level: u8| {
            if power.level_is_free(level, free_saber) {
                "free".to_owned()
            } else {
                power.level_cost(level).to_string()
            }
        };
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("Levels {} / {} / {}", price(1), price(2), price(3)),
            frame.rect(x + 24.0, y + height - 44.0, width - 48.0, 26.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// Beside the model, as in a gallery: what the page shows of it.
    fn sjk_caption(&mut self, frame: &Frame) {
        // The Force page shows the power under the pointer instead.
        if let Some(index) = self.sjk_box_power() {
            self.sjk_force_box(frame, index);
            return;
        }
        let s = frame.s;
        let (title, detail) = match self.page {
            ProfilePage::Character => {
                let (model, skin) = self
                    .draft
                    .model
                    .split_once('/')
                    .unwrap_or((&self.draft.model, "default"));
                (
                    crate::menu::classic::view::Sentence(model).to_string(),
                    format!("{} skin", crate::menu::classic::view::Sentence(skin)),
                )
            }
            ProfilePage::Saber => (
                crate::menu::classic::view::Sentence(
                    &self.saber.style().label().to_ascii_lowercase(),
                )
                .to_string(),
                format!(
                    "{}, {}",
                    self.saber.hilt_label(catalog_of(&self.loader), false),
                    self.blade_choice
                        .worn()
                        .map_or(blade_word(self.saber.color(false)), |skin| skin.name)
                ),
            ),
            ProfilePage::Force => (
                match self.force.allocation().side {
                    ForceSide::Light => "Light side".to_owned(),
                    ForceSide::Dark => "Dark side".to_owned(),
                },
                format!("Rank {}", self.force.allocation().rank),
            ),
        };
        text(
            &mut self.canvas,
            TextFamily::Display,
            format_args!("{title}"),
            frame.rect(1_224.0, 860.0, 600.0, 44.0),
            36.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::End,
        );
        text(
            &mut self.canvas,
            TextFamily::Body,
            format_args!("{detail}"),
            frame.rect(1_224.0, 904.0, 600.0, 26.0),
            18.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        let _ = self.canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(1_824.0 - 120.0, 850.0, 120.0, 1.5),
            color: color::alpha(color::GOLD, 0.7),
        });
    }

    /// The keys of what has the keyboard, right-aligned at the bottom.
    fn sjk_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let mut keys: Vec<(&[&str], &str)> = Vec::with_capacity(4);
        if self.name_editing || self.search_editing || self.numeric.is_some() {
            keys.extend([(&["Enter"][..], "done"), (&["Esc"][..], "cancel")]);
        } else {
            let row = self.sjk_row_kind();
            keys.push(match row {
                RowKind::Type => (&["Enter"][..], "type"),
                RowKind::Act => (&["Enter"][..], "do it"),
                RowKind::Pick => (&["Arrows"][..], "choose"),
                RowKind::Step => (&["Left", "Right"][..], "change"),
            });
            let next = if self.hub {
                crate::profile_hub::Tab::of_player_page(self.page.index())
                    .next(true)
                    .label()
            } else {
                TABS[(self.page.index() + 1) % TABS.len()]
            };
            keys.push((&["Tab"][..], next));
        }
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * keys.len().saturating_sub(1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.canvas, caps, action, x, y, s) + gap;
        }
    }

    /// What kind of row has the keyboard.
    fn sjk_row_kind(&self) -> RowKind {
        match self.page {
            ProfilePage::Character => match self.character_rows().get(self.selected) {
                Some(CharacterRow::Name | CharacterRow::Search) => RowKind::Type,
                Some(CharacterRow::Apply) => RowKind::Act,
                Some(CharacterRow::Grid) => RowKind::Pick,
                _ => RowKind::Step,
            },
            ProfilePage::Saber => match self.saber_rows().get(self.selected) {
                Some(SaberRow::Search) => RowKind::Type,
                _ => RowKind::Step,
            },
            ProfilePage::Force if self.selected >= FORCE_RESET_ROW => RowKind::Act,
            ProfilePage::Force => RowKind::Step,
        }
    }

    /// Where a slider row's track lies (window pixels), for the pointer:
    /// its rectangle is the track's ([`Self::sjk_targets`]).
    pub(super) fn sjk_slider_ratio(rect: sjk_ui::Rect, x: f32) -> f32 {
        // The registered rectangle runs 10 frame pixels past each end.
        let inset = rect.width * 10.0 / (TRACK_WIDTH + 20.0);
        ((x - rect.x - inset) / (rect.width - 2.0 * inset)).clamp(0.0, 1.0)
    }
}

/// What a row does with the keyboard, for the keys' line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RowKind {
    Type,
    Step,
    Act,
    /// The Character page's tiles: the arrows choose one, Enter picks it.
    Pick,
}

/// A power's name in sentence case ("Mind trick").
struct SentenceCase<'a>(&'a str);

impl std::fmt::Display for SentenceCase<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut words = self.0.split(' ');
        if let Some(first) = words.next() {
            formatter.write_str(first)?;
        }
        for word in words {
            write!(formatter, " {}", word.to_lowercase())?;
        }
        Ok(())
    }
}

/// A `color1` value's blade as words ("blue blade").
fn blade_word(index: u8) -> &'static str {
    match sjk_client::SaberColor::from_index(index) {
        Ok(sjk_client::SaberColor::Red) => "red blade",
        Ok(sjk_client::SaberColor::Orange) => "orange blade",
        Ok(sjk_client::SaberColor::Yellow) => "yellow blade",
        Ok(sjk_client::SaberColor::Green) => "green blade",
        Ok(sjk_client::SaberColor::Purple) => "purple blade",
        Ok(sjk_client::SaberColor::Rgb) => "custom blade",
        _ => "blue blade",
    }
}

pub(super) fn rgb_color(rgb: [u8; 3]) -> Color {
    Color::new(
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
        1.0,
    )
}

/// Dark behind the form on the left, fading out before the model; light
/// fades at the top behind the title and at the bottom behind the keys.
fn scrims(canvas: &mut crate::menu_widgets::MenuCanvas, viewport: [f32; 2], frame: &Frame) {
    let [width, height] = viewport;
    let space = |alpha| color::alpha(color::SPACE, alpha);
    let x = |frame_x: f32| frame.point(frame_x, 0.0)[0];
    fade_across(
        canvas,
        sjk_ui::Rect::new(0.0, 0.0, x(820.0), height),
        space(0.9),
        space(0.78),
    );
    fade_across(
        canvas,
        sjk_ui::Rect::new(x(820.0), 0.0, x(1_120.0) - x(820.0), height),
        space(0.78),
        space(0.0),
    );
    let y = |frame_y: f32| frame.point(0.0, frame_y)[1];
    fade(
        canvas,
        sjk_ui::Rect::new(0.0, 0.0, width, y(170.0)),
        space(0.55),
        space(0.0),
    );
    fade(
        canvas,
        sjk_ui::Rect::new(0.0, y(840.0), width, height - y(840.0)),
        space(0.0),
        space(0.7),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // The grid's eight tiles fill the column.
    // Dual sabers (the style, the hilt lists, the blade and eight blade colour
    // rows) end above the keys, as one saber with its six-line list does.
    const _: () = assert!(
        ROWS_TOP
            + ROW
            + 8.0
            + HILT_HEAD
            + DUAL_HILT_ROWS as f32 * HILT_ROW
            + 12.0
            + 10.0
            + 9.0 * ROW
            < BOTTOM
    );
    const _: () = assert!(
        ROWS_TOP + ROW + 8.0 + HILT_HEAD + HILT_ROWS as f32 * HILT_ROW + 12.0 + 10.0 + 5.0 * ROW
            < BOTTOM
    );
    // Dual's search field leaves "Left hand" room at the largest text
    // (`ui_textScale` 1.2; Rajdhani SemiBold at 22 is 8.5 pixels a character).
    const _: () = assert!(
        (COLUMN_WIDTH - GUTTER) * 0.5 - 12.0 - DUAL_SEARCH_WIDTH - 22.0 > 1.2 * 8.5 * 9.0 + 8.0
    );
    // The blade choice's swatches leave the row's name room.
    const _: () = assert!(SKIN_X - LABEL_X > 110.0);

    #[test]
    fn the_blade_choice_scrolls_to_keep_the_chosen_one_shown() {
        // Every blade skin and the stock one: more than the row shows.
        let count = crate::unlockables::ALL.len() + 1;
        assert!(count > SKIN_SHOWN);
        for chosen in 0..count {
            let first = skin_window(chosen, count);
            assert!((first..first + SKIN_SHOWN).contains(&chosen), "{chosen}");
            assert!(first + SKIN_SHOWN <= count);
        }
        assert_eq!(skin_window(0, count), 0);
        assert_eq!(skin_window(count - 1, count), count - SKIN_SHOWN);
        // Few enough to show them all: no scrolling.
        assert_eq!(skin_window(3, 4), 0);
    }
    // Force: the groups under the sides, the actions under Lightsaber, and the
    // power box above the keys.
    const _: () = assert!(
        METER_Y
            + 22.0
            + SIDE_HEIGHT
            + 22.0
            + 2.0 * GROUP_HEADING
            + 8.0 * CELL
            + GROUP_GAP
            + 24.0
            + 48.0
            < BOTTOM
    );
    const _: () = assert!(POWER_BOX[1] + POWER_BOX[3] < KEYS_Y - 20.0);
    // A power's name keeps room beside its three level cells.
    const _: () =
        assert!((COLUMN_WIDTH - GUTTER) * 0.5 - CELL - 10.0 - 3.0 * DISC - 2.0 * DISC_GAP > 120.0);
    // The token ranges stay apart: the levels, the blade choices, the style
    // buttons, the lists' wheel areas, then the hilts.
    const _: () = assert!(LEVEL_BASE + 18 * 3 <= SKIN_BASE);
    const _: () = assert!(SKIN_BASE + (crate::unlockables::ALL.len() + 1) as u16 <= SKIN_SCROLL);
    const _: () = assert!(STYLE_BASE + 3 <= HILT_SCROLL);
    const _: () = assert!(HILT_SCROLL + 2 <= HILT_BASE);

    /// The screen in the SJK UI on `page`, Dual sabers when `dual`, drawn at
    /// 1080 lines with a rank 7 profile that has bought nothing.
    fn drawn(page: ProfilePage, dual: bool) -> PlayerMenu {
        let mut menu = PlayerMenu::new();
        menu.set_sjk(true);
        menu.force.load_template("fresh", "7-1-000000000000000000");
        if dual {
            menu.saber.set_style(SaberStyle::Dual, None);
        }
        menu.set_page(page);
        draw(&mut menu);
        menu
    }

    fn draw(menu: &mut PlayerMenu) {
        let font = crate::text::load_modern(1.0, None).unwrap().font;
        let mut vertices = Vec::new();
        menu.append_sjk(
            TextTarget::Inter(&mut vertices, &font),
            [1920.0, 1080.0],
            1.0,
        );
    }

    /// As the Profile screen's first tabs every page draws the screen's row of tabs in
    /// place of its own three, at the same place, this page lit, within the canvas.
    #[test]
    fn the_profile_screens_row_stands_in_place_of_the_pages_tabs() {
        for page in ProfilePage::ALL {
            let mut menu = drawn(page, page == ProfilePage::Saber);
            assert!(menu.canvas.rect_for(crate::profile_hub::TOKEN).is_none());
            let own = menu.canvas.rect_for(TAB_BASE).expect("the page's tabs");
            menu.set_hub(true);
            draw(&mut menu);
            assert!(!menu.canvas.overflowed(), "{page:?}");
            assert!(menu.canvas.rect_for(TAB_BASE).is_none(), "{page:?}");
            let first = menu
                .canvas
                .rect_for(crate::profile_hub::TOKEN)
                .expect("the row");
            assert_eq!((first.x, first.y), (own.x, own.y), "{page:?}");
            for tab in crate::profile_hub::Screen::Profile.tabs() {
                let token = crate::profile_hub::TOKEN + tab.index() as u16;
                assert!(menu.canvas.rect_for(token).is_some(), "{tab:?}");
            }
        }
        const _: () = assert!(crate::profile_hub::TOKEN > HILT_BASE + 2 * HILT_STRIDE);
    }

    /// The retail hilts stand under their heading, the community's under theirs, each
    /// section starting on a line of its own; with no retail known the list is as before.
    #[test]
    fn hilt_sections_place_each_hilt_under_its_heading() {
        let lines = HiltLines {
            total: 7,
            retail: 3,
            columns: 2,
            headed: true,
        };
        // 0 heading, 1 and 2 the retail rows, 3 heading, 4 and 5 the community's.
        assert_eq!(lines.lines(), 6);
        assert_eq!(lines.place(0), (1, 0));
        assert_eq!(lines.place(1), (1, 1));
        assert_eq!(lines.place(2), (2, 0));
        assert_eq!(lines.place(3), (4, 0));
        assert_eq!(lines.place(6), (5, 1));
        assert_eq!(lines.heading(0), Some(("Jedi Academy", 3)));
        assert_eq!(lines.heading(3), Some(("Community", 4)));
        assert_eq!(lines.heading(1), None);
        let plain = HiltLines {
            total: 5,
            retail: 0,
            columns: 2,
            headed: false,
        };
        assert_eq!(plain.lines(), 3);
        assert_eq!(plain.place(3), (1, 1));
        assert_eq!(plain.heading(0), None);
        // Only retail ones listed (a search): one heading.
        let only = HiltLines {
            total: 3,
            retail: 3,
            columns: 2,
            headed: true,
        };
        assert_eq!(only.lines(), 3);
        assert_eq!(only.heading(0), Some(("Jedi Academy", 3)));
        assert_eq!(only.heading(2), None);
    }

    #[test]
    fn level_and_hilt_tokens_round_trip() {
        for power in 0..POWER_NAMES.len() {
            for level in 1..=3 {
                assert_eq!(level_of(level_token(power, level)), Some((power, level)));
            }
        }
        assert_eq!(level_of(LEVEL_BASE - 1), None);
        assert_eq!(level_of(level_token(17, 3) + 1), None);
        for second in [false, true] {
            for index in [0, 1, 58, 255] {
                assert_eq!(hilt_of(hilt_token(second, index)), Some((second, index)));
            }
        }
        assert_eq!(hilt_of(HILT_BASE - 1), None);
    }

    /// A press and release of the primary button at `position`.
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

    /// Every chip of the blade row takes the click aimed at it: its centre
    /// and its edges. Measured against the form's value zone, a click picked
    /// a chip to the left of the one under the pointer.
    #[test]
    fn a_click_on_a_blade_chip_chooses_that_colour() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut menu = drawn(ProfilePage::Saber, false);
        let row = menu
            .saber_rows()
            .iter()
            .position(|row| row.is_blade())
            .expect("a blade row");
        let control = menu.canvas.rect_for(row as u16).expect("the chips' area");
        let share = control.width / PALETTE.len() as f32;
        let middle = control.y + control.height * 0.5;
        for (chip, colour) in PALETTE.iter().enumerate().rev() {
            for offset in [0.5, 0.15, 0.85] {
                let x = control.x + share * (chip as f32 + offset);
                click(&mut menu, &mut console, sjk_ui::Vec2::new(x, middle));
                assert_eq!(menu.saber.color(false), *colour, "chip {chip} at {offset}");
                draw(&mut menu);
            }
        }
    }

    /// A click on a power's name only chooses its row; its marks buy.
    #[test]
    fn a_click_on_a_powers_name_does_not_change_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut menu = drawn(ProfilePage::Force, false);
        let push = 3;
        let mark = menu
            .canvas
            .rect_for(level_token(push, 2))
            .expect("Push's second level");
        let centre = sjk_ui::Vec2::new(mark.x + mark.width * 0.5, mark.y + mark.height * 0.5);
        click(&mut menu, &mut console, centre);
        assert_eq!(menu.force.allocation().levels[push], 2);
        draw(&mut menu);
        let row = menu
            .canvas
            .rect_for((FORCE_POWER_ROW + push) as u16)
            .expect("Push's row");
        // Its holocron, the start of its name and its end, short of the marks.
        for share in [0.05, 0.3, 0.55] {
            let name = sjk_ui::Vec2::new(row.x + row.width * share, row.y + row.height * 0.5);
            click(&mut menu, &mut console, name);
            assert_eq!(menu.selected, FORCE_POWER_ROW + push);
            assert_eq!(
                menu.force.allocation().levels[push],
                2,
                "a click at {share}"
            );
            draw(&mut menu);
        }
    }

    #[test]
    fn the_force_page_groups_powers_as_classic_plus_and_skips_the_other_side() {
        use super::super::classic::layout::{DARK_POWERS, LIGHT_POWERS};
        let light = force_key_order(ForceSide::Light);
        // The sides, thirteen powers, the three actions.
        assert_eq!(light.len(), 1 + 13 + 3);
        assert_eq!(light[0], FORCE_SIDE_ROW);
        assert_eq!(
            &light[light.len() - 3..],
            [FORCE_RESET_ROW, FORCE_DISCARD_ROW, FORCE_APPLY_ROW]
        );
        // Neutral first (Jump), then the side's (Absorb), then Lightsaber.
        assert_eq!(light[1], FORCE_POWER_ROW + 1);
        assert_eq!(light[6], FORCE_POWER_ROW + usize::from(LIGHT_POWERS[0]));
        assert_eq!(light[13], FORCE_POWER_ROW + 17);
        for power in DARK_POWERS {
            assert!(!light.contains(&(FORCE_POWER_ROW + usize::from(power))));
        }
        let menu = drawn(ProfilePage::Force, false);
        for (power, name) in POWER_NAMES.iter().enumerate() {
            let shown = light.contains(&(FORCE_POWER_ROW + power));
            for level in 1..=3 {
                assert_eq!(
                    menu.canvas.rect_for(level_token(power, level)).is_some(),
                    shown,
                    "{name} level {level}",
                );
            }
        }
        assert!(!menu.canvas.overflowed());
    }

    #[test]
    fn a_level_cell_buys_up_to_it_and_a_right_click_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut menu = drawn(ProfilePage::Force, false);
        let push = 3;
        let left = menu.force.remaining_points();
        let click = |menu: &mut PlayerMenu, console: &mut ViewerConsole, level| {
            menu.sjk_pointer(
                sjk_ui::UiEventKind::Activate,
                level_token(push, level),
                0.0,
                console,
            )
        };
        assert!(click(&mut menu, &mut console, 2).is_some());
        assert_eq!(menu.force.allocation().levels[push], 2);
        assert_eq!(menu.selected, FORCE_POWER_ROW + push);
        assert_eq!(menu.force.remaining_points(), left - 1 - 3);
        // A click on the power's own level, or below it, changes nothing.
        click(&mut menu, &mut console, 2);
        click(&mut menu, &mut console, 1);
        assert_eq!(menu.force.allocation().levels[push], 2);
        // A right click on a level removes it, from the pointer's own events.
        let right_click = |menu: &mut PlayerMenu, console: &mut ViewerConsole, level| {
            let rect = menu
                .canvas
                .rect_for(level_token(push, level))
                .expect("a level mark");
            let position = sjk_ui::Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
            let button = sjk_ui::PointerButton::Secondary;
            for event in [
                sjk_ui::InputEvent::PointerMove(position),
                sjk_ui::InputEvent::PointerPress { position, button },
                sjk_ui::InputEvent::PointerRelease { position, button },
            ] {
                let _ = menu.handle_pointer(event, console);
            }
        };
        right_click(&mut menu, &mut console, 3);
        assert_eq!(
            menu.force.allocation().levels[push],
            2,
            "a mark above the level has nothing to remove"
        );
        right_click(&mut menu, &mut console, 2);
        assert_eq!(menu.force.allocation().levels[push], 1);
        draw(&mut menu);
        right_click(&mut menu, &mut console, 1);
        assert_eq!(menu.force.allocation().levels[push], 0);
        assert_eq!(menu.force.remaining_points(), left);
        // The box at the bottom right now shows Push.
        assert_eq!(menu.sjk_box_power(), Some(push));
    }

    #[test]
    fn the_style_buttons_choose_the_style() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut menu = drawn(ProfilePage::Saber, false);
        for (index, style) in STYLES.into_iter().enumerate() {
            assert!(menu.canvas.rect_for(STYLE_BASE + index as u16).is_some());
            menu.sjk_pointer(
                sjk_ui::UiEventKind::Activate,
                STYLE_BASE + index as u16,
                0.0,
                &mut console,
            );
            assert_eq!(menu.saber.style(), style);
        }
        draw(&mut menu);
        assert!(!menu.canvas.overflowed());
    }

    #[test]
    fn dual_sabers_take_both_lists_before_the_blades() {
        let menu = drawn(ProfilePage::Saber, true);
        let rows = menu.saber_rows();
        let order: Vec<SaberRow> = menu
            .sjk_key_order()
            .unwrap()
            .into_iter()
            .map(|index| rows[index])
            .collect();
        assert_eq!(
            order[..6],
            [
                SaberRow::Style,
                SaberRow::Search,
                SaberRow::Hilt,
                SaberRow::SecondHilt,
                SaberRow::Skin,
                SaberRow::Blade
            ]
        );
        assert_eq!(order.len(), rows.len());
        // Both lists take the pointer, and the page fits its canvas.
        for list in 0..2 {
            assert!(menu.canvas.rect_for(HILT_SCROLL + list).is_some());
        }
        assert!(!menu.canvas.overflowed());
        assert!(
            drawn(ProfilePage::Character, false)
                .sjk_key_order()
                .is_none()
        );
    }

    /// The screen in the SJK UI on its Saber page over a catalogue of five single
    /// hilts and a staff, drawn once.
    fn with_hilts() -> PlayerMenu {
        let source: String = [
            ("single_1", "Katarn", "SABER_SINGLE"),
            ("single_2", "Arbiter", "SABER_SINGLE"),
            ("single_3", "Kazeshini", "SABER_SINGLE"),
            ("single_4", "Dark katana", "SABER_SINGLE"),
            ("single_5", "Praetor", "SABER_SINGLE"),
            ("staff_1", "Reborn staff", "SABER_STAFF"),
        ]
        .map(|(id, name, kind)| format!("{id}\n{{\n\tname \"{name}\"\n\tsaberType {kind}\n}}\n"))
        .concat();
        let mut vfs = sjk_vfs::VirtualFileSystem::new();
        vfs.mount_memory("base", [("ext_data/sabers/test.sab", source.into_bytes())])
            .unwrap();
        let mut menu = PlayerMenu::new();
        menu.set_sjk(true);
        menu.attach_catalogue(Arc::new(vfs));
        let loader = menu.loader.as_mut().unwrap();
        loader.request();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while loader.catalog().is_none() && std::time::Instant::now() < deadline {
            loader.poll();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(catalog_of(&menu.loader).unwrap().saber_hilts.len(), 6);
        menu.set_page(ProfilePage::Saber);
        draw(&mut menu);
        menu
    }

    /// The display names the first list shows.
    fn shown_hilts(menu: &PlayerMenu) -> Vec<&str> {
        let catalog = catalog_of(&menu.loader).unwrap();
        menu.saber
            .listed(catalog, false)
            .filter(|(index, _)| menu.canvas.rect_for(hilt_token(false, *index)).is_some())
            .map(|(_, hilt)| hilt.display_name.as_str())
            .collect()
    }

    /// The search types from its field (Enter, or a click on it) and lists the hilts
    /// whose name holds it, ignoring case; Left and Right keep to those, a click on
    /// one chooses that hilt, an empty result says so, and Escape clears the search
    /// before it leaves.
    #[test]
    fn the_hilt_search_lists_the_matching_hilts_and_the_keys_keep_to_them() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let mut menu = with_hilts();
        assert_eq!(
            shown_hilts(&menu),
            ["Katarn", "Arbiter", "Kazeshini", "Dark katana", "Praetor"]
        );
        let search = menu.saber_row(SaberRow::Search).expect("a search row");
        let hilts = menu.saber_row(SaberRow::Hilt).expect("a hilt row");
        assert!(search < hilts);
        // A click on the field starts typing.
        let field = menu.canvas.rect_for(search as u16).expect("the field");
        click(
            &mut menu,
            &mut console,
            sjk_ui::Vec2::new(field.x + field.width * 0.5, field.y + field.height * 0.5),
        );
        assert!(menu.search_editing && menu.typing());
        assert_eq!(menu.selected, search);
        assert_eq!(menu.sjk_row_kind(), RowKind::Type);
        menu.saber.set_search("KAT");
        draw(&mut menu);
        assert_eq!(shown_hilts(&menu), ["Katarn", "Dark katana"]);
        menu.search_editing = false;
        // Left and Right step through the matches only.
        menu.selected = hilts;
        menu.adjust(&mut console, 1);
        assert_eq!(menu.saber.hilt(false), "single_4");
        menu.adjust(&mut console, 1);
        assert_eq!(menu.saber.hilt(false), "single_1");
        menu.adjust(&mut console, -1);
        assert_eq!(console.text_value("saber1"), Some("single_4"));
        // A click picks the hilt drawn under it.
        draw(&mut menu);
        let katarn = menu.canvas.rect_for(hilt_token(false, 0)).expect("Katarn");
        click(
            &mut menu,
            &mut console,
            sjk_ui::Vec2::new(katarn.x + 10.0, katarn.y + katarn.height * 0.5),
        );
        assert_eq!(menu.saber.hilt(false), "single_1");
        // Nothing matches: the list says so and the keys leave the hilt be.
        menu.saber.set_search("zzz");
        draw(&mut menu);
        assert!(shown_hilts(&menu).is_empty());
        assert!(!menu.canvas.overflowed());
        menu.adjust(&mut console, 1);
        assert_eq!(menu.saber.hilt(false), "single_1");
        // Escape clears the search, then leaves.
        assert_eq!(menu.escape(), PlayerMenuResult::None);
        assert_eq!(menu.saber.search(), "");
        assert_eq!(
            menu.escape(),
            PlayerMenuResult::Back(ReturnTarget::MainMenu)
        );
        // The search filters both of Dual's lists, and Staff's staves.
        menu.saber.set_search("ka");
        menu.saber
            .set_style(SaberStyle::Dual, catalog_of(&menu.loader));
        draw(&mut menu);
        assert!(!menu.canvas.overflowed());
        let catalog = catalog_of(&menu.loader).unwrap();
        assert_eq!(menu.saber.listed(catalog, true).count(), 3);
        menu.saber
            .set_style(SaberStyle::Staff, catalog_of(&menu.loader));
        menu.saber.set_search("reborn");
        let catalog = catalog_of(&menu.loader).unwrap();
        assert_eq!(menu.saber.listed(catalog, false).count(), 1);
    }

    /// The blade row offers the stock blade and every skin owned, drawn as the
    /// Collection's swatches with all their effects within the canvas (Dual too); a
    /// click on one wears it as Equip does, and the caption names it.
    #[test]
    fn the_blade_row_offers_the_owned_skins_and_a_click_wears_one() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        // Every skin with every effect (lightning, motes, a turning hue): the most
        // shapes a swatch draws.
        let looks = || {
            let sample = crate::blade_skin_file::tests::sample_with_effects();
            let skins = crate::unlockables::blade_skins()
                .map(|skin| {
                    let def = crate::blade_skin_file::parse(skin.id, &sample).unwrap();
                    let packs = sjk_vfs::VirtualFileSystem::new();
                    crate::saber_skins::LoadedSkin::new(skin.id, def, &packs).unwrap()
                })
                .collect();
            Arc::new(crate::saber_skins::LoadedSkins::of(skins, 1))
        };
        for dual in [false, true] {
            let mut menu = drawn(ProfilePage::Saber, dual);
            menu.blade_choice.set_looks(looks());
            menu.blade_choice.preview = Some(
                crate::unlockables::ALL
                    .iter()
                    .map(|unlockable| sjk_identity::Unlock {
                        id: unlockable.id.to_owned(),
                        granted: 1_791_336_225,
                        note: String::new(),
                        count: 1,
                        medal: None,
                    })
                    .collect(),
            );
            menu.blade_choice.read(&console);
            // The arcs strike and the flare runs at some times only.
            for _ in 0..8 {
                draw(&mut menu);
                assert!(!menu.canvas.overflowed(), "dual {dual}");
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            let row = menu.saber_row(SaberRow::Skin).expect("a blade row");
            // The stock blade chosen: the first six shown, the rest scrolled off.
            let choices = crate::unlockables::ALL.len() + 1;
            for choice in 0..choices {
                assert_eq!(
                    menu.canvas.rect_for(SKIN_BASE + choice as u16).is_some(),
                    choice < SKIN_SHOWN,
                    "{choice}"
                );
            }
            // The second skin of the row, the rarest listed first.
            let second = menu.canvas.rect_for(SKIN_BASE + 2).unwrap();
            click(
                &mut menu,
                &mut console,
                sjk_ui::Vec2::new(
                    second.x + second.width * 0.5,
                    second.y + second.height * 0.5,
                ),
            );
            assert_eq!(menu.selected, row);
            assert_eq!(
                console.text_value(crate::unlockables::SABER_SKIN_CVAR),
                Some(crate::unlockables::blade_skins_by_tier()[1].id)
            );
            assert_eq!(menu.blade_choice.chosen(), 2);
            // The keys step along it, back to the stock blade.
            menu.adjust(&mut console, -1);
            menu.adjust(&mut console, -1);
            assert_eq!(
                console.text_value(crate::unlockables::SABER_SKIN_CVAR),
                Some("")
            );
        }
    }

    #[test]
    fn opened_from_a_game_the_model_stands_in_its_own_preview() {
        let mut menu = drawn(ProfilePage::Character, false);
        // On the menu map the stage holds the model.
        assert_eq!(menu.model_preview(), None);
        menu.return_target = ReturnTarget::InGame;
        let preview = menu.model_preview().expect("a preview in game");
        assert_eq!(preview.area, PreviewArea::Sjk(MODEL_AREA));
        assert!(preview.sabers && !preview.showcase);
        assert!(preview.angle.is_some() && preview.room > 1.0);
        let previews = |menu: &PlayerMenu| {
            menu.canvas
                .draw_list()
                .commands()
                .iter()
                .filter(|command| {
                    matches!(command, DrawCommand::TexturedQuad { texture, .. }
                        if *texture == crate::ui_renderer::PREVIEW_TEXTURE)
                })
                .count()
        };
        // Until the renderer has a frame, its shadow alone; then the model.
        draw(&mut menu);
        assert_eq!(previews(&menu), 0);
        menu.set_preview_ready(true);
        for page in ProfilePage::ALL {
            menu.set_page(page);
            draw(&mut menu);
            assert_eq!(previews(&menu), 1, "{page:?}");
            assert!(!menu.canvas.overflowed());
        }
    }

    #[test]
    fn power_names_read_in_sentence_case() {
        assert_eq!(SentenceCase("Mind Trick").to_string(), "Mind trick");
        assert_eq!(SentenceCase("Heal").to_string(), "Heal");
        assert_eq!(SentenceCase("Team Energize").to_string(), "Team energize");
    }

    #[test]
    fn a_sliders_pointer_maps_its_track_ends_to_the_range_ends() {
        let track = sjk_ui::Rect::new(100.0, 0.0, TRACK_WIDTH + 20.0, 32.0);
        assert_eq!(PlayerMenu::sjk_slider_ratio(track, 110.0), 0.0);
        assert_eq!(
            PlayerMenu::sjk_slider_ratio(track, 110.0 + TRACK_WIDTH),
            1.0
        );
        assert!(
            (PlayerMenu::sjk_slider_ratio(track, 110.0 + TRACK_WIDTH * 0.5) - 0.5).abs() < 1e-4
        );
        assert_eq!(PlayerMenu::sjk_slider_ratio(track, 0.0), 0.0);
    }

    /// The Force page on servers with every rule to show: each power's note
    /// and the server's panel stay in the draw list's room, and the panel ends
    /// above the keys (a debug assertion in the panel checks its last line).
    #[test]
    fn the_force_page_shows_a_servers_rules_within_its_room() {
        // None, two, seven, the eight longest names (the most a list shows),
        // and all of them.
        let longest = [5, 7, 9, 11, 12, 15, 16, 17].map(|bit| 1_u32 << bit);
        let longest = longest.iter().fold(0, |mask, bit| mask | bit);
        for mask in [
            0,
            0b100_0001,
            0b1_1110_0000_0110_0001,
            longest,
            (1 << 18) - 1,
        ] {
            let mut menu = PlayerMenu::new();
            menu.set_sjk(true);
            menu.force.load_on_server(
                "7-2-031330310000030333",
                sjk_client::ForceLegalizeRules {
                    max_rank: 5,
                    free_saber: true,
                    team_side: None,
                    gametype: 0,
                    disabled_mask: mask,
                },
            );
            menu.set_page(ProfilePage::Force);
            menu.selected = FORCE_POWER_ROW; // the power box, with its note
            draw(&mut menu);
            let list = menu.canvas.draw_list();
            assert!(
                list.len() < list.limit(),
                "mask {mask:b}: the draw list is full"
            );
            let (text, slots) = menu.canvas.text_budget();
            assert!(text < slots, "mask {mask:b}: {text} of {slots} text runs");
        }
        assert_eq!(server_off_list(0b100_0011), "Heal, Jump 1, Grip");
    }

    /// A model chosen on the Character page shows (it is the stage's model) but the
    /// `model` cvar keeps the one worn until the Apply button is pressed; leaving the
    /// screen drops it.
    #[test]
    fn a_model_chosen_waits_for_apply() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        console.set_cvar("model", "kyle/default");
        let mut menu = drawn(ProfilePage::Character, false);
        menu.open(&console, ReturnTarget::MainMenu);
        assert!(!menu.look_pending());

        menu.draft.model = "jan/default".to_owned();
        draw(&mut menu);
        assert!(!menu.canvas.overflowed());
        assert!(menu.look_pending());
        assert_eq!(menu.stage_model(), "jan/default");
        assert_eq!(console.text_value("model"), Some("kyle/default"));

        let apply = menu
            .character_rows()
            .iter()
            .position(|row| *row == CharacterRow::Apply)
            .expect("an Apply row");
        let rect = menu.canvas.rect_for(apply as u16).expect("the button");
        let centre = sjk_ui::Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
        click(&mut menu, &mut console, centre);
        assert_eq!(console.text_value("model"), Some("jan/default"));
        assert!(!menu.look_pending());

        menu.draft.model = "kyle/default".to_owned();
        assert!(menu.look_pending());
        menu.settle_force_page(false, &mut console);
        assert_eq!(menu.stage_model(), "jan/default");
        assert_eq!(console.text_value("model"), Some("jan/default"));
    }
}

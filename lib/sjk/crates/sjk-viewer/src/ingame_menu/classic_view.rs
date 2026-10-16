//! Drawing of the classic in-game menu: the retail bar along the top and
//! the open page's pop-up under its button, on the 640x480 canvas fitted
//! into the window. The match stays visible everywhere else, as in retail.
//!
//! With the player's retail artwork loaded the bar is `menu_top_mp`, the
//! pop-ups `menu_box_ingame` and the focus glow `menu_buttonback` (both
//! flickering as retail's shaders do); without it, SJK draws the same
//! layout with flat shapes. The focused item pulses, and labels are in
//! retail's capitals, except player and map names in vote lists and the
//! values of the about pop-up.

use super::classic::{self, BAR_HEIGHT, BAR_TOKEN, INFO_LINE, Tab};
use super::{Page, VOTE_SCROLL_TOKEN, View};
use crate::menu::art::{ArtPiece, ArtSet};
use crate::menu::classic::layout::Placement;
use crate::menu::classic::view::{
    Caps, DISABLED, FOCUS, GOLD, TITLE, art, focus_pulse, glow, gold, ink,
};
use crate::menu_widgets::MenuCanvas;
use sjk_ui::{DrawCommand, FontWeight, Gradient, Rect, TextAlign};

/// The prepared entries of one page.
pub(super) struct Rows<'a> {
    pub(super) labels: &'a [String],
    pub(super) enabled: &'a [bool],
    /// `(first visible, total)` of a scrolling vote list.
    pub(super) scroll: Option<(usize, usize)>,
    /// Read-only lines shown above the entries.
    pub(super) info: &'a [String],
}

/// Build the bar and the open pop-up of `view` into `canvas`.
pub(super) fn build(
    canvas: &mut MenuCanvas,
    view: &View<'_>,
    rows: Rows<'_>,
    art_set: ArtSet,
    viewport: [f32; 2],
) {
    let place = Placement::new(viewport);
    canvas.begin_transparent(viewport);
    bar(canvas, view, &place, art_set, viewport);
    if view.page != Page::Main {
        popup(canvas, view, &rows, &place, art_set);
    }
    let selected = if view.page == Page::Main {
        BAR_TOKEN + view.selected_row as u16
    } else {
        view.selected_row as u16
    };
    canvas.finish(selected);
}

/// The retail button bar, stretched across the window's width with its
/// buttons on the fitted canvas.
fn bar(
    canvas: &mut MenuCanvas,
    view: &View<'_>,
    place: &Placement,
    art_set: ArtSet,
    viewport: [f32; 2],
) {
    let s = place.scale;
    let strip = Rect::new(0.0, place.origin[1], viewport[0], BAR_HEIGHT * s);
    if art_set.has(ArtPiece::TopBar) {
        art(canvas, ArtPiece::TopBar, strip);
    } else {
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::GradientRect {
            rect: strip,
            radius: 0.0,
            gradient: Gradient {
                start: ink(0.82),
                end: ink(0.62),
                vertical: true,
            },
        });
        let _ = draw.push(DrawCommand::GradientRect {
            rect: Rect::new(0.0, strip.bottom() - s, viewport[0], s),
            radius: 0.0,
            gradient: Gradient {
                start: gold(0.0),
                end: gold(0.45),
                vertical: false,
            },
        });
    }
    let owner = Tab::of_page(view.page);
    let mut described = (view.page == Page::Main).then_some(view.selected_row);
    for tab in Tab::ALL {
        let token = BAR_TOKEN + tab.index() as u16;
        let target = place.rect(tab.rect());
        let hovered = canvas.token_hovered(token);
        if hovered {
            described = Some(tab.index());
        }
        let enabled = tab.unavailable(view.siege).is_none();
        let focused = view.page == Page::Main && view.selected_row == tab.index();
        let active = focused || hovered || owner == Some(tab);
        if active && enabled {
            glow(canvas, target, s, art_set);
        }
        // The focused button pulses; the open page's stays steady white.
        let color = match (enabled, focused || hovered, active) {
            (false, _, _) => DISABLED,
            (true, true, _) => focus_pulse(),
            (true, false, true) => FOCUS,
            (true, false, false) => GOLD,
        };
        canvas.text_fmt_aligned(
            format_args!("{}", Caps(tab.label(view.siege))),
            Rect::new(target.x, target.y + 9.0 * s, target.width, 15.0 * s),
            12.0 * s,
            color,
            FontWeight::Semibold,
            0.3 * s,
            TextAlign::Center,
        );
        canvas.hit_region(token, target);
    }
    // Retail has no description line; SJK only explains dimmed buttons,
    // and only while no pop-up covers the space under the bar.
    let note = described
        .filter(|_| owner.is_none())
        .and_then(|index| classic::note(Page::Main, index, view.siege));
    if let Some(note) = note {
        canvas.text_aligned(
            note,
            place.rect([0.0, BAR_HEIGHT + 6.0, 640.0, 14.0]),
            11.0 * s,
            DISABLED,
            FontWeight::Regular,
            0.2 * s,
            TextAlign::Center,
        );
    }
}

/// The open page's pop-up: a dark box under its bar button holding the
/// page's read-only lines and entries.
fn popup(
    canvas: &mut MenuCanvas,
    view: &View<'_>,
    rows: &Rows<'_>,
    place: &Placement,
    art_set: ArtSet,
) {
    let s = place.scale;
    let count = rows.labels.len();
    let [x, y, width, height] = classic::popup(view.page, count, rows.info.len());
    let frame = place.rect([x, y, width, height]);
    if art_set.has(ArtPiece::PopupBox) {
        art(canvas, ArtPiece::PopupBox, frame);
    } else {
        let draw = canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::SolidRect {
            rect: frame,
            color: ink(0.80),
        });
        let _ = draw.push(DrawCommand::Border {
            rect: frame,
            radius: 0.0,
            width: s.max(1.0),
            color: gold(0.35),
        });
    }
    let mut top = y + 4.0;
    if let Some(heading) = classic::heading(view.page) {
        canvas.text_fmt_aligned(
            format_args!("{}", Caps(heading)),
            place.rect([x, top + 8.0, width, 16.0]),
            13.0 * s,
            TITLE,
            FontWeight::Semibold,
            0.4 * s,
            TextAlign::Center,
        );
        top += 30.0;
    }
    for line in rows.info {
        info_line(canvas, place, line, [x, top, width]);
        top += INFO_LINE;
    }
    let row_height = classic::row_height(count);
    let compact = row_height < 30.0;
    let list = view.page.is_vote_page() || matches!(view.page, Page::Siege | Page::Players);
    let mut described = view.selected_row;
    for (row, label) in rows.labels.iter().enumerate() {
        let token = row as u16;
        let target = place.rect([
            x + 2.0,
            top + row as f32 * row_height,
            width - 4.0,
            row_height,
        ]);
        let hovered = canvas.token_hovered(token);
        if hovered {
            described = row;
        }
        let enabled = rows.enabled.get(row).copied().unwrap_or(true);
        let active = row == view.selected_row || hovered;
        if active && enabled {
            glow(canvas, target, s, art_set);
        }
        let color = match (enabled, active) {
            (false, _) => DISABLED,
            (true, true) => focus_pulse(),
            (true, false) => GOLD,
        };
        let size = if compact { 11.0 } else { 13.5 };
        let (inset, align) = if list {
            (10.0 * s, TextAlign::Start)
        } else {
            (0.0, TextAlign::Center)
        };
        let rect = Rect::new(
            target.x + inset,
            target.y + (target.height - size * 1.2 * s) * 0.5,
            target.width - inset * 2.0,
            size * 1.2 * s,
        );
        // Vote lists hold player and map names, shown as written.
        if list {
            canvas.text_aligned(
                label,
                rect,
                size * s,
                color,
                FontWeight::Semibold,
                0.3 * s,
                align,
            );
        } else {
            canvas.text_fmt_aligned(
                format_args!("{}", Caps(label)),
                rect,
                size * s,
                color,
                FontWeight::Semibold,
                0.3 * s,
                align,
            );
        }
        if let Some((flag, players)) = team_row(view, row) {
            team_badge(canvas, place, art_set, target, flag, players, enabled);
        }
        canvas.hit_region(token, target);
    }
    if let Some((first, total)) = rows.scroll {
        let track = place.rect([x + width - 8.0, top, 4.0, count as f32 * row_height]);
        canvas.scrollbar(
            VOTE_SCROLL_TOKEN,
            track,
            first,
            super::callvote::PAGE_ITEMS,
            total,
        );
    }
    let disabled = !rows.enabled.get(described).copied().unwrap_or(true);
    if let Some(note) = classic::note(view.page, described, view.siege).filter(|_| disabled) {
        canvas.text_aligned(
            note,
            place.rect([x - 120.0, y + height + 4.0, width + 240.0, 14.0]),
            11.0 * s,
            DISABLED,
            FontWeight::Regular,
            0.2 * s,
            TextAlign::Center,
        );
    }
}

/// The flag and player count of Join's team row `row`, in a team game
/// (classic+).
fn team_row(view: &View<'_>, row: usize) -> Option<(ArtPiece, usize)> {
    match (view.page, view.team_game, row) {
        (Page::Team, true, 1) => Some((ArtPiece::RedFlag, view.red_players)),
        (Page::Team, true, 2) => Some((ArtPiece::BlueFlag, view.blue_players)),
        _ => None,
    }
}

/// A team row's flag at its left end and its player count at its right.
fn team_badge(
    canvas: &mut MenuCanvas,
    place: &Placement,
    art_set: ArtSet,
    target: Rect,
    flag: ArtPiece,
    players: usize,
    enabled: bool,
) {
    let s = place.scale;
    let side = 18.0 * s;
    let top = target.y + (target.height - side) * 0.5;
    if art_set.has(flag) {
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: Rect::new(target.x + 6.0 * s, top, side, side),
            texture: flag.texture(),
            color: sjk_ui::Color::new(1.0, 1.0, 1.0, if enabled { 1.0 } else { 0.45 }),
        });
    }
    let line = 11.0 * 1.2 * s;
    canvas.text_fmt_aligned(
        format_args!("{players}"),
        Rect::new(
            target.right() - 26.0 * s,
            target.y + (target.height - line) * 0.5,
            20.0 * s,
            line,
        ),
        11.0 * s,
        DISABLED,
        FontWeight::Semibold,
        0.0,
        TextAlign::End,
    );
}

/// One "label  /  value" line of the about pop-up, laid out as retail's
/// `ingame_about.menu`: the label set against a column 150 units in, the
/// value after it.
fn info_line(canvas: &mut MenuCanvas, place: &Placement, line: &str, [x, y, width]: [f32; 3]) {
    let s = place.scale;
    let (label, value) = line.split_once("  /  ").unwrap_or((line, ""));
    canvas.text_fmt_aligned(
        format_args!("{}", Caps(label)),
        place.rect([x + 10.0, y + 2.0, 140.0, 15.0]),
        12.0 * s,
        GOLD,
        FontWeight::Semibold,
        0.2 * s,
        TextAlign::End,
    );
    canvas.text_aligned(
        value,
        place.rect([x + 160.0, y + 2.0, width - 170.0, 15.0]),
        12.0 * s,
        FOCUS,
        FontWeight::Regular,
        0.2 * s,
        TextAlign::Start,
    );
}

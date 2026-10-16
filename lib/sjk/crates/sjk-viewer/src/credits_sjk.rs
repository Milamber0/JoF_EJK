//! Credits in the SJK UI (`docs/sjk-ui.md`, SJK's pages). SJK's emblem
//! stands on the left as the page's sun: its sunburst turns behind it, golden
//! god rays reach out from it across the page and sparks rise through their
//! light, as on SJK's site; under it the sections down a lit rail and Expand
//! all. The people are in a reading column on the right, over the map darkened
//! as Settings is, without boxes: names in the display family, their medals in
//! a column beside their lines, the folds as rows with the band under the
//! pointer, the days of their work on a thread. The sections, cards, folds,
//! links, scrolling, keys and pointer are the page's own (`credits.rs`); this
//! look lays the pieces out in frame pixels, measured in its families.

use super::*;
use crate::menu::sjk::{
    Frame, TextTarget, color, fade_across, key_hint, key_hint_width, kit, text, top_bar,
};
use crate::menu_widgets::{MAX_WIDGETS, TextFamily};
use crate::text::TextStyle;

/// The sun: the emblem's centre and size, and the line under it.
const SUN: [f32; 2] = [300.0, 330.0];
const EMBLEM: f32 = 220.0;
const CAPTION_Y: f32 = 486.0;
/// The sections down a rail on the left, and Expand all under them.
const RAIL_X: f32 = 96.0;
const RAIL_TOP: f32 = 584.0;
const SECTION_STEP: f32 = 52.0;
const SECTION_HEIGHT: f32 = 44.0;
const RAIL_WIDTH: f32 = 420.0;
const EXPAND: [f32; 2] = [230.0, 46.0];
/// The reading column and the part of it on show.
const COLUMN_X: f32 = 640.0;
const COLUMN_WIDTH: f32 = 1120.0;
const VIEW_TOP: f32 = 140.0;
const VIEW_BOTTOM: f32 = 960.0;
/// Where a heading scrolled to stands, and where the first one is laid.
const FIRST: f32 = VIEW_TOP + 12.0;
/// The scrollbar right of the column, and the keys' line.
const BAR_X: f32 = 1800.0;
const KEYS_Y: f32 = 992.0;
/// The pointer areas the page's own controls take: the way back, Expand all,
/// the scroll area and the scrollbar, besides the rail's sections.
const CHROME_AREAS: usize = 4;

/// A section's heading with the room under it.
const HEADING: f32 = 56.0;
/// A person: the name (the first section's larger), the GitHub handle, the
/// role, the counts and the links.
const NAME: [f32; 2] = [84.0, 62.0];
const HANDLE: f32 = 18.0;
const ROLE: f32 = 20.0;
const ROLE_LINE: f32 = 30.0;
const COUNT_LINE: f32 = 42.0;
const LINK: f32 = 18.0;
const LINK_LINE: f32 = 34.0;
/// A person's medals, in a column right of their lines: the whole medal on its
/// ribbon, then its name and what it is for (characters a line of Exo 2 at 15).
const MEDAL_COLUMN: f32 = 360.0;
const MEDAL_ART: f32 = 104.0;
const MEDAL_CHARS: usize = 30;
const MEDAL_LINE: f32 = 22.0;
/// Fold rows, and the rows they unfold.
const FOLD: f32 = 50.0;
const HIGHLIGHT: f32 = 18.0;
const HIGHLIGHT_LINE: f32 = 28.0;
const DAY: f32 = 42.0;
const WORK: f32 = 18.0;
const WORK_LINE: f32 = 28.0;
const COMMIT: f32 = 17.0;
const COMMIT_LINE: f32 = 26.0;
/// Across a row: the days' thread, the text after it, a commit's branch line,
/// hash and subject and who made it, and the room right of a work's title for
/// its pull request's tag and its commits.
const THREAD: f32 = 22.0;
const INDENT: f32 = 48.0;
const BRANCH: f32 = 60.0;
const HASH_X: f32 = 76.0;
const SUBJECT_X: f32 = 160.0;
const BY: f32 = 150.0;
const TAIL: f32 = 270.0;
const TAG: f32 = 16.0;
/// Cards (people without a history) two across the column: the gap, the name
/// and handle, the role, a medal's line, the contributions.
const CARD_GAP: f32 = 48.0;
const CARD_NAME: f32 = 30.0;
const CARD_HANDLE: f32 = 16.0;
const CARD_ROLE: f32 = 17.0;
const CARD_ROLE_LINE: f32 = 26.0;
const CARD_MEDAL_LINE: f32 = 46.0;
const CARD_TEXT: f32 = 16.0;
const CARD_LINE: f32 = 25.0;
/// Space after a person, a row of cards and a section.
const PERSON_GAP: f32 = 52.0;
const ROW_GAP: f32 = 36.0;
const SECTION_GAP: f32 = 24.0;
/// The closing notice: its size, line and characters a line.
const NOTICE_SIZE: f32 = 15.0;
const NOTICE_LINE: f32 = 24.0;
const NOTICE_CHARS: usize = 140;

/// Widths of runs as the page draws them: in the UI's families once loaded,
/// else in Inter, which then draws them; in the player's text style. Sizes and
/// widths are frame pixels, so a layout holds at every window size.
#[derive(Clone, Copy)]
struct Measure<'a> {
    display: &'a UiFont,
    body: &'a UiFont,
    style: TextStyle,
    families: bool,
}

impl<'a> Measure<'a> {
    fn of(target: &TextTarget<'a>) -> Self {
        match target {
            TextTarget::Families(fonts, style) => Self {
                display: fonts.display.1,
                body: fonts.body.1,
                style: *style,
                families: true,
            },
            TextTarget::Inter(_, font) => Self {
                display: *font,
                body: *font,
                style: font.style(),
                families: false,
            },
        }
    }

    /// Width of `value` in `family` at `size`.
    fn width(&self, family: TextFamily, value: &str, size: f32, weight: FontWeight) -> f32 {
        let font = match family {
            TextFamily::Display => self.display,
            TextFamily::Body | TextFamily::Code => self.body,
        };
        let face = match weight {
            FontWeight::Regular => TextFace::Regular,
            FontWeight::Semibold => TextFace::Semibold,
        };
        let placed = self.style.place(Rect::new(0.0, 0.0, 0.0, size), size, 0.0);
        visible_text_width_style(
            font,
            value,
            placed.size / font.height.max(1.0),
            face,
            placed.letter_spacing,
        )
    }

    /// `value` in the body family at `size`, broken at spaces into lines of
    /// `width` (with a little to spare).
    fn wrap(&self, value: &str, width: f32, size: f32) -> Vec<String> {
        wrap(value, width * 0.97, |line| {
            self.width(TextFamily::Body, line, size, FontWeight::Regular)
        })
    }
}

/// Height of a person's name line.
fn name_line(featured: bool) -> f32 {
    NAME[usize::from(!featured)] * 1.1
}

/// Width a person's lines take: the column, less their medals' column.
fn text_room(card: &data::Card) -> f32 {
    if card.medals.is_empty() {
        COLUMN_WIDTH
    } else {
        COLUMN_WIDTH - MEDAL_COLUMN - 40.0
    }
}

/// Height one medal takes beside a person's lines.
fn medal_height(medal: Medal) -> f32 {
    let lines = crate::menu::sjk::wrap(medal.description(), MEDAL_CHARS).count();
    (40.0 + lines as f32 * MEDAL_LINE).max(MEDAL_ART) + 14.0
}

/// A card's width, two across the column.
fn card_width() -> f32 {
    (COLUMN_WIDTH - CARD_GAP) * 0.5
}

/// A card's height with `role` lines of role, `medals` medals, `lines` lines
/// of `items` contributions and `links` links.
fn card_height(role: usize, medals: usize, lines: usize, items: usize, links: usize) -> f32 {
    let mut height = 56.0 + role as f32 * CARD_ROLE_LINE + medals as f32 * CARD_MEDAL_LINE;
    if lines > 0 {
        height += 10.0 + lines as f32 * CARD_LINE + items as f32 * 4.0;
    }
    if links > 0 {
        height += 8.0 + links as f32 * CARD_LINE;
    }
    height + 12.0
}

/// `value` in decimal, written into `buffer`: measured and drawn without
/// allocating.
fn number_text(value: usize, buffer: &mut [u8; 20]) -> &str {
    let mut at = buffer.len();
    let mut rest = value;
    loop {
        at -= 1;
        buffer[at] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    std::str::from_utf8(&buffer[at..]).unwrap_or("0")
}

/// What every piece of a frame shares, and the pointer areas rows have left.
struct SjkFrame<'a> {
    /// The frame scrolled with the page.
    content: Frame,
    /// The part of the window the column shows.
    view: Rect,
    since: f32,
    now: f64,
    measure: Measure<'a>,
    areas: usize,
}

impl Panel {
    /// Draw the page in the SJK UI.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        let s = frame.s;
        let measure = Measure::of(&target);
        self.layout_sjk(&measure);
        self.page = (VIEW_BOTTOM - VIEW_TOP) * s;
        self.anchor = [FIRST, s];
        self.max_scroll = ((self.content - VIEW_BOTTOM) * s).max(0.0);
        let now = self.glide();
        let since = (now - self.opened_at).max(0.0) as f32;

        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        self.sun(&frame, viewport, now);
        top_bar(&mut self.ui, &frame, "Back", BACK_TOKEN, "Credits", None);
        let sections = self.sjk_rail(&frame);

        let view = frame.rect(
            COLUMN_X - 40.0,
            VIEW_TOP,
            COLUMN_WIDTH + 80.0,
            VIEW_BOTTOM - VIEW_TOP,
        );
        self.ui.scroll_region(PAGE_TOKEN, view);
        let _ = self.ui.draw_list_mut().push(DrawCommand::PushClip(view));
        let scroll = self.scroll / s;
        let content = frame.shifted(0.0, -scroll);
        if let Some(error) = &self.error {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{error}"),
                content.rect(COLUMN_X, FIRST, COLUMN_WIDTH, 30.0),
                19.0 * s,
                color::EMBER,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let mut shared = SjkFrame {
            content,
            view,
            since,
            now,
            measure,
            areas: MAX_WIDGETS.saturating_sub(CHROME_AREAS + sections),
        };
        for index in 0..self.pieces.len() {
            let (start, end) = self.pieces[index].span();
            // Pieces lift in by up to 18 pixels as they arrive.
            if start - scroll > VIEW_BOTTOM || end - scroll + 20.0 < VIEW_TOP {
                continue;
            }
            self.piece_sjk(index, &mut shared);
        }
        if self.notice_y - scroll < VIEW_BOTTOM {
            for (line, part) in crate::menu::sjk::wrap(NOTICE, NOTICE_CHARS).enumerate() {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    content.rect(
                        COLUMN_X,
                        self.notice_y + line as f32 * NOTICE_LINE,
                        COLUMN_WIDTH,
                        NOTICE_LINE,
                    ),
                    NOTICE_SIZE * s,
                    color::QUIET,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::PopClip);

        self.sjk_scrollbar(&frame);
        self.sjk_keys(&frame);
        self.ui.finish(BACK_TOKEN);
        target.append(&self.ui, viewport);
    }

    /// Lay the sections out in frame pixels: a heading each, a panel for each
    /// person with a history and their unfolded rows, then the rest as cards
    /// two across the column. Laid out again only when a fold, the fonts or the
    /// text style change, as the layout does not depend on the window.
    fn layout_sjk(&mut self, measure: &Measure<'_>) {
        let key = [
            1 + u32::from(measure.families),
            measure.style.scale.to_bits(),
            measure.style.tracking.to_bits(),
            0,
        ];
        if self.laid_out_for == Some(key) {
            return;
        }
        self.laid_out_for = Some(key);
        self.pieces.clear();
        self.born.clear();
        self.urls.clear();
        self.actions.clear();
        self.heading_ys.clear();
        let mut lay = Layout {
            pieces: &mut self.pieces,
            born: &mut self.born,
            urls: &mut self.urls,
            actions: &mut self.actions,
            folds: &self.folds,
            reveal: self.reveal,
            revealing: None,
        };
        let mut y = FIRST;
        let mut person = 0_u16;
        for (index, section) in self.sections.iter().enumerate() {
            self.heading_ys.push(y);
            lay.push(Piece::Heading { y, section: index });
            y += HEADING;
            for card in section.cards.iter().filter(|card| !card.work.is_empty()) {
                y = lay.person_sjk(card, person, index == 0, y, measure) + PERSON_GAP;
                person += 1;
            }
            let plain: Vec<(usize, &data::Card)> = section
                .cards
                .iter()
                .enumerate()
                .filter(|(_, card)| card.work.is_empty())
                .collect();
            for row in plain.chunks(2) {
                y = lay.cards_sjk(row, index, y, measure) + ROW_GAP;
            }
            y += SECTION_GAP;
        }
        self.notice_y = y;
        let lines = crate::menu::sjk::wrap(NOTICE, NOTICE_CHARS).count();
        self.content = y + lines as f32 * NOTICE_LINE + 24.0;
    }

    /// One piece, faded and lifted in as the page opens or its fold unfolds.
    fn piece_sjk(&mut self, index: usize, shared: &mut SjkFrame<'_>) {
        let s = shared.content.s;
        // The first pieces arrive 0.08 s after each other as the page opens;
        // unfolded rows from the moment they were born.
        let arrival = ((shared.since - index.min(14) as f32 * 0.08) / 0.45).clamp(0.0, 1.0);
        let unfold = ((shared.now - self.born[index]) as f32 / 0.3).clamp(0.0, 1.0);
        let ease = (1.0 - (1.0 - arrival).powi(3)) * (1.0 - (1.0 - unfold).powi(2));
        let c = shared.content.shifted(0.0, (1.0 - ease) * 18.0);
        let view = shared.view;
        let measure = shared.measure;
        let areas = &mut shared.areas;
        self.ui.push_opacity(ease);
        match &self.pieces[index] {
            Piece::Heading { y, section } => {
                kit::heading(
                    &mut self.ui,
                    &c,
                    COLUMN_X,
                    y + 20.0,
                    COLUMN_WIDTH,
                    &self.sections[*section].title,
                );
            }
            Piece::Person {
                rect,
                person,
                role,
                handle,
                links,
                ..
            } => {
                let (section, card) = self.people[usize::from(*person)];
                let featured = section == 0;
                let card = &self.sections[section].cards[card];
                let canvas = &mut self.ui;
                let (x, y) = (rect.x, rect.y);
                let line = name_line(featured);
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{}", card.name),
                    c.rect(x, y, COLUMN_WIDTH, line),
                    NAME[usize::from(!featured)] * s,
                    color::TEXT,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
                if let Some((token, width)) = *handle {
                    let place = [x + COLUMN_WIDTH - width, y + line - 36.0, width, 30.0];
                    link(
                        canvas,
                        &c,
                        (token, &card.github),
                        place,
                        HANDLE,
                        view,
                        areas,
                    );
                }
                let mut ty = y + line + 8.0;
                let room = text_room(card);
                for part in role {
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("{part}"),
                        c.rect(x, ty, room, ROLE_LINE),
                        ROLE * s,
                        color::MUTED,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    ty += ROLE_LINE;
                }
                ty += 8.0;
                counts(canvas, &c, card, x, ty, &measure);
                ty += COUNT_LINE;
                let mut lx = x;
                for (&(token, width), entry) in links.iter().zip(&card.links) {
                    dot(
                        canvas,
                        &c,
                        [lx + 4.0, ty + LINK_LINE * 0.5],
                        5.0,
                        color::HOLO,
                    );
                    let place = [lx + 16.0, ty, width, LINK_LINE];
                    link(canvas, &c, (token, &entry.label), place, LINK, view, areas);
                    lx += 16.0 + width + 40.0;
                }
                let mut my = y + line + 10.0;
                for &(medal, count) in &card.medals {
                    let place = [x + COLUMN_WIDTH - MEDAL_COLUMN, my];
                    medal_beside(canvas, &c, (medal, count), place, view);
                    my += medal_height(medal);
                }
            }
            Piece::Bar { rect, fold, token } => {
                let open = self.folds.contains(fold);
                let (Fold::Highlights(person) | Fold::Work(person) | Fold::Commits(person, _)) =
                    *fold;
                let (section, card) = self.people[usize::from(person)];
                let card = &self.sections[section].cards[card];
                let tally = card.tally();
                let (label, count) = match fold {
                    Fold::Highlights(_) => ("Highlights", card.did.len()),
                    _ if tally.pulls == tally.work => ("All pull requests", tally.work),
                    _ => ("All work", tally.work),
                };
                let canvas = &mut self.ui;
                let hovered = canvas.token_hovered(*token);
                let row = [rect.x, rect.y, rect.width, rect.height];
                if hovered {
                    kit::band(canvas, &c, row);
                }
                fold_mark(
                    canvas,
                    &c,
                    [rect.x + THREAD, rect.y + rect.height * 0.5],
                    14.0,
                    open,
                    match (open, hovered) {
                        (true, _) => color::GOLD_BRIGHT,
                        (false, true) => color::TEXT,
                        (false, false) => color::MUTED,
                    },
                );
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{label}"),
                    c.rect(rect.x + INDENT, rect.y, 600.0, rect.height),
                    24.0 * s,
                    match (open, hovered) {
                        (true, _) => color::GOLD_BRIGHT,
                        (false, true) => Color::new(1.0, 1.0, 1.0, 1.0),
                        (false, false) => color::TEXT,
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{count}"),
                    c.rect(rect.x, rect.y, rect.width - 20.0, rect.height),
                    22.0 * s,
                    if open { color::GOLD } else { color::MUTED },
                    FontWeight::Regular,
                    TextAlign::End,
                );
                region(
                    canvas,
                    *token,
                    c.rect(row[0], row[1], row[2], row[3]),
                    view,
                    areas,
                );
            }
            Piece::Highlight {
                rect,
                text: line,
                first,
            } => {
                let canvas = &mut self.ui;
                if *first {
                    dot(
                        canvas,
                        &c,
                        [rect.x + INDENT - 16.0, rect.y + rect.height * 0.5],
                        6.0,
                        color::GOLD,
                    );
                }
                text(
                    canvas,
                    TextFamily::Body,
                    format_args!("{line}"),
                    c.rect(
                        rect.x + INDENT,
                        rect.y,
                        rect.width - INDENT - 24.0,
                        rect.height,
                    ),
                    HIGHLIGHT * s,
                    color::alpha(color::TEXT, 0.92),
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            Piece::Day { rect, person, work } => {
                let (section, card) = self.people[usize::from(*person)];
                let date = &self.sections[section].cards[card].work[usize::from(*work)].date;
                let canvas = &mut self.ui;
                let middle = rect.y + rect.height * 0.5 + 4.0;
                thread(canvas, &c, rect.x, middle, rect.bottom() - middle);
                dot(
                    canvas,
                    &c,
                    [rect.x + THREAD, middle],
                    9.0,
                    color::alpha(color::HOLO, 0.75),
                );
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{date}"),
                    c.rect(rect.x + INDENT, middle - 14.0, 140.0, 28.0),
                    19.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                let rule = rect.x + INDENT + 112.0;
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: c.rect(rule, middle, rect.right() - rule, 1.0),
                    color: color::alpha(color::HOLO, 0.16),
                });
            }
            Piece::Work {
                rect,
                person,
                work,
                lines,
                token,
                reference,
            } => {
                let (section, card) = self.people[usize::from(*person)];
                let piece = &self.sections[section].cards[card].work[usize::from(*work)];
                let open = self.folds.contains(&Fold::Commits(*person, *work));
                let canvas = &mut self.ui;
                let hovered = canvas.token_hovered(*token);
                thread(canvas, &c, rect.x, rect.y, rect.height);
                let row = [rect.x + 36.0, rect.y, rect.width - 36.0, rect.height];
                if hovered {
                    kit::band(canvas, &c, row);
                } else if open {
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: c.rect(row[0], row[1], row[2], row[3]),
                        radius: 10.0 * s,
                        color: color::alpha(color::HOLO, 0.05),
                    });
                }
                let first = rect.y + 5.0 + WORK_LINE * 0.5;
                if piece.is_single() {
                    dot(canvas, &c, [rect.x + THREAD, first], 8.0, color::GOLD);
                } else {
                    // A plus to unfold its commits, on the thread's own dark.
                    dot(canvas, &c, [rect.x + THREAD, first], 16.0, color::SPACE);
                    fold_mark(
                        canvas,
                        &c,
                        [rect.x + THREAD, first],
                        11.0,
                        open,
                        if open || hovered {
                            color::GOLD_BRIGHT
                        } else {
                            color::GOLD
                        },
                    );
                }
                let mut ty = rect.y + 5.0;
                for line in lines {
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("{line}"),
                        c.rect(rect.x + INDENT, ty, COLUMN_WIDTH - INDENT - TAIL, WORK_LINE),
                        WORK * s,
                        if hovered {
                            Color::new(1.0, 1.0, 1.0, 1.0)
                        } else {
                            color::alpha(color::TEXT, 0.92)
                        },
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    ty += WORK_LINE;
                }
                // Right: the commits (or the one commit's hash).
                let tail = c.rect(rect.right() - 132.0, rect.y + 5.0, 120.0, WORK_LINE);
                if piece.is_single() {
                    text(
                        canvas,
                        TextFamily::Display,
                        format_args!("{}", piece.commits[0].hash),
                        tail,
                        17.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::End,
                    );
                } else {
                    let count = piece.commits.len();
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("{count} commit{}", if count == 1 { "" } else { "s" }),
                        tail,
                        15.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::End,
                    );
                }
                region(
                    canvas,
                    *token,
                    c.rect(row[0], row[1], row[2], row[3]),
                    view,
                    areas,
                );
                // The pull request's tag, which opens it.
                if let Some((url_token, width)) = *reference {
                    let width = width + 24.0;
                    let tag = c.rect(
                        rect.right() - 144.0 - width,
                        rect.y + 6.0,
                        width,
                        WORK_LINE - 2.0,
                    );
                    let tag_hovered = canvas.token_hovered(url_token);
                    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                        rect: tag,
                        radius: tag.height * 0.5,
                        color: color::alpha(color::GOLD, if tag_hovered { 0.24 } else { 0.08 }),
                    });
                    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
                        rect: tag,
                        radius: tag.height * 0.5,
                        width: 1.2 * s,
                        color: color::alpha(color::GOLD, if tag_hovered { 0.9 } else { 0.55 }),
                    });
                    text(
                        canvas,
                        TextFamily::Display,
                        format_args!("{}", piece.reference),
                        tag,
                        TAG * s,
                        if tag_hovered {
                            color::GOLD_BRIGHT
                        } else {
                            color::GOLD
                        },
                        FontWeight::Regular,
                        TextAlign::Center,
                    );
                    region(canvas, url_token, tag, view, areas);
                }
            }
            Piece::Commit {
                rect,
                person,
                work,
                commit,
                lines,
                token,
            } => {
                let (section, card) = self.people[usize::from(*person)];
                let piece = &self.sections[section].cards[card].work[usize::from(*work)];
                let entry = &piece.commits[usize::from(*commit)];
                let canvas = &mut self.ui;
                let hovered = canvas.token_hovered(*token);
                thread(canvas, &c, rect.x, rect.y, rect.height);
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: c.rect(rect.x + BRANCH - 0.5, rect.y, 1.0, rect.height),
                    color: color::alpha(color::HOLO, 0.3),
                });
                let row = [
                    rect.x + BRANCH + 8.0,
                    rect.y,
                    rect.width - BRANCH - 8.0,
                    rect.height,
                ];
                if hovered {
                    kit::band(canvas, &c, row);
                }
                let first = rect.y + 2.0 + COMMIT_LINE * 0.5;
                dot(
                    canvas,
                    &c,
                    [rect.x + BRANCH, first],
                    6.0,
                    if hovered {
                        color::GOLD_BRIGHT
                    } else {
                        color::GOLD
                    },
                );
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{}", entry.hash),
                    c.rect(
                        rect.x + HASH_X,
                        rect.y + 2.0,
                        SUBJECT_X - HASH_X,
                        COMMIT_LINE,
                    ),
                    COMMIT * s,
                    if hovered {
                        color::GOLD_BRIGHT
                    } else {
                        color::GOLD
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                let by = if entry.by.is_empty() { 0.0 } else { BY };
                let mut ty = rect.y + 2.0;
                for line in lines {
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("{line}"),
                        c.rect(
                            rect.x + SUBJECT_X,
                            ty,
                            rect.width - SUBJECT_X - by - 12.0,
                            COMMIT_LINE,
                        ),
                        COMMIT * s,
                        if hovered { color::TEXT } else { color::MUTED },
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    ty += COMMIT_LINE;
                }
                if !entry.by.is_empty() {
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("by {}", entry.by),
                        c.rect(rect.right() - by - 12.0, rect.y + 2.0, by, COMMIT_LINE),
                        14.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::End,
                    );
                }
                region(
                    canvas,
                    *token,
                    c.rect(row[0], row[1], row[2], row[3]),
                    view,
                    areas,
                );
            }
            Piece::Card {
                rect,
                section,
                card,
                role,
                lines,
                handle,
                links,
            } => {
                let person = &self.sections[*section].cards[*card];
                let canvas = &mut self.ui;
                let (x, y, width) = (rect.x, rect.y, rect.width);
                let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                    rect: c.rect(x, y, width, 1.0),
                    color: color::alpha(color::HOLO, 0.25),
                });
                text(
                    canvas,
                    TextFamily::Display,
                    format_args!("{}", person.name),
                    c.rect(x, y + 14.0, width, 40.0),
                    CARD_NAME * s,
                    color::TEXT,
                    FontWeight::Semibold,
                    TextAlign::Start,
                );
                if let Some((token, handle_width)) = *handle {
                    let place = [x + width - handle_width, y + 20.0, handle_width, 28.0];
                    link(
                        canvas,
                        &c,
                        (token, &person.github),
                        place,
                        CARD_HANDLE,
                        view,
                        areas,
                    );
                }
                let mut ty = y + 56.0;
                for part in role {
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("{part}"),
                        c.rect(x, ty, width, CARD_ROLE_LINE),
                        CARD_ROLE * s,
                        color::MUTED,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    ty += CARD_ROLE_LINE;
                }
                for &(medal, count) in &person.medals {
                    picture(canvas, c.rect(x, ty + 5.0, 36.0, 36.0), medal.icon(), view);
                    medal_name(
                        canvas,
                        c.rect(x + 46.0, ty + 5.0, width - 46.0, 36.0),
                        medal,
                        count,
                        20.0 * s,
                    );
                    ty += CARD_MEDAL_LINE;
                }
                if !lines.is_empty() {
                    ty += 10.0;
                }
                for (line, first) in lines {
                    if *first {
                        ty += 4.0;
                        dot(
                            canvas,
                            &c,
                            [x + 5.0, ty + CARD_LINE * 0.5],
                            6.0,
                            color::GOLD,
                        );
                    }
                    text(
                        canvas,
                        TextFamily::Body,
                        format_args!("{line}"),
                        c.rect(x + 18.0, ty, width - 18.0, CARD_LINE),
                        CARD_TEXT * s,
                        color::alpha(color::TEXT, 0.9),
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    ty += CARD_LINE;
                }
                if !links.is_empty() {
                    ty += 8.0;
                }
                for (&(token, link_width), entry) in links.iter().zip(&person.links) {
                    dot(
                        canvas,
                        &c,
                        [x + 5.0, ty + CARD_LINE * 0.5],
                        5.0,
                        color::HOLO,
                    );
                    let place = [x + 18.0, ty, link_width, CARD_LINE];
                    link(
                        canvas,
                        &c,
                        (token, &entry.label),
                        place,
                        CARD_TEXT,
                        view,
                        areas,
                    );
                    ty += CARD_LINE;
                }
            }
        }
        self.ui.pop_opacity();
    }

    /// The sun: golden god rays turning slowly out of the emblem across the
    /// page (two sets against each other, so the shafts shimmer where they
    /// cross), sparks rising through their light on the left, and the emblem
    /// breathing in front of two sunbursts turning opposite ways. The reading
    /// column has its own shade, so the rays cross it softly.
    fn sun(&mut self, frame: &Frame, viewport: [f32; 2], now: f64) {
        let s = frame.s;
        let canvas = &mut self.ui;
        let centre = frame.point(SUN[0], SUN[1]);
        let seconds = now as f32;
        let turn = (now % 3_600.0) as f32;
        let breathe = 0.5 + 0.5 * (seconds * 0.37).sin();
        let swell = 0.5 + 0.5 * (seconds * 1.4).sin();
        let reach = viewport[0].max(viewport[1]) * 1.05;
        emblem::rays(
            canvas,
            EmblemLayer::Godrays,
            centre,
            reach,
            turn * 0.021,
            color::alpha(color::GOLD, 0.34 + 0.08 * breathe),
        );
        emblem::rays(
            canvas,
            EmblemLayer::Godrays,
            centre,
            reach * 0.9,
            -turn * 0.013 + 2.1,
            color::alpha(color::GOLD, 0.24 - 0.06 * breathe),
        );
        let shade = color::alpha(color::SPACE, 0.5);
        let [start, _] = frame.point(COLUMN_X - 140.0, 0.0);
        let [full, _] = frame.point(COLUMN_X - 20.0, 0.0);
        fade_across(
            canvas,
            Rect::new(start, 0.0, full - start, viewport[1]),
            color::alpha(color::SPACE, 0.0),
            shade,
        );
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: Rect::new(full, 0.0, viewport[0] - full, viewport[1]),
            color: shade,
        });
        sparks(
            canvas,
            Rect::new(0.0, 0.0, start, viewport[1]),
            seconds,
            [color::GOLD_BRIGHT, color::GOLD],
            (28, 0.5),
            s,
        );
        let side = EMBLEM * s;
        emblem::rays(
            canvas,
            EmblemLayer::Sunburst,
            centre,
            side * 1.9,
            turn * 0.05,
            color::alpha(color::GOLD_BRIGHT, 0.30 + 0.08 * swell),
        );
        emblem::rays(
            canvas,
            EmblemLayer::Sunburst,
            centre,
            side * 1.35,
            -turn * 0.08 + 0.2,
            color::alpha(color::GOLD_BRIGHT, 0.16 + 0.06 * swell),
        );
        emblem::draw(
            canvas,
            Rect::new(centre[0] - side * 0.5, centre[1] - side * 0.5, side, side),
            now,
        );
        text(
            canvas,
            TextFamily::Body,
            format_args!("The people who make SJK"),
            frame.rect(SUN[0] - 240.0, CAPTION_Y, 480.0, 30.0),
            19.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Center,
        );
    }

    /// The sections down a lit rail, the one in view lit (a click scrolls to
    /// one), and Expand all under them; returns how many sections it shows.
    fn sjk_rail(&mut self, frame: &Frame) -> usize {
        let s = frame.s;
        let count = self.sections.len().min(usize::from(SECTION_TOKENS));
        if count > 0 {
            let current = self.current_section();
            let bottom = RAIL_TOP + count as f32 * SECTION_STEP - (SECTION_STEP - SECTION_HEIGHT);
            kit::rail(
                &mut self.ui,
                frame,
                RAIL_X,
                RAIL_TOP - 14.0,
                bottom + 14.0,
                Some(RAIL_TOP + current as f32 * SECTION_STEP + SECTION_HEIGHT * 0.5),
            );
            for (index, section) in self.sections.iter().take(count).enumerate() {
                let top = RAIL_TOP + index as f32 * SECTION_STEP;
                let token = SECTION_BASE + index as u16;
                let hovered = self.ui.token_hovered(token);
                text(
                    &mut self.ui,
                    TextFamily::Display,
                    format_args!("{}", section.title),
                    frame.rect(RAIL_X + 28.0, top, RAIL_WIDTH - 28.0, SECTION_HEIGHT),
                    24.0 * s,
                    match (index == current, hovered) {
                        (true, _) => color::GOLD_BRIGHT,
                        (false, true) => color::TEXT,
                        (false, false) => color::MUTED,
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                self.ui
                    .hit_region(token, frame.rect(RAIL_X, top, RAIL_WIDTH, SECTION_HEIGHT));
            }
        }
        let y = (RAIL_TOP + count as f32 * SECTION_STEP + 28.0).min(VIEW_BOTTOM - EXPAND[1]);
        kit::button(
            &mut self.ui,
            frame,
            [RAIL_X, y, EXPAND[0], EXPAND[1]],
            if self.folds.is_empty() {
                "Expand all"
            } else {
                "Collapse all"
            },
            false,
            true,
            false,
            ALL_TOKEN,
        );
        count
    }

    /// A thin holo scrollbar right of the column, dragged or clicked to jump.
    fn sjk_scrollbar(&mut self, frame: &Frame) {
        if self.max_scroll <= 0.0 {
            return;
        }
        let s = frame.s;
        let track = frame.rect(BAR_X, VIEW_TOP, 4.0, VIEW_BOTTOM - VIEW_TOP);
        let active = self.ui.token_hovered(SCROLLBAR_TOKEN);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: track,
            radius: track.width * 0.5,
            color: color::alpha(color::HOLO, 0.12),
        });
        let shown = self.page / (self.max_scroll + self.page);
        let height = (track.height * shown).max(40.0 * s);
        let y = track.y + (track.height - height) * (self.scroll / self.max_scroll);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: Rect::new(track.x, y, track.width, height),
            radius: track.width * 0.5,
            color: if active {
                color::GOLD_BRIGHT
            } else {
                color::alpha(color::HOLO, 0.6)
            },
        });
        // A wider grip than the bar drawn, so it is easy to catch.
        self.ui.scroll_region(
            SCROLLBAR_TOKEN,
            Rect::new(
                track.x - 10.0 * s,
                track.y,
                track.width + 20.0 * s,
                track.height,
            ),
        );
    }

    /// The page's keys, right-aligned at the bottom.
    fn sjk_keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let expand = if self.folds.is_empty() {
            "expand all"
        } else {
            "collapse all"
        };
        let keys: [(&[&str], &str); 4] = [
            (&["Up", "Down"], "scroll"),
            (&["Tab"], "section"),
            (&["E"], expand),
            (&["Esc"], "back"),
        ];
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (keys.len() - 1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

impl Layout<'_> {
    /// Lay out the panel of `card`, person number `person`, from `y` (frame
    /// pixels): their name, handle, role, counts and links with their medals
    /// beside them, then the folds; returns its bottom.
    fn person_sjk(
        &mut self,
        card: &data::Card,
        person: u16,
        featured: bool,
        y: f32,
        measure: &Measure<'_>,
    ) -> f32 {
        let panel = self.pieces.len();
        let handle = card.github_url().map(|url| {
            let width = measure.width(TextFamily::Body, &card.github, HANDLE, FontWeight::Regular);
            (self.url(url), width)
        });
        let links: Vec<_> = card
            .links
            .iter()
            .map(|entry| {
                let width =
                    measure.width(TextFamily::Body, &entry.label, LINK, FontWeight::Regular);
                (self.url(entry.url.clone()), width)
            })
            .collect();
        let role = measure.wrap(&card.role, text_room(card), ROLE);
        let lines_top = y + name_line(featured) + 8.0;
        let mut bottom = lines_top + role.len() as f32 * ROLE_LINE + 8.0 + COUNT_LINE;
        if !links.is_empty() {
            bottom += LINK_LINE;
        }
        let medals: f32 = card
            .medals
            .iter()
            .map(|&(medal, _)| medal_height(medal))
            .sum();
        let mut cy = bottom.max(lines_top + 2.0 + medals) + 18.0;
        self.push(Piece::Person {
            rect: Rect::new(COLUMN_X, y, COLUMN_WIDTH, 0.0),
            person,
            head: cy - y,
            role,
            handle,
            links,
        });

        if !card.did.is_empty() {
            let fold = Fold::Highlights(person);
            cy = self.fold_sjk(fold, cy);
            if self.enter(fold) {
                cy += 4.0;
                for did in &card.did {
                    let lines = measure.wrap(did, COLUMN_WIDTH - INDENT - 24.0, HIGHLIGHT);
                    for (index, line) in lines.into_iter().enumerate() {
                        if index == 0 {
                            cy += 6.0;
                        }
                        self.push(Piece::Highlight {
                            rect: Rect::new(COLUMN_X, cy, COLUMN_WIDTH, HIGHLIGHT_LINE),
                            text: line,
                            first: index == 0,
                        });
                        cy += HIGHLIGHT_LINE;
                    }
                }
                cy += 16.0;
            }
            self.leave(fold);
        }

        let fold = Fold::Work(person);
        cy = self.fold_sjk(fold, cy);
        if self.enter(fold) {
            let title_width = COLUMN_WIDTH - INDENT - TAIL;
            let subject_width = COLUMN_WIDTH - SUBJECT_X - 12.0;
            let mut day = "";
            for (index, piece) in card.work.iter().enumerate() {
                let work = index as u16;
                if piece.date != day {
                    day = &piece.date;
                    self.push(Piece::Day {
                        rect: Rect::new(COLUMN_X, cy, COLUMN_WIDTH, DAY),
                        person,
                        work,
                    });
                    cy += DAY;
                }
                let single = piece.is_single();
                let action = if single {
                    Action::Open(self.url(piece.commits[0].url()) - URL_BASE)
                } else {
                    Action::Toggle(Fold::Commits(person, work))
                };
                let token = self.action(action);
                let reference = piece.url().map(|url| {
                    let width = measure.width(
                        TextFamily::Display,
                        &piece.reference,
                        TAG,
                        FontWeight::Regular,
                    );
                    (self.url(url), width)
                });
                let lines = measure.wrap(&piece.title, title_width, WORK);
                let height = lines.len().max(1) as f32 * WORK_LINE + 10.0;
                self.push(Piece::Work {
                    rect: Rect::new(COLUMN_X, cy, COLUMN_WIDTH, height),
                    person,
                    work,
                    lines,
                    token,
                    reference,
                });
                cy += height;
                if single {
                    continue;
                }
                let fold = Fold::Commits(person, work);
                if self.enter(fold) {
                    for (index, commit) in piece.commits.iter().enumerate() {
                        let by = if commit.by.is_empty() { 0.0 } else { BY };
                        let lines = measure.wrap(&commit.subject, subject_width - by, COMMIT);
                        let height = lines.len().max(1) as f32 * COMMIT_LINE + 4.0;
                        let token = self.url(commit.url());
                        self.push(Piece::Commit {
                            rect: Rect::new(COLUMN_X, cy, COLUMN_WIDTH, height),
                            person,
                            work,
                            commit: index as u16,
                            lines,
                            token,
                        });
                        cy += height;
                    }
                    cy += 8.0;
                }
                self.leave(fold);
            }
            cy += 12.0;
        }
        self.leave(fold);
        if let Piece::Person { rect, .. } = &mut self.pieces[panel] {
            rect.height = cy - y;
        }
        cy
    }

    /// A fold's row at `y`; returns the row below it.
    fn fold_sjk(&mut self, fold: Fold, y: f32) -> f32 {
        let token = self.action(Action::Toggle(fold));
        self.push(Piece::Bar {
            rect: Rect::new(COLUMN_X, y, COLUMN_WIDTH, FOLD),
            fold,
            token,
        });
        y + FOLD + 6.0
    }

    /// Lay out a row of cards (people without a history), two across the
    /// column from its left, from `y`; returns the row's bottom.
    fn cards_sjk(
        &mut self,
        row: &[(usize, &data::Card)],
        section: usize,
        y: f32,
        measure: &Measure<'_>,
    ) -> f32 {
        let width = card_width();
        let mut laid = Vec::with_capacity(row.len());
        let mut tallest: f32 = 0.0;
        for &(index, card) in row {
            let handle = card.github_url().map(|url| {
                let handle_width = measure.width(
                    TextFamily::Body,
                    &card.github,
                    CARD_HANDLE,
                    FontWeight::Regular,
                );
                (self.url(url), handle_width)
            });
            let links: Vec<_> = card
                .links
                .iter()
                .map(|entry| {
                    let link_width = measure.width(
                        TextFamily::Body,
                        &entry.label,
                        CARD_TEXT,
                        FontWeight::Regular,
                    );
                    (self.url(entry.url.clone()), link_width)
                })
                .collect();
            let role = measure.wrap(&card.role, width, CARD_ROLE);
            let mut lines = Vec::new();
            for did in &card.did {
                for (line, part) in measure
                    .wrap(did, width - 18.0, CARD_TEXT)
                    .into_iter()
                    .enumerate()
                {
                    lines.push((part, line == 0));
                }
            }
            tallest = tallest.max(card_height(
                role.len(),
                card.medals.len(),
                lines.len(),
                card.did.len(),
                links.len(),
            ));
            laid.push((index, role, lines, handle, links));
        }
        for (slot, (card, role, lines, handle, links)) in laid.into_iter().enumerate() {
            self.push(Piece::Card {
                rect: Rect::new(
                    COLUMN_X + slot as f32 * (width + CARD_GAP),
                    y,
                    width,
                    tallest,
                ),
                section,
                card,
                role,
                lines,
                handle,
                links,
            });
        }
        y + tallest
    }
}

/// The days' thread down the left of a person's work, from `y` for `height`
/// (frame pixels, in `frame`).
fn thread(canvas: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, height: f32) {
    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
        rect: frame.rect(x + THREAD - 0.5, y, 1.0, height),
        color: color::alpha(color::HOLO, 0.25),
    });
}

/// A round dot `size` across centred on `centre` (frame pixels).
fn dot(canvas: &mut MenuCanvas, frame: &Frame, centre: [f32; 2], size: f32, colour: Color) {
    let rect = frame.rect(centre[0] - size * 0.5, centre[1] - size * 0.5, size, size);
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect,
        radius: rect.width * 0.5,
        color: colour,
    });
}

/// Let `token` take clicks over `area` (window pixels) while the column shows
/// it whole and the frame has pointer areas left.
fn region(canvas: &mut MenuCanvas, token: u16, area: Rect, view: Rect, areas: &mut usize) {
    if area.y >= view.y - 0.5 && area.bottom() <= view.bottom() + 0.5 && *areas > 0 {
        *areas -= 1;
        canvas.hit_region(token, area);
    }
}

/// A medal's name in gold, with how often it was given past once, over `rect`.
fn medal_name(canvas: &mut MenuCanvas, rect: Rect, medal: Medal, count: u32, size: f32) {
    let mut write = |args: std::fmt::Arguments<'_>| {
        text(
            canvas,
            TextFamily::Display,
            args,
            rect,
            size,
            color::GOLD_BRIGHT,
            FontWeight::Regular,
            TextAlign::Start,
        );
    };
    if count > 1 {
        write(format_args!("{} x{count}", medal.name()));
    } else {
        write(format_args!("{}", medal.name()));
    }
}

/// A fold's mark centred on `centre` (frame pixels): a plus while closed, a
/// minus while `open`, its bars `size` long. Drawn as rectangles, which the
/// column's clip cuts cleanly (it leaves arcs whole).
fn fold_mark(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    centre: [f32; 2],
    size: f32,
    open: bool,
    colour: Color,
) {
    let [x, y] = centre;
    let thick = 2.0;
    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
        rect: frame.rect(x - size * 0.5, y - thick * 0.5, size, thick),
        color: colour,
    });
    if !open {
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x - thick * 0.5, y - size * 0.5, thick, size),
            color: colour,
        });
    }
}

/// A picture over `rect` (window pixels) while the column shows it whole: the
/// renderer clips a picture by squeezing it, so one at the column's edge waits.
fn picture(canvas: &mut MenuCanvas, rect: Rect, texture: sjk_ui::TextureId, view: Rect) {
    if rect.y >= view.y && rect.bottom() <= view.bottom() {
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect,
            texture,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
    }
}

/// One of a person's medals (and how often it was given) in their medals'
/// column at `place` (frame pixels): the whole medal on its ribbon, its name
/// and what it is for.
fn medal_beside(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    (medal, count): (Medal, u32),
    place: [f32; 2],
    view: Rect,
) {
    let s = frame.s;
    let [x, y] = place;
    picture(
        canvas,
        frame.rect(x, y, MEDAL_ART, MEDAL_ART),
        medal.art(),
        view,
    );
    let text_x = x + MEDAL_ART + 14.0;
    let width = MEDAL_COLUMN - MEDAL_ART - 14.0;
    medal_name(
        canvas,
        frame.rect(text_x, y + 6.0, width, 32.0),
        medal,
        count,
        24.0 * s,
    );
    let mut line_y = y + 40.0;
    for part in crate::menu::sjk::wrap(medal.description(), MEDAL_CHARS) {
        text(
            canvas,
            TextFamily::Body,
            format_args!("{part}"),
            frame.rect(text_x, line_y, width, MEDAL_LINE),
            15.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        line_y += MEDAL_LINE;
    }
}

/// A person's counts on the line at `y` from `x`: each number large in gold,
/// then what it counts. Changes count only when some are not pull requests.
fn counts(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    card: &data::Card,
    x: f32,
    y: f32,
    measure: &Measure<'_>,
) {
    let s = frame.s;
    let tally = card.tally();
    let changes = if tally.pulls == tally.work {
        0
    } else {
        tally.work
    };
    let mut at = x;
    for (count, one, many) in [
        (changes, "change", "changes"),
        (tally.pulls, "pull request", "pull requests"),
        (tally.commits, "commit", "commits"),
    ] {
        if count == 0 {
            continue;
        }
        let mut digits = [0_u8; 20];
        let number = number_text(count, &mut digits);
        let number_width = measure.width(TextFamily::Display, number, 30.0, FontWeight::Semibold);
        text(
            canvas,
            TextFamily::Display,
            format_args!("{number}"),
            frame.rect(at, y, number_width + 12.0, COUNT_LINE),
            30.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let label = if count == 1 { one } else { many };
        let label_width = measure.width(TextFamily::Body, label, 17.0, FontWeight::Regular);
        text(
            canvas,
            TextFamily::Body,
            format_args!("{label}"),
            frame.rect(
                at + number_width + 8.0,
                y + 2.0,
                label_width + 12.0,
                COUNT_LINE,
            ),
            17.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        at += number_width + 8.0 + label_width + 32.0;
    }
}

/// Text that opens an address, in gold: bright and underlined under the
/// pointer. `link` is its token and label; `place` its frame rectangle, as
/// wide as the label measures; it takes clicks while the column shows it whole.
fn link(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    (token, label): (u16, &str),
    place: [f32; 4],
    size: f32,
    view: Rect,
    areas: &mut usize,
) {
    let [x, y, width, height] = place;
    let hovered = canvas.token_hovered(token);
    text(
        canvas,
        TextFamily::Body,
        format_args!("{label}"),
        frame.rect(x, y, width + 16.0, height),
        size * frame.s,
        if hovered {
            color::GOLD_BRIGHT
        } else {
            color::GOLD
        },
        FontWeight::Regular,
        TextAlign::Start,
    );
    if hovered {
        let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(x, y + height * 0.5 + size * 0.6, width, 1.2),
            color: color::GOLD_BRIGHT,
        });
    }
    region(
        canvas,
        token,
        frame.rect(x - 4.0, y, width + 8.0, height),
        view,
        areas,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_font::SjkFonts;

    /// The UI's families as the client loads them.
    fn families() -> (UiFont, UiFont) {
        let load = |family| crate::text::load_family(family, 1.0, None).expect("a bundled family");
        (
            load(&crate::text::DISPLAY).font,
            load(&crate::text::BODY).font,
        )
    }

    /// Draw `panel` in the SJK UI at `viewport` with `fonts`.
    fn draw(panel: &mut Panel, fonts: &(UiFont, UiFont), viewport: [f32; 2]) {
        let (mut display, mut body) = (Vec::new(), Vec::new());
        let target = TextTarget::Families(
            SjkFonts {
                display: (&mut display, &fonts.0),
                body: (&mut body, &fonts.1),
                code: None,
            },
            TextStyle::NEUTRAL,
        );
        panel.append_sjk(target, viewport);
    }

    fn sjk_panel() -> Panel {
        let mut panel = Panel::new();
        assert!(panel.error.is_none(), "{:?}", panel.error);
        panel.set_sjk(true);
        panel.open(false);
        panel.settle();
        panel
    }

    #[test]
    fn every_scroll_of_the_page_fits_its_canvas_folded_and_unfolded() {
        let fonts = families();
        for viewport in [
            [1920.0, 1080.0],
            [3840.0, 2160.0],
            [1024.0, 768.0],
            [2560.0, 1080.0],
        ] {
            let mut panel = sjk_panel();
            for expanded in [false, true] {
                if expanded {
                    panel.toggle_all();
                    panel.settle();
                }
                draw(&mut panel, &fonts, viewport);
                let step = panel.page * 0.5;
                let mut at = 0.0;
                loop {
                    panel.scroll_to(at);
                    draw(&mut panel, &fonts, viewport);
                    let (overflowed, areas) = panel.canvas_use();
                    assert!(
                        !overflowed && areas <= MAX_WIDGETS,
                        "{viewport:?} expanded {expanded} at {at}: {areas} areas"
                    );
                    if at >= panel.max_scroll {
                        break;
                    }
                    at = (at + step).min(panel.max_scroll);
                }
            }
            // Expanded, the page runs far past its folded length.
            assert!(panel.content > 10_000.0, "{}", panel.content);
        }
    }

    #[test]
    fn the_layout_follows_the_frame_and_keeps_inside_the_column() {
        let fonts = families();
        let mut panel = sjk_panel();
        panel.toggle_all();
        panel.settle();
        draw(&mut panel, &fonts, [1920.0, 1080.0]);
        let laid = panel.pieces.len();
        assert!(laid > 500, "{laid} pieces");
        for piece in &panel.pieces {
            let rect = match piece {
                Piece::Heading { .. } => continue,
                Piece::Person { rect, .. }
                | Piece::Bar { rect, .. }
                | Piece::Highlight { rect, .. }
                | Piece::Day { rect, .. }
                | Piece::Work { rect, .. }
                | Piece::Commit { rect, .. }
                | Piece::Card { rect, .. } => *rect,
            };
            assert!(
                rect.x >= COLUMN_X - 0.01 && rect.right() <= COLUMN_X + COLUMN_WIDTH + 0.01,
                "{rect:?}"
            );
        }
        // Frame pixels: another window size keeps the layout as it is.
        let content = panel.content;
        draw(&mut panel, &fonts, [3840.0, 2160.0]);
        assert_eq!(panel.pieces.len(), laid);
        assert_eq!(panel.content, content);
        assert!((panel.max_scroll - (content - VIEW_BOTTOM) * 2.0).abs() < 0.5);
    }

    #[test]
    fn medals_show_on_their_cards_with_their_names() {
        let fonts = families();
        let mut panel = sjk_panel();
        draw(&mut panel, &fonts, [1920.0, 1080.0]);
        // Creyon is person 1: scrolled to his panel, his medal's whole picture
        // and its name are drawn.
        panel.scroll_to_person(1);
        draw(&mut panel, &fonts, [1920.0, 1080.0]);
        let art = Medal::EarlyContributor.art();
        let pictures = panel
            .ui
            .draw_list()
            .commands()
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::TexturedQuad { texture, .. } if *texture == art)
            })
            .count();
        assert!(pictures >= 1, "no Early Contributor picture");
        let names = panel
            .ui
            .text_runs()
            .filter(|run| *run == "Early Contributor")
            .count();
        assert!(names >= 1, "no medal name");
        // A card's lines leave the medals' column free.
        let creyon = &panel.sections[1].cards[0];
        assert_eq!(creyon.name, "Creyon");
        assert_eq!(text_room(creyon), COLUMN_WIDTH - MEDAL_COLUMN - 40.0);
    }

    #[test]
    fn tab_and_the_rail_step_through_the_sections() {
        let fonts = families();
        let mut panel = sjk_panel();
        draw(&mut panel, &fonts, [1920.0, 1080.0]);
        let sections = panel.sections.len();
        assert!(sections >= 4, "{sections} sections");
        assert_eq!(panel.current_section(), 0);
        let mut seen = vec![panel.target];
        for _ in 1..sections {
            assert!(!panel.key(KeyCode::Tab, false));
            panel.scroll = panel.target;
            draw(&mut panel, &fonts, [1920.0, 1080.0]);
            seen.push(panel.target);
        }
        // Each Tab goes further, up to the end of the page.
        assert!(seen.windows(2).all(|pair| pair[1] > pair[0]), "{seen:?}");
        assert_eq!(panel.current_section(), sections - 1);
        // Shift+Tab goes back; Tab past the last section returns to the top.
        assert!(!panel.key(KeyCode::Tab, true));
        assert!(panel.target < seen[sections - 1]);
        panel.target = panel.max_scroll;
        assert!(!panel.key(KeyCode::Tab, false));
        assert_eq!(panel.target, 0.0);
        // E expands every fold and closes them again.
        assert!(!panel.key(KeyCode::KeyE, false));
        assert!(!panel.folds.is_empty());
        assert!(!panel.key(KeyCode::KeyE, false));
        assert!(panel.folds.is_empty());
        assert!(panel.key(KeyCode::Escape, false));
        // The second section's heading is where Tab scrolled to.
        assert!((panel.section_scroll(1) - seen[1]).abs() < 0.5);
    }

    #[test]
    fn numbers_are_written_without_allocating() {
        let mut buffer = [0_u8; 20];
        assert_eq!(number_text(0, &mut buffer), "0");
        assert_eq!(number_text(384, &mut buffer), "384");
        assert_eq!(number_text(usize::MAX, &mut buffer), usize::MAX.to_string());
    }
}

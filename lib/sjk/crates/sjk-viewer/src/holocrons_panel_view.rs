//! The Holocrons page's drawing, in the SJK UI's look. The world stays clear over the
//! left, where the 3D holocron floats ([`crate::holocrons::stage`], at [`STAGE_AT`]), with
//! the chosen tier's name, a padlock when none is held and how many are, under it. On the
//! right, over a dark panel: how many holocrons the player holds (or why that is not
//! known), the four tiers (picture, name in the tier's colour, the count held or a dash,
//! the odds of a drop), what the chosen tier is and holds, how long to the next holocron
//! and how many dropped today, and the newest ten. The player's credits stand at the
//! panel's top right, Loot table beside what the chosen tier holds, and Open and Open all
//! under the holocron's caption. While the opening's ceremony or the loot table shows,
//! only it is drawn ([`super::opening`], [`super::loot`]).
//!
//! Every position is on the 16:9 frame ([`Frame`]); under the Profile screen's row of
//! tabs the page moves down [`crate::profile_hub::SHIFT`] pixels. The pictures and
//! shapes are a fixed number of commands per state: nothing is allocated for them.

use super::*;
use crate::holocrons::{gem, icons};
use crate::menu::sjk::{
    Frame, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar, wrap,
};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign};

/// The right-hand panel: its left edge and width, and where its dark begins to fade in.
const RIGHT_X: f32 = 1_000.0;
const RIGHT_WIDTH: f32 = 824.0;
const PANEL_FADE: f32 = 940.0;
/// The panel's top: the headline, its line, and the first tier's row.
const HEAD_Y: f32 = 166.0;
const LINE_Y: f32 = 216.0;
const ROWS_TOP: f32 = 262.0;
const ROW_HEIGHT: f32 = 60.0;
const ROW_GAP: f32 = 8.0;
/// The chosen tier's description, what it holds, and the rule under.
const ABOUT_Y: f32 = 542.0;
const RULE_Y: f32 = 632.0;
/// The progress line, its bar, and the recent list's heading and first row.
const NEXT_Y: f32 = 642.0;
const BAR_Y: f32 = 682.0;
const RECENT_Y: f32 = 712.0;
const RECENT_TOP: f32 = 736.0;
const RECENT_HEIGHT: f32 = 38.0;
const RECENT_COLUMNS: usize = 2;
const RECENT_WIDTH: f32 = RIGHT_WIDTH / RECENT_COLUMNS as f32;
/// The caption under the holocron: the padlock's centre, the name and the count's line,
/// centred on [`STAGE_AT`]'s x.
const LOCK_Y: f32 = 706.0;
const NAME_Y: f32 = 726.0;
const HELD_Y: f32 = 788.0;
/// Open and Open all under the caption, and their widths.
const BUTTONS_Y: f32 = 838.0;
const OPEN_WIDTH: f32 = 190.0;
const OPEN_ALL_WIDTH: f32 = 230.0;
/// In place of Open and Open all while the hub keeps opening to staff.
pub(crate) const CLOSED_LINE: &str = "Opening comes soon: keep your holocrons, they will wait.";
const BUTTON_GAP: f32 = 20.0;
const BUTTON_HEIGHT: f32 = 48.0;
/// The keys' line.
pub(super) const KEYS_Y: f32 = 1_010.0;
/// Characters a line of the description holds (Exo 2 at 16 over the panel's width).
const ABOUT_CHARS: usize = 100;
/// Loot table, at the right of the line saying what the chosen tier holds.
const LOOT_WIDTH: f32 = 160.0;
const LOOT_HEIGHT: f32 = 34.0;

/// `text` cut to `chars` characters with an ellipsis.
fn cut(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_owned();
    }
    let kept: String = text.chars().take(chars.saturating_sub(3)).collect();
    format!("{}...", kept.trim_end())
}

/// Darkness over the world: the right-hand panel fades in from [`PANEL_FADE`] and stays;
/// the top and the foot of the window fade in so the title, the tabs, the caption and the
/// keys read over any map. The middle of the left stays clear for the holocron.
fn backdrop(canvas: &mut MenuCanvas, viewport: [f32; 2]) {
    let frame = Frame::new(viewport);
    let space = |alpha| color::alpha(color::SPACE, alpha);
    let [panel, _] = frame.point(PANEL_FADE, 0.0);
    let [solid, _] = frame.point(RIGHT_X, 0.0);
    let height = viewport[1];
    let mut push = |rect: Rect, start: Color, end: Color, vertical: bool| {
        let _ = canvas.draw_list_mut().push(DrawCommand::GradientRect {
            rect,
            radius: 0.0,
            gradient: Gradient {
                start,
                end,
                vertical,
            },
        });
    };
    // Solid under the title and the tabs, then fading out above the holocron.
    let [_, held] = frame.point(0.0, 215.0);
    let [_, fade] = frame.point(0.0, 340.0);
    push(
        Rect::new(0.0, 0.0, viewport[0], held),
        space(0.84),
        space(0.84),
        true,
    );
    push(
        Rect::new(0.0, held, viewport[0], fade - held),
        space(0.84),
        space(0.0),
        true,
    );
    let [_, foot] = frame.point(0.0, 640.0);
    let [_, end] = frame.point(0.0, 850.0);
    push(
        Rect::new(0.0, foot, viewport[0], end - foot),
        space(0.0),
        space(0.78),
        true,
    );
    if end < height {
        push(
            Rect::new(0.0, end, viewport[0], height - end),
            space(0.78),
            space(0.78),
            true,
        );
    }
    push(
        Rect::new(panel, 0.0, solid - panel, height),
        space(0.0),
        space(0.88),
        false,
    );
    push(
        Rect::new(solid, 0.0, viewport[0] - solid, height),
        space(0.88),
        space(0.88),
        false,
    );
}

/// A padlock of width `size` frame pixels centred on (`x`, `y`): a shackle behind a body
/// with a keyhole, in `colour`.
fn padlock(canvas: &mut MenuCanvas, frame: &Frame, x: f32, y: f32, size: f32, colour: Color) {
    let s = frame.s;
    let list = canvas.draw_list_mut();
    let _ = list.push(DrawCommand::Border {
        rect: frame.rect(x - size * 0.3, y - size * 0.62, size * 0.6, size * 0.8),
        radius: size * 0.3 * s,
        width: (size * 0.11 * s).max(1.5),
        color: colour,
    });
    let _ = list.push(DrawCommand::RoundedRect {
        rect: frame.rect(x - size * 0.5, y - size * 0.12, size, size * 0.66),
        radius: size * 0.12 * s,
        color: colour,
    });
    let _ = list.push(DrawCommand::RoundedRect {
        rect: frame.rect(x - size * 0.06, y + size * 0.08, size * 0.12, size * 0.26),
        radius: size * 0.05 * s,
        color: color::alpha(color::SPACE, 0.85),
    });
}

/// Tier `index`'s picture over the square of side `2 * half` frame pixels centred on
/// (`x`, `y`), at `alpha`: its icon, or its gem where the icon did not load.
pub(super) fn picture(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    index: usize,
    x: f32,
    y: f32,
    half: f32,
    alpha: f32,
) {
    if icons::is_loaded(index) {
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: frame.rect(x - half, y - half, half * 2.0, half * 2.0),
            texture: icons::texture(index),
            color: Color::new(1.0, 1.0, 1.0, alpha),
        });
    } else {
        let [cx, cy] = frame.point(x, y);
        gem::draw(
            canvas.draw_list_mut(),
            [cx, cy],
            half * frame.s,
            holocrons::TIERS[index].colour,
            alpha,
            gem::ROWS,
        );
    }
}

impl Panel {
    /// Draw the page with its text to `target`, reading the hub's data when it is old.
    pub(crate) fn append_sjk(&mut self, target: TextTarget<'_>, viewport: [f32; 2]) {
        self.build(viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the page out for a window of `viewport` pixels.
    pub(super) fn build(&mut self, viewport: [f32; 2]) {
        self.build_at(viewport, Instant::now());
    }

    /// Lay the page out for a window of `viewport` pixels at `now`.
    pub(super) fn build_at(&mut self, viewport: [f32; 2], now: Instant) {
        let frame = Frame::new(viewport);
        self.ui.begin_transparent(viewport);
        backdrop(&mut self.ui, viewport);
        if self.ceremony.is_some() {
            self.draw_ceremony(&frame, viewport, now);
            self.ui.finish(super::opening::NEXT_TOKEN);
            return;
        }
        if self.loot {
            self.draw_loot(&frame, viewport);
            self.ui.finish(LOOT_BACK_TOKEN);
            return;
        }
        if self.hub {
            crate::profile_hub::collection_header(
                &mut self.ui,
                &frame,
                crate::profile_hub::back_words(self.hub_header.back),
                BACK_TOKEN,
                crate::profile_hub::Tab::Holocrons,
                (&self.row, 1.0),
            );
        } else {
            top_bar(&mut self.ui, &frame, "Back", BACK_TOKEN, "Holocrons", None);
        }
        let keys = frame;
        // Under the Collection screen's row of tabs the page moves down.
        let frame = frame.shifted(
            0.0,
            if self.hub {
                crate::profile_hub::SHIFT
            } else {
                0.0
            },
        );
        self.header(&frame);
        self.tiers(&frame);
        self.about(&frame);
        if self.data.state == State::Known {
            self.progress(&frame);
            self.recent(&frame);
        }
        self.caption(&frame);
        self.buttons(&frame);
        self.keys(&keys);
        self.tier_hover(&keys, now);
        self.ui.finish(ROW_BASE + self.selected as u16);
    }

    /// The hover card of the tier row the pointer rests on, over the page: how
    /// holocrons of that tier are found. `frame` is the window's, not the page's moved one.
    fn tier_hover(&mut self, frame: &Frame, now: Instant) {
        self.hover.follow(self.ui.hovered(), now);
        let Some((token, alpha)) = self.hover.showing(now) else {
            return;
        };
        let Some(index) = token
            .checked_sub(ROW_BASE)
            .map(usize::from)
            .filter(|index| *index < COUNT)
        else {
            return;
        };
        let Some(anchor) = self.ui.rect_for(token) else {
            return;
        };
        let card = tier_card(index, &self.data);
        crate::menu::sjk::hover::draw(&mut self.ui, frame, anchor, &card, alpha);
    }

    /// How many holocrons the player holds, or why that is not known.
    fn header(&mut self, frame: &Frame) {
        let s = frame.s;
        let (headline, line): (String, &str) = match (self.data.state, self.data.total()) {
            (State::IdentityOff, _) => (
                "Identity is off".to_owned(),
                "Holocrons need the SJK identity: switch it on in Settings, Network.",
            ),
            (State::NoHub, _) => (
                "No hub is set".to_owned(),
                "Holocrons are kept on the SJK hub (cl_hubUrl).",
            ),
            (State::Waiting, _) | (State::Known, None) => (
                "Contacting the hub...".to_owned(),
                "Your holocrons show once the SJK hub has answered.",
            ),
            (State::Known, Some(0)) => (
                "No holocrons yet".to_owned(),
                "They are found by playing on a server.",
            ),
            (State::Known, Some(1)) => ("1 holocron".to_owned(), "Found by playing on a server."),
            (State::Known, Some(total)) => (
                format!("{total} holocrons"),
                "Found by playing on a server.",
            ),
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{headline}"),
            frame.rect(RIGHT_X, HEAD_Y, RIGHT_WIDTH, 48.0),
            38.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(RIGHT_X, LINE_Y, RIGHT_WIDTH, 26.0),
            17.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        if let Some(credits) = self.data.credits {
            self.credits(frame, credits);
        }
    }

    /// The player's credits at the panel's top right: the mark and the sum, the word
    /// under them.
    fn credits(&mut self, frame: &Frame, credits: u64) {
        let s = frame.s;
        let words = crate::cards::credits_text(credits);
        let size = 30.0;
        let width = crate::text::display_width(&words, size) * crate::text::style::current().scale;
        let right = RIGHT_X + RIGHT_WIDTH;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{words}"),
            frame.rect(right - width - 8.0, HEAD_Y + 4.0, width + 8.0, 38.0),
            size * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::End,
        );
        let mark = 24.0;
        let [x, y] = frame.point(
            right - width - 8.0 - 10.0 - crate::cards::mark::width(mark),
            HEAD_Y + 10.0,
        );
        crate::cards::mark::draw(self.ui.draw_list_mut(), [x, y], mark * s, color::GOLD);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("credits"),
            frame.rect(right - 200.0, LINE_Y, 200.0, 26.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// The four tiers, one row each.
    fn tiers(&mut self, frame: &Frame) {
        let s = frame.s;
        for (index, tier) in holocrons::TIERS.iter().enumerate() {
            let y = ROWS_TOP + index as f32 * (ROW_HEIGHT + ROW_GAP);
            let token = ROW_BASE + index as u16;
            let chosen = index == self.selected;
            let hovered = self.ui.token_hovered(token);
            let count = self.data.count(index);
            let held = count.is_some_and(|count| count > 0);
            let rect = frame.rect(RIGHT_X, y, RIGHT_WIDTH, ROW_HEIGHT);
            let list = self.ui.draw_list_mut();
            let _ = list.push(DrawCommand::RoundedRect {
                rect,
                radius: 14.0 * s,
                color: if chosen {
                    tier.colour_alpha(0.16)
                } else {
                    color::alpha(color::SPACE, 0.55)
                },
            });
            let _ = list.push(DrawCommand::Border {
                rect,
                radius: 14.0 * s,
                width: if chosen { 2.0 * s } else { 1.2 * s },
                color: if chosen {
                    Color::new(1.0, 1.0, 1.0, 0.7)
                } else if hovered {
                    tier.colour_alpha(0.55)
                } else {
                    color::alpha(color::HOLO, 0.16)
                },
            });
            let _ = list.push(DrawCommand::RoundedRect {
                rect: frame.rect(RIGHT_X + 10.0, y + 12.0, 5.0, ROW_HEIGHT - 24.0),
                radius: 2.5 * s,
                color: tier.colour_alpha(if held { 1.0 } else { 0.4 }),
            });
            // The tier's effects over its row, faint for a tier not held yet.
            crate::rarity_fx::draw(
                self.ui.draw_list_mut(),
                crate::rarity_fx::Item {
                    owned: held,
                    ..crate::rarity_fx::Item::holocron(tier, crate::rarity_fx::Size::Full)
                },
                rect,
                14.0 * s,
                s,
                crate::menu::art::motion::seconds() as f32,
            );
            self.ui.hit_region(token, rect);
            picture(
                &mut self.ui,
                frame,
                index,
                RIGHT_X + 52.0,
                y + ROW_HEIGHT * 0.5,
                22.0,
                if held { 1.0 } else { 0.5 },
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", tier.name),
                frame.rect(RIGHT_X + 90.0, y + 5.0, 440.0, 32.0),
                27.0 * s,
                tier.colour_alpha(if held { 1.0 } else { 0.62 }),
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{} of drops", tier.odds()),
                frame.rect(RIGHT_X + 90.0, y + 35.0, 440.0, 20.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let right = RIGHT_X + RIGHT_WIDTH - 28.0;
            match count {
                Some(count) => {
                    text(
                        &mut self.ui,
                        TextFamily::Display,
                        format_args!("{count}"),
                        frame.rect(right - 130.0 + 60.0, y + 4.0, 70.0, 36.0),
                        32.0 * s,
                        if held { color::TEXT } else { color::QUIET },
                        FontWeight::Semibold,
                        TextAlign::End,
                    );
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("held"),
                        frame.rect(right - 130.0, y + 38.0, 130.0, 18.0),
                        14.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::End,
                    );
                    if !held {
                        padlock(
                            &mut self.ui,
                            frame,
                            right - 80.0,
                            y + ROW_HEIGHT * 0.5 + 2.0,
                            22.0,
                            color::alpha(color::QUIET, 0.9),
                        );
                    }
                }
                None => text(
                    &mut self.ui,
                    TextFamily::Display,
                    format_args!("\u{2014}"),
                    frame.rect(right - 130.0, y + 12.0, 130.0, 36.0),
                    32.0 * s,
                    color::QUIET,
                    FontWeight::Semibold,
                    TextAlign::End,
                ),
            }
        }
    }

    /// What the chosen tier is, and what opening one gives.
    fn about(&mut self, frame: &Frame) {
        let s = frame.s;
        let tier = self.selected();
        let mut y = ABOUT_Y;
        for part in wrap(tier.about(), ABOUT_CHARS).take(2) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 24.0),
                16.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 24.0;
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", crate::cards::inside(self.selected)),
            frame.rect(RIGHT_X, ABOUT_Y + 52.0, RIGHT_WIDTH - LOOT_WIDTH, 24.0),
            15.0 * s,
            color::GOLD,
            FontWeight::Regular,
            TextAlign::Start,
        );
        kit::button(
            &mut self.ui,
            frame,
            [
                RIGHT_X + RIGHT_WIDTH - LOOT_WIDTH,
                ABOUT_Y + 50.0,
                LOOT_WIDTH,
                LOOT_HEIGHT,
            ],
            "Loot table",
            false,
            true,
            false,
            LOOT_TOKEN,
        );
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(RIGHT_X, RULE_Y, RIGHT_WIDTH, 1.0),
            color: color::alpha(color::HOLO, 0.18),
        });
    }

    /// How long to the next holocron, how many dropped today, and a bar of how far along.
    fn progress(&mut self, frame: &Frame) {
        let s = frame.s;
        let next: &str = if self.data.next.is_empty() {
            "The hub has not said how far your next holocron is."
        } else {
            &self.data.next
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{next}"),
            frame.rect(RIGHT_X, NEXT_Y, RIGHT_WIDTH - 190.0, 30.0),
            22.0 * s,
            if self.data.next.is_empty() {
                color::QUIET
            } else {
                color::TEXT
            },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if let Some((today, cap)) = self.data.today {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{today} of {cap} today"),
                frame.rect(RIGHT_X + RIGHT_WIDTH - 190.0, NEXT_Y, 190.0, 30.0),
                17.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::End,
            );
        }
        if let Some(fraction) = self.data.fraction {
            let list = self.ui.draw_list_mut();
            let _ = list.push(DrawCommand::RoundedRect {
                rect: frame.rect(RIGHT_X, BAR_Y, RIGHT_WIDTH, 6.0),
                radius: 3.0 * s,
                color: color::alpha(color::HOLO, 0.14),
            });
            if fraction > 0.0 {
                let _ = list.push(DrawCommand::RoundedRect {
                    rect: frame.rect(RIGHT_X, BAR_Y, (RIGHT_WIDTH * fraction).max(6.0), 6.0),
                    radius: 3.0 * s,
                    color: color::GOLD,
                });
            }
        }
    }

    /// The newest holocrons, two to a row: the tier, when it was found (`dd/mm/yyyy
    /// HH:MM`, UTC) and, for a gift, who gave it and the team's note.
    fn recent(&mut self, frame: &Frame) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            RIGHT_X,
            RECENT_Y + 10.0,
            RIGHT_WIDTH - 130.0,
            "Recent holocrons",
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("times in UTC"),
            frame.rect(RIGHT_X + RIGHT_WIDTH - 130.0, RECENT_Y - 4.0, 130.0, 28.0),
            14.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::End,
        );
        if self.data.recent.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("None yet."),
                frame.rect(RIGHT_X, RECENT_TOP, RIGHT_WIDTH, 24.0),
                16.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            return;
        }
        for (index, recent) in self.data.recent.iter().take(RECENT_SHOWN).enumerate() {
            let tier = &holocrons::TIERS[recent.tier];
            let x = RIGHT_X + (index % RECENT_COLUMNS) as f32 * RECENT_WIDTH;
            let y = RECENT_TOP + (index / RECENT_COLUMNS) as f32 * RECENT_HEIGHT;
            picture(
                &mut self.ui,
                frame,
                recent.tier,
                x + 14.0,
                y + 14.0,
                13.0,
                1.0,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", tier.name),
                frame.rect(x + 38.0, y + 3.0, 210.0, 22.0),
                18.0 * s,
                tier.colour_alpha(if recent.opened { 0.55 } else { 1.0 }),
                FontWeight::Semibold,
                TextAlign::Start,
            );
            if recent.opened && !recent.gift {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("Opened"),
                    frame.rect(x + 38.0, y + 22.0, RECENT_WIDTH - 50.0, 16.0),
                    13.0 * s,
                    color::QUIET,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", recent.when),
                frame.rect(x + 238.0, y + 3.0, RECENT_WIDTH - 238.0 - 12.0, 22.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::End,
            );
            if recent.gift {
                let note = recent.note.trim();
                if note.is_empty() {
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("Gift from the SJK team"),
                        frame.rect(x + 38.0, y + 22.0, RECENT_WIDTH - 50.0, 16.0),
                        13.0 * s,
                        color::GOLD_BRIGHT,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                } else {
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("Gift from the SJK team: \u{201c}{}\u{201d}", cut(note, 38)),
                        frame.rect(x + 38.0, y + 22.0, RECENT_WIDTH - 50.0, 16.0),
                        13.0 * s,
                        color::GOLD_BRIGHT,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
            }
        }
    }

    /// Under the holocron: the chosen tier's name in its colour and how many are held; a
    /// padlock over them when none is, and the holocron above is dimmed.
    fn caption(&mut self, frame: &Frame) {
        let s = frame.s;
        let tier = self.selected();
        let centre = STAGE_AT[0];
        let count = self.data.count(self.selected);
        let held = count.is_some_and(|count| count > 0);
        if !held {
            padlock(
                &mut self.ui,
                frame,
                centre,
                LOCK_Y,
                30.0,
                color::alpha(color::MUTED, 0.9),
            );
        }
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", tier.name),
            frame.rect(centre - 380.0, NAME_Y, 760.0, 56.0),
            46.0 * s,
            tier.colour_alpha(if held { 1.0 } else { 0.7 }),
            FontWeight::Semibold,
            TextAlign::Center,
        );
        let line: &str = match count {
            Some(0) => "None held yet: shown dimmed",
            Some(1) => "1 held",
            None => "Not known yet",
            Some(_) => "",
        };
        match count {
            Some(count) if count > 1 => text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{count} held"),
                frame.rect(centre - 380.0, HELD_Y, 760.0, 28.0),
                19.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Center,
            ),
            _ => text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(centre - 380.0, HELD_Y, 760.0, 28.0),
                19.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Center,
            ),
        }
    }

    /// Open (the oldest of the tier chosen) and Open all, under the caption, centred on
    /// the holocron; dimmed when there is nothing to open. While the hub keeps opening to
    /// staff, a line saying so instead.
    fn buttons(&mut self, frame: &Frame) {
        if self.data.opening_closed() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{CLOSED_LINE}"),
                frame.rect(STAGE_AT[0] - 380.0, BUTTONS_Y, 760.0, BUTTON_HEIGHT),
                19.0 * frame.s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Center,
            );
            return;
        }
        let one = self.open_one().is_some();
        let all = self.data.openable();
        let total = OPEN_WIDTH + BUTTON_GAP + OPEN_ALL_WIDTH;
        let left = STAGE_AT[0] - total * 0.5;
        kit::button(
            &mut self.ui,
            frame,
            [left, BUTTONS_Y, OPEN_WIDTH, BUTTON_HEIGHT],
            "Open",
            true,
            one,
            false,
            OPEN_TOKEN,
        );
        let label = if all > 1 {
            format!("Open all ({})", all.min(sjk_identity::OPEN_MAX))
        } else {
            "Open all".to_owned()
        };
        kit::button(
            &mut self.ui,
            frame,
            [
                left + OPEN_WIDTH + BUTTON_GAP,
                BUTTONS_Y,
                OPEN_ALL_WIDTH,
                BUTTON_HEIGHT,
            ],
            &label,
            false,
            all > 1,
            false,
            OPEN_ALL_TOKEN,
        );
    }

    /// The keys, right-aligned at the bottom.
    fn keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let mut keys: [(&[&str], &str); 7] = [(&[], ""); 7];
        let mut count = 0;
        let mut add = |caps: &'static [&'static str], action: &'static str| {
            keys[count] = (caps, action);
            count += 1;
        };
        add(&["Up", "Down"], "choose");
        add(&["Tab"], "next");
        if self.open_one().is_some() {
            add(&["Enter"], "open");
        }
        if self.data.openable() > 1 {
            add(&["A"], "open all");
        }
        add(&["L"], "loot table");
        if self.hub {
            add(
                &["Ctrl", "Tab"],
                crate::profile_hub::Tab::Holocrons.next(true).label(),
            );
        }
        add(&["Esc"], "back");
        let keys = &keys[..count];
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

/// What the hover card of tier `index` says: how holocrons are found, how often one is
/// of this tier, and how many the player holds.
pub(super) fn tier_card(index: usize, data: &Data) -> crate::menu::sjk::hover::Card {
    use crate::menu::sjk::hover::{Card, Line};
    let tier = &crate::holocrons::TIERS[index.min(COUNT - 1)];
    let minutes = data.every.map_or(30, |every| (every / 60).max(1));
    let cap = data.today.map_or(8, |(_, cap)| cap);
    let mut lines = vec![
        Line::how(format!(
            "Found by playing on a server: one holocron every {minutes} minutes of active play, at most {cap} a day."
        )),
        Line::status(format!(
            "{} of drops are {}. The SJK team can also give one.",
            tier.odds(),
            tier.label
        )),
    ];
    match data.count(tier.index) {
        Some(0) => lines.push(Line::status("None held yet")),
        Some(count) => lines.push(Line::held(format!("You hold {count}"))),
        None => {}
    }
    Card {
        title: tier.name.to_owned(),
        colour: tier.colour,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tier's hover card says how holocrons are found, the tier's share and how many
    /// the player holds.
    #[test]
    fn a_tier_card_says_how_holocrons_are_found() {
        let mut data = Data::nothing(State::Known);
        data.counts = Some([3, 0, 0, 1]);
        data.every = Some(1_800);
        let card = tier_card(1, &data);
        assert_eq!(card.title, "Rare Holocron");
        assert_eq!(
            card.lines[0].words,
            "Found by playing on a server: one holocron every 30 minutes of active play, at most 8 a day."
        );
        assert_eq!(
            card.lines[1].words,
            "28% of drops are Rare. The SJK team can also give one."
        );
        assert_eq!(card.lines[2].words, "None held yet");
        assert_eq!(tier_card(0, &data).lines[2].words, "You hold 3");
        assert_eq!(tier_card(0, &Data::nothing(State::Waiting)).lines.len(), 2);
    }

    fn counts(counts: [u32; COUNT]) -> Data {
        Data {
            counts: Some(counts),
            recent: vec![
                Recent {
                    tier: 3,
                    when: "10/10/2026 14:05".into(),
                    gift: true,
                    note: "n ".repeat(100),
                    opened: false,
                },
                Recent {
                    tier: 0,
                    when: "09/10/2026 21:40".into(),
                    gift: false,
                    note: String::new(),
                    opened: true,
                },
            ],
            next: "Next holocron in about 20 minutes of play".into(),
            today: Some((3, 8)),
            fraction: Some(1.0 / 3.0),
            ..Data::nothing(State::Known)
        }
    }

    /// The most the page can show: ten recent holocrons, gifts with the longest note.
    fn full() -> Data {
        let mut data = counts([123, 45, 6, 1]);
        data.recent = (0..RECENT_SHOWN)
            .map(|index| Recent {
                tier: index % COUNT,
                when: "10/10/2026 14:05".into(),
                gift: index % 3 == 0,
                note: if index % 3 == 0 {
                    "n ".repeat(100)
                } else {
                    String::new()
                },
                opened: index % 4 == 1,
            })
            .collect();
        data
    }

    /// Every state fits the canvas and its places at 1080p, 4K, 4:3 and 21:9, under the
    /// Profile screen's tabs and on its own, every tier chosen: the rows, the recent list
    /// and the caption stay inside the window and clear of the keys' line, and the
    /// holocron's place is clear of the right-hand panel.
    #[test]
    fn every_state_fits_the_canvas() {
        let states = [
            Data::nothing(State::IdentityOff),
            Data::nothing(State::NoHub),
            Data::nothing(State::Waiting),
            Data {
                counts: Some([0; COUNT]),
                ..Data::nothing(State::Known)
            },
            counts([3, 0, 1, 0]),
            counts([0, 0, 0, 1]),
            full(),
        ];
        for data in states {
            for hub in [true, false] {
                for tier in 0..COUNT {
                    let mut panel = Panel::new();
                    panel.open(true);
                    panel.set_hub(hub);
                    panel.data = data.clone();
                    panel.select(tier);
                    for viewport in [
                        [1920.0, 1080.0],
                        [3840.0, 2160.0],
                        [1440.0, 1080.0],
                        [2560.0, 1080.0],
                    ] {
                        panel.build(viewport);
                        assert!(
                            !panel.overflowed(),
                            "{:?} {tier} hub {hub} at {viewport:?}",
                            data.state
                        );
                        let frame = Frame::new(viewport);
                        let shift = if hub { crate::profile_hub::SHIFT } else { 0.0 };
                        let keys = frame.point(0.0, KEYS_Y)[1];
                        let panel_left = frame.point(RIGHT_X, 0.0)[0];
                        for index in 0..COUNT {
                            let rect = panel
                                .ui
                                .rect_for(ROW_BASE + index as u16)
                                .expect("a tier's row");
                            assert!(rect.x >= panel_left - 0.5 && rect.right() <= viewport[0]);
                            assert!(rect.bottom() < keys, "{viewport:?}");
                        }
                        // The last recent row ends above the keys.
                        let last = frame.point(
                            0.0,
                            RECENT_TOP
                                + shift
                                + (RECENT_SHOWN / RECENT_COLUMNS) as f32 * RECENT_HEIGHT,
                        )[1];
                        assert!(last < keys, "{viewport:?}: the list reaches {last}");
                        // The caption's last line ends above the keys too.
                        assert!(frame.point(0.0, HELD_Y + shift + 28.0)[1] < keys);
                        // The holocron floats in clear world, left of the panel.
                        let at = frame.point(STAGE_AT[0], STAGE_AT[1]);
                        assert!(
                            at[0] + holocrons::stage::SIZE * frame.s < panel_left,
                            "{viewport:?}: the holocron reaches the panel"
                        );
                    }
                }
            }
        }
    }

    /// The Collection screen's tabs are drawn by the page and answer where they are drawn.
    #[test]
    fn the_hub_page_draws_the_screens_tabs_with_holocrons_lit() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.set_hub(true);
        panel.build([1920.0, 1080.0]);
        for tab in crate::profile_hub::Screen::Collection.tabs() {
            assert!(
                panel
                    .ui
                    .rect_for(crate::profile_hub::TOKEN + tab.index() as u16)
                    .is_some(),
                "{tab:?}"
            );
        }
        panel.set_hub(false);
        panel.build([1920.0, 1080.0]);
        assert!(panel.ui.rect_for(crate::profile_hub::TOKEN).is_none());
    }

    #[test]
    fn long_notes_are_cut() {
        assert_eq!(cut("abcdefgh", 6), "abc...");
        assert_eq!(cut("abc", 6), "abc");
    }
}

//! The Collection's Achievements tab: a wall of medallions, a row to each category
//! under its name and count, each medallion ringed by the count in the category's
//! colour, filled and gold once unlocked, the goal inside and the name under it; the
//! one chosen is shown up close on the right with what to do and how far the player
//! is, and under it the three nearest to unlocking.

use super::view::{LEAD_Y, LEFT_X, SHOWCASE_WIDTH, SHOWCASE_X, centred_lines, glow};
use super::*;
use crate::achievements::Category;
use crate::achievements::medallion::{self, Medallion, tint};
use crate::menu::sjk::{Frame, color, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use sjk_ui::{DrawCommand, FontWeight, TextAlign};

/// The wall: where its first category starts, the room a category takes, a medallion's
/// cell and its radius.
const WALL_TOP: f32 = 286.0;
const GROUP_STEP: f32 = 172.0;
const CELL: f32 = 104.0;
const RADIUS: f32 = 33.0;
/// The medallion up close.
const BIG_RADIUS: f32 = 104.0;
/// How many the Next up list shows.
const NEXT_UP: usize = 3;

/// The categories in the catalogue's order, each with its achievements' indices.
fn groups() -> Vec<(Category, Vec<usize>)> {
    let mut groups: Vec<(Category, Vec<usize>)> = Vec::with_capacity(4);
    for (index, kind) in crate::achievements::ALL.iter().enumerate() {
        match groups
            .iter_mut()
            .find(|(category, _)| *category == kind.category)
        {
            Some((_, members)) => members.push(index),
            None => groups.push((kind.category, vec![index])),
        }
    }
    groups
}

/// The achievement Up (`down` false) or Down from `index` lands on: the next
/// category's, as near its column as it has.
pub(super) fn vertical(index: usize, down: bool) -> usize {
    let groups = groups();
    let Some((row, column)) = groups.iter().enumerate().find_map(|(row, (_, members))| {
        members
            .iter()
            .position(|member| *member == index)
            .map(|column| (row, column))
    }) else {
        return index.min(crate::achievements::ALL.len().saturating_sub(1));
    };
    let next = if down {
        (row + 1).min(groups.len() - 1)
    } else {
        row.saturating_sub(1)
    };
    let members = &groups[next].1;
    members[column.min(members.len() - 1)]
}

/// The one to show first: the locked one nearest to unlocking, else the first.
fn first_choice(standings: &[Standing]) -> usize {
    standings
        .iter()
        .enumerate()
        .filter(|(_, standing)| standing.unlocked.is_none())
        .max_by(|a, b| a.1.fraction().total_cmp(&b.1.fraction()))
        .map_or(0, |(index, _)| index)
}

/// The locked ones nearest to unlocking but `chosen`, nearest first.
fn next_up(standings: &[Standing], chosen: usize) -> Vec<usize> {
    let mut locked: Vec<usize> = (0..standings.len())
        .filter(|index| *index != chosen && standings[*index].unlocked.is_none())
        .collect();
    locked.sort_by(|a, b| {
        standings[*b]
            .fraction()
            .total_cmp(&standings[*a].fraction())
    });
    locked.truncate(NEXT_UP);
    locked
}

impl Panel {
    /// The Achievements tab.
    pub(super) fn achievements(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let standings = inputs.standings;
        if standings.is_empty() {
            self.lead(frame, "No achievements", "");
            return;
        }
        if self.achievement >= standings.len() {
            self.achievement = first_choice(standings);
        }
        let s = frame.s;
        let unlocked = standings
            .iter()
            .filter(|standing| standing.unlocked.is_some())
            .count();
        self.lead(
            frame,
            &format!("{unlocked} of {} unlocked", standings.len()),
            "",
        );
        let fraction = unlocked as f32 / standings.len() as f32;
        bar(
            &mut self.ui,
            frame,
            [LEFT_X + 300.0, LEAD_Y + 16.0, 300.0, 6.0],
            fraction,
            color::GOLD,
        );
        for (row, (category, members)) in groups().into_iter().enumerate() {
            let top = WALL_TOP + row as f32 * GROUP_STEP;
            let hue = tint(category);
            let held = members
                .iter()
                .filter(|index| standings.get(**index).is_some_and(|s| s.unlocked.is_some()))
                .count();
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", category.name()),
                frame.rect(LEFT_X, top, 300.0, 28.0),
                22.0 * s,
                hue,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let name_width = crate::text::display_width(category.name(), 22.0);
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{held} of {}", members.len()),
                frame.rect(LEFT_X + name_width + 14.0, top + 2.0, 120.0, 24.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let rule_x = LEFT_X + name_width + 90.0;
            let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
                rect: frame.rect(rule_x, top + 14.0, LEFT_X + 9.0 * CELL - rule_x, 1.0),
                color: color::alpha(hue, 0.3),
            });
            for (column, index) in members.iter().enumerate() {
                if let Some(standing) = standings.get(*index) {
                    let x = LEFT_X + column as f32 * CELL;
                    self.wall_medallion(frame, standing, *index, x, top + 40.0);
                }
            }
        }
        if let Some(standing) = standings.get(self.achievement) {
            self.achievement_up_close(frame, standing);
            self.next_up(frame, standings);
        }
        centred_lines(
            &mut self.ui,
            frame,
            "Counted on this PC in your matches on servers, and kept on the SJK hub with your identity on.",
            [SHOWCASE_X, 950.0, SHOWCASE_WIDTH],
            96,
            1,
            15.0,
            color::QUIET,
        );
    }

    /// Achievement `index`'s medallion in its cell from (`x`, `top`), its name under it.
    fn wall_medallion(
        &mut self,
        frame: &Frame,
        standing: &Standing,
        index: usize,
        x: f32,
        top: f32,
    ) {
        let s = frame.s;
        let chosen = index == self.achievement;
        let done = standing.unlocked.is_some();
        let centre = [x + CELL * 0.5 - 4.0, top + RADIUS + 4.0];
        if done {
            glow(&mut self.ui, frame, centre, RADIUS * 1.5, color::GOLD, 0.16);
        }
        medallion::draw(
            &mut self.ui,
            Medallion {
                kind: standing.kind,
                centre: frame.point(centre[0], centre[1]),
                radius: RADIUS * s,
                fraction: standing.fraction(),
                done,
            },
        );
        if chosen {
            let _ = self.ui.draw_list_mut().push(DrawCommand::Arc {
                center: frame.point(centre[0], centre[1]),
                radius: (RADIUS + 9.0) * s,
                width: 2.0 * s,
                start: 0.0,
                sweep: std::f32::consts::TAU,
                color: color::GOLD_BRIGHT,
                knockout: None,
            });
        }
        let mut y = top + RADIUS * 2.0 + 14.0;
        for part in wrap(standing.kind.name, 13).take(2) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(centre[0] - CELL * 0.5, y, CELL, 20.0),
                15.0 * s,
                match (chosen, done) {
                    (true, _) => color::GOLD_BRIGHT,
                    (false, true) => color::TEXT,
                    (false, false) => color::MUTED,
                },
                FontWeight::Regular,
                TextAlign::Center,
            );
            y += 18.0;
        }
        self.ui.hit_region(
            ACHIEVEMENT_BASE + index as u16,
            frame.rect(centre[0] - CELL * 0.5, top, CELL - 4.0, RADIUS * 2.0 + 52.0),
        );
    }

    /// The achievement chosen, up close on the right.
    fn achievement_up_close(&mut self, frame: &Frame, standing: &Standing) {
        let s = frame.s;
        let kind = standing.kind;
        let done = standing.unlocked.is_some();
        let hue = tint(kind.category);
        let middle = SHOWCASE_X + SHOWCASE_WIDTH * 0.5;
        let centre = [middle, 226.0 + BIG_RADIUS + 8.0];
        glow(
            &mut self.ui,
            frame,
            centre,
            BIG_RADIUS * 2.0,
            if done { color::GOLD } else { hue },
            if done { 0.32 } else { 0.16 },
        );
        medallion::draw(
            &mut self.ui,
            Medallion {
                kind,
                centre: frame.point(centre[0], centre[1]),
                radius: BIG_RADIUS * s,
                fraction: standing.fraction(),
                done,
            },
        );
        let mut y = centre[1] + BIG_RADIUS + 28.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", kind.name),
            frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 56.0),
            50.0 * s,
            if done {
                color::GOLD_BRIGHT
            } else {
                color::TEXT
            },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        y += 60.0;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", kind.category.name()),
            frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 22.0),
            16.0 * s,
            hue,
            FontWeight::Semibold,
            TextAlign::Center,
        );
        y += 30.0;
        y = centred_lines(
            &mut self.ui,
            frame,
            kind.description,
            [SHOWCASE_X, y, SHOWCASE_WIDTH],
            56,
            2,
            20.0,
            color::TEXT,
        );
        y += 12.0;
        let width = 520.0;
        bar(
            &mut self.ui,
            frame,
            [middle - width * 0.5, y, width, 6.0],
            standing.fraction(),
            if done { color::GOLD } else { hue },
        );
        let progress = format!(
            "{} / {}",
            kind.amount(standing.progress.min(kind.goal)),
            kind.amount(kind.goal)
        );
        let status = match standing.unlocked {
            Some(at) if at > 0 => format!("Unlocked {}", crate::medals::date_text(at)),
            Some(_) => "Unlocked".to_owned(),
            None => {
                let left = kind.goal.saturating_sub(standing.progress);
                format!("{} more to go", kind.amount(left))
            }
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{progress}"),
            frame.rect(middle - width * 0.5, y + 14.0, width * 0.5, 22.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{status}"),
            frame.rect(middle, y + 14.0, width * 0.5, 22.0),
            16.0 * s,
            if done {
                color::GOLD_BRIGHT
            } else {
                color::MUTED
            },
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// The locked achievements nearest to unlocking, under the one up close.
    fn next_up(&mut self, frame: &Frame, standings: &[Standing]) {
        let s = frame.s;
        let width = 560.0;
        let x = SHOWCASE_X + (SHOWCASE_WIDTH - width) * 0.5;
        let near = next_up(standings, self.achievement);
        if near.is_empty() {
            return;
        }
        let top = 762.0;
        kit::heading(&mut self.ui, frame, x, top, width, "Next up");
        for (row, index) in near.into_iter().enumerate() {
            let standing = &standings[index];
            let kind = standing.kind;
            let hue = tint(kind.category);
            let y = top + 34.0 + row as f32 * 50.0;
            medallion::draw(
                &mut self.ui,
                Medallion {
                    kind,
                    centre: frame.point(x + 20.0, y + 20.0),
                    radius: 19.0 * s,
                    fraction: standing.fraction(),
                    done: false,
                },
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", kind.name),
                frame.rect(x + 54.0, y + 6.0, 260.0, 28.0),
                22.0 * s,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            bar(
                &mut self.ui,
                frame,
                [x + 320.0, y + 18.0, 150.0, 4.0],
                standing.fraction(),
                hue,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{} / {}",
                    kind.amount(standing.progress),
                    kind.amount(kind.goal)
                ),
                frame.rect(x + 470.0, y + 8.0, 90.0, 22.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::End,
            );
        }
    }
}

/// A progress bar over `rect` (frame pixels), filled to `fraction` in `fill`.
fn bar(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    fraction: f32,
    fill: sjk_ui::Color,
) {
    let [x, y, width, height] = rect;
    let s = frame.s;
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect: frame.rect(x, y, width, height),
        radius: height * 0.5 * s,
        color: color::alpha(color::HOLO, 0.14),
    });
    let filled = width * fraction.clamp(0.0, 1.0);
    if filled > 0.5 {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, y, filled, height),
            radius: height * 0.5 * s,
            color: fill,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_and_down_move_between_the_categories_near_the_column() {
        let groups = groups();
        assert_eq!(groups.len(), 4);
        let total: usize = groups.iter().map(|(_, members)| members.len()).sum();
        assert_eq!(total, crate::achievements::ALL.len());
        let (first, second) = (&groups[0].1, &groups[1].1);
        // Down from a far column lands on the next category's last.
        let far = *first.last().unwrap();
        assert_eq!(vertical(far, true), *second.last().unwrap());
        assert_eq!(vertical(second[0], false), first[0]);
        // Up from the first category and Down from the last stay.
        assert_eq!(vertical(first[2], false), first[2]);
        let last = &groups[3].1;
        assert_eq!(vertical(last[1], true), last[1]);
    }

    #[test]
    fn the_board_opens_on_the_nearest_to_unlocking_and_lists_the_next() {
        let held = [sjk_identity::Achievement {
            id: "first_blood".into(),
            progress: 1,
            goal: 1,
            unlocked: 1_791_336_225,
        }];
        let standings = crate::achievements::standings(&held);
        let chosen = first_choice(&standings);
        assert!(standings[chosen].unlocked.is_none());
        let near = next_up(&standings, chosen);
        assert!(near.len() <= NEXT_UP && !near.contains(&chosen));
        assert!(
            near.iter()
                .all(|index| standings[*index].unlocked.is_none())
        );
    }

    /// Every medallion stands in the canvas over the keys and answers the pointer, at
    /// 1080 lines, 4K, 4:3 and 21:9.
    #[test]
    fn the_wall_fits_and_answers_the_pointer() {
        let standings = crate::achievements::standings(&[]);
        let inputs = Inputs {
            standings: &standings,
            ..super::super::tests::inputs(Holdings::Known(&[]), "")
        };
        let font = crate::text::load_modern(1.0, None).expect("Inter").font;
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [2_560.0, 1_080.0],
        ] {
            let mut panel = Panel::new();
            panel.open(Tab::Achievements, true, true, ReturnTarget::InGame);
            panel.build(&inputs, &font, viewport);
            assert!(!panel.ui.overflowed(), "{viewport:?}");
            let frame = Frame::new(viewport);
            let keys = frame.point(0.0, super::super::view::KEYS_Y)[1];
            for index in 0..standings.len() {
                let rect = panel
                    .ui
                    .rect_for(ACHIEVEMENT_BASE + index as u16)
                    .expect("a medallion's area");
                assert!(rect.bottom() <= keys, "{index} {viewport:?}");
                assert!(rect.right() <= frame.point(SHOWCASE_X, 0.0)[0]);
            }
            for command in panel.ui.draw_list().commands() {
                if let DrawCommand::Text { rect, .. } = command {
                    assert!(rect.bottom() <= keys || rect.y >= keys, "{rect:?}");
                }
            }
        }
    }
}

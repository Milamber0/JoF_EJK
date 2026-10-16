//! The mark after a player's name on the scoreboard when the SJK hub knows them
//! (`player_identity.rs`): SJK's emblem, in gold for a player the hub's operator
//! vouches for and in soft white for any other registered player, then up to three
//! ribbon bars for the medals the SJK team gave them (`medals.rs`), plain coloured
//! rectangles sized after the emblem. The marks take room from the name, never from
//! the columns: where the name would keep less than half its room, fewer bars show.

use crate::medals::{Medal, Medals};
use crate::player_identity::Tag;
use sjk_ui::{Color, DrawCommand, DrawList, Rect};

const VERIFIED: Color = Color::new(1.0, 0.82, 0.25, 1.0);
const REGISTERED: Color = Color::new(0.92, 0.94, 1.0, 0.9);
/// Most ribbon bars a row shows.
const RIBBONS_MAX: usize = 3;
/// A bar's width, height and the gap before it, as fractions of the emblem's side.
const RIBBON_WIDTH: f32 = 1.05;
const RIBBON_HEIGHT: f32 = 0.42;
const RIBBON_GAP: f32 = 0.2;
/// The share of the name's room the marks may take.
const MARKS_SHARE: f32 = 0.5;

/// The emblem's tint.
pub(super) fn tint(tag: Tag) -> Color {
    if tag.verified { VERIFIED } else { REGISTERED }
}

/// A ribbon bar's width and height, and the gap before it, for an emblem `side` across.
pub(super) fn ribbon_size(side: f32) -> (f32, f32, f32) {
    (side * RIBBON_WIDTH, side * RIBBON_HEIGHT, side * RIBBON_GAP)
}

/// How many of `medals` show as bars after an emblem `side` across when the marks may
/// take `room` pixels in all (the emblem's included): at most three, fewer where they
/// do not fit.
pub(super) fn ribbon_count(medals: Medals, side: f32, room: f32) -> usize {
    let (width, _, gap) = ribbon_size(side);
    let mut count = medals.len().min(RIBBONS_MAX);
    while count > 0 && side + count as f32 * (gap + width) > room {
        count -= 1;
    }
    count
}

/// The width the emblem and `count` bars take.
pub(super) fn marks_width(side: f32, count: usize) -> f32 {
    let (width, _, gap) = ribbon_size(side);
    side + count as f32 * (gap + width)
}

/// Ribbon bar `medal` over `rect`: its base colour, then its stripes.
pub(super) fn ribbon(medal: Medal, rect: Rect, mut push: impl FnMut(DrawCommand)) {
    let ribbon = medal.ribbon();
    push(DrawCommand::SolidRect {
        rect,
        color: ribbon.base,
    });
    for stripe in ribbon.stripes {
        push(DrawCommand::SolidRect {
            rect: Rect::new(
                rect.x + rect.width * stripe.at,
                rect.y,
                rect.width * stripe.width,
                rect.height,
            ),
            color: stripe.color,
        });
    }
}

/// The emblem and the first `count` of `medals` as bars, from `x` rightwards, centred
/// on `middle`.
pub(super) fn draw_marks(
    x: f32,
    middle: f32,
    side: f32,
    tag: Tag,
    count: usize,
    mut push: impl FnMut(DrawCommand),
) {
    push(DrawCommand::TexturedQuad {
        rect: Rect::new(x, middle - side * 0.5, side, side),
        texture: crate::ui_renderer::LOGO_TEXTURE,
        color: tint(tag),
    });
    let (width, height, gap) = ribbon_size(side);
    let mut at = x + side + gap;
    for (medal, _) in tag.medals.iter().take(count) {
        ribbon(
            medal,
            Rect::new(at, middle - height * 0.5, width, height),
            &mut push,
        );
        at += width + gap;
    }
}

/// The emblem and the bars that fit in the right end of `name` (a row's name
/// rectangle), the emblem `side` wide, centred vertically; returns `name` without the
/// room they take.
pub(super) fn push(list: &mut DrawList, name: Rect, side: f32, tag: Tag) -> Rect {
    let count = ribbon_count(tag.medals, side, name.width * MARKS_SHARE);
    let width = marks_width(side, count);
    draw_marks(
        name.x + name.width - width,
        name.y + name.height * 0.5,
        side,
        tag,
        count,
        |command| {
            let _ = list.push(command);
        },
    );
    Rect::new(
        name.x,
        name.y,
        (name.width - width - side * 0.25).max(0.0),
        name.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(verified: bool, medals: &[&str]) -> Tag {
        let list: Vec<sjk_identity::Medal> = medals
            .iter()
            .map(|id| sjk_identity::Medal {
                id: (*id).to_owned(),
                count: 1,
                awarded: 0,
                note: String::new(),
            })
            .collect();
        Tag {
            verified,
            medals: Medals::from_wire(&list),
        }
    }

    #[test]
    fn verified_players_are_tinted_apart_from_registered_ones() {
        assert_ne!(tint(tag(true, &[])), tint(tag(false, &[])));
    }

    #[test]
    fn the_emblem_sits_at_the_right_end_and_the_name_leaves_room() {
        let name = Rect::new(10.0, 20.0, 200.0, 30.0);
        let mut list = DrawList::new(16);
        let text = push(&mut list, name, 20.0, tag(false, &[]));
        let Some(DrawCommand::TexturedQuad { rect, .. }) = list.commands().first() else {
            panic!("a textured quad");
        };
        assert_eq!(
            (rect.x + rect.width, rect.y + rect.height * 0.5),
            (210.0, 35.0)
        );
        assert!(
            text.x + text.width < rect.x,
            "the name ends before the emblem"
        );
    }

    #[test]
    fn ribbons_follow_the_emblem_and_end_at_the_names_right_edge() {
        let name = Rect::new(10.0, 20.0, 300.0, 30.0);
        let mut list = DrawList::new(64);
        let text = push(
            &mut list,
            name,
            20.0,
            tag(true, &["early_tester", "bug_hunter"]),
        );
        let rects: Vec<Rect> =
            list.commands()
                .iter()
                .map(|command| match command {
                    DrawCommand::TexturedQuad { rect, .. }
                    | DrawCommand::SolidRect { rect, .. } => *rect,
                    other => panic!("unexpected {other:?}"),
                })
                .collect();
        let emblem = rects[0];
        // The early tester's base and three stripes, then the bug hunter's base and edges.
        assert_eq!(rects.len(), 1 + 4 + 3);
        let first = rects[1];
        let last = rects[5];
        assert!(first.x > emblem.x + emblem.width, "after the emblem");
        assert!(last.x > first.x + first.width, "one after the other");
        assert!(
            (last.x + last.width - 310.0).abs() < 1e-3,
            "ends at the edge"
        );
        assert!(text.x + text.width < emblem.x);
    }

    #[test]
    fn at_most_three_bars_and_fewer_when_the_name_is_short() {
        let all = tag(false, &["early_tester", "early_contributor", "bug_hunter"]);
        assert_eq!(ribbon_count(all.medals, 20.0, 1_000.0), 3);
        assert_eq!(ribbon_count(all.medals, 20.0, marks_width(20.0, 2)), 2);
        assert_eq!(ribbon_count(all.medals, 20.0, 25.0), 0);
        assert_eq!(ribbon_count(tag(false, &[]).medals, 20.0, 1_000.0), 0);
        let mut list = DrawList::new(64);
        let narrow = Rect::new(0.0, 0.0, 80.0, 20.0);
        let text = push(&mut list, narrow, 16.0, all);
        assert!(text.width >= 0.0);
        assert!(
            list.commands().len() < 1 + 4 + 2 + 3,
            "not every bar fits a short name"
        );
    }
}

//! The SJK UI's in-game main page beyond its list (`docs/sjk-ui.md`, In-game menu):
//! the row of small icon buttons under the emblem, and the match card's controls
//! (the vote on at its top, the player's side under its numbers, Call a vote at its
//! foot). The model, the order and the keys are [`super::super::sjk_focus`]'s.
//!
//! Positions are pixels of the SJK UI's 16:9 frame ([`Frame`]).

use super::*;
use crate::menu::sjk::{kit, wrap};
use sjk_client::LegacyTeamChoice;

/// The bottom row's icons: their top, size and pitch, from the frame's left
/// margin, and the line over them that names the one hovered or focused.
const ROW_X: f32 = 96.0;
const ROW_TOP: f32 = 852.0;
const ICON: f32 = 54.0;
const ICON_PITCH: f32 = 66.0;
const LABEL_Y: f32 = ROW_TOP - 40.0;
/// Pointer token of icon `i` of [`Icon::ALL`].
pub(crate) const ICON_TOKEN: u16 = 800;
/// Pointer token of the card's control `i` this frame.
pub(crate) const CONTROL_TOKEN: u16 = 820;

/// The vote's block over the card: its top, and the side's line under the card's
/// numbers.
const VOTE_TOP: f32 = 66.0;
const SIDE_TOP: f32 = 600.0;
/// The card's buttons: their height and gap.
const BUTTON: f32 = 40.0;
const BUTTON_GAP: f32 = 12.0;

/// The token the canvas focuses: the list's chosen row, an icon, a card control or the
/// docked chat's field.
pub(super) fn focus_token(row: usize, extras: &Extras<'_>) -> u16 {
    match extras.focus {
        Focus::List => row as u16,
        Focus::Row(icon) => icon_token(icon),
        Focus::Card(control) => extras
            .controls
            .as_slice()
            .iter()
            .position(|placed| placed.control == control)
            .map_or(row as u16, |index| CONTROL_TOKEN + index as u16),
        Focus::Chat => CHAT_TOKENS.field,
    }
}

/// The pointer token of `icon`.
pub(crate) fn icon_token(icon: Icon) -> u16 {
    let index = Icon::ALL.iter().position(|each| *each == icon).unwrap_or(0);
    ICON_TOKEN + index as u16
}

/// The icon a pointer token names.
pub(crate) fn icon_of(token: u16) -> Option<Icon> {
    token
        .checked_sub(ICON_TOKEN)
        .and_then(|index| Icon::ALL.get(usize::from(index)))
        .copied()
}

/// The row of small icon buttons under the emblem: each icon's picture, ringed gold
/// and lit while hovered or focused, Staff quieter; the one hovered or focused named
/// over the row with its key and what it opens.
pub(super) fn icon_row(canvas: &mut MenuCanvas, frame: &Frame, extras: &Extras<'_>) {
    let s = frame.s;
    let mut named = None;
    for (index, icon) in extras.icons.iter().enumerate() {
        let token = icon_token(*icon);
        let x = ROW_X + index as f32 * ICON_PITCH;
        let focused = extras.focus == Focus::Row(*icon);
        let lit = focused || canvas.token_hovered(token);
        if lit && (named.is_none() || focused) {
            named = Some(*icon);
        }
        let rect = frame.rect(x, ROW_TOP, ICON, ICON);
        let alpha = match (lit, *icon) {
            (true, _) => 1.0,
            (false, Icon::Staff) => 0.5,
            (false, _) => 0.78,
        };
        if let Some(texture) = crate::settings_icons::texture(icon.picture()) {
            let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect,
                texture,
                color: Color::new(1.0, 1.0, 1.0, alpha),
            });
        }
        if lit {
            let ring = frame.rect(x - 3.0, ROW_TOP - 3.0, ICON + 6.0, ICON + 6.0);
            let _ = canvas.draw_list_mut().push(DrawCommand::Border {
                rect: ring,
                radius: ring.height * 0.5,
                width: 2.0 * s,
                color: if focused {
                    color::GOLD_BRIGHT
                } else {
                    color::alpha(color::GOLD, 0.7)
                },
            });
        }
        canvas.hit_region(token, rect);
    }
    let Some(icon) = named else {
        return;
    };
    let label = icon.label();
    // Rajdhani at 24 is about 9.6 pixels a character.
    let label_width = 9.6 * label.chars().count() as f32 + 8.0;
    text(
        canvas,
        TextFamily::Display,
        format_args!("{label}"),
        frame.rect(ROW_X, LABEL_Y, label_width + 20.0, 30.0),
        24.0 * s,
        color::GOLD_BRIGHT,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let mut x = ROW_X + label_width + 10.0;
    if let Some(key) = icon.key() {
        let [left, top] = frame.point(x, LABEL_Y + 3.0);
        x = (key_hint(canvas, &[key], "", left, top, s) - frame.origin[0]) / s;
    }
    text(
        canvas,
        TextFamily::Body,
        format_args!("{}", icon.hint()),
        frame.rect(x, LABEL_Y + 3.0, 560.0, 24.0),
        16.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
}

/// The card's controls: while a vote is on, a block over the card with the question,
/// the count and Yes and No (gold); under the card's numbers the player's side and
/// its buttons; without a vote, a quiet Call a vote at the foot.
pub(super) fn card_controls(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    card: &Card,
    view: &View<'_>,
    extras: &Extras<'_>,
) {
    let s = frame.s;
    let placed = extras.controls.as_slice();
    let focused = |control| extras.focus == Focus::Card(control);
    let token = |control| {
        placed
            .iter()
            .position(|each| each.control == control)
            .map_or(CONTROL_TOKEN, |index| CONTROL_TOKEN + index as u16)
    };
    // The vote on.
    if extras.controls.takes(Control::VoteYes) {
        text(
            canvas,
            TextFamily::Display,
            format_args!("A vote is on"),
            frame.rect(CARD_X, VOTE_TOP, CARD_WIDTH, 28.0),
            22.0 * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            canvas,
            TextFamily::Body,
            format_args!("{} yes, {} no", card.vote_yes, card.vote_no),
            frame.rect(CARD_X, VOTE_TOP, CARD_WIDTH, 28.0),
            17.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::End,
        );
        let question = if card.vote.is_empty() {
            "Cast your vote"
        } else {
            card.vote.as_str()
        };
        let mut y = VOTE_TOP + 34.0;
        for line in wrap(question, 30).take(2) {
            text(
                canvas,
                TextFamily::Display,
                format_args!("{line}"),
                frame.rect(CARD_X, y, CARD_WIDTH, 36.0),
                30.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 36.0;
        }
        y += 10.0;
        let mut x = CARD_X;
        for control in [Control::VoteYes, Control::VoteNo] {
            kit::button(
                canvas,
                frame,
                [x, y, 140.0, BUTTON],
                control.label(),
                true,
                true,
                focused(control),
                token(control),
            );
            x += 140.0 + BUTTON_GAP;
        }
    }
    // The player's side.
    let (side, colour) = match (view.team_game, card.team, card.you) {
        (_, 3, _) | (_, _, You::Watching) => ("Watching", color::MUTED),
        (true, 1, _) => ("Red team", team_colour(1)),
        (true, 2, _) => ("Blue team", team_colour(2)),
        _ => ("Playing", color::TEXT),
    };
    text(
        canvas,
        TextFamily::Body,
        format_args!("Your side"),
        frame.rect(CARD_X, SIDE_TOP, 110.0, 30.0),
        16.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
    text(
        canvas,
        TextFamily::Display,
        format_args!("{side}"),
        frame.rect(CARD_X + 100.0, SIDE_TOP, CARD_WIDTH - 100.0, 30.0),
        26.0 * s,
        colour,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let mut x = CARD_X;
    let top = SIDE_TOP + 40.0;
    for each in placed
        .iter()
        .filter(|each| each.line == super::super::sjk_focus::SIDE_LINE)
    {
        let width = match each.control {
            Control::SiegeClass => 200.0,
            _ => (CARD_WIDTH - BUTTON_GAP * 2.0) / 3.0,
        };
        kit::button(
            canvas,
            frame,
            [x, top, width, BUTTON],
            each.control.label(),
            false,
            each.enabled,
            focused(each.control),
            token(each.control),
        );
        if let Control::Team(team @ (LegacyTeamChoice::Red | LegacyTeamChoice::Blue)) = each.control
        {
            // The side's colour as a mark at the button's left, over its pill.
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x + 14.0, top + 13.0, 4.0, 14.0),
                radius: 2.0 * s,
                color: color::alpha(
                    team_colour(if team == LegacyTeamChoice::Red { 1 } else { 2 }),
                    if each.enabled { 1.0 } else { 0.4 },
                ),
            });
        }
        x += width + BUTTON_GAP;
    }
    // Call a vote, quiet, at the foot.
    if extras.controls.takes(Control::CallVote) {
        let y = top + BUTTON + 26.0;
        let id = token(Control::CallVote);
        let lit = focused(Control::CallVote) || canvas.token_hovered(id);
        let label = Control::CallVote.label();
        // Rajdhani at 21 is about 7.6 pixels a character.
        let width = 7.6 * label.chars().count() as f32 + 6.0;
        text(
            canvas,
            TextFamily::Display,
            format_args!("{label}"),
            frame.rect(CARD_X, y, width + 20.0, 28.0),
            21.0 * s,
            if lit {
                color::GOLD_BRIGHT
            } else {
                color::MUTED
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        if lit {
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(CARD_X, y + 27.0, width, 2.0),
                radius: 1.0 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        text(
            canvas,
            TextFamily::Body,
            format_args!("Map, mode, kick, limits"),
            frame.rect(
                CARD_X + width + 14.0,
                y + 2.0,
                CARD_WIDTH - width - 14.0,
                26.0,
            ),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
        canvas.hit_region(id, frame.rect(CARD_X - 6.0, y - 4.0, width + 12.0, 36.0));
    }
}

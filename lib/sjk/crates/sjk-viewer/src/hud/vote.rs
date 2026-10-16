//! Modern vote panel backed by BaseJKA vote configstrings.

use super::{HudOverlay, widgets::EmitContext};
use crate::console::ViewerConsole;
use sjk_protocol::GameState;
use sjk_ui::{
    Color, DrawCommand, DrawList, FontWeight, HudWidget, Rect, TextAlign, TextId, TextOverflow,
    Theme,
};
use std::fmt::Write as _;

impl HudOverlay {
    pub(super) fn update_votes(
        &mut self,
        game_state: &GameState,
        team: u8,
        server_time: i32,
        console: Option<&ViewerConsole>,
    ) {
        let global = sjk_client::legacy_global_vote(game_state, server_time);
        let team_vote = sjk_client::legacy_team_vote(game_state, team, server_time);
        self.vote_active = global.active;
        self.team_vote_active = team_vote.is_some_and(|vote| vote.active);
        self.vote_heading.clear();
        self.vote_text.clear();
        self.team_vote_heading.clear();
        self.team_vote_text.clear();
        self.vote_keys.clear();
        if global.active {
            let _ = write!(
                self.vote_heading,
                "VOTE  /  {}s    YES {}  /  NO {}",
                global.remaining_seconds(),
                global.yes,
                global.no
            );
            let _ = write!(self.vote_text, "{}", global.text);
        }
        if let Some(vote) = team_vote.filter(|vote| vote.active) {
            let _ = write!(
                self.team_vote_heading,
                "TEAM VOTE  /  {}s    YES {}  /  NO {}",
                vote.remaining_seconds(),
                vote.yes,
                vote.no
            );
            let _ = write!(self.team_vote_text, "{}", vote.text);
        }
        if let Some(console) = console {
            console.write_keys_for_command("vote yes", &mut self.yes_keys);
            console.write_keys_for_command("vote no", &mut self.no_keys);
        } else {
            self.yes_keys.clear();
            self.yes_keys.push_str("F1");
            self.no_keys.clear();
            self.no_keys.push_str("F2");
        }
        let _ = write!(
            self.vote_keys,
            "{}  YES      {}  NO",
            self.yes_keys, self.no_keys
        );
    }
}

pub(super) fn emit(
    draw: &mut DrawList,
    theme: Theme,
    _widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
) {
    let cards = usize::from(context.data.vote_active) + usize::from(context.data.team_vote_active);
    if cards == 0 {
        return;
    }
    let scale = context.hero_scale;
    let ratio = scale / context.dpi_scale;
    let rect = Rect::new(
        rect.x + rect.width * (1.0 - ratio) * 0.5,
        rect.y * ratio,
        rect.width * ratio,
        rect.height * ratio,
    );
    let height = 92.0 * scale;
    let mut y = rect.y;
    if context.data.vote_active {
        emit_card(
            draw,
            theme,
            Rect::new(rect.x, y, rect.width, height),
            false,
            scale,
        );
        y += height + 12.0 * scale;
    }
    if context.data.team_vote_active {
        emit_card(
            draw,
            theme,
            Rect::new(rect.x, y, rect.width, height),
            true,
            scale,
        );
    }
}

fn emit_card(draw: &mut DrawList, theme: Theme, rect: Rect, team: bool, scale: f32) {
    let layout = crate::menu_widgets::VoteLayout::new(rect, scale);
    let _ = draw.push(DrawCommand::SolidRect {
        rect: layout.accent,
        color: theme.accent,
    });
    emit_text(
        draw,
        layout.heading,
        if team { TextId(203) } else { TextId(200) },
        14.0 * scale,
        theme.accent,
        FontWeight::Semibold,
    );
    emit_text(
        draw,
        layout.body,
        if team { TextId(204) } else { TextId(201) },
        22.0 * scale,
        theme.foreground,
        FontWeight::Semibold,
    );
    emit_text(
        draw,
        layout.hint,
        TextId(202),
        12.0 * scale,
        theme.muted,
        FontWeight::Regular,
    );
}

fn emit_text(
    draw: &mut DrawList,
    rect: Rect,
    text: TextId,
    size: f32,
    color: Color,
    weight: FontWeight,
) {
    let _ = draw.push(DrawCommand::Text {
        rect,
        text,
        size,
        color,
        align: TextAlign::Start,
        overflow: TextOverflow::Ellipsis,
        weight,
        letter_spacing: 0.5,
    });
}

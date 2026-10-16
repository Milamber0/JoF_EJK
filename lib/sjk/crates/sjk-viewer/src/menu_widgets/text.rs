//! Text storage and vector-font submission for [`MenuCanvas`].

use super::MenuCanvas;
use crate::game_font::{GameFonts, RetailFont, SjkFonts};
use crate::text::{TextStyle, TextVertex, UiFont};
use crate::ui_renderer;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextId, TextOverflow};
use std::fmt::{Arguments, Write as _};

/// The family a text run is drawn in on screens of the SJK UI: its display
/// type (Rajdhani) for navigation and titles, its body type (Exo 2) for the
/// rest, and its code type (JetBrains Mono, the console's font) for commands
/// and cvars in chips (`inline_code`). Other screens draw every run in one font
/// whatever its family; code falls back to the body type where its font is not
/// loaded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TextFamily {
    #[default]
    Body,
    Display,
    Code,
}

impl MenuCanvas {
    /// Store the text runs added from now on as `family` (until the next call or
    /// the next frame, which starts on [`TextFamily::Body`]).
    pub(crate) fn set_family(&mut self, family: TextFamily) {
        self.family = family;
    }

    /// Append retained text in the SJK UI's families: display runs to
    /// `fonts.display`, body runs to `fonts.body`, code runs to `fonts.code`
    /// (or the body's when it is not loaded), in the player's menu text
    /// `style`, colour codes lifted to read on the UI's navy
    /// ([`crate::text::CodePalette::Legible`]).
    pub(crate) fn append_text_families(
        &self,
        fonts: SjkFonts<'_>,
        viewport: [f32; 2],
        style: TextStyle,
    ) {
        let SjkFonts {
            display,
            body,
            code,
        } = fonts;
        let has_code = code.is_some();
        let family_of = |id: TextId| match self
            .families
            .get(id.0 as usize)
            .copied()
            .unwrap_or_default()
        {
            TextFamily::Code if !has_code => TextFamily::Body,
            family => family,
        };
        let targets = [
            Some((display, TextFamily::Display)),
            Some((body, TextFamily::Body)),
            code.map(|code| (code, TextFamily::Code)),
        ];
        for ((vertices, font), family) in targets.into_iter().flatten() {
            // Under a card: the page's runs first, less what the card covers, then the
            // card's.
            let Some((rect, first)) = self.cover else {
                ui_renderer::append_text_commands_where(
                    &self.draw,
                    |id| self.resolve(id),
                    |id, _| family_of(id) == family,
                    vertices,
                    font,
                    viewport,
                    style,
                    crate::text::CodePalette::Legible,
                );
                continue;
            };
            let start = vertices.len();
            ui_renderer::append_text_commands_where(
                &self.draw,
                |id| self.resolve(id),
                |id, _| id.0 < first && family_of(id) == family,
                vertices,
                font,
                viewport,
                style,
                crate::text::CodePalette::Legible,
            );
            crate::text::drop_quads_under(vertices, start, rect, viewport);
            ui_renderer::append_text_commands_where(
                &self.draw,
                |id| self.resolve(id),
                |id, _| id.0 >= first && family_of(id) == family,
                vertices,
                font,
                viewport,
                style,
                crate::text::CodePalette::Legible,
            );
        }
    }
    /// Add a non-interactive text run.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text(
        &mut self,
        value: &str,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        letter_spacing: f32,
    ) {
        self.text_aligned(
            value,
            rect,
            size,
            color,
            weight,
            letter_spacing,
            TextAlign::Start,
        );
    }

    /// Add aligned, non-interactive text without allocating on the frame path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_aligned(
        &mut self,
        value: &str,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        letter_spacing: f32,
        align: TextAlign,
    ) {
        let Some(id) = self.store_text(value) else {
            return;
        };
        let _ = self.draw.push(DrawCommand::Text {
            rect,
            text: id,
            size,
            color,
            align,
            overflow: TextOverflow::Ellipsis,
            weight,
            letter_spacing,
        });
    }

    /// Format directly into retained scratch storage and append aligned text.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_fmt_aligned(
        &mut self,
        value: Arguments<'_>,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        letter_spacing: f32,
        align: TextAlign,
    ) {
        let Some(id) = self.store_format(value) else {
            return;
        };
        let _ = self.draw.push(DrawCommand::Text {
            rect,
            text: id,
            size,
            color,
            align,
            overflow: TextOverflow::Ellipsis,
            weight,
            letter_spacing,
        });
    }

    /// Append retained text commands to the existing cached vector-font path,
    /// in the player's menu text style ([`UiFont::style`]).
    pub(crate) fn append_text(
        &self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        self.append_text_styled(vertices, font, viewport, font.style());
    }

    /// Append retained text commands in an explicit style, for surfaces that
    /// are not menus (chat, scoreboard) or size their text themselves (console).
    pub(crate) fn append_text_styled(
        &self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        style: TextStyle,
    ) {
        let Some((rect, first)) = self.cover else {
            ui_renderer::append_text_commands(
                &self.draw,
                |id| self.resolve(id),
                vertices,
                font,
                viewport,
                style,
            );
            return;
        };
        // Under a card: the page's runs, less what the card covers, then the card's.
        for card in [false, true] {
            let start = vertices.len();
            ui_renderer::append_text_commands_where(
                &self.draw,
                |id| self.resolve(id),
                |id, _| (id.0 >= first) == card,
                vertices,
                font,
                viewport,
                style,
                crate::text::CodePalette::Game,
            );
            if !card {
                crate::text::drop_quads_under(vertices, start, rect, viewport);
            }
        }
    }

    /// Append retained text, each command in the game font `font_of` names for
    /// it when that font is on (see [`GameFonts::append_routed`]), the rest to
    /// `vertices` with `font`. Routed surfaces (chat, scoreboard) are not menus,
    /// so they draw as laid out ([`TextStyle::NEUTRAL`]).
    pub(crate) fn append_text_routed(
        &self,
        fonts: &mut GameFonts,
        font_of: impl Fn(TextId, &str) -> Option<RetailFont> + Copy,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        fonts.append_routed(
            &self.draw,
            |id| self.resolve(id),
            font_of,
            (vertices, font),
            viewport,
            TextStyle::NEUTRAL,
        );
    }

    /// This frame's text runs in drawing order, for a screen's tests.
    #[cfg(test)]
    pub(crate) fn text_runs(&self) -> impl Iterator<Item = &str> + '_ {
        self.draw
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(self.resolve(*text)),
                _ => None,
            })
    }

    /// Id the next stored text run will get, to mark where a group of runs
    /// starts and ends.
    pub(crate) fn next_text_id(&self) -> u32 {
        self.text_len as u32
    }

    fn store_text(&mut self, value: &str) -> Option<TextId> {
        let Some(slot) = self.text.get_mut(self.text_len) else {
            self.dropped += 1;
            return None;
        };
        slot.clear();
        slot.push_str(value);
        Some(self.stored())
    }

    fn store_format(&mut self, value: Arguments<'_>) -> Option<TextId> {
        let Some(slot) = self.text.get_mut(self.text_len) else {
            self.dropped += 1;
            return None;
        };
        slot.clear();
        let _ = slot.write_fmt(value);
        Some(self.stored())
    }

    /// Close the run just written into slot `text_len`: record its family and
    /// return its id.
    fn stored(&mut self) -> TextId {
        if let Some(family) = self.families.get_mut(self.text_len) {
            *family = self.family;
        }
        let id = TextId(self.text_len as u32);
        self.text_len += 1;
        id
    }

    /// The text stored under `id`: for tests that check what was drawn, and for a
    /// screen measuring what a card drawn later would cover.
    pub(crate) fn stored_text(&self, id: TextId) -> &str {
        self.resolve(id)
    }

    fn resolve(&self, id: TextId) -> &str {
        self.text
            .get(id.0 as usize)
            .map(String::as_str)
            .unwrap_or("")
    }
}

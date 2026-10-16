//! Console overlay ordering, including the full-frame command browser.
use super::*;

/// Top of the upper right's readouts, in window pixels.
const FPS_TOP: f32 = 18.0;
/// The frame rate's and the match clock's line box at 1080 lines, before the frame's
/// text scale: JoF EJK's big console characters (`CG_DrawBigString`).
const FPS_LINE: f32 = 31.0;
/// The frame times' line under the rate, smaller.
const DETAILS_LINE: f32 = 18.0;
/// The readouts' right margin at 1080 lines (JoF EJK's 5 of 480).
const RIGHT_MARGIN: f32 = 11.0;

/// Bottom of the upper right's readouts, for a window `viewport_height` pixels tall,
/// with the frame's text scale (`main.rs`: the height scale, at least 0.85): the
/// frame rate and its frame times (`fps`) and the match clock (`timer`), as JoF EJK's
/// `CG_DrawUpperRight` stacks them. The HUD's kill feed stands under it.
pub(crate) fn upper_right_bottom(viewport_height: f32, fps: bool, timer: bool) -> f32 {
    let scale = ui_scale::height_scale(viewport_height).max(0.85);
    FPS_TOP
        + scale
            * (if fps { FPS_LINE + DETAILS_LINE } else { 0.0 } + if timer { FPS_LINE } else { 0.0 })
}

/// Where `line` set at `scale` starts to end at `right`. `append_text` breaks a line
/// whose next glyph's whole em would pass the window's edge, so the last glyph keeps
/// that em inside it, however close `right` is to the edge.
fn right_aligned(font: &UiFont, line: &str, scale: f32, right: f32) -> f32 {
    let width = crate::text::visible_text_width(font, line, scale);
    let last = line.chars().last().map_or(0.0, |last| {
        let mut one = [0_u8; 4];
        crate::text::visible_text_width(font, last.encode_utf8(&mut one), scale)
    });
    let em = font.height * scale;
    (right.min(right + last - em - 1.0) - width).max(8.0)
}

/// The upper right's lines, top to bottom: their text and line box at 1080 lines.
fn upper_right_lines<'a>(
    fps: Option<(u32, &'a str)>,
    timer: Option<&'a str>,
    rate: &'a mut String,
) -> impl Iterator<Item = (&'a str, f32)> {
    use std::fmt::Write as _;
    rate.clear();
    if let Some((fps, _)) = fps {
        let _ = write!(rate, "{fps}fps");
    }
    let rate: &'a str = rate;
    fps.into_iter()
        .flat_map(move |(_, details)| [(rate, FPS_LINE), (details, DETAILS_LINE)])
        .chain(timer.map(|timer| (timer, FPS_LINE)))
}

impl GpuState {
    pub(super) fn console_covers_frame(&self) -> bool {
        self.console
            .as_ref()
            .is_some_and(|console| console.covers_frame())
    }

    pub(super) fn append_console_overlay(&mut self, viewport: [f32; 2], text_scale: f32) {
        let covers_frame = self.console_covers_frame();
        if covers_frame {
            // Every font batch must be cleared: text is drawn above all UI shapes.
            self.text_vertices.clear();
            self.classic_text_vertices.clear();
            self.game_fonts.clear_text();
        }
        self.append_classic_console(viewport, covers_frame);
        // SJK's pages in the SJK UI draw in its families once they are loaded.
        if covers_frame
            && let Some(console) = self
                .console
                .as_mut()
                .filter(|console| console.sjk_page_open())
        {
            let target = match self.game_fonts.sjk() {
                Some(fonts) => menu::sjk::TextTarget::Families(fonts, self.ui_font.style()),
                None => menu::sjk::TextTarget::Inter(&mut self.text_vertices, &self.ui_font),
            };
            console.append_sjk_page(target, viewport);
            return;
        }
        // The console and its notify lines draw with the retail console character
        // set when `ui_gameFont` has it; the full-frame browser keeps Inter, or
        // the menus' retail font when it is the classic+ one.
        let classic_browser = self
            .console
            .as_ref()
            .is_some_and(crate::console::ViewerConsole::classic_browser_open);
        let (vertices, font) = if covers_frame && classic_browser {
            self.game_fonts.menu(&mut self.text_vertices, &self.ui_font)
        } else if covers_frame {
            (&mut self.text_vertices, &self.ui_font)
        } else {
            self.game_fonts.target(
                game_font::RetailFont::Console,
                &mut self.text_vertices,
                &self.ui_font,
            )
        };
        if let Some(console) = &mut self.console {
            console.append_overlay(vertices, font, viewport, text_scale);
        }
        if !covers_frame {
            // JoF EJK's upper right (`CG_DrawUpperRight`): the frame rate ("125fps", big
            // console characters, `CG_DrawFPS`), here with its frame times smaller under
            // it (Sol's request, 11/10/2026: the long line, tidier), then the match clock
            // (`CG_DrawTimer`), each right-aligned 5 of 480 from the edge.
            let fps = hud::family::fps(self.console.as_ref())
                .then(|| {
                    self.frame_pacer
                        .fps()
                        .map(|fps| (fps, self.frame_pacer.details()))
                })
                .flatten();
            let timer = self.hud.upper_right_timer();
            let right = viewport[0] - RIGHT_MARGIN * text_scale;
            let mut y = FPS_TOP;
            let mut rate = std::mem::take(&mut self.upper_right_rate);
            for (line, height) in upper_right_lines(fps, timer, &mut rate) {
                let scale = ui_scale::glyph_scale(font, height, text_scale);
                let x = right_aligned(font, line, scale, right);
                append_text(vertices, font, line, [x, y], scale, viewport);
                y += height * text_scale;
            }
            self.upper_right_rate = rate;
        }
    }

    /// Lay out and upload the classic console's own layer
    /// ([`console_backdrop`]), drawn after all other 2D text; empty under the
    /// full-frame browser.
    fn append_classic_console(&mut self, viewport: [f32; 2], covers_frame: bool) {
        let Some(console) = self.console.as_mut().filter(|_| !covers_frame) else {
            self.console_layer.clear();
            return;
        };
        if self.console_layer.needs_loading()
            && let Some(vfs) = &self.vfs
        {
            self.console_layer
                .ensure_loaded(&self.device, &self.queue, vfs, &self.shaders);
        }
        let (font, atlas) = match self.game_fonts.console_font() {
            Some(font) => (font, console_backdrop::TextAtlas::Console),
            None => (&self.ui_font, console_backdrop::TextAtlas::Inter),
        };
        // The SJK UI's console labels in its families once they are loaded.
        let labels = match self.game_fonts.sjk_metrics() {
            Some((display, body)) => console::classic::sjk::Labels {
                display,
                body,
                families: true,
            },
            None => console::classic::sjk::Labels::single(font),
        };
        let in_game =
            self.live_session.is_some() || self.demo_session.is_some() || self.resident.exploring();
        let menu_visible = self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible());
        let env = console::classic::ClassicEnv {
            in_game,
            menu_focus: menu_visible || self.game_menu,
            // Only the game client, whose menu is merely closed, not a map viewer
            // without one.
            full_screen: !in_game && !self.game_menu && self.client_menu.is_some() && !menu_visible,
        };
        let frame = self.console_layer.begin_frame();
        console.append_classic(frame, font, atlas, labels, viewport, env);
        self.console_layer.upload(&self.queue, viewport);
    }
}

#[cfg(test)]
mod upper_right_tests {
    use super::*;

    /// JoF EJK's order (`CG_DrawUpperRight`): the rate, here its frame times under it,
    /// then the match clock; each part only when it shows, and the kill feed's room
    /// grows with them.
    #[test]
    fn the_rate_its_frame_times_and_the_clock_stack_in_order() {
        let mut rate = String::new();
        let lines: Vec<_> = upper_right_lines(
            Some((125, "host 2.44 ms · p99 3.83")),
            Some("0:04.419"),
            &mut rate,
        )
        .map(|(text, line)| (text.to_owned(), line))
        .collect();
        assert_eq!(
            lines,
            [
                ("125fps".to_owned(), FPS_LINE),
                ("host 2.44 ms · p99 3.83".to_owned(), DETAILS_LINE),
                ("0:04.419".to_owned(), FPS_LINE),
            ]
        );
        let mut rate = String::new();
        let clock: Vec<_> = upper_right_lines(None, Some("12:00"), &mut rate).collect();
        assert_eq!(clock, [("12:00", FPS_LINE)]);
        assert_eq!(upper_right_bottom(1080.0, false, false), FPS_TOP);
        assert_eq!(upper_right_bottom(1080.0, false, true), FPS_TOP + FPS_LINE);
        assert_eq!(
            upper_right_bottom(1080.0, true, true),
            FPS_TOP + FPS_LINE * 2.0 + DETAILS_LINE
        );
    }
}

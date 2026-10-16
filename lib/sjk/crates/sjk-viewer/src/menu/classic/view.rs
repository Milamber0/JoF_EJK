//! Drawing of the classic main menu: the retail page composition (logo,
//! page title, gold entries with a glow behind the focused one, the
//! description line underneath) on the 640x480 canvas fitted into the
//! window.
//!
//! With the player's retail artwork loaded ([`crate::menu::art`]) the pages
//! are built from it in the retail item order: backdrop, side glyph
//! columns, the main page's ring and windows or the sub-pages' frames, the
//! logo and the button glow, opaque: no world is drawn behind the classic
//! pages, so the art's transparent centre and the pillarbox of a wide window
//! stay dark. Where retail played its logo video in that centre, the main
//! page shows SJK's emblem ([`emblem`]), over the frames, with or without
//! the art. Without the art, the same layout is drawn with SJK's own vector
//! shapes and text.
//!
//! The artwork moves as retail's shaders move it ([`motion`]): the ring
//! turns, the side glyphs climb over their backdrop, a reflection drifts
//! through the logo, the glows flicker (the renderer recomposes those), and
//! the focused entry's text pulses. The emblem's core glows and its blade
//! lights shimmer. Labels and buttons are in retail's capitals ([`Caps`]).

use super::ClassicMain;
use super::layout::{CANVAS, HINT_Y, LOGO, Page, Placement, Slot};
use crate::menu::art::{ArtPiece, ArtSet, motion};
use crate::menu::emblem;
use crate::menu_widgets::MenuCanvas;
use sjk_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign};

/// Retail entry colour (`forecolor 1 .682 0`).
pub(crate) const GOLD: Color = Color::new(1.0, 0.682, 0.0, 1.0);
/// Retail focus colour (`focusColor 1 1 1 1`).
pub(crate) const FOCUS: Color = Color::new(1.0, 1.0, 1.0, 1.0);
/// Retail disabled colour (`disableColor .5 .5 .5 1`).
pub(crate) const DISABLED: Color = Color::new(0.5, 0.5, 0.5, 1.0);
/// Retail page-title colour (`forecolor .695 .760 .861`).
pub(crate) const TITLE: Color = Color::new(0.695, 0.760, 0.861, 1.0);
/// Retail description colour (`descColor 1 .682 0 .8`).
pub(crate) const HINT: Color = Color::new(1.0, 0.682, 0.0, 0.8);
const INK: [f32; 3] = [0.004, 0.008, 0.020];
const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);

/// Retail `main.menu` artwork in draw order, with its canvas rectangles.
/// Retail's `background_video` (the `ja01` logo) came first, under
/// everything; SJK draws its emblem after these instead.
const MAIN_ART: [(ArtPiece, [f32; 4]); 7] = [
    (ArtPiece::SideLeft, [0.0, 0.0, 160.0, 480.0]),
    (ArtPiece::SideRight, [480.0, 0.0, 160.0, 480.0]),
    (ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]),
    (ArtPiece::Ring, [193.0, 145.0, 256.0, 256.0]),
    (ArtPiece::CenterWindow, [156.0, 154.0, 320.0, 240.0]),
    (ArtPiece::LeftWindow, [0.0, 150.0, 320.0, 240.0]),
    (ArtPiece::RightWindow, [320.0, 150.0, 320.0, 240.0]),
];

/// Artwork shared by the retail sub-pages (`multiplayer.menu`,
/// `controls.menu`, `setup.menu`, `quit.menu`), in draw order.
const SUB_PAGE_ART: [(ArtPiece, [f32; 4]); 6] = [
    (ArtPiece::CenterBlue, [156.0, 154.0, 320.0, 240.0]),
    (ArtPiece::SideLeft, [0.0, 0.0, 160.0, 480.0]),
    (ArtPiece::SideRight, [480.0, 0.0, 160.0, 480.0]),
    (ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]),
    (ArtPiece::BoxesLeft, [0.0, 50.0, 320.0, 160.0]),
    (ArtPiece::BoxesRight, [320.0, 50.0, 320.0, 160.0]),
];

pub(crate) fn ink(alpha: f32) -> Color {
    Color::new(INK[0], INK[1], INK[2], alpha)
}

pub(crate) fn gold(alpha: f32) -> Color {
    Color::new(GOLD.r, GOLD.g, GOLD.b, alpha)
}

/// Draw `piece` stretched over window rectangle `rect`.
pub(crate) fn art(canvas: &mut MenuCanvas, piece: ArtPiece, rect: Rect) {
    let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
        rect,
        texture: piece.texture(),
        color: WHITE,
    });
}

/// Draw `piece` over `rect` with texture coordinates `uv` at its corners
/// (top-left, top-right, bottom-right, bottom-left), tinted `color`.
fn art_uv(canvas: &mut MenuCanvas, piece: ArtPiece, rect: Rect, uv: [[f32; 2]; 4], color: Color) {
    let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuadUv {
        rect,
        texture: piece.texture(),
        color,
        uv,
    });
}

/// Draw one backdrop piece as retail's shader animates it: the side glyph
/// columns climb over `menu_side_text_b`, the ring turns, the rest stand
/// still.
fn moving_art(canvas: &mut MenuCanvas, art_set: ArtSet, piece: ArtPiece, rect: Rect) {
    let now = motion::seconds();
    match piece {
        ArtPiece::SideLeft | ArtPiece::SideRight => {
            if art_set.has(ArtPiece::SideBase) {
                art(canvas, ArtPiece::SideBase, rect);
            }
            art_uv(
                canvas,
                piece,
                rect,
                motion::scroll(motion::SIDE_SCROLL, now),
                WHITE,
            );
        }
        ArtPiece::Ring => art_uv(
            canvas,
            piece,
            rect,
            motion::rotation(motion::RING_DEGREES_PER_SECOND, now),
            WHITE,
        ),
        _ => art(canvas, piece, rect),
    }
}

/// The focus colour as retail paints the focused item: pulsing
/// ([`motion::pulse`]).
pub(crate) fn focus_pulse() -> Color {
    motion::pulse(FOCUS, motion::seconds())
}

/// Text shown in capitals, as retail's menus set their labels: ASCII and
/// Latin-1 letters are raised one to one (colour codes are untouched), so
/// it formats into the canvas without allocating.
pub(crate) struct Caps<'a>(pub(crate) &'a str);

impl std::fmt::Display for Caps<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        for character in self.0.chars() {
            formatter.write_char(capital(character))?;
        }
        Ok(())
    }
}

/// Text in sentence case, as classic+ sets values: its first letter raised
/// ([`Caps`]'s rule), the rest as written (`standard` is `Standard`).
pub(crate) struct Sentence<'a>(pub(crate) &'a str);

impl std::fmt::Display for Sentence<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        let mut characters = self.0.chars();
        if let Some(first) = characters.next() {
            formatter.write_char(capital(first))?;
        }
        formatter.write_str(characters.as_str())
    }
}

/// `character` in capitals where Latin-1 has a one-to-one capital; `ß` and
/// `ÿ`, whose capitals lie outside it, stay as they are.
fn capital(character: char) -> char {
    match character {
        'a'..='z' => character.to_ascii_uppercase(),
        '\u{e0}'..='\u{fe}' if character != '\u{f7}' => {
            char::from_u32(character as u32 - 0x20).unwrap_or(character)
        }
        _ => character,
    }
}

/// Build the classic main menu into `canvas` at `reveal` opacity, drawing
/// the retail artwork in `art_set` where it is loaded.
pub(crate) fn build(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    menu: &ClassicMain,
    reveal: f32,
    art_set: ArtSet,
) {
    let place = Placement::new(viewport);
    let s = place.scale;
    let page = menu.page();
    canvas.begin_transparent(viewport);
    canvas.push_opacity(reveal);
    page_backdrop(canvas, viewport, &place, page, art_set);
    title(canvas, &place, page, art_set);
    if page == Page::Quit {
        canvas.text_aligned(
            "Quit to the desktop?",
            place.centered([CANVAS[0] * 0.5, 260.0], 450.0, 24.0),
            18.0 * s,
            FOCUS,
            FontWeight::Regular,
            0.4 * s,
            TextAlign::Center,
        );
    }
    let mut described = menu.selection();
    for (index, slot) in menu.slots().iter().enumerate() {
        let token = index as u16;
        let target = place.rect(slot.target());
        let hovered = canvas.token_hovered(token);
        if hovered {
            described = index;
        }
        let active = index == menu.selection() || hovered;
        if active && slot.enabled() {
            glow(canvas, target, s, art_set);
        }
        entry_label(canvas, &place, slot, active);
        canvas.hit_region(token, target);
    }
    if let Some(slot) = menu.slots().get(described) {
        canvas.text_aligned(
            slot.hint,
            place.centered([CANVAS[0] * 0.5, HINT_Y], 560.0, 18.0),
            13.0 * s,
            if slot.enabled() { HINT } else { DISABLED },
            FontWeight::Regular,
            0.3 * s,
            TextAlign::Center,
        );
    }
    let muted = canvas.theme().muted;
    canvas.text_aligned(
        &super::super::main_view::version_line(),
        Rect::new(
            viewport[0] - 536.0 * s,
            viewport[1] - 26.0 * s,
            520.0 * s,
            16.0 * s,
        ),
        6.0 * s,
        muted,
        FontWeight::Regular,
        0.5 * s,
        TextAlign::End,
    );
    canvas.pop_opacity();
    canvas.finish(menu.selection() as u16);
}

/// The page's backdrop and logo: the retail artwork where it is loaded,
/// SJK's vector version otherwise.
pub(crate) fn page_backdrop(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    place: &Placement,
    page: Page,
    art_set: ArtSet,
) {
    if art_set.has(ArtPiece::Background) {
        backdrop_art(canvas, viewport, place, page, art_set);
    } else {
        backdrop(canvas, viewport, place);
    }
    if page == Page::Main {
        emblem::draw(canvas, place.rect(emblem::CLASSIC_RING), motion::seconds());
    }
    logo(canvas, place, art_set);
}

/// One entry's label: gold, white and pulsing while focused, grey when SJK
/// cannot open it yet.
pub(crate) fn entry_label(canvas: &mut MenuCanvas, place: &Placement, slot: &Slot, active: bool) {
    let color = match (slot.enabled(), active) {
        (false, _) => DISABLED,
        (true, true) => focus_pulse(),
        (true, false) => GOLD,
    };
    entry_label_colored(canvas, place, slot, color);
}

/// One entry's label in `color`, such as the open group's steady white.
pub(crate) fn entry_label_colored(
    canvas: &mut MenuCanvas,
    place: &Placement,
    slot: &Slot,
    color: Color,
) {
    let s = place.scale;
    let size = slot.size.text();
    let [x, _, width, _] = slot.target();
    let line = size * 1.2;
    let top = slot.center[1] - line * 0.5;
    let rect = match slot.align {
        // Retail list labels end 10 units inside their item.
        TextAlign::End => place.rect([x, top, width - 10.0, line]),
        _ => place.centered(slot.center, width + 40.0, line),
    };
    canvas.text_fmt_aligned(
        format_args!("{}", Caps(slot.label)),
        rect,
        size * s,
        color,
        FontWeight::Semibold,
        1.2 * s,
        slot.align,
    );
}

/// Page heading: the main page's "MULTIPLAYER" under the logo, the
/// sub-pages' title over a glow band (retail `title_glow`).
fn title(canvas: &mut MenuCanvas, place: &Placement, page: Page, art_set: ArtSet) {
    let s = place.scale;
    let (text, y) = page.title();
    if page != Page::Main {
        let band = place.rect([150.0, y - 10.0, 340.0, 20.0]);
        if art_set.has(ArtPiece::ButtonBack) {
            art(canvas, ArtPiece::ButtonBack, band);
        } else {
            soft_band(canvas, band, 0.16);
        }
    }
    canvas.text_aligned(
        text,
        place.centered([CANVAS[0] * 0.5, y], 400.0, 20.0),
        16.0 * s,
        TITLE,
        FontWeight::Semibold,
        3.0 * s,
        TextAlign::Center,
    );
}

/// The retail backdrop from the player's artwork. The pillarbox of a wide
/// window is darkened to match the art's edges.
fn backdrop_art(
    canvas: &mut MenuCanvas,
    viewport: [f32; 2],
    place: &Placement,
    page: Page,
    art_set: ArtSet,
) {
    let [width, height] = viewport;
    let page_rect = place.rect([0.0, 0.0, CANVAS[0], CANVAS[1]]);
    {
        let draw = canvas.draw_list_mut();
        // Opaque, as retail's: the world is not drawn behind the classic
        // pages (retail played its logo video in the centre gap, SJK shows
        // its emblem there).
        let _ = draw.push(DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, width, height),
            color: ink(1.0),
        });
        for side in [
            Rect::new(0.0, 0.0, page_rect.x, height),
            Rect::new(page_rect.right(), 0.0, width - page_rect.right(), height),
        ] {
            let _ = draw.push(DrawCommand::SolidRect {
                rect: side,
                color: ink(1.0),
            });
        }
    }
    let pieces: &[(ArtPiece, [f32; 4])] = if page == Page::Main {
        &MAIN_ART
    } else {
        &SUB_PAGE_ART
    };
    for (piece, rect) in pieces {
        if art_set.has(*piece) {
            moving_art(canvas, art_set, *piece, place.rect(*rect));
        }
    }
}

/// The retail background alone, opaque, under a screen a classic page
/// opened while the world is not drawn: the sub-pages' backdrop and
/// glyph columns, without their frames.
pub(crate) fn opaque_backdrop(canvas: &mut MenuCanvas, viewport: [f32; 2], art_set: ArtSet) {
    let place = Placement::new(viewport);
    canvas.begin_transparent(viewport);
    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
        rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
        color: ink(1.0),
    });
    for (piece, rect) in [
        (ArtPiece::SideLeft, [0.0, 0.0, 160.0, 480.0]),
        (ArtPiece::SideRight, [480.0, 0.0, 160.0, 480.0]),
        (ArtPiece::Background, [0.0, 0.0, 640.0, 480.0]),
    ] {
        if art_set.has(piece) {
            moving_art(canvas, art_set, piece, place.rect(rect));
        }
    }
    canvas.finish(0);
}

/// Dim the live map so the page reads as one surface (retail drew an
/// opaque background here), darker at the top and bottom bands.
fn backdrop(canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement) {
    let [width, height] = viewport;
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(0.0, 0.0, width, height),
        color: ink(0.58),
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(0.0, 0.0, width, height * 0.30),
        radius: 0.0,
        gradient: Gradient {
            start: ink(0.55),
            end: ink(0.0),
            vertical: true,
        },
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(0.0, height * 0.78, width, height * 0.22),
        radius: 0.0,
        gradient: Gradient {
            start: ink(0.0),
            end: ink(0.60),
            vertical: true,
        },
    });
    // Hairlines framing the button area, where retail drew its window art.
    for y in [180.0, 360.0] {
        let _ = draw.push(DrawCommand::GradientRect {
            rect: place.rect([16.0, y, 304.0, 1.0]),
            radius: 0.0,
            gradient: Gradient {
                start: gold(0.0),
                end: gold(0.22),
                vertical: false,
            },
        });
        let _ = draw.push(DrawCommand::GradientRect {
            rect: place.rect([320.0, y, 304.0, 1.0]),
            radius: 0.0,
            gradient: Gradient {
                start: gold(0.22),
                end: gold(0.0),
                vertical: false,
            },
        });
    }
}

/// The retail logo, or the game title where it sits. Retail's logo shader
/// draws the picture opaque, a quarter of the drifting `env_logo` over it,
/// then the picture again, so the reflection moves through its
/// translucent letters.
fn logo(canvas: &mut MenuCanvas, place: &Placement, art_set: ArtSet) {
    if art_set.has(ArtPiece::Logo) {
        let rect = place.rect(LOGO);
        if art_set.has(ArtPiece::LogoBase) && art_set.has(ArtPiece::EnvLogo) {
            art(canvas, ArtPiece::LogoBase, rect);
            art_uv(
                canvas,
                ArtPiece::EnvLogo,
                rect,
                motion::scroll(motion::LOGO_REFLECTION_SCROLL, motion::seconds()),
                Color::new(1.0, 1.0, 1.0, motion::LOGO_REFLECTION_ALPHA),
            );
        }
        art(canvas, ArtPiece::Logo, rect);
        return;
    }
    let s = place.scale;
    let [x, y, width, height] = LOGO;
    let center = x + width * 0.5;
    canvas.text_aligned(
        "JEDI KNIGHT",
        place.centered([center, y + height * 0.30], width, 16.0),
        13.0 * s,
        GOLD,
        FontWeight::Semibold,
        6.0 * s,
        TextAlign::Center,
    );
    canvas.text_aligned(
        "JEDI ACADEMY",
        place.centered([center, y + height * 0.62], width, 46.0),
        40.0 * s,
        FOCUS,
        FontWeight::Semibold,
        4.0 * s,
        TextAlign::Center,
    );
}

/// The retail `menu_buttonback` glow behind a focused entry, or a soft gold
/// band with an underline where that art is missing.
pub(crate) fn glow(canvas: &mut MenuCanvas, target: Rect, scale: f32, art_set: ArtSet) {
    if art_set.has(ArtPiece::ButtonBack) {
        art(canvas, ArtPiece::ButtonBack, target);
        return;
    }
    soft_band(canvas, target, 0.26);
    let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
        rect: Rect::new(
            target.x + target.width * 0.2,
            target.bottom() - 1.5 * scale,
            target.width * 0.6,
            1.5 * scale,
        ),
        color: gold(0.7),
    });
}

/// A gold band over `rect`, brightest (`peak` alpha) in the middle.
pub(crate) fn soft_band(canvas: &mut MenuCanvas, rect: Rect, peak: f32) {
    let half = Rect::new(rect.x, rect.y, rect.width * 0.5, rect.height);
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::GradientRect {
        rect: half,
        radius: 0.0,
        gradient: Gradient {
            start: gold(0.0),
            end: gold(peak),
            vertical: false,
        },
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(half.right(), rect.y, rect.width - half.width, rect.height),
        radius: 0.0,
        gradient: Gradient {
            start: gold(peak),
            end: gold(0.0),
            vertical: false,
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitals_raise_letters_one_to_one() {
        assert_eq!(Caps("Display mode").to_string(), "DISPLAY MODE");
        assert_eq!(Caps("^1red ^7é ß ÷ 3").to_string(), "^1RED ^7É ß ÷ 3");
        assert_eq!(Caps("already UPPER").to_string(), "ALREADY UPPER");
    }

    #[test]
    fn the_focused_label_pulses_and_others_hold() {
        let slot = &Page::Main.slots()[0];
        let mut canvas = MenuCanvas::new();
        let place = Placement::new([1280.0, 960.0]);
        canvas.begin_transparent([1280.0, 960.0]);
        entry_label(&mut canvas, &place, slot, true);
        entry_label(&mut canvas, &place, slot, false);
        let colors: Vec<Color> = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        assert_eq!(colors.len(), 2);
        assert!(colors[0].r >= 0.8 && colors[0].r <= 1.0);
        assert_eq!(colors[1], GOLD);
    }
}

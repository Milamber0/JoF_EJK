//! Camera control in the SJK UI's look (`docs/sjk-ui.md`, Camera control): a
//! column down the window's right edge over a dark fade, the rest of the scene
//! left clear for framing, with faint viewfinder marks over the whole picture
//! (its corners and the crossings of its thirds). The panel's state, tokens,
//! keys and pointer are the other look's ([`Panel`]); only the drawing differs.
//!
//! Positions are pixels of a 1080-line frame whose right edge is the window's
//! ([`frame`]), so the column hugs the edge on any window shape.

use super::*;
use crate::console::director::SunMode;
use crate::menu::sjk::{Frame, color, fade_across, key_hint, key_hint_width, kit, text};
use crate::menu_widgets::TextFamily;
use crate::menu_widgets::numeric::VALUE_BASE;
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// The column's left edge and width.
const COLUMN_X: f32 = 1440.0;
const COLUMN_WIDTH: f32 = 416.0;
/// Where the fade starts, clear, and where it is dark under the column.
const FADE_START: f32 = 1200.0;
const FADE_DARK: f32 = 1400.0;
/// The title's middle, the line under it, the tabs' top.
const TITLE_Y: f32 = 80.0;
const SUBTITLE_Y: f32 = 120.0;
const TABS_Y: f32 = 148.0;
/// A page's first sub-heading, and the shared part's.
const PAGE_Y: f32 = 226.0;
const SHARED_Y: f32 = 584.0;
/// The Sun page's reset button.
const RESET_SUN_Y: f32 = 420.0;
/// A slider's row, a button, a switch's row and the closing buttons.
const SLIDER_ROW: f32 = 56.0;
const BUTTON: f32 = 40.0;
const SWITCH_ROW: f32 = 44.0;
const CLOSE_BUTTON: f32 = 44.0;
/// Space between buttons on a line.
const GAP: f32 = 8.0;
/// The keys' first line; the second is under it.
const KEYS_Y: f32 = 964.0;
const KEYS_PITCH: f32 = 32.0;
/// A slider's number, at the right of its name's line.
const NUMBER_WIDTH: f32 = 140.0;

/// Each slider's unit after its number, set in Exo 2 (Rajdhani's degree sign
/// reads as an apostrophe), and about how wide it is at 18.
const UNITS: [(&str, f32); 8] = [
    ("°", 9.0),
    ("°", 9.0),
    ("", 0.0),
    ("", 0.0),
    ("°", 9.0),
    ("°", 9.0),
    (" s", 13.0),
    ("°/s", 24.0),
];

/// The SJK UI's 1080-line frame with its right edge on the window's.
fn frame(viewport: [f32; 2]) -> Frame {
    let s = crate::menu::sjk::scale(viewport);
    Frame {
        s,
        origin: [viewport[0] - 1920.0 * s, (viewport[1] - 1080.0 * s) * 0.5],
    }
}

/// Draw `panel` into `ui`.
pub(super) fn build(panel: &Panel, ui: &mut MenuCanvas, viewport: [f32; 2]) {
    let frame = frame(viewport);
    ui.begin_transparent(viewport);
    viewfinder(ui, viewport, frame.s);
    scrim(ui, &frame, viewport);
    header(ui, &frame, panel);
    if panel.sun_tab {
        sun(ui, &frame, panel);
    } else {
        camera(ui, &frame, panel);
    }
    shared(ui, &frame, panel);
    keys(ui, &frame, panel);
    ui.finish(panel.selected);
}

fn push(ui: &mut MenuCanvas, command: DrawCommand) {
    let _ = ui.draw_list_mut().push(command);
}

/// The picture's corners and the crossings of its thirds, as a camera's
/// viewfinder marks them: thin holo lines over the whole window, which is the
/// shot once the panel hides.
fn viewfinder(ui: &mut MenuCanvas, viewport: [f32; 2], s: f32) {
    let [width, height] = viewport;
    let line = (1.5 * s).max(1.0);
    let corner = color::alpha(color::HOLO, 0.55);
    let (inset, arm) = (28.0 * s, 40.0 * s);
    for (x, y, right, bottom) in [
        (inset, inset, false, false),
        (width - inset, inset, true, false),
        (inset, height - inset, false, true),
        (width - inset, height - inset, true, true),
    ] {
        let across = if right { x - arm } else { x };
        let down = if bottom { y - arm } else { y };
        let edge_x = if right { x - line } else { x };
        let edge_y = if bottom { y - line } else { y };
        push(
            ui,
            DrawCommand::SolidRect {
                rect: Rect::new(across, edge_y, arm, line),
                color: corner,
            },
        );
        push(
            ui,
            DrawCommand::SolidRect {
                rect: Rect::new(edge_x, down, line, arm),
                color: corner,
            },
        );
    }
    let mark = color::alpha(color::HOLO, 0.4);
    let reach = 9.0 * s;
    for x in [width / 3.0, width * 2.0 / 3.0] {
        for y in [height / 3.0, height * 2.0 / 3.0] {
            push(
                ui,
                DrawCommand::SolidRect {
                    rect: Rect::new(x - reach, y - line * 0.5, reach * 2.0, line),
                    color: mark,
                },
            );
            push(
                ui,
                DrawCommand::SolidRect {
                    rect: Rect::new(x - line * 0.5, y - reach, line, reach * 2.0),
                    color: mark,
                },
            );
        }
    }
}

/// The fade from clear to dark under the column, full height: the rest of the
/// scene stays as the shot will show it.
fn scrim(ui: &mut MenuCanvas, frame: &Frame, viewport: [f32; 2]) {
    let [width, height] = viewport;
    let x = |frame_x: f32| frame.point(frame_x, 0.0)[0];
    let space = |alpha| color::alpha(color::SPACE, alpha);
    let (start, dark) = (x(FADE_START).max(0.0), x(FADE_DARK));
    if dark > start {
        fade_across(
            ui,
            Rect::new(start, 0.0, dark - start, height),
            space(0.0),
            space(0.72),
        );
    }
    fade_across(
        ui,
        Rect::new(dark, 0.0, width - dark, height),
        space(0.72),
        space(0.86),
    );
}

/// The title, what the panel is for, and its pages as tabs.
fn header(ui: &mut MenuCanvas, frame: &Frame, panel: &Panel) {
    let s = frame.s;
    text(
        ui,
        TextFamily::Display,
        format_args!("{}", crate::ingame_menu::CAMERA_CONTROL),
        frame.rect(COLUMN_X, TITLE_Y - 26.0, COLUMN_WIDTH, 52.0),
        42.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    text(
        ui,
        TextFamily::Body,
        format_args!("Frame the scene, then hide the panel."),
        frame.rect(COLUMN_X, SUBTITLE_Y - 12.0, COLUMN_WIDTH, 24.0),
        17.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
    let mut x = COLUMN_X;
    for (token, label) in [(CAMERA, "Camera"), (SUN, "Sun")] {
        // Rajdhani at 26 is about 12 pixels a character.
        let width = 12.0 * label.len() as f32 + 4.0;
        let current = (token == SUN) == panel.sun_tab;
        let focused = panel.selected == token;
        if focused {
            kit::band(ui, frame, [x - 16.0, TABS_Y - 6.0, width + 32.0, 50.0]);
        }
        let lit = focused || ui.token_hovered(token);
        text(
            ui,
            TextFamily::Display,
            format_args!("{label}"),
            frame.rect(x, TABS_Y, width + 20.0, 34.0),
            26.0 * s,
            match (current, lit) {
                (true, _) => color::GOLD_BRIGHT,
                (false, true) => color::TEXT,
                (false, false) => color::MUTED,
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        if current {
            push(
                ui,
                DrawCommand::RoundedRect {
                    rect: frame.rect(x, TABS_Y + 38.0, width, 3.0),
                    radius: 1.5 * s,
                    color: color::GOLD_BRIGHT,
                },
            );
        }
        ui.hit_region(token, frame.rect(x - 8.0, TABS_Y - 4.0, width + 16.0, 46.0));
        x += width + 40.0;
    }
}

/// The Camera page: where the camera looks from, then the framing sliders.
fn camera(ui: &mut MenuCanvas, frame: &Frame, panel: &Panel) {
    let mut y = PAGE_Y;
    kit::heading(ui, frame, COLUMN_X, y, COLUMN_WIDTH, "View from");
    y += 16.0;
    let width = (COLUMN_WIDTH - GAP * 3.0) / 4.0;
    for (index, label) in ["Back", "Front", "Left", "Right"].into_iter().enumerate() {
        let token = PRESET + index as u16;
        kit::button(
            ui,
            frame,
            [COLUMN_X + index as f32 * (width + GAP), y, width, BUTTON],
            label,
            false,
            true,
            panel.selected == token,
            token,
        );
    }
    y += BUTTON + 32.0;
    kit::heading(ui, frame, COLUMN_X, y, COLUMN_WIDTH, "Framing");
    y += 16.0;
    for row in 0..4 {
        slider(ui, frame, panel, row, y);
        y += SLIDER_ROW;
    }
}

/// The Sun page: its direction and height, what owns it now, and the way back
/// to the day settings; or why it cannot be set here.
fn sun(ui: &mut MenuCanvas, frame: &Frame, panel: &Panel) {
    let mut y = PAGE_Y;
    kit::heading(ui, frame, COLUMN_X, y, COLUMN_WIDTH, "Sunlight");
    y += 16.0;
    let (first, second) = if panel.sun_available {
        for row in 4..6 {
            slider(ui, frame, panel, row, y);
            y += SLIDER_ROW;
        }
        let mode = match panel.sun_mode {
            SunMode::Automatic => ("Automatic: the day settings move it", color::MUTED),
            SunMode::Manual => ("Manual: Camera control holds it", color::TEXT),
            SunMode::Returning => ("Returning to the day settings...", color::MUTED),
        };
        (
            ("0° is the horizon, 90° straight overhead", color::MUTED),
            mode,
        )
    } else {
        (
            ("Sun controls are not available in this view.", color::TEXT),
            (
                "They need day and night (r_dayNight) and open sky.",
                color::MUTED,
            ),
        )
    };
    for (index, (line, colour)) in [first, second].into_iter().enumerate() {
        text(
            ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(COLUMN_X, y + 6.0 + index as f32 * 26.0, COLUMN_WIDTH, 24.0),
            16.0 * frame.s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }
    kit::button(
        ui,
        frame,
        [COLUMN_X, RESET_SUN_Y, COLUMN_WIDTH, BUTTON],
        "Reset sun, use the day settings",
        false,
        true,
        panel.selected == RESET_SUN,
        RESET_SUN,
    );
}

/// What both pages share: the motion's sliders and buttons, live preview and
/// the HUD, then hiding the panel.
fn shared(ui: &mut MenuCanvas, frame: &Frame, panel: &Panel) {
    let available = !panel.sun_tab || panel.sun_available;
    let mut y = SHARED_Y;
    kit::heading(ui, frame, COLUMN_X, y, COLUMN_WIDTH, "Motion");
    y += 16.0;
    slider(ui, frame, panel, 6, y);
    y += SLIDER_ROW;
    if available {
        slider(ui, frame, panel, 7, y);
    }
    y += SLIDER_ROW + 14.0;
    if available {
        let width = (COLUMN_WIDTH - GAP * 2.0) / 3.0;
        let reset = if panel.sun_tab { "Ease back" } else { "Reset" };
        for (index, (token, label)) in [(ORBIT, "Orbit"), (STOP, "Stop"), (RESET, reset)]
            .into_iter()
            .enumerate()
        {
            kit::button(
                ui,
                frame,
                [COLUMN_X + index as f32 * (width + GAP), y, width, BUTTON],
                label,
                false,
                true,
                panel.selected == token,
                token,
            );
        }
    }
    y += BUTTON + 16.0;
    switch_row(ui, frame, panel, LIVE, "Live preview", panel.live, y);
    y += SWITCH_ROW;
    switch_row(ui, frame, panel, HUD, "HUD", panel.hud, y);
    y += SWITCH_ROW + 20.0;
    let primary = (COLUMN_WIDTH - GAP) * 0.6;
    kit::button(
        ui,
        frame,
        [COLUMN_X, y, primary, CLOSE_BUTTON],
        if panel.live {
            "Hide panel"
        } else {
            "Move and hide"
        },
        true,
        true,
        panel.selected == APPLY_HIDE,
        APPLY_HIDE,
    );
    kit::button(
        ui,
        frame,
        [
            COLUMN_X + primary + GAP,
            y,
            COLUMN_WIDTH - primary - GAP,
            CLOSE_BUTTON,
        ],
        "Close",
        false,
        true,
        panel.selected == HIDE,
        HIDE,
    );
}

/// A row's name: white while focused.
fn name(ui: &mut MenuCanvas, frame: &Frame, label: &str, rect: Rect, focused: bool) {
    text(
        ui,
        TextFamily::Body,
        format_args!("{label}"),
        rect,
        19.0 * frame.s,
        if focused {
            Color::new(1.0, 1.0, 1.0, 1.0)
        } else {
            color::alpha(color::TEXT, 0.88)
        },
        FontWeight::Regular,
        TextAlign::Start,
    );
}

/// Slider `row` on the row whose top is `top`: its name and number (or the
/// number being typed) on one line, its track under them across the column.
/// The track's pointer area spans it exactly, so a click lands where it shows.
fn slider(ui: &mut MenuCanvas, frame: &Frame, panel: &Panel, row: usize, top: f32) {
    let s = frame.s;
    let (label, min, max, _) = SLIDERS[row];
    let value_token = VALUE_BASE + row as u16;
    let focused = panel.selected == row as u16 || panel.selected == value_token;
    if focused {
        kit::band(
            ui,
            frame,
            [COLUMN_X - 16.0, top, COLUMN_WIDTH + 32.0, SLIDER_ROW],
        );
    }
    let line = top + 18.0;
    name(
        ui,
        frame,
        label,
        frame.rect(
            COLUMN_X,
            line - 14.0,
            COLUMN_WIDTH - NUMBER_WIDTH - 10.0,
            28.0,
        ),
        focused,
    );
    let number = frame.rect(
        COLUMN_X + COLUMN_WIDTH - NUMBER_WIDTH,
        line - 13.0,
        NUMBER_WIDTH,
        26.0,
    );
    ui.hit_region(value_token, number);
    match panel.numeric.as_ref().filter(|edit| edit.row == row) {
        Some(edit) => {
            ui.set_family(TextFamily::Display);
            edit.draw(ui, number, 1.4 * s);
            ui.set_family(TextFamily::Body);
        }
        None => {
            let ink = if focused {
                color::TEXT
            } else {
                color::alpha(color::TEXT, 0.9)
            };
            let (unit, unit_width) = UNITS[row];
            let right = COLUMN_X + COLUMN_WIDTH;
            text(
                ui,
                TextFamily::Display,
                format_args!("{}", Number(panel.values[row])),
                frame.rect(
                    right - NUMBER_WIDTH,
                    line - 13.0,
                    NUMBER_WIDTH - unit_width,
                    26.0,
                ),
                21.0 * s,
                ink,
                FontWeight::Regular,
                TextAlign::End,
            );
            if !unit.is_empty() {
                text(
                    ui,
                    TextFamily::Body,
                    format_args!("{unit}"),
                    frame.rect(right - unit_width, line - 13.0, unit_width + 4.0, 26.0),
                    18.0 * s,
                    ink,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
            }
        }
    }
    let track = top + 42.0;
    kit::slider(
        ui,
        frame,
        COLUMN_X,
        track,
        COLUMN_WIDTH,
        (panel.values[row] - min) / (max - min),
        focused,
    );
    ui.hit_region(
        row as u16,
        frame.rect(COLUMN_X, track - 14.0, COLUMN_WIDTH, 28.0),
    );
}

/// A switch's row whose top is `top`, answering to `token` anywhere on it.
fn switch_row(
    ui: &mut MenuCanvas,
    frame: &Frame,
    panel: &Panel,
    token: u16,
    label: &str,
    on: bool,
    top: f32,
) {
    let focused = panel.selected == token;
    let area = [COLUMN_X - 16.0, top, COLUMN_WIDTH + 32.0, SWITCH_ROW];
    if focused {
        kit::band(ui, frame, area);
    }
    let middle = top + SWITCH_ROW * 0.5;
    name(
        ui,
        frame,
        label,
        frame.rect(COLUMN_X, middle - 14.0, 240.0, 28.0),
        focused,
    );
    kit::switch(ui, frame, COLUMN_X + COLUMN_WIDTH, middle, on, focused);
    let [x, y, width, height] = area;
    ui.hit_region(token, frame.rect(x, y, width, height));
}

/// A slider's number to a tenth, without float noise (2.3, not 2.3000002) or
/// a "-0"; the sun's angles, read from the light, are calmer so.
struct Number(f32);

impl std::fmt::Display for Number {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = (self.0 * 10.0).round() / 10.0;
        let value = if value == 0.0 { 0.0 } else { value };
        write!(formatter, "{value}")
    }
}

/// One key hint: its caps and what they do.
type Hint<'a> = (&'a [&'a str], &'a str);

/// What Enter does on `token`.
fn enter_word(token: u16) -> &'static str {
    match token {
        CAMERA | SUN => "show",
        LIVE | HUD => "switch",
        _ => "act",
    }
}

/// The keys of what has the keyboard, two lines right-aligned at the
/// column's foot.
fn keys(ui: &mut MenuCanvas, frame: &Frame, panel: &Panel) {
    let s = frame.s;
    let lines: [&[Hint<'_>]; 2] = if panel.numeric.is_some() {
        [&[(&["Enter"], "apply"), (&["Esc"], "cancel")], &[]]
    } else if panel.selects_slider() {
        [
            &[(&["Up", "Down"], "choose"), (&["Left", "Right"], "adjust")],
            &[(&["Enter"], "type"), (&["F8", "Esc"], "hide")],
        ]
    } else {
        [
            &[
                (&["Up", "Down"], "choose"),
                (&["Enter"], enter_word(panel.selected)),
            ],
            &[(&["F8", "Esc"], "hide")],
        ]
    };
    let gap = 20.0 * s;
    for (index, hints) in lines.iter().enumerate() {
        if hints.is_empty() {
            continue;
        }
        let total = hints
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (hints.len() - 1) as f32;
        let [right, y] = frame.point(COLUMN_X + COLUMN_WIDTH, KEYS_Y + index as f32 * KEYS_PITCH);
        let mut x = right - total;
        for (caps, action) in *hints {
            x = key_hint(ui, caps, action, x, y, s) + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_ui::{InputEvent, PointerButton, Vec2};

    const VIEWPORTS: [[f32; 2]; 5] = [
        [1_920.0, 1_080.0],
        [3_840.0, 2_160.0],
        [2_560.0, 1_080.0],
        [1_440.0, 1_080.0],
        [1_024.0, 768.0],
    ];

    /// The panel's states that change what it draws.
    fn states() -> Vec<(&'static str, Panel)> {
        let camera = Panel {
            sun_available: true,
            ..Panel::default()
        };
        let sun = Panel {
            sun_tab: true,
            sun_available: true,
            sun_mode: SunMode::Manual,
            selected: SUN,
            ..Panel::default()
        };
        let no_sun = Panel {
            sun_tab: true,
            selected: SUN,
            ..Panel::default()
        };
        let still = Panel {
            live: false,
            hud: false,
            selected: APPLY_HIDE,
            ..Panel::default()
        };
        let mut typing = Panel {
            selected: 2,
            ..Panel::default()
        };
        assert!(typing.begin_typed("123"));
        vec![
            ("camera", camera),
            ("sun", sun),
            ("no sun", no_sun),
            ("still", still),
            ("typing", typing),
        ]
    }

    /// The pointer areas `panel` must offer: every control on show.
    fn expected_tokens(panel: &Panel) -> Vec<u16> {
        let mut tokens = vec![CAMERA, SUN];
        let available = !panel.sun_tab || panel.sun_available;
        if panel.sun_tab {
            if panel.sun_available {
                tokens.extend([4, 5, VALUE_BASE + 4, VALUE_BASE + 5]);
            }
            tokens.push(RESET_SUN);
        } else {
            tokens.extend(PRESET..PRESET + 4);
            tokens.extend((0..4).flat_map(|row| [row, VALUE_BASE + row]));
        }
        tokens.extend([6, VALUE_BASE + 6]);
        if available {
            tokens.extend([7, VALUE_BASE + 7, ORBIT, STOP, RESET]);
        }
        tokens.extend([LIVE, HUD, APPLY_HIDE, HIDE]);
        tokens
    }

    #[test]
    fn every_state_fits_its_canvas_with_every_control_reachable() {
        for viewport in VIEWPORTS {
            for (state, panel) in states() {
                let mut canvas = MenuCanvas::new();
                build(&panel, &mut canvas, viewport);
                assert!(!canvas.overflowed(), "{state} {viewport:?}");
                let expected = expected_tokens(&panel);
                assert_eq!(
                    canvas.widget_count(),
                    expected.len(),
                    "{state} {viewport:?}: {:?}",
                    canvas.widget_tokens()
                );
                for token in expected {
                    let rect = canvas
                        .rect_for(token)
                        .unwrap_or_else(|| panic!("{state} {viewport:?}: token {token}"));
                    assert!(
                        rect.x >= 0.0
                            && rect.y >= 0.0
                            && rect.right() <= viewport[0] + 0.01
                            && rect.bottom() <= viewport[1] + 0.01,
                        "{state} {viewport:?}: token {token} at {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_scene_stays_clear_left_of_the_column() {
        for viewport in VIEWPORTS {
            let s = crate::menu::sjk::scale(viewport);
            let half = viewport[0] * 0.5;
            for (state, panel) in states() {
                let mut canvas = MenuCanvas::new();
                build(&panel, &mut canvas, viewport);
                for token in canvas.widget_tokens().to_vec() {
                    let rect = canvas.rect_for(token).expect("a registered token");
                    assert!(rect.x >= half, "{state} {viewport:?}: token {token}");
                }
                for command in canvas.draw_list().commands() {
                    let (rect, clear_start) = match command {
                        DrawCommand::Text { rect, .. } => (*rect, false),
                        DrawCommand::SolidRect { rect, .. }
                        | DrawCommand::RoundedRect { rect, .. }
                        | DrawCommand::Border { rect, .. } => (*rect, false),
                        DrawCommand::GradientRect { rect, gradient, .. } => {
                            (*rect, gradient.start.a == 0.0)
                        }
                        _ => continue,
                    };
                    if rect.x >= half - 0.01 || clear_start {
                        continue;
                    }
                    // Only the viewfinder's hairlines cross the scene.
                    assert!(
                        rect.width.min(rect.height) <= (1.5 * s).max(1.0) + 0.01,
                        "{state} {viewport:?}: {command:?}"
                    );
                    assert!(
                        !matches!(command, DrawCommand::Text { .. }),
                        "{state} {viewport:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn it_reads_camera_control_in_the_ui_colours() {
        let (_, panel) = states().remove(0);
        let mut canvas = MenuCanvas::new();
        build(&panel, &mut canvas, [1_920.0, 1_080.0]);
        let runs: Vec<&str> = canvas.text_runs().collect();
        assert_eq!(runs[0], "Camera control");
        assert!(
            runs.iter()
                .all(|run| !run.to_ascii_lowercase().contains("shot")),
            "{runs:?}"
        );
        let colours: Vec<Color> = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        let colour_of = |wanted: &str| {
            let index = runs
                .iter()
                .position(|run| *run == wanted)
                .unwrap_or_else(|| panic!("{wanted} in {runs:?}"));
            colours[index]
        };
        // The page on show is gold; the other tab waits muted.
        assert_eq!(colour_of("Camera"), color::GOLD_BRIGHT);
        assert_eq!(colour_of("Sun"), color::MUTED);
        assert_eq!(colour_of("Camera control"), color::TEXT);
        // Hiding the panel is the one main action: a gold button.
        let hide = canvas.rect_for(APPLY_HIDE).expect("Hide panel");
        assert!(canvas.draw_list().commands().iter().any(|command| matches!(
            command,
            DrawCommand::RoundedRect { rect, color, .. }
                if *rect == hide && *color == color::GOLD
        )));
        // The column's ground is the UI's navy fading in from clear.
        let fades: Vec<_> = canvas
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::GradientRect { gradient, .. } => Some(*gradient),
                _ => None,
            })
            .collect();
        assert_eq!(fades[0].start, color::alpha(color::SPACE, 0.0));
        assert_eq!(
            fades.last().expect("fades").end,
            color::alpha(color::SPACE, 0.86)
        );
    }

    /// Press and release the primary button at `position`.
    fn click(panel: &mut Panel, canvas: &mut MenuCanvas, position: Vec2) -> Option<Action> {
        let button = PointerButton::Primary;
        let _ = panel.pointer(canvas, InputEvent::PointerPress { position, button });
        panel.pointer(canvas, InputEvent::PointerRelease { position, button })
    }

    #[test]
    fn a_click_on_a_track_sets_the_slider_where_it_lands() {
        for viewport in VIEWPORTS {
            let mut panel = Panel::default();
            let mut canvas = MenuCanvas::new();
            build(&panel, &mut canvas, viewport);
            // Distance, 16 to 512: three quarters along is 388.
            let track = canvas.rect_for(2).expect("the track");
            let at = Vec2::new(track.x + track.width * 0.75, track.y + track.height * 0.5);
            assert_eq!(click(&mut panel, &mut canvas, at), Some(Action::Preview));
            assert_eq!(panel.values[2], 388.0, "{viewport:?}");
            assert_eq!(panel.selected, 2);
            // The track is the drawn one: it starts at the column's edge and
            // spans the column.
            let frame = frame(viewport);
            assert!((track.x - frame.point(COLUMN_X, 0.0)[0]).abs() < 0.01);
            assert!((track.width - COLUMN_WIDTH * frame.s).abs() < 0.01);
            // A click on the number opens it for typing.
            build(&panel, &mut canvas, viewport);
            let number = canvas.rect_for(VALUE_BASE + 2).expect("the number");
            let at = Vec2::new(
                number.x + number.width * 0.5,
                number.y + number.height * 0.5,
            );
            assert_eq!(click(&mut panel, &mut canvas, at), None);
            assert_eq!(panel.numeric.as_ref().map(|edit| edit.row), Some(2));
            // A tab, a switch and the main action answer anywhere on them.
            for (token, check) in [(SUN, Some(None)), (LIVE, Some(None)), (APPLY_HIDE, None)] {
                build(&panel, &mut canvas, viewport);
                let rect = canvas.rect_for(token).expect("a control");
                let at = Vec2::new(rect.x + 2.0, rect.y + rect.height * 0.5);
                let action = click(&mut panel, &mut canvas, at);
                match check {
                    Some(expected) => assert_eq!(action, expected, "{token}"),
                    None => assert_eq!(action, Some(Action::ApplyHide)),
                }
            }
            assert!(panel.sun_tab, "Sun shows");
            assert!(!panel.live, "live preview switched off");
        }
    }

    /// The tokens Down visits from the first tab, once round.
    fn walk(build: fn(&Panel, &mut MenuCanvas, [f32; 2]), forward: bool) -> Vec<u16> {
        let mut panel = Panel {
            sun_available: true,
            ..Panel::default()
        };
        let mut canvas = MenuCanvas::new();
        let mut visited = Vec::new();
        for _ in 0..40 {
            build(&panel, &mut canvas, [1_920.0, 1_080.0]);
            panel.step(&mut canvas, forward);
            if panel.selected == CAMERA {
                break;
            }
            visited.push(panel.selected);
        }
        visited
    }

    #[test]
    fn up_and_down_stop_once_on_each_control() {
        let sliders = |rows: std::ops::Range<u16>| rows.map(|row| VALUE_BASE + row);
        let mut expected = vec![SUN];
        expected.extend(PRESET..PRESET + 4);
        expected.extend(sliders(0..4));
        expected.extend(sliders(6..8));
        expected.extend([ORBIT, STOP, RESET, LIVE, HUD, APPLY_HIDE, HIDE]);
        assert_eq!(walk(build, true), expected);
        // Up goes the other way, onto each slider's track first.
        let back = walk(build, false);
        assert_eq!(back.first(), Some(&HIDE));
        assert_eq!(back.len(), expected.len());
        assert!(back.contains(&7) && !back.contains(&(VALUE_BASE + 7)));
    }

    #[test]
    fn numbers_show_without_float_noise() {
        assert_eq!(Number(2.300_000_2).to_string(), "2.3");
        assert_eq!(Number(97.5).to_string(), "97.5");
        assert_eq!(Number(122.47).to_string(), "122.5");
        assert_eq!(Number(-0.01).to_string(), "0");
        assert_eq!(Number(-37.0).to_string(), "-37");
        assert_eq!(Number(80.0).to_string(), "80");
    }
}

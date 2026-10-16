//! Create game in the SJK UI's look, in every menu style (it has no classic+
//! version): the top bar's way back and title, the match's rows under "Match"
//! and "Server" (cyclers, the server name's field, the LAN switch), the gold
//! Start button, the chosen map's levelshot with its names on the right, how
//! the start stands at the bottom left and the keys at the bottom right. Every
//! position is in frame pixels ([`Frame`]). Design: `docs/sjk-ui.md`.

use super::create_game::{BACK_TOKEN, CreateGameMenu, ROWS, Row};
use super::create_game_catalog::{MODES, ScoreLimit};
use super::levelshot::{Preview, cover_uv};
use super::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The rows' column: its left edge and width, a row's height.
const ROWS_X: f32 = 96.0;
const ROWS_WIDTH: f32 = 860.0;
const LINE: f32 = 56.0;
/// A row's name starts here; its control spans `CONTROL_LEFT` to
/// `CONTROL_RIGHT`.
const LABEL_X: f32 = ROWS_X + 22.0;
const CONTROL_RIGHT: f32 = ROWS_X + ROWS_WIDTH - 22.0;
const CONTROL_LEFT: f32 = CONTROL_RIGHT - 380.0;
/// The "Match" sub-heading's line and its rows' first top; the same for
/// "Server".
const MATCH_Y: f32 = 196.0;
const MATCH_TOP: f32 = 216.0;
const SERVER_Y: f32 = 594.0;
const SERVER_TOP: f32 = 614.0;
/// Start, under the rows.
const START: [f32; 4] = [ROWS_X, 768.0, 320.0, 52.0];
/// The chosen map's column: its levelshot (2:1, as the HD packs' are; a
/// retail one is cut at its top and bottom), its name, the match's figures
/// in two columns and the line saying who it is for.
pub(super) const DETAIL_X: f32 = 1064.0;
pub(super) const DETAIL_WIDTH: f32 = 760.0;
pub(super) const PICTURE: [f32; 4] = [DETAIL_X, 176.0, DETAIL_WIDTH, DETAIL_WIDTH * 0.5];
pub(super) const NAME_TOP: f32 = 576.0;
const STATS_TOP: f32 = 636.0;
const STAT_WIDTH: f32 = DETAIL_WIDTH * 0.5;
const ABOUT_Y: f32 = 764.0;
/// The keys' and the status's line at the bottom.
pub(super) const KEYS_Y: f32 = 992.0;

/// The stock game's names for bot skills 1 to 5.
const SKILL_NAMES: [&str; 5] = ["Initiate", "Padawan", "Jedi", "Jedi Knight", "Jedi Master"];

/// How many of [`ROWS`] sit under "Match"; the rest but Start under "Server".
const MATCH_ROWS: usize = 6;

impl CreateGameMenu {
    /// Draw the screen (or the open map list) at `reveal` opacity, its text to
    /// `target`.
    pub(crate) fn append(&mut self, target: TextTarget<'_>, viewport: [f32; 2], reveal: f32) {
        self.build(viewport, reveal);
        target.append(&self.ui, viewport);
    }

    /// Lay the screen (or the open map list) out on `self.ui`.
    pub(super) fn build(&mut self, viewport: [f32; 2], reveal: f32) {
        if self.picker.is_open() {
            self.build_picker(viewport, reveal);
            return;
        }
        let frame = Frame::new(viewport);
        self.ui.begin_transparent(viewport);
        self.ui.push_opacity(reveal);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        top_bar(
            &mut self.ui,
            &frame,
            "Main menu",
            BACK_TOKEN,
            "Create game",
            None,
        );
        kit::heading(&mut self.ui, &frame, ROWS_X, MATCH_Y, ROWS_WIDTH, "Match");
        kit::heading(&mut self.ui, &frame, ROWS_X, SERVER_Y, ROWS_WIDTH, "Server");
        for (row, kind) in ROWS.iter().enumerate() {
            self.row_view(&frame, row, *kind);
        }
        self.detail(&frame);
        let s = frame.s;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", self.status),
            frame.rect(ROWS_X, KEYS_Y, 760.0, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        self.keys(&frame);
        self.ui.pop_opacity();
        self.ui.finish(self.selected as u16);
    }

    /// Row `row`: the band when chosen, its name and its control; Start is the
    /// gold button under the rows.
    fn row_view(&mut self, frame: &Frame, row: usize, kind: Row) {
        let focused = row == self.selected;
        let token = row as u16;
        if kind == Row::Start {
            let hosting = self.hosting();
            let label = if hosting {
                "Starting..."
            } else {
                "Start the game"
            };
            kit::button(
                &mut self.ui,
                frame,
                START,
                label,
                true,
                !hosting,
                focused,
                token,
            );
            return;
        }
        let top = if row < MATCH_ROWS {
            MATCH_TOP + row as f32 * LINE
        } else {
            SERVER_TOP + (row - MATCH_ROWS) as f32 * LINE
        };
        let middle = top + LINE * 0.5;
        if focused {
            kit::band(&mut self.ui, frame, [ROWS_X, top, ROWS_WIDTH, LINE]);
        } else if self.ui.token_hovered(token) {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(ROWS_X, top, ROWS_WIDTH, LINE),
                radius: 10.0 * frame.s,
                color: color::alpha(color::HOLO, 0.05),
            });
        }
        let mode = MODES[self.draft.mode];
        let name = match (kind, mode.score) {
            (Row::ScoreLimit, ScoreLimit::Captures) => "Capture limit",
            (Row::ScoreLimit, ScoreLimit::Frags) => "Frag limit",
            _ => label(kind),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{name}"),
            frame.rect(LABEL_X, middle - 14.0, CONTROL_LEFT - LABEL_X - 24.0, 28.0),
            19.0 * frame.s,
            if focused {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                color::alpha(color::TEXT, 0.88)
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        let control = [
            CONTROL_LEFT,
            middle - kit::CONTROL_HEIGHT * 0.5,
            CONTROL_RIGHT - CONTROL_LEFT,
            kit::CONTROL_HEIGHT,
        ];
        self.control(frame, kind, control, middle, focused);
        // The control first: a click on a cycler steps the way the half of
        // it under the pointer points (`rect_for` gives the first target).
        let [x, y, width, height] = control;
        self.ui.hit_region(token, frame.rect(x, y, width, height));
        self.ui
            .hit_region(token, frame.rect(ROWS_X, top, ROWS_WIDTH, LINE));
    }

    /// Row `kind`'s control over `control` (frame pixels).
    fn control(&mut self, frame: &Frame, kind: Row, control: [f32; 4], middle: f32, focused: bool) {
        let mode = MODES[self.draft.mode];
        let draft = &self.draft;
        let ui = &mut self.ui;
        match kind {
            Row::Mode => kit::cycler(
                ui,
                frame,
                control,
                format_args!("{}", mode.label),
                None,
                focused,
            ),
            Row::Map => {
                let title = self
                    .catalogue
                    .map(draft.mode, &draft.map)
                    .map_or(draft.map.as_str(), |entry| entry.title.as_str());
                kit::cycler(ui, frame, control, format_args!("{title}"), None, focused);
            }
            Row::Bots if draft.bots == 0 => {
                kit::cycler(ui, frame, control, format_args!("None"), None, focused)
            }
            Row::Bots => kit::cycler(
                ui,
                frame,
                control,
                format_args!("{}", draft.bots),
                None,
                focused,
            ),
            Row::BotSkill => {
                let name = SKILL_NAMES[usize::from(draft.bot_skill.clamp(1, 5)) - 1];
                kit::cycler(ui, frame, control, format_args!("{name}"), None, focused);
            }
            Row::ScoreLimit => {
                let limit = match mode.score {
                    ScoreLimit::Frags => draft.fraglimit,
                    ScoreLimit::Captures => draft.capturelimit,
                    ScoreLimit::None => {
                        kit::field(
                            ui,
                            frame,
                            control,
                            format_args!("Objectives"),
                            focused,
                            false,
                        );
                        return;
                    }
                };
                if limit == 0 {
                    kit::cycler(ui, frame, control, format_args!("None"), None, focused);
                } else {
                    kit::cycler(ui, frame, control, format_args!("{limit}"), None, focused);
                }
            }
            Row::TimeLimit if draft.timelimit == 0 => {
                kit::cycler(ui, frame, control, format_args!("None"), None, focused)
            }
            Row::TimeLimit => kit::cycler(
                ui,
                frame,
                control,
                format_args!("{} min", draft.timelimit),
                None,
                focused,
            ),
            Row::Hostname => match &self.editing {
                Some(buffer) => {
                    kit::field(ui, frame, control, format_args!("{buffer}_"), true, false);
                }
                None => {
                    kit::field(
                        ui,
                        frame,
                        control,
                        format_args!("{}", draft.hostname),
                        focused,
                        false,
                    );
                }
            },
            Row::Lan => {
                let left = kit::switch(ui, frame, CONTROL_RIGHT, middle, draft.allow_lan, focused);
                text(
                    ui,
                    TextFamily::Body,
                    format_args!(
                        "{}",
                        if draft.allow_lan {
                            "Your network can join"
                        } else {
                            "This PC only"
                        }
                    ),
                    frame.rect(
                        CONTROL_LEFT,
                        middle - 12.0,
                        left - 16.0 - CONTROL_LEFT,
                        24.0,
                    ),
                    17.0 * frame.s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::End,
                );
            }
            Row::Start => {}
        }
    }

    /// The chosen map: its levelshot with its file name over its foot (a click
    /// opens the map list, as the Map row does), its name, the match in four
    /// figures and who it is for.
    fn detail(&mut self, frame: &Frame) {
        let s = frame.s;
        let draft = &self.draft;
        let map = draft.map.as_str();
        draw_picture(
            &mut self.ui,
            frame,
            PICTURE,
            self.levelshots.preview(map),
            self.levelshots.size(map),
            map,
        );
        if let Some(row) = ROWS.iter().position(|kind| *kind == Row::Map) {
            let [x, y, width, height] = PICTURE;
            self.ui
                .hit_region(row as u16, frame.rect(x, y, width, height));
        }
        let mode = MODES[draft.mode];
        let title = self
            .catalogue
            .map(draft.mode, map)
            .map_or(map, |entry| entry.title.as_str());
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{title}"),
            frame.rect(DETAIL_X, NAME_TOP, DETAIL_WIDTH, 40.0),
            32.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let skill = SKILL_NAMES[usize::from(draft.bot_skill.clamp(1, 5)) - 1];
        let bots = match draft.bots {
            0 => "None".to_owned(),
            bots => format!("{bots}, {skill}"),
        };
        let score = match mode.score {
            ScoreLimit::Frags if draft.fraglimit > 0 => format!("{} frags", draft.fraglimit),
            ScoreLimit::Captures if draft.capturelimit > 0 => {
                format!("{} captures", draft.capturelimit)
            }
            ScoreLimit::None => "Objectives".to_owned(),
            _ => String::new(),
        };
        let limits = match (score.is_empty(), draft.timelimit) {
            (true, 0) => "None".to_owned(),
            (true, minutes) => format!("{minutes} minutes"),
            (false, 0) => score,
            (false, minutes) => format!("{score}, {minutes} min"),
        };
        let stats = [
            ("Game type", mode.label.to_owned()),
            ("Bots", bots),
            ("Limits", limits),
            (
                "Who can join",
                if draft.allow_lan {
                    "Your network"
                } else {
                    "This PC only"
                }
                .to_owned(),
            ),
        ];
        for (index, (label, value)) in stats.iter().enumerate() {
            let left = DETAIL_X + (index % 2) as f32 * STAT_WIDTH;
            let top = STATS_TOP + (index / 2) as f32 * 56.0;
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{label}"),
                frame.rect(left, top, STAT_WIDTH - 16.0, 20.0),
                14.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{value}"),
                frame.rect(left, top + 20.0, STAT_WIDTH - 16.0, 30.0),
                22.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let about = if draft.allow_lan {
            "A match on this machine. Players on your network can join."
        } else {
            "A match on this machine, for you and the bots only."
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{about}"),
            frame.rect(DETAIL_X, ABOUT_Y, DETAIL_WIDTH, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The keys of the chosen row, right-aligned at the bottom.
    fn keys(&mut self, frame: &Frame) {
        let mut keys: [(&[&str], &str); 4] = [(&[], ""); 4];
        let mut count = 0;
        let mut add = |caps: &'static [&'static str], action: &'static str| {
            keys[count] = (caps, action);
            count += 1;
        };
        let hosting = self.hosting();
        if self.editing.is_some() {
            add(&["Enter"], "keep");
            add(&["Esc"], "cancel");
        } else {
            match ROWS[self.selected] {
                Row::Start if !hosting => add(&["Enter"], "start"),
                Row::Map if !hosting => {
                    add(&["Enter"], "all maps");
                    add(&["Left", "Right"], "change");
                }
                Row::Hostname if !hosting => add(&["Enter"], "edit"),
                Row::Lan if !hosting => add(&["Enter"], "switch"),
                Row::Start | Row::Map | Row::Hostname | Row::Lan => {}
                _ if !hosting => add(&["Left", "Right"], "change"),
                _ => {}
            }
            add(&["Up", "Down"], "choose");
            add(&["Esc"], "main menu");
        }
        draw_keys(&mut self.ui, frame, &keys[..count]);
    }
}

/// `keys` (caps and what they do) right-aligned on the bottom line.
pub(super) fn draw_keys(ui: &mut MenuCanvas, frame: &Frame, keys: &[(&[&str], &str)]) {
    let s = frame.s;
    let gap = 30.0 * s;
    let width: f32 = keys
        .iter()
        .map(|(caps, action)| key_hint_width(caps, action, s))
        .sum::<f32>()
        + gap * keys.len().saturating_sub(1) as f32;
    let [right, y] = frame.point(1824.0, KEYS_Y);
    let mut x = right - width;
    for (caps, action) in keys {
        x = key_hint(ui, caps, action, x, y, s) + gap;
    }
}

/// Map `map`'s levelshot over `rect` (frame pixels), cropped to it, never
/// stretched (a square retail levelshot is a 4:3 picture), darkened towards
/// its foot where `map` is written, with a holo edge; a line saying so for a
/// map that ships none.
pub(super) fn draw_picture(
    ui: &mut MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    preview: Preview,
    size: Option<[u32; 2]>,
    map: &str,
) {
    let s = frame.s;
    let [x, y, width, height] = rect;
    let area = frame.rect(x, y, width, height);
    let _ = ui.draw_list_mut().push(DrawCommand::SolidRect {
        rect: area,
        color: color::alpha(color::SPACE, 0.7),
    });
    match preview {
        Preview::Image => {
            let _ = ui.draw_list_mut().push(DrawCommand::TexturedQuadUv {
                rect: area,
                texture: crate::ui_renderer::LEVELSHOT_TEXTURE,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
                uv: cover_uv(size.unwrap_or([4, 3]), width / height),
            });
        }
        Preview::Missing => text(
            ui,
            TextFamily::Body,
            format_args!("No picture of this map"),
            frame.rect(x, y, width, height - 40.0),
            16.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Center,
        ),
        Preview::Loading => {}
    }
    super::sjk::fade(
        ui,
        frame.rect(x, y + height * 0.5, width, height * 0.5),
        color::alpha(color::SPACE, 0.0),
        color::alpha(color::SPACE, 0.85),
    );
    let _ = ui.draw_list_mut().push(DrawCommand::Border {
        rect: area,
        radius: 0.0,
        width: s.max(1.0),
        color: color::alpha(color::HOLO, 0.25),
    });
    text(
        ui,
        TextFamily::Display,
        format_args!("{map}"),
        frame.rect(x + 18.0, y + height - 46.0, width - 36.0, 36.0),
        24.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
}

/// A row's name when it does not depend on the mode.
fn label(kind: Row) -> &'static str {
    match kind {
        Row::Mode => "Game type",
        Row::Map => "Map",
        Row::Bots => "Bots",
        Row::BotSkill => "Bot skill",
        Row::ScoreLimit => "Score limit",
        Row::TimeLimit => "Time limit",
        Row::Hostname => "Server name",
        Row::Lan => "Allow LAN players",
        Row::Start => "Start",
    }
}

const _: () = assert!(ROWS.len() == MATCH_ROWS + 3);
const _: () = assert!(LABEL_X + 200.0 < CONTROL_LEFT);
const _: () = assert!(MATCH_TOP + MATCH_ROWS as f32 * LINE + 24.0 < SERVER_Y);
const _: () = assert!(SERVER_TOP + 2.0 * LINE + 20.0 < START[1]);
const _: () = assert!(START[1] + START[3] < KEYS_Y - 20.0);
const _: () = assert!(ROWS_X + ROWS_WIDTH + 60.0 < DETAIL_X);
const _: () = assert!(DETAIL_X + DETAIL_WIDTH <= 1824.0);
const _: () = assert!(PICTURE[1] + PICTURE[3] + 12.0 < NAME_TOP);
const _: () = assert!(NAME_TOP + 44.0 < STATS_TOP && STATS_TOP + 2.0 * 56.0 < ABOUT_Y);
const _: () = assert!(ABOUT_Y + 24.0 < KEYS_Y - 16.0);

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen and its map list lay out within the canvas at 1080p, 4K and
    /// 4:3; a cycler's first target is its control, so a click steps the way
    /// the half under the pointer points; Start and the way back answer.
    #[test]
    fn create_game_lays_out_its_rows_and_list() {
        let mut menu = CreateGameMenu::new();
        for viewport in [[1920.0, 1080.0], [3840.0, 2160.0], [1440.0, 1080.0]] {
            menu.for_shot(0, None);
            menu.build(viewport, 1.0);
            assert!(!menu.overflowed());
            let frame = Frame::new(viewport);
            let control = menu.ui.rect_for(0).expect("the game type's cycler");
            let left = frame.point(CONTROL_LEFT, 0.0)[0];
            assert!((control.x - left).abs() < 0.5);
            assert!(menu.ui.rect_for(8).is_some(), "Start");
            assert!(menu.ui.rect_for(BACK_TOKEN).is_some(), "the way back");
            menu.for_shot(1, Some(""));
            menu.build(viewport, 1.0);
            assert!(!menu.overflowed());
            assert!(menu.ui.rect_for(BACK_TOKEN).is_some());
        }
    }
}

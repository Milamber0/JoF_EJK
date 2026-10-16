//! The key bindings as the SJK UI draws them (`docs/sjk-ui.md`, Settings): a
//! category of the Settings screen, in its frame (the rail, the search, the
//! detail column), every action under its group's sub-heading with its two
//! keys as caps; the cap waiting for a key turns gold. The list is the
//! classic+ panel's ([`super::classic_view::ClassicList`]), so its search, its
//! keys and capture work as they do there.

use super::classic_view::{HEADINGS, ListRow, ROWS_SCROLL_TOKEN, rebound};
use super::*;
use crate::menu::sjk::{Frame, TextTarget, color, key_hint, key_hint_width, kit, text, wrap};
use crate::menu_widgets::TextFamily;
use crate::settings::Rail;
use crate::settings::sjk_view::{
    CONTROL_RIGHT, DETAIL_TOP, DETAIL_WIDTH, DETAIL_X, KEYS_Y, LABEL_X, LINE, ROWS_TOP, ROWS_WIDTH,
    ROWS_X, VISIBLE, backdrop, draw_rail, row_name, top_bar,
};
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// A key cap's height.
const CAP: f32 = 34.0;
/// How wide an action's name may run, clear of its caps.
const NAME_COLUMN: f32 = 330.0;

/// What a key slot shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Slot<'a> {
    Key(&'a str),
    Empty,
    Waiting,
}

impl<'a> Slot<'a> {
    /// The slot holding `key` (as the editor lists it: `UNBOUND` or `-` when
    /// empty), or waiting for one.
    fn of(waiting: bool, key: &'a str) -> Self {
        if waiting {
            Self::Waiting
        } else if key == "UNBOUND" || key == "-" || key.is_empty() {
            Self::Empty
        } else {
            Self::Key(key)
        }
    }
}

impl KeybindEditor {
    /// Draw the bindings as the SJK UI's Key bindings category, with `rail`'s
    /// categories, at `reveal` opacity.
    pub(crate) fn append_sjk(
        &mut self,
        target: TextTarget<'_>,
        viewport: [f32; 2],
        reveal: f32,
        rail: &Rail<'_>,
    ) {
        let frame = Frame::new(viewport);
        self.ui.begin_transparent(viewport);
        self.ui.push_opacity(reveal);
        backdrop(&mut self.ui, viewport);
        let Some(list) = self.classic.take() else {
            self.ui.pop_opacity();
            self.ui.finish(0);
            return;
        };
        let found = (!list.search.trim().is_empty()).then(|| list.actions());
        top_bar(
            &mut self.ui,
            &frame,
            rail.back,
            &list.search,
            list.searching,
            "Find an action or a key",
            found,
            rail.mode,
        );
        draw_rail(&mut self.ui, &frame, rail);
        self.sjk_rows(&frame, &target.body_measure(), &list);
        let focused = list.position(self.selected).map(|_| self.selected);
        let searching = list.searching;
        self.classic = Some(list);
        self.sjk_detail(&frame, focused);
        self.sjk_keys(&frame, searching, rail.mode);
        self.ui.pop_opacity();
        self.ui.finish(self.selected as u16);
        target.append(&self.ui, viewport);
    }

    /// The list's lines on show: sub-headings and actions with their caps;
    /// `measure` measures the actions' names.
    fn sjk_rows(
        &mut self,
        frame: &Frame,
        measure: &crate::sjk_chat_look::Measure<'_>,
        list: &super::classic_view::ClassicList,
    ) {
        let s = frame.s;
        self.visible = VISIBLE;
        let total = list.rows.len();
        self.first = self.first.min(total.saturating_sub(VISIBLE));
        let shown = self.first..total.min(self.first + VISIBLE);
        if total > VISIBLE {
            self.ui.scroll_region(
                ROWS_SCROLL_TOKEN,
                frame.rect(ROWS_X, ROWS_TOP, ROWS_WIDTH, VISIBLE as f32 * LINE),
            );
        }
        if list.rows.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("No action matches \u{201c}{}\u{201d}.", list.search.trim()),
                frame.rect(LABEL_X, ROWS_TOP + 14.0, ROWS_WIDTH - 44.0, 28.0),
                19.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let icons = icons::any(0..ACTIONS.len());
        for (index, row) in list.rows[shown].iter().enumerate() {
            let top = ROWS_TOP + index as f32 * LINE;
            let action = match *row {
                ListRow::Heading(category) => {
                    kit::heading(
                        &mut self.ui,
                        frame,
                        ROWS_X,
                        top + 36.0,
                        ROWS_WIDTH,
                        HEADINGS[category],
                    );
                    continue;
                }
                ListRow::Action(action) => action,
            };
            let focused = action == self.selected;
            let middle = top + LINE * 0.5;
            if focused {
                kit::band(&mut self.ui, frame, [ROWS_X, top, ROWS_WIDTH, LINE]);
            }
            self.ui
                .hit_region(action as u16, frame.rect(ROWS_X, top, ROWS_WIDTH, LINE));
            let mut label_x = LABEL_X;
            if let Some(texture) = self.icons.ready(action) {
                let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                    rect: frame.rect(LABEL_X, middle - 16.0, 32.0, 32.0),
                    texture,
                    color: Color::new(1.0, 1.0, 1.0, if focused { 1.0 } else { 0.85 }),
                });
            }
            if icons {
                label_x += 44.0;
            }
            let keys = self.keys.get(action).cloned().unwrap_or_default();
            row_name(
                &mut self.ui,
                frame,
                measure,
                ACTIONS[action].label,
                label_x,
                middle,
                NAME_COLUMN,
                focused,
                rebound(action, &keys),
            );
            let waiting = |slot| focused && self.capture && self.binding_slot == slot;
            let second = Slot::of(waiting(1), &keys[1]);
            let first = Slot::of(waiting(0), &keys[0]);
            let second_left = cap(&mut self.ui, frame, CONTROL_RIGHT, middle, second, focused);
            let first_right = second_left - 10.0;
            let _ = cap(&mut self.ui, frame, first_right, middle, first, focused);
            // After the row: the region registered last takes the pointer.
            self.ui.hit_region(
                SECONDARY_BASE + action as u16,
                frame.rect(
                    second_left,
                    middle - CAP * 0.5,
                    CONTROL_RIGHT - second_left,
                    CAP,
                ),
            );
        }
        if total > VISIBLE {
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                frame.rect(
                    ROWS_X + ROWS_WIDTH + 14.0,
                    ROWS_TOP,
                    4.0,
                    VISIBLE as f32 * LINE,
                ),
                self.first,
                VISIBLE,
                total,
            );
        }
    }

    /// The focused action explained: its group's icon and name, then while a
    /// key is awaited what to press, else its keys, default, console command
    /// and what else its keys do.
    fn sjk_detail(&mut self, frame: &Frame, action: Option<usize>) {
        let s = frame.s;
        let Some(action) = action else {
            return;
        };
        self.write_detail(Some(action));
        let entry = &ACTIONS[action];
        let icon = crate::menu::classic::layout::Entry::of_category(entry.category as usize)
            .and_then(crate::menu::classic::layout::Entry::icon)
            .and_then(crate::settings_icons::texture);
        let title_x = DETAIL_X + if icon.is_some() { 58.0 } else { 0.0 };
        if let Some(texture) = icon {
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(DETAIL_X, DETAIL_TOP - 2.0, 44.0, 44.0),
                texture,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", entry.label),
            frame.rect(title_x, DETAIL_TOP, DETAIL_X + DETAIL_WIDTH - title_x, 40.0),
            32.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let mut y = DETAIL_TOP + 64.0;
        if self.capture {
            let slot = if self.binding_slot == 0 {
                "first"
            } else {
                "second"
            };
            for (line, colour) in [
                (
                    format!("Press the key for {}, as its {slot} key.", entry.label),
                    color::GOLD_BRIGHT,
                ),
                (
                    "Esc cancels; Backspace clears its keys.".to_owned(),
                    color::MUTED,
                ),
            ] {
                for part in wrap(&line, 44) {
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("{part}"),
                        frame.rect(DETAIL_X, y, DETAIL_WIDTH, 30.0),
                        19.0 * s,
                        colour,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    y += 30.0;
                }
            }
            return;
        }
        let [keys, _, shared, _] = self.detail.clone();
        let default = if entry.default_key.is_empty() {
            "none".to_owned()
        } else {
            key_label(&sjk_shell::key_names::display_key(entry.default_key))
        };
        let shown_keys = keys
            .split(" or ")
            .map(key_label)
            .collect::<Vec<_>>()
            .join(" and ");
        let facts: [(&str, String, Color); 3] = [
            ("Keys", shown_keys, color::TEXT),
            ("Default", default, color::TEXT),
            ("Console", entry.command.to_owned(), color::HOLO),
        ];
        for (name, value, colour) in facts {
            fact(&mut self.ui, frame, &mut y, name, &value, colour);
        }
        if !shared.is_empty() {
            y += 14.0;
            for part in wrap(&shared, 44).take(4) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(DETAIL_X, y, DETAIL_WIDTH, 28.0),
                    17.0 * s,
                    color::EMBER,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 28.0;
            }
        }
    }

    /// The keys of what has the keyboard, right-aligned at the bottom.
    fn sjk_keys(
        &mut self,
        frame: &Frame,
        searching: bool,
        mode: crate::settings::mode::SettingsMode,
    ) {
        let s = frame.s;
        let other = mode.other().hint();
        let keys: &[(&[&str], &str)] = if self.capture {
            &[(&["Esc"], "cancel"), (&["Backspace"], "clear")]
        } else if searching {
            &[(&["Enter"], "results"), (&["Esc"], "clear")]
        } else {
            &[
                (&["Enter"], "bind a key"),
                (&["Backspace"], "clear"),
                (&["Tab"], "next group"),
                (&["F2"], other),
            ]
        };
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * keys.len().saturating_sub(1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

/// One key cap ending at `right`, centred on `middle`; returns its left edge.
fn cap(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    right: f32,
    middle: f32,
    slot: Slot<'_>,
    focused: bool,
) -> f32 {
    let s = frame.s;
    let label = match slot {
        Slot::Key(key) => key_label(key),
        Slot::Empty => "\u{2014}".to_owned(),
        Slot::Waiting => "Press a key".to_owned(),
    };
    // Rajdhani SemiBold at 19 is about 9.5 pixels a character.
    let width = (22.0 + 9.5 * label.chars().count() as f32).max(60.0);
    let left = right - width;
    let rect = frame.rect(left, middle - CAP * 0.5, width, CAP);
    let radius = 8.0 * s;
    let (fill, edge, ink) = match slot {
        Slot::Waiting => (
            color::GOLD,
            color::GOLD_BRIGHT,
            Color::new(0.078, 0.063, 0.02, 1.0),
        ),
        Slot::Key(_) => (
            color::alpha(color::SPACE, 0.6),
            color::alpha(
                if focused { color::TEXT } else { color::HOLO },
                if focused { 0.7 } else { 0.45 },
            ),
            color::TEXT,
        ),
        Slot::Empty => (
            color::alpha(color::SPACE, 0.3),
            color::alpha(color::HOLO, 0.18),
            color::QUIET,
        ),
    };
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect,
        radius,
        color: fill,
    });
    let _ = canvas.draw_list_mut().push(DrawCommand::Border {
        rect,
        radius,
        width: 1.5 * s,
        color: edge,
    });
    text(
        canvas,
        TextFamily::Display,
        format_args!("{label}"),
        frame.rect(left, middle - 13.0, width, 26.0),
        19.0 * s,
        ink,
        FontWeight::Semibold,
        TextAlign::Center,
    );
    left
}

/// One fact of the detail column: `name` then `value`, at `y`, which moves on.
fn fact(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    y: &mut f32,
    name: &str,
    value: &str,
    colour: Color,
) {
    let s = frame.s;
    text(
        canvas,
        TextFamily::Body,
        format_args!("{name}"),
        frame.rect(DETAIL_X, *y, 110.0, 28.0),
        17.0 * s,
        color::MUTED,
        FontWeight::Regular,
        TextAlign::Start,
    );
    text(
        canvas,
        TextFamily::Body,
        format_args!("{value}"),
        frame.rect(DETAIL_X + 110.0, *y, DETAIL_WIDTH - 110.0, 28.0),
        17.0 * s,
        colour,
        FontWeight::Regular,
        TextAlign::Start,
    );
    *y += 32.0;
}

/// A key's name as its cap shows it: a letter or digit as it is, the named
/// keys in words ("Space", "Mouse 1", "Wheel up", "Up"), the rest in sentence
/// case.
pub(crate) fn key_label(name: &str) -> String {
    let upper = name.trim().to_ascii_uppercase();
    let named = match upper.as_str() {
        "SPACE" => "Space",
        "MOUSE1" => "Mouse 1",
        "MOUSE2" => "Mouse 2",
        "MOUSE3" => "Mouse 3",
        "MOUSE4" => "Mouse 4",
        "MOUSE5" => "Mouse 5",
        "MWHEELUP" => "Wheel up",
        "MWHEELDOWN" => "Wheel down",
        "UPARROW" => "Up",
        "DOWNARROW" => "Down",
        "LEFTARROW" => "Left",
        "RIGHTARROW" => "Right",
        "CTRL" => "Ctrl",
        "ALT" => "Alt",
        "SHIFT" => "Shift",
        "TAB" => "Tab",
        "ENTER" => "Enter",
        "BACKSPACE" => "Backspace",
        "CAPSLOCK" => "Caps lock",
        "PGUP" => "Page up",
        "PGDN" => "Page down",
        "INS" => "Insert",
        "DEL" => "Delete",
        "HOME" => "Home",
        "END" => "End",
        "PAUSE" => "Pause",
        "ESCAPE" => "Esc",
        "SEMICOLON" => ";",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_owned();
    }
    if let Some(pad) = upper.strip_prefix("KP_") {
        return format!("Pad {}", key_label(pad));
    }
    if upper.chars().count() <= 3 {
        return upper;
    }
    let mut out = String::with_capacity(upper.len());
    for (index, character) in upper.chars().enumerate() {
        out.push(if index == 0 {
            character
        } else {
            character.to_ascii_lowercase()
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_read_as_their_caps_show_them() {
        assert_eq!(key_label("w"), "W");
        assert_eq!(key_label("SPACE"), "Space");
        assert_eq!(key_label("MOUSE1"), "Mouse 1");
        assert_eq!(key_label("MWHEELDOWN"), "Wheel down");
        assert_eq!(key_label("UPARROW"), "Up");
        assert_eq!(key_label("F10"), "F10");
        assert_eq!(key_label("KP_ENTER"), "Pad Enter");
        assert_eq!(key_label("JOY12"), "Joy12");
    }
}

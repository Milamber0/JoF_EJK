//! Pointer interaction for the retained settings form.

use super::*;
use crate::menu_widgets::cycler_direction;
use sjk_ui::{InputEvent, PointerButton, UiEventKind};

impl SettingsMenu {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        let was_searching = self.searching;
        self.usage.recording = true;
        let result = self.pointer(event, console);
        self.usage.recording = false;
        self.after_input(console, was_searching, &result);
        result
    }

    fn pointer(&mut self, event: InputEvent, console: &mut ViewerConsole) -> SettingsResult {
        if self.wheel.is_open() {
            return self.wheel_pointer(event, console);
        }
        if self.hud.is_open() {
            self.hud_picker_pointer(event, console);
            return SettingsResult::None;
        }
        if self.picker.is_open() {
            self.resolution_pointer(event, console);
            return SettingsResult::None;
        }
        let secondary = matches!(
            event,
            InputEvent::PointerRelease {
                button: PointerButton::Secondary,
                ..
            }
        );
        let Some(event) = self.ui.pointer(event) else {
            return SettingsResult::None;
        };
        // An open dropdown takes the pointer: a choice applies, a click
        // anywhere else closes it with nothing changed.
        if let Some(dropdown) = &mut self.dropdown {
            let base = super::classic_view::DROPDOWN_BASE;
            let pick = event
                .token
                .and_then(|token| token.checked_sub(base))
                .map(usize::from)
                .filter(|index| *index < dropdown.picks.len());
            match (event.kind, pick) {
                (UiEventKind::HoverEnter | UiEventKind::Hover, Some(index)) => {
                    dropdown.highlighted = index;
                }
                (UiEventKind::Activate, Some(index)) => self.apply_pick(console, index),
                (UiEventKind::Activate, None) => self.dropdown = None,
                _ => {}
            }
            return SettingsResult::None;
        }
        let Some(token) = event.token else {
            return SettingsResult::None;
        };
        if token == crate::menu::classic::panel::SEARCH_TOKEN {
            if event.kind == UiEventKind::Activate {
                self.settle_numeric(console);
                self.editing = None;
                self.searching = true;
            }
            return SettingsResult::None;
        }
        if event.kind == UiEventKind::Activate {
            self.searching = false;
        }
        if let Some(slot) = crate::menu::classic::panel::chrome_slot(token) {
            return match event.kind {
                UiEventKind::Activate if self.classic.is_some() => SettingsResult::Classic(slot),
                _ => SettingsResult::None,
            };
        }
        // Classic+: the right button returns a row to its default.
        if secondary && event.kind == UiEventKind::Click && self.classic.is_some() {
            if let Some(row) = self.setting_row(token) {
                self.selected = row;
                self.reset_to_default(console, row);
            }
            return SettingsResult::None;
        }
        if event.kind == UiEventKind::Press {
            // SJK: pressing anything but the draft's own value field applies it.
            let own = self.numeric.as_ref().map(|edit| edit.row);
            if crate::menu_widgets::numeric::value_row(token) != own {
                self.settle_numeric(console);
            }
            self.press_elsewhere(self.setting_row(token));
        }
        if event.kind == UiEventKind::Activate {
            if let Some(row) = crate::menu_widgets::numeric::value_row(token) {
                if self.numeric.as_ref().is_none_or(|edit| edit.row != row) {
                    self.begin_numeric(console, row);
                }
                return SettingsResult::None;
            }
            self.numeric = None;
        } else if self.drafting() {
            return SettingsResult::None;
        }
        if let Some(classic) = self
            .classic
            .as_mut()
            .filter(|classic| classic.max_first() > 0)
        {
            // A classic panel with more rows than fit scrolls them.
            match event.kind {
                UiEventKind::Wheel => {
                    let direction = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
                    classic.scroll_by(direction * super::classic_view::CLASSIC_WHEEL_ROWS);
                    return SettingsResult::None;
                }
                UiEventKind::Drag if token == super::classic_view::CLASSIC_SCROLLBAR_TOKEN => {
                    let track = self
                        .ui
                        .rect_for(super::classic_view::CLASSIC_SCROLLBAR_TOKEN);
                    if let (Some(point), Some(track)) = (event.position, track) {
                        classic.scroll_to_ratio((point.y - track.y) / track.height);
                    }
                    return SettingsResult::None;
                }
                _ => {}
            }
        }
        if event.kind == UiEventKind::Wheel {
            self.wheel(event.delta.map_or(0, |delta| -delta.y.signum() as i32));
            return SettingsResult::None;
        }
        if matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover) {
            if let Some(row) = self.setting_row(token) {
                self.hover_row(row);
            }
            return SettingsResult::None;
        }
        if event.kind == UiEventKind::Drag {
            if crate::menu_widgets::numeric::value_row(token).is_some() {
                return SettingsResult::None;
            }
            if let (Some(row), Some(position)) = (self.setting_row(token), event.position) {
                self.selected = row;
                self.set_numeric_from_pointer(console, row, position.x);
            }
            return SettingsResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return SettingsResult::None;
        }
        // Classic+: a segment or a reset button acts at once.
        if let Some(control) = super::classic_view::RowControl::of(token) {
            self.activate_control(console, control);
            return SettingsResult::None;
        }
        match token {
            500.. if usize::from(token - 500) < self.tabs().len() => {
                self.select_tab(console, usize::from(token - 500));
            }
            900 => return self.back(),
            super::sjk_popup::ALL_SETTINGS_TOKEN if self.popup() => {
                return SettingsResult::AllSettings;
            }
            _ => {
                let Some(row) = self.setting_row(token) else {
                    return SettingsResult::None;
                };
                self.selected = row;
                // SJK UI: a click on a row's name chooses it; only a switch
                // flips from anywhere on its row.
                if let (Some(controls), Some(position)) = (self.sjk_controls, event.position)
                    && position.x < controls
                    && self
                        .rows()
                        .get(row)
                        .is_some_and(|setting| !matches!(setting.kind, ValueKind::Bool))
                {
                    return SettingsResult::None;
                }
                if let Some(setting) = self.rows().get(row) {
                    if matches!(setting.kind, ValueKind::Text) {
                        self.begin_text(console, row);
                    } else if matches!(setting.kind, ValueKind::Resolution) {
                        self.open_resolutions(console);
                    } else if matches!(setting.kind, ValueKind::HudPicker) {
                        self.open_hud_picker(console);
                    } else if matches!(setting.kind, ValueKind::WheelPages) {
                        return SettingsResult::OpenWheelPages;
                    } else if matches!(setting.kind, ValueKind::IdentityPage) {
                        return SettingsResult::OpenIdentity;
                    } else if matches!(setting.kind, ValueKind::AssetBrowser) {
                        return SettingsResult::OpenAssets;
                    } else if matches!(setting.kind, ValueKind::ImportPage) {
                        return SettingsResult::OpenImport;
                    } else if matches!(setting.kind, ValueKind::GameplayFps) {
                        return SettingsResult::OpenGameplayFps;
                    } else if self.classic.is_some() && self.open_dropdown(console, row) {
                        // Classic+: a choice opens its list; nothing changes yet.
                    } else if let Some(position) = event.position {
                        if !self.set_numeric_from_pointer(console, row, position.x) {
                            let direction = self.click_direction(setting.kind, row, position.x);
                            self.adjust(console, direction);
                        }
                    } else {
                        self.adjust(console, 1);
                    }
                }
            }
        }
        SettingsResult::None
    }

    /// Which way a click at `x` turns row `row`: a cycler steps the way the
    /// clicked half points, like its `<` and `>`; anything else steps on.
    fn click_direction(&self, kind: ValueKind, row: usize, x: f32) -> i32 {
        match (kind, self.ui.rect_for(row as u16)) {
            (ValueKind::Choice(_) | ValueKind::DisplayMode | ValueKind::Quality, Some(rect)) => {
                cycler_direction(rect, x) as i32
            }
            _ => 1,
        }
    }

    fn setting_row(&self, token: u16) -> Option<usize> {
        let row = crate::menu_widgets::numeric::value_row(token)
            .or_else(|| super::classic_view::RowControl::of(token).map(|control| control.row()))
            .unwrap_or(usize::from(token));
        (row < self.rows().len() && self.shows(row)).then_some(row)
    }

    fn set_numeric_from_pointer(
        &mut self,
        console: &mut ViewerConsole,
        row: usize,
        pointer_x: f32,
    ) -> bool {
        let Some(setting) = self.rows().get(row) else {
            return false;
        };
        let Some(rect) = self.ui.rect_for(row as u16) else {
            return false;
        };
        let ratio = f64::from(match &self.classic {
            Some(classic) => {
                crate::menu::classic::panel::slider_ratio(rect, pointer_x, classic.slider_span)
            }
            None => self.ui.slider_ratio(rect, pointer_x),
        });
        let raw = match setting.kind {
            ValueKind::Integer { min, max, .. } => min as f64 + (max - min) as f64 * ratio,
            ValueKind::Float { min, max, .. } => min + (max - min) * ratio,
            _ => return false,
        };
        let Some(value) = setting.kind.snapped(raw) else {
            return false;
        };
        console.set_cvar(setting.cvar, &value);
        self.refresh(console);
        true
    }
}

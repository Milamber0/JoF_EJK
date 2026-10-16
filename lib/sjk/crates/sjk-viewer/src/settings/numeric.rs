//! Direct numeric entry, separate from cvar navigation and text settings.
use super::*;
use crate::menu_widgets::numeric::{EditResult, NumericEdit};

impl SettingsMenu {
    pub(super) fn begin_numeric(&mut self, console: &ViewerConsole, row: usize) -> bool {
        let Some(setting) = self.rows().get(row) else {
            return false;
        };
        let (min, max, integer) = match setting.kind {
            ValueKind::Integer { min, max, .. } => (min as f64, max as f64, true),
            ValueKind::Float { min, max, .. } => (min, max, false),
            _ => return false,
        };
        self.editing = None;
        self.selected = row;
        self.numeric = Some(NumericEdit::new(
            row,
            value_text(console, setting.cvar),
            min,
            max,
            integer,
        ));
        true
    }

    /// SJK: typing a number on a selected slider opens entry with what was
    /// typed. False when the row is not a slider or `typed` cannot start a number.
    pub(super) fn begin_typed(&mut self, row: usize, typed: &str) -> bool {
        let Some(setting) = self.rows().get(row) else {
            return false;
        };
        let (min, max, integer) = match setting.kind {
            ValueKind::Integer { min, max, .. } => (min as f64, max as f64, true),
            ValueKind::Float { min, max, .. } => (min, max, false),
            _ => return false,
        };
        let Some(edit) = NumericEdit::typed(row, typed, min, max, integer) else {
            return false;
        };
        self.editing = None;
        self.selected = row;
        self.numeric = Some(edit);
        true
    }

    pub(super) fn edit_numeric(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        repeat: bool,
        console: &mut ViewerConsole,
    ) -> bool {
        let Some(edit) = &mut self.numeric else {
            return false;
        };
        match edit.key(key, text, repeat) {
            EditResult::Pending => {}
            EditResult::Cancel => self.numeric = None,
            EditResult::Commit(value) => {
                let row = edit.row;
                self.commit_numeric(row, value, console);
            }
        }
        true
    }

    /// SJK: a press away from an open draft applies it when it is a valid
    /// number and discards it otherwise.
    pub(super) fn settle_numeric(&mut self, console: &mut ViewerConsole) {
        let Some(edit) = self.numeric.take() else {
            return;
        };
        if let Some(value) = edit.committed() {
            self.commit_numeric(edit.row, value, console);
        }
    }

    fn commit_numeric(&mut self, row: usize, value: f64, console: &mut ViewerConsole) {
        self.numeric = None;
        let Some(setting) = self.rows().get(row) else {
            return;
        };
        let value = if matches!(setting.kind, ValueKind::Integer { .. }) {
            (value as i64).to_string()
        } else {
            value.to_string()
        };
        console.set_cvar(setting.cvar, &value);
        self.refresh(console);
    }
}

impl ValueKind {
    /// The cvar text a pointer at `raw` lands on: the nearest step counted from
    /// the minimum, clamped to the range. `None` for rows that are not sliders.
    pub(super) fn snapped(self, raw: f64) -> Option<String> {
        match self {
            Self::Integer { min, max, step } if min < 0 => {
                // One special value below zero (AUTO, `com_maxfps -1`) is the
                // rail's left end; the rest snaps to multiples of the step from 0.
                if raw < 0.0 {
                    return Some(min.to_string());
                }
                let step = step.max(1);
                let value = ((raw / step as f64).round() as i64).saturating_mul(step);
                Some(value.clamp(0, max).to_string())
            }
            Self::Integer { min, max, step } => {
                let step = step.max(1);
                let steps = ((raw - min as f64) / step as f64).round() as i64;
                let value = min.saturating_add(steps.saturating_mul(step));
                Some(value.clamp(min, max).to_string())
            }
            Self::Float { min, max, step } => {
                let value = if step > 0.0 {
                    ((raw - min) / step).round() * step + min
                } else {
                    raw
                };
                let places = decimals(step).max(decimals(min));
                Some(round_to(value, places).clamp(min, max).to_string())
            }
            _ => None,
        }
    }

    /// The cvar text one arrow-key step from `current` on a float row: exact as
    /// before (a typed 97.5 steps to 102.5), only without the binary noise that
    /// steps like 0.05 accumulate (0.1 + 0.05 is 0.15, not 0.15000000000000002).
    pub(super) fn stepped(self, current: f64, direction: i32) -> Option<String> {
        let Self::Float { min, max, step } = self else {
            return None;
        };
        let value = current + f64::from(direction) * step;
        let places = decimals(step).max(decimals(current));
        Some(round_to(value, places).clamp(min, max).to_string())
    }
}

/// Decimal places `value` needs, at most six: 0.05 needs 2, 5.0 needs 0.
fn decimals(value: f64) -> i32 {
    (0..6)
        .find(|places| {
            let shifted = value * 10f64.powi(*places);
            (shifted - shifted.round()).abs() < 1e-6
        })
        .unwrap_or(6)
}

fn round_to(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    const FADE: ValueKind = ValueKind::Float {
        min: 0.0,
        max: 1.0,
        step: 0.05,
    };

    #[test]
    fn dragging_a_fine_float_slider_writes_no_binary_noise() {
        // Seven steps of 0.05 accumulate to 0.35000000000000003.
        assert_eq!((7.0 * 0.05_f64).to_string(), "0.35000000000000003");
        assert_eq!(FADE.snapped(0.349).as_deref(), Some("0.35"));
        for steps in 0..=20 {
            let text = FADE.snapped(f64::from(steps) * 0.05).unwrap();
            assert!(text.len() <= 4, "{text}");
        }
    }

    #[test]
    fn dragging_still_snaps_and_clamps() {
        let fov = ValueKind::Float {
            min: 70.0,
            max: 130.0,
            step: 5.0,
        };
        assert_eq!(fov.snapped(97.4).as_deref(), Some("95"));
        assert_eq!(fov.snapped(500.0).as_deref(), Some("130"));
        let fps = ValueKind::Integer {
            min: 0,
            max: 2000,
            step: 25,
        };
        assert_eq!(fps.snapped(142.0).as_deref(), Some("150"));
        assert_eq!(ValueKind::Text.snapped(1.0), None);
    }

    #[test]
    fn arrow_steps_keep_typed_values_and_drop_noise() {
        assert_eq!(FADE.stepped(0.1, 1).as_deref(), Some("0.15"));
        assert_eq!(FADE.stepped(0.35, -1).as_deref(), Some("0.3"));
        assert_eq!(FADE.stepped(1.0, 1).as_deref(), Some("1"));
        let fov = ValueKind::Float {
            min: 70.0,
            max: 130.0,
            step: 5.0,
        };
        assert_eq!(fov.stepped(97.5, 1).as_deref(), Some("102.5"));
    }

    fn editing_row_two() -> SettingsMenu {
        let mut menu = SettingsMenu::new();
        menu.selected = 2;
        menu.editing = Some(TextDraft {
            row: 2,
            text: "master.example".to_owned(),
        });
        menu
    }

    #[test]
    fn hovering_or_wheeling_does_not_move_a_text_edit() {
        let mut menu = editing_row_two();
        menu.hover_row(0);
        menu.wheel(1);
        assert_eq!(menu.selected, 2);
        assert_eq!(menu.editing.as_ref().map(|draft| draft.row), Some(2));
    }

    #[test]
    fn pressing_another_row_discards_the_text_draft() {
        let mut menu = editing_row_two();
        menu.press_elsewhere(Some(2));
        assert!(menu.editing.is_some());
        menu.press_elsewhere(Some(0));
        assert!(menu.editing.is_none());
        let mut menu = editing_row_two();
        menu.press_elsewhere(None);
        assert!(menu.editing.is_none());
    }

    #[test]
    fn without_a_draft_the_pointer_selects_as_before() {
        let mut menu = SettingsMenu::new();
        menu.hover_row(1);
        assert_eq!(menu.selected, 1);
        menu.wheel(1);
        assert_eq!(menu.selected, 2);
    }
}

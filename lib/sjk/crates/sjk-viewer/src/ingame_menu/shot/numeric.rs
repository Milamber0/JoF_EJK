//! Direct entry shares the same clamping and preview path as dragging.
use super::*;
use crate::menu_widgets::numeric::{EditResult, NumericEdit, VALUE_BASE};
use winit::keyboard::KeyCode;

impl Panel {
    pub(super) fn begin_numeric(&mut self, row: usize) {
        let Some(&(_, min, max, _)) = SLIDERS.get(row) else {
            return;
        };
        if self.sun_tab && !self.sun_available && row != 6 {
            return;
        }
        if self.numeric.as_ref().is_some_and(|edit| edit.row == row) {
            return;
        }
        self.selected = VALUE_BASE + row as u16;
        self.numeric = Some(NumericEdit::new(
            row,
            self.values[row].to_string(),
            f64::from(min),
            f64::from(max),
            false,
        ));
    }

    /// SJK: typing a number on a selected slider opens entry with what was
    /// typed. False when the selection is no slider or `typed` cannot start one.
    pub(crate) fn begin_typed(&mut self, typed: &str) -> bool {
        let row = crate::menu_widgets::numeric::value_row(self.selected)
            .unwrap_or(self.selected as usize);
        let Some(&(_, min, max, _)) = SLIDERS.get(row) else {
            return false;
        };
        if self.sun_tab && !self.sun_available && row != 6 {
            return false;
        }
        let Some(edit) = NumericEdit::typed(row, typed, f64::from(min), f64::from(max), false)
        else {
            return false;
        };
        self.selected = VALUE_BASE + row as u16;
        self.numeric = Some(edit);
        true
    }

    /// Whether the selection is a slider, which Space steps instead of opening.
    pub(crate) fn selects_slider(&self) -> bool {
        crate::menu_widgets::numeric::value_row(self.selected).unwrap_or(self.selected as usize)
            < SLIDERS.len()
    }

    /// SJK: a press away from an open draft applies it when it is a valid
    /// number and discards it otherwise.
    pub(super) fn settle_numeric(&mut self) -> Option<Action> {
        let edit = self.numeric.take()?;
        let value = edit.committed()?;
        self.set_value(edit.row, value as f32)
    }

    pub(crate) fn edit_numeric(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        repeat: bool,
    ) -> Option<Action> {
        let edit = self.numeric.as_mut()?;
        let row = edit.row;
        match edit.key(key, text, repeat) {
            EditResult::Pending => None,
            EditResult::Cancel => {
                self.numeric = None;
                None
            }
            EditResult::Commit(value) => {
                self.numeric = None;
                self.set_value(row, value as f32)
            }
        }
    }
}

//! Direct entry for either saber's RGB sliders.
use super::*;
use crate::menu_widgets::numeric::{EditResult, NumericEdit};
use winit::keyboard::KeyCode;

impl PlayerMenu {
    pub(super) fn begin_numeric(&mut self, index: usize) -> bool {
        if self.page != ProfilePage::Saber {
            return false;
        }
        let Some(row) = self.saber_rows().get(index).copied() else {
            return false;
        };
        let Some(channel) = row.channel() else {
            return false;
        };
        self.selected = index;
        self.numeric = Some(NumericEdit::new(
            index,
            self.saber.channel(row.second(), channel).to_string(),
            0.0,
            255.0,
            true,
        ));
        true
    }

    /// SJK: typing a number on a selected RGB row opens entry with what was
    /// typed. False when the row has no channel or `typed` cannot start a number.
    pub(super) fn begin_typed(&mut self, index: usize, typed: &str) -> bool {
        if self.page != ProfilePage::Saber {
            return false;
        }
        let Some(row) = self.saber_rows().get(index).copied() else {
            return false;
        };
        if row.channel().is_none() {
            return false;
        }
        let Some(edit) = NumericEdit::typed(index, typed, 0.0, 255.0, true) else {
            return false;
        };
        self.selected = index;
        self.numeric = Some(edit);
        true
    }

    /// Whether `index` is an RGB row, which Space steps instead of opening.
    pub(super) fn is_channel_row(&self, index: usize) -> bool {
        self.page == ProfilePage::Saber
            && self
                .saber_rows()
                .get(index)
                .is_some_and(|row| row.channel().is_some())
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
        let index = edit.row;
        match edit.key(key, text, repeat) {
            EditResult::Pending => {}
            EditResult::Cancel => self.numeric = None,
            EditResult::Commit(value) => self.commit_numeric(index, value, console),
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

    fn commit_numeric(&mut self, index: usize, value: f64, console: &mut ViewerConsole) {
        if let Some(row) = self.saber_rows().get(index).copied()
            && let Some(channel) = row.channel()
        {
            self.saber.set_channel(row.second(), channel, value as u8);
            self.saber.apply(console);
        }
        self.numeric = None;
    }
}

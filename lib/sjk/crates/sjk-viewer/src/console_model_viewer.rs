//! Console side of the model viewer (see `model_viewer.rs`): the `modelviewer` toggle,
//! routing keys and pointer events to the page, and what it tells the stage: the model
//! chosen, and the list of models the menu's catalogue gives it.

use super::collection_panel::StageWish;
use super::model_viewer::{Entry, PanelAction};
use super::*;
use sjk_ui::InputEvent;

impl ViewerConsole {
    /// `modelviewer`: show the page, opening the console if needed, or close it (and the
    /// console, if the page opened it).
    pub(crate) fn toggle_model_viewer(&mut self) {
        if self.open && self.model_viewer.is_open() {
            self.close_model_viewer();
            return;
        }
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.update_panel.close();
        self.config_import.close();
        self.credits.close();
        self.identity_panel.close();
        self.profile_panel.close();
        self.staff_panel.close();
        self.collection_panel.close();
        self.holocrons_panel.close();
        self.sjk_chat_panel.close();
        self.asset_browser.open = false;
        self.dead_key.settle();
        self.model_viewer.open(owns_console);
    }

    fn close_model_viewer(&mut self) {
        if self.model_viewer.close() {
            self.set_open(false);
        }
    }

    fn model_viewer_action(&mut self, action: PanelAction) {
        if action == PanelAction::Close {
            self.close_model_viewer();
        }
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn model_viewer_key(&mut self, event: &KeyEvent) -> bool {
        if !self.model_viewer.is_open() {
            return false;
        }
        let action = self
            .model_viewer
            .handle_key(event, self.shift, self.control);
        self.model_viewer_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn model_viewer_pointer(&mut self, event: InputEvent) -> bool {
        if !self.model_viewer.is_open() {
            return false;
        }
        let action = self.model_viewer.handle_pointer(event);
        self.model_viewer_action(action);
        true
    }

    /// Draw the page with its text to `target`.
    pub(super) fn append_model_viewer(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        self.model_viewer.append_sjk(target, viewport);
    }

    /// Whether the page is open and still lacks its list of models.
    pub(crate) fn model_viewer_needs_models(&self) -> bool {
        self.open && self.model_viewer.is_open() && !self.model_viewer.has_models()
    }

    /// The menu's catalogue is read: list its models.
    pub(crate) fn set_model_viewer_models(&mut self, entries: Vec<Entry>) {
        self.model_viewer.set_models(entries);
    }

    /// The entry the page has chosen, while it shows.
    pub(crate) fn model_viewer_choice(&self) -> Option<&Entry> {
        if !self.open {
            return None;
        }
        self.model_viewer.choice()
    }

    /// The page wants the stage while it shows, whether or not a model is chosen yet.
    pub(super) fn model_viewer_wish(&self) -> Option<StageWish> {
        (self.open && self.model_viewer.is_open()).then_some(StageWish {
            skin: super::collection_panel::PreviewSkin::Worn,
            holocron: false,
        })
    }

    /// The page's draw list while it is shown.
    pub(super) fn model_viewer_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.model_viewer.is_open()).then(|| self.model_viewer.draw_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_opens_over_the_console_and_closes_the_console_it_took() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert!(console.model_viewer_wish().is_none());
        console.toggle_model_viewer();
        assert!(console.is_open());
        assert!(console.model_viewer_wish().is_some());
        assert!(console.model_viewer_needs_models());
        assert!(console.sjk_page_open(), "drawn in the SJK UI's look");
        assert!(console.covers_frame());
        console.set_model_viewer_models(vec![Entry {
            name: "kyle/default".into(),
            retail: true,
            kind: super::model_viewer::Kind::Player,
        }]);
        assert!(!console.model_viewer_needs_models());
        assert_eq!(
            console.model_viewer_choice().map(|e| e.name.as_str()),
            Some("kyle/default")
        );
        console.toggle_model_viewer();
        assert!(!console.is_open());
        assert!(console.model_viewer_choice().is_none());
    }
}

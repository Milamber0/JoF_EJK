//! Console side of the Holocrons page (see `holocrons_panel.rs`): opening it, asking the
//! hub for fresh progress, routing keys and pointer events to it, and saying what the 3D
//! holocron behind it shows, as for the Collection page. It is drawn in the SJK UI's look
//! whatever the menu style (`console_sjk_pages.rs`).

use super::holocrons_panel::{Data, PanelAction};
use super::*;
use sjk_ui::InputEvent;

/// The `holocrons` command.
pub(crate) const HOLOCRONS_COMMAND: &str = "holocrons";
pub(crate) const HOLOCRONS_HELP: &str =
    "Open your SJK holocrons: how many of each tier you hold, the recent ones and the odds";

impl ViewerConsole {
    /// `holocrons` in the classic menus: show the page, or close it.
    pub(crate) fn toggle_holocrons_panel(&mut self) {
        if self.open && self.holocrons_panel.is_open() {
            self.close_holocrons_panel();
            return;
        }
        self.open_holocrons_panel();
    }

    /// Show the page on its own (the Profile page's See holocrons in the classic menus).
    pub(crate) fn open_holocrons_panel(&mut self) {
        let owns_console = !self.open;
        self.show_holocrons_panel(owns_console);
    }

    /// Show the page on its own; closing it closes the console too when `owns_console`.
    /// The hub is asked for fresh progress (at most every 30 seconds).
    pub(super) fn show_holocrons_panel(&mut self, owns_console: bool) {
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
        self.sjk_chat_panel.close();
        self.collection_panel.close();
        self.dead_key.settle();
        self.holocrons_panel.open(owns_console);
        crate::holocrons::refresh();
    }

    fn close_holocrons_panel(&mut self) {
        if self.holocrons_panel.close() {
            self.set_open(false);
        }
    }

    fn holocrons_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_holocrons_panel(),
            PanelAction::Hub(tab) => {
                let target = self.profile_hub_return;
                self.open_profile_hub_page(tab, target);
            }
            PanelAction::Open(ids) => {
                if !crate::player_identity::open_holocrons(ids) {
                    self.holocrons_panel.refused(
                        "Holocrons need the SJK identity: switch it on in Settings, Network.",
                        std::time::Instant::now(),
                    );
                }
            }
        }
    }

    /// `debug_open`: show the Holocrons page and play the opening of made-up holocrons
    /// (`crate::cards::rehearsal`); alone, list the tiers. Nothing is sent.
    pub(crate) fn debug_open(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        if args.is_empty() {
            return Ok(crate::cards::rehearsal::listing());
        }
        let opened = crate::cards::rehearsal::parse(args)?;
        let shown: Vec<_> = opened
            .iter()
            .filter_map(super::holocrons_panel::opening::Prize::of)
            .collect();
        let count = shown.len();
        if !self.holocrons_panel.is_open() {
            self.open_holocrons_panel();
        }
        self.holocrons_panel.show(shown, std::time::Instant::now());
        Ok(vec![format!(
            "Opening {count} rehearsed holocron{} (nothing sent).",
            if count == 1 { "" } else { "s" }
        )])
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn holocrons_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.holocrons_panel.is_open() {
            return false;
        }
        let action = self.holocrons_panel.handle_key(event, self.shift);
        self.holocrons_panel_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn holocrons_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.holocrons_panel.is_open() {
            return false;
        }
        let action = self.holocrons_panel.handle_pointer(event);
        self.holocrons_panel_action(action);
        true
    }

    /// Draw the page with its text to `target`.
    pub(super) fn append_holocrons_panel(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        if self.holocrons_panel.is_hub() {
            let back = self.profile_hub_return;
            let name = self.text_value("name").unwrap_or_default();
            if !self.holocrons_panel.hub_header().is(back, name) {
                let header = crate::profile_hub::Header {
                    back,
                    name: name.to_owned(),
                };
                self.holocrons_panel.set_hub_header(header);
            }
        }
        let enabled = self.bool_cvar("cl_identity") == Some(true);
        if self.holocrons_panel.is_hub() {
            let snapshot = crate::player_identity::collection_snapshot(
                crate::debug_collectibles::wanted(self),
            );
            let held = snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.me.as_ref())
                .map(|me| me.achievements.clone())
                .unwrap_or_default();
            let standings = crate::achievements::standings(&held);
            let row = super::collection_panel::row_labels(enabled, snapshot.as_ref(), &standings);
            self.holocrons_panel.set_collection_row(row);
        }
        let now = std::time::Instant::now();
        self.holocrons_panel.refresh(now, || Data::read(enabled));
        // The opening's answer is looked for every frame only while the page waits.
        if self.holocrons_panel.opening() {
            let outcome = crate::player_identity::opening_outcome();
            self.holocrons_panel.follow_opening(now, outcome.as_ref());
        }
        self.holocrons_panel.append_sjk(target, viewport);
    }

    /// What the 3D holocron behind the page shows while it is open: the tier chosen and
    /// whether the player holds one. `None` while the page is closed.
    pub(crate) fn holocrons_stage(&self) -> Option<crate::holocrons::stage::Request> {
        (self.open && self.holocrons_panel.is_open()).then(|| self.holocrons_panel.stage_request())
    }

    /// Show `data` in place of the live identity's, whenever the page is read, for a world
    /// shot; the tier `index` chosen.
    #[cfg(test)]
    pub(crate) fn preview_holocrons(&mut self, data: Data, index: usize) {
        self.holocrons_panel.pin(data);
        self.holocrons_panel.select(index);
    }

    /// Play the ceremony of `shown` on the page, drawn `elapsed` seconds into its card's
    /// entrance (negative: into the crate spin before it) with its finish at `seconds`, for a world shot; `None` ends it.
    #[cfg(test)]
    pub(crate) fn preview_opening(
        &mut self,
        shown: Option<Vec<super::holocrons_panel::opening::Prize>>,
        elapsed: f32,
        seconds: f32,
    ) {
        self.holocrons_panel.shot_elapsed = Some(elapsed);
        self.holocrons_panel.shot_seconds = Some(seconds);
        match shown {
            Some(shown) => self.holocrons_panel.show(shown, std::time::Instant::now()),
            None => self.holocrons_panel.end_ceremony(),
        }
    }

    /// Show the opening's wait for the hub (`None`) or its refusal, for a world shot.
    #[cfg(test)]
    pub(crate) fn preview_opening_state(&mut self, refusal: Option<&str>) {
        let now = std::time::Instant::now();
        match refusal {
            Some(message) => self.holocrons_panel.refused(message, now),
            None => {
                let _ = self.holocrons_panel.ask(vec![1], now);
            }
        }
    }

    /// Show the loot table over the open page, or go back to it, for a world shot.
    #[cfg(test)]
    pub(crate) fn preview_loot(&mut self, show: bool) {
        self.holocrons_panel.show_loot(show);
    }

    /// Show the hover card of tier row `index` on the open page, for a world shot.
    #[cfg(test)]
    pub(crate) fn holocron_hover_for_shot(&mut self, index: Option<usize>) {
        self.holocrons_panel.hover_for_shot(index);
    }

    /// Choose tier `index` on the open page, for a world shot.
    #[cfg(test)]
    pub(crate) fn select_holocron(&mut self, index: usize) {
        self.holocrons_panel.select(index);
    }

    /// Whether the page's last frame ran out of room on its canvas, for the world shots.
    #[cfg(test)]
    pub(crate) fn holocrons_page_overflowed(&self) -> bool {
        self.holocrons_panel.overflowed()
    }

    /// The page's draw list while it is shown.
    pub(super) fn holocrons_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.holocrons_panel.is_open()).then(|| self.holocrons_panel.draw_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_opens_alone_asks_for_the_stage_and_closes() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert!(
            console.holocrons_stage().is_none(),
            "no stage while it is closed"
        );
        console.open_profile_panel();
        console.toggle_holocrons_panel();
        assert!(console.holocrons_panel.is_open());
        assert!(!console.profile_panel.is_open());
        assert!(console.sjk_page_open(), "drawn in the SJK UI's look");
        assert!(console.covers_frame());
        let request = console
            .holocrons_stage()
            .expect("the stage while it is open");
        assert_eq!(request.tier, 0);
        assert!(!request.owned, "nothing is known: shown dimmed, not hidden");
        console.select_holocron(2);
        assert_eq!(
            console.holocrons_stage().map(|request| request.tier),
            Some(2)
        );
        // It was opened with the console already open (the Profile page's).
        console.holocrons_panel_action(PanelAction::Close);
        assert!(!console.holocrons_panel.is_open());
        assert!(console.is_open());
        assert!(console.holocrons_stage().is_none());
        // From a closed console it closes the console it took over.
        console.set_open(false);
        console.toggle_holocrons_panel();
        assert!(console.is_open());
        console.toggle_holocrons_panel();
        assert!(!console.is_open(), "it closes the console it took over");
    }

    /// Another page opened over it, or the console closing, takes it away: nothing of it
    /// (and no stage) is left behind.
    #[test]
    fn another_page_replaces_it_and_the_stage_goes() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        console.open_holocrons_panel();
        console.toggle_collection_panel(Some(crate::profile_hub::Tab::Shaders));
        assert!(!console.holocrons_panel.is_open());
        assert!(console.holocrons_stage().is_none());
        console.open_holocrons_panel();
        assert!(!console.collection_panel.is_open());
        console.set_open(false);
        assert!(!console.holocrons_panel.is_open());
        assert!(console.holocrons_stage().is_none());
    }
}

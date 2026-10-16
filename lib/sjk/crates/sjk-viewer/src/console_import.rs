//! Console side of the Import page (see `config_import_panel.rs`): opening it on a
//! dropped, browsed or found file, copying the ticked parts into the profile, setting
//! the gameplay FPS asked after it (or on its own, `gameplayfps`), and routing keys
//! and pointer events to it while it is open, as for the Update page.

use super::config_import_panel::{Browsed, FpsInfo, PanelAction};
use super::*;
use crate::config_import::{self, COLOR_CVARS, Found, Item};
use sjk_ui::InputEvent;
use std::path::Path;

impl ViewerConsole {
    /// Show the Import page with what `path` holds, or, without a file, how to
    /// give one; opens the console if needed.
    pub(crate) fn open_config_import(&mut self, path: Option<&Path>) {
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.credits.close();
        self.update_panel.close();
        self.profile_panel.close();
        self.collection_panel.close();
        self.holocrons_panel.close();
        self.staff_panel.close();
        self.sjk_chat_panel.close();
        self.identity_panel.close();
        self.dead_key.settle();
        let file = path
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(path) = path {
            self.shell.push_log(format!("Import: {}", path.display()));
        }
        self.config_import
            .open(owns_console, file, path.map(config_import::read));
    }

    /// First setup's "Import from another client" row: the Import page with the
    /// configs found from other clients, or the file dialog over it when none are.
    pub(crate) fn browse_config_import(&mut self) {
        self.open_config_import(None);
        if !self.config_import.has_detected() {
            self.config_import.browse();
        }
    }

    /// The game folder, where the Import page looks for other clients' configs.
    pub(crate) fn set_game_data(&mut self, game_data: &Path) {
        self.config_import.set_game_data(game_data);
    }

    /// Show the gameplay FPS step on its own: `gameplayfps` and the Gameplay FPS
    /// setting. Opens the console if needed.
    pub(crate) fn open_gameplay_fps(&mut self) {
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.dead_key.settle();
        let choice = crate::gameplay_fps::Choice::new(self.gameplay_fps(), None);
        let info = self.fps_info();
        self.config_import.open_fps(owns_console, choice, info);
    }

    /// The server has not been taking the inputs at this step
    /// ([`crate::input_health`]): the gameplay FPS card, saying so, with 125
    /// suggested; and a line in the console.
    pub(crate) fn open_gameplay_fps_alert(&mut self) {
        let made = 1000 / self.command_step().max(1);
        let alert = format!(
            "Your inputs are not going through on this server at {made} a second: it takes \
             fewer. 125, what JoF EJK sends, works everywhere."
        );
        self.shell.push_log(format!("^3{alert}"));
        self.open_gameplay_fps();
        self.config_import
            .alert_fps(alert, sjk_client::command_rate::DEFAULT_GAMEPLAY_FPS);
    }

    /// The FPS cap and the server's fixed step, for the gameplay FPS step.
    fn fps_info(&self) -> FpsInfo {
        let cap = match self.integer_cvar("com_maxfps") {
            Some(0) => "uncapped".to_owned(),
            Some(cap) if cap > 0 => cap.to_string(),
            _ => "AUTO".to_owned(),
        };
        FpsInfo {
            cap,
            server_fixed: self.pmove_fixed_msec().or(self.server_step_floor()),
            alert: None,
            cap_name: self
                .pmove_fixed_msec()
                .is_none()
                .then(|| self.server_step_floor().and(self.server_cap()))
                .flatten()
                .map(|cap| cap.name),
        }
    }

    /// Set the gameplay FPS; the line saying to what.
    pub(crate) fn set_gameplay_fps(&mut self, fps: i64) -> Option<String> {
        if !self.set_cvar(crate::gameplay_fps::CVAR, &fps.to_string()) {
            return None;
        }
        Some(format!(
            "Gameplay FPS: {}",
            crate::gameplay_fps::describe(fps)
        ))
    }

    /// Read the file the dialog gave, as a dropped one; called every frame the page
    /// shows.
    pub(super) fn sync_config_browse(&mut self) {
        match self.config_import.take_browsed() {
            Some(Browsed::File(path)) => self.open_config_import(Some(&path)),
            Some(Browsed::Nothing(Some(reason))) => self.config_import.fail(reason),
            Some(Browsed::Nothing(None)) | None => {}
        }
    }

    fn close_config_import(&mut self) {
        if self.config_import.close() {
            self.set_open(false);
        }
    }

    /// Copy the chosen parts of `found` into the profile; one line per part
    /// for the page.
    fn import_config(&mut self, found: &Found, items: &[Item]) -> Vec<String> {
        let mut lines = Vec::with_capacity(items.len());
        for item in items {
            match item {
                Item::Name => {
                    if let Some(name) = &found.name
                        && self.set_cvar("name", name)
                    {
                        lines.push(format!("Name: {name}"));
                    }
                }
                Item::Model => {
                    if let Some(model) = &found.model
                        && self.set_cvar("model", model)
                    {
                        for (cvar, value) in COLOR_CVARS.iter().zip(&found.colors) {
                            if let Some(value) = value {
                                self.set_cvar(cvar, value);
                            }
                        }
                        lines.push(format!("Model: {model}"));
                    }
                }
                Item::Fov => {
                    if let Some(fov) = &found.fov
                        && self.set_cvar("cg_fov", fov)
                    {
                        lines.push(format!("Field of view: {fov}"));
                    }
                }
                Item::FpsCap => {
                    if let Some(fps) = found.max_fps
                        && self.set_cvar("com_maxfps", &fps.to_string())
                    {
                        lines.push(format!("FPS cap: {fps}"));
                    }
                }
                Item::Binds => {
                    let count = self.import_binds(found);
                    lines.push(format!(
                        "Key bindings: {count} key{}{}",
                        if count == 1 { "" } else { "s" },
                        if found.whole_table {
                            ", the rest unbound"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        for line in &lines {
            self.shell.push_log(format!("Imported {line}"));
        }
        lines
    }

    /// Bind the file's keys; returns how many were bound. A whole table (the
    /// file starts with `unbindall`) replaces the player's, keeping the keys the
    /// editor locks and giving SJK's own actions their default keys where the
    /// file left those free; a partial file only adds its keys.
    fn import_binds(&mut self, found: &Found) -> usize {
        let mut binds = self.shell.binds.clone();
        if found.whole_table {
            let locked: Vec<(String, String)> = binds
                .iter()
                .filter(|(key, _)| keybind_editor::is_locked_key(key))
                .map(|(key, command)| (key.to_owned(), command.to_owned()))
                .collect();
            binds.clear();
            for (key, command) in locked {
                let _ = binds.bind(&key, command);
            }
        }
        let mut count = 0;
        for (key, script) in &found.binds {
            if !keybind_editor::is_locked_key(key) && binds.bind(key, script.clone()).is_ok() {
                count += 1;
            }
        }
        if found.whole_table {
            keybind_editor::migrate_missing_defaults(&mut binds);
        }
        self.shell.binds = binds;
        self.persist();
        count
    }

    fn config_import_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_config_import(),
            PanelAction::ImportDetected(path) => self.open_config_import(Some(&path)),
            // Copy the ticked parts, then ask for the gameplay FPS with the file's
            // old frame rate picked; a finished page's Done closes it.
            PanelAction::Import => {
                let chosen = self
                    .config_import
                    .chosen()
                    .map(|(found, items)| (found.clone(), items));
                let Some((found, items)) = chosen else {
                    self.close_config_import();
                    return;
                };
                let origin = self.config_import.origin();
                let lines = self.import_config(&found, &items);
                let choice = crate::gameplay_fps::Choice::new(self.gameplay_fps(), origin);
                let info = self.fps_info();
                self.config_import.ask_fps(lines, choice, info);
            }
            PanelAction::UseFps(fps) => {
                let line = self.set_gameplay_fps(fps);
                if let Some(line) = &line {
                    self.shell.push_log(line.clone());
                }
                if self.config_import.fps_done(line) {
                    self.close_config_import();
                }
            }
        }
    }

    /// Import the loaded file's ticked parts, as Enter does, for the world shots.
    #[cfg(test)]
    pub(crate) fn import_for_shot(&mut self) {
        self.config_import_action(PanelAction::Import);
    }

    /// Use the gameplay FPS chosen, as Enter does, for the world shots.
    #[cfg(test)]
    pub(crate) fn config_import_done_for_shot(&mut self) {
        let action = self.config_import.primary_for_shot();
        self.config_import_action(action);
    }

    /// Type `digits` on the gameplay FPS step, for the world shots.
    #[cfg(test)]
    pub(crate) fn type_gameplay_fps_for_shot(&mut self, digits: &str) {
        self.config_import.type_fps_for_shot(digits);
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn config_import_key(&mut self, event: &KeyEvent) -> bool {
        if !self.config_import.is_open() {
            return false;
        }
        let action = self.config_import.handle_key(event);
        self.config_import_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn config_import_pointer(&mut self, event: InputEvent) -> bool {
        if !self.config_import.is_open() {
            return false;
        }
        let action = self.config_import.handle_pointer(event);
        self.config_import_action(action);
        true
    }

    /// The page's draw list while it is shown.
    pub(super) fn config_import_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.config_import.is_open()).then(|| self.config_import.draw_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_import::parse;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    #[test]
    fn the_chosen_parts_are_copied_and_the_others_left_alone() {
        let (_directory, mut console) = console();
        let found = parse(
            "seta name \"^1Sol\"\nseta model \"tavion/default\"\nseta char_color_red 10\n\
             seta cg_fov 105\n",
        );
        let lines = console.import_config(&found, &[Item::Name, Item::Model]);
        assert_eq!(lines.len(), 2);
        assert_eq!(console.text_cvar("name").unwrap(), "^1Sol");
        assert_eq!(console.text_cvar("model").unwrap(), "tavion/default");
        assert_eq!(console.integer_cvar("char_color_red"), Some(10));
        assert_ne!(console.float_cvar("cg_fov"), Some(105.0));
    }

    #[test]
    fn a_whole_table_replaces_the_binds_but_keeps_locked_keys_and_free_defaults() {
        let (_directory, mut console) = console();
        let escape = console.shell.binds.get("ESCAPE").map(str::to_owned);
        let found = parse("unbindall\nbind w \"+back\"\nbind ESCAPE \"quit\"\n");
        console.import_config(&found, &[Item::Binds]);
        assert_eq!(console.shell.binds.get("w"), Some("+back"));
        assert_eq!(console.shell.binds.get("ESCAPE").map(str::to_owned), escape);
        // +forward lost its key to the file, so it is not given one back on W.
        assert!(console.keys_for_command("+forward").is_empty());
    }

    #[test]
    fn the_fps_cap_is_copied_and_the_gameplay_fps_set_apart() {
        let (_directory, mut console) = console();
        let found = parse("seta com_maxfps 333\n");
        let lines = console.import_config(&found, &[Item::FpsCap]);
        assert_eq!(lines, ["FPS cap: 333"]);
        assert_eq!(console.integer_cvar("com_maxfps"), Some(333));
        assert_eq!(
            console.gameplay_fps(),
            0,
            "Auto: the movement is asked apart"
        );
        // Auto follows the FPS cap (AUTO's 125 until the frame pacer says more).
        assert_eq!(console.command_step(), 8);
        console.set_frame_cap(333);
        assert_eq!(console.command_step(), 3);
        console.set_frame_cap(125);
        let line = console.set_gameplay_fps(333).unwrap();
        assert_eq!(line, "Gameplay FPS: 333 (3 ms steps)");
        assert_eq!(console.command_step(), 3);
        // A pmove_fixed server's step wins.
        console
            .shell
            .cvars
            .restore_text("pmove_fixed", "1")
            .unwrap();
        assert_eq!(console.command_step(), 8);
    }

    #[test]
    fn a_known_server_cap_holds_unless_lifted() {
        let (_directory, mut console) = console();
        console.set_frame_cap(0);
        assert_eq!(console.command_step(), 1, "Auto uncapped");
        console.server_cap = crate::server_caps::find(&sjk_mod::Server {
            kind: sjk_mod::ServerKind::JaPlus,
            address: "127.0.0.1:29070".into(),
            info: "\\V\\2.5B0".into(),
        });
        assert_eq!(console.command_step(), 8, "JoF takes 125 a second");
        assert!(console.set_cvar(crate::server_caps::OVERRIDE_CVAR, "1"));
        assert_eq!(console.command_step(), 1, "lifted by the player");
        assert_eq!(console.server_step_floor(), None);
    }

    #[test]
    fn a_partial_file_only_adds_its_keys() {
        let (_directory, mut console) = console();
        let before = console.shell.binds.iter().count();
        let found = parse("bind F11 \"say hi\"\n");
        console.import_config(&found, &[Item::Binds]);
        assert_eq!(console.shell.binds.get("F11"), Some("say hi"));
        assert!(console.shell.binds.iter().count() >= before);
    }
}

//! Local demo record/play command routing.

use crate::{GpuState, demo_playback};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Deferred local action produced by a demo console command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// Begin recording, using the stock timestamp name when absent.
    Record(Option<String>),
    /// Finish an active recording.
    StopRecord,
    /// Tear down the current session and play this demo.
    Play(String),
    /// Replay the last console-opened demo from its initial gamestate.
    Restart,
    /// Delete a recording from the writable demo directory only.
    Delete(String),
}

/// Parse commands registered by `CL_Init` at
/// `codemp/client/cl_main.cpp:2871-2874`.
pub(crate) fn parse(tokens: &[String]) -> Result<Option<Action>, String> {
    let Some(command) = tokens.first() else {
        return Ok(None);
    };
    if command.eq_ignore_ascii_case("record") {
        if tokens.len() > 2 {
            return Err("usage: record [demoname]".to_owned());
        }
        return Ok(Some(Action::Record(tokens.get(1).cloned())));
    }
    if command.eq_ignore_ascii_case("stoprecord") {
        return exact(tokens, Action::StopRecord, "usage: stoprecord");
    }
    if command.eq_ignore_ascii_case("demo") || command.eq_ignore_ascii_case("playdemo") {
        if tokens.len() != 2 {
            return Err("usage: demo <demoname>".to_owned());
        }
        return Ok(Some(Action::Play(tokens[1].clone())));
    }
    if command.eq_ignore_ascii_case("demo_restart") {
        return exact(tokens, Action::Restart, "usage: demo_restart");
    }
    if command.eq_ignore_ascii_case("deletedemo") {
        if tokens.len() != 2 {
            return Err("usage: deletedemo <demoname>".into());
        }
        return Ok(Some(Action::Delete(tokens[1].clone())));
    }
    Ok(None)
}

fn exact(tokens: &[String], action: Action, usage: &str) -> Result<Option<Action>, String> {
    if tokens.len() == 1 {
        Ok(Some(action))
    } else {
        Err(usage.to_owned())
    }
}

impl GpuState {
    pub(crate) fn apply_console_demo_action(&mut self, action: Action) {
        match action {
            Action::Record(name) => self.start_console_recording(name.as_deref()),
            Action::StopRecord => self.stop_console_recording(),
            Action::Play(name) => self.start_console_demo(&name),
            Action::Restart => {
                let name = self
                    .console
                    .as_ref()
                    .and_then(|console| console.last_console_demo())
                    .map(str::to_owned);
                if let Some(name) = name {
                    self.start_console_demo(&name);
                } else if let Some(console) = &mut self.console {
                    console.push_log("No demo available to restart.");
                }
            }
            Action::Delete(name) => {
                if self
                    .live_session
                    .as_ref()
                    .is_some_and(|s| s.is_recording_demo())
                {
                    if let Some(console) = &mut self.console {
                        console.push_log("Stop recording before deleting demos.");
                    }
                    return;
                }
                if let Some(console) = &mut self.console {
                    let result = delete_demo(console.config_directory(), &name);
                    console.push_log(match result {
                        Ok(()) => format!("Deleted demo {name}; deletion is permanent."),
                        Err(error) => format!("^1{error}"),
                    });
                }
            }
        }
    }

    fn start_console_recording(&mut self, name: Option<&str>) {
        let Some(console) = self.console.as_ref() else {
            return;
        };
        let config_directory = console.config_directory().to_owned();
        let result = self
            .live_session
            .as_mut()
            .ok_or_else(|| "You must be in a level to record.".to_owned())
            .and_then(|session| {
                session
                    .start_demo_recording(&config_directory, name, SystemTime::now())
                    .map(|path| path.to_owned())
                    .map_err(|error| error.to_string())
            });
        if let Some(console) = &mut self.console {
            match result {
                Ok(path) => console.push_log(format!("recording to {}", path.display())),
                Err(error) => console.push_log(format!("^1{error}")),
            }
        }
    }

    fn stop_console_recording(&mut self) {
        let result = self
            .live_session
            .as_mut()
            .map(|session| session.stop_demo_recording())
            .transpose();
        if let Some(console) = &mut self.console {
            match result {
                Ok(Some(Some(path))) => {
                    console.push_log(format!("Stopped demo: {}", path.display()));
                }
                Ok(Some(None)) | Ok(None) => console.push_log("Not recording a demo."),
                Err(error) => console.push_log(format!("^1{error}")),
            }
        }
    }

    fn start_console_demo(&mut self, name: &str) {
        let Some(config) = self
            .console
            .as_ref()
            .map(|console| console.config_directory().to_owned())
        else {
            return;
        };
        let path = match resolve_demo_path(&config, &self.game_data, name) {
            Ok(path) => path,
            Err(error) => {
                if let Some(console) = &mut self.console {
                    console.push_log(format!("^1{error}"));
                }
                return;
            }
        };
        let session =
            match demo_playback::Session::open(&path, demo_playback::Camera::FirstPerson, 60) {
                Ok(session) => session,
                Err(error) => {
                    if let Some(console) = &mut self.console {
                        console.push_log(format!("^1Could not play demo: {error}"));
                    }
                    return;
                }
            };
        self.leave_session();
        self.demo_session = Some(session);
        self.pending_map_reload = true;
        self.game_menu = false;
        if let Some(console) = &mut self.console {
            console.push_log(format!("Playing demo {}", path.display()));
            console.remember_console_demo(name);
            console.close_for_connection();
        }
        if let Some(menu) = &mut self.client_menu {
            menu.joined();
        }
    }
}

fn delete_demo(config: &Path, name: &str) -> Result<(), String> {
    let name = name.strip_prefix("demos/").unwrap_or(name);
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err("Expected a plain demo name".into());
    }
    let filename = if name.to_ascii_lowercase().ends_with(".dm_26") {
        name.to_owned()
    } else {
        format!("{name}.dm_26")
    };
    let directory = config.join("demos");
    if directory
        .symlink_metadata()
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Refusing a symlinked demo directory".into());
    }
    let path = directory.join(filename);
    let metadata = path.symlink_metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Refusing a non-regular demo file".into());
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

/// Resolve `demo` through the writable homepath before the GameData search path.
///
/// This mirrors the logical `demos/<name>.dm_26` lookup in `CL_PlayDemo_f`
/// (`codemp/client/cl_main.cpp:514-566`) while preserving explicit filesystem
/// ownership in the viewer.
pub(crate) fn resolve_demo_path(
    config_directory: &Path,
    game_data: &Path,
    requested: &str,
) -> Result<PathBuf, String> {
    let trimmed = requested.trim();
    if trimmed.is_empty() || trimmed.contains(['/', '\\']) {
        return Err("usage: demo <demoname>".to_owned());
    }
    let filename = if trimmed.to_ascii_lowercase().ends_with(".dm_26") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}.dm_26")
    };
    [
        config_directory.join("demos").join(&filename),
        game_data.join("base/demos").join(&filename),
        game_data.join("demos").join(&filename),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| format!("couldn't open demos/{filename}"))
}

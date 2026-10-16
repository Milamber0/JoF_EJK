//! Deferred, off-thread saving of the config file.
//!
//! Saving used to run after every buffered command frame, so each bound key press
//! rewrote the whole file on the main thread. Now a frame only compares two
//! stamps ([`crate::CvarRegistry::archive_revision`], [`crate::BindTable::revision`]);
//! once they have held still for [`CONFIG_SAVE_DELAY`], the text is built on the
//! caller's thread and written by one background writer. Jobs run in the order they
//! were queued, so an older text never lands after a newer one, and a synchronous
//! save first waits for the writer to finish.

use crate::ConfigError;
use crate::config::write_config_text;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long the archived state must stay unchanged before it is saved: a burst of
/// changes (a slider dragged, a script setting many cvars) is written once.
pub const CONFIG_SAVE_DELAY: Duration = Duration::from_secs(2);

/// The cvar and bind stamps together: what the file holds, in two numbers.
pub(crate) type Revision = (u64, u64);

#[derive(Default)]
pub(crate) struct ConfigSaver {
    /// The revision on disk or queued for it; `None` until loaded or saved.
    saved: Option<Revision>,
    /// The unsaved revision last seen and since when it held still.
    seen: Option<(Revision, Instant)>,
    writer: Option<Writer>,
}

impl ConfigSaver {
    pub(crate) fn is_dirty(&self, current: Revision) -> bool {
        self.saved != Some(current)
    }

    /// Record that the file holds `current` (after a load or a synchronous save).
    pub(crate) fn mark_saved(&mut self, current: Revision) {
        self.saved = Some(current);
        self.seen = None;
    }

    /// Whether `current` has held still long enough to be saved at `now`.
    pub(crate) fn due(&mut self, current: Revision, now: Instant) -> bool {
        if !self.is_dirty(current) {
            self.seen = None;
            return false;
        }
        match self.seen {
            Some((seen, since)) if seen == current => {
                now.saturating_duration_since(since) >= CONFIG_SAVE_DELAY
            }
            _ => {
                self.seen = Some((current, now));
                false
            }
        }
    }

    /// Hand `contents` to the background writer; written in place if no thread
    /// can be started.
    pub(crate) fn queue(
        &mut self,
        current: Revision,
        path: PathBuf,
        contents: String,
    ) -> Result<(), ConfigError> {
        self.mark_saved(current);
        if self.writer.is_none() {
            self.writer = Writer::start();
        }
        match &self.writer {
            Some(writer) => {
                writer.send(path, contents);
                Ok(())
            }
            None => write_config_text(&path, &contents),
        }
    }

    /// Wait until every queued write has finished, then return the first error a
    /// background write met since the last call, if any.
    pub(crate) fn wait_idle(&mut self) -> Result<(), ConfigError> {
        match &self.writer {
            Some(writer) => writer.wait_idle(),
            None => Ok(()),
        }
    }

    /// The first error a background write met since the last call, if any.
    pub(crate) fn take_error(&mut self) -> Option<ConfigError> {
        self.writer.as_ref().and_then(Writer::take_error)
    }
}

struct Job {
    path: PathBuf,
    contents: String,
}

#[derive(Default)]
struct Progress {
    queued: usize,
    error: Option<ConfigError>,
}

#[derive(Default)]
struct Shared {
    progress: Mutex<Progress>,
    idle: Condvar,
}

struct Writer {
    sender: Option<Sender<Job>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    fn start() -> Option<Self> {
        let (sender, receiver) = mpsc::channel::<Job>();
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("config-save".into())
            .spawn(move || {
                for job in receiver {
                    let result = write_config_text(&job.path, &job.contents);
                    let mut progress = worker.progress.lock().unwrap_or_else(|e| e.into_inner());
                    progress.queued -= 1;
                    if let Err(error) = result {
                        progress.error.get_or_insert(error);
                    }
                    worker.idle.notify_all();
                }
            })
            .ok()?;
        Some(Self {
            sender: Some(sender),
            shared,
            thread: Some(thread),
        })
    }

    fn progress(&self) -> std::sync::MutexGuard<'_, Progress> {
        self.shared
            .progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn send(&self, path: PathBuf, contents: String) {
        self.progress().queued += 1;
        let sent = self.sender.as_ref().is_some_and(|sender| {
            sender
                .send(Job {
                    path: path.clone(),
                    contents,
                })
                .is_ok()
        });
        if !sent {
            // The writer is gone: nothing will count this job down.
            let mut progress = self.progress();
            progress.queued -= 1;
            progress
                .error
                .get_or_insert(ConfigError::Io(std::io::Error::other(format!(
                    "the config writer stopped before saving {}",
                    path.display()
                ))));
        }
    }

    fn wait_idle(&self) -> Result<(), ConfigError> {
        let mut progress = self.progress();
        while progress.queued > 0 {
            progress = self
                .shared
                .idle
                .wait(progress)
                .unwrap_or_else(|e| e.into_inner());
        }
        progress.error.take().map_or(Ok(()), Err)
    }

    fn take_error(&self) -> Option<ConfigError> {
        self.progress().error.take()
    }
}

impl Drop for Writer {
    /// Let queued writes finish: the process may exit right after.
    fn drop(&mut self) {
        self.sender = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CONFIG_SAVE_DELAY;
    use crate::{BindTable, CvarDefinition, CvarFlags, CvarRegistry, Shell};
    use std::time::Instant;

    fn shell(path: &std::path::Path) -> Shell {
        let mut cvars = CvarRegistry::new();
        cvars
            .register(CvarDefinition::new(
                "kept",
                1_i64,
                CvarFlags::ARCHIVE,
                "saved",
            ))
            .unwrap();
        cvars
            .register(CvarDefinition::new(
                "scratch",
                1_i64,
                CvarFlags::NONE,
                "not saved",
            ))
            .unwrap();
        let mut binds = BindTable::new();
        binds.bind("w", "+forward").unwrap();
        let mut shell = Shell::new(cvars, binds);
        shell.set_config_path(path);
        shell.load().unwrap();
        shell
    }

    fn scratch_path(test: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "sjk-shell-config-save-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory.join("config.cfg")
    }

    #[test]
    fn only_saved_state_makes_the_config_dirty() {
        let path = scratch_path("dirty");
        let mut shell = shell(&path);
        // Nothing on disk yet: the defaults count as loaded.
        assert!(!shell.config_dirty());
        shell.execute_line("scratch 5").unwrap();
        shell.binds.commands_for_event("w", true).unwrap();
        shell.binds.bind("w", "+forward").unwrap();
        assert!(!shell.config_dirty());
        shell.execute_line("kept 2").unwrap();
        assert!(shell.config_dirty());
        shell.save().unwrap();
        assert!(!shell.config_dirty());
        shell.execute_line("unbind w").unwrap();
        assert!(shell.config_dirty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn autosave_waits_for_the_state_to_hold_still_then_writes_once() {
        let path = scratch_path("autosave");
        let mut shell = shell(&path);
        let start = Instant::now();
        shell.execute_line("kept 4").unwrap();
        shell.autosave(start).unwrap();
        shell.autosave(start + CONFIG_SAVE_DELAY / 2).unwrap();
        assert!(shell.config_dirty());
        assert!(!path.exists());
        shell.autosave(start + CONFIG_SAVE_DELAY).unwrap();
        assert!(!shell.config_dirty());
        shell.save_if_dirty().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("seta kept \"4\""), "{text}");
        std::fs::remove_file(&path).unwrap();
        shell.autosave(start + CONFIG_SAVE_DELAY * 4).unwrap();
        shell.save_if_dirty().unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

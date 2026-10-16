//! Optional application-owned console transcript, with no implicit filesystem location.
use super::Shell;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

#[derive(Default)]
/// Lazy transcript sink owned by one shell, never shared across threads.
pub(super) struct FileLog {
    path: Option<PathBuf>,
    file: Option<File>,
    failed: bool,
}

impl Shell {
    /// Set an explicit transcript destination; no file opens until logging is enabled.
    pub fn set_log_path(&mut self, path: impl Into<PathBuf>) {
        self.file_log = FileLog {
            path: Some(path.into()),
            ..FileLog::default()
        };
    }
}

impl FileLog {
    /// Append one line when enabled, reporting a failed destination only once.
    pub(super) fn write(&mut self, mode: i64, text: &str) {
        if mode == 0 {
            return;
        }
        if self.failed || self.path.is_none() {
            return;
        }
        if let Err(error) = self.try_write(mode, text) {
            self.failed = true;
            eprintln!("console file logging stopped: {error}");
        }
    }

    fn try_write(&mut self, mode: i64, text: &str) -> std::io::Result<()> {
        if self.file.is_none() {
            let path = self.path.as_ref().expect("checked by write");
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            self.file = Some(File::create(path)?);
        }
        let file = self.file.as_mut().expect("opened above");
        writeln!(file, "{text}")?;
        if mode > 1 {
            file.sync_data()?;
        }
        Ok(())
    }
}

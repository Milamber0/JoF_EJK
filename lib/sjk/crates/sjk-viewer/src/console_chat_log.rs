//! Opt-in chat persistence. Only incoming chat events perform I/O, never HUD frames.
use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Owned log handle and sticky failure latch; retry after disabling logging.
#[derive(Default)]
pub(super) struct ChatLog {
    file: Option<File>,
    failed: bool,
}

/// TaystJK cl_main.cpp:3456 and cl_cgame.cpp:563-580, 664-675.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        "cl_logChat",
        0_i64,
        CvarFlags::ARCHIVE,
        "Append received chat to chatlogs/chat.log; 2 synchronizes each entry",
    ))
}

impl ViewerConsole {
    /// Log live chat before display filtering, with no audio or network side effects.
    pub(crate) fn log_chat(&mut self, kind: &sjk_client::ServerEventKind, text: &str) {
        use sjk_client::ServerEventKind::{Chat, TeamChat};
        if !matches!(kind, Chat | TeamChat) {
            return;
        }
        let mode = self.integer_cvar("cl_logchat").unwrap_or(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Err(error) = self
            .chat_log
            .append(&self.config_directory, mode, timestamp, text)
        {
            self.push_log(format!("Chat logging disabled after write error: {error}"));
        }
    }
}

impl ChatLog {
    fn append(
        &mut self,
        directory: &std::path::Path,
        mode: i64,
        time: u64,
        text: &str,
    ) -> std::io::Result<()> {
        if mode == 0 {
            *self = Self::default();
            return Ok(());
        }
        if self.failed {
            return Ok(());
        }
        let result = self.write(directory, mode, time, text);
        if result.is_err() {
            self.failed = true;
            self.file = None;
        }
        result
    }

    fn write(
        &mut self,
        directory: &std::path::Path,
        mode: i64,
        time: u64,
        text: &str,
    ) -> std::io::Result<()> {
        if self.file.is_none() {
            let directory = directory.join("chatlogs");
            std::fs::create_dir_all(&directory)?;
            self.file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(directory.join("chat.log"))?,
            );
        }
        // Bounded UTF-8-safe cleaning; no per-message string or texture allocations.
        let mut cleaned = [0_u8; 4096];
        let length = clean(text.as_bytes(), &mut cleaned);
        let file = self.file.as_mut().expect("opened above");
        write!(file, "[{time}] ")?;
        file.write_all(&cleaned[..length])?;
        file.write_all(b"\n")?;
        if mode == 2 {
            file.sync_data()?;
        }
        Ok(())
    }
}

fn clean(text: &[u8], output: &mut [u8]) -> usize {
    let (mut source, mut written) = (0, 0);
    while source < text.len() && written < output.len() {
        if text[source] == b'^' && text.get(source + 1).is_some_and(u8::is_ascii_digit) {
            source += 2;
            continue;
        }
        let byte = text[source];
        source += 1;
        if byte < 32 || byte == 127 {
            continue;
        }
        output[written] = byte;
        written += 1;
    }
    while std::str::from_utf8(&output[..written]).is_err() && written > 0 {
        written -= 1;
    }
    written
}

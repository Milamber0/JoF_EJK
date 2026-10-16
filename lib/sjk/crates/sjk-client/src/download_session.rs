//! Move map-change downloads onto the host's worker without reconnecting.

use crate::{ClientError, ClientSession, download};
use std::time::Duration;

impl ClientSession {
    /// Whether a replacement gamestate requires worker-owned content I/O.
    pub fn needs_download(&self) -> bool {
        self.pending_download.is_some()
    }

    /// Complete a suspended map change on a worker, preserving this netchan.
    pub fn complete_download(
        &mut self,
        progress: std::sync::mpsc::SyncSender<String>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), ClientError> {
        let Some(message) = self.pending_download.take() else {
            return Ok(());
        };
        let storage = self
            .download_storage
            .as_mut()
            .ok_or_else(|| ClientError::Download("download capability unavailable".into()))?;
        storage.feedback(progress, cancelled);
        self.downloaded_message = Some(download::run(
            self.connection.as_mut().expect("network download"),
            message,
            storage.as_mut(),
            &mut self.client_reliable_sequence,
            &mut self.pending_client_commands,
        )?);
        self.receive_snapshot(Duration::from_secs(5))?;
        Ok(())
    }
}

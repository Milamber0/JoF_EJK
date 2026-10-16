//! Suspend the live netchan while a worker downloads map-change content.

use super::*;

impl JoinTask {
    /// Resume content loading with the same client and reliable-command sequences.
    pub(crate) fn resume(mut session: ClientSession) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (_, map) = mpsc::channel();
        let (_, prepared) = mpsc::channel();
        let (_, phase) = mpsc::channel();
        let (progress_tx, progress) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        thread::spawn(move || {
            let result = session
                .complete_download(progress_tx, worker_cancel)
                .map(|()| JoinedSession {
                    session: Box::new(session),
                    timeline: ConnectTimeline::new(),
                    forcepowers: String::new(),
                })
                .map_err(|e| e.to_string());
            if let Err(undelivered) = sender.send(result)
                && let Ok(mut joined) = undelivered.0
            {
                let _ = joined.session.disconnect();
            }
        });
        Self {
            receiver,
            progress,
            map,
            prepared,
            phase,
            cancelled,
        }
    }
}

impl Drop for JoinTask {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

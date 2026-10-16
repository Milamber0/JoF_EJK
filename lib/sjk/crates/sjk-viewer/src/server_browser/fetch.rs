//! The master-server fetch behind the browser: one worker at a time, rows
//! handed over as servers answer.

use super::{FRESH_FOR, RefreshPoll, ServerBrowser, ServerEntry, discover};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::Instant;

/// What the fetch worker sends back: rows as servers answer, then the end.
pub(super) enum Fetched {
    Row(ServerEntry),
    Done(Result<(), String>),
}

impl ServerBrowser {
    pub(crate) fn is_refreshing(&self) -> bool {
        self.refresh.is_some()
    }

    /// Whether the rows are missing or old enough that opening the browser
    /// should fetch again (a fetch in flight counts as fresh).
    pub(crate) fn is_stale(&self) -> bool {
        !self.is_refreshing()
            && self
                .fetched_at
                .is_none_or(|fetched_at| fetched_at.elapsed() > FRESH_FOR)
    }

    pub(crate) fn refresh(&mut self) {
        if self.refresh.is_some() {
            return;
        }
        self.details.retry();
        let master = self.master.clone();
        let favourites: Vec<_> = self.favorites.iter().copied().collect();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let found = |entry| {
                let _ = sender.send(Fetched::Row(entry));
            };
            let result =
                discover::discover(&master, &favourites, found).map_err(|error| error.to_string());
            let _ = sender.send(Fetched::Done(result));
        });
        self.replace_on_first_row = true;
        self.refresh_started = Instant::now();
        self.refresh = Some(receiver);
    }

    pub(crate) fn set_master(&mut self, master: String) {
        if self.master != master {
            // Rows from another master are no answer for this one.
            self.fetched_at = None;
            self.master = master;
        }
    }

    pub(crate) fn poll(&mut self) -> RefreshPoll {
        let Some(receiver) = &self.refresh else {
            return RefreshPoll::Idle;
        };
        // Take every row that has arrived since last frame, then the end.
        let mut arrived = false;
        let outcome = loop {
            match receiver.try_recv() {
                Ok(Fetched::Row(entry)) => {
                    if std::mem::take(&mut self.replace_on_first_row) {
                        self.entries.clear();
                    }
                    self.entries.push(entry);
                    arrived = true;
                }
                Ok(Fetched::Done(Ok(()))) => {
                    if std::mem::take(&mut self.replace_on_first_row) {
                        self.entries.clear();
                        arrived = true;
                    }
                    self.refresh = None;
                    self.fetched_at = Some(Instant::now());
                    crate::log::progress(format_args!(
                        "server browser: {} servers answered within {:.0?}",
                        self.entries.len(),
                        self.refresh_started.elapsed()
                    ));
                    break RefreshPoll::Complete;
                }
                Ok(Fetched::Done(Err(error))) => {
                    self.refresh = None;
                    break RefreshPoll::Failed(error);
                }
                Err(TryRecvError::Empty) => break RefreshPoll::Pending,
                Err(TryRecvError::Disconnected) => {
                    self.refresh = None;
                    break RefreshPoll::Failed(
                        "server-browser worker stopped unexpectedly".to_owned(),
                    );
                }
            }
        };
        if arrived {
            self.rebuild_visible();
        }
        outcome
    }
}

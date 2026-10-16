//! Rate-limited in-session userinfo command tracking.
//!
//! OpenJK sends `userinfo "<CVAR_USERINFO>"` through the reliable command
//! queue when any userinfo cvar changes (`codemp/client/cl_main.cpp:2157-2169`).

use sjk_network::{LegacyUserInfo, NetworkError, legacy_userinfo_payload_with_extensions};
use std::time::{Duration, Instant};

const MIN_UPDATE_INTERVAL: Duration = Duration::from_secs(1);

/// Result of checking a prospective userinfo payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserinfoUpdateStatus {
    /// The payload equals the last successfully sent value.
    Unchanged,
    /// A changed payload is waiting for the one-second send interval.
    RateLimited,
    /// A changed payload was sent on the reliable channel.
    Sent,
}

pub(crate) struct UserinfoUpdateTracker {
    last_payload: String,
    last_sent_at: Instant,
    /// Connection identity is not part of the mutable player profile.
    guid: Option<String>,
}

impl UserinfoUpdateTracker {
    pub(crate) fn new(initial_payload: String, guid: Option<String>, now: Instant) -> Self {
        Self {
            last_payload: initial_payload,
            last_sent_at: now,
            guid,
        }
    }

    pub(crate) fn payload(
        &self,
        userinfo: &LegacyUserInfo,
        extensions: &[(&str, &str)],
    ) -> Result<String, NetworkError> {
        let mut userinfo = userinfo.clone();
        userinfo.guid.clone_from(&self.guid);
        legacy_userinfo_payload_with_extensions(&userinfo, extensions)
    }

    pub(crate) fn status(&self, payload: &str, now: Instant) -> UserinfoUpdateStatus {
        if payload == self.last_payload {
            UserinfoUpdateStatus::Unchanged
        } else if now.duration_since(self.last_sent_at) < MIN_UPDATE_INTERVAL {
            UserinfoUpdateStatus::RateLimited
        } else {
            UserinfoUpdateStatus::Sent
        }
    }

    pub(crate) fn command(payload: &str) -> Vec<u8> {
        let mut command = Vec::with_capacity(payload.len() + 11);
        command.extend_from_slice(b"userinfo \"");
        command.extend_from_slice(payload.as_bytes());
        command.push(b'"');
        command
    }

    pub(crate) fn mark_sent(&mut self, payload: String, now: Instant) {
        self.last_payload = payload;
        self.last_sent_at = now;
    }
}

/// Console transport diagnostics are read without changing the wire codec.
impl crate::ClientSession {
    /// Send the optional stock team-status subscription using the existing userinfo codec.
    /// TaystJK cg_cvar.c:46-54; OpenJK g_client.c:2294-2304.
    pub fn update_userinfo_options(
        &mut self,
        userinfo: &LegacyUserInfo,
        team_overlay: Option<bool>,
        now: Instant,
    ) -> Result<UserinfoUpdateStatus, crate::ClientError> {
        let mut extensions = self.compat_profile.userinfo_extensions().to_vec();
        if let Some(enabled) = team_overlay {
            extensions.push(("teamoverlay", if enabled { "1" } else { "0" }));
        }
        let userinfo = self.compat_profile.userinfo_for(userinfo);
        let payload = self.userinfo_updates.payload(&userinfo, &extensions)?;
        let status = self.userinfo_updates.status(&payload, now);
        if status != UserinfoUpdateStatus::Sent {
            return Ok(status);
        }
        self.send_reliable_command(&UserinfoUpdateTracker::command(&payload))?;
        self.userinfo_updates.mark_sent(payload, now);
        Ok(UserinfoUpdateStatus::Sent)
    }

    /// Take an expected-server OOB print for the frontend console.
    pub fn pop_server_print(&mut self) -> Option<String> {
        self.connection.as_mut().and_then(|c| c.pop_server_print())
    }

    /// How long the connected server has been silent on the sequenced channel.
    pub fn packet_silence(&self) -> Option<std::time::Duration> {
        self.connection.as_ref().and_then(|c| c.packet_silence())
    }
}

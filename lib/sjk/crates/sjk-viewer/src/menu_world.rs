//! The boot map comes back whenever the client is back in the menu off a server,
//! so the standalone menu always shows the same map — with its authored camera
//! routes and stage — whatever map the server ran.
//!
//! A server world replaces the menu world as the current [`GpuState`] when it
//! is installed, and the menu world is dropped then: keeping it during the match
//! held a whole second world (its textures, light caches and screen targets,
//! about 4.8 GB of the process at 4K on duel6). Only its map is kept. Once the
//! shell is back in the menu with nothing in flight, the menu world is built
//! again in the background from the installed game data
//! ([`GpuState::rebuild_menu_world`]), on the graphics context of the moment,
//! while the last server's world stays on show under the menus; it takes the
//! shell over when built (`poll_rebuild`). Server worlds are dropped as before.
//!
//! A join in flight (the reconnect after a server's map change timed out) is not "off
//! the server": `poll_rebuild` cancels a rebuild under a join, and before 10/10/2026
//! the next frame started one again, each starting a worker that read the whole map:
//! thousands of them in a few minutes, which stalled the join and could exhaust memory.
//! Rebuilds also start at most once per [`RETRY`].

use crate::GpuState;
use std::time::{Duration, Instant};

/// Least time between two starts of the menu world's rebuild.
const RETRY: Duration = Duration::from_secs(5);

/// The menu world's map while the client is on a server world.
pub(crate) struct Parked {
    map: Option<String>,
    /// When the last rebuild started.
    started: Option<Instant>,
}

/// Whether a rebuild may start at `now` after one started at `started`.
fn may_start(started: Option<Instant>, now: Instant) -> bool {
    started.is_none_or(|started| now.saturating_duration_since(started) >= RETRY)
}

impl Parked {
    pub(crate) const fn new() -> Self {
        Self {
            map: None,
            started: None,
        }
    }

    /// Make `installed` the current world. The world it replaces is dropped; a
    /// menu world leaves its map to be built again later.
    pub(crate) fn install(&mut self, current: &mut GpuState, installed: GpuState) {
        let previous = std::mem::replace(current, installed);
        if previous.is_menu_world && !previous.graphics_reload.map.is_empty() {
            self.map = Some(previous.graphics_reload.map.clone());
        }
    }

    /// Start building the menu world again once the client has left the server
    /// and no map is loading. A join started meanwhile cancels it, and it starts
    /// again when the menus are back.
    pub(crate) fn restore_if_idle(&mut self, current: &mut GpuState) {
        if current.is_menu_world {
            self.map = None;
            return;
        }
        let now = Instant::now();
        if !current.shell_is_off_server()
            || current.graphics_reload.rebuilding()
            || current.graphics_reload.menu_failed
            || !may_start(self.started, now)
        {
            return;
        }
        if let Some(map) = &self.map {
            self.started = Some(now);
            current.rebuild_menu_world(map.clone());
        }
    }
}

impl GpuState {
    /// No session and no map load in flight, and the menu is up: the world
    /// only serves as the menu backdrop now.
    fn shell_is_off_server(&self) -> bool {
        !self.resident.exploring()
            && self.resident.session.is_none()
            && self.join_task.is_none()
            && self.live_session.is_none()
            && self.demo_session.is_none()
            && self.world_load_task.is_none()
            && self.world_install_task.is_none()
            && !self.pending_map_reload
            && self
                .client_menu
                .as_ref()
                .is_some_and(crate::menu::ClientMenu::is_visible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rebuild_starts_at_most_once_per_retry() {
        let start = Instant::now();
        assert!(may_start(None, start));
        assert!(!may_start(Some(start), start));
        assert!(!may_start(Some(start), start + RETRY / 2));
        assert!(may_start(Some(start), start + RETRY));
    }
}

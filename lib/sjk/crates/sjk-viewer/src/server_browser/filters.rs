//! Browser policy, sampled on cvar changes and applied to the existing sorted view.
use super::{ServerBrowser, ServerEntry};
use crate::console::ViewerConsole;

/// Retained filter values; no string formatting or allocation when unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Filters {
    /// Include servers with no non-bot players.
    pub(crate) empty: bool,
    /// Include servers whose player count equals capacity.
    pub(crate) full: bool,
    /// Include password-protected servers.
    pub(crate) password: bool,
    /// Reject unprintable info or a blank hostname.
    pub(crate) valid: bool,
    /// Protocol gametype number; -1 means all modes.
    pub(crate) mode: i32,
    /// Maximum accepted round-trip time in milliseconds.
    pub(crate) ping: u32,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            empty: true,
            full: true,
            password: true,
            valid: true,
            mode: -1,
            ping: 800,
        }
    }
}

impl Filters {
    /// Apply TaystJK UI_BuildServerDisplayList's empty/full/password/validity predicates.
    pub(crate) fn accepts(self, row: &ServerEntry) -> bool {
        (self.empty || i32::from(row.players) - row.bots != 0)
            && (self.full || row.players != row.capacity)
            && (self.password || !row.password)
            && (!self.valid || row.valid_info)
            && (self.mode < 0 || row.mode == Some(self.mode))
            && row.ping_millis <= self.ping
    }
}

/// TaystJK ui_main.c:8979-8997 and shared/qcommon/q_string.c:21-36.
pub(super) fn valid_info(info: &sjk_protocol::InfoString) -> bool {
    info.iter().all(|(key, value)| {
        key.bytes()
            .chain(value.bytes())
            .all(|b| b >= 32 && b != 127 && b != 255)
    }) && info
        .get("hostname")
        .is_some_and(|s| s.bytes().any(|b| b >= 33 && b != 127 && b != 255))
}

impl ServerBrowser {
    /// Apply only changed cvars; keep the selected server if it survives filtering.
    pub(crate) fn configure(&mut self, console: &mut ViewerConsole) {
        let default = Filters::default();
        // Fall back to each switch's own default rather than a shared one: an
        // unresolvable mode or ping cap must not silently hide every server.
        let number = |name, fallback| console.integer_cvar(name).unwrap_or(fallback);
        // The menus have no separate create-game selector: these two inputs share a mode.
        // If both changed together, the explicit network mode wins.
        let actual = number("ui_actualnetgametype", default.mode.into()) as i32;
        let mode = if actual != self.filters.mode {
            actual
        } else {
            number("ui_gametype", default.mode.into()) as i32
        }
        .clamp(-1, 9);
        let filters = Filters {
            empty: number("ui_browsershowempty", 1) != 0,
            full: number("ui_browsershowfull", 1) != 0,
            password: number("ui_browsershowpasswordprotected", 1) != 0,
            valid: number("ui_browserfilterinvalidinfo", 1) != 0,
            mode,
            ping: number("cl_maxping", default.ping.into()).clamp(1, 60_000) as u32,
        };
        if filters == self.filters {
            return;
        }
        if mode != self.filters.mode {
            let value = mode.to_string();
            console.set_cvar("ui_gametype", &value);
            console.set_cvar("ui_actualNetGametype", &value);
        }
        let selected = self.visible_entry(self.selected).map(|row| row.address);
        self.filters = filters;
        self.rebuild_visible();
        self.selected = selected
            .and_then(|address| self.row_of(address))
            .unwrap_or(0);
        self.reveal_selection();
    }

    /// Values used by the browser's retained filter strip.
    pub(crate) fn filters(&self) -> Filters {
        self.filters
    }
}

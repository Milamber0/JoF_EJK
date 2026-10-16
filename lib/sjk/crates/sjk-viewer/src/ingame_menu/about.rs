//! The "Server info" page: what the stock `ingame_about` menu shows — host,
//! address, map, game type, player cap and the limits that apply to the
//! current game type — read from the server's serverinfo config string.

use crate::server_browser::gametype_name;
use sjk_protocol::{GameState, InfoString};
use std::fmt::Write as _;

/// Info lines the page can carry.
pub(super) const LINES: usize = 8;

/// Retained "label  /  value" lines, refreshed when the page opens.
pub(super) struct State {
    lines: [String; LINES],
    len: usize,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            lines: std::array::from_fn(|_| String::with_capacity(96)),
            len: 0,
        }
    }

    /// The lines to show, in order.
    pub(super) fn lines(&self) -> &[String] {
        &self.lines[..self.len]
    }

    /// Rebuild the lines from `game_state`'s serverinfo and the address
    /// the client connected to.
    pub(super) fn refresh(&mut self, game_state: Option<&GameState>, address: Option<&str>) {
        self.len = 0;
        let info = game_state
            .and_then(|state| state.config_string(0))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| InfoString::parse(text).ok());
        let Some(info) = info else {
            self.push("Server info", "unavailable");
            return;
        };
        let gametype = info.get_i32("g_gametype");
        self.push("Host", info.get("sv_hostname").unwrap_or("?"));
        self.push("Address", address.unwrap_or("?"));
        self.push("Map", info.get("mapname").unwrap_or("?"));
        self.push("Game type", gametype_name(gametype));
        self.push("Max players", info.get("sv_maxclients").unwrap_or("?"));
        self.push_limit("Time limit", info.get("timelimit"), "minutes");
        // The one score limit the stock about screen shows for this type
        // (`ui/jamp/ingame_about.menu`: frag, capture or duel limit).
        let (label, key) = match gametype {
            Some(3 | 4) => ("Duel limit", "duellimit"),
            Some(8 | 9) => ("Capture limit", "capturelimit"),
            _ => ("Frag limit", "fraglimit"),
        };
        self.push_limit(label, info.get(key), "");
    }

    fn push(&mut self, label: &str, value: &str) {
        if let Some(line) = self.lines.get_mut(self.len) {
            line.clear();
            let _ = write!(line, "{label}  /  {value}");
            self.len += 1;
        }
    }

    /// A numeric limit, where 0 or absent means "none".
    fn push_limit(&mut self, label: &str, value: Option<&str>, unit: &str) {
        match value.and_then(|value| value.trim().parse::<i32>().ok()) {
            Some(limit) if limit > 0 => {
                if let Some(line) = self.lines.get_mut(self.len) {
                    line.clear();
                    let _ = write!(line, "{label}  /  {limit}");
                    if !unit.is_empty() {
                        let _ = write!(line, " {unit}");
                    }
                    self.len += 1;
                }
            }
            _ => self.push(label, "none"),
        }
    }
}

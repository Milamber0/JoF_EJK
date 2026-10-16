//! The servers the player joined last, newest first, for the SJK UI's main page:
//! kept in `recent_servers.json` beside `favorites.json`, with each server's
//! name and map as they were when it was last joined (the server list gives the
//! live ones while it has the server) and when that was.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// How many servers the page lists.
pub(crate) const MAX: usize = 4;

/// One server joined.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Recent {
    pub(crate) address: String,
    /// The server's name and map when it was last joined; empty when unknown.
    pub(crate) name: String,
    pub(crate) map: String,
    /// When it was last joined, in Unix seconds.
    pub(crate) played: u64,
}

/// The list and where it is kept.
#[derive(Debug, Default)]
pub(crate) struct RecentServers {
    path: Option<PathBuf>,
    servers: Vec<Recent>,
}

impl RecentServers {
    /// The player's list, from the profile folder.
    pub(crate) fn load() -> Self {
        let path = crate::platform::user_config_file().ok().and_then(|path| {
            path.parent()
                .map(|parent| parent.join("recent_servers.json"))
        });
        let servers = path.as_deref().map(read).unwrap_or_default();
        Self { path, servers }
    }

    /// A list kept at `path`, for tests.
    #[cfg(test)]
    pub(crate) fn at(path: PathBuf) -> Self {
        let servers = read(&path);
        Self {
            path: Some(path),
            servers,
        }
    }

    /// The servers, newest first.
    pub(crate) fn servers(&self) -> &[Recent] {
        &self.servers
    }

    /// `address` was joined at Unix time `now`, on a server named `name` playing
    /// `map` (either empty when unknown, which keeps what was known before). It
    /// goes to the top of the list, which keeps [`MAX`] servers, and the list is
    /// saved.
    pub(crate) fn record(&mut self, address: &str, name: &str, map: &str, now: u64) {
        let address = address.trim();
        if address.is_empty() {
            return;
        }
        let known = self
            .servers
            .iter()
            .position(|server| same_address(&server.address, address))
            .map(|index| self.servers.remove(index));
        let keep = |new: &str, old: Option<&String>| {
            if new.trim().is_empty() {
                old.cloned().unwrap_or_default()
            } else {
                new.trim().to_owned()
            }
        };
        self.servers.insert(
            0,
            Recent {
                address: address.to_owned(),
                name: keep(name, known.as_ref().map(|server| &server.name)),
                map: keep(map, known.as_ref().map(|server| &server.map)),
                played: now,
            },
        );
        self.servers.truncate(MAX);
        if let Some(path) = &self.path
            && let Err(error) = write(path, &self.servers)
        {
            crate::log::progress(format_args!("warning: recent servers not saved: {error}"));
        }
    }
}

/// Whether two addresses name the same server: equal socket addresses, or the
/// same text when either is a host name.
pub(crate) fn same_address(a: &str, b: &str) -> bool {
    match (
        a.trim().parse::<std::net::SocketAddr>(),
        b.trim().parse::<std::net::SocketAddr>(),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

fn read(path: &Path) -> Vec<Recent> {
    let Some(Value::Array(items)) = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let text = |key: &str| {
                item.get(key)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            let address = text("address");
            (!address.trim().is_empty()).then(|| Recent {
                address,
                name: text("name"),
                map: text("map"),
                played: item.get("played").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .take(MAX)
        .collect()
}

fn write(path: &Path, servers: &[Recent]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let items: Vec<Value> = servers
        .iter()
        .map(|server| {
            json!({
                "address": server.address,
                "name": server.name,
                "map": server.map,
                "played": server.played,
            })
        })
        .collect();
    let bytes = serde_json::to_vec_pretty(&items).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

/// How long ago Unix time `then` was at `now`, as the page says it: "just now",
/// "5 minutes ago", "2 hours ago", "yesterday", "3 days ago". Relative, so it
/// needs no time zone.
pub(crate) fn ago(then: u64, now: u64) -> Ago {
    let seconds = now.saturating_sub(then);
    match seconds {
        0..60 => Ago::JustNow,
        60..3_600 => Ago::Minutes(seconds / 60),
        3_600..86_400 => Ago::Hours(seconds / 3_600),
        86_400..172_800 => Ago::Yesterday,
        _ => Ago::Days(seconds / 86_400),
    }
}

/// A time before now, in words ([`ago`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Ago {
    JustNow,
    Minutes(u64),
    Hours(u64),
    Yesterday,
    Days(u64),
}

impl std::fmt::Display for Ago {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let plural = |count: &u64| if *count == 1 { "" } else { "s" };
        match self {
            Self::JustNow => formatter.write_str("just now"),
            Self::Minutes(count) => write!(formatter, "{count} minute{} ago", plural(count)),
            Self::Hours(count) => write!(formatter, "{count} hour{} ago", plural(count)),
            Self::Yesterday => formatter.write_str("yesterday"),
            Self::Days(count) => write!(formatter, "{count} days ago"),
        }
    }
}

/// Now, in Unix seconds.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_go_to_the_top_once_each_and_survive_a_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recent_servers.json");
        let mut recent = RecentServers::at(path.clone());
        assert!(recent.servers().is_empty());
        recent.record("135.125.145.49:29070", "JoF", "mp/ffa3", 100);
        recent.record("10.0.0.2:29070", "Duel Arena", "mp/duel6", 200);
        // Joining JoF again moves it up; an unknown name keeps the old one.
        recent.record(" 135.125.145.49:29070 ", "", "mp/ffa2", 300);
        let servers = recent.servers();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].address, "135.125.145.49:29070");
        assert_eq!(
            (
                servers[0].name.as_str(),
                servers[0].map.as_str(),
                servers[0].played
            ),
            ("JoF", "mp/ffa2", 300)
        );
        // Only the newest few are kept.
        for index in 0..6 {
            recent.record(&format!("10.0.1.{index}:29070"), "", "", 400 + index);
        }
        assert_eq!(recent.servers().len(), MAX);
        assert_eq!(recent.servers()[0].address, "10.0.1.5:29070");
        // The list is read back as saved; a broken file reads as empty.
        assert_eq!(RecentServers::at(path.clone()).servers(), recent.servers());
        std::fs::write(&path, b"not json").unwrap();
        assert!(RecentServers::at(path).servers().is_empty());
        // A blank address records nothing.
        recent.record("  ", "x", "y", 9);
        assert_eq!(recent.servers()[0].address, "10.0.1.5:29070");
    }

    #[test]
    fn addresses_compare_as_addresses_or_as_names() {
        assert!(same_address(
            "135.125.145.49:29070",
            " 135.125.145.49:29070"
        ));
        assert!(!same_address(
            "135.125.145.49:29070",
            "135.125.145.49:29071"
        ));
        assert!(same_address("Duel.Example.org", "duel.example.org"));
    }

    #[test]
    fn times_read_as_words() {
        assert_eq!(ago(100, 130).to_string(), "just now");
        assert_eq!(ago(0, 60).to_string(), "1 minute ago");
        assert_eq!(ago(0, 7_200).to_string(), "2 hours ago");
        assert_eq!(ago(0, 90_000).to_string(), "yesterday");
        assert_eq!(ago(0, 3 * 86_400).to_string(), "3 days ago");
        // A clock set back reads as just now.
        assert_eq!(ago(500, 100), Ago::JustNow);
    }
}

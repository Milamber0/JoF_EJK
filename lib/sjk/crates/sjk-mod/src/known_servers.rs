//! Server families SJK knows by name: one entry each, told from the server info,
//! so mods and the client read one table instead of each testing version strings
//! (AGENTS.md, no hardcoded special cases). What SJK does differently on a family
//! lives in its own table keyed by [`KnownServer::id`] (the client's command caps,
//! `server_caps.rs`), each with a player override.

use crate::{Server, ServerKind};

/// A server family and how its servers are told.
#[derive(Debug, Eq, PartialEq)]
pub struct KnownServer {
    /// The family's key in the tables that act on it.
    pub id: &'static str,
    /// The name players know it by, as the log and the menus show it.
    pub name: &'static str,
    /// The game module its servers run.
    pub kind: ServerKind,
    /// The server info key and the value (case and surrounding spaces ignored)
    /// that tell it.
    pub key: &'static str,
    pub value: &'static str,
}

impl KnownServer {
    fn matches(&self, server: &Server) -> bool {
        server.kind == self.kind
            && server
                .info(self.key)
                .is_some_and(|value| value.trim().eq_ignore_ascii_case(self.value))
    }
}

/// The families SJK knows.
pub const KNOWN_SERVERS: &[KnownServer] = &[
    // JoF's JA+ servers advertise this exact version, as JoF EJK's
    // `CL_JoFTrustedServer` (`codemp/client/cl_main.cpp:1793-1800`) and
    // `ui_isJoFJAPlus` (`codemp/ui/ui_main.c:702-704`) read it.
    KnownServer {
        id: "jof",
        name: "JoF",
        kind: ServerKind::JaPlus,
        key: "V",
        value: "2.5B0",
    },
];

/// The family `server` belongs to, if SJK knows it.
pub fn identify(server: &Server) -> Option<&'static KnownServer> {
    KNOWN_SERVERS.iter().find(|family| family.matches(server))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(kind: ServerKind, info: &str) -> Server {
        Server {
            kind,
            address: "127.0.0.1:29070".into(),
            info: info.into(),
        }
    }

    #[test]
    fn jof_servers_are_told_by_their_version() {
        let jof = identify(&server(
            ServerKind::JaPlus,
            r"\gamename\JA+ Mod v2.5\V\2.5B0",
        ));
        assert_eq!(jof.map(|family| family.id), Some("jof"));
        assert!(identify(&server(ServerKind::JaPlus, r"\V\2.4B7")).is_none());
        assert!(identify(&server(ServerKind::JaPro, r"\V\2.5B0")).is_none());
        assert!(identify(&server(ServerKind::JaPlus, r"\v\ 2.5b0 ")).is_some());
    }

    #[test]
    fn every_family_has_a_unique_id_and_a_rule() {
        for (index, family) in KNOWN_SERVERS.iter().enumerate() {
            assert!(
                !family.key.is_empty() && !family.value.is_empty(),
                "{}",
                family.id
            );
            assert!(
                KNOWN_SERVERS[..index]
                    .iter()
                    .all(|other| other.id != family.id),
                "{} twice",
                family.id
            );
        }
    }
}

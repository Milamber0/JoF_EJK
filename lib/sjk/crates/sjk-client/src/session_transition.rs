//! Session-lifecycle events derived from legacy server messages.
//!
//! The compatibility client owns protocol-26 restart semantics; renderers see
//! typed events and never inspect wire service commands themselves.
//! Replacement gamestates follow `codemp/client/cl_parse.cpp:525-642`.
//! Same-map fast restarts are the reliable `map_restart` command emitted by
//! `codemp/server/sv_ccmds.cpp:352-359` and reset cgame state according to
//! `codemp/cgame/cg_servercmds.c:1171-1225`.

/// Why a live client session changed state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionTransitionKind {
    /// A replacement `svc_gamestate` selected a different BSP.
    NewMap,
    /// A replacement `svc_gamestate` retained the current BSP.
    SameMapGamestate,
    /// The server sent the reliable `map_restart` command.
    MapRestart,
    /// The server sent the reliable `disconnect` command.
    Disconnected,
}

/// A restart/disconnect notification emitted at the client boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionTransition {
    pub kind: SessionTransitionKind,
    pub old_map: Option<String>,
    pub new_map: Option<String>,
    pub server_id: i32,
    pub checksum_feed: i32,
    pub gamestate_message_sequence: Option<i32>,
    pub first_snapshot_message_sequence: Option<i32>,
    pub gamestate_to_first_snapshot_micros: Option<u64>,
    pub reason: Option<String>,
}

impl SessionTransition {
    pub(crate) fn disconnected(reason: Option<String>, server_id: i32, checksum_feed: i32) -> Self {
        Self {
            kind: SessionTransitionKind::Disconnected,
            old_map: None,
            new_map: None,
            server_id,
            checksum_feed,
            gamestate_message_sequence: None,
            first_snapshot_message_sequence: None,
            gamestate_to_first_snapshot_micros: None,
            reason,
        }
    }
}

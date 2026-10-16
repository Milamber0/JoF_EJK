//! Authoritative world ownership, independent of game rules and wire formats.
//!
//! A [`Server`] owns worlds and issues world-qualified entity handles. Game
//! adapters supply world resources and entity state as ordinary Rust values;
//! client presentation, protocol state and wire identifiers are separate owners.
//! This foundation does not yet execute gameplay or host network sessions.
//!
//! Handles are scoped to one server instance. Persisted games and process-to-process
//! transfers must resolve their own identities instead of retaining these handles.

mod entities;
mod server;
mod world;

pub use server::{CreateWorldError, Server};
pub use sjk_runtime::{EntityId, WorldId};
pub use world::{EntityHandle, EntityRejected, ServerWorld};

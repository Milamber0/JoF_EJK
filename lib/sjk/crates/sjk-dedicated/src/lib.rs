//! SJK's headless server: platform composition of the native server core
//! (`sjk-server`), the JKA game rules (`sjk-game-jka`) and the protocol-26 endpoint
//! (`sjk-network`). The executable in `main.rs` adds only the socket, the clocks and
//! the operator's options.

pub mod bridge;
pub mod collision;
pub mod command_buffer;
pub mod config_files;
pub mod cvars;
pub mod frame_clock;
pub mod map;
pub mod master;
pub mod peer;
pub(crate) mod players;
pub mod visibility;

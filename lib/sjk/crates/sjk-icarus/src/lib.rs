//! ICARUS, the script interpreter of the Quake III-based Raven engines: compiled block
//! files (`.IBI`), a sequencer and task manager for every entity that runs scripts, and
//! the scheduling between them.
//!
//! This is engine functionality with no game in it. The interpreter knows the block
//! format and the language — `affect`, `wait`, `loop`, `if`/`else`, `task`/`do`,
//! `run`, `flush`, `signal`/`waitsignal`, `declare`/`free`, `get`, `random`, `tag`,
//! `camera` and the commands a game carries out (`set`, `move`, `rotate`, `use`,
//! `kill`, `remove`, `play`, `sound`, `print`) — and runs scripts frame by frame. What a
//! command *does* is the game's: every command reaches it through [`IcarusHost`], the
//! stand-in for the reference's `GVM_ICARUS_*` exports and the engine services around
//! them. The names a game gives its `set` fields, its task slots and its entities are
//! the game's too (Jedi Academy's are in `sjk-game-jka`).
//!
//! The behaviour is Raven's (`codemp/icarus` in OpenJK: `BlockStream.cpp`,
//! `Sequence.cpp`, `Sequencer.cpp`, `TaskManager.cpp`, `Instance.cpp`, and the engine's
//! `GameInterface.cpp`, `Q3_Interface.cpp` and `Q3_Registers.cpp`), held to the
//! reference by `tools/icarus-oracle`, which runs the unmodified reference on the same
//! scenarios as `tests/oracle.rs`. That includes the reference's quirks where a script
//! can see them: sequence ids travel through blocks as floats, a condition compares
//! numbers printed to three places, a failed `get` inside an `affect` routes the body
//! as ordinary commands, a `get(STRING, ...)` answer is overwritten by the next game
//! call. Where the reference would crash or read freed memory (an entity freed while its
//! own script runs, a sequence freed while another still names it), this stops or skips
//! instead; those places say so.
//!
//! Save games (`Save`/`Load`), which the multiplayer reference compiles out, are not
//! here.

mod block;
mod camera;
pub mod cnum;
mod conditional;
mod execute;
mod host;
pub mod ids;
mod instance;
mod prep;
mod print;
mod route;
mod scripts;
mod sequence;
mod sequencer;
mod shared;
mod stream;
mod tasks;
mod variables;

pub use block::{BF_ELSE, Block, Member, latin1};
pub use host::{DebugLevel, EntityNames, IcarusHost, Owner, SetKind, StringAnswer};
pub use instance::{Icarus, IcarusConfig};
pub use scripts::{SCRIPT_DIR, strip_extension};
pub use stream::{BlockStream, read_all};
pub use tasks::RUNAWAY_LIMIT;
pub use variables::{MAX_VARIABLES, VariableType, Variables};

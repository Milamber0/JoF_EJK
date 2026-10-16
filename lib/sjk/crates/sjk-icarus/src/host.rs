//! What the interpreter asks of the engine and the game around it.
//!
//! In the reference the interpreter reaches its world through two tables: the engine's
//! services (`svs.time`, `FS_ReadFile`, `Com_Printf`, `SV_SendServerCommand`, the
//! entities' `SVF_ICARUS_FREEZE`) and the game module's `GVM_ICARUS_*` exports
//! (`Q3_Set`, `Q3_Lerp2Pos`, `Q3_GetFloat`, ...). [`IcarusHost`] is both. Everything a
//! command *means* — what `set("SET_ORIGIN", ...)` does, which entity `use("door")`
//! reaches, where a tag is — is the host's; the interpreter only schedules.
//!
//! A host call may come back into the interpreter: a `use` can run another entity's
//! script, a move of no duration completes its task at once, a `remove` can free the
//! very entity whose script is running. Every game call therefore receives the
//! interpreter (`icarus`) to call back into. The reference crashes when a running
//! script's entity is freed under it; this interpreter stops that script's frame instead.

use crate::Icarus;
use std::fmt;
use std::hash::Hash;

/// Who owns a sequencer: the game's identity of a script-running entity. It is printed
/// where the reference prints an entity number (`%4d` in the debug lines), so its
/// `Display` should honour a width.
pub trait Owner: Copy + Eq + Hash + fmt::Debug + fmt::Display {}

impl<T: Copy + Eq + Hash + fmt::Debug + fmt::Display> Owner for T {}

/// The level of a developer message (`WL_ERROR` ... `WL_DEBUG`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugLevel {
    /// `WL_ERROR`, printed red.
    Error = 1,
    /// `WL_WARNING`, printed yellow.
    Warning = 2,
    /// `WL_VERBOSE`, printed green.
    Verbose = 3,
    /// `WL_DEBUG`: a command as it runs, prefixed with its entity.
    Debug = 4,
}

/// The names the debug lines print for an entity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityNames {
    /// `classname`.
    pub classname: Option<String>,
    /// `targetname`.
    pub targetname: Option<String>,
    /// `script_targetname`.
    pub script_targetname: Option<String>,
}

/// What a `set` name means to the script precacher (`ICARUS_InterrogateScript`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKind {
    /// Its value is another script, precached in turn (`SET_SPAWNSCRIPT` ...).
    Script,
    /// Its value is a looping sound, precached (`SET_LOOPSOUND`).
    LoopSound,
    /// Nothing to precache.
    Other,
}

/// A game's answer to `get(STRING, name)` (`GVM_ICARUS_GetString`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringAnswer {
    /// Whether the name was found.
    pub found: bool,
    /// The text, copied into the engine's shared buffer; `None` leaves that buffer as it
    /// was, which is what the reference's game does for a declared string variable.
    pub value: Option<String>,
}

/// The engine and game around the interpreter.
pub trait IcarusHost<O: Owner> {
    // ---- the engine ----

    /// The server's time in milliseconds (`svs.time`).
    fn time(&self) -> u32;
    /// A file's bytes (`FS_ReadFile`), or `None`.
    fn read_file(&mut self, path: &str) -> Option<Vec<u8>>;
    /// A random float in `[min, max)` from the engine's generator (`Q_flrand`).
    fn random(&mut self, min: f32, max: f32) -> f32;
    /// Whether the entity's scripts are frozen (`SVF_ICARUS_FREEZE`).
    fn frozen(&self, owner: O) -> bool;
    /// Whether developer messages are printed (`com_developer`).
    fn developer(&self) -> bool;
    /// A console line (`Com_Printf`); `text` carries its own line breaks.
    fn print(&mut self, text: &str);
    /// A reliable command to every client (`SV_SendServerCommand(NULL, ...)`).
    fn broadcast_command(&mut self, command: &str);
    /// The names a debug line prints for an entity.
    fn entity_names(&self, owner: O) -> EntityNames;
    /// Precache a ROFF file a script plays (`theROFFSystem.Cache`).
    fn cache_roff(&mut self, file: &str);

    // ---- the game (`GVM_ICARUS_*`) ----

    /// `sound(channel, name)`: true if the task is complete at once.
    fn play_sound(
        &mut self,
        icarus: &mut Icarus<O>,
        task: i32,
        owner: O,
        name: &str,
        channel: &str,
    ) -> bool;
    /// `set(name, value)`: true if the task is complete at once; otherwise the game
    /// completes it later through [`Icarus::task_id_complete`].
    fn set(&mut self, icarus: &mut Icarus<O>, task: i32, owner: O, name: &str, value: &str)
    -> bool;
    /// `move(origin, [angles,] duration)`. The game may change the vectors; the
    /// interpreter keeps what it hands back, as the reference copies them back.
    fn lerp_to_position(
        &mut self,
        icarus: &mut Icarus<O>,
        task: i32,
        owner: O,
        origin: &mut [f32; 3],
        angles: Option<&mut [f32; 3]>,
        duration: f32,
    );
    /// `rotate(angles, duration)`.
    fn lerp_to_angles(
        &mut self,
        icarus: &mut Icarus<O>,
        task: i32,
        owner: O,
        angles: &mut [f32; 3],
        duration: f32,
    );
    /// `tag(name, ORIGIN|ANGLES)`: the tag's vector into `info`; false if there is none.
    fn tag(
        &mut self,
        icarus: &mut Icarus<O>,
        owner: O,
        name: &str,
        lookup: i32,
        info: &mut [f32; 3],
    ) -> bool;
    /// `use(name)`.
    fn use_target(&mut self, icarus: &mut Icarus<O>, owner: O, target: &str);
    /// `kill(name)`.
    fn kill(&mut self, icarus: &mut Icarus<O>, owner: O, name: &str);
    /// `remove(name)`.
    fn remove(&mut self, icarus: &mut Icarus<O>, owner: O, name: &str);
    /// `play(type, name)`.
    fn play(&mut self, icarus: &mut Icarus<O>, task: i32, owner: O, kind: &str, name: &str);
    /// `get(FLOAT|INT, name)`: the value, or `None` if the name is not found.
    fn get_float(&mut self, icarus: &mut Icarus<O>, owner: O, kind: i32, name: &str)
    -> Option<f32>;
    /// `get(VECTOR, name)`.
    fn get_vector(
        &mut self,
        icarus: &mut Icarus<O>,
        owner: O,
        kind: i32,
        name: &str,
    ) -> Option<[f32; 3]>;
    /// `get(STRING, name)`.
    fn get_string(
        &mut self,
        icarus: &mut Icarus<O>,
        owner: O,
        kind: i32,
        name: &str,
    ) -> StringAnswer;
    /// A sound a script names, precached (`G_SoundIndex`).
    fn precache_sound(&mut self, file: &str);
    /// What a `set` name means to the precacher (`GetIDForString(setTable, ...)`).
    fn set_kind(&mut self, name: &str) -> SetKind;
}

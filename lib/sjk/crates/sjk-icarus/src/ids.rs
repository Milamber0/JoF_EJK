//! The numbers of the compiled script format: block ids, member ids and the type
//! markers the script compiler writes (`codemp/icarus/tokenizer.h`, `interpreter.h`).
//!
//! These are the file format's own vocabulary — a block file carries them — not any
//! game's; a game's own words (which `set` names exist, what a task slot means) are the
//! game's business and never appear here.

// Token ids (`tokenizer.h`).
/// A character member.
pub const TK_CHAR: i32 = 3;
/// A string member.
pub const TK_STRING: i32 = 4;
/// An integer member (also the `INT` type of `get`).
pub const TK_INT: i32 = 5;
/// A float member (also the `FLOAT` type of `get` and `declare`).
pub const TK_FLOAT: i32 = 6;
/// An identifier member.
pub const TK_IDENTIFIER: i32 = 7;
/// A vector marker member: three float members follow (also the `VECTOR` type).
pub const TK_VECTOR: i32 = 14;
/// The `>` operator of a condition.
pub const TK_GREATER_THAN: i32 = 15;
/// The `<` operator of a condition.
pub const TK_LESS_THAN: i32 = 16;
/// The `=` operator of a condition.
pub const TK_EQUALS: i32 = 17;
/// The `!` operator of a condition ("not equal").
pub const TK_NOT: i32 = 18;

// Block and inline ids (`interpreter.h`).
/// `affect(name, type) { ... }`.
pub const ID_AFFECT: i32 = 19;
/// `sound(channel, name)`.
pub const ID_SOUND: i32 = 20;
/// `move(origin, [angles,] duration)`.
pub const ID_MOVE: i32 = 21;
/// `rotate(angles, duration)`.
pub const ID_ROTATE: i32 = 22;
/// `wait(milliseconds)` or `wait("task")`.
pub const ID_WAIT: i32 = 23;
/// The end of a `{ ... }` body.
pub const ID_BLOCK_END: i32 = 25;
/// `set(name, value)`.
pub const ID_SET: i32 = 26;
/// `loop(count) { ... }`.
pub const ID_LOOP: i32 = 27;
/// `print(text)`.
pub const ID_PRINT: i32 = 29;
/// `use(name)`.
pub const ID_USE: i32 = 30;
/// `flush()`.
pub const ID_FLUSH: i32 = 31;
/// `run(script)`.
pub const ID_RUN: i32 = 32;
/// `kill(name)`.
pub const ID_KILL: i32 = 33;
/// `remove(name)`.
pub const ID_REMOVE: i32 = 34;
/// `camera(type, ...)`.
pub const ID_CAMERA: i32 = 35;
/// An inline `get(type, name)`.
pub const ID_GET: i32 = 36;
/// An inline `random(min, max)`.
pub const ID_RANDOM: i32 = 37;
/// `if (a op b) { ... }`.
pub const ID_IF: i32 = 38;
/// `else { ... }`.
pub const ID_ELSE: i32 = 39;
/// `task(name) { ... }`.
pub const ID_TASK: i32 = 41;
/// `do(task)`.
pub const ID_DO: i32 = 42;
/// `declare(type, name)`.
pub const ID_DECLARE: i32 = 43;
/// `free(name)`.
pub const ID_FREE: i32 = 44;
/// `signal(name)`.
pub const ID_SIGNAL: i32 = 46;
/// `waitsignal(name)`.
pub const ID_WAITSIGNAL: i32 = 47;
/// `play(type, name)`.
pub const ID_PLAY: i32 = 48;
/// An inline `tag(name, ORIGIN|ANGLES)`.
pub const ID_TAG: i32 = 49;

// Type ids (`interpreter.h`).
/// The `ANGLES` lookup of a tag.
pub const TYPE_ANGLES: i32 = 53;
/// The `ORIGIN` lookup of a tag.
pub const TYPE_ORIGIN: i32 = 54;
/// `affect(..., INSERT)`.
pub const TYPE_INSERT: i32 = 55;
/// `affect(..., FLUSH)`.
pub const TYPE_FLUSH: i32 = 56;
/// `camera(PAN, ...)`.
pub const TYPE_PAN: i32 = 57;
/// `camera(ZOOM, ...)`.
pub const TYPE_ZOOM: i32 = 58;
/// `camera(MOVE, ...)`.
pub const TYPE_MOVE: i32 = 59;
/// `camera(FADE, ...)`.
pub const TYPE_FADE: i32 = 60;
/// `camera(PATH, ...)`.
pub const TYPE_PATH: i32 = 61;
/// `camera(ENABLE)`.
pub const TYPE_ENABLE: i32 = 62;
/// `camera(DISABLE)`.
pub const TYPE_DISABLE: i32 = 63;
/// `camera(SHAKE, ...)`.
pub const TYPE_SHAKE: i32 = 64;
/// `camera(ROLL, ...)`.
pub const TYPE_ROLL: i32 = 65;
/// `camera(TRACK, ...)`.
pub const TYPE_TRACK: i32 = 66;
/// `camera(DISTANCE, ...)`.
pub const TYPE_DISTANCE: i32 = 67;
/// `camera(FOLLOW, ...)`.
pub const TYPE_FOLLOW: i32 = 68;

/// "Wait forever": the value a `random` member holds until a `wait` draws it
/// (`Q3_INFINITE`).
pub const INFINITE: f32 = 16_777_216.0;

/// The version a block file must carry (`IBI_VERSION`).
pub const IBI_VERSION: f32 = 1.57;

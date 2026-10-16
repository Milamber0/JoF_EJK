//! What the first entity of a map publishes: `SP_worldspawn` (`g_spawn.c:1387-1520`).
use sjk_entity::Entity;

/// `GAME_VERSION` (`bg_public.h`): a client refuses a server whose string differs.
pub const GAME_VERSION: &[u8] = b"basejka-1";

/// Configstring indices `SP_worldspawn` writes (`bg_public.h`).
pub const CS_MUSIC: usize = 2;
/// The map's own message.
pub const CS_MESSAGE: usize = 3;
/// `g_motd`.
pub const CS_MOTD: usize = 4;
/// The warmup state: empty, or `-1` while waiting for players.
pub const CS_WARMUP: usize = 5;
/// [`GAME_VERSION`].
pub const CS_GAME_VERSION: usize = 20;
/// `level.startTime`.
pub const CS_LEVEL_START_TIME: usize = 21;
/// The map's ambient sound set.
pub const CS_GLOBAL_AMBIENT_SET: usize = 32;
/// 32 light styles of three colour strings each.
pub const CS_LIGHT_STYLES: usize = 1419;

/// `GT_DUEL`, `GT_POWERDUEL` and `GT_SIEGE` run their own warmup.
const OWN_WARMUP_GAMETYPES: [i32; 3] = [3, 4, 7];

/// `defaultStyles` (`g_spawn.c:1198`), generated from the reference's text.
const DEFAULT_STYLES: [[&[u8]; 3]; 32] = [
    [b"z", b"z", b"z"],
    [
        b"mmnmmommommnonmmonqnmmo",
        b"mmnmmommommnonmmonqnmmo",
        b"mmnmmommommnonmmonqnmmo",
    ],
    [
        b"abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcb",
        b"abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcb",
        b"abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcb",
    ],
    [
        b"mmmmmaaaaammmmmaaaaaabcdefgabcdefg",
        b"mmmmmaaaaammmmmaaaaaabcdefgabcdefg",
        b"mmmmmaaaaammmmmaaaaaabcdefgabcdefg",
    ],
    [b"mamamamamama", b"mamamamamama", b"mamamamamama"],
    [
        b"jklmnopqrstuvwxyzyxwvutsrqponmlkj",
        b"jklmnopqrstuvwxyzyxwvutsrqponmlkj",
        b"jklmnopqrstuvwxyzyxwvutsrqponmlkj",
    ],
    [
        b"nmonqnmomnmomomno",
        b"nmonqnmomnmomomno",
        b"nmonqnmomnmomomno",
    ],
    [
        b"mmmaaaabcdefgmmmmaaaammmaamm",
        b"mmmaaaabcdefgmmmmaaaammmaamm",
        b"mmmaaaabcdefgmmmmaaaammmaamm",
    ],
    [
        b"mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa",
        b"mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa",
        b"mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa",
    ],
    [
        b"aaaaaaaazzzzzzzz",
        b"aaaaaaaazzzzzzzz",
        b"aaaaaaaazzzzzzzz",
    ],
    [
        b"mmamammmmammamamaaamammma",
        b"mmamammmmammamamaaamammma",
        b"mmamammmmammamamaaamammma",
    ],
    [
        b"abcdefghijklmnopqrrqponmlkjihgfedcba",
        b"abcdefghijklmnopqrrqponmlkjihgfedcba",
        b"abcdefghijklmnopqrrqponmlkjihgfedcba",
    ],
    [b"mkigegik", b"mkigegik", b"mkigegik"],
    [
        b"abcdefghijklmqrstuvwxyz",
        b"zyxwvutsrqmlkjihgfedcba",
        b"aammbbzzccllcckkffyyggp",
    ],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
    [b"", b"", b""],
];

/// Server settings `SP_worldspawn` reads.
#[derive(Clone, Copy, Debug)]
pub struct WorldSettings<'a> {
    /// `g_gametype`.
    pub gametype: i32,
    /// `level.startTime`, in server milliseconds.
    pub start_time: i32,
    /// `g_motd`.
    pub motd: &'a [u8],
    /// `g_doWarmup`.
    pub do_warmup: bool,
    /// `g_restarted`: this map load is a `map_restart`.
    pub restarted: bool,
}

/// What worldspawn decided, besides the configstrings it wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    /// `g_gravity`, from the map's `gravity` key or 800, as the text a cvar holds.
    pub gravity: Vec<u8>,
    /// `level.warmupTime`: -1 while waiting for players, else 0.
    pub warmup_time: i32,
    /// `g_restarted` must be cleared.
    pub clear_restarted: bool,
    /// `g_cullDistance`: the map's `distanceCull` key, 6000 without one
    /// (`G_SpawnFloat`, `g_spawn.c:1395`), as far as a walker's crosshair reaches.
    pub distance_cull: f32,
}

/// The map cannot be run; the reference drops it with `ERR_DROP`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    /// The first entity of the lump is not a `worldspawn`.
    NotWorldspawn,
    /// A light style's three colour strings differ in length.
    LightStyleLengths(usize),
}

/// `G_SpawnString`: the *first* pair whose key matches without regard to case, where
/// `Entity::get` answers the last.
fn spawn_string<'a>(entity: &'a Entity, key: &str, default: &'a [u8]) -> &'a [u8] {
    entity
        .fields()
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map_or(default, |(_, value)| value.as_bytes())
}

/// Run worldspawn: `set` receives every configstring in the reference's order, empty
/// values included (they clear a string left from the previous map).
pub fn spawn_world(
    entity: &Entity,
    settings: WorldSettings<'_>,
    mut set: impl FnMut(usize, &[u8]),
) -> Result<World, WorldError> {
    if !spawn_string(entity, "classname", b"").eq_ignore_ascii_case(b"worldspawn") {
        return Err(WorldError::NotWorldspawn);
    }
    set(CS_GAME_VERSION, GAME_VERSION);
    set(
        CS_LEVEL_START_TIME,
        settings.start_time.to_string().as_bytes(),
    );
    set(CS_MUSIC, spawn_string(entity, "music", b""));
    set(CS_MESSAGE, spawn_string(entity, "message", b""));
    set(CS_MOTD, settings.motd);
    let gravity = spawn_string(entity, "gravity", b"800").to_vec();
    set(
        CS_GLOBAL_AMBIENT_SET,
        spawn_string(entity, "soundSet", b"default"),
    );
    set(CS_WARMUP, b"");
    let distance_cull = crate::text_parse::atof(spawn_string(entity, "distanceCull", b"6000.0"));
    let mut world = World {
        gravity,
        warmup_time: 0,
        clear_restarted: settings.restarted,
        distance_cull,
    };
    if !settings.restarted
        && settings.do_warmup
        && !OWN_WARMUP_GAMETYPES.contains(&settings.gametype)
    {
        world.warmup_time = -1;
        set(CS_WARMUP, b"-1");
    }
    for (style, defaults) in DEFAULT_STYLES.iter().enumerate() {
        // Style 0 is never overridden; the others by `ls_<n>r`, `ls_<n>g`, `ls_<n>b`.
        let mut lengths = [0; 3];
        for (colour, suffix) in ["r", "g", "b"].into_iter().enumerate() {
            let value = match style {
                0 => defaults[colour],
                _ => spawn_string(entity, &format!("ls_{style}{suffix}"), defaults[colour]),
            };
            lengths[colour] = value.len();
            set(CS_LIGHT_STYLES + style * 3 + colour, value);
            // The reference compares after writing blue, so red and green of a bad
            // style are already published when it drops the map.
        }
        if lengths[0] != lengths[1] || lengths[1] != lengths[2] {
            return Err(WorldError::LightStyleLengths(style));
        }
    }
    Ok(world)
}

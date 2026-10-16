//! What a script can read and write of an entity (the `gentity_t` fields `g_ICARUScb.c`
//! touches), and the entities that exist to run scripts: `target_scriptrunner`,
//! `func_static` and `trigger_always`.
//!
//! A server keeps one [`ScriptEntity`] for every entity the interpreter knows. Where the
//! entity is also something else the server runs (a player, a trigger, a door), the
//! server reads what a script changed here — a trigger switched off, a door locked —
//! and applies it there.

use sjk_entity::Entity;

use crate::script_mover::ScriptMover;

/// `NUM_BSETS`: the behaviour sets an entity can name scripts for.
pub const NUM_BSETS: usize = 17;
/// `BSET_SPAWN`, `BSET_USE`, `BSET_DEATH`, ... (`bSet_t`).
pub const BSET_SPAWN: usize = 0;
pub const BSET_USE: usize = 1;
pub const BSET_AWAKE: usize = 2;
pub const BSET_ANGER: usize = 3;
pub const BSET_ATTACK: usize = 4;
pub const BSET_VICTORY: usize = 5;
pub const BSET_LOSTENEMY: usize = 6;
pub const BSET_PAIN: usize = 7;
pub const BSET_FLEE: usize = 8;
pub const BSET_DEATH: usize = 9;
pub const BSET_DELAYED: usize = 10;
pub const BSET_BLOCKED: usize = 11;
pub const BSET_FFIRE: usize = 14;
pub const BSET_FFDEATH: usize = 15;
pub const BSET_MINDTRICK: usize = 16;

/// The spawn keys of the behaviour sets (`g_spawn.c`'s field table), by set.
pub const BSET_KEYS: [(&str, usize); 15] = [
    ("spawnscript", BSET_SPAWN),
    ("usescript", BSET_USE),
    ("awakescript", BSET_AWAKE),
    ("angerscript", BSET_ANGER),
    ("attackscript", BSET_ATTACK),
    ("victoryscript", BSET_VICTORY),
    ("lostenemyscript", BSET_LOSTENEMY),
    ("painscript", BSET_PAIN),
    ("fleescript", BSET_FLEE),
    ("deathscript", BSET_DEATH),
    ("delayscript", BSET_DELAYED),
    ("blockedscript", BSET_BLOCKED),
    ("ffirescript", BSET_FFIRE),
    ("ffdeathscript", BSET_FFDEATH),
    ("mindtrickscript", BSET_MINDTRICK),
];

/// `BSTable`: behaviour-set values that name a behaviour state, not a script.
pub const BEHAVIOR_STATES: [&str; 10] = [
    "BS_DEFAULT",
    "BS_ADVANCE_FIGHT",
    "BS_SLEEP",
    "BS_FOLLOW_LEADER",
    "BS_JUMP",
    "BS_SEARCH",
    "BS_WANDER",
    "BS_NOCLIP",
    "BS_REMOVE",
    "BS_CINEMATIC",
];

/// Task slots (`taskID_t`).
pub const TID_CHAN_VOICE: usize = 0;
pub const TID_ANIM_UPPER: usize = 1;
pub const TID_ANIM_LOWER: usize = 2;
pub const TID_ANIM_BOTH: usize = 3;
pub const TID_MOVE_NAV: usize = 4;
pub const TID_ANGLE_FACE: usize = 5;
pub const TID_BSTATE: usize = 6;
pub const TID_LOCATION: usize = 7;
pub const TID_RESIZE: usize = 8;
pub const TID_SHOOT: usize = 9;
/// `NUM_TIDS`.
pub const NUM_TIDS: usize = 10;

/// `gentity_t::flags` a script sets.
pub const FL_GODMODE: u32 = 0x0000_0010;
pub const FL_NOTARGET: u32 = 0x0000_0020;
pub const FL_NO_KNOCKBACK: u32 = 0x0000_0800;
pub const FL_INACTIVE: u32 = 0x0001_0000;
pub const FL_DONT_SHOOT: u32 = 0x0004_0000;
pub const FL_UNDYING: u32 = 0x0010_0000;
/// `r.svFlags` a script sets.
pub const SVF_NOCLIENT: u32 = 0x0000_0001;
pub const SVF_PLAYER_USABLE: u32 = 0x0000_0010;
pub const SVF_BROADCAST: u32 = 0x0000_0020;
pub const SVF_USE_CURRENT_ORIGIN: u32 = 0x0000_0080;
pub const SVF_ICARUS_FREEZE: u32 = 0x0000_8000;
/// `s.eFlags` a script sets.
pub const EF_NODRAW: u32 = 1 << 8;
pub const EF_SHADER_ANIM: u32 = 1 << 30;
/// `r.contents` a script sets.
pub const CONTENTS_SOLID: u32 = 1;
pub const CONTENTS_BODY: u32 = 0x100;
pub const CONTENTS_CORPSE: u32 = 0x200;
pub const CONTENTS_TRIGGER: u32 = 0x400;
/// `MAX_PARMS` and `MAX_PARM_STRING_LENGTH` (`MAX_QPATH`).
pub const MAX_PARMS: usize = 16;
pub const MAX_PARM_STRING_LENGTH: usize = 64;

/// What kind of thing an entity is, as the callbacks tell them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    /// A player (`ent->client` without `ent->NPC`).
    Client,
    /// An NPC (`ent->client` and `ent->NPC`).
    Npc,
    /// Anything else.
    Other,
}

/// The entity's think, where it is one this module runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Think {
    /// None (`think == NULL`).
    #[default]
    Nothing,
    /// `scriptrunner_run`: a scriptrunner's delay is up (or the world's spawn script).
    ScriptRunner,
    /// `trigger_always_think`: it fires its targets and goes.
    TriggerAlways,
    /// `anglerCallback`: a scripted turn is over.
    StopTurning,
    /// `G_FreeEntity`: a script removed it.
    Free,
    /// `MoveOwner`: a helper that teleports its owner once the spot is clear.
    MoveOwner,
    /// `SolidifyOwner`: a helper that makes its owner solid once nothing is in the way.
    SolidifyOwner,
    /// A think of the server's own for this kind of entity.
    Other,
}

/// What using the entity does (`ent->use`), where it is one this module runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Uses {
    /// Nothing can use it (`use == NULL`).
    #[default]
    Nothing,
    /// `target_scriptrunner_use`.
    ScriptRunner,
    /// `func_static_use`.
    FuncStatic,
    /// A use of the server's own for this kind of entity.
    Other,
}

/// An entity as a script sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptEntity<Id> {
    pub classname: String,
    pub kind: EntityKind,
    pub targetname: Option<String>,
    pub script_targetname: Option<String>,
    pub target: Option<String>,
    /// `ownername`: whose reference tags it sees first.
    pub ownername: Option<String>,
    pub full_name: Option<String>,
    /// `parms`, allocated by the first parm set.
    pub parms: Option<Box<[String; MAX_PARMS]>>,
    pub behavior_sets: [Option<String>; NUM_BSETS],
    pub count: i32,
    pub health: i32,
    pub wait: f32,
    /// `delay` in milliseconds.
    pub delay: i32,
    pub spawnflags: u32,
    pub flags: u32,
    pub svflags: u32,
    pub eflags: u32,
    pub contents: u32,
    /// `clipmask`.
    pub clipmask: u32,
    /// `r.mins` and `r.maxs`.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `r.ownerNum`: whom a helper entity works for.
    pub owner: Option<Id>,
    /// `s.iModelScale`.
    pub model_scale: i32,
    /// `s.frame`: a `SWITCH_SHADER` func_static's shader stage.
    pub frame: i32,
    /// `s.loopSound`.
    pub loop_sound: u16,
    /// `s.origin` and `s.angles` as the map set them (`s.angles` also as a script does).
    pub spawn_origin: [f32; 3],
    pub spawn_angles: [f32; 3],
    /// Where it is and how it moves.
    pub motion: ScriptMover,
    pub think: Think,
    pub next_think: i32,
    /// `ent->use`.
    pub uses: Uses,
    /// `activator` and `enemy` (`other` of a scriptrunner's last use).
    pub activator: Option<Id>,
    pub enemy: Option<Id>,
}

impl<Id> ScriptEntity<Id> {
    /// A fresh entity of a classname, standing at `origin`.
    pub fn new(classname: &str, kind: EntityKind, origin: [f32; 3], angles: [f32; 3]) -> Self {
        Self {
            classname: classname.to_owned(),
            kind,
            targetname: None,
            script_targetname: None,
            target: None,
            ownername: None,
            full_name: None,
            parms: None,
            behavior_sets: Default::default(),
            count: 0,
            health: 0,
            wait: 0.0,
            delay: 0,
            spawnflags: 0,
            flags: 0,
            svflags: 0,
            eflags: 0,
            contents: 0,
            clipmask: 0,
            mins: [0.0; 3],
            maxs: [0.0; 3],
            owner: None,
            model_scale: 0,
            frame: 0,
            loop_sound: 0,
            spawn_origin: origin,
            spawn_angles: angles,
            motion: ScriptMover::standing(origin, angles),
            think: Think::Nothing,
            next_think: 0,
            uses: Uses::Nothing,
            activator: None,
            enemy: None,
        }
    }

    /// The map's keys a script reads, as `G_ParseField` sets them: the names, the
    /// behaviour sets, `parm1`..`parm16`, `count`, `wait`, `health`, `spawnflags`.
    pub fn from_map(entity: &Entity, kind: EntityKind) -> Self {
        let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
        let angles = match entity.vector("angles").ok().flatten() {
            Some(angles) => angles,
            None => [
                0.0,
                entity
                    .get("angle")
                    .map_or(0.0, |angle| crate::text_parse::atof(angle.as_bytes())),
                0.0,
            ],
        };
        let mut this = Self::new(
            entity.get("classname").unwrap_or_default(),
            kind,
            origin,
            angles,
        );
        // `G_SpawnGEntityFromSpawnVars` carries the origin into `s.pos` and
        // `r.currentOrigin`; the angles stay in `s.angles` until a spawn function sets them.
        this.motion = ScriptMover::standing(origin, [0.0; 3]);
        // `G_ParseField` for each key in turn: the last of a repeated key stands.
        let text = |key: &str| {
            entity
                .fields()
                .iter()
                .rev()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value.clone())
        };
        this.targetname = text("targetname");
        this.script_targetname = text("script_targetname");
        this.target = text("target");
        this.ownername = text("ownername");
        this.full_name = text("fullName");
        for (key, set) in BSET_KEYS {
            this.behavior_sets[set] = text(key);
        }
        // `F_PARMn` goes through `Q3_SetParm`, in the order the keys come.
        for (key, value) in entity.fields() {
            if let Some(parm) =
                (1..=MAX_PARMS).find(|parm| key.eq_ignore_ascii_case(&format!("parm{parm}")))
            {
                let parms = this.parms.get_or_insert_with(Default::default);
                parms[parm - 1] = parm_text(&parms[parm - 1], value).0;
            }
        }
        let int = |key: &str| text(key).map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
        let float =
            |key: &str| text(key).map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()));
        this.count = int("count");
        this.health = int("health");
        this.wait = float("wait");
        this.spawnflags = int("spawnflags") as u32;
        this
    }

    /// The parm, if parms exist.
    pub fn parm(&self, index: usize) -> Option<&str> {
        self.parms
            .as_ref()
            .and_then(|parms| parms.get(index))
            .map(String::as_str)
    }

    /// Whether it is a client (a player or an NPC).
    pub fn is_client(&self) -> bool {
        self.kind != EntityKind::Other
    }

    /// Whether it is an NPC.
    pub fn is_npc(&self) -> bool {
        self.kind == EntityKind::Npc
    }
}

/// `Q3_SetParm`'s new text for a parm (`g_ICARUScb.c:4014-4057`): `+n` and `-n` change
/// the number it holds (printed `%f`), anything else is copied into the 64-byte parm.
/// Also answers whether the copy was cut short.
pub fn parm_text(previous: &str, data: &str) -> (String, bool) {
    let bytes = data.as_bytes();
    let change = match bytes.first() {
        Some(b'+') if bytes.len() > 1 => sjk_icarus::cnum::atof(&data[1..]) as f32,
        Some(b'-') if bytes.len() > 1 => sjk_icarus::cnum::atof(&data[1..]) as f32 * -1.0,
        _ => 0.0,
    };
    if change != 0.0 {
        return (
            sjk_icarus::cnum::format_f(change + sjk_icarus::cnum::atof(previous) as f32),
            false,
        );
    }
    if data.len() >= MAX_PARM_STRING_LENGTH {
        return (cut(data, MAX_PARM_STRING_LENGTH - 1).to_owned(), true);
    }
    (data.to_owned(), false)
}

/// `strncpy` into a buffer of `limit + 1` bytes, cut at a character boundary.
pub(crate) fn cut(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Whether a map entity will run scripts once spawned ([`valid_for_scripts`] read off its
/// keys): a non-empty `script_targetname` or any behaviour set. A server that keeps such
/// an entity with its scripts keeps it there alone.
pub fn runs_scripts(entity: &Entity) -> bool {
    let set = |key: &str| entity.get(key).is_some_and(|value| !value.is_empty());
    set("script_targetname") || BSET_KEYS.iter().any(|(key, _)| set(key))
}

/// `ICARUS_ValidEnt`: whether an entity runs scripts — it has a `script_targetname`, or
/// any behaviour set (then it is named by its `targetname`, which the reference copies
/// over, `GameInterface.cpp:278-310`).
pub fn valid_for_scripts<Id>(entity: &mut ScriptEntity<Id>) -> bool {
    if entity
        .script_targetname
        .as_deref()
        .is_some_and(|name| !name.is_empty())
    {
        return true;
    }
    if entity
        .behavior_sets
        .iter()
        .any(|set| set.as_deref().is_some_and(|name| !name.is_empty()))
    {
        entity.script_targetname = entity.targetname.clone();
        return true;
    }
    false
}

/// The scripts `ICARUS_PrecacheEnt` precaches for an entity: every behaviour set that
/// names no behaviour state, as a path under `scripts/`.
pub fn scripts_to_precache<Id>(entity: &ScriptEntity<Id>) -> impl Iterator<Item = String> + '_ {
    entity
        .behavior_sets
        .iter()
        .flatten()
        .filter(|name| {
            !BEHAVIOR_STATES
                .iter()
                .any(|state| state.eq_ignore_ascii_case(name))
        })
        .map(|name| format!("scripts/{name}"))
}

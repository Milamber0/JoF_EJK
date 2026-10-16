//! The world a script acts on: what `g_ICARUScb.c` reaches through `g_entities`, the
//! clients' player states, `level` and the engine's traps, as one trait a server (or a
//! test) implements.
//!
//! The game's script callbacks ([`crate::script_calls`], [`crate::script_set`],
//! [`crate::script_gets`]) and the entities that run scripts
//! ([`crate::script_runner`]) are written against [`ScriptWorld`] only. Every entity a
//! script can name is a [`ScriptEntity`]; a client's player state and an NPC's mind are
//! the world's, reached through [`ClientView`], [`ClientAction`] and [`NpcAction`].

use sjk_icarus::{Icarus, Owner};

use crate::ref_tags::RefTags;
use crate::script_entity::ScriptEntity;

/// Which name `G_Find` compares (`FOFS(targetname)`, `FOFS(script_targetname)`,
/// `FOFS(NPC_targetname)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameField {
    Targetname,
    ScriptTargetname,
    NpcTargetname,
}

/// A client's state a script reads (`ent->health` and `ent->client->ps`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClientView {
    /// `ent->health`.
    pub health: i32,
    /// `ps.stats[STAT_HEALTH]`, `[STAT_ARMOR]`, `[STAT_MAX_HEALTH]`.
    pub stat_health: i32,
    pub armor: i32,
    pub max_health: i32,
    /// `ps.velocity`.
    pub velocity: [f32; 3],
    /// `ps.legsTimer` and `ps.torsoTimer`.
    pub legs_timer: i32,
    pub torso_timer: i32,
    /// `ps.viewheight`.
    pub view_height: i32,
    /// `sess.sessionTeam == TEAM_SPECTATOR`.
    pub spectator: bool,
    /// `tempSpectate >= level.time`.
    pub temp_spectating: bool,
    /// `ps.saberHolstered` and `BG_SabersOff`.
    pub saber_holstered: i32,
    pub sabers_off: bool,
    /// `renderInfo.eyePoint`.
    pub eye_point: [f32; 3],
}

/// A change a script makes to a client's state.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientAction {
    /// `ent->health` and `ps.stats[STAT_HEALTH]`.
    SetHealth { health: i32, stat: i32 },
    /// `ps.stats[STAT_ARMOR]`.
    SetArmor(i32),
    /// `Q3_SetHealth` of zero: `FL_GODMODE` cleared, health -999 and
    /// `player_die(ent, ent, ent, 100000, MOD_FALLING)`.
    Die,
    /// `Q3_SetOrigin` on a client: to `origin` one unit up, still, knockback time 160,
    /// `EF_TELEPORT_BIT` toggled.
    Teleport([f32; 3]),
    /// `SetClientViewAngle`.
    SetViewAngles([f32; 3]),
    /// `Q3_SetVelocity`: added to one axis, knockback time 500.
    AddVelocity { axis: usize, speed: f32 },
    /// `ps.gravity`.
    SetGravity(f32),
    /// `ps.iModelScale`.
    SetModelScale(i32),
    /// `EF_NODRAW` on the player state.
    SetNoDraw(bool),
    /// `ps.fd.forcePowerLevel[power]`, and the power known while its level is not zero.
    SetForcePowerLevel { power: i32, level: i32 },
    /// `Q3_SetWeapon`: the weapons are only this one (`WPTable` name) and it is changed to.
    SetWeapon(String),
    /// `Cmd_ToggleSaber_f`.
    ToggleSaber,
    /// `G_SetAnim(... SETANIM_LEGS or SETANIM_TORSO, anim, RESTART|HOLD|OVERRIDE)`.
    SetAnimation { torso: bool, animation: i32 },
}

/// A change a script makes to an NPC's mind, behind the reference's `ent->NPC` checks
/// (which the callers make, with the reference's messages).
#[derive(Clone, Debug, PartialEq)]
pub enum NpcAction {
    /// A `Q3_Set` type whose NPC branch the world applies; the value is the script's
    /// text. The world answers whether the task completes at once.
    Set { set: i32, value: String },
    /// `G_ActivateBehavior` of a behaviour set naming a behaviour state
    /// (`tempBehavior = BS_DEFAULT; behaviorState = ...`).
    BehaviorState(String),
}

/// The world around the scripts.
pub trait ScriptWorld {
    /// The world's identity of an entity (the interpreter's owner).
    type Id: Owner;

    // ---- the engine and `level` ----

    /// `level.time`.
    fn level_time(&self) -> i32;
    /// `level.previousTime`.
    fn previous_time(&self) -> i32;
    /// `svs.time`, the interpreter's clock.
    fn server_time(&self) -> u32;
    /// `developer.integer` (the game's developer messages print at 2).
    fn developer(&self) -> i32;
    /// A console line from the game (`Com_Printf`, `trap->Print`).
    fn print(&mut self, text: &str);
    /// A console line from the interpreter itself (the engine's `Com_Printf`): the same
    /// console unless the world tells them apart.
    fn interpreter_print(&mut self, text: &str) {
        self.print(text);
    }
    /// A reliable command to every client.
    fn broadcast_command(&mut self, command: &str);
    /// A game file's bytes.
    fn read_file(&mut self, path: &str) -> Option<Vec<u8>>;
    /// The engine's random float in `[min, max)` (`Q_flrand` in the engine).
    fn engine_random(&mut self, min: f32, max: f32) -> f32;
    /// `G_SoundIndex`.
    fn sound_index(&mut self, name: &str) -> i32;
    /// `G_SoundSetIndex`.
    fn sound_set_index(&mut self, name: &str) -> i32;
    /// `trap->Cvar_VariableStringBuffer("timescale")`, as a float.
    fn timescale(&self) -> f32;
    /// `trap->Cvar_Set("timescale", value)`.
    fn set_timescale(&mut self, value: &str);
    /// `g_gravity.value`.
    fn gravity(&self) -> f32;
    /// The level's reference tags.
    fn tags(&self) -> &RefTags;
    /// `GetIDForString(animTable, name)`.
    fn animation(&self, name: &str) -> Option<i32>;
    /// The animation names of a client's legs and torso (`animTable[ps.legsAnim]`,
    /// `animTable[ps.torsoAnim]`).
    fn client_animations(&self, id: Self::Id) -> Option<(String, String)>;

    // ---- entities ----

    /// An entity by its identity, if in use.
    fn entity(&self, id: Self::Id) -> Option<&ScriptEntity<Self::Id>>;
    /// The same, to change.
    fn entity_mut(&mut self, id: Self::Id) -> Option<&mut ScriptEntity<Self::Id>>;
    /// `G_Find`: the next entity after `after` (in the reference's number order) whose
    /// name field equals `name` (case-insensitive, as `Q_stricmp`).
    fn find(&self, after: Option<Self::Id>, field: NameField, name: &str) -> Option<Self::Id>;
    /// `G_Spawn`: a new, empty entity of `classname`.
    fn spawn(&mut self, classname: &str) -> Option<Self::Id>;
    /// `G_FreeEntity` after its `ICARUS_FreeEnt` ([`crate::script_runner::free`]): the
    /// entity is gone.
    fn free(&mut self, icarus: &mut Icarus<Self::Id>, id: Self::Id);
    /// `numNewICARUSEnts++`: the number for a name made up for an activator that has
    /// none.
    fn next_new_script_name(&mut self) -> i32;
    /// `G_UseTargets2(ent, activator, target)` over every entity, scripted or not.
    fn use_targets(
        &mut self,
        icarus: &mut Icarus<Self::Id>,
        ent: Self::Id,
        activator: Option<Self::Id>,
        target: &str,
    );
    /// `victim->die(victim, victim, victim, damage, MOD_UNKNOWN)`, if it has one.
    fn die(&mut self, icarus: &mut Icarus<Self::Id>, victim: Self::Id, damage: i32);
    /// `SpotWouldTelefrag2(mover, dest)`.
    fn spot_would_telefrag(&self, mover: Self::Id, dest: [f32; 3]) -> bool;
    /// The entity's new place is to be linked (`trap->LinkEntity`); a mover's contents
    /// or motion changed.
    fn link(&mut self, id: Self::Id);
    /// `G_Sound(ent, channel, index)`: `channel` is `CHAN_AUTO` (0) or `CHAN_VOICE` (3).
    fn sound(&mut self, id: Self::Id, channel: i32, index: i32);
    /// `G_TempEntity(origin, EV_GLOBAL_SOUND)` broadcast with the sound.
    fn global_sound(&mut self, origin: [f32; 3], index: i32);
    /// `G_AddEvent(ent, EV_PLAYDOORSOUND, type)` with the soundset index set.
    fn door_sound(&mut self, id: Self::Id, sound_set: i32, kind: u32);
    /// `trap->ROFF_Cache`: the id of a ROFF file, or 0 when there is none.
    fn cache_roff(&mut self, name: &str) -> i32;
    /// `trap->ROFF_Play`.
    fn play_roff(&mut self, id: Self::Id, roff: i32);
    /// `LockDoors` / `UnLockDoors` for the entity.
    fn lock_doors(&mut self, id: Self::Id, locked: bool);
    /// `G_MoverPush` of a scripted mover by `move_by` and `turn_by`: whether nothing
    /// blocked it; the obstacle if something did.
    fn push_mover(
        &mut self,
        icarus: &mut Icarus<Self::Id>,
        id: Self::Id,
        move_by: [f32; 3],
        turn_by: [f32; 3],
    ) -> Result<(), Option<Self::Id>>;
    /// `Blocked_Mover(ent, other)`: frees or damages what blocked a scripted mover.
    fn mover_blocked(
        &mut self,
        icarus: &mut Icarus<Self::Id>,
        id: Self::Id,
        obstacle: Option<Self::Id>,
    );
    /// A think of the world's own (`Think::Other`).
    fn run_own_think(&mut self, icarus: &mut Icarus<Self::Id>, id: Self::Id);

    // ---- clients and NPCs ----

    /// A client's state, for a player or an NPC.
    fn client(&self, id: Self::Id) -> Option<ClientView>;
    /// A change to a client's state.
    fn client_action(&mut self, icarus: &mut Icarus<Self::Id>, id: Self::Id, action: ClientAction);
    /// An NPC's part of a set: whether its task completes at once.
    fn npc_action(
        &mut self,
        icarus: &mut Icarus<Self::Id>,
        task: i32,
        id: Self::Id,
        action: NpcAction,
    ) -> bool;
}

/// `G_DebugPrint` (`g_ICARUScb.c:293-345`): the game's developer message, only at
/// `developer 2`; `WL_DEBUG` is never used by the game side.
pub fn debug_print<W: ScriptWorld + ?Sized>(
    world: &mut W,
    level: sjk_icarus::DebugLevel,
    text: &str,
) {
    if world.developer() != 2 {
        return;
    }
    let text = crate::script_entity::cut(text, 1023);
    let line = match level {
        sjk_icarus::DebugLevel::Error => format!("^1ERROR: {text}"),
        sjk_icarus::DebugLevel::Warning => format!("^3WARNING: {text}"),
        _ => format!("^2INFO: {text}"),
    };
    world.print(&line);
}

/// A C string argument as `%s` prints it: `(null)` for none.
pub fn c_str(text: Option<&str>) -> &str {
    text.unwrap_or("(null)")
}

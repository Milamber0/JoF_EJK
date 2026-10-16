//! A script's `set(name, value)` (`Q3_Set`, `g_ICARUScb.c:5873-6938`): an entity field
//! by name, or — for a name the game does not know — a script variable.
//!
//! Every set type is here with its multiplayer behaviour. What only an NPC takes is
//! refused with the reference's message on anything else and handed to the world's NPC
//! code ([`ScriptWorld::npc_action`]) for an NPC. The reference crashes on two sets of
//! a non-client (`SET_WEAPON`, `SET_SABERACTIVE`) and on `SET_COPY_ORIGIN` of one; here
//! those do nothing past the reference's message.

use sjk_icarus::{DebugLevel, Icarus, cnum};

use crate::icarus_set_table::*;
use crate::script_entity::{
    BSET_ANGER, BSET_ATTACK, BSET_AWAKE, BSET_BLOCKED, BSET_DEATH, BSET_DELAYED, BSET_FFDEATH,
    BSET_FFIRE, BSET_FLEE, BSET_MINDTRICK, BSET_PAIN, BSET_SPAWN, BSET_USE, BSET_VICTORY,
    CONTENTS_BODY, CONTENTS_CORPSE, EF_NODRAW, FL_DONT_SHOOT, FL_GODMODE, FL_INACTIVE,
    FL_NO_KNOCKBACK, FL_NOTARGET, FL_UNDYING, MAX_PARMS, SVF_ICARUS_FREEZE, SVF_NOCLIENT,
    SVF_PLAYER_USABLE, TID_ANIM_BOTH, TID_ANIM_LOWER, TID_ANIM_UPPER, TID_MOVE_NAV, TID_RESIZE,
    Think,
};
use crate::script_set_tables::{CLIENT_UNSUPPORTED, Refusal, UNSUPPORTED, called};
use crate::script_world::{ClientAction, NameField, NpcAction, ScriptWorld, c_str, debug_print};

/// `FRAMETIME`: the helpers a set spawns think a frame later.
pub const FRAMETIME: i32 = 100;

/// `Q3_Set`: true if the task is complete at once.
pub fn set<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    name: &str,
    data: &str,
) -> bool {
    let set = set_id(name);
    if world.entity(id).is_none() {
        return true;
    }
    if let Some(done) = set_motion(world, icarus, task, id, set, name, data) {
        return done;
    }
    if let Some(done) = crate::script_set_npc::set_npc(world, icarus, task, id, set, data) {
        return done;
    }
    if let Some(done) = set_animation(world, icarus, task, id, set, data) {
        return done;
    }
    if let Some(entry) = UNSUPPORTED.iter().find(|entry| entry.set == set) {
        if called(entry.calls, data) {
            debug_print(world, entry.level, entry.message);
        }
        return true;
    }
    if let Some(&(_, setter, warning)) = CLIENT_UNSUPPORTED
        .iter()
        .find(|(entry, _, _)| *entry == set)
    {
        if client_or_refuse(world, id, setter) {
            debug_print(world, DebugLevel::Warning, warning);
        }
        return true;
    }
    if set == SET_SOLID {
        return set_solid(world, icarus, task, id, is_true(data));
    }
    if set_field(world, icarus, id, set, data) || set_flag(world, icarus, id, set, data) {
        return true;
    }
    if set == SET_MENU_SCREEN || set == SET_SKILL {
        return true;
    }
    // `default`: a variable of that name (`ICARUS_SetVar`).
    icarus.set_variable(&mut crate::script_host::ScriptHost::new(world), name, data);
    true
}

/// `sscanf(data, "%f %f %f")`, cleared with the reference's warning if it fails.
fn vector<W: ScriptWorld>(world: &mut W, data: &str, label: &str, name: &str) -> [f32; 3] {
    let mut value = [0.0; 3];
    if cnum::scan_vector(data, &mut value) != 3 {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_Set: failed sscanf on {label} ({name})\n"),
        );
        value = [0.0; 3];
    }
    value
}

/// `atof` as the game's setters take it.
pub(crate) fn float(data: &str) -> f32 {
    cnum::atof(data) as f32
}

/// Whether the value is `"true"` (`Q_stricmp`).
fn is_true(data: &str) -> bool {
    data.eq_ignore_ascii_case("true")
}

/// The entity's targetname as `%s` prints it.
pub(crate) fn targetname<W: ScriptWorld>(world: &W, id: W::Id) -> String {
    c_str(world.entity(id).and_then(|ent| ent.targetname.as_deref())).to_owned()
}

/// A client check with the `'<targetname>' is not an NPC/player!` refusal.
pub(crate) fn client_or_refuse<W: ScriptWorld>(world: &mut W, id: W::Id, setter: &str) -> bool {
    if world.entity(id).is_some_and(|ent| ent.is_client()) {
        return true;
    }
    let line = format!(
        "{setter}: '{}' is not an NPC/player!\n",
        targetname(world, id)
    );
    debug_print(world, DebugLevel::Error, &line);
    false
}

/// An NPC check with one of the reference's refusals.
pub(crate) fn npc_or_refuse<W: ScriptWorld>(
    world: &mut W,
    id: W::Id,
    setter: &str,
    refusal: Refusal,
) -> bool {
    if world.entity(id).is_some_and(|ent| ent.is_npc()) {
        return true;
    }
    let name = targetname(world, id);
    let (level, line) = match refusal {
        Refusal::Plain => (
            DebugLevel::Error,
            format!("{setter}: '{name}' is not an NPC\n"),
        ),
        Refusal::Exclaimed => (
            DebugLevel::Error,
            format!("{setter}: '{name}' is not an NPC!\n"),
        ),
        Refusal::Warned => (
            DebugLevel::Warning,
            format!("{setter}: ent {name} is not an NPC!\n"),
        ),
    };
    debug_print(world, level, &line);
    false
}

/// Places and motion: `SET_ORIGIN`, `SET_TELEPORT_DEST`, `SET_COPY_ORIGIN`,
/// `SET_ANGLES`, the velocities and `SET_Z_OFFSET`.
fn set_motion<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    set: i32,
    name: &str,
    data: &str,
) -> Option<bool> {
    match set {
        SET_ORIGIN => {
            let origin = vector(world, data, "SET_ORIGIN", name);
            let ent = world.entity_mut(id)?;
            ent.motion.set_origin(origin);
            if ent
                .classname
                .get(..4)
                .is_some_and(|prefix| prefix == "NPC_")
            {
                ent.spawn_origin = origin;
            }
            world.link(id);
        }
        SET_TELEPORT_DEST => {
            let origin = vector(world, data, "SET_TELEPORT_DEST", name);
            if !teleport_dest(world, id, origin) {
                icarus.task_id_set(id, TID_MOVE_NAV, task);
                return Some(false);
            }
        }
        SET_COPY_ORIGIN => copy_origin(world, icarus, id, data),
        SET_ANGLES => {
            let angles = vector(world, data, "SET_ANGLES", name);
            set_angles(world, icarus, id, angles);
        }
        SET_XVELOCITY | SET_YVELOCITY | SET_ZVELOCITY => {
            let axis = (set - SET_XVELOCITY) as usize;
            if world.entity(id).is_some_and(|ent| ent.is_client()) {
                world.client_action(
                    icarus,
                    id,
                    ClientAction::AddVelocity {
                        axis,
                        speed: float(data),
                    },
                );
            } else {
                debug_print(
                    world,
                    DebugLevel::Warning,
                    &format!("Q3_SetVelocity: not a client {id}\n"),
                );
            }
        }
        SET_Z_OFFSET => origin_offset(world, icarus, id, 2, float(data)),
        _ => return None,
    }
    Some(true)
}

/// `Q3_SetTeleportDest` (`:1916-1942`): there now, or a helper that moves it there
/// once the spot is clear (then false: the task waits).
fn teleport_dest<W: ScriptWorld>(world: &mut W, id: W::Id, origin: [f32; 3]) -> bool {
    if !world.spot_would_telefrag(id, origin) {
        if let Some(ent) = world.entity_mut(id) {
            ent.motion.set_origin(origin);
        }
        world.link(id);
        return true;
    }
    spawn_helper(world, id, origin, Think::MoveOwner);
    false
}

/// A helper entity that thinks for its owner a frame later (`MoveOwner`,
/// `SolidifyOwner`).
fn spawn_helper<W: ScriptWorld>(world: &mut W, owner: W::Id, origin: [f32; 3], think: Think) {
    let time = world.level_time();
    let Some(helper) = world.spawn("noclass") else {
        return;
    };
    if let Some(ent) = world.entity_mut(helper) {
        ent.motion.set_origin(origin);
        ent.owner = Some(owner);
        ent.think = think;
        ent.next_think = time + FRAMETIME;
    }
}

/// `Q3_SetOrigin` (`:1950-1984`): a client is teleported, anything else placed.
fn set_origin<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    origin: [f32; 3],
) {
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if ent.is_client() {
        ent.motion.origin = origin;
        world.client_action(icarus, id, ClientAction::Teleport(origin));
    } else {
        ent.motion.set_origin(origin);
    }
    world.link(id);
}

/// `Q3_SetCopyOrigin` (`:1991-2005`).
fn copy_origin<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id, name: &str) {
    let Some(found) = world.find(None, NameField::Targetname, name) else {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_SetCopyOrigin: ent {name} not found!\n"),
        );
        return;
    };
    let Some((origin, angles)) = world
        .entity(found)
        .map(|ent| (ent.motion.origin, ent.spawn_angles))
    else {
        return;
    };
    set_origin(world, icarus, id, origin);
    // `SetClientViewAngle` reads the client; the reference crashes on anything else.
    if world.entity(id).is_some_and(|ent| ent.is_client()) {
        world.client_action(icarus, id, ClientAction::SetViewAngles(angles));
    }
}

/// `Q3_SetAngles` (`:2043-2064`).
fn set_angles<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    angles: [f32; 3],
) {
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if ent.is_client() {
        world.client_action(icarus, id, ClientAction::SetViewAngles(angles));
    } else {
        ent.spawn_angles = angles;
    }
    world.link(id);
}

/// `Q3_SetOriginOffset` (`:2135-2162`): a move of the entity's spawn place by
/// `offset` along an axis, at its speed.
fn origin_offset<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    axis: usize,
    offset: f32,
) {
    let Some(ent) = world.entity(id) else { return };
    if ent.is_client() || ent.classname.eq_ignore_ascii_case("target_scriptrunner") {
        debug_print(
            world,
            DebugLevel::Error,
            &format!("Q3_SetOriginOffset: ent {id} is NOT a mover!\n"),
        );
        return;
    }
    let mut origin = ent.spawn_origin;
    origin[axis] += offset;
    let speed = ent.motion.speed;
    // `fabs(offset)/fabs(speed)*1000.0f`: double arithmetic, stored as a float.
    let duration = if speed != 0.0 {
        (f64::from(offset).abs() / f64::from(speed).abs() * 1000.0) as f32
    } else {
        0.0
    };
    crate::script_calls::lerp_to_origin(world, icarus, -1, id, origin, duration);
}

/// The animation sets: a client's legs or torso, the hold times and frames that
/// multiplayer does not support.
fn set_animation<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    set: i32,
    data: &str,
) -> Option<bool> {
    match set {
        SET_ANIM_UPPER | SET_ANIM_LOWER => {
            let (torso, slot) = if set == SET_ANIM_UPPER {
                (true, TID_ANIM_UPPER)
            } else {
                (false, TID_ANIM_LOWER)
            };
            if set_anim(world, icarus, id, torso, data) {
                icarus.task_id_clear(id, TID_ANIM_BOTH);
                icarus.task_id_set(id, slot, task);
                return Some(false);
            }
        }
        SET_ANIM_BOTH => {
            let mut both = 0;
            for (torso, slot, setter) in [
                (true, TID_ANIM_UPPER, "Q3_SetAnimUpper"),
                (false, TID_ANIM_LOWER, "Q3_SetAnimLower"),
            ] {
                if set_anim(world, icarus, id, torso, data) {
                    icarus.task_id_set(id, slot, task);
                    both += 1;
                } else {
                    let line = format!(
                        "{setter}: {} does not have anim {data}!\n",
                        targetname(world, id)
                    );
                    debug_print(world, DebugLevel::Error, &line);
                }
            }
            if both >= 2 {
                icarus.task_id_set(id, TID_ANIM_BOTH, task);
            }
            if both > 0 {
                return Some(false);
            }
        }
        SET_ANIM_HOLDTIME_LOWER | SET_ANIM_HOLDTIME_UPPER => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_SetAnimHoldTime is not currently supported in MP\n",
            );
            icarus.task_id_clear(id, TID_ANIM_BOTH);
            icarus.task_id_set(
                id,
                if set == SET_ANIM_HOLDTIME_LOWER {
                    TID_ANIM_LOWER
                } else {
                    TID_ANIM_UPPER
                },
                task,
            );
            return Some(false);
        }
        SET_ANIM_HOLDTIME_BOTH => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_SetAnimHoldTime is not currently supported in MP\n",
            );
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_SetAnimHoldTime is not currently supported in MP\n",
            );
            for slot in [TID_ANIM_BOTH, TID_ANIM_UPPER, TID_ANIM_LOWER] {
                icarus.task_id_set(id, slot, task);
            }
            return Some(false);
        }
        SET_WIDTH => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_SetWidth: NOT SUPPORTED IN MP\n",
            );
            return Some(false);
        }
        SET_ENDFRAME => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_SetEndFrame: NOT SUPPORTED IN MP\n",
            );
            icarus.task_id_set(id, TID_ANIM_BOTH, task);
            return Some(false);
        }
        SET_ANIMFRAME => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_SetAnimFrame: NOT SUPPORTED IN MP\n",
            );
            return Some(false);
        }
        _ => return None,
    }
    Some(true)
}

/// `Q3_SetAnimUpper` / `Q3_SetAnimLower` (`:2405-2462`): false for an animation the
/// table does not have. A non-client is refused but still counted as set (sic).
fn set_anim<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    torso: bool,
    name: &str,
) -> bool {
    let Some(animation) = world.animation(name) else {
        let setter = if torso {
            "Q3_SetAnimUpper"
        } else {
            "Q3_SetAnimLower"
        };
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("{setter}: unknown animation sequence '{name}'\n"),
        );
        return false;
    };
    if world.entity(id).is_some_and(|ent| ent.is_client()) {
        world.client_action(icarus, id, ClientAction::SetAnimation { torso, animation });
    } else {
        // (sic: `SetUpperAnim` prints the lower's name)
        debug_print(
            world,
            DebugLevel::Error,
            &format!("SetLowerAnim: ent {id} is NOT a player or NPC!\n"),
        );
    }
    true
}

/// `Q3_GameSideCheckStringCounterIncrement` (`:3713-3739`): `+n` and `-n` as a change.
fn increment(data: &str) -> f32 {
    let bytes = data.as_bytes();
    match bytes.first() {
        Some(b'+') if bytes.len() > 1 => float(&data[1..]),
        Some(b'-') if bytes.len() > 1 => float(&data[1..]) * -1.0,
        _ => 0.0,
    }
}

/// The behaviour set a `SET_*SCRIPT` names.
fn behavior_set(set: i32) -> Option<usize> {
    Some(match set {
        SET_SPAWNSCRIPT => BSET_SPAWN,
        SET_USESCRIPT => BSET_USE,
        SET_AWAKESCRIPT => BSET_AWAKE,
        SET_ANGERSCRIPT => BSET_ANGER,
        SET_ATTACKSCRIPT => BSET_ATTACK,
        SET_VICTORYSCRIPT => BSET_VICTORY,
        SET_PAINSCRIPT => BSET_PAIN,
        SET_FLEESCRIPT => BSET_FLEE,
        SET_DEATHSCRIPT => BSET_DEATH,
        SET_DELAYEDSCRIPT => BSET_DELAYED,
        SET_BLOCKEDSCRIPT => BSET_BLOCKED,
        SET_FFIRESCRIPT => BSET_FFIRE,
        SET_FFDEATHSCRIPT => BSET_FFDEATH,
        SET_MINDTRICKSCRIPT => BSET_MINDTRICK,
        _ => return None,
    })
}

/// A name the script may set to `NULL`.
fn name_or_null(data: &str) -> Option<String> {
    (!data.eq_ignore_ascii_case("NULL")).then(|| data.to_owned())
}

/// The fields: health, armour, names, parms, behaviour sets, counts and sizes.
fn set_field<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    set: i32,
    data: &str,
) -> bool {
    if let Some(bset) = behavior_set(set) {
        if let Some(ent) = world.entity_mut(id) {
            ent.behavior_sets[bset] = name_or_null(data);
        }
        return true;
    }
    if (SET_PARM1..=SET_PARM16).contains(&set) {
        set_parm(world, id, (set - SET_PARM1) as usize, data);
        return true;
    }
    if (SET_FORCE_HEAL_LEVEL..=SET_SABER_OFFENSE).contains(&set) {
        force_power_level(
            world,
            icarus,
            id,
            set - SET_FORCE_HEAL_LEVEL,
            cnum::atoi(data),
        );
        return true;
    }
    match set {
        SET_HEALTH => set_health(world, icarus, id, cnum::atoi(data)),
        SET_ARMOR => {
            if let Some(client) = world.client(id) {
                world.client_action(
                    icarus,
                    id,
                    ClientAction::SetArmor(cnum::atoi(data).min(client.max_health)),
                );
            }
        }
        SET_WAIT => {
            if let Some(ent) = world.entity_mut(id) {
                ent.wait = float(data);
            }
        }
        SET_SCALE => {
            let scale = (float(data) * 100.0) as i32;
            if world.entity(id).is_some_and(|ent| ent.is_client()) {
                world.client_action(icarus, id, ClientAction::SetModelScale(scale));
            } else if let Some(ent) = world.entity_mut(id) {
                ent.model_scale = scale;
            }
        }
        SET_COUNT => {
            let change = increment(data);
            if let Some(ent) = world.entity_mut(id) {
                if change != 0.0 {
                    ent.count = ent.count.wrapping_add(change as i32);
                } else {
                    ent.count = cnum::atoi(data);
                }
            }
        }
        SET_TARGETNAME | SET_TARGET | SET_FULLNAME => {
            if let Some(ent) = world.entity_mut(id) {
                let field = match set {
                    SET_TARGETNAME => &mut ent.targetname,
                    SET_TARGET => &mut ent.target,
                    _ => &mut ent.full_name,
                };
                *field = name_or_null(data);
            }
        }
        SET_TIMESCALE => world.set_timescale(data),
        SET_LOOPSOUND => loop_sound(world, id, data),
        SET_ICARUS_FREEZE | SET_ICARUS_UNFREEZE => freeze(world, data, set == SET_ICARUS_FREEZE),
        SET_WEAPON => {
            // The reference writes the client's weapons without checking for one.
            if world.entity(id).is_some_and(|ent| ent.is_client()) {
                world.client_action(icarus, id, ClientAction::SetWeapon(data.to_owned()));
            }
        }
        SET_GRAVITY => {
            if client_or_refuse(world, id, "Q3_SetGravity") {
                if world.entity(id).is_some_and(|ent| ent.is_npc()) {
                    world.npc_action(
                        icarus,
                        -1,
                        id,
                        NpcAction::Set {
                            set,
                            value: data.to_owned(),
                        },
                    );
                }
                world.client_action(icarus, id, ClientAction::SetGravity(float(data)));
            }
        }
        SET_SABERACTIVE => saber_active(world, icarus, id, is_true(data)),
        _ => return false,
    }
    true
}

/// `Q3_SetParm` (`:4014-4057`): a text, or a change of the number it holds.
fn set_parm<W: ScriptWorld>(world: &mut W, id: W::Id, parm: usize, data: &str) {
    if parm >= MAX_PARMS {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("SET_PARM: parmNum {parm} out of range!\n"),
        );
        return;
    }
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    let parms = ent.parms.get_or_insert_with(Default::default);
    let (text, cut) = crate::script_entity::parm_text(&parms[parm], data);
    parms[parm] = text;
    if cut {
        let line = format!(
            "SET_PARM: parm{parm} string too long, truncated to '{}'!\n",
            parms[parm]
        );
        debug_print(world, DebugLevel::Warning, &line);
    }
}

/// `Q3_SetHealth` (`:2508-2557`): at least zero; a client's is capped at its maximum,
/// and a client set to zero dies.
fn set_health<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id, data: i32) {
    let data = data.max(0);
    let Some(client) = world.client(id) else {
        if let Some(ent) = world.entity_mut(id) {
            ent.health = data;
        }
        return;
    };
    let capped = if data > client.max_health {
        client.max_health
    } else {
        data
    };
    if data != 0 {
        world.client_action(
            icarus,
            id,
            ClientAction::SetHealth {
                health: capped,
                stat: capped,
            },
        );
        return;
    }
    world.client_action(icarus, id, ClientAction::SetHealth { health: 1, stat: 0 });
    if client.spectator || client.temp_spectating {
        return;
    }
    if let Some(ent) = world.entity_mut(id) {
        ent.flags &= !FL_GODMODE;
    }
    world.client_action(icarus, id, ClientAction::Die);
}

/// `Q3_SetForcePowerLevel` (`:3963-4007`).
fn force_power_level<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    power: i32,
    level: i32,
) {
    const NUM_FORCE_POWERS: i32 = 18;
    const NUM_FORCE_POWER_LEVELS: i32 = 4;
    const FP_SABER_OFFENSE: i32 = 15;
    const SS_NUM_SABER_STYLES: i32 = 8;
    // (sic: the reference checks the level against the number of powers)
    if power < 0 || level >= NUM_FORCE_POWERS {
        let line = format!(
            "Q3_SetForcePowerLevel: Force Power index {power} out of range (0-{})\n",
            NUM_FORCE_POWERS - 1
        );
        debug_print(world, DebugLevel::Error, &line);
        return;
    }
    if (level < 0 || level >= NUM_FORCE_POWER_LEVELS)
        && (power != FP_SABER_OFFENSE || level >= SS_NUM_SABER_STYLES)
    {
        debug_print(
            world,
            DebugLevel::Error,
            &format!("Q3_SetForcePowerLevel: Force power setting {level} out of range (0-3)\n"),
        );
        return;
    }
    if !world.entity(id).is_some_and(|ent| ent.is_client()) {
        let line = format!(
            "Q3_SetForcePowerLevel: ent {} is not a player or NPC\n",
            targetname(world, id)
        );
        debug_print(world, DebugLevel::Error, &line);
        return;
    }
    world.client_action(
        icarus,
        id,
        ClientAction::SetForcePowerLevel { power, level },
    );
}

/// `Q3_SetLoopSound` (`:3279-3302`).
fn loop_sound<W: ScriptWorld>(world: &mut W, id: W::Id, name: &str) {
    if name.eq_ignore_ascii_case("NULL") || name.eq_ignore_ascii_case("NONE") {
        if let Some(ent) = world.entity_mut(id) {
            ent.loop_sound = 0;
        }
        return;
    }
    let index = world.sound_index(name);
    if index != 0 {
        if let Some(ent) = world.entity_mut(id) {
            ent.loop_sound = index as u16;
        }
    } else {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_SetLoopSound: can't find sound file: '{name}'\n"),
        );
    }
}

/// `Q3_SetICARUSFreeze` (`:3304-3330`): by targetname, else by script_targetname.
fn freeze<W: ScriptWorld>(world: &mut W, name: &str, frozen: bool) {
    let found = world
        .find(None, NameField::Targetname, name)
        .or_else(|| world.find(None, NameField::ScriptTargetname, name));
    let Some(ent) = found.and_then(|found| world.entity_mut(found)) else {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_SetICARUSFreeze: invalid ent {name}\n"),
        );
        return;
    };
    if frozen {
        ent.svflags |= SVF_ICARUS_FREEZE;
    } else {
        ent.svflags &= !SVF_ICARUS_FREEZE;
    }
}

/// `Q3_SetSaberActive` (`:5721-5749`) (sic: it toggles a saber that is on when asked
/// to turn it on).
fn saber_active<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    active: bool,
) {
    let Some(client) = world.client(id) else {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_SetSaberActive: {id} is not a client\n"),
        );
        return;
    };
    if (client.saber_holstered == 0 && active) || (client.sabers_off && !active) {
        world.client_action(icarus, id, ClientAction::ToggleSaber);
    }
}

/// The flag sets: `true` and `false` switch a bit.
fn set_flag<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    set: i32,
    data: &str,
) -> bool {
    let on = is_true(data);
    let either = on || data.eq_ignore_ascii_case("false");
    let switch = |bits: &mut u32, bit: u32, on: bool| if on { *bits |= bit } else { *bits &= !bit };
    match set {
        SET_NOTARGET | SET_DONTSHOOT => {
            if let Some(ent) = world.entity_mut(id).filter(|_| either) {
                switch(
                    &mut ent.flags,
                    if set == SET_NOTARGET {
                        FL_NOTARGET
                    } else {
                        FL_DONT_SHOOT
                    },
                    on,
                );
            }
        }
        SET_UNDYING => {
            if let Some(ent) = world.entity_mut(id) {
                switch(&mut ent.flags, FL_UNDYING, on);
            }
        }
        SET_NO_KNOCKBACK => {
            if let Some(ent) = world.entity_mut(id) {
                switch(&mut ent.flags, FL_NO_KNOCKBACK, on);
            }
        }
        SET_INVINCIBLE => {
            if let Some(ent) = world.entity_mut(id) {
                if ent.classname.eq_ignore_ascii_case("func_breakable") {
                    switch(&mut ent.spawnflags, 1, on);
                } else {
                    switch(&mut ent.flags, FL_GODMODE, on);
                }
            }
        }
        SET_PLAYER_USABLE => {
            if let Some(ent) = world.entity_mut(id) {
                switch(&mut ent.svflags, SVF_PLAYER_USABLE, on);
            }
        }
        SET_INVISIBLE => {
            let Some(ent) = world.entity_mut(id) else {
                return true;
            };
            switch(&mut ent.eflags, EF_NODRAW, on);
            if on {
                ent.contents = 0;
            }
            if ent.is_client() {
                world.client_action(icarus, id, ClientAction::SetNoDraw(on));
            }
            world.link(id);
        }
        SET_FUNC_USABLE_VISIBLE => {
            if let Some(ent) = world.entity_mut(id).filter(|_| either) {
                switch(&mut ent.svflags, SVF_NOCLIENT, !on);
                switch(&mut ent.eflags, EF_NODRAW, !on);
            }
        }
        SET_INACTIVE => {
            if either {
                if let Some(ent) = world.entity_mut(id) {
                    switch(&mut ent.flags, FL_INACTIVE, on);
                }
            } else if data.eq_ignore_ascii_case("unlocked") {
                world.lock_doors(id, false);
            } else if data.eq_ignore_ascii_case("locked") {
                world.lock_doors(id, true);
            }
        }
        _ => return false,
    }
    true
}

/// `SET_SOLID` (`Q3_SetSolid`, `:5125-5166`): true if complete at once.
fn set_solid<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    on: bool,
) -> bool {
    if on {
        if !solidify(world, id) {
            icarus.task_id_set(id, TID_RESIZE, task);
            return false;
        }
    } else if let Some(ent) = world.entity_mut(id) {
        ent.contents = if ent.eflags & EF_NODRAW != 0 {
            0
        } else {
            CONTENTS_CORPSE
        };
        world.link(id);
    }
    true
}

/// `Q3_SetSolid(true)` (`:5125-5150`): solid now, or a helper that makes it solid once
/// nothing is in the way (then false).
fn solidify<W: ScriptWorld>(world: &mut W, id: W::Id) -> bool {
    let Some(ent) = world.entity_mut(id) else {
        return true;
    };
    let old = ent.contents;
    ent.contents = CONTENTS_BODY;
    let origin = ent.motion.origin;
    if world.spot_would_telefrag(id, origin) {
        spawn_helper(world, id, origin, Think::SolidifyOwner);
        if let Some(ent) = world.entity_mut(id) {
            ent.contents = old;
        }
        return false;
    }
    if let Some(ent) = world.entity_mut(id) {
        ent.clipmask |= CONTENTS_BODY;
    }
    world.link(id);
    true
}

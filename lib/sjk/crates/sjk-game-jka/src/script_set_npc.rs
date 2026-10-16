//! The `set` types that act on an NPC's mind (`Q3_SetBState`, `Q3_SetNavGoal`,
//! `Q3_SetEnemy`, `Q3_SetViewTarget`, the NPC-only setters of
//! [`crate::script_set_tables::NPC_ONLY`] ...): refused with the reference's messages on
//! anything that is not an NPC, handed to [`ScriptWorld::npc_action`] for one.

use sjk_icarus::{DebugLevel, Icarus};

use crate::icarus_set_table::*;
use crate::script_entity::{FL_NOTARGET, TID_ANGLE_FACE, TID_BSTATE, TID_MOVE_NAV};
use crate::script_set::{client_or_refuse, float, npc_or_refuse, targetname};
use crate::script_set_tables::{NPC_ONLY, Refusal, called};
use crate::script_world::{NameField, NpcAction, ScriptWorld, c_str, debug_print};

/// The set types only an NPC takes, and those that wait on an NPC.
pub(crate) fn set_npc<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    set: i32,
    data: &str,
) -> Option<bool> {
    let action = |set: i32| NpcAction::Set {
        set,
        value: data.to_owned(),
    };
    if let Some(entry) = NPC_ONLY.iter().find(|entry| entry.set == set) {
        if called(entry.calls, data) && npc_or_refuse(world, id, entry.name, entry.refusal) {
            world.npc_action(icarus, task, id, action(set));
        }
        return Some(true);
    }
    match set {
        SET_BEHAVIOR_STATE | SET_TEMP_BSTATE => {
            let setter = if set == SET_BEHAVIOR_STATE {
                "Q3_SetBState"
            } else {
                "Q3_SetTempBState"
            };
            if npc_or_refuse(world, id, setter, Refusal::Plain)
                && !world.npc_action(icarus, task, id, action(set))
            {
                icarus.task_id_set(id, TID_BSTATE, task);
                return Some(false);
            }
            Some(true)
        }
        SET_DPITCH | SET_DYAW => {
            face(world, icarus, task, id, set, data);
            icarus.task_id_set(id, TID_ANGLE_FACE, task);
            Some(false)
        }
        SET_VIEWTARGET => {
            view_target(world, icarus, task, id, data);
            icarus.task_id_set(id, TID_ANGLE_FACE, task);
            Some(false)
        }
        SET_NAVGOAL => Some(nav_goal(world, icarus, task, id, data)),
        SET_ENEMY => {
            set_enemy(world, icarus, task, id, data);
            Some(true)
        }
        SET_LEADER => {
            if world.entity(id).is_some_and(|ent| ent.is_client()) {
                world.npc_action(icarus, task, id, action(set));
            } else {
                debug_print(
                    world,
                    DebugLevel::Error,
                    &format!("Q3_SetLeader: ent {id} is NOT a player or NPC!\n"),
                );
            }
            Some(true)
        }
        SET_LOOK_TARGET => {
            look_target(world, icarus, task, id, data);
            Some(true)
        }
        _ => None,
    }
}

/// `Q3_SetDPitch` / `Q3_SetDYaw` (`:2809-2890`): an NPC's desired angle; its task
/// waits whatever the entity is.
fn face<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    set: i32,
    data: &str,
) {
    let setter = if set == SET_DPITCH {
        "Q3_SetDPitch"
    } else {
        "Q3_SetDYaw"
    };
    if !npc_or_refuse(world, id, setter, Refusal::Plain) {
        return;
    }
    if set == SET_DYAW {
        if let Some(enemy) = world.entity(id).and_then(|ent| ent.enemy) {
            let line = format!(
                "Could not set DYAW: '{}' has an enemy ({})!\n",
                targetname(world, id),
                targetname(world, enemy)
            );
            debug_print(world, DebugLevel::Warning, &line);
            return;
        }
        if let Some(ent) = world.entity_mut(id) {
            ent.spawn_angles[1] = float(data);
        }
    }
    world.npc_action(
        icarus,
        task,
        id,
        NpcAction::Set {
            set,
            value: data.to_owned(),
        },
    );
}

/// `Q3_SetViewTarget` (`:3190-3240`).
fn view_target<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    name: &str,
) {
    let target = world.find(None, NameField::Targetname, name);
    if !world.entity(id).is_some_and(|ent| ent.is_client()) {
        let line = format!(
            "Q3_SetViewTarget: '{}' is not a player/NPC!\n",
            targetname(world, id)
        );
        debug_print(world, DebugLevel::Error, &line);
        return;
    }
    if target.is_none() {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_SetViewTarget: can't find ViewTarget: '{name}'\n"),
        );
        return;
    }
    if world.entity(id).is_some_and(|ent| ent.is_npc()) {
        world.npc_action(
            icarus,
            task,
            id,
            NpcAction::Set {
                set: SET_VIEWTARGET,
                value: name.to_owned(),
            },
        );
        return;
    }
    // A player: `Q3_SetDYaw` then `Q3_SetDPitch` refuse it.
    let name = targetname(world, id);
    debug_print(
        world,
        DebugLevel::Error,
        &format!("Q3_SetDYaw: '{name}' is not an NPC\n"),
    );
    debug_print(
        world,
        DebugLevel::Error,
        &format!("Q3_SetDPitch: '{name}' is not an NPC\n"),
    );
}

/// `Q3_SetNavGoal` (`:2276-2346`): true if complete at once.
fn nav_goal<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    name: &str,
) -> bool {
    let Some(ent) = world.entity(id) else {
        return true;
    };
    let health = world.client(id).map_or(ent.health, |client| client.health);
    let script_name = c_str(ent.script_targetname.as_deref()).to_owned();
    if health == 0 {
        let line = format!(
            "Q3_SetNavGoal: tried to set a navgoal (\"{name}\") on a corpse! \"{script_name}\"\n"
        );
        debug_print(world, DebugLevel::Error, &line);
        return true;
    }
    if !ent.is_npc() {
        let line = format!(
            "Q3_SetNavGoal: tried to set a navgoal (\"{name}\") on a non-NPC: \"{script_name}\"\n"
        );
        debug_print(world, DebugLevel::Error, &line);
        return true;
    }
    if world.npc_action(
        icarus,
        task,
        id,
        NpcAction::Set {
            set: SET_NAVGOAL,
            value: name.to_owned(),
        },
    ) {
        return true;
    }
    icarus.task_id_set(id, TID_MOVE_NAV, task);
    false
}

/// `Q3_SetEnemy` (`:2170-2220`).
fn set_enemy<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    name: &str,
) {
    let npc = world.entity(id).is_some_and(|ent| ent.is_npc());
    let clearing = name.eq_ignore_ascii_case("NONE") || name.eq_ignore_ascii_case("NULL");
    if !clearing && world.find(None, NameField::Targetname, name).is_none() {
        debug_print(
            world,
            DebugLevel::Error,
            &format!("Q3_SetEnemy: no such enemy: '{name}'\n"),
        );
        return;
    }
    if npc {
        world.npc_action(
            icarus,
            task,
            id,
            NpcAction::Set {
                set: SET_ENEMY,
                value: name.to_owned(),
            },
        );
        return;
    }
    let enemy = if clearing {
        None
    } else {
        world.find(None, NameField::Targetname, name)
    };
    // `G_SetEnemy` for anything but an NPC: taken unless it is in notarget.
    if let Some(enemy) = enemy {
        if world
            .entity(enemy)
            .is_some_and(|ent| ent.flags & FL_NOTARGET != 0)
        {
            return;
        }
    }
    if let Some(ent) = world.entity_mut(id) {
        ent.enemy = enemy;
    }
}

/// `Q3_LookTarget` (`:5353-5398`).
fn look_target<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    name: &str,
) {
    if !client_or_refuse(world, id, "Q3_LookTarget") {
        return;
    }
    let clearing = name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("NULL");
    if !clearing
        && [
            NameField::Targetname,
            NameField::ScriptTargetname,
            NameField::NpcTargetname,
        ]
        .iter()
        .all(|&field| world.find(None, field, name).is_none())
    {
        debug_print(
            world,
            DebugLevel::Error,
            &format!("Q3_LookTarget: Can't find ent {name}\n"),
        );
        return;
    }
    world.npc_action(
        icarus,
        task,
        id,
        NpcAction::Set {
            set: SET_LOOK_TARGET,
            value: name.to_owned(),
        },
    );
}

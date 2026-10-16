//! The entities that run scripts and the frame that drives them: `target_scriptrunner`
//! (`g_target.c:770-913`), `func_static` (`g_mover.c:1971-2057`), `trigger_always`
//! (`g_trigger.c:893-905`), the world's spawn script (`g_spawn.c:1604-1619`),
//! `G_ActivateBehavior` (`NPC_utils.c:873-917`) and `G_RunThink` with its
//! `ICARUS_MaintainTaskManager` (`g_main.c:2822-2850`).

use sjk_entity::Entity;
use sjk_icarus::{Icarus, cnum};

use crate::script_calls;
use crate::script_entity::{
    BEHAVIOR_STATES, BSET_SPAWN, BSET_USE, CONTENTS_BODY, EF_SHADER_ANIM, EntityKind, FL_INACTIVE,
    SVF_BROADCAST, SVF_PLAYER_USABLE, SVF_USE_CURRENT_ORIGIN, ScriptEntity, TID_MOVE_NAV,
    TID_RESIZE, Think, Uses, scripts_to_precache, valid_for_scripts,
};
use crate::script_host::ScriptHost;
use crate::script_mover::ScriptMover;
use crate::script_set::FRAMETIME;
use crate::script_world::{NpcAction, ScriptWorld, c_str};

/// `Q3_SCRIPT_DIR`.
pub const SCRIPT_DIR: &str = "scripts";

/// `ICARUS_InitEnt` (`GameInterface.cpp:660-685`): a sequencer, the name associated,
/// and every script its behaviour sets name read ahead.
pub fn init_entity<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let Some(ent) = world.entity(id) else { return };
    if icarus.is_initialized(id) {
        return;
    }
    let name = ent.script_targetname.clone();
    let scripts: Vec<String> = scripts_to_precache(ent).collect();
    icarus.init_entity(id, name.as_deref());
    let mut host = ScriptHost::new(world);
    for script in scripts {
        icarus.interrogate(&script, &mut host);
    }
}

/// The tail of `G_SpawnGEntityFromSpawnVars` (`g_spawn.c:950-962`): an entity with a
/// script name or a behaviour set gets a sequencer and, unless it is an NPC spawner,
/// runs its spawn script.
pub fn spawned<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if !valid_for_scripts(ent) {
        return;
    }
    let spawner = ent.classname.starts_with("NPC_");
    let named = !ent.classname.is_empty();
    init_entity(world, icarus, id);
    if named && !spawner {
        activate_behavior(world, icarus, id, BSET_SPAWN);
    }
}

/// `G_ActivateBehavior`: runs the script a behaviour set names (an NPC's behaviour
/// state instead, where it names one). False if the set is empty.
pub fn activate_behavior<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    bset: usize,
) -> bool {
    let Some(ent) = world.entity(id) else {
        return false;
    };
    let Some(name) = ent
        .behavior_sets
        .get(bset)
        .and_then(Option::as_deref)
        .filter(|name| !name.is_empty())
    else {
        return false;
    };
    let name = name.to_owned();
    if ent.is_npc()
        && BEHAVIOR_STATES
            .iter()
            .any(|state| state.eq_ignore_ascii_case(&name))
    {
        world.npc_action(icarus, -1, id, NpcAction::BehaviorState(name));
        return true;
    }
    icarus.run_script(
        id,
        &format!("{SCRIPT_DIR}/{name}"),
        &mut ScriptHost::new(world),
    );
    true
}

/// `G_FreeEntity`: its scripts go first (`ICARUS_FreeEnt`), then the entity.
pub fn free<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    free_scripts(world, icarus, id);
    world.free(icarus, id);
}

/// `ICARUS_FreeEnt` alone: the entity stays, its scripts go (`ClientSpawn` does this
/// before [`init_entity`], `g_client.c:3840`).
pub fn free_scripts<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let name = world
        .entity(id)
        .and_then(|ent| ent.script_targetname.clone());
    icarus.free_entity(id, name.as_deref());
}

/// `ICARUS_MaintainTaskManager` for one entity.
pub fn maintain<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    icarus.maintain(id, &mut ScriptHost::new(world));
}

/// One entity's turn in `G_RunFrame` for what this module runs: a scripted mover moves
/// (`G_RunMover`), anything else thinks (`G_RunThink`); both then run its scripts. A
/// client's scripts run before its move (`g_main.c:3328`): call [`maintain`] there.
pub fn run_entity<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let Some(ent) = world.entity(id) else { return };
    if ent.motion.is_mover {
        script_calls::run_mover(world, icarus, id);
    } else {
        run_think(world, icarus, id);
    }
}

/// `G_RunThink`: the think if it is due, then the entity's scripts if it still exists.
pub fn run_think<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let time = world.level_time();
    if let Some(ent) = world.entity_mut(id) {
        if ent.next_think > 0 && ent.next_think <= time {
            ent.next_think = 0;
            let think = ent.think;
            think_now(world, icarus, id, think);
        }
    }
    if world.entity(id).is_some() {
        maintain(world, icarus, id);
    }
}

/// The think functions.
fn think_now<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id, think: Think) {
    match think {
        Think::Nothing => {}
        Think::ScriptRunner => scriptrunner_run(world, icarus, id),
        Think::TriggerAlways => {
            let target = world.entity(id).and_then(|ent| ent.target.clone());
            if let Some(target) = target.filter(|target| !target.is_empty()) {
                world.use_targets(icarus, id, Some(id), &target);
            }
            free(world, icarus, id);
        }
        Think::StopTurning => script_calls::stop_turning(world, icarus, id),
        Think::Free => free(world, icarus, id),
        Think::MoveOwner => move_owner(world, icarus, id),
        Think::SolidifyOwner => solidify_owner(world, icarus, id),
        Think::Other => world.run_own_think(icarus, id),
    }
}

/// `GlobalUse` (`g_utils.c:561-573`) for the uses this module runs: nothing while the
/// entity is inactive.
pub fn use_entity<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    other: Option<W::Id>,
    activator: Option<W::Id>,
) {
    let Some(ent) = world.entity(id) else { return };
    if ent.flags & FL_INACTIVE != 0 {
        return;
    }
    match ent.uses {
        Uses::ScriptRunner => scriptrunner_use(world, icarus, id, other, activator),
        Uses::FuncStatic => func_static_use(world, icarus, id, activator),
        Uses::Nothing | Uses::Other => {}
    }
}

/// `SP_target_scriptrunner`: runs once unless counted, `delay` and `wait` in seconds.
pub fn spawn_scriptrunner<Id>(entity: &Entity) -> ScriptEntity<Id> {
    let mut ent = ScriptEntity::from_map(entity, EntityKind::Other);
    if ent.spawnflags & 128 != 0 {
        ent.flags |= FL_INACTIVE;
    }
    if ent.count == 0 {
        ent.count = 1;
    }
    let delay = entity
        .get("delay")
        .map_or(0.0, |value| cnum::atof(value) as f32);
    ent.delay = (delay * 1000.0) as i32;
    ent.wait *= 1000.0;
    // `G_SetOrigin`: the origin only; its angles stay in `s.angles`.
    ent.motion = ScriptMover::standing(ent.spawn_origin, [0.0; 3]);
    ent.uses = Uses::ScriptRunner;
    ent
}

/// `target_scriptrunner_use`: not again before its wait is over; run now or after the
/// delay.
fn scriptrunner_use<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    other: Option<W::Id>,
    activator: Option<W::Id>,
) {
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if ent.next_think > time {
        return;
    }
    ent.activator = activator;
    ent.enemy = other;
    if ent.delay != 0 {
        ent.think = Think::ScriptRunner;
        ent.next_think = time + ent.delay;
    } else {
        scriptrunner_run(world, icarus, id);
    }
}

/// `scriptrunner_run`: counts a use down, runs the script on itself or on its
/// activator, and waits.
fn scriptrunner_run<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let time = world.level_time();
    let developer = world.developer() != 0;
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if ent.count != -1 {
        if ent.count <= 0 {
            ent.uses = Uses::Nothing;
            ent.behavior_sets[BSET_USE] = None;
            return;
        }
        ent.count -= 1;
    }
    if let Some(script) = ent.behavior_sets[BSET_USE].clone() {
        if ent.spawnflags & 1 != 0 {
            run_on_activator(world, icarus, id, &script, developer);
        } else {
            if developer {
                if let Some(activator) = ent.activator {
                    let runner = c_str(ent.targetname.as_deref()).to_owned();
                    let by = c_str(
                        world
                            .entity(activator)
                            .and_then(|by| by.targetname.as_deref()),
                    )
                    .to_owned();
                    world.print(&format!("target_scriptrunner {runner} used by {by}\n"));
                }
            }
            activate_behavior(world, icarus, id, BSET_USE);
        }
    }
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if ent.wait != 0.0 {
        ent.next_think = (time as f32 + ent.wait) as i32;
    }
}

/// `scriptrunner_run`'s `runonactivator` branch (`g_target.c:797-842`).
fn run_on_activator<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    script: &str,
    developer: bool,
) {
    let Some(activator) = world.entity(id).and_then(|ent| ent.activator) else {
        if developer {
            world.print("target_scriptrunner tried to run on invalid entity!\n");
        }
        return;
    };
    // (sic: the reference asks whether the runner, not the activator, has a sequencer)
    if !icarus.is_initialized(id) {
        let unnamed = world
            .entity(activator)
            .is_some_and(|ent| ent.script_targetname.as_deref().is_none_or(str::is_empty));
        if unnamed {
            let number = world.next_new_script_name();
            if let Some(ent) = world.entity_mut(activator) {
                ent.script_targetname = Some(format!("newICARUSEnt{number}"));
            }
        }
        if world.entity_mut(activator).is_some_and(valid_for_scripts) {
            init_entity(world, icarus, activator);
        } else {
            if developer {
                world.print("target_scriptrunner tried to run on invalid ICARUS activator!\n");
            }
            return;
        }
    }
    if developer {
        let by = c_str(
            world
                .entity(activator)
                .and_then(|ent| ent.targetname.as_deref()),
        )
        .to_owned();
        world.print(&format!(
            "target_scriptrunner running {script} on activator {by}\n"
        ));
    }
    icarus.run_script(
        activator,
        &format!("{SCRIPT_DIR}/{script}"),
        &mut ScriptHost::new(world),
    );
}

/// `SP_func_static` with `InitMover`: a brush model at rest where the map put it. The
/// server gives the model's `contents` and bounds.
pub fn spawn_func_static<Id>(
    entity: &Entity,
    contents: u32,
    mins: [f32; 3],
    maxs: [f32; 3],
) -> ScriptEntity<Id> {
    let mut ent = ScriptEntity::from_map(entity, EntityKind::Other);
    let number = |key: &str| {
        entity
            .fields()
            .iter()
            .rev()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    };
    let speed = number("speed").map_or(0.0, |value| cnum::atof(value) as f32);
    let linear = number("linear").is_some_and(|value| cnum::atoi(value) != 0);
    let damage = number("dmg").map_or(0, cnum::atoi);
    let sound_set = number("soundset").unwrap_or("");
    ent.motion = ScriptMover::resting(
        ent.spawn_origin,
        ent.spawn_angles,
        speed,
        linear,
        damage,
        sound_set,
    );
    ent.svflags = SVF_USE_CURRENT_ORIGIN;
    if ent.spawnflags & 128 != 0 {
        ent.flags |= FL_INACTIVE;
    }
    if ent.spawnflags & 64 != 0 {
        ent.svflags |= SVF_PLAYER_USABLE;
    }
    if ent.spawnflags & 2048 != 0 {
        ent.svflags |= SVF_BROADCAST;
    }
    if ent.spawnflags & 4 != 0 {
        ent.eflags |= EF_SHADER_ANIM;
    }
    ent.model_scale = number("model2scale").map_or(0, cnum::atoi).clamp(0, 1023);
    ent.contents = contents;
    ent.mins = mins;
    ent.maxs = maxs;
    ent.uses = Uses::FuncStatic;
    ent
}

/// `func_static_use`: its use script, its shader stage flipped, its targets used.
fn func_static_use<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    activator: Option<W::Id>,
) {
    activate_behavior(world, icarus, id, BSET_USE);
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if ent.spawnflags & 4 != 0 {
        ent.frame = if ent.frame != 0 { 0 } else { 1 };
    }
    if let Some(target) = ent.target.clone().filter(|target| !target.is_empty()) {
        world.use_targets(icarus, id, activator, &target);
    }
}

/// `SP_trigger_always`: fires its targets 300 ms into the level, then goes.
pub fn spawn_trigger_always<Id>(entity: &Entity, level_time: i32) -> ScriptEntity<Id> {
    let mut ent = ScriptEntity::from_map(entity, EntityKind::Other);
    ent.think = Think::TriggerAlways;
    ent.next_think = level_time + 300;
    ent
}

/// The world's spawn script (`g_spawn.c:1604-1619`): a scriptrunner made for it, run a
/// tenth of a second into the level. `id` is the fresh entity the server made
/// ([`ScriptWorld::spawn`]).
pub fn start_world_script<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    spawnscript: &str,
) {
    if spawnscript.is_empty() {
        return;
    }
    let time = world.level_time();
    let Some(id) = world.spawn("noclass") else {
        return;
    };
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    ent.behavior_sets[BSET_USE] = Some(spawnscript.to_owned());
    ent.count = 1;
    ent.think = Think::ScriptRunner;
    ent.next_think = time + 100;
    init_entity(world, icarus, id);
}

/// `MoveOwner` (`g_ICARUScb.c:1886-1908`): the owner is put here once the spot is
/// clear, and its move task completes.
fn move_owner<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    ent.next_think = time + FRAMETIME;
    ent.think = Think::Free;
    let (origin, owner) = (ent.motion.origin, ent.owner);
    let Some(owner) = owner.filter(|&owner| world.entity(owner).is_some()) else {
        return;
    };
    if world.spot_would_telefrag(owner, origin) {
        if let Some(ent) = world.entity_mut(id) {
            ent.think = Think::MoveOwner;
        }
        return;
    }
    if let Some(owner_ent) = world.entity_mut(owner) {
        owner_ent.motion.set_origin(origin);
    }
    world.link(owner);
    icarus.task_id_complete(owner, TID_MOVE_NAV);
}

/// `SolidifyOwner` (`:5091-5118`): the owner turns solid once nothing is in the way,
/// and its resize task completes.
fn solidify_owner<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    ent.next_think = time + FRAMETIME;
    ent.think = Think::Free;
    let Some(owner) = ent.owner.filter(|&owner| world.entity(owner).is_some()) else {
        return;
    };
    let Some(owner_ent) = world.entity_mut(owner) else {
        return;
    };
    let old = owner_ent.contents;
    owner_ent.contents = CONTENTS_BODY;
    let origin = owner_ent.motion.origin;
    if world.spot_would_telefrag(owner, origin) {
        if let Some(owner_ent) = world.entity_mut(owner) {
            owner_ent.contents = old;
        }
        if let Some(ent) = world.entity_mut(id) {
            ent.think = Think::SolidifyOwner;
        }
    } else {
        world.link(owner);
        icarus.task_id_complete(owner, TID_RESIZE);
    }
}

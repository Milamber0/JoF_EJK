//! The game's script commands other than `set` and `get` (`g_ICARUScb.c`): `sound`,
//! `play`, `move`, `rotate`, `tag`, `use`, `kill` and `remove`, and the mover callbacks
//! that complete their tasks.
//!
//! `Q3_Lerp2Start` and `Q3_Lerp2End` are not here: the multiplayer interpreter never
//! calls them (no command reaches `GVM_ICARUS_LERP2START` / `LERP2END`).

use sjk_icarus::{DebugLevel, Icarus};

use crate::movers::{BMS_END, BMS_START};
use crate::script_entity::{TID_ANGLE_FACE, TID_CHAN_VOICE, TID_MOVE_NAV, Think};
use crate::script_mover::{Completed, Waits};
use crate::script_world::{NameField, ScriptWorld, debug_print};

/// `CHAN_AUTO` and `CHAN_VOICE` (`soundChannel_t`).
pub const CHAN_AUTO: i32 = 0;
pub const CHAN_VOICE: i32 = 3;
/// `BMS_MID`: a mover's travel sound loops from its soundset.
pub const BMS_MID: u16 = 1;
/// `TYPE_ORIGIN` and `TYPE_ANGLES`: what `tag()` looks up.
pub const TYPE_ANGLES: i32 = 53;
pub const TYPE_ORIGIN: i32 = 54;

/// `Q3_PlaySound` (`g_ICARUScb.c:418-540`): true if the task is complete at once; a
/// voice waits (`TID_CHAN_VOICE`) unless the game runs fast.
pub fn play_sound<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    name: &str,
    channel: &str,
) -> bool {
    let Some(ent) = world.entity(id) else {
        return true;
    };
    let runner = ent.classname.eq_ignore_ascii_case("target_scriptrunner");
    let origin = ent.motion.origin;
    // `Q_strncpyz` into MAX_QPATH, upper-cased, extension stripped.
    let final_name = sjk_icarus::strip_extension(
        &crate::script_entity::cut(name, 63).to_ascii_uppercase(),
        64,
    );
    let handle = world.sound_index(&final_name);
    let mut broadcast = channel.eq_ignore_ascii_case("CHAN_ANNOUNCER") || runner;
    let voice = if channel.eq_ignore_ascii_case("CHAN_VOICE") {
        Some(CHAN_VOICE)
    } else if channel.eq_ignore_ascii_case("CHAN_VOICE_ATTEN") {
        Some(CHAN_AUTO)
    } else if channel.eq_ignore_ascii_case("CHAN_VOICE_GLOBAL") {
        broadcast = true;
        Some(CHAN_AUTO)
    } else {
        None
    };
    if let Some(voice_channel) = voice {
        if world.timescale() > 1.0 {
            return true;
        }
        let index = world.sound_index(&final_name);
        world.sound(id, voice_channel, index);
        icarus.task_id_set(id, TID_CHAN_VOICE, task);
        return false;
    }
    if broadcast {
        world.global_sound(origin, handle);
    } else {
        world.sound(id, CHAN_AUTO, handle);
    }
    true
}

/// `Q3_Play` (`:545-581`): a `PLAY_ROFF` starts the ROFF if the game has it; it moves
/// the entity until done (`TID_MOVE_NAV`). A ROFF that is not there does nothing, as in
/// the reference.
pub fn play<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    kind: &str,
    name: &str,
) {
    if !kind.eq_ignore_ascii_case("PLAY_ROFF") {
        return;
    }
    let roff = world.cache_roff(name);
    if roff == 0 {
        return;
    }
    icarus.task_id_set(id, TID_MOVE_NAV, task);
    world.link(id);
    world.play_roff(id, roff);
}

/// Whether a lerp may move the entity: not a client, not a scriptrunner. Prints the
/// reference's error otherwise.
fn movable<W: ScriptWorld>(world: &mut W, id: W::Id, command: &str) -> bool {
    let Some(ent) = world.entity(id) else {
        return false;
    };
    if ent.is_client() || ent.classname.eq_ignore_ascii_case("target_scriptrunner") {
        debug_print(
            world,
            DebugLevel::Error,
            &format!("{command}: ent {id} is NOT a mover!\n"),
        );
        return false;
    }
    true
}

/// The waits a move leaves (`TID_ANGLE_FACE` first) and its travel sounds
/// (`G_PlayDoorLoopSound`, `G_PlayDoorSound(BMS_START)`).
fn start_travel<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    waits: Waits,
) {
    if waits.angles {
        icarus.task_id_set(id, TID_ANGLE_FACE, task);
    }
    if waits.travel && task != -1 {
        icarus.task_id_set(id, TID_MOVE_NAV, task);
    }
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    if !ent.motion.sound_set.is_empty() {
        ent.loop_sound = BMS_MID;
        let set = ent.motion.sound_set.clone();
        let index = world.sound_set_index(&set);
        world.door_sound(id, index, BMS_START);
    }
    world.link(id);
}

/// `Q3_Lerp2Pos` (`:799-901`): to `origin`, turning to `angles` if given.
pub fn lerp_to_position<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    origin: [f32; 3],
    angles: Option<[f32; 3]>,
    duration: f32,
) {
    if !movable(world, id, "Q3_Lerp2Pos") {
        return;
    }
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    let waits = ent.motion.lerp_to_position(origin, angles, duration, time);
    start_travel(world, icarus, task, id, waits);
}

/// `Q3_Lerp2Origin` (`:2072-2133`): to `origin`; a task of -1 waits for nothing.
pub fn lerp_to_origin<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    origin: [f32; 3],
    duration: f32,
) {
    if !movable(world, id, "Q3_Lerp2Origin") {
        return;
    }
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    let waits = ent.motion.lerp_to_origin(origin, duration, time);
    start_travel(world, icarus, task, id, waits);
}

/// `Q3_Lerp2Angles` (`:910-957`): the turn, stopped by the entity's think.
pub fn lerp_to_angles<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    task: i32,
    id: W::Id,
    angles: [f32; 3],
    duration: f32,
) {
    if !movable(world, id, "Q3_Lerp2Angles") {
        return;
    }
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    let next = ent.motion.lerp_to_angles(angles, duration, time);
    ent.think = Think::StopTurning;
    ent.next_think = next;
    icarus.task_id_set(id, TID_ANGLE_FACE, task);
    world.link(id);
}

/// `anglerCallback` (`:587-609`): the turn is over and its task complete.
pub fn stop_turning<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    icarus.task_id_complete(id, TID_ANGLE_FACE);
    let time = world.level_time();
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    ent.motion.stop_turning(time);
    if ent.think == Think::StopTurning {
        ent.think = Think::Nothing;
    }
    world.link(id);
}

/// `moverCallback` (`:621-651`): the move is over; its task complete, the travel sound
/// stops and the stop sound plays.
fn stop_travel<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    icarus.task_id_complete(id, TID_MOVE_NAV);
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    ent.loop_sound = 0;
    if !ent.motion.sound_set.is_empty() {
        let set = ent.motion.sound_set.clone();
        let index = world.sound_set_index(&set);
        world.door_sound(id, index, BMS_END);
    }
}

/// `G_RunMover` (`g_mover.c:509-522`) for a scripted mover: its trajectories move it
/// (`G_MoverTeam`, a team of one), pushing what is in the way; at the end of the travel
/// its `reached` callback completes the tasks. Then its think runs.
pub fn run_mover<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id) {
    let time = world.level_time();
    let Some(ent) = world.entity(id) else { return };
    if ent.motion.in_motion() {
        let (origin, angles) = ent.motion.place_at(time);
        let move_by: [f32; 3] = std::array::from_fn(|axis| origin[axis] - ent.motion.origin[axis]);
        let turn_by: [f32; 3] = std::array::from_fn(|axis| angles[axis] - ent.motion.angles[axis]);
        let pushed = if move_by != [0.0; 3] || turn_by != [0.0; 3] {
            world.push_mover(icarus, id, move_by, turn_by)
        } else {
            Ok(())
        };
        match pushed {
            Err(obstacle) => {
                let previous = world.previous_time();
                if let Some(ent) = world.entity_mut(id) {
                    ent.motion.hold(time, previous);
                }
                world.link(id);
                let crushes = world.entity(id).is_some_and(|ent| ent.motion.crushes);
                if crushes {
                    world.mover_blocked(icarus, id, obstacle);
                }
            }
            Ok(()) => {
                if let Some(ent) = world.entity_mut(id) {
                    ent.motion.origin = origin;
                    ent.motion.angles = angles;
                }
                arrive(world, icarus, id, time);
            }
        }
    }
    crate::script_runner::run_think(world, icarus, id);
}

/// The success half of `G_MoverTeam`: at or past the travel's end, `reached`.
fn arrive<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id, time: i32) {
    let Some(ent) = world.entity_mut(id) else {
        return;
    };
    let turning = ent.think == Think::StopTurning;
    let completed = ent.motion.arrive_if_due(true, time);
    for done in completed {
        match done {
            Completed::Angles => {
                icarus.task_id_complete(id, TID_ANGLE_FACE);
                if turning {
                    if let Some(ent) = world.entity_mut(id) {
                        ent.think = Think::Nothing;
                    }
                }
            }
            Completed::Travel => stop_travel(world, icarus, id),
        }
    }
    world.link(id);
}

/// `Q3_GetTag` (`:966-989`): the tag the entity's owner sees, or zeros and false.
pub fn tag<W: ScriptWorld>(
    world: &W,
    id: W::Id,
    name: &str,
    lookup: i32,
    info: &mut [f32; 3],
) -> bool {
    let Some(ent) = world.entity(id) else {
        return false;
    };
    let owner = ent.ownername.as_deref();
    let found = match lookup {
        TYPE_ORIGIN => world.tags().origin(owner, name),
        TYPE_ANGLES => world.tags().angles(owner, name),
        _ => return false,
    };
    *info = found.unwrap_or([0.0; 3]);
    found.is_some()
}

/// `Q3_Use` (`:999-1016`).
pub fn use_target<W: ScriptWorld>(
    world: &mut W,
    icarus: &mut Icarus<W::Id>,
    id: W::Id,
    target: &str,
) {
    if target.is_empty() {
        debug_print(world, DebugLevel::Warning, "Q3_Use: string is NULL!\n");
        return;
    }
    world.use_targets(icarus, id, Some(id), target);
}

/// `self`, `enemy` or the first entity of that targetname.
fn victim<W: ScriptWorld>(world: &W, id: W::Id, name: &str) -> Option<W::Id> {
    if name.eq_ignore_ascii_case("self") {
        Some(id)
    } else if name.eq_ignore_ascii_case("enemy") {
        world.entity(id).and_then(|ent| ent.enemy)
    } else {
        world.find(None, NameField::Targetname, name)
    }
}

/// `Q3_Kill` (`:1027-1072`): health zero, a client takes no knockback, and it dies.
pub fn kill<W: ScriptWorld>(world: &mut W, icarus: &mut Icarus<W::Id>, id: W::Id, name: &str) {
    let Some(victim) = victim(world, id, name) else {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_Kill: can't find {name}\n"),
        );
        return;
    };
    let old_health = match world.client(victim) {
        Some(client) => client.health,
        None => world.entity(victim).map_or(0, |ent| ent.health),
    };
    if world.entity(victim).is_some_and(|ent| ent.is_client()) {
        let stat = world.client(victim).map_or(0, |client| client.stat_health);
        world.client_action(
            icarus,
            victim,
            crate::script_world::ClientAction::SetHealth { health: 0, stat },
        );
        if let Some(ent) = world.entity_mut(victim) {
            ent.flags |= crate::script_entity::FL_NO_KNOCKBACK;
        }
    } else if let Some(ent) = world.entity_mut(victim) {
        ent.health = 0;
    }
    world.die(icarus, victim, old_health);
}

/// `Q3_RemoveEnt` (`:1080-1138`): freed in a tenth of a second; a player cannot be.
fn remove_entity<W: ScriptWorld>(world: &mut W, victim: W::Id) {
    let time = world.level_time();
    let Some(ent) = world.entity_mut(victim) else {
        return;
    };
    if ent.is_client() && !ent.is_npc() {
        debug_print(
            world,
            DebugLevel::Warning,
            "Q3_RemoveEnt: You can't remove clients in MP!\n",
        );
        return;
    }
    // An NPC vehicle ejects its riders first; that is the world's, at the free.
    ent.think = Think::Free;
    ent.next_think = time + 100;
}

/// `Q3_Remove` (`:1146-1194`): `self`, `enemy`, or every entity of that targetname.
pub fn remove<W: ScriptWorld>(world: &mut W, id: W::Id, name: &str) {
    let named = !name.eq_ignore_ascii_case("self") && !name.eq_ignore_ascii_case("enemy");
    let Some(first) = victim(world, id, name) else {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!("Q3_Remove: can't find {name}\n"),
        );
        return;
    };
    remove_entity(world, first);
    if !named {
        return;
    }
    let mut after = first;
    while let Some(next) = world.find(Some(after), NameField::Targetname, name) {
        remove_entity(world, next);
        after = next;
    }
}

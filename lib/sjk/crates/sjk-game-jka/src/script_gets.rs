//! A script's `get(FLOAT|VECTOR|STRING, name)` (`Q3_GetFloat`, `Q3_GetVector`,
//! `Q3_GetString`, `g_ICARUScb.c:1207-1876`): an entity field by its `set` name, or a
//! declared variable of the same type.

use sjk_icarus::{DebugLevel, Icarus, StringAnswer, VariableType, cnum};

use crate::icarus_set_table::*;
use crate::script_entity::{
    BSET_ANGER, BSET_ATTACK, BSET_AWAKE, BSET_BLOCKED, BSET_DEATH, BSET_DELAYED, BSET_FFDEATH,
    BSET_FFIRE, BSET_FLEE, BSET_LOSTENEMY, BSET_PAIN, BSET_SPAWN, BSET_USE, BSET_VICTORY,
    EF_NODRAW, FL_GODMODE, FL_NO_KNOCKBACK, FL_NOTARGET, SVF_PLAYER_USABLE,
};
use crate::script_world::{ScriptWorld, c_str, debug_print};

/// The parm index of a `SET_PARMn`.
fn parm_index(set: i32) -> Option<usize> {
    (SET_PARM1..=SET_PARM16)
        .contains(&set)
        .then(|| (set - SET_PARM1) as usize)
}

/// `Q3_GetFloat` (`:1207-1580`).
pub fn get_float<W: ScriptWorld>(
    world: &mut W,
    icarus: &Icarus<W::Id>,
    id: W::Id,
    name: &str,
) -> Option<f32> {
    let set = set_id(name);
    let ent = world.entity(id)?;
    let client = world.client(id);
    let targetname = ent.targetname.clone();
    let not_client = |world: &mut W, what: &str| {
        debug_print(
            world,
            DebugLevel::Warning,
            &format!(
                "Q3_GetFloat: {what}, {} not a client\n",
                c_str(targetname.as_deref())
            ),
        );
        None
    };
    if let Some(parm) = parm_index(set) {
        return match ent.parm(parm) {
            Some(text) => Some(cnum::atof(text) as f32),
            None => {
                let line = format!(
                    "GET_PARM: {} {} did not have any parms set!\n",
                    ent.classname,
                    c_str(ent.targetname.as_deref())
                );
                debug_print(world, DebugLevel::Error, &line);
                None
            }
        };
    }
    let value = match set {
        SET_COUNT => ent.count as f32,
        SET_HEALTH => client.map_or(ent.health, |client| client.health) as f32,
        SET_XVELOCITY | SET_YVELOCITY | SET_ZVELOCITY => {
            let axis = (set - SET_XVELOCITY) as usize;
            let label = ["SET_XVELOCITY", "SET_YVELOCITY", "SET_ZVELOCITY"][axis];
            match client {
                Some(client) => client.velocity[axis],
                None => return not_client(world, label),
            }
        }
        SET_Z_OFFSET => ent.motion.origin[2] - ent.spawn_origin[2],
        // `r.mins[0]`: the world keeps the bounds; a script entity's are its model's.
        SET_WIDTH => ent.mins[0],
        SET_GRAVITY => world.gravity(),
        SET_FACEEYESCLOSED | SET_FACEEYESOPENED | SET_FACEAUX | SET_FACEBLINK
        | SET_FACEBLINKFROWN | SET_FACEFROWN | SET_FACENORMAL => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetFloat: SET_FACE___ not implemented\n",
            );
            return None;
        }
        SET_WAIT => ent.wait,
        SET_ANIM_HOLDTIME_LOWER => match client {
            Some(client) => client.legs_timer as f32,
            None => return not_client(world, "SET_ANIM_HOLDTIME_LOWER"),
        },
        SET_ANIM_HOLDTIME_UPPER => match client {
            Some(client) => client.torso_timer as f32,
            None => return not_client(world, "SET_ANIM_HOLDTIME_UPPER"),
        },
        SET_ANIM_HOLDTIME_BOTH => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetFloat: SET_ANIM_HOLDTIME_BOTH not implemented\n",
            );
            return None;
        }
        SET_ARMOR => match client {
            Some(client) => client.armor as f32,
            None => return not_client(world, "SET_ARMOR"),
        },
        SET_NOTARGET => (ent.flags & FL_NOTARGET) as f32,
        SET_SOLID => ent.contents as f32,
        SET_PLAYER_USABLE => (ent.svflags & SVF_PLAYER_USABLE) as f32,
        SET_INTERFACE => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetFloat: SET_INTERFACE not implemented\n",
            );
            return None;
        }
        SET_INVISIBLE => (ent.eflags & EF_NODRAW) as f32,
        SET_VIDEO_FADE_IN => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetFloat: SET_VIDEO_FADE_IN not implemented\n",
            );
            return None;
        }
        SET_VIDEO_FADE_OUT => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetFloat: SET_VIDEO_FADE_OUT not implemented\n",
            );
            return None;
        }
        SET_NO_KNOCKBACK => (ent.flags & FL_NO_KNOCKBACK) as f32,
        SET_INVINCIBLE => (ent.flags & FL_GODMODE) as f32,
        // Names the table has that cannot be read: `return 0`.
        SET_SKILL
        | SET_DPITCH
        | SET_DYAW
        | SET_TIMESCALE
        | SET_CAMERA_GROUP_Z_OFS
        | SET_VISRANGE
        | SET_EARSHOT
        | SET_VIGILANCE
        | SET_FOLLOWDIST
        | SET_WALKSPEED
        | SET_RUNSPEED
        | SET_YAWSPEED
        | SET_AGGRESSION
        | SET_AIM
        | SET_FRICTION
        | SET_SHOOTDIST
        | SET_HFOV
        | SET_VFOV
        | SET_DELAYSCRIPTTIME
        | SET_FORWARDMOVE
        | SET_RIGHTMOVE
        | SET_STARTFRAME
        | SET_ENDFRAME
        | SET_ANIMFRAME
        | SET_SHOT_SPACING
        | SET_MISSIONSTATUSTIME
        | SET_IGNOREPAIN
        | SET_IGNOREENEMIES
        | SET_IGNOREALERTS
        | SET_DONTSHOOT
        | SET_DONTFIRE
        | SET_LOCKED_ENEMY
        | SET_CROUCHED
        | SET_WALKING
        | SET_RUNNING
        | SET_CHASE_ENEMIES
        | SET_LOOK_FOR_ENEMIES
        | SET_FACE_MOVE_DIR
        | SET_FORCED_MARCH
        | SET_UNDYING
        | SET_NOAVOID
        | SET_LOOP_ANIM
        | SET_SHIELDS
        | SET_VAMPIRE
        | SET_FORCE_INVINCIBLE
        | SET_GREET_ALLIES
        | SET_PLAYER_LOCKED
        | SET_LOCK_PLAYER_WEAPONS
        | SET_NO_IMPACT_DAMAGE
        | SET_ALT_FIRE
        | SET_NO_RESPONSE
        | SET_NO_COMBAT_TALK
        | SET_NO_ALERT_TALK
        | SET_USE_CP_NEAREST
        | SET_DISMEMBERABLE
        | SET_NO_FORCE
        | SET_NO_ACROBATICS
        | SET_USE_SUBTITLES
        | SET_NO_FALLTODEATH
        | SET_MORELIGHT
        | SET_TREASONED
        | SET_DISABLE_SHADER_ANIM
        | SET_SHADER_ANIM => return None,
        _ => {
            if icarus.variables().declared(name) != VariableType::Float {
                return None;
            }
            return icarus.variables().float(name);
        }
    };
    Some(value)
}

/// `Q3_GetVector` (`:1591-1655`).
pub fn get_vector<W: ScriptWorld>(
    world: &mut W,
    icarus: &Icarus<W::Id>,
    id: W::Id,
    name: &str,
) -> Option<[f32; 3]> {
    let set = set_id(name);
    let ent = world.entity(id)?;
    if let Some(parm) = parm_index(set) {
        // The reference reads the parm without checking that parms exist; an entity
        // without any has none to read here.
        let text = ent.parm(parm).unwrap_or("");
        let mut value = [0.0; 3];
        if cnum::scan_vector(text, &mut value) != 3 {
            debug_print(
                world,
                DebugLevel::Warning,
                &format!("Q3_GetVector: failed sscanf on SET_PARM{set} ({name})\n"),
            );
            value = [0.0; 3];
        }
        return Some(value);
    }
    match set {
        SET_ORIGIN => Some(ent.motion.origin),
        SET_ANGLES => Some(ent.motion.angles),
        SET_TELEPORT_DEST => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetVector: SET_TELEPORT_DEST not implemented\n",
            );
            None
        }
        _ => {
            if icarus.variables().declared(name) != VariableType::Vector {
                return None;
            }
            icarus.variables().vector(name)
        }
    }
}

/// The behaviour set a `SET_*SCRIPT` reads.
fn behavior_set(set: i32) -> Option<usize> {
    Some(match set {
        SET_SPAWNSCRIPT => BSET_SPAWN,
        SET_USESCRIPT => BSET_USE,
        SET_AWAKESCRIPT => BSET_AWAKE,
        SET_ANGERSCRIPT => BSET_ANGER,
        SET_ATTACKSCRIPT => BSET_ATTACK,
        SET_VICTORYSCRIPT => BSET_VICTORY,
        SET_LOSTENEMYSCRIPT => BSET_LOSTENEMY,
        SET_PAINSCRIPT => BSET_PAIN,
        SET_FLEESCRIPT => BSET_FLEE,
        SET_DEATHSCRIPT => BSET_DEATH,
        SET_DELAYEDSCRIPT => BSET_DELAYED,
        SET_BLOCKEDSCRIPT => BSET_BLOCKED,
        SET_FFIRESCRIPT => BSET_FFIRE,
        SET_FFDEATHSCRIPT => BSET_FFDEATH,
        _ => return None,
    })
}

/// The `get(STRING)` names whose read prints that it is not implemented.
const UNIMPLEMENTED_STRINGS: [(i32, &str); 14] = [
    (SET_NAVGOAL, "SET_NAVGOAL"),
    (SET_VIEWTARGET, "SET_VIEWTARGET"),
    (SET_VIEWENTITY, "SET_VIEWENTITY"),
    (SET_CAPTIONTEXTCOLOR, "SET_CAPTIONTEXTCOLOR"),
    (SET_CENTERTEXTCOLOR, "SET_CENTERTEXTCOLOR"),
    (SET_SCROLLTEXTCOLOR, "SET_SCROLLTEXTCOLOR"),
    (SET_COPY_ORIGIN, "SET_COPY_ORIGIN"),
    // (sic: the reference names the wrong field)
    (SET_DEFEND_TARGET, "SET_COPY_ORIGIN"),
    (SET_VIDEO_PLAY, "SET_VIDEO_PLAY"),
    (SET_LOADGAME, "SET_LOADGAME"),
    (SET_LOCKYAW, "SET_LOCKYAW"),
    (SET_SCROLLTEXT, "SET_SCROLLTEXT"),
    (SET_LCARSTEXT, "SET_LCARSTEXT"),
    (SET_LOOK_TARGET, ""),
];

/// `Q3_GetString` (`:1663-1876`). A `NULL` field is found with no text (the engine's
/// wrapper then copies nothing into its buffer, which it guards against).
pub fn get_string<W: ScriptWorld>(
    world: &mut W,
    icarus: &Icarus<W::Id>,
    id: W::Id,
    name: &str,
) -> StringAnswer {
    let not_found = StringAnswer {
        found: false,
        value: None,
    };
    let found = |value: Option<String>| StringAnswer { found: true, value };
    let set = set_id(name);
    let Some(ent) = world.entity(id) else {
        return not_found;
    };
    if let Some(parm) = parm_index(set) {
        return match &ent.parms {
            Some(parms) => found(Some(parms[parm].clone())),
            None => {
                let line = format!(
                    "Q3_GetString: invalid ent {} has no parms!\n",
                    c_str(ent.targetname.as_deref())
                );
                debug_print(world, DebugLevel::Warning, &line);
                not_found
            }
        };
    }
    if let Some(bset) = behavior_set(set) {
        return found(ent.behavior_sets[bset].clone());
    }
    match set {
        SET_ANIM_BOTH => match animation_both(world, id) {
            Some(both) => found(Some(both)),
            None => not_found,
        },
        SET_TARGET => found(ent.target.clone()),
        SET_TARGETNAME => found(ent.targetname.clone()),
        SET_FULLNAME => found(ent.full_name.clone()),
        SET_LOOK_TARGET => {
            debug_print(
                world,
                DebugLevel::Warning,
                "Q3_GetString: SET_LOOK_TARGET, NOT SUPPORTED IN MULTIPLAYER\n",
            );
            // No `return 0`: it falls out as found, the value left as it was.
            StringAnswer {
                found: true,
                value: None,
            }
        }
        SET_LOCATION | SET_ENEMY | SET_LEADER | SET_CAPTURE | SET_PAINTARGET | SET_CAMERA_GROUP
        | SET_CAMERA_GROUP_TAG | SET_TARGET2 | SET_REMOVE_TARGET | SET_WEAPON | SET_ITEM
        | SET_MUSIC_STATE | SET_WATCHTARGET => not_found,
        _ => {
            if let Some(&(_, label)) = UNIMPLEMENTED_STRINGS
                .iter()
                .find(|(entry, _)| *entry == set)
            {
                debug_print(
                    world,
                    DebugLevel::Warning,
                    &format!("Q3_GetString: {label} not implemented\n"),
                );
                return not_found;
            }
            if icarus.variables().declared(name) != VariableType::String {
                return not_found;
            }
            // `ICARUS_GetStringVariable(name, *value)`: the reference hands the variable
            // the output pointer, so the answer is found and the buffer unchanged.
            StringAnswer {
                found: icarus.variables().string(name).is_some(),
                value: None,
            }
        }
    }
}

/// `Q3_GetAnimBoth` (`:389-414`): the legs' animation, if both are named.
fn animation_both<W: ScriptWorld>(world: &mut W, id: W::Id) -> Option<String> {
    let Some((legs, torso)) = world.client_animations(id) else {
        debug_print(
            world,
            DebugLevel::Warning,
            "Q3_GetAnimLower: attempted to read animation state off non-client!\n",
        );
        debug_print(
            world,
            DebugLevel::Warning,
            "Q3_GetAnimUpper: attempted to read animation state off non-client!\n",
        );
        debug_print(
            world,
            DebugLevel::Warning,
            "Q3_GetAnimBoth: NULL legs animation string found!\n",
        );
        return None;
    };
    if legs.is_empty() {
        debug_print(
            world,
            DebugLevel::Warning,
            "Q3_GetAnimBoth: NULL legs animation string found!\n",
        );
        return None;
    }
    if torso.is_empty() {
        debug_print(
            world,
            DebugLevel::Warning,
            "Q3_GetAnimBoth: NULL torso animation string found!\n",
        );
        return None;
    }
    Some(legs)
}

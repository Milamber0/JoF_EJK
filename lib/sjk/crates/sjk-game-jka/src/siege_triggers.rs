//! What siege adds to `trigger_multiple` and `trigger_once` (`g_trigger.c`): nothing fires
//! before the round begins, a trigger of a side is that side's alone, one with an
//! `idealclass` is for those classes, a `siegetrig` only for the carrier of the item whose
//! `goaltarget` it is (which the trigger takes and frees), a `teambalance` zone changes hands
//! when one side outnumbers the other inside it (`target3`/`target4`), and a USE_BUTTON
//! trigger with a `usetime` has to be hacked — the use key held that long, facing the same
//! way, standing inside it.
//!
//! The rules are here; the server gathers what they read (the activator, the carried item,
//! who stands in the zone) and does what they answer.

use sjk_entity::Entity;

/// `SIEGETEAM_TEAM1`, `SIEGETEAM_TEAM2`.
const TEAM1: i32 = 1;
const TEAM2: i32 = 2;
/// The longest hack a client's bar can show (`hackingBaseTime` is 16 bits on the wire).
const HACK_LIMIT: i32 = 60_000;
/// How far a hacker may turn from where it faced when it began (`g_main.c:3240`).
const HACK_TURN: f32 = 10.0;

/// A trigger's siege keys (`SP_trigger_multiple`, `SP_trigger_once`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiegeKeys {
    /// `genericValue1` (`siegetrig`): only an item's carrier sets it off.
    pub siegetrig: bool,
    /// `genericValue2` (`teambalance`, `trigger_multiple` only).
    pub teambalance: bool,
    /// `genericValue7` (`usetime`): how long the use key is held, in milliseconds.
    pub usetime: i32,
    /// `idealclass`: the class (or `|`-separated classes) that may use it.
    pub idealclass: Option<String>,
    /// Fired when team 1 or team 2 takes a `teambalance` zone.
    pub target3: Option<String>,
    pub target4: Option<String>,
    /// `genericValue3`: the side that holds the zone.
    pub owner: i32,
    /// `genericValue4`: the side whose target the next firing uses first.
    pub taken_by: i32,
}

/// The siege keys of a trigger entity; `once` for `trigger_once`, which has no
/// `teambalance`.
pub fn keys(entity: &Entity, once: bool) -> SiegeKeys {
    let int = |key: &str| {
        entity
            .get(key)
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
    };
    let text = |key: &str| {
        entity
            .get(key)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    SiegeKeys {
        siegetrig: int("siegetrig") != 0,
        teambalance: !once && int("teambalance") != 0,
        usetime: int("usetime"),
        idealclass: text("idealclass"),
        target3: text("target3"),
        target4: text("target4"),
        owner: 0,
        taken_by: 0,
    }
}

/// `G_NameInTriggerClassList(list, str)`: whether `name` is one of the `|`-separated
/// entries of `list`, any case. The reference calls it with the player's class as the
/// list and the trigger's `idealclass` as the name, so an `idealclass` naming more than one
/// class matches nobody; that order is kept.
pub fn name_in_class_list(list: &str, name: &str) -> bool {
    list.split('|')
        .any(|entry| entry.eq_ignore_ascii_case(name))
}

/// The class rule of a trigger with an `idealclass` in siege: a player with no class, or a
/// class not in the list, may not use it.
fn class_allows(idealclass: &str, class: Option<&str>) -> bool {
    class.is_some_and(|class| name_in_class_list(class, idealclass))
}

/// The carried item as a goal trigger reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carried<'a> {
    pub goaltarget: Option<&'a str>,
    /// `teamnocomplete`.
    pub team_no_complete: i32,
    pub target3: Option<&'a str>,
}

/// Who sets a trigger off, as `multi_trigger` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Activator<'a> {
    /// `activator->client`.
    pub client: bool,
    pub team: i32,
    /// The class it plays; `None` for none (`siegeClass < 0`).
    pub class: Option<&'a str>,
    /// The item it carries (`holdingObjectiveItem`), where it is still in the level.
    pub carried: Option<Carried<'a>>,
}

/// What `multi_trigger`'s siege half decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gate {
    /// Whether the trigger goes on to its waits and its firing.
    pub pass: bool,
    /// The carried item was delivered: the server frees it (the carrier lets go), after
    /// firing its `target3` where it has one.
    pub delivered: Option<Option<String>>,
}

/// `multi_trigger`'s siege gates (`g_trigger.c:160-300`) for trigger `trigger` named
/// `targetname`. `counts` gives the living team 1 and team 2 players in the trigger's box,
/// asked only for a `teambalance` zone.
pub fn gates(
    keys: &mut SiegeKeys,
    allied_team: i32,
    targetname: &str,
    siege: bool,
    round_begun: bool,
    activator: Option<&Activator>,
    counts: &mut dyn FnMut() -> (i32, i32),
) -> Gate {
    let refuse = |delivered| Gate {
        pass: false,
        delivered,
    };
    if siege && !round_begun {
        return refuse(None);
    }
    if siege
        && allied_team != 0
        && activator.is_some_and(|who| who.client && who.team != allied_team)
    {
        return refuse(None);
    }
    if siege && let Some(idealclass) = &keys.idealclass {
        if !activator.is_some_and(|who| who.client && class_allows(idealclass, who.class)) {
            return refuse(None);
        }
    }
    let mut halt = false;
    let mut delivered = None;
    if siege && keys.siegetrig {
        halt = true;
        let carried = activator
            .filter(|who| who.client)
            .and_then(|who| who.carried.map(|item| (who.team, item)));
        if let Some((team, item)) = carried.filter(|_| !targetname.is_empty())
            && item
                .goaltarget
                .is_some_and(|goal| goal.eq_ignore_ascii_case(targetname))
            && item.team_no_complete != team
        {
            // With a `target3` it is fired instead, and the trigger's own target too since
            // this trigger has a name.
            halt = false;
            delivered = Some(item.target3.map(str::to_owned));
        }
    } else if keys.siegetrig {
        return refuse(None);
    }
    if keys.teambalance {
        if !siege {
            return refuse(delivered);
        }
        let Some(who) =
            activator.filter(|who| who.client && (who.team == TEAM1 || who.team == TEAM2))
        else {
            return refuse(delivered);
        };
        let _ = who;
        let (team1, team2) = counts();
        if (team1 == 0 && team2 == 0) || team1 == team2 {
            return refuse(delivered);
        }
        let owner = if team1 > team2 { TEAM1 } else { TEAM2 };
        if keys.owner == owner {
            return refuse(delivered);
        }
        keys.owner = owner;
        keys.taken_by = owner;
    }
    if halt {
        return refuse(delivered);
    }
    Gate {
        pass: true,
        delivered,
    }
}

/// `multi_trigger_run`'s siege half: the target of the side that just took a
/// `teambalance` zone, fired before the trigger's own (and forgotten).
pub fn taken_target(keys: &mut SiegeKeys) -> Option<String> {
    let taken = std::mem::take(&mut keys.taken_by);
    match taken {
        TEAM1 => keys.target3.clone(),
        TEAM2 => keys.target4.clone(),
        _ => None,
    }
}

/// A client's hack (`isHacking`, `hackingAngles`, `ps.hackingTime`, `ps.hackingBaseTime`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hack {
    /// The trigger being hacked (its entity number), 0 for none.
    pub trigger: u16,
    pub angles: [f32; 3],
    pub time: i32,
    pub base_time: i32,
}

/// What siege keeps on a client for its objectives: the item it carries
/// (`holdingObjectiveItem`, 0 for none) and its hack.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SiegeHands {
    pub holding: u16,
    pub hack: Hack,
}

/// Who presses a hack trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hacker<'a> {
    /// Its number is a client's (`s.number < MAX_CLIENTS`).
    pub player: bool,
    pub class: Option<&'a str>,
    pub origin: [f32; 3],
    pub view_angles: [f32; 3],
}

/// `Touch_Multi`'s `usetime` block (`g_trigger.c:462-497`) for trigger `number` whose box
/// is `absmin`..`absmax`: whether the touch goes on to fire it. A hack is started (and the
/// touch ends), goes on (and the touch ends), or is done (and the touch goes on).
pub fn hack_gate(
    keys: &SiegeKeys,
    number: u16,
    absmin: [f32; 3],
    absmax: [f32; 3],
    siege: bool,
    who: &Hacker,
    hack: &mut Hack,
    level_time: i32,
) -> bool {
    if keys.usetime == 0 {
        return true;
    }
    if siege && let Some(idealclass) = &keys.idealclass {
        if !class_allows(idealclass, who.class) {
            return false;
        }
    }
    if !point_in_bounds(who.origin, absmin, absmax) {
        return false;
    }
    if hack.trigger != number && who.player {
        hack.trigger = number;
        hack.angles = who.view_angles;
        hack.time = level_time + keys.usetime;
        hack.base_time = keys.usetime;
        if hack.base_time > HACK_LIMIT {
            hack.time = level_time + HACK_LIMIT;
            hack.base_time = HACK_LIMIT;
        }
        return false;
    }
    if hack.time < level_time {
        hack.trigger = 0;
        hack.time = 0;
        return true;
    }
    false
}

/// `G_RunFrame`'s hacking checks (`g_main.c:3206-3244`) once the pose is kept: the hack
/// ends when the use key is let go, the trigger is gone, the hacker steps out of its box
/// (`hacked` is its box) or turns more than ten degrees.
pub fn hack_frame(
    hack: &mut Hack,
    using: bool,
    hacked: Option<([f32; 3], [f32; 3])>,
    origin: [f32; 3],
    view_angles: [f32; 3],
) {
    let difference: [f32; 3] = std::array::from_fn(|axis| view_angles[axis] - hack.angles[axis]);
    let turned = (difference[0] * difference[0]
        + difference[1] * difference[1]
        + difference[2] * difference[2])
        .sqrt()
        > HACK_TURN;
    let broken =
        !using || hacked.is_none_or(|(min, max)| !point_in_bounds(origin, min, max)) || turned;
    if broken {
        hack.trigger = 0;
        hack.time = 0;
    }
}

/// `G_PointInBounds`: inside or on the box.
pub fn point_in_bounds(point: [f32; 3], mins: [f32; 3], maxs: [f32; 3]) -> bool {
    (0..3).all(|axis| point[axis] >= mins[axis] && point[axis] <= maxs[axis])
}

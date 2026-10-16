//! Force push and pull (`ForceThrow`, `w_force.c:2914-3677`): the thrower's hand, sound,
//! cost and powerups; who is reached — at level 1 the one aimed at, at 2 and 3 anyone
//! within the arc (60 or 180 degrees) — in front, within 1024 units, in the thrower's
//! PVS and in its sight; and what it does to each: a player is shoved along the line
//! between them (less the further it is, at least 16), knocked down at level 3 up close,
//! may push back if it has the power and stands (`CanCounterThrow`, a level less if it
//! moves), may lose the weapon in its hand to a pull, and absorption takes some of it
//! (`WP_AbsorbConversion`); a flying missile is pushed back at whoever fired it
//! (`G_ReflectMissile`).
//!
//! The update's Force button calls for the throw ([`crate::force_powers::Begun::throw`]);
//! the caller runs [`throw`] with a [`ThrowWorld`] that reaches every player and entity,
//! then finishes the update.
//!
//! A push or pull on the one gripping the thrower breaks its grip at the grip's level or
//! above.
//!
//! NPCs are clients too: they throw and are thrown alike (an NPC's world is
//! [`crate::npc_force_throw`]'s), but a vehicle, and any NPC in siege, is out of reach.
//!
//! Not yet: doors, buttons and `func_static`s pushed or pulled (the movers' part), thrown
//! sabers and limbs, a push breaking a saber lock, and riders thrown off their vehicles.

use crate::event_entity::EventEntity;
use crate::force_powers::{FORCE_POWER_NEEDED, FP_ABSORB, FP_GRIP, FP_PULL, FP_PUSH, ForcePowers};
use crate::knockdown::Knockdown;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use sjk_protocol::{PlayerState, UserCommand};

/// A client (a player or an NPC) the throw reads and changes: its wire state, its Force,
/// the game's health, its knockdown memory, who last shoved it, its class if it is an NPC,
/// the last command, and the push effect's end.
pub struct ThrowPlayer<'a> {
    pub state: &'a mut PlayerState,
    pub force: &'a mut ForcePowers,
    pub health: i32,
    pub knockdown: &'a mut Knockdown,
    /// Who last pushed or held it (`ps.otherKiller`), which a shove sets.
    pub other_killer: &'a mut crate::damage::OtherKiller,
    /// An NPC's class (`NPC_class`); `None` for a player.
    pub npc_class: Option<i32>,
    pub invulnerable_until: &'a mut i32,
    /// `pers.cmd`.
    pub command: UserCommand,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// `pushEffectTime`: until when `EF_BODYPUSH` shows.
    pub push_effect_until: &'a mut i32,
    /// `ps.saberLockHits`, which a push in a saber lock adds to.
    pub lock_hits: &'a mut i32,
}

/// An entity `EntitiesInBox` found, as the throw reads it.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub number: u16,
    pub kind: CandidateKind,
    /// `r.absmin`, `r.absmax`.
    pub absmin: [f32; 3],
    pub absmax: [f32; 3],
    /// A player's `ps.origin`; anything else's `s.pos.trBase`.
    pub origin: [f32; 3],
}

/// What a candidate is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateKind {
    Player,
    /// A missile: its trajectory type, `EF_MISSILE_STICK`, its weapon.
    Missile {
        trajectory: u32,
        stuck: bool,
        weapon: u32,
    },
    /// Anything else the throw leaves alone (items, the world, movers not yet ported).
    Other,
}

/// Everything beyond the thrower that a throw reaches.
pub trait ThrowWorld {
    /// `trap->EntitiesInBox`, in entity number order.
    fn entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3], out: &mut Vec<Candidate>);
    /// A point trace (`trap->Trace` with no box) past `pass`.
    fn trace(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace;
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// A player, one at a time.
    fn player(&mut self, number: u16) -> Option<ThrowPlayer<'_>>;
    /// `G_ReflectMissile(thrower, missile, forward)`.
    fn reflect_missile(&mut self, missile: u16, thrower: u16, forward: [f32; 3]);
    /// `TossClientWeapon(victim, direction, speed)`.
    fn toss_weapon(&mut self, victim: u16, direction: [f32; 3], speed: f32);
    /// A temp entity raised now.
    fn raise(&mut self, event: EventEntity);
    /// `G_SoundIndex`.
    fn sound_index(&mut self, name: &[u8]) -> u16;
    /// The game's generator (`Q_irand`).
    fn rng(&mut self) -> &mut Rng;
}

const RADIUS: f32 = 1_024.0;
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const ENTITY_NUMBER_NONE: u16 = 1_023;
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_FORCEPUSH: u32 = 1;
const HANDEXTEND_FORCEPULL: u32 = 2;
const HANDEXTEND_KNOCKDOWN: u32 = 8;
const PW_PULL: usize = 3;
const PW_DISINT_4: usize = 9;
const CHAN_VOICE: u32 = 3;
const CHAN_BODY: u32 = 6;
const TR_STATIONARY: u32 = 0;
const TR_INTERPOLATE: u32 = 1;
const WP_THERMAL: u32 = 12;
const WEAPON_CHARGING: u32 = 4;
const WEAPON_CHARGING_ALT: u32 = 5;
const EF_INVULNERABLE: u32 = 1 << 27;
const PDSOUND_ABSORBHIT: u32 = 3;
const GT_SIEGE: i32 = 7;
const PS_VELOCITY: [usize; 3] = [6, 7, 8];
const PS_WEAPON_TIME: usize = 10;
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
const PS_GROUND_ENTITY: usize = 16;
const PS_EFLAGS: usize = 17;
const PS_FORCE_POWER: usize = 18;
const PS_VIEW_HEIGHT: usize = 22;
const PS_ROCKET_LOCK_INDEX: usize = 24;
const PS_WEAPON_STATE: usize = 33;
const PS_ACTIVE: usize = 82;
const PS_ROCKET_TARGET_TIME: usize = 71;
const PS_ROCKET_LOCK_TIME: usize = 79;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_FORCE_DODGE_ANIM: usize = 89;
const ES_SABER_ENTITY: usize = 37;
const ES_TRICKED: usize = 58;
const EV_GENERAL_SOUND: u32 = 76;

/// The throw's context: the time, the game type, the thrower's number.
#[derive(Clone, Copy, Debug)]
pub struct Throw {
    pub level_time: i32,
    pub gametype: i32,
    pub thrower: u16,
    pub pull: bool,
}

/// What the thrower brings to every target, read once.
struct Thrower {
    origin: [f32; 3],
    eye: [f32; 3],
    view: [f32; 3],
    forward: [f32; 3],
    level: usize,
    cost: i32,
    team: i32,
}

/// `ForceThrow(self, pull)`, called by the thrower's Force button. The thrower's own
/// state changes first (the cost, the hand, the sound, the powerups); then each target
/// in turn; then the thrower's invulnerability ends.
pub fn throw(context: Throw, world: &mut dyn ThrowWorld) {
    let Some(thrower) = begin(context, world) else {
        return;
    };
    let mut candidates = Vec::new();
    // At level 1 an aim that strikes nothing, or a player the power cannot reach, ends
    // the throw there: the thrower's protection and danger time are left as they were.
    let Some(listed) = targets(context, &thrower, world, &mut candidates) else {
        return;
    };
    let mut reached = Vec::new();
    for candidate in listed {
        if let Some(candidate) = in_reach(context, &thrower, candidate, world) {
            reached.push(candidate);
        }
    }
    for candidate in reached {
        match candidate.kind {
            CandidateKind::Player => shove(context, &thrower, candidate, world),
            CandidateKind::Missile {
                trajectory, weapon, ..
            } => {
                let rolling_thermal = trajectory == TR_INTERPOLATE && weapon == WP_THERMAL;
                if trajectory != TR_STATIONARY && !rolling_thermal && !context.pull {
                    world.reflect_missile(candidate.number, context.thrower, thrower.forward);
                }
            }
            CandidateKind::Other => {}
        }
    }
    // The pusher's spawn protection is over; the grip on it ends.
    if let Some(player) = world.player(context.thrower) {
        let flags = player.state.raw_field(PS_EFLAGS).unwrap_or(0);
        player
            .state
            .set_raw_field(PS_EFLAGS, flags & !EF_INVULNERABLE);
        *player.invulnerable_until = 0;
        player.force.danger_time = context.level_time;
        if player.force.grip_being_gripped > context.level_time as f32 {
            player.force.grip_being_gripped = 0.0;
        }
    }
}

/// The thrower's part before anyone is reached (`w_force.c:2941-3068`): whether it may
/// throw at all, and then its cost, its hand, its sound and its powerups.
fn begin(context: Throw, world: &mut dyn ThrowWorld) -> Option<Thrower> {
    let level_time = context.level_time;
    let power = if context.pull { FP_PULL } else { FP_PUSH };
    let player = world.player(context.thrower)?;
    let state = player.state;
    let field = |state: &PlayerState, index: usize| state.raw_field(index).unwrap_or(0);
    let hand = field(state, PS_FORCE_HAND_EXTEND);
    if hand != HANDEXTEND_NONE && (hand != HANDEXTEND_KNOCKDOWN || !in_get_up(state)) {
        return None;
    }
    // `g_useWhileThrowing` is 1: a thrown saber does not keep its thrower from throwing.
    if field(state, PS_WEAPON_TIME) as i32 > 0
        || player.health <= 0
        || state.powerups[PW_DISINT_4] as i32 > level_time
    {
        return None;
    }
    if !crate::force_powers::usable(
        state,
        player.force,
        player.health,
        power,
        level_time,
        context.gametype,
    ) {
        return None;
    }
    // `G_SoundIndex` once the thrower may throw (`w_force.c:2984`, `3056`).
    let sound = world.sound_index(if context.pull {
        b"sound/weapons/force/pull.wav"
    } else {
        b"sound/weapons/force/push.wav"
    });
    let player = world.player(context.thrower)?;
    let state = player.state;
    // `BG_ClearRocketLock`.
    state.set_raw_field(PS_ROCKET_LOCK_INDEX, u32::from(ENTITY_NUMBER_NONE));
    state.set_raw_field(PS_ROCKET_LOCK_TIME, (-1.0f32).to_bits());
    state.set_raw_field(PS_ROCKET_TARGET_TIME, 0);
    // A push in a saber lock (`w_force.c:2982-2991`) pushes the lock: its level twice
    // over in hits, and nothing else is thrown.
    const PS_SABER_LOCK_TIME: usize = 107;
    const PS_SABER_LOCK_FRAME: usize = 108;
    if !context.pull
        && field(state, PS_SABER_LOCK_TIME) as i32 > level_time
        && field(state, PS_SABER_LOCK_FRAME) != 0
    {
        let mut sound_event = EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(sound),
            origin: state.origin(),
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        sound_event.extra[0] = (ES_SABER_ENTITY, CHAN_BODY);
        state.powerups[PW_DISINT_4] = (level_time + 1_500) as u32;
        *player.lock_hits += i32::from(player.force.levels[FP_PUSH]) * 2;
        start(state, player.force, power, level_time);
        world.raise(sound_event);
        return None;
    }
    start(state, player.force, power, level_time);
    let origin = state.origin();
    let mut sound_event = EventEntity {
        event: EV_GENERAL_SOUND,
        parameter: u32::from(sound),
        origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    sound_event.extra[0] = (ES_SABER_ENTITY, CHAN_BODY);
    if context.pull {
        if hand == HANDEXTEND_NONE {
            state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_FORCEPULL);
            let hold = if context.gametype == GT_SIEGE && field(state, 47) == 3 {
                200
            } else {
                400
            };
            player.knockdown.hand_extend_time = level_time + hold;
        }
        state.powerups[PW_DISINT_4] = (player.knockdown.hand_extend_time + 200) as u32;
        state.powerups[PW_PULL] = state.powerups[PW_DISINT_4];
    } else {
        if hand == HANDEXTEND_NONE {
            state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_FORCEPUSH);
            player.knockdown.hand_extend_time = level_time + 1_000;
        } else if hand == HANDEXTEND_KNOCKDOWN && in_get_up(state) {
            // Pushed from the ground: the push on the upper body, the get-up on the legs.
            let mut dodge = field(state, PS_FORCE_DODGE_ANIM);
            if dodge > 4 {
                dodge -= 8;
            }
            state.set_raw_field(PS_FORCE_DODGE_ANIM, dodge + 8);
        }
        state.powerups[PW_DISINT_4] = (level_time + 1_100) as u32;
        state.powerups[PW_PULL] = 0;
    }
    let view = state.view_angles();
    let (forward, _) = crate::pmove::flight::flight_axes(view);
    let forward = forward.to_array();
    let level = usize::from(player.force.levels[power]);
    let eye = [
        origin[0],
        origin[1],
        origin[2] + field(state, PS_VIEW_HEIGHT) as i32 as f32,
    ];
    let thrower = Thrower {
        origin,
        eye,
        view,
        forward,
        level,
        cost: FORCE_POWER_NEEDED[level][power],
        team: player.team,
    };
    world.raise(sound_event);
    (level != 0).then_some(thrower)
}

/// `WP_ForcePowerStart` for a throw: a full-body taunt cut short, no duration, the noise
/// the bots hear, the cost.
fn start(state: &mut PlayerState, force: &mut ForcePowers, power: usize, level_time: i32) {
    crate::force_powers::heard(force, power, level_time);
    for (animation, timer) in [(PS_LEGS_ANIM, 21), (PS_TORSO_ANIM, 20)] {
        if crate::pmove_input_freeze::full_body_taunt(state.raw_field(animation).unwrap_or(0) as u16)
        {
            state.set_raw_field(timer, 0);
        }
    }
    force.duration[power] = 0;
    force.debounce[power] = 0;
    crate::force_powers::drain(state, force, power, 0);
}

/// Who may be reached (`w_force.c:3108-3224`): at level 1 whatever the aim strikes within
/// 512 units — `None`, the throw given up, where it strikes nothing or a player out of the
/// power's reach; above, everything in the box of 1024 around the thrower but the players
/// outside the arc or out of the power's reach (`ForcePowerUsableOn`).
fn targets(
    context: Throw,
    thrower: &Thrower,
    world: &mut dyn ThrowWorld,
    candidates: &mut Vec<Candidate>,
) -> Option<Vec<Candidate>> {
    let power = if context.pull { FP_PULL } else { FP_PUSH };
    if thrower.level == 1 {
        let end: [f32; 3] =
            std::array::from_fn(|axis| thrower.eye[axis] + thrower.forward[axis] * RADIUS / 2.0);
        let hit = world.trace(thrower.eye, end, context.thrower, MASK_PLAYERSOLID);
        if hit.fraction == 1.0 || hit.entity_number == ENTITY_NUMBER_NONE {
            return None;
        }
        let mins = thrower.origin.map(|axis| axis - RADIUS);
        let maxs = thrower.origin.map(|axis| axis + RADIUS);
        world.entities_in_box(mins, maxs, candidates);
        let target = candidates
            .iter()
            .find(|candidate| candidate.number == hit.entity_number)
            .copied();
        let Some(target) = target else {
            return Some(Vec::new());
        };
        if target.kind == CandidateKind::Player && !usable_on(context, target.number, power, world)
        {
            return None;
        }
        return Some(vec![target]);
    }
    let arc = if thrower.level == 2 { 60.0 } else { 180.0 };
    let mins = thrower.origin.map(|axis| axis - RADIUS);
    let maxs = thrower.origin.map(|axis| axis + RADIUS);
    world.entities_in_box(mins, maxs, candidates);
    let mut listed = Vec::new();
    for candidate in candidates.iter().copied() {
        if candidate.kind == CandidateKind::Player {
            let towards: [f32; 3] =
                std::array::from_fn(|axis| candidate.origin[axis] - thrower.eye[axis]);
            let (pitch, yaw) = crate::damage::vector_to_angles(towards);
            if !in_field_of_vision(thrower.view, arc, [pitch, yaw])
                || !usable_on(context, candidate.number, power, world)
            {
                continue;
            }
        }
        listed.push(candidate);
    }
    Some(listed)
}

/// Whether a listed candidate is really reached (`w_force.c:3226-3322`): not the thrower
/// nor a teammate, a kind the throw moves, in front (within 53 degrees of the aim),
/// within reach of the box's nearest point, in the PVS, and in sight from the thrower's
/// feet or eyes.
fn in_reach(
    context: Throw,
    thrower: &Thrower,
    candidate: Candidate,
    world: &mut dyn ThrowWorld,
) -> Option<Candidate> {
    if candidate.number == context.thrower {
        return None;
    }
    match candidate.kind {
        CandidateKind::Player => {
            let team = world.player(candidate.number).map(|player| player.team)?;
            if on_same_team(context.gametype, thrower.team, team) {
                return None;
            }
        }
        CandidateKind::Missile {
            trajectory,
            stuck,
            weapon,
        } => {
            if trajectory == TR_STATIONARY && (stuck || weapon != WP_THERMAL) {
                return None;
            }
        }
        CandidateKind::Other => return None,
    }
    let centre = thrower.origin;
    let mut v = [0.0f32; 3];
    for axis in 0..3 {
        v[axis] = if centre[axis] < candidate.absmin[axis] {
            candidate.absmin[axis] - centre[axis]
        } else if centre[axis] > candidate.absmax[axis] {
            centre[axis] - candidate.absmax[axis]
        } else {
            0.0
        };
    }
    let size: [f32; 3] =
        std::array::from_fn(|axis| candidate.absmax[axis] - candidate.absmin[axis]);
    let middle: [f32; 3] = std::array::from_fn(|axis| candidate.absmin[axis] + 0.5 * size[axis]);
    let mut dir: [f32; 3] = std::array::from_fn(|axis| middle[axis] - centre[axis]);
    normalize(&mut dir);
    if dot(dir, thrower.forward) < 0.6 {
        return None;
    }
    if length(v) >= RADIUS {
        return None;
    }
    if !world.in_pvs(middle, thrower.origin) {
        return None;
    }
    let sighted = |world: &mut dyn ThrowWorld, from: [f32; 3]| {
        let hit = world.trace(from, middle, context.thrower, MASK_SHOT);
        !(hit.fraction < 1.0 && hit.entity_number != candidate.number)
    };
    if !sighted(world, thrower.origin) && !sighted(world, thrower.eye) {
        return None;
    }
    Some(candidate)
}

/// What the throw does to a player it reached (`w_force.c:3326-3571`).
fn shove(context: Throw, thrower: &Thrower, candidate: Candidate, world: &mut dyn ThrowWorld) {
    let level_time = context.level_time;
    let power = if context.pull { FP_PULL } else { FP_PUSH };
    let mut events = Vec::new();
    let mod_level = {
        let Some(victim) = world.player(candidate.number) else {
            return;
        };
        match absorb_conversion(
            victim.state,
            victim.force,
            victim.state.client_num(),
            thrower.level as i32,
            thrower.cost,
            level_time,
        ) {
            Some((level, event)) => {
                events.extend(event);
                level
            }
            None => thrower.level as i32,
        }
    };
    let push_sound = world.sound_index(if context.pull {
        b"sound/weapons/force/pull.wav"
    } else {
        b"sound/weapons/force/push.wav"
    });
    let push_power = 256 * mod_level;
    let mut can_pull_weapon = true;
    let mut dir_len = 0.0f32;
    let origin;
    let other_push;
    let mut push_mod = push_power;
    let mut toss = None;
    {
        let Some(victim) = world.player(candidate.number) else {
            return;
        };
        origin = victim.state.origin();
        let mut other = i32::from(victim.force.levels[power]);
        if victim.command.forward_move != 0 || victim.command.right_move != 0 {
            other = (other - 1).max(0);
        }
        other_push = other;
        if other != 0 && can_counter_throw(&victim, thrower, context, power) {
            let hand = if context.pull {
                HANDEXTEND_FORCEPULL
            } else {
                HANDEXTEND_FORCEPUSH
            };
            victim.state.set_raw_field(PS_FORCE_HAND_EXTEND, hand);
            victim.knockdown.hand_extend_time = level_time + if context.pull { 400 } else { 1_000 };
            victim.state.powerups[PW_DISINT_4] = (victim.knockdown.hand_extend_time + 200) as u32;
            victim.state.powerups[PW_PULL] = if context.pull {
                victim.state.powerups[PW_DISINT_4]
            } else {
                0
            };
            let mut sound = EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(push_sound),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            sound.extra[0] = (ES_SABER_ENTITY, CHAN_BODY);
            events.push(sound);
            if other >= mod_level {
                push_mod = 0;
                can_pull_weapon = false;
            } else {
                let difference = mod_level - other;
                let cut = match difference {
                    3.. => 0.2,
                    2 => 0.4,
                    _ => 0.8,
                };
                push_mod = (push_mod as f64 - f64::from(push_mod) * cut) as i32;
                push_mod = push_mod.max(0);
            }
        }
    }
    for event in events {
        world.raise(event);
    }
    let push_dir: [f32; 3] = if context.pull {
        let towards: [f32; 3] = std::array::from_fn(|axis| thrower.origin[axis] - origin[axis]);
        if length(towards) <= 256.0 {
            let chance = match mod_level {
                1 => 3,
                2 => 7,
                3 => 10,
                _ => 0,
            };
            let foe = world
                .player(candidate.number)
                .is_some_and(|victim| !on_same_team(context.gametype, thrower.team, victim.team));
            if foe && world.rng().irand(1, 10) <= chance && can_pull_weapon {
                let mut above = thrower.origin;
                above[2] += 64.0;
                let mut to_thrower: [f32; 3] =
                    std::array::from_fn(|axis| above[axis] - origin[axis]);
                normalize(&mut to_thrower);
                toss = Some(to_thrower);
            }
        }
        towards
    } else {
        std::array::from_fn(|axis| origin[axis] - thrower.origin[axis])
    };
    if let Some(direction) = toss {
        world.toss_weapon(candidate.number, direction, 500.0);
    }
    let Some(victim) = world.player(candidate.number) else {
        return;
    };
    let mut push_dir = push_dir;
    let vehicle = victim.state.raw_field(84).unwrap_or(0) != 0;
    if (mod_level > other_push || vehicle)
        && mod_level == 3
        && victim.state.raw_field(PS_FORCE_HAND_EXTEND) != Some(HANDEXTEND_KNOCKDOWN)
    {
        dir_len = length(push_dir);
        if crate::knockdown::knockdownable(victim.state)
            && dir_len <= (64 * ((mod_level - other_push) - 1)) as f32
        {
            victim
                .state
                .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_KNOCKDOWN);
            victim.knockdown.hand_extend_time = level_time + 700;
            victim.state.set_raw_field(PS_FORCE_DODGE_ANIM, 0);
            victim.knockdown.quicker_getup = true;
        }
    }
    if dir_len == 0.0 {
        dir_len = length(push_dir);
    }
    normalize(&mut push_dir);
    break_grip(context, candidate.number, mod_level, world);
    let Some(victim) = world.player(candidate.number) else {
        return;
    };
    *victim.other_killer = crate::damage::OtherKiller::credit(context.thrower, level_time);
    // `pushPowerMod -= dirLen*0.7`: an int less a double product, truncated.
    push_mod = (f64::from(push_mod) - f64::from(dir_len) * 0.7) as i32;
    push_mod = push_mod.max(16);
    *victim.push_effect_until = level_time + 600;
    let mut velocity = victim.state.velocity();
    velocity[0] = push_dir[0] * push_mod as f32;
    velocity[1] = push_dir[1] * push_mod as f32;
    if velocity[2] as i32 == 0 {
        velocity[2] = (push_dir[2] * push_mod as f32).max(128.0);
    } else {
        velocity[2] = push_dir[2] * push_mod as f32;
    }
    for (axis, index) in PS_VELOCITY.into_iter().enumerate() {
        victim.state.set_raw_field(index, velocity[axis].to_bits());
    }
}

/// A thrower held in a grip breaks it by throwing the one who grips it, at the grip's
/// level or above (`w_force.c:3517-3529`): the gripper lets go (`WP_ForcePowerStop`) and
/// may grip again in a second, and the thrower is free.
fn break_grip(context: Throw, number: u16, level: i32, world: &mut dyn ThrowWorld) {
    let level_time = context.level_time;
    if !world
        .player(context.thrower)
        .is_some_and(|thrower| thrower.force.grip_being_gripped > level_time as f32)
    {
        return;
    }
    let Some(gripper) = world.player(number) else {
        return;
    };
    if gripper.force.grip_entity != context.thrower
        || level < i32::from(gripper.force.levels[FP_GRIP])
    {
        return;
    }
    let active = gripper.state.raw_field(PS_ACTIVE).unwrap_or(0);
    gripper
        .state
        .set_raw_field(PS_ACTIVE, active & !(1 << FP_GRIP));
    let (held, gasp) = crate::force_dark::let_go(
        gripper.state,
        gripper.force,
        &mut gripper.knockdown.hand_extend_time,
        active & (1 << FP_GRIP) != 0,
        level_time,
    );
    gripper.force.grip_use_time = level_time + 1_000;
    let Some(thrower) = world.player(held) else {
        return;
    };
    let gasps = crate::force_dark::released(thrower.force, thrower.health, gasp, level_time);
    thrower.force.grip_being_gripped = 0.0;
    let origin = thrower.state.origin();
    if gasps {
        let mut sound = crate::knockdown::entity_sound(origin, held, CHAN_VOICE);
        sound.parameter = u32::from(world.sound_index(b"*gasp.wav"));
        world.raise(sound);
    }
}

/// `WP_AbsorbConversion` (`w_force.c:791-848`) on player `number` for a push, pull,
/// grip, lightning or drain at `level` that cost `spent`: with absorption running, the
/// power loses the victim's absorb levels and the victim's pool gains a third of the cost
/// for each (one at least), with the absorbing hit's sound at most every 400 ms. Returns
/// the power's level now and that sound's event, or `None` where nothing is absorbed.
pub(crate) fn absorb_conversion(
    state: &mut PlayerState,
    force: &mut ForcePowers,
    number: u16,
    level: i32,
    spent: i32,
    level_time: i32,
) -> Option<(i32, Option<EventEntity>)> {
    let absorb = i32::from(force.levels[FP_ABSORB]);
    if absorb == 0 || state.raw_field(PS_ACTIVE).unwrap_or(0) & (1 << FP_ABSORB) == 0 {
        return None;
    }
    let mut gain = (spent / 3) * absorb;
    if gain < 1 && spent >= 1 {
        gain = 1;
    }
    let pool = (state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32 + gain).min(force.max);
    state.set_raw_field(PS_FORCE_POWER, pool as u32);
    let event = (force.sound_debounce < level_time).then(|| {
        force.sound_debounce = level_time + 400;
        let mut event = crate::knockdown::predef_sound(state.origin(), PDSOUND_ABSORBHIT);
        event.extra[3] = (ES_TRICKED, u32::from(number));
        event
    });
    Some(((level - absorb).max(0), event))
}

/// `CanCounterThrow` (`w_force.c:2761-2829`): a victim with its hands and weapon free,
/// alive, not charging, on the ground, who could use the power itself, pushes back.
fn can_counter_throw(
    victim: &ThrowPlayer<'_>,
    thrower: &Thrower,
    context: Throw,
    power: usize,
) -> bool {
    let state = &*victim.state;
    let field = |index: usize| state.raw_field(index).unwrap_or(0);
    if field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_NONE
        || field(PS_WEAPON_TIME) as i32 > 0
        || victim.health <= 0
    {
        return false;
    }
    if state.powerups[PW_DISINT_4] as i32 > context.level_time
        || matches!(
            field(PS_WEAPON_STATE),
            WEAPON_CHARGING | WEAPON_CHARGING_ALT
        )
    {
        return false;
    }
    if context.gametype == GT_SIEGE && context.pull {
        // A pulled player facing more than 60 degrees away cannot resist.
        let towards: [f32; 3] =
            std::array::from_fn(|axis| thrower.origin[axis] - state.origin()[axis]);
        let (_, yaw) = crate::damage::vector_to_angles(towards);
        let difference = angle_subtract(yaw, state.view_angles()[1]);
        if !(-60.0..=60.0).contains(&difference) {
            return false;
        }
    }
    if !crate::force_powers::usable(
        state,
        victim.force,
        victim.health,
        power,
        context.level_time,
        context.gametype,
    ) {
        return false;
    }
    field(PS_GROUND_ENTITY) != u32::from(ENTITY_NUMBER_NONE)
}

/// `ForcePowerUsableOn` for a player (`w_force.c:543-618`): not one with ysalamiri, nor
/// in a duel, nor — for a push or pull — knocked down.
fn usable_on(context: Throw, number: u16, _power: usize, world: &mut dyn ThrowWorld) -> bool {
    let Some(thrower) = world.player(context.thrower) else {
        return false;
    };
    if !crate::force_powers::can_use_now(
        thrower.state,
        if context.pull { FP_PULL } else { FP_PUSH },
        context.level_time,
        context.gametype,
    ) {
        return false;
    }
    if thrower.state.duel_in_progress() {
        return false;
    }
    let Some(other) = world.player(number) else {
        return false;
    };
    if crate::force_powers::has_ysalamiri(other.state, context.gametype)
        || other.state.duel_in_progress()
    {
        return false;
    }
    if crate::knockdown::in_knockdown(other.state.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16) {
        return false;
    }
    // No push or pull on a vehicle, nor on any NPC in siege (`w_force.c:598-615`).
    !npc_immune(other.npc_class, context.gametype, false)
}

/// `ForcePowerUsableOn`'s NPC rules (`w_force.c:598-615`): the Force does not reach a
/// vehicle (but its `lightning`), nor any NPC in siege.
pub(crate) fn npc_immune(npc_class: Option<i32>, gametype: i32, lightning: bool) -> bool {
    const CLASS_VEHICLE: i32 = 53;
    match npc_class {
        Some(CLASS_VEHICLE) if !lightning => true,
        Some(_) => gametype == GT_SIEGE,
        None => false,
    }
}

/// `OnSameTeam` for players: teammates in the team games; nobody in the others.
fn on_same_team(gametype: i32, one: i32, other: i32) -> bool {
    gametype >= 6 && one == other
}

/// `G_InGetUpAnim`: the legs or the torso in a get-up.
fn in_get_up(state: &PlayerState) -> bool {
    [PS_LEGS_ANIM, PS_TORSO_ANIM]
        .into_iter()
        .any(|index| crate::knockdown::in_get_up(state.raw_field(index).unwrap_or(0) as u16))
}

/// `InFieldOfVision` (`ai_main.c:2058-2095`): `angles` within half of `fov` of the view,
/// in pitch and in yaw.
pub(crate) fn in_field_of_vision(view: [f32; 3], fov: f32, angles: [f32; 2]) -> bool {
    for axis in 0..2 {
        let angle = angle_mod(view[axis]);
        let target = angle_mod(angles[axis]);
        let mut difference = target - angle;
        if target > angle {
            if difference > 180.0 {
                difference -= 360.0;
            }
        } else if difference < -180.0 {
            difference += 360.0;
        }
        if difference > 0.0 {
            if difference > fov * 0.5 {
                return false;
            }
        } else if difference < -fov * 0.5 {
            return false;
        }
    }
    true
}

/// `AngleMod`: an angle's short, as the game rounds it (double constants, a float
/// result).
fn angle_mod(angle: f32) -> f32 {
    ((360.0 / 65_536.0) * f64::from((f64::from(angle) * (65_536.0 / 360.0)) as i32 & 65_535)) as f32
}

/// `AngleSubtract`: the difference brought into -180..180.
fn angle_subtract(a: f32, b: f32) -> f32 {
    let mut difference = a - b;
    while difference > 180.0 {
        difference -= 360.0;
    }
    while difference < -180.0 {
        difference += 360.0;
    }
    difference
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

/// `VectorNormalize`.
fn normalize(v: &mut [f32; 3]) {
    let length = length(*v);
    if length != 0.0 {
        let inverse = 1.0 / length;
        for axis in v.iter_mut() {
            *axis *= inverse;
        }
    }
}

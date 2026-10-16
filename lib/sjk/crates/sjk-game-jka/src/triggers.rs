//! The triggers a map places (OpenJK `codemp/game/g_trigger.c` with `g_active.c`'s
//! `G_TouchTriggers`): brush entities that do something to whoever stands in them.
//!
//! Ported here are the hurt brushes — every pit, every lava floor and every crusher's
//! kill box is one. A hurt brush either takes health at its own rate (`dmg`, every
//! frame or, with the SLOW spawnflag, once a second) or, with `dmg -1`, begins the fall
//! to death that a bottomless pit is: the player screams, loses its controls, and dies
//! three seconds later wherever it has fallen to.
//!
//! A brush entity's shape comes from the map's inline model (`trap->SetBrushModel`),
//! which is the engine's; this module is given the bounds and asks the caller for the
//! exact contact, so the game crate keeps no map format of its own.

use sjk_entity::Entity;
use sjk_protocol::PlayerState;

/// `CONTENTS_TRIGGER`, which `InitTrigger` puts on every trigger.
pub const CONTENTS_TRIGGER: u32 = 0x400;
/// `SVF_NOCLIENT` (`g_public.h:38`): a trigger is never sent to a client.
pub const SVF_NOCLIENT: u32 = 0x0001;
/// `FRAMETIME`: the reference's frame, and a hurt brush's rate without SLOW.
pub const FRAMETIME: i32 = 100;
/// `FALL_FADE_TIME` (`q_shared.h:1050`): how long the fall to death lasts.
pub const FALL_FADE_TIME: i32 = 3_000;
/// `MOD_TRIGGER_HURT`.
pub const MOD_TRIGGER_HURT: u32 = 41;
/// `DAMAGE_NO_PROTECTION`, which a hurt brush always deals.
pub const DAMAGE_NO_PROTECTION: u32 = 8;
/// `CHAN_VOICE`, which the fall's scream is played on.
pub const CHAN_VOICE: u32 = 3;
/// The scream a fall to death plays (`G_SoundIndex`).
pub const FALLING_SOUND: &str = "*falling1.wav";

/// `START_OFF`: not in the world until something uses it.
const START_OFF: u32 = 1;
/// `CAN_TARGET`: a target toggles it.
const CAN_TARGET: u32 = 2;
/// `SLOW`: once a second instead of every frame.
const SLOW: u32 = 16;
/// The spawnflag `InitTrigger` reads as `FL_INACTIVE`.
const INACTIVE: u32 = 128;

/// `playerState_t::fallingToDeath` on the wire.
pub const PS_FALLING_TO_DEATH: usize = 97;
/// `playerState_t::eFlags`.
pub const PS_EFLAGS: usize = 17;
/// `EF_RAG`: limp while falling, even alive.
pub const EF_RAG: u32 = 1 << 6;

/// A `trigger_hurt` as the map's dictionary has it, before the engine gives it a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The inline model the brush is (`"model" "*N"`).
    pub model: usize,
    /// `dmg`, or -1 for the fade-to-death brush a pit is made of.
    pub damage: i32,
    pub spawnflags: u32,
}

/// Every `trigger_hurt` of a map's entity lump, in the lump's order. Other classnames
/// are other steps' (`trigger_multiple`, `trigger_push`, `trigger_teleport`).
pub fn placed(entities: &[Entity]) -> Vec<Placed> {
    let mut placed = Vec::new();
    for entity in entities {
        if entity.get("classname") != Some("trigger_hurt") {
            continue;
        }
        let Some(model) = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse().ok())
        else {
            continue;
        };
        let number = |key: &str| {
            entity
                .get(key)
                .and_then(|text| text.trim().parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        // `SP_trigger_hurt`: five a frame where the map asked for nothing.
        let damage = number("dmg") as i32;
        placed.push(Placed {
            model,
            damage: if damage == 0 { 5 } else { damage },
            spawnflags: number("spawnflags") as u32,
        });
    }
    placed
}

/// A spawned hurt brush: `InitTrigger` with `SP_trigger_hurt`'s tail.
#[derive(Clone, Debug, PartialEq)]
pub struct Hurt {
    pub model: usize,
    /// `r.mins`/`r.maxs` from the inline model — a brush entity stands at the origin, so
    /// these are where it is in the world.
    pub bounds: ([f32; 3], [f32; 3]),
    pub damage: i32,
    pub spawnflags: u32,
    pub contents: u32,
    pub svflags: u32,
    /// `r.linked`: START_OFF leaves it out of the world until something uses it.
    pub linked: bool,
    /// `FL_INACTIVE` from the 128 spawnflag: in the world, but does nothing.
    pub inactive: bool,
    /// `timestamp`: the next moment it may hurt anyone.
    pub next_hurt: i32,
}

/// `SP_trigger_hurt` (`g_trigger.c:1431-1451`) over `InitTrigger` (`:29-41`): the brush's
/// shape, a trigger that no client is told about, five a frame unless the map said
/// otherwise, and out of the world entirely while it starts off.
pub fn spawn_hurt(placed: &Placed, bounds: ([f32; 3], [f32; 3])) -> Hurt {
    Hurt {
        model: placed.model,
        bounds,
        damage: placed.damage,
        spawnflags: placed.spawnflags,
        contents: CONTENTS_TRIGGER,
        svflags: SVF_NOCLIENT,
        linked: placed.spawnflags & START_OFF == 0,
        inactive: placed.spawnflags & INACTIVE != 0,
        next_hurt: 0,
    }
}

/// `ET_PUSH_TRIGGER`: a jump pad, which every client is sent because every client
/// predicts its throw itself.
pub const ET_PUSH_TRIGGER: u32 = 10;
/// `ET_TELEPORT_TRIGGER`.
pub const ET_TELEPORT_TRIGGER: u32 = 11;
/// `s.origin2` on the wire (x, y, z): a jump pad's velocity, a teleporter's nothing.
pub const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
/// `s.eType`, `s.modelindex`.
pub const ES_TYPE: usize = 8;
pub const ES_MODEL: usize = 46;
/// `s.solid` on the wire, and `SOLID_BMODEL`, the value `SV_LinkEntity` gives every entity
/// made of one of the map's brush models (`sv_world.cpp:228`).
pub const ES_SOLID: usize = 26;
pub const SOLID_BMODEL: u32 = 0x00ff_ffff;

/// `trap->SetBrushModel` with the link that follows it: the entity is the map's inline
/// model `model`, and `s.solid` says so. A client draws a brush entity only with
/// `SOLID_BMODEL` (`cg_ents.c`'s `CG_Mover`), and predicts triggers only with it
/// (`CG_TouchTriggerPrediction`).
pub fn set_brush_model(state: &mut sjk_protocol::EntityState, model: usize) {
    state.set_raw_field(ES_MODEL, model as u32);
    state.set_raw_field(ES_SOLID, SOLID_BMODEL);
}
/// `playerState_t::jumppad_ent` on the wire.
pub const PS_JUMPPAD_ENT: usize = 86;
/// `PUSH_LINEAR`: pushed towards the target at a speed instead of thrown at it.
const PUSH_LINEAR: u32 = 4;
/// `PUSH_RELATIVE`: pushed from where the player is, not from the trigger's middle.
const PUSH_RELATIVE: u32 = 16;
/// `PUSH_MULTIPLE`: more than one player a frame.
const PUSH_MULTIPLE: u32 = 2_048;
/// `trigger_push`'s second spawnflag starts it without a touch at all.
const PUSH_START_OFF: u32 = 2;
/// `trigger_teleport`'s SPECTATOR: only they go through, and no client is told about it.
const TELEPORT_SPECTATOR: u32 = 1;
/// `g_gravity`'s default, which a jump pad's arc is worked out in.
pub const GRAVITY: f32 = 800.0;
/// The sounds the two register as they spawn (`G_SoundIndex`).
pub const JUMP_SOUND: &str = "sound/weapons/force/jump.wav";
pub const TELEPORT_SOUND: &str = "sound/weapons/force/speed.wav";

/// A `trigger_push` or `trigger_teleport` as the map's dictionary has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mover {
    /// The inline model the brush is.
    pub model: usize,
    /// What it points at (`target`), which is a `target_position`.
    pub target: String,
    pub spawnflags: u32,
    /// `speed`, which LINEAR pushes at (1000 unless the map said otherwise).
    pub speed: f32,
    /// `wait`, in milliseconds as the reference reads it for a pusher.
    pub wait: f32,
}

/// A spawned `trigger_push` (`:1133-1157`) or `trigger_teleport` (`:1257-1275`): the
/// brush's shape and, unlike every other trigger, a wire state every client is sent —
/// the throw is the client's own to predict.
#[derive(Clone, Debug, PartialEq)]
pub struct Moving {
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    pub kind: u32,
    pub spawnflags: u32,
    pub speed: f32,
    pub wait: f32,
    pub contents: u32,
    pub svflags: u32,
    /// `s.origin2`: what a jump pad throws with, once `AimAtTarget` has worked it out.
    pub velocity: [f32; 3],
    /// Whether it still has a touch at all (a pusher with `wait` of -1 loses it).
    pub touches: bool,
    /// `painDebounceTime`: when it last pushed anybody.
    pub last_push: i32,
    /// `nextthink` for `AimAtTarget`, a frame after the map spawned.
    pub aim_at: i32,
}

/// Every `trigger_push` and `trigger_teleport` of a map's entity lump, with the
/// `target_position` entities they point at (by targetname).
pub fn movers(entities: &[Entity], classname: &str) -> Vec<Mover> {
    let mut placed = Vec::new();
    for entity in entities {
        if entity.get("classname") != Some(classname) {
            continue;
        }
        let Some(model) = entity
            .get("model")
            .and_then(|name| name.strip_prefix('*'))
            .and_then(|index| index.parse().ok())
        else {
            continue;
        };
        let number = |key: &str| {
            entity
                .get(key)
                .and_then(|text| text.trim().parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        placed.push(Mover {
            model,
            target: entity.get("target").unwrap_or_default().to_owned(),
            spawnflags: number("spawnflags") as u32,
            speed: number("speed"),
            wait: number("wait"),
        });
    }
    placed
}

/// Where a map's `target_position` (or any entity) of that name stands and faces
/// (`G_PickTarget` picks among several with the C library's generator; a map with one of
/// each name needs none of that).
pub fn target_of(entities: &[Entity], name: &str) -> Option<([f32; 3], [f32; 3])> {
    entities
        .iter()
        .find(|entity| entity.get("targetname") == Some(name))
        .map(|entity| {
            let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
            let angles = entity.vector("angles").ok().flatten().unwrap_or_else(|| {
                let yaw = entity
                    .get("angle")
                    .and_then(|text| text.trim().parse::<f32>().ok())
                    .unwrap_or(0.0);
                [0.0, yaw, 0.0]
            });
            (origin, angles)
        })
}

/// `SP_trigger_push`: the brush, sent to every client as an `ET_PUSH_TRIGGER`, its
/// LINEAR speed of a thousand unless the map named one, and `AimAtTarget` a frame on.
pub fn spawn_push(placed: &Mover, bounds: ([f32; 3], [f32; 3]), level_time: i32) -> Moving {
    Moving {
        model: placed.model,
        bounds,
        kind: ET_PUSH_TRIGGER,
        spawnflags: placed.spawnflags,
        // `SP_trigger_push`: a LINEAR pusher is a thousand whatever the map wrote — the
        // code overrides the key, though its own comment calls it a default.
        speed: if placed.spawnflags & PUSH_LINEAR != 0 {
            1_000.0
        } else {
            placed.speed
        },
        wait: placed.wait,
        contents: CONTENTS_TRIGGER,
        svflags: 0,
        velocity: [0.0; 3],
        touches: placed.spawnflags & PUSH_START_OFF == 0,
        last_push: 0,
        aim_at: level_time + FRAMETIME,
    }
}

/// `SP_trigger_teleport`: the brush as an `ET_TELEPORT_TRIGGER`, sent to every client
/// unless it is the spectators' own.
pub fn spawn_teleport(placed: &Mover, bounds: ([f32; 3], [f32; 3])) -> Moving {
    Moving {
        model: placed.model,
        bounds,
        kind: ET_TELEPORT_TRIGGER,
        spawnflags: placed.spawnflags,
        speed: placed.speed,
        wait: placed.wait,
        contents: CONTENTS_TRIGGER,
        svflags: if placed.spawnflags & TELEPORT_SPECTATOR != 0 {
            SVF_NOCLIENT
        } else {
            0
        },
        velocity: [0.0; 3],
        touches: true,
        last_push: 0,
        aim_at: 0,
    }
}

/// `AimAtTarget` (`:1044-1118`): the velocity a jump pad throws with, so that the arc's
/// apex is the target — worked out from the trigger's own middle, in the world's
/// gravity. LINEAR points at it instead, RELATIVE keeps the target's place to push
/// towards from wherever the player is. `None` frees the trigger, as the reference does
/// for a target it cannot reach.
pub fn aim_at_target(trigger: &Moving, target: [f32; 3], gravity: f32) -> Option<[f32; 3]> {
    // `r.absmin`/`r.absmax` are the brush's box grown by a unit each way; their middle is
    // the brush's own middle.
    let middle: [f32; 3] =
        std::array::from_fn(|axis| (trigger.bounds.0[axis] + trigger.bounds.1[axis]) * 0.5);
    if trigger.spawnflags & PUSH_RELATIVE != 0 {
        return Some(target);
    }
    if trigger.spawnflags & PUSH_LINEAR != 0 {
        let mut direction: [f32; 3] = std::array::from_fn(|axis| target[axis] - middle[axis]);
        normalize(&mut direction);
        return Some(direction);
    }
    let height = target[2] - middle[2];
    // `sqrt( height / ( .5 * gravity ) )`: `.5` is a double in C, so the division and
    // the root are done in double and only then made a float.
    let time = (f64::from(height) / (0.5 * f64::from(gravity))).sqrt() as f32;
    if time == 0.0 {
        return None;
    }
    let mut throw: [f32; 3] = [target[0] - middle[0], target[1] - middle[1], 0.0];
    let distance = normalize(&mut throw);
    let forward = distance / time;
    Some([throw[0] * forward, throw[1] * forward, time * gravity])
}

/// `VectorNormalize`: the length, and the vector made a unit long (a zero vector is left
/// alone, as the reference leaves it).
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let scale = 1.0 / length;
        for axis in vector.iter_mut() {
            *axis *= scale;
        }
    }
    length
}

/// `hurt_use` (`:1301-1318`): a target toggles the brush in and out of the world. Only a
/// brush spawned with CAN_TARGET has it.
pub fn used(trigger: &mut Hurt) {
    if trigger.spawnflags & CAN_TARGET == 0 {
        return;
    }
    trigger.linked = !trigger.linked;
}

/// `G_TouchTriggers`' own gate (`g_active.c:531-546`) for a trigger a spectator does
/// not touch: a client that is not dead, and not a spectator.
pub fn may_touch(health: i32, spectating: bool) -> bool {
    health > 0 && !spectating
}

/// The same gate for the two kinds a spectator touches too (`g_active.c:567-574`), a
/// teleporter and a door's trigger: any client that is not dead.
pub fn may_touch_as_spectator(health: i32) -> bool {
    health > 0
}

/// The box `G_TouchTriggers` looks for triggers in (`{40, 40, 52}` around the player),
/// and then the player's own box for the contact itself.
pub const RANGE: [f32; 3] = [40.0, 40.0, 52.0];

/// Whether a trigger is near enough to be looked at at all — the area query
/// (`trap->EntitiesInBox` over `SV_LinkEntity`'s boxes, which are a unit wider each way).
pub fn near(player: [f32; 3], trigger: &Hurt) -> bool {
    (0..3).all(|axis| {
        let (low, high) = (trigger.bounds.0[axis] - 1.0, trigger.bounds.1[axis] + 1.0);
        low <= player[axis] + RANGE[axis] && high >= player[axis] - RANGE[axis]
    })
}

/// What `hurt_touch` did to the player it was called for.
#[derive(Clone, Debug, PartialEq)]
pub enum Touched {
    /// It was not its moment, or the player was beyond hurting.
    Nothing,
    /// Health taken, as `G_Damage` takes it.
    Hurt { damage: i32, flags: u32, means: u32 },
    /// The fall to death began: the scream on the player's own voice, its controls gone.
    Fade { sound: &'static str, channel: u32 },
    /// A player already dead in a pit is put back into the game (`ClientRespawn`).
    Respawn,
}

/// `hurt_touch` (`:1320-1425`) for a player standing in the brush: nothing while it is
/// inactive, beyond hurting or before its own next moment; then the rate is set (a
/// second with SLOW, else a frame) and either health is taken — always past every
/// protection, as `MOD_TRIGGER_HURT` — or, for the `dmg -1` brush a pit is made of, the
/// fall to death begins. `take_damage` is the entity's, which a dead player loses.
pub fn hurt_touch(
    trigger: &mut Hurt,
    state: &mut PlayerState,
    health: i32,
    take_damage: bool,
    level_time: i32,
) -> Touched {
    if trigger.inactive || !take_damage || trigger.next_hurt > level_time {
        return Touched::Nothing;
    }
    let falling = state.raw_field(PS_FALLING_TO_DEATH).unwrap_or(0);
    if trigger.damage == -1 {
        // Already fallen and dead: back into the game, wherever it may be put.
        if health < 1 {
            state.set_raw_field(PS_FALLING_TO_DEATH, 0);
            return Touched::Respawn;
        }
        // Already falling: it happens once.
        if falling != 0 {
            return Touched::Nothing;
        }
    }
    trigger.next_hurt = level_time
        + if trigger.spawnflags & SLOW != 0 {
            1_000
        } else {
            FRAMETIME
        };
    if trigger.damage == -1 {
        state.set_raw_field(PS_FALLING_TO_DEATH, level_time as u32);
        let flags = state.raw_field(PS_EFLAGS).unwrap_or(0);
        state.set_raw_field(PS_EFLAGS, flags | EF_RAG);
        // Nobody else is kept waiting by a brush that only starts a fall.
        trigger.next_hurt = 0;
        return Touched::Fade {
            sound: FALLING_SOUND,
            channel: CHAN_VOICE,
        };
    }
    Touched::Hurt {
        damage: trigger.damage,
        flags: DAMAGE_NO_PROTECTION,
        means: MOD_TRIGGER_HURT,
    }
}

/// What a player's move needs of a jump pad it stood in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpPad {
    /// The trigger's entity number, which the player remembers this frame.
    pub number: u16,
    /// `s.origin2`: what it throws with.
    pub velocity: [f32; 3],
}

/// `trigger_push_touch` (`:922-1049`) for an ordinary jump pad: the throw is the
/// movement's to make, because the client makes it too (`BG_TouchJumpPad`). A linear or
/// relative pusher is the game's own and answers [`pushed`] instead.
pub fn is_jump_pad(trigger: &Moving) -> bool {
    trigger.spawnflags & (PUSH_LINEAR | PUSH_RELATIVE) == 0
}

/// `trigger_push_touch`'s linear and relative branches (`:1002-1027`): what the player's
/// velocity becomes, and whether the push is allowed at all this frame — a pusher with a
/// `wait` keeps everyone out until it is over (MULTIPLE lets a whole frame through), and
/// one with a `wait` of -1 pushes once and never again.
pub fn pushed(
    trigger: &mut Moving,
    state: &PlayerState,
    movement_type: u8,
    level_time: i32,
) -> Option<[f32; 3]> {
    if trigger.spawnflags & (PUSH_LINEAR | PUSH_RELATIVE) == 0 || !trigger.touches {
        return None;
    }
    if (level_time as f32) < trigger.last_push as f32 + trigger.wait {
        let multiple = trigger.spawnflags & PUSH_MULTIPLE != 0;
        if !multiple || (trigger.last_push != 0 && level_time > trigger.last_push) {
            return None;
        }
    }
    // `PM_NORMAL`, `PM_DEAD` and `PM_FREEZE` are pushed; nothing else is.
    if !matches!(movement_type, PM_NORMAL | PM_DEAD | PM_FREEZE) {
        return None;
    }
    let velocity = if trigger.spawnflags & PUSH_RELATIVE != 0 {
        let origin = state.origin();
        let mut direction: [f32; 3] =
            std::array::from_fn(|axis| trigger.velocity[axis] - origin[axis]);
        if trigger.speed != 0.0 {
            normalize(&mut direction);
            for axis in direction.iter_mut() {
                *axis *= trigger.speed;
            }
        }
        direction
    } else {
        std::array::from_fn(|axis| trigger.velocity[axis] * trigger.speed)
    };
    if trigger.wait == -1.0 {
        trigger.touches = false;
    } else if trigger.wait > 0.0 {
        trigger.last_push = level_time;
    }
    Some(velocity)
}

/// `PM_NORMAL`, `PM_DEAD`, `PM_FREEZE`, and the two a jump pad also throws.
const PM_NORMAL: u8 = 0;
const PM_DEAD: u8 = 5;
const PM_FREEZE: u8 = 6;
const PM_JETPACK: u8 = 1;
const PM_FLOAT: u8 = 2;

/// `BG_TouchJumpPad` (`bg_misc.c:2543-2569`): the movement's own, which the client makes
/// as well — the player remembers the pad this move, takes its velocity whole, and stops
/// levitating. A spectator (any movement type but the three) is not thrown.
pub fn touch_jump_pad(
    state: &mut PlayerState,
    pad: JumpPad,
    movement_type: u8,
    pmove_framecount: u32,
) -> bool {
    if !matches!(movement_type, PM_NORMAL | PM_JETPACK | PM_FLOAT) {
        return false;
    }
    state.set_raw_field(PS_JUMPPAD_ENT, u32::from(pad.number));
    state.set_velocity(pad.velocity);
    let active = state.force_powers_active();
    state.set_raw_field(PS_FORCE_POWERS_ACTIVE, active & !FP_LEVITATION);
    let _ = pmove_framecount;
    true
}

/// `fd.forcePowersActive` and its levitation bit, which a jump pad clears so that a
/// throw costs no Force.
const PS_FORCE_POWERS_ACTIVE: usize = 82;
const FP_LEVITATION: u32 = 1 << 1;

/// What a teleport did (`TeleportPlayer`, `g_misc.c:197-252`).
#[derive(Clone, Debug, PartialEq)]
pub struct Teleported {
    /// Where the player is put: a unit above the destination (the flashes are made at
    /// the destination itself, which `TeleportPlayer` still has when it makes them).
    pub origin: [f32; 3],
    /// The angles it faces, and the 400 along them it is spat out with.
    pub angles: [f32; 3],
    /// Whether the two flashes are made: a spectator's teleport is silent.
    pub flashes: bool,
    /// Whether anything standing at the destination is killed (`G_KillBox`).
    pub kill_box: bool,
}

/// `trigger_teleporter_touch` (`:1218-1245`): a living player, and only a spectator
/// where the brush says spectators. The destination is the map's to find.
pub fn may_teleport(trigger: &Moving, movement_type: u8, spectating: bool) -> bool {
    if movement_type == PM_DEAD {
        return false;
    }
    trigger.spawnflags & TELEPORT_SPECTATOR == 0 || spectating
}

/// `TeleportPlayer` (`g_misc.c:197-252`): the player is put a unit above the destination
/// facing where it faces, spat out at 400 along those angles and held there 160 ms, its
/// teleport bit flipped so no client lerps the jump — and anything standing where it
/// arrives is killed. Angles past 999999 (the spectator's pass through a door) keep the
/// player's own facing and speed: nothing but the place changes.
pub fn teleport_player(
    state: &mut PlayerState,
    destination: [f32; 3],
    angles: [f32; 3],
    spectating: bool,
) -> Teleported {
    let origin = [destination[0], destination[1], destination[2] + 1.0];
    state.set_origin(origin);
    // Angles past 999999 (`doorangles`) mean none: the player keeps its facing and speed.
    if angles[0] <= 999_999.0 {
        let (sin, cos) = angles[1].to_radians().sin_cos();
        let (pitch_sin, pitch_cos) = (-angles[0]).to_radians().sin_cos();
        let forward = [pitch_cos * cos, pitch_cos * sin, pitch_sin];
        state.set_velocity(forward.map(|axis| axis * 400.0));
        state.set_movement_time(160);
        state.set_movement_flags(state.movement_flags() | PMF_TIME_KNOCKBACK);
    }
    let flags = state.raw_field(PS_EFLAGS).unwrap_or(0);
    state.set_raw_field(PS_EFLAGS, flags ^ EF_TELEPORT_BIT);
    Teleported {
        origin,
        angles,
        flashes: !spectating,
        kill_box: !spectating,
    }
}

/// `PMF_TIME_KNOCKBACK`, and `EF_TELEPORT_BIT`, which tells a client not to lerp.
const PMF_TIME_KNOCKBACK: u16 = 64;
const EF_TELEPORT_BIT: u32 = 1 << 3;

/// `SetClientViewAngle` (`g_client.c:1171-1183`): the player is turned to face `angles`
/// by giving it the delta from the command's own angles, which is what a client adds to
/// them; its view angles and its entity's angles follow.
pub fn face(state: &mut PlayerState, angles: [f32; 3], command_angles: [i32; 3]) {
    for axis in 0..3 {
        let short = (angles[axis] * (65_536.0 / 360.0)) as i32 & 65_535;
        state.set_raw_field(
            PS_DELTA_ANGLES[axis],
            short.wrapping_sub(command_angles[axis]) as u32,
        );
        state.set_raw_field(PS_VIEW_ANGLES[axis], angles[axis].to_bits());
    }
}

/// `playerState_t::delta_angles` and `viewangles` on the wire.
const PS_DELTA_ANGLES: [usize; 3] = [14, 11, 48];
const PS_VIEW_ANGLES: [usize; 3] = [4, 3, 50];

/// `ClientEndFrame`'s end of the fall (`g_active.c:2761-2787`): three seconds after it
/// began, a player still alive is killed outright — by whoever pushed it in, if that is
/// still recent — and its scream is stopped.
pub fn fade_over(state: &PlayerState, health: i32, level_time: i32) -> bool {
    let falling = state.raw_field(PS_FALLING_TO_DEATH).unwrap_or(0) as i32;
    falling != 0 && level_time - FALL_FADE_TIME > falling && health > 0
}

/// `G_MuteSound(self->s.number, CHAN_VOICE)` at the end of the fall: the scream stops,
/// because whoever was screaming is dead (`EV_MUTE_SOUND`, told to everyone).
pub fn scream_muted(client: u16) -> crate::event_entity::EventEntity {
    crate::event_entity::EventEntity {
        event: EV_MUTE_SOUND,
        parameter: 0,
        origin: [0.0; 3],
        client: None,
        broadcast: true,
        extra: [
            (ES_TRICKED, CHAN_VOICE),
            (ES_MUTED, u32::from(client)),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ],
    }
}

/// `EV_MUTE_SOUND`, and the two wire fields it names the player and the channel in.
const EV_MUTE_SOUND: u32 = 74;
const ES_TRICKED: usize = 58;
const ES_MUTED: usize = 74;

/// Whether the player's controls are its own (`Pmove`'s head, `bg_pmove.c:11185-11191`:
/// a player falling to death moves nothing itself).
pub fn controls_taken(state: &PlayerState) -> bool {
    state.raw_field(PS_FALLING_TO_DEATH).unwrap_or(0) != 0
}

/// `FL_INACTIVE` (`g_local.h:78`), which `target_deactivate` sets on whatever it points
/// at and `target_activate` clears.
pub const FL_INACTIVE: u32 = 0x0001_0000;
/// `BUTTON_USE` (`q_shared.h:1360`): the ol' use key.
pub const BUTTON_USE: u16 = 32;
/// `CHAN_AUTO`, which a trigger's own noise is played on.
pub const CHAN_AUTO: u32 = 0;
/// `trigger_multiple`'s spawnflags: CLIENTONLY (no NPC), FACING, USE_BUTTON, FIRE_BUTTON,
/// NPCONLY, INACTIVE, MULTIPLE.
const MULTI_CLIENT_ONLY: u32 = 1;
const MULTI_FACING: u32 = 2;
const MULTI_USE_BUTTON: u32 = 4;
const MULTI_FIRE_BUTTON: u32 = 8;
const MULTI_NPC_ONLY: u32 = 16;
const MULTI_MULTIPLE: u32 = 2_048;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`: FIRE_BUTTON's keys.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;

/// A `trigger_multiple` as the map's dictionary has it, and as `SP_trigger_multiple`
/// leaves it (`g_trigger.c:628-676`): the delay and the `target2` speed are seconds on
/// the wire and milliseconds in the game, and a `random` at least as long as the `wait`
/// is cut back to it less a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Multiple {
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    pub spawnflags: u32,
    /// What it fires, and what it is called if something fires or deactivates it.
    pub target: String,
    pub targetname: String,
    /// `NPC_targetname`: the one NPC (by its `script_targetname`) that may set it off.
    pub npc_targetname: String,
    /// `wait` and `random` in seconds; `delay` in milliseconds.
    pub wait: f32,
    pub random: f32,
    pub delay: i32,
    /// The sound it plays on whoever set it off, if the map named one.
    pub noise: u16,
    pub contents: u32,
    pub svflags: u32,
    /// `nextthink`: when it may fire again, or when a delayed firing is due.
    pub next_fire: i32,
    /// `painDebounceTime`: when it last fired.
    pub last_fire: i32,
    /// `aimDebounceTime`: the frame a player last set it off.
    pub last_frame: i32,
    /// `FL_INACTIVE`, which `target_deactivate` sets.
    pub inactive: bool,
    /// Whether a delayed firing is pending (`think == multi_trigger_run`).
    pub pending: bool,
    /// `alliedTeam` (the `team` key, `atoi`'d): only a player of that team sets it off
    /// (`Touch_Multi`, `g_trigger.c:383-389`); 0 for everyone.
    pub allied_team: i32,
    /// `target2`, fired once the trigger is left ("cleared") `speed` milliseconds after
    /// its last touch (`g_trigger.c:594-595`).
    pub target2: String,
    pub speed: i32,
    /// `think == trigger_cleared_fire`: fired, waiting to be cleared (`next_fire` is its
    /// `nextthink`).
    pub clearing: bool,
    /// `activator`: whoever last set it off, which `target2` is fired for.
    pub activator: Option<usize>,
    /// What siege adds ([`crate::siege_triggers`]).
    pub siege: crate::siege_triggers::SiegeKeys,
}

/// `SP_trigger_multiple` over `InitTrigger`.
pub fn spawn_multiple(
    entity: &Entity,
    bounds: ([f32; 3], [f32; 3]),
    noise: u16,
) -> Option<Multiple> {
    // `SP_trigger_once` (`g_trigger.c:715-748`) is the same brush with one difference:
    // its `wait` is forced to -1, whatever the map said, so it fires once and never
    // again. `Touch_Multi` and `multi_trigger` are shared between the two.
    let once = match entity.get("classname") {
        Some("trigger_multiple") => false,
        Some("trigger_once") => true,
        _ => return None,
    };
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let number = |key: &str| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    let (wait, mut random) = (if once { -1.0 } else { number("wait") }, number("random"));
    // `SP_trigger_multiple`'s own complaint: a random at least as long as the wait is
    // cut back to the wait less a frame.
    if wait > 0.0 && random >= wait {
        random = wait - FRAMETIME as f32 / 1_000.0;
    }
    let spawnflags = number("spawnflags") as u32;
    let target2 = entity.get("target2").unwrap_or_default().to_owned();
    // `SP_trigger_multiple` (`:657-672`): a `target2` without a speed clears after a
    // second; `team` is read as a number (`atoi`).
    let speed = if number("speed") == 0.0 && !target2.is_empty() {
        1_000
    } else {
        (number("speed") * 1_000.0) as i32
    };
    Some(Multiple {
        model,
        bounds,
        spawnflags,
        target: entity.get("target").unwrap_or_default().to_owned(),
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        npc_targetname: entity.get("NPC_targetname").unwrap_or_default().to_owned(),
        wait,
        random,
        delay: (number("delay") * 1_000.0) as i32,
        noise,
        contents: CONTENTS_TRIGGER,
        svflags: SVF_NOCLIENT,
        next_fire: 0,
        last_fire: 0,
        last_frame: 0,
        inactive: spawnflags & INACTIVE != 0,
        pending: false,
        allied_team: entity
            .get("team")
            .filter(|team| !team.is_empty())
            .map_or(0, |team| crate::userinfo::atoi(team.as_bytes())),
        target2,
        speed,
        clearing: false,
        activator: None,
        siege: crate::siege_triggers::keys(entity, once),
    })
}

/// What `Touch_Multi` and `multi_trigger` decided (`:151-372`, `:371-566`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touched2 {
    /// A gate turned it away: the wrong way round, no use key, its wait, its frame, or
    /// `FL_INACTIVE`.
    Nothing,
    /// It fires now.
    Fires,
    /// It will fire when its delay is up.
    Waits,
    /// A USE_BUTTON trigger the player is pressing: it poses first
    /// ([`using_pose`]), and the caller asks again with [`pressed`] for what the trigger
    /// itself does.
    Uses,
}

/// `Touch_Multi`'s USE_BUTTON pose (`:547-559`): the player presses the button —
/// `BOTH_BUTTON_HOLD` held over whatever it was doing — or, already pressing, holds it
/// half a second longer; either way its weapon waits as long as the pose does.
pub fn using_pose(torso_anim: u16) -> Option<u16> {
    (!matches!(torso_anim, BOTH_BUTTON_HOLD | BOTH_CONSOLE1)).then_some(BOTH_BUTTON_HOLD)
}

/// `BOTH_BUTTON_HOLD` and `BOTH_CONSOLE1`, the two poses a used trigger leaves alone.
pub const BOTH_BUTTON_HOLD: u16 = 1_328;
pub const BOTH_CONSOLE1: u16 = 954;
/// How much longer a player already pressing holds it.
pub const USING_AGAIN: i32 = 500;

/// What the trigger itself does once the player is posed: the rest of `multi_trigger`.
pub fn pressed(trigger: &mut Multiple, level_time: i32) -> Touched2 {
    pressed_as(trigger, Activator::Player, level_time)
}

/// `multi_trigger` (`g_trigger.c:151-362`) for `activator`: not while it waits to run after
/// a delay or before its next firing (MULTIPLE lets the rest of this frame's clients
/// through), a player once a frame (`activator->s.number < MAX_CLIENTS`), and a delay
/// waited out before it fires.
pub fn pressed_as(trigger: &mut Multiple, activator: Activator<'_>, level_time: i32) -> Touched2 {
    pressed_as_with(trigger, activator, level_time, &mut NoHooks)
}

/// [`pressed_as`] with a game type's [`TouchHooks`] (siege's gates after the delayed
/// firing's).
pub fn pressed_as_with(
    trigger: &mut Multiple,
    activator: Activator<'_>,
    level_time: i32,
    hooks: &mut dyn TouchHooks,
) -> Touched2 {
    // `Touch_Multi`'s last gate (`:561-565`): waiting to fire its `target2`, it is still
    // being touched.
    if still_clearing(trigger, level_time) {
        return Touched2::Nothing;
    }
    if trigger.pending {
        return Touched2::Nothing;
    }
    if !hooks.gate(trigger) {
        return Touched2::Nothing;
    }
    if trigger.next_fire > level_time {
        let multiple = trigger.spawnflags & MULTI_MULTIPLE != 0;
        if !multiple || (trigger.last_fire != 0 && trigger.last_fire != level_time) {
            return Touched2::Nothing;
        }
    }
    if activator == Activator::Player && trigger.last_frame == level_time {
        return Touched2::Nothing;
    }
    if trigger.delay != 0 && trigger.last_fire < level_time + trigger.delay {
        trigger.pending = true;
        trigger.next_fire = level_time + trigger.delay;
        trigger.last_fire = level_time;
        return Touched2::Waits;
    }
    Touched2::Fires
}

/// The player as `Touch_Multi` reads it.
#[derive(Clone, Copy, Debug)]
pub struct Toucher {
    pub view_angles: [f32; 3],
    pub buttons: u16,
    pub health: i32,
    pub spectating: bool,
    pub weapon_time: i32,
    pub hand_extend: u8,
    /// What its torso is doing, which the use pose reads before it overrides it.
    pub torso_anim: u16,
    /// `sess.sessionTeam`, which a trigger with an `alliedTeam` reads.
    pub team: i32,
}

/// Which client a `Touch_Multi` toucher is: a player, or an NPC going by its
/// `script_targetname`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activator<'a> {
    Player,
    Npc { script_targetname: Option<&'a [u8]> },
}

/// [`touch_multiple_as`] for a player.
pub fn touch_multiple(
    trigger: &mut Multiple,
    who: &Toucher,
    movedir: [f32; 3],
    level_time: i32,
) -> Touched2 {
    touch_multiple_as(trigger, who, Activator::Player, movedir, level_time)
}

/// `Touch_Multi`'s gates and `multi_trigger`'s (`g_trigger.c:371-566`, `:151-362`),
/// without Siege's: a client only, not while inactive, of its `alliedTeam` where it has
/// one; no NPC where CLIENTONLY says so,
/// else only an NPC where NPCONLY does, and only the NPC `NPC_targetname` names where it
/// is set; facing it where FACING says so, holding the use key where USE_BUTTON does, a
/// fire key where FIRE_BUTTON does — and then the trigger's own waits (a player sets it
/// off once a frame, an NPC as often as it may). The caller fires it with
/// [`fire_multiple`] when this answers `Fires`.
pub fn touch_multiple_as(
    trigger: &mut Multiple,
    who: &Toucher,
    activator: Activator<'_>,
    movedir: [f32; 3],
    level_time: i32,
) -> Touched2 {
    touch_multiple_as_with(trigger, who, activator, movedir, level_time, &mut NoHooks)
}

/// What a game type adds to a trigger's touch: siege's hack and its gates
/// ([`crate::siege_triggers`]). The defaults add nothing.
pub trait TouchHooks {
    /// `Touch_Multi`'s `usetime` block, for a USE_BUTTON trigger the player presses:
    /// whether the touch goes on.
    fn hack(&mut self, _trigger: &Multiple) -> bool {
        true
    }
    /// `multi_trigger`'s siege gates, after its delayed-firing check: whether it goes on.
    fn gate(&mut self, _trigger: &mut Multiple) -> bool {
        true
    }
}

/// [`TouchHooks`] that add nothing.
pub struct NoHooks;

impl TouchHooks for NoHooks {}

/// [`touch_multiple_as`] with a game type's [`TouchHooks`].
pub fn touch_multiple_as_with(
    trigger: &mut Multiple,
    who: &Toucher,
    activator: Activator<'_>,
    movedir: [f32; 3],
    level_time: i32,
    hooks: &mut dyn TouchHooks,
) -> Touched2 {
    if trigger.inactive || who.spectating {
        return Touched2::Nothing;
    }
    if trigger.allied_team != 0 && who.team != trigger.allied_team {
        return Touched2::Nothing;
    }
    if trigger.spawnflags & MULTI_CLIENT_ONLY != 0 {
        if matches!(activator, Activator::Npc { .. }) {
            return Touched2::Nothing;
        }
    } else {
        if trigger.spawnflags & MULTI_NPC_ONLY != 0 && activator == Activator::Player {
            return Touched2::Nothing;
        }
        if !trigger.npc_targetname.is_empty() {
            let named = match activator {
                Activator::Npc {
                    script_targetname: Some(name),
                } if !name.is_empty() => {
                    name.eq_ignore_ascii_case(trigger.npc_targetname.as_bytes())
                }
                _ => false,
            };
            if !named {
                return Touched2::Nothing;
            }
        }
    }
    if trigger.spawnflags & MULTI_FACING != 0 {
        let (sin, cos) = who.view_angles[1].to_radians().sin_cos();
        let (pitch_sin, pitch_cos) = (-who.view_angles[0]).to_radians().sin_cos();
        let forward = [pitch_cos * cos, pitch_cos * sin, pitch_sin];
        if (0..3)
            .map(|axis| movedir[axis] * forward[axis])
            .sum::<f32>()
            < 0.5
        {
            return Touched2::Nothing;
        }
    }
    if trigger.spawnflags & MULTI_USE_BUTTON != 0 {
        if who.buttons & BUTTON_USE == 0 {
            return Touched2::Nothing;
        }
        // The player has to be free of everything else to use — but the pose this very
        // trigger puts it in does not count against it.
        let using = matches!(who.torso_anim, BOTH_BUTTON_HOLD | BOTH_CONSOLE1);
        if (who.weapon_time > 0 && !using) || who.health < 1 || who.hand_extend != 0 {
            return Touched2::Nothing;
        }
        if !hooks.hack(trigger) {
            return Touched2::Nothing;
        }
    }
    if trigger.spawnflags & MULTI_FIRE_BUTTON != 0
        && who.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0
    {
        return Touched2::Nothing;
    }
    // `Touch_Multi`'s own pose for a USE_BUTTON trigger (`:547-559`), before anything
    // the trigger itself decides: the player holds the button down, or holds it longer.
    if trigger.spawnflags & MULTI_USE_BUTTON != 0 {
        return Touched2::Uses;
    }
    pressed_as_with(trigger, activator, level_time, hooks)
}

/// `Touch_Multi`'s last gate (`:561-565`): a trigger waiting to fire its `target2` is
/// still being touched, so its clearing is put off by its `speed` again.
fn still_clearing(trigger: &mut Multiple, level_time: i32) -> bool {
    if !trigger.clearing {
        return false;
    }
    trigger.next_fire = level_time + trigger.speed;
    true
}

/// `trigger_cleared_fire` (`:568-576`) once the trigger's `nextthink` is due: nobody
/// touched it for `speed` milliseconds. Its `target2` is to be fired, and its wait
/// starts now. `spread` is `Q_flrand(-1, 1)`, drawn only where there is a wait.
pub fn cleared(trigger: &mut Multiple, level_time: i32, spread: impl FnOnce() -> f32) -> String {
    trigger.clearing = false;
    if trigger.wait > 0.0 {
        trigger.next_fire =
            level_time + ((trigger.wait + trigger.random * spread()) * 1_000.0) as i32;
    }
    trigger.target2.clone()
}

/// What a firing does (`multi_trigger_run`, `:53-113`).
#[derive(Clone, Debug, PartialEq)]
pub struct Fired {
    /// The targetname to fire, which the caller looks up in the map.
    pub target: String,
    /// The trigger's own sound, played on whoever set it off.
    pub sound: Option<u16>,
    /// Whether it is out of the world for good (a `wait` below zero).
    pub gone: bool,
    /// The target of the side that just took a siege `teambalance` zone, fired first.
    pub taken: Option<String>,
}

/// Whether [`fire_multiple`] reads its `spread` (`Q_flrand(-1, 1)`): only a trigger with a
/// `wait`, not waiting to be cleared, and first to fire this frame draws it — a caller
/// draws from the game's generator only then, as `multi_trigger_run` does.
pub fn draws_spread(trigger: &Multiple, level_time: i32) -> bool {
    let clears = !trigger.target2.is_empty() && trigger.wait >= 0.0;
    !clears && trigger.wait > 0.0 && trigger.last_fire != level_time
}

/// `multi_trigger_run`: its targets are fired, its noise plays on whoever set it off,
/// and then it waits — the map's `wait` wandered by its `random`, or never again where
/// the `wait` is below zero. `spread` is `Q_flrand(-1, 1)`.
pub fn fire_multiple(
    trigger: &mut Multiple,
    level_time: i32,
    spread: f32,
    by_a_player: bool,
) -> Fired {
    trigger.pending = false;
    let taken = crate::siege_triggers::taken_target(&mut trigger.siege);
    let mut gone = false;
    if !trigger.target2.is_empty() && trigger.wait >= 0.0 {
        // `trigger_cleared_fire` once it is left.
        trigger.clearing = true;
        trigger.next_fire = level_time + trigger.speed;
    } else if trigger.wait > 0.0 {
        // The first entity to set it off this frame decides when it may fire again.
        if trigger.last_fire != level_time {
            trigger.next_fire =
                level_time + ((trigger.wait + trigger.random * spread) * 1_000.0) as i32;
            trigger.last_fire = level_time;
        }
    } else if trigger.wait < 0.0 {
        // It is not even a trigger any more: nothing can touch it again.
        trigger.contents &= !CONTENTS_TRIGGER;
        gone = true;
    }
    if by_a_player {
        trigger.last_frame = level_time;
    }
    Fired {
        target: trigger.target.clone(),
        sound: (trigger.noise != 0).then_some(trigger.noise),
        gone,
        taken,
    }
}

/// The targets a map places, as the ones this step fires behave.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// `target_print`: the message, and who is told — the activator alone (spawnflag 4),
    /// a team (1 or 2), or everyone.
    Print {
        message: String,
        to_activator: bool,
        team: Option<u8>,
    },
    /// `target_speaker`: the sound, and how it is played (a looping toggle at 3, on the
    /// activator at 8, everywhere at 4, else on itself).
    Speaker { noise: u16, spawnflags: u32 },
    /// `target_relay`: fires its own targets, or one of them at random (spawnflag 4).
    Relay { target: String, spawnflags: u32 },
    /// `target_delay`: fires its targets `wait` seconds on, wandered by `random`.
    Delay {
        target: String,
        wait: f32,
        random: f32,
        spawnflags: u32,
    },
    /// `target_activate` and `target_deactivate`: clear or set `FL_INACTIVE` on
    /// everything of that name.
    SetActive { target: String, active: bool },
}

/// Every target of a map's entity lump, by the name things fire it with. `sound` names
/// each `noise` as `G_SoundIndex` would.
pub fn map_targets(
    entities: &[Entity],
    sound: &mut dyn FnMut(&str) -> u16,
) -> Vec<(String, Target)> {
    let mut targets = Vec::new();
    for entity in entities {
        let Some(classname) = entity.get("classname") else {
            continue;
        };
        let name = entity.get("targetname").unwrap_or_default().to_owned();
        let target = entity.get("target").unwrap_or_default().to_owned();
        let number = |key: &str| {
            entity
                .get(key)
                .and_then(|text| text.trim().parse::<f32>().ok())
                .unwrap_or(0.0)
        };
        let spawnflags = number("spawnflags") as u32;
        let target = match classname {
            "target_print" => Target::Print {
                message: entity.get("message").unwrap_or_default().to_owned(),
                to_activator: spawnflags & 4 != 0,
                team: match spawnflags & 3 {
                    1 => Some(1),
                    2 => Some(2),
                    _ => None,
                },
            },
            "target_speaker" => Target::Speaker {
                noise: entity.get("noise").map_or(0, |noise| sound(noise)),
                spawnflags,
            },
            "target_relay" => Target::Relay { target, spawnflags },
            // `SP_target_delay`: `delay` for the old maps, else `wait`, and never zero.
            "target_delay" => {
                let wait = entity
                    .get("delay")
                    .or(entity.get("wait"))
                    .and_then(|text| text.trim().parse::<f32>().ok())
                    .unwrap_or(1.0);
                Target::Delay {
                    target,
                    wait: if wait == 0.0 { 1.0 } else { wait },
                    random: number("random"),
                    spawnflags,
                }
            }
            "target_activate" => Target::SetActive {
                target,
                active: true,
            },
            "target_deactivate" => Target::SetActive {
                target,
                active: false,
            },
            _ => continue,
        };
        targets.push((name, target));
    }
    targets
}

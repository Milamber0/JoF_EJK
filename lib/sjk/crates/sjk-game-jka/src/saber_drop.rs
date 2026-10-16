//! A saber out of its owner's hand (OpenJK `codemp/game/w_saber.c`):
//! - knocked down (`saberKnockDown`, `:6504-6597`): turned off and falling, as a
//!   bouncing missile, towards where the one who knocked it looks;
//! - knocked out of the hand (`saberKnockOutOfHand`, `:6625-6691`): the same, thrown off
//!   the hand with the blades' momentum;
//! - lying and called back (`DownedSaberThink`, `saberReactivate`, `:6334-6502`): the
//!   owner's attack button three seconds on, or twenty seconds more of lying there,
//!   brings it back (`saberBackToOwner`); an owner dead or without the saber skill has it
//!   back in hand at once;
//! - the disarms that send it there: a lost duel's lock and a broken parry
//!   (`saberCheckKnockdown_DuelLoss`, `_BrokenParry`, `:6693-6860`), a thrown saber
//!   smashed or knocked out of the air (`_Smashed`, `_Thrown`, `:6862-6925`).
//!
//! The fall is `G_RunMissile` for a knocked saber (`g_missile.c:815-1027`: forced under
//! gravity, bouncing at 0.65 with `EV_GRENADE_BOUNCE` and a bounce sound, never
//! exploding), the missile code's own ([`crate::weapon_fire::run_missile_against`]); the
//! lying is `G_RunObject` (`g_object.c:94-260`) for a saber.
//!
//! An owner who dies or leaves with its saber out of hand leaves a falling copy of it
//! (`MakeDeadSaber`, `:6216-6329`), a bouncing missile with a think of its own
//! ([`crate::weapon_fire::DeadSaber`]) gone four seconds on.

use crate::saber_clash::SaberStorage;
use crate::saber_throw::{CONTENTS_LIGHTSABER, Flown, Saber, SaberFlight, SaberThink, es, ps};
use crate::weapon_fire::Missile;

/// `SFL_NOT_DISARMABLE`: a first saber that is never knocked out of the hand.
const SFL_NOT_DISARMABLE: u32 = 1 << 2;

/// `SABER_RETRIEVE_DELAY`, `MAX_LEAVE_TIME`.
const RETRIEVE_DELAY: i32 = 3_000;
const MAX_LEAVE_TIME: i32 = 20_000;
/// `CONTENTS_TRIGGER`, `MASK_SOLID`, `MASK_PLAYERSOLID`.
const CONTENTS_TRIGGER: u32 = 0x400;
const MASK_SOLID: u32 = 0x1 | 0x1000;
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
/// A knocked saber's box, and a saber knocked out of the hand's before that.
const DOWNED_BOX: [f32; 3] = [3.0, 3.0, 1.5];
const OUT_OF_HAND_BOX: [f32; 3] = [24.0, 24.0, 8.0];
/// `FL_BOUNCE_HALF`.
pub(crate) const FL_BOUNCE_HALF: u32 = 0x20_0000;
/// `ET_GENERAL`, `ET_MISSILE`, `TR_STATIONARY`, `TR_LINEAR`, `TR_GRAVITY`.
const ET_GENERAL: u32 = 0;
pub(crate) const ET_MISSILE: u32 = 3;
const TR_STATIONARY: u32 = 0;
const TR_LINEAR: u32 = 2;
const TR_GRAVITY: u32 = 6;
/// `CHAN_BODY`, `BUTTON_ATTACK`, `WP_SABER`, `FRAMETIME`.
const CHAN_BODY: u32 = 6;
const BUTTON_ATTACK: u16 = 1;
const WP_SABER: u32 = 3;
const FRAME_TIME: i32 = 100;
/// `LS_V1_BL`, `BLOCKED_BOUNCE_MOVE`: a lost lock's loser's move.
const LS_V1_BL: u32 = crate::saber_move_data::movement::LS_V1_BL as u32;
const BLOCKED_BOUNCE_MOVE: u32 = 1;
/// The owner's `saberMove`, `saberBlocked`.
const PS_SABER_MOVE: usize = 34;
const PS_SABER_BLOCKED: usize = 77;
/// The stock saber's sounds (`WP_SaberSetDefaults`) and the pull.
const PULL_SOUND: &[u8] = b"sound/weapons/force/pull.wav";

/// `G_SetOrigin`: stood at `origin`, its trajectory stationary there.
fn set_origin(saber: &mut Saber, origin: [f32; 3]) {
    saber.set_vector(es::POS_BASE, origin);
    saber.set(es::POS_TYPE, TR_STATIONARY);
    saber.set(es::POS_TIME, 0);
    saber.set(es::POS_DURATION, 0);
    saber.set_vector(es::POS_DELTA, [0.0; 3]);
    saber.entity.current = origin;
}

/// `saberKnockDown(saberent, owner, other)`: the saber turned off and falling where it
/// is (popped up, or put on its owner, where that is in something solid), spinning at
/// random, a bouncing missile from 50 ms ago; knocked by somebody else, it flies off at
/// 200 along where that one looks. The owner cannot call it back for three seconds.
pub fn knock_down(saber: &mut Saber, world: &mut dyn SaberFlight, other: u16, level_time: i32) {
    let model = world.owner_saber().model;
    let Some(owner) = world.owner() else { return };
    let (owner_number, owner_origin) = (owner.number, owner.state.origin());
    owner.state.set_raw_field(ps::SABER_ENTITY, 0);
    owner.memory.knocked_time = level_time + RETRIEVE_DELAY;
    saber.entity.clip_mask = MASK_SOLID;
    saber.entity.contents = CONTENTS_TRIGGER;
    saber.entity.mins = DOWNED_BOX.map(|axis| -axis);
    saber.entity.maxs = DOWNED_BOX;
    let (mins, maxs, number) = (saber.entity.mins, saber.entity.maxs, saber.number);
    let stuck = |world: &mut dyn SaberFlight, at: [f32; 3]| {
        let trace = world.trace(at, mins, maxs, at, number, MASK_SOLID);
        trace.start_solid || trace.fraction != 1.0
    };
    if stuck(world, saber.entity.current) {
        let mut raised = saber.entity.current;
        raised[2] += 20.0;
        set_origin(saber, raised);
        if stuck(world, raised) {
            set_origin(saber, owner_origin);
        }
    }
    saber.set(es::APOS_TYPE, TR_GRAVITY);
    let spin: [f32; 3] = std::array::from_fn(|_| world.rng().irand(200, 800) as f32);
    saber.set_vector(es::APOS_DELTA, spin);
    saber.set(es::APOS_TIME, (level_time - 50) as u32);
    saber.set(es::POS_TYPE, TR_GRAVITY);
    saber.set(es::POS_TIME, (level_time - 50) as u32);
    saber.entity.flags |= FL_BOUNCE_HALF;
    saber.set(es::MODEL, u32::from(model));
    saber.set(es::MODEL_GHOUL2, 1);
    saber.set(es::G2_RADIUS, 20);
    saber.set(es::TYPE, ET_MISSILE);
    saber.set(es::WEAPON, WP_SABER);
    saber.entity.speed = (level_time + 4_000) as f32;
    saber.entity.bounce_count = -5;
    let _ = crate::saber_throw::move_back(saber, world, true, level_time);
    saber.set(es::POS_TYPE, TR_GRAVITY);
    saber.set(es::LOOP_SOUND, 0);
    saber.set(es::LOOP_IS_SOUNDSET, 0);
    saber.entity.shown = true;
    saber.entity.think = SaberThink::Downed;
    saber.entity.next_think = level_time;
    if other != owner_number
        && let Some(view) = world.view_angles(other)
    {
        let (forward, _) = crate::pmove::flight::flight_axes(view);
        saber.set_vector(es::POS_DELTA, forward.to_array().map(|axis| axis * 200.0));
    }
    let off = world.owner_saber().off;
    if off != 0 {
        world.raise(crate::weapon_fire::sound_event(
            saber.entity.current,
            CHAN_BODY,
            off,
        ));
    }
}

/// `saberKnockOutOfHand`: a saber in hand, its blade read in the last 50 ms and no lock
/// lately, sent flying off the hand at `velocity` — knocked down with nobody's look to
/// follow. Returns whether it was.
pub fn knock_out_of_hand(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    velocity: [f32; 3],
    level_time: i32,
) -> bool {
    // `SFL_NOT_DISARMABLE` on the first saber (`w_saber.c:6648`), after the other checks —
    // none of which changes anything.
    let not_disarmable = world
        .owner_first_saber()
        .is_some_and(|first| first.flags & SFL_NOT_DISARMABLE != 0);
    let model = world.owner_saber().model;
    let Some(owner) = world.owner() else {
        return false;
    };
    let lock_time = owner.state.raw_field(ps::LOCK_TIME).unwrap_or(0) as i32;
    if owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0) == 0
        || level_time - owner.storage.last_time > 50
        || lock_time > level_time - 100
        || not_disarmable
    {
        return false;
    }
    let (owner_number, hand) = (owner.number, owner.storage.last_base);
    owner.state.set_raw_field(ps::IN_FLIGHT, 1);
    owner.memory.started = true;
    saber.set(es::IN_FLIGHT, 0);
    saber.set(es::POS_TYPE, TR_LINEAR);
    saber.set(es::TYPE, ET_GENERAL);
    saber.set(es::FLAGS, 0);
    saber.set(es::MODEL, u32::from(model));
    saber.set(es::MODEL_GHOUL2, 127);
    saber.set(es::SOLID, 2);
    saber.entity.contents = CONTENTS_LIGHTSABER;
    saber.entity.value5 = 0;
    saber.entity.mins = OUT_OF_HAND_BOX.map(|axis| -axis);
    saber.entity.maxs = OUT_OF_HAND_BOX;
    saber.set(es::GENERIC_ENEMY, u32::from(owner_number) + 1_024);
    saber.set(es::WEAPON, WP_SABER);
    set_origin(saber, hand);
    knock_down(saber, world, owner_number, level_time);
    saber.set_vector(es::POS_DELTA, velocity);
    true
}

/// `G_RunMissile` for a knocked saber, then its think: under gravity again, on to where
/// the trajectory has it, bouncing off what it strikes; then `DownedSaberThink`.
pub(crate) fn run_downed(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
    previous_time: i32,
) -> Flown {
    saber.set(es::POS_TYPE, TR_GRAVITY);
    let owner = world.owner().map_or(saber.number, |owner| owner.number);
    let placeholder = sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
    let mut missile = Missile {
        state: std::mem::replace(saber.state, placeholder),
        current: saber.entity.current,
        bounds: (saber.entity.mins, saber.entity.maxs),
        owner,
        clip_mask: saber.entity.clip_mask,
        damage: 0,
        method_of_death: 0,
        free_at: 0,
        impact_velocity: [0.0; 3],
        impact_point: [0.0; 3],
        linked: true,
        bounces: false,
        bounce_count: saber.entity.bounce_count,
        event_time: saber.entity.event_time,
        damage_flags: 0,
        splash_damage: 0,
        splash_radius: 0.0,
        splash_method_of_death: 0,
        contents: saber.entity.contents,
        homing: None,
        bounce_half: saber.entity.flags & FL_BOUNCE_HALF != 0,
        bounce_shrapnel: false,
        blows: false,
        dead_saber: None,
        thermal: None,
        broadcast: false,
        explodes: false,
        parent: None,
        pass_through: None,
        activator: None,
    };
    // Whatever the run — a bounce, or the sky struck — a knocked saber thinks after it
    // (`G_RunThink`).
    world.run_missile(&mut missile, level_time, previous_time);
    *saber.state = std::mem::replace(
        &mut missile.state,
        sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
    );
    saber.entity.current = missile.current;
    saber.entity.bounce_count = missile.bounce_count;
    saber.entity.event_time = missile.event_time;
    run_think_downed(saber, world, level_time, previous_time)
}

/// `G_RunThink` for a knocked saber: `DownedSaberThink` while its time has come, else
/// (called back) the flight home.
fn run_think_downed(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
    previous_time: i32,
) -> Flown {
    let at = saber.entity.next_think;
    if at <= 0 || at > level_time {
        return Flown::Nothing;
    }
    saber.entity.next_think = 0;
    match saber.entity.think {
        SaberThink::Downed => downed_think(saber, world, level_time, previous_time),
        // Called back in the frame it fell: home from the next.
        _ => Flown::Nothing,
    }
}

/// `DownedSaberThink`: an owner gone leaves the saber for good; one that has it named
/// again, is dead or has no saber skill gets it back in hand; the attack button after
/// the retrieve delay, or twenty seconds more, calls it home; otherwise it lies on
/// (`G_RunObject`).
fn downed_think(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
    previous_time: i32,
) -> Flown {
    saber.entity.next_think = level_time;
    let gone = world.owner().is_none_or(|owner| {
        let named = owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0);
        owner.spectator || (named != 0 && named != u32::from(saber.number))
    });
    if gone {
        make_dead_saber(saber, world, level_time);
        return Flown::Freed;
    }
    let Some(owner) = world.owner() else {
        return Flown::Freed;
    };
    let named = owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0);
    let (health, offense) = (owner.health, owner.offense);
    if named != 0 || health < 1 || offense == 0 {
        owner
            .state
            .set_raw_field(ps::SABER_ENTITY, u32::from(saber.number));
        reactivate(saber, world);
        if health < 1 {
            if let Some(owner) = world.owner() {
                owner.state.set_raw_field(ps::IN_FLIGHT, 0);
            }
            make_dead_saber(saber, world, level_time);
        }
        saber.entity.think = SaberThink::InHand;
        saber.entity.value5 = 0;
        saber.entity.next_think = level_time;
        saber.entity.shown = false;
        saber.set(es::LOOP_SOUND, 0);
        saber.set(es::LOOP_IS_SOUNDSET, 0);
        // Dead or alive, the owner holds no saber in flight any more.
        let Some(owner) = world.owner() else {
            return Flown::Freed;
        };
        crate::saber_throw::back_in_hand(owner, level_time, 500);
        return Flown::Nothing;
    }
    let called = (owner.memory.knocked_time < level_time
        && owner.command_buttons & BUTTON_ATTACK != 0)
        || level_time - owner.memory.knocked_time > MAX_LEAVE_TIME;
    if called {
        let owner_origin = owner.state.origin();
        owner
            .state
            .set_raw_field(ps::SABER_ENTITY, u32::from(saber.number));
        reactivate(saber, world);
        saber.entity.think = SaberThink::Back;
        saber.entity.speed = 0.0;
        saber.entity.value5 = 0;
        saber.entity.next_think = level_time;
        saber.entity.contents = CONTENTS_LIGHTSABER;
        let (pull, on) = (world.sound_index(PULL_SOUND), world.owner_saber().on);
        world.raise(crate::weapon_fire::sound_event(
            owner_origin,
            CHAN_BODY,
            pull,
        ));
        if on[0] != 0 {
            world.raise(crate::weapon_fire::sound_event(
                saber.entity.current,
                CHAN_BODY,
                on[0],
            ));
        }
        // The second hand's on-sound, which an empty hand has by default too.
        if on[1] != 0 {
            world.raise(crate::weapon_fire::sound_event(
                owner_origin,
                CHAN_BODY,
                on[1],
            ));
        }
        return Flown::Nothing;
    }
    run_object(saber, world, level_time, previous_time);
    saber.entity.next_think = level_time;
    Flown::Nothing
}

/// `MakeDeadSaber`: a falling copy of the saber where it is — popped up out of anything
/// solid — spinning at random, under gravity from 50 ms ago with the saber's own
/// velocity, bouncing a dozen times at most and gone four seconds on; nothing in the
/// Jedi Master game, whose one saber is a world object.
pub fn make_dead_saber(saber: &Saber, world: &mut dyn SaberFlight, level_time: i32) {
    const GT_JEDIMASTER: i32 = 2;
    const DEAD_LIFETIME: i32 = 4_000;
    let gametype = world.owner().map_or(0, |owner| owner.gametype);
    if gametype == GT_JEDIMASTER {
        return;
    }
    let bounds = (DOWNED_BOX.map(|axis| -axis), DOWNED_BOX);
    let mut start = saber.entity.current;
    let trace = world.trace(
        start,
        bounds.0,
        bounds.1,
        start,
        sjk_protocol::ENTITY_NUMBER_NONE,
        MASK_PLAYERSOLID,
    );
    if trace.start_solid || trace.fraction != 1.0 {
        // Popped up; where that is solid too, the owner's place is tried and then written
        // over, which leaves the popped place.
        start[2] += 20.0;
    }
    let angles = saber.entity.current_angles;
    let spin: [f32; 3] = std::array::from_fn(|_| world.rng().irand(200, 800) as f32);
    // Without its owner in the game the copy is freed at once (no model to give it).
    if world.owner().is_none() {
        return;
    }
    let model = world.owner_saber().model;
    let mut state = sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS);
    let delta = saber.vector(es::POS_DELTA);
    for (indices, value) in [
        (es::POS_BASE, start),
        (es::APOS_BASE, angles),
        (es::ORIGIN, start),
        (es::ANGLES, angles),
        (es::APOS_DELTA, spin),
        (es::POS_DELTA, delta),
    ] {
        for (index, value) in indices.into_iter().zip(value) {
            state.set_raw_field(index, value.to_bits());
        }
    }
    let fields = [
        (es::APOS_TYPE, TR_GRAVITY),
        (es::APOS_TIME, (level_time - 50) as u32),
        (es::POS_TIME, (level_time - 50) as u32),
        (es::MODEL, u32::from(model)),
        (es::MODEL_GHOUL2, 1),
        (es::G2_RADIUS, 20),
        (es::TYPE, ET_MISSILE),
        (es::WEAPON, WP_SABER),
        // `saberMoveBack(qtrue)` leaves it linear; then under gravity.
        (es::POS_TYPE, TR_GRAVITY),
    ];
    for (index, value) in fields {
        state.set_raw_field(index, value);
    }
    // `saberMoveBack(qtrue)`: on from 50 ms ago in a straight line, the spin with it.
    let at = |base: [f32; 3], delta: [f32; 3], kind: u32| {
        crate::trajectory::legacy_evaluate_trajectory(
            base,
            delta,
            kind as u8,
            level_time - 50,
            0,
            level_time,
        )
    };
    let current = at(start, delta, TR_LINEAR);
    let current_angles = at(angles, spin, TR_GRAVITY);
    let missile = Missile {
        state,
        current,
        bounds,
        owner: saber.number,
        clip_mask: MASK_PLAYERSOLID,
        damage: 0,
        method_of_death: 0,
        free_at: 0,
        impact_velocity: [0.0; 3],
        impact_point: [0.0; 3],
        linked: true,
        bounces: false,
        bounce_count: 12,
        event_time: 0,
        damage_flags: 0,
        splash_damage: 0,
        splash_radius: 0.0,
        splash_method_of_death: 0,
        contents: CONTENTS_TRIGGER,
        homing: None,
        bounce_half: true,
        bounce_shrapnel: false,
        blows: false,
        dead_saber: Some(crate::weapon_fire::DeadSaber {
            until: level_time + DEAD_LIFETIME,
            next_think: level_time,
            angles: current_angles,
            freeing: false,
        }),
        thermal: None,
        broadcast: false,
        explodes: false,
        parent: None,
        pass_through: None,
        activator: None,
    };
    world.spawn_dead_saber(missile);
}

/// `saberReactivate`: flying again, spinning, its box the blade's (`SetSaberBoxSize`),
/// a thrown saber's touch; the owner's throw under way.
fn reactivate(saber: &mut Saber, world: &mut dyn SaberFlight) {
    saber.set(es::IN_FLIGHT, 1);
    saber.spin();
    saber.set(es::POS_TYPE, TR_LINEAR);
    saber.set(es::TYPE, ET_GENERAL);
    saber.set(es::FLAGS, 0);
    saber.entity.value5 = 0;
    (saber.entity.mins, saber.entity.maxs) = world.saber_box(saber.entity.current);
    saber.set(es::WEAPON, WP_SABER);
    if let Some(owner) = world.owner() {
        owner.memory.started = true;
    }
}

/// `G_RunObject` for a knocked saber: at rest it falls again from the last frame's time;
/// on to where the trajectory has it (the owner passed); on a floor it stops, laid along
/// the slope, and its touch (`SaberBounceSound`) stands its angles upright.
pub(crate) fn run_object(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
    previous_time: i32,
) {
    if saber.get(es::POS_TYPE) == TR_STATIONARY {
        saber.set(es::POS_TYPE, TR_GRAVITY);
        let base = saber.entity.current;
        saber.set_vector(es::POS_BASE, base);
        saber.set(es::POS_TIME, previous_time as u32);
    }
    saber.entity.next_think = level_time + FRAME_TIME;
    let origin = saber.position_at(level_time);
    saber.entity.current_angles = saber.angles_at(level_time);
    if saber.entity.current == origin {
        return;
    }
    let owner = world.owner().map_or(saber.number, |owner| owner.number);
    let mut trace = world.trace(
        saber.entity.current,
        saber.entity.mins,
        saber.entity.maxs,
        origin,
        owner,
        saber.entity.clip_mask,
    );
    if !trace.start_solid && !trace.all_solid && trace.fraction != 0.0 {
        saber.entity.current = trace.end_position;
    } else {
        trace.fraction = 0.0;
    }
    if trace.fraction == 1.0 {
        return;
    }
    if saber.get(es::POS_TYPE) == TR_GRAVITY {
        if trace.plane_normal[2] < 0.7 {
            if saber.entity.flags & FL_BOUNCE_HALF != 0 {
                if trace.fraction <= 0.0 {
                    saber.entity.current = trace.end_position;
                    saber.set_vector(es::POS_BASE, trace.end_position);
                    saber.set_vector(es::POS_DELTA, [0.0; 3]);
                    saber.set(es::POS_TIME, level_time as u32);
                } else {
                    bounce_object(saber, &trace, level_time, previous_time);
                }
            }
        } else {
            saber.set(es::APOS_TYPE, TR_STATIONARY);
            crate::weapon_fire::pitch_roll_for_slope(
                &mut saber.entity.current_angles,
                trace.plane_normal,
            );
            let angles = saber.entity.current_angles;
            saber.set_vector(es::APOS_BASE, angles);
            // `G_StopObjectMoving`.
            saber.set(es::POS_TYPE, TR_STATIONARY);
            let current = saber.entity.current;
            saber.set_vector(es::ORIGIN, current);
            saber.set_vector(es::POS_BASE, current);
            saber.set_vector(es::POS_DELTA, [0.0; 3]);
        }
    }
    bounce_sound(saber);
}

/// `SaberBounceSound`, a knocked saber's touch: the angles it has, stood upright.
pub fn bounce_sound(saber: &mut Saber) {
    let mut angles = saber.entity.current_angles;
    angles[0] = 90.0;
    saber.set_vector(es::APOS_BASE, angles);
}

/// `G_TouchTriggers`' contact (`trap->EntityContact`) of a player's box — its `r.mins`,
/// `r.maxs` at `origin` — with a knocked saber lying or falling, which is a trigger to it.
pub fn touched_by(
    saber: &crate::saber_throw::SaberEntity,
    origin: [f32; 3],
    bounds: ([f32; 3], [f32; 3]),
) -> bool {
    saber.shown
        && saber.contents & CONTENTS_TRIGGER != 0
        && (0..3).all(|axis| {
            origin[axis] + bounds.0[axis] < saber.current[axis] + saber.maxs[axis]
                && origin[axis] + bounds.1[axis] > saber.current[axis] + saber.mins[axis]
        })
}

/// `G_BounceObject` with `FL_BOUNCE_HALF`: the velocity at the hit reflected and halved;
/// on a floor under 40 a second up it stops there, its angles kept.
fn bounce_object(
    saber: &mut Saber,
    trace: &crate::pmove::MovementTrace,
    level_time: i32,
    previous_time: i32,
) {
    let hit_time = previous_time + ((level_time - previous_time) as f32 * trace.fraction) as i32;
    let delta = saber.vector(es::POS_DELTA);
    let velocity = crate::trajectory::legacy_evaluate_trajectory_delta(
        delta,
        saber.get(es::POS_TYPE) as u8,
        saber.get(es::POS_TIME) as i32,
        saber.get(es::POS_DURATION) as i32,
        hit_time,
    );
    let normal = trace.plane_normal;
    let dot = velocity[0] * normal[0] + velocity[1] * normal[1] + velocity[2] * normal[2];
    let reflected: [f32; 3] =
        std::array::from_fn(|axis| (velocity[axis] + -2.0 * dot * normal[axis]) * 0.5);
    saber.set_vector(es::POS_DELTA, reflected);
    if normal[2] > 0.7 && reflected[2] < 40.0 {
        saber.set(es::APOS_TYPE, TR_STATIONARY);
        let angles = saber.entity.current_angles;
        saber.set_vector(es::APOS_BASE, angles);
        saber.entity.current = trace.end_position;
        saber.set_vector(es::POS_BASE, trace.end_position);
        saber.set(es::POS_TIME, level_time as u32);
        return;
    }
    saber.entity.current = trace.end_position;
    saber.set(es::POS_TIME, hit_time as u32);
    let current = saber.entity.current;
    saber.set_vector(es::POS_BASE, current);
}

/// The velocity a disarm throws a saber off with: the other's swing (its blade's last
/// two readings), else the owner's own, else — for a lost lock only — from the other's
/// blade to the owner's; at least 20 long, scaled by 6.5. `None` where the readings
/// give no way to throw it (`BrokenParry` gives up there; a lost lock drops it).
pub fn disarm_velocity(
    owner: &SaberStorage,
    other: &SaberStorage,
    between_blades: bool,
    level_time: i32,
) -> Option<[f32; 3]> {
    let fresh =
        |storage: &SaberStorage| storage.older_valid && level_time - storage.last_time < 200;
    if !fresh(other) {
        return None;
    }
    let (mut throw, mut distance) = direction(sub(other.last_base, other.older_base));
    if distance == 0.0 {
        if !fresh(owner) {
            return None;
        }
        (throw, distance) = direction(sub(owner.last_base, owner.older_base));
    }
    if distance == 0.0 && between_blades {
        (throw, distance) = direction(sub(owner.last_base, other.last_base));
    }
    if distance == 0.0 {
        return None;
    }
    let scale = distance.max(20.0) * 6.5;
    Some(throw.map(|axis| axis * scale))
}

/// `saberCheckKnockdown_DuelLoss`: the loser of a lock, its saber still named and no
/// lock lately, bounced (`LS_V1_BL`), and its saber knocked out of its hand along the
/// blades' momentum (or dropped) on a draw of one in two. Returns whether it was.
pub fn duel_loss(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    other: &SaberStorage,
    disarm_chance: i32,
    level_time: i32,
) -> bool {
    let Some(owner) = world.owner() else {
        return false;
    };
    let lock_time = owner.state.raw_field(ps::LOCK_TIME).unwrap_or(0) as i32;
    if owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0) == 0 || lock_time > level_time - 100 {
        return false;
    }
    let velocity = disarm_velocity(&owner.storage, other, true, level_time).unwrap_or([0.0; 3]);
    owner.state.set_raw_field(PS_SABER_MOVE, LS_V1_BL);
    owner
        .state
        .set_raw_field(PS_SABER_BLOCKED, BLOCKED_BOUNCE_MOVE);
    // `disarmChance`: the winner's sabers' (`PlayerSabers::disarm_chance`).
    if world.rng().irand(0, disarm_chance) != 0 {
        return knock_out_of_hand(saber, world, velocity, level_time);
    }
    false
}

/// `saberCheckKnockdown_Smashed`: a thrown saber struck by a blade in a defence move, or
/// by more than 10 damage, is knocked down by the striker. Returns whether it was.
pub fn smashed(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    striker: u16,
    striker_defending: bool,
    damage: i32,
    level_time: i32,
) -> bool {
    let Some(owner) = world.owner() else {
        return false;
    };
    let lock_time = owner.state.raw_field(ps::LOCK_TIME).unwrap_or(0) as i32;
    let valid =
        owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0) != 0 && lock_time <= level_time - 100;
    if !valid || owner.state.raw_field(ps::IN_FLIGHT).unwrap_or(0) == 0 {
        return false;
    }
    if striker_defending || damage > 10 {
        knock_down(saber, world, striker, level_time);
        return true;
    }
    false
}

/// `saberCheckKnockdown_Thrown` after a block by `defender`: a defence above the throw's
/// level knocks it down, an equal one four times in ten. Returns whether it was.
pub(crate) fn thrown(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    defender: u16,
    throw_level: u8,
    level_time: i32,
) -> bool {
    let Some(defence) = world.defence(defender) else {
        return false;
    };
    let toss = defence > throw_level || (defence == throw_level && world.rng().irand(1, 10) <= 4);
    if toss {
        knock_down(saber, world, defender, level_time);
    }
    toss
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

/// `VectorNormalize` of a copy: the direction and the length.
fn direction(mut vector: [f32; 3]) -> ([f32; 3], f32) {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        vector = vector.map(|axis| axis * inverse);
    }
    (vector, length)
}

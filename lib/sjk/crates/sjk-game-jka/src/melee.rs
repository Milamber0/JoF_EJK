//! The fists and the stun baton (OpenJK `codemp/game/g_weapon.c:3391-3547`): a short
//! trace in a box of six from a hand's reach in front of the eyes — a punch of ten or
//! twelve past the armour with the punch sound on the puncher (a swing into anything at
//! all makes it, a swing into nothing is silent), or the baton's twenty half-absorbed
//! with the shock effect, the sound on the victim and 700 ms of electrification.

use crate::damage::{
    Attacker, DAMAGE_HALF_ABSORB, DAMAGE_NO_ARMOR, DAMAGE_NO_KNOCKBACK, DamageRequest,
};
use crate::event_entity::EventEntity;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use sjk_protocol::PlayerState;

/// `MELEE_RANGE`, `MELEE_SWING1_DAMAGE`, `MELEE_SWING2_DAMAGE`, `STUN_BATON_RANGE`,
/// `STUN_BATON_DAMAGE`; the reach of the hand (twenty ahead, four to the right, six
/// under the eyes) and the box of six.
const MELEE_RANGE: f32 = 8.0;
const MELEE_SWING1_DAMAGE: i32 = 10;
const MELEE_SWING2_DAMAGE: i32 = 12;
const STUN_BATON_RANGE: f32 = 8.0;
const STUN_BATON_DAMAGE: i32 = 20;
const HAND_AHEAD: f32 = 20.0;
const HAND_RIGHT: f32 = 4.0;
const HAND_UNDER_EYES: f32 = 6.0;
const HAND_BOX: f32 = 6.0;
/// `MOD_STUN_BATON`, `MOD_MELEE`, `MASK_SHOT`, `ENTITYNUM_WORLD`, `ENTITYNUM_NONE`.
pub const MOD_STUN_BATON: u32 = 1;
const MOD_MELEE: u32 = 2;
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const ENTITY_WORLD: u16 = 1_022;
const ENTITY_NONE: u16 = 1_023;
/// `BOTH_MELEE2`: the second, harder swing.
const BOTH_MELEE2: u32 = 123;
/// `EV_PLAY_EFFECT`, `EFFECT_STUNHIT`, `EV_GENERAL_SOUND`, `CHAN_AUTO`, `CHAN_WEAPON`,
/// and the fields the effect and the sound carry.
const EV_PLAY_EFFECT: u32 = 68;
const EFFECT_STUNHIT: u32 = 8;
const EV_GENERAL_SOUND: u32 = 76;
const CHAN_AUTO: u32 = 0;
const CHAN_WEAPON: u32 = 2;
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_SABER_ENTITY: usize = 37;
/// `ps.torsoAnim`, `ps.electrifyTime`.
const PS_TORSO_ANIM: usize = 15;
const PS_ELECTRIFY_TIME: usize = 73;

/// The world as a punch or a zap reaches into it.
pub trait MeleeTargets {
    /// `trap->Trace` from `start` to `end` in the box of six, passing `skip`.
    fn trace(
        &mut self,
        skip: u16,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace;
    /// An entity that takes damage: its origin (`r.currentOrigin`) and whether it is a
    /// player; `None` for the world and anything else.
    fn struck(&self, number: u16) -> Option<Struck>;
    /// `G_Damage` on `number`.
    fn hurt(&mut self, number: u16, request: DamageRequest);
    /// `G_SoundIndex` for a name.
    fn sound(&mut self, name: &[u8]) -> u16;
    /// `G_TempEntity`.
    fn raise(&mut self, event: EventEntity);
    /// `ps.electrifyTime` on a player struck by the baton.
    fn electrify(&mut self, number: u16, until: i32);
}

/// What a swing or a zap found in its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Struck {
    pub origin: [f32; 3],
    pub player: bool,
    /// Whom a struck player duels (`ps.duelIndex` while `ps.duelInProgress`).
    pub duelling: Option<u16>,
}

/// The special duel checks (`g_weapon.c:3426-3440`, `3521-3534`): a hand or a baton
/// reaches no duellist but its opponent, and a duellist's none but its opponent.
fn barred_by_duel(state: &PlayerState, struck: &Struck, number: u16) -> bool {
    struck
        .duelling
        .is_some_and(|opponent| opponent != state.client_num())
        || state.duel_in_progress() && state.duel_index() != number
}

/// Where the hand reaches from: the eyes less six, twenty ahead, four to the right.
fn hand(state: &PlayerState, forward: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    let mut start = state.origin();
    start[2] += state.view_height() as f32 - HAND_UNDER_EYES;
    std::array::from_fn(|axis| start[axis] + HAND_AHEAD * forward[axis] + HAND_RIGHT * right[axis])
}

/// The punch sound: `sound/weapons/melee/punch1` to `4`, drawn with the game's generator.
fn punch_sound(rng: &mut Rng, targets: &mut dyn MeleeTargets) -> u16 {
    let number = rng.irand(1, 4);
    targets.sound(format!("sound/weapons/melee/punch{number}").as_bytes())
}

/// `G_Sound`: the sound's temp entity at `origin` on `channel`.
fn sound_event(origin: [f32; 3], channel: u32, index: u16) -> EventEntity {
    let mut event = EventEntity {
        event: EV_GENERAL_SOUND,
        parameter: u32::from(index),
        origin,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    event.extra[0] = (ES_SABER_ENTITY, channel);
    event
}

/// `WP_FireMelee`: the trace from the hand eight along the view; anything in the way
/// makes the punch sound on the puncher (`CHAN_AUTO`); what takes damage is hurt ten —
/// twelve on the second swing (`BOTH_MELEE2` on the torso) — past the armour as
/// `MOD_MELEE` — but between a duellist and anyone else. (Broken limbs need
/// `g_armBreakage`; a heavy melee class is Siege's.)
pub fn fire_melee(
    state: &PlayerState,
    forward: [f32; 3],
    right: [f32; 3],
    attacker: Attacker,
    level_time: i32,
    rng: &mut Rng,
    targets: &mut dyn MeleeTargets,
) {
    let start = hand(state, forward, right);
    let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + MELEE_RANGE * forward[axis]);
    let trace = targets.trace(
        state.client_num(),
        start,
        [-HAND_BOX; 3],
        [HAND_BOX; 3],
        end,
        MASK_SHOT,
    );
    if trace.entity_number == ENTITY_NONE {
        return;
    }
    let sound = punch_sound(rng, targets);
    targets.raise(sound_event(state.origin(), CHAN_AUTO, sound));
    let Some(struck) = targets.struck(trace.entity_number) else {
        return;
    };
    if barred_by_duel(state, &struck, trace.entity_number) {
        return;
    }
    let damage = if state.raw_field(PS_TORSO_ANIM) == Some(BOTH_MELEE2) {
        MELEE_SWING2_DAMAGE
    } else {
        MELEE_SWING1_DAMAGE
    };
    targets.hurt(
        trace.entity_number,
        DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: Some(forward),
            point: Some(trace.end_position),
            damage,
            flags: DAMAGE_NO_ARMOR,
            means: MOD_MELEE,
        },
    );
}

/// `WP_FireStunBaton`: the same trace; the world and a miss are nothing; what takes
/// damage gets the shock effect where the trace ended (`EFFECT_STUNHIT`, the plane's
/// normal as its angles), the punch sound on itself (`CHAN_WEAPON`), twenty
/// half-absorbed without knockback as `MOD_STUN_BATON`, and — a player — 700 ms of
/// electrification.
pub fn fire_stun_baton(
    state: &PlayerState,
    forward: [f32; 3],
    right: [f32; 3],
    attacker: Attacker,
    level_time: i32,
    rng: &mut Rng,
    targets: &mut dyn MeleeTargets,
) {
    let start = hand(state, forward, right);
    let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + STUN_BATON_RANGE * forward[axis]);
    let trace = targets.trace(
        state.client_num(),
        start,
        [-HAND_BOX; 3],
        [HAND_BOX; 3],
        end,
        MASK_SHOT,
    );
    if trace.entity_number >= ENTITY_WORLD {
        return;
    }
    let Some(struck) = targets.struck(trace.entity_number) else {
        return;
    };
    if barred_by_duel(state, &struck, trace.entity_number) {
        return;
    }
    let mut effect = EventEntity {
        event: EV_PLAY_EFFECT,
        parameter: EFFECT_STUNHIT,
        origin: trace.end_position,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    for axis in 0..3 {
        effect.extra[axis] = (ES_ANGLES[axis], trace.plane_normal[axis].to_bits());
        effect.extra[3 + axis] = (ES_ORIGIN[axis], trace.end_position[axis].to_bits());
    }
    targets.raise(effect);
    let sound = punch_sound(rng, targets);
    targets.raise(sound_event(struck.origin, CHAN_WEAPON, sound));
    targets.hurt(
        trace.entity_number,
        DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: Some(forward),
            point: Some(trace.end_position),
            damage: STUN_BATON_DAMAGE,
            flags: DAMAGE_NO_KNOCKBACK | DAMAGE_HALF_ABSORB,
            means: MOD_STUN_BATON,
        },
    );
    if struck.player {
        targets.electrify(trace.entity_number, level_time + 700);
    }
}

/// The field a zap sets on its victim.
pub const ELECTRIFY_TIME: usize = PS_ELECTRIFY_TIME;

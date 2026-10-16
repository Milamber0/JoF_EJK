//! The thrown saber (OpenJK `codemp/game/w_saber.c`):
//! - the throw's start in `WP_SaberPositionUpdate` (`:8617-8726`);
//! - its flight out (`saberFirstThrown`, `:7127-7262`), steered at the higher levels;
//! - what it strikes (`thrownSaberTouch`, `CheckThrownSaberDamaged`,
//!   `saberCheckRadiusDamage`, `:5856-6133`, `:7090-7125`), and `G_RunObject`
//!   (`g_object.c:94-260`) for a saber, which neither bounces nor stops;
//! - its flight back and the catch (`saberBackToOwner`, `saberMoveBack`, `:6134-6190`,
//!   `:6927-7086`).
//!
//! The saber is its owner's saber entity, which the game keeps for the player's whole
//! life. [`SaberEntity`] is what the game keeps of it beside its wire state, and the
//! wire state is the caller's, handed in. What the flight reaches — traces, the owner,
//! the others, damage, events — is a [`SaberFlight`].
//!
//! A thrown saber knocked out of the air is [`crate::saber_drop`]'s.
//!
//! Not here yet: the blade's own damage traces in flight, which need the saber model's
//! Ghoul2 bolt.

use crate::damage::{Attacker, DamageRequest, MOD_SABER};
use crate::event_entity::EventEntity;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use sjk_protocol::{EntityState, PlayerState};

/// `SABER_THROWN_HIT_DAMAGE`, `MIN_SABER_SLICE_DISTANCE`,
/// `MIN_SABER_SLICE_RETURN_DISTANCE`, `SABER_MAX_THROW_DISTANCE`.
const THROWN_HIT_DAMAGE: i32 = 30;
const SLICE_DISTANCE: f32 = 50.0;
const SLICE_RETURN_DISTANCE: f32 = 30.0;
const MAX_THROW_DISTANCE: f32 = 700.0;
/// `PROPER_THROWN_VALUE`: the in-hand think saw its owner throw before the owner's
/// update started the throw.
const PROPER_THROWN_VALUE: i32 = 999;
/// `FRAMETIME`.
const FRAME_TIME: i32 = 100;
/// `CONTENTS_LIGHTSABER`, `MASK_PLAYERSOLID`, `MASK_SOLID`, `MASK_SHOT`.
pub const CONTENTS_LIGHTSABER: u32 = 0x4_0000;
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
const MASK_SOLID: u32 = 0x1 | 0x1000;
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
/// The thrown saber's box (`SABERMINS_*`, `SABERMAXS_*`).
const THROWN_BOX: [f32; 3] = [3.0, 3.0, 3.0];
/// `saberMoveBack`'s look-ahead box and reach (`THROWN_SABER_COMP`).
const COMPENSATION_BOX: [f32; 3] = [24.0, 24.0, 8.0];
const COMPENSATION_LENGTH: f32 = 32.0;
/// `ENTITYNUM_NONE`, `ENTITYNUM_WORLD`, `MAX_CLIENTS`.
const ENTITY_NONE: u16 = 1_023;
const ENTITY_WORLD: u16 = 1_022;
const MAX_CLIENTS: u16 = 32;
/// `FORCE_LEVEL_2`, `FORCE_LEVEL_3`, `WP_SABER`, `ET_GENERAL`, `TR_LINEAR`.
const LEVEL_2: u8 = 2;
const LEVEL_3: u8 = 3;
const WP_SABER: u32 = 3;
const ET_GENERAL: u32 = 0;
const TR_LINEAR: u32 = 2;
const TR_GRAVITY: u32 = 6;
/// `CHAN_AUTO`, `EV_SABER_HIT`, `EV_SABER_BLOCK`, `EV_SABER_CLASHFLARE`.
const CHAN_AUTO: u32 = 0;
const EV_SABER_HIT: u32 = 30;
const EV_SABER_BLOCK: u32 = 31;
const EV_SABER_CLASHFLARE: u32 = 32;
/// `EF_NODRAW`.
const EF_NODRAW: u32 = 0x100;
/// `BUTTON_ALT_ATTACK`, `EF_INVULNERABLE`.
const BUTTON_ALT_ATTACK: u16 = 128;
const EF_INVULNERABLE: u32 = 1 << 27;
/// The saber model with no saber definitions (`DEFAULT_SABER_MODEL`), and the spin every
/// saber without its own makes in flight (`saberSpinSound`).
pub const SABER_MODEL: &[u8] = b"models/weapons2/saber/saber_w.glm";
const SPIN_SOUND: &[u8] = b"sound/weapons/saber/saberspin.wav";
const CATCH_SOUND: &[u8] = b"sound/weapons/saber/saber_catch.wav";

/// The wire fields the flight writes.
pub(crate) mod es {
    pub const POS_TIME: usize = 0;
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const POS_DELTA: [usize; 3] = [6, 7, 10];
    pub const POS_TYPE: usize = 23;
    pub const POS_DURATION: usize = 20;
    pub const APOS_BASE: [usize; 3] = [5, 3, 33];
    pub const APOS_DELTA: [usize; 3] = [48, 44, 49];
    pub const APOS_TYPE: usize = 15;
    pub const APOS_TIME: usize = 34;
    pub const APOS_DURATION: usize = 89;
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const ANGLES: [usize; 3] = [25, 9, 24];
    pub const TYPE: usize = 8;
    pub const WEAPON: usize = 14;
    pub const GENERIC_ENEMY: usize = 18;
    pub const FLAGS: usize = 19;
    pub const SOLID: usize = 26;
    pub const MODEL: usize = 46;
    pub const MODEL_GHOUL2: usize = 54;
    pub const LOOP_SOUND: usize = 55;
    pub const LOOP_IS_SOUNDSET: usize = 70;
    pub const IN_FLIGHT: usize = 81;
    pub const G2_RADIUS: usize = 38;
    pub const OTHER_ENTITY: usize = 59;
    pub const OTHER_ENTITY2: usize = 39;
}

/// The player fields the flight reads and writes: `saberEntityNum`, `saberCanThrow`,
/// `saberInFlight`, `eFlags`, `saberLockTime`, `duelIndex`, `duelInProgress`,
/// `isJediMaster`.
pub(crate) mod ps {
    pub const SABER_ENTITY: usize = 31;
    pub const CAN_THROW: usize = 49;
    pub const IN_FLIGHT: usize = 88;
    pub const FLAGS: usize = 17;
    pub const LOCK_TIME: usize = 107;
    pub const DUEL_INDEX: usize = 44;
    pub const DUEL_IN_PROGRESS: usize = 119;
    pub const JEDI_MASTER: usize = 114;
}

/// What the saber entity is doing: its `think`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SaberThink {
    /// `SaberUpdateSelf`: in its owner's hand.
    #[default]
    InHand,
    /// `saberFirstThrown`: flying out.
    Thrown,
    /// `saberBackToOwner`: coming back.
    Back,
    /// `DownedSaberThink`: knocked down, lying until called back.
    Downed,
}

/// What the game keeps of a saber entity beside its wire state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SaberEntity {
    pub think: SaberThink,
    /// `nextthink`: 0 for none.
    pub next_think: i32,
    /// `speed`, which the flight uses as a time: when it next turns.
    pub speed: f32,
    /// `r.currentOrigin`, `r.currentAngles`.
    pub current: [f32; 3],
    pub current_angles: [f32; 3],
    /// `r.mins`, `r.maxs` while it flies.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `r.contents`.
    pub contents: u32,
    /// `pos1`: its owner's hand, where it flies back to.
    pub pos1: [f32; 3],
    /// `genericValue5`.
    pub value5: i32,
    /// Sent to the clients (`SVF_NOCLIENT` cleared) and linked.
    pub shown: bool,
    /// `clipmask`, `flags` (`FL_BOUNCE_HALF` once knocked down), `bounceCount`, and
    /// `eventTime` of its bounce event — which the entity pool clears it by.
    pub clip_mask: u32,
    pub flags: u32,
    pub bounce_count: i32,
    pub event_time: i32,
}

/// What the game keeps of a throw on its thrower beside the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThrowMemory {
    /// `ps.saberEntityState`: the saber's flight has been started.
    pub started: bool,
    /// `ps.saberDidThrowTime`.
    pub did_throw_time: i32,
    /// `client->saberKnockedTime`: no calling a knocked saber back before this time.
    pub knocked_time: i32,
}

/// The saber's owner as the flight reads and changes it.
pub struct SaberOwner<'a> {
    pub number: u16,
    pub state: &'a mut PlayerState,
    pub health: i32,
    /// `sess.sessionTeam == TEAM_SPECTATOR`.
    pub spectator: bool,
    /// `fd.forcePowerLevel[FP_SABER_OFFENSE]`, `[FP_SABERTHROW]`.
    pub offense: u8,
    pub throw_level: u8,
    /// `client->buttons`, latched at the last think's end, and `pers.cmd.buttons`, the
    /// last command's.
    pub buttons: u16,
    pub command_buttons: u16,
    /// `lastSaberBase_Always`, `olderSaberBase` and when: the blade's last readings.
    pub storage: crate::saber_clash::SaberStorage,
    /// `level.gametype`, for `BG_CanUseFPNow`.
    pub gametype: i32,
    /// `sess.sessionTeam`, for the damage it does.
    pub team: i32,
    pub memory: &'a mut ThrowMemory,
    /// `ps.saberThrowDelay`, `ps.saberAttackWound`.
    pub throw_delay: &'a mut i32,
    pub attack_wound: &'a mut i32,
    /// `client->dangerTime`, `invulnerableTimer`.
    pub danger_time: &'a mut i32,
    pub invulnerable_until: &'a mut i32,
}

/// Anything the thrown saber may strike, as `CheckThrownSaberDamaged` reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlightTarget {
    /// A player (`ent->client`), connected and in use.
    pub client: bool,
    /// `ps.origin` for a player; `r.currentOrigin` for anything else.
    pub origin: [f32; 3],
    /// A mover is struck at its box's centre (`r.absmin`, `r.absmax`).
    pub mover: Option<([f32; 3], [f32; 3])>,
    pub takes_damage: bool,
    pub health: i32,
    pub spectator: bool,
    /// `ps.duelIndex` while `ps.duelInProgress`.
    pub duel: Option<u16>,
    /// `r.contents`, and `r.ownerNum` (a saber entity is struck through its owner).
    pub contents: u32,
    pub owner: u16,
}

/// Everything beyond the saber and its owner that the flight reaches.
pub trait SaberFlight {
    /// `trap->Trace` with the given box, past `pass` and what it owns.
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace;
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// The saber's owner, when it is in the game.
    fn owner(&mut self) -> Option<SaberOwner<'_>>;
    /// How many entity numbers there are to look through (`level.num_entities`).
    fn entity_count(&self) -> u16;
    /// Entity `number` as the flight may strike it, when it is in use.
    fn target(&mut self, number: u16) -> Option<FlightTarget>;
    /// Player `number`'s `FP_SABER_DEFENSE` level.
    fn defence(&mut self, number: u16) -> Option<u8>;
    /// `WP_SaberCanBlock(ent, point, 0, MOD_SABER, qfalse, 999)` and, where it can,
    /// `WP_SaberBlockNonRandom(ent, point, qfalse)`: whether player `number` blocked.
    fn block(&mut self, number: u16, point: [f32; 3]) -> bool;
    /// Player `number`'s view (`ps.viewangles`).
    fn view_angles(&mut self, number: u16) -> Option<[f32; 3]>;
    /// `SetSaberBoxSize` for the saber at `current`: from its owner's blade read lately,
    /// else the default box.
    fn saber_box(&mut self, current: [f32; 3]) -> ([f32; 3], [f32; 3]);
    /// `G_RunMissile` for the knocked saber as a missile: through the map and the
    /// others, its owner passed; bouncing off what it strikes.
    fn run_missile(
        &mut self,
        missile: &mut crate::weapon_fire::Missile,
        level_time: i32,
        previous_time: i32,
    );
    /// `G_Spawn` for a dead saber (`MakeDeadSaber`), linked and running from now.
    fn spawn_dead_saber(&mut self, missile: crate::weapon_fire::Missile);
    /// `G_Damage`.
    fn hurt(&mut self, target: u16, request: DamageRequest);
    /// A temp entity raised now.
    fn raise(&mut self, event: EventEntity);
    /// What the owner's sabers lend a saber out of the hand.
    fn owner_saber(&mut self) -> SaberLook;
    /// The owner's first saber (the default one for none), for what its touch does.
    fn owner_first_saber(&self) -> Option<&crate::saber_definition::SaberDefinition>;
    /// `G_SoundIndex`, `G_ModelIndex`.
    fn sound_index(&mut self, name: &[u8]) -> u16;
    fn model_index(&mut self, name: &[u8]) -> u16;
    /// The game's generator (`Q_irand`).
    fn rng(&mut self) -> &mut Rng;
}

/// What a saber out of the hand shows of its owner's sabers: the first's hilt
/// (`saber[0].model`, registered, the default one for none) and the sounds its
/// definitions give (`soundOn` of each hand, the first's `soundLoop`, `soundOff`,
/// `spinSound`), 0 where one has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaberLook {
    pub model: u16,
    pub on: [u16; 2],
    pub hum: u16,
    pub off: u16,
    pub spin: u16,
}

/// A flying saber's touch (`w_saber.c:5949-5956, 6030-6037`): the owner's first saber's
/// `SFL2_NO_DISMEMBERMENT` and knockback scale.
fn touch_flags(first: Option<&crate::saber_definition::SaberDefinition>) -> u32 {
    let Some(first) = first else { return 0 };
    let mut flags = 0;
    if first.flags2 & (1 << 4) != 0 {
        flags |= crate::saber_damage::DAMAGE_NO_DISMEMBER;
    }
    if first.knockback_scale[0] > 0.0 {
        flags |= crate::damage::DAMAGE_SABER_KNOCKBACK1;
    }
    flags
}

/// `SFL_RETURN_DAMAGE`: "when returning from a saber throw, it keeps spinning and doing
/// damage".
const SFL_RETURN_DAMAGE: u32 = 1 << 6;

/// `s.saberInFlight` on the saber entity: still going out, spinning.
pub const ES_SABER_IN_FLIGHT: usize = 81;

/// The thrown saber's blade (`w_saber.c:8848-8888`): its bolt is read on the flying
/// hilt's own Ghoul2 instance, which has no bolts, so `G2API_GetBoltMatrix` answers with
/// the world matrix itself (its fallback, without the 90° swap): the saber's origin,
/// along its angles' forward. Going out, where its trajectories have it 50 ms on; coming
/// back, where it is now, aimed at its owner. Every blade of it reads the same.
pub fn thrown_blade(
    state: &EntityState,
    owner_origin: [f32; 3],
    level_time: i32,
) -> ([f32; 3], [f32; 3]) {
    let get = |index: usize| state.raw_field(index).unwrap_or(0);
    let vector = |indices: [usize; 3]| indices.map(|index| f32::from_bits(get(index)));
    let evaluate = |base: [usize; 3],
                    delta: [usize; 3],
                    kind: usize,
                    time: usize,
                    duration: usize,
                    at: i32| {
        crate::trajectory::legacy_evaluate_trajectory(
            vector(base),
            vector(delta),
            get(kind) as u8,
            get(time) as i32,
            get(duration) as i32,
            at,
        )
    };
    let position = |at: i32| {
        evaluate(
            es::POS_BASE,
            es::POS_DELTA,
            es::POS_TYPE,
            es::POS_TIME,
            es::POS_DURATION,
            at,
        )
    };
    let (origin, angles) = if get(ES_SABER_IN_FLIGHT) != 0 {
        (
            position(level_time + 50),
            evaluate(
                es::APOS_BASE,
                es::APOS_DELTA,
                es::APOS_TYPE,
                es::APOS_TIME,
                es::APOS_DURATION,
                level_time + 50,
            ),
        )
    } else {
        let origin = position(level_time);
        let (pitch, yaw) = crate::damage::vector_to_angles(sub(owner_origin, origin));
        (origin, [pitch, yaw, 0.0])
    };
    (origin, crate::pmove::flight::angles_to_axis(angles)[0])
}

/// What a think asks of the game besides the saber.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flown {
    #[default]
    Nothing,
    /// The owner is gone: the saber entity is freed (`G_FreeEntity`).
    Freed,
}

/// The saber and its wire state together, as the flight handles them.
pub struct Saber<'a> {
    pub number: u16,
    pub entity: &'a mut SaberEntity,
    pub state: &'a mut EntityState,
}

impl Saber<'_> {
    pub(crate) fn get(&self, index: usize) -> u32 {
        self.state.raw_field(index).unwrap_or(0)
    }
    pub(crate) fn set(&mut self, index: usize, value: u32) {
        self.state.set_raw_field(index, value);
    }
    pub(crate) fn vector(&self, indices: [usize; 3]) -> [f32; 3] {
        indices.map(|index| f32::from_bits(self.get(index)))
    }
    pub(crate) fn set_vector(&mut self, indices: [usize; 3], value: [f32; 3]) {
        for (index, value) in indices.into_iter().zip(value) {
            self.set(index, value.to_bits());
        }
    }
    /// `BG_EvaluateTrajectory(&s.pos, time)`.
    pub(crate) fn position_at(&self, time: i32) -> [f32; 3] {
        let (base, delta) = (self.vector(es::POS_BASE), self.vector(es::POS_DELTA));
        crate::trajectory::legacy_evaluate_trajectory(
            base,
            delta,
            self.get(es::POS_TYPE) as u8,
            self.get(es::POS_TIME) as i32,
            self.get(es::POS_DURATION) as i32,
            time,
        )
    }
    /// `BG_EvaluateTrajectory(&s.apos, time)`.
    pub(crate) fn angles_at(&self, time: i32) -> [f32; 3] {
        let (base, delta) = (self.vector(es::APOS_BASE), self.vector(es::APOS_DELTA));
        crate::trajectory::legacy_evaluate_trajectory(
            base,
            delta,
            self.get(es::APOS_TYPE) as u8,
            self.get(es::APOS_TIME) as i32,
            self.get(es::APOS_DURATION) as i32,
            time,
        )
    }
    /// The spin a flying saber turns with: `apos` linear, 800 a second about yaw.
    pub(crate) fn spin(&mut self) {
        self.set(es::APOS_TYPE, TR_LINEAR);
        self.set_vector(es::APOS_DELTA, [0.0, 800.0, 0.0]);
    }
}

/// `WP_SaberPositionUpdate` for an owner whose saber is out of its hand
/// (`w_saber.c:8617-8726`): a throw the movement began is started from the hand at
/// `bolt_origin`, facing `bolt_angles` — the saber's own direction with the view's yaw —
/// straight along the view at 400 a second; a flying saber learns where the hand is
/// (`pos1`), and one the in-hand think had flagged is taken back into the hand.
pub fn owner_update(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    bolt_origin: [f32; 3],
    bolt_angles: [f32; 3],
    level_time: i32,
) {
    let starting = world.owner().is_some_and(|owner| {
        !owner.memory.started && owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0) != 0
    });
    // `WP_SaberAddG2Model` registers the model as the throw starts.
    // `WP_SaberAddG2Model` registers the hilt; the saber's own spin, else the common one.
    let (model, spin) = if starting {
        let look = world.owner_saber();
        (
            look.model,
            if look.spin != 0 {
                look.spin
            } else {
                world.sound_index(SPIN_SOUND)
            },
        )
    } else {
        (0, 0)
    };
    let Some(owner) = world.owner() else { return };
    let entity_number = owner.state.raw_field(ps::SABER_ENTITY).unwrap_or(0);
    if starting {
        saber.entity.current = bolt_origin;
        saber.entity.shown = true;
        saber.set_vector(es::POS_BASE, bolt_origin);
        saber.set_vector(es::APOS_BASE, bolt_angles);
        saber.set_vector(es::ORIGIN, bolt_origin);
        saber.set_vector(es::ANGLES, bolt_angles);
        saber.set(es::IN_FLIGHT, 1);
        saber.spin();
        saber.set(es::POS_TYPE, TR_LINEAR);
        saber.set(es::TYPE, ET_GENERAL);
        saber.set(es::FLAGS, 0);
        saber.set(es::MODEL, u32::from(model));
        saber.set(es::MODEL_GHOUL2, 127);
        owner.memory.started = true;
        let (forward, _) = crate::pmove::flight::flight_axes(owner.state.view_angles());
        saber.entity.next_think = level_time + FRAME_TIME;
        saber.entity.think = SaberThink::Thrown;
        saber.set(es::SOLID, 2);
        saber.entity.contents = CONTENTS_LIGHTSABER;
        saber.entity.value5 = 0;
        saber.entity.mins = THROWN_BOX.map(|axis| -axis);
        saber.entity.maxs = THROWN_BOX;
        saber.set(es::GENERIC_ENEMY, u32::from(owner.number) + 1_024);
        saber.set(es::WEAPON, WP_SABER);
        saber.set_vector(es::POS_DELTA, forward.to_array().map(|axis| axis * 400.0));
        saber.set(es::POS_TIME, level_time as u32);
        saber.set(es::LOOP_SOUND, u32::from(spin));
        saber.set(es::LOOP_IS_SOUNDSET, 0);
        owner.memory.did_throw_time = level_time;
        *owner.danger_time = level_time;
        let flags = owner.state.raw_field(ps::FLAGS).unwrap_or(0);
        owner
            .state
            .set_raw_field(ps::FLAGS, flags & !EF_INVULNERABLE);
        *owner.invulnerable_until = 0;
    } else if entity_number != 0 {
        saber.entity.pos1 = bolt_origin;
        if saber.entity.value5 == PROPER_THROWN_VALUE {
            saber.entity.value5 = 0;
            saber.entity.think = SaberThink::InHand;
            saber.entity.next_think = level_time;
            back_in_hand(owner, level_time, 500);
        }
    }
}

/// `WP_SaberInitBladeData`'s wire state for a saber entity in hand: not drawn, its
/// Ghoul2 instance the hilt's.
pub fn in_hand_state(state: &mut EntityState) {
    state.set_raw_field(es::FLAGS, EF_NODRAW);
    state.set_raw_field(es::MODEL_GHOUL2, 1);
}

/// `WP_SaberPositionUpdate` for a lit saber in its owner's hand (`w_saber.c:8738-8745`):
/// sent to nobody, a blade to other blades, its loop quiet. Its box is the caller's
/// (`SetSaberBoxSize`).
pub fn hand_update(saber: &mut Saber) {
    saber.entity.shown = false;
    saber.entity.contents = CONTENTS_LIGHTSABER;
    saber.set(es::LOOP_SOUND, 0);
    saber.set(es::LOOP_IS_SOUNDSET, 0);
}

/// The owner has its saber back: not in flight, the throw over, no throw again for
/// `delay` ms.
pub(crate) fn back_in_hand(owner: SaberOwner, level_time: i32, delay: i32) {
    owner.state.set_raw_field(ps::IN_FLIGHT, 0);
    owner.memory.started = false;
    *owner.throw_delay = level_time + delay;
    owner.state.set_raw_field(ps::CAN_THROW, 0);
}

/// `G_RunThink` for the saber entity: its think, once its time has come.
pub fn run_think(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
    previous_time: i32,
) -> Flown {
    // Knocked down, it is a missile: `G_RunMissile`, which thinks after it.
    if saber.get(es::TYPE) == crate::saber_drop::ET_MISSILE {
        return crate::saber_drop::run_downed(saber, world, level_time, previous_time);
    }
    let at = saber.entity.next_think;
    if at <= 0 || at > level_time {
        return Flown::Nothing;
    }
    saber.entity.next_think = 0;
    match saber.entity.think {
        SaberThink::InHand => {
            in_hand(saber, world, level_time);
            Flown::Nothing
        }
        SaberThink::Thrown => first_thrown(saber, world, level_time, previous_time),
        SaberThink::Back => back_to_owner(saber, world, level_time),
        SaberThink::Downed => Flown::Nothing,
    }
}

/// `SaberUpdateSelf`, every frame: flagged while its owner has thrown it and lives;
/// else a blade to other blades while its owner holds it lit with the skill to use it,
/// once the blade was read lately — and nothing to them otherwise.
fn in_hand(saber: &mut Saber, world: &mut dyn SaberFlight, level_time: i32) {
    let Some(owner) = world.owner() else { return };
    saber.entity.next_think = level_time;
    let thrown = owner.state.raw_field(ps::IN_FLIGHT).unwrap_or(0) != 0 && owner.health > 0;
    if thrown {
        saber.entity.value5 = PROPER_THROWN_VALUE;
        return;
    }
    saber.entity.value5 = 0;
    let state = &*owner.state;
    let lit = state.weapon() == WP_SABER as u8
        && !owner.spectator
        && owner.health >= 1
        && state.saber_holstered() == 0
        && owner.offense != 0;
    if !lit {
        saber.entity.contents = 0;
        saber.entity.clip_mask = 0;
    } else if saber.entity.contents != CONTENTS_LIGHTSABER {
        if level_time - owner.storage.last_time <= 200 {
            saber.entity.contents = CONTENTS_LIGHTSABER;
            saber.entity.clip_mask = MASK_PLAYERSOLID | CONTENTS_LIGHTSABER;
        }
    } else {
        saber.entity.clip_mask = MASK_PLAYERSOLID | CONTENTS_LIGHTSABER;
    }
}

/// The owner is gone, a spectator, dead or without its saber skill: the flight ends.
/// Gone or a spectator, the saber entity goes too; dead, it is back in hand, off.
fn owner_lost(saber: &mut Saber, world: &mut dyn SaberFlight, level_time: i32) -> Option<Flown> {
    let Some(owner) = world.owner() else {
        crate::saber_drop::make_dead_saber(saber, world, level_time);
        return Some(Flown::Freed);
    };
    if owner.spectator {
        crate::saber_drop::make_dead_saber(saber, world, level_time);
        return Some(Flown::Freed);
    }
    if owner.health >= 1 && owner.offense != 0 {
        return None;
    }
    let _ = owner;
    let off = world.owner_saber().off;
    let Some(owner) = world.owner() else {
        return Some(Flown::Freed);
    };
    saber.entity.think = SaberThink::InHand;
    saber.entity.value5 = 0;
    saber.entity.next_think = level_time;
    saber.entity.shown = false;
    saber.entity.contents = CONTENTS_LIGHTSABER;
    saber.set(es::LOOP_SOUND, 0);
    saber.set(es::LOOP_IS_SOUNDSET, 0);
    back_in_hand(owner, level_time, 500);
    if off != 0 {
        world.raise(crate::weapon_fire::sound_event(
            saber.entity.current,
            CHAN_AUTO,
            off,
        ));
    }
    crate::saber_drop::make_dead_saber(saber, world, level_time);
    Some(Flown::Nothing)
}

/// `saberFirstThrown`: back if let go after half a second, out six seconds, the power
/// no longer usable, or past its reach; steered where its thrower looks at the second
/// level and up; then what it passes near is cut, and it moves.
fn first_thrown(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
    previous_time: i32,
) -> Flown {
    if let Some(lost) = owner_lost(saber, world, level_time) {
        return lost;
    }
    let mut flown = Flown::Nothing;
    let Some(owner) = world.owner() else {
        return Flown::Freed;
    };
    let out_for = level_time - owner.memory.did_throw_time;
    let movement = crate::pmove::MovementState::from_player_state(owner.state);
    let usable =
        crate::pmove_saber_attack::can_use_power_now(&movement, level_time, owner.gametype);
    let (origin, view, view_height, level, number) = (
        owner.state.origin(),
        owner.state.view_angles(),
        owner.state.view_height() as f32,
        owner.throw_level,
        owner.number,
    );
    let turn_back = (out_for > 500 && (owner.buttons & BUTTON_ALT_ATTACK == 0 || out_for > 6_000))
        || !usable
        || length(sub(origin, saber.entity.current)) >= MAX_THROW_DISTANCE * f32::from(level);
    if turn_back {
        flown = touched(saber, world, None, level_time);
    } else if level >= LEVEL_2 && saber.entity.speed < level_time as f32 {
        let (forward, _) = crate::pmove::flight::flight_axes(view);
        let forward = forward.to_array();
        let from = [origin[0], origin[1], origin[2] + view_height];
        let to = std::array::from_fn(|axis| from[axis] + forward[axis] * 4_096.0);
        if let Some(hit) = move_back(saber, world, false, level_time) {
            flown = hit;
        }
        let base = saber.entity.current;
        saber.set_vector(es::POS_BASE, base);
        // At the third level players are sought too.
        let mask = if level >= LEVEL_3 {
            MASK_PLAYERSOLID
        } else {
            MASK_SOLID
        };
        let trace = world.trace(from, [0.0; 3], [0.0; 3], to, number, mask);
        let mut direction = sub(trace.end_position, saber.entity.current);
        normalize(&mut direction);
        saber.set_vector(es::POS_DELTA, direction.map(|axis| axis * 500.0));
        saber.set(es::POS_TIME, level_time as u32);
        saber.entity.speed = (level_time + if level >= LEVEL_3 { 100 } else { 400 }) as f32;
    }
    let cut = radius_damage(saber, world, 0, level_time);
    if cut != Flown::Nothing {
        flown = cut;
    }
    // Knocked out of the air on the way, it lies as an object; else it flies on.
    if saber.entity.think == SaberThink::Downed {
        crate::saber_drop::run_object(saber, world, level_time, previous_time);
        return flown;
    }
    let ran = run_object(saber, world, level_time);
    if ran != Flown::Nothing {
        flown = ran;
    }
    flown
}

/// `G_RunObject` for a thrown saber (`TR_LINEAR`, `WP_SABER`: no impact physics): on to
/// where its trajectory has it now, 100 ms until the next; what it strikes on the way is
/// touched (`thrownSaberTouch`).
fn run_object(saber: &mut Saber, world: &mut dyn SaberFlight, level_time: i32) -> Flown {
    saber.entity.next_think = level_time + FRAME_TIME;
    let origin = saber.position_at(level_time);
    saber.entity.current_angles = saber.angles_at(level_time);
    if saber.entity.current == origin {
        return Flown::Nothing;
    }
    let owner = world.owner().map_or(saber.number, |owner| owner.number);
    let mask = MASK_PLAYERSOLID | CONTENTS_LIGHTSABER;
    let mut trace = world.trace(
        saber.entity.current,
        saber.entity.mins,
        saber.entity.maxs,
        origin,
        owner,
        mask,
    );
    if !trace.start_solid && !trace.all_solid && trace.fraction != 0.0 {
        saber.entity.current = trace.end_position;
    } else {
        trace.fraction = 0.0;
    }
    if trace.fraction == 1.0 {
        return Flown::Nothing;
    }
    touched(saber, world, Some(trace.entity_number), level_time)
}

/// `thrownSaberTouch`: the saber stops where it is and turns back, cutting what it
/// struck — a saber through its owner — within 256 without the distance asked
/// (`noDCheck`). `None` is the saber touching itself (a turn back of its own).
fn touched(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    other: Option<u16>,
    level_time: i32,
) -> Flown {
    let owner = world.owner().map(|owner| owner.number);
    if other.is_some() && other == owner {
        return Flown::Nothing;
    }
    saber.set_vector(es::POS_DELTA, [0.0; 3]);
    saber.set(es::POS_TIME, level_time as u32);
    saber.spin();
    let base = saber.entity.current;
    saber.set_vector(es::POS_BASE, base);
    saber.entity.think = SaberThink::Back;
    saber.entity.next_think = level_time;
    let struck = other.unwrap_or(saber.number);
    let struck = match world.target(struck) {
        Some(target)
            if target.owner < MAX_CLIENTS
                && target.contents & CONTENTS_LIGHTSABER != 0
                && world.target(target.owner).is_some_and(|owner| owner.client) =>
        {
            target.owner
        }
        _ => struck,
    };
    let flown = damage_one(saber, world, struck, 256.0, 0, true, level_time);
    saber.entity.speed = 0.0;
    flown
}

/// `saberMoveBack`: on along the trajectory (made linear), the angles with it. Going
/// out, a look 32 units further on with a wider box stops it at anything but its owner
/// and sabers, cutting and touching what it met (`THROWN_SABER_COMP`).
pub(crate) fn move_back(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    going_back: bool,
    level_time: i32,
) -> Option<Flown> {
    saber.set(es::POS_TYPE, TR_LINEAR);
    let old = saber.entity.current;
    let origin = saber.position_at(level_time);
    saber.entity.current_angles = saber.angles_at(level_time);
    if !going_back {
        let mut direction = sub(origin, old);
        let length = normalize(&mut direction);
        let reach = std::array::from_fn(|axis| {
            old[axis] + direction[axis] * (length + COMPENSATION_LENGTH)
        });
        let owner = world.owner().map_or(ENTITY_NONE, |owner| owner.number);
        let trace = world.trace(
            old,
            COMPENSATION_BOX.map(|axis| -axis),
            COMPENSATION_BOX,
            reach,
            owner,
            MASK_PLAYERSOLID,
        );
        let met = trace.fraction != 1.0 || trace.start_solid || trace.all_solid;
        let saber_met = world
            .target(trace.entity_number)
            .is_some_and(|target| target.contents & CONTENTS_LIGHTSABER != 0);
        if met && trace.entity_number != owner && !saber_met {
            saber.set_vector(es::POS_DELTA, [0.0; 3]);
            let mut flown = damage_one(
                saber,
                world,
                trace.entity_number,
                256.0,
                0,
                true,
                level_time,
            );
            // Knocked out of the air by a block, it is not touched.
            if saber.get(es::POS_TYPE) == TR_GRAVITY {
                return Some(flown);
            }
            let struck = if trace.entity_number == ENTITY_NONE {
                ENTITY_WORLD
            } else {
                trace.entity_number
            };
            let touched = touched(saber, world, Some(struck), level_time);
            if touched != Flown::Nothing {
                flown = touched;
            }
            return Some(flown);
        }
    }
    saber.entity.current = origin;
    None
}

/// `saberBackToOwner`: home to the hand (`pos1`), faster at the third level and slowing
/// as it nears; caught within 32 units, cutting on the way.
pub(crate) fn back_to_owner(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    level_time: i32,
) -> Flown {
    if let Some(lost) = owner_lost(saber, world, level_time) {
        return lost;
    }
    let hum = world.owner_saber().hum;
    let Some(owner) = world.owner() else {
        return Flown::Freed;
    };
    owner
        .state
        .set_raw_field(ps::SABER_ENTITY, u32::from(saber.number));
    let level = owner.throw_level;
    saber.entity.contents = CONTENTS_LIGHTSABER;
    let mut direction = sub(saber.entity.pos1, saber.entity.current);
    let distance = length(direction);
    if saber.entity.speed < level_time as f32 {
        normalize(&mut direction);
        let _ = move_back(saber, world, true, level_time);
        let base = saber.entity.current;
        saber.set_vector(es::POS_BASE, base);
        let (base_speed, next) = if level >= LEVEL_3 {
            (900.0, level_time)
        } else {
            (700.0, level_time + 50)
        };
        saber.entity.speed = next as f32;
        let speed = if distance < 64.0 {
            base_speed - 200.0
        } else if distance < 128.0 {
            base_speed - 150.0
        } else if distance < 256.0 {
            base_speed - 100.0
        } else {
            base_speed
        };
        saber.set_vector(es::POS_DELTA, direction.map(|axis| axis * speed));
        saber.set(es::POS_TIME, level_time as u32);
    }
    // A first saber marked `returnDamage` keeps spinning, and cutting, on its way back
    // while a blade is lit (`w_saber.c:7044-7050`); any other stops.
    let returns_damage = world
        .owner_first_saber()
        .is_some_and(|first| first.flags & SFL_RETURN_DAMAGE != 0);
    let Some(owner) = world.owner() else {
        return Flown::Freed;
    };
    if !returns_damage || owner.state.saber_holstered() != 0 {
        saber.set(es::IN_FLIGHT, 0);
    }
    saber.set(es::LOOP_SOUND, u32::from(hum));
    saber.set(es::LOOP_IS_SOUNDSET, 0);
    if distance <= 32.0 {
        back_in_hand(owner, level_time, 300);
        let catch = world.sound_index(CATCH_SOUND);
        world.raise(crate::weapon_fire::sound_event(
            saber.entity.current,
            CHAN_AUTO,
            catch,
        ));
        saber.entity.think = SaberThink::InHand;
        saber.entity.value5 = 0;
        saber.entity.next_think = level_time + 50;
        return Flown::Nothing;
    }
    let returning = if saber.get(es::IN_FLIGHT) == 0 { 1 } else { 2 };
    let flown = radius_damage(saber, world, returning, level_time);
    let _ = move_back(saber, world, true, level_time);
    saber.entity.next_think = level_time;
    flown
}

/// `saberCheckRadiusDamage`: everything within reach — 30 coming straight back, 50
/// otherwise — cut, in entity number order, while the owner's wound allows.
fn radius_damage(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    returning: i32,
    level_time: i32,
) -> Flown {
    let reach = if returning == 1 {
        SLICE_RETURN_DISTANCE
    } else {
        SLICE_DISTANCE
    };
    let Some(owner) = world.owner() else {
        return Flown::Nothing;
    };
    if *owner.attack_wound > level_time {
        return Flown::Nothing;
    }
    let mut flown = Flown::Nothing;
    for number in 0..world.entity_count() {
        let cut = damage_one(saber, world, number, reach, returning, false, level_time);
        if cut != Flown::Nothing {
            flown = cut;
        }
    }
    flown
}

/// `CheckThrownSaberDamaged` on entity `number`: a player in sight and within `reach`
/// with a clear line is cut for 30 — or blocks, which may knock the saber out of the air
/// and otherwise sends it back — and anything else that can be hurt is cut for 5 (40 an
/// NPC). Going out, a cut sends the saber back. The owner's wound keeps it from cutting
/// again for half a second.
fn damage_one(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    number: u16,
    reach: f32,
    returning: i32,
    no_distance_check: bool,
    level_time: i32,
) -> Flown {
    let Some(owner) = world.owner() else {
        return Flown::Nothing;
    };
    if *owner.attack_wound > level_time {
        return Flown::Nothing;
    }
    let owner_number = owner.number;
    let owner_duel = (owner.state.raw_field(ps::DUEL_IN_PROGRESS).unwrap_or(0) != 0)
        .then(|| owner.state.raw_field(ps::DUEL_INDEX).unwrap_or(0) as u16);
    let jedi_master = owner.state.raw_field(ps::JEDI_MASTER).unwrap_or(0) != 0;
    let (throw_level, locked_lately) = (
        owner.throw_level,
        owner.state.raw_field(ps::LOCK_TIME).unwrap_or(0) as i32 > level_time - 100,
    );
    let attacker = Attacker {
        npc: false,
        client: owner_number,
        max_health: owner.state.max_health(),
        team: owner.team,
        saber_knockback: world.owner_first_saber().map_or([0.0; 4], |first| {
            [first.knockback_scale[0], first.knockback_scale[1], 0.0, 0.0]
        }),
    };
    let Some(target) = world.target(number) else {
        return Flown::Nothing;
    };
    let current = saber.entity.current;
    if target.client {
        let valid = number != owner_number
            && target.health > 0
            && target.takes_damage
            && !target.spectator
            && world.in_pvs(target.origin, current);
        if !valid
            || target.duel.is_some_and(|with| with != owner_number)
            || owner_duel.is_some_and(|with| with != number)
        {
            return Flown::Nothing;
        }
        if length(sub(current, target.origin)) >= reach {
            return Flown::Nothing;
        }
        let trace = world.trace(
            current,
            [0.0; 3],
            [0.0; 3],
            target.origin,
            saber.number,
            MASK_SHOT,
        );
        if trace.fraction != 1.0 && trace.entity_number != number {
            return Flown::Nothing;
        }
        let mut flown = Flown::Nothing;
        if !jedi_master && world.block(number, trace.end_position) {
            world.raise(struck_event(EV_SABER_BLOCK, &trace, None, owner_number));
            // `saberCheckKnockdown_Thrown`: out of the air, and nothing more of it.
            if !locked_lately
                && crate::saber_drop::thrown(
                    saber,
                    world,
                    trace.entity_number,
                    throw_level,
                    level_time,
                )
            {
                return Flown::Nothing;
            }
            if returning == 0 {
                flown = touched(saber, world, None, level_time);
            }
        } else {
            let mut direction = sub(trace.end_position, current);
            normalize(&mut direction);
            if direction == [0.0; 3] {
                direction[1] = 1.0;
            }
            let damage = if jedi_master {
                THROWN_HIT_DAMAGE * 2
            } else {
                THROWN_HIT_DAMAGE
            };
            world.hurt(
                number,
                DamageRequest {
                    level_time,
                    attacker: Some(attacker),
                    direction: Some(direction),
                    point: Some(trace.end_position),
                    damage,
                    flags: touch_flags(world.owner_first_saber()),
                    means: MOD_SABER,
                },
            );
            world.raise(struck_event(
                EV_SABER_HIT,
                &trace,
                Some(number),
                owner_number,
            ));
            if returning == 0 {
                flown = touched(saber, world, None, level_time);
            }
        }
        if let Some(owner) = world.owner() {
            *owner.attack_wound = level_time + 500;
        }
        return flown;
    }
    let valid = target.takes_damage
        && target.health > 0
        && number != owner_number
        && number != saber.number
        && (no_distance_check || world.in_pvs(target.origin, current));
    if !valid {
        return Flown::Nothing;
    }
    let target_origin = match target.mover {
        Some((absmin, absmax)) => std::array::from_fn(|axis| (absmin[axis] + absmax[axis]) * 0.5),
        None => target.origin,
    };
    let distance = if no_distance_check {
        0.0
    } else {
        length(sub(current, target.origin))
    };
    if distance >= reach {
        return Flown::Nothing;
    }
    let trace = world.trace(
        current,
        [0.0; 3],
        [0.0; 3],
        target_origin,
        saber.number,
        MASK_SHOT,
    );
    if trace.fraction != 1.0 && trace.entity_number != number {
        return Flown::Nothing;
    }
    let mut direction = sub(trace.end_position, target_origin);
    normalize(&mut direction);
    world.hurt(
        number,
        DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: Some(direction),
            point: Some(trace.end_position),
            damage: 5,
            flags: touch_flags(world.owner_first_saber()),
            means: MOD_SABER,
        },
    );
    let mut hit = struck_event(EV_SABER_HIT, &trace, Some(ENTITY_NONE), owner_number);
    // "don't do clash flare - NOTE: assumes same is true for both sabers if using dual
    // sabers!" (`w_saber.c:6062-6068`): on a mover the hit event is freed at once and
    // nothing is sent (the reference's freed slot is not modelled).
    let no_flare = world
        .owner_first_saber()
        .is_some_and(|saber| saber.flags2 & crate::saber_damage::SFL2_NO_CLASH_FLARE != 0);
    if target.mover.is_some() {
        if no_flare {
            return finish_touch(saber, world, returning, level_time);
        }
        hit.parameter = 0;
        world.raise(hit);
        let mut flare = EventEntity {
            event: EV_SABER_CLASHFLARE,
            parameter: 0,
            origin: trace.end_position,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for (slot, (index, value)) in es::ORIGIN.into_iter().zip(trace.end_position).enumerate() {
            flare.extra[slot] = (index, value.to_bits());
        }
        world.raise(flare);
    } else {
        world.raise(hit);
    }
    finish_touch(saber, world, returning, level_time)
}

/// The end of a thrown saber's blow on a non-client: back to its owner unless already
/// returning (`thrownSaberTouch`), and the owner's `saberAttackWound`.
fn finish_touch(
    saber: &mut Saber,
    world: &mut dyn SaberFlight,
    returning: i32,
    level_time: i32,
) -> Flown {
    let flown = if returning == 0 {
        touched(saber, world, None, level_time)
    } else {
        Flown::Nothing
    };
    if let Some(owner) = world.owner() {
        *owner.attack_wound = level_time + 500;
    }
    flown
}

/// A thrown saber's `EV_SABER_HIT` or `EV_SABER_BLOCK` where the trace ended, facing its
/// plane (up the y axis where it has none), for saber 0, blade 0.
fn struck_event(event: u32, trace: &MovementTrace, struck: Option<u16>, owner: u16) -> EventEntity {
    let mut angles = trace.plane_normal;
    if angles == [0.0; 3] {
        angles[1] = 1.0;
    }
    let mut hit = EventEntity {
        event,
        parameter: 1,
        origin: trace.end_position,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    let mut slot = 0;
    let mut put = |index: usize, value: u32| {
        hit.extra[slot] = (index, value);
        slot += 1;
    };
    for (index, value) in es::ORIGIN.into_iter().zip(trace.end_position) {
        put(index, value.to_bits());
    }
    for (index, value) in es::ANGLES.into_iter().zip(angles) {
        put(index, value.to_bits());
    }
    if let Some(struck) = struck {
        put(es::OTHER_ENTITY, u32::from(struck));
        put(es::OTHER_ENTITY2, u32::from(owner));
    }
    hit
}

fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

/// `VectorLength`.
fn length(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

/// `VectorNormalize`, returning the length.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = length(*vector);
    if length != 0.0 {
        let inverse = 1.0 / length;
        *vector = vector.map(|axis| axis * inverse);
    }
    length
}

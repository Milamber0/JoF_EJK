//! Jedi Master (`g_gametype 2`): the one saber on the level and the player who holds it
//! (OpenJK `codemp/game/g_client.c:278-541`, `g_combat.c:2552-2620, 4831-4836`,
//! `w_force.c:4833-4885, 5429`, `g_main.c:337-340, 414-436`).
//!
//! - **The saber** ([`JediMasterSaber`], `SP_info_jedimaster_start`): a world object at
//!   the map's `info_jedimaster_start`, or dropped at a deathmatch spawn point when the
//!   map has none. It falls and bounces as a missile would (`G_RunMissile`: it is an
//!   `ET_MISSILE`), thinks every 50 ms (`JMSaberThink`), and lies as an object
//!   (`G_RunObject`).
//! - **Taking it** ([`JediMasterSaber::touch`], `JMSaberTouch`): a living player without
//!   a saber becomes the master — the saber alone, 200 health, a full pool, every power
//!   at the third rank, the spawn's protection, `CS_CLIENT_JEDIMASTER` and a centre print
//!   to everyone. The saber hides and remembers to come home twenty seconds on.
//! - **Losing it** ([`JediMasterSaber::throw_to_attacker`], `ThrowSaberToAttacker`): a
//!   master killed by another player throws the saber towards the killer; one who dies
//!   by its own hand or the world sends it home; one who leaves drops it where it stood,
//!   and it goes home at the next think.
//! - **The rules while there is a master** ([`scoring`], [`spares`],
//!   `Forcer::jedi_master_update`):
//!   only the master scores or is scored on, the others cannot hurt each other, the
//!   master's powers are kept at the top and it regenerates four times as fast, the
//!   others know levitation alone.
//!
//! Two reference quirks are kept as they are: the configstring is not cleared when the
//! master leaves the game, and the departed master's slot keeps counting as a master for
//! [`spares`] (`G_ThereIsAMaster` reads `ps.isJediMaster` of every slot, in use or not)
//! until someone connects into it.

use crate::entity_id::EntityId;
use crate::pmove::MovementCollision;
use crate::weapon_fire::{Missile, MissileFrame, MissileRun};
use sjk_protocol::{EntityState, PlayerState};

/// `GT_JEDIMASTER`.
pub const GT_JEDIMASTER: i32 = 2;
/// `CS_CLIENT_JEDIMASTER`.
pub const CS_CLIENT_JEDIMASTER: usize = 28;
/// `DEFAULT_SABER_MODEL`: the saber's own model while nobody holds it.
pub const DEFAULT_SABER_MODEL: &[u8] = b"models/weapons2/saber/saber_w.glm";
/// `JMSABER_RESPAWN_TIME`: "in case it gets stuck somewhere no one can reach".
const RESPAWN_TIME: i32 = 20_000;
/// `g_spawnInvulnerability`'s default: the new master's protection.
const SPAWN_INVULNERABILITY: i32 = 3_000;
/// `EVENT_VALID_MSEC`.
const EVENT_VALID_MS: i32 = 300;
/// `EV_BECOME_JEDIMASTER`.
const EV_BECOME_JEDIMASTER: u32 = 34;
/// `CONTENTS_TRIGGER`, `MASK_SOLID`, `FL_BOUNCE_HALF`'s bounce count `-5` (never spent).
const CONTENTS_TRIGGER: u32 = 0x400;
const MASK_SOLID: u32 = 0x1 | 0x1000;
const BOUNCE_FOREVER: i32 = -5;
/// `ET_GENERAL`, `ET_MISSILE`, `TR_GRAVITY`, `WP_SABER`, `EF_NODRAW`, `EF_INVULNERABLE`.
const ET_GENERAL: u32 = 0;
const ET_MISSILE: u32 = 3;
const TR_GRAVITY: u32 = 6;
const WP_SABER: u32 = 3;
const EF_NODRAW: u32 = 1 << 8;
const EF_INVULNERABLE: u32 = 1 << 27;
/// `STAT_HEALTH`, `STAT_WEAPONS`.
const STAT_HEALTH: usize = 0;
const STAT_WEAPONS: usize = 4;
/// The entity's wire fields (`msg.cpp` `entityStateFields`).
mod es {
    pub const POS_TIME: usize = 0;
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const POS_DELTA: [usize; 3] = [6, 7, 10];
    pub const TYPE: usize = 8;
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const WEAPON: usize = 14;
    pub const EFLAGS: usize = 19;
    pub const POS_TYPE: usize = 23;
    pub const EVENT: usize = 28;
    pub const APOS_BASE: [usize; 3] = [5, 3, 33];
    pub const APOS_DELTA: [usize; 3] = [48, 44, 49];
    pub const G2_RADIUS: usize = 38;
    pub const MODEL: usize = 46;
    pub const ORIGIN2: [usize; 3] = [56, 60, 53];
    pub const GHOUL2: usize = 54;
}
/// The player's wire fields.
mod ps {
    pub const EFLAGS: usize = 17;
    pub const FORCE_POWER: usize = 18;
    pub const WEAPON: usize = 47;
    pub const KNOWN: usize = 51;
    pub const SABER_IN_FLIGHT: usize = 88;
    pub const ZOOM_MODE: usize = 90;
    pub const JEDI_MASTER: usize = 114;
}

/// Where the map places the saber: every `info_jedimaster_start`'s origin, in the order
/// of the entity lump.
pub fn starts(entities: &[sjk_entity::Entity]) -> Vec<[f32; 3]> {
    entities
        .iter()
        .filter(|entity| entity.classname() == Some("info_jedimaster_start"))
        .map(|entity| entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]))
        .collect()
}

/// `maxJediMasterDistance` (squared), `maxJediMasterFOV`, `maxForceSightDistance` (the
/// reference's own `Square(1500) * 1500`), `maxForceSightFOV`.
const MAX_MASTER_DISTANCE_SQUARED: f32 = 2500.0 * 2500.0;
const MAX_MASTER_FOV: f32 = 100.0;
const MAX_SIGHT_DISTANCE: f32 = 1500.0 * 1500.0 * 1500.0;
const MAX_SIGHT_FOV: f32 = 100.0;
/// `forcePowersActive`, and seeing's bit in it.
const PS_FORCE_ACTIVE: usize = 82;
const FP_SEE: u32 = 14;

/// `G_UpdateClientBroadcasts` (`g_active.c:1114-1168`): whether player `this` is sent to
/// player `other` wherever they are — the Jedi Master to anyone near enough who looks its
/// way, anyone to a player using Force sight who looks its way. Run at each of `this`'s
/// thinks against every other client in the game.
pub fn broadcast_to(gametype: i32, this: &PlayerState, other: &PlayerState) -> bool {
    let (from, to) = (this.origin(), other.origin());
    let between: [f32; 3] = std::array::from_fn(|axis| from[axis] - to[axis]);
    let distance = between.iter().map(|axis| axis * axis).sum::<f32>();
    let angles = crate::player_angle_math::vector_angles(between);
    let looking = |fov: f32| {
        crate::force_throw::in_field_of_vision(other.view_angles(), fov, [angles[0], angles[1]])
    };
    let master = gametype == GT_JEDIMASTER
        && is_master(this)
        && distance < MAX_MASTER_DISTANCE_SQUARED
        && looking(MAX_MASTER_FOV);
    let sight = other.raw_field(PS_FORCE_ACTIVE).unwrap_or(0) & (1 << FP_SEE) != 0
        && distance < MAX_SIGHT_DISTANCE
        && looking(MAX_SIGHT_FOV);
    master || sight
}

/// Whether a player's state says it is the Jedi Master (`ps.isJediMaster`).
pub fn is_master(state: &PlayerState) -> bool {
    state.raw_field(ps::JEDI_MASTER).unwrap_or(0) != 0
}

/// The one saber of a Jedi Master game, as the level keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct JediMasterSaber {
    /// Its entity.
    pub id: EntityId,
    /// Its wire state, place and physics, run as the missile it is while it lies about.
    pub missile: Missile,
    /// `r.currentAngles`.
    pub angles: [f32; 3],
    /// `enemy`: who holds it.
    pub holder: Option<u16>,
    /// `pos2[0]`, `pos2[1]`: whether it is to go home, and when.
    pub returning: bool,
    pub return_time: i32,
    /// `nextthink`.
    pub next_think: i32,
    /// `G_ModelIndex(DEFAULT_SABER_MODEL)`.
    pub default_model: u32,
}

/// What became of a touch: the new master, to be announced (`CS_CLIENT_JEDIMASTER` and
/// the centre print, `@@@BECOMEJM` after its name) and given the event; the saber's
/// Ghoul2 model is to be killed (`G_KillG2Queue`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Became {
    pub client: u16,
}

/// The player who touches the saber, as `JMSaberTouch` reads and changes it.
pub struct Toucher<'a> {
    pub client: u16,
    pub state: &'a mut PlayerState,
    /// Its entity's wire state (`s.weapon`).
    pub entity: &'a mut EntityState,
    /// `gentity_t::health`.
    pub health: &'a mut i32,
    pub force: &'a mut crate::force_powers::ForcePowers,
    /// `invulnerableTimer`.
    pub invulnerable_until: &'a mut i32,
}

/// The holder's own thrown saber, whose flight the Jedi Master's saber takes over when
/// its holder is killed with it out (`ThrowSaberToAttacker`'s `altVelocity`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InFlight {
    pub pos_base: [f32; 3],
    pub pos_delta: [f32; 3],
    pub apos_base: [f32; 3],
    pub apos_delta: [f32; 3],
    pub current: [f32; 3],
    pub current_angles: [f32; 3],
}

/// How its holder fares at the saber's think.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Holder {
    /// Still in the game.
    Present,
    /// Gone from the game (`!inuse`), last at its entity's `s.pos.trBase`.
    Gone { base: [f32; 3] },
}

impl JediMasterSaber {
    /// `SP_info_jedimaster_start` at `origin`: a bouncing, falling missile with the stock
    /// saber's model, a trigger to touch, its spawn spot remembered (`s.origin2`); linked,
    /// thinking 50 ms on. `default_model` is `G_ModelIndex(DEFAULT_SABER_MODEL)`.
    pub fn spawn(id: EntityId, origin: [f32; 3], default_model: u32, level_time: i32) -> Self {
        let mut state = EntityState::zero(id.legacy_number(), &sjk_protocol::LEGACY_ENTITY_FIELDS);
        let mut set = |index: usize, value: u32| state.set_raw_field(index, value);
        set(es::MODEL, default_model);
        set(es::GHOUL2, 1);
        set(es::G2_RADIUS, 20);
        set(es::TYPE, ET_MISSILE);
        set(es::WEAPON, WP_SABER);
        set(es::POS_TYPE, TR_GRAVITY);
        set(es::POS_TIME, level_time as u32);
        for axis in 0..3 {
            set(es::POS_BASE[axis], origin[axis].to_bits());
            set(es::ORIGIN[axis], origin[axis].to_bits());
            set(es::ORIGIN2[axis], origin[axis].to_bits());
        }
        let missile = Missile {
            state,
            current: origin,
            bounds: ([-3.0; 3], [3.0; 3]),
            owner: sjk_protocol::ENTITY_NUMBER_NONE,
            clip_mask: MASK_SOLID,
            damage: 0,
            method_of_death: 0,
            free_at: i32::MAX,
            impact_velocity: [0.0; 3],
            impact_point: [0.0; 3],
            linked: true,
            bounces: false,
            bounce_count: BOUNCE_FOREVER,
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
            dead_saber: None,
            thermal: None,
            broadcast: false,
            explodes: false,
            parent: None,
            pass_through: None,
            activator: None,
        };
        Self {
            id,
            missile,
            angles: [0.0; 3],
            holder: None,
            returning: false,
            return_time: 0,
            next_think: level_time + 50,
            default_model,
        }
    }

    fn get(&self, index: usize) -> u32 {
        self.missile.state.raw_field(index).unwrap_or(0)
    }

    fn set(&mut self, index: usize, value: u32) {
        self.missile.state.set_raw_field(index, value);
    }

    fn set_vector(&mut self, fields: [usize; 3], value: [f32; 3]) {
        for axis in 0..3 {
            self.set(fields[axis], value[axis].to_bits());
        }
    }

    /// `s.origin2`: where it spawned.
    pub fn home(&self) -> [f32; 3] {
        std::array::from_fn(|axis| f32::from_bits(self.get(es::ORIGIN2[axis])))
    }

    /// Whether it is shown (its model, not hidden): lying about to be taken.
    pub fn lying(&self) -> bool {
        self.holder.is_none() && self.get(es::MODEL) != 0
    }

    /// Placed at `origin` at rest where it was, `trBase` and `s.origin` with it.
    fn place(&mut self, origin: [f32; 3]) {
        self.set_vector(es::POS_BASE, origin);
        self.set_vector(es::ORIGIN, origin);
        self.missile.current = origin;
    }

    /// Shown again and a missile (`EF_NODRAW` off, `modelGhoul2`, `ET_MISSILE`), held by
    /// nobody.
    fn show(&mut self) {
        let flags = self.get(es::EFLAGS);
        self.set(es::EFLAGS, flags & !EF_NODRAW);
        self.set(es::GHOUL2, 1);
        self.set(es::TYPE, ET_MISSILE);
        self.holder = None;
    }

    /// `G_TouchTriggers`' contact (`trap->EntityContact`) of a player's box — its
    /// `r.mins`, `r.maxs` at `origin` — with the saber, a trigger wherever it is.
    pub fn touched_by(&self, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])) -> bool {
        let (current, (mins, maxs)) = (self.missile.current, self.missile.bounds);
        self.missile.contents & CONTENTS_TRIGGER != 0
            && (0..3).all(|axis| {
                origin[axis] + bounds.0[axis] < current[axis] + maxs[axis]
                    && origin[axis] + bounds.1[axis] > current[axis] + mins[axis]
            })
    }

    /// One server frame of the saber (`G_RunFrame`, `g_main.c:3080-3170`): an event
    /// shown long enough cleared; as a missile, `G_RunMissile` (the fall, the bounces at
    /// 0.65 with their event and sound); then its think when due (`G_RunThink`).
    pub fn run_frame(
        &mut self,
        level_time: i32,
        previous_time: i32,
        holder: Holder,
        world: &dyn MovementCollision,
        frame: &mut MissileFrame,
    ) {
        if self.get(es::TYPE) == ET_MISSILE {
            // A missile run's own `G_RunThink` is this one's: the saber never goes.
            let run = crate::weapon_fire::run_missile_against(
                &mut self.missile,
                level_time,
                previous_time,
                world,
                &mut |_, _, _| None,
                frame,
            );
            debug_assert!(
                !matches!(run, MissileRun::Hit(_) | MissileRun::Blocked { .. }),
                "the saber's mask has no bodies"
            );
            self.run_think(level_time, previous_time, holder, world);
            return;
        }
        if level_time - self.missile.event_time > EVENT_VALID_MS && self.get(es::EVENT) != 0 {
            self.set(es::EVENT, 0);
        }
        self.run_item(level_time, previous_time, holder, world);
    }

    /// `G_RunThink`: the think, once it is due.
    fn run_think(
        &mut self,
        level_time: i32,
        previous_time: i32,
        holder: Holder,
        world: &dyn MovementCollision,
    ) {
        if self.next_think > 0 && self.next_think <= level_time {
            self.next_think = 0;
            self.think(level_time, previous_time, holder, world);
        }
    }

    /// `G_RunItem` (`g_items.c:3218-3272`), which runs the saber while it is no missile
    /// (`physicsObject`: held, hidden): off the ground it falls; at rest it only thinks;
    /// else it is traced along its trajectory, thinks, and where the move struck
    /// something `G_BounceItem` stops it — `physicsBounce` is never set — a unit above a
    /// floor, or dropping from a wall. The reference frees it in a no-drop volume, which
    /// would leave the game without its saber and a dangling `gJMSaberEnt`; it is kept.
    fn run_item(
        &mut self,
        level_time: i32,
        previous_time: i32,
        holder: Holder,
        world: &dyn MovementCollision,
    ) {
        let moved = crate::item_physics::item_move(&mut self.missile, level_time, world);
        self.run_think(level_time, previous_time, holder, world);
        if let crate::item_physics::ItemMove::Moved(trace) = moved {
            let _ = crate::item_physics::item_bounce(
                &mut self.missile,
                &trace,
                level_time,
                previous_time,
                world,
            );
        }
    }

    /// `JMSaberThink`: a holder gone from the game drops it where it was, to go home at
    /// the next think; a holder still here keeps putting the homecoming off; one lying
    /// about past its time goes home. Then `G_RunObject`, 100 ms on.
    fn think(
        &mut self,
        level_time: i32,
        previous_time: i32,
        holder: Holder,
        world: &dyn MovementCollision,
    ) {
        match (self.holder, holder) {
            (Some(_), Holder::Gone { base }) => {
                self.place(base);
                let model = self.default_model;
                self.set(es::MODEL, model);
                self.show();
                self.returning = true;
                self.return_time = 0;
            }
            (Some(_), Holder::Present) => self.return_time = level_time + RESPAWN_TIME,
            (None, _) if self.returning && self.return_time < level_time => {
                let home = self.home();
                self.place(home);
                self.returning = false;
            }
            (None, _) => {}
        }
        // (`nextthink` 50 on, which `G_RunObject` makes 100.)
        // `G_RunObject`: the angles from their trajectory, then the move; its touch is
        // `JMSaberTouch`, which the world does not satisfy.
        let base: [f32; 3] =
            std::array::from_fn(|axis| f32::from_bits(self.get(es::APOS_BASE[axis])));
        let delta: [f32; 3] =
            std::array::from_fn(|axis| f32::from_bits(self.get(es::APOS_DELTA[axis])));
        let kind = self.get(15) as u8;
        self.angles = crate::trajectory::legacy_evaluate_trajectory(
            base,
            delta,
            kind,
            self.get(34) as i32,
            0,
            level_time,
        );
        self.next_think = level_time + 100;
        let _ = crate::weapon_fire::run_object(
            &mut self.missile,
            &mut self.angles,
            level_time,
            previous_time,
            world,
        );
    }

    /// `JMSaberTouch`: a living player without a saber, not already the master, takes the
    /// saber lying about and becomes the master. `level_time` is the touch's.
    pub fn touch(&mut self, player: Toucher<'_>, level_time: i32) -> Option<Became> {
        if *player.health < 1
            || !self.lying()
            || player.state.stats[STAT_WEAPONS] & (1 << WP_SABER) != 0
            || is_master(player.state)
        {
            return None;
        }
        self.holder = Some(player.client);
        let state = player.state;
        state.stats[STAT_WEAPONS] = 1 << WP_SABER;
        state.set_raw_field(ps::WEAPON, WP_SABER);
        player.entity.set_raw_field(es::WEAPON, WP_SABER);
        state.set_raw_field(ps::ZOOM_MODE, 0);
        crate::player_entity::add_event(state, EV_BECOME_JEDIMASTER, 0);
        let flags = state.raw_field(ps::EFLAGS).unwrap_or(0);
        state.set_raw_field(ps::EFLAGS, flags | EF_INVULNERABLE);
        *player.invulnerable_until = level_time + SPAWN_INVULNERABILITY;
        state.set_raw_field(ps::JEDI_MASTER, 1);
        if *player.health < 200 && *player.health > 0 {
            *player.health = 200;
            state.stats[STAT_HEALTH] = 200;
        }
        if (state.raw_field(ps::FORCE_POWER).unwrap_or(0) as i32) < 100 {
            state.set_raw_field(ps::FORCE_POWER, 100);
        }
        let known = state.raw_field(ps::KNOWN).unwrap_or(0);
        state.set_raw_field(
            ps::KNOWN,
            known | ((1 << crate::force_powers::NUM_FORCE_POWERS) - 1),
        );
        player.force.levels = [3; crate::force_powers::NUM_FORCE_POWERS];
        crate::force_powers::mirror_levels(state, player.force);
        self.returning = true;
        self.return_time = level_time + RESPAWN_TIME;
        self.set(es::MODEL, 0);
        let flags = self.get(es::EFLAGS);
        self.set(es::EFLAGS, flags | EF_NODRAW);
        self.set(es::GHOUL2, 0);
        self.set(es::TYPE, ET_GENERAL);
        Some(Became {
            client: player.client,
        })
    }

    /// `ThrowSaberToAttacker` for its dead holder (`state` its player state, `base` its
    /// entity's `s.pos.trBase`, `model` the index of its first saber's model): shown
    /// again as that model, and — with a killer (`toward`: the killer's origin) — flung
    /// from the dead towards the killer, or carried on in the flight of the holder's own
    /// thrown saber; without one, home at once. The holder is left as having thrown it,
    /// so that its body keeps none. `CS_CLIENT_JEDIMASTER` goes to `-1` and
    /// `ps.isJediMaster` is cleared: the caller's.
    pub fn throw_to_attacker(
        &mut self,
        state: &mut PlayerState,
        base: [f32; 3],
        model: u32,
        toward: Option<[f32; 3]>,
        in_flight: Option<InFlight>,
    ) {
        let flight = in_flight
            .filter(|_| toward.is_some() && state.raw_field(ps::SABER_IN_FLIGHT).unwrap_or(0) != 0);
        if let Some(flight) = flight {
            self.set_vector(es::POS_BASE, flight.pos_base);
            self.set_vector(es::POS_DELTA, flight.pos_delta);
            self.set_vector(es::APOS_BASE, flight.apos_base);
            self.set_vector(es::APOS_DELTA, flight.apos_delta);
            self.missile.current = flight.current;
            self.angles = flight.current_angles;
        }
        state.set_raw_field(ps::SABER_IN_FLIGHT, 1);
        self.set(es::MODEL, model);
        self.show();
        let Some(toward) = toward else {
            let home = self.home();
            self.place(home);
            self.returning = false;
            return;
        };
        if flight.is_none() {
            self.place(base);
            let mut direction: [f32; 3] = std::array::from_fn(|axis| toward[axis] - base[axis]);
            crate::player_angle_math::normalize(&mut direction);
            self.set_vector(
                es::POS_DELTA,
                [direction[0] * 256.0, direction[1] * 256.0, 256.0],
            );
        }
    }
}

/// The masters as `player_die` reads them in a Jedi Master game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Masters {
    /// The killer is the master (`attacker->client->ps.isJediMaster`).
    pub killer_is_master: bool,
    /// `G_GetJediMaster`: the master in the game (in use), if any.
    pub master: Option<u16>,
}

/// `player_die`'s scoring in a Jedi Master game (`g_combat.c:2552-2594`), for a death by
/// another player: the points the killer gains (one where either is the master, none
/// otherwise) and the master, if any other, who gains the point instead.
pub fn scoring(
    killer_is_master: bool,
    dead_is_master: bool,
    master: Option<u16>,
) -> (i32, Option<u16>) {
    if killer_is_master || dead_is_master {
        (1, None)
    } else {
        (0, master)
    }
}

/// `G_Damage`'s Jedi Master rule (`g_combat.c:4831-4836`, `g_friendlyFire` 0): while
/// there is a master, two others do each other no harm. `master_about` is
/// `G_ThereIsAMaster` (every slot, in use or not).
pub fn spares(
    gametype: i32,
    attacker_is_master: bool,
    target_is_master: bool,
    master_about: bool,
) -> bool {
    gametype == GT_JEDIMASTER && !attacker_is_master && !target_is_master && master_about
}

impl crate::force_powers::Forcer<'_, '_> {
    /// `JediMasterUpdate` (`w_force.c:4833-4885`) in a Jedi Master game: the master knows
    /// every power at the third rank but the team powers, drain and absorb (none) and mind
    /// trick (the second); anyone else knows levitation at the first and nothing more, any
    /// other power it had on stopped first.
    pub(crate) fn jedi_master_update(&mut self) {
        use crate::force_powers::{
            FP_ABSORB, FP_DRAIN, FP_LEVITATION, FP_TEAM_FORCE, FP_TEAM_HEAL, FP_TELEPATHY,
            NUM_FORCE_POWERS,
        };
        let master = is_master(self.state);
        for power in 0..NUM_FORCE_POWERS {
            let known = self.state.raw_field(ps::KNOWN).unwrap_or(0);
            if master {
                let unused = matches!(power, FP_TEAM_HEAL | FP_TEAM_FORCE | FP_DRAIN | FP_ABSORB);
                self.state.set_raw_field(
                    ps::KNOWN,
                    if unused {
                        known & !(1 << power)
                    } else {
                        known | 1 << power
                    },
                );
                self.force.levels[power] = match power {
                    _ if unused => 0,
                    FP_TELEPATHY => 2,
                    _ => 3,
                };
            } else if power == FP_LEVITATION {
                self.force.levels[power] = 1;
            } else {
                self.state.set_raw_field(ps::KNOWN, known & !(1 << power));
                if self.active(power) {
                    self.stop(power);
                }
                self.force.levels[power] = 0;
            }
        }
        crate::force_powers::mirror_levels(self.state, self.force);
    }
}

//! Holocron FFA (`g_gametype 1`): the holocrons on the level and the powers they carry
//! (OpenJK `codemp/game/g_misc.c:768-1150`, `w_force.c:4739-4831, 5431-5445`).
//!
//! - **A holocron** ([`Holocron`], `SP_misc_holocron`): one Force power's, dropped onto the
//!   floor below where the map puts it, run as a physics object (`G_RunItem`), thinking
//!   every 50 ms (`HolocronThink`) and moved by `G_RunObject` while it has a velocity.
//! - **Taking it** ([`Holocron::touch`], `HolocronTouch`): a living player not carrying its
//!   power, and not the one it just popped out of, carries it from now (`holocronsCarried`,
//!   the time it was taken), has its power selected unless it is using the selected one,
//!   and — at `g_maxHolocronCarry` — lets go of the oldest it carries. The holocron hides.
//! - **Losing it** (`HolocronThink`): a carrier who dies, or who let go of it, has it pop
//!   out where it stands, flung at random (`HolocronPopOut`); one who left the game or
//!   fell to its death sends it home; one lying about for thirty seconds goes home.
//! - **The powers** ([`crate::force_powers::Forcer::holocron_update`], `HolocronUpdate`):
//!   a carried holocron's power is known at the third rank; nothing else is, but
//!   levitation and saber attack at the first — and without the attack holocron the
//!   saber style is held at its base.

use crate::entity_id::EntityId;
use crate::pmove::MovementCollision;
use crate::weapon_fire::Missile;
use sjk_protocol::{EntityState, PlayerState};

/// `GT_HOLOCRON`.
pub const GT_HOLOCRON: i32 = 1;
/// `g_maxHolocronCarry`'s default.
pub const MAX_CARRY: usize = 3;
/// `HOLOCRON_RESPAWN_TIME`.
const RESPAWN_TIME: i32 = 30_000;
/// How long a holocron that popped out of a player cannot be taken back by it.
const CANT_TOUCH_TIME: i32 = 5_000;
/// `ET_HOLOCRON`, `TR_GRAVITY`, `CONTENTS_TRIGGER`, `MASK_SOLID`.
const ET_HOLOCRON: u32 = 5;
const TR_GRAVITY: u32 = 6;
const CONTENTS_TRIGGER: u32 = 0x400;
const MASK_SOLID: u32 = 0x1 | 0x1000;
/// The powers a holocron never selects.
const NEVER_SELECTED: [usize; 4] = [
    crate::force_powers::FP_SABER_OFFENSE,
    crate::force_powers::FP_SABER_DEFENSE,
    crate::force_powers::FP_SABER_THROW,
    crate::force_powers::FP_LEVITATION,
];

/// The entity's wire fields.
mod es {
    pub const POS_TIME: usize = 0;
    pub const POS_BASE: [usize; 3] = [2, 1, 4];
    pub const POS_DELTA: [usize; 3] = [6, 7, 10];
    pub const TYPE: usize = 8;
    pub const ORIGIN: [usize; 3] = [11, 12, 13];
    pub const GROUND: usize = 22;
    pub const POS_TYPE: usize = 23;
    pub const MODEL: usize = 46;
    pub const ORIGIN2: [usize; 3] = [56, 60, 53];
    pub const TRICKED3: usize = 92;
    pub const TRICKED4: usize = 94;
    pub const JEDI_MASTER: usize = 97;
}
/// The player's wire fields.
mod ps {
    pub const SELECTED: usize = 54;
    pub const ACTIVE: usize = 82;
    pub const FALLING_TO_DEATH: usize = 97;
    pub const HOLOCRON_BITS: usize = 113;
}

/// What a player carries of holocrons, which no client is sent (`holocronsCarried`,
/// `holocronCantTouch`, `holocronCantTouchTime`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Carried {
    /// When each power's holocron was taken (`level.time` as a float); 0 for not carried.
    pub since: [f32; crate::force_powers::NUM_FORCE_POWERS],
    /// The holocron that last popped out of the player, and until when it cannot take it.
    pub cant_touch: i32,
    pub cant_touch_time: f32,
}

impl Carried {
    /// How many the player carries.
    pub fn count(&self) -> usize {
        self.since.iter().filter(|since| **since != 0.0).count()
    }
}

/// Where the map places holocrons: every `misc_holocron`'s origin and power (`count`,
/// held within the powers), in the order of the entity lump.
pub fn placed(entities: &[sjk_entity::Entity]) -> Vec<([f32; 3], usize)> {
    entities
        .iter()
        .filter(|entity| entity.classname() == Some("misc_holocron"))
        .map(|entity| {
            let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
            let count = entity
                .get("count")
                .and_then(|text| text.trim().parse::<i32>().ok())
                .unwrap_or(0);
            (
                origin,
                count.clamp(0, crate::force_powers::NUM_FORCE_POWERS as i32 - 1) as usize,
            )
        })
        .collect()
}

/// A holocron on the level.
#[derive(Clone, Debug, PartialEq)]
pub struct Holocron {
    /// Its entity.
    pub id: EntityId,
    /// Its wire state, place and physics.
    pub missile: Missile,
    /// `r.currentAngles`.
    pub angles: [f32; 3],
    /// `count`: its power.
    pub power: usize,
    /// `enemy`: who carries it.
    pub carrier: Option<u16>,
    /// `pos2[0]`, `pos2[1]`: away from home, and when it goes back.
    pub away: bool,
    pub home_time: i32,
    pub next_think: i32,
}

/// The carrier as `HolocronThink` reads and changes it.
pub struct CarrierView<'a> {
    /// In the game (`inuse`).
    pub in_game: bool,
    pub health: i32,
    pub state: &'a mut PlayerState,
    pub carried: &'a mut Carried,
}

/// The player touching a holocron, as `HolocronTouch` reads and changes it.
pub struct Toucher<'a> {
    pub client: u16,
    pub health: i32,
    pub state: &'a mut PlayerState,
    pub carried: &'a mut Carried,
}

impl Holocron {
    /// `SP_misc_holocron` at `origin` for `power`: its box traced down from a tenth above
    /// the spot to the floor through `world` (`None` where it starts in something solid,
    /// which the reference frees); stood there and set falling, a trigger with its power's
    /// icon (`modelindex` `count - 128`), side and number, remembered as its home.
    pub fn spawn(
        id: EntityId,
        origin: [f32; 3],
        power: usize,
        level_time: i32,
        world: &dyn MovementCollision,
    ) -> Option<Self> {
        let lifted = [origin[0], origin[1], origin[2] + 0.1];
        let (mins, maxs) = ([-8.0; 3], [8.0, 8.0, 8.0 - 0.1]);
        let trace = world.trace(
            lifted,
            mins,
            maxs,
            [lifted[0], lifted[1], lifted[2] - 4096.0],
            MASK_SOLID,
        );
        if trace.start_solid {
            return None;
        }
        let mut state = EntityState::zero(id.legacy_number(), &sjk_protocol::LEGACY_ENTITY_FIELDS);
        let rest = trace.end_position;
        let side = match crate::force_powers::FORCE_POWER_SIDES[power] {
            2 => 1,
            1 => 2,
            _ => 3,
        };
        for axis in 0..3 {
            state.set_raw_field(es::POS_BASE[axis], rest[axis].to_bits());
            state.set_raw_field(es::ORIGIN[axis], lifted[axis].to_bits());
            state.set_raw_field(es::ORIGIN2[axis], rest[axis].to_bits());
        }
        for (index, value) in [
            (es::JEDI_MASTER, 1),
            (es::MODEL, (power as i32 - 128) as u32),
            (es::TYPE, ET_HOLOCRON),
            (es::POS_TYPE, TR_GRAVITY),
            (es::POS_TIME, level_time as u32),
            (es::TRICKED4, power as u32),
            (es::TRICKED3, side),
        ] {
            state.set_raw_field(index, value);
        }
        let missile = Missile {
            state,
            current: rest,
            bounds: ([-8.0; 3], [8.0; 3]),
            owner: sjk_protocol::ENTITY_NUMBER_NONE,
            clip_mask: MASK_SOLID,
            damage: 0,
            method_of_death: 0,
            free_at: i32::MAX,
            impact_velocity: [0.0; 3],
            impact_point: [0.0; 3],
            linked: true,
            bounces: false,
            bounce_count: 0,
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
        Some(Self {
            id,
            missile,
            angles: [0.0; 3],
            power,
            carrier: None,
            away: false,
            home_time: 0,
            next_think: level_time + 50,
        })
    }

    fn get(&self, index: usize) -> u32 {
        self.missile.state.raw_field(index).unwrap_or(0)
    }

    fn set(&mut self, index: usize, value: u32) {
        self.missile.state.set_raw_field(index, value);
    }

    fn place(&mut self, origin: [f32; 3]) {
        for axis in 0..3 {
            self.set(es::POS_BASE[axis], origin[axis].to_bits());
            self.set(es::ORIGIN[axis], origin[axis].to_bits());
        }
        self.missile.current = origin;
    }

    /// `HolocronRespawn`: its icon again.
    fn show(&mut self) {
        let model = (self.power as i32 - 128) as u32;
        self.set(es::MODEL, model);
    }

    /// `HolocronPopOut`: flung 151 to 250 a second each way across, at random, and up.
    fn pop_out(&mut self, rng: &mut crate::player_death::Rng) {
        for axis in 0..2 {
            let speed = if rng.irand(1, 10) < 5 {
                150 + rng.irand(1, 100)
            } else {
                -150 - rng.irand(1, 100)
            };
            self.set(es::POS_DELTA[axis], (speed as f32).to_bits());
        }
        let up = 150 + rng.irand(1, 100);
        self.set(es::POS_DELTA[2], (up as f32).to_bits());
    }

    /// Home again (`s.origin2`), from now.
    fn go_home(&mut self, level_time: i32) {
        let home = std::array::from_fn(|axis| f32::from_bits(self.get(es::ORIGIN2[axis])));
        self.place(home);
        self.set(es::POS_TIME, level_time as u32);
        self.away = false;
    }

    /// `HolocronTouch`'s first line: whatever touched it, it now stands on (`groundEntityNum`).
    fn touched_ground(&mut self, entity: u16) {
        self.set(es::GROUND, u32::from(entity));
    }

    /// `G_TouchTriggers`' contact of a player's box with the holocron.
    pub fn touched_by(&self, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])) -> bool {
        let (current, (mins, maxs)) = (self.missile.current, self.missile.bounds);
        (0..3).all(|axis| {
            origin[axis] + bounds.0[axis] < current[axis] + maxs[axis]
                && origin[axis] + bounds.1[axis] > current[axis] + mins[axis]
        })
    }

    /// `HolocronTouch` by a player (`G_TouchTriggers`' trace is zeroed). Returns
    /// whether it was taken: the pickup event is then on the player
    /// (`G_AddEvent(EV_ITEM_PICKUP)`, whose time is the caller's).
    pub fn touch(&mut self, player: Toucher<'_>, max_carry: usize, level_time: i32) -> bool {
        // `G_TouchTriggers` passes a zeroed trace: whoever touches it, it "stands" on 0.
        self.touched_ground(0);
        if player.health < 1
            || self.get(es::MODEL) == 0
            || self.carrier.is_some()
            || player.carried.since[self.power] != 0.0
        {
            return false;
        }
        if player.carried.cant_touch == i32::from(self.id.legacy_number())
            && player.carried.cant_touch_time > level_time as f32
        {
            return false;
        }
        // The oldest carried (the first of the earliest).
        let mut oldest: Option<usize> = None;
        for power in 0..crate::force_powers::NUM_FORCE_POWERS {
            let since = player.carried.since[power];
            if since != 0.0 && oldest.is_none_or(|old| since < player.carried.since[old]) {
                oldest = Some(power);
            }
        }
        let state = player.state;
        let (active, selected) = (
            state.raw_field(ps::ACTIVE).unwrap_or(0),
            state.raw_field(ps::SELECTED).unwrap_or(0),
        );
        if active & (1 << selected) == 0 && !NEVER_SELECTED.contains(&self.power) {
            state.set_raw_field(ps::SELECTED, self.power as u32);
        }
        if max_carry != 0
            && player.carried.count() >= max_carry
            && let Some(oldest) = oldest
        {
            player.carried.since[oldest] = 0.0;
        }
        crate::player_entity::add_event(
            state,
            crate::items::EV_ITEM_PICKUP,
            u32::from(self.id.legacy_number()),
        );
        player.carried.since[self.power] = level_time as f32;
        self.set(es::MODEL, 0);
        self.carrier = Some(player.client);
        self.away = true;
        self.home_time = level_time + RESPAWN_TIME;
        true
    }

    /// One server frame of the holocron (`G_RunItem`: `physicsObject`), its think when due
    /// between the move and the bounce; a bounce off a wall is its touch with the world.
    pub fn run_frame(
        &mut self,
        level_time: i32,
        previous_time: i32,
        carrier: Option<CarrierView<'_>>,
        world: &dyn MovementCollision,
        rng: &mut crate::player_death::Rng,
    ) {
        let moved = crate::item_physics::item_move(&mut self.missile, level_time, world);
        if self.next_think > 0 && self.next_think <= level_time {
            self.next_think = 0;
            self.think(level_time, previous_time, carrier, world, rng);
        }
        if let crate::item_physics::ItemMove::Moved(trace) = moved
            && let crate::item_physics::ItemBounce::Bounced(trace) =
                crate::item_physics::item_bounce(
                    &mut self.missile,
                    &trace,
                    level_time,
                    previous_time,
                    world,
                )
        {
            self.touched_ground(trace.entity_number);
        }
    }

    /// `HolocronThink`: a carrier dead, or no longer carrying it, has it pop out where it
    /// stands; one gone from the game or falling to its death sends it home; a carrier
    /// still holding it keeps the homecoming off; one lying about past its time goes home.
    /// Then, every 50 ms, `G_RunObject` while it has a velocity.
    fn think(
        &mut self,
        level_time: i32,
        previous_time: i32,
        carrier: Option<CarrierView<'_>>,
        world: &dyn MovementCollision,
        rng: &mut crate::player_death::Rng,
    ) {
        let bit = 1 << self.power;
        'think: {
            let Some(carrier) = carrier.filter(|_| self.carrier.is_some()) else {
                if self.carrier.take().is_some() {
                    // A carrier whose state is gone with it: left the game (`!inuse`).
                    self.show();
                    self.go_home(level_time);
                } else if self.away && self.home_time < level_time {
                    self.go_home(level_time);
                }
                break 'think;
            };
            if self.away && carrier.health < 1 {
                self.show();
                self.place(carrier.state.origin());
                self.pop_out(rng);
                carrier.carried.since[self.power] = 0.0;
                self.carrier = None;
                break 'think;
            }
            if self.away {
                self.home_time = level_time + RESPAWN_TIME;
            }
            if carrier.carried.since[self.power] == 0.0 {
                carrier.carried.cant_touch = i32::from(self.id.legacy_number());
                carrier.carried.cant_touch_time = (level_time + CANT_TOUCH_TIME) as f32;
                self.show();
                self.place(carrier.state.origin());
                self.pop_out(rng);
                self.carrier = None;
                break 'think;
            }
            if !carrier.in_game || carrier.state.raw_field(ps::FALLING_TO_DEATH).unwrap_or(0) != 0 {
                if carrier.in_game {
                    let bits = carrier.state.raw_field(ps::HOLOCRON_BITS).unwrap_or(0);
                    carrier.state.set_raw_field(ps::HOLOCRON_BITS, bits & !bit);
                    carrier.carried.since[self.power] = 0.0;
                }
                self.carrier = None;
                self.show();
                self.go_home(level_time);
                break 'think;
            }
            if self.away && self.home_time < level_time {
                self.go_home(level_time);
            }
        }
        self.next_think = level_time + 50;
        // (A negative zero is no velocity.)
        let moving = es::POS_DELTA
            .iter()
            .any(|field| f32::from_bits(self.get(*field)) != 0.0);
        if moving {
            // `G_RunObject`, 100 ms on; its touch is `HolocronTouch` with what it struck.
            self.next_think = level_time + 100;
            let base: [f32; 3] =
                std::array::from_fn(|axis| f32::from_bits(self.get([5, 3, 33][axis])));
            let delta: [f32; 3] =
                std::array::from_fn(|axis| f32::from_bits(self.get([48, 44, 49][axis])));
            self.angles = crate::trajectory::legacy_evaluate_trajectory(
                base,
                delta,
                self.get(15) as u8,
                self.get(34) as i32,
                0,
                level_time,
            );
            if let Some(trace) = crate::weapon_fire::run_object(
                &mut self.missile,
                &mut self.angles,
                level_time,
                previous_time,
                world,
            ) {
                self.touched_ground(trace.entity_number);
            }
        }
    }
}

impl crate::force_powers::Forcer<'_, '_> {
    /// `HolocronUpdate` (`w_force.c:4739-4831`) with no base rank (`noHRank` 0): a carried
    /// holocron's power known at the third rank and marked in `holocronBits`; every other
    /// power at none and forgotten, and stopped — but levitation and saber attack, known at
    /// the first, the saber style then held at the saber's base (`SS_MEDIUM`, or the dual
    /// or staff style its sabers ask for). A saber-only server grants the saber's two at
    /// the first whatever is carried.
    pub(crate) fn holocron_update(&mut self) {
        let (style, saber_only) = (u32::from(self.frame.saber_style), self.frame.saber_only);
        use crate::force_powers::{
            FP_LEVITATION, FP_SABER_DEFENSE, FP_SABER_OFFENSE, NUM_FORCE_POWERS,
        };
        const PS_KNOWN: usize = 51;
        const PS_SABER_ANIM_LEVEL: usize = 23;
        const PS_SABER_DRAW_ANIM_LEVEL: usize = 25;
        const SS_STAFF: u32 = 5;
        for power in 0..NUM_FORCE_POWERS {
            let (bits, known) = (
                self.state.raw_field(ps::HOLOCRON_BITS).unwrap_or(0),
                self.state.raw_field(PS_KNOWN).unwrap_or(0),
            );
            if self.force.holocrons.since[power] != 0.0 {
                self.state
                    .set_raw_field(ps::HOLOCRON_BITS, bits | 1 << power);
                self.state.set_raw_field(PS_KNOWN, known | 1 << power);
                self.force.levels[power] = 3;
                continue;
            }
            self.force.levels[power] = 0;
            self.state
                .set_raw_field(ps::HOLOCRON_BITS, bits & !(1 << power));
            let kept = power == FP_LEVITATION || power == FP_SABER_OFFENSE;
            if !kept {
                self.state.set_raw_field(PS_KNOWN, known & !(1 << power));
                if self.active(power) {
                    self.stop(power);
                }
            } else {
                self.force.levels[power] = 1;
            }
            if power == FP_SABER_OFFENSE {
                let known = self.state.raw_field(PS_KNOWN).unwrap_or(0);
                self.state.set_raw_field(PS_KNOWN, known | 1 << power);
                // "make sure that the player's saber stance is reset".
                self.state.set_raw_field(PS_SABER_ANIM_LEVEL, style);
                self.state.set_raw_field(PS_SABER_DRAW_ANIM_LEVEL, style);
                // The staff's base is left as it is.
                if style != SS_STAFF {
                    self.force.saber_base_reset = Some(style as u8);
                }
            }
        }
        if saber_only {
            for power in [FP_SABER_OFFENSE, FP_SABER_DEFENSE] {
                self.force.levels[power] = self.force.levels[power].max(1);
            }
        }
        crate::force_powers::mirror_levels(self.state, self.force);
    }
}

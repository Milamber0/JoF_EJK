//! Portable forcefields (`codemp/game/g_items.c::PlaceShield`, `CreateShield`,
//! `Shield*` callbacks). Geometry, health, timing and game-owned entity fields are
//! held to `tools/game-oracle/shield.c`; engine linking remains the adapter's job.

use crate::pmove::MovementTrace;
use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

/// Sound-table indices registered by `PlaceShield`, in reference order.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sounds {
    pub looping: u16,
    pub attach: u16,
    pub activate: u16,
    pub deactivate: u16,
    pub damage: u16,
}

/// The shield's next callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Create,
    GoSolid,
    Decay,
    Free,
}

/// Traces through the current world, omitting the specified entity.
pub trait World {
    fn trace(
        &self,
        skip: u16,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace;
}

/// The player placing a shield (`PlaceShield` reads pitch as well as yaw).
#[derive(Clone, Copy, Debug)]
pub struct Placer {
    pub number: u16,
    pub team: i32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

/// A placed shield. `origin` is the linked center; the wire trajectory deliberately
/// stays at the original placement point even after `CreateShield` expands it.
#[derive(Debug)]
pub struct Shield {
    pub state: EntityState,
    pub origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub contents: u32,
    pub health: i32,
    pub takes_damage: bool,
    pub next_think: i32,
    pub phase: Phase,
    pub event_time: i32,
    pub freed: bool,
    pub owner: u16,
    sounds: Sounds,
}

const MASK_SHOT: u32 = 0x1301;
const MASK_SOLID: u32 = 0x1001;
const BODY: u32 = 0x100;
const NO_DRAW: u32 = 0x100;
const EFLAGS: usize = 19;
const EVENT: usize = 28;
const LOOP_SOUND: usize = 55;
const TRICKED: usize = 58;

impl Shield {
    /// `PlaceShield`: probe ahead, drop to the floor, and schedule expansion in 500 ms.
    /// An unsuccessful placement returns none; the caller still consumes the item.
    pub fn place(
        placer: Placer,
        gametype: i32,
        now: i32,
        sounds: Sounds,
        world: &impl World,
    ) -> Option<Self> {
        let mut forward = crate::pmove::flight::flight_axes(placer.angles)
            .0
            .to_array();
        forward[2] = 0.0;
        let end = std::array::from_fn(|axis| placer.origin[axis] + 64.0 * forward[axis]);
        let (mins, maxs) = ([-4.0, -4.0, 0.0], [4.0, 4.0, 4.0]);
        let ahead = world.trace(placer.number, placer.origin, mins, maxs, end, MASK_SHOT);
        if ahead.fraction <= 0.9 {
            return None;
        }
        let pos = ahead.end_position;
        let ground = world.trace(
            placer.number,
            pos,
            mins,
            maxs,
            [pos[0], pos[1], pos[2] - 4096.0],
            MASK_SOLID,
        );
        if ground.start_solid || ground.all_solid {
            return None;
        }
        let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
        for (field, value) in [
            (8, 4),
            (46, 2),
            (39, placer.team as u32),
            (40, u32::from(placer.number)),
            (57, 1),
            (
                21,
                if gametype >= 6 {
                    placer.team as u32
                } else {
                    16
                },
            ),
            (22, u32::from(ground.entity_number)),
            (
                9,
                if forward[0].abs() > forward[1].abs() {
                    0.0_f32
                } else {
                    90.0_f32
                }
                .to_bits(),
            ),
        ] {
            state.set_raw_field(field, value);
        }
        for (axis, field) in [2, 1, 4].into_iter().enumerate() {
            state.set_raw_field(field, ground.end_position[axis].to_bits());
        }
        let mut shield = Self {
            state,
            origin: ground.end_position,
            mins: [0.0; 3],
            maxs: [0.0; 3],
            contents: 0x400,
            health: 0,
            takes_damage: false,
            next_think: now + 500,
            phase: Phase::Create,
            event_time: now,
            freed: false,
            owner: placer.number,
            sounds,
        };
        shield.sound(sounds.attach, now);
        Some(shield)
    }

    fn sound(&mut self, sound: u16, now: i32) {
        let bits = (self.state.raw_field(EVENT).unwrap_or(0) & 0x300).wrapping_add(0x100) & 0x300;
        self.state.set_raw_field(EVENT, 76 | bits);
        self.state.set_raw_field(42, u32::from(sound));
        self.event_time = now;
    }

    fn looping(&mut self, on: bool) {
        self.state.set_raw_field(
            LOOP_SOUND,
            if on {
                u32::from(self.sounds.looping)
            } else {
                0
            },
        );
        self.state.set_raw_field(70, 0);
    }

    /// `G_RunFrame`'s event expiry; returns whether the wire event changed.
    pub fn expire_event(&mut self, now: i32) -> bool {
        if now - self.event_time > 300 && self.state.raw_field(EVENT) != Some(0) {
            self.state.set_raw_field(EVENT, 0);
            true
        } else {
            false
        }
    }

    /// `G_RunThink` for this shield. Returns whether its callback ran.
    pub fn run(&mut self, number: u16, gametype: i32, now: i32, world: &impl World) -> bool {
        if self.freed || self.next_think <= 0 || self.next_think > now {
            return false;
        }
        self.next_think = 0;
        match self.phase {
            Phase::Create => self.create(number, gametype, now, world),
            Phase::GoSolid => self.go_solid(number, now, world),
            Phase::Decay => {
                self.state.set_raw_field(TRICKED, 0);
                self.health -= if gametype == 7 { 80 } else { 10 };
                self.next_think = now + 1000;
                if self.health <= 0 {
                    self.remove(now);
                }
            }
            Phase::Free => self.freed = true,
        }
        true
    }

    fn create(&mut self, number: u16, gametype: i32, now: i32, world: &impl World) {
        let mut end = self.origin;
        end[2] += 254.0;
        let up = world.trace(number, self.origin, [0.0; 3], [0.0; 3], end, MASK_SHOT);
        let height = (254.0 * up.fraction) as i32;
        let axis = if f32::from_bits(self.state.raw_field(9).unwrap_or(0)) as i32 == 0 {
            1
        } else {
            0
        };
        let mut positive = self.origin;
        let mut negative = self.origin;
        positive[axis] += 255.0;
        negative[axis] -= 255.0;
        let mut start = self.origin;
        start[2] += (height >> 1) as f32;
        // These traces slope down to the placement point's original height.
        let pos = (255.0
            * world
                .trace(number, start, [0.0; 3], [0.0; 3], positive, MASK_SHOT)
                .fraction) as i32;
        let neg = (255.0
            * world
                .trace(number, start, [0.0; 3], [0.0; 3], negative, MASK_SHOT)
                .fraction) as i32;
        let half = (pos + neg) >> 1;
        self.origin[axis] = self.origin[axis] - neg as f32 + half as f32;
        self.origin[2] += (height >> 1) as f32;
        self.mins = [-4.0, -4.0, -(height >> 1) as f32];
        self.maxs = [
            4.0,
            4.0,
            if axis == 0 {
                (height >> 1) as f32
            } else {
                height as f32
            },
        ];
        self.mins[axis] = -half as f32;
        self.maxs[axis] = half as f32;
        self.state.set_raw_field(
            61,
            (u32::from(axis == 0) << 24)
                | ((height as u32) << 16)
                | ((pos as u32) << 8)
                | neg as u32,
        );
        self.health = if gametype == 7 { 2000 } else { 250 };
        self.state.set_raw_field(65, self.health as u32);
        let blocked = world
            .trace(number, self.origin, self.mins, self.maxs, self.origin, BODY)
            .start_solid;
        if blocked {
            self.contents = 0;
            self.hide(true);
            self.next_think = now + 200;
            self.phase = Phase::GoSolid;
            self.takes_damage = false;
        } else {
            self.contents = 0x90;
            self.next_think = now;
            self.phase = Phase::Decay;
            self.takes_damage = true;
            self.sound(self.sounds.activate, now);
            self.looping(true);
        }
        // The reference unconditionally tries again, costing one health and, if
        // unobstructed, raising a second activation event in the same callback.
        self.go_solid(number, now, world);
    }

    fn hide(&mut self, hidden: bool) {
        let flags = self.state.raw_field(EFLAGS).unwrap_or(0);
        self.state.set_raw_field(
            EFLAGS,
            if hidden {
                flags | NO_DRAW
            } else {
                flags & !NO_DRAW
            },
        );
    }

    fn go_solid(&mut self, number: u16, now: i32, world: &impl World) {
        self.health -= 1;
        if self.health <= 0 {
            self.remove(now);
            return;
        }
        if world
            .trace(number, self.origin, self.mins, self.maxs, self.origin, BODY)
            .start_solid
        {
            self.next_think = now + 200;
            self.phase = Phase::GoSolid;
        } else {
            self.hide(false);
            self.contents = 1;
            self.next_think = now + 1000;
            self.phase = Phase::Decay;
            self.takes_damage = true;
            self.sound(self.sounds.activate, now);
            self.looping(true);
        }
    }

    /// `ShieldTouch`. The parent's current team is read at touch time, not cached
    /// from placement. A missing parent never opens the field.
    pub fn touch(
        &mut self,
        toucher: u16,
        team: Option<i32>,
        owner_team: Option<i32>,
        gametype: i32,
        now: i32,
    ) -> bool {
        if self.phase == Phase::Create || self.freed {
            return false;
        }
        let friend = if gametype >= 6 {
            team.zip(owner_team).is_some_and(|(a, b)| a == b)
        } else {
            owner_team.is_some() && toucher == self.owner
        };
        if !friend {
            return false;
        }
        self.contents = 0;
        self.hide(true);
        self.next_think = now + 200;
        self.phase = Phase::GoSolid;
        self.takes_damage = false;
        self.sound(self.sounds.deactivate, now);
        self.looping(false);
        true
    }

    /// `ShieldPain`/`ShieldDie`, after `G_Damage` has decided the actual damage.
    pub fn damage(&mut self, amount: i32, now: i32) {
        if !self.takes_damage || self.freed {
            return;
        }
        self.health = (self.health - amount).max(-999);
        self.sound(self.sounds.damage, now);
        if self.health <= 0 {
            self.remove(now);
        } else {
            self.phase = Phase::Decay;
            self.next_think = now + 400;
            self.state.set_raw_field(TRICKED, 1);
        }
    }

    fn remove(&mut self, now: i32) {
        self.phase = Phase::Free;
        self.next_think = now + 100;
        self.sound(self.sounds.deactivate, now);
        self.looping(false);
    }
}

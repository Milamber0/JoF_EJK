//! The dark side's held powers, which reach other players: grip (`ForceGrip`,
//! `DoGripAction`, `w_force.c:1374-1446` and `3805-4013`), lightning (`ForceLightning`,
//! `ForceLightningDamage`, `ForceShootLightning`, `w_force.c:1628-1874`) and drain
//! (`ForceDrain`, `ForceDrainDamage`, `ForceShootDrain`, `w_force.c:1876-2168`), with their
//! parts of `WP_ForcePowerStop` and `WP_ForcePowerRun`.
//!
//! Grip holds the one aimed at within 256 units: two damage a second, held in place at
//! level 1 (five seconds), lifted at 2 and drawn in front of the gripper at 3 (four
//! seconds, and after three the crushing blow — 20 or 40 — and the choke), and a gasp
//! when a lifted victim is let go. Lightning is a line at levels 1 and 2 and an arc of
//! 300 units at 3, one or two damage a shot, the victim electrified; drain a line or an
//! arc of 512 units, the victim's Force taken (2, 3 or 4 a shot) and the drainer healed by
//! as much. Absorption turns each of them down and feeds the absorber
//! ([`crate::force_throw::absorb_conversion`]). Held against
//! `tools/game-oracle/forcedark.c`.
//!
//! The powers run inside the user's Force update ([`crate::force_powers`]), which reaches
//! everyone else through a [`OtherPlayers`]. Their blows ([`OtherPlayers::damage`]) are the
//! world's to deal: the server deals them once the update is over, in order.
//!
//! A push or pull breaks a grip ([`let_go`], [`released`]; `crate::force_throw`).
//!
//! Not yet: NPCs, vehicles (a gripped rider thrown off), breakables and other damageable
//! entities in an arc, cloaking (lightning uncloaks), the jetpack a grip switches off.

use crate::damage::DamageRequest;
use crate::event_entity::EventEntity;
use crate::force_powers::{
    FORCE_POWER_NEEDED, FP_ABSORB, FP_DRAIN, FP_GRIP, FP_LIGHTNING, FP_PULL, ForcePowers, Forcer,
};
use crate::knockdown::Knockdown;
use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use sjk_protocol::PlayerState;

/// Another player as the dark powers read and change it: its wire state, its Force, the
/// game's health, its knockdown memory (`forceHandExtendTime`, `otherKiller`), its team,
/// and its linked box.
pub struct OtherPlayer<'a> {
    pub state: &'a mut PlayerState,
    pub force: &'a mut ForcePowers,
    /// `gentity_t::health`, which team heal raises.
    pub health: &'a mut i32,
    pub knockdown: &'a mut Knockdown,
    /// Who last pushed or held it (`ps.otherKiller`).
    pub other_killer: &'a mut crate::damage::OtherKiller,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// An NPC's class (`NPC_class`); `None` for a player.
    pub npc_class: Option<i32>,
    /// `r.absmin`, `r.absmax`.
    pub absmin: [f32; 3],
    pub absmax: [f32; 3],
}

/// Everyone but a Force power's user, as its grip, lightning, drain and mind trick reach
/// them, and the game its sounds and events go to.
pub trait OtherPlayers {
    /// One past the highest player number.
    fn slots(&self) -> u16;
    /// A player in the game (a corpse included), one at a time; never the user.
    fn player(&mut self, number: u16) -> Option<OtherPlayer<'_>>;
    /// A point trace (`trap->Trace` with no box) past `pass`.
    fn trace(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace;
    /// `trap->InPVS`.
    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `G_Damage` on player `victim` by the user.
    fn damage(&mut self, victim: u16, request: DamageRequest);
    /// The game's generator (`Q_irand`).
    fn rng(&mut self) -> &mut Rng;
    /// The entities the user's sounds, sound trackers and events become.
    fn pool(&mut self) -> &mut crate::entity_pool::EntityPool;
    /// `G_SoundIndex`.
    fn sound_index(&mut self, name: &[u8]) -> u16;
    /// A sound tracker freed: every client is told (`kls <player> <tracker>`,
    /// `g_utils.c:1039`) to stop the loop it tracked.
    fn loop_stopped(&mut self, player: u16, tracker: u16);
}

/// `MAX_GRIP_DISTANCE`, `GRIP_DRAIN_AMOUNT`, `FORCE_LIGHTNING_RADIUS`,
/// `MAX_DRAIN_DISTANCE`, `FORCE_DEBOUNCE_TIME` (`w_saber.h:52-57`, `w_force.c:4133`).
const MAX_GRIP_DISTANCE: f32 = 256.0;
const GRIP_DRAIN_AMOUNT: i32 = 30;
const FORCE_LIGHTNING_RADIUS: f32 = 300.0;
const MAX_DRAIN_DISTANCE: f32 = 512.0;
const FORCE_DEBOUNCE_TIME: i32 = 50;
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const ENTITY_NUMBER_NONE: u16 = 1_023;
const DAMAGE_NO_ARMOR: u32 = 0x2;
const MOD_FORCE_DARK: u32 = crate::means_of_death::MOD_FORCE_DARK;
/// `PM_NORMAL`, `PM_FLOAT`.
const PM_NORMAL: u8 = 0;
const PM_FLOAT: u8 = 2;
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_FORCE_HOLD: u32 = 3;
const HANDEXTEND_CHOKE: u32 = 5;
const WP_MELEE: u32 = 2;
const WP_SABER: u32 = 3;
const PW_DISINT_4: usize = 9;
const EF_INVULNERABLE: u32 = 1 << 27;
const CHAN_VOICE: u32 = 3;
const CHAN_BODY: u32 = 6;
const EV_FORCE_DRAINED: u32 = 96;
const PDSOUND_ABSORBHIT: u32 = 3;
const PS_VELOCITY: [usize; 3] = [6, 7, 8];
const PS_WEAPON_TIME: usize = 10;
const PS_EFLAGS: usize = 17;
const PS_FORCE_POWER: usize = 18;
const PS_VIEW_HEIGHT: usize = 22;
const PS_ROCKET_LOCK_INDEX: usize = 24;
const PS_SABER_MOVE: usize = 34;
const PS_WEAPON: usize = 47;
const PS_ROCKET_TARGET_TIME: usize = 71;
const PS_ACTIVE_FORCE_PASS: usize = 72;
const PS_ELECTRIFY_TIME: usize = 73;
const PS_ROCKET_LOCK_TIME: usize = 79;
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_ACTIVE: usize = 82;
const PS_GRIP_CRIPPLE: usize = 111;
const STAT_HEALTH: usize = 0;
const STAT_MAX_HEALTH: usize = 8;
const ES_OWNER: usize = 40;
const ES_TRICKED: usize = 58;

/// `ClientThink_real`'s movement type for a living player (`g_active.c:2103-2126`): the
/// one a grip holds it in, else `PM_NORMAL`. Returns the new type when it changes; the
/// types other rules own (spectators, the dead, noclip, freezes) are left alone.
pub fn gripped_movement_type(movement_type: u8, health: i32, grip: u8) -> Option<u8> {
    if health <= 0 || !matches!(movement_type, PM_NORMAL | PM_FLOAT) {
        return None;
    }
    let wanted = if grip != 0 { grip } else { PM_NORMAL };
    (wanted != movement_type).then_some(wanted)
}

/// The gripper's side of `WP_ForcePowerStop(FP_GRIP)` (`w_force.c:3719-3747`), the power
/// already off: no grip for three seconds, the hand in, nobody held. Returns whom it held
/// and whether a gasp may be due (the power was on, above level 1).
pub(crate) fn let_go(
    state: &mut PlayerState,
    force: &mut ForcePowers,
    hand_extend_time: &mut i32,
    was_active: bool,
    level_time: i32,
) -> (u16, bool) {
    force.grip_use_time = level_time + 3_000;
    let held = (force.grip_entity, was_active && force.levels[FP_GRIP] > 1);
    if state.raw_field(PS_FORCE_HAND_EXTEND) == Some(HANDEXTEND_FORCE_HOLD) {
        *hand_extend_time = 0;
    }
    force.grip_entity = ENTITY_NUMBER_NONE;
    state.powerups[PW_DISINT_4] = 0;
    held
}

/// The held player's side of a grip let go: its movement back to normal. Returns whether
/// it gasps — `gasp` due, alive, held more than half a second.
pub(crate) fn released(force: &mut ForcePowers, health: i32, gasp: bool, level_time: i32) -> bool {
    force.grip_movement_type = PM_NORMAL;
    gasp && health > 0 && level_time as f32 - force.grip_started > 500.0
}

/// What `DoGripAction` reads of its victim before it acts.
struct Victim {
    origin: [f32; 3],
    health: i32,
}

impl Forcer<'_, '_> {
    /// `dangerTime = level.time; eFlags &= ~EF_INVULNERABLE; invulnerableTimer = 0`: a
    /// dark power used is an attack, and ends the spawn's protection.
    pub(crate) fn unprotect(&mut self) {
        self.force.danger_time = self.frame.level_time;
        let flags = self.field(PS_EFLAGS);
        self.state
            .set_raw_field(PS_EFLAGS, flags & !EF_INVULNERABLE);
        *self.frame.invulnerable_until = 0;
    }

    /// `AngleVectors(viewangles)` and, where the game normalizes it again, the same.
    fn aim(&self) -> [f32; 3] {
        crate::pmove::flight::flight_axes(self.state.view_angles())
            .0
            .to_array()
    }

    /// `ForcePowerUsableOn(self, victim, power)` (`w_force.c:543-618`) for a player; an
    /// absorbing victim refuses a grip, with the absorbing hit's sound at most every 400
    /// ms.
    pub(crate) fn usable_on(&mut self, number: u16, power: usize) -> bool {
        let (level_time, gametype) = (self.frame.level_time, self.frame.gametype);
        let Some(victim) = self.frame.others.player(number) else {
            return false;
        };
        if crate::force_powers::has_ysalamiri(victim.state, gametype) {
            return false;
        }
        if !crate::force_powers::can_use_now(self.state, power, level_time, gametype)
            || self.state.duel_in_progress()
            || victim.state.duel_in_progress()
        {
            return false;
        }
        if crate::force_throw::npc_immune(
            victim.npc_class,
            gametype,
            power == crate::force_powers::FP_LIGHTNING,
        ) {
            return false;
        }
        if power != FP_GRIP {
            return true;
        }
        if victim.state.raw_field(PS_ACTIVE).unwrap_or(0) & (1 << FP_ABSORB) != 0 {
            if victim.force.sound_debounce < level_time {
                victim.force.sound_debounce = level_time + 400;
                let mut event =
                    crate::knockdown::predef_sound(victim.state.origin(), PDSOUND_ABSORBHIT);
                event.extra[3] = (ES_TRICKED, u32::from(number));
                self.raise(event);
            }
            return false;
        }
        let saber = victim.state.raw_field(PS_WEAPON).unwrap_or(0) == WP_SABER;
        !(saber
            && crate::saber_rules::in_special(victim.state.raw_field(PS_SABER_MOVE).unwrap_or(0)))
    }

    /// `OnSameTeam` with `g_friendlyFire` off: teammates in the team games.
    pub(crate) fn teammate(&self, team: i32) -> bool {
        same_team(self.frame.gametype, self.frame.team, team)
    }

    /// `WP_AbsorbConversion` on player `number` for `power` at the user's level: the
    /// level left, or `None` where nothing is absorbed.
    fn absorbed(&mut self, number: u16, power: usize, spent: i32) -> Option<i32> {
        let (level_time, level) = (self.frame.level_time, i32::from(self.force.levels[power]));
        let victim = self.frame.others.player(number)?;
        let (left, event) = crate::force_throw::absorb_conversion(
            victim.state,
            victim.force,
            number,
            level,
            spent,
            level_time,
        )?;
        if let Some(event) = event {
            self.raise(event);
        }
        Some(left)
    }

    /// `G_Sound(entity, channel, index)` at `origin` on a plain channel.
    pub(crate) fn sound_at(&mut self, origin: [f32; 3], channel: u32, name: &[u8]) {
        let index = self.frame.others.sound_index(name);
        self.raise(crate::weapon_fire::sound_event(origin, channel, index));
    }

    /// `G_EntitySound(player, CHAN_VOICE, name)`: a sound the player's own voice plays.
    fn voice(&mut self, number: u16, origin: [f32; 3], name: &[u8]) {
        let index = self.frame.others.sound_index(name);
        let mut event = crate::knockdown::entity_sound(origin, number, CHAN_VOICE);
        event.parameter = u32::from(index);
        self.raise(event);
    }

    /// `BG_ClearRocketLock`.
    pub(crate) fn clear_rocket_lock(&mut self) {
        self.state
            .set_raw_field(PS_ROCKET_LOCK_INDEX, u32::from(ENTITY_NUMBER_NONE));
        self.state
            .set_raw_field(PS_ROCKET_LOCK_TIME, (-1.0f32).to_bits());
        self.state.set_raw_field(PS_ROCKET_TARGET_TIME, 0);
    }

    /// `WP_DoSpecificPower(self, FP_GRIP)`: a grip found if none is held, and started
    /// (with its thirty) if not yet on.
    pub(crate) fn grip_button(&mut self) {
        if self.force.grip_entity == ENTITY_NUMBER_NONE {
            self.grip();
        }
        if self.force.grip_entity != ENTITY_NUMBER_NONE && !self.active(FP_GRIP) {
            self.start(FP_GRIP, 0);
            self.drain(FP_GRIP, GRIP_DRAIN_AMOUNT);
        }
    }

    /// `ForceGrip`: whoever the aim strikes within 256 units from the eyes — a player not
    /// crippled nor already held, not a teammate, the power usable on it — is held from
    /// now, the hand out for five seconds at most.
    fn grip(&mut self) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0
            || self.field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_NONE
            || self.field(PS_WEAPON_TIME) as i32 > 0
        {
            return;
        }
        if self.force.grip_use_time > level_time || !self.usable(FP_GRIP) {
            return;
        }
        let origin = self.origin();
        let from = [
            origin[0],
            origin[1],
            origin[2] + self.field(PS_VIEW_HEIGHT) as i32 as f32,
        ];
        let forward = self.aim();
        let to: [f32; 3] =
            std::array::from_fn(|axis| from[axis] + forward[axis] * MAX_GRIP_DISTANCE);
        let hit = self
            .frame
            .others
            .trace(from, to, self.frame.client, MASK_PLAYERSOLID);
        let number = hit.entity_number;
        let held = hit.fraction != 1.0
            && number != ENTITY_NUMBER_NONE
            && self.frame.others.player(number).is_some_and(|victim| {
                victim.state.raw_field(PS_GRIP_CRIPPLE).unwrap_or(0) == 0
                    && victim.force.grip_being_gripped < level_time as f32
            });
        if !held
            || !self.usable_on(number, FP_GRIP)
            || self
                .frame
                .others
                .player(number)
                .map(|victim| victim.team)
                .is_some_and(|team| self.teammate(team))
        {
            self.force.grip_entity = ENTITY_NUMBER_NONE;
            return;
        }
        self.force.grip_entity = number;
        if let Some(victim) = self.frame.others.player(number) {
            victim.force.grip_started = level_time as f32;
        }
        self.force.grip_damage_debounce = 0;
        self.state
            .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_FORCE_HOLD);
        *self.frame.hand_extend_time = level_time + 5_000;
    }

    /// `WP_ForcePowerRun`'s grip: the hand must stay out, a point every 100 ms, then the
    /// hold itself.
    pub(crate) fn run_grip(&mut self) {
        let level_time = self.frame.level_time;
        if self.field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_FORCE_HOLD {
            self.stop(FP_GRIP);
            return;
        }
        // The game keeps this pace in pull's debounce, which pull does not use.
        if self.force.debounce[FP_PULL] < level_time {
            self.drain(FP_GRIP, 1);
            self.force.debounce[FP_PULL] = level_time + 100;
        }
        if self.pool() < 1 {
            self.stop(FP_GRIP);
            return;
        }
        self.grip_action();
    }

    /// `DoGripAction`: the hold on the victim, let go when it is gone, out of reach, out
    /// of sight or (below level 3) out of the gripper's front.
    fn grip_action(&mut self) {
        let level_time = self.frame.level_time;
        self.unprotect();
        let number = self.force.grip_entity;
        let victim = self.frame.others.player(number).map(|victim| Victim {
            origin: victim.state.origin(),
            health: *victim.health,
        });
        let usable = victim.as_ref().is_some_and(|victim| victim.health >= 1)
            && self.usable_on(number, FP_GRIP);
        let Some(victim) = victim.filter(|_| usable) else {
            self.stop(FP_GRIP);
            self.force.grip_entity = ENTITY_NUMBER_NONE;
            if let Some(victim) = self.frame.others.player(number) {
                victim.force.grip_movement_type = PM_NORMAL;
            }
            return;
        };
        let origin = self.origin();
        let apart: [f32; 3] = std::array::from_fn(|axis| victim.origin[axis] - origin[axis]);
        let sight =
            self.frame
                .others
                .trace(origin, victim.origin, self.frame.client, MASK_PLAYERSOLID);
        let level = usize::from(self.force.levels[FP_GRIP]);
        let level = self
            .absorbed(number, FP_GRIP, FORCE_POWER_NEEDED[level][FP_GRIP])
            .unwrap_or(level as i32);
        if level == 0 || length(apart) > MAX_GRIP_DISTANCE {
            self.stop(FP_GRIP);
            return;
        }
        if !in_front(victim.origin, origin, self.state.view_angles(), 0.9) && level < 3 {
            self.stop(FP_GRIP);
            return;
        }
        if sight.fraction != 1.0 && sight.entity_number != number {
            self.stop(FP_GRIP);
            return;
        }
        if self.force.debounce[FP_GRIP] < level_time {
            // Two a second while held: ten over a grip, the crushing blow aside.
            self.force.debounce[FP_GRIP] = level_time + 1_000;
            self.blow(number, 2, None, None, DAMAGE_NO_ARMOR);
        }
        let Some(held) = self.frame.others.player(number) else {
            return;
        };
        held.force.grip_being_gripped = (level_time + 1_000) as f32;
        let elapsed = level_time as f32 - held.force.grip_started;
        if level == 1 {
            if elapsed > 5_000.0 {
                self.stop(FP_GRIP);
            }
            return;
        }
        if level == 2 {
            if held.force.grip_move_interval < level_time {
                held.state.set_raw_field(PS_VELOCITY[2], 30.0f32.to_bits());
                held.force.grip_move_interval = level_time + 300;
            }
        }
        *held.other_killer = crate::damage::OtherKiller::credit(self.frame.client, level_time);
        held.force.grip_movement_type = PM_FLOAT;
        if level == 3 && held.force.grip_move_interval < level_time {
            // Drawn to a point 128 units before the gripper, 16 up: the further, the
            // faster.
            let forward = crate::pmove::flight::flight_axes(self.state.view_angles())
                .0
                .to_array();
            let mut target: [f32; 3] =
                std::array::from_fn(|axis| origin[axis] + forward[axis] * 128.0);
            target[2] += 16.0;
            let from = held.state.origin();
            let mut towards: [f32; 3] = std::array::from_fn(|axis| target[axis] - from[axis]);
            let distance = length(towards);
            let speed = match distance {
                d if d < 16.0 => 8.0,
                d if d < 64.0 => 128.0,
                d if d < 128.0 => 256.0,
                d if d < 200.0 => 512.0,
                _ => 700.0,
            };
            normalize(&mut towards);
            for axis in 0..3 {
                held.state
                    .set_raw_field(PS_VELOCITY[axis], (towards[axis] * speed).to_bits());
            }
            held.force.grip_move_interval = level_time + 300;
        }
        if elapsed > 3_000.0 && self.force.grip_damage_debounce == 0 {
            // Lifted three seconds: the crushing blow, the choke.
            self.force.grip_damage_debounce = 1;
            self.blow(
                number,
                if level == 2 { 20 } else { 40 },
                None,
                None,
                DAMAGE_NO_ARMOR,
            );
            let choke = self.frame.others.rng().irand(1, 3);
            let name: &[u8] = match choke {
                1 => b"*choke1.wav",
                2 => b"*choke2.wav",
                _ => b"*choke3.wav",
            };
            self.voice(number, victim.origin, name);
            let Some(held) = self.frame.others.player(number) else {
                return;
            };
            held.state
                .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_CHOKE);
            held.knockdown.hand_extend_time = level_time + 2_000;
            if held.state.raw_field(PS_ACTIVE).unwrap_or(0) & (1 << FP_GRIP) != 0 {
                // Choking, it cannot hold its own grip.
                self.stop_victims_grip(number);
            }
        } else if elapsed > 4_000.0 {
            self.stop(FP_GRIP);
        }
    }

    /// `WP_ForcePowerStop(self, FP_GRIP)`, the power already off.
    pub(crate) fn stop_grip(&mut self, was_active: bool) {
        let (number, gasp) = let_go(
            self.state,
            self.force,
            self.frame.hand_extend_time,
            was_active,
            self.frame.level_time,
        );
        self.released(number, gasp);
    }

    /// [`released`] for player `number` — the user itself when a victim lets go of it —
    /// and its gasp.
    fn released(&mut self, number: u16, gasp: bool) {
        let level_time = self.frame.level_time;
        if number == self.frame.client {
            if released(self.force, *self.frame.health, gasp, level_time) {
                let origin = self.origin();
                self.voice(number, origin, b"*gasp.wav");
            }
            return;
        }
        let Some(victim) = self.frame.others.player(number) else {
            return;
        };
        let origin = victim.state.origin();
        if released(victim.force, *victim.health, gasp, level_time) {
            self.voice(number, origin, b"*gasp.wav");
        }
    }

    /// `WP_ForcePowerStop(victim, FP_GRIP)` for another player choked out of its own grip.
    fn stop_victims_grip(&mut self, number: u16) {
        let level_time = self.frame.level_time;
        let Some(victim) = self.frame.others.player(number) else {
            return;
        };
        let active = victim.state.raw_field(PS_ACTIVE).unwrap_or(0);
        victim
            .state
            .set_raw_field(PS_ACTIVE, active & !(1 << FP_GRIP));
        let (target, gasp) = let_go(
            victim.state,
            victim.force,
            &mut victim.knockdown.hand_extend_time,
            active & (1 << FP_GRIP) != 0,
            level_time,
        );
        self.released(target, gasp);
    }

    /// `ForceLightning`: with 25 Force at least, off its debounce, the hands and weapon
    /// free: the hand out, the sound, and the power on for half a second.
    pub(crate) fn lightning(&mut self) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0
            || self.pool() < 25
            || !self.usable(FP_LIGHTNING)
            || self.force.debounce[FP_LIGHTNING] > level_time
        {
            return;
        }
        if self.field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_NONE
            || self.field(PS_WEAPON_TIME) as i32 > 0
        {
            return;
        }
        self.clear_rocket_lock();
        self.state
            .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_FORCE_HOLD);
        *self.frame.hand_extend_time = level_time + 20_000;
        let origin = self.origin();
        self.sound_at(origin, CHAN_BODY, b"sound/weapons/force/lightning");
        self.start(FP_LIGHTNING, 500);
    }

    /// `ForceDrain`: as lightning, with drain's sound.
    pub(crate) fn force_drain(&mut self) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0
            || self.field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_NONE
            || self.field(PS_WEAPON_TIME) as i32 > 0
        {
            return;
        }
        if self.pool() < 25 || !self.usable(FP_DRAIN) || self.force.debounce[FP_DRAIN] > level_time
        {
            return;
        }
        self.state
            .set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_FORCE_HOLD);
        *self.frame.hand_extend_time = level_time + 20_000;
        let origin = self.origin();
        self.sound_at(origin, CHAN_BODY, b"sound/weapons/force/drain.wav");
        self.start(FP_DRAIN, 500);
    }

    /// `WP_ForcePowerRun` for lightning and drain: the hand must stay out; above level 1
    /// holding the button keeps it going; with 25 Force, a shot every 50 ms (lightning
    /// costs its point a shot).
    pub(crate) fn run_stream(&mut self, power: usize, button: u16) {
        let level_time = self.frame.level_time;
        if self.field(PS_FORCE_HAND_EXTEND) != HANDEXTEND_FORCE_HOLD {
            self.stop(power);
            return;
        }
        let selected = self.field(crate::force_powers::PS_SELECTED) as usize;
        let held = self.buttons & button != 0
            || (self.buttons & crate::force_powers::BUTTON_FORCEPOWER != 0 && selected == power);
        if self.force.levels[power] > 1 && held {
            self.force.duration[power] = level_time + 500;
        }
        if !self.available(power, 0) || self.force.duration[power] < level_time || self.pool() < 25
        {
            self.stop(power);
            return;
        }
        if power == FP_LIGHTNING {
            while self.force.lightning_debounce < level_time {
                self.shoot_lightning();
                self.drain(FP_LIGHTNING, 0);
                self.force.lightning_debounce += FORCE_DEBOUNCE_TIME;
            }
        } else {
            while self.force.drain_debounce < level_time {
                self.shoot_drain();
                self.force.drain_debounce += FORCE_DEBOUNCE_TIME;
            }
        }
    }

    /// `WP_ForcePowerStop` for lightning and drain: three seconds before the next at level
    /// 1, one and a half above; the hand comes in.
    pub(crate) fn stop_stream(&mut self, power: usize) {
        let level_time = self.frame.level_time;
        self.force.debounce[power] = level_time
            + if self.force.levels[power] < 2 {
                3_000
            } else {
                1_500
            };
        if self.field(PS_FORCE_HAND_EXTEND) == HANDEXTEND_FORCE_HOLD {
            *self.frame.hand_extend_time = 0;
        }
        self.state.set_raw_field(PS_ACTIVE_FORCE_PASS, 0);
    }

    /// The players in front of the user within `radius` (from the nearest point of each
    /// one's box), alive, not teammates, in its PVS and in its sight: `visit` for each in
    /// entity order, with the direction and the box's centre. `wants` filters first.
    fn arc(
        &mut self,
        radius: f32,
        forward: [f32; 3],
        wants: fn(&OtherPlayer<'_>) -> bool,
        visit: fn(&mut Self, u16, [f32; 3], [f32; 3]),
    ) {
        let center = self.origin();
        let (gametype, own_team, client) =
            (self.frame.gametype, self.frame.team, self.frame.client);
        for number in 0..self.frame.others.slots() {
            if number == client {
                continue;
            }
            let Some(player) = self.frame.others.player(number) else {
                continue;
            };
            let (absmin, absmax) = (player.absmin, player.absmax);
            // `EntitiesInBox`.
            if (0..3).any(|axis| {
                absmin[axis] > center[axis] + radius || absmax[axis] < center[axis] - radius
            }) {
                continue;
            }
            if *player.health <= 0 || same_team(gametype, own_team, player.team) || !wants(&player)
            {
                continue;
            }
            let nearest: [f32; 3] = std::array::from_fn(|axis| {
                if center[axis] < absmin[axis] {
                    absmin[axis] - center[axis]
                } else if center[axis] > absmax[axis] {
                    center[axis] - absmax[axis]
                } else {
                    0.0
                }
            });
            let size: [f32; 3] = std::array::from_fn(|axis| absmax[axis] - absmin[axis]);
            let middle: [f32; 3] = std::array::from_fn(|axis| absmin[axis] + 0.5 * size[axis]);
            let mut direction: [f32; 3] = std::array::from_fn(|axis| middle[axis] - center[axis]);
            normalize(&mut direction);
            if dot(direction, forward) < 0.5 || length(nearest) >= radius {
                continue;
            }
            if !self.frame.others.in_pvs(middle, center) {
                continue;
            }
            let sight = self.frame.others.trace(center, middle, client, MASK_SHOT);
            if sight.fraction < 1.0 && sight.entity_number != number {
                continue;
            }
            visit(self, number, direction, middle);
        }
    }

    /// `ForceShootLightning`: the arc at level 3, else the line of 2048 units.
    fn shoot_lightning(&mut self) {
        if *self.frame.health <= 0 {
            return;
        }
        let mut forward = self.aim();
        normalize(&mut forward);
        if self.force.levels[FP_LIGHTNING] > 2 {
            self.arc(
                FORCE_LIGHTNING_RADIUS,
                forward,
                |_| true,
                Self::lightning_damage,
            );
            return;
        }
        let origin = self.origin();
        let end: [f32; 3] = std::array::from_fn(|axis| origin[axis] + 2_048.0 * forward[axis]);
        let hit = self
            .frame
            .others
            .trace(origin, end, self.frame.client, MASK_SHOT);
        if hit.entity_number == ENTITY_NUMBER_NONE
            || hit.fraction == 1.0
            || hit.all_solid
            || hit.start_solid
        {
            return;
        }
        self.lightning_damage(hit.entity_number, forward, hit.end_position);
    }

    /// `ForceLightningDamage` on player `number`: fed instead of hurt while absorption's
    /// respite lasts; else one or two (absorption turns it down; two-handed at level 3
    /// doubles it), sometimes a hit sound, and electrified.
    fn lightning_damage(&mut self, number: u16, direction: [f32; 3], point: [f32; 3]) {
        let level_time = self.frame.level_time;
        self.unprotect();
        let Some(victim) = self.frame.others.player(number) else {
            return;
        };
        if victim.force.no_lightning_time >= level_time {
            let pool = (victim.state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32 + 1)
                .min(victim.force.max);
            victim.state.set_raw_field(PS_FORCE_POWER, pool as u32);
            return;
        }
        if !self.usable_on(number, FP_LIGHTNING) {
            return;
        }
        let mut damage = self.frame.others.rng().irand(1, 2);
        if let Some(left) = self.absorbed(number, FP_LIGHTNING, 1) {
            let (points, respite) = match left {
                0 => (0, 400),
                1 => (1, 300),
                _ => (1, 100),
            };
            damage = points;
            if let Some(victim) = self.frame.others.player(number) {
                victim.force.no_lightning_time = level_time + respite;
            }
        }
        if self.field(PS_WEAPON) == WP_MELEE && self.force.levels[FP_LIGHTNING] > 2 {
            damage *= 2;
        }
        if damage != 0 {
            self.blow(number, damage, Some(direction), Some(point), 0);
        }
        if self.frame.others.rng().irand(0, 2) == 0 {
            let name: &[u8] = match self.frame.others.rng().irand(1, 3) {
                1 => b"sound/weapons/force/lightninghit1",
                2 => b"sound/weapons/force/lightninghit2",
                _ => b"sound/weapons/force/lightninghit3",
            };
            let Some(origin) = self
                .frame
                .others
                .player(number)
                .map(|victim| victim.state.origin())
            else {
                return;
            };
            self.sound_at(origin, CHAN_BODY, name);
        }
        let Some(victim) = self.frame.others.player(number) else {
            return;
        };
        if (victim.state.raw_field(PS_ELECTRIFY_TIME).unwrap_or(0) as i32) < level_time + 400 {
            victim
                .state
                .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 800) as u32);
        }
    }

    /// `ForceShootDrain`: the arc at level 3 (those with Force to take), else the line;
    /// a shot that found nobody on the line costs nothing.
    fn shoot_drain(&mut self) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0 {
            return;
        }
        let mut forward = self.aim();
        normalize(&mut forward);
        if self.force.levels[FP_DRAIN] > 2 {
            let wants =
                |player: &OtherPlayer<'_>| player.state.raw_field(PS_FORCE_POWER).unwrap_or(0) != 0;
            self.arc(MAX_DRAIN_DISTANCE, forward, wants, Self::drain_damage);
        } else {
            let origin = self.origin();
            let end: [f32; 3] = std::array::from_fn(|axis| origin[axis] + 2_048.0 * forward[axis]);
            let hit = self
                .frame
                .others
                .trace(origin, end, self.frame.client, MASK_SHOT);
            if hit.entity_number == ENTITY_NUMBER_NONE
                || hit.fraction == 1.0
                || hit.all_solid
                || hit.start_solid
                || self.frame.others.player(hit.entity_number).is_none()
            {
                return;
            }
            self.drain_damage(hit.entity_number, forward, hit.end_position);
        }
        self.state.set_raw_field(
            PS_ACTIVE_FORCE_PASS,
            u32::from(self.force.levels[FP_DRAIN]) + 3,
        );
        self.drain(FP_DRAIN, 5);
        self.force.regen_debounce = level_time + 500;
    }

    /// `ForceDrainDamage` on player `number`, if it has Force: 2, 3 or 4 of it taken (what
    /// absorption leaves), the drainer healed by as much, the victim's regeneration held
    /// back, and the drained effect at most every 400 ms.
    fn drain_damage(&mut self, number: u16, direction: [f32; 3], point: [f32; 3]) {
        let level_time = self.frame.level_time;
        self.unprotect();
        let Some(victim) = self.frame.others.player(number) else {
            return;
        };
        let (team, pool) = (
            victim.team,
            victim.state.raw_field(PS_FORCE_POWER).unwrap_or(0),
        );
        if self.teammate(team) || pool == 0 {
            return;
        }
        if !self.usable_on(number, FP_DRAIN) {
            return;
        }
        let mut amount = [0, 2, 3, 4][usize::from(self.force.levels[FP_DRAIN].min(3))];
        if let Some(left) = self.absorbed(number, FP_DRAIN, 1) {
            amount = left.min(2);
        }
        let Some(victim) = self.frame.others.player(number) else {
            return;
        };
        let pool = (victim.state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32 - amount).max(0);
        victim.state.set_raw_field(PS_FORCE_POWER, pool as u32);
        victim.force.regen_debounce = level_time + 800;
        let drained = victim.force.sound_debounce < level_time;
        if drained {
            victim.force.sound_debounce = level_time + 400;
        }
        let max_health = self.state.stats[STAT_MAX_HEALTH] as i32;
        if (self.state.stats[STAT_HEALTH] as i32) < max_health
            && *self.frame.health > 0
            && self.state.stats[STAT_HEALTH] as i32 > 0
        {
            *self.frame.health = (*self.frame.health + amount).min(max_health);
            self.state.stats[STAT_HEALTH] = *self.frame.health as u32;
        }
        if drained {
            let mut event = EventEntity {
                event: EV_FORCE_DRAINED,
                parameter: u32::from(sjk_protocol::legacy_direction_to_byte(direction)),
                origin: point,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            event.extra[0] = (ES_OWNER, u32::from(number));
            self.raise(event);
        }
    }

    /// `G_Damage(victim, self, self, direction, point, amount, flags, MOD_FORCE_DARK)`.
    fn blow(
        &mut self,
        number: u16,
        amount: i32,
        direction: Option<[f32; 3]>,
        point: Option<[f32; 3]>,
        flags: u32,
    ) {
        let attacker = crate::damage::Attacker {
            npc: self.frame.npc,
            client: self.frame.client,
            max_health: self.state.stats[STAT_MAX_HEALTH] as i32,
            team: self.frame.team,
            saber_knockback: [0.0; 4],
        };
        let request = DamageRequest {
            level_time: self.frame.level_time,
            attacker: Some(attacker),
            direction,
            point,
            damage: amount,
            flags,
            means: MOD_FORCE_DARK,
        };
        self.frame.others.damage(number, request);
    }
}

/// `OnSameTeam` for players: teammates in the team games; nobody in the others.
pub(crate) fn same_team(gametype: i32, one: i32, other: i32) -> bool {
    gametype >= 6 && one == other
}

/// `InFront` (`NPC_senses.c:103-119`): `spot` within the cone of `threshold` before
/// `from`, looking along `angles`' yaw, level.
fn in_front(spot: [f32; 3], from: [f32; 3], angles: [f32; 3], threshold: f32) -> bool {
    let mut direction = [spot[0] - from[0], spot[1] - from[1], 0.0];
    normalize(&mut direction);
    let forward = crate::pmove::flight::flight_axes([0.0, angles[1], angles[2]])
        .0
        .to_array();
    dot(direction, forward) > threshold
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

//! An NPC's saber out of its hand (OpenJK `codemp/game/w_saber.c`): the throw an NPC's
//! `BUTTON_ALT_ATTACK` began in its move started from its hand (`WP_SaberPositionUpdate`,
//! `:8617-8726`), the saber entity's flight out and back and the catch (`saberFirstThrown`,
//! `saberBackToOwner`, `:6927-7262`), what it cuts on the way (`CheckThrownSaberDamaged`,
//! `:5856-6133`), and a saber knocked out of an NPC's hand lying until called back
//! (`DownedSaberThink`, `:6334-6502`).
//!
//! The reference gives an NPC the player's saber code whole, so the rules are the players'
//! ([`crate::saber_throw`], [`crate::saber_drop`]); this is where the NPCs' world meets
//! them ([`NpcFlight`]). The saber entity's wire state is the host's (its entity slot),
//! taken out for the flight and given back ([`crate::npc_spawn::NpcHost::take_entity_state`]).

use crate::damage::{Attacker, DamageRequest};
use crate::entity_clip::BoxObstacle;
use crate::event_entity::EventEntity;
use crate::npc_damage::NpcBlow;
use crate::npc_saber::{CONTENTS_LIGHTSABER, npc_fighter};
use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::player_death::Rng;
use crate::pmove::{MovementCollision, MovementTrace};
use crate::saber_block::Defender;
use crate::saber_throw::{
    FlightTarget, Flown, Saber, SaberFlight, SaberLook, SaberOwner, SaberThink,
};
use std::cell::RefCell;

/// `FP_SABER_OFFENSE`, `FP_SABER_DEFENSE`, `FP_SABERTHROW`; `TEAM_SPECTATOR`.
const FP_SABER_OFFENSE: usize = 15;
const FP_SABER_DEFENSE: usize = 16;
const FP_SABERTHROW: usize = 17;
const TEAM_SPECTATOR: i32 = 3;
/// `CONTENTS_BODY`; `ENTITYNUM_NONE`.
const CONTENTS_BODY: u32 = 0x100;
const ENTITY_NONE: u16 = 1_023;
/// `PERS_HITS`, `PERS_ATTACKEE_ARMOR`; `STAT_MAX_HEALTH`.
const PERS_HITS: usize = 1;
const PERS_ATTACKEE_ARMOR: usize = 7;
const STAT_MAX_HEALTH: usize = 8;
/// `MASK_PLAYERSOLID | CONTENTS_LIGHTSABER`: a lit saber entity's `clipmask`.
const SABER_CLIP_MASK: u32 = 0x1 | 0x10 | 0x100 | 0x1000 | CONTENTS_LIGHTSABER;

/// The NPCs' world as the thrown saber of the NPC at `me` sees it.
pub(crate) struct NpcFlight<'w, 'a, H: NpcHost> {
    world: &'w mut NpcWorld<'a, H>,
    me: usize,
    /// `client->dangerTime`, `invulnerableTimer`: an NPC's, which nothing reads.
    danger_time: i32,
    invulnerable: i32,
}

/// Its level of a Force power, as a byte.
fn level(npc: &NpcActor, power: usize) -> u8 {
    npc.force_levels
        .get(power)
        .copied()
        .unwrap_or(0)
        .clamp(0, 255) as u8
}

/// The NPC as the attacker its thrown saber's blows name.
pub(crate) fn npc_attacker(npc: &NpcActor) -> Attacker {
    let sabers = &npc.definition.sabers;
    Attacker {
        npc: true,
        client: npc.number,
        max_health: npc.player.stats[STAT_MAX_HEALTH] as i32,
        team: npc.session_team,
        saber_knockback: [
            sabers[0].knockback_scale[0],
            sabers[0].knockback_scale[1],
            sabers[1].knockback_scale[0],
            sabers[1].knockback_scale[1],
        ],
    }
}

impl<H: NpcHost> NpcFlight<'_, '_, H> {
    fn owner_number(&self) -> u16 {
        self.world.actors[self.me].number
    }

    /// The bodies a trace passing `pass` meets: every NPC's but `pass`'s (the host skips
    /// it), the lit saber entities of the others, and the players' (the host's).
    fn gather(&mut self, pass: u16) {
        let owner = self.owner_number();
        let NpcWorld {
            actors,
            bodies,
            host,
            ..
        } = &mut *self.world;
        bodies.clear();
        bodies.extend(
            actors
                .iter()
                .filter(|npc| npc.contents != 0)
                .map(NpcActor::body),
        );
        for npc in actors
            .iter()
            .filter(|npc| npc.number != owner && npc.saber.entity_solid())
        {
            let Some(saber) = npc.saber_entity else {
                continue;
            };
            let entity = &npc.saber.entity;
            bodies.push(BoxObstacle {
                entity: saber,
                origin: entity.origin,
                bounds: (entity.mins, entity.maxs),
                contents: CONTENTS_LIGHTSABER,
                model: None,
            });
        }
        host.player_saber_boxes(bodies);
        let _ = pass;
    }
}

/// `trap->Trace` through the host for a missile's run, the bodies gathered.
struct HostTrace<'x, H: NpcHost> {
    host: RefCell<&'x mut H>,
    bodies: &'x [BoxObstacle],
    pass: u16,
}

impl<H: NpcHost> MovementCollision for HostTrace<'_, H> {
    fn trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        self.host
            .borrow_mut()
            .trace(start, mins, maxs, end, self.pass, mask, self.bodies)
    }
}

impl<H: NpcHost> SaberFlight for NpcFlight<'_, '_, H> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
    ) -> MovementTrace {
        self.gather(pass);
        let NpcWorld { bodies, host, .. } = &mut *self.world;
        host.trace(start, mins, maxs, end, pass, mask, bodies)
    }

    fn in_pvs(&mut self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.world.host.in_pvs(from, to)
    }

    fn owner(&mut self) -> Option<SaberOwner<'_>> {
        let gametype = self.world.host.gametype();
        let npc = &mut self.world.actors[self.me];
        Some(SaberOwner {
            number: npc.number,
            health: npc.health,
            spectator: npc.session_team == TEAM_SPECTATOR,
            // `SaberUpdateSelf` spares an NPC without the attack level; the flight does not.
            offense: level(npc, FP_SABER_OFFENSE),
            throw_level: level(npc, FP_SABERTHROW),
            buttons: npc.mind.command.buttons,
            command_buttons: npc.mind.command.buttons,
            storage: npc.saber.storage,
            gametype,
            team: npc.session_team,
            memory: &mut npc.saber.throw,
            throw_delay: &mut npc.saber.throw_delay,
            attack_wound: &mut npc.saber.attack_wound,
            danger_time: &mut self.danger_time,
            invulnerable_until: &mut self.invulnerable,
            state: &mut npc.player,
        })
    }

    fn entity_count(&self) -> u16 {
        let npcs = self
            .world
            .actors
            .iter()
            .map(|npc| npc.number.max(npc.saber_entity.unwrap_or(0)) + 1)
            .max()
            .unwrap_or(0);
        let players = self
            .world
            .host
            .players()
            .iter()
            .map(|player| player.number + 1)
            .max()
            .unwrap_or(0);
        npcs.max(players)
    }

    fn target(&mut self, number: u16) -> Option<FlightTarget> {
        if let Some(at) = self.world.actor_at(number) {
            let npc = &self.world.actors[at];
            return Some(FlightTarget {
                client: true,
                origin: npc.player.origin(),
                takes_damage: npc.takes_damage,
                health: npc.health,
                spectator: npc.session_team == TEAM_SPECTATOR,
                contents: npc.contents,
                owner: ENTITY_NONE,
                ..FlightTarget::default()
            });
        }
        if let Some(player) = self
            .world
            .host
            .players()
            .iter()
            .find(|player| player.number == number)
        {
            let contents = if player.health > 0 && !player.spectating {
                CONTENTS_BODY
            } else {
                0
            };
            return Some(FlightTarget {
                client: true,
                origin: player.origin,
                takes_damage: !player.spectating,
                health: player.health,
                spectator: player.spectating,
                contents,
                owner: ENTITY_NONE,
                ..FlightTarget::default()
            });
        }
        // A saber entity: an NPC's, a blade while lit in hand; a player's, the host's.
        if let Some(npc) = self
            .world
            .actors
            .iter()
            .find(|npc| npc.saber_entity == Some(number))
        {
            let contents = if npc.saber.flight.think == SaberThink::InHand {
                if npc.saber.entity_solid() {
                    CONTENTS_LIGHTSABER
                } else {
                    0
                }
            } else {
                npc.saber.flight.contents
            };
            return Some(FlightTarget {
                origin: npc.saber.entity.origin,
                contents,
                owner: npc.number,
                ..FlightTarget::default()
            });
        }
        let fighter = self.world.host.player_saber(number)?;
        Some(FlightTarget {
            contents: CONTENTS_LIGHTSABER,
            owner: fighter.number,
            ..FlightTarget::default()
        })
    }

    fn defence(&mut self, number: u16) -> Option<u8> {
        match self.world.actor_at(number) {
            Some(at) => Some(level(&self.world.actors[at], FP_SABER_DEFENSE)),
            None => self.world.host.player_saber_defense(number),
        }
    }

    fn block(&mut self, number: u16, point: [f32; 3]) -> bool {
        let level_time = self.world.level_time;
        let Some(at) = self.world.actor_at(number) else {
            return self.world.host.player_blocks_thrown(number, point);
        };
        let npc = &mut self.world.actors[at];
        let (saber_blocking, command, defense) = (
            npc.movement.state().saber_blocking,
            npc.mind.command,
            level(npc, FP_SABER_DEFENSE),
        );
        let mut defender = Defender {
            client: npc.number,
            state: &mut npc.player,
            saber_blocking,
            buttons: command.buttons,
            forward_move: command.forward_move,
            defense,
            block_time: &mut npc.saber.block_time,
        };
        defender.can_block_blow(point, level_time)
    }

    fn view_angles(&mut self, number: u16) -> Option<[f32; 3]> {
        self.world.body(number).map(|body| body.view_angles)
    }

    /// `SetSaberBoxSize` from the NPC's blades as its damage loop last stored them.
    fn saber_box(&mut self, current: [f32; 3]) -> ([f32; 3], [f32; 3]) {
        let owner = npc_fighter(&self.world.actors[self.me]);
        crate::saber_clash::saber_box(
            &owner,
            crate::saber_rules::super_break_lose(owner.torso),
            current,
            self.world.level_time,
        )
    }

    fn run_missile(
        &mut self,
        missile: &mut crate::weapon_fire::Missile,
        level_time: i32,
        previous_time: i32,
    ) {
        let owner = self.owner_number();
        self.gather(owner);
        let NpcWorld {
            actors,
            bodies,
            host,
            ..
        } = &mut *self.world;
        let npcs: Vec<u16> = actors.iter().map(|npc| npc.number).collect();
        let is_npc = |number: u16| npcs.contains(&number);
        let mut rng = *host.rng();
        let world = HostTrace {
            host: RefCell::new(&mut **host),
            bodies,
            pass: owner,
        };
        // A knocked saber's bounce sounds (`G_BounceMissile`), raised as it bounces.
        let mut sounds = |name: &[u8]| world.host.borrow_mut().sound_index(name);
        let mut raise = |event: EventEntity| world.host.borrow_mut().raise(event);
        let mut frame = crate::weapon_fire::MissileFrame {
            homing: &crate::weapon_fire::NoTargets,
            rng: &mut rng,
            sounds: &mut sounds,
            raise: &mut raise,
            models: None,
            npcs: &is_npc,
        };
        let _ = crate::weapon_fire::run_missile_against(
            missile,
            level_time,
            previous_time,
            &world,
            &mut |_, _, _| None,
            &mut frame,
        );
        *host.rng() = rng;
    }

    fn spawn_dead_saber(&mut self, missile: crate::weapon_fire::Missile) {
        let _ = self.world.host.launch(missile);
    }

    fn hurt(&mut self, target: u16, request: DamageRequest) {
        let me = self.me;
        let request = DamageRequest {
            attacker: Some(npc_attacker(&self.world.actors[me])),
            ..request
        };
        let number = self.owner_number();
        let (hits, armor) = if let Some(at) = self.world.actor_at(target) {
            self.world.host.noting_damage(target, number, &request);
            let damaged = self.world.damage(
                at,
                NpcBlow {
                    request,
                    spared_by_master: false,
                    surface: None,
                },
            );
            (damaged.attacker_hits, damaged.attackee_armor)
        } else if self
            .world
            .host
            .players()
            .iter()
            .any(|player| player.number == target)
        {
            self.world.host.saber_blow_on_player(target, request)
        } else {
            self.world.host.saber_blow_on_entity(target, request);
            (0, None)
        };
        if hits != 0 {
            let persistent = &mut self.world.actors[me].player.persistent;
            persistent[PERS_HITS] = (persistent[PERS_HITS] as i32 + hits) as u32;
            persistent[PERS_ATTACKEE_ARMOR] = armor.unwrap_or(0);
        }
    }

    fn raise(&mut self, event: EventEntity) {
        self.world.host.raise(event);
    }

    /// The NPC's sabers as their definitions give them.
    fn owner_saber(&mut self) -> SaberLook {
        let [first, second] = &self.world.actors[self.me].definition.sabers;
        let model = if first.model.is_empty() {
            crate::saber_throw::SABER_MODEL.to_vec()
        } else {
            first.model.clone()
        };
        let (on, hum, off, spin) = (
            [first.sound_on, second.sound_on],
            first.sound_loop,
            first.sound_off,
            first.spin_sound,
        );
        SaberLook {
            model: self.world.host.model_index(&model),
            on,
            hum,
            off,
            spin,
        }
    }

    fn owner_first_saber(&self) -> Option<&crate::saber_definition::SaberDefinition> {
        Some(&self.world.actors[self.me].definition.sabers[0])
    }

    fn sound_index(&mut self, name: &[u8]) -> u16 {
        self.world.host.sound_index(name)
    }

    fn model_index(&mut self, name: &[u8]) -> u16 {
        self.world.host.model_index(name)
    }

    fn rng(&mut self) -> &mut Rng {
        self.world.host.rng()
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `run` on the saber of the NPC at `me` with the NPCs' world as its world: its wire
    /// state taken from the host and given back (sent while it flies), and the saber
    /// entity the other blades meet kept where the flight left it.
    pub(crate) fn with_npc_saber<T>(
        &mut self,
        me: usize,
        run: impl FnOnce(&mut Saber, &mut dyn SaberFlight) -> T,
    ) -> Option<T> {
        let number = self.actors[me].saber_entity?;
        let mut state = self.host.take_entity_state(number)?;
        let mut entity = self.actors[me].saber.flight;
        let (was, event_time) = (entity.think, entity.event_time);
        let result = run(
            &mut Saber {
                number,
                entity: &mut entity,
                state: &mut state,
            },
            &mut NpcFlight {
                world: &mut *self,
                me,
                danger_time: 0,
                invulnerable: 0,
            },
        );
        self.host.put_entity_state(number, state, entity.shown);
        if entity.event_time != event_time {
            self.host.entity_event(number, entity.event_time);
        }
        let npc = &mut self.actors[me];
        // `ps.saberEntityState` as the Jedi AI reads it (`SES_LEAVING` while out).
        npc.mind.saber_entity_state = i32::from(npc.saber.throw.started);
        let saber = &mut npc.saber;
        saber.flight = entity;
        // Out of the hand, or just back in it (`SaberUpdateSelf` goes on from where the
        // flight left it): the saber entity the others meet is the flight's.
        if entity.think != SaberThink::InHand || was != SaberThink::InHand {
            saber.entity.origin = entity.current;
            (saber.entity.mins, saber.entity.maxs, saber.entity.contents) =
                (entity.mins, entity.maxs, entity.contents);
            saber.entity.linked = true;
            saber.entity.think_at = entity.next_think;
        }
        Some(result)
    }

    /// `WP_SaberPositionUpdate` for the NPC at `me` whose saber is out of its hand: a throw
    /// its move began starts from the hand at `bolt_origin`, facing `bolt_direction` with
    /// the view's yaw; a flying saber learns where the hand is.
    pub(crate) fn throw_update(
        &mut self,
        me: usize,
        bolt_origin: [f32; 3],
        bolt_direction: [f32; 3],
    ) {
        let level_time = self.level_time;
        let yaw = self.actors[me].player.view_angles()[1];
        let angles = [bolt_direction[0], yaw, bolt_direction[2]];
        let _ = self.with_npc_saber(me, |saber, world| {
            crate::saber_throw::owner_update(saber, world, bolt_origin, angles, level_time)
        });
    }

    /// `G_RunThink` for the saber entity of the NPC at `me` while it is out of the hand
    /// (`saberFirstThrown`, `saberBackToOwner`, a knocked saber's fall and lying).
    pub(crate) fn saber_flight_think(&mut self, me: usize) {
        let level_time = self.level_time;
        let previous_time = level_time - 50;
        let flown = self.with_npc_saber(me, |saber, world| {
            crate::saber_throw::run_think(saber, world, level_time, previous_time)
        });
        if flown == Some(Flown::Freed) {
            // Its owner is gone: the saber entity with it.
            let saber = &mut self.actors[me].saber;
            saber.flight.think = SaberThink::InHand;
            saber.flight.shown = false;
        }
    }

    /// `G_TouchTriggers` for the NPC at `me` (`g_active.c:531-600`), as far as the knocked
    /// sabers go: each one its box is in contact with, in entity order, stands its angles
    /// upright (`SaberBounceSound`) — an NPC's here, a player's through the host. A dead NPC
    /// touches nothing.
    pub(crate) fn touch_downed_sabers(&mut self, me: usize) {
        let npc = &self.actors[me];
        if npc.player.stats[0] as i32 <= 0 {
            return;
        }
        let origin = npc.player.origin();
        let bounds = npc.movement.box_bounds();
        self.host.touch_player_sabers(origin, bounds);
        for &at in self.order {
            if crate::saber_drop::touched_by(&self.actors[at].saber.flight, origin, bounds) {
                let _ = self.with_npc_saber(at, |saber, _| crate::saber_drop::bounce_sound(saber));
            }
        }
    }

    /// The thrown saber's blade (`w_saber.c:8848-8888`), read from its wire state: where
    /// it flies, along its spin going out, aimed at the NPC coming back.
    pub(crate) fn thrown_blade(
        &mut self,
        me: usize,
    ) -> Option<(crate::server_skeleton::Blade, bool)> {
        let number = self.actors[me].saber_entity?;
        let state = self.host.take_entity_state(number)?;
        let origin = self.actors[me].current_origin;
        let (base, direction) = crate::saber_throw::thrown_blade(&state, origin, self.level_time);
        let going_out = state
            .raw_field(crate::saber_throw::ES_SABER_IN_FLIGHT)
            .unwrap_or(0)
            != 0;
        let shown = self.actors[me].saber.flight.shown;
        self.host.put_entity_state(number, state, shown);
        Some((crate::server_skeleton::Blade { base, direction }, going_out))
    }
}

/// `SaberUpdateSelf`'s part the throw reads (`w_saber.c:320-366`): flagged
/// (`PROPER_THROWN_VALUE`) while the NPC has thrown it and lives; else its `clipmask`
/// with its contents — none unless `lit`, the blades' once solid.
pub(crate) fn in_hand_flags(npc: &mut NpcActor, thrown: bool, lit: bool) {
    let flight = &mut npc.saber.flight;
    if thrown {
        flight.value5 = 999;
        return;
    }
    flight.value5 = 0;
    if !lit {
        flight.clip_mask = 0;
    } else if npc.saber.entity.contents == CONTENTS_LIGHTSABER {
        flight.clip_mask = SABER_CLIP_MASK;
    }
}

/// `WP_SaberInitBladeData`'s wire state for an NPC's saber entity in hand, through the
/// host: not drawn, its Ghoul2 instance the hilt's.
pub(crate) fn init_saber_state(host: &mut impl NpcHost, number: u16) {
    if let Some(mut state) = host.take_entity_state(number) {
        crate::saber_throw::in_hand_state(&mut state);
        host.put_entity_state(number, state, false);
    }
}

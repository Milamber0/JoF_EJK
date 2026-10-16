//! The map's emplaced guns on this server ([`sjk_game_jka::emplaced`], `SP_emplaced_gun`
//! and its thinks): spawned with the level, taken with the use key, fired by their
//! gunner's fire events, thinking every 50 ms in the frame before the missiles, solid in
//! everybody's way but their gunner's and their own bolts', hurt by missiles, splash and
//! the world as any damageable thing, and sent to legacy clients as a stock server sends
//! them: a Ghoul2 `turret_chair` with its box packed into `solid`, its health bar, its
//! gunner in `emplacedOwner` and each shot's `EV_FIRE_WEAPON`.
//!
//! A gun's bolt kills for its gunner (`player_die`'s `WP_TURRET` rule): the gun is the
//! blow's attacker, its gunner the one credited ([`Mounted::credited`]).

use super::super::*;
use sjk_game_jka::emplaced::{self, Blast, EmplacedGun, GunThink, Gunner, Used};
use sjk_game_jka::try_heal::{Heal, try_heal};

/// `WP_EMPLACED_GUN`; `GT_SIEGE`; `MOD_UNKNOWN`; `EV_GENERAL_SOUND`; `ES_SOLID`,
/// `ES_EVENT`.
const WP_EMPLACED_GUN: u8 = 17;
const GT_SIEGE: i32 = 7;
const MOD_UNKNOWN: u32 = 0;
const EV_GENERAL_SOUND: u32 = 76;
const ES_SOLID: usize = 26;
const ES_EVENT: usize = 28;
/// `EVENT_VALID_MSEC`.
const EVENT_VALID_MSEC: i32 = 300;
/// `SETANIM_TORSO`, `SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD`.
const SETANIM_TORSO: u8 = 2;
const OVERRIDE_HOLD: u8 = 1 | 2;

/// The level's guns and what their shots are credited to.
#[derive(Default)]
pub(crate) struct Mounted {
    /// The guns, by native identity; each keeps its wire number.
    pub(crate) guns: Vec<(EntityId, EmplacedGun)>,
    /// While a gun's bolt strikes: the gun and its gunner.
    credit: Option<(u16, u16)>,
    /// The guns' boxes as a blast's lines meet them; kept to be reused.
    solids: Vec<BoxObstacle>,
}

impl Mounted {
    /// Who a kill by `attacker` is credited to: a gun's bolt's gunner, else `attacker`.
    pub(crate) fn credited(&self, attacker: u16) -> u16 {
        match self.credit {
            Some((gun, gunner)) if gun == attacker => gunner,
            _ => attacker,
        }
    }

    /// The guns as solid boxes in the way of `passer`'s traces — all but the gun it is
    /// (a gun's own bolt) and the one it is owned by (a gunner's): `SV_ClipMoveToEntities`.
    pub(crate) fn obstacles(
        &self,
        passer: u16,
        passer_owner: u16,
    ) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.guns
            .iter()
            .filter(move |(_, gun)| gun.number != passer && gun.number != passer_owner)
            .map(|(_, gun)| BoxObstacle {
                entity: gun.number,
                origin: gun.origin,
                bounds: emplaced::BOUNDS,
                contents: emplaced::CONTENTS_SOLID,
                model: None,
            })
    }

    /// The guns a blast reaches, with their linked boxes (`takedamage` stays on).
    pub(crate) fn splash_targets(&self) -> impl Iterator<Item = SplashTarget> + '_ {
        self.guns.iter().map(|(_, gun)| {
            let bounds = (
                std::array::from_fn(|axis| gun.origin[axis] + emplaced::BOUNDS.0[axis] - 1.0),
                std::array::from_fn(|axis| gun.origin[axis] + emplaced::BOUNDS.1[axis] + 1.0),
            );
            SplashTarget {
                number: gun.number,
                bounds,
                origin: gun.origin,
                takes_damage: gun.takes_damage,
            }
        })
    }

    fn index(&self, number: u16) -> Option<usize> {
        self.guns.iter().position(|(_, gun)| gun.number == number)
    }
}

impl NativeGame {
    /// `SP_emplaced_gun` for every gun the map places in this game type, the weapon's
    /// item registered in `CS_ITEMS`; the guns placed before (another game type's) freed
    /// and their gunners let go.
    pub(in super::super) fn spawn_emplaced(&mut self) {
        let level_time = self.last_frame_time;
        for (id, gun) in std::mem::take(&mut self.mounted.guns) {
            self.pool.free(id, level_time);
            if let Some(peer) = gun
                .activator
                .and_then(|user| self.peer_mut(usize::from(user)))
            {
                peer.riding.owner = sjk_protocol::ENTITY_NUMBER_NONE;
                peer.state.set_raw_field(112, 0);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        self.mounted = Mounted::default();
        let Some(map) = self.map.take() else { return };
        for entity in map.entities.iter().filter(|entity| {
            entity
                .classname()
                .is_some_and(|name| name.eq_ignore_ascii_case("emplaced_gun"))
        }) {
            if !sjk_game_jka::bot_routes::spawns_in(entity, self.gametype) {
                continue;
            }
            let Some(id) = self.pool.spawn_entity(
                EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
                level_time,
            ) else {
                break;
            };
            let number = id.legacy_number();
            let (models, told) = (&mut self.models, &mut self.told);
            let model = models.index(emplaced::MODEL, &mut |index, value| {
                told.push(Told::ConfigString {
                    index,
                    previous: Vec::new(),
                    value: value.to_vec(),
                })
            });
            let world = WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            };
            let gun = emplaced::spawn(
                entity,
                number,
                model,
                level_time,
                |start, mins, maxs, end| world.trace(start, mins, maxs, end, emplaced::MASK_SOLID),
            );
            // `G_SpawnGEntityFromSpawnVars` precaches a `healingsound`.
            if let Some(sound) = gun.healing.sound.clone() {
                let (sounds, told) = (&mut self.sounds, &mut self.told);
                sounds.index(&sound, &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                });
            }
            self.mounted.guns.push((id, gun));
            self.publish_gun(self.mounted.guns.len() - 1);
        }
        self.map = Some(map);
        if let Some(item) = sjk_game_jka::items::find(emplaced::ITEM_CLASSNAME)
            .filter(|_| !self.mounted.guns.is_empty())
        {
            let mut present = self
                .config_strings
                .iter()
                .find(|(index, _)| *index == CS_ITEMS)
                .map(|(_, value)| value.clone())
                .unwrap_or_default();
            if present.get(item) == Some(&b'0') {
                present[item] = b'1';
                self.publish_config_string(CS_ITEMS, &present);
            }
        }
    }

    /// The gun at `index` as a legacy client is sent it: its wire state copied field by
    /// field into its slot, its box packed into `solid` as `SV_LinkEntity` packs it, linked
    /// with its box.
    fn publish_gun(&mut self, index: usize) {
        let (id, gun) = &self.mounted.guns[index];
        let id = *id;
        let Some(slot) = self.pool.state_mut(id) else {
            return;
        };
        for field in 0..sjk_protocol::LEGACY_ENTITY_FIELDS.len() {
            slot.set_raw_field(field, gun.state.raw_field(field).unwrap_or(0));
        }
        let byte = |value: f32| (value as i32).clamp(1, 255) as u32;
        slot.set_raw_field(
            ES_SOLID,
            byte(emplaced::BOUNDS.1[2] + 32.0) << 16
                | byte(-emplaced::BOUNDS.0[2]) << 8
                | byte(emplaced::BOUNDS.1[0]),
        );
        self.pool.set_bounds(id, emplaced::BOUNDS);
    }

    /// `G_RunFrame` for the guns (`G_RunThink`): an event shown long enough taken off,
    /// then `emplaced_gun_update` when its time has come — its blast dealt between the two
    /// halves of it — and its gunner's movement told what it changed.
    pub(in super::super) fn run_emplaced(&mut self, level_time: i32) {
        if self.mounted.guns.is_empty() {
            return;
        }
        for index in 0..self.mounted.guns.len() {
            let gun = &mut self.mounted.guns[index].1;
            if level_time - gun.event_time > EVENT_VALID_MSEC
                && gun.state.raw_field(ES_EVENT) != Some(0)
            {
                gun.state.set_raw_field(ES_EVENT, 0);
            }
            if gun.next_think > 0 && gun.next_think <= level_time {
                gun.next_think = 0;
                let (number, user) = (gun.number, gun.activator);
                let blast = {
                    let Self {
                        mounted,
                        deaths,
                        pool,
                        ..
                    } = &mut *self;
                    let mut irand = |low, high| deaths.rng.irand(low, high);
                    let mut raise = |event: sjk_game_jka::event_entity::EventEntity| {
                        let _ = pool.spawn_temporary(event.state(), level_time, None);
                    };
                    emplaced::update_dying(
                        &mut mounted.guns[index].1,
                        level_time,
                        &mut GunThink {
                            irand: &mut irand,
                            raise: &mut raise,
                        },
                    )
                };
                if let Some(blast) = blast {
                    self.gun_blast(number, user, blast, level_time);
                }
                let Self {
                    server,
                    world,
                    players,
                    mounted,
                    deaths,
                    pool,
                    ..
                } = &mut *self;
                let peer = user
                    .and_then(|user| players.at(usize::from(user)))
                    .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
                let gunner = peer.map(|peer| {
                    let entity_weapon = peer.entity.state().raw_field(14).unwrap_or(0);
                    Gunner {
                        number: user.unwrap_or(0),
                        state: &mut peer.state,
                        emplaced_time: &mut peer.riding.emplaced_time,
                        owner: &mut peer.riding.owner,
                        buttons: peer.last_command.buttons,
                        entity_weapon,
                        in_use: true,
                    }
                });
                let mut irand = |low, high| deaths.rng.irand(low, high);
                let mut raise = |event: sjk_game_jka::event_entity::EventEntity| {
                    let _ = pool.spawn_temporary(event.state(), level_time, None);
                };
                emplaced::update_gunner(
                    &mut mounted.guns[index].1,
                    level_time,
                    &mut GunThink {
                        irand: &mut irand,
                        raise: &mut raise,
                    },
                    gunner,
                );
                if let Some(peer) = user.and_then(|number| self.peer_mut(usize::from(number))) {
                    peer.movement = peer.movement.reseeded(&peer.state);
                }
            }
            self.publish_gun(index);
        }
    }

    /// `TryUse` reaching gun `found` (`ValidUseTarget`, `GlobalUse`): the use pose — its
    /// time the weapon's — then `emplaced_gun_use`, or `TryHeal` for a player facing
    /// another way. Whether `found` is a gun.
    pub(in super::super) fn use_gun(&mut self, client: usize, found: u16, level_time: i32) -> bool {
        let Some(index) = self.mounted.index(found) else {
            return false;
        };
        let (gametype, allied) = (self.gametype, self.mounted.guns[index].1.teams.allied);
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        // "nothing can be used" by a siege team the gun is allied to — it is the other's to
        // take (`TryUse`'s `alliedTeam` rule, with `g_ff_objectives` 0).
        if gametype == GT_SIEGE && allied != 0 && allied == peer.session.team {
            return true;
        }
        if matches!(
            peer.state.torso_animation(),
            sjk_game_jka::try_heal::BOTH_BUTTON_HOLD | sjk_game_jka::try_heal::BOTH_CONSOLE1
        ) {
            peer.state.set_raw_field(20, 500);
        } else {
            peer.movement.set_animation_parts(
                SETANIM_TORSO,
                sjk_game_jka::try_heal::BOTH_BUTTON_HOLD,
                OVERRIDE_HOLD,
            );
            peer.movement.write_player_state(&mut peer.state);
        }
        let torso_timer = peer.state.torso_timer();
        peer.state.set_raw_field(10, torso_timer as u32);
        let Self {
            server,
            world,
            players,
            mounted,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return true;
        };
        let entity_weapon = peer.entity.state().raw_field(14).unwrap_or(0);
        let gunner = Gunner {
            number: client as u16,
            state: &mut peer.state,
            emplaced_time: &mut peer.riding.emplaced_time,
            owner: &mut peer.riding.owner,
            buttons: peer.last_command.buttons,
            entity_weapon,
            in_use: true,
        };
        let used = emplaced::use_gun(&mut mounted.guns[index].1, gunner, level_time);
        if used == Used::TryHeal {
            self.heal_gun(client, index, level_time);
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        self.publish_gun(index);
        true
    }

    /// `TryHeal` on the gun at `index`: a siege class's repair, its sound on the gun, the
    /// repairer held in the use pose.
    fn heal_gun(&mut self, client: usize, index: usize, level_time: i32) {
        let gametype = self.gametype;
        // `bgSiegeClasses[client->siegeClass].name`: the class the player plays.
        let class = self
            .siege_class_of(client)
            .map(|class| class.name)
            .unwrap_or_default();
        let gun = &mut self.mounted.guns[index].1;
        let (mut health, max) = (gun.health, gun.max_health);
        let healed = try_heal(
            &mut gun.healing,
            &mut health,
            max,
            (!class.is_empty()).then_some(class.as_bytes()),
            gametype,
            level_time,
        );
        let Heal::Yes { repaired } = healed else {
            return;
        };
        gun.health = health;
        if repaired {
            gun.state.set_raw_field(69, health.max(0) as u32);
            if let Some(sound) = gun.healing.sound.clone() {
                let origin = gun.origin;
                let (sounds, told) = (&mut self.sounds, &mut self.told);
                let index = sounds.index(&sound, &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                });
                let event = sjk_game_jka::event_entity::EventEntity {
                    event: EV_GENERAL_SOUND,
                    parameter: u32::from(index),
                    origin,
                    client: None,
                    broadcast: false,
                    extra: [(0, 0); 12],
                };
                let _ = self.pool.spawn_temporary(event.state(), level_time, None);
            }
        }
        if let Some(peer) = self.peer_mut(client) {
            match sjk_game_jka::try_heal::healer_pose(peer.state.torso_animation()) {
                Some(pose) => {
                    peer.movement
                        .set_animation_parts(SETANIM_TORSO, pose, OVERRIDE_HOLD);
                    peer.movement.write_player_state(&mut peer.state);
                }
                None => {
                    peer.state.set_raw_field(20, 500);
                }
            }
        }
    }

    /// `FireWeapon` for a gunner (`g_weapon.c:4509-4538`, `WP_FireEmplaced`): the gun it
    /// is on fires; the bolt joins the missiles in flight.
    pub(in super::super) fn fire_emplaced(
        &mut self,
        client: usize,
        alternate: bool,
        level_time: i32,
    ) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (on, view) = (peer.state.emplaced_index(), peer.state.view_angles());
        if peer.state.weapon() != WP_EMPLACED_GUN || on == 0 {
            return;
        }
        let Some(index) = self.mounted.index(on) else {
            return;
        };
        let Some(missile) = emplaced::fire(
            &mut self.mounted.guns[index].1,
            client as u16,
            view,
            level_time,
            alternate,
        ) else {
            return;
        };
        if let Some(id) = self.pool.spawn_entity(missile.state.clone(), level_time) {
            self.pool.set_bounds(id, missile.bounds);
            self.missiles.push((id, missile));
        }
        self.publish_gun(index);
    }

    /// `G_Damage` on gun `number` by `attacker` (siege's `teamnodmg`: a team's blows do
    /// the gun no harm, nor another gun's of that team unless its gunner is of another).
    /// Whether `number` is a gun.
    pub(in super::super) fn hurt_gun(
        &mut self,
        number: u16,
        damage: i32,
        attacker: Option<u16>,
        level_time: i32,
    ) -> bool {
        let Some(index) = self.mounted.index(number) else {
            return false;
        };
        let no_damage = self.mounted.guns[index].1.teams.no_damage;
        if self.gametype == GT_SIEGE
            && no_damage != 0
            && let Some(attacker) = attacker
        {
            if let Some(peer) = self.peer(usize::from(attacker)) {
                if peer.session.team == no_damage {
                    return true;
                }
            } else if let Some(other) = self.mounted.index(attacker) {
                let other = &self.mounted.guns[other].1;
                let user_team = other
                    .activator
                    .and_then(|user| self.peer(usize::from(user)))
                    .map(|peer| peer.session.team);
                if other.teams.no_damage == no_damage
                    && user_team.is_none_or(|team| team == no_damage)
                {
                    return true;
                }
            }
        }
        let _ = emplaced::damage(&mut self.mounted.guns[index].1, damage, level_time);
        self.publish_gun(index);
        true
    }

    /// A gun as `G_Damage`'s attacker of `target`: no client, sparing its gunner's team
    /// (but not its gunner) and, without a gunner in play, its allied team.
    fn gun_attacker(&self, gun: u16, user: Option<u16>, target: usize) -> Attacker {
        let user_team = user
            .filter(|user| usize::from(*user) != target)
            .and_then(|user| self.peer(usize::from(user)))
            .map(|peer| peer.session.team);
        let allied = self
            .mounted
            .index(gun)
            .map_or(0, |index| self.mounted.guns[index].1.teams.allied);
        Attacker {
            npc: false,
            client: gun,
            max_health: 0,
            team: user_team.unwrap_or(allied),
            saber_knockback: [0.0; 4],
        }
    }

    /// `G_MissileImpact`'s damage for a gun's bolt on player or NPC `target`: the gun the
    /// attacker, the kill its gunner's. Whether it counted for anyone's accuracy (never:
    /// the gun is no client).
    pub(in super::super) fn gun_missile_hit(
        &mut self,
        target: usize,
        missile: &Missile,
        level_time: i32,
    ) -> bool {
        let Some(user) = missile.activator else {
            return false;
        };
        let attacker = self.gun_attacker(missile.owner, Some(user), target);
        let request = DamageRequest {
            level_time,
            attacker: Some(attacker),
            direction: Some(missile.impact_velocity),
            point: Some(missile.impact_point),
            damage: missile.damage,
            flags: missile.damage_flags,
            means: missile.method_of_death,
        };
        self.mounted.credit = Some((missile.owner, user));
        let _ = self.strike(usize::from(missile.owner), target, request, false);
        self.mounted.credit = None;
        false
    }

    /// The gun's end (`G_RadiusDamage(origin, gun, damage, radius, gun, NULL,
    /// MOD_UNKNOWN)`) over the players, NPCs and other guns, its lines drawn through the
    /// map and the guns' boxes (`CanDamage`, `MASK_SOLID`).
    fn gun_blast(&mut self, gun: u16, user: Option<u16>, blast: Blast, level_time: i32) {
        self.gather_splash_targets();
        let targets = std::mem::take(&mut self.splash_targets);
        let mut solids = std::mem::take(&mut self.mounted.solids);
        solids.clear();
        solids.extend(self.mounted.obstacles(
            sjk_protocol::ENTITY_NUMBER_NONE,
            sjk_protocol::ENTITY_NUMBER_NONE,
        ));
        let attacker = Attacker {
            npc: false,
            client: gun,
            max_health: 0,
            team: 0,
            saber_knockback: [0.0; 4],
        };
        let map = self.map.take();
        let mut hurt = |number: u16, mut request: DamageRequest| {
            request.attacker = Some(self.gun_attacker(gun, user, usize::from(number)));
            self.strike(usize::from(gun), usize::from(number), request, false)
                .1
        };
        let (origin, damage, radius) = (blast.origin, blast.damage as f32, blast.radius as f32);
        match &map {
            Some(map) => {
                let world = WithPlayers {
                    world: WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    players: &solids,
                };
                let _ = radius_damage(
                    origin,
                    Some(attacker),
                    damage,
                    radius,
                    Some(gun),
                    MOD_UNKNOWN,
                    level_time,
                    &targets,
                    &world,
                    &mut hurt,
                );
            }
            None => {
                let _ = radius_damage(
                    origin,
                    Some(attacker),
                    damage,
                    radius,
                    Some(gun),
                    MOD_UNKNOWN,
                    level_time,
                    &targets,
                    &WithPlayers {
                        world: Void,
                        players: &solids,
                    },
                    &mut hurt,
                );
            }
        }
        self.map = map;
        self.flush_npc_blows();
        self.splash_targets = targets;
        self.mounted.solids = solids;
    }
}

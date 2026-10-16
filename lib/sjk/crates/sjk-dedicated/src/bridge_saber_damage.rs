//! The saber's damage on this server: `WP_SaberPositionUpdate`'s damage half
//! ([`sjk_game_jka::saber_damage`], [`sjk_game_jka::saber_clash`]), after
//! [`super::bridge_saber`] has posed the player and read its blade.
//!
//! Each frame, for each playing client in number order: the blade is stored (how fast it
//! swings) and its saber entity placed and boxed around last frame's blade; the blade is
//! swept through the map, the other players' boxes and saber entities and their posed
//! meshes; the hits and clashes raise their events, the clash rules change both
//! players' saber moves, and the blows are dealt. After every client, each saber entity
//! is made solid or not (`SaberUpdateSelf`).

use super::NativeGame;
use super::bridge_saber::{PS_SABER_ANIM_LEVEL, PlayerSkeleton};
use crate::collision::{Void, WithPlayers, WorldCollision};
use crate::peer::Peer;
use sjk_game_jka::damage::{Attacker, DamageRequest, HitLocation, MOD_SABER};
use sjk_game_jka::disruptor::EF_DISINTEGRATION;
use sjk_game_jka::entity_clip::BoxObstacle;
use sjk_game_jka::player_death::Rng;
use sjk_game_jka::pmove::{MovementCollision, MovementTrace};
use sjk_game_jka::saber_clash::{
    BladeHistory, ClashWorld, Fighter, MAX_BLADES, SaberSet, SaberStorage, idle_in_world, saber_box,
};
use sjk_game_jka::saber_damage::{
    Ghoul2Answer, SaberHits, SaberTargets, SaberVictim, Swinger, ThrownSwing, Trail, WallBounce,
    location_from_surface, placed_by_surface,
};
use sjk_game_jka::server_skeleton::CollisionQuery;
use sjk_model::g2_collision::{CollisionRecord, CollisionScratch};

/// A blade's length before the player's sabers are known (`single_1`: `saberLength 40`).
const SABER_LENGTH: f32 = 40.0;
/// `BROKENLIMB_LARM` in `ps.brokenLimbs`: no second saber is swung.
const BROKEN_LEFT_ARM: u8 = 1 << 1;
/// `SS_DUAL`, `SS_STAFF`: base styles whose sabers are on with one holstered.
const SS_DUAL: u8 = 6;
const SS_STAFF: u8 = 7;
/// `BROKENLIMB_LARM`, `BROKENLIMB_RARM` in `ps.brokenLimbs`.
const BROKEN_ARMS: u8 = (1 << 1) | (1 << 2);
/// `g_g2TraceLod`.
const TRACE_LOD: usize = 3;
/// `CFL_MORESABERDMG` in a siege class's flags.
const MORE_SABER_DAMAGE: u32 = 1;
/// `CONTENTS_LIGHTSABER`: what a saber entity is to the blades that meet it.
const CONTENTS_LIGHTSABER: u32 = 0x4_0000;
/// `SABER_BOX_SIZE`: a new saber entity's box.
const SABER_BOX_SIZE: f32 = 16.0;
/// `WP_SABER`; player-state fields `saberMove`, `saberBlocked`, `viewheight`.
const WP_SABER: u8 = 3;
const PS_SABER_MOVE: usize = 34;
const PS_SABER_BLOCKED: usize = 77;
/// `FP_SABER_OFFENSE`, `FP_SABER_DEFENSE`.
const FP_SABER_OFFENSE: usize = 15;
const FP_SABER_DEFENSE: usize = 16;
const FP_SABERTHROW: usize = 17;
/// `DEFAULT_VIEWHEIGHT`: the eyes a saber is raised to block below.
const VIEW_HEIGHT: f32 = 36.0;

/// What a player's saber remembers between frames.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SaberCut {
    /// Each blade's trail (`saber[n].blade[m].trail`): where its last sweep ended.
    trails: [[Trail; MAX_BLADES]; 2],
    /// `ps.saberAttackWound`, `ps.saberIdleWound`: server-only, cleared at a spawn.
    pub(super) attack_wound: i32,
    idle_wound: i32,
    /// `g2LastSurfaceHit`, `g2LastSurfaceTime`: the surface a blade last struck on this
    /// player, and when.
    last_surface: (usize, i32),
    /// `lastSaberBase_Always` and the reading before: how fast the blade swings.
    pub(super) storage: SaberStorage,
    /// Every blade of both sabers now and the frame before, with what of the sabers'
    /// definitions the blade rules read (lengths, counts, flags).
    sabers: SaberSet,
    /// The saber entity: where it is, its box, whether blades meet it
    /// (`CONTENTS_LIGHTSABER`).
    pub(super) entity: ([f32; 3], [f32; 3], [f32; 3], bool),
    /// Whether it was ever linked — the owner's blade read once. Unlinked, it is in no
    /// trace's way and does not think (`G_RunFrame` skips a `neverFree` entity that is
    /// not linked).
    linked: bool,
}

impl Default for SaberCut {
    /// `WP_SaberInitBladeData`: solid from the start, with the default box.
    fn default() -> Self {
        Self {
            trails: [[Trail::default(); MAX_BLADES]; 2],
            attack_wound: 0,
            idle_wound: 0,
            last_surface: (0, 0),
            storage: SaberStorage::default(),
            sabers: SaberSet::single(BladeHistory {
                length: SABER_LENGTH,
                ..BladeHistory::default()
            }),
            entity: ([0.0; 3], [-SABER_BOX_SIZE; 3], [SABER_BOX_SIZE; 3], true),
            linked: false,
        }
    }
}

impl SaberCut {
    /// `g2LastSurfaceHit`, `g2LastSurfaceTime`: a surface of this player's model struck.
    pub(super) fn stamp_surface(&mut self, surface: usize, level_time: i32) {
        self.last_surface = (surface, level_time);
    }

    /// Whether the saber entity is linked and solid to what meets it
    /// (`CONTENTS_LIGHTSABER`).
    pub(super) fn entity_solid(&self) -> bool {
        self.linked && self.entity.3
    }
}

/// Buffers the frame's Ghoul2 collisions reuse, so a sweep allocates nothing.
#[derive(Default)]
pub(crate) struct SaberWork {
    scratch: CollisionScratch,
    records: Vec<CollisionRecord>,
}

impl SaberWork {
    /// Both buffers at once.
    pub(super) fn buffers(&mut self) -> (&mut CollisionScratch, &mut Vec<CollisionRecord>) {
        (&mut self.scratch, &mut self.records)
    }
}

/// `BG_SabersOff`: holstered, but for a pair or a staff with only its second saber or
/// blades off.
fn sabers_off(peer: &Peer) -> bool {
    let holstered = peer.state.saber_holstered();
    holstered != 0
        && !(matches!(
            peer.movement.state().saber_anim_level_base,
            SS_DUAL | SS_STAFF
        ) && holstered < 2)
}

/// The player's saber definitions into what the blade rules read of them: each saber's
/// blades and their lengths, whether a hilt is held, its type and flags.
fn refresh_sabers(peer: &mut Peer) {
    for (blades, saber) in peer
        .saber_cut
        .sabers
        .sabers
        .iter_mut()
        .zip(&peer.sabers.hands)
    {
        blades.count = saber.num_blades.clamp(0, MAX_BLADES as i32) as usize;
        blades.held = saber.is_held();
        blades.typed = saber.saber_type != 0;
        blades.combat = sjk_game_jka::saber_clash::SaberCombat::of(saber);
        for (history, blade) in blades.blades.iter_mut().zip(&saber.blades) {
            history.length = blade.length_max;
        }
    }
}

/// A player as the clash rules read it.
pub(super) fn fighter(number: usize, peer: &Peer) -> Fighter {
    let state = &peer.state;
    let command = &peer.last_command;
    let levels = peer
        .session
        .force
        .as_ref()
        .map_or([0; 18], |force| force.levels);
    let torso = state.torso_animation();
    Fighter {
        number: number as u16,
        saber_move: state.saber_move(),
        saber_blocked: u32::from(state.saber_blocked()),
        torso,
        torso_timer: state.torso_timer(),
        torso_length: peer
            .skeleton
            .as_ref()
            .map_or(0, |skeleton| skeleton.models.animation_length(torso)),
        weapon_state: state.weapon_state(),
        style: state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as i32,
        broken_arm: state.broken_limbs() & BROKEN_ARMS != 0,
        offense: i32::from(levels[FP_SABER_OFFENSE]),
        defense: i32::from(levels[FP_SABER_DEFENSE]),
        storage: peer.saber_cut.storage,
        blade: peer.saber_cut.sabers.sabers[0].blades[0],
        sabers: peer.saber_cut.sabers,
        holstered: state.saber_holstered(),
        sabers_off: state.weapon() != WP_SABER || sabers_off(peer),
        in_flight: state.saber_in_flight(),
        has_saber: state.saber_entity_num() != 0,
        lock_time: state.saber_lock_time(),
        team: peer.session.team,
        duel: state.duel_in_progress().then(|| state.duel_index()),
        idle_in_world: idle_in_world(
            command.buttons,
            command.forward_move,
            command.right_move,
            command.up_move,
        ),
        origin: state.origin(),
        view_height: VIEW_HEIGHT,
        view_yaw: state.view_angles()[1],
        lone_duelist: false,
        npc: false,
        // `NPCTEAM_PLAYER`: a player's `playerTeam` outside siege, where the NPC team rule
        // of a clash does not hold anyway.
        player_team: 2,
        event_flags: 0,
    }
}

impl NativeGame {
    /// `WP_SaberPositionUpdate`'s damage half for one client: store the first blade read
    /// this frame and box its saber entity, then sweep every lit blade of its sabers
    /// (`w_saber.c:8786-9092`), their blows dealt once all are swept.
    pub(super) fn swing_saber(&mut self, client: usize, level_time: i32) {
        let gametype = self.gametype;
        let more_saber_damage = self.siege_class_flags(client) & MORE_SABER_DAMAGE != 0;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        refresh_sabers(peer);
        let readings = (peer.blades.1 == level_time).then_some(peer.blades.0);
        // A thrown saber is where it flies (`bridge_throw`), not in the hand.
        let flying = peer.state.saber_in_flight();
        let Some((readings, first)) =
            readings.and_then(|readings| Some((readings, readings[0][0]?)))
        else {
            // `returnAfterUpdate`: the saber entity sits on the player.
            if !flying {
                peer.saber_cut.entity.0 = peer.state.origin();
            }
            return;
        };
        peer.saber_cut.storage.store(first.base, level_time);
        let length = peer.saber_cut.sabers.sabers[0].blades[0].length;
        let tip = std::array::from_fn(|axis| first.base[axis] + length * first.direction[axis]);
        if !flying {
            peer.saber_cut.entity.0 = tip;
        }
        peer.saber_cut.linked = true;
        // `BG_SabersOff`: nothing is boxed or swept.
        if sabers_off(peer) {
            return;
        }
        if !flying {
            // `SetSaberBoxSize`, from the blades as the damage loop last stored them.
            let owner = fighter(client, peer);
            let (mins, maxs) = saber_box(
                &owner,
                sjk_game_jka::saber_rules::super_break_lose(owner.torso),
                tip,
                level_time,
            );
            peer.saber_cut.entity = (tip, mins, maxs, true);
        }
        // A saber lock (`w_saber.c:8747-8781`): the blades' block effect now and then,
        // every blade's trail set to the first's, no block raised, and no damage traces.
        if peer.state.saber_lock_time() > level_time {
            let mut rng = self.deaths.rng;
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let health = peer.health;
            let (locked, effect) = sjk_game_jka::saber_lock::lock_blocks(
                &mut peer.state,
                health,
                &mut peer.saber_cut.idle_wound,
                tip,
                level_time,
                &mut rng,
            );
            if locked {
                let trail = Trail {
                    base: first.base,
                    tip,
                    last_time: level_time,
                };
                for (saber, trails) in peer.saber_cut.trails.iter_mut().enumerate() {
                    let count = peer.saber_cut.sabers.sabers[saber].count.min(MAX_BLADES);
                    trails[..count].fill(trail);
                }
                peer.movement.set_saber_blocked(0);
            }
            self.deaths.rng = rng;
            if let Some(effect) = effect {
                let _ = self.pool.spawn_temporary(effect.state(), level_time, None);
            }
            return;
        }
        // A saber knocked out of the hand is swept no more; a thrown one is, from where it
        // flies (`w_saber.c:8788-8797`).
        let knocked = flying && peer.state.saber_entity_num() == 0;
        let first_saber = usize::from(knocked);
        let (owner_origin, saber) = (peer.state.origin(), peer.saber_entity);
        let thrown = if flying && !knocked {
            saber.and_then(|saber| self.pool.state(saber)).map(|state| {
                let (origin, direction) =
                    sjk_game_jka::saber_throw::thrown_blade(state, owner_origin, level_time);
                (
                    sjk_game_jka::server_skeleton::Blade {
                        base: origin,
                        direction,
                    },
                    state
                        .raw_field(sjk_game_jka::saber_throw::ES_SABER_IN_FLIGHT)
                        .unwrap_or(0)
                        != 0,
                )
            })
        } else {
            None
        };
        let Some(peer) = self.peer(client) else {
            return;
        };
        let throw_level = peer
            .session
            .force
            .as_ref()
            .map_or(0, |force| force.levels[FP_SABERTHROW]);
        let mut hits = SaberHits::default();
        for saber in first_saber..2 {
            let Some(peer) = self.peer(client) else {
                return;
            };
            let set = peer.saber_cut.sabers;
            if !set.sabers[saber].held {
                continue;
            }
            let holstered = peer.state.saber_holstered();
            if saber == 1 && peer.state.broken_limbs() & BROKEN_LEFT_ARM != 0 {
                break;
            }
            if saber > 0 && set.pair() && holstered == 1 {
                break;
            }
            let saber_type = peer.sabers.hands[saber].saber_type;
            for blade in 0..set.sabers[saber].count.min(MAX_BLADES) {
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                // The muzzle's last reading becomes the old one before anything else.
                let history = &mut peer.saber_cut.sabers.sabers[saber].blades[blade];
                (history.point_old, history.direction_old) = (history.point, history.direction);
                if blade > 0 && !set.pair() && set.sabers[saber].count > 1 && holstered == 1 {
                    break;
                }
                let reading = if saber == 0 && flying {
                    thrown.map(|(blade, _)| blade)
                } else {
                    readings[saber][blade]
                };
                let Some(reading) = reading else { continue };
                (history.point, history.direction, history.storage_time) =
                    (reading.base, reading.direction, level_time);
                let fighter = Fighter {
                    blade: *history,
                    ..fighter(client, peer)
                };
                let state = &peer.state;
                let mut swinger = Swinger {
                    fighter,
                    level_time,
                    gametype,
                    legs: state.leg_animation(),
                    weapon_time: state.weapon_time(),
                    attack_wound: peer.saber_cut.attack_wound,
                    idle_wound: peer.saber_cut.idle_wound,
                    riding: state.vehicle_entity_num() != 0,
                    saber_type,
                    thrown: ThrownSwing {
                        in_flight: flying,
                        first_saber: saber == 0,
                        going_out: thrown.is_some_and(|(_, out)| out),
                        level: throw_level,
                    },
                    saber,
                    blade,
                    jedi_master: state.is_jedi_master(),
                    more_saber_damage,
                    // A player's `saberEventFlags` are read by nothing on this server.
                    enemy: None,
                };
                let trail = peer.saber_cut.trails[saber][blade];
                self.gather_obstacles(client);
                self.gather_saber_entities(client);
                let mut rng = self.deaths.rng;
                let (base, end) = hits.sweep(
                    &mut swinger,
                    &trail,
                    &mut BladeTargets {
                        game: self,
                        swinger: client,
                        level_time,
                        rng: &mut rng,
                    },
                );
                self.deaths.rng = rng;
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                let cut = &mut peer.saber_cut;
                cut.trails[saber][blade] = Trail {
                    base,
                    tip: end,
                    last_time: level_time,
                };
                (cut.idle_wound, cut.attack_wound) = (swinger.idle_wound, swinger.attack_wound);
                set_saber_state(peer, &swinger.fighter);
                // `WP_SaberDoHit`, `WP_SaberDoClash` after each blade: the frame's clash,
                // once met, is raised again for every blade after.
                let peers = self.players.places();
                let no_flare = self.peer(client).is_some_and(|peer| {
                    sjk_game_jka::saber_damage::no_clash_flare(&peer.sabers.hands[saber], blade)
                });
                let Self { pool, npcs, .. } = self;
                hits.hit_effects(
                    client as u16,
                    saber as u32,
                    blade as u32,
                    no_flare,
                    &|number| usize::from(number) < peers || npcs.bleeds(number),
                    &mut |event| {
                        let _ = pool.spawn_temporary(event.state(), level_time, None);
                    },
                );
                if let Some(event) = hits.clash_effect(client as u16, saber as u32, blade as u32) {
                    let _ = pool.spawn_temporary(event.state(), level_time, None);
                }
            }
        }
        self.deal_saber_blows(client, &hits, level_time);
    }

    /// `SaberUpdateSelf` for every player's saber entity, after the players' frames: solid
    /// while its owner holds it lit, and once unlit, solid again only after the blade was
    /// read within 200 ms.
    pub(super) fn update_saber_entities(&mut self, level_time: i32) {
        for client in 0..self.players.places() {
            let Some(peer) = self.peer_mut(client) else {
                continue;
            };
            let state = &peer.state;
            let offense = peer
                .session
                .force
                .as_ref()
                .map_or(0, |force| force.levels[FP_SABER_OFFENSE]);
            let lit = peer.body_active()
                && state.weapon() == WP_SABER
                && peer.health > 0
                && !sabers_off(peer)
                && offense > 0;
            let cut = &mut peer.saber_cut;
            if !cut.linked {
                continue;
            }
            cut.entity.3 = lit && (cut.entity.3 || level_time - cut.storage.last_time <= 200);
        }
    }

    /// `PM_FootSlopeTrace`'s foot bolt reads during a command: they stamp the player's
    /// skeleton cache at the Ghoul2 clock, which a later collision can leave stale.
    pub(super) fn read_foot_bolts(&mut self, client: usize) {
        let clock = self.last_frame_time;
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if !peer.movement.take_foot_bolt_read() {
            return;
        }
        if let Some(PlayerSkeleton {
            models, skeleton, ..
        }) = peer.skeleton.as_mut()
        {
            let _ = skeleton.read_foot_bolts(models, clock);
        }
    }

    /// The other players' saber entities, to the obstacles a blade's traces meet: every
    /// solid one not the swinger's; and every NPC's lit saber entity.
    pub(super) fn gather_saber_entities(&mut self, swinger: usize) {
        let Self {
            server,
            world,
            players,
            obstacles,
            npcs,
            ..
        } = self;
        let Some(world) = server.world(*world) else {
            return;
        };
        for (number, handle) in players.holders().enumerate() {
            let peer = handle
                .and_then(|handle| world.entity(handle))
                .filter(|peer| number != swinger && peer.begun && peer.body_active());
            let Some(peer) = peer else { continue };
            let (origin, mins, maxs, _) = peer.saber_cut.entity;
            let entity = peer.state.saber_entity_num();
            if peer.saber_cut.entity_solid() && entity != 0 {
                obstacles.push(BoxObstacle {
                    entity,
                    origin,
                    bounds: (mins, maxs),
                    contents: CONTENTS_LIGHTSABER,
                    model: None,
                });
            }
        }
        for npc in npcs
            .roster
            .actors
            .iter()
            .filter(|npc| npc.saber.entity_solid())
        {
            let (Some(entity), saber) = (npc.saber_entity, &npc.saber.entity) else {
                continue;
            };
            obstacles.push(BoxObstacle {
                entity,
                origin: saber.origin,
                bounds: (saber.mins, saber.maxs),
                contents: CONTENTS_LIGHTSABER,
                model: None,
            });
        }
    }

    /// `WP_SaberApplyDamage`: each victim's total as one `G_Damage`, a player's placed
    /// by the surface the blade struck on it.
    fn deal_saber_blows(&mut self, client: usize, hits: &SaberHits, level_time: i32) {
        let peers = self.players.places();
        let Some(swinger) = self.peer(client) else {
            return;
        };
        let attacker = Attacker {
            npc: false,
            client: client as u16,
            max_health: swinger.state.max_health(),
            team: swinger.session.team,
            saber_knockback: swinger.sabers.knockback_scales(),
        };
        // The NPCs among the victims are clients too: no wall's share of the damage.
        let mut npcs = [u16::MAX; 16];
        for (slot, blow) in npcs.iter_mut().zip(hits.blows(&|_| true)) {
            if self.npcs.roster.is_npc(blow.victim) {
                *slot = blow.victim;
            }
        }
        for blow in hits.blows(&|number| usize::from(number) < peers || npcs.contains(&number)) {
            let target = usize::from(blow.victim);
            let location = if target < peers {
                self.surface_location(target, blow.flags, blow.spot, level_time)
            } else if self.npcs.roster.is_npc(blow.victim) {
                self.npc_surface_location(blow.victim, blow.flags, blow.spot, level_time)
            } else {
                let _ = self.hurt_brush(
                    blow.victim,
                    blow.damage,
                    MOD_SABER,
                    client as u16,
                    level_time,
                );
                continue;
            };
            let request = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(blow.direction),
                point: Some(blow.spot),
                damage: blow.damage,
                flags: blow.flags,
                means: MOD_SABER,
            };
            let _ = self.strike_at(client, target, request, location, false);
        }
    }

    /// `G_LocationBasedDamageModifier`'s surface half: when a blade struck `target`'s
    /// mesh this frame, the body part of that surface, its bolts read at the Ghoul2
    /// clock — the knees, hands and feet with the angles a player's entity never sets,
    /// the torso point facing the view (`UpdateClientRenderBolts`).
    pub(super) fn surface_location(
        &mut self,
        target: usize,
        flags: u32,
        spot: [f32; 3],
        level_time: i32,
    ) -> Option<HitLocation> {
        let ghoul2_time = if self.previous_frame_time == 0 {
            level_time
        } else {
            self.previous_frame_time
        };
        peer_surface_location(self.peer_mut(target)?, flags, spot, level_time, ghoul2_time)
    }

    /// The flags of the siege class `client` plays, in a siege.
    pub(super) fn siege_class_flags(&self, client: usize) -> u32 {
        if self.gametype != super::GAMETYPE_SIEGE {
            return 0;
        }
        let (Some(peer), Some(siege)) = (self.peer(client), self.siege.as_ref()) else {
            return 0;
        };
        peer.siege_class_index
            .map_or(0, |index| siege.registry.classes[index].class_flags)
    }
}

/// [`NativeGame::surface_location`] for `peer`, its bolts read at `ghoul2_time`.
pub(super) fn peer_surface_location(
    peer: &mut Peer,
    flags: u32,
    spot: [f32; 3],
    level_time: i32,
    ghoul2_time: i32,
) -> Option<HitLocation> {
    let (surface, struck_at) = peer.saber_cut.last_surface;
    if !placed_by_surface(flags, struck_at, level_time) {
        return None;
    }
    let (origin, view_yaw) = (peer.state.origin(), peer.state.view_angles()[1]);
    let PlayerSkeleton {
        models, skeleton, ..
    } = peer.skeleton.as_mut()?;
    let name = models.surface_name(surface)?;
    Some(location_from_surface(
        name,
        spot,
        &mut |bolt, facing_view| {
            let yaw = if facing_view { view_yaw } else { 0.0 };
            skeleton
                .bolt_point(models, bolt, yaw, origin, ghoul2_time)
                .ok()
                .flatten()
        },
    ))
}

/// A clash's change to a player's saber move and block, on the wire state the next
/// command's movement restarts from.
pub(super) fn set_saber_state(peer: &mut Peer, fighter: &Fighter) {
    if peer.state.saber_move() == fighter.saber_move
        && u32::from(peer.state.saber_blocked()) == fighter.saber_blocked
    {
        return;
    }
    peer.state.set_raw_field(PS_SABER_MOVE, fighter.saber_move);
    peer.state
        .set_raw_field(PS_SABER_BLOCKED, fighter.saber_blocked);
    peer.movement = peer.movement.reseeded(&peer.state);
}

/// `SetSaberBoxSize` for `peer`'s saber entity at `current`, from its blade as last read.
pub(super) fn saber_box_of(
    client: usize,
    peer: &Peer,
    current: [f32; 3],
    level_time: i32,
) -> ([f32; 3], [f32; 3]) {
    let owner = fighter(client, peer);
    saber_box(
        &owner,
        sjk_game_jka::saber_rules::super_break_lose(owner.torso),
        current,
        level_time,
    )
}

/// The world a blade sweeps: the map, the boxes gathered for the swinger (players,
/// missiles, brushes, saber entities), and the players' posed meshes.
struct BladeTargets<'a> {
    game: &'a mut NativeGame,
    swinger: usize,
    level_time: i32,
    rng: &'a mut Rng,
}

impl SaberTargets for BladeTargets<'_> {
    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> MovementTrace {
        let NativeGame { map, obstacles, .. } = &*self.game;
        match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, mins, maxs, end, mask),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, mins, maxs, end, mask),
        }
    }

    fn collide(
        &mut self,
        number: u16,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
    ) -> Ghoul2Answer {
        let level_time = self.level_time;
        if usize::from(number) >= self.game.players.places() {
            // An NPC's posed model; anything else is its box.
            return self
                .game
                .npc_collide(number, start, end, radius, level_time)
                .unwrap_or(Ghoul2Answer::NoModel);
        }
        let NativeGame {
            server,
            world,
            players,
            saber_work,
            ..
        } = &mut *self.game;
        let peer = players
            .at(usize::from(number))
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else {
            return Ghoul2Answer::NoModel;
        };
        let (origin, yaw) = (peer.state.origin(), peer.state.view_angles()[1]);
        let Some(PlayerSkeleton {
            models, skeleton, ..
        }) = peer.skeleton.as_mut()
        else {
            return Ghoul2Answer::NoModel;
        };
        let query = CollisionQuery {
            origin,
            yaw,
            time: level_time,
            start,
            end,
            lod: TRACE_LOD,
            radius,
        };
        let SaberWork { scratch, records } = saber_work;
        match skeleton.collide(models, &query, scratch, records) {
            Ok(_) => match records.first() {
                Some(record) => {
                    peer.saber_cut.last_surface = (record.surface, level_time);
                    Ghoul2Answer::Hit {
                        position: record.position,
                        normal: record.normal,
                    }
                }
                None => Ghoul2Answer::Miss,
            },
            Err(error) => {
                eprintln!("client {number}'s collision: {error}");
                Ghoul2Answer::NoModel
            }
        }
    }

    fn victim(&self, number: u16) -> Option<SaberVictim> {
        let game = &*self.game;
        if usize::from(number) >= game.players.places() {
            if let Some(npc) = game.npc_saber_victim(number, self.swinger) {
                return Some(npc);
            }
            // A breakable brush; a saber entity is not a victim but a saber owner.
            return game
                .breakables
                .iter()
                .any(|(ours, _)| ours.legacy_number() == number)
                .then_some(SaberVictim {
                    takes_damage: true,
                    health: 1,
                    ..SaberVictim::default()
                });
        }
        let peer = game.peer(usize::from(number))?;
        let swinger = game.peer(self.swinger)?;
        let state = &peer.state;
        let team = swinger.session.team;
        let legs = state.leg_animation();
        let length = peer
            .skeleton
            .as_ref()
            .map_or(0, |skeleton| skeleton.models.animation_length(legs));
        let duelling = |one: &sjk_protocol::PlayerState, other: usize| {
            one.duel_in_progress() && usize::from(one.duel_index()) != other
        };
        Some(SaberVictim {
            client: true,
            takes_damage: peer.begun && peer.body_active(),
            health: peer.health,
            disintegrated: state.entity_flags() & EF_DISINTEGRATION != 0,
            // `OnSameTeam` with `g_friendlySaber 0`.
            spared_by_idle: team != 0 && team == peer.session.team,
            duel_elsewhere: duelling(state, self.swinger)
                || duelling(&swinger.state, usize::from(number)),
            knocked_down_on_ground: sjk_game_jka::saber_rules::knocked_down_on_ground(
                legs,
                state.legs_timer(),
                length,
            ),
            // Read only for an NPC's blade.
            player_team: 2,
        })
    }

    fn saber_owner(&self, number: u16) -> Option<Fighter> {
        let game = &*self.game;
        let npc = game
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.saber_entity == Some(number) && npc.saber.entity_solid());
        if let Some(npc) = npc {
            return Some(sjk_game_jka::npc_saber::npc_fighter(npc));
        }
        (0..game.players.places())
            .filter(|client| *client != self.swinger)
            .find_map(|client| {
                let peer = game.peer(client)?;
                (peer.state.saber_entity_num() == number && peer.saber_cut.entity_solid())
                    .then(|| fighter(client, peer))
            })
    }

    fn set_saber_owner(&mut self, owner: &Fighter) {
        if let Some(npc) = self
            .game
            .npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == owner.number)
        {
            sjk_game_jka::npc_saber::set_npc_saber(npc, owner);
        } else if let Some(peer) = self.game.peer_mut(usize::from(owner.number)) {
            set_saber_state(peer, owner);
        }
    }

    fn wall_bounce(&mut self, bounce: &WallBounce) {
        // The splash's damage draws from the game's generator too: lent back.
        std::mem::swap(&mut self.game.deaths.rng, self.rng);
        self.game
            .saber_wall_bounce(self.swinger, bounce, self.level_time);
        std::mem::swap(&mut self.game.deaths.rng, self.rng);
    }
}

impl ClashWorld for BladeTargets<'_> {
    fn debug_saber_locks(&self) -> bool {
        self.game.debug_saber_locks()
    }

    fn rng(&mut self) -> &mut Rng {
        self.rng
    }

    fn check_lock(&mut self, me: u16, other: u16) -> bool {
        self.game.check_lock(
            usize::from(me),
            usize::from(other),
            self.rng,
            self.level_time,
        )
    }

    fn knock_out(&mut self, owner: u16, velocity: [f32; 3]) -> bool {
        // The knockout draws from the game's generator too: this clash's is lent back.
        std::mem::swap(&mut self.game.deaths.rng, self.rng);
        let knocked = self
            .game
            .knock_out(usize::from(owner), velocity, self.level_time);
        std::mem::swap(&mut self.game.deaths.rng, self.rng);
        knocked
    }

    fn smash(&mut self, owner: u16, striker: u16, damage: i32) -> bool {
        std::mem::swap(&mut self.game.deaths.rng, self.rng);
        let smashed = self
            .game
            .smash(usize::from(owner), striker, damage, self.level_time);
        std::mem::swap(&mut self.game.deaths.rng, self.rng);
        smashed
    }
}

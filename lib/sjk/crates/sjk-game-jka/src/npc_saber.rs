//! An NPC's own saber: `WP_SaberPositionUpdate` for a saber carrier at its turn in the frame
//! (`g_main.c:3342-3346`, `w_saber.c:8091-9092`), its saber entity's think
//! (`SaberUpdateSelf`, `w_saber.c:303-383`), and the look target
//! `WP_SaberStartMissileBlockCheck` gives it (`w_saber.c:5456-5794`).
//!
//! The reference gives an NPC the player's saber code whole. Every frame, before its think:
//! - the saber's upkeep ([`upkeep`]): the style drawn, a superbreak's winner firing, the
//!   leave to throw, and the eyes (`UpdateClientRenderinfo`);
//! - its server-side model posed and, while it holds its lit saber ([`reads_blades`]), its
//!   blades read ([`crate::npc_skeleton::NpcSkeleton::pose`], through the host);
//! - the first blade stored (how fast the saber swings), the saber entity put at the first
//!   blade's tip and boxed round every blade (`SetSaberBoxSize`), solid to blades
//!   (`CONTENTS_LIGHTSABER`);
//! - each blade swept from where it was last frame ([`crate::saber_damage::SaberHits::sweep`]),
//!   through the map, the players and NPCs (their posed models), and every other lit saber
//!   entity — a player's or another NPC's — whose owner's blade it may clash with
//!   ([`crate::saber_clash::clash`]);
//! - the hits' events, and each victim's blow dealt through `G_Damage` (an NPC's through
//!   [`crate::npc_damage`], a player's through the host).
//!
//! A saber lock between an NPC and anyone, a saber knocked out of an NPC's hand, a thrown
//! NPC saber and a blade bouncing off a wall are not ported: the host is told
//! ([`crate::npc_spawn::NpcHost::stub`]) where the reference would do them.

use crate::npc_skeleton::NpcBlades;
use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::saber_clash::{
    BladeHistory, Fighter, MAX_BLADES, SaberCombat, SaberSet, SaberStorage, saber_box,
};
use crate::saber_damage::{SaberHits, Swinger, ThrownSwing, Trail};
use crate::server_skeleton::Blade;

/// `CONTENTS_LIGHTSABER`: what a lit saber entity is to the blades that meet it.
pub const CONTENTS_LIGHTSABER: u32 = 0x4_0000;
/// `WP_SABER`; `WEAPON_RAISING`, `WEAPON_DROPPING`, `WEAPON_FIRING`.
const WP_SABER: u8 = 3;
const WEAPON_RAISING: u8 = 1;
const WEAPON_DROPPING: u8 = 2;
const WEAPON_FIRING: u32 = 3;
/// `SS_DUAL`, `SS_STAFF`.
const SS_DUAL: u8 = 6;
const SS_STAFF: u8 = 7;
/// `BROKENLIMB_LARM`, and both arms.
const BROKEN_LEFT_ARM: u8 = 1 << 1;
const BROKEN_ARMS: u8 = (1 << 1) | (1 << 2);
/// Player-state fields read or written without an accessor: `fd.saberAnimLevel`,
/// `fd.saberDrawAnimLevel`, `weaponstate`, `saberCanThrow`, `saberMove`, `saberBlocked`,
/// `hasLookTarget`, `lookTarget`, `eFlags2`, `torsoAnim`.
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_DRAW_ANIM_LEVEL: usize = 25;
const PS_WEAPON_STATE: usize = 33;
const PS_SABER_CAN_THROW: usize = 49;
const PS_SABER_MOVE: usize = 34;
const PS_SABER_BLOCKED: usize = 77;
pub(crate) const PS_HAS_LOOK_TARGET: usize = 76;
pub(crate) const PS_LOOK_TARGET: usize = 66;
pub(crate) const PS_EFLAGS2: usize = 103;
const PS_TORSO_ANIM: usize = 15;
/// `EF2_HELD_BY_MONSTER`: a held NPC's look target is its holder's to set.
pub(crate) const EF2_HELD_BY_MONSTER: u32 = 1 << 0;
/// `SFL_NOT_ACTIVE_BLOCKING`: a saber that never raises itself to block.
pub(crate) const SFL_NOT_ACTIVE_BLOCKING: u32 = 1 << 3;
/// `FP_SABER_OFFENSE`, `FP_SABER_DEFENSE`.
const FP_SABER_OFFENSE: usize = 15;
const FP_SABER_DEFENSE: usize = 16;
const FP_SABERTHROW: usize = 17;
/// `ENTITYNUM_WORLD`.
pub(crate) const ENTITYNUM_WORLD: u16 = 1_022;
/// `SABER_BOX_SIZE`: the box `WP_SaberInitBladeData` gives before any blade was read.
const SABER_BOX_SIZE: f32 = 16.0;

/// An NPC's saber entity (`saberStoredIndex`, `ps.saberEntityNum`) as the server keeps it:
/// never sent to a client, solid to blades while its owner holds the saber lit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SaberEntity {
    /// `r.currentOrigin`: at the first blade's tip while held.
    pub origin: [f32; 3],
    /// `r.mins`, `r.maxs`: round every lit blade (`SetSaberBoxSize`).
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `r.contents`: [`CONTENTS_LIGHTSABER`] or nothing.
    pub contents: u32,
    /// `r.linked`: `G_RunFrame` skips an unlinked saber entity (`neverFree`,
    /// `g_main.c:3117`), and no trace meets it.
    pub linked: bool,
    /// `nextthink` (`SaberUpdateSelf`): 0 for none.
    pub think_at: i32,
}

/// What an NPC's saber keeps between frames: each blade's last reading and trail, the
/// wounds' times, how fast it swings, what its saber did (`saberEventFlags`), and its
/// saber entity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NpcSaber {
    /// Every blade of both sabers now and the frame before, with what of the sabers'
    /// definitions the blade rules read.
    pub sabers: SaberSet,
    /// Each blade's trail (`blade[n].trail`): where its last sweep ended.
    pub trails: [[Trail; MAX_BLADES]; 2],
    /// `ps.saberAttackWound`, `ps.saberIdleWound`: no blow before these.
    pub attack_wound: i32,
    pub idle_wound: i32,
    /// `lastSaberBase_Always`, `lastSaberStorageTime` and the reading before.
    pub storage: SaberStorage,
    /// `ps.saberEventFlags` ([`crate::saber_clash::sef`]).
    pub event_flags: u32,
    /// `ps.saberThrowDelay`: no throw before this time.
    pub throw_delay: i32,
    /// `ps.saberBlockTime`: no missile blocked before this time.
    pub block_time: i32,
    /// The saber entity.
    pub entity: SaberEntity,
    /// The saber entity as a throw keeps it (its think, trajectory helpers, `pos1`,
    /// `genericValue5`, `clipmask`), and the throw on the NPC (`saberEntityState`,
    /// `saberDidThrowTime`, `saberKnockedTime`): [`crate::npc_saber_throw`]'s.
    pub flight: crate::saber_throw::SaberEntity,
    pub throw: crate::saber_throw::ThrowMemory,
    /// A lock's weight and presses (`ps.saberLockHits`, [`crate::npc_saber_lock`]).
    pub lock: crate::saber_lock::LockMemory,
}

impl NpcSaber {
    /// `WP_SaberInitBladeData` for an NPC holding `sabers` at `level_time`: the saber entity
    /// solid, boxed by the default cube, thinking 50 ms on and not yet linked; the blades'
    /// lengths and what the blade rules read of the sabers' definitions.
    pub fn new(sabers: &[crate::saber_definition::SaberDefinition; 2], level_time: i32) -> Self {
        let mut saber = Self {
            entity: SaberEntity {
                mins: [-SABER_BOX_SIZE; 3],
                maxs: [SABER_BOX_SIZE; 3],
                contents: CONTENTS_LIGHTSABER,
                think_at: level_time + 50,
                ..SaberEntity::default()
            },
            ..Self::default()
        };
        saber.refresh(sabers);
        saber
    }

    /// What the blade rules read of the sabers' definitions: each saber's blades and their
    /// lengths, whether a hilt is held, its type and flags.
    pub fn refresh(&mut self, sabers: &[crate::saber_definition::SaberDefinition; 2]) {
        for (blades, saber) in self.sabers.sabers.iter_mut().zip(sabers) {
            blades.count = saber.num_blades.clamp(0, MAX_BLADES as i32) as usize;
            blades.held = saber.is_held();
            blades.typed = saber.saber_type != 0;
            blades.combat = SaberCombat::of(saber);
            for (history, blade) in blades.blades.iter_mut().zip(&saber.blades) {
                history.length = blade.length_max;
            }
        }
    }

    /// Whether the saber entity is linked and solid to what meets it.
    pub fn entity_solid(&self) -> bool {
        self.entity.linked && self.entity.contents & CONTENTS_LIGHTSABER != 0
    }
}

/// `BG_SabersOff`: holstered, but for a pair or a staff with only its second saber or
/// blades off.
pub fn sabers_off(npc: &NpcActor) -> bool {
    let holstered = npc.player.saber_holstered();
    holstered != 0
        && !(matches!(
            npc.movement.state().saber_anim_level_base,
            SS_DUAL | SS_STAFF
        ) && holstered < 2)
}

/// Whether `WP_SaberPositionUpdate` reads the NPC's blades this frame: it has a saber
/// entity, holds the saber out (not raising or lowering it) and lives (`returnAfterUpdate`,
/// `w_saber.c:8404-8436`). Run [`upkeep`] first: a superbreak's winner fires.
pub fn reads_blades(npc: &NpcActor) -> bool {
    let state = &npc.player;
    npc.saber_entity.is_some()
        && state.weapon() == WP_SABER
        && ![WEAPON_RAISING, WEAPON_DROPPING].contains(&state.weapon_state())
        && npc.health >= 1
}

/// `WP_SaberPositionUpdate`'s opening for an NPC (`w_saber.c:8117-8470`, `7332-7479`): the
/// style drawn is the style in use (an NPC queues no style change); for a saber carrier a
/// superbreak's winner keeps firing and, once the throw delay is past, the first saber may
/// be thrown unless its definition forbids it; and its eyes (`UpdateClientRenderinfo`): its
/// view height over where it stands, looking along its view.
pub fn upkeep(npc: &mut NpcActor, level_time: i32) {
    let level = npc.player.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0);
    npc.player.set_raw_field(PS_SABER_DRAW_ANIM_LEVEL, level);
    if npc.saber_entity.is_some() {
        if crate::saber_rules::super_break_win(
            npc.player.raw_field(PS_TORSO_ANIM).unwrap_or(0) as u16
        ) {
            npc.player.set_raw_field(PS_WEAPON_STATE, WEAPON_FIRING);
        }
        if npc.saber.throw_delay < level_time {
            let can = crate::saber_frame::throwable(
                &npc.definition.sabers[0],
                u32::from(npc.player.saber_holstered()),
            );
            npc.player.set_raw_field(PS_SABER_CAN_THROW, u32::from(can));
        }
    }
    let mut eyes = npc.player.origin();
    eyes[2] += npc.player.view_height() as f32;
    npc.mind.eye_point = eyes;
    npc.mind.eye_angles = npc.player.view_angles();
    npc.mind.muzzle_point_old = npc.mind.muzzle_point;
    npc.mind.muzzle_point = npc.player.origin();
}

/// An NPC as the saber rules read it ([`Fighter`]): an NPC is never idle in the world
/// (`G_ClientIdleInWorld`, `w_saber.c:2297-2300`) and duels nobody.
pub fn npc_fighter(npc: &NpcActor) -> Fighter {
    let state = &npc.player;
    let torso = state.torso_animation();
    let level = |power: usize| npc.force_levels.get(power).copied().unwrap_or(0);
    Fighter {
        number: npc.number,
        saber_move: state.saber_move(),
        saber_blocked: u32::from(state.saber_blocked()),
        torso,
        torso_timer: state.torso_timer(),
        torso_length: npc
            .movement
            .animation_lengths()
            .and_then(|lengths| lengths.length_ms(torso))
            .unwrap_or(0),
        weapon_state: state.weapon_state(),
        style: state.raw_field(PS_SABER_ANIM_LEVEL).unwrap_or(0) as i32,
        broken_arm: state.broken_limbs() & BROKEN_ARMS != 0,
        offense: level(FP_SABER_OFFENSE),
        defense: level(FP_SABER_DEFENSE),
        storage: npc.saber.storage,
        blade: npc.saber.sabers.sabers[0].blades[0],
        sabers: npc.saber.sabers,
        holstered: state.saber_holstered(),
        sabers_off: state.weapon() != WP_SABER || sabers_off(npc),
        in_flight: state.saber_in_flight(),
        has_saber: state.saber_entity_num() != 0,
        lock_time: state.saber_lock_time(),
        team: npc.session_team,
        duel: None,
        idle_in_world: false,
        origin: state.origin(),
        view_height: state.view_height() as f32,
        view_yaw: state.view_angles()[1],
        lone_duelist: false,
        npc: true,
        player_team: npc.player_team,
        event_flags: npc.saber.event_flags,
    }
}

/// A clash's change to an NPC's saber: its move, its block and what its saber did.
pub fn set_npc_saber(npc: &mut NpcActor, fighter: &Fighter) {
    npc.player.set_raw_field(PS_SABER_MOVE, fighter.saber_move);
    npc.player
        .set_raw_field(PS_SABER_BLOCKED, fighter.saber_blocked);
    npc.saber.event_flags = fighter.event_flags;
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `WP_SaberPositionUpdate` for the saber carrier at `me` after its pose read `blades`
    /// (all `None` unless [`reads_blades`]): the saber entity kept on it or at its blade,
    /// and — its sabers lit — boxed, solid and linked, every blade swept and the blows dealt.
    pub fn saber_update(&mut self, me: usize, blades: &NpcBlades) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        if npc.saber_entity.is_none() {
            return;
        }
        // "place it on top of the player origin" while no blade is read, for a saber entity
        // that is solid or has no contents at all.
        let placeable =
            npc.saber.entity.contents & CONTENTS_LIGHTSABER != 0 || npc.saber.entity.contents == 0;
        let Some(first) = blades[0][0].filter(|_| reads_blades(npc)) else {
            if placeable && !npc.player.saber_in_flight() {
                npc.saber.entity.origin = npc.player.origin();
            }
            return;
        };
        let saber = &mut npc.saber;
        saber.storage.store(first.base, level_time);
        let length = saber.sabers.sabers[0].blades[0].length;
        let tip: [f32; 3] =
            std::array::from_fn(|axis| first.base[axis] + length * first.direction[axis]);
        let in_flight = npc.player.saber_in_flight();
        if placeable && !in_flight {
            saber.entity.origin = tip;
        }
        // The thrown saber's own part (`w_saber.c:8617-8726`); in hand, once a throw or a
        // knock-out has shown it, drawn by nobody and quiet again (`8738-8745`).
        let quieted = !in_flight
            && (saber.throw.did_throw_time != 0 || saber.throw.knocked_time != 0)
            && !sabers_off(npc);
        if in_flight {
            self.throw_update(me, first.base, first.direction);
        } else if quieted {
            let _ = self.with_npc_saber(me, |saber, _| crate::saber_throw::hand_update(saber));
        }
        let npc = &mut self.actors[me];
        if sabers_off(npc) {
            return;
        }
        if in_flight {
            // Out of the hand, it is not boxed here; the blades still cut and lock.
            if npc.player.saber_lock_time() > level_time {
                self.locked_saber(me, first, tip);
                return;
            }
            let mut hits = SaberHits::default();
            self.sweep_blades(me, blades, &mut hits);
            self.deal_saber_blows(me, &hits);
            return;
        }
        // `SetSaberBoxSize`, from the blades as the damage loop last stored them.
        let owner = npc_fighter(npc);
        let (mins, maxs) = saber_box(
            &owner,
            crate::saber_rules::super_break_lose(owner.torso),
            npc.saber.entity.origin,
            level_time,
        );
        let entity = &mut npc.saber.entity;
        (entity.contents, entity.mins, entity.maxs) = (CONTENTS_LIGHTSABER, mins, maxs);
        if npc.player.saber_lock_time() > level_time {
            self.locked_saber(me, first, tip);
            return;
        }
        let mut hits = SaberHits::default();
        self.sweep_blades(me, blades, &mut hits);
        self.deal_saber_blows(me, &hits);
        // The saber entity linked after the damage (`w_saber.c:9075-9078`).
        self.actors[me].saber.entity.linked = true;
    }

    /// A saber lock's frame (`w_saber.c:8747-8781`): the block effect now and then, every
    /// blade's trail set to the first's, no block raised, no damage traces.
    fn locked_saber(&mut self, me: usize, first: Blade, tip: [f32; 3]) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let (locked, effect) = crate::saber_lock::lock_blocks(
            &mut npc.player,
            npc.health,
            &mut npc.saber.idle_wound,
            npc.saber.entity.origin,
            level_time,
            self.host.rng(),
        );
        if locked {
            let trail = Trail {
                base: first.base,
                tip,
                last_time: level_time,
            };
            for (saber, trails) in npc.saber.trails.iter_mut().enumerate() {
                let count = npc.saber.sabers.sabers[saber].count.min(MAX_BLADES);
                trails[..count].fill(trail);
            }
            npc.player.set_raw_field(PS_SABER_BLOCKED, 0);
        }
        if let Some(effect) = effect {
            self.host.raise(effect);
        }
    }

    /// The damage loop (`w_saber.c:8800-9070`): each lit blade of each held saber swept from
    /// its trail, its hits and clash raised after it.
    fn sweep_blades(&mut self, me: usize, blades: &NpcBlades, hits: &mut SaberHits) {
        let level_time = self.level_time;
        let gametype = self.host.gametype();
        let number = self.actors[me].number;
        // Thrown, the first saber's blade is read where it flies; knocked out of the hand, it
        // is swept no more (`w_saber.c:8788-8797`, `8848-8888`).
        let flying = self.actors[me].player.saber_in_flight();
        let knocked = flying && self.actors[me].player.saber_entity_num() == 0;
        let thrown = if flying && !knocked {
            self.thrown_blade(me)
        } else {
            None
        };
        let throw_level = self.actors[me]
            .force_levels
            .get(FP_SABERTHROW)
            .copied()
            .unwrap_or(0)
            .clamp(0, 255) as u8;
        for saber in usize::from(knocked)..2 {
            let npc = &self.actors[me];
            let set = npc.saber.sabers;
            if !set.sabers[saber].held {
                continue;
            }
            let holstered = npc.player.saber_holstered();
            if saber == 1 && npc.player.broken_limbs() & BROKEN_LEFT_ARM != 0 {
                break;
            }
            if saber > 0 && set.pair() && holstered == 1 {
                break;
            }
            let saber_type = npc.definition.sabers[saber].saber_type;
            for blade in 0..set.sabers[saber].count.min(MAX_BLADES) {
                let npc = &mut self.actors[me];
                let history = &mut npc.saber.sabers.sabers[saber].blades[blade];
                (history.point_old, history.direction_old) = (history.point, history.direction);
                if blade > 0 && !set.pair() && set.sabers[saber].count > 1 && holstered == 1 {
                    break;
                }
                let reading = if saber == 0 && flying {
                    thrown.map(|(blade, _)| blade)
                } else {
                    blades[saber][blade]
                };
                let Some(reading) = reading else { continue };
                (history.point, history.direction, history.storage_time) =
                    (reading.base, reading.direction, level_time);
                let blade_now: BladeHistory = *history;
                let state = &npc.player;
                let mut swinger = Swinger {
                    fighter: Fighter {
                        blade: blade_now,
                        ..npc_fighter(npc)
                    },
                    level_time,
                    gametype,
                    legs: state.leg_animation(),
                    weapon_time: state.weapon_time(),
                    attack_wound: npc.saber.attack_wound,
                    idle_wound: npc.saber.idle_wound,
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
                    jedi_master: false,
                    more_saber_damage: false,
                    enemy: npc.mind.enemy,
                };
                let trail = npc.saber.trails[saber][blade];
                let (base, end) = {
                    let mut targets = crate::npc_saber_targets::BladeWorld::new(self, me);
                    hits.sweep(&mut swinger, &trail, &mut targets)
                };
                let npc = &mut self.actors[me];
                npc.saber.trails[saber][blade] = Trail {
                    base,
                    tip: end,
                    last_time: level_time,
                };
                (npc.saber.idle_wound, npc.saber.attack_wound) =
                    (swinger.idle_wound, swinger.attack_wound);
                if npc.player.saber_move() != swinger.fighter.saber_move
                    || u32::from(npc.player.saber_blocked()) != swinger.fighter.saber_blocked
                    || npc.saber.event_flags != swinger.fighter.event_flags
                {
                    set_npc_saber(npc, &swinger.fighter);
                }
                // `WP_SaberDoHit`, `WP_SaberDoClash` after each blade: a player bleeds, and so
                // does an NPC but a droid.
                // Raised once the victims are read: at most a flare and a hit for each.
                let mut events = [None; 32];
                let mut count = 0;
                {
                    let (actors, host) = (&*self.actors, &*self.host);
                    let bleeds = |victim: u16| {
                        let npc = actors.iter().find(|npc| npc.number == victim);
                        npc.map_or(
                            host.players().iter().any(|player| player.number == victim),
                            |npc| crate::npc_skeleton::bleeds(npc.definition.client_class),
                        )
                    };
                    let no_flare =
                        actors
                            .iter()
                            .find(|npc| npc.number == number)
                            .is_some_and(|npc| {
                                crate::saber_damage::no_clash_flare(
                                    &npc.definition.sabers[saber],
                                    blade,
                                )
                            });
                    hits.hit_effects(
                        number,
                        saber as u32,
                        blade as u32,
                        no_flare,
                        &bleeds,
                        &mut |event| {
                            if let Some(slot) = events.get_mut(count) {
                                *slot = Some(event);
                                count += 1;
                            }
                        },
                    );
                }
                for event in events.into_iter().flatten() {
                    self.host.raise(event);
                }
                if let Some(event) = hits.clash_effect(number, saber as u32, blade as u32) {
                    self.host.raise(event);
                }
            }
        }
    }

    /// `SaberUpdateSelf` for the saber entity of the NPC at `me`, at its think: no contents
    /// while the NPC's saber is out of its hand, off, or the NPC dead or not holding it (an
    /// NPC's needs no attack level); solid again once its blade was read within 200 ms.
    /// Linked, and thinking again next frame.
    pub fn saber_entity_think(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let lit = npc.player.weapon() == WP_SABER
            && npc.health >= 1
            && !sabers_off(npc)
            && npc.session_team != 3;
        let recent = level_time - npc.saber.storage.last_time <= 200;
        if npc.player.saber_in_flight() && npc.health > 0 {
            npc.saber.entity.think_at = level_time;
            crate::npc_saber_throw::in_hand_flags(npc, true, lit);
            return;
        }
        let entity = &mut npc.saber.entity;
        entity.contents = if !lit {
            0
        } else if entity.contents != CONTENTS_LIGHTSABER && !recent {
            entity.contents
        } else {
            CONTENTS_LIGHTSABER
        };
        entity.linked = true;
        entity.think_at = level_time;
        crate::npc_saber_throw::in_hand_flags(npc, false, lit);
    }

    /// `WP_SaberStartMissileBlockCheck` for the NPC at `me`: its look target, and what comes
    /// at it ([`crate::npc_missile_block`]).
    pub fn look_target_update(&mut self, me: usize) {
        self.missile_block_check(me);
    }
}

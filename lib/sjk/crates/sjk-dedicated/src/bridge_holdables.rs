//! Holdable items on this server: `ClientEvents`' `EV_USE_ITEM1..11` after a move (the
//! use key's event, `sjk_game_jka::pmove_holdable`), the generic commands' own path
//! (`GENCMD_USE_*` with `G_ItemUsable` and the external event), and every frame the
//! jetpack and cloak batteries and the seeker drone. The rules are
//! `sjk_game_jka::holdables` and `seeker_drone`, held to `holdables.c`.
//!
//! Not yet: the sentry, the E-Web and the jetpack's flight. Used, each says
//! so once and is given back (the movement had used it up), so no item is lost.

use super::*;
use sjk_game_jka::holdables::{
    self, EF_SEEKERDRONE, EV_ITEMUSEFAIL, EV_USE_ITEM0, Gear, HI_BINOCULARS, HI_CLOAK, HI_EWEB,
    HI_HEALTHDISP, HI_JETPACK, HI_MEDPAC, HI_MEDPAC_BIG, HI_SEEKER, HI_SENTRY_GUN, HI_SHIELD,
    Holder, MEDPACK_BIG_HEAL, MEDPACK_HEAL, Q3_INFINITE, Toggled, Wearer, Zoom,
};
use sjk_game_jka::seeker_drone::{self, Candidate, Deed, DroneWorld, Owner};

/// Player-state fields: `eFlags`, `genericEnemyIndex`, `weaponstate`, the zoom,
/// `jetpackFuel`, `cloakFuel`, `externalEvent`, `externalEventParm`, `pm_flags`.
const PS_EFLAGS: usize = 17;
const PS_GENERIC_ENEMY: usize = 26;
const PS_WEAPON_STATE: usize = 33;
const PS_ZOOM_MODE: usize = 90;
const PS_ZOOM_TIME: usize = 92;
const PS_ZOOM_LOCKED: usize = 94;
const PS_ZOOM_FOV: usize = 95;
const PS_JETPACK_FUEL: usize = 39;
const PS_CLOAK_FUEL: usize = 40;
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
/// `powerups[PW_CLOAKED]`, `stats[STAT_HOLDABLE_ITEM]`, `stats[STAT_HOLDABLE_ITEMS]`,
/// `stats[STAT_MAX_HEALTH]`.
const PW_CLOAKED: usize = 11;
const STAT_HOLDABLE_ITEM: usize = 1;
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_MAX_HEALTH: usize = 8;
/// `GENCMD_USE_SEEKER` .. `GENCMD_USE_CLOAK`, in `genCmds_t`'s order, as holdable tags.
const GENCMD_USE_SEEKER: u8 = 14;
const GENCMD_USE_CLOAK: u8 = 25;
const GENERIC_TAGS: [i32; 12] = [
    HI_SEEKER,
    HI_SHIELD,
    HI_MEDPAC,
    HI_BINOCULARS,
    HI_BINOCULARS,
    HI_SENTRY_GUN,
    HI_JETPACK,
    HI_MEDPAC_BIG,
    HI_HEALTHDISP,
    holdables::HI_AMMODISP,
    HI_EWEB,
    HI_CLOAK,
];
/// The holdables this server cannot use yet.
const UNPORTED: [i32; 3] = [HI_SENTRY_GUN, HI_JETPACK, HI_EWEB];
/// `EV_PLAY_EFFECT`, `EFFECT_SPARK_EXPLOSION`, `s.angles`, `s.origin`.
const EV_PLAY_EFFECT: u32 = 68;
const EFFECT_SPARK_EXPLOSION: u32 = 4;
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
/// `CHAN_WEAPON`, `CHAN_ITEM`, `CHAN_BODY`, `CHAN_AUTO`, carried in `saberEntityNum`.
const CHAN_WEAPON: u32 = 2;
const CHAN_ITEM: u32 = 4;
const CHAN_BODY: u32 = 5;
const ES_SABER_ENTITY: usize = 31;

impl NativeGame {
    /// `ClientEvents`' `EV_USE_ITEM1..11`: the use key's item `tag`, used by the move.
    pub(super) fn use_item_of_move(&mut self, client: usize, tag: i32, level_time: i32) {
        self.use_item(client, tag, level_time, true);
    }

    /// The generic commands `GENCMD_USE_*` (`g_active.c:3171-3300`): the item, if held
    /// and usable (`G_ItemUsable`), used, and the external event its user's client plays.
    /// Returns whether `generic` was one of them.
    pub(super) fn generic_holdable(&mut self, client: usize, generic: u8, level_time: i32) -> bool {
        if !(GENCMD_USE_SEEKER..=GENCMD_USE_CLOAK).contains(&generic) {
            return false;
        }
        let tag = GENERIC_TAGS[usize::from(generic - GENCMD_USE_SEEKER)];
        let Some(peer) = self.peer(client) else {
            return true;
        };
        if peer.state.stats[STAT_HOLDABLE_ITEMS] & (1 << tag) == 0 {
            return true;
        }
        let holder = holder_of(peer);
        self.gather_obstacles(client);
        let usable = match self.map.as_ref() {
            Some(map) => holdables::usable(
                &holder,
                tag,
                false,
                &WithPlayers {
                    world: WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    players: &self.obstacles,
                },
            ),
            None => holdables::usable(
                &holder,
                tag,
                false,
                &WithPlayers {
                    world: Void,
                    players: &self.obstacles,
                },
            ),
        };
        match usable {
            Err(Some(fail)) => {
                self.external_event(client, u32::from(EV_ITEMUSEFAIL), fail as u32, level_time)
            }
            Err(None) => {}
            Ok(()) if tag == HI_CLOAK => {
                // `Jedi_Cloak`/`Jedi_Decloak` straight: no toggle time, no battery check.
                let cloaked = self
                    .peer(client)
                    .is_some_and(|peer| peer.state.powerups[PW_CLOAKED] != 0);
                self.cloak_toggled(
                    client,
                    if cloaked {
                        Toggled::Decloaked
                    } else {
                        Toggled::Cloaked
                    },
                    level_time,
                );
            }
            Ok(()) if UNPORTED.contains(&tag) => self.told_unported_holdable(tag),
            Ok(()) => {
                // The dispensers' `ItemUse_UseDisp` is commented out: only the event.
                if !matches!(tag, HI_HEALTHDISP | holdables::HI_AMMODISP) {
                    self.use_item(client, tag, level_time, false);
                }
                let zoomed = self
                    .peer(client)
                    .map_or(0, |peer| peer.state.raw_field(PS_ZOOM_MODE).unwrap_or(0));
                let parm = if tag == HI_BINOCULARS {
                    if zoomed == 0 { 1 } else { 2 }
                } else {
                    0
                };
                self.external_event(
                    client,
                    u32::from(EV_USE_ITEM0) + tag as u32,
                    parm,
                    level_time,
                );
                if matches!(
                    tag,
                    HI_SEEKER | HI_SHIELD | HI_MEDPAC | HI_MEDPAC_BIG | HI_SENTRY_GUN
                ) && let Some(peer) = self.peer_mut(client)
                {
                    peer.state.stats[STAT_HOLDABLE_ITEMS] &= !(1 << tag);
                }
            }
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        true
    }

    /// `ItemUse_*` for `tag` by `client`; `moved` when the use key's movement used it (and
    /// so already took a used-up item away).
    fn use_item(&mut self, client: usize, tag: i32, level_time: i32, moved: bool) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        match tag {
            HI_MEDPAC | HI_MEDPAC_BIG => {
                let amount = if tag == HI_MEDPAC {
                    MEDPACK_HEAL
                } else {
                    MEDPACK_BIG_HEAL
                };
                let dead = peer.state.health() <= 0
                    || peer.state.raw_field(PS_EFLAGS).unwrap_or(0) & holdables::EF_DEAD != 0;
                peer.health = holdables::medpack_give(
                    peer.health,
                    peer.state.stats[STAT_MAX_HEALTH] as i32,
                    dead,
                    amount,
                );
                // `MedPackGive` changes entity health only. `ClientEndFrame`
                // publishes STAT_HEALTH and updates the movement state.
            }
            HI_BINOCULARS => {
                let ready = peer.state.raw_field(PS_WEAPON_STATE).unwrap_or(0) == 0;
                let zoom = peer.state.raw_field(PS_ZOOM_MODE).unwrap_or(0) as u8;
                match holdables::binoculars(zoom, ready, level_time) {
                    Some(Zoom::On) => {
                        peer.state.set_raw_field(PS_ZOOM_MODE, 2);
                        peer.state.set_raw_field(PS_ZOOM_LOCKED, 0);
                        peer.state.set_raw_field(PS_ZOOM_FOV, 40.0_f32.to_bits());
                    }
                    Some(Zoom::Off(time)) => {
                        peer.state.set_raw_field(PS_ZOOM_MODE, 0);
                        peer.state.set_raw_field(PS_ZOOM_TIME, time as u32);
                    }
                    None => {}
                }
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            HI_SEEKER => {
                // Siege's `d_siegeSeekerNPC` (a remote NPC) is off by default: the drone.
                let flags = peer.state.raw_field(PS_EFLAGS).unwrap_or(0);
                peer.state.set_raw_field(PS_EFLAGS, flags | EF_SEEKERDRONE);
                peer.gear.drone_exist = (level_time + 30_000) as f32;
                peer.gear.drone_fire = (level_time + 1_500) as f32;
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            HI_CLOAK => {
                let wearer = wearer_of(peer);
                let toggled = holdables::use_cloak(&mut peer.gear.packs, &wearer, level_time);
                self.cloak_toggled(client, toggled, level_time);
            }
            HI_SHIELD => self.place_shield(client, level_time),
            HI_JETPACK | HI_SENTRY_GUN | HI_EWEB => {
                // Given back: the movement used a sentry up.
                if moved && tag == HI_SENTRY_GUN {
                    peer.state.stats[STAT_HOLDABLE_ITEMS] |= 1 << tag;
                    peer.state.stats[STAT_HOLDABLE_ITEM] = holdables::index_of(tag);
                    peer.movement = peer.movement.reseeded(&peer.state);
                }
                self.told_unported_holdable(tag);
            }
            _ => {}
        }
    }

    /// A holdable this server cannot use yet, said once per process.
    fn told_unported_holdable(&mut self, tag: i32) {
        static TOLD: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        let name = [
            "",
            "seeker",
            "shield",
            "medpac",
            "medpac_big",
            "binoculars",
            "sentry",
            "jetpack",
            "healthdisp",
            "ammodisp",
            "eweb",
            "cloak",
        ][tag as usize];
        if TOLD.fetch_or(1 << tag, std::sync::atomic::Ordering::Relaxed) & (1 << tag) == 0 {
            eprintln!("holdable {name} used: not ported yet, given back");
        }
    }

    /// `Jedi_Cloak`/`Jedi_Decloak` on a player: `PW_CLOAKED` and the sound.
    fn cloak_toggled(&mut self, client: usize, toggled: Toggled, level_time: i32) {
        let (cloak, sound): (i32, &[u8]) = match toggled {
            Toggled::Cloaked => (Q3_INFINITE, b"sound/chars/shadowtrooper/cloak.wav"),
            Toggled::Decloaked => (0, b"sound/chars/shadowtrooper/decloak.wav"),
            _ => return,
        };
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        peer.state.powerups[PW_CLOAKED] = cloak as u32;
        peer.movement = peer.movement.reseeded(&peer.state);
        let origin = peer.state.origin();
        self.holdable_sound(sound, origin, CHAN_ITEM, level_time);
    }

    /// `G_AddEvent` on a player: its external event (`EV_EVENT_BITS` stepped).
    fn external_event(&mut self, client: usize, event: u32, parm: u32, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let bits = (peer.state.raw_field(PS_EXTERNAL_EVENT).unwrap_or(0) & 0x300)
            .wrapping_add(0x100)
            & 0x300;
        peer.state.set_raw_field(PS_EXTERNAL_EVENT, event | bits);
        peer.state.set_raw_field(PS_EXTERNAL_EVENT_PARM, parm);
        peer.entity.event_raised(level_time);
    }

    /// `G_Sound`/`G_SoundAtLoc`: a sound event at `origin` on `channel`.
    fn holdable_sound(&mut self, name: &[u8], origin: [f32; 3], channel: u32, level_time: i32) {
        let told = &mut self.told;
        let index = self.sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        });
        let mut sound = EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(index),
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        sound.extra[0] = (ES_SABER_ENTITY, channel);
        let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
    }

    /// `G_RunFrame`'s jetpack and cloak batteries for a player (`g_main.c:3246-3301`):
    /// drained while on, recharged while off, the cloak dropped when it runs dry.
    pub(super) fn battery_frame(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (mut jet, mut cloak) = (
            peer.state.raw_field(PS_JETPACK_FUEL).unwrap_or(0) as i32,
            peer.state.raw_field(PS_CLOAK_FUEL).unwrap_or(0) as i32,
        );
        let thrusting = peer.last_command.up_move > 0;
        let cloaked = peer.state.powerups[PW_CLOAKED] != 0;
        let off = holdables::batteries(
            &mut peer.gear.packs,
            &mut jet,
            &mut cloak,
            cloaked,
            thrusting,
            level_time,
        );
        if (jet, cloak)
            != (
                peer.state.raw_field(PS_JETPACK_FUEL).unwrap_or(0) as i32,
                peer.state.raw_field(PS_CLOAK_FUEL).unwrap_or(0) as i32,
            )
        {
            peer.state.set_raw_field(PS_JETPACK_FUEL, jet as u32);
            peer.state.set_raw_field(PS_CLOAK_FUEL, cloak as u32);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        if off == Toggled::Decloaked {
            self.cloak_toggled(client, off, level_time);
        }
    }

    /// `SeekerDroneUpdate` for `client`, from `WP_ForcePowersUpdate`.
    pub(super) fn seeker_drone(&mut self, client: usize, level_time: i32) {
        // No drone: `genericEnemyIndex` -1, and nothing to look at.
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.state.raw_field(PS_EFLAGS).unwrap_or(0) & EF_SEEKERDRONE == 0 {
            peer.state.set_raw_field(PS_GENERIC_ENEMY, u32::MAX);
            return;
        }
        let peer = self.peer(client).unwrap();
        let world = Sight {
            game: self,
            owner: client,
        };
        let mut rng = self.deaths.rng;
        let mut flags = peer.state.raw_field(PS_EFLAGS).unwrap_or(0);
        let mut enemy = peer.state.raw_field(PS_GENERIC_ENEMY).unwrap_or(0) as i32;
        let Gear {
            mut drone_exist,
            mut drone_fire,
            ..
        } = peer.gear;
        let mut owner = Owner {
            number: client as u16,
            health: peer.health,
            origin: peer.state.origin(),
            view_angles: peer.state.view_angles(),
            entity_flags: &mut flags,
            exist_time: &mut drone_exist,
            fire_time: &mut drone_fire,
            enemy: &mut enemy,
        };
        let deeds = seeker_drone::update(&mut owner, &world, level_time, &mut rng);
        self.deaths.rng = rng;
        let peer = self.peer_mut(client).unwrap();
        (peer.gear.drone_exist, peer.gear.drone_fire) = (drone_exist, drone_fire);
        if flags != peer.state.raw_field(PS_EFLAGS).unwrap_or(0) {
            peer.state.set_raw_field(PS_EFLAGS, flags);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        peer.state.set_raw_field(PS_GENERIC_ENEMY, enemy as u32);
        let origin = peer.state.origin();
        if let Some(deed) = deeds {
            match deed {
                Deed::Spark(at) => {
                    let mut effect = EventEntity {
                        event: EV_PLAY_EFFECT,
                        parameter: EFFECT_SPARK_EXPLOSION,
                        origin: at,
                        client: None,
                        broadcast: false,
                        extra: [(0, 0); 12],
                    };
                    for axis in 0..3 {
                        effect.extra[axis] = (
                            ES_ANGLES[axis],
                            if axis == 0 { 1.0_f32 } else { 0.0 }.to_bits(),
                        );
                        effect.extra[3 + axis] = (ES_ORIGIN[axis], at[axis].to_bits());
                    }
                    let _ = self.pool.spawn_temporary(effect.state(), level_time, None);
                }
                Deed::Warning => self.holdable_sound(
                    b"sound/weapons/laser_trap/warning.wav",
                    origin,
                    CHAN_BODY,
                    level_time,
                ),
                Deed::Fire(missile) => {
                    let at = std::array::from_fn(|axis| {
                        f32::from_bits(missile.state.raw_field([2, 1, 4][axis]).unwrap_or(0))
                    });
                    if let Some(id) = self.pool.spawn_entity(missile.state.clone(), level_time) {
                        self.pool.set_bounds(id, missile.bounds);
                        self.missiles.push((id, missile));
                    }
                    self.holdable_sound(
                        b"sound/weapons/bryar/fire.wav",
                        at,
                        CHAN_WEAPON,
                        level_time,
                    );
                }
            }
        }
    }
}

/// `PM_ItemUsable`'s and `G_ItemUsable`'s view of a player.
fn holder_of(peer: &Peer) -> Holder {
    Holder {
        client: peer.state.client_num(),
        origin: peer.state.origin(),
        view_angles: peer.state.view_angles(),
        health: peer.state.stats[0] as i16 as i32,
        max_health: peer.state.stats[STAT_MAX_HEALTH] as i16 as i32,
        entity_flags: peer.state.raw_field(PS_EFLAGS).unwrap_or(0),
        movement_flags: peer.state.movement_flags(),
        riding: peer.state.vehicle_entity_num() != 0,
        duel_in_progress: peer.state.duel_in_progress(),
        sentry_deployed: peer.state.raw_field(106).unwrap_or(0) != 0,
    }
}

/// The jetpack's and cloak's view of a player.
fn wearer_of(peer: &Peer) -> Wearer {
    Wearer {
        health: peer.health,
        dead: peer.state.raw_field(PS_EFLAGS).unwrap_or(0) & holdables::EF_DEAD != 0,
        jetpack_fuel: peer.state.raw_field(PS_JETPACK_FUEL).unwrap_or(0) as i32,
        cloak_fuel: peer.state.raw_field(PS_CLOAK_FUEL).unwrap_or(0) as i32,
        cloaked: peer.state.powerups[PW_CLOAKED] != 0,
        gripped: false,
        falling_to_death: false,
    }
}

/// What the drone sees: player slots and the map's static and moving brushes.
struct Sight<'a> {
    game: &'a NativeGame,
    owner: usize,
}

impl Sight<'_> {
    /// A point trace, `MASK_SOLID`.
    fn trace(&self, from: [f32; 3], to: [f32; 3]) -> sjk_game_jka::pmove::MovementTrace {
        let Some(map) = &self.game.map else {
            return sjk_game_jka::pmove::MovementTrace::miss(to);
        };
        let world = WorldCollision {
            bsp: &map.bsp,
            scratch: &map.scratch,
        };
        // Trace existing brush entities without gathering another obstacle buffer.
        let brushes = crate::collision::brush_obstacles(
            &self.game.breakables,
            &self.game.doors,
            self.game.last_frame_time,
        )
        .chain(self.game.stock.obstacles())
        .chain(self.game.scripts.brushes());
        WithPlayers {
            world,
            players: &[],
        }
        .trace_through(brushes, from, [0.0; 3], [0.0; 3], to, 0x1001)
    }
}

impl DroneWorld for Sight<'_> {
    fn slots(&self) -> usize {
        self.game.players.places()
    }
    fn candidate(&self, slot: usize) -> Option<Candidate> {
        let peer = self.game.peer(slot)?;
        let same_team = self.game.gametype >= GAMETYPE_TEAM
            && slot != self.owner
            && self
                .game
                .peer(self.owner)
                .is_some_and(|owner| owner.session.team == peer.session.team);
        Some(Candidate {
            number: slot as u16,
            origin: peer.state.origin(),
            health: peer.health,
            same_team,
            in_play: peer.body_active()
                && peer.state.movement_type() != sjk_game_jka::PM_INTERMISSION,
            client: true,
        })
    }
    fn visible(&self, from: [f32; 3], to: [f32; 3], _: u16) -> bool {
        self.trace(from, to).fraction == 1.0
    }
    fn clear(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        let trace = self.trace(from, to);
        trace.fraction == 1.0 && !trace.start_solid && !trace.all_solid
    }
}

#[path = "bridge_shields.rs"]
pub(super) mod shields;

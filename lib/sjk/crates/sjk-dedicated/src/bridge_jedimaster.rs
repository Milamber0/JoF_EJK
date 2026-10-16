//! Jedi Master on the server: the saber placed as the level starts, run every frame and
//! taken by touch, the masters as damage and death read them, the saber a dead or
//! departed master loses, and the master seen from afar. The rules are
//! [`sjk_game_jka::jedi_master`]'s.

use super::{NativeGame, Told};
use crate::collision::{Void, WorldCollision};
use sjk_game_jka::jedi_master::{
    self, CS_CLIENT_JEDIMASTER, GT_JEDIMASTER, Holder, JediMasterSaber, Masters, Toucher, is_master,
};
use sjk_game_jka::weapon_fire::MissileFrame;

/// What the server keeps of a Jedi Master game.
#[derive(Default)]
pub(super) struct JediMaster {
    /// The level's saber, in a Jedi Master game.
    pub(super) saber: Option<JediMasterSaber>,
    /// Where the holder that left the game stood (its entity's `s.pos.trBase`), for the
    /// saber's next think to drop it there.
    departed_holder: Option<(u16, [f32; 3])>,
    /// The slots of masters who left: the reference's `G_ThereIsAMaster` still counts
    /// them (`ps.isJediMaster` is never cleared) until someone connects into the slot.
    departed_masters: Vec<usize>,
}

impl NativeGame {
    /// `SP_info_jedimaster_start` for the level (and `G_InitGame`'s `-1` for
    /// `CS_CLIENT_JEDIMASTER`): at the map's first saber spot, or else at a random
    /// deathmatch spawn point (`g_main.c:414-436`); nothing outside Jedi Master. A map
    /// with several spots gets one saber, at the first.
    pub(super) fn init_jedi_master(&mut self, level_time: i32) {
        if let Some(saber) = self.jedi_master.saber.take() {
            self.pool.free(saber.id, level_time);
        }
        // `G_InitGame` clears every client (`memset`): no departed master counts.
        self.jedi_master.departed_holder = None;
        self.jedi_master.departed_masters.clear();
        if self.gametype != GT_JEDIMASTER {
            return;
        }
        self.publish_config_string(CS_CLIENT_JEDIMASTER, b"-1");
        let Some(map) = self.map.as_ref() else { return };
        let origin = match map.jedi_master_starts.first() {
            Some(origin) => *origin,
            None => {
                let spots: Vec<[f32; 3]> = map
                    .spawn_points
                    .iter()
                    .filter(|point| point.kind == sjk_game_jka::map::SpawnKind::Deathmatch)
                    .map(|point| point.origin)
                    .collect();
                if spots.is_empty() {
                    return;
                }
                spots[self.rand.next() as usize % spots.len()]
            }
        };
        let model = u32::from(self.saber_model_index(jedi_master::DEFAULT_SABER_MODEL));
        let Some(number) = self.pool.spawn_entity(
            sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
            level_time,
        ) else {
            return;
        };
        let saber = JediMasterSaber::spawn(number, origin, model, level_time);
        self.publish_jedi_master(&saber);
        self.jedi_master.saber = Some(saber);
    }

    /// `G_ModelIndex` for a saber's model, published as it is first registered.
    fn saber_model_index(&mut self, name: &[u8]) -> u16 {
        let NativeGame { models, told, .. } = self;
        models.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    fn publish_jedi_master(&mut self, saber: &JediMasterSaber) {
        self.pool.set_state(saber.id, &saber.missile.state);
    }

    /// The saber's frame: its fall and bounces as a missile, or its drift as an object
    /// while held, and its think.
    pub(super) fn run_jedi_master(&mut self, server_time: i32) {
        let Some(mut saber) = self.jedi_master.saber.take() else {
            return;
        };
        let previous_time = self.previous_frame_time;
        let holder = match (saber.holder, self.jedi_master.departed_holder) {
            (Some(holder), Some((departed, base))) if holder == departed => Holder::Gone { base },
            _ => Holder::Present,
        };
        let Self {
            map,
            deaths,
            sounds,
            told,
            pool,
            ..
        } = self;
        let homing = NoTargets;
        let mut sounds = |name: &[u8]| {
            sounds.index(name, &mut |index, value| {
                told.push(Told::ConfigString {
                    index,
                    previous: Vec::new(),
                    value: value.to_vec(),
                })
            })
        };
        let mut raise = |event: sjk_game_jka::event_entity::EventEntity| {
            let _ = pool.spawn_temporary(event.state(), server_time, None);
        };
        let mut frame = MissileFrame {
            homing: &homing,
            rng: &mut deaths.rng,
            sounds: &mut sounds,
            raise: &mut raise,
            models: None,
            npcs: &|_| false,
        };
        match map {
            Some(map) => saber.run_frame(
                server_time,
                previous_time,
                holder,
                &WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                &mut frame,
            ),
            None => saber.run_frame(server_time, previous_time, holder, &Void, &mut frame),
        }
        if saber.holder.is_none() {
            self.jedi_master.departed_holder = None;
        }
        self.publish_jedi_master(&saber);
        self.jedi_master.saber = Some(saber);
    }

    /// `G_TouchTriggers`' part for the saber (`JMSaberTouch`): a living player whose box
    /// meets it may take it, and is announced as the master.
    pub(super) fn touch_jedi_master(&mut self, client: usize, level_time: i32) {
        let Some(mut saber) = self.jedi_master.saber.take() else {
            return;
        };
        let became = self.peer_mut(client).and_then(|peer| {
            let (bottom, top) = peer.movement.box_bounds();
            if peer.health <= 0
                || !peer.playing()
                || !saber.touched_by(peer.state.origin(), (bottom, top))
            {
                return None;
            }
            let toucher = Toucher {
                client: client as u16,
                state: &mut peer.state,
                entity: peer.entity.state_mut(),
                health: &mut peer.health,
                force: &mut peer.force,
                invulnerable_until: &mut peer.invulnerable_until,
            };
            let became = saber.touch(toucher, level_time)?;
            peer.entity.event_raised(level_time);
            peer.movement = peer.movement.reseeded(&peer.state);
            Some((became, peer.name.clone()))
        });
        self.publish_jedi_master(&saber);
        let number = saber.id.legacy_number();
        self.jedi_master.saber = Some(saber);
        let Some((became, name)) = became else { return };
        // `G_KillG2Queue`: its model goes with it.
        self.told
            .push(Told::Everyone(format!("kg2 {number}").into_bytes()));
        self.publish_config_string(CS_CLIENT_JEDIMASTER, became.client.to_string().as_bytes());
        let mut message = b"cp \"".to_vec();
        message.extend_from_slice(&name);
        message.extend_from_slice(b" @@@BECOMEJM\n\"");
        self.told.push(Told::Everyone(message));
    }

    /// `G_ThereIsAMaster`: a master in the game, or one who left and whose slot nobody has
    /// taken since.
    fn master_about(&self) -> bool {
        !self.jedi_master.departed_masters.is_empty()
            || (0..self.players.places())
                .any(|client| self.peer(client).is_some_and(|peer| is_master(&peer.state)))
    }

    /// `G_Damage`'s Jedi Master rule between `attacker` and `target`.
    pub(super) fn jedi_master_spares(&self, attacker: Option<u16>, target: usize) -> bool {
        let Some(attacker) = attacker
            .map(usize::from)
            .filter(|attacker| *attacker != target)
        else {
            return false;
        };
        let master = |client: usize| self.peer(client).is_some_and(|peer| is_master(&peer.state));
        self.peer(attacker).is_some()
            && jedi_master::spares(
                self.gametype,
                master(attacker),
                master(target),
                self.master_about(),
            )
    }

    /// The masters as `player_die` reads them (`G_GetJediMaster`: one in the game), in a
    /// Jedi Master game.
    pub(super) fn jedi_masters(&self, killer: Option<u16>) -> Option<Masters> {
        (self.gametype == GT_JEDIMASTER).then(|| Masters {
            killer_is_master: killer
                .and_then(|killer| self.peer(usize::from(killer)))
                .is_some_and(|peer| is_master(&peer.state)),
            master: (0..self.players.places())
                .find(|client| {
                    self.peer(*client)
                        .is_some_and(|peer| is_master(&peer.state))
                })
                .map(|client| client as u16),
        })
    }

    /// What a death left of Jedi Master: the master's point for a death between two
    /// others (`AddScore`), and the saber a dead master lost — flung to its killer or
    /// sent home (`ThrowSaberToAttacker`), `CS_CLIENT_JEDIMASTER` back to `-1`.
    pub(super) fn jedi_master_died(
        &mut self,
        client: usize,
        master_point: Option<u16>,
        saber_lost: Option<Option<u16>>,
    ) {
        if let Some(master) = master_point
            .filter(|_| self.scoring())
            .and_then(|master| self.peer_mut(usize::from(master)))
        {
            master.state.persistent[0] = master.state.persistent[0].wrapping_add(1);
        }
        let Some(killer) = saber_lost else { return };
        let Some(mut saber) = self.jedi_master.saber.take() else {
            return;
        };
        self.publish_config_string(CS_CLIENT_JEDIMASTER, b"-1");
        let toward = killer
            .and_then(|killer| self.peer(usize::from(killer)))
            .map(|killer| killer.state.origin());
        let model_name = self
            .peer(client)
            .map(|dead| dead.sabers.hands[0].model.clone())
            .unwrap_or_default();
        let model = if model_name.is_empty() {
            jedi_master::DEFAULT_SABER_MODEL.to_vec()
        } else {
            model_name
        };
        let model = u32::from(self.saber_model_index(&model));
        let flying = self
            .peer(client)
            .and_then(|dead| self.pool.state(dead.saber_entity?))
            .cloned();
        if let Some(dead) = self.peer_mut(client) {
            let entity = dead.entity.state();
            let base = std::array::from_fn(|axis| {
                f32::from_bits(entity.raw_field([2, 1, 4][axis]).unwrap_or(0))
            });
            // The master's own thrown saber, whose flight the level's saber takes over.
            let in_flight = flying.map(|state| {
                let vector = |fields: [usize; 3]| {
                    std::array::from_fn(|axis| {
                        f32::from_bits(state.raw_field(fields[axis]).unwrap_or(0))
                    })
                };
                jedi_master::InFlight {
                    pos_base: vector([2, 1, 4]),
                    pos_delta: vector([6, 7, 10]),
                    apos_base: vector([5, 3, 33]),
                    apos_delta: vector([48, 44, 49]),
                    current: dead.flight.current,
                    current_angles: dead.flight.current_angles,
                }
            });
            saber.throw_to_attacker(&mut dead.state, base, model, toward, in_flight);
            dead.movement = dead.movement.reseeded(&dead.state);
        }
        self.publish_jedi_master(&saber);
        self.jedi_master.saber = Some(saber);
    }

    /// `ClientDisconnect`'s part: a master who leaves drops the saber where it stood,
    /// for the saber's next think, and keeps counting as a master.
    pub(super) fn jedi_master_left(&mut self, client: usize) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        if !is_master(&peer.state) {
            return;
        }
        let entity = peer.entity.state();
        let base = std::array::from_fn(|axis| {
            f32::from_bits(entity.raw_field([2, 1, 4][axis]).unwrap_or(0))
        });
        self.jedi_master.departed_holder = Some((client as u16, base));
        self.jedi_master.departed_masters.push(client);
    }

    /// `ClientConnect` clears the slot (`memset`): a master who left it is forgotten.
    pub(super) fn jedi_master_slot_taken(&mut self, client: usize) {
        self.jedi_master
            .departed_masters
            .retain(|slot| *slot != client);
    }

    /// `G_UpdateClientBroadcasts` at `client`'s think: every other client in the game it
    /// is sent to wherever it stands (the master in view, or to Force sight), kept on the
    /// peer for its snapshots.
    pub(super) fn update_client_broadcasts(&mut self, client: usize) {
        let gametype = self.gametype;
        let Some(this) = self.peer(client).map(|peer| peer.state.clone()) else {
            return;
        };
        let mut words = std::mem::take(&mut self.broadcast_scratch);
        words.clear();
        words.resize(self.players.places().div_ceil(64), 0);
        // A siege item's carrier is sent to everyone (`SVF_BROADCAST`, `SiegeItemTouch`).
        let carrying = self
            .peer(client)
            .is_some_and(|peer| peer.siege_hands.holding != 0);
        for other in (0..self.players.places()).filter(|other| *other != client) {
            if self.peer(other).is_some_and(|peer| {
                peer.begun && (carrying || jedi_master::broadcast_to(gametype, &this, &peer.state))
            }) {
                words[other / 64] |= 1 << (other % 64);
            }
        }
        if let Some(peer) = self.peer_mut(client) {
            peer.broadcast_to.clear();
            peer.broadcast_to.extend_from_slice(&words);
        }
        self.broadcast_scratch = words;
    }
}

/// A saber no homing rocket ever asks about.
struct NoTargets;

impl sjk_game_jka::weapon_fire::HomingTargets for NoTargets {
    fn target(&self, _: u16) -> Option<sjk_game_jka::weapon_fire::HomingTarget> {
        None
    }
}

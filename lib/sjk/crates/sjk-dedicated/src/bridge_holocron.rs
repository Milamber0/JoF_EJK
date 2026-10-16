//! Holocron FFA on the server: the map's holocrons placed as the level starts, run every
//! frame and taken by touch. The rules are [`sjk_game_jka::holocron`]'s; the powers they
//! carry are the Force update's.

use super::NativeGame;
use crate::collision::{Void, WorldCollision};
use sjk_game_jka::holocron::{CarrierView, GT_HOLOCRON, Holocron, MAX_CARRY, Toucher};

impl NativeGame {
    /// `SP_misc_holocron` for each of the map's, in Holocron FFA: one starting in something
    /// solid is dropped, as the reference frees it; a saber-only server has none of the
    /// saber's three.
    pub(super) fn init_holocrons(&mut self, level_time: i32) {
        for holocron in std::mem::take(&mut self.holocrons) {
            self.pool.free(holocron.id, level_time);
        }
        if self.gametype != GT_HOLOCRON {
            return;
        }
        let saber_only = self.settings.saber_only();
        let Some(map) = self.map.as_ref() else { return };
        let world = WorldCollision {
            bsp: &map.bsp,
            scratch: &map.scratch,
        };
        for (origin, power) in map.holocrons.clone() {
            if saber_only && (15..=17).contains(&power) {
                continue;
            }
            let Some(number) = self.pool.spawn_entity(
                sjk_protocol::EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
                level_time,
            ) else {
                break;
            };
            match Holocron::spawn(number, origin, power, level_time, &world) {
                Some(holocron) => {
                    self.pool.set_state(number, &holocron.missile.state);
                    self.holocrons.push(holocron);
                }
                None => self.pool.free(number, level_time),
            }
        }
    }

    /// Every holocron's frame; a carrier gone from the server is no carrier to it.
    pub(super) fn run_holocrons(&mut self, server_time: i32) {
        let previous_time = self.previous_frame_time;
        for index in 0..self.holocrons.len() {
            let carrier = self.holocrons[index].carrier.map(usize::from);
            let Self {
                server,
                world,
                players,
                map,
                deaths,
                holocrons,
                pool,
                ..
            } = self;
            let holocron = &mut holocrons[index];
            let peer = carrier
                .and_then(|client| players.at(client))
                .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
            let view = peer.map(|peer| CarrierView {
                in_game: true,
                health: peer.health,
                state: &mut peer.state,
                carried: &mut peer.force.holocrons,
            });
            match map {
                Some(map) => holocron.run_frame(
                    server_time,
                    previous_time,
                    view,
                    &WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    &mut deaths.rng,
                ),
                None => {
                    holocron.run_frame(server_time, previous_time, view, &Void, &mut deaths.rng)
                }
            }
            pool.set_state(holocron.id, &holocron.missile.state);
        }
    }

    /// `G_TouchTriggers`' part for the holocrons: a living player whose box meets one may
    /// take it.
    pub(super) fn touch_holocrons(&mut self, client: usize, level_time: i32) {
        for index in 0..self.holocrons.len() {
            let Self {
                server,
                world,
                players,
                holocrons,
                pool,
                ..
            } = self;
            let holocron = &mut holocrons[index];
            let Some(peer) = players
                .at(client)
                .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
            else {
                return;
            };
            let (bottom, top) = peer.movement.box_bounds();
            if peer.health <= 0
                || !peer.playing()
                || !holocron.touched_by(peer.state.origin(), (bottom, top))
            {
                continue;
            }
            let toucher = Toucher {
                client: client as u16,
                health: peer.health,
                state: &mut peer.state,
                carried: &mut peer.force.holocrons,
            };
            if holocron.touch(toucher, MAX_CARRY, level_time) {
                peer.entity.event_raised(level_time);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            pool.set_state(holocron.id, &holocron.missile.state);
        }
    }
}

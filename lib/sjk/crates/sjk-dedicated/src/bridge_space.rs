//! Players and the space-ship triggers on this server (`trigger_space`,
//! `trigger_shipboundary`, `trigger_hyperspace`; the rules are
//! `sjk_game_jka::vehicle_triggers`): a player in space floats with its speed bled off
//! (`g_active.c:2382-2389`), chokes once its air is out (`g_main.c:3173-3203`), and a
//! pilot is carried through its ship's jump (`g_trigger.c:1700-1716`). A ship's own
//! touches are the roster's (the vehicle is an NPC).

use super::*;
use sjk_game_jka::vehicle_triggers::{self, ENTITYNUM_NONE};

/// `ps.eFlags2`; `EF2_SHIP_DEATH`.
const PS_EFLAGS2: usize = 103;
const EF2_SHIP_DEATH: u32 = 1 << 7;
/// `DAMAGE_NO_ARMOR`; `CHAN_VOICE`.
const DAMAGE_NO_ARMOR: u32 = 2;
const CHAN_VOICE: u32 = 3;
/// The first entity number that is not a client's.
const MAX_CLIENTS: u16 = 32;

impl NativeGame {
    /// `G_RunFrame`'s opening for client `client`: its space, its batteries, then its Force.
    pub(crate) fn client_frame(&mut self, client: usize, server_time: i32) {
        self.space_frame(client, server_time);
        self.battery_frame(client, server_time);
        self.force_update(client, server_time);
    }

    /// `ClientThink_real`'s gravity for a playing client in space: none, and a player's
    /// speed bled off to 0.8 each think; one dying in its ship floats still. (Out of space it is `g_gravity`, which the player
    /// keeps from its spawn and gets back as it leaves space, [`Self::space_frame`].)
    pub(crate) fn space_gravity(&mut self, client: usize) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if !peer.playing() {
            return;
        }
        let ship_death = peer.state.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_SHIP_DEATH != 0;
        let state = peer.movement.state_mut();
        if vehicle_triggers::in_space(peer.in_space) {
            state.gravity = 1.0;
            state.velocity = state.velocity.map(|axis| axis * 0.8);
        } else if ship_death {
            // "float there".
            state.gravity = 1.0;
            state.velocity = [0.0; 3];
        }
    }

    /// `player_die`'s `noCorpse` (`g_combat.c:2251-2255`): who dies in space, or already in
    /// its ship's death, leaves no body.
    pub(crate) fn no_corpse_in_space(&mut self, client: usize) {
        if let Some(peer) = self.peer_mut(client)
            && (vehicle_triggers::in_space(peer.in_space)
                || peer.state.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_SHIP_DEATH != 0)
        {
            peer.no_corpse = true;
        }
    }

    /// `player_die`'s pilot of a fighter (`g_combat.c:2266-2273`): once thrown off, it goes
    /// into "die in ship" mode (`EF2_SHIP_DEATH`: no body, held still where it is) over
    /// where the fighter is.
    pub(crate) fn ship_death(&mut self, client: usize, vehicle: u16) {
        let Some(origin) = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == vehicle && vehicle != 0)
            .filter(|npc| {
                npc.vehicle.as_deref().is_some_and(|vehicle| {
                    vehicle.kind() == sjk_game_jka::vehicle_fields::kind::FIGHTER
                })
            })
            .map(|npc| npc.player.origin())
        else {
            return;
        };
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let flags = peer.state.raw_field(PS_EFLAGS2).unwrap_or(0);
        peer.state.set_raw_field(PS_EFLAGS2, flags | EF2_SHIP_DEATH);
        peer.state.set_origin(origin);
        peer.movement = peer.movement.reseeded(&peer.state);
    }

    /// `G_TouchTriggers`' space for a living player: the space triggers it is in — none
    /// while a ship that hides it carries it.
    pub(crate) fn touch_space(&mut self, client: usize, level_time: i32) {
        let Self {
            server,
            world,
            players,
            npcs,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        if !peer.playing()
            || peer.state.health() <= 0
            || npcs.roster.ship_triggers.triggers.is_empty()
        {
            return;
        }
        let riding = peer.state.vehicle_entity_num();
        let hidden = (MAX_CLIENTS..ENTITYNUM_NONE).contains(&riding)
            && npcs
                .roster
                .actors
                .iter()
                .find(|npc| npc.number == riding)
                .and_then(|npc| npc.vehicle.as_deref())
                .is_some_and(|vehicle| vehicle.info.hide_rider);
        let (mins, maxs) = peer.movement.box_bounds();
        npcs.roster.player_space_touch(
            peer.state.origin(),
            mins,
            maxs,
            hidden,
            &mut peer.in_space,
            &mut peer.suffocation,
            level_time,
        );
    }

    /// `G_RunFrame`'s space for a client (`g_main.c:3173-3203`): out of the trigger it is
    /// out of space; past its air it takes 50 to 70 (`DAMAGE_NO_ARMOR`, `MOD_SUICIDE`),
    /// and if it lives chokes aloud with its hand at its throat; next 100 to 200 ms on.
    fn space_frame(&mut self, client: usize, level_time: i32) {
        let gravity = self.gravity();
        let Self {
            server,
            world,
            players,
            npcs,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        let was_in_space = vehicle_triggers::in_space(peer.in_space);
        let suffocates = npcs.roster.player_space_frame(
            peer.state.origin(),
            &mut peer.in_space,
            peer.suffocation,
            level_time,
        );
        if was_in_space && peer.in_space == 0 {
            // Out of space: `g_gravity` again from the next think.
            peer.movement.state_mut().gravity = gravity;
        }
        if !suffocates {
            return;
        }
        if peer.health > 0 {
            let damage = vehicle_triggers::suffocation_damage(&mut |low, high| {
                self.deaths.rng.irand(low, high)
            });
            let point = self.peer(client).map(|peer| peer.state.origin());
            let request = DamageRequest {
                level_time,
                attacker: None,
                direction: None,
                point,
                damage,
                flags: DAMAGE_NO_ARMOR,
                means: sjk_game_jka::means_of_death::MOD_SUICIDE,
            };
            let _ = self.hurt(client, request);
            if self.peer(client).is_some_and(|peer| peer.health > 0) {
                let name = vehicle_triggers::choke_sound(&mut |low, high| {
                    self.deaths.rng.irand(low, high)
                });
                let told = &mut self.told;
                let sound = self.sounds.index(name.as_bytes(), &mut |index, value| {
                    told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                });
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                let mut event = sjk_game_jka::knockdown::entity_sound(
                    peer.state.origin(),
                    client as u16,
                    CHAN_VOICE,
                );
                event.parameter = u32::from(sound);
                let movement = peer.movement.state_mut();
                movement.force_hand_extend = vehicle_triggers::HANDEXTEND_CHOKE;
                movement.force_hand_extend_time = level_time + vehicle_triggers::CHOKE_TIME;
                peer.knockdown.hand_extend_time = level_time + vehicle_triggers::CHOKE_TIME;
                peer.movement.write_player_state(&mut peer.state);
                let _ = self.pool.spawn_temporary(event.state(), level_time, None);
            }
        }
        let next = vehicle_triggers::next_suffocation(
            &mut |low, high| self.deaths.rng.irand(low, high),
            level_time,
        );
        if let Some(peer) = self.peer_mut(client) {
            peer.suffocation = next;
        }
    }

    /// A ship's jump: its player pilot put out with it (`TeleportPlayer`: the flashes, a
    /// unit above the place facing the far point's angles, spat out along them; the ship,
    /// its owner, spared by `G_KillBox`), then the jump's end heard where the ship now is.
    pub(crate) fn jumped(
        &mut self,
        vehicle: u16,
        pilot: Option<u16>,
        origin: [f32; 3],
        angles: [f32; 3],
        sound: u16,
        level_time: i32,
    ) {
        if let Some(pilot) = pilot.filter(|pilot| *pilot < MAX_CLIENTS) {
            let Self {
                server,
                world,
                players,
                pool,
                ..
            } = self;
            if let Some(peer) = players
                .at(usize::from(pilot))
                .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
            {
                let before = peer.state.origin();
                let _ = pool.spawn_temporary(
                    sjk_game_jka::event_entity::EventEntity::teleport_out(before, pilot).state(),
                    level_time,
                    None,
                );
                let _ = pool.spawn_temporary(
                    sjk_game_jka::event_entity::EventEntity::teleport_in(origin, pilot).state(),
                    level_time,
                    None,
                );
                let teleported =
                    sjk_game_jka::triggers::teleport_player(&mut peer.state, origin, angles, false);
                sjk_game_jka::triggers::face(
                    &mut peer.state,
                    teleported.angles,
                    peer.last_command.angles,
                );
                peer.entity.spawn_finished(&peer.state);
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        let Some(at) = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == vehicle)
            .map(|npc| npc.current_origin)
        else {
            return;
        };
        let event = sjk_game_jka::vehicle_ship_touch::jump_sound(at, sound);
        let _ = self.pool.spawn_temporary(event.state(), level_time, None);
    }
}

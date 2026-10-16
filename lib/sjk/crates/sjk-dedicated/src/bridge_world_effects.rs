//! The end of a playing client's server frame (`ClientEndFrame`, `g_active.c:3751-3781`):
//! `P_WorldEffects` — drowning, and the burns of lava and slime — then the rest the peer
//! does itself (`Peer::end_frame`: `P_DamageFeedback`, the connection flag, the health
//! stat, the entity written from the state). The rules are
//! `sjk_game_jka::world_effects`; this is where they meet the player, the damage and the
//! sound table.

use super::*;
use sjk_game_jka::world_effects::{
    CHAN_VOICE, EV_POWERUP_BATTLESUIT, LAVA, PW_BATTLESUIT, SLIME, Surroundings, drown, sizzle,
};

/// `EV_GENERAL_SOUND`, and the wire field its channel goes in (`saberEntityNum`).
const EV_GENERAL_SOUND: u32 = 76;
const ES_SABER_ENTITY: usize = 50;

impl NativeGame {
    /// `ClientEndFrame` for every client, in slot order, after `G_RunFrame`'s entities.
    pub(super) fn end_client_frames(&mut self, server_time: i32) {
        // `ClientEndFrame` opens by ending the powerups whose time is past.
        let Self {
            server,
            world,
            players,
            ..
        } = self;
        if let Some(world) = server.world_mut(*world) {
            for handle in players.holders().flatten() {
                if let Some(peer) = world.entity_mut(handle)
                    && peer.playing()
                {
                    sjk_game_jka::player_entity::expire_powerups(&mut peer.state, server_time);
                }
            }
        }
        // `ClientEndFrame` returns before any of this at an intermission
        // (`g_active.c:3742-3749`), which is what leaves `MoveClientToIntermission`'s
        // work standing: a player at the scoreboard is not drawn, sounded or moved.
        if self.match_end.intermission_time != 0 {
            return;
        }
        for client in 0..self.players.places() {
            let Self {
                server,
                world,
                players,
                ..
            } = self;
            let Some(peer) = players
                .at(client)
                .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
            else {
                continue;
            };

            peer.entity.frame_began(&mut peer.state, server_time);
            if peer.playing() {
                self.end_player_frame(client, server_time);
            } else {
                // `SpectatorClientEndFrame`, in slot order with the players' own.
                self.follow_end_frame(client, server_time);
            }
        }
    }

    /// `ClientEndFrame` from `P_WorldEffects` on, for a playing `client`.
    fn end_player_frame(&mut self, client: usize, level_time: i32) {
        self.world_effects(client, level_time);
        let Self {
            server,
            world,
            players,
            pool,
            ..
        } = self;
        if let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        {
            peer.end_frame(pool, level_time);
        }
    }

    /// `P_WorldEffects`: the drowning blow and its gurp once the air is out, then the lava's
    /// and the slime's, each through `G_Damage` from the world.
    fn world_effects(&mut self, client: usize, level_time: i32) {
        let Some(player) = self.surroundings(client, level_time) else {
            return;
        };
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let mut breath = peer.breath;
        let drowning = drown(&mut breath, &player, &mut self.rand);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        peer.breath = breath;
        if let Some(drowning) = drowning {
            let origin = peer.state.origin();
            peer.wounds.pain_debounce_time = drowning.pain_debounce_time;
            let index = self.sound_index(drowning.sound.as_bytes());
            let mut sound = EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(index),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            sound.extra[0] = (ES_SABER_ENTITY, CHAN_VOICE);
            let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
            self.world_blow(
                client,
                drowning.damage,
                drowning.flags,
                drowning.means,
                level_time,
            );
        }
        // The sizzle reads the player as the drowning left it.
        let Some(player) = self.surroundings(client, level_time) else {
            return;
        };
        let burns = sizzle(&player);
        if burns.battlesuit
            && let Some(peer) = self.peer_mut(client)
        {
            sjk_game_jka::player_entity::add_event(&mut peer.state, EV_POWERUP_BATTLESUIT, 0);
            peer.entity.event_raised(level_time);
        }
        for (damage, means) in [(burns.lava, LAVA), (burns.slime, SLIME)] {
            if let Some(damage) = damage {
                self.world_blow(client, damage, 0, means, level_time);
            }
        }
    }

    /// What `P_WorldEffects` reads of a playing client.
    fn surroundings(&self, client: usize, level_time: i32) -> Option<Surroundings> {
        let peer = self.peer(client)?;
        let movement = peer.movement.state();
        Some(Surroundings {
            level_time,
            water_level: movement.water_level,
            water_type: movement.water_type,
            noclip: peer.noclip,
            health: peer.health,
            battlesuit_until: peer.state.powerups[PW_BATTLESUIT] as i32,
            // `tempSpectate` (siege's wait as a spectator) is not kept on this server.
            temp_spectate: 0,
            pain_debounce_time: peer.wounds.pain_debounce_time,
        })
    }

    /// `G_Damage(ent, NULL, NULL, NULL, NULL, damage, flags, means)`: the world hurts the player.
    fn world_blow(&mut self, client: usize, damage: i32, flags: u32, means: u32, level_time: i32) {
        let request = DamageRequest {
            level_time,
            attacker: None,
            direction: None,
            point: None,
            damage,
            flags,
            means,
        };
        let _ = self.hurt(client, request);
    }

    /// `G_SoundIndex`: `name`'s place in the sound table, registered (and told to every
    /// client) the first time it is asked for.
    pub(super) fn sound_index(&mut self, name: &[u8]) -> u16 {
        let (sounds, told) = (&mut self.sounds, &mut self.told);
        sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }
}

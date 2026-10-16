//! Apply `ClientSpawn` state and reset the per-life server state.

use super::*;

impl NativeGame {
    /// What follows `ClientSpawn`'s state in the bridge, for a begin and a respawn
    /// alike: the player's string, what it is told, the flash, the spawn's own think with
    /// `command`, the frame's end for it, and the ranks.
    pub(super) fn spawned(
        &mut self,
        client: usize,
        begun: Begun,
        place: SpawnPlace,
        command: UserCommand,
        server_time: i32,
    ) {
        let gravity = self.gravity();
        let (parms, rules) = (self.saber_parms(), self.userinfo_rules());
        // The class a siege player spawns as (`ClientUserinfoChanged`'s, kept in `siegeClass`).
        let siege_class = if self.siege.is_some() {
            self.siege_class_of(client)
        } else {
            None
        };
        let duel_fraglimit = self.limits.duel_fraglimit;
        let Self {
            server,
            world,
            players,
            map,
            told,
            obstacles,
            body_legs,
            gametype,
            pool,
            sounds,
            last_frame_time,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let playing = peer.playing();
        let mut class_speed = 1.0;
        let mut class_sabers: Option<(Option<String>, Option<String>)> = None;
        (peer.state, peer.in_space, peer.no_corpse) = (begun.state, 0, false);
        peer.switch_class_time = 0;
        // `ClientSpawn`'s siege branch: what the class hands its player.
        if let Some(class) = siege_class.as_ref() {
            class_speed = class.speed;
            class_sabers = Some((class.saber1.clone(), class.saber2.clone()));
            bridge_siege_clients::apply_siege_kit(&mut peer.state, class, playing);
        }
        sjk_game_jka::power_duel::spawned(
            &mut peer.state,
            &peer.session,
            *gametype,
            duel_fraglimit,
        );
        // `ClientSpawn` wipes the client: no longer a loser of the round, nor in noclip.
        (peer.loser, peer.noclip) = (false, false);
        peer.health = peer.state.health();
        peer.corpse = None;
        peer.saber = SaberFrame::default();
        peer.throw_memory = Default::default();
        // `ClientSpawn`'s `WP_SaberInitBladeData`: the same saber entity, back in hand.
        init_saber_entity(peer, pool, server_time);
        let levels = peer
            .session
            .force
            .as_ref()
            .map_or([0; sjk_game_jka::force_powers::NUM_FORCE_POWERS], |force| {
                force.levels
            });
        peer.force.respawned(levels, server_time);
        // `ClientSpawn` clears `legsAnimExecute` and `torsoAnimExecute` with the rest of
        // the client, so the spawn's animations are installed again over the old pose.
        if let Some(skeleton) = peer.skeleton.as_mut() {
            skeleton.forget_animations();
        }
        // `ClientSpawn` clears the client but its persistent data; its air lasts 12 s.
        (peer.wounds, peer.breath.air_out_time) = (
            Wounds::default(),
            server_time + sjk_game_jka::world_effects::AIR_TIME,
        );
        peer.saber_cut = Default::default();
        peer.gear = Default::default();
        peer.time_residual = 0;
        peer.block_time = 0;
        peer.entity.spawned(place.angles);
        peer.invulnerable_until = if playing {
            server_time + SPAWN_INVULNERABILITY
        } else {
            0
        };
        // The player's string names its team: `ClientSpawn` rewrites it when it changed.
        // A class's own sabers, or the default one and none (`g_client.c:2235-2240`),
        // written into the userinfo so that the next spawn's check keeps them.
        if let Some((first, second)) = class_sabers {
            let mut host = bridge_sabers::SaberHost {
                sounds,
                told,
                rng: &mut self.deaths.rng,
            };
            for (hand, name) in [
                (0, first.as_deref().unwrap_or("Kyle")),
                (1, second.as_deref().unwrap_or("none")),
            ] {
                if let Err(error) = peer.sabers.set(&parms, hand, name.as_bytes(), &mut host) {
                    eprintln!("client {client}: {error}; the class's saber is refused");
                }
            }
            peer.userinfo = bridge_sabers::with_saber_names(&peer.userinfo, &peer.sabers);
        }
        if let Ok(accepted) = judge(
            &bridge_sabers::with_saber_names(&peer.userinfo, &peer.sabers),
            rules,
            &peer.session,
            *gametype,
            &parms,
            siege_class.as_ref(),
        ) && accepted.client_info != peer.client_info
        {
            let previous = std::mem::replace(&mut peer.client_info, accepted.client_info.clone());
            told.push(Told::PlayerString {
                client,
                previous,
                value: accepted.client_info,
            });
        }
        told.extend(
            begun
                .commands
                .into_iter()
                .map(|text| Told::One(client, text)),
        );
        told.extend(begun.entered.map(Told::Everyone));
        for event in &begun.events {
            let _ = pool.spawn_temporary(event.state(), server_time, None);
        }
        // The spawn's own think, with the speed and gravity `ClientThink_real` and
        // `SpectatorThink` set before every move.
        // `ClientThink_real` (`g_active.c:2356-2363`): `g_speed`, then a siege class's
        // own multiplier — which is why some classes run and others lumber, and which a
        // client predicts from the class file it read itself.
        let (speed, gravity) = if playing {
            (PLAYER_SPEED * class_speed, gravity)
        } else {
            (400.0, 0.0)
        };
        peer.state.set_speed(speed);
        peer.state.set_base_speed(speed as i32);
        peer.state.set_gravity(gravity as i32);
        if !playing {
            peer.state.set_movement_type(PM_SPECTATOR);
        }
        peer.movement = Predictor::from_state(
            MovementState::from_player_state(&peer.state),
            authoritative(),
        );
        if let Some(lengths) = map.as_ref().and_then(|map| map.animations.clone()) {
            peer.movement.set_animation_lengths(lengths);
        }
        peer.movement.copied_to_entity();
        // `ClientSpawn` ends with its think, the frame's end, and the snapped conversion.
        let others = crate::peer::Others {
            boxes: obstacles,
            legs: body_legs,
            gametype: *gametype,
            ghoul2_time: *last_frame_time,
        };
        let _ = peer.think(
            command,
            map.as_ref(),
            others,
            pool,
            server_time,
            &mut self.deaths.rng,
        );
        peer.end_frame(pool, server_time);
        peer.entity.spawn_finished(&peer.state);
        // `G_KillBox`: whoever stands in the spawned player's box is telefragged.
        if playing {
            let origin = peer.state.origin();
            let (bottom, top) = peer.movement.box_bounds();
            let victims: Vec<u16> = obstacles
                .iter()
                .filter(|other| {
                    (0..3).all(|axis| {
                        origin[axis] + bottom[axis]
                            <= other.origin[axis] + other.bounds.1[axis] + 1.0
                            && origin[axis] + top[axis]
                                >= other.origin[axis] + other.bounds.0[axis] - 1.0
                    })
                })
                .map(|other| other.entity)
                .collect();
            let max_health = peer.state.max_health();
            let team = peer.session.team;
            for victim in victims {
                let attacker = Attacker {
                    npc: false,
                    client: client as u16,
                    max_health,
                    team,
                    saber_knockback: [0.0; 4],
                };
                let _ = self.hurt(
                    usize::from(victim),
                    DamageRequest {
                        level_time: server_time,
                        attacker: Some(attacker),
                        direction: None,
                        point: None,
                        damage: 100_000,
                        flags: DAMAGE_NO_PROTECTION,
                        means: MOD_TELEFRAG,
                    },
                );
            }
        }
        // `ClientSpawn`'s fresh sequencer, then the ranks `ClientBegin` and `ClientRespawn` recalculate.
        self.scripts_client_spawned(client);
        self.siege_client_spawned(client);
        self.calculate_ranks();
    }
}

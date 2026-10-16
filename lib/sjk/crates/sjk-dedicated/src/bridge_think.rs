//! The part of `ClientThink_real` after the upkeep, the intermission and a follower's
//! stop: what a player in the world, or a spectator flying free, does with a command.

use super::*;

/// `WP_EMPLACED_GUN`: a gunner's shot is the gun's (`bridge_emplaced`).
const WP_EMPLACED_GUN: u8 = 17;

impl NativeGame {
    /// The rest of `ClientThink_real`, for a player in the world or a spectator flying
    /// free: the duel, the fall, the lock, the move and all that follows it.
    pub(super) fn think_in_world(
        &mut self,
        client: usize,
        mut command: UserCommand,
        server_time: i32,
    ) {
        // `ClientThink_real`'s duel: a duel begun, ended or held still (`g_active.c:2405`).
        self.duel_think(client, &mut command, self.last_frame_time);
        // The fall to death a pit's brush began ends here, three seconds on; then who
        // gets the credit for one is kept or forgotten (`g_active.c:2789-2803`).
        let level_time = self.last_frame_time;
        self.run_fall_to_death(client, level_time);
        if let Some(peer) = self.peer_mut(client) {
            // `ENTITYNUM_NONE`: in the air.
            let grounded = peer.state.ground_entity_num() != 1_023;
            peer.wounds.other_killer.end_frame(grounded, level_time);
        }
        // The saber lock's part: facing the other, the presses counted (`g_active.c:2917`).
        self.lock_think(client, &command, level_time);
        let lock_enemy = self.lock_enemy(client);
        self.gather_obstacles(client);
        self.board_from_ground(client, None, level_time);
        self.riding_move(client);
        self.space_gravity(client);
        // Whom a rocket's lock may hold on: a living foe (`OnSameTeam` in team games only).
        let own_team = self.peer_mut(client).map_or(0, |peer| peer.session.team);
        let mut foes = 0_u64;
        if let Some(world) = self.server.world(self.world) {
            for (number, handle) in self.players.holders().enumerate().take(64) {
                let foe = handle
                    .and_then(|handle| world.entity(handle))
                    .is_some_and(|peer| {
                        peer.begun
                            && peer.health > 0
                            && number != client
                            && !(own_team != 0 && peer.session.team == own_team)
                    });
                foes |= u64::from(foe) << number;
            }
        }
        let Self {
            server,
            world,
            players,
            map,
            obstacles,
            body_legs,
            gametype,
            last_frame_time,
            deaths,
            npcs,
            ..
        } = self;
        let (Some(world), Some(own)) = (server.world_mut(*world), players.at(client)) else {
            return;
        };
        // A locked player's move moves its opponent too.
        let (peer, mut opponent) = match lock_enemy.flatten().and_then(|enemy| players.at(enemy)) {
            Some(them) => match world.entity_pair_mut(own, them) {
                Some((peer, them)) => (peer, Some(them)),
                None => return,
            },
            None => match world.entity_mut(own) {
                Some(peer) => (peer, None),
                None => return,
            },
        };
        // `ClientThink_real` shows a Force push's full-body effect (`g_active.c:2083-2091`),
        // ends spawn protection before anything moves, holds a gripped player in the air
        // and floats a disintegrated one (`PM_NOCLIP`).
        let flags = peer.state.raw_field(EFLAGS).unwrap_or(0);
        if peer.push_effect_until > server_time {
            peer.state.set_raw_field(EFLAGS, flags | EF_BODYPUSH);
        } else if peer.push_effect_until != 0 {
            peer.push_effect_until = 0;
            peer.state.set_raw_field(EFLAGS, flags & !EF_BODYPUSH);
        }
        if peer.invulnerable_until <= server_time {
            let flags = peer.state.raw_field(EFLAGS).unwrap_or(0);
            peer.state.set_raw_field(EFLAGS, flags & !EF_INVULNERABLE);
        }
        // `client->noclip` heads the chain (`g_active.c`): `PM_NOCLIP` while it is on,
        // the type the rest of the chain picks once it is off. A spectator has none.
        if peer.playing()
            && let Some(kind) = sjk_game_jka::noclip::movement_type(
                peer.noclip,
                peer.state.movement_type(),
                peer.state.stats[0] as i32,
                peer.state.raw_field(EFLAGS).unwrap_or(0) & EF_DISINTEGRATION != 0,
            )
        {
            peer.state.set_movement_type(kind);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        // A gripped player floats (`PM_FLOAT`) and goes back to `PM_NORMAL` when let go.
        if let Some(kind) = sjk_game_jka::force_dark::gripped_movement_type(
            peer.state.movement_type(),
            peer.state.stats[0] as i32,
            peer.force.grip_movement_type,
        ) {
            peer.state.set_movement_type(kind);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        if peer.state.raw_field(EFLAGS).unwrap_or(0) & EF_DISINTEGRATION != 0
            && peer.state.movement_type() != PM_NOCLIP
        {
            peer.state.set_movement_type(PM_NOCLIP);
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        let others = crate::peer::Others {
            boxes: obstacles,
            legs: body_legs,
            gametype: *gametype,
            ghoul2_time: *last_frame_time,
        };
        if let Some(them) = opponent.as_deref_mut() {
            them.movement = them.movement.reseeded(&them.state);
        }
        let hits = peer.lock.hits;
        // Locked with an NPC (`genemy`), its movement is the one pushed.
        let npc_enemy = peer.state.raw_field(110).unwrap_or(0) as u16;
        let mut npc_foe = if lock_enemy == Some(None) {
            npcs.roster
                .actors
                .iter_mut()
                .find(|npc| npc.number == npc_enemy)
        } else {
            None
        };
        if let Some(npc) = npc_foe.as_deref_mut() {
            npc.movement = npc.movement.reseeded(&npc.player);
        }
        let foe = opponent
            .as_deref_mut()
            .map(|them| &mut them.movement)
            .or(npc_foe.as_deref_mut().map(|npc| &mut npc.movement));
        let lock = lock_enemy.map(|_| sjk_game_jka::pmove_saber_lock::LockContext {
            opponent: foe,
            rng: &mut deaths.rng,
            hits,
        });
        let (moved, outcome) = peer.move_command(command, map.as_ref(), others, server_time, lock);
        // Later item/trigger updates may reseed prediction and clear its touch list.
        let shield_impacts = peer.movement.touched();
        let vehicle_landing = peer.movement.vehicle_landing();
        if let Some(npc) = npc_foe {
            sjk_game_jka::npc_saber_lock::pushed_by_player(npc, &outcome);
        }
        if let Some(them) = opponent {
            // What the lock's break did to the opponent, and what it asks of the game.
            them.movement.write_player_state(&mut them.state);
            if let Some(until) = outcome.knocked_down_until {
                them.knockdown.hand_extend_time = until;
            }
            if let Some((number, time, debounce_time)) = outcome.other_killer {
                them.wounds.other_killer = sjk_game_jka::damage::OtherKiller {
                    number,
                    time,
                    debounce_time,
                };
            }
        }
        // The key's generic command, after the move and before the entity is converted
        // (`g_active.c:3109-3330`); a lock won with weight to spare before it
        // (`pmove.checkDuelLoss`, `g_active.c:3072`).
        let level_time = *last_frame_time;
        let generic = sjk_game_jka::generic_commands::take(
            &mut peer.generic,
            command.generic_command,
            level_time,
        );
        if vehicle_landing.is_some() {
            self.board_from_ground(client, vehicle_landing, level_time);
        }
        if let Some(loser) = outcome.duel_loss {
            self.duel_loss(client, loser, level_time);
        }
        if generic {
            self.generic_command(client, &command, moved.origin_before, level_time);
        }
        self.ridden(client, level_time);
        let Self {
            server,
            world,
            players,
            map,
            pool,
            last_frame_time,
            missiles,
            deaths,
            fired,
            sounds: self_sounds,
            told: self_told,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let ran = peer.after_move(
            command,
            moved,
            map.as_ref(),
            pool,
            server_time,
            &mut deaths.rng,
        );
        // `ClientEvents`: the fire events of this think fire the weapon from the entity
        // as it was just converted, and end the spawn's protection.
        let level_time = *last_frame_time;
        // A think raises a fire event at most for each of its slices; no allocation.
        let mut fires = [None; 16];
        if ran {
            for (slot, alternate) in fires.iter_mut().zip(peer.fire_events()) {
                *slot = Some(alternate);
            }
            // `EV_SABER_ATTACK`: a swing is an attack, and ends the spawn's protection.
            if peer.swung() {
                let flags = peer.state.raw_field(EFLAGS).unwrap_or(0);
                peer.state.set_raw_field(EFLAGS, flags & !EF_INVULNERABLE);
                peer.invulnerable_until = 0;
                peer.force.danger_time = level_time;
            }
        }
        // `ClientEvents`' landings and item uses.
        let (mut landings, mut uses) = ([None; 16], [None; 16]);
        if ran {
            for (slot, delta) in landings.iter_mut().zip(peer.fall_events()) {
                *slot = Some(delta);
            }
            for (slot, tag) in uses.iter_mut().zip(peer.item_uses()) {
                *slot = Some(tag);
            }
        }
        // The disruptor's instant shots strike other players: fired once this player's
        // borrow is over.
        let mut instant = [None; 16];
        for (alternate, slot) in fires.into_iter().flatten().zip(instant.iter_mut()) {
            // `FireWeapon` counts the shot for the accuracy of every weapon but the
            // saber, the stun baton and the fists — the flechette's as five, alternate or not.
            if !matches!(peer.state.weapon(), 1..=3) {
                peer.accuracy.1 += if peer.state.weapon() == 10 { 5 } else { 1 };
            }
            let flags = peer.state.raw_field(EFLAGS).unwrap_or(0);
            peer.state.set_raw_field(EFLAGS, flags & !EF_INVULNERABLE);
            peer.invulnerable_until = 0;
            peer.force.danger_time = level_time;
            if matches!(
                peer.state.weapon(),
                WP_DISRUPTOR
                    | WP_STUN_BATON
                    | WP_MELEE
                    | WP_TRIP_MINE
                    | WP_DET_PACK
                    | WP_EMPLACED_GUN
            ) || (matches!(peer.state.weapon(), WP_DEMP2 | WP_CONCUSSION) && alternate)
            {
                *slot = Some(alternate);
                continue;
            }
            fired.clear();
            let last_valid = peer.movement.state().rocket_last_valid_time;
            let lock_valid = |number: u16| number < 64 && foes & (1 << number) != 0;
            let (sounds, told) = (&mut *self_sounds, &mut *self_told);
            fire_weapon(
                &mut peer.state,
                peer.entity.state(),
                level_time,
                alternate,
                &mut deaths.rng,
                last_valid,
                &lock_valid,
                &mut |name| {
                    sounds.index(name, &mut |index, value| {
                        told.push(Told::ConfigString {
                            index,
                            previous: Vec::new(),
                            value: value.to_vec(),
                        })
                    })
                },
                fired,
            );
            for missile in fired.drain(..) {
                if let Some(number) = pool.spawn_entity(missile.state.clone(), level_time) {
                    pool.set_bounds(number, missile.bounds);
                    missiles.push((number, missile));
                }
            }
            if peer.state.weapon() == WP_ROCKET_LAUNCHER {
                // The lock the rocket cleared is the movement's to know too.
                peer.movement = peer.movement.reseeded(&peer.state);
            }
        }
        for alternate in instant.into_iter().flatten() {
            match self.peer_mut(client).map(|peer| peer.state.weapon()) {
                Some(WP_DEMP2) => self.fire_demp2_sphere(client, level_time),
                Some(WP_CONCUSSION) => self.fire_concussion_alt(client, level_time),
                Some(WP_STUN_BATON | WP_MELEE) => self.fire_hand(client, level_time),
                Some(WP_TRIP_MINE | WP_DET_PACK) => self.fire_charge(client, alternate, level_time),
                Some(WP_EMPLACED_GUN) => self.fire_emplaced(client, alternate, level_time),
                _ => self.fire_disruptor(client, alternate, level_time),
            }
        }
        for tag in uses.into_iter().flatten() {
            self.use_item_of_move(client, tag, level_time);
        }
        // `G_TouchTriggers`, which a player in noclip skips (`g_active.c`); the client's
        // own trigger prediction skips `PM_NOCLIP` the same way (`cg_predict.c`).
        let touches = !self.peer(client).is_some_and(|peer| peer.noclip);
        if touches {
            self.touch_items(client, level_time);
            self.touch_jedi_master(client, level_time);
            self.touch_holocrons(client, level_time);
            self.touch_triggers(client, level_time);
            self.touch_sabers(client);
            self.touch_movers(client, level_time);
            self.touch_siege_items(client, level_time);
            self.touch_multiples(client, level_time);
            self.touch_doors(client, level_time);
        }
        self.touch_plats(client, level_time);
        self.touch_shields(client, shield_impacts.entities(), level_time);
        self.try_use(client, level_time);
        if touches {
            self.touch_space(client, level_time);
        }
        self.update_client_broadcasts(client);
        for delta in landings.into_iter().flatten() {
            self.fell(client, delta, level_time);
        }
        self.read_foot_bolts(client);
        let Self {
            server,
            world,
            players,
            last_frame_time,
            ..
        } = self;
        let peer = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle));
        let Some(peer) = peer else { return };
        let level_time = *last_frame_time;
        // `ClientThink_real`'s dead branch (`g_active.c:3453-3487`): once `level.time` is
        // past `respawnTime`, the attack or use button respawns the player — or
        // `g_forceRespawn` seconds do, so nobody waits out the others' powerups.
        let (dead, playing, respawn_time) = (
            peer.health <= 0,
            peer.playing(),
            peer.mortality.respawn_time,
        );
        // "can't respawn while being eaten" (`g_active.c:3455`).
        let eaten = peer.state.raw_field(103).unwrap_or(0) & 1 != 0;
        if dead && playing && level_time > respawn_time && !eaten {
            // A power duel forces it a second on (`forceRes = 1`).
            // So does siege with its respawn waves on (`g_active.c:3466-3469`).
            let waves = self.siege.is_some() && self.cvars.integer(b"g_siegeRespawn") != 0;
            let force_seconds = if self.gametype == GAMETYPE_POWERDUEL || waves {
                1
            } else {
                FORCE_RESPAWN_SECONDS
            };
            let forced = level_time - respawn_time > force_seconds * 1_000;
            if forced || command.buttons & (BUTTON_ATTACK | BUTTON_USE_HOLDABLE) != 0 {
                self.respawn(client, command, level_time, server_time);
                return;
            }
        }
        // The vehicle the player drives thinks on its command (`g_active.c:3497-3510`).
        self.drive_ridden(client, level_time);
    }

    /// `ClientEvents`' `EV_FALL`: a landing that hurts is damage from the world, with no
    /// ordinary pain sound for 200 ms; a landing that kills plays the splat, whose sound
    /// is registered as it is first needed.
    pub(super) fn fell(&mut self, client: usize, delta: i32, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let knocked_down = sjk_game_jka::knockdown::in_knockdown_only(
            peer.state.raw_field(13).unwrap_or(0) as u16,
        );
        let Some(points) = fall_damage(delta, 0, knocked_down) else {
            return;
        };
        peer.wounds.pain_debounce_time = level_time + 200;
        let request = DamageRequest {
            level_time,
            attacker: None,
            direction: None,
            point: None,
            damage: points,
            flags: DAMAGE_NO_ARMOR,
            means: MOD_FALLING,
        };
        let _ = self.hurt(client, request);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.health < 1 {
            let origin = peer.state.origin();
            let (sounds, told) = (&mut self.sounds, &mut self.told);
            let splat = sounds.index(b"sound/player/fallsplat.wav", &mut |index, value| {
                told.push(Told::ConfigString {
                    index,
                    previous: Vec::new(),
                    value: value.to_vec(),
                })
            });
            let sound = EventEntity {
                event: EV_GENERAL_SOUND,
                parameter: u32::from(splat),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            let _ = self.pool.spawn_temporary(sound.state(), level_time, None);
        }
    }

    /// `Cmd_Kill_f`: a living player kills itself; a spectator or a corpse cannot.
    pub(super) fn kill(&mut self, client: usize, server_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        // `CMD_ALIVE` reads the health: a corpse in noclip is in `PM_NOCLIP`, not `PM_DEAD`.
        if !peer.playing() || peer.health <= 0 || peer.state.movement_type() == PM_DEAD {
            return;
        }
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        // `G_Kill` (`g_cmds.c:526-527`): the caller sets both the health and its stat.
        peer.health = -999;
        peer.state.stats[0] = (-999i32) as u32;
        let request = DeathRequest::suicide(
            server_time,
            client as u16,
            peer.state.origin(),
            peer.saber_off_sounds(),
            -999,
        );
        self.die(client, request);
    }
}

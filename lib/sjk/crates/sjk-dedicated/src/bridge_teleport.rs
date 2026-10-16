//! `TeleportPlayer` on this server (`g_misc.c:197-256`), which a teleporter's touch and the
//! `setviewpos` cheat (`Cmd_SetViewpos_f`, `g_cmds.c:2502-2522`) share: the player put where
//! it goes, facing its angles, the flashes where it left and where it arrived, and
//! whatever stands there telefragged (`G_KillBox`).

use super::*;

impl NativeGame {
    /// `TeleportPlayer` for player `client`, standing at `from`, to `destination` facing
    /// `angles`: a spectator's jump is silent and kills nobody.
    pub(super) fn teleport_client(
        &mut self,
        client: usize,
        from: [f32; 3],
        destination: [f32; 3],
        angles: [f32; 3],
        spectating: bool,
        level_time: i32,
    ) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let command_angles = peer.last_command.angles;
        let teleported = sjk_game_jka::triggers::teleport_player(
            &mut peer.state,
            destination,
            angles,
            spectating,
        );
        sjk_game_jka::triggers::face(&mut peer.state, teleported.angles, command_angles);
        peer.movement = peer.movement.reseeded(&peer.state);
        if teleported.flashes {
            let out = EventEntity::teleport_out(from, client as u16);
            let arrived = EventEntity::teleport_in(destination, client as u16);
            let _ = self.pool.spawn_temporary(out.state(), level_time, None);
            let _ = self.pool.spawn_temporary(arrived.state(), level_time, None);
        }
        // `G_KillBox`: anything standing where it arrives is telefragged.
        if teleported.kill_box {
            self.gather_obstacles(client);
            let obstacles = std::mem::take(&mut self.obstacles);
            let Some(peer) = self.peer_mut(client) else {
                return;
            };
            let (bottom, top) = peer.movement.box_bounds();
            let arrived = teleported.origin;
            let (max_health, team) = (peer.state.max_health(), peer.session.team);
            let victims: Vec<u16> = obstacles
                .iter()
                .filter(|other| {
                    (0..3).all(|axis| {
                        arrived[axis] + bottom[axis]
                            <= other.origin[axis] + other.bounds.1[axis] + 1.0
                            && arrived[axis] + top[axis]
                                >= other.origin[axis] + other.bounds.0[axis] - 1.0
                    })
                })
                .map(|other| other.entity)
                .collect();
            self.obstacles = obstacles;
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
                        level_time,
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
    }

    /// `Cmd_SetViewpos_f` (`g_cmds.c:2502-2522`, `CMD_CHEAT`): `setviewpos x y z yaw` puts
    /// player `client` there, facing along the yaw.
    pub(super) fn set_view_position(
        &mut self,
        client: usize,
        arguments: &[&[u8]],
        level_time: i32,
    ) {
        if !self.settings.cheats {
            self.told
                .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
            return;
        }
        let Ok([x, y, z, yaw]) = <[&[u8]; 4]>::try_from(arguments) else {
            self.told.push(Told::One(
                client,
                b"print \"usage: setviewpos x y z yaw\n\"".to_vec(),
            ));
            return;
        };
        let value = |text: &[u8]| sjk_game_jka::text_parse::atof(text);
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let (from, spectating) = (peer.state.origin(), !peer.playing());
        self.teleport_client(
            client,
            from,
            [value(x), value(y), value(z)],
            [0.0, value(yaw), 0.0],
            spectating,
            level_time,
        );
    }
}

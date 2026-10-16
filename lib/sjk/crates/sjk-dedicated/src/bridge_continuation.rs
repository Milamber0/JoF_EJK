//! One-player continuation of a client-visible world using ordinary server gameplay.
use super::*;

impl NativeGame {
    /// Populate the server skeleton cache during background world preparation.
    /// The temporary peer is removed before any continuation can become visible.
    pub fn precache_local_player(&mut self, client: usize, userinfo: &[u8]) -> Result<(), String> {
        self.client_connect(client, userinfo)
            .map_err(|e| String::from_utf8_lossy(&e).into_owned())?;
        self.enter_world(client, &UserCommand::default(), 0);
        self.pose_saber(client, 0);
        self.client_disconnect(client);
        self.reset_local_world(0);
        Ok(())
    }

    /// Reset a dormant continuation using the normal same-map restart path.
    pub fn reset_local_world(&mut self, time: i32) {
        self.restart_level(time);
    }

    /// Adopt the last visible player into a private local game. Remote clients are
    /// not imported. This seeds ordinary simulation; it does not alter movement rules.
    pub fn resume_local_player(&mut self, player: &PlayerState, time: i32) -> Result<(), String> {
        let client = usize::from(player.client_num());
        self.last_frame_time = time;
        self.previous_frame_time = time;
        self.level_start_time = time;
        self.set_limits(0, 0.0, 0);
        let peer = self
            .peer_mut(client)
            .ok_or("local continuation player was not admitted")?;
        let saber_entity = peer.state.saber_entity_num();
        peer.state.copy_from(player);
        peer.state.set_raw_field(31, u32::from(saber_entity));
        // Other players are gone, including any duel/lock counterpart.
        peer.state.set_raw_field(107, 0);
        peer.state.set_raw_field(119, 0);
        // A remote thrown entity cannot own this local world's newly allocated saber.
        peer.state.set_raw_field(88, 0);
        peer.state.set_command_time(time);
        peer.health = player.health();
        peer.session.team = player.persistent[3] as i32;
        peer.begun = true;
        peer.enter_time = time;
        peer.movement = peer.movement.reseeded(&peer.state);
        peer.entity.spawn_finished(&peer.state);
        Ok(())
    }

    /// Continue visible binary brush movers from their last replicated trajectories.
    /// Hidden server/script state is not available in a client snapshot; unseen
    /// map entities keep their ordinary authored initial state.
    pub fn resume_local_movers(&mut self, snapshot: &sjk_protocol::Snapshot) {
        use sjk_game_jka::movers::{MoverState, Waiting};
        for index in 0..self.doors.len() {
            let door = &mut self.doors[index].1;
            let Some(state) = snapshot.entities.iter().find(|state| {
                state.entity_type() == 6 && state.model_index() as usize == door.model
            }) else {
                continue;
            };
            door.base = state.trajectory_base();
            door.delta = state.trajectory_delta();
            door.started = state.trajectory_time();
            door.duration = state.trajectory_duration();
            door.trajectory = u32::from(state.trajectory_type());
            let travel: f32 = (0..3)
                .map(|axis| door.delta[axis] * (door.pos2[axis] - door.pos1[axis]))
                .sum();
            door.state = if travel > 0.0 {
                MoverState::OneToTwo
            } else if travel < 0.0 {
                MoverState::TwoToOne
            } else if (0..3)
                .map(|i| (door.base[i] - door.pos1[i]).abs())
                .sum::<f32>()
                < 0.1
            {
                MoverState::Pos1
            } else {
                MoverState::Pos2
            };
            door.waiting = Waiting::None;
            if door.state == MoverState::Pos2 && door.wait >= 0 {
                door.waiting = Waiting::Return;
                door.next_think = snapshot.server_time.saturating_add(door.wait.max(0));
            }
            let open = door.state != MoverState::Pos1;
            let (at, bounds) = (door.pos1, door.bounds);
            if self.door_portals[index] != open {
                self.door_portals[index] = open;
                self.adjust_portal(at, bounds, open);
            }
            self.publish_door(index);
        }
    }
}

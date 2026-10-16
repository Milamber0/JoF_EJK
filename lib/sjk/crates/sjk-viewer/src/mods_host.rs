//! What a mod sees of the client while one of its commands runs: the live
//! session, the predicted player, the crosshair trace and the settings. Nothing
//! here is kept between commands.

use crate::GpuState;
use sjk_mod::{Drawn, Host, Me, Player, Server};
use sjk_runtime::EntityId;

/// `CS_PLAYERS`, the first client's configstring.
const CS_PLAYERS: usize = 1131;
const MAX_CLIENTS: u16 = 32;
/// `ET_PLAYER`.
const ENTITY_PLAYER: u8 = 1;
/// `PM_INTERMISSION`.
const INTERMISSION: u8 = 7;

pub(super) struct ViewerHost<'a> {
    gpu: &'a mut GpuState,
}

impl<'a> ViewerHost<'a> {
    pub(super) fn new(gpu: &'a mut GpuState) -> Self {
        Self { gpu }
    }
}

impl Host for ViewerHost<'_> {
    fn send(&mut self, command: &str) -> Result<(), String> {
        self.gpu
            .live_session
            .as_mut()
            .ok_or("Not connected to a server.")?
            .send_reliable_command(command.as_bytes())
            .map_err(|error| error.to_string())
    }

    fn server(&self) -> Option<Server> {
        let session = self.gpu.live_session.as_ref()?;
        Some(Server {
            kind: super::server_kind(session.compat_profile()),
            address: session.server().to_string(),
            info: String::from_utf8_lossy(
                session.game_state().config_string(0).unwrap_or_default(),
            )
            .into_owned(),
        })
    }

    fn me(&self) -> Option<Me> {
        let player = &self.gpu.live_session.as_ref()?.latest_snapshot().player;
        let (origin, view_angles) = self
            .gpu
            .local_prediction
            .predicted_state()
            .map_or((player.origin(), player.view_angles()), |state| {
                (state.origin, state.view_angles)
            });
        Some(Me {
            client: player.client_num(),
            origin,
            view_angles,
            intermission: player.movement_type() == INTERMISSION,
        })
    }

    fn crosshair_point(&self) -> Option<[f32; 3]> {
        let now = self
            .gpu
            .live_session
            .as_ref()?
            .latest_snapshot()
            .server_time;
        self.gpu
            .crosshair_scan
            .endpoint_near(now)
            .map(|point| point.to_array())
    }

    fn crosshair_player(&self) -> Option<u16> {
        let now = self
            .gpu
            .live_session
            .as_ref()?
            .latest_snapshot()
            .server_time;
        self.gpu.crosshair_scan.chat_client(now)
    }

    fn players(&self) -> Vec<Player> {
        let Some(session) = self.gpu.live_session.as_ref() else {
            return Vec::new();
        };
        let game = session.game_state();
        let snapshot = session.latest_snapshot();
        let time = snapshot.server_time;
        (0..MAX_CLIENTS)
            .filter_map(|client| {
                let info = game.config_string(CS_PLAYERS + usize::from(client))?;
                let name = sjk_client::LegacyClientInfo::new(info).bytes("n")?;
                let drawn = snapshot
                    .entities
                    .iter()
                    .find(|entity| {
                        entity.number() == client && entity.entity_type() == ENTITY_PLAYER
                    })
                    .and_then(|state| {
                        let entity = self
                            .gpu
                            .live_world
                            .entity(EntityId::new(u64::from(client) + 1))?;
                        Some(Drawn {
                            origin: entity.sample(i64::from(time)).translation,
                            angles: entity
                                .sample_pose(i64::from(time))
                                .map_or([0.0; 3], |pose| pose.view_angles_degrees),
                            base_z: state.trajectory_base()[2],
                            age_ms: time.saturating_sub(state.trajectory_time()),
                        })
                    });
                Some(Player {
                    client,
                    name: name
                        .iter()
                        .copied()
                        .map(sjk_protocol::windows_1252_char)
                        .collect(),
                    drawn,
                })
            })
            .collect()
    }

    fn cvar(&self, name: &str) -> Option<String> {
        Some(self.gpu.console.as_ref()?.cvar(name)?.as_text())
    }

    fn set_cvar(&mut self, name: &str, value: &str) -> Result<(), String> {
        self.gpu
            .console
            .as_mut()
            .ok_or("Console unavailable")?
            .try_set_cvar(name, value)
    }
}

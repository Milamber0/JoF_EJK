//! In-process authority uses the normal presentation session without a socket.
use super::*;

/// A locally owned game supplying snapshots to the same client presentation pipeline.
/// Implementations own their simulation and reuse snapshot storage between commands.
pub trait LocalSimulation: Send + std::any::Any {
    /// Apply one normally quantized gameplay command to the local authority.
    fn command(&mut self, command: &UserCommand);
    /// Apply a console/gameplay command locally, never to the parked remote connection.
    fn reliable(&mut self, command: &[u8]);
    /// Publish a newly simulated frame; false means there is no new frame yet.
    fn receive(
        &mut self,
        game: &mut GameState,
        snapshot: &mut Snapshot,
        dirty: &mut sjk_protocol::ConfigStringDirty,
    ) -> bool;
}

impl ClientSession {
    /// Whether this presentation session has local authority and no network transport.
    pub fn is_local(&self) -> bool {
        self.connection.is_none()
    }

    /// Return a suspended local authority to its owner when remote play resumes.
    pub fn take_local_authority(&mut self) -> Option<Box<dyn LocalSimulation>> {
        self.disconnected = true;
        self.local.take()
    }

    /// Rebind presentation resources after an independently simulated local world.
    pub fn invalidate_presentation_config(&mut self) {
        self.config_string_dirty.mark_all();
    }

    /// Take the last world before a replacement gamestate/restart was applied.
    /// Capturing happens only at transitions, not on every received snapshot.
    pub fn take_retired_world(&mut self) -> Option<(GameState, Snapshot)> {
        self.retired_world.take()
    }

    /// Construct a socket-free presentation session over an independently owned game.
    /// The originating address is retained only as UI metadata.
    pub fn local_continuation(
        &self,
        game_state: GameState,
        latest_snapshot: Snapshot,
        simulation: Box<dyn LocalSimulation>,
    ) -> Self {
        let compat_profile = CompatProfile::from_game_state(&game_state);
        let mut dirty = sjk_protocol::ConfigStringDirty::default();
        dirty.mark_all();
        Self {
            local: Some(simulation),
            origin_server: self.origin_server,
            retired_world: None,
            download_storage: None,
            pending_download: None,
            downloaded_message: None,
            connection: None,
            shader_remaps: ShaderRemaps::from_game_state(&game_state),
            game_state,
            latest_snapshot,
            config_string_dirty: dirty,
            server_id: 0,
            history: VecDeque::new(),
            reliable_sequence: 0,
            client_reliable_sequence: 0,
            client_reliable_acknowledge: 0,
            pending_client_commands: VecDeque::new(),
            command_history: Default::default(),
            gamestate_probe: gamestate_probe::GamestateProbe::new(0),
            highest_server_command: 0,
            pending_big_config_string: None,
            events: VecDeque::new(),
            transitions: VecDeque::new(),
            restart_received_at: None,
            pure_command_builder: Box::new(|_| None),
            request_full_snapshot: false,
            disconnected: false,
            scores: Vec::new(),
            team_scores: [0; 2],
            team_info: Default::default(),
            base_command_events: VecDeque::with_capacity(64),
            unknown_command_names: Vec::new(),
            compat_profile,
            cosmetic_unlocks: Default::default(),
            userinfo_updates: userinfo_update::UserinfoUpdateTracker::new(
                String::new(),
                None,
                Instant::now(),
            ),
            command_pacer: reliable_pacing::ReliableCommandPacer::new(
                reliable_pacing::DEFAULT_COMMAND_INTERVAL,
            ),
            demo_recorder: Default::default(),
        }
    }
}

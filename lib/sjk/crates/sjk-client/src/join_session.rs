//! Gamestate, optional content transfer, pure proof and first-snapshot bootstrap.

use super::*;
use std::time::Instant;

/// How many receive timeouts a join waits for its gamestate, re-requesting it.
const GAMESTATE_REQUESTS: u32 = 3;
/// A silent wait this long re-sends the gamestate request.
const GAMESTATE_RESEND: Duration = Duration::from_secs(1);

impl ClientSession {
    /// Join with an optional host-owned download capability, before entering cgame.
    pub fn join_with_downloads(
        server: SocketAddr,
        userinfo: &LegacyUserInfo,
        profile: CompatProfile,
        timeout: Duration,
        build_pure_command: impl Fn(&GameState) -> Option<Vec<u8>> + Send + 'static,
        mut observe: impl FnMut(JoinPhase<'_>),
        mut download_storage: Option<Box<dyn download::DownloadStorage>>,
    ) -> Result<Self, ClientError> {
        let userinfo = &profile.userinfo_for(userinfo);
        let initial_userinfo =
            legacy_userinfo_payload_with_extensions(userinfo, profile.userinfo_extensions())?;
        let mut connection = connect_legacy_with_userinfo_extensions_observed(
            server,
            userinfo,
            profile.userinfo_extensions(),
            timeout,
            |phase| match phase {
                NetworkConnectPhase::Challenge => observe(JoinPhase::Challenge),
                NetworkConnectPhase::Connected => observe(JoinPhase::Connected),
                NetworkConnectPhase::UnknownReply(command) => {
                    observe(JoinPhase::UnknownReply(command));
                }
            },
        )?;
        connection.request_initial_gamestate()?;
        let mut client_reliable_sequence = 0;
        let mut pending_client_commands = VecDeque::new();
        // Each receive is bounded, but a server that keeps sending messages
        // without a gamestate would otherwise be re-asked forever while the
        // player waits on a half-joined map. Downloads start after the gamestate
        // and are not counted.
        let gamestate_deadline = Instant::now() + timeout.saturating_mul(GAMESTATE_REQUESTS);
        let initial = loop {
            if Instant::now() >= gamestate_deadline {
                return Err(NetworkError::TimedOut("gamestate").into());
            }
            // Ask again whenever nothing arrives for a moment, as a stock client
            // keeps sending while it waits; a lost request or gamestate is then
            // recovered instead of ending the join.
            let wait =
                GAMESTATE_RESEND.min(gamestate_deadline.saturating_duration_since(Instant::now()));
            let message = match connection.receive_server_message(wait) {
                Err(NetworkError::TimedOut(_)) => {
                    connection.request_initial_gamestate()?;
                    continue;
                }
                result => result?,
            };
            match decode_initial_gamestate(&message.payload) {
                Ok(_) => {
                    let message = if let Some(storage) = &mut download_storage {
                        download::run(
                            &mut connection,
                            message,
                            storage.as_mut(),
                            &mut client_reliable_sequence,
                            &mut pending_client_commands,
                        )?
                    } else {
                        message
                    };
                    break (
                        message.sequence,
                        decode_initial_gamestate(&message.payload)?,
                    );
                }
                Err(GameStateError::ExpectedCommand { .. }) => {
                    connection.request_initial_gamestate()?;
                }
                Err(error) => return Err(error.into()),
            }
        };
        let (gamestate_sequence, initial) = initial;
        observe(JoinPhase::Gamestate);
        let system_info = initial
            .game_state
            .config_string(1)
            .ok_or(ClientError::MissingSystemInfo)?;
        let system_info =
            std::str::from_utf8(system_info).map_err(|_| ClientError::NonUtf8SystemInfo)?;
        let system_info = InfoString::parse(system_info)?;
        let server_id = system_info
            .get_i32("sv_serverid")
            .ok_or(ClientError::MissingServerId)?;
        let build_pure_command: PureCommandBuilder = Box::new(build_pure_command);
        if let Some(command) = build_pure_command(&initial.game_state) {
            client_reliable_sequence += 1;
            pending_client_commands.push_back((client_reliable_sequence, command));
        }
        let latest_snapshot = join_bootstrap::enter(
            &mut connection,
            &initial,
            gamestate_sequence,
            server_id,
            &pending_client_commands,
            timeout,
        )?;
        observe(JoinPhase::FirstSnapshot);
        let reliable_sequence = latest_snapshot
            .server_commands
            .last()
            .map_or(initial.game_state.server_command_sequence, |command| {
                command.sequence
            })
            .max(initial.game_state.server_command_sequence);
        let mut bootstrap_commands = initial.server_commands.clone();
        bootstrap_commands.extend(latest_snapshot.server_commands.iter().cloned());
        let compat_profile = CompatProfile::from_game_state(&initial.game_state);
        let client_reliable_acknowledge = latest_snapshot
            .reliable_acknowledge
            .min(client_reliable_sequence);
        pending_client_commands.retain(|(sequence, _)| *sequence > client_reliable_acknowledge);
        let mut session = Self {
            local: None,
            origin_server: server,
            retired_world: None,
            download_storage,
            pending_download: None,
            downloaded_message: None,
            connection: Some(connection),
            shader_remaps: ShaderRemaps::from_game_state(&initial.game_state),
            game_state: initial.game_state,
            config_string_dirty: sjk_protocol::ConfigStringDirty::default(),
            server_id,
            latest_snapshot,
            history: VecDeque::new(),
            reliable_sequence,
            client_reliable_sequence,
            client_reliable_acknowledge,
            pending_client_commands,
            command_history: command_history::CommandHistory::default(),
            gamestate_probe: gamestate_probe::GamestateProbe::new(server_id),
            highest_server_command: 0,
            pending_big_config_string: None,
            events: VecDeque::new(),
            transitions: VecDeque::new(),
            restart_received_at: None,
            pure_command_builder: build_pure_command,
            request_full_snapshot: false,
            disconnected: false,
            scores: Vec::new(),
            team_scores: [0; 2],
            team_info: TeamInfoTable::default(),
            base_command_events: VecDeque::with_capacity(64),
            unknown_command_names: Vec::new(),
            compat_profile,
            cosmetic_unlocks: CosmeticUnlockTable::default(),
            userinfo_updates: userinfo_update::UserinfoUpdateTracker::new(
                initial_userinfo,
                userinfo.guid.clone(),
                Instant::now(),
            ),
            command_pacer: reliable_pacing::ReliableCommandPacer::new(
                reliable_pacing::DEFAULT_COMMAND_INTERVAL,
            ),
            demo_recorder: DemoRecorder::default(),
        };
        for command in &bootstrap_commands {
            session
                .gamestate_probe
                .command(command.sequence, &command.command);
            session.apply_server_command(&command.command)?;
        }
        for command in &bootstrap_commands {
            session
                .connection
                .as_mut()
                .expect("network bootstrap")
                .record_server_command(command.sequence, &command.command);
        }
        Ok(session)
    }
}

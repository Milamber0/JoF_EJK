//! The initial gamestate acknowledgement, before a live session exists.
//!
//! Stock records server-command keys before acknowledging the gamestate and
//! writes pending reliable commands ahead of movement in the same packet
//! (`codemp/client/cl_parse.cpp`, `cl_input.cpp:1528-1579`).

use crate::{ClientError, Snapshot};
use sjk_network::{LegacyConnection, NetworkError};
use sjk_protocol::{
    InitialGameStateMessage, ServiceCommand, SnapshotError, UserCommand, decode_snapshot,
};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const RETRY_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn enter(
    connection: &mut LegacyConnection,
    initial: &InitialGameStateMessage,
    gamestate_sequence: i32,
    server_id: i32,
    pending: &VecDeque<(i32, Vec<u8>)>,
    timeout: Duration,
) -> Result<Snapshot, ClientError> {
    // Waiting until a first snapshot arrives is too late: cp itself uses
    // this key. A greeting before svc_gamestate exposed the missing ordering.
    for command in &initial.server_commands {
        connection.record_server_command(command.sequence, &command.command);
    }
    let commands: Vec<_> = pending
        .iter()
        .map(|(sequence, text)| (*sequence, text.as_slice()))
        .collect();
    let deadline = Instant::now() + timeout;
    let mut next_send = Instant::now();
    let mut rejected = 0;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(NetworkError::TimedOut("first snapshot").into());
        }
        if now >= next_send {
            // Keep cp with the first move so UDP loss/reordering cannot let
            // movement reach a strict pure server before its proof. Repeats
            // retain the reliable sequence; servers execute it only once.
            connection.send_user_command_with_reliables(
                server_id,
                gamestate_sequence,
                initial.game_state.server_command_sequence,
                initial.game_state.checksum_feed,
                false,
                &commands,
                &UserCommand {
                    server_time: 100,
                    ..UserCommand::default()
                },
            )?;
            next_send = now + RETRY_INTERVAL;
        }
        let wait = next_send
            .min(deadline)
            .saturating_duration_since(Instant::now());
        if wait.is_zero() {
            continue;
        }
        let message = match connection.receive_server_message(wait) {
            Err(NetworkError::TimedOut(_)) => continue,
            result => result?,
        };
        match decode_snapshot(
            &message.payload,
            message.sequence,
            &initial.game_state,
            None,
        ) {
            Ok(snapshot) => return Ok(snapshot),
            Err(SnapshotError::UnexpectedCommand(
                ServiceCommand::GameState
                | ServiceCommand::MapChange
                | ServiceCommand::Bad
                | ServiceCommand::End,
            )) => {}
            Err(error) => {
                rejected += 1;
                if rejected >= 8 {
                    return Err(error.into());
                }
            }
        }
    }
}

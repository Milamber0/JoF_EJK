use super::{LegacyReliableState, Lifetime, ReliableError, text::CommandText};
use sjk_protocol::MessageReader;

/// Session-supplied inputs to stock reliable-command flood control.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClientCommandPolicy {
    /// Server time in the legacy adapter's integer milliseconds.
    pub now: i32,
    /// Client has entered the active state, rather than only primed/downloading.
    pub active: bool,
    /// Stock listen-server exemption (`com_cl_running`).
    pub local_client_running: bool,
    /// Zero disables; one means 1,000 ms; other values are the configured interval.
    pub flood_protect: i32,
    /// Update the flood timestamp even for a flood-limited command.
    pub slow: bool,
}

/// Result of one ordered reliable client command (not its game-specific effect).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientCommandDecision {
    /// Already dispatched; no callback, timestamp or key changes.
    Duplicate,
    /// Dispatched to the engine; the boolean controls permission for game commands.
    Dispatched { game_commands_allowed: bool },
}

impl LegacyReliableState {
    /// Read and dispatch one `clc_clientCommand` body after its command tag.
    ///
    /// The callback always receives engine commands, even when flood policy disables
    /// game commands. It must implement actual engine/game dispatch and any session
    /// termination. It may queue server replies through the supplied reliable state.
    /// Counters/key update after callback return, matching stock execution order.
    /// No usercmd or simulation behavior is implemented here.
    pub fn read_client_command(
        &mut self,
        reader: &mut MessageReader<'_>,
        policy: ClientCommandPolicy,
        execute: impl FnOnce(&mut Self, &[u8], bool),
    ) -> Result<ClientCommandDecision, ReliableError> {
        if self.is_closed() {
            return Err(ReliableError::Closed);
        }
        let sequence = reader.read_i32()?;
        let command = CommandText::read(reader);
        if self.counters.client_sequence >= sequence {
            return Ok(ClientCommandDecision::Duplicate);
        }
        if i64::from(sequence) > i64::from(self.counters.client_sequence) + 1 {
            self.lifetime = Lifetime::Closing;
            return Err(ReliableError::ClientGap);
        }
        let mut allowed = true;
        if !policy.local_client_running && policy.active && policy.flood_protect != 0 {
            let interval = if policy.flood_protect == 1 {
                1000
            } else {
                policy.flood_protect
            };
            if policy.now < self.counters.last_reliable_time.wrapping_add(interval) {
                allowed = false;
            } else {
                self.counters.last_reliable_time = policy.now;
            }
            if policy.slow {
                self.counters.last_reliable_time = policy.now;
            }
        }
        execute(self, command.as_bytes(), allowed);
        self.counters.client_sequence = sequence;
        self.last_client = command;
        Ok(ClientCommandDecision::Dispatched {
            game_commands_allowed: allowed,
        })
    }
}

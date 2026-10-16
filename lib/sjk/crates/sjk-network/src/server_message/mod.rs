//! Composition of the legacy channel, reliable history and movement receiver.
use crate::{
    ClientCommandPolicy, LegacyClientPacket, LegacyClientPhase, LegacyMoveAdmission,
    LegacyMoveEvent, LegacyMoveHost, LegacyMoveOutcome, LegacyMovePolicy, LegacyMoveState,
    LegacyReliableState, ReliableError,
};
use sjk_protocol::MessageReader;

/// Current map/session policy, re-read after command callbacks.
#[derive(Clone, Copy, Debug)]
pub struct LegacyMessageContext {
    /// Current legacy map generation (not an engine world identity).
    pub server_id: i32,
    /// Earliest map generation retained by the current map_restart interval.
    pub restarted_server_id: i32,
    /// Sequence of the last gamestate sent to this peer.
    pub gamestate_message: i32,
    /// Old map time offset; cleared when the current gamestate is acknowledged.
    pub old_server_time: i32,
    /// A legacy download name is currently nonempty.
    pub downloading: bool,
    /// Current client phase and pure-proof state.
    pub admission: LegacyMoveAdmission,
    /// Current flood policy and time; `active` is derived from admission instead.
    pub command_policy: ClientCommandPolicy,
    /// Current map's stock checksum feed.
    pub checksum_feed: i32,
    /// Enable stock force-selection and roll filtering.
    pub legacy_fixes: bool,
}

/// Synchronous session effects in addition to reliable command execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyMessageEvent {
    /// Set the peer's old-server-time offset to zero before executing commands.
    ClearOldServerTime,
    /// Drop through the ordinary lifecycle with reason `Lost reliable commands`.
    DropLostReliableCommands,
    /// Perform the movement receiver's session/game operation.
    Movement(LegacyMoveEvent),
}

/// Native session/game binding; resource and game effects must execute synchronously.
pub trait LegacyMessageHost {
    /// Read current configuration and admission, including callback changes.
    fn context(&self) -> LegacyMessageContext;
    /// Execute an engine command, or a game command only when allowed. The reliable
    /// state permits immediate replies; update admission if this command drops.
    fn client_command(
        &mut self,
        reliable: &mut LegacyReliableState,
        text: &[u8],
        game_allowed: bool,
    );
    /// Apply the real session/game/resource operation, updating context before return.
    /// Drop handlers must run the existing drop lifecycle and retain final output.
    fn message_event(&mut self, reliable: &mut LegacyReliableState, event: LegacyMessageEvent);
}

/// Result of dispatching a complete, reassembled client packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyMessageOutcome {
    /// Invalid acknowledgements caused the body to be ignored.
    IgnoredAcknowledgement,
    /// A stale map generation was ignored, optionally requesting a new gamestate.
    IgnoredMap { resent_gamestate: bool },
    /// A reliable sequence gap requested a drop; nothing afterward was executed.
    ReliableGap,
    /// A reliable command made the peer a zombie; nothing afterward was executed.
    Disconnected,
    /// Reached the stock EOF command after any reliable commands.
    End,
    /// Stock ignores an unexpected terminal command byte.
    BadCommand(u8),
    /// The movement receiver handled the terminal move batch.
    Movement(LegacyMoveOutcome),
}

/// Decode and dispatch a packet already routed to an admitted peer's channel.
///
/// Uses the existing ring key and acknowledgement checks, then applies map lifetime
/// rules, reliable commands and at most one movement batch. Trailing bytes after
/// that terminal command are ignored as in codemp. No per-packet scratch allocation
/// is performed. Transport errors and truncated fields propagate to the caller;
/// earlier synchronous command effects are not rolled back on a later parse error.
/// Socket ownership, address/qport authentication and timeout scheduling are external.
pub fn execute_legacy_client_packet(
    packet: LegacyClientPacket<'_>,
    reliable: &mut LegacyReliableState,
    movement: &mut LegacyMoveState,
    host: &mut impl LegacyMessageHost,
) -> Result<LegacyMessageOutcome, ReliableError> {
    let Some((header, payload)) = reliable.decode(packet)? else {
        return Ok(LegacyMessageOutcome::IgnoredAcknowledgement);
    };
    let context = host.context();
    let previous_download = reliable
        .last_client_command()
        .windows(6)
        .any(|s| s == b"nextdl");
    if header.server_id != context.server_id && !context.downloading && !previous_download {
        let restarted =
            header.server_id >= context.restarted_server_id && header.server_id < context.server_id;
        let resend = !restarted
            && context.admission.phase != LegacyClientPhase::Active
            && header.message_acknowledge > context.gamestate_message;
        if resend {
            host.message_event(
                reliable,
                LegacyMessageEvent::Movement(LegacyMoveEvent::SendGamestate),
            );
        }
        return Ok(LegacyMessageOutcome::IgnoredMap {
            resent_gamestate: resend,
        });
    }
    if context.old_server_time != 0 && header.server_id == context.server_id {
        host.message_event(reliable, LegacyMessageEvent::ClearOldServerTime);
    }
    let mut reader = MessageReader::new(payload);
    for _ in 0..3 {
        reader.read_i32()?;
    }
    let terminal = loop {
        let command = reader.read_u8()?;
        if command != 4 {
            break command;
        } // clc_clientCommand
        let context = host.context();
        let mut policy = context.command_policy;
        policy.active = context.admission.phase == LegacyClientPhase::Active;
        match reliable.read_client_command(&mut reader, policy, |reliable, text, allowed| {
            host.client_command(reliable, text, allowed);
        }) {
            Err(ReliableError::ClientGap) => {
                host.message_event(reliable, LegacyMessageEvent::DropLostReliableCommands);
                return Ok(LegacyMessageOutcome::ReliableGap);
            }
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        if host.context().admission.phase == LegacyClientPhase::Zombie {
            return Ok(LegacyMessageOutcome::Disconnected);
        }
    };
    match terminal {
        2 | 3 => {
            // clc_move / clc_moveNoDelta
            let context = host.context();
            // The hash uses only the first 32 bytes. Copy before allowing callbacks
            // to mutate history; ring selection happens AFTER reliable commands.
            let text = reliable.server_command_key(header.reliable_acknowledge);
            let mut key = [0_u8; 32];
            let length = text.len().min(key.len());
            key[..length].copy_from_slice(&text[..length]);
            let policy = LegacyMovePolicy {
                delta: terminal == 2,
                message_acknowledge: header.message_acknowledge,
                checksum_feed: context.checksum_feed,
                server_command: &key[..length],
                now: context.command_policy.now,
                legacy_fixes: context.legacy_fixes,
            };
            let mut bridge = MoveBridge { host, reliable };
            Ok(LegacyMessageOutcome::Movement(movement.receive(
                &mut reader,
                policy,
                &mut bridge,
            )?))
        }
        5 => Ok(LegacyMessageOutcome::End),
        other => Ok(LegacyMessageOutcome::BadCommand(other)),
    }
}

struct MoveBridge<'a, H> {
    host: &'a mut H,
    reliable: &'a mut LegacyReliableState,
}
impl<H: LegacyMessageHost> LegacyMoveHost for MoveBridge<'_, H> {
    fn admission(&self) -> LegacyMoveAdmission {
        self.host.context().admission
    }
    fn movement_event(&mut self, event: LegacyMoveEvent) {
        self.host
            .message_event(self.reliable, LegacyMessageEvent::Movement(event));
    }
}

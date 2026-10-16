//! SV_UserMove command reception; authoritative physics remains a game callback.
use crate::{LegacyClientPhase, MAX_PACKET_USER_COMMANDS};
use sjk_protocol::{
    MessageError, MessageReader, UserCommand, legacy_command_hash, read_delta_user_command,
};

/// Current admission state, re-read after callbacks rather than cached for the batch.
#[derive(Clone, Copy, Debug)]
pub struct LegacyMoveAdmission {
    /// Legacy session phase, owned by the session coordinator.
    pub phase: LegacyClientPhase,
    /// Server requires stock pure-content validation.
    pub pure_required: bool,
    /// Client's content proof was accepted.
    pub pure_authentic: bool,
    /// A content-proof command was received, whether valid or not.
    pub got_cp: bool,
}

/// Inputs from the already validated packet header and server compatibility profile.
pub struct LegacyMovePolicy<'a> {
    /// `clc_move` selects an acknowledged delta; `clc_moveNoDelta` clears it.
    pub delta: bool,
    /// This packet's acknowledged server message.
    pub message_acknowledge: i32,
    /// Map's stock checksum feed used in keyed usercmd decoding.
    pub checksum_feed: i32,
    /// Exact reliable ring entry selected by this packet's acknowledgement.
    pub server_command: &'a [u8],
    /// Current server time, used for the acknowledgement/ping history.
    pub now: i32,
    /// Stock `sv_legacyFixes`: filter levitation/out-of-range selection and view roll.
    pub legacy_fixes: bool,
}

/// Synchronous session/game operations requested in stock order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyMoveEvent {
    /// Record arrival time for this acknowledged server message's ping history.
    Acknowledge { message: i32, at: i32 },
    /// Resend gamestate because an active pure client has not supplied its proof.
    SendGamestate,
    /// Enter the world using this first command (not an extra movement simulation).
    /// The host must perform the real entry/configstring/entity/game-begin work and
    /// update its phase before returning. The movement seed/delta are already set.
    EnterWorld(UserCommand),
    /// Execute the normal drop path with reason `Cannot validate pure client!`.
    DropUnpure,
    /// Run authoritative game ClientThink using this command; update phase if it drops.
    Think(UserCommand),
}

/// Session/game consumer for movement reception, independent of sockets/rendering.
pub trait LegacyMoveHost {
    /// Fetch current phase and proof state, including changes made by callbacks.
    fn admission(&self) -> LegacyMoveAdmission;
    /// Perform an operation synchronously. Recording events is only a test fixture;
    /// production hosts must implement the actual game/resource/session effects.
    fn movement_event(&mut self, event: LegacyMoveEvent);
}

/// Why a decoded movement batch did or did not reach game callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyMoveOutcome {
    /// The byte count was outside the stock 1..=32 command range.
    InvalidCount(u8),
    /// A pure client has not sent a proof; an active client requests gamestate resend.
    AwaitingPure,
    /// A supplied proof was not authentic; a normal drop was requested.
    RejectedPure,
    /// The client was not active after entry/proof checks.
    Inactive,
    /// Finished timestamp filtering; count includes only actual Think callbacks.
    Processed { dispatched: usize },
}

/// Per-client raw command history and snapshot-delta request, confined to the adapter.
pub struct LegacyMoveState {
    last: UserCommand,
    delta_message: i32,
}
impl Default for LegacyMoveState {
    fn default() -> Self {
        Self::new(UserCommand::default())
    }
}
impl LegacyMoveState {
    /// Seed from a new session or an explicit reference-compatible world transition.
    pub fn new(last: UserCommand) -> Self {
        Self {
            last,
            delta_message: -1,
        }
    }
    /// Last raw command remembered by entry/ClientThink, even if a prior callback dropped.
    pub fn last_command(&self) -> UserCommand {
        self.last
    }
    /// Requested server message to delta against; -1 requests a full snapshot.
    pub fn delta_message(&self) -> i32 {
        self.delta_message
    }

    /// `SV_ClientEnterWorld` again at a `map_restart`: the next snapshot is a full one.
    pub fn reenter(&mut self) {
        self.delta_message = -1;
    }

    /// Decode one move body after its command tag and synchronously dispatch eligible inputs.
    ///
    /// Uses fixed stack storage, decodes the whole batch before effects, and preserves
    /// raw integer timestamps. No timestep conversion or physics is performed here.
    /// A malformed compressed body returns an error before callbacks/history changes
    /// (except the delta request, selected before the count as in the reference).
    pub fn receive(
        &mut self,
        reader: &mut MessageReader<'_>,
        policy: LegacyMovePolicy<'_>,
        host: &mut impl LegacyMoveHost,
    ) -> Result<LegacyMoveOutcome, MessageError> {
        self.delta_message = if policy.delta {
            policy.message_acknowledge
        } else {
            -1
        };
        let count = reader.read_u8()?;
        if count == 0 || usize::from(count) > MAX_PACKET_USER_COMMANDS {
            return Ok(LegacyMoveOutcome::InvalidCount(count));
        }
        let key = policy.checksum_feed
            ^ policy.message_acknowledge
            ^ legacy_command_hash(policy.server_command);
        let mut commands = [UserCommand::default(); MAX_PACKET_USER_COMMANDS];
        let mut previous = UserCommand::default();
        for command in &mut commands[..usize::from(count)] {
            *command = read_delta_user_command(reader, key, &previous)?;
            if policy.legacy_fixes {
                // OpenJK codemp forcePowers_t, not engine-wide gameplay constants.
                if command.force_selection == 1 || command.force_selection >= 18 {
                    command.force_selection = 0xff;
                }
                command.angles[2] = 0;
            }
            // Stock chains decoding against the filtered command, not the raw input.
            previous = *command;
        }
        host.movement_event(LegacyMoveEvent::Acknowledge {
            message: policy.message_acknowledge,
            at: policy.now,
        });
        let admission = host.admission();
        if admission.pure_required && !admission.pure_authentic && !admission.got_cp {
            if admission.phase == LegacyClientPhase::Active {
                host.movement_event(LegacyMoveEvent::SendGamestate);
            }
            return Ok(LegacyMoveOutcome::AwaitingPure);
        }
        if admission.phase == LegacyClientPhase::Primed {
            self.last = commands[0];
            self.delta_message = -1;
            host.movement_event(LegacyMoveEvent::EnterWorld(commands[0]));
        }
        let admission = host.admission();
        if admission.pure_required && !admission.pure_authentic {
            host.movement_event(LegacyMoveEvent::DropUnpure);
            return Ok(LegacyMoveOutcome::RejectedPure);
        }
        if host.admission().phase != LegacyClientPhase::Active {
            self.delta_message = -1;
            return Ok(LegacyMoveOutcome::Inactive);
        }
        let mut dispatched = 0;
        let newest = commands[usize::from(count) - 1].server_time;
        for command in commands[..usize::from(count)].iter().copied() {
            if command.server_time > newest || command.server_time <= self.last.server_time {
                continue;
            }
            self.last = command;
            // SV_ClientThink remembers every eligible command even after an earlier
            // callback dropped the client, but only active clients reach the game.
            if host.admission().phase == LegacyClientPhase::Active {
                host.movement_event(LegacyMoveEvent::Think(command));
                dispatched += 1;
            }
        }
        Ok(LegacyMoveOutcome::Processed { dispatched })
    }
}

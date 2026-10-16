use super::{
    LegacyClientPhase, LegacyDropEffect, LegacyDropError, LegacyDropHost,
    commands::FormattedCommand, send_legacy_server_command,
};

/// Execute stock SV_DropClient ordering, retaining final reliable output for humans.
///
/// Returns false for an already-zombie peer, with no repeated effects. Bot slots
/// become free immediately. The host supplies actual game/resource effects; zombie
/// expiry, final-message scheduling and heartbeat delivery remain session work.
/// Broadcast overflows may synchronously drop other peers (or re-enter this drop)
/// before its game-disconnect effect, exactly as in the reference.
/// A final-output error is returned after local game/resource cleanup completes.
pub fn drop_legacy_client(
    host: &mut impl LegacyDropHost,
    client: usize,
    reason: &[u8],
) -> Result<bool, LegacyDropError> {
    if client >= host.client_count() {
        return Err(LegacyDropError::InvalidClient);
    }
    if host.phase(client) == LegacyClientPhase::Zombie {
        return Ok(false);
    }
    let bot = host.is_bot(client);
    let mut output_error = None;
    host.reliable(client).begin_drop();
    host.effect(LegacyDropEffect::CloseDownload(client));
    if let Some(print) =
        FormattedCommand::from_parts(&[b"print \"", host.name(client), b"^7 ", reason, b"\n\""])
    {
        output_error = send_legacy_server_command(host, None, print.as_bytes()).err();
    }
    host.effect(LegacyDropEffect::GameDisconnect(client));
    if let Some(disconnect) = FormattedCommand::from_parts(&[b"disconnect \"", reason, b"\""]) {
        if let Err(error) = send_legacy_server_command(host, Some(client), disconnect.as_bytes()) {
            output_error.get_or_insert(error);
        }
    }
    if bot {
        // SV_BotFreeClient performs these changes before SV_DropClient's userinfo clear.
        host.set_phase(client, LegacyClientPhase::Free);
        host.effect(LegacyDropEffect::FreeBot(client));
        if host.is_recording(client) {
            host.effect(LegacyDropEffect::StopDemo(client));
        }
    }
    host.effect(LegacyDropEffect::ClearUserinfo(client));
    host.set_phase(
        client,
        if bot {
            LegacyClientPhase::Free
        } else {
            LegacyClientPhase::Zombie
        },
    );
    host.reliable(client).finish_drop();
    if host.is_recording(client) {
        host.effect(LegacyDropEffect::StopDemo(client));
    }
    if !(0..host.client_count()).any(|peer| host.phase(peer) >= LegacyClientPhase::Connected) {
        host.effect(LegacyDropEffect::Heartbeat);
    }
    match output_error {
        Some(error) => Err(error),
        None => Ok(true),
    }
}

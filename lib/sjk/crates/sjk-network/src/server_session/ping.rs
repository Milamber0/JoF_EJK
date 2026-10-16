//! `SV_CalcPings` (`sv_main.cpp:851`): a client's ping is the mean time between the
//! server sending one of its last 32 messages and the client acknowledging it.
use super::{LegacyGameHost, LegacyServerSession};
use crate::{LegacyClientPhase, LegacyPeerAddress};

/// `PACKET_BACKUP`: the messages a client's ping is measured over.
pub(super) const MESSAGE_BACKUP: usize = 32;

/// When one message went out (`messageSent`) and when the client first said it had it
/// (`messageAcked`; -1 until then), in server time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MessageTiming {
    pub sent: i32,
    pub acked: i32,
}

/// Who stands behind a client as `SV_CalcPings` asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stand {
    /// No entity (`gentity` null).
    Nobody,
    Bot,
    Player,
}

/// One client's ping, and whether the game is told it (`ps->ping`): 999 for a client
/// not in the world or without an entity, 0 for a bot, otherwise the mean round trip of
/// the acknowledged messages (a time of zero or less counts as none), truncated, at
/// most 999 — 999 again when none was acknowledged.
pub(super) fn calc_ping(
    phase: LegacyClientPhase,
    stand: Stand,
    timings: &[MessageTiming; MESSAGE_BACKUP],
) -> (i32, bool) {
    if phase != LegacyClientPhase::Active || stand == Stand::Nobody {
        return (999, false);
    }
    if stand == Stand::Bot {
        return (0, false);
    }
    let (total, count) = timings.iter().filter(|timing| timing.acked > 0).fold(
        (0_i32, 0_i32),
        |(total, count), timing| {
            (
                total.wrapping_add(timing.acked.wrapping_sub(timing.sent)),
                count + 1,
            )
        },
    );
    if count == 0 {
        (999, true)
    } else {
        ((total / count).min(999), true)
    }
}

/// `SV_SendMessageToClient`: message `sequence` goes out at `now`, not acknowledged yet.
pub(super) fn record_sent(timings: &mut [MessageTiming; MESSAGE_BACKUP], sequence: i32, now: i32) {
    timings[(sequence & (MESSAGE_BACKUP as i32 - 1)) as usize] = MessageTiming {
        sent: now,
        acked: -1,
    };
}

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// `SV_CalcPings`: measure every client's ping and tell the game each one it keeps
    /// ([`LegacyGameHost::client_ping`]). The reference runs it at the top of every
    /// server frame, before the game's; call it before the game's frame.
    pub fn calc_pings(&mut self) {
        for (client, slot) in self.slots.iter_mut().enumerate() {
            let stand = if slot.peer.address == Some(LegacyPeerAddress::Bot) {
                Stand::Bot
            } else {
                Stand::Player
            };
            let (ping, told) = calc_ping(slot.phase, stand, &slot.timings);
            slot.ping = ping;
            if told {
                self.game.client_ping(client, ping);
            }
        }
    }
}

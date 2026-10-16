//! Bot slots (`sv_bot.cpp`): a bot holds a wire slot so that legacy clients see it like
//! any player, but it has no transport. The game asks for one as its `addbot` runs
//! (`SV_BotAllocateClient`), gives it back when the bot cannot join
//! (`SV_BotFreeClient`), and tells the engine the bot's userinfo (`SV_SetUserinfo`).
use super::Slot;
use crate::{LegacyClientPhase, LegacyInfoString, LegacyPeerAddress};

/// `MAX_NAME_LENGTH`: the name the engine keeps of a userinfo.
const NAME_BYTES: usize = 32;

/// The engine's bot slots, as the game's `addbot` asks for them.
pub trait LegacyBotSlots {
    /// `SV_BotAllocateClient`: the lowest free slot, now a bot's and active at once;
    /// `None` when every slot is taken.
    fn allocate(&mut self) -> Option<usize>;
    /// `SV_BotFreeClient`: the slot free again.
    fn free(&mut self, client: usize);
    /// `SV_SetUserinfo`: the userinfo the engine keeps for the bot, and its name.
    fn set_userinfo(&mut self, client: usize, userinfo: &[u8]);
}

/// The session's slots as the game sees them while a console command runs.
pub(super) struct BotSlots<'a> {
    pub slots: &'a mut [Slot],
    pub ring: usize,
}

impl LegacyBotSlots for BotSlots<'_> {
    fn allocate(&mut self) -> Option<usize> {
        let client = self
            .slots
            .iter()
            .position(|slot| slot.phase == LegacyClientPhase::Free)?;
        let slot = &mut self.slots[client];
        *slot = Slot::free(self.ring);
        slot.phase = LegacyClientPhase::Active;
        slot.peer.address = Some(LegacyPeerAddress::Bot);
        slot.rate = 16384;
        Some(client)
    }
    fn free(&mut self, client: usize) {
        if let Some(slot) = self.slots.get_mut(client) {
            *slot = Slot::free(self.ring);
        }
    }
    fn set_userinfo(&mut self, client: usize, userinfo: &[u8]) {
        let Some(slot) = self.slots.get_mut(client) else {
            return;
        };
        slot.userinfo = LegacyInfoString::from_truncated(userinfo);
        slot.name.clear();
        slot.name
            .extend_from_slice(sjk_protocol::info_value(userinfo, b"name").unwrap_or_default());
        slot.name.truncate(NAME_BYTES - 1);
    }
}

/// `SV_BotGetConsoleMessage`, which a bot's thinking calls until nothing is left: every
/// bot has read what it was sent, and has been heard from now (so it never times out).
pub(super) fn acknowledge_bot_commands(slots: &mut [Slot], now: i32) {
    for slot in slots
        .iter_mut()
        .filter(|slot| slot.peer.address == Some(LegacyPeerAddress::Bot))
    {
        slot.peer.last_packet_time = now;
        slot.wire.reliable.acknowledge_all();
    }
}

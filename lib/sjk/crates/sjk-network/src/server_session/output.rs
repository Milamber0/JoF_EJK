//! What the game tells its clients between snapshots: server commands, configstring
//! changes and drops, taken from the game after every datagram and frame.
use super::{LegacyGameHost, LegacyGameOutput, hosts::View};
use crate::{
    LegacyClientPhase, LegacyConfigStringHost, LegacyConfigStringMarks, LegacyReliableState,
    legacy_config_string_changed, legacy_config_strings_catch_up,
};

/// The game's output, copied out of it so that acting on it may call the game again
/// (a command that overflows a client's ring drops that client). Both vectors are
/// reused; nothing is allocated once they have grown.
#[derive(Default)]
pub(super) struct Pending {
    records: Vec<Record>,
    bytes: Vec<u8>,
    /// One command being put together.
    scratch: Vec<u8>,
}

enum Record {
    ServerCommand {
        client: Option<usize>,
        text: std::ops::Range<usize>,
    },
    ConfigString {
        index: usize,
        previous: std::ops::Range<usize>,
        value: std::ops::Range<usize>,
    },
    Drop {
        client: usize,
        reason: std::ops::Range<usize>,
    },
    MapChanged {
        server_time: i32,
    },
    MapRestarted,
}

/// `SNAPFLAG_SERVERCOUNT`.
const SNAPFLAG_SERVERCOUNT: u8 = 4;

impl Pending {
    fn keep(&mut self, bytes: &[u8]) -> std::ops::Range<usize> {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        start..self.bytes.len()
    }
}

impl<G: LegacyGameHost> View<'_, G> {
    /// Take the game's output and act on it, in order.
    pub fn tell(&mut self, pending: &mut Pending) {
        pending.records.clear();
        pending.bytes.clear();
        self.game.take_output(&mut |output| {
            let record = match output {
                LegacyGameOutput::ServerCommand { client, text } => Record::ServerCommand {
                    client,
                    text: pending.keep(text),
                },
                LegacyGameOutput::ConfigString {
                    index,
                    previous,
                    value,
                } => Record::ConfigString {
                    index,
                    previous: pending.keep(previous),
                    value: pending.keep(value),
                },
                LegacyGameOutput::Drop { client, reason } => Record::Drop {
                    client,
                    reason: pending.keep(reason),
                },
                LegacyGameOutput::MapChanged { server_time } => Record::MapChanged { server_time },
                LegacyGameOutput::MapRestarted => Record::MapRestarted,
            };
            pending.records.push(record);
        });
        let Pending {
            records,
            bytes,
            scratch,
        } = pending;
        for record in records.drain(..) {
            match record {
                Record::ServerCommand { client, text } => self.command(client, &bytes[text]),
                // This endpoint runs one map for as long as it exists: always `SS_GAME`.
                Record::ConfigString {
                    index,
                    previous,
                    value,
                } => {
                    legacy_config_string_changed(
                        self,
                        index,
                        &bytes[previous],
                        &bytes[value],
                        true,
                        scratch,
                    );
                }
                Record::Drop { client, reason } if client < self.slots.len() => {
                    self.drop(client, &bytes[reason], None)
                }
                Record::Drop { .. } => {}
                Record::MapChanged { server_time } => self.map_changed(server_time),
                Record::MapRestarted => self.map_restarted(),
            }
        }
    }

    /// `SV_SpawnServer`'s effect on the clients already connected (`sv_init.cpp:640-684`):
    /// each keeps its slot, remembers the old world's clock as its `oldServerTime`, and
    /// goes back to `CS_CONNECTED` with a gamestate owed. The reference does not push
    /// one — *"when we get the next packet from a connected client, the new gamestate
    /// will be sent"* — and neither does this, so a client that has gone quiet is not
    /// sent a gamestate it cannot acknowledge.
    ///
    /// The configstring marks are cleared too: they track what changed while a client
    /// was primed, and nothing about the old world's strings means anything now.
    fn map_changed(&mut self, server_time: i32) {
        // A stopped server (`killserver`) runs again with the new map.
        self.lifecycle.running = true;
        super::auto_demo::stop(self);
        // "send a heartbeat now so the master will get up to date info".
        *self.heartbeat_due = true;
        *self.server_bit ^= SNAPFLAG_SERVERCOUNT;
        for (client, slot) in self.slots.iter_mut().enumerate() {
            if !matches!(
                slot.phase,
                LegacyClientPhase::Primed | LegacyClientPhase::Active
            ) {
                continue;
            }
            slot.old_server_time = server_time;
            slot.phase = LegacyClientPhase::Connected;
            slot.gamestate_due = true;
            slot.marks = LegacyConfigStringMarks::default();
            // The next snapshot must wait for the new gamestate to be acknowledged.
            slot.next_snapshot_time = server_time;
            // SV_SpawnServer begins bots immediately: they have no transport to
            // acknowledge a gamestate (codemp/server/sv_init.cpp).
            if slot.peer.address == Some(crate::LegacyPeerAddress::Bot) {
                slot.phase = LegacyClientPhase::Active;
                slot.old_server_time = 0;
                slot.gamestate_due = false;
                slot.wire.movement.reenter();
                self.game
                    .enter_world(client, &sjk_protocol::UserCommand::default(), server_time);
            }
        }
        // `sv.realMapTimeStarted`, `sv.demosPruned`; nobody is in the new world yet, so
        // this only prunes.
        *self.auto_demo = super::auto_demo::AutoDemoLevel::starting(self.game.timestamp());
        super::auto_demo::begin(self);
    }

    /// `SV_MapRestart_f`'s wire half (`sv_ccmds.cpp:289-373`): the server bit toggles,
    /// and every client from `CS_CONNECTED` up is sent `map_restart` and keeps its
    /// gamestate. One in the world enters it again (`SV_ClientEnterWorld`: a full
    /// snapshot next); one still loading forgets its last command, "or the client will
    /// hang", and a primed one is owed no old time (`sv.restartTime`, 0 by now).
    fn map_restarted(&mut self) {
        super::auto_demo::stop(self);
        *self.server_bit ^= SNAPFLAG_SERVERCOUNT;
        *self.auto_demo = super::auto_demo::AutoDemoLevel::starting(self.game.timestamp());
        for client in 0..self.slots.len() {
            let phase = self.slots[client].phase;
            if !matches!(
                phase,
                LegacyClientPhase::Connected
                    | LegacyClientPhase::Primed
                    | LegacyClientPhase::Active
            ) {
                continue;
            }
            self.command(Some(client), b"map_restart\n");
            let slot = &mut self.slots[client];
            if phase == LegacyClientPhase::Primed {
                slot.old_server_time = 0;
            }
            if phase == LegacyClientPhase::Active {
                slot.wire.movement.reenter();
            } else {
                slot.wire.movement = crate::LegacyMoveState::default();
            }
        }
        // Those in the world enter it again (`SV_ClientEnterWorld`), each starting a demo.
        super::auto_demo::begin(self);
    }

    /// `SV_SendServerCommand`: to one client, or to everyone who holds a gamestate. A
    /// full ring drops its client; the broadcast goes on.
    pub(super) fn command(&mut self, client: Option<usize>, text: &[u8]) {
        if client.is_some_and(|client| client >= self.slots.len()) {
            return;
        }
        let mut drop = super::hosts::DropView {
            view: self,
            executing: None,
        };
        // The only other failure is an exhausted 31-bit command counter; see `View::drop`.
        let _ = crate::send_legacy_server_command(&mut drop, client, text);
    }
}

impl<G: LegacyGameHost> LegacyConfigStringHost for View<'_, G> {
    fn client_count(&self) -> usize {
        self.slots.len()
    }
    fn phase(&self, client: usize) -> LegacyClientPhase {
        self.slots[client].phase
    }
    // Bots, whose entities carry `SVF_NOSERVERINFO`, do not exist yet.
    fn withholds_server_info(&self, _: usize) -> bool {
        false
    }
    fn marks(&mut self, client: usize) -> &mut LegacyConfigStringMarks {
        &mut self.slots[client].marks
    }
    fn send(&mut self, client: usize, command: &[u8]) {
        self.command(Some(client), command);
    }
}

/// The client whose packet is executing, entering the world: its reliable history is
/// out of its slot, so its catch-up is written to the history passed along.
struct Entering<'a> {
    marks: &'a mut LegacyConfigStringMarks,
    reliable: &'a mut LegacyReliableState,
    overflowed: bool,
}

impl LegacyConfigStringHost for Entering<'_> {
    fn client_count(&self) -> usize {
        1
    }
    fn phase(&self, _: usize) -> LegacyClientPhase {
        LegacyClientPhase::Active
    }
    fn withholds_server_info(&self, _: usize) -> bool {
        false
    }
    fn marks(&mut self, _: usize) -> &mut LegacyConfigStringMarks {
        self.marks
    }
    fn send(&mut self, _: usize, command: &[u8]) {
        self.overflowed |= self.reliable.queue_server_command(true, command).is_err();
    }
}

/// `SV_UpdateConfigstrings` for the executing client; returns whether its ring
/// overflowed, which costs it the connection.
pub(super) fn catch_up<G: LegacyGameHost>(
    view: &mut View<'_, G>,
    client: usize,
    reliable: &mut LegacyReliableState,
) -> bool {
    let View { slots, game, .. } = view;
    let mut entering = Entering {
        marks: &mut slots[client].marks,
        reliable,
        overflowed: false,
    };
    let mut scratch = Vec::new();
    let game: &G = game;
    let value = |index: usize| {
        game.config_strings()
            .find(|(found, _)| *found == index)
            .map_or(&[][..], |(_, value)| value)
    };
    legacy_config_strings_catch_up(&mut entering, client, value, &mut scratch);
    entering.overflowed
}

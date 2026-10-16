//! Move-packet batching and `cl_packetdup` command redundancy.
//!
//! Commands are made on their own clock ([`crate::command_rate`]) and wait here
//! until a packet leaves (`cl_maxpackets`), which carries every command made
//! since the previous packet, as `CL_WritePacket` does
//! (`codemp/client/cl_input.cpp:1536-1580`). The stock client also resends the
//! usercmds of the previous `cl_packetdup` packets so that a dropped or
//! reordered datagram does not lose a movement step server-side. The server
//! executes only the commands newer than the last one it ran
//! (`codemp/server/sv_client.cpp:1425-1440`), so duplicates cost bandwidth,
//! never movement.

use sjk_protocol::UserCommand;

/// Stock clamp for `cl_packetdup` (`cl_input.cpp:1539-1543`).
pub const MAX_PACKET_DUP: usize = 5;
/// Stock default (`codemp/client/cl_main.cpp:2771`).
pub const DEFAULT_PACKET_DUP: usize = 1;
/// Most commands one packet carries (`MAX_PACKET_USERCMDS`); no command older
/// than that is ever sent again, so it is also all the history kept.
const CAPACITY: usize = sjk_network::MAX_PACKET_USER_COMMANDS;

/// Fixed-size history of made commands, oldest first; no heap use.
#[derive(Debug)]
pub struct CommandHistory {
    recent: [UserCommand; CAPACITY],
    len: usize,
    /// The newest `unsent` commands of `recent` wait for the next packet.
    unsent: usize,
    /// Commands carried new by each of the last packets, newest last.
    packets: [usize; MAX_PACKET_DUP],
    packet_count: usize,
    packet_dup: usize,
    awaiting_gamestate_snapshot: bool,
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(DEFAULT_PACKET_DUP)
    }
}

impl CommandHistory {
    /// History repeating the previous `packet_dup` packets (clamped to stock).
    pub fn new(packet_dup: usize) -> Self {
        Self {
            recent: [UserCommand::default(); CAPACITY],
            len: 0,
            unsent: 0,
            packets: [0; MAX_PACKET_DUP],
            packet_count: 0,
            packet_dup: packet_dup.min(MAX_PACKET_DUP),
            awaiting_gamestate_snapshot: false,
        }
    }

    /// Number of earlier packets whose commands each packet repeats.
    pub fn packet_dup(&self) -> usize {
        self.packet_dup
    }

    /// Change the redundancy; takes effect from the next packet.
    pub fn set_packet_dup(&mut self, packet_dup: usize) {
        self.packet_dup = packet_dup.min(MAX_PACKET_DUP);
    }

    /// Forget the history (new gamestate: the server's time line restarted).
    pub fn clear(&mut self) {
        self.len = 0;
        self.unsent = 0;
        self.packet_count = 0;
    }

    /// Atomically invalidate old-map input when a gamestate is decoded.
    /// `CL_ParseGamestate` clears client time before installing the new server ID
    /// (codemp/client/cl_parse.cpp:514-517,605-606).
    pub fn begin_gamestate(&mut self) {
        self.clear();
        self.awaiting_gamestate_snapshot = true;
    }

    /// A snapshot of the replacement gamestate can now reach the input clock.
    pub fn received_gamestate_snapshot(&mut self) {
        self.awaiting_gamestate_snapshot = false;
    }

    /// Hold `command` for the next packet. Past [`CAPACITY`] unsent commands
    /// the oldest is dropped, as `CL_WritePacket` truncates to
    /// `MAX_PACKET_USERCMDS`.
    pub fn queue(&mut self, command: UserCommand) {
        // A receive timeout can fall between svc_gamestate and its snapshot.
        // Never pair a new server ID with a command from the departed timeline.
        let command = if self.awaiting_gamestate_snapshot {
            UserCommand::default()
        } else {
            command
        };
        if self.len == CAPACITY {
            self.recent.copy_within(1.., 0);
            self.len -= 1;
        }
        self.recent[self.len] = command;
        self.len += 1;
        self.unsent = (self.unsent + 1).min(self.len);
    }

    /// Commands waiting for a packet.
    pub fn unsent(&self) -> usize {
        self.unsent
    }

    /// Close a packet and return its batch, oldest first: the commands of the
    /// previous `packet_dup` packets, then every unsent one, at most
    /// [`CAPACITY`]. Only a run of strictly increasing server times ending at
    /// the newest command goes out: the server treats any command newer than
    /// the packet's last as pre-restart garbage (`sv_client.cpp:1428-1430`).
    /// Empty when nothing is waiting.
    pub fn take_packet(&mut self) -> &[UserCommand] {
        if self.unsent == 0 {
            return &[];
        }
        let new = self.unsent;
        let repeated: usize = self.packets
            [self.packet_count.saturating_sub(self.packet_dup)..self.packet_count]
            .iter()
            .sum();
        if self.packet_count == MAX_PACKET_DUP {
            self.packets.copy_within(1.., 0);
            self.packet_count -= 1;
        }
        self.packets[self.packet_count] = new;
        self.packet_count += 1;
        self.unsent = 0;
        let newest = self.len - 1;
        let oldest_wanted = self.len - (new + repeated).min(self.len);
        let mut start = newest;
        while start > oldest_wanted
            && self.recent[start - 1].server_time < self.recent[start].server_time
        {
            start -= 1;
        }
        &self.recent[start..self.len]
    }

    /// Queue `command` and close a packet with it at once.
    pub fn push(&mut self, command: UserCommand) -> &[UserCommand] {
        self.queue(command);
        self.take_packet()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(server_time: i32) -> UserCommand {
        UserCommand {
            server_time,
            ..UserCommand::default()
        }
    }

    fn times(batch: &[UserCommand]) -> Vec<i32> {
        batch.iter().map(|command| command.server_time).collect()
    }

    #[test]
    fn a_packet_carries_every_command_since_the_last_one() {
        let mut history = CommandHistory::new(0);
        history.queue(at(8));
        history.queue(at(16));
        assert_eq!(history.unsent(), 2);
        assert_eq!(times(history.take_packet()), [8, 16]);
        assert_eq!(history.unsent(), 0);
        assert!(history.take_packet().is_empty());
    }

    #[test]
    fn packet_dup_repeats_previous_packets_not_commands() {
        let mut history = CommandHistory::new(1);
        history.queue(at(8));
        history.queue(at(16));
        assert_eq!(times(history.take_packet()), [8, 16]);
        history.queue(at(24));
        history.queue(at(32));
        assert_eq!(times(history.take_packet()), [8, 16, 24, 32]);
        assert_eq!(times(history.push(at(40))), [24, 32, 40]);
        history.set_packet_dup(2);
        assert_eq!(times(history.push(at(48))), [24, 32, 40, 48]);
    }

    #[test]
    fn one_command_per_packet_repeats_the_previous_commands() {
        let mut history = CommandHistory::new(2);
        assert_eq!(times(history.push(at(8))), [8]);
        assert_eq!(times(history.push(at(16))), [8, 16]);
        assert_eq!(times(history.push(at(24))), [8, 16, 24]);
        assert_eq!(times(history.push(at(32))), [16, 24, 32]);
    }

    #[test]
    fn a_batch_never_exceeds_the_protocol_limit() {
        let mut history = CommandHistory::new(MAX_PACKET_DUP);
        for packet in 0..10 {
            for step in 0..10 {
                history.queue(at(8 * (packet * 10 + step + 1)));
            }
            let batch = history.take_packet();
            assert!(batch.len() <= CAPACITY);
            assert_eq!(batch.last().unwrap().server_time, 8 * (packet * 10 + 10));
        }
        for step in 0..40 {
            history.queue(at(1_000 + step));
        }
        let batch = history.take_packet();
        assert_eq!(batch.len(), CAPACITY, "oldest unsent dropped");
        assert_eq!(batch[0].server_time, 1_008);
    }

    #[test]
    fn stale_or_repeated_times_stay_out_of_the_batch() {
        let mut history = CommandHistory::new(3);
        history.push(at(5_000));
        history.push(at(9_000));
        // The server restarted its time line: older stamps would outrank this one.
        assert_eq!(times(history.push(at(40))), [40]);
        // Unanchored stamps are all 0: only the newest goes.
        history.queue(at(0));
        history.queue(at(0));
        assert_eq!(times(history.take_packet()), [0]);
    }

    #[test]
    fn a_new_gamestate_forgets_old_commands_and_stamps_zero() {
        let mut history = CommandHistory::new(1);
        history.push(at(100));
        history.queue(at(108));
        history.begin_gamestate();
        assert_eq!(history.unsent(), 0);
        assert_eq!(times(history.push(at(116))), [0]);
        history.received_gamestate_snapshot();
        assert_eq!(times(history.push(at(124))), [0, 124]);
    }
}

//! One legacy server endpoint: every protocol-26 piece behind a datagram interface.
//!
//! [`LegacyServerSession`] owns the roster of wire client numbers with each one's
//! channel, reliable history and movement history, and composes admission,
//! out-of-band queries, packet dispatch, the drop lifecycle and timeouts. It opens
//! no socket and reads no clock: the process that embeds it passes datagrams and
//! times in and receives datagrams to send through a callback, so the whole
//! endpoint runs unchanged under a test, a UDP socket or any other transport.
//!
//! The authoritative server is reached only through [`LegacyGameHost`]. A wire
//! client number is the key on that interface; which native peer, world and entity
//! stand behind it is the implementor's mapping, never this adapter's.
use crate::{
    LegacyChallenge, LegacyClientPhase, LegacyInfoString, LegacyMoveState, LegacyOobLimiter,
    LegacyOobLine, LegacyOobRates, LegacyPeer, LegacyReliableState, LegacyServerChannel,
    LegacyServerInfo, LegacyStatusPlayer,
};
use sjk_protocol::{EntityState, MessageWriter, UserCommand};
mod auto_demo;
mod bots;
mod console;
mod datagram;
mod demo;
mod download;
mod gamestate;
mod hosts;
mod output;
mod ping;
mod schedule;
mod shutdown;
mod snapshot;
mod transmit;
mod whitelist;
pub use auto_demo::{
    LegacyAutoDemoSettings, LegacyDemoFolders, legacy_auto_demo_name, legacy_prune_auto_demos,
};
pub use bots::LegacyBotSlots;
pub use console::LegacyConsoleSettings;
pub use download::LegacyDownloadFile;
pub use schedule::{LegacyRateSettings, legacy_is_private_address};
pub use snapshot::{
    LEGACY_SNAPSHOT_ENTITIES, LegacySnapshotFrame, LegacySnapshotRefusal, LocalSnapshotBuffer,
};

/// The authoritative server and game, as the legacy endpoint needs them.
///
/// `client` is always a wire client number below the session's roster size.
/// Every call completes before it returns; none may call back into the session.
pub trait LegacyGameHost {
    /// Admit the peer under the server's own budgets and run game connect.
    /// `Err` carries the refusal text the client is shown.
    fn client_connect(&mut self, client: usize, userinfo: &[u8]) -> Result<(), Vec<u8>>;
    /// Release everything `client_connect` admitted, including its actor.
    fn client_disconnect(&mut self, client: usize);
    /// A reliable client command the endpoint does not handle itself, at the server's
    /// clock `server_time`.
    fn client_command(&mut self, client: usize, text: &[u8], server_time: i32);
    /// The client's userinfo changed and was accepted; `userinfo` carries the
    /// server's `ip` key.
    fn client_userinfo_changed(&mut self, client: usize, userinfo: &[u8]);
    /// Place the client in the world at the server's clock `server_time`, seeded with
    /// its first movement command.
    fn enter_world(&mut self, client: usize, command: &UserCommand, server_time: i32);
    /// Simulate one movement command. `server_time` is the server's clock, which the
    /// game holds a command's own time against.
    fn client_think(&mut self, client: usize, command: &UserCommand, server_time: i32);
    /// What `getinfo` should report. Figures need not be roster figures.
    fn server_info(&self) -> LegacyServerInfo<'_>;
    /// The server-info string `getstatus` reports.
    fn status_info(&self) -> &[u8];
    /// The players `getstatus` should list, in order; need not be roster clients.
    fn status_players(&self) -> impl Iterator<Item = LegacyStatusPlayer<'_>>;
    /// Current legacy map generation and the oldest one of this restart interval.
    fn server_ids(&self) -> (i32, i32);
    /// The running map's non-empty configstrings, in index order.
    fn config_strings(&self) -> impl Iterator<Item = (usize, &[u8])>;
    /// The running map's entity baselines, in wire entity-number order.
    fn baselines(&self) -> impl Iterator<Item = &EntityState>;
    /// The map's checksum feed, which keys every movement packet.
    fn checksum_feed(&self) -> i32;
    /// The baseline of one wire entity number, as sent in the gamestate.
    fn baseline(&self, number: u16) -> Option<&EntityState>;
    /// Project the world onto what this client is shown in one snapshot. Called for
    /// every connected client, before it has entered the world too, as the
    /// reference builds a frame for anyone who is not a zombie.
    fn build_snapshot(&self, client: usize, frame: &mut LegacySnapshotFrame<'_>);
    /// Hand over what the game wants told since the last call, in order, and forget
    /// it. The endpoint asks after every datagram and at every frame, so output raised
    /// inside any other call of this trait leaves with that datagram or frame. A game
    /// that never tells anything need not implement this.
    fn take_output(&mut self, _tell: &mut dyn FnMut(LegacyGameOutput<'_>)) {}
    /// The client's `PERS_SCORE`, which `status` lists.
    fn client_score(&self, _client: usize) -> i32 {
        0
    }
    /// An operator's console line no engine command claims: a cvar, one of the game's
    /// console commands, or a world command (`map`, `map_restart`), at the server's clock
    /// `server_time`. What it prints goes to `print`, one message per call, and reaches
    /// the operator whether it typed the line or sent it by `rcon`. Returns whether
    /// anything took the line; an unclaimed one is silent, as on a dedicated server.
    fn console_command(
        &mut self,
        _line: &[u8],
        _server_time: i32,
        _bots: &mut dyn LegacyBotSlots,
        _print: &mut dyn FnMut(&[u8]),
    ) -> bool {
        false
    }
    /// A line for the server's own console that no client is shown: who sent an `rcon`.
    fn console_log(&mut self, _text: &[u8]) {}
    /// The client's ping was measured (`ps->ping`, which the scoreboard and `getstatus`
    /// report). Only a client in the world is told.
    fn client_ping(&mut self, _client: usize, _ping: i32) {}
    /// The ban file's text (`sv_banFile` in the server's own directory), `None` where
    /// there is none.
    fn ban_file(&mut self) -> Option<Vec<u8>> {
        None
    }
    /// Keep the ban list's text in the ban file; a server without one keeps nothing.
    fn save_ban_file(&mut self, _text: &[u8]) {}
    /// Look a host name up, for a ban an operator gives by name.
    fn resolve_host(&mut self, _name: &str) -> Option<std::net::Ipv4Addr> {
        None
    }
    /// Open a server demo's file (`demos/<name>.dm_26`) for `client`; `false` where it
    /// cannot be. A server that keeps no files records nothing.
    fn demo_open(&mut self, _client: usize, _path: &str) -> bool {
        false
    }
    /// Bytes for `client`'s open demo file, in order.
    fn demo_data(&mut self, _client: usize, _bytes: &[u8]) {}
    /// `client`'s demo file is complete.
    fn demo_close(&mut self, _client: usize) {}
    /// Whether a file of that path exists among the server's own (`FS_FileExists`).
    fn file_exists(&mut self, _path: &str) -> bool {
        false
    }
    /// The local date and time as `SV_DemoFilename` writes it (`%Y-%m-%d_%H-%M-%S`).
    fn timestamp(&self) -> String {
        String::new()
    }
    /// A file a client asks to download (`download <name>`): only a pak the server
    /// references may be sent (`FS_MV_VerifyDownloadPath`).
    fn download_file(&self, _name: &[u8]) -> LegacyDownloadFile {
        LegacyDownloadFile::NotReferenced
    }
    /// Keep only the `keep` newest maps' folders of automatic demos
    /// (`sv_autoDemoMaxMaps`); see [`legacy_prune_auto_demos`].
    fn prune_auto_demos(&mut self, _keep: usize) {}
    /// `ipwhitelist.dat` in the server's own directory, `None` where there is none.
    fn whitelist_file(&mut self) -> Option<Vec<u8>> {
        None
    }
    /// Append one address's record to `ipwhitelist.dat`; `false` where the file cannot
    /// be opened. A host that keeps no files succeeds without writing.
    fn append_whitelist(&mut self, _record: [u8; 4]) -> bool {
        true
    }
}

/// Something the game wants done on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyGameOutput<'a> {
    /// `trap->SendServerCommand`: a reliable command for one client, or for everyone
    /// who holds a gamestate (`None`).
    ServerCommand {
        /// The wire client number, or `None` for a broadcast.
        client: Option<usize>,
        /// The command text, without a terminator.
        text: &'a [u8],
    },
    /// `trap->SetConfigstring` changed a value. [`LegacyGameHost::config_strings`]
    /// must already yield the new one.
    ConfigString {
        /// The configstring's index.
        index: usize,
        /// What it was.
        previous: &'a [u8],
        /// What it is.
        value: &'a [u8],
    },
    /// `trap->DropClient`.
    Drop {
        /// The wire client number.
        client: usize,
        /// The reason the client and everyone else is shown.
        reason: &'a [u8],
    },
    /// The world is a new one: a map change (`SV_SpawnServer`). The game has already
    /// rebuilt everything [`LegacyGameHost::config_strings`],
    /// [`LegacyGameHost::baselines`], [`LegacyGameHost::server_ids`] and
    /// [`LegacyGameHost::checksum_feed`] answer with, and this is what makes the legacy
    /// side of the change happen.
    ///
    /// Every client that holds a gamestate goes back to
    /// [`LegacyClientPhase::Connected`] with a gamestate owed — the reference does not
    /// push one either: *"when we get the next packet from a connected client, the new
    /// gamestate will be sent"* (`sv_init.cpp:663-666`). `server_time` is the old
    /// world's clock at the moment it ended, which each client is told as its
    /// `oldServerTime` so that it can keep its own clock across the change.
    MapChanged {
        /// The server time the old world stopped at (`client->oldServerTime`).
        server_time: i32,
    },
    /// The level was played again on the same world: a `map_restart`
    /// (`SV_MapRestart_f`, `sv_ccmds.cpp:287-373`). The game has already rebuilt its
    /// level, told the configstrings that changed, and advanced
    /// [`LegacyGameHost::server_ids`]' current id while keeping the restarted one. Every
    /// client that holds a gamestate keeps it and is sent the `map_restart` command;
    /// the snapshots' `SNAPFLAG_SERVERCOUNT` bit toggles; a client in the world gets a
    /// full snapshot next, and one still loading forgets its last command. The game
    /// speaks for each client's reconnect and begin after this.
    MapRestarted,
}

/// Stock server settings the endpoint consults; all times in seconds.
#[derive(Clone, Debug)]
pub struct LegacySessionSettings {
    /// `sv_timeout`.
    pub timeout_seconds: i32,
    /// `sv_zombietime`.
    pub zombie_seconds: i32,
    /// `sv_reconnectlimit`.
    pub reconnect_limit_seconds: i32,
    /// `sv_privateClients`.
    pub private_clients: i32,
    /// `sv_privatePassword`.
    pub private_password: Vec<u8>,
    /// `sv_maxOOBRateIP` and `sv_maxOOBRate`.
    pub oob_rates: LegacyOobRates,
    /// `sv_floodProtect`: zero disables, one means a second, otherwise milliseconds.
    pub flood_protect: i32,
    /// `sv_legacyFixes`.
    pub legacy_fixes: bool,
    /// Rate and snapshot-rate cvars.
    pub rates: LegacyRateSettings,
    /// Entity states each client's snapshot history holds across its 32 frames.
    /// The reference shares `sv_maxclients * 32 * 64` among all clients.
    pub snapshot_entity_ring: usize,
    /// The console and remote console.
    pub console: LegacyConsoleSettings,
    /// `sv_allowDownload`: whether a referenced pak is sent to a client missing it.
    pub allow_download: bool,
    /// `sv_pure`, which words a refused download's reason.
    pub pure: bool,
    /// `sv_autoDemo` and `sv_autoDemoMaxMaps`.
    pub auto_demo: LegacyAutoDemoSettings,
    /// `sv_autoWhitelist`: every player that enters the world is whitelisted.
    pub auto_whitelist: bool,
}

impl Default for LegacySessionSettings {
    /// The reference's cvar defaults.
    fn default() -> Self {
        Self {
            timeout_seconds: 200,
            zombie_seconds: 2,
            reconnect_limit_seconds: 3,
            private_clients: 0,
            private_password: Vec::new(),
            oob_rates: LegacyOobRates {
                per_address: 1,
                global: 1000,
            },
            flood_protect: 1,
            legacy_fixes: true,
            rates: LegacyRateSettings::default(),
            snapshot_entity_ring: 32 * 64,
            console: LegacyConsoleSettings::default(),
            allow_download: false,
            pure: false,
            auto_demo: LegacyAutoDemoSettings::default(),
            auto_whitelist: true,
        }
    }
}

/// The two clocks the reference keeps apart.
#[derive(Clone, Copy, Debug)]
pub struct LegacyClock {
    /// `svs.time`: server time in integer milliseconds, advanced by server frames.
    pub server_time: i32,
    /// `Sys_Milliseconds`: wall-clock milliseconds, used by the rate limiters.
    pub wall_time: i32,
}

/// Transport state of one wire client number; swapped out while its packet runs.
struct Wire {
    channel: LegacyServerChannel,
    reliable: LegacyReliableState,
    movement: LegacyMoveState,
    history: snapshot::SnapshotHistory,
}

impl Wire {
    fn new(challenge: i32, snapshot_entity_ring: usize) -> Self {
        Self {
            channel: LegacyServerChannel::new(challenge),
            reliable: LegacyReliableState::new(),
            movement: LegacyMoveState::default(),
            history: snapshot::SnapshotHistory::new(snapshot_entity_ring),
        }
    }

    /// Start over for a new connection; the snapshot history keeps its storage.
    fn reset(&mut self, challenge: i32) {
        self.channel = LegacyServerChannel::new(challenge);
        self.reliable = LegacyReliableState::new();
        self.movement = LegacyMoveState::default();
        self.history.reset();
    }
}

struct Slot {
    phase: LegacyClientPhase,
    peer: LegacyPeer,
    wire: Wire,
    name: Vec<u8>,
    userinfo: LegacyInfoString,
    /// `rate`: bytes per second this client may be sent.
    rate: i32,
    /// `wishSnaps`: snapshots per second the client asked for.
    wish_snaps: i32,
    /// `snapshotMsec`: the least time between two snapshots.
    snapshot_msec: i32,
    /// `nextSnapshotTime`: server time before which nothing is sent.
    next_snapshot_time: i32,
    /// `rateDelayed`: the last message was held back by the rate, not the interval.
    rate_delayed: bool,
    /// Server time until which further userinfo changes count against the limit.
    last_userinfo_change: i32,
    last_userinfo_count: i32,
    /// A gamestate was requested and leaves as soon as the channel is free.
    gamestate_due: bool,
    /// `client->oldServerTime`: the clock the last world stopped at, which a client
    /// carries across a map change so that its own does not jump. Zero until this
    /// client has ridden one.
    old_server_time: i32,
    /// The client's pure checksums were accepted (`pureAuthentic`).
    pure_authentic: bool,
    /// The client answered the pure check at all (`gotCP`).
    got_cp: bool,
    /// Configstrings that changed while this client was primed (`csUpdated`).
    marks: crate::LegacyConfigStringMarks,
    /// `ping`, as [`LegacyServerSession::calc_pings`] last measured it.
    ping: i32,
    /// `frames[].messageSent`/`messageAcked` of the last 32 messages, by sequence.
    timings: [ping::MessageTiming; ping::MESSAGE_BACKUP],
    /// A server demo of this client (`client->demo`).
    demo: demo::DemoState,
    /// A download to this client (`client->download*`).
    download: download::DownloadState,
}

impl Slot {
    /// A slot nobody holds.
    fn free(ring: usize) -> Self {
        Self {
            phase: LegacyClientPhase::Free,
            peer: LegacyPeer::default(),
            wire: Wire::new(0, ring),
            name: Vec::new(),
            userinfo: LegacyInfoString::new(),
            rate: 0,
            wish_snaps: 0,
            snapshot_msec: 0,
            next_snapshot_time: 0,
            rate_delayed: false,
            last_userinfo_change: 0,
            last_userinfo_count: 0,
            gamestate_due: false,
            old_server_time: 0,
            pure_authentic: false,
            got_cp: false,
            marks: crate::LegacyConfigStringMarks::default(),
            ping: 0,
            timings: Default::default(),
            demo: Default::default(),
            download: Default::default(),
        }
    }
}

/// A legacy endpoint in front of one authoritative server.
pub struct LegacyServerSession<G> {
    game: G,
    settings: LegacySessionSettings,
    slots: Vec<Slot>,
    /// Holds the executing client's transport while its slot stays reachable.
    spare: Wire,
    challenge: LegacyChallenge,
    limiter: LegacyOobLimiter,
    line: LegacyOobLine,
    reply: Vec<u8>,
    /// The remote console's output buffer; reused.
    redirect: crate::LegacyRedirect,
    /// The engine's ban list.
    bans: crate::LegacyBanList,
    /// Reused for every outgoing snapshot message.
    message: MessageWriter,
    heartbeat_due: bool,
    /// Whether the server runs, and whether `quit` or `killserver` ran.
    lifecycle: shutdown::Lifecycle,
    /// The game's output between being taken and being acted on; reused.
    output: output::Pending,
    /// `svs.snapFlagServerBit`: `SNAPFLAG_SERVERCOUNT`, toggled by every map change and
    /// restart so that a client can tell its snapshots come from a new level.
    server_bit: u8,
    /// The level's start and whether its demos were pruned.
    auto_demo: auto_demo::AutoDemoLevel,
    /// The denial-of-service whitelist.
    whitelist: crate::LegacyWhitelist,
}

/// Roster sizes protocol 26 cannot represent are refused at construction.
#[derive(Debug, Eq, PartialEq)]
pub struct LegacyRosterTooLarge {
    /// The size that was asked for.
    pub requested: usize,
}

impl<G: LegacyGameHost> LegacyServerSession<G> {
    /// Allocate every wire client number's transport once.
    ///
    /// `wire_clients` is how many legacy clients can be connected at a time, at
    /// most [`crate::LEGACY_WIRE_CLIENTS`]; it says nothing about how many peers
    /// the server behind `game` holds. `secret` must come from the operating
    /// system's random source, and `oob_senders` bounds the rate limiter's table.
    pub fn new(
        game: G,
        settings: LegacySessionSettings,
        wire_clients: usize,
        secret: [u8; 16],
        oob_senders: usize,
    ) -> Result<Self, LegacyRosterTooLarge> {
        if wire_clients > crate::LEGACY_WIRE_CLIENTS {
            return Err(LegacyRosterTooLarge {
                requested: wire_clients,
            });
        }
        let ring = settings.snapshot_entity_ring;
        let auto_demo = auto_demo::AutoDemoLevel::first(game.timestamp());
        Ok(Self {
            game,
            slots: (0..wire_clients).map(|_| Slot::free(ring)).collect(),
            spare: Wire::new(0, ring),
            settings,
            challenge: LegacyChallenge::new(secret),
            limiter: LegacyOobLimiter::new(oob_senders),
            line: LegacyOobLine::default(),
            reply: Vec::with_capacity(2048),
            redirect: crate::LegacyRedirect::default(),
            bans: crate::LegacyBanList::default(),
            message: MessageWriter::new(sjk_protocol::MAX_LEGACY_MESSAGE_BYTES),
            heartbeat_due: false,
            lifecycle: Default::default(),
            output: output::Pending::default(),
            server_bit: 0,
            auto_demo,
            whitelist: crate::LegacyWhitelist::default(),
        })
    }

    /// The authoritative side.
    pub fn game(&self) -> &G {
        &self.game
    }
    /// Mutable access to the authoritative side between datagrams and frames.
    pub fn game_mut(&mut self) -> &mut G {
        &mut self.game
    }
    /// The endpoint's settings, for an operator's change between datagrams and frames.
    pub fn settings_mut(&mut self) -> &mut LegacySessionSettings {
        &mut self.settings
    }
    /// Phase of a wire client number, or `None` beyond the roster.
    pub fn phase(&self, client: usize) -> Option<LegacyClientPhase> {
        self.slots.get(client).map(|slot| slot.phase)
    }
    /// Whether a master heartbeat became due; reading clears it.
    pub fn take_heartbeat_due(&mut self) -> bool {
        std::mem::take(&mut self.heartbeat_due)
    }
}

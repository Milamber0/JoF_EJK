//! Remote console and the engine's operator commands: `SVC_RemoteCommand`
//! (`sv_main.cpp:575`), print redirection (`common.cpp:97-149`) and the roster commands
//! of `sv_ccmds.cpp` — `status`, the kicks, `svsay`, `svtell`, `serverinfo`,
//! `dumpuser` — with `echo` (`cmd.cpp`).
//!
//! A command line runs as `Cmd_ExecuteString` runs it: the first token names the
//! command, without regard to case; a line no engine command claims goes to the host
//! ([`LegacyConsoleHost::game_command`]), which owns the cvars, the game's own console
//! commands and the world commands (`map`, `map_restart`). What a command prints is
//! handed over one `Com_Printf` message at a time, because that is the unit the
//! redirection packs into datagrams.
use crate::{LegacyClientPhase, LegacyPeerAddress, LegacyTokens, OOB_PREFIX};
use std::net::SocketAddrV4;
mod bans;
mod commands;
mod demos;
mod whitelist;

/// `SV_OUTPUTBUF_LENGTH`: the redirection buffer, terminator included.
pub const LEGACY_REDIRECT_BYTES: usize = 1024 - 16;
/// `remaining[1024]`: a longer command is not run at all.
const REMAINING_BYTES: usize = 1024;

/// What the operator commands read of the server and do to it.
///
/// `client` is always a wire client number below [`Self::client_count`].
pub trait LegacyConsoleHost {
    /// `sv_maxclients`.
    fn client_count(&self) -> usize;
    fn phase(&self, client: usize) -> LegacyClientPhase;
    fn address(&self, client: usize) -> LegacyPeerAddress;
    /// The name the server holds for the client (`client_t::name`).
    fn name(&self, client: usize) -> &[u8];
    fn userinfo(&self, client: usize) -> &[u8];
    fn rate(&self, client: usize) -> i32;
    fn ping(&self, client: usize) -> i32;
    /// The client's `PERS_SCORE`.
    fn score(&self, client: usize) -> i32;
    /// What `status` reports about the server itself.
    fn status(&self) -> LegacyConsoleStatus<'_>;
    /// `Cvar_InfoString(CVAR_SERVERINFO)`.
    fn server_info(&self) -> &[u8];
    /// A kick: `SV_DropClient` (which leaves a zombie alone), then the client's
    /// `lastPacketTime` set to now so that its zombie time runs from the kick.
    fn kick(&mut self, client: usize, reason: &[u8]);
    /// `SV_SendServerCommand`: one client, or everyone (`None`).
    fn server_command(&mut self, client: Option<usize>, text: &[u8]);
    /// A line no engine command claims: a cvar, a game console command or a world
    /// command. Output goes through `print`, one message per call. Returns whether
    /// anything took it; an unclaimed line is silent on a dedicated server.
    fn game_command(&mut self, line: &[u8], print: &mut dyn FnMut(&[u8])) -> bool;
    /// A line for the server's own console, which no client is shown: who sent an
    /// `rcon`, and whether its password was good.
    fn log(&mut self, text: &[u8]);
    /// The ban list the endpoint admits connections by.
    fn bans(&mut self) -> &mut crate::LegacyBanList;
    /// The ban file's text (`sv_banFile` in the server's own directory), `None` without
    /// one.
    fn ban_file(&mut self) -> Option<Vec<u8>>;
    /// Write the ban file (`SV_WriteBans`); nothing without one.
    fn save_ban_file(&mut self, text: &[u8]);
    /// A host name looked up (`gethostbyname`), for a ban given by name.
    fn resolve(&mut self, name: &str) -> Option<std::net::Ipv4Addr>;
    /// Whether a server demo of the client is being recorded.
    fn demo_recording(&self, client: usize) -> bool;
    /// Open `path` and start a demo of the client (`SV_RecordDemo` past its checks);
    /// `false` where the file could not be opened.
    fn start_demo(&mut self, client: usize, name: &[u8], path: &str) -> bool;
    /// End the client's demo (`SV_StopRecordDemo`).
    fn stop_demo(&mut self, client: usize);
    /// `FS_FileExists` among the server's own files.
    fn file_exists(&mut self, path: &str) -> bool;
    /// The local date and time, `%Y-%m-%d_%H-%M-%S`.
    fn timestamp(&self) -> String;
    /// `SVC_WhitelistAdr`: list an address and append it to the whitelist file, telling
    /// `print` what went wrong.
    fn whitelist(&mut self, address: std::net::Ipv4Addr, print: &mut dyn FnMut(&[u8]));
    /// `SV_Heartbeat_f`: a master heartbeat is due at once.
    fn heartbeat(&mut self);
    /// `Com_Quit_f`: the server shuts down, and nothing after this command runs.
    fn quit(&mut self);
    /// `SV_KillServer_f`: the server shuts down and waits for a map.
    fn kill_server(&mut self);
    /// `sv_running`: whether a server is running.
    fn running(&self) -> bool;
}

/// The server figures `SV_Status_f` prints.
#[derive(Clone, Copy, Debug)]
pub struct LegacyConsoleStatus<'a> {
    /// `sv_hostname`, colours still in.
    pub hostname: &'a [u8],
    /// `FS_GetCurrentGameDir`.
    pub game_directory: &'a [u8],
    /// `net_ip` and `net_port`.
    pub net_ip: &'a [u8],
    pub net_port: i32,
    /// `dedicated`: 0 listen, 1 LAN, 2 public.
    pub dedicated: i32,
    pub mapname: &'a [u8],
    pub gametype: i32,
    /// `sv_privateClients`.
    pub private_clients: i32,
    /// Seconds since the server started (`svs.startTime`).
    pub uptime_seconds: i64,
}

/// `Com_BeginRedirect`'s buffer: messages are packed until the next would not fit,
/// and each full buffer leaves as one `print` datagram.
#[derive(Default)]
pub struct LegacyRedirect {
    buffer: Vec<u8>,
    datagram: Vec<u8>,
}

impl LegacyRedirect {
    /// `Com_Printf` while redirected: the buffer is flushed first when `message` would
    /// overflow it, and a message that does not fit even an empty buffer is lost
    /// (`Q_strcat` refuses it whole). Stops at a NUL, as the C string does.
    pub fn print(&mut self, message: &[u8], send: &mut dyn FnMut(&[u8])) {
        let message = &message[..message
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(message.len())];
        if message.len() + self.buffer.len() > LEGACY_REDIRECT_BYTES - 1 {
            self.flush(send);
        }
        if message.len() < LEGACY_REDIRECT_BYTES - self.buffer.len() {
            self.buffer.extend_from_slice(message);
        }
    }

    /// `Com_EndRedirect`: whatever is buffered leaves, even nothing.
    pub fn finish(&mut self, send: &mut dyn FnMut(&[u8])) {
        self.flush(send);
    }

    /// `SV_FlushRedirect`: `NET_OutOfBandPrint("print\n%s")`.
    fn flush(&mut self, send: &mut dyn FnMut(&[u8])) {
        self.datagram.clear();
        self.datagram.extend_from_slice(&OOB_PREFIX);
        self.datagram.extend_from_slice(b"print\n");
        self.datagram.extend_from_slice(&self.buffer);
        send(&self.datagram);
        self.buffer.clear();
    }
}

/// `SVC_RemoteCommand` for one `rcon` line (the whole line as read, from `rcon` on).
///
/// The sender's line is logged first ([`LegacyConsoleHost::log`]), as the reference
/// logs it to its own console. The password is the line's second token, compared exactly; the command
/// run is what follows the password *as written*, found by the reference's own walk
/// (four bytes, spaces, the password's non-space bytes, spaces), so a quoted password
/// with a space in it passes the check and then runs its own tail. Every reply leaves
/// through `send` as a complete `print` datagram.
pub fn run_legacy_rcon(
    host: &mut impl LegacyConsoleHost,
    line: &[u8],
    password: &[u8],
    from: SocketAddrV4,
    redirect: &mut LegacyRedirect,
    send: &mut dyn FnMut(&[u8]),
) {
    let given = LegacyTokens::new(line).nth(1).unwrap_or_default();
    let valid = !password.is_empty() && given == password;
    let mut entry = Vec::new();
    entry.extend_from_slice(if valid {
        b"Rcon from "
    } else {
        b"Bad rcon from "
    });
    entry.extend_from_slice(from.to_string().as_bytes());
    entry.extend_from_slice(b": ");
    args_from(line, 2, &mut entry);
    entry.push(b'\n');
    host.log(&entry);
    if password.is_empty() {
        redirect.print(b"No rconpassword set.\n", send);
    } else if !valid {
        redirect.print(b"Bad rconpassword.\n", send);
    } else {
        let command = rcon_command(line);
        let command = if command.len() < REMAINING_BYTES {
            command
        } else {
            &[]
        };
        execute_legacy_console(host, command, &mut |message| redirect.print(message, send));
    }
    redirect.finish(send);
}

/// The command after `rcon <password>`, by `SVC_RemoteCommand`'s walk.
fn rcon_command(line: &[u8]) -> &[u8] {
    let mut rest = line.get(4..).unwrap_or_default();
    let skip = |rest: &mut &[u8], keep: fn(u8) -> bool| {
        let end = rest
            .iter()
            .position(|&byte| !keep(byte))
            .unwrap_or(rest.len());
        *rest = &rest[end..];
    };
    skip(&mut rest, |byte| byte == b' ');
    skip(&mut rest, |byte| byte != b' ' && byte != 0);
    skip(&mut rest, |byte| byte == b' ');
    rest
}

/// `Cmd_ExecuteString` for the operator: an engine command by name, else the host's.
pub fn execute_legacy_console(
    host: &mut impl LegacyConsoleHost,
    line: &[u8],
    print: &mut dyn FnMut(&[u8]),
) {
    let mut tokens = LegacyTokens::new(line);
    let Some(name) = tokens.next() else { return };
    let is = |command: &[u8]| name.eq_ignore_ascii_case(command);
    let arguments = Arguments { line };
    // The commands that need a running server (`com_sv_running`): most say so, the ban
    // file's reading and `svrecord` in their own way.
    if !host.running() {
        const NEEDS_SERVER: [&[u8]; 16] = [
            b"status",
            b"kick",
            b"kickbots",
            b"kickall",
            b"kicknum",
            b"clientkick",
            b"svsay",
            b"svtell",
            b"serverinfo",
            b"dumpuser",
            b"sv_banaddr",
            b"sv_exceptaddr",
            b"sv_bandel",
            b"sv_exceptdel",
            b"sv_listbans",
            b"sv_flushbans",
        ];
        if NEEDS_SERVER.iter().any(|command| is(command)) {
            print(b"Server is not running.\n");
            return;
        }
        if is(b"sv_rehashbans") {
            return;
        }
        if is(b"svrecord") {
            print(b"cannot record server demo - null svs.clients\n");
            return;
        }
    }
    if is(b"echo") {
        let mut message = Vec::new();
        args_from(line, 1, &mut message);
        message.push(b'\n');
        print(&message);
    } else if is(b"status") {
        commands::status(host, &arguments, print);
    } else if is(b"kick") {
        commands::kick(host, &arguments, print);
    } else if is(b"kickbots") {
        commands::kick_where(host, |host, client| {
            host.address(client) == LegacyPeerAddress::Bot
        });
    } else if is(b"kickall") {
        commands::kick_where(host, |host, client| {
            host.address(client) != LegacyPeerAddress::Loopback
        });
    } else if is(b"kicknum") || is(b"clientkick") {
        commands::kick_number(host, &arguments, print);
    } else if is(b"svsay") {
        commands::say(host, &arguments, print);
    } else if is(b"svtell") {
        commands::tell(host, &arguments, print);
    } else if is(b"serverinfo") {
        print(b"Server info settings:\n");
        info_print(host.server_info(), print);
    } else if is(b"dumpuser") {
        commands::dump_user(host, &arguments, print);
    } else if is(b"sv_rehashbans") {
        bans::rehash(host);
    } else if is(b"sv_listbans") {
        host.bans().list(print);
    } else if is(b"sv_banaddr") || is(b"sv_exceptaddr") {
        bans::add(host, &arguments, is(b"sv_exceptaddr"), print);
    } else if is(b"sv_bandel") || is(b"sv_exceptdel") {
        bans::remove(host, &arguments, is(b"sv_exceptdel"), print);
    } else if is(b"sv_flushbans") {
        bans::flush(host, print);
    } else if is(b"quit") {
        host.quit();
    } else if is(b"killserver") {
        host.kill_server();
    } else if is(b"heartbeat") {
        host.heartbeat();
    } else if is(b"whitelistip") {
        whitelist::command(host, &arguments, print);
    } else if is(b"svrecord") {
        demos::record(host, &arguments, print);
    } else if is(b"svstoprecord") {
        demos::stop(host, &arguments, print);
    } else {
        host.game_command(line, print);
    }
}

/// One command line's arguments, read as `Cmd_Argv` and `Cmd_ArgsFrom` read them.
struct Arguments<'a> {
    line: &'a [u8],
}

impl<'a> Arguments<'a> {
    fn count(&self) -> usize {
        LegacyTokens::new(self.line).count()
    }
    fn get(&self, index: usize) -> &'a [u8] {
        LegacyTokens::new(self.line).nth(index).unwrap_or_default()
    }
    /// `Cmd_ArgsFrom(from)` truncated as `Q_strncpyz` into `size` bytes.
    fn from(&self, from: usize, size: usize) -> Vec<u8> {
        let mut out = Vec::new();
        args_from(self.line, from, &mut out);
        out.truncate(size - 1);
        out
    }
}

/// `Cmd_ArgsFrom`: the arguments from `from` on, one space apart.
fn args_from(line: &[u8], from: usize, out: &mut Vec<u8>) {
    for (index, token) in LegacyTokens::new(line).skip(from).enumerate() {
        if index > 0 {
            out.push(b' ');
        }
        out.extend_from_slice(token);
    }
}

/// `Info_Print`: each key padded to twenty and a space, then its value and a newline —
/// two messages per pair, or `MISSING VALUE` for a key without one.
fn info_print(info: &[u8], print: &mut dyn FnMut(&[u8])) {
    let mut rest = info.strip_prefix(b"\\").unwrap_or(info);
    let mut message = Vec::new();
    while !rest.is_empty() {
        let end = rest
            .iter()
            .position(|&byte| byte == b'\\')
            .unwrap_or(rest.len());
        message.clear();
        message.extend_from_slice(&rest[..end]);
        if message.len() < 20 {
            message.resize(20, b' ');
        }
        message.push(b' ');
        print(&message);
        rest = &rest[end..];
        let Some(value) = rest.strip_prefix(b"\\") else {
            print(b"MISSING VALUE\n");
            return;
        };
        let end = value
            .iter()
            .position(|&byte| byte == b'\\')
            .unwrap_or(value.len());
        message.clear();
        message.extend_from_slice(&value[..end]);
        message.push(b'\n');
        print(&message);
        rest = value.get(end + 1..).unwrap_or_default();
    }
}

/// `Q_StripColor`: every `^` and digit goes, pass after pass until none is left.
fn strip_color(text: &mut Vec<u8>) {
    loop {
        let before = text.len();
        let (mut read, mut write) = (0, 0);
        while read < text.len() {
            if text[read] == b'^' && text.get(read + 1).is_some_and(u8::is_ascii_digit) {
                read += 2;
            } else {
                text[write] = text[read];
                write += 1;
                read += 1;
            }
        }
        text.truncate(write);
        if text.len() == before {
            return;
        }
    }
}

/// `SV_ExpandNewlines`: each newline written as `\n`, at most 1,021 bytes out.
fn expand_newlines(text: &[u8], out: &mut Vec<u8>) {
    let start = out.len();
    for &byte in text {
        if out.len() - start >= 1024 - 3 {
            break;
        }
        if byte == b'\n' {
            out.extend_from_slice(b"\\n");
        } else {
            out.push(byte);
        }
    }
}

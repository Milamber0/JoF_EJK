//! The client's own game server: an `sjk-server` child process started by
//! the Create game screen, watched until it answers queries, fed its bots
//! through its console, and stopped when the player leaves.
//!
//! Safety of the owner's machine comes first: the server binds `127.0.0.1`
//! unless the player explicitly allows LAN players, runs as a LAN server
//! (`dedicated 1`, which never sends a master heartbeat) with every master
//! address cleared, and is started with `--quit-on-eof`, so it quits when
//! the client's end of its console pipe closes — however the client ends.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// File name of the server binary next to the client.
const SERVER_BINARY: &str = "sjk-server";
/// Environment variable naming the server binary explicitly.
pub(crate) const SERVER_BINARY_ENV: &str = "JKA_DEDICATED";
/// Environment variable naming the server log file explicitly.
pub(crate) const SERVER_LOG_ENV: &str = "SJK_SERVER_LOG";
/// File name of the server log beside the client's own log.
const SERVER_LOG: &str = "last-server.log";
/// Ports a LAN-visible server tries first: the stock game port and the nine
/// after it, which LAN browsers probe.
pub(crate) const LAN_PORTS: std::ops::RangeInclusive<u16> = 29_070..=29_079;
/// How long the server may take from `listening on` to answering `getinfo`.
const ANSWER_DEADLINE: Duration = Duration::from_secs(20);
/// How long a stopped server gets to quit on its own before it is killed.
const QUIT_GRACE: Duration = Duration::from_millis(1_500);
/// Master-server variables cleared on the command line.
const MASTER_CVARS: [&str; 5] = [
    "sv_master1",
    "sv_master2",
    "sv_master3",
    "sv_master4",
    "sv_master5",
];

/// What the player chose on the Create game screen, independent of where
/// the server binary and the game data are.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostSettings {
    /// Map without `maps/` and `.bsp` (`mp/ffa3`).
    pub(crate) map: String,
    /// `sjk-server --gametype` name (`ffa`, `ctf`, ...).
    pub(crate) gametype: &'static str,
    /// `sv_hostname`.
    pub(crate) hostname: String,
    /// Bot definitions added with `addbot`, one per bot.
    pub(crate) bots: Vec<String>,
    /// `addbot` skill, 1 to 5.
    pub(crate) bot_skill: u8,
    /// `fraglimit`, 0 for none.
    pub(crate) fraglimit: u32,
    /// `timelimit` in minutes, 0 for none.
    pub(crate) timelimit: u32,
    /// `capturelimit`, 0 for none.
    pub(crate) capturelimit: u32,
    /// Bind every interface instead of loopback only.
    pub(crate) allow_lan: bool,
    /// Enable server-authorized development commands for a devmap launch.
    pub(crate) cheats: bool,
}

impl HostSettings {
    /// Address the server binds: loopback only unless LAN players are
    /// allowed; port 0 lets the system choose a free one.
    pub(crate) fn bind_address(&self, port: u16) -> SocketAddr {
        let ip = if self.allow_lan {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        SocketAddr::new(IpAddr::V4(ip), port)
    }

    /// The server's command line for `game_data` bound on `port`.
    pub(crate) fn arguments(&self, game_data: &Path, port: u16) -> Vec<OsString> {
        let mut arguments: Vec<OsString> = Vec::with_capacity(40);
        let mut pair = |flag: &str, value: String| {
            arguments.push(flag.into());
            arguments.push(value.into());
        };
        pair("--bind", self.bind_address(port).to_string());
        pair("--map", self.map.clone());
        pair("--gametype", self.gametype.to_owned());
        pair("--hostname", self.hostname.clone());
        pair("--fraglimit", self.fraglimit.to_string());
        pair("--timelimit", self.timelimit.to_string());
        pair("--capturelimit", self.capturelimit.to_string());
        arguments.push("--game-data".into());
        arguments.push(game_data.as_os_str().to_owned());
        // A LAN server: `dedicated 2` is the only value that heartbeats.
        for (name, value) in [("dedicated", "1")]
            .into_iter()
            .chain(MASTER_CVARS.map(|name| (name, "")))
        {
            arguments.extend(["--set".into(), name.into(), value.into()]);
        }
        if self.cheats {
            arguments.push("--cheats".into());
            arguments.push("--team-auto-join".into());
        }
        arguments.push("--quit-on-eof".into());
        arguments
    }

    /// Console lines typed into the server once it answers: one `addbot`
    /// per bot, at the chosen skill.
    pub(crate) fn console_lines(&self) -> Vec<String> {
        let skill = self.bot_skill.clamp(1, 5);
        self.bots
            .iter()
            .map(|name| format!("addbot \"{name}\" {skill}"))
            .collect()
    }
}

/// The first port `is_free` accepts in `candidates`, else 0 (the system's
/// choice).
pub(crate) fn pick_port(
    candidates: impl IntoIterator<Item = u16>,
    mut is_free: impl FnMut(u16) -> bool,
) -> u16 {
    candidates
        .into_iter()
        .find(|&port| is_free(port))
        .unwrap_or(0)
}

/// Whether UDP `port` can be bound on every interface right now.
pub(crate) fn udp_port_free(port: u16) -> bool {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

/// The port the server should bind for `settings`: a LAN server prefers the
/// stock ports; a loopback one lets the system choose.
pub(crate) fn choose_port(settings: &HostSettings) -> u16 {
    if settings.allow_lan {
        pick_port(LAN_PORTS, udp_port_free)
    } else {
        0
    }
}

/// Where the server binary is: `override_path` if given; else beside the
/// client (`sjk-server-<suffix>` for a client named `sjk-<suffix>`, then plain
/// `sjk-server`); else the first `sjk-server` on `search_path`.
pub(crate) fn find_server_binary(
    client: &Path,
    override_path: Option<PathBuf>,
    search_path: Option<OsString>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(path);
    }
    let file = |name: &str| format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = Vec::with_capacity(4);
    if let Some(directory) = client.parent() {
        let stem = client
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        if let Some(suffix) = stem
            .strip_prefix("sjk-")
            .filter(|suffix| !suffix.starts_with("server"))
        {
            candidates.push(directory.join(file(&format!("{SERVER_BINARY}-{suffix}"))));
        }
        candidates.push(directory.join(file(SERVER_BINARY)));
    }
    if let Some(search_path) = search_path {
        candidates
            .extend(std::env::split_paths(&search_path).map(|dir| dir.join(file(SERVER_BINARY))));
    }
    candidates.into_iter().find(|path| exists(path))
}

/// [`find_server_binary`] for this process.
pub(crate) fn locate_server_binary() -> Option<PathBuf> {
    let client = std::env::current_exe().ok()?;
    let override_path = std::env::var_os(SERVER_BINARY_ENV)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    find_server_binary(
        &client,
        override_path,
        std::env::var_os("PATH"),
        Path::is_file,
    )
}

/// Where the server log goes: `override_path` if given; else beside the
/// client's own log when standard error is a file (`~/sjk-test/last-run.log`
/// gives `~/sjk-test/last-server.log`); else in `config_directory`.
pub(crate) fn server_log_path(
    override_path: Option<PathBuf>,
    client_log: Option<PathBuf>,
    config_directory: &Path,
) -> PathBuf {
    if let Some(path) = override_path {
        return path;
    }
    client_log.as_deref().and_then(Path::parent).map_or_else(
        || config_directory.join(SERVER_LOG),
        |directory| directory.join(SERVER_LOG),
    )
}

/// The file this process's standard error is written to, if it is one.
pub(crate) fn client_log_file() -> Option<PathBuf> {
    let target = std::fs::read_link("/proc/self/fd/2").ok()?;
    target.is_file().then_some(target)
}

/// The address the server printed in its `listening on ADDRESS` line, made
/// connectable: a server bound on every interface is joined over loopback.
pub(crate) fn listening_address(line: &str) -> Option<SocketAddr> {
    let mut address: SocketAddr = line.trim().strip_prefix("listening on ")?.parse().ok()?;
    if address.ip().is_unspecified() {
        address.set_ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    Some(address)
}

/// What the watcher threads report about a starting server.
enum Event {
    /// It printed where it listens.
    Listening(SocketAddr),
    /// It answered a `getinfo` at that address.
    Answering(SocketAddr),
    /// It did not answer in time.
    Silent,
}

/// Progress of a started server, polled each frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ServerPoll {
    /// Loading the map or not yet answering.
    Starting,
    /// Answering queries at this address; its bots have been sent.
    Ready(SocketAddr),
    /// Gone or never answered; the reason names the log.
    Failed(String),
}

/// A running `sjk-server` child owned by the client.
pub(crate) struct LocalServer {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    events: Receiver<Event>,
    address: Option<SocketAddr>,
    ready: bool,
    console: Vec<String>,
    log_path: PathBuf,
}

impl LocalServer {
    /// Start `program` with `arguments`; its output goes to `log_path`
    /// (rewritten), and `console` lines are typed once it answers.
    pub(crate) fn start(
        program: &Path,
        arguments: &[OsString],
        console: Vec<String>,
        log_path: &Path,
    ) -> std::io::Result<Self> {
        if let Some(directory) = log_path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        let log = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_path)?;
        let mut child = Command::new(program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log.try_clone()?))
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("no server output"))?;
        let (sender, events) = mpsc::channel();
        std::thread::Builder::new()
            .name("local-server-log".into())
            .spawn(move || copy_output(stdout, log, sender))?;
        Ok(Self {
            child: Some(child),
            stdin,
            events,
            address: None,
            ready: false,
            console,
            log_path: log_path.to_owned(),
        })
    }

    /// Where the server's output is written.
    pub(crate) fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// The address clients join, once the server has printed it.
    pub(crate) fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    /// Advance the start-up without blocking: note what the watchers saw,
    /// and type the bot lines the moment the server answers.
    pub(crate) fn poll(&mut self) -> ServerPoll {
        if let Some(status) = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
        {
            return ServerPoll::Failed(format!(
                "the server exited ({status}); see {}",
                self.log_path.display()
            ));
        }
        if self.child.is_none() {
            return ServerPoll::Failed("the server was stopped".to_owned());
        }
        loop {
            match self.events.try_recv() {
                Ok(Event::Listening(address)) => self.address = Some(address),
                Ok(Event::Answering(address)) => {
                    self.address = Some(address);
                    self.ready = true;
                    self.type_console();
                }
                Ok(Event::Silent) => {
                    return ServerPoll::Failed(format!(
                        "the server did not answer; see {}",
                        self.log_path.display()
                    ));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) if !self.ready => {
                    return ServerPoll::Failed(format!(
                        "the server closed its output; see {}",
                        self.log_path.display()
                    ));
                }
                Err(TryRecvError::Disconnected) => break,
            }
        }
        match self.address {
            Some(address) if self.ready => ServerPoll::Ready(address),
            _ => ServerPoll::Starting,
        }
    }

    /// Type the queued console lines into the server.
    fn type_console(&mut self) {
        let Some(stdin) = &mut self.stdin else { return };
        for line in self.console.drain(..) {
            crate::log::progress(format_args!("local server: {line}"));
            if writeln!(stdin, "{line}").is_err() {
                break;
            }
        }
        let _ = stdin.flush();
    }

    /// Stop the server without blocking the caller: it is asked to `quit`
    /// and, on a helper thread, killed if it has not within the grace time.
    pub(crate) fn stop(mut self) {
        let (Some(child), stdin) = (self.child.take(), self.stdin.take()) else {
            return;
        };
        let spawned = std::thread::Builder::new()
            .name("local-server-stop".into())
            .spawn(move || shut_down(child, stdin));
        if let Err(error) = spawned {
            crate::log::progress(format_args!("local server: no stop thread ({error})"));
        }
    }
}

impl Drop for LocalServer {
    /// The client is going: stop the server before returning.
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            shut_down(child, self.stdin.take());
        }
    }
}

/// `quit` on the server's console, its pipe closed, then a bounded wait and
/// a kill if it is still there; the child is always reaped.
fn shut_down(mut child: Child, stdin: Option<ChildStdin>) {
    if let Some(mut stdin) = stdin {
        let _ = writeln!(stdin, "quit");
        let _ = stdin.flush();
    }
    let deadline = Instant::now() + QUIT_GRACE;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => {
                crate::log::progress(format_args!("local server stopped ({status})"));
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    crate::log::progress(format_args!(
        "local server killed after {} ms",
        QUIT_GRACE.as_millis()
    ));
}

/// Copy the server's standard output into the log, reporting its
/// `listening on` line and then whether it answers there.
fn copy_output(stdout: std::process::ChildStdout, mut log: File, events: Sender<Event>) {
    let mut announced = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let _ = writeln!(log, "{line}");
        if announced {
            continue;
        }
        if let Some(address) = listening_address(&line) {
            announced = true;
            let _ = events.send(Event::Listening(address));
            // Probed on its own thread: this one must keep draining the pipe,
            // or a chatty server would block on a full one.
            let events = events.clone();
            let _ = std::thread::Builder::new()
                .name("local-server-probe".into())
                .spawn(move || probe(address, &events));
        }
    }
}

/// Ask `address` for its info until it answers or the deadline passes.
fn probe(address: SocketAddr, events: &Sender<Event>) {
    let deadline = Instant::now() + ANSWER_DEADLINE;
    while Instant::now() < deadline {
        if sjk_network::query_server_info(address, Duration::from_millis(300)).is_ok() {
            let _ = events.send(Event::Answering(address));
            return;
        }
        // A refused query fails at once; do not spin.
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = events.send(Event::Silent);
}

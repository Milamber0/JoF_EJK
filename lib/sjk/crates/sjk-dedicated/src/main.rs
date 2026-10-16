//! SJK's headless server process.
//!
//! Platform composition only: a UDP socket, the clocks and the operator's
//! settings. Every protocol decision lives in `sjk-network`'s legacy endpoint and
//! every authoritative one in `sjk-server`; `sjk_dedicated::bridge` is where they meet.
use sjk_dedicated::bridge::{Identity, NativeGame, SpawnOrder};
use sjk_dedicated::command_buffer::{CommandBuffer, CommandPass};
use sjk_dedicated::config_files::ConfigFiles;
use sjk_dedicated::cvars::Cvars;
use sjk_dedicated::frame_clock::FrameClock;
use sjk_dedicated::map;
use sjk_dedicated::master::Masters;
use sjk_network::{LegacyClock, LegacyConsoleSettings, LegacyServerSession, LegacySessionSettings};
use std::{
    error::Error,
    net::{SocketAddr, SocketAddrV4, UdpSocket},
    time::{Duration, Instant},
};

struct Options {
    bind: SocketAddr,
    mapname: String,
    wire_clients: usize,
    peers: usize,
    game_data: Option<std::path::PathBuf>,
    /// `fs_homepath`: where configs are read first and archived variables written.
    home: Option<std::path::PathBuf>,
    spawn_order: SpawnOrder,
    maps: Vec<String>,
    /// Console variables from the options and `--set`, in order, set as `+set` sets them.
    sets: Vec<(String, String)>,
    /// `--cheats`: `sv_cheats`, which only the server itself may set.
    cheats: bool,
    /// The command line's `+` commands, one console line each.
    commands: Vec<String>,
    /// `--quit-on-eof`: the end of standard input quits the server.
    quit_on_eof: bool,
}

const USAGE: &str = "sjk-server [--bind ADDRESS:PORT] [--hostname NAME] [--map NAME]
                     [--game-data DIRECTORY] [--wire-clients 0..32] [--peers N]
                     [--team-auto-join] [--spawn-order random|map|map:N] [--cheats]
                     [--gametype ffa|holocron|jm|duel|powerduel|team|siege|ctf|cty] [--fraglimit N] [--duel-fraglimit N] [--timelimit MINUTES]
                     [--capturelimit N] [--maps NAME,NAME,...]
                     [--allow-vote MASK] [--vote-delay MS] [--rconpassword PASSWORD]
                     [--set NAME VALUE]... [--home DIRECTORY] [--quit-on-eof]
                     [+COMMAND ARGUMENTS...]...

  --game-data     the game's GameData directory; with it the map is loaded from
                  base/*.pk3, without it only its name is advertised
  --maps          the maps played after this one, in order, comma-separated; the list
                  wraps. Without it the server replays its own map when a match ends,
                  which is what a server with no `nextmap` set does
  --fraglimit     points that end a match, 0 for none (the reference's default is 20)
  --duel-fraglimit  duels a player must win to end a tournament, 0 for none (10)
  --timelimit     minutes that end a match, 0 for none; a fraction is allowed, as the
                  reference's cvar is a float
  --capturelimit  captures that end a CTF match, 0 for none
  --wire-clients  protocol-26 clients that can be connected at once
  --peers         players the server itself holds; independent of --wire-clients
  --team-auto-join  g_teamAutoJoin: a newcomer plays at once instead of being sent to
                  the spectators to set its Force powers up
  --spawn-order   where players spawn: `random` (the default, as every JKA server
                  does: one of the further half of the free points, away from the
                  death), `map` (the first free point in map order — reproducible,
                  for practice and testing; no stock server offers it), or `map:N`
                  (the same, starting at the map's Nth deathmatch point, which puts a
                  client beside a particular thing in the map)
  --gametype      `ffa` (the default), `holocron` (Holocron FFA: the map's holocrons
                  carry the Force powers, three at most), `jm` (Jedi Master: one saber on the map,
                  whoever holds it is the master and the only one who scores),
                  `duel` (a tournament: two duel, the rest
                  wait in line, the loser of each round goes to its back),
                  `powerduel` (one lone duellist against a pair, `duelteam` to
                  choose a side), `team`
                  (team deathmatch: kills score the
                  killer's team, teammates are spared, the team overlay is sent),
                  `ctf` (capture the flag: the map's flags, their team spawn points),
                  `cty` (capture the ysalamiri: the same, a flag carrier without
                  the Force and out of its reach) or
                  `siege`. A siege server puts players on the
                  map's own `info_player_siegeteam` points, which is the only way to
                  stand where a stock map keeps its breakables and objectives
  --cheats        sv_cheats: the cheat commands (`give`, `noclip`) work
  --allow-vote    g_allowVote: 0 turns voting off, otherwise a bit per vote in the
                  reference's table (-1, the default, is all of them); votes this
                  server cannot carry out yet are never offered
  --vote-delay    g_voteDelay: milliseconds between a vote passing and it taking
                  effect (3000)
  --set           any console variable, as `+set` sets it (`sv_fps`, `sv_privateClients`,
                  `g_motd`, ...); after the options above, in order. Every option that
                  names a setting is a console variable set the same way
  g_npcNav        (a console variable, this server's own) NPC navigation for maps that
                  ship none, server-side only, so stock clients see it too: 0 (the
                  default) the map's own `.nav` or waypoints, exactly as the reference;
                  1 a graph made from the bots' route file (`botroutes/<map>.wnt`); 2 the
                  route file, else a graph sampled from the map's collision. Either way
                  combat points are placed at cover. Read as a level begins (`--set
                  g_npcNav 2`, or `g_npcNav 2` then `map_restart`)
  --home          the operator's directory: `exec` looks in its `base` first, and the
                  archived variables are kept in `base/sjk_server.cfg` there (read at
                  start, rewritten when one changes). Without it nothing is written
  +COMMAND        a console line, as the reference's command line takes them: `+set`
                  lines before the configs, then `mpdefault.cfg`, `sjk_server.cfg` and
                  `autoexec.cfg`, then every `+` line in order (`+exec server.cfg`,
                  `+map mp/ffa3`, ...)
  --rconpassword  rconpassword: the remote console's password; without one every
                  `rcon` is refused. Lines typed on standard input run at the
                  server's own console either way (`status`, `kick`, `svsay`, `map`, ...)
  --quit-on-eof   the end of standard input runs `quit`: for a program that starts the
                  server as its child (the client's Create game) and holds its input
                  open, so the server never outlives it, however it ends. Without it an
                  ended input only stops the console, as the reference's does";

fn options() -> Result<Options, Box<dyn Error>> {
    let mut options = Options {
        bind: "0.0.0.0:29070".parse()?,
        mapname: "nomap".to_owned(),
        wire_clients: 32,
        peers: 64,
        game_data: None,
        home: None,
        spawn_order: SpawnOrder::Random,
        maps: Vec::new(),
        sets: Vec::new(),
        cheats: false,
        commands: Vec::new(),
        quit_on_eof: false,
    };
    let mut arguments = std::env::args().skip(1).peekable();
    while let Some(flag) = arguments.next() {
        // `Com_ParseCommandLine`: a `+` starts a console line, and every word up to the
        // next option or `+` belongs to it.
        if let Some(command) = flag.strip_prefix('+') {
            let mut line = command.to_owned();
            while let Some(word) =
                arguments.next_if(|word| !word.starts_with('+') && !word.starts_with("--"))
            {
                let quoted = word.contains(char::is_whitespace);
                line.push(' ');
                line.push_str(&if quoted { format!("\"{word}\"") } else { word });
            }
            options.commands.push(line);
            continue;
        }
        if flag == "--team-auto-join" {
            options.sets.push(("g_teamAutoJoin".into(), "1".into()));
            continue;
        }
        if flag == "--cheats" {
            options.cheats = true;
            continue;
        }
        if flag == "--quit-on-eof" {
            options.quit_on_eof = true;
            continue;
        }
        let mut value = || {
            arguments
                .next()
                .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))
        };
        let cvar = match flag.as_str() {
            "--hostname" => Some("sv_hostname"),
            "--fraglimit" => Some("fraglimit"),
            "--duel-fraglimit" => Some("duel_fraglimit"),
            "--timelimit" => Some("timelimit"),
            "--capturelimit" => Some("capturelimit"),
            "--allow-vote" => Some("g_allowVote"),
            "--vote-delay" => Some("g_voteDelay"),
            "--rconpassword" => Some("rconPassword"),
            _ => None,
        };
        if let Some(cvar) = cvar {
            options.sets.push((cvar.to_owned(), value()?));
            continue;
        }
        match flag.as_str() {
            "--bind" => options.bind = value()?.parse()?,
            "--map" => options.mapname = value()?,
            "--wire-clients" => options.wire_clients = value()?.parse()?,
            "--gametype" => {
                let gametype = match value()?.as_str() {
                    "ffa" => sjk_dedicated::bridge::GAMETYPE_FFA,
                    "jm" => sjk_dedicated::bridge::GAMETYPE_JEDIMASTER,
                    "holocron" => sjk_dedicated::bridge::GAMETYPE_HOLOCRON,
                    "duel" => sjk_dedicated::bridge::GAMETYPE_DUEL,
                    "powerduel" => sjk_dedicated::bridge::GAMETYPE_POWERDUEL,
                    "team" => sjk_dedicated::bridge::GAMETYPE_TEAM,
                    "siege" => sjk_dedicated::bridge::GAMETYPE_SIEGE,
                    "ctf" => sjk_dedicated::bridge::GAMETYPE_CTF,
                    "cty" => sjk_dedicated::bridge::GAMETYPE_CTY,
                    other => return Err(format!("--gametype is ffa, holocron, jm, duel, powerduel, team, siege, ctf or cty, not {other}\n{USAGE}").into()),
                };
                options
                    .sets
                    .push(("g_gametype".into(), gametype.to_string()));
            }
            "--spawn-order" => {
                options.spawn_order = match value()?.as_str() {
                    "random" => SpawnOrder::Random,
                    "map" => SpawnOrder::Map,
                    // `map:N` starts the map order at the Nth deathmatch point, which
                    // is how a probe puts a client beside a particular thing.
                    other => match other.strip_prefix("map:").map(str::parse::<usize>) {
                        Some(Ok(index)) => SpawnOrder::MapFrom(index),
                        _ => {
                            return Err(format!(
                                "--spawn-order is random, map or map:N, not {other}\n{USAGE}"
                            )
                            .into());
                        }
                    },
                }
            }
            "--maps" => {
                options.maps = value()?
                    .split(',')
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .collect()
            }
            "--set" => {
                let name = value()?;
                let setting = value()?;
                options.sets.push((name, setting));
            }
            "--peers" => options.peers = value()?.parse()?,
            "--game-data" => options.game_data = Some(value()?.into()),
            "--home" => options.home = Some(value()?.into()),
            _ => return Err(format!("unknown option {flag}\n{USAGE}").into()),
        }
    }
    Ok(options)
}

/// `Com_StartupVariable`: the options' variables and the command line's `+set` lines,
/// set by the user before anything registers them.
fn startup_variables(cvars: &mut Cvars, options: &Options) {
    let quiet = &mut |_: &[u8]| {};
    for (name, value) in &options.sets {
        cvars.user_set(name.as_bytes(), Some(value.as_bytes()), quiet);
    }
    for line in &options.commands {
        let words: Vec<&[u8]> = sjk_network::LegacyTokens::new(line.as_bytes()).collect();
        if words.first() == Some(&&b"set"[..]) && words.len() > 1 {
            cvars.user_set(words[1], Some(&words[2..].join(&b' ')), quiet);
        }
    }
}

/// `Com_ExecuteCfg`: the stock defaults, the archived variables and the operator's
/// `autoexec.cfg`, each run to the end at once. Only the buffer's own commands and the
/// variables exist yet; anything else in them is ignored, as nothing else is registered.
fn startup_configs(cvars: &mut Cvars, files: &mut ConfigFiles) {
    let print = &mut |message: &[u8]| {
        // On stderr: whatever launched the server reads the address from stdout.
        let _ = std::io::Write::write_all(&mut std::io::stderr(), message);
    };
    for config in [
        "mpdefault.cfg",
        sjk_dedicated::config_files::ARCHIVE_FILE,
        "autoexec.cfg",
    ] {
        let mut buffer = CommandBuffer::default();
        buffer.add_text(format!("exec {config}\n").as_bytes(), print);
        let mut pass = CommandPass::default();
        while let Some(line) = buffer.next_line(&mut pass) {
            let words: Vec<&[u8]> = sjk_network::LegacyTokens::new(&line).collect();
            if !buffer.command(&words, cvars, &mut |name| files.read(name), print) {
                cvars.command(&line, print);
            }
        }
    }
}

/// The copyright and licence announcement printed at startup (GPLv2 §2(c));
/// the client prints the same lines (`sjk-viewer`'s `notice`). The version is
/// the build's, from `scripts/build_version.rs`.
const NOTICE: [&str; 2] = [
    concat!(
        "SJK server ",
        env!("SJK_BUILD_VERSION"),
        ", Copyright (C) 2026 Sol-Vulpes, Bishop-R and the JKR contributors"
    ),
    "Free software under the GNU GPL v2, with ABSOLUTELY NO WARRANTY; see LICENSE and CREDITS.md",
];

fn main() -> Result<(), Box<dyn Error>> {
    for line in NOTICE {
        println!("{line}");
    }
    let options = options()?;
    let mut secret = [0; 16];
    getrandom::fill(&mut secret)
        .map_err(|error| format!("no operating-system randomness: {error}"))?;
    let map = match &options.game_data {
        Some(game_data) => Some(map::load(game_data, &options.mapname)?),
        None => None,
    };
    if let Some(map) = &map {
        println!(
            "loaded maps/{}.bsp: {} spawn points",
            options.mapname,
            map.spawn_points.len()
        );
    }
    let mut cvars = Cvars::new();
    startup_variables(&mut cvars, &options);
    let mut files = ConfigFiles::new(options.home.clone(), options.game_data.clone());
    startup_configs(&mut cvars, &mut files);
    startup_variables(&mut cvars, &options);
    cvars.register_server();
    let identity = Identity {
        hostname: cvars.string(b"sv_hostname").to_vec(),
        mapname: options.mapname.clone().into_bytes(),
    };
    let mut game =
        NativeGame::with_cvars(identity, map, options.peers, options.wire_clients, cvars)?;
    if options.cheats {
        game.set_cvar("sv_cheats", "1");
    }
    game.set_rotation(options.maps.clone(), options.game_data.clone());
    let offered = game.offered_votes();
    let names = |mask: i32| {
        let names: Vec<&str> = sjk_game_jka::vote::VOTES
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, vote)| vote.string)
            .collect();
        names.join(" ")
    };
    game.set_spawn_order(options.spawn_order);
    let socket = UdpSocket::bind(options.bind)?;
    socket.set_read_timeout(Some(Duration::from_millis(1)))?;
    let bound = socket.local_addr()?;
    let mut settings = LegacySessionSettings::default();
    settings.console = LegacyConsoleSettings {
        net_ip: bound.ip().to_string().into_bytes(),
        net_port: i32::from(bound.port()),
        ..LegacyConsoleSettings::default()
    };
    game.endpoint_settings(&mut settings);
    let mut generation = game.cvars_generation();
    let mut session = LegacyServerSession::new(game, settings, options.wire_clients, secret, 4096)
        .map_err(|error| {
            format!(
                "protocol 26 has no room for {} wire clients",
                error.requested
            )
        })?;

    println!("listening on {}", socket.local_addr()?);
    // The first level's game log opens now: whatever launched the server reads its
    // address from the first line.
    session.game_mut().set_config_files(files);
    // On stderr: whatever launched the server may read the address from stdout and close it.
    eprintln!(
        "votes offered: {}",
        if offered == 0 {
            "none".to_owned()
        } else {
            names(offered)
        }
    );
    let withheld = session.game().cvars().integer(b"g_allowVote")
        & !offered
        & ((1 << sjk_game_jka::vote::VOTES.len()) - 1);
    if withheld != 0 {
        eprintln!(
            "votes withheld because this server cannot carry them out yet: {}",
            names(withheld)
        );
    }

    // `SV_Init`: the whitelist and the saved bans, then (`Com_AddStartupCommands`) every `+` line, all run
    // by the first frame.
    session.load_whitelist();
    session
        .game_mut()
        .queue_console(b"sv_rehashbans\n", &mut |_| {});
    for line in &options.commands {
        session
            .game_mut()
            .queue_console(format!("{line}\n").as_bytes(), &mut |_| {});
    }
    let console = console_lines(options.quit_on_eof);
    let (started, mut frame_clock, mut datagram) =
        (Instant::now(), FrameClock::default(), [0; 65_535]);
    let mut masters = Masters::default();
    loop {
        let received = socket.recv_from(&mut datagram);
        let elapsed = started.elapsed().as_millis() as i32;
        frame_clock.observe(elapsed);
        let clock = LegacyClock {
            server_time: frame_clock.server_time(),
            wall_time: elapsed,
        };
        let mut send = |to: SocketAddrV4, bytes: &[u8]| {
            // A failed send is a lost datagram; the protocol already tolerates those.
            let _ = socket.send_to(bytes, to);
        };
        if let Ok((length, SocketAddr::V4(from))) = received {
            session.handle_datagram(from, &datagram[..length], clock, &mut send);
        }
        if session.quit_due() {
            // `SV_Shutdown("Server quit\n")`: the clients told, the masters told twice,
            // the game shut down; then the process ends.
            session.final_message(b"Server quit\n", clock, &mut send);
            masters.shutdown(&mut session, clock, &socket);
            session.game_mut().shut_down();
            return Ok(());
        }
        if session.kill_due() {
            // `SV_Shutdown("killserver")`: the same, but the process stays, answering
            // nothing, until a `map` starts a server again.
            session.final_message(b"killserver", clock, &mut send);
            masters.shutdown(&mut session, clock, &socket);
            session.game_mut().stop();
            session.stop();
        }
        let print = &mut |message: &[u8]| {
            let _ = std::io::Write::write_all(&mut std::io::stdout(), message);
        };
        // A typed line joins the command buffer (`Sys_ConsoleInput`), `;` and all.
        while let Ok(line) = console.try_recv() {
            session
                .game_mut()
                .queue_console(format!("{line}\n").as_bytes(), print);
        }
        // What the console changed that the endpoint keeps (`rconPassword`, timeouts, rates).
        if session.game().cvars_generation() != generation {
            generation = session.game().cvars_generation();
            let mut settings = session.settings_mut().clone();
            session.game().endpoint_settings(&mut settings);
            *session.settings_mut() = settings;
        }
        if frame_clock.ready(session.game_mut().frame_msec()) {
            // `Com_Frame`: the command buffer runs, then the archive is written if a
            // variable in it changed, then the server's frame.
            let mut pass = CommandPass::default();
            while let Some(line) = session.game_mut().next_console_line(&mut pass) {
                session.console(&line, clock, print);
                // `Com_Quit_f` does not return: nothing after it runs, and neither does
                // anything after `killserver` before the server stops.
                if session.quit_due() || session.kill_due() {
                    break;
                }
            }
            if session.quit_due() || session.kill_due() || !session.running() {
                // Do not replay time spent with the server stopped when a map starts.
                frame_clock.discard_pending();
                continue;
            }
            session.game_mut().write_config(print);
            // Console commands may have changed sv_fps since the readiness check.
            let Some(server_time) = frame_clock.advance(session.game_mut().frame_msec()) else {
                continue;
            };
            let clock = LegacyClock {
                server_time,
                wall_time: elapsed,
            };
            session.calc_pings();
            session.game_mut().run_frame(clock.server_time);
            session.frame(clock, &mut send);
            // "send a heartbeat to the master if needed".
            masters.frame(&mut session, clock, &socket);
        }
    }
}

/// The server's own console: every line typed on standard input, handed to the frame
/// loop as it arrives. The reader ends with its input; with `quit_on_eof` its last line
/// is `quit`, so a server whose launcher has gone (the pipe closed) goes too.
fn console_lines(quit_on_eof: bool) -> std::sync::mpsc::Receiver<String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                return;
            }
        }
        if quit_on_eof {
            let _ = sender.send("quit".to_owned());
        }
    });
    receiver
}

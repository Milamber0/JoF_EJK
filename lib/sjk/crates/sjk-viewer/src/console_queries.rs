//! Worker-side queries share sjk-network's browser OOB transport and parsers.
use super::super::*;
use sjk_network::query::{Answer, Request};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value) in [
        ("sv_master1", "masterjk3.ravensoft.com"),
        ("sv_master2", "master.jkhub.org"),
        ("sv_master3", ""),
        ("sv_master4", ""),
        ("sv_master5", ""),
    ] {
        cvars.register(CvarDefinition::new(
            name,
            value,
            CvarFlags::ARCHIVE,
            "Master server",
        ))?;
    }
    for (name, help) in [
        ("rconPassword", "Remote console password (never archived)"),
        (
            "rconAddress",
            "Remote console destination when disconnected",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, "", CvarFlags::NONE, help))?;
    }
    cvars.register(CvarDefinition::new(
        "cl_serverStatusResendTime",
        750_i64,
        CvarFlags::NONE,
        "Status retry interval in ms",
    ))?;
    cvars.register(CvarDefinition::new(
        "cl_maxPing",
        800_i64,
        CvarFlags::ARCHIVE,
        "Maximum query wait in ms",
    ))?;
    cvars.register(CvarDefinition::new(
        "cl_timeout",
        200.0,
        CvarFlags::NONE,
        "Disconnect after this many seconds without server packets",
    ))?;
    Ok(())
}

pub(super) fn address(value: &str, default_port: u16) -> Result<SocketAddr, String> {
    if let Ok(address) = value.parse() {
        return Ok(address);
    }
    if let Ok(ip) = value.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, default_port));
    }
    let value = if value.contains(':') {
        value.to_owned()
    } else {
        format!("{value}:{default_port}")
    };
    value
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("Address did not resolve".into())
}

pub(super) fn start(
    console: &mut ViewerConsole,
    name: &str,
    args: &[String],
    server: Option<SocketAddr>,
) -> Result<(), String> {
    if console.client_commands.jobs.len() >= 8 {
        return Err("Too many active queries".into());
    }
    let timeout = Duration::from_millis(
        console
            .integer_cvar("cl_maxping")
            .unwrap_or(800)
            .clamp(1, 60_000) as u64,
    );
    let resend = Duration::from_millis(
        console
            .integer_cvar("cl_serverstatusresendtime")
            .unwrap_or(750)
            .clamp(1, 60_000) as u64,
    );
    let mut work = Vec::new();
    let current = server.map(|value| value.to_string());
    match name {
        "ping" => {
            let [address] = args else {
                return Err("usage: ping <address>".into());
            };
            work.push((address.clone(), 29070, Request::Ping, timeout));
        }
        "serverstatus" => {
            let address = match args {
                [] => current.ok_or("Not connected to a server.")?,
                [address] => address.clone(),
                _ => return Err("usage: serverstatus [address]".into()),
            };
            work.push((address, 29070, Request::Status, resend.saturating_mul(4)));
        }
        "rcon" => {
            let password = console.text_value("rconPassword").unwrap_or("").to_owned();
            if password.is_empty() {
                return Err("You must set 'rconpassword' before issuing an rcon command.".into());
            }
            let address = current
                .or_else(|| {
                    console
                        .text_value("rconAddress")
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                })
                .ok_or("You must either be connected, or set the 'rconAddress' cvar")?;
            work.push((
                address,
                29070,
                Request::Rcon {
                    password,
                    command: args.join(" "),
                },
                Duration::from_secs(3),
            ));
        }
        "globalservers" => {
            let [master, protocol, keywords @ ..] = args else {
                return Err("usage: globalservers <master 0-5> <protocol> [keywords]".into());
            };
            let master: usize = master.parse().map_err(|_| "Invalid master number")?;
            let protocol: u32 = protocol.parse().map_err(|_| "Invalid protocol")?;
            if master > 5 {
                return Err("Invalid master number".into());
            }
            for index in 1..=5 {
                if master != 0 && index != master {
                    continue;
                }
                let name = format!("sv_master{index}");
                if let Some(address) = console.text_value(&name).filter(|v| !v.is_empty()) {
                    work.push((
                        address.to_owned(),
                        29060,
                        Request::Master {
                            protocol,
                            keywords: keywords.join(" "),
                        },
                        timeout,
                    ));
                }
            }
            if work.is_empty() {
                return Err("No master server address configured".into());
            }
        }
        "localservers" => work.push(("255.255.255.255:29070".into(), 29070, Request::Lan, timeout)),
        "showip" => {}
        _ => return Err("Unknown query command".into()),
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let show_ip = name == "showip";
    std::thread::Builder::new()
        .name("sjk-console-query".into())
        .spawn(move || {
            let mut lines = Vec::new();
            if show_ip {
                match crate::platform::interface_addresses() {
                    Ok(value) => lines.extend(value.lines().map(str::to_owned)),
                    Err(error) => lines.push(format!("showip: {error}")),
                }
            }
            for (host, port, request, timeout) in work {
                let result = address(&host, port).and_then(|address| {
                    sjk_network::query::run(address, request, timeout, resend)
                        .map_err(|e| e.to_string())
                });
                match result {
                    Ok(responses) => {
                        for response in responses {
                            lines.extend(format_response(response));
                        }
                    }
                    Err(error) => lines.push(format!("{host}: {error}")),
                }
            }
            let _ = sender.send(lines);
        })
        .map_err(|e| e.to_string())?;
    console.client_commands.jobs.push(receiver);
    Ok(())
}

fn format_response(response: sjk_network::query::Response) -> Vec<String> {
    let mut lines = vec![format!(
        "{}: {} ms",
        response.source,
        response.elapsed.as_millis()
    )];
    match response.answer {
        Answer::Info(info) => lines.extend(info.iter().map(|(k, v)| format!("{k}: {v}"))),
        Answer::Status(status) => {
            lines.extend(status.info.iter().map(|(k, v)| format!("{k}: {v}")));
            lines.extend(
                status
                    .players
                    .iter()
                    .map(|p| format!("{} {} \"{}\"", p.score, p.ping, p.name)),
            );
        }
        Answer::Print(text) => lines.extend(text.lines().map(str::to_owned)),
        Answer::Servers(servers) => lines.extend(servers.iter().map(ToString::to_string)),
    }
    lines
}

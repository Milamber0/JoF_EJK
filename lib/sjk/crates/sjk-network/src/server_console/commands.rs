//! The roster commands of `sv_ccmds.cpp`, message for message.
use super::{Arguments, LegacyConsoleHost, expand_newlines, info_print, strip_color};
use crate::{LegacyClientPhase, LegacyPeerAddress};

/// `SV_DropClient`'s reason for every kick (`SV_GetStringEdString("MP_SVGAME","WAS_KICKED")`).
const WAS_KICKED: &[u8] = b"@@@WAS_KICKED";
/// `MAX_SAY_TEXT`.
const SAY_TEXT: usize = 150;
/// `sv_ccmds.cpp:1226, 1257`.
const SAY_PREFIX: &[u8] = b"Server^7\x19: ";
const TELL_PREFIX: &[u8] = b"\x19[Server^7\x19]\x19: ";
/// `STATUS_OS` for the platform the server was built for.
const STATUS_OS: &str = if cfg!(windows) {
    "Windows"
} else if cfg!(target_os = "linux") {
    "Linux"
} else if cfg!(target_os = "macos") {
    "OSX"
} else {
    "Unknown"
};

fn occupied(host: &impl LegacyConsoleHost, client: usize) -> bool {
    host.phase(client) != LegacyClientPhase::Free
}

/// `atoi` of a string of digits, as glibc's `strtol` saturates and `int` truncates it.
fn atoi_digits(digits: &[u8]) -> i32 {
    let value = digits.iter().fold(0_i64, |value, &digit| {
        value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'))
    });
    value as i32
}

/// `cleanName`: the name cut to 63 bytes, colours stripped.
fn clean_name(name: &[u8]) -> Vec<u8> {
    let mut clean = name[..name.len().min(63)].to_vec();
    strip_color(&mut clean);
    clean
}

/// `SV_GetPlayerByHandle`: a slot number (any run of digits, even none) that is
/// occupied, else a name, as held or with its colours stripped.
fn player_by_handle(
    host: &impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) -> Option<usize> {
    if arguments.count() < 2 {
        print(b"No player specified.\n");
        return None;
    }
    let handle = arguments.get(1);
    if handle.iter().all(u8::is_ascii_digit) {
        let number = atoi_digits(handle);
        if let Ok(client) = usize::try_from(number)
            && client < host.client_count()
            && occupied(host, client)
        {
            return Some(client);
        }
    }
    let found = (0..host.client_count())
        .filter(|&client| occupied(host, client))
        .find(|&client| {
            host.name(client).eq_ignore_ascii_case(handle)
                || clean_name(host.name(client)).eq_ignore_ascii_case(handle)
        });
    if found.is_none() {
        print(&[&b"Player "[..], handle, b" is not on the server\n"].concat());
    }
    found
}

/// `SV_GetPlayerByNum`: an occupied slot, by number only.
pub(super) fn player_by_number(
    host: &impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) -> Option<usize> {
    if arguments.count() < 2 {
        print(b"No player specified.\n");
        return None;
    }
    let handle = arguments.get(1);
    if !handle.iter().all(u8::is_ascii_digit) {
        print(&[&b"Bad slot number: "[..], handle, b"\n"].concat());
        return None;
    }
    let number = atoi_digits(handle);
    let Some(client) = usize::try_from(number)
        .ok()
        .filter(|&client| client < host.client_count())
    else {
        print(format!("Bad client slot: {number}\n").as_bytes());
        return None;
    };
    if !occupied(host, client) {
        print(format!("Client {number} is not active\n").as_bytes());
        return None;
    }
    Some(client)
}

/// Kick every occupied slot `which` picks, in slot order (`SV_KickBots_f`,
/// `SV_KickAll_f`, and `kick all`/`kick allbots`).
pub(super) fn kick_where<H: LegacyConsoleHost>(host: &mut H, which: impl Fn(&H, usize) -> bool) {
    for client in 0..host.client_count() {
        if occupied(host, client) && which(host, client) {
            host.kick(client, WAS_KICKED);
        }
    }
}

/// `SV_Kick_f`.
pub(super) fn kick(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if arguments.count() != 2 {
        print(
            b"Usage: kick <player name>\nkick all = kick everyone\nkick allbots = kick all bots\n",
        );
        return;
    }
    let handle = arguments.get(1);
    // `SV_KickBlankPlayers`: kicking the default name kicks the nameless too.
    if handle.eq_ignore_ascii_case(b"Padawan") {
        kick_where(host, |host, client| {
            host.address(client) != LegacyPeerAddress::Loopback
                && clean_name(host.name(client)).is_empty()
        });
    }
    let Some(client) = player_by_handle(host, arguments, print) else {
        if handle.eq_ignore_ascii_case(b"all") {
            kick_where(host, |host, client| {
                host.address(client) != LegacyPeerAddress::Loopback
            });
        } else if handle.eq_ignore_ascii_case(b"allbots") {
            kick_where(host, |host, client| {
                host.address(client) == LegacyPeerAddress::Bot
            });
        }
        return;
    };
    kick_one(host, client, print);
}

fn kick_one(host: &mut impl LegacyConsoleHost, client: usize, print: &mut dyn FnMut(&[u8])) {
    if host.address(client) == LegacyPeerAddress::Loopback {
        print(b"Cannot kick host player\n");
        return;
    }
    host.kick(client, WAS_KICKED);
}

/// `SV_KickNum_f`, also `clientkick`.
pub(super) fn kick_number(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if arguments.count() != 2 {
        print(&[&b"Usage: "[..], arguments.get(0), b" <client number>\n"].concat());
        return;
    }
    if let Some(client) = player_by_number(host, arguments, print) {
        kick_one(host, client, print);
    }
}

/// `SV_ConSay_f`.
pub(super) fn say(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if host.status().dedicated == 0 {
        print(b"Server is not dedicated.\n");
        return;
    }
    if arguments.count() < 2 {
        return;
    }
    let text = arguments.from(1, SAY_TEXT);
    let mut message = [&b"broadcast: chat \""[..], SAY_PREFIX].concat();
    expand_newlines(&text, &mut message);
    message.extend_from_slice(b"\\n\"\n");
    print(&message);
    host.server_command(
        None,
        &[&b"chat \""[..], SAY_PREFIX, &text, b"\"\n"].concat(),
    );
}

/// `SV_ConTell_f`.
pub(super) fn tell(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if host.status().dedicated == 0 {
        print(b"Server is not dedicated.\n");
        return;
    }
    if arguments.count() < 3 {
        print(b"Usage: svtell <client number> <text>\n");
        return;
    }
    let Some(client) = player_by_number(host, arguments, print) else {
        return;
    };
    let text = arguments.from(2, SAY_TEXT);
    let mut message = [&b"tell: svtell to "[..], host.name(client), b"^7: "].concat();
    expand_newlines(&text, &mut message);
    message.push(b'\n');
    print(&message);
    host.server_command(
        Some(client),
        &[&b"chat \""[..], TELL_PREFIX, b"^6", &text, b"^7\"\n"].concat(),
    );
}

/// `SV_DumpUser_f`.
pub(super) fn dump_user(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    if arguments.count() != 2 {
        print(b"Usage: dumpuser <userid>\n");
        return;
    }
    let Some(client) = player_by_handle(host, arguments, print) else {
        return;
    };
    print(b"userinfo\n");
    print(b"--------\n");
    info_print(host.userinfo(client), print);
}

/// `SV_CalcUptime`.
fn uptime(seconds: i64) -> String {
    let (minutes, hours, days) = (seconds / 60, seconds / 3600, seconds / 86_400);
    let clock = format!("{}h{}m{}s", hours % 24, minutes % 60, seconds % 60);
    if days > 0 {
        format!("{days} days {clock}")
    } else {
        clock
    }
}

/// `printf("%-15.15s")` or `%39s` on bytes.
fn padded(out: &mut Vec<u8>, text: &[u8], width: usize, left: bool, cut: bool) {
    let text = if cut {
        &text[..text.len().min(width)]
    } else {
        text
    };
    let pad = width.saturating_sub(text.len());
    if !left {
        out.resize(out.len() + pad, b' ');
    }
    out.extend_from_slice(text);
    if left {
        out.resize(out.len() + pad, b' ');
    }
}

/// `SV_Status_f`.
pub(super) fn status(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let whole_names = arguments.count() > 1 && arguments.get(1).eq_ignore_ascii_case(b"notrunc");
    let (mut humans, mut bots) = (0, 0);
    for client in 0..host.client_count() {
        if host.phase(client) >= LegacyClientPhase::Connected {
            if host.address(client) == LegacyPeerAddress::Bot {
                bots += 1
            } else {
                humans += 1
            }
        }
    }
    let status = host.status();
    let mut hostname = status.hostname[..status.hostname.len().min(255)].to_vec();
    strip_color(&mut hostname);
    let dedicated = ["listen", "lan dedicated", "public dedicated"]
        .get(status.dedicated as usize)
        .copied()
        .unwrap_or_default();
    print(&[&b"hostname: "[..], &hostname, b"^7\n"].concat());
    print(b"version : 1.0.1.0 26\n");
    print(&[&b"game    : "[..], status.game_directory, b"\n"].concat());
    let address = [&b"udp/ip  : "[..], status.net_ip].concat();
    print(
        &[
            &address[..],
            format!(":{} os({STATUS_OS}) type({dedicated})\n", status.net_port).as_bytes(),
        ]
        .concat(),
    );
    print(
        &[
            &b"map     : "[..],
            status.mapname,
            format!(" gametype({})\n", status.gametype).as_bytes(),
        ]
        .concat(),
    );
    let open = host.client_count() as i32 - status.private_clients;
    print(format!("players : {humans} humans, {bots} bots ({open} max)\n").as_bytes());
    print(format!("uptime  : {}\n", uptime(status.uptime_seconds)).as_bytes());
    print(b"cl score ping name            address                                 rate \n");
    print(b"-- ----- ---- --------------- --------------------------------------- -----\n");
    for client in 0..host.client_count() {
        let phase = host.phase(client);
        if phase == LegacyClientPhase::Free {
            continue;
        }
        let state = match phase {
            LegacyClientPhase::Connected => "CON ".to_owned(),
            LegacyClientPhase::Zombie => "ZMB ".to_owned(),
            _ => format!("{:4}", host.ping(client).min(9999)),
        };
        let mut line = format!("{client:2} {:5} {state} ", host.score(client)).into_bytes();
        padded(
            &mut line,
            host.name(client),
            if whole_names { 0 } else { 15 },
            true,
            !whole_names,
        );
        line.extend_from_slice(b" ^7");
        let address = match host.address(client) {
            LegacyPeerAddress::Loopback => "loopback".to_owned(),
            LegacyPeerAddress::Bot => "bot".to_owned(),
            LegacyPeerAddress::Ip(address) => address.to_string(),
        };
        padded(&mut line, address.as_bytes(), 39, false, false);
        line.extend_from_slice(format!(" {:5}\n", host.rate(client)).as_bytes());
        print(&line);
    }
    print(b"\n");
}

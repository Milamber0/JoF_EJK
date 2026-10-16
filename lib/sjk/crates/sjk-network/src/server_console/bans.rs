//! The ban commands of `sv_ccmds.cpp:575-1080`, over the endpoint's
//! [`crate::LegacyBanList`].
use super::{Arguments, LegacyConsoleHost, commands::player_by_number};
use crate::{LegacyBanAddress, LegacyPeerAddress, legacy_parse_cidr, server_bans::atoi_text};

/// `SV_RehashBans_f`: the list read again from the ban file, if there is one.
pub(super) fn rehash(host: &mut impl LegacyConsoleHost) {
    let text = host.ban_file();
    let mut list = std::mem::take(host.bans());
    match text {
        Some(text) => list.read_file(&text, &mut |name| host.resolve(name)),
        None => list.clear(),
    }
    *host.bans() = list;
}

/// `SV_WriteBans`.
fn save(host: &mut impl LegacyConsoleHost) {
    let text = host.bans().to_file();
    host.save_ban_file(&text);
}

/// `SV_AddBanToList`: `sv_banaddr` and `sv_exceptaddr`, by address or client number.
pub(super) fn add(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    exception: bool,
    print: &mut dyn FnMut(&[u8]),
) {
    let count = arguments.count();
    if !(2..=3).contains(&count) {
        print(
            &[
                &b"Usage: "[..],
                arguments.get(0),
                b" (ip[/subnet] | clientnum [subnet])\n",
            ]
            .concat(),
        );
        return;
    }
    if host.bans().is_full() {
        print(b"Error: Maximum number of bans/exceptions exceeded.\n");
        return;
    }
    let text = String::from_utf8_lossy(arguments.get(1)).into_owned();
    let (address, mask) = if text.contains('.') {
        match legacy_parse_cidr(&text, &mut |name| host.resolve(name)) {
            Some(parsed) => parsed,
            None => {
                print(format!("Error: Invalid address {text}\n").as_bytes());
                return;
            }
        }
    } else {
        let Some(client) = player_by_number(host, arguments, print) else {
            print(format!("Error: Playernum {text} does not exist.\n").as_bytes());
            return;
        };
        let address = match host.address(client) {
            LegacyPeerAddress::Ip(address) => LegacyBanAddress::Ip(address),
            LegacyPeerAddress::Loopback => LegacyBanAddress::Loopback,
            LegacyPeerAddress::Bot => LegacyBanAddress::Bad,
        };
        let mask = match (count, address) {
            (3, LegacyBanAddress::Ip(_)) => {
                let mask = atoi_text(arguments.get(2));
                if (1..=32).contains(&mask) { mask } else { 32 }
            }
            _ => 32,
        };
        (address, mask)
    };
    let LegacyBanAddress::Ip(ip) = address else {
        print(b"Error: Can ban players connected via the internet only.\n");
        return;
    };
    if let Err(message) = host.bans().add(ip, mask, exception) {
        print(message.as_bytes());
        return;
    }
    save(host);
    print(
        format!(
            "Added {}: {}/{mask}\n",
            if exception { "ban exception" } else { "ban" },
            address.to_text()
        )
        .as_bytes(),
    );
}

/// `SV_DelBanFromList`: `sv_bandel` and `sv_exceptdel`, by address or number.
pub(super) fn remove(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    exception: bool,
    print: &mut dyn FnMut(&[u8]),
) {
    if arguments.count() != 2 {
        print(&[&b"Usage: "[..], arguments.get(0), b" (ip[/subnet] | num)\n"].concat());
        return;
    }
    let text = String::from_utf8_lossy(arguments.get(1)).into_owned();
    if text.contains('.') || text.contains(':') {
        let Some((address, mask)) = legacy_parse_cidr(&text, &mut |name| host.resolve(name)) else {
            print(format!("Error: Invalid address {text}\n").as_bytes());
            return;
        };
        host.bans().remove_matching(address, mask, exception, print);
    } else {
        let number = atoi_text(arguments.get(1));
        if number < 1 || number as usize > host.bans().len() {
            print(b"Error: Invalid ban number given\n");
            return;
        }
        host.bans().remove_numbered(number, exception, print);
    }
    save(host);
}

/// `SV_FlushBans_f`.
pub(super) fn flush(host: &mut impl LegacyConsoleHost, print: &mut dyn FnMut(&[u8])) {
    host.bans().clear();
    save(host);
    print(b"All bans and exceptions have been deleted.\n");
}

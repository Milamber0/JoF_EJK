//! Master discovery and `getinfo` row construction for the server browser.

use super::{INFO_TIMEOUT, MASTER_TIMEOUT, MAX_BROWSER_SERVERS, ServerEntry};
use sjk_client::CompatProfile;
use sjk_network::{query_master, query_server_infos};
use sjk_protocol::InfoString;
use std::net::SocketAddr;

/// Ask every master in `masters` (addresses apart by spaces, as
/// [`crate::console::ViewerConsole::master_server`] joins `cl_master` and `cl_master2`)
/// for its list at once, and every server on any of them, and every favourite
/// (`favourites`, which no master need list: one added by its address), for its info,
/// handing each row to `found` as it answers so the browser fills in while slow and
/// dead servers are still timing out. A server some masters do not know still shows
/// (Sol's report, 11/10/2026: servers were missing with one master); a master that
/// fails is passed over while another answers. When every master fails the favourites
/// are still asked, and the masters' error is returned after them.
pub(super) fn discover(
    masters: &str,
    favourites: &[SocketAddr],
    found: impl FnMut(ServerEntry),
) -> Result<(), Box<dyn std::error::Error>> {
    let masters = master_list(masters);
    let answers = std::thread::scope(|scope| {
        let asking: Vec<_> = masters
            .iter()
            .map(|master| scope.spawn(move || query_master(*master, MASTER_TIMEOUT)))
            .collect();
        asking
            .into_iter()
            .map(|asked| {
                asked
                    .join()
                    .unwrap_or(Err(sjk_network::NetworkError::NoResolvedAddress))
            })
            .collect::<Vec<_>>()
    });
    let (mut addresses, failure) = match merged(answers) {
        Ok(addresses) => (addresses, None),
        Err(error) if !favourites.is_empty() => (Vec::new(), Some(error)),
        Err(error) => return Err(error.into()),
    };
    addresses.truncate(MAX_BROWSER_SERVERS);
    with_favourites(&mut addresses, favourites);
    if addresses.is_empty() {
        return Ok(());
    }
    let mut found = found;
    query_server_infos(&addresses, INFO_TIMEOUT, |address, info, round_trip| {
        let ping_millis = round_trip.as_millis().min(u128::from(u32::MAX)) as u32;
        found(entry_from_info(address, &info, ping_millis));
    })?;
    match failure {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

/// Add the favourites the master did not list to `addresses`, each once.
fn with_favourites(addresses: &mut Vec<SocketAddr>, favourites: &[SocketAddr]) {
    for favourite in favourites {
        if !addresses.contains(favourite) {
            addresses.push(*favourite);
        }
    }
}

pub(crate) fn entry_from_info(
    address: SocketAddr,
    info: &InfoString,
    ping_millis: u32,
) -> ServerEntry {
    let name = info.get("hostname").unwrap_or("Unnamed server").to_owned();
    let map = info.get("mapname").unwrap_or("?").to_owned();
    let players = info
        .get("clients")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .min(u32::from(u16::MAX)) as u16;
    let capacity = info
        .get("sv_maxclients")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .min(u32::from(u16::MAX)) as u16;
    let gametype_number = info
        .get_i32("g_gametype")
        .or_else(|| info.get_i32("gametype"));
    let gametype = gametype_name(gametype_number).to_owned();
    let profile = CompatProfile::from_server_info(info);
    let password = info.get_i32("needpass").is_some_and(|value| value != 0);
    let display = format!(
        "{name:<30.30} {map:<18.18} {players:>2}/{capacity:<2} {ping_millis:>4}ms {gametype}{} {}",
        if password { " LOCKED" } else { "" },
        profile_name(&profile)
    );
    ServerEntry {
        address,
        name,
        map,
        players,
        capacity,
        ping_millis,
        gametype,
        profile,
        password,
        display,
        mode: gametype_number,
        bots: bot_count(info, players),
        valid_info: super::filters::valid_info(info),
    }
}

/// The masters named in `list`, apart by spaces or commas, each once, in order.
fn master_list(list: &str) -> Vec<&str> {
    let mut masters: Vec<&str> = Vec::new();
    for master in list.split(|c: char| c.is_whitespace() || c == ',') {
        if !master.is_empty() && !masters.contains(&master) {
            masters.push(master);
        }
    }
    masters
}

/// Every address the masters answered with, each once, in order: an error only when no
/// master answered (the first master's, which says why best).
fn merged<E>(answers: Vec<Result<Vec<SocketAddr>, E>>) -> Result<Vec<SocketAddr>, E> {
    let mut addresses = Vec::new();
    let mut first_error = None;
    let mut answered = false;
    for answer in answers {
        match answer {
            Ok(list) => {
                answered = true;
                for address in list {
                    if !addresses.contains(&address) {
                        addresses.push(address);
                    }
                }
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) if !answered => Err(error),
        _ => Ok(addresses),
    }
}

/// Bots among `players`, for stock's "real players" browser test.
///
/// Stock's UI reads `filterBots` (`ui_main.c:9162`), but that key exists only inside the
/// info string the client builds for its own menu (`cl_lan.cpp:307`) — no server puts it
/// on the wire. What a `getinfo` reply does carry is `g_humanplayers`
/// (`sv_main.cpp` `SVC_Info`), so derive the count from it and fall back to the `bots` key
/// `CL_SetServerInfo` reads for mods that send one.
fn bot_count(info: &InfoString, players: u16) -> i32 {
    info.get_i32("g_humanplayers")
        .map(|humans| i32::from(players).saturating_sub(humans).max(0))
        .or_else(|| info.get_i32("bots"))
        .or_else(|| info.get_i32("filterBots"))
        .unwrap_or(0)
}

fn profile_name(profile: &CompatProfile) -> &'static str {
    match profile {
        CompatProfile::BaseJka => "BASE",
        CompatProfile::JaPlus { .. } => "JA+",
        CompatProfile::TaystJk => "TAYST",
        CompatProfile::Unknown(_) => "MOD",
    }
}

pub(crate) fn gametype_name(value: Option<i32>) -> &'static str {
    match value {
        Some(0) => "FFA",
        Some(1) => "Holocron",
        Some(2) => "Jedi Master",
        Some(3) => "Duel",
        Some(4) => "Power Duel",
        Some(5) => "Single Player",
        Some(6) => "Team FFA",
        Some(7) => "Siege",
        Some(8) => "CTF",
        Some(9) => "CTY",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod master_tests {
    use super::{master_list, merged};
    use std::net::SocketAddr;

    #[test]
    fn the_master_list_splits_on_spaces_and_commas_once_each() {
        assert_eq!(
            master_list(" master.jkhub.org:29060  master.ouned.de:29060,master.jkhub.org:29060 "),
            ["master.jkhub.org:29060", "master.ouned.de:29060"]
        );
        assert!(master_list("  ").is_empty());
    }

    #[test]
    fn the_masters_lists_merge_and_one_failing_master_is_passed_over() {
        let a: SocketAddr = "192.0.2.1:29070".parse().unwrap();
        let b: SocketAddr = "192.0.2.2:29070".parse().unwrap();
        let c: SocketAddr = "192.0.2.3:29071".parse().unwrap();
        assert_eq!(
            merged::<&str>(vec![Ok(vec![a, b]), Ok(vec![b, c])]),
            Ok(vec![a, b, c])
        );
        assert_eq!(merged(vec![Err("down"), Ok(vec![c])]), Ok(vec![c]));
        assert_eq!(
            merged::<&str>(vec![Err("first"), Err("second")]),
            Err("first")
        );
        assert_eq!(merged::<&str>(Vec::new()), Ok(Vec::new()));
    }
}

#[cfg(test)]
mod favourite_tests {
    use super::with_favourites;
    use std::net::SocketAddr;

    /// A favourite added by its address is asked even when no master lists it, once.
    #[test]
    fn favourites_no_master_lists_are_asked_too() {
        let listed: SocketAddr = "192.0.2.1:29070".parse().unwrap();
        let private: SocketAddr = "198.51.100.7:29071".parse().unwrap();
        let mut addresses = vec![listed];
        with_favourites(&mut addresses, &[listed, private]);
        assert_eq!(addresses, [listed, private]);
        let mut none = Vec::new();
        with_favourites(&mut none, &[private]);
        assert_eq!(none, [private]);
    }
}

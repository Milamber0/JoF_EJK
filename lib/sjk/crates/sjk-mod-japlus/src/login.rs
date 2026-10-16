//! `japlus.autologin`: EternalJK's `autoLogin` (`cg_consolecmds.c:1088-1140`). Up
//! to three servers each keep an admin password; the command sends `amlogin` with
//! the one saved for the server the client is on. Like EternalJK it runs only when
//! asked (bind it or put it in an `exec`), and the passwords are saved as plain
//! text in the config.

use sjk_mod::Host;
use std::net::{SocketAddr, ToSocketAddrs};

/// How many servers can keep a password.
pub(crate) const SLOTS: usize = 3;

/// The `japlus.loginServer<n>` and `japlus.loginPass<n>` names of `slot` (1 based).
pub(crate) fn names(slot: usize) -> (String, String) {
    (
        format!("japlus.loginServer{slot}"),
        format!("japlus.loginPass{slot}"),
    )
}

/// Whether the saved `entry` names `current`, `ip:port`. A missing port is
/// 29070; a host name is looked up (only here, when the command runs).
fn matches(entry: &str, current: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty() || entry == "0" {
        return false;
    }
    let entry = if entry.contains(':') {
        entry.to_owned()
    } else {
        format!("{entry}:29070")
    };
    if entry.eq_ignore_ascii_case(current) {
        return true;
    }
    let Ok(current) = current.parse::<SocketAddr>() else {
        return false;
    };
    match entry.parse::<SocketAddr>() {
        Ok(address) => address == current,
        Err(_) => entry
            .to_socket_addrs()
            .is_ok_and(|mut addresses| addresses.any(|address| address == current)),
    }
}

pub(crate) fn auto_login(host: &mut dyn Host) -> Result<Vec<String>, String> {
    let server = host.server().ok_or("Not connected to a server.")?;
    if host.me().is_some_and(|me| me.intermission) {
        return Err("japlus.autologin: not during the intermission".into());
    }
    for slot in 1..=SLOTS {
        let (address, password) = names(slot);
        if host
            .cvar(&address)
            .is_some_and(|entry| matches(&entry, &server.address))
        {
            let password = host.cvar(&password).unwrap_or_default();
            host.send(&format!("amlogin {password}"))?;
            return Ok(Vec::new());
        }
    }
    Ok(vec![format!(
        "^3No admin password is saved for {} (japlus.loginServer1-{SLOTS})",
        server.address
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_match_with_the_default_port() {
        assert!(matches("127.0.0.1", "127.0.0.1:29070"));
        assert!(matches(" 127.0.0.1:29071 ", "127.0.0.1:29071"));
        assert!(!matches("127.0.0.1", "127.0.0.1:29071"));
        assert!(!matches("0", "0:29070"));
        assert!(!matches("", "127.0.0.1:29070"));
    }
}

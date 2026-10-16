//! `japlus.serverconfig` and `japlus.plugin`: the JA+ client plugin's
//! `serverconfig` and `pluginDisable`.
//!
//! On JA+ the plugin answers `serverconfig` itself from the server's `jp_cinfo`
//! (EternalJK `codemp/cgame/cg_consolecmds.c:505-530`); jaPRO answers it on the
//! server, so it is forwarded there. `japlus.plugin` views and toggles the bits of
//! `cp_pluginDisable`, the userinfo a JA+ server reads to switch plugin features off
//! for this client (bit names from the JA+ 1.4B4 plugin's `cgamex86.dll`, in
//! EternalJK's order at `cg_consolecmds.c:1160-1185`; a set bit disables the
//! feature). The setting itself is the core client's: it is sent to every server.

use sjk_mod::{Host, ServerKind};
use sjk_protocol::JaPlusCapabilities;

/// The userinfo cvar holding the plugin's disabled-feature bits.
pub(crate) const PLUGIN_DISABLE: &str = "cp_pluginDisable";

/// JA+ plugin features in `cp_pluginDisable` bit order.
const PLUGIN_FEATURES: [&str; 15] = [
    "New drain effect (applies on reconnect)",
    "Hide other players during a duel",
    "End-of-duel camera rotation",
    "See black sabers",
    "Auto replier",
    "New Force effect",
    "No new death message",
    "New Force sight effect",
    "No alternate-dimension effect",
    "Holstered saber on the body",
    "Ledge grab",
    "New DFA (primary attack)",
    "New DFA (alternate attack)",
    "No single-player cartwheel",
    "Download URL redirect",
];

/// `jp_cinfo` options in EternalJK's listing order, with the macro-scan pair
/// and the three roll modes reported separately.
const SERVER_OPTIONS: [(u32, &str); 12] = [
    (JaPlusCapabilities::FLIP_KICK, "Flip kick"),
    (JaPlusCapabilities::YELLOW_DFA, "Improved yellow DFA"),
    (JaPlusCapabilities::HEAD_SLIDE, "Head slide"),
    (JaPlusCapabilities::SINGLE_PLAYER_ATTACKS, "SP attacks"),
    (JaPlusCapabilities::NEW_DFA, "New close-range DFA"),
    (JaPlusCapabilities::MODEL_SCALE, "Model scale"),
    (
        JaPlusCapabilities::DAMAGE_SPEED_SCALE,
        "Model scale damage speed scale",
    ),
    (JaPlusCapabilities::JK2_DFA, "JK2 red DFA"),
    (JaPlusCapabilities::NO_KATA, "Remove kata"),
    (JaPlusCapabilities::NO_AUTO_REPLIER, "Remove auto replier"),
    (JaPlusCapabilities::GLA_ANIMATIONS, "New GLA animations"),
    (JaPlusCapabilities::LEDGE_GRAB, "Ledge grab"),
];

fn yes_no(on: bool) -> &'static str {
    if on { "^2Yes" } else { "^1No" }
}

/// The JA+ server options `jp_cinfo` advertises, one line each.
fn server_option_lines(capabilities: JaPlusCapabilities) -> Vec<String> {
    let roll = if capabilities.contains(JaPlusCapabilities::FIX_ROLL_3) {
        "^23 (long, breakable)"
    } else if capabilities.contains(JaPlusCapabilities::FIX_ROLL_2) {
        "^22 (grip, chainable)"
    } else if capabilities.contains(JaPlusCapabilities::FIX_ROLL_1) {
        "^21 (grip while rolling)"
    } else {
        "^10 (stock)"
    };
    let mut lines = vec![
        format!("^5JA+ server options^3:^7 jp_cinfo {}", capabilities.0),
        format!(
            "^5Flip kick^3:^7 {}",
            yes_no(capabilities.contains(JaPlusCapabilities::FLIP_KICK))
        ),
        format!("^5Roll fix^3:^7 {roll}"),
    ];
    lines.extend(
        SERVER_OPTIONS[1..].iter().map(|(flag, label)| {
            format!("^5{label}^3:^7 {}", yes_no(capabilities.contains(*flag)))
        }),
    );
    let macro_scan = capabilities.contains(JaPlusCapabilities::MACRO_SCAN_1)
        || capabilities.contains(JaPlusCapabilities::MACRO_SCAN_2);
    lines.push(format!("^5Macro scan^3:^7 {}", yes_no(macro_scan)));
    lines.push(format!(
        "^5Alternate dimension^3:^7 {}",
        yes_no(capabilities.contains(JaPlusCapabilities::ALTERNATE_DIMENSION))
    ));
    lines
}

/// `japlus.serverconfig`: list JA+ options locally, ask jaPRO.
pub(crate) fn server_config(host: &mut dyn Host) -> Result<Vec<String>, String> {
    let server = host.server().ok_or("Not connected to a server.")?;
    match server.kind {
        ServerKind::JaPlus => {
            let bits = server
                .info("jp_cinfo")
                .and_then(|value| value.trim().parse::<i64>().ok())
                .unwrap_or(0);
            Ok(server_option_lines(JaPlusCapabilities(bits as u32)))
        }
        ServerKind::JaPro => {
            host.send("serverconfig")?;
            Ok(Vec::new())
        }
        ServerKind::BaseJka | ServerKind::Other => {
            Ok(vec!["^5Server is not running JA+ or jaPRO.".into()])
        }
    }
}

/// The plugin feature list with each feature's state under `bits`.
fn plugin_lines(bits: i64) -> Vec<String> {
    let mut lines = vec![
        "Usage: japlus.plugin [id]  (toggles a JA+ plugin feature)".to_owned(),
        "^5ID ^7<----> ^5Feature".to_owned(),
    ];
    lines.extend(
        PLUGIN_FEATURES
            .iter()
            .enumerate()
            .map(|(id, name)| format!("^5{id:2} ^7<----> ^5{name} ^7=> {}", state(bits, id))),
    );
    lines
}

fn state(bits: i64, id: usize) -> &'static str {
    if bits & (1 << id) != 0 {
        "^1Disallowed"
    } else {
        "^2Allowed"
    }
}

/// Parse a plugin feature id and return the toggled bits.
fn toggle(bits: i64, argument: &str) -> Result<(usize, i64), String> {
    let id = argument
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|id| *id < PLUGIN_FEATURES.len())
        .ok_or_else(|| {
            format!(
                "japlus.plugin: invalid id {argument} [0, {}]",
                PLUGIN_FEATURES.len() - 1
            )
        })?;
    Ok((id, bits ^ (1 << id)))
}

/// `japlus.plugin [id]`: list the JA+ plugin features or toggle one.
pub(crate) fn plugin(args: &[String], host: &mut dyn Host) -> Result<Vec<String>, String> {
    let bits = host
        .cvar(PLUGIN_DISABLE)
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0);
    let [argument] = args else {
        return if args.is_empty() {
            Ok(plugin_lines(bits))
        } else {
            Err("usage: japlus.plugin [id]".into())
        };
    };
    let (id, bits) = toggle(bits, argument)?;
    host.set_cvar(PLUGIN_DISABLE, &bits.to_string())?;
    Ok(vec![format!(
        "^5{} ^7=> {}",
        PLUGIN_FEATURES[id],
        state(bits, id)
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core client's default: holstered saber and ledge grab off.
    const PLUGIN_DISABLE_DEFAULT: i64 = 1_536;

    #[test]
    fn lists_the_local_ja_plus_server_options() {
        // JA+ Mod v2.4 B7 as configured on the test server: 0x300d3.
        let lines = server_option_lines(JaPlusCapabilities(196_819));
        assert_eq!(lines[0], "^5JA+ server options^3:^7 jp_cinfo 196819");
        assert_eq!(lines[1], "^5Flip kick^3:^7 ^2Yes");
        assert_eq!(lines[2], "^5Roll fix^3:^7 ^21 (grip while rolling)");
        let find = |label: &str| {
            lines
                .iter()
                .find(|line| line.starts_with(&format!("^5{label}^3")))
                .unwrap()
                .clone()
        };
        assert!(find("Improved yellow DFA").ends_with("^2Yes"));
        assert!(find("Head slide").ends_with("^1No"));
        assert!(find("SP attacks").ends_with("^2Yes"));
        assert!(find("New close-range DFA").ends_with("^2Yes"));
        assert!(find("Remove kata").ends_with("^1No"));
        assert!(find("Ledge grab").ends_with("^2Yes"));
        assert!(find("Macro scan").ends_with("^1No"));
        assert!(find("Alternate dimension").ends_with("^2Yes"));
        // One line per option plus the header.
        assert_eq!(lines.len(), 1 + 2 + 11 + 2);
    }

    #[test]
    fn highest_roll_mode_wins() {
        let lines = server_option_lines(JaPlusCapabilities(
            JaPlusCapabilities::FIX_ROLL_1 | JaPlusCapabilities::FIX_ROLL_3,
        ));
        assert_eq!(lines[2], "^5Roll fix^3:^7 ^23 (long, breakable)");
        assert_eq!(
            server_option_lines(JaPlusCapabilities(0))[2],
            "^5Roll fix^3:^7 ^10 (stock)"
        );
    }

    #[test]
    fn default_disables_holstered_saber_and_ledge_grab() {
        let lines = plugin_lines(PLUGIN_DISABLE_DEFAULT);
        assert_eq!(lines.len(), 2 + PLUGIN_FEATURES.len());
        assert!(lines[2 + 9].contains("Holstered saber on the body ^7=> ^1Disallowed"));
        assert!(lines[2 + 10].contains("Ledge grab ^7=> ^1Disallowed"));
        assert!(lines[2].contains("^5 0 ^7<----> ^5New drain effect"));
        assert!(lines[2].ends_with("^2Allowed"));
    }

    #[test]
    fn toggling_flips_one_bit_and_keeps_the_others() {
        assert_eq!(toggle(1_536, "10"), Ok((10, 512)));
        assert_eq!(toggle(512, "10"), Ok((10, 1_536)));
        // Bits outside the JA+ list are preserved.
        assert_eq!(toggle(1 << 20, "0"), Ok((0, (1 << 20) | 1)));
        assert!(toggle(0, "15").is_err());
        assert!(toggle(0, "-1").is_err());
        assert!(toggle(0, "ledge").is_err());
    }
}

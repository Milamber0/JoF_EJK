//! Local console connection and development-map command parsing.

use sjk_client::LegacyServerAddress;

/// A connection action requested by the local console.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// Tear down any current session and connect to this normalized address.
    Connect(String),
    /// Cancel an in-progress connection or leave the current server.
    Disconnect,
    /// Connect to the most recently requested address.
    Reconnect,
    /// Start an owned local map with server-authorized cheats.
    DevMap(String),
}

/// Parse client connection commands plus the local `devmap` launch request.
/// OpenJK registers connection commands in `codemp/client/cl_main.cpp`;
/// `codemp/server/sv_ccmds.cpp:SV_Map_f` enables cheats for `devmap`.
///
/// `CL_Connect_f` requires one argument and saves it for `CL_Reconnect_f`
/// (`cl_main.cpp:994-1068`). `CL_Disconnect_f` tears down the active session
/// at `cl_main.cpp:976-986`.
pub(crate) fn parse(tokens: &[String]) -> Result<Option<Action>, String> {
    let Some(name) = tokens.first() else {
        return Ok(None);
    };
    if name.eq_ignore_ascii_case("devmap") {
        if tokens.len() != 2 {
            return Err("usage: devmap <map> (for example: devmap mp/ffa3)".to_owned());
        }
        return map_name(&tokens[1]).map(|map| Some(Action::DevMap(map)));
    }
    if name.eq_ignore_ascii_case("connect") {
        if tokens.len() != 2 {
            return Err("usage: connect <host[:port]>".to_owned());
        }
        return LegacyServerAddress::parse(&tokens[1])
            .map(|address| Some(Action::Connect(address.into_string())))
            .map_err(|error| format!("Bad server address: {error}"));
    }
    if name.eq_ignore_ascii_case("disconnect") {
        return exact_arity(tokens, Action::Disconnect, "usage: disconnect");
    }
    if name.eq_ignore_ascii_case("reconnect") {
        return exact_arity(tokens, Action::Reconnect, "usage: reconnect");
    }
    Ok(None)
}

fn exact_arity(
    tokens: &[String],
    action: Action,
    usage: &'static str,
) -> Result<Option<Action>, String> {
    if tokens.len() == 1 {
        Ok(Some(action))
    } else {
        Err(usage.to_owned())
    }
}

/// Accept an installed BSP name, optionally with its virtual path/extension.
fn map_name(input: &str) -> Result<String, String> {
    if input.contains('\\') || input.chars().any(char::is_whitespace) {
        return Err("Invalid map name; use forward slashes, for example mp/ffa3".into());
    }
    let path =
        sjk_vfs::VirtualPath::new(input).map_err(|error| format!("Invalid map name: {error}"))?;
    let name = path.as_str().strip_prefix("maps/").unwrap_or(path.as_str());
    let name = name.strip_suffix(".bsp").unwrap_or(name);
    if name.is_empty() || name.starts_with('-') || name.contains([';', '"']) {
        return Err("Invalid map name".into());
    }
    Ok(name.to_owned())
}

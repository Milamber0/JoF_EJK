//! Siege selection metadata, loaded only when opening the selector.
//! TaystJK codemp/ui/ui_main.c:8730-8810; game/bg_saga.c:773-823,1255-1366.

use sjk_protocol::{GameState, InfoString};
use sjk_vfs::VirtualFileSystem;

/// The map's two allowed class lists, in theme order.
#[derive(Default)]
pub(super) struct Classes {
    /// Red and blue lists contain declared names accepted by `siegeclass`.
    pub teams: [Vec<String>; 2],
}

/// Reject commands outside Siege, just as UI_SetSiegeTeams does.
pub(crate) fn is_siege(game: Option<&GameState>) -> bool {
    game.and_then(|g| g.config_string(0))
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| InfoString::parse(s).ok())
        .is_some_and(|info| info.get_i32("g_gametype") == Some(7))
}

fn local_info(game: &GameState) -> Option<InfoString> {
    let client = usize::try_from(game.client_num).ok().filter(|n| *n < 32)?;
    let text = std::str::from_utf8(game.config_string(1_131 + client)?).ok()?;
    InfoString::parse(text).ok()
}

/// Local clientinfo, not the followed player's team in a spectator snapshot.
pub(crate) fn local_team(game: &GameState) -> u8 {
    local_info(game)
        .and_then(|info| info.get_i32("t"))
        .and_then(|team| u8::try_from(team).ok())
        .unwrap_or(3)
}

/// UI setsiegeclassandteam closes unchanged selections without triggering another respawn.
pub(crate) fn unchanged_choice(game: &GameState, command: &str) -> bool {
    let Some(info) = local_info(game) else {
        return false;
    };
    let team = info.get_i32("t");
    if command == "team spectator" {
        return team == Some(3);
    }
    let name = command
        .strip_prefix("siegeclass \"")
        .and_then(|s| s.strip_suffix('"'));
    matches!(team, Some(1 | 2)) && name.is_some_and(|name| info.get("siegeclass") == Some(name))
}

impl Classes {
    /// Resolve the map's two themes and their declared classes from the mounted search path.
    pub fn load(vfs: &VirtualFileSystem, game: &GameState) -> Result<Self, String> {
        if !is_siege(Some(game)) {
            return Err("Class selection is only available in Siege".into());
        }
        let info =
            std::str::from_utf8(game.config_string(0).unwrap()).map_err(|e| e.to_string())?;
        let info = InfoString::parse(info).map_err(|e| e.to_string())?;
        let map = info.get("mapname").ok_or("Missing Siege map name")?;
        let map_data = read(vfs, &format!("maps/{map}.siege"))?;
        let teams = group(&map_data, "Teams").ok_or("Missing Siege Teams group")?;
        let mut class_names = Vec::new();
        let mut themes = Vec::new();
        for path in vfs.paths() {
            let path = path.as_str();
            if path.starts_with("ext_data/siege/classes/") && path.ends_with(".scl") {
                let data = read(vfs, path)?;
                let class = group(&data, "ClassInfo").ok_or("Missing ClassInfo group")?;
                let name = value(class, "name").ok_or("Missing Siege class name")?;
                if !safe_name(name) {
                    return Err(format!("Unsafe or oversized Siege class name in {path}"));
                }
                class_names.push(name.to_owned());
            } else if path.starts_with("ext_data/siege/teams/") && path.ends_with(".team") {
                themes.push(read(vfs, path)?);
            }
        }
        let mut result = Self::default();
        for (index, output) in result.teams.iter_mut().enumerate() {
            let key = format!("team{}", index + 1);
            let override_key = format!("g_siegeTeam{}", index + 1);
            let team = info
                .get(&override_key)
                .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
                .or_else(|| value(teams, &key))
                .ok_or("Missing Siege team")?;
            let team = group(&map_data, team).ok_or("Missing Siege team group")?;
            let theme = value(team, "UseTeam").ok_or("Missing Siege UseTeam")?;
            let theme = themes
                .iter()
                .find(|data| value(data, "name").is_some_and(|n| n.eq_ignore_ascii_case(theme)))
                .ok_or_else(|| format!("Siege theme not installed: {theme}"))?;
            let entries = group(theme, "Classes").ok_or("Missing theme Classes")?;
            for number in 1..64 {
                let Some(name) = value(entries, &format!("class{number}")) else {
                    break;
                };
                let name = class_names
                    .iter()
                    .find(|n| n.eq_ignore_ascii_case(name))
                    .ok_or_else(|| format!("Siege class not installed: {name}"))?;
                output.push(name.clone());
            }
            if output.is_empty() {
                return Err("Siege team has no selectable classes".into());
            }
        }
        Ok(result)
    }
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 64
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | ';' | '\\'))
}

fn read(vfs: &VirtualFileSystem, path: &str) -> Result<Vec<String>, String> {
    let asset = vfs
        .read(path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Siege data not installed: {path}"))?;
    let text = std::str::from_utf8(&asset.bytes).map_err(|e| e.to_string())?;
    tokenize(text, path.ends_with(".scl")).map_err(|error| format!("{path}: {error}"))
}

fn tokenize(text: &str, class_only: bool) -> Result<Vec<String>, String> {
    let mut chars = text.chars().peekable();
    let mut result = Vec::new();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut last = ' ';
            let mut closed = false;
            for c in chars.by_ref() {
                if last == '*' && c == '/' {
                    closed = true;
                    break;
                }
                last = c;
            }
            if !closed {
                return Err("Unterminated Siege comment".into());
            }
            continue;
        }
        let mut token = String::new();
        if c == '"' {
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '"' {
                    closed = true;
                    break;
                }
                token.push(c);
            }
            if !closed {
                return Err("Unterminated Siege quote".into());
            }
        } else {
            token.push(c);
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth = depth
                    .checked_sub(1)
                    .ok_or("Unexpected Siege closing brace")?;
            } else {
                while chars
                    .peek()
                    .is_some_and(|c| !c.is_whitespace() && *c != '{' && *c != '}')
                {
                    let mut lookahead = chars.clone();
                    if lookahead.next() == Some('/') && lookahead.next() == Some('/') {
                        break;
                    }
                    token.push(chars.next().unwrap());
                }
            }
        }
        result.push(token);
        // BG_SiegeParseClassFile extracts ClassInfo separately from description text.
        // Retail mercassault.scl has a stray quote AFTER that valid group.
        if class_only
            && depth == 0
            && result.last().is_some_and(|s| s == "}")
            && result
                .windows(2)
                .any(|p| p[0].eq_ignore_ascii_case("ClassInfo") && p[1] == "{")
        {
            break;
        }
    }
    if depth != 0 {
        return Err("Unclosed Siege group".into());
    }
    Ok(result)
}

fn value<'a>(tokens: &'a [String], key: &str) -> Option<&'a str> {
    let mut depth = 0;
    for pair in tokens.windows(2) {
        match pair[0].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            name if depth == 0 && name.eq_ignore_ascii_case(key) && pair[1] != "{" => {
                return Some(&pair[1]);
            }
            _ => {}
        }
    }
    None
}

fn group<'a>(tokens: &'a [String], key: &str) -> Option<&'a [String]> {
    let mut depth = 0;
    for (i, pair) in tokens.windows(2).enumerate() {
        if depth == 0 && pair[0].eq_ignore_ascii_case(key) && pair[1] == "{" {
            let mut nested = 1;
            for j in i + 2..tokens.len() {
                match tokens[j].as_str() {
                    "{" => nested += 1,
                    "}" => nested -= 1,
                    _ => {}
                }
                if nested == 0 {
                    return Some(&tokens[i + 2..j]);
                }
            }
        }
        match pair[0].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ => {}
        }
    }
    None
}

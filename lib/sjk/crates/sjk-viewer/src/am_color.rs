//! `amcolor <red> <green> <blue>`: EternalJK's `amColor` (`cg_consolecmds.c`, "JAPRO -
//! Clientside - Amcolor").
//!
//! It sets the archived `char_color_*` userinfo cvars, so the tint is sent to the server
//! and kept for the next connect and the next start. Before SJK ran it, `amcolor` went to
//! the server as an unknown command, which some servers apply for the current connection
//! only, so the colour was gone after a reconnect.

use crate::console::ViewerConsole;

/// The console command name (EternalJK spells it `amColor`; commands ignore case).
pub(crate) const COMMAND: &str = "amcolor";
/// Its console help line.
pub(crate) const HELP: &str = "Set and save your player tint: amcolor <red> <green> <blue>, each 0 to 255 (EternalJK's amColor)";

/// The tint cvars, red, green and blue.
const CVARS: [&str; 3] = ["char_color_red", "char_color_green", "char_color_blue"];

/// Run `amcolor` with `args`; bare, it prints the current tint.
pub(crate) fn run(console: &mut ViewerConsole, args: &[String]) -> Result<Vec<String>, String> {
    let [red, green, blue] = args else {
        if args.is_empty() {
            let [red, green, blue] = CVARS.map(|name| console.integer_cvar(name).unwrap_or(255));
            return Ok(vec![format!(
                "Player tint {red} {green} {blue}; usage: amcolor <red> <green> <blue>"
            )]);
        }
        return Err("usage: amcolor <red> <green> <blue>, such as amcolor 255 255 0".into());
    };
    let values = parse([red, green, blue])?;
    let texts = values.map(|value| value.to_string());
    console.set_cvars(CVARS.iter().copied().zip(texts.iter().map(String::as_str)));
    Ok(Vec::new())
}

/// The three channels, each a whole number from 0 to 255.
fn parse(texts: [&String; 3]) -> Result<[u8; 3], String> {
    let mut values = [0_u8; 3];
    for (value, text) in values.iter_mut().zip(texts) {
        *value = text
            .trim()
            .parse()
            .map_err(|_| format!("amcolor: {text} is not 0 to 255"))?;
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_tint_is_set_and_kept_for_the_next_start() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(run(&mut console, &args("10 200 0")), Ok(Vec::new()));
        console.flush_config();
        let console = ViewerConsole::new(path).unwrap();
        let tint = CVARS.map(|name| console.integer_cvar(name));
        assert_eq!(tint, [Some(10), Some(200), Some(0)]);
    }

    #[test]
    fn a_bad_channel_changes_nothing() {
        let profile = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(profile.path().join("config.cfg")).unwrap();
        assert!(run(&mut console, &args("10 300 0")).is_err());
        assert!(run(&mut console, &args("10 20")).is_err());
        assert_eq!(console.integer_cvar("char_color_red"), Some(255));
        assert_eq!(
            run(&mut console, &[]),
            Ok(vec![
                "Player tint 255 255 255; usage: amcolor <red> <green> <blue>".to_owned()
            ])
        );
    }
}

//! The `sjkavatar` console command (`docs/identity.md`, "Pictures"): the words for
//! what the Profile page's picture panel does with Browse... or a file dropped on it.
//! `sjkavatar <file>` reads a picture file and shows it on the Profile page, where Use
//! this picture sends it; `sjkavatar clear` takes the player's picture down; with no
//! words it opens the picture panel.

use std::path::PathBuf;

/// Console command name.
pub(crate) const COMMAND: &str = "sjkavatar";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str =
    "Your SJK picture: sjkavatar <file> shows a picture to send, sjkavatar clear takes yours down";

/// What the player asked for.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// Open the picture panel.
    Open,
    /// Read this picture file.
    Load(PathBuf),
    /// Take the picture down.
    Clear,
}

/// Read the command's words: a path with spaces may come unquoted, its words joined
/// again (a file named `clear` is `./clear`).
pub(crate) fn parse(args: &[String]) -> Action {
    match args {
        [] => Action::Open,
        [word] if word.eq_ignore_ascii_case("clear") || word.eq_ignore_ascii_case("remove") => {
            Action::Clear
        }
        words => {
            let joined = words.join(" ");
            let path = joined.trim().trim_matches('"');
            Action::Load(PathBuf::from(path))
        }
    }
}

impl crate::GpuState {
    /// `sjkavatar [<file> | clear]`.
    pub(crate) fn avatar_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let action = parse(args);
        if matches!(&action, Action::Load(path) if !path.is_file()) {
            return Err("sjkavatar: there is no file there".to_owned());
        }
        // The SJK UI shows the picture panel on the Profile screen's SJK Profile tab.
        if self.in_game_menu.is_sjk() {
            self.open_profile_hub(crate::profile_hub::Tab::Profile);
        }
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        let lines = match action {
            Action::Open => {
                console.profile_show_picture();
                vec![
                    "sjkavatar: Browse... for a picture, drop its file on the window, or type sjkavatar <file>"
                        .to_owned(),
                ]
            }
            Action::Load(path) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                console.profile_load_picture(&path);
                vec![format!(
                    "sjkavatar: reading {name}; Use this picture on the Profile page sends it"
                )]
            }
            Action::Clear => {
                console.profile_remove_picture();
                vec!["sjkavatar: asking the SJK hub to take your picture down".to_owned()]
            }
        };
        self.sync_cursor_policy();
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn the_words_say_what_to_do() {
        assert_eq!(parse(&[]), Action::Open);
        assert_eq!(parse(&words("clear")), Action::Clear);
        assert_eq!(parse(&words("REMOVE")), Action::Clear);
        assert_eq!(
            parse(&words("my pictures/me.png")),
            Action::Load(PathBuf::from("my pictures/me.png"))
        );
        assert_eq!(
            parse(&words("\"me.png\"")),
            Action::Load(PathBuf::from("me.png"))
        );
        assert_eq!(
            parse(&words("./clear")),
            Action::Load(PathBuf::from("./clear"))
        );
    }
}

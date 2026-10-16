//! Command-line compatibility and native no-argument launch resolution.

use std::env;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// Fully resolved startup request for the viewer application.
pub(crate) struct LaunchPlan {
    pub(crate) game_data: PathBuf,
    pub(crate) map_path: Option<String>,
    pub(crate) player_directory: Option<String>,
    pub(crate) connect_address: Option<String>,
    pub(crate) open_main_menu: bool,
}

/// Read only the installation hint before selecting storage. Constructing a
/// temporary console here would autosave the old user profile when dropped.
pub(crate) fn saved_game_data(config: &Path) -> Option<PathBuf> {
    std::fs::read_to_string(config)
        .ok()?
        .lines()
        .filter_map(|line| {
            let tokens = sjk_shell::tokenize(line.trim()).ok()?;
            match tokens.as_slice() {
                [command, name, value]
                    if command.eq_ignore_ascii_case("seta")
                        && name.eq_ignore_ascii_case("fs_gameData") =>
                {
                    Some(value.trim().to_owned())
                }
                _ => None,
            }
        })
        .last()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Preserve the historical positional developer syntax while making an empty
/// argument list the normal native-client launch path.
pub(crate) fn resolve(
    arguments: impl Iterator<Item = OsString>,
    configured_game_data: Option<PathBuf>,
) -> Result<LaunchPlan, LaunchError> {
    let arguments = arguments.collect::<Vec<_>>();
    if arguments.is_empty() {
        return Ok(LaunchPlan {
            game_data: find_game_data(configured_game_data.as_deref())?,
            map_path: None,
            player_directory: None,
            connect_address: None,
            open_main_menu: true,
        });
    }

    let (game_data, trailing) = if arguments[0] == "--connect" {
        (
            find_game_data(configured_game_data.as_deref())?,
            &arguments[..],
        )
    } else {
        let game_data = PathBuf::from(&arguments[0]);
        if !is_game_data(&game_data) {
            return Err(LaunchError::InvalidGameData(game_data));
        }
        (game_data, &arguments[1..])
    };
    let trailing = trailing
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut positionals = Vec::new();
    let mut connect_address = None;
    let mut index = 0;
    while index < trailing.len() {
        if trailing[index] == "--connect" {
            index += 1;
            connect_address = Some(
                trailing
                    .get(index)
                    .ok_or(LaunchError::MissingConnectAddress)?
                    .clone(),
            );
        } else {
            positionals.push(trailing[index].clone());
        }
        index += 1;
    }
    Ok(LaunchPlan {
        game_data,
        map_path: positionals.first().cloned(),
        player_directory: positionals.get(1).cloned(),
        connect_address,
        open_main_menu: false,
    })
}

fn find_game_data(configured: Option<&Path>) -> Result<PathBuf, LaunchError> {
    let current = env::current_dir().ok();
    let executable = env::current_exe().ok();
    let home = env::var_os("HOME").map(PathBuf::from);
    let environment = env::var_os("JKA_GAME_DATA")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    find_game_data_in(
        configured,
        environment.as_deref(),
        executable.as_deref().and_then(Path::parent),
        current.as_deref(),
        home.as_deref(),
    )
    .ok_or(LaunchError::GameDataNotFound)
}

// Explicit overrides win; a drop-in installation must not be redirected by a
// saved path from another installation. Never change cwd: other relative paths
// (for example a command-line demo path) still belong to the caller.
fn find_game_data_in(
    configured: Option<&Path>,
    environment: Option<&Path>,
    executable_directory: Option<&Path>,
    current: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let mut candidates = Vec::with_capacity(10);
    candidates.extend(environment.map(Path::to_owned));
    if let Some(directory) = executable_directory {
        candidates.push(directory.to_owned());
        candidates.push(directory.join("GameData"));
    }
    candidates.extend(configured.map(Path::to_owned));
    if let Some(current) = current {
        candidates.push(current.to_owned());
        candidates.push(current.join("GameData"));
        candidates.push(current.join("Star Wars Jedi Knight - Jedi Academy/GameData"));
    }
    if let Some(home) = home {
        candidates.push(home.join(".local/share/Steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join(".steam/steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join("Games/Jedi Academy/GameData"));
    }
    candidates
        .into_iter()
        .find(|candidate| is_game_data(candidate))
}

fn is_game_data(path: &Path) -> bool {
    path.join("base/assets0.pk3").is_file() && path.join("base/assets3.pk3").is_file()
}

#[derive(Debug)]
pub(crate) enum LaunchError {
    GameDataNotFound,
    InvalidGameData(PathBuf),
    MissingConnectAddress,
}

impl Display for LaunchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GameDataNotFound => formatter.write_str(concat!(
                "Jedi Academy GameData was not found. Put SJK in the game's GameData ",
                "folder beside base/ (containing assets0.pk3 through assets3.pk3), ",
                "then launch it again. For a separate installation, set JKA_GAME_DATA, ",
                "set fs_gameData in the SJK config, or pass GameData as the first argument."
            )),
            Self::InvalidGameData(path) => write!(
                formatter,
                concat!(
                    "{} is not a Jedi Academy GameData directory ",
                    "(base/assets0.pk3 and assets3.pk3 are required)"
                ),
                path.display()
            ),
            Self::MissingConnectAddress => {
                formatter.write_str("--connect requires server-host:port")
            }
        }
    }
}

impl std::error::Error for LaunchError {}

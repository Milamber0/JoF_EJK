//! Platform composition for per-user SJK state.

use std::env;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::sync::OnceLock;

#[path = "platform/storage.rs"]
mod storage;

static CONFIG_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Hosted inside another engine (`rd-sjk`): keep settings in `config`, a file that
/// engine chose, so the hosted renderer never writes SJK's own `GameData/SJK` profile.
pub(crate) fn initialize_hosted_storage(config: PathBuf) -> Result<(), Error> {
    if let Some(directory) = config.parent() {
        std::fs::create_dir_all(directory)?;
    }
    CONFIG_FILE
        .set(config)
        .map_err(|_| Error::other("client storage already initialized"))
}

/// Select writable client storage once, before constructing any UI or workers.
pub(crate) fn initialize_storage(game_data: &std::path::Path) -> Result<(), Error> {
    if CONFIG_FILE.get().is_some() {
        return Err(Error::other("client storage already initialized"));
    }
    let per_user = per_user_config_file().ok();
    let selection = storage::select(game_data, per_user.as_deref())?;
    if let Some(reason) = selection.fallback_reason {
        crate::log::progress(format_args!(
            "GameData/SJK is not writable ({reason}); using user storage"
        ));
    }
    crate::log::progress(format_args!("client files: {}", selection.config.display()));
    CONFIG_FILE
        .set(selection.config)
        .map_err(|_| Error::other("client storage already initialized"))
}

/// Ask the native interface-listing utility; no external probe packets or DNS tricks.
pub(crate) fn interface_addresses() -> Result<String, Error> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "windows") {
        ("ipconfig", &[])
    } else if cfg!(target_os = "linux") {
        ("ip", &["-brief", "address", "show"])
    } else {
        ("ifconfig", &[])
    };
    let output = std::process::Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The selected client configuration; all file consumers share this location.
pub(crate) fn user_config_file() -> Result<PathBuf, Error> {
    CONFIG_FILE
        .get()
        .cloned()
        .map_or_else(per_user_config_file, Ok)
}

/// The per-user configuration, used when `GameData/SJK` is not writable and read
/// for a saved `fs_gameData` before the game data is known.
pub(crate) fn per_user_config_file() -> Result<PathBuf, Error> {
    let root = if cfg!(target_os = "windows") {
        env_path("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env_path("HOME")?.join("Library/Application Support")
    } else if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        PathBuf::from(path)
    } else {
        env_path("HOME")?.join(".config")
    };
    Ok(root.join("SJK/config.cfg"))
}

fn env_path(name: &str) -> Result<PathBuf, Error> {
    env::var_os(name)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("{name} is not set")))
}

//! The `ja_guid` a stock client identifies itself with.
//!
//! `CL_GenerateQKey` writes 2048 random bytes to `jakey` once and keeps it
//! (`codemp/client/cl_main.cpp:2676-2700`), and `CL_UpdateGUID` publishes
//! `ja_guid` as a `CVAR_USERINFO` value: the uppercase hex MD5 of the server
//! address followed by that key (`:739-757`, `Com_MD5File`,
//! `qcommon/md5.cpp:255-306`). With `cl_guidServerUniq` at its stock default
//! the address is included, so each server sees a different, stable id and
//! none of them learns the key itself.
//!
//! Servers and mods use it to recognise a returning player, and protection
//! layers in front of a server may insist on one. A client that sends none
//! is the `NOGUID` a server logs.

use md5::{Digest, Md5};
use std::fs;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// `QKEY_SIZE` (`codemp/client/client.h:41`).
const KEY_BYTES: usize = 2048;

/// The `ja_guid` for `server`, generating the key file if it is missing.
///
/// Errors leave the caller free to connect without a GUID, as stock does,
/// but must be reported so a broken identity store is not invisible.
pub(crate) fn ja_guid(config_file: &Path, server: SocketAddr) -> io::Result<String> {
    generate(config_file, Some(server))
}

/// Connection-time GUID policy; the key remains private and outside userinfo.
pub(crate) struct Policy {
    /// Config location determining the adjacent persistent key path.
    pub(crate) config: PathBuf,
    /// False omits the GUID and never opens or creates a key.
    pub(crate) enabled: bool,
    /// Include the server address in the digest when true.
    pub(crate) server_unique: bool,
}

impl Policy {
    /// Apply the stock connect-time choice without changing the userinfo codec.
    pub(crate) fn identity(&self, server: SocketAddr) -> io::Result<Option<String>> {
        if !self.enabled {
            return Ok(None);
        }
        generate(&self.config, self.server_unique.then_some(server)).map(Some)
    }
}

fn generate(config_file: &Path, server: Option<SocketAddr>) -> io::Result<String> {
    let key = load_or_create_key(&key_path(config_file))?;
    let mut digest = Md5::new();
    if let Some(server) = server {
        digest.update(server.to_string().as_bytes());
    }
    digest.update(&key);
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect())
}

/// `jakey`, beside the configuration file it belongs to.
fn key_path(config_file: &Path) -> PathBuf {
    config_file.with_file_name("jakey")
}

fn load_or_create_key(path: &Path) -> io::Result<[u8; KEY_BYTES]> {
    if let Some(key) = read_key(path)? {
        return Ok(key);
    }
    let mut key = [0; KEY_BYTES];
    // Bounded OS entropy on Linux, Windows and macOS; never read a random
    // device to EOF (it has none).
    getrandom::fill(&mut key)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    // Publish a complete key, with private permissions, on the same volume.
    // Competing first connections must reuse the winner's identity.
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    pending.write_all(&key)?;
    pending.as_file().sync_all()?;
    match pending.persist_noclobber(path) {
        Ok(_) => Ok(key),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
            if let Some(winner) = read_key(path)? {
                return Ok(winner);
            }
            // Stock regenerates an invalid-size key. Atomic replacement also
            // avoids following a symlink to overwrite an unrelated file.
            error.file.persist(path).map_err(|error| error.error)?;
            Ok(key)
        }
        Err(error) => Err(error.error),
    }
}

fn read_key(path: &Path) -> io::Result<Option<[u8; KEY_BYTES]>> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "jakey is not a regular file",
        ));
    }
    if metadata.len() != KEY_BYTES as u64 {
        return Ok(None);
    }
    let mut key = [0; KEY_BYTES];
    file.read_exact(&mut key)?;
    Ok(Some(key))
}

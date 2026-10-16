//! Where the console finds a config file for `exec`, and where the server keeps its
//! archived variables — the reference's search paths as far as a server's configs go.
//!
//! A name is looked up in the operator's home directory (`fs_homepath/base`), then in
//! the game data's archives (`base/*.pk3`, in the reference's order, where the stock
//! `mpdefault.cfg` lives), then loose in the game data's `base` directory. Nothing is
//! written unless a home directory was given.

use sjk_vfs::VirtualFileSystem;
use std::path::PathBuf;

/// The file `Com_WriteConfiguration` keeps archived variables in (`openjk_server.cfg`
/// in the reference), in the home directory's `base`.
pub const ARCHIVE_FILE: &str = "sjk_server.cfg";

/// The search paths.
#[derive(Default)]
pub struct ConfigFiles {
    /// Written to and read first; `None` for a server that writes nothing.
    pub home: Option<PathBuf>,
    /// The retail `GameData` directory.
    pub game_data: Option<PathBuf>,
    /// The game data's archives, mounted on first use.
    archives: Option<VirtualFileSystem>,
}

impl ConfigFiles {
    /// Search paths over an optional home and game-data directory.
    pub fn new(home: Option<PathBuf>, game_data: Option<PathBuf>) -> Self {
        Self {
            home,
            game_data,
            archives: None,
        }
    }

    /// A file's bytes by its game path, or `None` where no search path has it.
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let name = name.trim_start_matches(['/', '\\']);
        if let Some(home) = &self.home
            && let Ok(bytes) = std::fs::read(home.join("base").join(name))
        {
            return Some(bytes);
        }
        let game_data = self.game_data.clone()?;
        if self.archives.is_none() {
            let mut files = VirtualFileSystem::new();
            let _ = files.mount_pk3_directory_with_warnings(
                game_data.join("base"),
                |archive, error| {
                    eprintln!("skipping {}: {error}", archive.display());
                },
            );
            self.archives = Some(files);
        }
        if let Some(Ok(Some(asset))) = self.archives.as_ref().map(|files| files.read(name)) {
            return Some(asset.bytes.to_vec());
        }
        std::fs::read(game_data.join("base").join(name)).ok()
    }

    /// Whether a search path has the file (`FS_Open` > 0), without reading it. The
    /// archives are those an earlier [`Self::read`] or [`Self::list_files`] mounted.
    pub fn has(&self, name: &str) -> bool {
        let name = name.trim_start_matches(['/', '\\']);
        if self
            .home
            .as_ref()
            .is_some_and(|home| home.join("base").join(name).is_file())
        {
            return true;
        }
        let archived = self
            .archives
            .as_ref()
            .is_some_and(|files| files.contains(name).unwrap_or(false));
        archived
            || self
                .game_data
                .as_ref()
                .is_some_and(|game_data| game_data.join("base").join(name).is_file())
    }

    /// `FS_GetFileList` of `directory` in the game data's archives: the names with
    /// `extension`, in the archives' order.
    pub fn list_files(&mut self, directory: &str, extension: &str) -> Vec<String> {
        self.read("");
        self.archives
            .as_ref()
            .map(|files| files.list_files(directory, extension))
            .unwrap_or_default()
    }

    /// A file of the server's own in the home directory's `base` (`FS_SV_FOpenFileRead`,
    /// as the ban file is read); `None` without a home or the file.
    pub fn read_home(&self, name: &str) -> Option<Vec<u8>> {
        if !stays_inside(name) {
            return None;
        }
        std::fs::read(self.home.as_ref()?.join("base").join(name)).ok()
    }

    /// Write a file of the server's own in the home directory's `base`
    /// (`FS_SV_FOpenFileWrite`); nothing without a home.
    pub fn write_home(&self, name: &str, text: &[u8]) -> std::io::Result<()> {
        let Some(home) = &self.home else {
            return Ok(());
        };
        if !stays_inside(name) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        let directory = home.join("base");
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join(name), text)
    }

    /// A file of the server's own in the home directory's `base`, opened for appending
    /// (`FS_APPEND`, created if missing); `None` without a home or where it cannot be.
    pub fn append_home(&self, name: &str) -> Option<std::fs::File> {
        if !stays_inside(name) {
            return None;
        }
        let directory = self.home.as_ref()?.join("base");
        std::fs::create_dir_all(&directory).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join(name))
            .ok()
    }

    /// Append `bytes` to a file of the server's own in the home directory's `base`
    /// (`FS_SV_FOpenFileAppend`); `true` without a home, where nothing is kept, and
    /// `false` where it cannot be opened.
    pub fn append_record(&self, name: &str, bytes: &[u8]) -> bool {
        if self.home.is_none() {
            return true;
        }
        self.append_home(name)
            .is_some_and(|mut file| std::io::Write::write_all(&mut file, bytes).is_ok())
    }

    /// A file of the server's own in the home directory's `base`, created afresh with
    /// its directories (`FS_FOpenFileWrite`); `None` without a home or where it cannot be.
    pub fn create_home(&self, name: &str) -> Option<std::fs::File> {
        if !stays_inside(name) {
            return None;
        }
        let path = self.home.as_ref()?.join("base").join(name);
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::File::create(path).ok()
    }

    /// Whether a file of the server's own exists in the home directory's `base`.
    pub fn home_has(&self, name: &str) -> bool {
        stays_inside(name)
            && self
                .home
                .as_ref()
                .is_some_and(|home| home.join("base").join(name).exists())
    }

    /// `sv_autoDemoMaxMaps`: only the `keep` newest maps' folders of automatic demos
    /// stay in the home directory's `base/demos/autorecord`; nothing without a home.
    pub fn prune_auto_demos(&self, keep: usize) {
        if let Some(home) = &self.home {
            sjk_network::legacy_prune_auto_demos(&mut HomeFolders(home.join("base")), keep);
        }
    }

    /// `Com_WriteConfigToFile`: the archived variables' `seta` lines under the
    /// reference's header. Nothing happens without a home directory.
    pub fn write_archive(&self, lines: &[u8]) -> std::io::Result<()> {
        self.write_home(
            ARCHIVE_FILE,
            &[&b"// generated by SJK, do not modify\n"[..], lines].concat(),
        )
    }
}

/// `FS_FOpenFileRead`'s and `FS_SV_FOpenFileRead`'s guard: nothing that could leave the
/// search path, and no absolute path.
fn stays_inside(name: &str) -> bool {
    !name.contains("..") && !name.contains("::") && !name.starts_with(['/', '\\'])
}

/// `gethostbyname`: a host name's first IPv4 address.
pub fn resolve_host(name: &str) -> Option<std::net::Ipv4Addr> {
    std::net::ToSocketAddrs::to_socket_addrs(&(name, 0))
        .ok()?
        .find_map(|address| match address {
            std::net::SocketAddr::V4(address) => Some(*address.ip()),
            std::net::SocketAddr::V6(_) => None,
        })
}

/// The home directory's `base` as automatic demos' folders are listed and removed in it.
struct HomeFolders(PathBuf);

impl sjk_network::LegacyDemoFolders for HomeFolders {
    fn folders(&self, path: &str) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.0.join(path)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
    fn has_files(&self, path: &str) -> bool {
        std::fs::read_dir(self.0.join(path)).is_ok_and(|mut entries| {
            entries.any(|entry| {
                entry.is_ok_and(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()))
            })
        })
    }
    fn remove(&mut self, path: &str, recursive: bool) {
        if !stays_inside(path) {
            return;
        }
        let path = self.0.join(path);
        // What cannot be removed stays, as `FS_Rmdir` leaves it.
        let _ = if recursive {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_dir(path)
        };
    }
}

//! A narrowly scoped, worker-only capability for downloaded content.

use sjk_client::download::{DownloadStorage, PakRequest};
use sjk_protocol::{GameState, InfoString};
use sjk_vfs::Pk3Fingerprint;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

#[path = "download_policy.rs"]
pub(crate) mod policy;

const FILE_LIMIT: u64 = 256 * 1024 * 1024;
const CONNECT_LIMIT: u64 = 1024 * 1024 * 1024;
const EXPANDED_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Isolated content home; never the configuration file or the retail installation.
pub(crate) fn home() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("SJK_DOWNLOAD_HOME").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(root).join("base"));
    }
    if cfg!(target_os = "windows") {
        return std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("SJK/downloads/base"))
            .ok_or_else(|| "LOCALAPPDATA is unset; set SJK_DOWNLOAD_HOME".into());
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/SJK/downloads/base"))
            .ok_or_else(|| "HOME is unset; set SJK_DOWNLOAD_HOME".into());
    }
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .ok_or("cannot determine download home; set SJK_DOWNLOAD_HOME")?;
    Ok(root.join("SJK/downloads/base"))
}

/// Whether `archive` (a mounted pak's path) lives in the download home, so the
/// download policy applies to it.
pub(crate) fn is_download(archive: &Path) -> bool {
    // Kept once found: the folder only exists after the first download.
    static HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let home = match HOME.get() {
        Some(home) => home,
        None => {
            match home().and_then(|home| std::fs::canonicalize(home).map_err(|e| e.to_string())) {
                Ok(home) => HOME.get_or_init(|| home),
                Err(_) => return false,
            }
        }
    };
    // Mount names are canonical already; compare canonically all the same.
    archive.parent().is_some_and(|parent| {
        parent == home || std::fs::canonicalize(parent).is_ok_and(|parent| &parent == home)
    })
}

/// Cached comparison plus temporary output. No ambient remote paths reach filesystem calls.
pub(crate) struct Store {
    root: PathBuf,
    checksums: Vec<i32>,
    enabled: bool,
    total: u64,
    active: Option<Active>,
    progress: SyncSender<String>,
    cancelled: Arc<AtomicBool>,
    last_progress: Instant,
}

struct Active {
    file: tempfile::NamedTempFile,
    target: PathBuf,
    request: PakRequest,
    expected: Option<u64>,
    received: u64,
}

impl Store {
    /// Inventory mounted sources once on the worker, without creating the home yet.
    pub(crate) fn open(
        game_data: &Path,
        root: PathBuf,
        enabled: bool,
        progress: SyncSender<String>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let mut checksums = Vec::new();
        let mut directories = super::search_paths::startup()
            .directories(game_data)
            .map_err(|error| error.to_string())?;
        if !directories.contains(&root) {
            directories.push(root.clone());
        }
        for directory in directories {
            if !directory.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("connection cancelled".into());
                }
                let path = entry.map_err(|e| e.to_string())?.path();
                if path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("pk3"))
                    && let Ok(pak) = Pk3Fingerprint::open(&path)
                {
                    checksums.push(pak.checksum());
                }
            }
        }
        if cancelled.load(Ordering::Relaxed) {
            return Err("connection cancelled".into());
        }
        Ok(Self {
            root,
            checksums,
            enabled,
            total: 0,
            active: None,
            progress,
            cancelled,
            last_progress: Instant::now() - Duration::from_secs(1),
        })
    }

    fn notify(&mut self) {
        if self.last_progress.elapsed() < Duration::from_millis(100) {
            return;
        }
        if let Some(active) = &self.active {
            let size = active.expected.unwrap_or(0);
            let text = format!(
                "Downloading {}: {} / {} KiB",
                active.request.filename,
                active.received / 1024,
                size / 1024
            );
            let _ = self.progress.try_send(text);
        }
        self.last_progress = Instant::now();
    }
}

/// Validate a plain filename, including script/command metacharacters and platform aliases.
///
/// Community paks use spaces, braces and apostrophes (`{JoF}Sentinel Nexus.pk3`,
/// `{JoF}'s-GWing.pk3`), so those pass; quotes, separators, `;`, `%`, `$`,
/// wildcards and control bytes never do. The request is sent quoted.
pub(crate) fn valid_filename(name: &str) -> bool {
    name.len() > 4
        && name.len() <= 180
        && name.ends_with(".pk3")
        && !name.contains("..")
        && !name.starts_with(['.', ' '])
        && !name.trim_end_matches(".pk3").ends_with([' ', '.'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.()+{}[]' !,&~".contains(&b))
        && !matches!(
            name.split('.')
                .next()
                .unwrap_or("")
                .to_ascii_uppercase()
                .as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
}

fn requests(game: &GameState, checksums: &[i32]) -> Result<Vec<PakRequest>, String> {
    let server_info = InfoString::parse(&String::from_utf8_lossy(
        game.config_string(0).unwrap_or(b""),
    ))
    .map_err(|e| e.to_string())?;
    // A referenced pak is not an admission requirement. Tayst's CL_InitDownloads
    // continues with local content when downloads are disabled. We support UDP
    // only, so an HTTP URL does not enable this transport.
    if server_info.get_i32("sv_allowDownload") == Some(0) {
        crate::log::progress(format_args!(
            "session content: server disabled UDP downloads; using available content"
        ));
        return Ok(Vec::new());
    }
    let Some(raw) = game.config_string(1) else {
        return Ok(Vec::new());
    };
    let info = InfoString::parse(std::str::from_utf8(raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let references = sjk_client::referenced_paks::parse(
        info.get("sv_referencedPaks").unwrap_or(""),
        info.get("sv_referencedPakNames").unwrap_or(""),
    )?;
    let mut missing = Vec::new();
    for reference in references {
        let name = reference.name;
        let checksum = reference.checksum;
        if checksums.contains(&checksum) {
            continue;
        }
        let Some((directory, stem)) = name.split_once('/') else {
            crate::log::progress(format_args!(
                "session content: skipping unrequestable pak name {name:?}"
            ));
            continue;
        };
        if directory.is_empty()
            || !directory
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            crate::log::progress(format_args!(
                "session content: skipping unsafe pak game directory in {name:?}"
            ));
            continue;
        }
        let filename = format!("{stem}.pk3");
        if !valid_filename(&filename) {
            crate::log::progress(format_args!(
                "session content: skipping unsafe pak filename {filename:?}"
            ));
            continue;
        }
        // FS_ComparePaks never downloads the retail asset packs.
        if (directory.eq_ignore_ascii_case("base") || directory.eq_ignore_ascii_case("missionpack"))
            && stem.len() == 7
            && stem[..6].eq_ignore_ascii_case("assets")
            && (b'0'..=b'8').contains(&stem.as_bytes()[6])
        {
            continue;
        }
        if !missing.iter().any(|r: &PakRequest| r.checksum == checksum) {
            missing.push(PakRequest {
                remote: format!("{name}.pk3"),
                filename,
                checksum,
            });
        }
        if missing.len() > 64 {
            return Err("too many missing paks (limit 64)".into());
        }
    }
    Ok(missing)
}

impl DownloadStorage for Store {
    fn feedback(&mut self, progress: SyncSender<String>, cancelled: Arc<AtomicBool>) {
        self.progress = progress;
        self.cancelled = cancelled;
    }
    fn missing(&self, game: &GameState) -> Result<Vec<PakRequest>, String> {
        // cl_allowDownload controls transfers, not permission to join using the
        // installed map. Missing maps still fail when the BSP is loaded.
        if !self.enabled {
            return Ok(Vec::new());
        }
        requests(game, &self.checksums)
    }

    fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("download cancelled".into())
        } else {
            Ok(())
        }
    }

    fn begin(&mut self, request: &PakRequest) -> Result<(), String> {
        self.check()?;
        if !valid_filename(&request.filename) {
            return Err("unsafe pak filename".into());
        }
        // Checksum suffix avoids replacing a different locally installed version.
        let name = format!(
            "dl_{}.{:08x}.pk3",
            request.filename.trim_end_matches(".pk3"),
            request.checksum as u32
        );
        let target = self.root.join(name);
        if target.symlink_metadata().is_ok() {
            return Err("download target already exists".into());
        }
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let file = tempfile::Builder::new()
            .prefix(".download-")
            .suffix(".tmp")
            .tempfile_in(&self.root)
            .map_err(|e| e.to_string())?;
        self.active = Some(Active {
            file,
            target,
            request: request.clone(),
            expected: None,
            received: 0,
        });
        self.last_progress = Instant::now() - Duration::from_secs(1);
        self.notify();
        Ok(())
    }

    fn size(&mut self, bytes: u64) -> Result<(), String> {
        if bytes == 0 || bytes > FILE_LIMIT || bytes > CONNECT_LIMIT.saturating_sub(self.total) {
            self.abort();
            return Err("download size cap exceeded (256 MiB/pak, 1 GiB/connect)".into());
        }
        let active = self.active.as_mut().ok_or("unsolicited download size")?;
        if active.expected.replace(bytes).is_some() {
            return Err("duplicate download size".into());
        }
        self.total += bytes;
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.check()?;
        let active = self.active.as_mut().ok_or("unsolicited download data")?;
        let expected = active.expected.ok_or("download data before size")?;
        if bytes.len() as u64 > expected.saturating_sub(active.received) {
            return Err("download exceeds declared length".into());
        }
        active.file.write_all(bytes).map_err(|e| e.to_string())?;
        active.received += bytes.len() as u64;
        self.notify();
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        self.check()?;
        let mut active = self
            .active
            .take()
            .ok_or("unsolicited download terminator")?;
        if active.expected != Some(active.received) {
            return Err("truncated download".into());
        }
        active.file.flush().map_err(|e| e.to_string())?;
        verify(active.file.path(), active.request.checksum)?;
        // Reject invalid virtual paths before publishing a pak that startup would reject.
        let mut vfs = sjk_vfs::VirtualFileSystem::new();
        vfs.mount_pk3(active.file.path())
            .map_err(|e| e.to_string())?;
        drop(vfs); // Release the archive handle before renaming (also on Windows).
        self.check()?;
        active
            .file
            .as_file()
            .sync_all()
            .map_err(|e| e.to_string())?;
        // Atomic no-clobber publication. A competing file/symlink is never overwritten.
        active
            .file
            .persist_noclobber(&active.target)
            .map_err(|e| e.to_string())?;
        self.checksums.push(active.request.checksum);
        Ok(())
    }

    fn abort(&mut self) {
        self.active = None;
    }

    fn skipped(&mut self, request: &PakRequest, reason: &str) {
        crate::log::progress(format_args!(
            "session content: skipped download {}: {reason}",
            request.remote
        ));
        let _ = self
            .progress
            .try_send(format!("Skipped {}: {reason}", request.filename));
    }
}

fn verify(path: &Path, checksum: i32) -> Result<(), String> {
    let pak = Pk3Fingerprint::open(path).map_err(|e| e.to_string())?;
    if pak.checksum() != checksum {
        return Err("download checksum mismatch".into());
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut expanded = 0_u64;
    let mut buffer = [0_u8; 32768];
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        expanded = expanded
            .checked_add(entry.size())
            .ok_or("zip size overflow")?;
        if expanded > EXPANDED_LIMIT {
            return Err("expanded pak exceeds 2 GiB".into());
        }
        // Streaming to EOF checks CRCs without extracting paths or allocating asset buffers.
        let mut actual = 0_u64;
        loop {
            let count = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            actual += count as u64;
            if actual > entry.size() {
                return Err("zip entry exceeds declared length".into());
            }
        }
    }
    Ok(())
}

//! Self-update: look for a newer SJK release on GitHub, download it, check it
//! against the release's checksums and replace the programs beside the running
//! one (`docs/client.md`, "Updates").
//!
//! The check runs on a worker thread at start-up (`cl_autoUpdate`), again every
//! [`RECHECK`] while the client runs and found nothing new, and from the menu's
//! Update page; none blocks a frame. The state they share is read
//! with [`state`]. Nothing is installed without the player pressing Install, and
//! the new version starts when the client exits ([`restart_if_requested`]), so
//! the settings the old one saves on its way out are not overwritten.
//!
//! A release is the ZIP `SJK-<version>-<platform>.zip` that `release.yml`
//! publishes with `SJK-<version>-SHA256SUMS-<platform>.txt`. Only flat file
//! names are taken from the ZIP, beside the running program.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The repository that publishes SJK's releases.
const REPOSITORY: &str = "Sol-Vulpes/SJK";
/// The platform the release ZIPs name, or `None` where there is no release.
const PLATFORM: Option<&str> = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("windows-x64")
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x64")
} else {
    None
};
/// Longest the release lookup and the checksum file may take.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);
/// Longest the ZIP download may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(900);
/// How long a running client waits to ask again after finding nothing new (or
/// failing to ask), so a release made while players are in a match reaches them
/// as its card without a restart.
pub(crate) const RECHECK: Duration = Duration::from_secs(30 * 60);

/// A release file: its name, address and size in bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Asset {
    name: String,
    url: String,
    size: u64,
}

/// A published release this platform can install.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Release {
    /// `2026.1010.1`.
    pub(crate) version: String,
    /// The release's page, for its notes.
    pub(crate) page: String,
    zip: Asset,
    sums: Asset,
}

/// Where the update stands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum State {
    /// Nothing asked yet.
    Idle,
    /// Asking GitHub for the latest release.
    Checking,
    /// The installed version is the latest one.
    UpToDate,
    /// A newer release exists; installing it needs the player's go-ahead.
    Available(Release),
    /// Downloading `done` of `total` bytes (0 when the size is unknown).
    Downloading { done: u64, total: u64 },
    /// Checked and unpacking.
    Installing,
    /// Installed; the new version starts when the client exits.
    Installed,
    /// This build or folder cannot update itself; the release page is the way.
    Manual(Release, String),
    /// The last step failed.
    Failed(String),
    /// A local build has no release number to compare.
    Unversioned,
}

static STATE: Mutex<State> = Mutex::new(State::Idle);
/// When the last check started.
static LAST_CHECK: Mutex<Option<Instant>> = Mutex::new(None);
static RESTART: AtomicBool = AtomicBool::new(false);
/// Rises with every change of [`STATE`], so a frame can see one without locking it.
static GENERATION: AtomicU32 = AtomicU32::new(0);

fn lock() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn set(state: State) {
    *lock() = state;
    changed();
}

fn changed() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// A number that changes whenever the state does: read it every frame, and the state
/// only when it moved.
pub(crate) fn generation() -> u32 {
    GENERATION.load(Ordering::Acquire)
}

/// Pretend release `version` is out (world shots of the Update page); nothing
/// is fetched, and installing it would fail on its empty assets.
#[cfg(test)]
pub(crate) fn pretend_available(version: &str) {
    let asset = || Asset {
        name: String::new(),
        url: String::new(),
        size: 0,
    };
    set(State::Available(Release {
        version: version.to_owned(),
        page: String::new(),
        zip: asset(),
        sums: asset(),
    }));
}

/// The current state.
pub(crate) fn state() -> State {
    lock().clone()
}

/// The newer version waiting, and whether this folder must take it from the release
/// page ([`State::Manual`]).
pub(crate) fn available() -> Option<(String, bool)> {
    match &*lock() {
        State::Available(release) => Some((release.version.clone(), false)),
        State::Manual(release, _) => Some((release.version.clone(), true)),
        _ => None,
    }
}

/// The newer version waiting to be installed, if any.
pub(crate) fn available_version() -> Option<String> {
    match &*lock() {
        State::Available(release) | State::Manual(release, _) => Some(release.version.clone()),
        _ => None,
    }
}

/// A release's version as its text (`2026.1010.2`), kept inline so it is `Copy` and a
/// frame can carry it without allocating.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Version {
    bytes: [u8; Version::CAPACITY],
    len: u8,
}

impl Version {
    /// The longest version kept, in bytes.
    pub(crate) const CAPACITY: usize = 24;

    /// `text` when it is a release version ([`parse_version`] reads it) that fits.
    pub(crate) fn new(text: &str) -> Option<Self> {
        let text = text.trim();
        parse_version(text)?;
        if !text.is_ascii() || text.len() > Self::CAPACITY {
            return None;
        }
        let mut bytes = [0; Self::CAPACITY];
        bytes[..text.len()].copy_from_slice(text.as_bytes());
        Some(Self {
            bytes,
            len: text.len() as u8,
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

/// `2026.1005.1`, `sjk-v2026.1005.1` or `v2026.1005.1` as numbers; `None` for
/// anything else (a local build says `dev`).
pub(crate) fn parse_version(text: &str) -> Option<[u32; 3]> {
    let text = text.trim();
    let text = text.strip_prefix("sjk-").unwrap_or(text);
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.split('.');
    let version = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(version)
}

/// The release in the GitHub API's `releases/latest` answer, when it has this
/// platform's ZIP and checksums.
fn parse_latest(json: &str, platform: &str) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("unreadable answer: {error}"))?;
    let tag = value["tag_name"].as_str().ok_or("the answer has no tag")?;
    let version = tag.strip_prefix("sjk-v").unwrap_or(tag).to_owned();
    if parse_version(&version).is_none() {
        return Err(format!("the latest release, {tag}, has no version number"));
    }
    let assets = value["assets"].as_array().map_or(&[][..], Vec::as_slice);
    let find = |name: String| {
        assets.iter().find_map(|asset| {
            (asset["name"].as_str() == Some(&name)).then(|| Asset {
                url: asset["browser_download_url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                size: asset["size"].as_u64().unwrap_or(0),
                name: name.clone(),
            })
        })
    };
    let zip = find(format!("SJK-{version}-{platform}.zip"))
        .ok_or_else(|| format!("release {version} has no {platform} download"))?;
    let sums = find(format!("SJK-{version}-SHA256SUMS-{platform}.txt"))
        .ok_or_else(|| format!("release {version} has no checksums for {platform}"))?;
    if !zip.url.starts_with("https://") || !sums.url.starts_with("https://") {
        return Err("the release's files are not served over https".to_owned());
    }
    let page = value["html_url"]
        .as_str()
        .filter(|page| page.starts_with("https://"))
        .map_or_else(
            || format!("https://github.com/{REPOSITORY}/releases/latest"),
            str::to_owned,
        );
    Ok(Release {
        version,
        page,
        zip,
        sums,
    })
}

/// The hash `sums` lists for the file `name` (`<hash>  <name>` lines).
fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(format!("SJK/{}", crate::build_info::VERSION))
        .build()
        .into()
}

fn get(url: &str, timeout: Duration) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent(timeout)
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|error| format!("could not reach GitHub: {error}"))
}

/// Ask GitHub for the latest release and compare it with `installed` (a
/// version, or `dev` for a local build, which only reports it cannot compare).
pub(crate) fn check(installed: &str) {
    let installed = installed.to_owned();
    {
        let mut state = lock();
        if matches!(
            *state,
            State::Checking | State::Downloading { .. } | State::Installing
        ) {
            return;
        }
        *state = State::Checking;
    }
    *LAST_CHECK.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    changed();
    std::thread::spawn(move || set(check_now(&installed)));
}

/// Check again when one was made at least [`RECHECK`] ago and found nothing new or
/// failed; a release found, being installed or installed is left as it is.
pub(crate) fn recheck_if_due(installed: &str, now: Instant) {
    let last = *LAST_CHECK.lock().unwrap_or_else(|e| e.into_inner());
    if !due(last, &lock(), now) {
        return;
    }
    check(installed);
}

/// Whether a check made at `last` that ended in `state` should be made again at `now`.
fn due(last: Option<Instant>, state: &State, now: Instant) -> bool {
    matches!(state, State::UpToDate | State::Failed(_))
        && last.is_some_and(|last| now.saturating_duration_since(last) >= RECHECK)
}

fn check_now(installed: &str) -> State {
    let Some(installed) = parse_version(installed) else {
        return State::Unversioned;
    };
    let Some(platform) = PLATFORM else {
        return State::Failed("there is no SJK release for this system".to_owned());
    };
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let json = match get(&url, LOOKUP_TIMEOUT).and_then(|mut response| {
        response
            .body_mut()
            .read_to_string()
            .map_err(|error| format!("could not read the answer: {error}"))
    }) {
        Ok(json) => json,
        Err(error) => return State::Failed(error),
    };
    match parse_latest(&json, platform) {
        Ok(release) if parse_version(&release.version) > Some(installed) => {
            match install_directory().and_then(|directory| writable(&directory)) {
                Ok(()) => State::Available(release),
                Err(reason) => State::Manual(release, reason),
            }
        }
        Ok(_) => State::UpToDate,
        Err(error) => State::Failed(error),
    }
}

/// The folder of the running program, where the update's files go.
fn install_directory() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("no program path: {error}"))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "no program folder".to_owned())
}

/// Whether files can be created in `directory`.
fn writable(directory: &Path) -> Result<(), String> {
    let probe = directory.join(".sjk-update-probe");
    std::fs::write(&probe, b"")
        .map_err(|error| format!("{} is not writable ({error})", directory.display()))?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

/// Download, verify and install the available release.
pub(crate) fn install() {
    let release = {
        let mut state = lock();
        let State::Available(release) = &*state else {
            return;
        };
        let release = release.clone();
        *state = State::Downloading {
            done: 0,
            total: release.zip.size,
        };
        release
    };
    changed();
    std::thread::spawn(move || {
        set(match install_now(&release) {
            Ok(()) => {
                RESTART.store(true, Ordering::SeqCst);
                State::Installed
            }
            Err(error) => State::Failed(error),
        })
    });
}

fn install_now(release: &Release) -> Result<(), String> {
    let directory = install_directory()?;
    let sums = get(&release.sums.url, LOOKUP_TIMEOUT)?
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("could not read the checksums: {error}"))?;
    let expected = checksum_for(&sums, &release.zip.name)
        .ok_or_else(|| format!("the checksums do not list {}", release.zip.name))?;
    let staging = directory.join(format!(".sjk-update-{}", release.version));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|error| format!("cannot stage the update: {error}"))?;
    let result = download(release, &staging, &expected).and_then(|zip| {
        set(State::Installing);
        install_zip(&zip, &directory)
    });
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Stream the release ZIP into `staging`, refusing it unless its SHA-256 is
/// `expected`.
fn download(release: &Release, staging: &Path, expected: &str) -> Result<PathBuf, String> {
    let mut response = get(&release.zip.url, DOWNLOAD_TIMEOUT)?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(release.zip.size);
    let path = staging.join(&release.zip.name);
    let mut file = std::fs::File::create(&path)
        .map_err(|error| format!("cannot write the download: {error}"))?;
    let mut reader = response.body_mut().as_reader();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut done = 0_u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("the download broke off: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| format!("cannot write the download: {error}"))?;
        done += read as u64;
        set(State::Downloading { done, total });
    }
    file.flush()
        .map_err(|error| format!("cannot write the download: {error}"))?;
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        return Err("the download does not match its checksum; nothing was installed".to_owned());
    }
    Ok(path)
}

/// A name taken from the ZIP: one plain file name, no folders.
fn flat_name(name: &str) -> Option<&str> {
    let plain = !name.is_empty()
        && !name.contains(['/', '\\', ':'])
        && !name.starts_with('.')
        && !name.ends_with(".old")
        && !name.ends_with(".new");
    plain.then_some(name)
}

/// Put the ZIP's files in `directory`. Each is written as `<name>.new` first;
/// then the running files are renamed to `<name>.old` (a program cannot be
/// overwritten while it runs on Windows, but it can be renamed) and the new ones
/// renamed into place. A failure puts the old files back.
fn install_zip(zip: &Path, directory: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|error| format!("cannot open the ZIP: {error}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("unreadable ZIP: {error}"))?;
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("unreadable ZIP: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = flat_name(entry.name())
            .ok_or_else(|| format!("unexpected file in the ZIP: {}", entry.name()))?
            .to_owned();
        let target = directory.join(format!("{name}.new"));
        let mut out = std::fs::File::create(&target)
            .map_err(|error| format!("cannot write {name}: {error}"))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|error| format!("cannot write {name}: {error}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = entry.unix_mode().unwrap_or(0o644) & 0o777;
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode));
        }
        names.push(name);
    }
    if names.is_empty() {
        return Err("the ZIP is empty".to_owned());
    }
    let mut moved: Vec<&String> = Vec::new();
    let mut failure = None;
    for name in &names {
        let current = directory.join(name);
        let old = directory.join(format!("{name}.old"));
        let _ = std::fs::remove_file(&old);
        let swapped = (!current.exists() || std::fs::rename(&current, &old).is_ok())
            && std::fs::rename(directory.join(format!("{name}.new")), &current).is_ok();
        if swapped {
            moved.push(name);
        } else {
            // Undo this one's first half, then the earlier ones below.
            if !current.exists() {
                let _ = std::fs::rename(&old, &current);
            }
            failure = Some(format!("cannot replace {name}"));
            break;
        }
    }
    if let Some(error) = failure {
        for name in moved {
            let current = directory.join(name);
            let _ = std::fs::remove_file(&current);
            let _ = std::fs::rename(directory.join(format!("{name}.old")), &current);
        }
        for name in &names {
            let _ = std::fs::remove_file(directory.join(format!("{name}.new")));
        }
        return Err(error);
    }
    Ok(())
}

/// Delete the `.old` programs a previous update left beside this one.
pub(crate) fn clean_up_old_files() {
    let Ok(directory) = install_directory() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".old") && name.to_ascii_lowercase().contains("sjk") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    // A staging folder left by a crash.
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".sjk-update-")
            {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

/// Start the updated program when the client exits after an install, with the
/// arguments this one had. Call it once the event loop has ended.
pub(crate) fn restart_if_requested() {
    if !RESTART.swap(false, Ordering::SeqCst) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .spawn();
}

/// Open `url` in the player's browser. Only https addresses are opened.
pub(crate) fn open_page(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    let _ = if cfg!(target_os = "windows") {
        // `start` reads its first quoted argument as a window title.
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_client_asks_again_only_after_nothing_new() {
        let start = Instant::now();
        let later = start + RECHECK;
        assert!(due(Some(start), &State::UpToDate, later));
        assert!(due(Some(start), &State::Failed("offline".into()), later));
        assert!(!due(Some(start), &State::UpToDate, start + RECHECK / 2));
        // Never asked (cl_autoUpdate off at start), or asking now: not yet.
        assert!(!due(None, &State::UpToDate, later));
        assert!(!due(Some(start), &State::Checking, later));
        // A release found or installed stays as it is.
        assert!(!due(Some(start), &State::Installed, later));
        assert!(!due(Some(start), &State::Unversioned, later));
    }

    #[test]
    fn versions_compare_by_date_then_counter() {
        assert_eq!(parse_version("2026.1005.1"), Some([2026, 1005, 1]));
        assert_eq!(parse_version("sjk-v2026.1005.2"), Some([2026, 1005, 2]));
        assert_eq!(parse_version("dev"), None);
        assert_eq!(parse_version("2026.1005"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert!(parse_version("2026.1010.1") > parse_version("2026.1005.9"));
        assert!(parse_version("2026.1005.10") > parse_version("2026.1005.9"));
        assert!(parse_version("2027.0101.1") > parse_version("2026.1231.3"));
    }

    #[test]
    fn a_version_is_kept_as_its_text() {
        let version = Version::new(" 2026.1010.2 ").expect("a version");
        assert_eq!(version.as_str(), "2026.1010.2");
        assert_eq!(Version::new("2026.1010.2"), Some(version));
        assert_ne!(Version::new("2026.1010.3"), Some(version));
        assert_eq!(Version::new("dev"), None);
        assert!(parse_version("2026.1010.00000000000000000002").is_some());
        assert_eq!(
            Version::new("2026.1010.00000000000000000002"),
            None,
            "too long"
        );
    }

    const ANSWER: &str = r#"{
        "tag_name": "sjk-v2026.1010.1",
        "html_url": "https://github.com/Sol-Vulpes/SJK/releases/tag/sjk-v2026.1010.1",
        "assets": [
            {"name": "SJK-2026.1010.1-windows-x64.zip", "size": 100,
             "browser_download_url": "https://github.com/x/win.zip"},
            {"name": "SJK-2026.1010.1-SHA256SUMS-windows-x64.txt", "size": 10,
             "browser_download_url": "https://github.com/x/win.txt"},
            {"name": "SJK-2026.1010.1-linux-x64.zip", "size": 90,
             "browser_download_url": "https://github.com/x/lin.zip"}
        ]
    }"#;

    #[test]
    fn the_latest_release_needs_its_zip_and_checksums() {
        let release = parse_latest(ANSWER, "windows-x64").unwrap();
        assert_eq!(release.version, "2026.1010.1");
        assert_eq!(release.zip.size, 100);
        assert_eq!(release.sums.url, "https://github.com/x/win.txt");
        assert!(
            parse_latest(ANSWER, "linux-x64")
                .unwrap_err()
                .contains("checksums")
        );
        assert!(
            parse_latest(ANSWER, "macos")
                .unwrap_err()
                .contains("download")
        );
        assert!(parse_latest("{}", "windows-x64").is_err());
        let insecure = ANSWER.replace("https://github.com/x/win.zip", "http://example.com/win.zip");
        assert!(parse_latest(&insecure, "windows-x64").is_err());
    }

    #[test]
    fn checksums_are_matched_by_file_name() {
        let hash = "a".repeat(64);
        let sums = format!(
            "{hash}  SJK-1-windows-x64.zip\n{}  other.zip\n",
            "b".repeat(64)
        );
        assert_eq!(checksum_for(&sums, "SJK-1-windows-x64.zip"), Some(hash));
        assert_eq!(checksum_for(&sums, "missing.zip"), None);
        assert_eq!(checksum_for("short  file.zip", "file.zip"), None);
    }

    #[test]
    fn only_plain_file_names_are_taken_from_the_zip() {
        assert_eq!(flat_name("sjk.exe"), Some("sjk.exe"));
        assert_eq!(flat_name("README-SJK.txt"), Some("README-SJK.txt"));
        for bad in [
            "../sjk.exe",
            "base/x.pk3",
            "a\\b.exe",
            "C:x.exe",
            ".hidden",
            "sjk.exe.old",
            "",
        ] {
            assert_eq!(flat_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn installing_replaces_files_and_keeps_the_old_ones_aside() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("sjk.exe"), b"old").unwrap();
        let zip_path = root.join("update.zip");
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("sjk.exe", options).unwrap();
            writer.write_all(b"new").unwrap();
            writer.start_file("README-SJK.txt", options).unwrap();
            writer.write_all(b"readme").unwrap();
            writer.finish().unwrap();
        }
        install_zip(&zip_path, root).unwrap();
        assert_eq!(std::fs::read(root.join("sjk.exe")).unwrap(), b"new");
        assert_eq!(std::fs::read(root.join("sjk.exe.old")).unwrap(), b"old");
        assert_eq!(
            std::fs::read(root.join("README-SJK.txt")).unwrap(),
            b"readme"
        );
        assert!(!root.join("sjk.exe.new").exists());
    }

    #[test]
    fn a_zip_with_a_folder_path_installs_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(root.join("sjk.exe"), b"old").unwrap();
        let zip_path = root.join("update.zip");
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("../evil.exe", options).unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        assert!(install_zip(&zip_path, root).is_err());
        assert_eq!(std::fs::read(root.join("sjk.exe")).unwrap(), b"old");
    }
}

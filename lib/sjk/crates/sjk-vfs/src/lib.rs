//! Host-independent virtual filesystem for loose JKA assets and PK3 archives.

mod cache_identity;
mod path;
mod pk3_directory;
mod pk3_fingerprint;
pub use cache_identity::AssetCacheIdentity;
pub use pk3_directory::pk3_search_order;

pub use path::{VirtualPath, VirtualPathError};

use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use md4::{Digest, Md4};

const DEFAULT_MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MountId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetSource {
    pub mount_id: MountId,
    pub mount_name: Arc<str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Asset {
    pub bytes: Vec<u8>,
    pub source: AssetSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountSummary {
    pub id: MountId,
    pub name: Arc<str>,
    pub entries: usize,
}

/// CRC catalogue used by the id Tech 3 pure-server proof.
///
/// The ordinary checksum is feed-independent. The pure checksum prepends the
/// server's per-gamestate feed before hashing the same ZIP entry CRC sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pk3Fingerprint {
    entry_crcs: Vec<u32>,
}

impl Pk3Fingerprint {
    /// Read ordered, nonempty ZIP-entry CRCs from the validated central directory.
    pub fn open(archive_path: impl AsRef<Path>) -> Result<Self, VfsError> {
        let archive_path = archive_path.as_ref();
        let file = File::open(archive_path).map_err(|source| VfsError::Io {
            operation: "open PK3 for checksum",
            path: archive_path.to_owned(),
            source,
        })?;
        let archive = zip::ZipArchive::new(file).map_err(|source| VfsError::Zip {
            path: archive_path.to_owned(),
            source,
        })?;
        let entry_crcs = pk3_fingerprint::catalog(archive).map_err(|source| VfsError::Io {
            operation: "read PK3 checksum catalogue",
            path: archive_path.to_owned(),
            source,
        })?;
        Ok(Self { entry_crcs })
    }

    pub fn checksum(&self) -> i32 {
        block_checksum(self.entry_crcs.iter().copied())
    }

    pub fn pure_checksum(&self, checksum_feed: i32) -> i32 {
        block_checksum(std::iter::once(checksum_feed as u32).chain(self.entry_crcs.iter().copied()))
    }

    pub fn entry_count(&self) -> usize {
        self.entry_crcs.len()
    }
}

/// `Com_BlockChecksum` over a file's bytes (`qcommon/md4.cpp`): the four words of its MD4
/// digest XORed together — a map's `sv_mapChecksum` (`CM_LoadMap`) for its `.bsp`.
pub fn file_checksum(bytes: &[u8]) -> i32 {
    let digest = Md4::digest(bytes);
    digest
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("MD4 word is four bytes")))
        .fold(0_u32, std::ops::BitXor::bitxor) as i32
}

fn block_checksum(values: impl IntoIterator<Item = u32>) -> i32 {
    let mut hasher = Md4::new();
    for value in values {
        hasher.update(value.to_le_bytes());
    }
    let digest = hasher.finalize();
    digest
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("MD4 word is four bytes")))
        .fold(0_u32, std::ops::BitXor::bitxor) as i32
}

/// Ordered asset mounts. The most recently mounted source has highest priority.
#[derive(Clone)]
pub struct VirtualFileSystem {
    mounts: Vec<MountedSource>,
    next_mount_id: u64,
    max_asset_bytes: u64,
    read_diagnostics: bool,
}

impl Default for VirtualFileSystem {
    fn default() -> Self {
        Self {
            mounts: Vec::new(),
            next_mount_id: 0,
            max_asset_bytes: DEFAULT_MAX_ASSET_BYTES,
            read_diagnostics: false,
        }
    }
}

impl VirtualFileSystem {
    /// Enable source-resolution diagnostics on asset reads, disabled by default.
    pub fn set_read_diagnostics(&mut self, enabled: bool) {
        self.read_diagnostics = enabled;
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_asset_bytes(max_asset_bytes: u64) -> Self {
        Self {
            max_asset_bytes,
            ..Self::default()
        }
    }

    pub fn mount_directory(&mut self, root: impl AsRef<Path>) -> Result<MountId, VfsError> {
        let source = DirectorySource::scan(root.as_ref())?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    pub fn mount_pk3(&mut self, archive: impl AsRef<Path>) -> Result<MountId, VfsError> {
        let source = Pk3Source::open(archive.as_ref(), |_| true)?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    /// Mounts an archive with only the entries `keep` accepts: the others are
    /// not read, listed or counted, as if the archive lacked them. The file is
    /// not changed, so its checksums (and a pure server's proof) stay the same.
    pub fn mount_pk3_filtered(
        &mut self,
        archive: impl AsRef<Path>,
        keep: impl FnMut(&VirtualPath) -> bool,
    ) -> Result<MountId, VfsError> {
        let source = Pk3Source::open(archive.as_ref(), keep)?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    /// Mounts an in-memory collection using the same normalized path and
    /// precedence rules as directory and PK3 mounts.
    ///
    /// This is useful to embedders and to deterministic tests which should not
    /// need host temporary files merely to exercise VFS-backed adapters.
    pub fn mount_memory<I, P, B>(
        &mut self,
        name: impl Into<Arc<str>>,
        entries: I,
    ) -> Result<MountId, VfsError>
    where
        I: IntoIterator<Item = (P, B)>,
        P: AsRef<str>,
        B: Into<Vec<u8>>,
    {
        let source = MemorySource::new(name.into(), entries)?;
        self.add_mount(source.name.clone(), Arc::new(source))
    }

    /// Read `path` from the highest-priority mount that has it. Like
    /// `FS_FOpenFileRead`, one leading slash is ignored ([`VirtualPath::for_read`]).
    pub fn read(&self, path: &str) -> Result<Option<Asset>, VfsError> {
        let path = VirtualPath::for_read(path)?;

        for mount in self.mounts.iter().rev() {
            if let Some(bytes) = mount.source.read(&path, self.max_asset_bytes)? {
                if self.read_diagnostics {
                    eprintln!(
                        "asset read {path}: {} bytes from {}",
                        bytes.len(),
                        mount.name
                    );
                }
                return Ok(Some(Asset {
                    bytes,
                    source: AssetSource {
                        mount_id: mount.id,
                        mount_name: Arc::clone(&mount.name),
                    },
                }));
            }
        }

        if self.read_diagnostics {
            eprintln!("asset read {path}: not found");
        }
        Ok(None)
    }

    pub fn contains(&self, path: &str) -> Result<bool, VfsError> {
        let path = VirtualPath::for_read(path)?;
        Ok(self
            .mounts
            .iter()
            .rev()
            .any(|mount| mount.source.contains(&path)))
    }

    /// Whether the mount `id` holds `path`, without reading it; false for a missing mount.
    pub fn mount_contains(&self, id: MountId, path: &str) -> Result<bool, VfsError> {
        let path = VirtualPath::for_read(path)?;
        Ok(self
            .mounts
            .iter()
            .find(|mount| mount.id == id)
            .is_some_and(|mount| mount.source.contains(&path)))
    }

    /// Reads from one explicitly selected mount, without changing normal search precedence.
    /// Missing mounts or paths return `None`; normal asset size limits still apply.
    pub fn read_from_mount(&self, id: MountId, path: &str) -> Result<Option<Asset>, VfsError> {
        let path = VirtualPath::for_read(path)?;
        let Some(mount) = self.mounts.iter().find(|mount| mount.id == id) else {
            return Ok(None);
        };
        Ok(mount
            .source
            .read(&path, self.max_asset_bytes)?
            .map(|bytes| Asset {
                bytes,
                source: AssetSource {
                    mount_id: mount.id,
                    mount_name: Arc::clone(&mount.name),
                },
            }))
    }

    /// This file system without the mounts in `hidden`: what reads would see had
    /// they never been mounted. The other sources are shared, not reopened, and
    /// keep their cache identities.
    pub fn without_mounts(&self, hidden: &[MountId]) -> Self {
        Self {
            mounts: self
                .mounts
                .iter()
                .filter(|mount| !hidden.contains(&mount.id))
                .cloned()
                .collect(),
            next_mount_id: self.next_mount_id,
            max_asset_bytes: self.max_asset_bytes,
            read_diagnostics: self.read_diagnostics,
        }
    }

    /// This file system over `lower`'s mounts: what reads would see had `lower`'s
    /// mounts been mounted first, below every one of this one's. The sources are
    /// shared, not reopened; `lower`'s mounts get new ids in the result.
    pub fn with_lower(&self, lower: &Self) -> Result<Self, VfsError> {
        let mut next_mount_id = self.next_mount_id;
        let mut mounts = Vec::with_capacity(lower.mounts.len() + self.mounts.len());
        for mount in &lower.mounts {
            mounts.push(MountedSource {
                id: MountId(next_mount_id),
                ..mount.clone()
            });
            next_mount_id = next_mount_id
                .checked_add(1)
                .ok_or(VfsError::TooManyMounts)?;
        }
        mounts.extend(self.mounts.iter().cloned());
        Ok(Self {
            mounts,
            next_mount_id,
            max_asset_bytes: self.max_asset_bytes,
            read_diagnostics: self.read_diagnostics,
        })
    }

    pub fn mounts(&self) -> impl DoubleEndedIterator<Item = MountSummary> + '_ {
        self.mounts.iter().map(|mount| MountSummary {
            id: mount.id,
            name: Arc::clone(&mount.name),
            entries: mount.source.entry_count(),
        })
    }

    /// `FS_ListFilteredFiles` for a directory: the names, relative to `directory`, of
    /// the files in it (or one directory below) ending in `extension`. The
    /// highest-priority source comes first, each source in its own order (an archive
    /// in its central directory's), and a name only the first time it is met — the
    /// order a game reading "every file of a kind" joins them in.
    pub fn list_files(&self, directory: &str, extension: &str) -> Vec<String> {
        let prefix = format!("{}/", directory.trim_end_matches('/').to_ascii_lowercase());
        let extension = extension.to_ascii_lowercase();
        let mut listed: Vec<String> = Vec::new();
        let mut paths = Vec::new();
        for mount in self.mounts.iter().rev() {
            paths.clear();
            mount.source.append_in_order(&mut paths);
            for path in &paths {
                let Some(name) = path.as_str().strip_prefix(&prefix) else {
                    continue;
                };
                if name.matches('/').count() > 1
                    || !name.ends_with(&extension)
                    || listed.iter().any(|known| known == name)
                {
                    continue;
                }
                listed.push(name.to_owned());
            }
        }
        listed
    }

    /// The name `path` is stored under in the highest-priority source that
    /// holds it, with its original letter case (paths are matched without
    /// case, so [`Self::list_files`] gives them in lowercase): the archive's
    /// entry name, the loose file's relative path on disk, or the name a
    /// memory mount was given. Separators are `/`.
    pub fn original_name(&self, path: &str) -> Option<String> {
        let path = VirtualPath::new(path).ok()?;
        self.mounts
            .iter()
            .rev()
            .find(|mount| mount.source.contains(&path))
            .and_then(|mount| mount.source.original_name(&path))
    }

    /// Returns the normalized union of paths visible across all mounts.
    pub fn paths(&self) -> Vec<VirtualPath> {
        let mut paths = BTreeSet::new();
        for mount in &self.mounts {
            mount.source.append_paths(&mut paths);
        }
        paths.into_iter().collect()
    }

    fn add_mount(
        &mut self,
        name: Arc<str>,
        source: Arc<dyn AssetSourceBackend>,
    ) -> Result<MountId, VfsError> {
        let id = MountId(self.next_mount_id);
        self.next_mount_id = self
            .next_mount_id
            .checked_add(1)
            .ok_or(VfsError::TooManyMounts)?;
        self.mounts.push(MountedSource {
            id,
            name,
            source,
            cache_identity: cache_identity::next(),
        });
        Ok(id)
    }
}

#[derive(Clone)]
struct MountedSource {
    id: MountId,
    name: Arc<str>,
    source: Arc<dyn AssetSourceBackend>,
    cache_identity: u64,
}

trait AssetSourceBackend: Send + Sync {
    fn cacheable(&self) -> bool {
        false
    }
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError>;
    fn contains(&self, path: &VirtualPath) -> bool;
    fn entry_count(&self) -> usize;
    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>);
    /// The name `path` has in this source, letter case kept.
    fn original_name(&self, path: &VirtualPath) -> Option<String>;
    /// Every path in the source's own order: an archive's central directory, else sorted.
    fn append_in_order(&self, output: &mut Vec<VirtualPath>) {
        let mut paths = BTreeSet::new();
        self.append_paths(&mut paths);
        output.extend(paths);
    }
}

struct MemorySource {
    name: Arc<str>,
    files: HashMap<VirtualPath, Vec<u8>>,
    originals: HashMap<VirtualPath, String>,
}

impl MemorySource {
    fn new<I, P, B>(name: Arc<str>, entries: I) -> Result<Self, VfsError>
    where
        I: IntoIterator<Item = (P, B)>,
        P: AsRef<str>,
        B: Into<Vec<u8>>,
    {
        let mut files = HashMap::new();
        let mut originals = HashMap::new();
        for (path, bytes) in entries {
            let original = path.as_ref().replace('\\', "/");
            let path = VirtualPath::new(path.as_ref())?;
            originals.insert(path.clone(), original);
            files.insert(path, bytes.into());
        }
        Ok(Self {
            name,
            files,
            originals,
        })
    }
}

impl AssetSourceBackend for MemorySource {
    fn cacheable(&self) -> bool {
        true
    }
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError> {
        let Some(bytes) = self.files.get(path) else {
            return Ok(None);
        };
        ensure_size(path, bytes.len() as u64, max_bytes)?;
        Ok(Some(bytes.clone()))
    }

    fn contains(&self, path: &VirtualPath) -> bool {
        self.files.contains_key(path)
    }

    fn entry_count(&self) -> usize {
        self.files.len()
    }

    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>) {
        output.extend(self.files.keys().cloned());
    }

    fn original_name(&self, path: &VirtualPath) -> Option<String> {
        self.originals.get(path).cloned()
    }
}

struct DirectorySource {
    name: Arc<str>,
    files: HashMap<VirtualPath, PathBuf>,
}

impl DirectorySource {
    fn scan(root: &Path) -> Result<Self, VfsError> {
        let root = root.canonicalize().map_err(|source| VfsError::Io {
            operation: "canonicalize directory mount",
            path: root.to_owned(),
            source,
        })?;
        if !root.is_dir() {
            return Err(VfsError::NotDirectory(root));
        }

        let mut files = HashMap::new();
        let mut pending = vec![root.clone()];
        while let Some(directory) = pending.pop() {
            let entries = fs::read_dir(&directory).map_err(|source| VfsError::Io {
                operation: "scan directory mount",
                path: directory.clone(),
                source,
            })?;

            for entry in entries {
                let entry = entry.map_err(|source| VfsError::Io {
                    operation: "read directory entry",
                    path: directory.clone(),
                    source,
                })?;
                let file_type = entry.file_type().map_err(|source| VfsError::Io {
                    operation: "inspect directory entry",
                    path: entry.path(),
                    source,
                })?;

                if file_type.is_dir() {
                    pending.push(entry.path());
                    continue;
                }

                if !file_type.is_file() {
                    // Symlinks and device files are deliberately not exposed.
                    continue;
                }

                let entry_path = entry.path();
                let relative =
                    entry_path
                        .strip_prefix(&root)
                        .map_err(|_| VfsError::EscapedDirectoryRoot {
                            root: root.clone(),
                            path: entry_path.clone(),
                        })?;
                let virtual_path = VirtualPath::from_host_relative(relative)?;
                if let Some(previous) = files.insert(virtual_path.clone(), entry_path.clone()) {
                    return Err(VfsError::CaseCollision {
                        virtual_path,
                        first: previous,
                        second: entry_path,
                    });
                }
            }
        }

        let name: Arc<str> = root.to_string_lossy().into_owned().into();
        Ok(Self { name, files })
    }
}

impl AssetSourceBackend for DirectorySource {
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError> {
        let Some(host_path) = self.files.get(path) else {
            return Ok(None);
        };
        let metadata = host_path.metadata().map_err(|source| VfsError::Io {
            operation: "inspect loose asset",
            path: host_path.clone(),
            source,
        })?;
        ensure_size(path, metadata.len(), max_bytes)?;
        fs::read(host_path)
            .map(Some)
            .map_err(|source| VfsError::Io {
                operation: "read loose asset",
                path: host_path.clone(),
                source,
            })
    }

    fn contains(&self, path: &VirtualPath) -> bool {
        self.files.contains_key(path)
    }

    fn entry_count(&self) -> usize {
        self.files.len()
    }

    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>) {
        output.extend(self.files.keys().cloned());
    }

    fn original_name(&self, path: &VirtualPath) -> Option<String> {
        // The host path ends in as many components as the virtual path has.
        let host = self.files.get(path)?;
        let depth = path.as_str().split('/').count();
        let mut components: Vec<&str> = host
            .components()
            .rev()
            .take(depth)
            .filter_map(|component| component.as_os_str().to_str())
            .collect();
        components.reverse();
        Some(components.join("/"))
    }
}

struct Pk3Source {
    name: Arc<str>,
    archive_path: PathBuf,
    entries: HashMap<VirtualPath, usize>,
    archive: Mutex<zip::ZipArchive<File>>,
}

impl Pk3Source {
    fn open(
        archive_path: &Path,
        mut keep: impl FnMut(&VirtualPath) -> bool,
    ) -> Result<Self, VfsError> {
        let archive_path = archive_path.canonicalize().map_err(|source| VfsError::Io {
            operation: "canonicalize PK3",
            path: archive_path.to_owned(),
            source,
        })?;
        let file = File::open(&archive_path).map_err(|source| VfsError::Io {
            operation: "open PK3",
            path: archive_path.clone(),
            source,
        })?;
        let archive = zip::ZipArchive::new(file).map_err(|source| VfsError::Zip {
            path: archive_path.clone(),
            source,
        })?;
        let mut entries = HashMap::new();

        // Like OpenJK codemp FS_LoadZipFile, mount from the central directory.
        // Opening each local header here makes an unused broken map prevent
        // startup. Payload/header validation belongs in read(). file_names()
        // and by_index() use the same central-directory index order.
        for (index, name) in archive.file_names().enumerate() {
            if name.ends_with(['/', '\\']) {
                continue;
            }

            let virtual_path =
                VirtualPath::new(name).map_err(|source| VfsError::InvalidArchivePath {
                    archive: archive_path.clone(),
                    entry: name.to_owned(),
                    source,
                })?;
            if !keep(&virtual_path) {
                continue;
            }
            // A later entry wins, matching the VFS mount precedence rule.
            entries.insert(virtual_path, index);
        }

        let name: Arc<str> = archive_path.to_string_lossy().into_owned().into();
        Ok(Self {
            name,
            archive_path,
            entries,
            archive: Mutex::new(archive),
        })
    }
}

impl AssetSourceBackend for Pk3Source {
    fn cacheable(&self) -> bool {
        true
    }
    fn read(&self, path: &VirtualPath, max_bytes: u64) -> Result<Option<Vec<u8>>, VfsError> {
        let Some(index) = self.entries.get(path).copied() else {
            return Ok(None);
        };
        let mut archive = self.archive.lock().map_err(|_| VfsError::Io {
            operation: "lock PK3",
            path: self.archive_path.clone(),
            source: io::Error::other("PK3 reader lock was poisoned"),
        })?;
        let mut entry = archive.by_index(index).map_err(|source| VfsError::Zip {
            path: self.archive_path.clone(),
            source,
        })?;
        ensure_size(path, entry.size(), max_bytes)?;

        let mut bytes = Vec::with_capacity(entry.size().min(usize::MAX as u64) as usize);
        entry
            .by_ref()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| VfsError::Io {
                operation: "decompress PK3 asset",
                path: self.archive_path.clone(),
                source,
            })?;
        ensure_size(path, bytes.len() as u64, max_bytes)?;
        Ok(Some(bytes))
    }

    fn contains(&self, path: &VirtualPath) -> bool {
        self.entries.contains_key(path)
    }

    fn entry_count(&self) -> usize {
        self.entries.len()
    }

    fn append_paths(&self, output: &mut BTreeSet<VirtualPath>) {
        output.extend(self.entries.keys().cloned());
    }

    fn original_name(&self, path: &VirtualPath) -> Option<String> {
        let index = self.entries.get(path).copied()?;
        let archive = self.archive.lock().ok()?;
        archive
            .name_for_index(index)
            .map(|name| name.replace('\\', "/"))
    }

    fn append_in_order(&self, output: &mut Vec<VirtualPath>) {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_unstable_by_key(|(_, index)| **index);
        output.extend(entries.into_iter().map(|(path, _)| path.clone()));
    }
}

fn ensure_size(path: &VirtualPath, actual: u64, maximum: u64) -> Result<(), VfsError> {
    if actual > maximum {
        return Err(VfsError::AssetTooLarge {
            path: path.clone(),
            actual,
            maximum,
        });
    }
    Ok(())
}

#[derive(Debug)]
pub enum VfsError {
    VirtualPath(VirtualPathError),
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Zip {
        path: PathBuf,
        source: zip::result::ZipError,
    },
    InvalidArchivePath {
        archive: PathBuf,
        entry: String,
        source: VirtualPathError,
    },
    NotDirectory(PathBuf),
    EscapedDirectoryRoot {
        root: PathBuf,
        path: PathBuf,
    },
    CaseCollision {
        virtual_path: VirtualPath,
        first: PathBuf,
        second: PathBuf,
    },
    AssetTooLarge {
        path: VirtualPath,
        actual: u64,
        maximum: u64,
    },
    TooManyMounts,
}

impl fmt::Display for VfsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VirtualPath(error) => error.fmt(formatter),
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} {}: {source}",
                path.display()
            ),
            Self::Zip { path, source } => {
                write!(
                    formatter,
                    "invalid PK3 archive {}: {source}",
                    path.display()
                )
            }
            Self::InvalidArchivePath {
                archive,
                entry,
                source,
            } => write!(
                formatter,
                "unsafe entry {entry:?} in PK3 {}: {source}",
                archive.display()
            ),
            Self::NotDirectory(path) => {
                write!(
                    formatter,
                    "directory mount is not a directory: {}",
                    path.display()
                )
            }
            Self::EscapedDirectoryRoot { root, path } => write!(
                formatter,
                "asset {} escaped directory mount {}",
                path.display(),
                root.display()
            ),
            Self::CaseCollision {
                virtual_path,
                first,
                second,
            } => write!(
                formatter,
                "case-insensitive path collision for {virtual_path}: {} and {}",
                first.display(),
                second.display()
            ),
            Self::AssetTooLarge {
                path,
                actual,
                maximum,
            } => write!(
                formatter,
                "asset {path} is {actual} bytes, exceeding the {maximum}-byte limit"
            ),
            Self::TooManyMounts => formatter.write_str("virtual filesystem mount ID overflow"),
        }
    }
}

impl Error for VfsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::VirtualPath(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Zip { source, .. } => Some(source),
            Self::InvalidArchivePath { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<VirtualPathError> for VfsError {
    fn from(value: VirtualPathError) -> Self {
        Self::VirtualPath(value)
    }
}

#[cfg(test)]
mod original_name_tests {
    use super::*;

    #[test]
    fn listed_names_are_lowercase_but_the_original_case_is_kept() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "base",
            [("forcecfg/light/^2Side Mission.fcf", b"7-1-0".to_vec())],
        )
        .unwrap();
        assert_eq!(
            vfs.list_files("forcecfg/light", ".fcf"),
            ["^2side mission.fcf"]
        );
        assert_eq!(
            vfs.original_name("FORCECFG/light/^2side mission.fcf")
                .as_deref(),
            Some("forcecfg/light/^2Side Mission.fcf")
        );
        assert_eq!(vfs.original_name("forcecfg/light/none.fcf"), None);
    }
}

#[cfg(test)]
mod without_mounts_tests {
    use super::*;

    #[test]
    fn lower_mounts_are_read_only_where_nothing_above_has_the_path() {
        let mut vfs = VirtualFileSystem::new();
        let base = vfs
            .mount_memory("base", [("ui/hud.menu", b"base".to_vec())])
            .unwrap();
        let mut lower = VirtualFileSystem::new();
        lower
            .mount_memory(
                "pack",
                [
                    ("ui/hud.menu", b"pack".to_vec()),
                    ("sound/new.wav", b"new".to_vec()),
                ],
            )
            .unwrap();
        let both = vfs.with_lower(&lower).unwrap();
        let read = |path| both.read(path).unwrap().unwrap().bytes;
        assert_eq!(read("ui/hud.menu"), b"base");
        assert_eq!(read("sound/new.wav"), b"new");
        let ids: Vec<_> = both.mounts().map(|mount| mount.id).collect();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[1], base);
        assert_ne!(ids[0], base, "the lower mount gets an id of its own");
        assert!(vfs.read("sound/new.wav").unwrap().is_none());
    }

    #[test]
    fn hidden_mounts_are_skipped_and_the_rest_keep_their_order() {
        let mut vfs = VirtualFileSystem::new();
        let base = vfs
            .mount_memory("base", [("ui/hud.menu", b"base".to_vec())])
            .unwrap();
        let pack = vfs
            .mount_memory("pack", [("ui/hud.menu", b"pack".to_vec())])
            .unwrap();
        let read = |vfs: &VirtualFileSystem| vfs.read("ui/hud.menu").unwrap().unwrap().bytes;
        assert_eq!(read(&vfs), b"pack");
        let view = vfs.without_mounts(&[pack]);
        assert_eq!(read(&view), b"base");
        assert_eq!(
            view.mounts().map(|mount| mount.id).collect::<Vec<_>>(),
            [base]
        );
        assert_eq!(read(&vfs), b"pack", "the original keeps every mount");
        assert!(
            vfs.without_mounts(&[base, pack])
                .read("ui/hud.menu")
                .unwrap()
                .is_none()
        );
    }
}

#[cfg(test)]
mod read_path_tests {
    use super::*;

    #[test]
    fn a_leading_slash_reads_the_mounted_asset() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory("test", [("models/items/a.md3", b"md3".to_vec())])
            .unwrap();
        let asset = vfs.read("/models/items/a.md3").unwrap().unwrap();
        assert_eq!(asset.bytes, b"md3");
        assert!(vfs.contains("\\models\\items\\a.md3").unwrap());
        assert!(vfs.read("//models/items/a.md3").is_err());
    }

    #[test]
    fn a_mounted_name_with_a_leading_slash_is_still_refused() {
        let mut vfs = VirtualFileSystem::new();
        assert!(
            vfs.mount_memory("test", [("/models/a.md3", Vec::new())])
                .is_err()
        );
    }
}

#[cfg(test)]
mod filtered_pk3_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejected_entries_are_neither_read_nor_listed() {
        let path = std::env::temp_dir().join(format!(
            "sjk-vfs-filtered-{}-{:?}.pk3",
            std::process::id(),
            std::thread::current().id()
        ));
        {
            let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
            for (name, bytes) in [
                ("maps/new.bsp", &b"bsp"[..]),
                ("autoexec.cfg", b"quit"),
                ("cgamex86.dll", b"MZ"),
            ] {
                zip.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        let mut vfs = VirtualFileSystem::new();
        let mut asked = Vec::new();
        vfs.mount_pk3_filtered(&path, |entry| {
            asked.push(entry.as_str().to_owned());
            entry.as_str().starts_with("maps/")
        })
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(asked, ["maps/new.bsp", "autoexec.cfg", "cgamex86.dll"]);
        assert_eq!(vfs.read("maps/new.bsp").unwrap().unwrap().bytes, b"bsp");
        assert!(vfs.read("autoexec.cfg").unwrap().is_none());
        assert!(!vfs.contains("cgamex86.dll").unwrap());
        assert_eq!(vfs.paths().len(), 1);
        assert_eq!(vfs.mounts().next().unwrap().entries, 1);
    }
}

#[cfg(test)]
mod mount_contains_tests {
    use super::*;

    #[test]
    fn only_the_named_mount_is_asked() {
        let mut vfs = VirtualFileSystem::new();
        let lower = vfs
            .mount_memory("lower", [("tool/manifest.json", b"{}".to_vec())])
            .unwrap();
        let upper = vfs
            .mount_memory("upper", [("textures/a.png", b"x".to_vec())])
            .unwrap();
        assert!(vfs.mount_contains(lower, "TOOL/manifest.json").unwrap());
        assert!(!vfs.mount_contains(upper, "tool/manifest.json").unwrap());
        assert!(
            !vfs.without_mounts(&[lower])
                .mount_contains(lower, "tool/manifest.json")
                .unwrap()
        );
    }
}

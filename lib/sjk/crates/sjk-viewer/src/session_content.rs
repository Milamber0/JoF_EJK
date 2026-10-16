//! Per-gamestate cached content selection. Offline mounts never include the download cache.
use md4::{Digest, Md4};
use sjk_protocol::{GameState, InfoString};
use sjk_vfs::{Pk3Fingerprint, VirtualFileSystem};
use std::error::Error;
use std::path::Path;

/// Immutable request copied to the world-loading worker, never sampled per frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    checksums: Vec<i32>,
    map_checksum: Option<i32>,
    /// `mapname` from the server info, for a cached map nobody referenced.
    map: Option<String>,
}

impl Selection {
    /// Parse stock's references, which sv_init.cpp publishes on both pure and unpure servers.
    pub(crate) fn from_game(game: &GameState) -> Result<Self, Box<dyn Error>> {
        let mut selection = Self::from_system_info(game.config_string(1).unwrap_or(b""))?;
        selection.map = std::str::from_utf8(game.config_string(0).unwrap_or(b""))
            .ok()
            .and_then(|text| InfoString::parse(text).ok())
            .and_then(|info| info.get("mapname").map(str::to_owned))
            .filter(|map| sjk_vfs::VirtualPath::new(&format!("maps/{map}.bsp")).is_ok());
        Ok(selection)
    }

    fn from_system_info(raw: &[u8]) -> Result<Self, Box<dyn Error>> {
        let info = InfoString::parse(std::str::from_utf8(raw)?)?;
        let checksums = sjk_client::referenced_paks::parse(
            info.get("sv_referencedPaks").unwrap_or(""),
            info.get("sv_referencedPakNames").unwrap_or(""),
        )?
        .into_iter()
        .map(|reference| reference.checksum)
        .collect::<Vec<_>>();
        let map_checksum = info
            .get("sv_mapChecksum")
            .map(|value| {
                value
                    .parse::<i32>()
                    .or_else(|_| value.parse::<u32>().map(|v| v as i32))
            })
            .transpose()?;
        if checksums.is_empty() {
            crate::log::progress(format_args!(
                "session content: no pak references; downloads only for a missing map"
            ));
        }
        Ok(Self {
            checksums,
            map_checksum,
            map: None,
        })
    }

    /// Build a fresh configured search path; no previous world's mounts can leak into it.
    pub(crate) fn mount(&self, game_data: &Path) -> Result<VirtualFileSystem, Box<dyn Error>> {
        self.mount_in(game_data, &super::downloads::home()?)
    }

    fn mount_in(
        &self,
        game_data: &Path,
        cache: &Path,
    ) -> Result<VirtualFileSystem, Box<dyn Error>> {
        let mut vfs = super::mount_game_data(game_data)?;
        let mounts = vfs.mounts().count();
        self.mount_cache(&mut vfs, cache)?;
        // The server's packs went above the generated ones: put those back on top.
        if vfs.mounts().count() != mounts {
            super::generated::mount_on_top(&mut vfs);
        }
        Ok(vfs)
    }

    fn mount_cache(
        &self,
        vfs: &mut VirtualFileSystem,
        directory: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let map = self.map.as_ref().map(|map| format!("maps/{map}.bsp"));
        let map_missing = |vfs: &VirtualFileSystem| {
            map.as_deref()
                .is_some_and(|path| !vfs.contains(path).unwrap_or(true))
        };
        if self.checksums.is_empty() && !map_missing(vfs) {
            return Ok(());
        }
        let mut files = if directory.is_dir() {
            std::fs::read_dir(directory)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        files.retain(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pk3"))
        });
        files.sort();
        // Downloads are screened against the content mounted before them.
        let base = vfs.clone();
        let mut candidates = Vec::new();
        let installed: Vec<_> = vfs
            .mounts()
            .filter_map(|mount| {
                Pk3Fingerprint::open(mount.name.as_ref())
                    .ok()
                    .map(|p| (p.checksum(), std::path::PathBuf::from(mount.name.as_ref())))
            })
            .collect();
        for path in files {
            match Pk3Fingerprint::open(&path) {
                Ok(pak) => candidates.push((pak.checksum(), path)),
                Err(error) => crate::log::progress(format_args!(
                    "session content: ignoring unreadable cached pak {}: {error}",
                    path.display()
                )),
            }
        }
        // The server list is high-to-low search priority; VFS mounts are low-to-high.
        // Match contents, never the local cache filename or the requested map's basename.
        // Installed references also participate: a cached pak may be LOWER priority than
        // retail in the server list. Simply appending that pak would select the wrong BSP.
        for checksum in self.checksums.iter().rev() {
            if let Some((_, path)) = installed.iter().find(|(sum, _)| sum == checksum) {
                let canonical = std::fs::canonicalize(path)?;
                vfs.mount_pk3(&canonical)?;
                crate::log::progress(format_args!(
                    "session content: selected {} checksum={checksum}",
                    canonical.display()
                ));
            } else if let Some((_, path)) = candidates.iter().find(|(sum, _)| sum == checksum) {
                mount_download(vfs, &base, path)?;
                crate::log::progress(format_args!(
                    "session content: selected download {} checksum={checksum}",
                    path.display()
                ));
            } else {
                // References describe content the server touched, not a required
                // client install manifest. Server-only/cosmetic paks may be absent.
                // Do not mount a same-name substitute; load_bsp still checks the
                // actual map bytes, and pure-server admission remains separate.
                crate::log::progress(format_args!(
                    "session content: referenced pak checksum {checksum} unavailable; using available content"
                ));
            }
        }
        // A server that lists no paks (JoF's send an empty list) still names its
        // map: a download holding a map that nothing installed has is the only
        // copy there is. The newest wins; the map checksum, when sent, still
        // guards collision.
        if let Some(path) = map.as_deref()
            && map_missing(vfs)
        {
            candidates.sort_by_key(|(_, pak)| {
                std::fs::metadata(pak)
                    .and_then(|metadata| metadata.modified())
                    .ok()
            });
            for (checksum, pak) in candidates.iter().rev() {
                let mut probe = VirtualFileSystem::new();
                if probe.mount_pk3(pak).is_ok() && probe.contains(path).unwrap_or(false) {
                    mount_download(vfs, &base, pak)?;
                    crate::log::progress(format_args!(
                        "session content: {path} is in no listed pak; using download {} checksum={checksum}",
                        pak.display()
                    ));
                    break;
                }
            }
        }
        Ok(())
    }

    /// Parse one selected BSP for both collision and presentation; reject advertised mismatches.
    pub(crate) fn load_bsp(
        &self,
        vfs: &VirtualFileSystem,
        path: &str,
    ) -> Result<sjk_bsp::Bsp, Box<dyn Error>> {
        let asset = vfs
            .read(path)?
            .ok_or_else(|| format!("session map missing: {path}"))?;
        let actual = checksum(&asset.bytes);
        if self.map_checksum.is_some_and(|expected| actual != expected) {
            return Err(format!(
                "session map checksum mismatch: {path} from {}; expected {:?}, got {actual}; \
                 refusing mismatched collision geometry",
                asset.source.mount_name, self.map_checksum
            )
            .into());
        }
        crate::log::progress(format_args!(
            "session map: {path} source={} checksum={actual} verified={}",
            asset.source.mount_name,
            self.map_checksum.is_some()
        ));
        Ok(sjk_bsp::Bsp::parse(&asset.bytes)?)
    }
}

/// Mount a downloaded pak over `base` without what the download policy refuses.
fn mount_download(
    vfs: &mut VirtualFileSystem,
    base: &VirtualFileSystem,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let canonical = std::fs::canonicalize(path)?;
    let mut tally = super::downloads::policy::Tally::default();
    vfs.mount_pk3_filtered(
        &canonical,
        |entry| match super::downloads::policy::refusal(entry, base) {
            Some(refusal) => {
                tally.add(refusal, entry);
                false
            }
            None => true,
        },
    )?;
    if let Some(summary) = tally.summary() {
        crate::log::progress(format_args!(
            "session content: download {} stripped of {summary}",
            canonical.display()
        ));
    }
    Ok(())
}

/// codemp CM_LoadMap: XOR the four little-endian words of MD4 over the entire BSP file.
fn checksum(bytes: &[u8]) -> i32 {
    Md4::digest(bytes).chunks_exact(4).fold(0u32, |sum, word| {
        sum ^ u32::from_le_bytes(word.try_into().unwrap())
    }) as i32
}

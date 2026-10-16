//! UDP downloads (`sv_client.cpp:600-880`): a client missing a pak the server
//! references asks for it by name, and the file follows its snapshots in blocks of
//! 2,048 bytes, eight in flight at a time, each acknowledged by `nextdl`; a block not
//! acknowledged within a second is sent again.
use super::{LegacyGameHost, Slot};
use crate::{LegacyClientPhase, LegacyRateSettings};
use sjk_protocol::{MessageWriter, ServiceCommand};

/// `MAX_DOWNLOAD_WINDOW`.
const WINDOW: i32 = 8;
/// `MAX_DOWNLOAD_BLKSIZE`.
const BLOCK_BYTES: usize = 2048;
/// `downloadName[MAX_QPATH]`.
const NAME_BYTES: usize = 64;

/// What the host has of a file a client asks for (`FS_MV_VerifyDownloadPath`,
/// `FS_SV_FOpenFileRead`).
pub enum LegacyDownloadFile {
    /// Not a pak the server references (or one of the game's own): never sent.
    NotReferenced,
    /// Referenced, but not found.
    Missing,
    /// The file's bytes, which a host may share among every client downloading it.
    Found(std::sync::Arc<[u8]>),
}

/// One client's download (`client_t::download*`).
#[derive(Clone, Debug, Default)]
pub(super) struct DownloadState {
    /// `downloadName`: empty when nothing is asked for.
    pub(super) name: Vec<u8>,
    /// The open file, and how far it has been read.
    pub(super) file: Option<std::sync::Arc<[u8]>>,
    pub(super) read_at: usize,
    pub(super) size: i32,
    pub(super) count: i32,
    pub(super) current_block: i32,
    pub(super) client_block: i32,
    pub(super) xmit_block: i32,
    pub(super) blocks: [Vec<u8>; WINDOW as usize],
    pub(super) eof: bool,
    pub(super) send_time: i32,
    /// Console lines, handed to the host after the datagram or frame.
    pub(super) log: Vec<u8>,
}

impl DownloadState {
    /// `SV_CloseDownload`. The blocks' sizes stay what they were, as the reference's
    /// `downloadBlockSize` does: an acknowledgement after the end still reads them.
    pub(super) fn close(&mut self) {
        self.file = None;
        self.name.clear();
    }

    /// Whether a file is asked for or under way (`*downloadName`).
    pub(super) fn active(&self) -> bool {
        !self.name.is_empty()
    }
}

/// The server's download settings.
#[derive(Clone, Copy, Debug)]
pub(super) struct DownloadPolicy {
    /// `sv_allowDownload`.
    pub allow: bool,
    /// `sv_pure`, which words the refusal.
    pub pure: bool,
}

impl Slot {
    /// Hand the download's console lines to `print` (the host's console).
    pub(super) fn flush_download_log(&mut self, print: impl FnOnce(&[u8])) {
        if !self.download.log.is_empty() {
            print(&self.download.log);
            self.download.log.clear();
        }
    }

    /// `SV_BeginDownload_f`: any download under way ends, and the one named waits for
    /// the next message to open it. Ignored in the game.
    pub(super) fn begin_download(&mut self, name: &[u8]) {
        if self.phase == LegacyClientPhase::Active {
            return;
        }
        self.download.close();
        self.download.name = name[..name.len().min(NAME_BYTES - 1)].to_vec();
    }

    /// `SV_StopDownload_f`. Ignored in the game.
    pub(super) fn stop_download(&mut self) {
        if self.phase != LegacyClientPhase::Active {
            self.download.close();
        }
    }

    /// `SV_NextDownload_f`: `Err` for an acknowledgement of any other block than the
    /// one awaited, which drops the client ("broken download"). Ignored in the game.
    pub(super) fn next_download(&mut self, block: i32, client: usize, now: i32) -> Result<(), ()> {
        if self.phase == LegacyClientPhase::Active {
            return Ok(());
        }
        let download = &mut self.download;
        if block != download.client_block {
            return Err(());
        }
        // A block of no bytes is the end of the file.
        if download.blocks[(download.client_block % WINDOW) as usize].is_empty() {
            download.log.extend_from_slice(
                format!(
                    "clientDownload: {client} : file \"{}\" completed\n",
                    String::from_utf8_lossy(&download.name)
                )
                .as_bytes(),
            );
            download.close();
            return Ok(());
        }
        download.send_time = now;
        download.client_block += 1;
        Ok(())
    }

    /// `SV_WriteDownloadToClient`: the blocks this message carries, after its snapshot.
    pub(super) fn write_download(
        &mut self,
        client: usize,
        message: &mut MessageWriter,
        now: i32,
        policy: DownloadPolicy,
        rates: &mut LegacyRateSettings,
        game: &impl LegacyGameHost,
    ) -> Result<(), sjk_protocol::MessageError> {
        let (rate, snapshot_msec) = (self.rate, self.snapshot_msec);
        let download = &mut self.download;
        if download.name.is_empty() {
            return Ok(());
        }
        let name = String::from_utf8_lossy(&download.name).into_owned();
        if download.file.is_none() {
            let found = game.download_file(&download.name);
            let error = match (&found, policy.allow) {
                (LegacyDownloadFile::NotReferenced, _) => {
                    download.log.extend_from_slice(format!("clientDownload: {client} : \"{name}\" is not referenced and cannot be downloaded.\n").as_bytes());
                    Some(format!(
                        "File \"{name}\" is not referenced and cannot be downloaded."
                    ))
                }
                (_, false) => {
                    download.log.extend_from_slice(
                        format!("clientDownload: {client} : \"{name}\" download disabled\n")
                            .as_bytes(),
                    );
                    Some(if policy.pure {
                        format!(
                            "Could not download \"{name}\" because autodownloading is disabled on the server.\n\nYou will need to get this file elsewhere before you can connect to this pure server.\n"
                        )
                    } else {
                        format!(
                            "Could not download \"{name}\" because autodownloading is disabled on the server.\n\nThe server you are connecting to is not a pure server, set autodownload to No in your settings and you might be able to join the game anyway.\n"
                        )
                    })
                }
                (LegacyDownloadFile::Missing, true) => {
                    download.size = -1;
                    download.log.extend_from_slice(
                        format!("clientDownload: {client} : \"{name}\" file not found on server\n")
                            .as_bytes(),
                    );
                    Some(format!(
                        "File \"{name}\" not found on server for autodownloading.\n"
                    ))
                }
                (LegacyDownloadFile::Found(_), true) => None,
            };
            if let Some(error) = error {
                message.write_u8(ServiceCommand::Download as u8)?;
                message.write_i16(0)?;
                message.write_i32(-1)?;
                message.write_c_string(error.as_bytes())?;
                download.name.clear();
                return Ok(());
            }
            let LegacyDownloadFile::Found(bytes) = found else {
                unreachable!()
            };
            download.log.extend_from_slice(
                format!("clientDownload: {client} : beginning \"{name}\"\n").as_bytes(),
            );
            download.size = bytes.len() as i32;
            download.file = Some(bytes);
            download.read_at = 0;
            (
                download.current_block,
                download.client_block,
                download.xmit_block,
                download.count,
                download.eof,
            ) = (0, 0, 0, 0, false);
        }
        // Read as far as the window allows.
        while download.current_block - download.client_block < WINDOW
            && download.size != download.count
        {
            let file = download.file.as_deref().unwrap_or_default();
            let end = (download.read_at + BLOCK_BYTES).min(file.len());
            let block = &mut download.blocks[(download.current_block % WINDOW) as usize];
            block.clear();
            block.extend_from_slice(&file[download.read_at..end]);
            download.read_at = end;
            download.count += block.len() as i32;
            download.current_block += 1;
        }
        // The end-of-file block: empty, once there is room for it.
        if download.count == download.size
            && !download.eof
            && download.current_block - download.client_block < WINDOW
        {
            download.blocks[(download.current_block % WINDOW) as usize].clear();
            download.current_block += 1;
            download.eof = true;
        }
        // As many blocks as the client's rate carries in one snapshot interval.
        let mut rate = rate;
        if rates.max_rate != 0 {
            if rates.max_rate < 1000 {
                rates.max_rate = 1000;
            }
            rate = rate.min(rates.max_rate);
        }
        let mut blocks_per_snapshot = if rate == 0 {
            1
        } else {
            (rate * snapshot_msec / 1000 + BLOCK_BYTES as i32) / BLOCK_BYTES as i32
        };
        if blocks_per_snapshot < 0 {
            blocks_per_snapshot = 1;
        }
        for _ in 0..blocks_per_snapshot {
            if download.client_block == download.current_block {
                return Ok(());
            }
            if download.xmit_block == download.current_block {
                // The whole window is out: after a second, again from the client's block.
                if now - download.send_time > 1000 {
                    download.xmit_block = download.client_block;
                } else {
                    return Ok(());
                }
            }
            let block = &download.blocks[(download.xmit_block % WINDOW) as usize];
            message.write_u8(ServiceCommand::Download as u8)?;
            message.write_i16(download.xmit_block as i16)?;
            if download.xmit_block == 0 {
                message.write_i32(download.size)?;
            }
            message.write_i16(block.len() as i16)?;
            for &byte in block {
                message.write_u8(byte)?;
            }
            download.xmit_block += 1;
            download.send_time = now;
        }
        Ok(())
    }
}

//! Read the pure-checksum catalogue from the validated ZIP central directory.
//!
//! Opening each payload's local header turns an inventory into scattered reads
//! across every installed archive. OpenJK's FS_LoadZipFile uses directory CRCs
//! without opening payloads; payload/header validation remains in asset reads.
use std::collections::HashMap;
use std::io::{self, BufReader, Read, Seek, SeekFrom};

pub(super) fn catalog<R: Read + Seek>(archive: zip::ZipArchive<R>) -> io::Result<Vec<u32>> {
    let expected = archive.len();
    let start = archive.central_directory_start();
    let mut reader = BufReader::with_capacity(64 * 1024, archive.into_inner());
    reader.seek(SeekFrom::Start(start))?;
    let mut names = HashMap::with_capacity(expected);
    let mut entries = Vec::with_capacity(expected);
    let mut extra = Vec::new();
    loop {
        let mut signature = [0; 4];
        reader.read_exact(&mut signature)?;
        if signature != *b"PK\x01\x02" {
            break;
        }
        let mut header = [0; 42];
        reader.read_exact(&mut header)?;
        let crc = word(&header[12..16]);
        let size = word(&header[20..24]);
        let name_len = short(&header[24..26]);
        let extra_len = short(&header[26..28]);
        let comment_len = short(&header[28..30]);
        let mut name = vec![0; name_len];
        reader.read_exact(&mut name)?;
        extra.resize(extra_len, 0);
        reader.read_exact(&mut extra)?;
        reader.seek_relative(comment_len as i64)?;
        let size = if size == u32::MAX {
            zip64_size(&extra)?
        } else {
            u64::from(size)
        };
        // Match ZipArchive's stable index order and last duplicate-name value.
        let next = entries.len();
        let index = *names.entry(name).or_insert(next);
        if index == next {
            entries.push(None);
        }
        entries[index] = (size != 0).then_some(crc);
    }
    if entries.len() != expected {
        return Err(invalid(
            "ZIP directory changed while reading checksum catalogue",
        ));
    }
    Ok(entries.into_iter().flatten().collect())
}

fn short(bytes: &[u8]) -> usize {
    u16::from_le_bytes(bytes.try_into().unwrap()) as usize
}
fn word(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().unwrap())
}
fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

fn zip64_size(mut extra: &[u8]) -> io::Result<u64> {
    while extra.len() >= 4 {
        let tag = short(&extra[..2]);
        let length = short(&extra[2..4]);
        let value = extra
            .get(4..4 + length)
            .ok_or_else(|| invalid("truncated ZIP extra field"))?;
        if tag == 1 {
            return value
                .get(..8)
                .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap()))
                .ok_or_else(|| invalid("missing ZIP64 uncompressed size"));
        }
        extra = &extra[4 + length..];
    }
    Err(invalid("missing ZIP64 size field"))
}

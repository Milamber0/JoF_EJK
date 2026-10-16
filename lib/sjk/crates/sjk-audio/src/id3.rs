//! ID3v1 trailer handling for MP3 sounds.
//!
//! Raven's tools appended a 128-byte ID3v1 tag (`TAG` + album `#UNCOMP n` +
//! comment `#MAXVOL n`) to many MP3 sounds, and community packs copy that
//! layout. The legacy decoder drops it before decoding
//! (`codemp/mp3code/towave.c` `BYTESREMAINING_ACCOUNT_FOR_REAR_TAG`).
//!
//! The tag matters for short sounds: minimp3 only accepts a frame once the
//! frames after it carry matching headers, up to ten frames or the end of
//! the buffer. A sound of fewer than ten frames reaches the tag first, the
//! tag is not a frame header, and no frame decodes at all.

/// Size of an ID3v1 tag, always the last bytes of the file.
const ID3V1_LEN: usize = 128;

/// `bytes` without a trailing ID3v1 tag, as the legacy decoder reads them.
pub(crate) fn without_id3v1(bytes: &[u8]) -> &[u8] {
    match bytes.len().checked_sub(ID3V1_LEN) {
        Some(start) if bytes[start..].starts_with(b"TAG") => &bytes[..start],
        _ => bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_encoded;

    /// MPEG-1 Layer III, 128 kbit/s, 44.1 kHz, no padding, mono.
    const HEADER: [u8; 4] = [0xFF, 0xFB, 0x90, 0xC0];
    /// `144 * 128000 / 44100` bytes per frame.
    const FRAME_LEN: usize = 417;

    /// `frames` silent frames: zeroed side information codes no samples.
    fn silent_mp3(frames: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(frames * FRAME_LEN + ID3V1_LEN);
        for _ in 0..frames {
            bytes.extend_from_slice(&HEADER);
            bytes.resize(bytes.len() + FRAME_LEN - HEADER.len(), 0);
        }
        bytes
    }

    fn raven_tag() -> Vec<u8> {
        let mut tag = vec![0; ID3V1_LEN];
        tag[..3].copy_from_slice(b"TAG");
        tag[63..76].copy_from_slice(b"#UNCOMP 23040");
        tag[97..107].copy_from_slice(b"#MAXVOL 47");
        tag
    }

    #[test]
    fn strips_only_a_trailing_tag() {
        let mut tagged = silent_mp3(1);
        let audio_len = tagged.len();
        tagged.extend(raven_tag());
        assert_eq!(without_id3v1(&tagged).len(), audio_len);

        let untagged = silent_mp3(1);
        assert_eq!(without_id3v1(&untagged), &untagged[..]);
        assert_eq!(without_id3v1(b"TAG"), b"TAG");
    }

    #[test]
    fn short_tagged_mp3_decodes() {
        let mut bytes = silent_mp3(3);
        bytes.extend(raven_tag());
        let decoded = decode_encoded(&bytes, "mp3", 44_100).expect("tagged mp3 decodes");
        assert_eq!(decoded.samples.len(), 3 * 1152);
    }

    #[test]
    fn short_tagged_mp3_needs_the_tag_removed() {
        let mut bytes = silent_mp3(3);
        bytes.extend(raven_tag());
        let mut decoder = minimp3_fixed::Decoder::new(std::io::Cursor::new(&bytes[..]));
        assert!(
            matches!(decoder.next_frame(), Err(minimp3_fixed::Error::Eof)),
            "minimp3 is expected to reject the tagged stream on its own"
        );
    }
}

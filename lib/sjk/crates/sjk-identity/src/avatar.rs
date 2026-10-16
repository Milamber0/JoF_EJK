//! Players' pictures (`PROTOCOL.md` in Sol-Vulpes/SJK-hub, "Pictures"): a small square
//! image a key shows beside its name, public like the rest of its profile. The client
//! crops and scales the player's image to [`SIZE`] square and sends it as a PNG; the hub
//! writes a copy of its own (pixels only) and serves that, named by a version that
//! profiles and presence carry.

/// Edge of the picture the client sends and the hub serves, in pixels.
pub const SIZE: u32 = 128;
/// Largest picture the hub takes, in bytes.
pub const UPLOAD_MAX: usize = 256 * 1024;
/// Largest picture the client reads back from the hub, in bytes.
pub const ANSWER_MAX: u64 = 256 * 1024;
/// The bytes every PNG starts with.
pub const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Whether `version` is a picture version as profiles carry it: 16 lowercase hex
/// digits (the start of the SHA-256 of the hub's PNG).
pub fn valid_version(version: &str) -> bool {
    version.len() == 16
        && version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether `key_id` is a key id: 16 hex digits.
pub fn valid_key_id(key_id: &str) -> bool {
    key_id.len() == 16 && key_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The path `version` of `key_id`'s picture is read at; the version in the query lets
/// the hub (and anything between) cache it.
pub fn path(key_id: &str, version: &str) -> String {
    format!("/v1/avatar/{key_id}?v={version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_key_ids_are_checked_before_a_request() {
        assert!(valid_version("0123456789abcdef"));
        assert!(!valid_version("0123456789ABCDEF"));
        assert!(!valid_version("../../etc/passwd"));
        assert!(!valid_version(""));
        assert!(valid_key_id("0123456789ABCDEF"));
        assert!(!valid_key_id("0123456789abcde"));
        assert_eq!(
            path("0123456789abcdef", "fedcba9876543210"),
            "/v1/avatar/0123456789abcdef?v=fedcba9876543210"
        );
    }
}

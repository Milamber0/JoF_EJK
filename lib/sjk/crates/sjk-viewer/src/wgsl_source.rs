//! Line endings of WGSL sources the viewer patches by text.
//!
//! Programs are embedded with `include_str!`, which keeps the bytes of the checkout:
//! with Git's `core.autocrlf` (the Git for Windows default) every line ends in CRLF
//! unless `.gitattributes` applies. Rust string literals always use LF, so a pattern
//! that spans a line break only matches an LF source. Code that matches or patches
//! WGSL across lines normalises the source with [`lf`] first; patterns confined to one
//! line match either way.

use std::borrow::Cow;

/// `source` with CRLF line endings replaced by LF, borrowed when it has none.
pub(crate) fn lf(source: &str) -> Cow<'_, str> {
    if source.contains('\r') {
        Cow::Owned(source.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(source)
    }
}

/// `source` with every line ending in CRLF, as a Windows checkout embeds it.
#[cfg(test)]
pub(crate) fn crlf(source: &str) -> String {
    lf(source).replace('\n', "\r\n")
}

/// Parse and validate a whole program with naga, as wgpu would, panicking with naga's
/// own report; tests use it for programs no unit test renders.
#[cfg(test)]
pub(crate) fn validate(source: &str) -> wgpu::naga::valid::ModuleInfo {
    use wgpu::naga;
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lf_borrows_lf_sources_and_converts_crlf() {
        assert!(matches!(lf("a\nb\n"), Cow::Borrowed("a\nb\n")));
        assert_eq!(lf("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(lf("mixed\r\nends\n"), "mixed\nends\n");
    }

    #[test]
    fn crlf_round_trips_through_lf() {
        assert_eq!(crlf("a\nb\r\nc"), "a\r\nb\r\nc");
        assert_eq!(lf(&crlf("a\nb\n")), "a\nb\n");
    }
}

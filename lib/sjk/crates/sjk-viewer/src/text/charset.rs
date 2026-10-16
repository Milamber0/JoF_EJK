//! The code page other clients' text is read and written in (`cl_charset`).
//!
//! Retail and EternalJK send one byte per character: Windows-1252 for Western
//! players, Windows-1251 for Russian ones ([`sjk_protocol::LegacyCharset`]). The
//! bytes cannot say which, so by default SJK reads a line as Russian when it looks
//! Russian and sends Cyrillic in Windows-1251 when it reads back the same; the
//! player can pin either code page instead.

use sjk_protocol::LegacyCharset;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};

/// The setting: `auto`, `western` or `cyrillic`.
pub(crate) const CVAR: &str = "cl_charset";

/// The setting's values, in the order Settings shows them.
pub(crate) const NAMES: [&str; 3] = ["auto", "western", "cyrillic"];

/// Register `cl_charset` before configuration loading.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        CVAR,
        "auto",
        CvarFlags::ARCHIVE,
        "Code page of other clients' text: auto, western (Windows-1252) or cyrillic (Windows-1251)",
    ))?;
    Ok(())
}

/// The code page a `cl_charset` value names; anything unknown is `auto`.
pub(crate) fn parse(value: Option<&str>) -> LegacyCharset {
    match value.map(str::trim) {
        Some(name) if name.eq_ignore_ascii_case("western") => LegacyCharset::Western,
        Some(name) if name.eq_ignore_ascii_case("cyrillic") => LegacyCharset::Cyrillic,
        _ => LegacyCharset::Auto,
    }
}

/// Apply `value` (the cvar's) to every decode and encode of legacy text.
pub(crate) fn sync(value: Option<&str>) {
    let charset = parse(value);
    if sjk_protocol::legacy_charset() != charset {
        sjk_protocol::set_legacy_charset(charset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_name_their_code_page() {
        assert_eq!(parse(Some("western")), LegacyCharset::Western);
        assert_eq!(parse(Some(" Cyrillic ")), LegacyCharset::Cyrillic);
        assert_eq!(parse(Some("auto")), LegacyCharset::Auto);
        assert_eq!(parse(Some("1251")), LegacyCharset::Auto);
        assert_eq!(parse(None), LegacyCharset::Auto);
    }
}

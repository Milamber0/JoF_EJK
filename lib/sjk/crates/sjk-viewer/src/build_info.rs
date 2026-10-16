//! Which build this is: its version, source commit and commit time, set at
//! compile time by `scripts/build_version.rs`. The version overlay, the menus'
//! version line and the startup notice show them.
//!
//! Dates read as SJK writes them for people: `dd/mm/yyyy` and a 24-hour time
//! (`docs/sjk.md`, "Dates and times").

use std::sync::OnceLock;

/// `0.1.0-alpha.3` in a release, the package version with `-dev` otherwise.
pub(crate) const VERSION: &str = env!("SJK_BUILD_VERSION");
/// Short hash of the source commit; empty when the build did not know it.
const COMMIT: &str = env!("SJK_BUILD_COMMIT");
/// Committer time of the source commit, ISO 8601; empty when unknown.
const COMMIT_TIME: &str = env!("SJK_BUILD_COMMIT_TIME");

/// `SJK <version> · <dd/mm/yyyy HH:MM> · <commit>`, built once.
pub(crate) fn label() -> &'static str {
    static LABEL: OnceLock<String> = OnceLock::new();
    LABEL.get_or_init(|| format_label(VERSION, COMMIT_TIME, COMMIT))
}

/// The label of a build, leaving out a time or commit the build did not know.
fn format_label(version: &str, time: &str, commit: &str) -> String {
    let mut label = format!("SJK {version}");
    for part in [eu_date_time(time), Some(commit.to_owned())]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
    {
        label.push_str(" \u{b7} ");
        label.push_str(&part);
    }
    label
}

/// `2026-10-05T19:56:05+02:00` as `05/10/2026 19:56`: the date and time as
/// written in the commit's own time zone, day first, on the 24-hour clock.
fn eu_date_time(iso: &str) -> Option<String> {
    let bytes = iso.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        let part = iso.get(range)?;
        part.bytes().all(|b| b.is_ascii_digit()).then_some(part)
    };
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute) = (digits(11..13)?, digits(14..16)?);
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':')];
    if separators
        .iter()
        .any(|&(index, byte)| bytes.get(index) != Some(&byte))
    {
        return None;
    }
    Some(format!("{day}/{month}/{year} {hour}:{minute}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_time_reads_day_first_on_the_24_hour_clock() {
        assert_eq!(
            eu_date_time("2026-10-05T19:56:05+02:00").as_deref(),
            Some("05/10/2026 19:56")
        );
        assert_eq!(
            eu_date_time("2026-01-31T00:07:00Z").as_deref(),
            Some("31/01/2026 00:07")
        );
        for bad in [
            "",
            "yesterday",
            "2026-10-05",
            "2026/10/05T19:56",
            "2026-1x-05T19:56",
        ] {
            assert_eq!(eu_date_time(bad), None, "{bad}");
        }
    }

    #[test]
    fn label_leaves_out_what_the_build_did_not_know() {
        assert_eq!(
            format_label("2026.1005.1", "2026-10-05T19:56:05+02:00", "474caf6"),
            "SJK 2026.1005.1 \u{b7} 05/10/2026 19:56 \u{b7} 474caf6"
        );
        assert_eq!(format_label("dev", "", ""), "SJK dev");
        assert_eq!(format_label("dev", "", "474caf6"), "SJK dev \u{b7} 474caf6");
    }

    #[test]
    fn this_build_has_a_label() {
        assert!(label().starts_with(&format!("SJK {VERSION}")));
    }
}

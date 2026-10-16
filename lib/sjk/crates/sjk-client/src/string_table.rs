//! Retail `strings/<language>/*.str` tables: `REFERENCE <KEY>` lines followed
//! (possibly after `NOTES`) by `LANG_ENGLISH "<text>"`. Assets refer to
//! entries as `@KEY` (menu labels, `.sab` names) or `@@@KEY` inside server
//! text.

use sjk_vfs::VirtualFileSystem;
use std::collections::HashMap;

/// Parse one table's text into `target`, later keys overriding earlier ones.
pub fn parse_into(target: &mut HashMap<String, String>, text: &str) {
    let mut reference = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix("REFERENCE") {
            reference = Some(key.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("LANG_ENGLISH") {
            let Some(key) = reference.take() else {
                continue;
            };
            let value = value.trim();
            if let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                target.insert(key, value.to_owned());
            }
        }
    }
}

/// Read and parse the tables at `paths` (missing ones are skipped), keyed
/// the way assets reference them: `<FILE>_<KEY>` with the file stem upper
/// case, so `strings/english/menus.str` `SINGLE_HILT1` is found by
/// `@MENUS_SINGLE_HILT1`.
pub fn load_referenced(vfs: &VirtualFileSystem, paths: &[&str]) -> HashMap<String, String> {
    let mut strings = HashMap::new();
    for path in paths {
        let Some(asset) = vfs.read(path).ok().flatten() else {
            continue;
        };
        let stem = path
            .rsplit('/')
            .next()
            .and_then(|file| file.split('.').next())
            .unwrap_or_default()
            .to_ascii_uppercase();
        let mut table = HashMap::new();
        parse_into(&mut table, &String::from_utf8_lossy(&asset.bytes));
        strings.extend(
            table
                .into_iter()
                .map(|(key, value)| (format!("{stem}_{key}"), value)),
        );
    }
    strings
}

/// Resolve a `@FILE_KEY` reference against `strings`; other text is
/// returned as it is, and an unknown key keeps its reference so it stays
/// visible.
pub fn resolve<'a>(strings: &'a HashMap<String, String>, text: &'a str) -> &'a str {
    text.strip_prefix('@')
        .and_then(|key| strings.get(key))
        .map_or(text, String::as_str)
}

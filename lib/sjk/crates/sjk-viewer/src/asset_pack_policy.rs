//! Portable, case-insensitive PK3 choices. Host paths never enter the saved list.
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) const CVAR: &str = "fs_disabledPaks";

#[derive(Clone, Default)]
pub(crate) struct Disabled(BTreeSet<String>);

pub(crate) fn key(archive: &Path) -> Option<String> {
    let directory = archive.parent()?.file_name()?.to_str()?;
    let name = archive.file_name()?.to_str()?;
    Some(format!("{directory}/{name}").to_ascii_lowercase())
}

pub(crate) fn retail(key: &str) -> bool {
    [
        "base/assets0.pk3",
        "base/assets1.pk3",
        "base/assets2.pk3",
        "base/assets3.pk3",
    ]
    .iter()
    .any(|name| key.eq_ignore_ascii_case(name))
}

impl Disabled {
    pub(crate) fn parse(value: &str) -> Result<Self, serde_json::Error> {
        let names: Vec<String> = serde_json::from_str(value)?;
        Ok(Self(
            names
                .into_iter()
                .map(|name| name.to_ascii_lowercase())
                .filter(|name| !retail(name))
                .collect(),
        ))
    }

    /// Query a normalized key from [`key`] without allocating during UI drawing.
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.0.contains(key) && !retail(key)
    }

    pub(crate) fn allows(&self, path: &Path) -> bool {
        key(path).is_none_or(|key| !self.contains(&key))
    }

    pub(crate) fn toggle(&mut self, key: &str) {
        if retail(key) {
            return;
        }
        let key = key.to_ascii_lowercase();
        if !self.0.remove(&key) {
            self.0.insert(key);
        }
    }

    pub(crate) fn saved(&self) -> String {
        serde_json::to_string(&self.0).expect("pack names serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_are_portable_case_insensitive_and_round_trip_names_with_spaces() {
        let mut disabled = Disabled::default();
        disabled.toggle("Base/My DL-44.pk3");
        let loaded = Disabled::parse(&disabled.saved()).unwrap();
        assert!(!loaded.allows(Path::new("/install/base/my dl-44.pk3")));
        assert!(loaded.allows(Path::new("/install/mod/my dl-44.pk3")));
        disabled.toggle("BASE/MY DL-44.PK3");
        assert!(disabled.allows(Path::new("/other/base/My DL-44.pk3")));
    }

    #[test]
    fn retail_files_cannot_be_disabled() {
        let disabled = Disabled::parse(r#"["BASE/ASSETS0.PK3", "base/assets3.pk3"]"#).unwrap();
        assert!(disabled.allows(Path::new("/install/base/assets0.pk3")));
        assert!(disabled.allows(Path::new("/install/base/assets3.pk3")));
    }
}

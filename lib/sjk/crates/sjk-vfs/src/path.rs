use std::error::Error;
use std::fmt;
use std::path::{Component, Path};

/// A normalized, case-insensitive path inside the SJK virtual filesystem.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn new(input: &str) -> Result<Self, VirtualPathError> {
        if input.is_empty() {
            return Err(VirtualPathError::Empty);
        }
        if input.contains('\0') {
            return Err(VirtualPathError::ContainsNul);
        }
        if input.starts_with(['/', '\\']) {
            return Err(VirtualPathError::Absolute);
        }

        let normalized_separators = input.replace('\\', "/");
        let mut components = Vec::new();
        for component in normalized_separators.split('/') {
            match component {
                "" | "." => continue,
                ".." => return Err(VirtualPathError::ParentTraversal),
                value if value.contains(':') => {
                    return Err(VirtualPathError::InvalidComponent(value.to_owned()));
                }
                value if value.chars().any(char::is_control) => {
                    return Err(VirtualPathError::InvalidComponent(value.to_owned()));
                }
                value => components.push(value.to_ascii_lowercase()),
            }
        }

        if components.is_empty() {
            return Err(VirtualPathError::Empty);
        }
        Ok(Self(components.join("/")))
    }

    /// The path a read looks up for `input`: [`Self::new`] after dropping one
    /// leading `/` or `\`, as `FS_FOpenFileRead` does (`codemp/qcommon/files.cpp`,
    /// "qpaths are not supposed to have a leading slash"). Game data names some
    /// assets that way, such as a map's `/models/items/...` item model. Mounts,
    /// archive entries and writes keep [`Self::new`]'s rejection.
    pub fn for_read(input: &str) -> Result<Self, VirtualPathError> {
        Self::new(input.strip_prefix(['/', '\\']).unwrap_or(input))
    }

    pub(crate) fn from_host_relative(path: &Path) -> Result<Self, VirtualPathError> {
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(value) => {
                    let value = value.to_str().ok_or(VirtualPathError::NonUtf8)?;
                    components.push(value);
                }
                Component::CurDir => {}
                Component::ParentDir => return Err(VirtualPathError::ParentTraversal),
                Component::RootDir | Component::Prefix(_) => {
                    return Err(VirtualPathError::Absolute);
                }
            }
        }
        Self::new(&components.join("/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VirtualPathError {
    Empty,
    Absolute,
    ParentTraversal,
    ContainsNul,
    NonUtf8,
    InvalidComponent(String),
}

impl fmt::Display for VirtualPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("virtual path is empty"),
            Self::Absolute => formatter.write_str("virtual path must be relative"),
            Self::ParentTraversal => formatter.write_str("virtual path contains parent traversal"),
            Self::ContainsNul => formatter.write_str("virtual path contains a NUL byte"),
            Self::NonUtf8 => formatter.write_str("host path is not valid UTF-8"),
            Self::InvalidComponent(component) => {
                write!(formatter, "invalid virtual path component {component:?}")
            }
        }
    }
}

impl Error for VirtualPathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_drop_one_leading_slash_like_fs_fopen_file_read() {
        for input in ["/models/items/A.md3", "\\models\\items\\a.md3"] {
            assert_eq!(
                VirtualPath::for_read(input).unwrap().as_str(),
                "models/items/a.md3"
            );
        }
        // Only one: a second slash still makes the qpath absolute.
        assert_eq!(
            VirtualPath::for_read("//models/a.md3"),
            Err(VirtualPathError::Absolute)
        );
        assert_eq!(
            VirtualPath::for_read("/../a.md3"),
            Err(VirtualPathError::ParentTraversal)
        );
    }

    #[test]
    fn mounted_names_keep_rejecting_a_leading_slash() {
        assert_eq!(
            VirtualPath::new("/models/a.md3"),
            Err(VirtualPathError::Absolute)
        );
    }
}

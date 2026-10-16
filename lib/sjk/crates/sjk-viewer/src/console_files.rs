//! OpenJK files.cpp:2728-2963; cl_console.cpp:192-277 (condump).
//! Writes remain beneath the viewer's explicit config root, not the process cwd.
use super::*;

impl Resolver<'_> {
    pub(super) fn command(
        &self,
        name: &str,
        args: &[String],
        dump: &str,
    ) -> Result<Vec<String>, String> {
        if name == "path" {
            let mut paths = vec![self.config_directory.display().to_string()];
            if let Some(vfs) = self.vfs {
                paths.extend(
                    vfs.mounts()
                        .rev()
                        .map(|m| format!("{} ({} files)", m.name, m.entries)),
                );
            }
            return Ok(paths);
        }
        let [path, rest @ ..] = args else {
            return Err(format!("usage: {name} <path>"));
        };
        validate_relative_path(path)?;
        if name == "condump" {
            if !rest.is_empty() {
                return Err("usage: condump <filename>".into());
            }
            let path = if Path::new(path).extension().is_none() {
                format!("{path}.txt")
            } else {
                path.clone()
            };
            if !path.to_ascii_lowercase().ends_with(".txt") {
                return Err("condump requires a .txt file".into());
            }
            let output = self.config_directory.join(&path);
            let root = self
                .config_directory
                .canonicalize()
                .map_err(|e| e.to_string())?;
            let parent = output
                .parent()
                .ok_or("invalid dump path")?
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !parent.starts_with(&root) || output.is_symlink() {
                return Err("condump path escapes the config root".into());
            }
            let mut clean = String::new();
            let mut chars = dump.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                } else {
                    clean.push(c);
                }
            }
            clean.push('\n');
            fs::write(&output, clean).map_err(|e| e.to_string())?;
            return Ok(vec![format!("Dumped console to {}", output.display())]);
        }
        if name == "which" || name == "touchfile" {
            if !rest.is_empty() {
                return Err(format!("usage: {name} <file>"));
            }
            let host = self.config_directory.join(path);
            let source = if host.is_file() {
                fs::read(&host).map_err(|e| e.to_string())?;
                Some(host.display().to_string())
            } else if let Some(vfs) = self.vfs {
                vfs.read(path)
                    .map_err(|e| e.to_string())?
                    .map(|a| a.source.mount_name.to_string())
            } else {
                None
            };
            return source
                .map(|source| vec![format!("{path}: {source}")])
                .ok_or_else(|| format!("File not found: {path}"));
        }
        let mut paths = std::collections::BTreeSet::new();
        host_paths(self.config_directory, self.config_directory, &mut paths)?;
        if let Some(vfs) = self.vfs {
            paths.extend(vfs.paths().iter().map(ToString::to_string));
        }
        let prefix = path.trim_end_matches('/').trim_start_matches("./");
        let filter = if name == "fdir" {
            path.to_ascii_lowercase()
        } else {
            format!(
                "{}*{}",
                if prefix.is_empty() {
                    String::new()
                } else {
                    format!("{prefix}/")
                },
                rest.first().map_or("", String::as_str)
            )
        };
        Ok(paths
            .into_iter()
            .filter(|p| wildcard(&filter, &p.to_ascii_lowercase()))
            .filter(|p| {
                name == "fdir"
                    || p.get(prefix.len() + usize::from(!prefix.is_empty())..)
                        .is_some_and(|tail| !tail.contains('/'))
            })
            .collect())
    }
}

fn host_paths(
    root: &Path,
    directory: &Path,
    paths: &mut std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            host_paths(root, &entry.path(), paths)?;
        } else if kind.is_file() {
            paths.insert(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut i, mut j, mut star, mut retry) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == b'?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == b'*' {
            star = Some(i);
            i += 1;
            retry = j;
        } else if let Some(s) = star {
            retry += 1;
            j = retry;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == b'*' {
        i += 1;
    }
    i == p.len()
}

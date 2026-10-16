//! The build's version, source commit and commit time, for the build scripts of
//! both programs (`sjk-viewer` and `sjk-dedicated` include this file), so the
//! version is decided in one place.
//!
//! The programs read three variables at compile time:
//!
//! - `SJK_BUILD_VERSION`: `SJK_VERSION` when the build sets it (the release
//!   workflow sets it from the `sjk-v<version>` tag, a date version such as
//!   `2026.1005.1`), otherwise `dev`, so a local build never passes for a release;
//! - `SJK_BUILD_COMMIT`: the source commit's short hash;
//! - `SJK_BUILD_COMMIT_TIME`: its committer time, strict ISO 8601 with offset.
//!
//! The commit and its time come from git when the source is a checkout, else from
//! `SJK_COMMIT` and `SJK_COMMIT_TIME`, else they are empty and the programs leave
//! them out. A build is redone when `HEAD` moves, not for uncommitted edits.

use std::path::Path;
use std::process::Command;

/// Print the `cargo:` lines that give the crate the build variables, and return
/// the version.
pub fn emit() -> String {
    for name in ["SJK_VERSION", "SJK_COMMIT", "SJK_COMMIT_TIME"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let directory = Path::new(&manifest);
    let version = variable("SJK_VERSION").unwrap_or_else(|| "dev".into());
    let commit = git(directory, &["rev-parse", "--short=7", "HEAD"])
        .or_else(|| variable("SJK_COMMIT"))
        .unwrap_or_default();
    let time = git(directory, &["log", "-1", "--format=%cI"])
        .or_else(|| variable("SJK_COMMIT_TIME"))
        .unwrap_or_default();
    println!("cargo:rustc-env=SJK_BUILD_VERSION={version}");
    println!("cargo:rustc-env=SJK_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SJK_BUILD_COMMIT_TIME={time}");
    for path in watched_git_files(directory) {
        println!("cargo:rerun-if-changed={path}");
    }
    version
}

/// A non-empty environment variable.
fn variable(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The trimmed output of a successful git command run in `directory`.
fn git(directory: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}

/// The files that change when `HEAD` moves: `HEAD` itself, the branch's loose
/// ref and `packed-refs`, wherever this checkout or worktree keeps them.
fn watched_git_files(directory: &Path) -> Vec<String> {
    let path = |name: &str| {
        git(
            directory,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )
    };
    let mut files = Vec::new();
    files.extend(path("HEAD"));
    if let Some(branch) = git(directory, &["rev-parse", "--symbolic-full-name", "HEAD"])
        .filter(|name| name.starts_with("refs/"))
    {
        files.extend(path(&branch));
    }
    files.extend(path("packed-refs"));
    files.retain(|file| Path::new(file).exists());
    files
}

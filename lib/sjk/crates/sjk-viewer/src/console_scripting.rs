//! Viewer filesystem policy for portable shell scripts.

use sjk_shell::CommandFileResolver;
use std::fs;
use std::path::{Component, Path};

pub(super) struct Resolver<'a> {
    config_directory: &'a Path,
    vfs: Option<&'a sjk_vfs::VirtualFileSystem>,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(
        config_directory: &'a Path,
        vfs: Option<&'a sjk_vfs::VirtualFileSystem>,
    ) -> Self {
        Self {
            config_directory,
            vfs,
        }
    }
}

impl CommandFileResolver for Resolver<'_> {
    fn file_command(
        &mut self,
        name: &str,
        args: &[String],
        dump: &str,
    ) -> Result<Vec<String>, String> {
        self.command(name, args, dump)
    }
    fn read_command_file(&mut self, path: &str) -> Result<Option<String>, String> {
        validate_relative_path(path)?;
        let host_path = self.config_directory.join(path);
        match fs::read(&host_path) {
            Ok(bytes) => return Ok(Some(sjk_shell::decode_config_text(bytes))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("could not read {}: {error}", host_path.display())),
        }
        let Some(vfs) = self.vfs else {
            return Ok(None);
        };
        let asset = vfs.read(path).map_err(|error| error.to_string())?;
        Ok(asset.map(|asset| sjk_shell::decode_config_text(asset.bytes)))
    }
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err("exec path must stay inside the config or VFS root".to_owned());
    }
    Ok(())
}

#[path = "console_files.rs"]
mod files;

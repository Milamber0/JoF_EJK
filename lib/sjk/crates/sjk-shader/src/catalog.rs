//! Strict tooling and fault-isolated client shader catalogue loading.

use sjk_vfs::{VirtualFileSystem, VirtualPath};

use crate::{ShaderCatalog, ShaderError, normalize_name, parse::parse_script};

mod authority;
pub use authority::DefinitionSource;

impl ShaderCatalog {
    /// Loads every shader script, returning the first read or parse error.
    /// Useful for validation tools which must detect broken content.
    pub fn load(vfs: &VirtualFileSystem) -> Result<Self, ShaderError> {
        let mut catalog = Self::default();
        for path in shader_paths(vfs) {
            catalog.load_script(vfs, &path)?;
        }
        Ok(catalog)
    }

    /// Loads usable definitions, reporting each malformed shader independently.
    /// Unreadable files are reported once. Valid definitions retain normal override order.
    pub fn load_with_warnings(
        vfs: &VirtualFileSystem,
        mut warning: impl FnMut(&VirtualPath, &ShaderError),
    ) -> Self {
        let mut catalog = Self::default();
        for path in shader_paths(vfs) {
            match vfs.read(path.as_str()) {
                Ok(Some(asset)) => {
                    let definitions =
                        crate::recovery::parse_recovering(&asset.bytes, &path, |error| {
                            warning(&path, &error)
                        });
                    for definition in definitions {
                        catalog
                            .definitions
                            .insert(normalize_name(&definition.name), definition);
                    }
                }
                Ok(None) => warning(&path, &ShaderError::DisappearedAsset(path.clone())),
                Err(error) => warning(&path, &ShaderError::Vfs(error)),
            }
        }
        catalog
    }

    fn load_script(
        &mut self,
        vfs: &VirtualFileSystem,
        path: &VirtualPath,
    ) -> Result<(), ShaderError> {
        let asset = vfs
            .read(path.as_str())?
            .ok_or_else(|| ShaderError::DisappearedAsset(path.clone()))?;
        for definition in parse_script(&asset.bytes, path)? {
            self.definitions
                .insert(normalize_name(&definition.name), definition);
        }
        Ok(())
    }
}

fn shader_paths(vfs: &VirtualFileSystem) -> impl Iterator<Item = VirtualPath> {
    vfs.paths()
        .into_iter()
        .filter(|path| path.as_str().starts_with("shaders/") && path.as_str().ends_with(".shader"))
}

//! Optional source authority, independent of any game's archive naming conventions.
use super::*;
use sjk_vfs::AssetSource;
use std::collections::HashMap;

/// Script and mount supplying a shader definition.
#[derive(Clone, Debug)]
pub struct DefinitionSource {
    /// Normalized virtual script path.
    pub script: VirtualPath,
    /// Mounted asset origin.
    pub asset: AssetSource,
}

impl ShaderCatalog {
    /// Prefers definitions from sources classified as authoritative by the application.
    ///
    /// Authoritative scripts are read even when another mount hides the same script path.
    /// Between authoritative sources normal script/mount order wins. Visible non-authoritative
    /// scripts retain legacy order and may add unique definitions, but cannot replace preferred
    /// ones. Each discarded definition reports its name, winner, and loser to `suppressed`.
    /// Use `load_with_warnings` instead for exactly the legacy visible-file, last-wins policy.
    pub fn load_with_authority(
        vfs: &VirtualFileSystem,
        authoritative: impl Fn(&AssetSource) -> bool,
        mut warning: impl FnMut(&VirtualPath, &ShaderError),
        mut suppressed: impl FnMut(&str, &DefinitionSource, &DefinitionSource),
    ) -> Self {
        let mut catalog = Self::default();
        let mut origins: HashMap<String, DefinitionSource> = HashMap::new();
        let paths: Vec<_> = shader_paths(vfs).collect();
        for path in &paths {
            for mount in vfs.mounts() {
                let source = AssetSource {
                    mount_id: mount.id,
                    mount_name: mount.name,
                };
                if !authoritative(&source) {
                    continue;
                }
                match vfs.read_from_mount(source.mount_id, path.as_str()) {
                    Ok(Some(asset)) => {
                        for definition in
                            crate::recovery::parse_recovering(&asset.bytes, path, |error| {
                                warning(path, &error)
                            })
                        {
                            let name = normalize_name(&definition.name);
                            origins.insert(
                                name.clone(),
                                DefinitionSource {
                                    script: path.clone(),
                                    asset: source.clone(),
                                },
                            );
                            catalog.definitions.insert(name, definition);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => warning(path, &ShaderError::Vfs(error)),
                }
            }
        }
        let mut reported = std::collections::HashSet::new();
        for path in &paths {
            match vfs.read(path.as_str()) {
                Ok(Some(asset)) => {
                    if authoritative(&asset.source) {
                        continue;
                    }
                    let source = DefinitionSource {
                        script: path.clone(),
                        asset: asset.source,
                    };
                    for definition in
                        crate::recovery::parse_recovering(&asset.bytes, path, |error| {
                            warning(path, &error)
                        })
                    {
                        let name = normalize_name(&definition.name);
                        if let Some(winner) = origins.get(&name) {
                            let conflict = (name.clone(), source.asset.mount_id, path.clone());
                            if reported.insert(conflict) {
                                suppressed(&name, winner, &source);
                            }
                        } else {
                            catalog.definitions.insert(name, definition);
                        }
                    }
                }
                Ok(None) => warning(path, &ShaderError::DisappearedAsset(path.clone())),
                Err(error) => warning(path, &ShaderError::Vfs(error)),
            }
        }
        catalog
    }

    /// Adds the definitions of the visible scripts that `added` mounts supply, without
    /// replacing any name already in the catalogue or any name `claimed` reports: content
    /// mounted last may bring shaders of its own but not redefine anyone else's. Between
    /// added scripts the usual order wins. Each refused definition goes to `refused`.
    pub fn add_unclaimed(
        &mut self,
        vfs: &VirtualFileSystem,
        added: impl Fn(&AssetSource) -> bool,
        mut claimed: impl FnMut(&str) -> bool,
        mut warning: impl FnMut(&VirtualPath, &ShaderError),
        mut refused: impl FnMut(&str, &DefinitionSource),
    ) {
        let mut own = std::collections::HashSet::new();
        for path in shader_paths(vfs) {
            let asset = match vfs.read(path.as_str()) {
                Ok(Some(asset)) if added(&asset.source) => asset,
                Ok(_) => continue,
                Err(error) => {
                    warning(&path, &ShaderError::Vfs(error));
                    continue;
                }
            };
            let source = DefinitionSource {
                script: path.clone(),
                asset: asset.source,
            };
            for definition in crate::recovery::parse_recovering(&asset.bytes, &path, |error| {
                warning(&path, &error)
            }) {
                let name = normalize_name(&definition.name);
                if !own.contains(&name) && (self.definitions.contains_key(&name) || claimed(&name))
                {
                    refused(&name, &source);
                    continue;
                }
                own.insert(name.clone());
                self.definitions.insert(name, definition);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_scripts_bring_new_shaders_but_cannot_redefine_others() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "base",
            [(
                "shaders/walls.shader",
                b"textures/base/wall
{
	surfaceparm nonsolid
}
"
                .to_vec(),
            )],
        )
        .unwrap();
        let base = vfs.clone();
        let download = vfs
            .mount_memory(
                "download",
                [(
                    "shaders/walls.shader",
                    b"textures/base/wall
{
	surfaceparm trans
}
                      textures/base/floor
{
}
                      textures/map/new
{
}
"
                    .to_vec(),
                )],
            )
            .unwrap();
        let mut catalog = ShaderCatalog::load_with_warnings(&base, |_, _| {});
        let mut refusals = Vec::new();
        catalog.add_unclaimed(
            &vfs,
            |source| source.mount_id == download,
            |name| name == "textures/base/floor",
            |_, _| {},
            |name, source| refusals.push((name.to_owned(), source.script.as_str().to_owned())),
        );
        assert_eq!(
            refusals,
            [
                (
                    "textures/base/wall".to_owned(),
                    "shaders/walls.shader".to_owned()
                ),
                (
                    "textures/base/floor".to_owned(),
                    "shaders/walls.shader".to_owned()
                ),
            ]
        );
        assert!(catalog.get("textures/map/new").is_some());
        assert!(catalog.get("textures/base/floor").is_none());
        let original = ShaderCatalog::load_with_warnings(&base, |_, _| {});
        assert_eq!(
            catalog.get("textures/base/wall"),
            original.get("textures/base/wall")
        );
    }
}

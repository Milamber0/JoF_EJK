//! JKA archive authority at the adapter boundary; not a cryptographic trust claim.
use sjk_shader::ShaderCatalog;
use sjk_vfs::{AssetSource, VirtualFileSystem};
use std::path::Path;

fn authoritative(source: &AssetSource) -> bool {
    let path = Path::new(source.mount_name.as_ref());
    let base = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name.eq_ignore_ascii_case("base"));
    base && path.file_name().is_some_and(|name| {
        ["assets0.pk3", "assets1.pk3", "assets2.pk3", "assets3.pk3"]
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
    })
}

/// Load with the frozen startup policy, logging each discarded definition once per catalogue.
///
/// Downloaded paks come last and only add: their scripts cannot redefine a shader
/// the game or an installed pack defines, nor one named after an installed
/// texture (an implicit shader every map using that texture would pick up).
pub(crate) fn load(vfs: &VirtualFileSystem, protect: bool) -> ShaderCatalog {
    let downloads: Vec<_> = vfs
        .mounts()
        .filter(|mount| super::downloads::is_download(Path::new(mount.name.as_ref())))
        .map(|mount| mount.id)
        .collect();
    if downloads.is_empty() {
        return load_installed(vfs, protect);
    }
    let base = vfs.without_mounts(&downloads);
    let mut catalog = load_installed(&base, protect);
    let mut refused: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    catalog.add_unclaimed(
        vfs,
        |source| downloads.contains(&source.mount_id),
        |name| {
            let stem = ["tga", "png", "jpg"]
                .iter()
                .find_map(|extension| name.strip_suffix(&format!(".{extension}")))
                .unwrap_or(name);
            ["tga", "png", "jpg"].iter().any(|extension| {
                base.contains(&format!("{stem}.{extension}"))
                    .unwrap_or(false)
            })
        },
        warning,
        |name, loser| {
            refused
                .entry(loser.asset.mount_name.to_string())
                .or_default()
                .push(name.to_owned())
        },
    );
    for (pak, names) in refused {
        crate::log::progress(format_args!(
            "shader authority: download {pak} may not redefine {} shaders ({}{})",
            names.len(),
            names[..names.len().min(4)].join(", "),
            if names.len() > 4 { ", ..." } else { "" }
        ));
    }
    catalog
}

fn warning(path: &sjk_vfs::VirtualPath, error: &sjk_shader::ShaderError) {
    crate::log::progress(format_args!("warning: shader recovery in {path}: {error}"));
}

fn load_installed(vfs: &VirtualFileSystem, protect: bool) -> ShaderCatalog {
    if !protect {
        return ShaderCatalog::load_with_warnings(vfs, warning);
    }
    ShaderCatalog::load_with_authority(vfs, authoritative, warning, |name, winner, loser| {
        crate::log::progress(format_args!(
            "shader authority: suppressed {name}; winner={} [{}]; loser={} [{}]",
            winner.asset.mount_name, winner.script, loser.asset.mount_name, loser.script,
        ));
    })
}

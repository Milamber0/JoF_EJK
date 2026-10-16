//! Parser for Raven's Jedi Academy `.efx` effect definitions.

use sjk_vfs::VirtualFileSystem;
mod model;
mod parser;
mod parser_flags;
pub use model::*;

pub fn load_effect(vfs: &VirtualFileSystem, name: &str) -> Result<EffectDefinition, EffectError> {
    let path = effect_path(name);
    let asset = vfs
        .read(&path)?
        .ok_or_else(|| EffectError::Missing(path.clone()))?;
    parse_effect(&String::from_utf8_lossy(&asset.bytes))
}

pub fn effect_path(name: &str) -> String {
    let normalized = name.trim().replace('\\', "/");
    if normalized.to_ascii_lowercase().starts_with("effects/") {
        if normalized.to_ascii_lowercase().ends_with(".efx") {
            normalized
        } else {
            format!("{normalized}.efx")
        }
    } else if normalized.to_ascii_lowercase().ends_with(".efx") {
        format!("effects/{normalized}")
    } else {
        format!("effects/{normalized}.efx")
    }
}

pub fn parse_effect(source: &str) -> Result<EffectDefinition, EffectError> {
    parser::parse(source)
}

//! Shader image resolution shared by runtime effects and textures.
use image::RgbaImage;
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;

pub(crate) fn load_shader_image(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    shader: &str,
) -> Result<RgbaImage, Box<dyn Error>> {
    let path = shaders
        .resolve_image(vfs, shader)?
        .ok_or_else(|| format!("shader {shader} has no image"))?;
    let asset = vfs
        .read(path.as_str())?
        .ok_or_else(|| format!("shader image {path} is missing"))?;
    Ok(crate::decode_image(&asset.bytes, path.as_str())?.into_rgba8())
}

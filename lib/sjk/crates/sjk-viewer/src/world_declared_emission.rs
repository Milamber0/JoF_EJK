//! Resolve the spatial field of compiler-declared light without losing room-scale flux.
use super::super::emission::Texture;
use super::*;

/// Resolve a declared emitter's spatial profile and mean radiance at installation.
pub(super) fn resolve(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    definition: &sjk_shader::ShaderDefinition,
    inferred: Texture,
    fallback: Texture,
    cache: &mut ImageCache,
) -> Result<(Texture, [f32; 3]), Box<dyn Error>> {
    // Retain the previous compiler conversion and area-average flux. Replacing the
    // diffuse footprint with a thin glow must not remove the room's distant light.
    let old = fallback.mean().max(glam::Vec3::splat(0.05));
    let units = if definition.surface_light.is_finite() {
        definition.surface_light
    } else {
        0.
    };
    let power = old.dot(glam::Vec3::new(0.2126, 0.7152, 0.0722)) * units / 60.;
    // Explicit emission image, then visible glow, then diffuse fallback. A missing
    // compiler image must not turn a valid custom-map lamp into checkerboard light.
    let mut selected = None;
    if let Some(name) = &definition.light_image {
        let mut stage = material_stages(None, -3).0.remove(0);
        stage.images = vec![name.clone()];
        let (images, resolved, key) =
            load_stage_images(vfs, shaders, &stage, &definition.name, cache)?;
        if resolved || key.contains("$white;") {
            let mut texture = Texture::default();
            texture.observe(&stage, &images, true, true, false, false);
            selected = Some(texture);
        }
    }
    let (texture, mean) = if let Some(texture) = selected {
        let mean = texture.mean();
        (texture, mean)
    } else if inferred.mean().max_element() > 1e-8 {
        let mean = inferred.mean() / 4.;
        (inferred, mean)
    } else {
        let mean = fallback.mean();
        (fallback, mean)
    };
    let mean = if mean.max_element() > 1e-8 {
        mean
    } else {
        glam::Vec3::splat(0.05)
    };
    let luminance = mean.dot(glam::Vec3::new(0.2126, 0.7152, 0.0722));
    Ok((texture, (mean * (power / luminance)).to_array()))
}

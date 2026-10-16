//! Effect texture atlas construction and expansion when new EFX shaders arrive.

use super::*;

/// Whether a stage image names the renderer's built-in all-white image (the
/// `white` shader in `gfx.shader`, drawn by `CG_TestLine` lines).
fn is_white_image(name: &str) -> bool {
    name.eq_ignore_ascii_case("$whiteimage") || name.eq_ignore_ascii_case("*white")
}

/// Whether a stage's blend leaves the frame as it is (`GL_ZERO GL_ONE`, the `clear`
/// shader in `gfx2.shader`). Servers remap effects to `clear` to hide them; drawn with
/// the alpha fallback, its white image put an opaque white square where the effect was.
fn draws_nothing(blend: &StageBlend) -> bool {
    matches!(
        blend,
        StageBlend::Custom { source, destination }
            if source == "gl_zero" && destination == "gl_one"
    )
}

/// Whether a stage takes the effect's own colour, as rd-vanilla's `CGEN_VERTEX`,
/// `CGEN_EXACT_VERTEX` and `CGEN_ENTITY` do (`RB_CalcColors`); without an `rgbGen`, a
/// stage is `identity`.
fn takes_effect_colour(stage: &sjk_shader::ShaderStage) -> bool {
    matches!(
        stage.rgb_generator.as_deref(),
        Some("vertex" | "exactvertex" | "entity")
    )
}

/// The shared bind-group layout used by effect rendering and atlas expansion.
pub(crate) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("SJK effect texture atlas layout"),
        entries: &[
            texture_layout_entry(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Mip levels of the effect atlas: 128-pixel tiles down to 8.
const ATLAS_MIP_LEVELS: u32 = 5;
/// Side of one effect picture in the atlas.
const TILE: u32 = 128;
/// Border around each picture, its edge texels repeated, so filtering at the
/// smallest level (one texel of border there) never reaches the next picture.
const GUTTER: u32 = 1 << (ATLAS_MIP_LEVELS - 1);
/// One picture's cell: the picture and its border on every side. Cells are
/// multiples of `1 << (levels - 1)`, so every 2x2 box of every level stays in one
/// cell.
const CELL: u32 = TILE + 2 * GUTTER;

/// Build all requested shader tiles into the effect texture atlas.
pub(crate) fn create(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    required_effect_shaders: &BTreeSet<String>,
) -> Result<ParticleAtlas, Box<dyn Error>> {
    const EFFECT_SHADERS: &[&str] = &[
        player_shadows::SHADER,
        "gfx/misc/spark",
        "gfx/misc/spark2",
        "gfx/misc/steam",
        "gfx/misc/steam2",
        "gfx/effects/whiteflare",
        "gfx/effects/blaster_blob",
        "gfx/effects/blasterfrontflash",
        "gfx/effects/blastersideflash",
        "gfx/exp/explosion1",
        "gfx/exp/slower_rocket_explosion",
        "gfx/exp/rocket_explosion",
        "gfx/misc/dotfill_a",
        "gfx/effects/whiteflash",
        "gfx/effects/light_cone",
        "gfx/misc/smoke",
        "gfx/misc/black_smoke",
    ];
    struct PendingAnimation {
        shader: String,
        paths: Vec<sjk_vfs::VirtualPath>,
        /// One generated all-white frame instead of `paths`.
        white: bool,
        frequency: f32,
        one_shot: bool,
        blend: ParticleBlend,
        rgb_wave: Option<WaveForm>,
        alpha_wave: Option<WaveForm>,
        tc_scale: [f32; 2],
        tc_scroll: [f32; 2],
        glow: bool,
        tinted: bool,
    }
    let requested_shaders = EFFECT_SHADERS
        .iter()
        .map(|shader| (*shader).to_owned())
        .chain(required_effect_shaders.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut pending = Vec::new();
    // Shaders whose every stage draws nothing: an entry without stages, so effects (and
    // remaps onto them) draw nothing instead of the shader's image.
    let mut hidden = BTreeSet::new();
    for shader in &requested_shaders {
        let before = pending.len();
        let mut skipped = false;
        if let Some(definition) = shaders.get(shader) {
            for stage in definition
                .stages
                .iter()
                .filter(|stage| !stage.images.is_empty())
            {
                if draws_nothing(&stage.blend) {
                    skipped = true;
                    continue;
                }
                let mut paths = Vec::new();
                let white = stage.images.iter().any(|name| is_white_image(name));
                for name in &stage.images {
                    if name.starts_with('$') || name == "-" {
                        continue;
                    }
                    if let Some(path) = shaders.resolve_image(vfs, name)?
                        && !paths.contains(&path)
                    {
                        paths.push(path);
                    }
                }
                if !paths.is_empty() || white {
                    let (tc_scale, tc_scroll) =
                        effect_texcoords::compile(&stage.texture_modifications);
                    let blend = effect_runtime::particle_blend_for_stage(Some(&stage.blend));
                    if blend == ParticleBlend::Unsupported {
                        eprintln!(
                            "unsupported effect blend falls back to alpha: {shader} {:?}",
                            stage.blend
                        );
                    }
                    pending.push(PendingAnimation {
                        shader: shader.to_ascii_lowercase(),
                        white: white && paths.is_empty(),
                        paths,
                        frequency: stage.animation_frequency.unwrap_or(0.0),
                        one_shot: stage.one_shot,
                        blend,
                        rgb_wave: stage
                            .rgb_wave
                            .clone()
                            .or_else(|| constant_wave(stage.rgb_constant.and_then(grey))),
                        alpha_wave: stage
                            .alpha_wave
                            .clone()
                            .or_else(|| constant_wave(stage.alpha_constant)),
                        tc_scale,
                        tc_scroll,
                        glow: stage.glow,
                        tinted: takes_effect_colour(stage),
                    });
                }
            }
        }
        if skipped && pending.len() == before {
            hidden.insert(shader.to_ascii_lowercase());
            continue;
        }
        if effect_debug::enabled()
            && pending.len() == before
            && shaders.resolve_image(vfs, shader)?.is_none()
        {
            static REPORTED: std::sync::Mutex<BTreeSet<String>> =
                std::sync::Mutex::new(BTreeSet::new());
            if REPORTED
                .lock()
                .is_ok_and(|mut seen| seen.insert(shader.clone()))
            {
                log::progress(format_args!("effect shader {shader}: no image found"));
            }
        }
        if pending.len() == before
            && let Some(path) = shaders.resolve_image(vfs, shader)?
        {
            pending.push(PendingAnimation {
                shader: shader.to_ascii_lowercase(),
                paths: vec![path],
                white: false,
                frequency: 0.0,
                one_shot: false,
                blend: ParticleBlend::Alpha,
                rgb_wave: None,
                alpha_wave: None,
                tc_scale: [1.0; 2],
                tc_scroll: [0.0; 2],
                glow: false,
                tinted: true,
            });
        }
    }
    let tile_count = pending
        .iter()
        .map(|animation| animation.paths.len() + usize::from(animation.white))
        .sum::<usize>()
        .max(1);
    let columns = (tile_count as f32).sqrt().ceil() as u32;
    let rows = u32::try_from(tile_count)?.div_ceil(columns);
    let width = CELL * columns;
    let height = CELL * rows;
    let mut atlas = image::RgbaImage::new(width, height);
    let mut animations = HashMap::new();
    let mut tile_index = 0_u32;
    for animation in pending {
        let mut frames = Vec::new();
        let mut tiles = Vec::with_capacity(animation.paths.len() + 1);
        if animation.white {
            tiles.push(image::RgbaImage::from_pixel(
                TILE,
                TILE,
                image::Rgba([255; 4]),
            ));
        }
        for path in &animation.paths {
            let Some(asset) = vfs.read(path.as_str())? else {
                continue;
            };
            let Ok(decoded) = decode_image(&asset.bytes, path.as_str()) else {
                continue;
            };
            tiles.push(image::imageops::resize(
                &decoded.into_rgba8(),
                TILE,
                TILE,
                image::imageops::FilterType::Triangle,
            ));
        }
        for resized in tiles {
            let tile_x = tile_index % columns;
            let tile_y = tile_index / columns;
            let [left, top] = [tile_x * CELL + GUTTER, tile_y * CELL + GUTTER];
            place_tile(&mut atlas, &resized, left, top);
            let inset = 0.5;
            frames.push([
                (left as f32 + inset) / width as f32,
                (top as f32 + inset) / height as f32,
                ((left + TILE) as f32 - inset) / width as f32,
                ((top + TILE) as f32 - inset) / height as f32,
            ]);
            tile_index += 1;
        }
        if !frames.is_empty() {
            animations
                .entry(animation.shader)
                .or_insert_with(Vec::new)
                .push(ParticleAtlasAnimation {
                    frames,
                    frequency: animation.frequency,
                    one_shot: animation.one_shot,
                    blend: animation.blend,
                    rgb_wave: animation.rgb_wave,
                    alpha_wave: animation.alpha_wave,
                    tc_scale: animation.tc_scale,
                    tc_scroll: animation.tc_scroll,
                    glow: animation.glow,
                    tinted: animation.tinted,
                    time_offset: 0.,
                });
        }
    }
    for shader in hidden {
        animations.entry(shader).or_default();
    }
    let fallback = animations
        .get("gfx/misc/spark")
        .and_then(|animations| animations.first())
        .and_then(|animation| animation.frames.first())
        .copied()
        .unwrap_or([0.0, 0.0, 1.0 / columns as f32, 1.0 / rows as f32]);
    // Mipmapped like rd-vanilla's images: without levels, a small decal or far
    // sprite samples a few texels of its 128-pixel tile (a hard black dot for a
    // scorch mark). Five levels keep each tile at least 8 pixels; each tile's
    // border keeps its neighbours out of the filtering.
    let texture = crate::gpu_texture::upload_levels(
        device,
        queue,
        "SJK retail effect texture atlas",
        &premultiplied_levels(atlas, ATLAS_MIP_LEVELS),
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("SJK effect texture sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("SJK effect texture atlas bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let any_glow = animations
        .values()
        .flatten()
        .any(|animation| animation.glow);
    Ok(ParticleAtlas {
        bind_group,
        animations,
        fallback,
        any_glow,
        remaps: Default::default(),
    })
}

impl GpuState {
    /// Retain existing shader names when new graphs require additional tiles.
    pub(crate) fn refresh_effect_atlas(&mut self) -> Result<(), Box<dyn Error>> {
        let mut required = BTreeSet::new();
        self.effects.append_shader_paths(&mut required);
        if required
            .iter()
            .all(|name| self.particle_atlas.animations.contains_key(name))
        {
            return Ok(());
        }
        required.extend(self.particle_atlas.animations.keys().cloned());
        let vfs = self.vfs.as_ref().ok_or("no VFS")?;
        self.particle_atlas = create(
            &self.device,
            &self.queue,
            &layout(&self.device),
            vfs,
            &self.shaders,
            &required,
        )?;
        Ok(())
    }
}

/// `rgbGen const` / `alphaGen const` as a flat wave, so effect layers take a stage's
/// constant colour and opacity. JoF's HD scorch marks, for example, are drawn at 15%
/// grey and 80% opacity; without this they were drawn fully opaque in their texture's
/// own colour. Effect layers carry one brightness, so only a grey constant is taken.
fn constant_wave(value: Option<f32>) -> Option<WaveForm> {
    value.map(|base| WaveForm {
        function: "const".to_owned(),
        base: base.clamp(0.0, 1.0),
        amplitude: 0.0,
        phase: 0.0,
        frequency: 0.0,
    })
}

/// The brightness of a grey `rgbGen const`; a coloured one is left to the texture.
fn grey([r, g, b]: [f32; 3]) -> Option<f32> {
    ((r - g).abs() < 1e-3 && (g - b).abs() < 1e-3).then_some(r)
}

#[cfg(test)]
mod constant_colour_tests {
    use super::*;

    #[test]
    fn grey_constants_become_flat_waves() {
        let rgb = constant_wave(grey([0.15, 0.15, 0.15])).unwrap();
        assert!((crate::effect_wave::evaluate(Some(&rgb), 3.7) - 0.15).abs() < 1e-6);
        let alpha = constant_wave(Some(0.8)).unwrap();
        assert!((crate::effect_wave::evaluate(Some(&alpha), 0.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn coloured_constants_are_left_to_the_texture() {
        assert_eq!(grey([1.0, 0.2, 0.2]), None);
        assert!(constant_wave(None).is_none());
    }
}

/// Copy `tile` into `atlas` at `(left, top)` and fill its [`GUTTER`] with the
/// tile's edge texels, as clamp-to-edge sampling of the tile alone would see.
fn place_tile(atlas: &mut image::RgbaImage, tile: &image::RgbaImage, left: u32, top: u32) {
    let (width, height) = tile.dimensions();
    for y in 0..height + 2 * GUTTER {
        for x in 0..width + 2 * GUTTER {
            let source = tile.get_pixel(
                x.saturating_sub(GUTTER).min(width - 1),
                y.saturating_sub(GUTTER).min(height - 1),
            );
            atlas.put_pixel(left + x - GUTTER, top + y - GUTTER, *source);
        }
    }
}

/// `levels` mip levels of `image`, each texel the mean of its 2x2 parents weighted
/// by alpha (averaged premultiplied, stored straight), so a transparent texel's
/// colour never darkens or tints the visible ones next to it.
fn premultiplied_levels(image: image::RgbaImage, levels: u32) -> Vec<image::RgbaImage> {
    let mut chain = vec![image];
    for _ in 1..levels {
        let previous = chain.last().expect("the chain starts with the image");
        let (width, height) = previous.dimensions();
        if width < 2 || height < 2 {
            break;
        }
        let next = image::RgbaImage::from_fn(width / 2, height / 2, |x, y| {
            let mut sum = [0.0_f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let image::Rgba([r, g, b, a]) = *previous.get_pixel(2 * x + dx, 2 * y + dy);
                let alpha = f32::from(a) / 255.0;
                sum[0] += f32::from(r) * alpha;
                sum[1] += f32::from(g) * alpha;
                sum[2] += f32::from(b) * alpha;
                sum[3] += alpha;
            }
            if sum[3] <= 0.0 {
                return image::Rgba([0; 4]);
            }
            let colour = |channel: f32| (channel / sum[3]).round().clamp(0.0, 255.0) as u8;
            image::Rgba([
                colour(sum[0]),
                colour(sum[1]),
                colour(sum[2]),
                (sum[3] / 4.0 * 255.0).round() as u8,
            ])
        });
        chain.push(next);
    }
    chain
}

#[cfg(test)]
mod mip_tests {
    use super::*;

    #[test]
    fn transparent_texels_do_not_darken_their_neighbours() {
        // Opaque white beside transparent black: the half-covered texel stays white.
        let mut image = image::RgbaImage::new(2, 2);
        image.put_pixel(0, 0, image::Rgba([255, 255, 255, 255]));
        image.put_pixel(1, 0, image::Rgba([255, 255, 255, 255]));
        let levels = premultiplied_levels(image, 2);
        assert_eq!(
            *levels[1].get_pixel(0, 0),
            image::Rgba([255, 255, 255, 128])
        );
    }

    #[test]
    fn neighbouring_pictures_never_mix_at_any_level() {
        let red = image::RgbaImage::from_pixel(TILE, TILE, image::Rgba([255, 0, 0, 255]));
        let blue = image::RgbaImage::from_pixel(TILE, TILE, image::Rgba([0, 0, 255, 255]));
        let mut atlas = image::RgbaImage::new(2 * CELL, CELL);
        place_tile(&mut atlas, &red, GUTTER, GUTTER);
        place_tile(&mut atlas, &blue, CELL + GUTTER, GUTTER);
        let levels = premultiplied_levels(atlas, ATLAS_MIP_LEVELS);
        assert_eq!(levels.len(), ATLAS_MIP_LEVELS as usize);
        let last = levels.last().unwrap();
        let cell = CELL >> (ATLAS_MIP_LEVELS - 1);
        // Every texel of each cell, border included, is that picture's colour.
        for y in 0..cell {
            for x in 0..cell {
                assert_eq!(*last.get_pixel(x, y), image::Rgba([255, 0, 0, 255]));
                assert_eq!(*last.get_pixel(cell + x, y), image::Rgba([0, 0, 255, 255]));
            }
        }
    }
}

#[cfg(test)]
mod shader_tests {
    /// The atlas samplers take explicit gradients; both programs must still validate.
    #[test]
    fn atlas_sampling_programs_validate() {
        crate::wgsl_source::validate(include_str!("effect_geometry.wgsl"));
        crate::wgsl_source::validate(include_str!("entity.wgsl"));
    }
}

#[cfg(test)]
mod stage_tests {
    use super::*;

    fn stage(text: &str) -> sjk_shader::ShaderStage {
        sjk_shader::parse_shader_script(text.as_bytes(), "shaders/test.shader")
            .unwrap()
            .remove(0)
            .stages
            .remove(0)
    }

    #[test]
    fn the_clear_shader_draws_nothing() {
        // gfx2.shader's `clear`, which servers remap effects onto to hide them.
        let clear = stage(
            "clear
{
 {
 map $whiteimage
 blendFunc GL_ZERO GL_ONE
 }
}
",
        );
        assert!(draws_nothing(&clear.blend));
        let flash = stage(
            "gfx/effects/whiteflash
{
 {
 map $whiteimage
 blendFunc GL_ONE GL_ONE
 }
}
",
        );
        assert!(!draws_nothing(&flash.blend));
    }

    #[test]
    fn only_vertex_colour_stages_take_the_effect_colour() {
        let shock_ball = stage(
            "gfx/effects/shock_ball
{
 {
 map gfx/effects/shock_ball
              blendFunc GL_DST_COLOR GL_SRC_COLOR
 rgbGen identity
 }
}
",
        );
        assert!(!takes_effect_colour(&shock_ball));
        let unspecified = stage(
            "gfx/effects/scorch
{
 {
 map gfx/effects/scorch
 }
}
",
        );
        assert!(!takes_effect_colour(&unspecified));
        for generator in ["vertex", "exactVertex", "entity"] {
            let tinted = stage(&format!(
                "gfx/misc/steam
{{
 {{
 map gfx/misc/steam
 rgbGen {generator}
 }}
}}
"
            ));
            assert!(takes_effect_colour(&tinted), "{generator}");
        }
    }
}

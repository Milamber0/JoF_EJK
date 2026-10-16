//! GPU side of the menu-file HUD: one atlas of the HUD's pictures, packed at
//! load, and a fixed vertex batch drawn with the scope's textured-quad program.
//!
//! 2D pictures blend as their shader's first stage says; a picture without a
//! shader script gets the implicit 2D shader, which alpha-blends. Additive
//! pictures (`blendFunc GL_ONE GL_ONE`) go in a second batch after the
//! alpha-blended ones.

use super::frame::{Frame, MAX_PICTURES};
use super::layout::Side;
use bytemuck::{Pod, Zeroable};
use sjk_shader::{ShaderCatalog, StageBlend};
use sjk_vfs::VirtualFileSystem;
use wgpu::util::DeviceExt;

const MAX_VERTICES: usize = MAX_PICTURES * 6;
const ATLAS_WIDTH: u32 = 2_048;
const ATLAS_MAX_HEIGHT: u32 = 4_096;
/// Gap between packed pictures, so filtering never reaches a neighbour.
const PADDING: u32 = 2;
/// Texels per 640x480 unit at 2160 lines: a picture is stored no larger
/// than a 4K screen draws it.
const TEXELS_PER_UNIT: f32 = 2_160.0 / 480.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// Where one picture sits in the atlas.
#[derive(Clone, Copy)]
struct Entry {
    uv: [f32; 4],
    additive: bool,
}

struct Atlas {
    bind_group: wgpu::BindGroup,
    entries: Vec<Option<Entry>>,
}

/// Pipelines and vertex storage, created with the device; the atlas follows
/// each HUD load.
pub(super) struct Gpu {
    pipelines: [wgpu::RenderPipeline; 2],
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    buffer: wgpu::Buffer,
    vertices: Vec<Vertex>,
    additive: Vec<Vertex>,
    atlas: Option<Atlas>,
    /// Alpha-blended and additive vertex counts uploaded this frame.
    counts: [u32; 2],
}

impl Gpu {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("menu HUD pictures"),
            entries: &[
                crate::render_helpers::texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("menu HUD pictures"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // The scope's program is exactly a textured, vertex-coloured quad.
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("menu HUD"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../scope.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("menu HUD"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let pipelines = [
            wgpu::BlendState::ALPHA_BLENDING,
            wgpu::BlendState {
                color: add,
                alpha: wgpu::BlendComponent::OVER,
            },
        ]
        .map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("menu HUD"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &ATTRIBUTES,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("menu HUD quads"),
            size: (MAX_VERTICES * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipelines,
            layout,
            sampler,
            buffer,
            vertices: Vec::with_capacity(MAX_VERTICES),
            additive: Vec::with_capacity(MAX_VERTICES),
            atlas: None,
            counts: [0; 2],
        }
    }

    /// Decode `names` (shader or image names) and pack them into a new atlas.
    /// `largest` is each picture's largest drawn side in 640x480 units.
    /// Returns how many pictures were found.
    pub(super) fn load(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        names: &[String],
        largest: &[f32],
    ) -> usize {
        let decoded: Vec<Option<(image::RgbaImage, bool)>> = names
            .iter()
            .map(|name| {
                let image = crate::shader_image::load_shader_image(vfs, shaders, name).ok()?;
                let additive = shaders
                    .get(name)
                    .and_then(|definition| definition.stages.first())
                    .is_some_and(|stage| stage.blend == StageBlend::Add);
                Some((image, additive))
            })
            .collect();
        let found = decoded.iter().flatten().count();
        let mut limit_scale = 1.0;
        let (packed, sizes, height) = loop {
            let sizes: Vec<Option<[u32; 2]>> = decoded
                .iter()
                .zip(largest)
                .map(|(image, &side)| {
                    let (image, _) = image.as_ref()?;
                    Some(stored_size(
                        [image.width(), image.height()],
                        side * limit_scale,
                    ))
                })
                .collect();
            if let Some((placements, height)) = pack(&sizes) {
                break (placements, sizes, height);
            }
            limit_scale *= 0.5;
        };
        let mut atlas = image::RgbaImage::from_pixel(ATLAS_WIDTH, height, image::Rgba([0; 4]));
        let mut entries = Vec::with_capacity(names.len());
        for ((image, size), placement) in decoded.iter().zip(&sizes).zip(&packed) {
            let (Some((image, additive)), Some([width, height_px]), Some([x, y])) =
                (image, size, placement)
            else {
                entries.push(None);
                continue;
            };
            let scaled = if [image.width(), image.height()] == [*width, *height_px] {
                image.clone()
            } else {
                image::imageops::resize(
                    image,
                    *width,
                    *height_px,
                    image::imageops::FilterType::Triangle,
                )
            };
            image::imageops::replace(&mut atlas, &scaled, i64::from(*x), i64::from(*y));
            // Half a texel in, so linear filtering stays inside the picture.
            let (atlas_width, atlas_height) = (ATLAS_WIDTH as f32, height as f32);
            entries.push(Some(Entry {
                uv: [
                    (*x as f32 + 0.5) / atlas_width,
                    (*y as f32 + 0.5) / atlas_height,
                    (*x + *width) as f32 / atlas_width - 0.5 / atlas_width,
                    (*y + *height_px) as f32 / atlas_height - 0.5 / atlas_height,
                ],
                additive: *additive,
            }));
        }
        let texture = device.create_texture_with_data(
            queue.raw(),
            &wgpu::TextureDescriptor {
                label: Some("menu HUD atlas"),
                size: wgpu::Extent3d {
                    width: ATLAS_WIDTH,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::ui_target::TEXTURE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            atlas.as_raw(),
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("menu HUD atlas"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.atlas = Some(Atlas {
            bind_group,
            entries,
        });
        found
    }

    /// Drop the atlas (the HUD is off or failed to load).
    pub(super) fn unload(&mut self) {
        self.atlas = None;
        self.counts = [0; 2];
    }

    /// Turn this frame's pictures into vertices and upload them.
    pub(super) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        frame: Option<&Frame>,
        viewport: [f32; 2],
        scale: f32,
    ) {
        self.counts = [0; 2];
        let (Some(atlas), Some(frame)) = (&self.atlas, frame) else {
            return;
        };
        self.vertices.clear();
        self.additive.clear();
        for picture in &frame.pictures[..frame.picture_count] {
            let Some(Some(entry)) = atlas.entries.get(usize::from(picture.picture)) else {
                continue;
            };
            let [x, y, width, height] = to_pixels(picture.rect, picture.side, viewport, scale);
            let target = if entry.additive {
                &mut self.additive
            } else {
                &mut self.vertices
            };
            let ndc =
                |px: f32, py: f32| [px / viewport[0] * 2.0 - 1.0, 1.0 - py / viewport[1] * 2.0];
            let [u0, v0, u1, v1] = entry.uv;
            // A negative width or height mirrors the picture, as retail's
            // stretch-pic does for the right HUD frame.
            let corners = [
                (x, y, u0, v0),
                (x + width, y, u1, v0),
                (x + width, y + height, u1, v1),
                (x, y, u0, v0),
                (x + width, y + height, u1, v1),
                (x, y + height, u0, v1),
            ];
            target.extend(corners.map(|(px, py, u, v)| Vertex {
                position: ndc(px, py),
                uv: [u, v],
                color: picture.color,
            }));
        }
        self.counts = [self.vertices.len() as u32, self.additive.len() as u32];
        self.vertices.extend_from_slice(&self.additive);
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
    }

    /// Draw the uploaded pictures, alpha-blended first, then additive.
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(atlas) = &self.atlas else {
            return;
        };
        let [alpha, additive] = self.counts;
        if alpha + additive == 0 {
            return;
        }
        pass.set_bind_group(0, &atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        if alpha != 0 {
            pass.set_pipeline(&self.pipelines[0]);
            pass.draw(0..alpha, 0..1);
        }
        if additive != 0 {
            pass.set_pipeline(&self.pipelines[1]);
            pass.draw(alpha..alpha + additive, 0..1);
        }
    }
}

/// A 640x480 rectangle in physical pixels. The HUD keeps its 4:3 shape on
/// wider screens and sticks to the edge its menu belongs to, as EternalJK's
/// `widthRatioCoef` places the HUD (`cg_draw.c` `CG_DrawHealth`): left
/// menus scale from the left edge, right menus from the right. Everything
/// is anchored to the bottom, so `scale` (`cg_hudScale`) grows the HUD
/// out of its corner.
pub(super) fn to_pixels(rect: [f32; 4], side: Side, viewport: [f32; 2], scale: f32) -> [f32; 4] {
    let [width, height] = viewport;
    let unit = height / 480.0 * scale;
    let [x, y, w, h] = rect;
    let px = match side {
        Side::Left => x * unit,
        Side::Right => width - (640.0 - x) * unit,
    };
    [px, height - (480.0 - y) * unit, w * unit, h * unit]
}

/// Stored size of a `size` picture drawn at most `largest` units: no larger
/// than a 4K screen draws it, as a power of two between 16 and 512 texels,
/// never enlarged, aspect kept.
fn stored_size(size: [u32; 2], largest: f32) -> [u32; 2] {
    let limit = ((largest.abs() * TEXELS_PER_UNIT).ceil().max(1.0) as u32)
        .next_power_of_two()
        .clamp(16, 512);
    let [width, height] = size;
    let longest = width.max(height).max(1);
    if longest <= limit {
        return [width.max(1), height.max(1)];
    }
    let factor = limit as f32 / longest as f32;
    [
        ((width as f32 * factor).round() as u32).max(1),
        ((height as f32 * factor).round() as u32).max(1),
    ]
}

/// Shelf-pack `sizes` into an [`ATLAS_WIDTH`]-wide atlas; the height is the
/// next power of two that holds them, or `None` past [`ATLAS_MAX_HEIGHT`].
fn pack(sizes: &[Option<[u32; 2]>]) -> Option<(Vec<Option<[u32; 2]>>, u32)> {
    let mut order: Vec<usize> = (0..sizes.len()).filter(|&i| sizes[i].is_some()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i].map_or(0, |[_, h]| h)));
    let mut placements = vec![None; sizes.len()];
    let (mut x, mut y, mut row) = (PADDING, PADDING, 0);
    for index in order {
        let [width, height] = sizes[index]?;
        if width + 2 * PADDING > ATLAS_WIDTH {
            return None;
        }
        if x + width + PADDING > ATLAS_WIDTH {
            x = PADDING;
            y += row + PADDING;
            row = 0;
        }
        placements[index] = Some([x, y]);
        x += width + PADDING;
        row = row.max(height);
    }
    let height = (y + row + PADDING).next_power_of_two().max(16);
    (height <= ATLAS_MAX_HEIGHT).then_some((placements, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_menus_keep_to_the_right_edge_on_wide_screens() {
        // 1920x1080: 2.25 px per unit, 4:3 kept.
        let viewport = [1_920.0, 1_080.0];
        let left = to_pixels([0.0, 368.0, 112.0, 112.0], Side::Left, viewport, 1.0);
        assert_eq!(left, [0.0, 828.0, 252.0, 252.0]);
        let right = to_pixels([640.0, 368.0, -112.0, 112.0], Side::Right, viewport, 1.0);
        assert_eq!(right, [1_920.0, 828.0, -252.0, 252.0]);
        // At 4:3 the placement is retail's plain 640x480 stretch.
        let square = to_pixels(
            [592.0, 393.0, 28.0, 28.0],
            Side::Right,
            [1_440.0, 1_080.0],
            1.0,
        );
        assert_eq!(square, [592.0 * 2.25, 393.0 * 2.25, 63.0, 63.0]);
        // cg_hudScale grows from the bottom corner.
        let scaled = to_pixels([0.0, 368.0, 112.0, 112.0], Side::Left, viewport, 2.0);
        assert_eq!(scaled, [0.0, 1_080.0 - 112.0 * 4.5, 504.0, 504.0]);
    }

    #[test]
    fn pictures_are_stored_no_larger_than_4k_draws_them() {
        // The retail frame: 256x256, drawn 112 units (504 px at 4K).
        assert_eq!(stored_size([256, 256], 112.0), [256, 256]);
        // An 8x HD frame is reduced to 512.
        assert_eq!(stored_size([2_048, 1_024], 112.0), [512, 256]);
        // A 6x12 digit (54 px at 4K) keeps 64 texels at most.
        assert_eq!(stored_size([128, 256], 12.0), [32, 64]);
        assert_eq!(stored_size([8, 16], 12.0), [8, 16]);
    }

    #[test]
    fn packing_fits_shelves_and_rejects_oversize() {
        let sizes = [Some([512, 512]), None, Some([1_600, 64]), Some([600, 300])];
        let (placements, height) = pack(&sizes).unwrap();
        assert_eq!(placements[1], None);
        assert_eq!(placements[0], Some([PADDING, PADDING]));
        assert_eq!(placements[3], Some([PADDING + 512 + PADDING, PADDING]));
        assert_eq!(placements[2], Some([PADDING, PADDING + 512 + PADDING]));
        assert_eq!(height, 1_024);
        assert!(pack(&[Some([4_000, 16])]).is_none());
    }
}

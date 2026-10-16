//! Player outlines: a ring around the outside of a body, and only the outside (an arm
//! across the chest or the gap between the legs draws no line). Two kinds share it:
//!
//! - the player card's ([`crate::hud::player_card`]): while a card is up, its player
//!   in the card's accent colour, fading with the card;
//! - the team outlines ([`crate::hud::team_outline`]): in team games, every enemy in
//!   red and every teammate in green.
//!
//! Two steps. After the scene, each outlined body is drawn once more through its usual
//! vertex path into a one-channel mask, against the scene's depth, so the mask holds
//! only the part of the body that is in view (none through walls); the queue keeps
//! those draws apart from every other pass ([`crate::entity_materials::Queue::outline`]).
//! Each body writes its [`Group`]'s value, so one mask serves every ring. Then the 2D
//! layer, after anti-aliasing and the colour grade so the colours are exact, draws the
//! rings: every pixel outside the mask within [`RADIUS`] pixels of it, in the colour of
//! the body it is nearest, with a soft outer edge, only inside scissors around the
//! outlined players (overlapping ones merged, so no pixel is drawn twice).

use sjk_vfs::{VfsError, VirtualFileSystem};
use wgpu::util::DeviceExt;

/// The card outline's mask material: one plain stage, no deforms, so the body's
/// vertices land where the scene put them.
pub(crate) const MASK_SHADER: &str = "sjk/cardOutlineMask";
/// The team outlines' mask materials, the same stage under their own names so the
/// draw queue tells the groups apart.
pub(crate) const FRIEND_MASK_SHADER: &str = "sjk/teamOutlineFriend";
pub(crate) const ENEMY_MASK_SHADER: &str = "sjk/teamOutlineEnemy";
/// The mask image's format.
pub(crate) const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// The ring's width in pixels of a 1080-line screen; it scales with the screen height.
/// Half the first ring's 4 (Sol's request, 13/10/2026): 4 pixels on a 4K screen.
pub(crate) const RADIUS: f32 = 2.0;
/// The thinnest ring, on a small window: below this it fades to a hairline.
const MIN_RADIUS: f32 = 1.5;
/// Most mask lookups along one direction (the radius in steps of about a pixel).
const MAX_STEPS: f32 = 8.0;
/// Most players outlined at once: every client slot and the card's.
const MAX_RINGS: usize = 33;
/// The team rings' colours (display values): green for teammates, red for enemies.
pub(crate) const FRIEND_COLOR: [u8; 4] = [70, 230, 95, 235];
pub(crate) const ENEMY_COLOR: [u8; 4] = [255, 60, 50, 235];

const SHADER_FILE: &str = "// SJK: the player outlines' masks (card_outline.rs).
sjk/cardOutlineMask
{
\tcull none
\t{
\t\tmap $whiteimage
\t\trgbGen identity
\t}
}
sjk/teamOutlineFriend
{
\tcull none
\t{
\t\tmap $whiteimage
\t\trgbGen identity
\t}
}
sjk/teamOutlineEnemy
{
\tcull none
\t{
\t\tmap $whiteimage
\t\trgbGen identity
\t}
}
";

/// Mount the shader file, below game data like SJK's other content.
pub(crate) fn mount(vfs: &mut VirtualFileSystem) -> Result<(), VfsError> {
    vfs.mount_memory(
        "SJK card outline",
        [("shaders/sjk_card_outline.shader", SHADER_FILE.as_bytes())],
    )?;
    Ok(())
}

/// Whose ring a body in the mask gets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Group {
    /// The player whose card is up.
    Card,
    /// A teammate.
    Friend,
    /// A player of the other team.
    Enemy,
}

impl Group {
    pub(crate) const ALL: [Self; 3] = [Self::Card, Self::Friend, Self::Enemy];

    /// The group's mask material.
    pub(crate) const fn shader(self) -> &'static str {
        match self {
            Self::Card => MASK_SHADER,
            Self::Friend => FRIEND_MASK_SHADER,
            Self::Enemy => ENEMY_MASK_SHADER,
        }
    }

    /// What the group's bodies write into the mask; `card_outline.wgsl` reads it back
    /// with thresholds halfway between.
    pub(crate) const fn mask_value(self) -> f64 {
        match self {
            Self::Card => 1.0,
            Self::Enemy => 2.0 / 3.0,
            Self::Friend => 1.0 / 3.0,
        }
    }
}

/// The ring's width in output pixels for a screen `height` pixels tall.
pub(crate) fn radius(height: f32) -> f32 {
    (RADIUS * height / 1080.0).clamp(MIN_RADIUS, MAX_STEPS)
}

/// The card's outline this frame: its colour (display values, alpha the card's fade)
/// and the screen area it may cover, in output pixels (`[x, y, width, height]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub(crate) color: [u8; 4],
    pub(crate) area: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    card: [f32; 4],
    friend: [f32; 4],
    enemy: [f32; 4],
    /// Output size in pixels, then the radius in pixels and its step count.
    shape: [f32; 4],
}

/// The mask image, the ring's program and its settings.
pub(crate) struct Gpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    /// The mask at the scene's depth size, its view and the ring's bind group.
    mask: Option<([u32; 2], wgpu::TextureView, wgpu::BindGroup)>,
    /// This frame's scissors, in output pixels, none overlapping another.
    scissors: [[u32; 4]; MAX_RINGS],
    scissor_count: usize,
}

impl Gpu {
    /// The ring's program for the 2D layer's `format`; the mask comes with the first
    /// outline.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("card outline"),
            entries: &[
                crate::render_helpers::texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("card outline"),
            source: wgpu::ShaderSource::Wgsl(include_str!("card_outline.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("card outline"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("card outline ring"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            // The 2D layer's pass carries the scene's depth, never tested here.
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
        });
        // Nearest: the mask holds group values, which a blend between texels would
        // turn into another group's.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("card outline mask"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("card outline"),
            contents: bytemuck::bytes_of(&Uniform {
                card: [0.0; 4],
                friend: [0.0; 4],
                enemy: [0.0; 4],
                shape: [1.0, 1.0, 1.0, 1.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            layout,
            sampler,
            uniform,
            mask: None,
            scissors: [[0; 4]; MAX_RINGS],
            scissor_count: 0,
        }
    }

    /// Prepare this frame's outlines: `card` when a card is up with its outline on, the
    /// screen areas of the team-outlined players (`team`), the scene's depth `size`
    /// and the output's. With none on screen, nothing is drawn and the mask is kept
    /// for the next one.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        card: Option<Frame>,
        team: &[[f32; 4]],
        size: [u32; 2],
        output: [u32; 2],
    ) {
        let card = card.filter(|frame| frame.color[3] > 0);
        self.scissor_count = 0;
        for area in card.map(|frame| frame.area).iter().chain(team) {
            if self.scissor_count == MAX_RINGS {
                break;
            }
            if let Some(scissor) = scissor(*area, output) {
                self.scissors[self.scissor_count] = scissor;
                self.scissor_count += 1;
            }
        }
        self.scissor_count = merge(&mut self.scissors[..self.scissor_count]);
        if self.scissor_count == 0 {
            return;
        }
        if self.mask.as_ref().is_none_or(|(at, _, _)| *at != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("card outline mask"),
                size: wgpu::Extent3d {
                    width: size[0].max(1),
                    height: size[1].max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: MASK_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("card outline"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            });
            self.mask = Some((size, view, group));
        }
        let radius = radius(output[1] as f32);
        let color = |bytes: [u8; 4]| bytes.map(|channel| f32::from(channel) / 255.0);
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&Uniform {
                card: card.map_or([0.0; 4], |frame| color(frame.color)),
                friend: color(FRIEND_COLOR),
                enemy: color(ENEMY_COLOR),
                shape: [output[0] as f32, output[1] as f32, radius, radius.ceil()],
            }),
        );
    }

    /// The mask to draw the bodies into this frame, when there is an outline on screen.
    pub(crate) fn mask_target(&self) -> Option<&wgpu::TextureView> {
        if self.scissor_count == 0 {
            return None;
        }
        self.mask.as_ref().map(|(_, view, _)| view)
    }

    /// Draw the rings into the 2D layer's `pass`, `output` pixels large.
    pub(crate) fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>, output: [u32; 2]) {
        let Some((_, _, group)) = &self.mask else {
            return;
        };
        if self.scissor_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, group, &[]);
        for [x, y, width, height] in &self.scissors[..self.scissor_count] {
            pass.set_scissor_rect(*x, *y, *width, *height);
            pass.draw(0..3, 0..1);
        }
        pass.set_scissor_rect(0, 0, output[0], output[1]);
    }
}

/// `area` grown by the ring's width and cut to the `output`, in whole pixels; `None`
/// when nothing of it is on screen.
fn scissor(area: [f32; 4], output: [u32; 2]) -> Option<[u32; 4]> {
    let grow = radius(output[1] as f32) + 2.0;
    let [w, h] = output.map(|value| value as f32);
    let left = (area[0] - grow).clamp(0.0, w);
    let top = (area[1] - grow).clamp(0.0, h);
    let right = (area[0] + area[2] + grow).clamp(0.0, w);
    let bottom = (area[1] + area[3] + grow).clamp(0.0, h);
    if right - left < 1.0 || bottom - top < 1.0 {
        return None;
    }
    let (x, y) = (left.floor() as u32, top.floor() as u32);
    Some([
        x,
        y,
        (right.ceil() as u32).min(output[0]) - x,
        (bottom.ceil() as u32).min(output[1]) - y,
    ])
}

/// Replace overlapping scissors in `rects` by their bounding box until none overlap,
/// so the ring, drawn once per scissor, covers each pixel once; returns how many are
/// left at the front of `rects`. Players stand apart, so the boxes stay small.
fn merge(rects: &mut [[u32; 4]]) -> usize {
    let overlap = |a: [u32; 4], b: [u32; 4]| {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    };
    let mut count = rects.len();
    loop {
        let pair = (0..count)
            .flat_map(|i| (i + 1..count).map(move |j| (i, j)))
            .find(|&(i, j)| overlap(rects[i], rects[j]));
        let Some((i, j)) = pair else {
            return count;
        };
        let (a, b) = (rects[i], rects[j]);
        let left = a[0].min(b[0]);
        let top = a[1].min(b[1]);
        let right = (a[0] + a[2]).max(b[0] + b[2]);
        let bottom = (a[1] + a[3]).max(b[1] + b[3]);
        rects[i] = [left, top, right - left, bottom - top];
        rects[j] = rects[count - 1];
        count -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mask_shaders_are_one_plain_two_sided_stage_each() {
        let shaders =
            sjk_shader::parse_shader_script(SHADER_FILE.as_bytes(), "card_outline").unwrap();
        assert_eq!(shaders.len(), Group::ALL.len());
        for (shader, group) in shaders.iter().zip(Group::ALL) {
            assert_eq!(shader.name, group.shader());
            assert_eq!(shader.cull, sjk_shader::ShaderCull::TwoSided);
            assert!(shader.deforms.is_empty());
            assert_eq!(shader.stages.len(), 1);
        }
    }

    #[test]
    fn the_groups_mask_values_stand_apart_from_the_rings_thresholds() {
        // card_outline.wgsl: above 0.83 the card, above 0.5 an enemy, above 0.16 a
        // friend. An 8-bit mask stores each within half a step.
        let thresholds = [0.83, 0.5, 0.16];
        for (group, floor) in [Group::Card, Group::Enemy, Group::Friend]
            .into_iter()
            .zip(thresholds)
        {
            let stored = (group.mask_value() * 255.0).round() / 255.0;
            assert!(stored > floor + 0.1, "{group:?}");
        }
        assert!((Group::Enemy.mask_value() * 255.0).round() / 255.0 < 0.83 - 0.1);
        assert!((Group::Friend.mask_value() * 255.0).round() / 255.0 < 0.5 - 0.1);
    }

    #[test]
    fn the_ring_program_validates() {
        crate::wgsl_source::validate(include_str!("card_outline.wgsl"));
    }

    #[test]
    fn the_ring_widens_with_the_screen() {
        assert_eq!(radius(1080.0), RADIUS);
        assert_eq!(radius(2160.0), RADIUS * 2.0);
        // Half the first ring's width.
        assert_eq!(radius(1080.0), 2.0);
        // Never a hairline, never more lookups than the shader makes.
        assert_eq!(radius(240.0), MIN_RADIUS);
        assert_eq!(radius(100_000.0), MAX_STEPS);
    }

    #[test]
    fn the_scissor_grows_by_the_ring_and_stays_on_screen() {
        let output = [1920, 1080];
        let grow = RADIUS + 2.0;
        assert_eq!(
            scissor([100.0, 200.0, 50.0, 120.0], output),
            Some([
                (100.0 - grow) as u32,
                (200.0 - grow) as u32,
                (50.0 + 2.0 * grow) as u32,
                (120.0 + 2.0 * grow) as u32,
            ])
        );
        // Cut at the edges.
        assert_eq!(
            scissor([-500.0, -500.0, 600.0, 4000.0], output),
            Some([0, 0, (100.0 + grow).ceil() as u32, 1080])
        );
        // Wholly off screen.
        assert_eq!(scissor([3000.0, 10.0, 50.0, 50.0], output), None);
    }

    #[test]
    fn overlapping_scissors_merge_and_apart_ones_stay() {
        let mut rects = [
            [0, 0, 10, 10],
            [100, 100, 10, 10],
            [5, 5, 10, 10],
            // Overlaps the third and the second: all four become one box.
            [12, 12, 95, 95],
            [500, 0, 10, 10],
        ];
        let count = merge(&mut rects);
        assert_eq!(count, 2);
        let mut kept = rects[..count].to_vec();
        kept.sort();
        assert_eq!(kept, [[0, 0, 110, 110], [500, 0, 10, 10]]);
        // Touching edges do not overlap.
        let mut rects = [[0, 0, 10, 10], [10, 0, 10, 10]];
        assert_eq!(merge(&mut rects), 2);
        assert_eq!(merge(&mut []), 0);
    }
}

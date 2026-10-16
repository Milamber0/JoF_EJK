//! Sampled crosshair policy and retained position telemetry; no second target scan.
use super::*;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_ui::{DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};
use std::fmt::Write;

/// Register supported options and the old crosshair spelling before config restoration.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    super::identification::register(cvars)?;
    super::player_card::register(cvars)?;
    super::nameplate::register(cvars)?;
    super::team_outline::register(cvars)?;
    super::tints::register(cvars)?;
    super::enemy_info::register(cvars)?;
    cvars.register(CvarDefinition::new(
        "cg_dynamicCrosshair",
        1_i64,
        CvarFlags::ARCHIVE,
        "On-foot muzzle crosshair: 0 static, 1 dynamic, 2 selective; box traces only",
    ))?;
    cvars.register_alias("cg_crosshair", "cg_drawCrosshair")?;
    for (name, value, help) in [
        (
            "cg_crosshairX",
            0.0,
            "Crosshair horizontal offset in virtual units",
        ),
        (
            "cg_crosshairY",
            0.0,
            "Crosshair vertical offset in virtual units",
        ),
        (
            "cg_lagometerX",
            48.0,
            "Lagometer left edge measured from the right",
        ),
        (
            "cg_lagometerY",
            144.0,
            "Lagometer top edge measured from the bottom",
        ),
        (
            "cg_lagometerScale",
            1.0,
            "Lagometer size multiplier (0.25 to 4), anchored at its bottom right",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for (name, value, help) in [
        (
            "cg_crosshairIdentifyTarget",
            true,
            "Classify the traced entity by team and ownership",
        ),
        (
            "cg_crosshairSizeScale",
            true,
            "Scale crosshair with the viewport",
        ),
        (
            "cg_crosshairHealth",
            false,
            "Color crosshair using local health and armor",
        ),
        (
            "cg_crosshairSaberStyleColor",
            false,
            "Color crosshair using the drawn saber style",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    cvars.register(CvarDefinition::new(
        "cg_showpos",
        false,
        CvarFlags::NONE,
        "Show position, angles and three-dimensional speed",
    ))?;
    Ok(())
}

/// Scalar settings sampled once during the normal HUD update.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    offset: [f32; 2],
    scaled: bool,
    health: bool,
    identify: bool,
    style: bool,
    showpos: bool,
    lagometer: [f32; 2],
    lagometer_scale: f32,
    color: [f32; 4],
    /// The classic crosshair's picture and size.
    pub(crate) look: super::crosshair::Look,
}

impl Policy {
    fn color_for(self, health: i32, armor: i32, weapon: u8, style: u32) -> [f32; 4] {
        if self.health {
            health_color(health, armor)
        } else if self.style && weapon == 3 {
            style_color(style, self.color[3])
        } else {
            self.color
        }
    }

    fn read(console: Option<&ViewerConsole>) -> Self {
        let f = |name, fallback| crate::cgame_options::scalar(console, name, fallback);
        let b = |name, fallback| console.and_then(|c| c.bool_cvar(name)).unwrap_or(fallback);
        Self {
            offset: [f("cg_crosshairx", 0.0), f("cg_crosshairy", 0.0)],
            scaled: b("cg_crosshairsizescale", true),
            health: b("cg_crosshairhealth", false),
            identify: b("cg_crosshairidentifytarget", true),
            style: b("cg_crosshairsaberstylecolor", false),
            showpos: b("cg_showpos", false),
            lagometer: [f("cg_lagometerx", 48.0), f("cg_lagometery", 144.0)],
            lagometer_scale: f("cg_lagometerscale", 1.0).clamp(0.25, 4.0),
            color: crate::cgame_options::crosshair_color(console),
            look: super::crosshair::Look::new(
                console
                    .and_then(|c| c.integer_cvar("cg_drawcrosshair"))
                    .unwrap_or(1),
                super::options::crosshair_size(console, true),
                b("cg_crosshairsizescale", true),
            ),
        }
    }

    /// Shader offset in normalized screen coordinates and size-coordinate dimensions.
    pub(crate) fn parameters(self, viewport: [f32; 2]) -> [f32; 4] {
        let dimensions = if self.scaled {
            [viewport[0] * 480.0 / viewport[1].max(1.0), 480.0]
        } else {
            viewport
        };
        [
            self.offset[0] / 640.0,
            // Full-screen triangle UVs increase upward; stock Y increases downward.
            -self.offset[1] / 480.0,
            dimensions[0],
            dimensions[1],
        ]
    }

    /// Scale the graph around its existing bottom-right corner, retaining X/Y offsets.
    pub(super) fn lagometer_rect(self, rect: Rect, viewport: [f32; 2]) -> Rect {
        // TaystJK cg_draw.c:7175-7176; horizontal virtual units are aspect corrected.
        let scale = viewport[1] / 480.0;
        let width = rect.width * self.lagometer_scale;
        let height = rect.height * self.lagometer_scale;
        Rect::new(
            viewport[0]
                - self.lagometer[0] * scale
                - (rect.width - 48.0 * scale).max(0.0)
                - (width - rect.width),
            viewport[1] - self.lagometer[1] * scale - (height - rect.height),
            width,
            height,
        )
    }
}

/// Retained text and color, updated from the existing player-state projection.
pub(crate) struct State {
    /// Options sampled by the normal HUD update.
    pub(super) policy: Policy,
    /// Bounded, preallocated position/angle/velocity label.
    pub(super) position: String,
    /// Final shader color, with stock health-before-style precedence.
    pub(crate) color: [f32; 4],

    /// Projected muzzle trace offset, or zero for the static crosshair.
    pub(crate) dynamic_offset: Option<[f32; 2]>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            policy: Policy::read(None),
            position: String::with_capacity(384),
            color: [1.0, 1.0, 1.0, 1.0],

            dynamic_offset: None,
        }
    }
}

impl State {
    /// Apply the existing trace classification after local-health/style precedence.
    pub(crate) fn classify(&mut self, color: Option<[f32; 4]>, weapon: u8) {
        if self.policy.identify && !self.policy.health && !(self.policy.style && weapon == 3) {
            self.color = color.unwrap_or([1.0; 4]);
        }
    }
    pub(super) fn update(&mut self, console: Option<&ViewerConsole>, player: &PlayerState) {
        self.policy = Policy::read(console);
        self.color = self.policy.color_for(
            player.health(),
            player.armor(),
            player.weapon(),
            player.raw_field(25).unwrap_or(0),
        );
        self.position.clear();
        if self.policy.showpos {
            let origin = player.origin();
            let angles = player.view_angles();
            let velocity = player.velocity();
            // Bound hostile float formatting to fit the retained capacity.
            let f = |value: f32| value.clamp(-9999999.0, 9999999.0);
            let speed = velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
            let _ = write!(
                self.position,
                "POS {:.2} {:.2} {:.2}  ANG {:.2} {:.2}  VEL {:.2}",
                f(origin[0]),
                f(origin[1]),
                f(origin[2]),
                f(angles[0]),
                f(angles[1]),
                f(speed)
            );
        }
    }

    /// The GPU consumes this cached policy without additional cvar lookup or formatting.
    pub(crate) fn parameters(&self, viewport: [f32; 2]) -> [f32; 4] {
        let mut parameters = self.policy.parameters(viewport);
        if let Some(offset) = self.dynamic_offset {
            parameters[0] = offset[0];
            parameters[1] = offset[1];
        }
        parameters
    }

    /// Where the crosshair's middle goes, as a fraction of the viewport from the
    /// top left: the dynamic muzzle point or the `cg_crosshairX/Y` offset.
    pub(crate) fn center(&self, viewport: [f32; 2]) -> [f32; 2] {
        let [x, y, ..] = self.parameters(viewport);
        // Shader UVs increase upward; the draw list's rows run down.
        [0.5 + x, 0.5 - y]
    }

    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], theme: Theme) {
        if !self.policy.showpos {
            return;
        }
        let scale = crate::ui_scale::height_scale(viewport[1]);
        let _ = list.push(DrawCommand::Text {
            rect: Rect::new(
                32.0 * scale,
                28.0 * scale,
                viewport[0] - 64.0 * scale,
                30.0 * scale,
            ),
            text: TextId(317),
            size: 20.0 * scale,
            color: theme.foreground,
            align: TextAlign::Start,
            overflow: TextOverflow::Ellipsis,
            weight: FontWeight::Regular,
            letter_spacing: 0.0,
        });
    }
}

// TaystJK cg_drawtools.c:52-97. Armor protection is 0.5 in MP bg_public.h.
fn health_color(health: i32, armor: i32) -> [f32; 4] {
    if health <= 0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let health = health.saturating_add(armor.min(health)) as f32;
    [
        1.0,
        ((health - 30.0) / 30.0).clamp(0.0, 1.0),
        ((health - 66.0) / 33.0).clamp(0.0, 1.0),
        1.0,
    ]
}

// TaystJK cg_draw.c:7897-7925.
fn style_color(style: u32, alpha: f32) -> [f32; 4] {
    match style {
        1 | 5 => [0.0, 0.0, 1.0, alpha],
        2 | 6 | 7 => [1.0, 1.0, 0.0, alpha],
        3 | 4 => [1.0, 0.0, 0.0, alpha],
        _ => [1.0, 1.0, 1.0, alpha],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagometer_scale_keeps_the_default_anchor_and_history_dimensions() {
        let default =
            Policy::read(None).lagometer_rect(Rect::new(0.0, 0.0, 48.0, 48.0), [640.0, 480.0]);
        assert_eq!(
            [default.x, default.y, default.width, default.height],
            [592.0, 336.0, 48.0, 48.0]
        );
        for viewport in [[640.0, 480.0], [1920.0, 1080.0], [2560.0, 1080.0]] {
            for size in [48.0, 64.0] {
                let rect = Rect::new(0.0, 0.0, size, size);
                let mut policy = Policy::read(None);
                policy.lagometer = [80.0, 160.0];
                let normal = policy.lagometer_rect(rect, viewport);
                assert_eq!(normal.width, size);
                assert_eq!(normal.height, size);
                for scale in [0.25, 0.5, 2.0, 4.0] {
                    policy.lagometer_scale = scale;
                    let scaled = policy.lagometer_rect(rect, viewport);
                    assert_eq!(scaled.width, normal.width * scale);
                    assert_eq!(scaled.height, normal.height * scale);
                    assert_eq!(scaled.right(), normal.right());
                    assert_eq!(scaled.bottom(), normal.bottom());
                }
            }
        }
    }

    #[test]
    fn lagometer_scale_is_live_bounded_and_archived() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert_eq!(Policy::read(Some(&console)).lagometer_scale, 1.0);
        for (value, expected) in [
            ("0", 0.25),
            ("-1", 0.25),
            ("100", 4.0),
            ("0.5", 0.5),
            ("2", 2.0),
        ] {
            assert!(console.set_cvar("cg_lagometerScale", value));
            assert_eq!(Policy::read(Some(&console)).lagometer_scale, expected);
        }
        console.flush_config();
        let console = ViewerConsole::new(path).unwrap();
        assert_eq!(Policy::read(Some(&console)).lagometer_scale, 2.0);
    }

    #[test]
    #[ignore = "requires GPU and retail assets in JKA_GAME_DATA"]
    fn lagometer_scale_gpu() {
        crate::world_shot::on_big_stack(|| {
            let (mut gpu, _profile) = crate::world_shot::open(
                "maps/mp/duel6.bsp",
                [960, 540],
                None,
                &[("cg_lagometer", "1"), ("r_clouds", "0"), ("r_weather", "0")],
            )
            .expect("GPU startup");
            gpu.is_menu_world = false;
            gpu.console.as_mut().unwrap().close_for_connection();
            for sample in 0..64 {
                gpu.hud.lagometer.add_frame(20 + sample);
                gpu.hud.lagometer.add_snapshot(150 + sample * 8, 0);
            }
            let _ = crate::world_shot::frame(&mut gpu, 30);
            // A snapshot-less world suppresses the gameplay UI layer. Render its
            // retained commands through the real UI pipeline into our own target.
            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("lagometer scale comparison"),
                size: wgpu::Extent3d {
                    width: 960,
                    height: 540,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::ui_target::format(gpu.context.format),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = target.create_view(&Default::default());
            let mut sheet = image::RgbaImage::new(1080, 870);
            for (row, style) in ["classic", "game", "radial"].into_iter().enumerate() {
                gpu.console.as_mut().unwrap().set_cvar("cg_hudStyle", style);
                let mut previous: Option<(usize, f32)> = None;
                for (column, scale) in ["0.5", "1", "2"].into_iter().enumerate() {
                    gpu.console
                        .as_mut()
                        .unwrap()
                        .set_cvar("cg_lagometerScale", scale);
                    gpu.hud.targeting.policy = Policy::read(gpu.console.as_ref());
                    let _ = crate::world_shot::frame(&mut gpu, 2);
                    gpu.ui_shapes
                        .prepare_layers(&gpu.queue, [gpu.hud.draw_list()], [960.0, 540.0]);
                    let mut encoder = gpu.device.create_command_encoder(&Default::default());
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("lagometer retained UI"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.08,
                                        g: 0.08,
                                        b: 0.08,
                                        a: 1.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: gpu
                                        .render_scale
                                        .as_ref()
                                        .map_or(&gpu.depth.view, |scale| &scale.hud_depth.view),
                                    depth_ops: None,
                                    stencil_ops: None,
                                },
                            ),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        gpu.ui_shapes.draw(&mut pass);
                    }
                    gpu.queue.submit([encoder.finish()]);
                    let image = crate::world_shot::read_back(&gpu.device, &gpu.queue, &target);
                    let bars: Vec<_> = gpu
                        .hud
                        .draw_list()
                        .commands()
                        .iter()
                        .filter_map(|command| match command {
                            DrawCommand::SolidRect { rect, color }
                                if color.r == 0.28 && color.g == 0.95 =>
                            {
                                Some(*rect)
                            }
                            _ => None,
                        })
                        .collect();
                    assert!(!bars.is_empty(), "lagometer must be drawn in {style}");
                    if let Some((count, width)) = previous {
                        assert_eq!(bars.len(), count, "scaling preserves displayed history");
                        assert!((bars[0].width - width * 2.0).abs() < 0.001);
                    }
                    previous = Some((bars.len(), bars[0].width));
                    let crop = image::imageops::crop_imm(&image, 600, 250, 360, 290).to_image();
                    assert!(
                        crop.pixels()
                            .any(|pixel| pixel[1] > 160 && pixel[0] < 100 && pixel[2] < 150),
                        "GPU must draw green lagometer bars in {style} at scale {scale}"
                    );
                    image::imageops::overlay(
                        &mut sheet,
                        &crop,
                        column as i64 * 360,
                        row as i64 * 290,
                    );
                }
            }
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/parity-reports/lagometer-scale.png");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            sheet.save(&path).unwrap();
            println!(
                "lagometer scales 0.5/1/2, rows classic/game/radial: {}",
                path.display()
            );
        });
    }
}

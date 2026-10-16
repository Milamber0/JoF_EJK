//! Local-player damage direction and timing from authoritative player state.
//!
//! `Feedback::observe` mirrors the damage-event transition in
//! `codemp/cgame/cg_playerstate.c:505-516`. Direction, health scaling, kick
//! clamping, and screen-coordinate clamping mirror `CG_DamageFeedback` at
//! lines 106-195. The 500ms lifetime is `DAMAGE_TIME` from
//! `codemp/cgame/cg_local.h:44`; the strict `0 < age < DAMAGE_TIME` gate and
//! linear decay mirror `CG_DamageBlendBlob` in `codemp/cgame/cg_view.c`
//! lines 1293-1339. Only the visual style is modernized. `view_kick` is the
//! damage view kick of `CG_OffsetFirstPersonView` (`cg_view.c:967-981`).

use sjk_protocol::PlayerState;

/// Codemp view-kick and damage-feedback lifetime (`cg_local.h:44`).
pub(crate) const DAMAGE_TIME_MILLIS: i32 = 500;

/// Shader-ready screen-space feedback at a presentation time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) alpha: f32,
    pub(crate) strength: f32,
}

/// `DAMAGE_DEFLECT_TIME` / `DAMAGE_RETURN_TIME` (`cg_local.h:42-43`).
const DEFLECT_MILLIS: i32 = 100;
const RETURN_MILLIS: i32 = 400;

#[derive(Clone, Copy, Debug)]
struct Event {
    x: f32,
    y: f32,
    kick: f32,
    /// `cg.v_dmg_pitch` / `cg.v_dmg_roll`, degrees.
    kick_pitch: f32,
    kick_roll: f32,
    server_time: i32,
}

/// Tracks `damageEvent` and exposes the active 500ms directional pulse.
#[derive(Default)]
pub(crate) struct Feedback {
    previous_event: Option<u8>,
    active: Option<Event>,
}

impl Feedback {
    /// Sample only the damage marker; disabling it preserves event tracking and view kick.
    pub(crate) fn marker(
        &self,
        at_time: i32,
        console: Option<&crate::console::ViewerConsole>,
    ) -> Option<Sample> {
        console
            .and_then(|console| console.bool_cvar("cg_hitmarker"))
            .unwrap_or(false)
            .then(|| self.sample(at_time))
            .flatten()
    }

    /// Whether damage feedback has established stock's `cg.attackerTime` gate.
    pub(crate) fn has_attacker(&self) -> bool {
        self.active.is_some_and(|event| event.server_time != 0)
    }

    /// Consume one authoritative snapshot. The initial state establishes the
    /// transition baseline, exactly as cgame copies its first playerState.
    pub(crate) fn observe(&mut self, player: &PlayerState, server_time: i32) {
        let damage_event = player.damage_event();
        let changed = self
            .previous_event
            .is_some_and(|previous| previous != damage_event);
        self.previous_event = Some(damage_event);
        if changed && player.damage_count() != 0 {
            self.active = Some(calculate(
                player.damage_yaw(),
                player.damage_pitch(),
                player.damage_count(),
                player.health(),
                player.view_angles(),
                server_time,
            ));
        }
    }

    pub(crate) fn sample(&self, at_time: i32) -> Option<Sample> {
        let event = self.active?;
        let age = at_time.wrapping_sub(event.server_time);
        if !(1..DAMAGE_TIME_MILLIS).contains(&age) {
            return None;
        }
        Some(Sample {
            x: event.x,
            y: event.y,
            alpha: 1.0 - age as f32 / DAMAGE_TIME_MILLIS as f32,
            strength: event.kick / 10.0,
        })
    }

    /// The view kick `[pitch, roll]` in Quake degrees at `at_time`: a linear
    /// deflection over `DAMAGE_DEFLECT_TIME`, then a linear return over
    /// `DAMAGE_RETURN_TIME` (`cg_view.c:967-981`). Presentation times before
    /// the damage snapshot yield nothing rather than a negative deflection.
    pub(crate) fn view_kick(&self, at_time: i32) -> Option<[f32; 2]> {
        let event = self.active?;
        let age = at_time.wrapping_sub(event.server_time);
        let ratio = if age < 0 {
            return None;
        } else if age < DEFLECT_MILLIS {
            age as f32 / DEFLECT_MILLIS as f32
        } else {
            1.0 - (age - DEFLECT_MILLIS) as f32 / RETURN_MILLIS as f32
        };
        (ratio > 0.0).then(|| [ratio * event.kick_pitch, ratio * event.kick_roll])
    }
}

fn calculate(
    yaw_byte: u8,
    pitch_byte: u8,
    damage: u8,
    health: i32,
    view_angles: [f32; 3],
    server_time: i32,
) -> Event {
    let scale = if health < 40 {
        1.0
    } else {
        40.0 / health as f32
    };
    let kick = (f32::from(damage) * scale).clamp(5.0, 10.0);
    let (x, y, kick_pitch, kick_roll) = if yaw_byte == 255 && pitch_byte == 255 {
        (0.0, 0.0, -kick, 0.0)
    } else {
        let pitch = f32::from(pitch_byte) / 255.0 * 360.0;
        let yaw = f32::from(yaw_byte) / 255.0 * 360.0;
        let damage_forward = angle_forward(pitch, yaw);
        let direction = damage_forward.map(|value| -value);
        let [view_forward, view_left, view_up] = view_axis(view_angles);
        let mut front = dot(direction, view_forward);
        let left = dot(direction, view_left);
        let up = dot(direction, view_up);
        let distance = (front * front + left * left).sqrt().max(0.1);
        let kick_roll = kick * left;
        let kick_pitch = -kick * front;
        if front <= 0.1 {
            front = 0.1;
        }
        (
            (-left / front).clamp(-1.0, 1.0),
            (up / distance).clamp(-1.0, 1.0),
            kick_pitch,
            kick_roll,
        )
    };
    Event {
        x,
        y,
        kick,
        kick_pitch,
        kick_roll,
        server_time,
    }
}

fn angle_forward(pitch_degrees: f32, yaw_degrees: f32) -> [f32; 3] {
    let (pitch_sine, pitch_cosine) = pitch_degrees.to_radians().sin_cos();
    let (yaw_sine, yaw_cosine) = yaw_degrees.to_radians().sin_cos();
    [
        pitch_cosine * yaw_cosine,
        pitch_cosine * yaw_sine,
        -pitch_sine,
    ]
}

fn view_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (pitch_sine, pitch_cosine) = angles[0].to_radians().sin_cos();
    let (yaw_sine, yaw_cosine) = angles[1].to_radians().sin_cos();
    let (roll_sine, roll_cosine) = angles[2].to_radians().sin_cos();
    let forward = [
        pitch_cosine * yaw_cosine,
        pitch_cosine * yaw_sine,
        -pitch_sine,
    ];
    let right = [
        -roll_sine * pitch_sine * yaw_cosine + roll_cosine * yaw_sine,
        -roll_sine * pitch_sine * yaw_sine - roll_cosine * yaw_cosine,
        -roll_sine * pitch_cosine,
    ];
    let up = [
        roll_cosine * pitch_sine * yaw_cosine + roll_sine * yaw_sine,
        roll_cosine * pitch_sine * yaw_sine - roll_sine * yaw_cosine,
        roll_cosine * pitch_cosine,
    ];
    [forward, right.map(|value| -value), up]
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.into_iter().zip(right).map(|(a, b)| a * b).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_defaults_off_can_toggle_live_and_persists_without_changing_kick() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = crate::console::ViewerConsole::new(path.clone()).unwrap();
        let feedback = Feedback {
            active: Some(calculate(255, 255, 20, 100, [0.0; 3], 1000)),
            ..Default::default()
        };
        let kick = feedback.view_kick(1050);
        assert!(kick.is_some());
        assert_eq!(console.bool_cvar("cg_hitMarker"), Some(false));
        assert_eq!(feedback.marker(1050, Some(&console)), None);
        assert_eq!(feedback.marker(1050, None), None);
        assert!(console.set_cvar("cg_hitMarker", "1"));
        assert_eq!(feedback.marker(1050, Some(&console)), feedback.sample(1050));
        assert_eq!(feedback.view_kick(1050), kick);
        assert_eq!(feedback.marker(1500, Some(&console)), None);
        drop(console);
        let mut console = crate::console::ViewerConsole::new(path).unwrap();
        assert_eq!(console.bool_cvar("cg_hitMarker"), Some(true));
        assert!(console.set_cvar("cg_hitMarker", "0"));
        assert_eq!(feedback.marker(1050, Some(&console)), None);
        assert_eq!(feedback.view_kick(1050), kick);
    }

    #[test]
    #[ignore = "needs a GPU and external JKA_GAME_DATA"]
    fn damage_marker_gpu_comparison() {
        use bytemuck::Zeroable;
        crate::world_shot::on_big_stack(|| {
            let (mut gpu, _profile) = crate::world_shot::open(
                "maps/mp/duel6.bsp",
                [640, 480],
                None,
                &[("r_hdr", "0"), ("cg_materialMaps", "0")],
            )
            .expect("a GPU adapter");
            assert!(matches!(
                gpu.render(&mut None),
                crate::gpu_context::FrameStatus::Rendered
            ));
            let feedback = Feedback {
                active: Some(calculate(255, 255, 20, 100, [0.0; 3], 1000)),
                ..Default::default()
            };
            let mut shots = Vec::new();
            for enabled in ["0", "1"] {
                let console = gpu.console.as_mut().unwrap();
                assert!(console.set_cvar("cg_hitMarker", enabled));
                let sample = feedback.marker(1050, Some(console));
                let uniform = crate::hud_runtime::HudUniform {
                    hud_visible: 1.0,
                    inverse_width: 1.0 / 640.0,
                    inverse_height: 1.0 / 480.0,
                    damage_alpha: sample.map_or(0.0, |s| s.alpha),
                    damage_strength: sample.map_or(0.0, |s| s.strength),
                    ..crate::hud_runtime::HudUniform::zeroed()
                };
                gpu.queue
                    .write_buffer(&gpu.hud_buffer, 0, bytemuck::bytes_of(&uniform));
                let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("damage marker comparison"),
                    size: wgpu::Extent3d {
                        width: 640,
                        height: 480,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: crate::ui_target::format(gpu.context.format),
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            depth_slice: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &gpu.depth.view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        ..Default::default()
                    });
                    pass.set_pipeline(&gpu.hud_pipeline);
                    pass.set_bind_group(0, &gpu.hud_bind_group, &[]);
                    pass.draw(0..3, 0..1);
                }
                gpu.queue.submit([encoder.finish()]);
                shots.push(crate::world_shot::read_back(
                    &gpu.device,
                    &gpu.queue,
                    &texture,
                ));
            }
            let red = |image: &image::RgbaImage| {
                image
                    .pixels()
                    .filter(|p| p[0] > 64 && p[0] > p[1].saturating_add(32))
                    .count()
            };
            assert_eq!(red(&shots[0]), 0);
            assert!(red(&shots[1]) > 50);
            println!(
                "damage marker red pixels: off={}, on={}",
                red(&shots[0]),
                red(&shots[1])
            );
            if let Some(path) = std::env::var_os("SJK_DAMAGE_MARKER_SHOT") {
                shots[1].save(path).unwrap();
            }
        });
    }
}

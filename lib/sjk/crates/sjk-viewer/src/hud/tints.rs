//! Retained Force/liquid overlays from TaystJK CG_Draw2DScreenTints, not damage feedback.
use super::*;
use sjk_ui::{Color, DrawCommand, Rect};

/// Register the active jaPRO/TaystJK screen-tint switch.
pub(crate) fn register(cvars: &mut sjk_shell::CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(sjk_shell::CvarDefinition::new(
        "cg_drawScreenTints",
        true,
        sjk_shell::CvarFlags::ARCHIVE,
        "Full-screen Force and liquid tints (damage direction remains independent)",
    ))
}

/// Jedi Academy contents bits (`codemp/game/surfaceflags.h:35-51`). They are not Quake
/// 3's: there 8, 16 and 32 are lava, slime and water, here 8 is fog and 16/32 are clip
/// brushes. Reading them as Quake 3's tinted every fog volume as lava.
pub(crate) const CONTENTS_LAVA: u32 = 0x0000_0002;
pub(crate) const CONTENTS_WATER: u32 = 0x0000_0004;
pub(crate) const CONTENTS_SLIME: u32 = 0x0002_0000;

#[derive(Clone, Copy, Default)]
struct Envelope {
    start: Option<i32>,
    fade_start: Option<i32>,
    fade: f32,
}

impl Envelope {
    fn sample(&mut self, active: bool, now: i32) -> f32 {
        if active {
            let start = *self.start.get_or_insert(now);
            self.fade_start = None;
            self.fade = 0.0;
            return ((now - start) as f32 / 9000.0).clamp(0.0, 0.15);
        }
        if self.start.is_none() {
            return 0.0;
        }
        let start = *self.fade_start.get_or_insert_with(|| {
            self.fade = 0.15;
            now
        });
        let result = self.fade.clamp(0.0, 0.15);
        // Stock subtracts age since fade start each frame, not frame delta.
        self.fade -= (now - start) as f32 * 0.000005;
        if result == 0.0 {
            *self = Self::default();
        }
        result
    }
}

/// Small fixed-lifetime envelopes and at most five composited screen rectangles.
#[derive(Default)]
pub(crate) struct State {
    envelopes: [Envelope; 5],
    layers: [Option<Color>; 5],
    last_time: Option<i32>,
}

/// Scalar inputs from the accepted player state and the rendered eye's BSP contents.
#[derive(Clone, Copy, Default)]
pub(crate) struct Input {
    /// Force bitmask (rage 8, protect 9, absorb 10).
    pub(crate) powers: u32,
    /// Absolute recovery deadline.
    pub(crate) recovery: i32,
    /// BG_HasYsalamiri, including a carried CTY flag.
    pub(crate) ysalamiri: bool,
    /// Hide the ysalamiri shell under jaPRO race/style policy.
    pub(crate) hide_ysalamiri: bool,
    /// Spectator team resets Force envelopes, not liquid tinting.
    pub(crate) spectator: bool,
    /// Force tints are first-person, liquids also affect third-person eyes.
    pub(crate) third_person: bool,
    /// Jedi Academy `CONTENTS_LAVA`/`SLIME`/`WATER` at the rendered eye.
    pub(crate) contents: u32,
}

impl State {
    /// Advance stock envelopes once per rendered frame; disabling freezes their state.
    pub(crate) fn sample(&mut self, enabled: bool, now: i32, input: Input) {
        self.layers.fill(None);
        if self.last_time.is_some_and(|last| now < last) || input.spectator {
            self.envelopes.fill(Envelope::default());
        }
        self.last_time = Some(now);
        if !enabled {
            return;
        }
        if !input.spectator {
            let rage_active = input.powers & (1 << 8) != 0;
            let rage_was_active = self.envelopes[0].start.is_some();
            let rage = self.envelopes[0].sample(rage_active, now);
            if rage_active || rage_was_active {
                if !input.third_person && rage > 0.0 {
                    self.layers[0] = Some(if !rage_active && input.recovery > now {
                        Color::new((rage * 4.0).max(0.2), 0.2, 0.2, 0.15)
                    } else {
                        Color::new(0.7, 0.0, 0.0, rage)
                    });
                } else if !rage_active {
                    // Preserve stock's recovery exception in the rage-fade else branch.
                    if input.recovery > now {
                        self.layers[0] = Some(Color::new(0.2, 0.2, 0.2, 0.15));
                    }
                    self.envelopes[0] = Envelope::default();
                }
            } else {
                let active = input.recovery > now;
                let mut alpha = self.envelopes[1].sample(active, now);
                if active {
                    alpha = 0.15;
                }
                if !input.third_person && alpha > 0.0 {
                    self.layers[0] = Some(Color::new(0.2, 0.2, 0.2, alpha));
                } else if !active {
                    self.envelopes[1] = Envelope::default();
                }
            }
            for (slot, active, rgb) in [
                (2, input.powers & (1 << 10) != 0, [0.0, 0.0, 0.7]),
                (3, input.powers & (1 << 9) != 0, [0.0, 0.7, 0.0]),
                (4, input.ysalamiri, [0.7, 0.7, 0.0]),
            ] {
                let alpha = self.envelopes[slot].sample(active, now) / 2.0;
                let hidden = input.third_person || (slot == 4 && input.hide_ysalamiri);
                if !hidden && alpha > 0.0 {
                    self.layers[slot - 1] = Some(Color::new(rgb[0], rgb[1], rgb[2], alpha));
                } else if !active {
                    self.envelopes[slot] = Envelope::default();
                }
            }
        }
        let wave = (now as f32 / 1000.0 * 0.4 * std::f32::consts::TAU).sin();
        self.layers[4] = if input.contents & CONTENTS_LAVA != 0 {
            Some(Color::new(0.7, 0.0, 0.0, 0.5 + 0.15 * wave))
        } else if input.contents & CONTENTS_SLIME != 0 {
            Some(Color::new(0.0, 0.7, 0.0, 0.4 + 0.1 * wave))
        } else if input.contents & CONTENTS_WATER != 0 {
            Some(Color::new(0.0, 0.2, 0.4, 0.1 + 0.025 * wave))
        } else {
            None
        };
    }

    /// Append existing solid-rectangle commands without allocating or formatting.
    pub(crate) fn emit(&self, list: &mut DrawList, viewport: [f32; 2]) {
        for color in self.layers.iter().flatten() {
            let _ = list.push(DrawCommand::SolidRect {
                rect: Rect::new(0.0, 0.0, viewport[0], viewport[1]),
                color: *color,
            });
        }
    }
}

/// Sample policy and contents through the existing HUD runtime, for both live and demo views.
pub(crate) fn update(gpu: &mut crate::GpuState, eye: glam::Vec3, now: i32, intermission: bool) {
    let pair = gpu
        .live_session
        .as_ref()
        .map(|s| (s.latest_snapshot(), s.game_state()))
        .or_else(|| {
            gpu.demo_session
                .as_ref()
                .map(|s| (s.latest_snapshot(), s.game_state()))
        });
    let Some((snapshot, game)) = pair else {
        gpu.hud.tints = State::default();
        return;
    };
    let player = &snapshot.player;
    let console = gpu.console.as_ref();
    let enabled = !intermission
        && console
            .and_then(|c| c.bool_cvar("cg_draw2d"))
            .unwrap_or(true)
        && console
            .and_then(|c| c.bool_cvar("cg_drawscreentints"))
            .unwrap_or(true);
    let cty = game
        .config_string(0)
        .and_then(|s| sjk_client::LegacyClientInfo::new(s).integer("g_gametype"))
        == Some(8);
    let japro = game
        .config_string(0)
        .is_some_and(|s| s.windows(5).any(|w| w.eq_ignore_ascii_case(b"japro")));
    gpu.hud.tints.sample(
        enabled,
        now,
        Input {
            powers: player.force_powers_active(),
            recovery: player.force_rage_recovery_time(),
            ysalamiri: player.powerups[15] != 0
                || (cty && (player.powerups[4] != 0 || player.powerups[5] != 0)),
            spectator: player.team() == 3,
            hide_ysalamiri: japro && player.stats[11] != 0,
            third_person: gpu.third_person,
            contents: if enabled {
                gpu.bsp.point_contents(
                    eye.to_array(),
                    CONTENTS_LAVA | CONTENTS_SLIME | CONTENTS_WATER,
                )
            } else {
                0
            },
        },
    );
}

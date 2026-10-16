//! Application of shell cvars to the native window, renderer, and frame loop.

use super::console::ViewerConsole;
use super::pointer_input::MouseLook;
use super::{DepthTarget, GpuState};
use crate::settings::{
    DisplayMode, MonitorModes, exclusive_supported, exclusive_video_mode, suspended_after_focus,
};
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::window::Fullscreen;

impl GpuState {
    pub(crate) fn sync_runtime_cvars(&mut self) {
        self.sync_post_color();
        self.sync_menu_style();
        if let (Some(console), Some(window)) = (&mut self.console, &self.window) {
            console.apply_window_options(window);
        }
        let Some(console) = &self.console else {
            return; // evidence runs keep the settings they set
        };
        let text_style = crate::text::TextStyle::from_cvars(
            console.float_cvar(crate::text::style::SCALE_CVAR),
            console.float_cvar(crate::text::style::TRACKING_CVAR),
        );
        self.ui_font.set_style(text_style);
        crate::text::style::publish(text_style);
        crate::text::charset::sync(console.text_value(crate::text::charset::CVAR));
        self.mouse_look = MouseLook {
            sensitivity: console.float_cvar("sensitivity").unwrap_or(5.0) as f32,
            yaw_scale: console.float_cvar("m_yaw").unwrap_or(0.022) as f32,
            pitch_scale: console.float_cvar("m_pitch").unwrap_or(0.022) as f32,
            invert: console.bool_cvar("m_invert").unwrap_or(false),
        };
        self.gameplay_input
            .set_always_run(console.bool_cvar("cl_run").unwrap_or(true));
        self.gameplay_input
            .set_idrive(crate::input::idrive::Idrive::read(console));
        self.gameplay_input.motion.sync(console);
        // Retail defaults from `codemp/cgame/cg_xcvar.h`: cg_marks 1,
        // cg_shadows 1, cg_drawGun 1.
        self.effect_aux
            .decals
            .set_marks_enabled(console.bool_cvar("cg_marks").unwrap_or(true));
        self.effect_aux.saber_contacts.enabled =
            console.bool_cvar("cg_saberContact").unwrap_or(true);
        self.player_shadows.set_enabled(
            console.bool_cvar("cg_shadows").unwrap_or(true)
                && !self.world_materials.sun_shadows_active(),
        );
        self.first_person_weapon
            .set_visible(console.bool_cvar("cg_drawGun").unwrap_or(true));
        self.field_of_view = console.float_cvar("cg_fov").unwrap_or(90.0) as f32;
        self.local_prediction.set_error_decay_millis(
            console
                .float_cvar("cg_errorDecay")
                .unwrap_or(f64::from(sjk_client::DEFAULT_ERROR_DECAY_MILLIS)) as f32,
        );
        self.local_prediction
            .set_predict_items(console.bool_cvar("cg_predictItems").unwrap_or(true));
        if let Some(adapter) = &mut self.legacy_world_adapter {
            adapter.set_smooth_clients(console.smooth_clients());
        }
        self.local_prediction
            .set_smooth_clients(console.smooth_clients());
        if let Some(session) = &mut self.demo_session {
            session.set_smooth_clients(console.smooth_clients());
        }
        self.net_timing.set_enabled(console.show_timedelta());
        self.presentation_clock
            .set_time_nudge_millis(console.time_nudge_millis());
        let Some(window) = &self.window else {
            return;
        };
        if let Some(menu) = &mut self.client_menu
            && menu.wants_monitor_modes()
        {
            menu.set_monitor_modes(MonitorModes::query(window), console);
        }
        let vsync = console.bool_cvar("r_vsync").unwrap_or(false);
        self.apply_display();
        let present_mode = preferred_present_mode(&self.present_modes, vsync);
        if present_mode != self.configuration.present_mode {
            self.configuration.present_mode = present_mode;
            // The frame in flight presents before its swapchain is reconfigured.
            self.frame_pacer.split.wait_previous();
            if let Some(surface) = &self.context.surface {
                surface.configure(&self.device, &self.configuration);
            }
            self.depth = DepthTarget::new(
                &self.device,
                self.configuration.width,
                self.configuration.height,
            );
        }
    }

    /// Apply the requested display mode and `r_resolution` to the window.
    ///
    /// While exclusive fullscreen is suspended (the window lost focus) the
    /// window is windowed and `r_resolution` is left alone, so the exclusive
    /// size is entered again unchanged when focus returns.
    fn apply_display(&mut self) {
        let (Some(console), Some(window)) = (&self.console, &self.window) else {
            return;
        };
        let requested = DisplayMode::requested(console);
        let display = requested.while_suspended(self.display_suspended);
        let suspended = display != requested;
        let resolution = console
            .text_value("r_resolution")
            .and_then(parse_resolution);
        let exclusive = display == DisplayMode::Exclusive;
        // An exclusive video mode is the resolution, so a new size re-enters it.
        let resized = exclusive && resolution.is_some_and(|size| size != self.applied_resolution);
        if Some(display) != self.applied_display || resized {
            window.set_fullscreen(fullscreen_for(window, display, resolution));
            self.display_changed_at = Instant::now();
            // Leaving fullscreen restores the old window size; ask for
            // r_resolution again in case it changed meanwhile.
            if display == DisplayMode::Windowed
                && !suspended
                && self
                    .applied_display
                    .is_some_and(|applied| applied != DisplayMode::Windowed)
            {
                self.applied_resolution = [0; 2];
            }
            self.applied_display = Some(display);
            if let Some(size) = resolution.filter(|_| exclusive) {
                self.applied_resolution = size;
            }
        }
        if let Some([width, height]) = resolution
            && !suspended
            && [width, height] != self.applied_resolution
        {
            let _ = window.request_inner_size(PhysicalSize::new(width, height));
            self.applied_resolution = [width, height];
        }
    }

    /// The window gained or lost focus: pause exclusive fullscreen while it is
    /// away and enter it again on return.
    ///
    /// An exclusive window is topmost and holds the monitor's video mode, and
    /// winit does nothing about that on focus loss, so Alt+Tab and Win+D left
    /// the game over the desktop. Applied here, not on the next frame: a
    /// minimised window draws none.
    pub(crate) fn display_focus_changed(&mut self, focused: bool) {
        let exclusive_active = self
            .window
            .as_ref()
            .is_some_and(|window| matches!(window.fullscreen(), Some(Fullscreen::Exclusive(_))));
        let suspended = suspended_after_focus(
            self.display_suspended,
            exclusive_active,
            self.display_changed_at.elapsed(),
            focused,
        );
        if suspended != self.display_suspended {
            self.display_suspended = suspended;
            self.apply_display();
        }
    }

    /// The FPS cap `com_maxfps` asks for (AUTO resolved, 0 uncapped), without the
    /// unfocused and minimized caps: what the gameplay FPS's Auto follows.
    pub(crate) fn frame_cap(&self) -> u32 {
        normal_cap(self.console.as_ref(), || self.refresh_rate_cap())
    }

    pub(crate) fn maximum_fps(&self) -> u32 {
        let console = self.console.as_ref();
        let normal = normal_cap(console, || self.refresh_rate_cap());
        console.map_or(normal, |console| console.window_fps(normal))
    }

    /// `com_maxfps -1` with [`MONITOR_CAP_CVAR`] on: the refresh rate of the
    /// window's monitor, rounded to whole hertz (59.94 Hz caps at 60), or
    /// stock's 125 when it cannot be read.
    fn refresh_rate_cap(&self) -> u32 {
        let now = Instant::now();
        if let Some((read, cap)) = self.refresh_cap.get()
            && now.duration_since(read) < REFRESH_RECHECK
        {
            return cap;
        }
        let cap = self
            .window
            .as_ref()
            .and_then(|window| window.current_monitor())
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .map_or(UNKNOWN_REFRESH_CAP, refresh_cap_from_millihertz);
        self.refresh_cap.set(Some((now, cap)));
        cap
    }

    pub(crate) fn finish_frame(&mut self) {
        let maximum_fps = self.maximum_fps();
        self.frame_pacer.frame_rendered(maximum_fps);
    }
}

/// Whether `com_maxfps -1` (AUTO) follows the monitor's refresh rate. Off (0,
/// the default), AUTO caps at stock's 125 and the monitor's rate is never read.
/// Registered and cached for frame reads with the other frame caps
/// (`console_window_options.rs`).
pub(crate) const MONITOR_CAP_CVAR: &str = "com_maxfpsMonitor";
/// Stock's `com_maxfps` default: AUTO's cap with [`MONITOR_CAP_CVAR`] off, or
/// when the monitor reports no refresh rate.
const UNKNOWN_REFRESH_CAP: u32 = 125;
/// AUTO's cap with the monitor's rate off: what the console assumes before the
/// first frame.
pub(crate) const STOCK_FPS_CAP: u32 = UNKNOWN_REFRESH_CAP;
/// How long a monitor refresh-rate reading is reused; moving the window to
/// another monitor is picked up within this.
const REFRESH_RECHECK: Duration = Duration::from_secs(1);

/// The cap `com_maxfps` asks for, before the unfocused and minimized caps: its
/// value from 0 up (0 is uncapped), else AUTO. AUTO is `monitor_cap` with
/// [`MONITOR_CAP_CVAR`] on, and with it off (or no console) stock's 125, the
/// cap of a monitor whose rate is unknown; `monitor_cap` is called only with it
/// on.
fn normal_cap(console: Option<&ViewerConsole>, monitor_cap: impl FnOnce() -> u32) -> u32 {
    match console.and_then(|console| console.integer_cvar("com_maxfps")) {
        Some(value) if value >= 0 => u32::try_from(value).unwrap_or(u32::MAX),
        _ if console.is_some_and(ViewerConsole::auto_fps_follows_monitor) => monitor_cap(),
        _ => UNKNOWN_REFRESH_CAP,
    }
}

fn refresh_cap_from_millihertz(millihertz: u32) -> u32 {
    match millihertz.saturating_add(500) / 1000 {
        0 => UNKNOWN_REFRESH_CAP,
        hertz => hertz,
    }
}

/// The player's `cg_hudScale`; 1.0 when the console is absent (evidence runs).
pub(crate) fn hud_scale(console: Option<&super::console::ViewerConsole>) -> f32 {
    console
        .and_then(|console| console.float_cvar("cg_hudScale"))
        .map_or(1.0, |value| value as f32)
}

/// The winit fullscreen state for `display`: exclusive takes the monitor's
/// video mode of `resolution`, and falls back to borderless (with a log line)
/// where there is none or the windowing system has no exclusive mode.
fn fullscreen_for(
    window: &winit::window::Window,
    display: DisplayMode,
    resolution: Option<[u32; 2]>,
) -> Option<Fullscreen> {
    match display {
        DisplayMode::Windowed => None,
        DisplayMode::Borderless => Some(Fullscreen::Borderless(None)),
        DisplayMode::Exclusive => {
            let mode = resolution
                .filter(|_| exclusive_supported(window))
                .and_then(|size| exclusive_video_mode(window, size));
            if mode.is_none() {
                eprintln!(
                    "exclusive fullscreen unavailable at r_resolution {resolution:?};                     using borderless fullscreen"
                );
            }
            Some(mode.map_or(Fullscreen::Borderless(None), Fullscreen::Exclusive))
        }
    }
}

fn parse_resolution(value: &str) -> Option<[u32; 2]> {
    let (width, height) = value.split_once('x')?;
    let width = width.parse().ok()?;
    let height = height.parse().ok()?;
    (width > 0 && height > 0).then_some([width, height])
}

pub(crate) fn preferred_present_mode(
    modes: &[wgpu::PresentMode],
    vsync: bool,
) -> wgpu::PresentMode {
    if modes.is_empty() {
        return wgpu::PresentMode::Fifo;
    }
    let preferences: &[wgpu::PresentMode] = if vsync {
        &[wgpu::PresentMode::Fifo]
    } else {
        &[
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::AutoNoVsync,
        ]
    };
    preferences
        .iter()
        .copied()
        .find(|mode| modes.contains(mode))
        .unwrap_or(modes[0])
}

#[cfg(test)]
mod refresh_cap_tests {
    use super::{MONITOR_CAP_CVAR, normal_cap, refresh_cap_from_millihertz};
    use crate::console::ViewerConsole;
    use sjk_shell::CvarValue;

    /// A monitor that must not be asked.
    fn unread() -> u32 {
        panic!("the monitor's refresh rate was read")
    }

    /// A monitor reporting `millihertz`.
    fn monitor(millihertz: u32) -> impl FnOnce() -> u32 {
        move || refresh_cap_from_millihertz(millihertz)
    }

    #[test]
    fn rounds_reported_rates_to_whole_hertz() {
        assert_eq!(refresh_cap_from_millihertz(59_940), 60);
        assert_eq!(refresh_cap_from_millihertz(143_856), 144);
        assert_eq!(refresh_cap_from_millihertz(240_000), 240);
    }

    #[test]
    fn a_zero_rate_falls_back_to_stock() {
        assert_eq!(refresh_cap_from_millihertz(0), 125);
    }

    /// A new profile has the monitor's rate off, so AUTO (the `com_maxfps`
    /// default) caps at 125 without asking the monitor, as for a monitor that
    /// reports no rate; so does a run without a console.
    #[test]
    fn off_by_default_auto_caps_at_stock_without_reading_the_monitor() {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        assert_eq!(
            console.cvar_default(MONITOR_CAP_CVAR),
            Some(&CvarValue::Integer(0))
        );
        assert!(!console.auto_fps_follows_monitor());
        assert_eq!(console.integer_cvar("com_maxfps"), Some(-1));
        assert_eq!(normal_cap(Some(&console), unread), 125);
        assert_eq!(normal_cap(None, unread), 125);
    }

    /// On, AUTO follows the monitor as before: its rate in whole hertz, or 125
    /// when it reports none. The setting is archived and leaves `com_maxfps`
    /// as it was.
    #[test]
    fn on_auto_follows_the_monitor_and_stays_on() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        assert!(console.set_cvar(MONITOR_CAP_CVAR, "1"));
        assert!(console.auto_fps_follows_monitor());
        assert_eq!(normal_cap(Some(&console), monitor(143_856)), 144);
        assert_eq!(normal_cap(Some(&console), monitor(59_940)), 60);
        assert_eq!(normal_cap(Some(&console), monitor(240_000)), 240);
        assert_eq!(normal_cap(Some(&console), monitor(0)), 125);
        drop(console);
        let console = ViewerConsole::new(path).unwrap();
        assert!(console.auto_fps_follows_monitor());
        assert_eq!(console.integer_cvar("com_maxfps"), Some(-1));
        assert_eq!(normal_cap(Some(&console), monitor(165_000)), 165);
    }

    /// A cap the player set, uncapped included, is kept with the setting off
    /// or on, and a profile saved before the setting existed keeps its cap and
    /// gets the setting off.
    #[test]
    fn a_cap_the_player_set_is_kept_either_way() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.cfg");
        let mut console = ViewerConsole::new(path.clone()).unwrap();
        for setting in ["0", "1"] {
            assert!(console.set_cvar(MONITOR_CAP_CVAR, setting));
            for cap in [0, 60, 240, 1000] {
                assert!(console.set_cvar("com_maxfps", &cap.to_string()));
                assert_eq!(normal_cap(Some(&console), unread), cap);
            }
        }
        drop(console);
        std::fs::write(
            &path,
            "seta com_maxfpsDefaultVersion 1\nseta com_maxfps 144\n",
        )
        .unwrap();
        let console = ViewerConsole::new(path).unwrap();
        assert!(!console.auto_fps_follows_monitor());
        assert_eq!(normal_cap(Some(&console), unread), 144);
    }
}

//! Explicit restarts reuse the live settings, audio and pointer paths.
use super::super::*;
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};

const MODES: [[u32; 2]; 13] = [
    [320, 240],
    [400, 300],
    [512, 384],
    [640, 480],
    [800, 600],
    [960, 720],
    [1024, 768],
    [1152, 864],
    [1280, 1024],
    [1600, 1200],
    [2048, 1536],
    [856, 480],
    [2400, 600],
];

pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value, help) in [
        (
            "r_mode",
            4_i64,
            "Stock video mode; -1 selects custom dimensions",
        ),
        ("r_customwidth", 1600, "Custom window width"),
        ("r_customheight", 1024, "Custom window height"),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

pub(super) fn mode_size(mode: i64, width: i64, height: i64) -> Result<[u32; 2], String> {
    if mode == -1 {
        let width = u32::try_from(width).ok().filter(|v| *v > 0);
        let height = u32::try_from(height).ok().filter(|v| *v > 0);
        return width
            .zip(height)
            .map(|(w, h)| [w, h])
            .ok_or("Invalid custom resolution".into());
    }
    usize::try_from(mode)
        .ok()
        .and_then(|mode| MODES.get(mode))
        .copied()
        .ok_or("Invalid r_mode; use modelist".into())
}

impl ViewerConsole {
    pub(crate) fn sync_custom_resolution(&mut self, value: &str) {
        if let Some((w, h)) = value.split_once('x') {
            let _ = self.shell.cvars.set_text("r_customwidth", w);
            let _ = self.shell.cvars.set_text("r_customheight", h);
            let _ = self.shell.cvars.set_text("r_mode", "-1");
        }
    }
}

impl crate::GpuState {
    pub(super) fn restart_command(
        &mut self,
        name: &str,
        audio: &mut Option<crate::GameAudio>,
    ) -> Result<Vec<String>, String> {
        match name {
            "modelist" => {
                let mut lines: Vec<_> = MODES
                    .iter()
                    .enumerate()
                    .map(|(mode, [w, h])| format!("Mode {mode:2}: {w}x{h}"))
                    .collect();
                lines.push("Mode -1: r_customwidth x r_customheight".into());
                for resolution in crate::settings::RESOLUTIONS {
                    lines.push(format!("Windowed/borderless: {resolution}"));
                }
                if let Some(monitor) = self.window.as_ref().and_then(|w| w.current_monitor()) {
                    for mode in monitor.video_modes() {
                        lines.push(format!(
                            "Monitor: {}x{} @ {:.3} Hz",
                            mode.size().width,
                            mode.size().height,
                            mode.refresh_rate_millihertz() as f32 / 1000.0
                        ));
                    }
                }
                Ok(lines)
            }
            "vid_restart" => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                let size = mode_size(
                    console.integer_cvar("r_mode").unwrap_or(4),
                    console.integer_cvar("r_customwidth").unwrap_or(1600),
                    console.integer_cvar("r_customheight").unwrap_or(1024),
                )?;
                console
                    .shell
                    .cvars
                    .set_text("r_resolution", &format!("{}x{}", size[0], size[1]))
                    .map_err(|e| e.to_string())?;
                console.persist();
                self.applied_resolution = [0; 2];
                self.applied_display = None;
                self.hitches.settle(std::time::Instant::now());
                self.sync_runtime_cvars();
                let mut lines =
                    vec!["Video settings reapplied; GPU device/resources retained.".into()];
                // The settings read only when the graphics start: build the world again
                // on them (`graphics_reload.rs`), as the original game's vid_restart
                // restarted its renderer.
                if !self.graphics_changes().is_empty() {
                    lines.push(self.reload_graphics(audio)?);
                }
                Ok(lines)
            }
            "snd_restart" => {
                drop(audio.take());
                if self
                    .console
                    .as_ref()
                    .and_then(|c| c.bool_cvar("s_initsound"))
                    == Some(false)
                {
                    return Ok(vec!["Audio disabled by s_initsound".into()]);
                }
                *audio = crate::GameAudio::start();
                let Some(restarted) = audio else {
                    return Ok(vec!["Audio stopped; no output device available.".into()]);
                };
                restarted.sync_gains(self.console.as_ref());
                if let (Some(session), Some(vfs)) = (&self.live_session, &self.vfs) {
                    restarted.install_gamestate(
                        session.game_state(),
                        std::sync::Arc::clone(vfs),
                        &self.bsp,
                    );
                } else if let (Some(session), Some(vfs)) = (&self.demo_session, &self.vfs) {
                    restarted.install_gamestate(
                        session.game_state(),
                        std::sync::Arc::clone(vfs),
                        &self.bsp,
                    );
                }
                Ok(vec!["Audio output recreated; sample cache cleared.".into()])
            }
            "in_restart" => {
                self.gameplay_input.clear();
                self.release_pointer();
                self.sync_cursor_policy();
                Ok(vec!["Input reset.".into()])
            }
            _ => Err("Unknown restart command".into()),
        }
    }
}

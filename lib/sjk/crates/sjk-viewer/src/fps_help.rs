//! The low-FPS help (`docs/client.md`, "Low FPS help"): SJK's real-time lighting is
//! most of what a heavy map costs, and Vanilla graphics turn it off. A few seconds after
//! a server's map is in, the GPU times of some frames ([`crate::gpu_phases`]) say
//! whether the frame misses the player's FPS target with the lighting a large part of
//! it; then a card over the game menu ([`card`]) offers Vanilla graphics, once per map,
//! until the player ticks "Don't suggest this again" (`cg_fpsHelp 0`). `help_fps`
//! opens the same card at any time, with this map's numbers when it has them.

use crate::gpu_phases::Sample;
use std::time::{Duration, Instant};

#[path = "fps_help_card.rs"]
pub(crate) mod card;

/// Console command that opens the card.
pub(crate) const COMMAND: &str = "help_fps";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str =
    "Low FPS? Shows what SJK's lighting costs here and offers Vanilla graphics";
/// Whether the card is offered when a map's lighting is heavy.
pub(crate) const CVAR: &str = "cg_fpsHelp";

/// Time after the map is in before frames are timed: loading's last work (probes,
/// light caches, the first snapshots) is over by then.
const SETTLE: Duration = Duration::from_secs(6);
/// Sampled frames the check takes (one every 32 frames).
const SAMPLES: usize = 8;
/// The check gives up past this, so the card never comes long after the map loaded.
const WINDOW: Duration = Duration::from_secs(30);
/// The share of the frame the lighting must take to be worth the offer.
const LIGHT_SHARE: f32 = 0.4;
/// The FPS target's bounds: below 60 is no target, and past 144 the frame is fast
/// enough that a heavy map is not what the player notices.
const TARGET_FPS: (u32, u32) = (60, 144);

/// What the check found on this world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Finding {
    /// The median sampled frame.
    pub(crate) sample: Sample,
    /// The FPS the frame should reach.
    pub(crate) target_fps: u32,
}

impl Finding {
    /// The frame misses the target and the lighting is a large part of it.
    pub(crate) fn heavy(self) -> bool {
        let budget = 1_000.0 / self.target_fps as f32;
        self.sample.frame_ms > budget && self.sample.light_ms >= LIGHT_SHARE * self.sample.frame_ms
    }
}

/// The FPS the frame should reach for `maximum_fps` (0 is no cap).
pub(crate) fn target_fps(maximum_fps: u32) -> u32 {
    let (low, high) = TARGET_FPS;
    if maximum_fps == 0 {
        high
    } else {
        maximum_fps.clamp(low, high)
    }
}

/// The middle of `samples` by frame time.
fn median(samples: &mut [Sample]) -> Option<Sample> {
    samples.sort_by(|a, b| a.frame_ms.total_cmp(&b.frame_ms));
    samples.get(samples.len() / 2).copied()
}

/// One world's check and the card, which goes with the shell from world to world.
#[derive(Default)]
pub(crate) struct State {
    /// When this world's map went live.
    live_since: Option<Instant>,
    samples: Vec<Sample>,
    /// The check is over on this world.
    checked: bool,
    /// What it found, for the card.
    pub(crate) finding: Option<Finding>,
    /// The card opened the game menu, which goes with it.
    pub(crate) opened_menu: bool,
}

impl crate::GpuState {
    /// Once a frame: run the check while it is due, and keep the card's numbers fresh
    /// while it shows.
    pub(crate) fn update_fps_help(&mut self) {
        let Some(phases) = &self.gpu_phases else {
            return;
        };
        let on_map = self.live_map_installed && self.live_session.is_some() && !self.is_menu_world;
        let console = self.console.as_ref();
        let wanted = console
            .and_then(|console| console.bool_cvar(CVAR))
            .unwrap_or(true);
        let vanilla = console.is_some_and(crate::graphics_quality::vanilla);
        let card_open = self.fps_card.wants();
        let checking = on_map && wanted && !vanilla && !self.fps_help.checked;
        phases.watch(checking || card_open);
        if card_open {
            if let Some(sample) = phases.take_sample() {
                self.fps_help.finding = Some(Finding {
                    sample,
                    target_fps: target_fps(self.maximum_fps()),
                });
            }
            return;
        }
        if !checking {
            self.fps_help.live_since = None;
            return;
        }
        let now = Instant::now();
        let since = *self.fps_help.live_since.get_or_insert(now);
        if now.duration_since(since) < SETTLE {
            phases.take_sample();
            return;
        }
        if now.duration_since(since) > WINDOW {
            self.fps_help.checked = true;
            return;
        }
        if let Some(sample) = phases.take_sample() {
            self.fps_help.samples.push(sample);
        }
        if self.fps_help.samples.len() < SAMPLES {
            return;
        }
        self.fps_help.checked = true;
        let Some(sample) = median(&mut self.fps_help.samples) else {
            return;
        };
        let finding = Finding {
            sample,
            target_fps: target_fps(self.maximum_fps()),
        };
        self.fps_help.finding = Some(finding);
        let heavy = finding.heavy();
        crate::log::progress(format_args!(
            "fps help: frame {:.2} ms, SJK lighting {:.2} ms, target {} FPS: {}",
            sample.frame_ms,
            sample.light_ms,
            finding.target_fps,
            if heavy {
                "Vanilla graphics offered"
            } else {
                "fine"
            }
        ));
        if heavy {
            self.open_fps_card(card::Mode::Offered);
        }
    }

    /// Show the card, over the game menu when on a map.
    pub(crate) fn open_fps_card(&mut self, mode: card::Mode) {
        if let Some(console) = &mut self.console {
            console.set_open(false);
        }
        let menu_visible = self
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_visible);
        if !menu_visible && self.live_session.is_some() && !self.game_menu {
            self.release_pointer();
            self.game_menu = true;
            self.game_menu_page = crate::ingame_menu::Page::Main;
            self.game_menu_row = 0;
            self.fps_help.opened_menu = true;
        }
        self.fps_card.show(mode);
        self.sync_cursor_policy();
    }

    /// `help_fps`.
    pub(crate) fn fps_help_command(&mut self) -> Result<Vec<String>, String> {
        self.open_fps_card(card::Mode::Asked);
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(frame_ms: f32, light_ms: f32) -> Sample {
        Sample { frame_ms, light_ms }
    }

    #[test]
    fn the_target_follows_the_cap_within_bounds() {
        assert_eq!(target_fps(0), 144);
        assert_eq!(target_fps(1000), 144);
        assert_eq!(target_fps(125), 125);
        assert_eq!(target_fps(30), 60);
    }

    #[test]
    fn heavy_needs_a_missed_target_and_the_lighting_most_of_it() {
        // JoFTemple at 4K before the floor fix: 8.1 ms, 5.6 of them lighting.
        let temple = Finding {
            sample: sample(8.07, 5.64),
            target_fps: 144,
        };
        assert!(temple.heavy());
        // The same frame for a 60 FPS target is fast enough.
        assert!(
            !Finding {
                target_fps: 60,
                ..temple
            }
            .heavy()
        );
        // A slow frame that is not the lighting's.
        assert!(
            !Finding {
                sample: sample(12.0, 2.0),
                target_fps: 144,
            }
            .heavy()
        );
        // After the floor fix: 3.6 ms in all.
        assert!(
            !Finding {
                sample: sample(3.6, 1.8),
                target_fps: 144,
            }
            .heavy()
        );
    }

    #[test]
    fn the_median_ignores_a_stray_frame() {
        let mut samples = vec![
            sample(8.0, 5.0),
            sample(40.0, 5.0),
            sample(7.9, 5.1),
            sample(8.1, 5.2),
            sample(2.0, 1.0),
        ];
        assert_eq!(median(&mut samples), Some(sample(8.0, 5.0)));
        assert_eq!(median(&mut []), None);
    }
}

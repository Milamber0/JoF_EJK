//! Gameplay FPS: the frame rate whose movement the player wants, apart from the
//! frames the screen draws (`gameplay_fps`, `sjk_client::command_rate`).
//!
//! In the original game the frame rate is also the movement: each rendered frame
//! makes one user command, and `com_maxfps` 125, 142, 250 or 333 moves the player
//! in steps of 8, 7, 4 or 3 ms, whose rounding changes jumps, strafes and
//! acceleration. Players lock those numbers for that feel. SJK makes commands on
//! their own clock, as JoF EJK's `cl_cmdratecap`. On Auto (`gameplay_fps 0`, the
//! default) the clock runs at the FPS cap, so the movement is what the other
//! clients give at that cap; a `pmove_fixed` server uses its own step, and JoF's
//! servers take 125 a second at most; a number keeps that movement whatever the FPS
//! cap. Inputs a server does not take open the card by themselves
//! (`input_health.rs`).
//!
//! The choice is a step of the Import card (`config_import_panel.rs`), after a
//! config from another client is read; the `help_gameplay_fps` command and the
//! Video settings' Gameplay FPS row open the same card on its own. Auto, four
//! presets for what players lock, and "Other" for any value.

use sjk_client::command_rate::{
    AUTO_GAMEPLAY_FPS, DEFAULT_GAMEPLAY_FPS, GAMEPLAY_FPS_PRESETS, MAX_GAMEPLAY_FPS,
    MIN_GAMEPLAY_FPS, commands_per_second, gameplay_step,
};

/// The setting: 0 is Auto.
pub(crate) const CVAR: &str = "gameplay_fps";
/// Explains it and opens the card, or with a number sets it.
pub(crate) const COMMAND: &str = "help_gameplay_fps";
pub(crate) const HELP: &str = "Explain and choose your gameplay FPS (Auto, or the movement of 125, 142, 250, 333...); help_gameplay_fps <fps> sets it";

/// What the card and the command say about it, one paragraph.
pub(crate) const EXPLANATION: &str = "In the original game your FPS was also your physics: at \
    125, 142, 250 or 333 FPS jumps, strafes and acceleration felt different. On Auto SJK moves \
    you at your FPS cap, as the other clients do. Pick a number to keep that movement whatever \
    FPS you draw.";

/// The note on servers that fix the movement step.
pub(crate) const FIXED_NOTE: &str =
    "Servers that fix the step (pmove_fixed) or cap it (JoF's, at 125) use their own step.";

/// The choices in the card's order: Auto, then the presets; "Other" comes after.
const CHOICES: [i64; 5] = [
    AUTO_GAMEPLAY_FPS,
    GAMEPLAY_FPS_PRESETS[0],
    GAMEPLAY_FPS_PRESETS[1],
    GAMEPLAY_FPS_PRESETS[2],
    GAMEPLAY_FPS_PRESETS[3],
];
/// The "Other" choice after Auto and the presets.
pub(crate) const OTHER: usize = CHOICES.len();
/// Most digits the advanced value takes.
const DRAFT_DIGITS: usize = 4;

/// What another client's config said about its frame rate.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Origin {
    /// The config's file name.
    pub(crate) file: String,
    /// Its `com_maxfps`, when set and above 0.
    pub(crate) max_fps: Option<i64>,
    /// Its `cl_cmdratecap` was on (JoF EJK): its movement ran at 125.
    pub(crate) cmd_rate_cap: bool,
}

impl Origin {
    /// The gameplay FPS to pick for the old client's movement: 125 under JoF EJK's
    /// `cl_cmdratecap`, which moved at 125 whatever it drew; otherwise Auto, which
    /// follows the FPS cap the import brings.
    pub(crate) fn old_fps(&self) -> i64 {
        if self.cmd_rate_cap {
            DEFAULT_GAMEPLAY_FPS
        } else {
            AUTO_GAMEPLAY_FPS
        }
    }

    /// The line under the explanation that says what was picked and why.
    pub(crate) fn line(&self) -> Option<String> {
        if self.cmd_rate_cap {
            return Some(format!(
                "{} had cl_cmdratecap 1 (JoF EJK): your movement ran at 125, so 125 is picked.",
                self.file
            ));
        }
        let fps = self.max_fps?;
        Some(format!(
            "{} locked com_maxfps at {fps}: Auto moves you at it, as you are used to.",
            self.file
        ))
    }
}

/// The card's choice: Auto, a preset or "Other" with its typed value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Choice {
    /// A choice's index (0 Auto, then the presets), or [`OTHER`].
    pub(crate) selected: usize,
    /// "Other"'s digits.
    pub(crate) draft: String,
    /// The setting when the card opened.
    pub(crate) current: i64,
    pub(crate) origin: Option<Origin>,
}

impl Choice {
    /// Open on what suits the old config when there is one, else the setting.
    pub(crate) fn new(current: i64, origin: Option<Origin>) -> Self {
        let start = origin.as_ref().map_or(current, Origin::old_fps);
        let mut choice = Self {
            selected: OTHER,
            draft: String::new(),
            current,
            origin,
        };
        choice.pick(start);
        choice
    }

    /// Select `fps` for the player (the card opened to suggest it).
    pub(crate) fn suggest(&mut self, fps: i64) {
        self.pick(fps);
    }

    /// Select `fps`: Auto or its preset, or "Other" holding it.
    fn pick(&mut self, fps: i64) {
        match CHOICES.iter().position(|choice| *choice == fps) {
            Some(index) => self.selected = index,
            None => {
                self.selected = OTHER;
                self.draft = fps.to_string();
            }
        }
    }

    /// The chosen value when it is Auto or in range.
    pub(crate) fn value(&self) -> Option<i64> {
        let fps = match CHOICES.get(self.selected) {
            Some(choice) => *choice,
            None => self.draft.parse().ok()?,
        };
        valid(fps).then_some(fps)
    }

    /// What a choice's tile shows.
    pub(crate) fn label(index: usize) -> String {
        match CHOICES.get(index) {
            Some(&AUTO_GAMEPLAY_FPS) => "Auto".to_owned(),
            Some(fps) => fps.to_string(),
            None => "Other".to_owned(),
        }
    }

    /// Move the selection by `delta` choices, wrapping.
    pub(crate) fn step(&mut self, delta: isize) {
        let count = OTHER as isize + 1;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    /// Select choice `index` (a click).
    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(OTHER);
    }

    /// A digit typed: it goes to "Other", which it selects; the first digit after
    /// selecting by another way starts a new value.
    pub(crate) fn type_digit(&mut self, digit: char) {
        if self.selected != OTHER {
            self.selected = OTHER;
            self.draft.clear();
        }
        if self.draft.len() < DRAFT_DIGITS && digit.is_ascii_digit() {
            self.draft.push(digit);
        }
    }

    pub(crate) fn backspace(&mut self) {
        if self.selected == OTHER {
            self.draft.pop();
        }
    }

    /// What a choice's tile says under it.
    pub(crate) fn caption(&self, index: usize) -> &'static str {
        let fps = CHOICES.get(index).copied();
        let imported = self.origin.as_ref().map(Origin::old_fps);
        match fps {
            Some(DEFAULT_GAMEPLAY_FPS) if imported == Some(DEFAULT_GAMEPLAY_FPS) => "your old one",
            Some(AUTO_GAMEPLAY_FPS) => "your FPS cap",
            Some(fps) if fps == self.current && imported.is_none() => "in use",
            Some(DEFAULT_GAMEPLAY_FPS) => "JoF EJK",
            Some(_) => "",
            None => "any value",
        }
    }

    /// What "Other"'s field says about its value.
    pub(crate) fn draft_note(&self) -> String {
        match self.draft.parse::<i64>() {
            Ok(fps) if (MIN_GAMEPLAY_FPS..=MAX_GAMEPLAY_FPS).contains(&fps) => {
                let made = commands_per_second(fps);
                let step = gameplay_step(fps);
                if made == fps {
                    format!("{step} ms steps")
                } else {
                    format!("moves in {step} ms steps, {made} a second")
                }
            }
            _ => format!("from {MIN_GAMEPLAY_FPS} to {MAX_GAMEPLAY_FPS}"),
        }
    }
}

/// Auto, or a gameplay FPS in range.
fn valid(fps: i64) -> bool {
    fps == AUTO_GAMEPLAY_FPS || (MIN_GAMEPLAY_FPS..=MAX_GAMEPLAY_FPS).contains(&fps)
}

/// Left/Right on the setting: the next choice (Auto, then the presets) above
/// (`direction` > 0) or below `current`, wrapping; a typed value steps to its
/// nearest preset that way.
pub(crate) fn step_preset(current: i64, direction: i32) -> i64 {
    if direction > 0 {
        CHOICES
            .into_iter()
            .find(|choice| *choice > current)
            .unwrap_or(CHOICES[0])
    } else {
        CHOICES
            .into_iter()
            .rev()
            .find(|choice| *choice < current)
            .unwrap_or(CHOICES[CHOICES.len() - 1])
    }
}

/// The setting's row value: Auto with the FPS cap it follows, or the number; and
/// the server's step while it fixes one.
pub(crate) fn row_text(console: &crate::console::ViewerConsole) -> String {
    let shown = match console.gameplay_fps() {
        AUTO_GAMEPLAY_FPS if console.frame_cap() == 0 => "Auto (uncapped)".to_owned(),
        AUTO_GAMEPLAY_FPS => format!("Auto ({})", console.frame_cap()),
        fps => fps.to_string(),
    };
    match (console.pmove_fixed_msec(), console.server_cap()) {
        (Some(step), _) => format!("{shown}, this server {step} ms"),
        (None, Some(cap)) if console.server_caps_lifted() => {
            format!("{shown}, {} cap lifted", cap.name)
        }
        (None, Some(cap)) => format!("{shown}, {} server {} ms", cap.name, cap.step),
        (None, None) => shown,
    }
}

/// What `fps` moves like, for the console and the card's result line.
pub(crate) fn describe(fps: i64) -> String {
    if fps == AUTO_GAMEPLAY_FPS {
        return "Auto (your FPS cap)".to_owned();
    }
    let made = commands_per_second(fps);
    let step = gameplay_step(fps);
    if made == fps {
        format!("{fps} ({step} ms steps)")
    } else {
        format!("{fps} ({step} ms steps, {made} a second)")
    }
}

/// `help_gameplay_fps <fps>`: the value to set, or why not.
pub(crate) fn parse_argument(text: &str) -> Result<i64, String> {
    text.trim()
        .parse::<i64>()
        .ok()
        .filter(|fps| valid(*fps))
        .ok_or_else(|| {
            format!(
                "usage: {COMMAND} [fps], 0 for Auto or {MIN_GAMEPLAY_FPS} to {MAX_GAMEPLAY_FPS}"
            )
        })
}

impl crate::GpuState {
    /// `help_gameplay_fps`: explain it in the console and open the card to choose;
    /// `help_gameplay_fps <fps>` sets it. The lines go to the console.
    pub(crate) fn gameplay_fps_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        let lines = match args {
            [] => {
                let lines = vec![
                    EXPLANATION.to_owned(),
                    format!(
                        "Yours is {} ({CVAR}). {FIXED_NOTE}",
                        describe(console.gameplay_fps())
                    ),
                ];
                console.open_gameplay_fps();
                self.sync_cursor_policy();
                lines
            }
            [value] => {
                let fps = parse_argument(value)?;
                let line = console
                    .set_gameplay_fps(fps)
                    .ok_or("Could not set the gameplay FPS")?;
                vec![line]
            }
            _ => return Err(parse_argument("").unwrap_err()),
        };
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(max_fps: Option<i64>, cmd_rate_cap: bool) -> Origin {
        Origin {
            file: "eternaljk.cfg".to_owned(),
            max_fps,
            cmd_rate_cap,
        }
    }

    #[test]
    fn an_import_picks_auto_or_jof_125() {
        // Auto follows the imported FPS cap: 333 moves like 333.
        let choice = Choice::new(142, Some(origin(Some(333), false)));
        assert_eq!((choice.selected, choice.value()), (0, Some(0)));
        assert_eq!(choice.caption(0), "your FPS cap");
        // JoF EJK's cl_cmdratecap moved at 125, whatever its FPS cap.
        let choice = Choice::new(0, Some(origin(Some(333), true)));
        assert_eq!(choice.value(), Some(125));
        assert_eq!(choice.caption(1), "your old one");
        assert!(origin(None, false).line().is_none());
        assert_eq!(Choice::label(0), "Auto");
        assert_eq!(Choice::label(4), "333");
        assert_eq!(Choice::label(OTHER), "Other");
    }

    #[test]
    fn digits_go_to_other_and_only_in_range_values_count() {
        let mut choice = Choice::new(125, None);
        assert_eq!(choice.caption(1), "in use");
        choice.type_digit('4');
        choice.type_digit('0');
        assert_eq!(choice.selected, OTHER);
        assert_eq!(choice.value(), Some(40));
        choice.type_digit('0');
        assert_eq!(choice.draft_note(), "moves in 2 ms steps, 500 a second");
        choice.type_digit('0');
        choice.type_digit('9');
        assert_eq!(choice.draft, "4000", "four digits at most");
        assert_eq!(choice.value(), None, "above 1000");
        choice.backspace();
        choice.backspace();
        choice.backspace();
        assert_eq!(choice.value(), None, "4 is below 20");
        choice.step(1);
        assert_eq!(choice.value(), Some(0), "wraps to Auto");
        choice.step(-1);
        assert_eq!(choice.selected, OTHER);
    }

    #[test]
    fn left_and_right_step_through_the_presets() {
        assert_eq!(step_preset(0, 1), 125);
        assert_eq!(step_preset(125, 1), 142);
        assert_eq!(step_preset(333, 1), 0, "wraps to Auto");
        assert_eq!(step_preset(0, -1), 333, "wraps");
        assert_eq!(
            step_preset(200, 1),
            250,
            "a typed value goes to its neighbour"
        );
        assert_eq!(step_preset(200, -1), 142);
    }

    #[test]
    fn the_command_takes_a_number_in_range() {
        assert_eq!(parse_argument("333"), Ok(333));
        assert_eq!(parse_argument("0"), Ok(0), "Auto");
        assert!(parse_argument("10").is_err());
        assert!(parse_argument("fast").is_err());
        assert_eq!(describe(0), "Auto (your FPS cap)");
        assert_eq!(describe(333), "333 (3 ms steps)");
        assert_eq!(describe(142), "142 (7 ms steps)");
        assert_eq!(describe(400), "400 (2 ms steps, 500 a second)");
    }
}

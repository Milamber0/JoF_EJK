//! Resolution choices for the settings screen and the list that picks one.
//!
//! The choices are the monitor's video-mode sizes, grouped by aspect ratio
//! with the monitor's own aspect first, then the others by how close their
//! shape is to it (16:10 after 16:9, then 3:2, 4:3, 5:4), and the largest size
//! first in each group. A window can be any size, so outside exclusive fullscreen the
//! classic presets that fit the monitor and the current custom size are
//! offered too; exclusive fullscreen offers only real video modes.

use super::catalog::RESOLUTIONS;
use super::display::MonitorModes;
use winit::keyboard::KeyCode;

/// Rows the wheel scrolls per notch.
const WHEEL_ROWS: usize = 3;

/// Named aspect ratios with their width/height ratio. A size takes the
/// nearest name within [`ASPECT_TOLERANCE`]; 1366x768 is 16:9 and
/// 3440x1440 is 21:9, as monitors are sold.
const ASPECTS: [(&str, f32); 9] = [
    ("5:4", 1.25),
    ("4:3", 4.0 / 3.0),
    ("3:2", 1.5),
    ("16:10", 1.6),
    ("5:3", 5.0 / 3.0),
    ("16:9", 16.0 / 9.0),
    ("21:9", 64.0 / 27.0),
    ("32:10", 3.2),
    ("32:9", 32.0 / 9.0),
];

/// Largest relative distance from a named ratio that still takes its name.
const ASPECT_TOLERANCE: f32 = 0.025;

/// Name of sizes matching no named ratio.
const OTHER_ASPECT: &str = "Other";

/// One size the player can pick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ResolutionChoice {
    pub(super) size: [u32; 2],
    /// Aspect-ratio group name (`16:9`).
    pub(super) aspect: &'static str,
    /// The monitor's desktop size.
    pub(super) desktop: bool,
}

/// Aspect-ratio group of `size`: the nearest named ratio, or "Other".
pub(super) fn aspect_name([width, height]: [u32; 2]) -> &'static str {
    if width == 0 || height == 0 {
        return OTHER_ASPECT;
    }
    let ratio = width as f32 / height as f32;
    ASPECTS
        .iter()
        .map(|(name, nominal)| (name, ((ratio - nominal) / nominal).abs()))
        .filter(|(_, error)| *error <= ASPECT_TOLERANCE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(OTHER_ASPECT, |(name, _)| name)
}

/// The width/height ratio of a named aspect group; `None` for "Other".
fn aspect_ratio(aspect: &str) -> Option<f32> {
    ASPECTS
        .iter()
        .find(|(name, _)| *name == aspect)
        .map(|(_, ratio)| *ratio)
}

/// How far an aspect group's shape is from `reference` (a width/height ratio):
/// the size of the log of their quotient, so 16:10 is nearer 16:9 than 4:3 is,
/// and a group as much wider counts as much as one narrower. "Other" is last.
fn aspect_distance(aspect: &str, reference: f32) -> f32 {
    aspect_ratio(aspect).map_or(f32::INFINITY, |ratio| (ratio / reference).ln().abs())
}

/// Parse `r_resolution` text (`1920x1080`).
pub(super) fn parse_size(text: &str) -> Option<[u32; 2]> {
    let (width, height) = text.trim().split_once(['x', 'X'])?;
    let width = width.trim().parse().ok()?;
    let height = height.trim().parse().ok()?;
    (width > 0 && height > 0).then_some([width, height])
}

/// Fill `out` with the sizes on offer, grouped by aspect ratio (the
/// monitor's first, then the nearest shapes) and largest first within a group.
pub(super) fn build_choices(
    monitor: Option<&MonitorModes>,
    current: Option<[u32; 2]>,
    exclusive: bool,
    out: &mut Vec<ResolutionChoice>,
) {
    out.clear();
    let desktop = monitor.and_then(|monitor| monitor.desktop);
    let mut push = |size: [u32; 2]| {
        if !out.iter().any(|choice| choice.size == size) {
            out.push(ResolutionChoice {
                size,
                aspect: aspect_name(size),
                desktop: desktop == Some(size),
            });
        }
    };
    let modes = monitor.map_or(&[][..], |monitor| monitor.sizes.as_slice());
    for size in modes {
        push(*size);
    }
    if !exclusive || modes.is_empty() {
        let fits = |[width, height]: [u32; 2]| {
            desktop.is_none_or(|[max_width, max_height]| width <= max_width && height <= max_height)
        };
        for size in RESOLUTIONS.iter().filter_map(|text| parse_size(text)) {
            if fits(size) {
                push(size);
            }
        }
        if let Some(size) = current {
            push(size);
        }
    }
    let native = desktop.map_or("", aspect_name);
    // Without a known monitor shape the groups are ordered from 16:9's.
    let reference = aspect_ratio(native).unwrap_or(16.0 / 9.0);
    out.sort_by(|a, b| {
        (a.aspect != native)
            .cmp(&(b.aspect != native))
            .then_with(|| {
                aspect_distance(a.aspect, reference)
                    .total_cmp(&aspect_distance(b.aspect, reference))
            })
            .then_with(|| a.aspect.cmp(b.aspect))
            .then_with(|| b.size.cmp(&a.size))
    });
}

/// The size `direction` steps to from `current` among `choices`: smaller
/// (-1) or larger (+1) within the current size's aspect group, stopping at
/// the ends. A size not on offer steps into the first group.
pub(super) fn step(
    choices: &[ResolutionChoice],
    current: Option<[u32; 2]>,
    direction: i32,
) -> Option<[u32; 2]> {
    let position = current.and_then(|size| choices.iter().position(|c| c.size == size));
    let Some(position) = position else {
        let first = choices.first()?.aspect;
        let group = choices.iter().filter(|choice| choice.aspect == first);
        // Largest is first; stepping down from nowhere starts at the top.
        return if direction < 0 {
            group.map(|choice| choice.size).next()
        } else {
            group.map(|choice| choice.size).last()
        };
    };
    let aspect = choices[position].aspect;
    // Groups list largest first, so a larger size is an earlier entry.
    let next = if direction > 0 {
        position.checked_sub(1)
    } else {
        Some(position + 1)
    };
    next.and_then(|index| choices.get(index))
        .filter(|choice| choice.aspect == aspect)
        .map(|choice| choice.size)
        .or(Some(choices[position].size))
}

/// What a key on the open list asks for.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum PickResult {
    /// Still choosing.
    None,
    /// Close the list; `Some` picks that size.
    Close(Option<[u32; 2]>),
}

/// The open resolution list: the choices, the highlight and the view.
pub(super) struct ResolutionPicker {
    open: bool,
    choices: Vec<ResolutionChoice>,
    /// The size in use when the list opened.
    current: Option<[u32; 2]>,
    /// One line on what the size means in the current display mode.
    note: &'static str,
    selected: usize,
    first: usize,
    /// Rows the view fits; set by the view.
    page: usize,
}

impl ResolutionPicker {
    pub(super) fn new() -> Self {
        Self {
            open: false,
            choices: Vec::with_capacity(48),
            current: None,
            note: "",
            selected: 0,
            first: 0,
            page: 10,
        }
    }

    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    /// Open with `choices`, the `current` size highlighted in mid view and
    /// `note` under the title.
    pub(super) fn open(
        &mut self,
        choices: &[ResolutionChoice],
        current: Option<[u32; 2]>,
        note: &'static str,
    ) {
        self.open = true;
        self.current = current;
        self.note = note;
        self.choices.clear();
        self.choices.extend_from_slice(choices);
        self.selected = current
            .and_then(|size| self.choices.iter().position(|c| c.size == size))
            .unwrap_or(0);
        self.first = self.selected.saturating_sub(self.page / 2);
        self.clamp_scroll();
    }

    /// Replace the choices (monitor facts arrived), keeping the highlight.
    pub(super) fn update_choices(&mut self, choices: &[ResolutionChoice]) {
        let keep = self.highlighted().map(|choice| choice.size);
        self.choices.clear();
        self.choices.extend_from_slice(choices);
        self.selected = keep
            .and_then(|size| self.choices.iter().position(|c| c.size == size))
            .unwrap_or(0);
        self.reveal_selection();
    }

    pub(super) fn close(&mut self) {
        self.open = false;
    }

    pub(super) fn choices(&self) -> &[ResolutionChoice] {
        &self.choices
    }

    pub(super) fn current(&self) -> Option<[u32; 2]> {
        self.current
    }

    pub(super) fn note(&self) -> &'static str {
        self.note
    }

    pub(super) fn selected(&self) -> usize {
        self.selected
    }

    pub(super) fn first(&self) -> usize {
        self.first
    }

    pub(super) fn page(&self) -> usize {
        self.page
    }

    /// The view reports how many rows fit.
    pub(super) fn set_page(&mut self, rows: usize) {
        self.page = rows.max(1);
        self.clamp_scroll();
        self.reveal_selection();
    }

    fn highlighted(&self) -> Option<ResolutionChoice> {
        self.choices.get(self.selected).copied()
    }

    /// Handle one key.
    pub(super) fn key(&mut self, key: KeyCode) -> PickResult {
        match key {
            KeyCode::Escape | KeyCode::Backspace => {
                self.open = false;
                return PickResult::Close(None);
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => return self.pick(),
            KeyCode::ArrowUp | KeyCode::KeyW => self.move_by(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.page as isize)),
            KeyCode::PageDown => self.move_by(self.page as isize),
            KeyCode::Home => self.move_to(0),
            KeyCode::End => self.move_to(self.choices.len().saturating_sub(1)),
            _ => {}
        }
        PickResult::None
    }

    /// Pick the highlighted size and close.
    pub(super) fn pick(&mut self) -> PickResult {
        match self.highlighted() {
            Some(choice) => {
                self.open = false;
                PickResult::Close(Some(choice.size))
            }
            None => PickResult::None,
        }
    }

    /// Highlight list position `position` (the pointer rests on it).
    pub(super) fn hover(&mut self, position: usize) {
        if position < self.choices.len() {
            self.selected = position;
        }
    }

    /// Scroll by wheel `notches` (positive = down the list), keeping the
    /// highlight inside the view.
    pub(super) fn scroll(&mut self, notches: isize) {
        let first = self.first as isize + notches * WHEEL_ROWS as isize;
        self.first = first.max(0) as usize;
        self.clamp_scroll();
        let last = (self.first + self.page - 1).min(self.choices.len().saturating_sub(1));
        self.selected = self.selected.clamp(self.first.min(last), last);
    }

    fn move_by(&mut self, step: isize) {
        let last = self.choices.len().saturating_sub(1) as isize;
        self.move_to((self.selected as isize + step).clamp(0, last.max(0)) as usize);
    }

    fn move_to(&mut self, position: usize) {
        self.selected = position.min(self.choices.len().saturating_sub(1));
        self.reveal_selection();
    }

    fn reveal_selection(&mut self) {
        if self.selected < self.first {
            self.first = self.selected;
        } else if self.selected >= self.first + self.page {
            self.first = self.selected + 1 - self.page;
        }
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        self.first = self.first.min(self.choices.len().saturating_sub(self.page));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(sizes: &[[u32; 2]], desktop: [u32; 2]) -> MonitorModes {
        MonitorModes {
            sizes: sizes.to_vec(),
            desktop: Some(desktop),
            exclusive: true,
        }
    }

    fn sizes(choices: &[ResolutionChoice]) -> Vec<[u32; 2]> {
        choices.iter().map(|choice| choice.size).collect()
    }

    #[test]
    fn aspect_names_follow_how_monitors_are_sold() {
        assert_eq!(aspect_name([1920, 1080]), "16:9");
        assert_eq!(aspect_name([1366, 768]), "16:9");
        assert_eq!(aspect_name([1920, 1200]), "16:10");
        assert_eq!(aspect_name([1024, 768]), "4:3");
        assert_eq!(aspect_name([1280, 1024]), "5:4");
        assert_eq!(aspect_name([2560, 1080]), "21:9");
        assert_eq!(aspect_name([3440, 1440]), "21:9");
        assert_eq!(aspect_name([5120, 1440]), "32:9");
        assert_eq!(aspect_name([2400, 600]), "Other");
        assert_eq!(aspect_name([0, 600]), "Other");
    }

    #[test]
    fn sizes_parse_like_the_cvar() {
        assert_eq!(parse_size("1920x1080"), Some([1920, 1080]));
        assert_eq!(parse_size(" 800X600 "), Some([800, 600]));
        assert_eq!(parse_size("0x600"), None);
        assert_eq!(parse_size("wide"), None);
    }

    #[test]
    fn the_monitors_aspect_comes_first_then_the_nearest_shapes() {
        let wide = monitor(
            &[
                [2560, 1440],
                [1920, 1080],
                [1680, 1050],
                [1280, 1024],
                [1024, 768],
            ],
            [2560, 1440],
        );
        let mut out = Vec::new();
        build_choices(Some(&wide), Some([1920, 1080]), true, &mut out);
        assert_eq!(
            sizes(&out),
            [
                [2560, 1440],
                [1920, 1080],
                [1680, 1050],
                [1024, 768],
                [1280, 1024],
            ]
        );
        assert!(out[0].desktop);
        assert!(!out[1].desktop);
        // A 4:3 monitor: 5:4 is next to it, 16:10 before 16:9, 21:9 last.
        let square = monitor(
            &[
                [2560, 1080],
                [1920, 1080],
                [1680, 1050],
                [1280, 1024],
                [1024, 768],
            ],
            [1024, 768],
        );
        build_choices(Some(&square), None, true, &mut out);
        assert_eq!(
            out.iter().map(|choice| choice.aspect).collect::<Vec<_>>(),
            ["4:3", "5:4", "16:10", "16:9", "21:9"]
        );
    }

    #[test]
    fn windows_also_get_fitting_presets_and_the_custom_size() {
        let monitor = monitor(&[[1920, 1080], [1280, 720]], [1920, 1080]);
        let mut out = Vec::new();
        build_choices(Some(&monitor), Some([1500, 900]), false, &mut out);
        // 2560x1440 and 3840x2160 do not fit; 1500x900 is 5:3.
        assert_eq!(
            sizes(&out),
            [[1920, 1080], [1600, 900], [1280, 720], [1500, 900]]
        );
    }

    #[test]
    fn exclusive_offers_only_video_modes() {
        let monitor = monitor(&[[1920, 1080], [1280, 720]], [1920, 1080]);
        let mut out = Vec::new();
        build_choices(Some(&monitor), Some([1500, 900]), true, &mut out);
        assert_eq!(sizes(&out), [[1920, 1080], [1280, 720]]);
    }

    #[test]
    fn without_a_monitor_the_presets_remain() {
        let mut out = Vec::new();
        build_choices(None, Some([1280, 720]), false, &mut out);
        assert_eq!(
            sizes(&out),
            [
                [3840, 2160],
                [2560, 1440],
                [1920, 1080],
                [1600, 900],
                [1280, 720],
            ]
        );
    }

    #[test]
    fn stepping_stays_in_the_aspect_group() {
        let monitor = monitor(
            &[[1920, 1080], [1280, 720], [1280, 1024], [1024, 768]],
            [1920, 1080],
        );
        let mut out = Vec::new();
        build_choices(Some(&monitor), None, true, &mut out);
        assert_eq!(step(&out, Some([1280, 720]), 1), Some([1920, 1080]));
        assert_eq!(step(&out, Some([1920, 1080]), 1), Some([1920, 1080]));
        assert_eq!(step(&out, Some([1920, 1080]), -1), Some([1280, 720]));
        assert_eq!(step(&out, Some([1280, 720]), -1), Some([1280, 720]));
        // 4:3 and 5:4 are groups of one: stepping never crosses into another.
        assert_eq!(step(&out, Some([1024, 768]), 1), Some([1024, 768]));
        assert_eq!(step(&out, Some([1280, 1024]), 1), Some([1280, 1024]));
        assert_eq!(step(&out, Some([999, 999]), 1), Some([1280, 720]));
        assert_eq!(step(&out, Some([999, 999]), -1), Some([1920, 1080]));
        assert_eq!(step(&[], Some([1920, 1080]), 1), None);
    }

    #[test]
    fn the_list_opens_on_the_current_size_and_picks() {
        let monitor = monitor(&[[1920, 1080], [1600, 900], [1280, 720]], [1920, 1080]);
        let mut out = Vec::new();
        build_choices(Some(&monitor), None, true, &mut out);
        let mut picker = ResolutionPicker::new();
        picker.open(&out, Some([1600, 900]), "");
        assert!(picker.is_open());
        assert_eq!(picker.selected(), 1);
        assert_eq!(picker.key(KeyCode::ArrowDown), PickResult::None);
        assert_eq!(
            picker.key(KeyCode::Enter),
            PickResult::Close(Some([1280, 720]))
        );
        assert!(!picker.is_open());
        picker.open(&out, Some([1600, 900]), "");
        assert_eq!(picker.key(KeyCode::Escape), PickResult::Close(None));
    }

    #[test]
    fn scrolling_keeps_the_highlight_in_view() {
        let modes: Vec<[u32; 2]> = (0..20).map(|i| [16 * (100 - i), 9 * (100 - i)]).collect();
        let monitor = monitor(&modes, [1600, 900]);
        let mut out = Vec::new();
        build_choices(Some(&monitor), None, true, &mut out);
        let mut picker = ResolutionPicker::new();
        picker.set_page(5);
        picker.open(&out, Some(out[0].size), "");
        assert_eq!((picker.first(), picker.selected()), (0, 0));
        picker.scroll(2);
        assert_eq!((picker.first(), picker.selected()), (6, 6));
        picker.key(KeyCode::End);
        assert_eq!((picker.first(), picker.selected()), (15, 19));
    }
}

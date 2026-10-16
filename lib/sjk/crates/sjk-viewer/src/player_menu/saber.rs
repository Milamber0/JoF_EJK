//! Draft state and catalogue filtering for the saber page.

use super::controller::wrap;
use super::rows::SaberRow;
use super::*;
use crate::saber::Color;
use sjk_client::{LegacySaberDefinition, LegacySaberType, pack_saber_rgb, unpack_saber_rgb};

/// `color1`/`color2` value selecting a custom RGB tint (JA+/TaystJK
/// `SABER_RGB`).
pub(super) const RGB_COLOR_INDEX: u8 = 6;
/// Keyboard step of one RGB channel.
const CHANNEL_STEP: isize = 5;
/// The blade colour palette: the six stock colours, then the custom chip.
pub(super) const PALETTE: [u8; 7] = [0, 1, 2, 3, 4, 5, RGB_COLOR_INDEX];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SaberStyle {
    Single,
    Staff,
    Dual,
}

impl SaberStyle {
    const ALL: [Self; 3] = [Self::Single, Self::Staff, Self::Dual];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Staff => "Staff",
            Self::Dual => "Dual",
        }
    }

    /// Ready stance the game idles in with this style's sabers drawn
    /// (`PM_GetSaberStance`, `codemp/game/bg_pmove.c:276-333`: dual and
    /// staff have their own stances, every single-saber style but fast and
    /// strong stands in `BOTH_STAND2`).
    pub(super) fn stance(self) -> &'static str {
        match self {
            Self::Single => "BOTH_STAND2",
            Self::Staff => "BOTH_SABERSTAFF_STANCE",
            Self::Dual => "BOTH_SABERDUAL_STANCE",
        }
    }
}

/// What the stage model holds: the `.sab` name in each hand (`None` for an
/// empty hand), the blade tint of each saber, the stance to idle in, and
/// whether the sabers are thrown out to the backdrop's saber shot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StageSabers<'a> {
    pub(crate) hilts: [Option<&'a str>; 2],
    pub(crate) colors: [[u8; 3]; 2],
    pub(crate) stance: &'static str,
    pub(crate) thrown: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SaberDraft {
    style: SaberStyle,
    primary: String,
    secondary: String,
    color1: u8,
    color2: u8,
    /// Custom tints behind `cp_sbRGB1`/`cp_sbRGB2`; `None` while the cvar
    /// is still 0 (never chosen), so switching to RGB can start from the
    /// colour that was selected before.
    rgb: [Option<[u8; 3]>; 2],
}

impl Default for SaberDraft {
    fn default() -> Self {
        Self {
            style: SaberStyle::Single,
            primary: "single_1".to_owned(),
            secondary: "none".to_owned(),
            color1: 4,
            color2: 4,
            rgb: [None; 2],
        }
    }
}

/// Blade tint of a `color1`/`color2` value with its custom RGB.
fn blade_rgb(index: u8, rgb: Option<[u8; 3]>) -> [u8; 3] {
    match Color::ALL.get(usize::from(index)) {
        Some(color) => color.blade_rgb(),
        None => rgb.unwrap_or_else(|| unpack_saber_rgb(0)),
    }
}

pub(super) struct SaberMenu {
    draft: SaberDraft,
    /// Words a listed hilt's name must contain (the SJK UI's hilt search; empty
    /// lists every hilt the style allows).
    search: String,
}

impl SaberMenu {
    pub(super) fn new() -> Self {
        Self {
            draft: SaberDraft::default(),
            search: String::with_capacity(32),
        }
    }

    /// The hilt search as typed.
    pub(super) fn search(&self) -> &str {
        &self.search
    }

    /// Change the hilt search to `text`.
    pub(super) fn set_search(&mut self, text: &str) {
        if self.search != text {
            self.search.clear();
            self.search.push_str(text);
        }
    }

    /// The hilts the first saber's list (`second` false) or the second's offers,
    /// in the catalogue's order (the game's load order), with each one's place
    /// among the hilts the style allows (what [`Self::select`] takes): the
    /// style's hilts whose name or file name holds every word of the search.
    pub(super) fn listed<'a>(
        &'a self,
        catalog: &'a LegacyAssetCatalog,
        second: bool,
    ) -> impl Iterator<Item = (usize, &'a LegacySaberDefinition)> + 'a {
        let style = self.hand_style(second);
        catalog
            .saber_hilts
            .iter()
            .filter(move |hilt| allowed(hilt, style))
            .enumerate()
            .filter(|(_, hilt)| {
                let search = self.search.trim();
                search.is_empty()
                    || super::team_filter::search_matches(search, &hilt.display_name)
                    || super::team_filter::search_matches(search, &hilt.name)
            })
    }

    /// The style a hand's hilts must suit: the second saber is a single one.
    fn hand_style(&self, second: bool) -> SaberStyle {
        if second {
            SaberStyle::Single
        } else {
            self.draft.style
        }
    }

    pub(super) fn open(&mut self, console: &ViewerConsole) {
        self.draft.primary = console
            .text_value("saber1")
            .unwrap_or("single_1")
            .to_owned();
        self.draft.secondary = console.text_value("saber2").unwrap_or("none").to_owned();
        self.draft.color1 = colour_cvar(console, "color1").min(RGB_COLOR_INDEX);
        self.draft.color2 = colour_cvar(console, "color2").min(RGB_COLOR_INDEX);
        for (slot, cvar) in self.draft.rgb.iter_mut().zip(["cp_sbRGB1", "cp_sbRGB2"]) {
            let packed = console
                .integer_cvar(cvar)
                .and_then(|value| u32::try_from(value).ok());
            *slot = packed.filter(|packed| *packed != 0).map(unpack_saber_rgb);
        }
        self.draft.style = if self.draft.secondary.eq_ignore_ascii_case("none") {
            SaberStyle::Single
        } else {
            SaberStyle::Dual
        };
    }

    /// A `saber1` that names a staff hilt is a Staff profile, which the
    /// cvars alone cannot tell from Single.
    pub(super) fn reconcile_style(&mut self, catalog: Option<&LegacyAssetCatalog>) {
        if self.draft.style == SaberStyle::Single
            && catalog
                .and_then(|catalog| find_hilt(catalog, &self.draft.primary))
                .is_some_and(|hilt| hilt.saber_type == LegacySaberType::Staff)
        {
            self.draft.style = SaberStyle::Staff;
        }
    }

    pub(super) fn style(&self) -> SaberStyle {
        self.draft.style
    }

    pub(super) fn hilt(&self, second: bool) -> &str {
        if second {
            &self.draft.secondary
        } else {
            &self.draft.primary
        }
    }

    pub(super) fn color(&self, second: bool) -> u8 {
        if second {
            self.draft.color2
        } else {
            self.draft.color1
        }
    }

    /// The tint the blade is drawn with: the stock triplet, or the custom
    /// RGB when the colour is RGB.
    pub(super) fn rgb(&self, second: bool) -> [u8; 3] {
        blade_rgb(self.color(second), self.draft.rgb[usize::from(second)])
    }

    /// The custom tint the RGB chip shows: the stored one, else what the
    /// blade is drawn with now.
    pub(super) fn custom_rgb(&self, second: bool) -> [u8; 3] {
        self.draft.rgb[usize::from(second)].unwrap_or_else(|| self.rgb(second))
    }

    /// One channel of the custom RGB.
    pub(super) fn channel(&self, second: bool, channel: usize) -> u8 {
        self.rgb(second)[channel]
    }

    /// Set one channel; the colour becomes custom (RGB), starting from the
    /// tint that was showing, so a stock colour can be nudged directly.
    pub(super) fn set_channel(&mut self, second: bool, channel: usize, value: u8) {
        let mut rgb = self.rgb(second);
        rgb[channel] = value;
        self.draft.rgb[usize::from(second)] = Some(rgb);
        self.select_color(second, RGB_COLOR_INDEX);
    }

    /// The draft as the stage model should hold it.
    pub(super) fn stage_sabers(&self, thrown: bool) -> StageSabers<'_> {
        fn hand(name: &str) -> Option<&str> {
            (!name.eq_ignore_ascii_case("none")).then_some(name)
        }
        StageSabers {
            hilts: [hand(&self.draft.primary), hand(&self.draft.secondary)],
            colors: [self.rgb(false), self.rgb(true)],
            stance: self.draft.style.stance(),
            thrown,
        }
    }

    /// Display name of the hilt in `saber1`/`saber2`, falling back to the
    /// cvar value while the catalogue is unavailable.
    pub(super) fn hilt_label<'a>(
        &'a self,
        catalog: Option<&'a LegacyAssetCatalog>,
        second: bool,
    ) -> &'a str {
        let name = self.hilt(second);
        catalog
            .and_then(|catalog| find_hilt(catalog, name))
            .map_or(name, |hilt| hilt.display_name.as_str())
    }

    pub(super) fn set_style(&mut self, style: SaberStyle, catalog: Option<&LegacyAssetCatalog>) {
        self.draft.style = style;
        if let Some(catalog) = catalog {
            if !matches_style(find_hilt(catalog, &self.draft.primary), style) {
                if let Some(hilt) = catalog.saber_hilts.iter().find(|hilt| allowed(hilt, style)) {
                    self.draft.primary.clone_from(&hilt.name);
                }
            }
            if style == SaberStyle::Dual
                && !matches_style(
                    find_hilt(catalog, &self.draft.secondary),
                    SaberStyle::Single,
                )
            {
                if let Some(hilt) = catalog
                    .saber_hilts
                    .iter()
                    .find(|hilt| allowed(hilt, SaberStyle::Single))
                {
                    self.draft.secondary.clone_from(&hilt.name);
                }
            }
        }
        if style != SaberStyle::Dual {
            self.draft.secondary.clear();
            self.draft.secondary.push_str("none");
        }
    }

    /// Step `row` by `direction`, wrapping within the hilts the style allows.
    pub(super) fn adjust(
        &mut self,
        row: SaberRow,
        direction: isize,
        catalog: Option<&LegacyAssetCatalog>,
    ) {
        match row {
            SaberRow::Style => {
                let index = SaberStyle::ALL
                    .iter()
                    .position(|style| *style == self.draft.style)
                    .unwrap_or(0);
                let style = SaberStyle::ALL[wrap(index, direction, SaberStyle::ALL.len())];
                self.set_style(style, catalog);
            }
            SaberRow::Hilt | SaberRow::SecondHilt => {
                if let Some(catalog) = catalog {
                    self.cycle_hilt(catalog, row.second(), direction);
                }
            }
            SaberRow::Blade | SaberRow::SecondBlade => {
                let current = usize::from(self.color(row.second()));
                self.select_color(
                    row.second(),
                    PALETTE[wrap(current, direction, PALETTE.len())],
                );
            }
            _ => {
                let Some(channel) = row.channel() else {
                    return;
                };
                let current = isize::from(self.channel(row.second(), channel));
                let value = (current + direction * CHANNEL_STEP).clamp(0, 255) as u8;
                self.set_channel(row.second(), channel, value);
            }
        }
    }

    /// Step the hand's hilt `direction` through the hilts listed (the search's
    /// matches); a hilt the search leaves out steps in from the list's edge.
    fn cycle_hilt(&mut self, catalog: &LegacyAssetCatalog, second: bool, direction: isize) {
        let current = self.hilt(second);
        let (mut count, mut at, mut first, mut last) = (0, None, None, None);
        for (index, hilt) in self.listed(catalog, second) {
            if hilt.name.eq_ignore_ascii_case(current) {
                at = Some(count);
            }
            first = first.or(Some(index));
            last = Some(index);
            count += 1;
        }
        let target = match at {
            Some(at) => self
                .listed(catalog, second)
                .nth(wrap(at, direction, count))
                .map(|(index, _)| index),
            None if direction > 0 => first,
            None => last,
        };
        if let Some(index) = target {
            self.select(catalog, index, second);
        }
    }

    pub(super) fn select(&mut self, catalog: &LegacyAssetCatalog, index: usize, second: bool) {
        let style = self.hand_style(second);
        let Some(hilt) = catalog
            .saber_hilts
            .iter()
            .filter(|hilt| allowed(hilt, style))
            .nth(index)
        else {
            return;
        };
        if second {
            self.draft.secondary.clone_from(&hilt.name);
        } else {
            self.draft.primary.clone_from(&hilt.name);
        }
    }

    /// Choose a colour; picking RGB for the first time starts from the tint
    /// of the colour that was selected until then.
    pub(super) fn select_color(&mut self, second: bool, index: u8) {
        let index = index.min(RGB_COLOR_INDEX);
        let previous = self.rgb(second);
        let slot = usize::from(second);
        if index == RGB_COLOR_INDEX && self.draft.rgb[slot].is_none() {
            self.draft.rgb[slot] = Some(previous);
        }
        if second {
            self.draft.color2 = index;
        } else {
            self.draft.color1 = index;
        }
    }

    /// Write the saber cvars to the console. Called after every change.
    pub(super) fn apply(&mut self, console: &mut ViewerConsole) {
        console.set_cvar("saber1", &self.draft.primary);
        console.set_cvar("saber2", &self.draft.secondary);
        // The colour keys also carry the worn hat and cape (JoF EJK's
        // `CG_SetSaberColorCvar` keeps them the same way).
        for (cvar, colour) in [("color1", self.draft.color1), ("color2", self.draft.color2)] {
            let worn = console
                .text_value(cvar)
                .and_then(|value| sjk_client::split_color_value(value).1)
                .map(str::to_owned);
            let value = sjk_client::join_color_value(i64::from(colour), worn.as_deref());
            console.set_cvar(cvar, &value);
        }
        for (slot, cvar) in self.draft.rgb.iter().zip(["cp_sbRGB1", "cp_sbRGB2"]) {
            let packed = slot.map_or(0, pack_saber_rgb);
            console.set_cvar(cvar, &packed.to_string());
        }
    }
}

/// The saber colour of a `color1`/`color2` value (`atoi`, so a worn
/// cosmetic after the digits does not change it); blue when unset.
fn colour_cvar(console: &ViewerConsole, cvar: &str) -> u8 {
    console
        .text_value(cvar)
        .map(|value| sjk_client::split_color_value(value).0)
        .and_then(|colour| u8::try_from(colour).ok())
        .unwrap_or(4)
}

fn find_hilt<'a>(catalog: &'a LegacyAssetCatalog, name: &str) -> Option<&'a LegacySaberDefinition> {
    catalog
        .saber_hilts
        .iter()
        .find(|hilt| hilt.name.eq_ignore_ascii_case(name))
}

fn matches_style(hilt: Option<&LegacySaberDefinition>, style: SaberStyle) -> bool {
    hilt.is_some_and(|hilt| allowed(hilt, style))
}

pub(super) fn allowed(hilt: &LegacySaberDefinition, style: SaberStyle) -> bool {
    match style {
        SaberStyle::Staff => hilt.saber_type == LegacySaberType::Staff,
        SaberStyle::Single | SaberStyle::Dual => hilt.saber_type == LegacySaberType::Single,
    }
}

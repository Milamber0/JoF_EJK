//! The SJK UI's Profile and Collection screens (`docs/sjk-ui.md`, Profile screen and
//! Collection): existing screens shown as one, under one row of tabs each. The Profile
//! screen's Character, Saber and Force are the player screen's pages (`player_menu`),
//! its SJK Profile the console's Profile page (`profile_panel`: bio, picture and
//! record). The Collection screen's Medals, Achievements, Shaders, Toys and Nameplates
//! are the console's Collection page (`collection_panel`), one page turning between its
//! tabs, and its Holocrons the console's Holocrons page (`holocrons_panel`).
//! Each screen keeps its state, keys and pointer; the Profile row is the player screen's
//! own tabs grown to four ([`tabs`]), drawn at the same place by whichever screen shows,
//! with the tab on show lit, and the switch between them is here.
//!
//! The tabs change with Ctrl+Tab (Ctrl+Shift+Tab back) from any of them, even while a
//! field is typed in, within the screen on show; on the player screen's pages Tab, `]`
//! and `[` walk the Profile screen's four as they walked its three (Tab alone moves
//! within the console's pages, as everywhere in the UI). A click on a tab shows it; it
//! never reaches the screen under the row. Escape leaves the screen as each tab's own
//! way back does: to the game menu on the screen's entry, or to the main page.

use crate::GpuState;
use crate::menu::sjk::{Frame, color, text, top_bar};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use crate::player_menu::ReturnTarget;
use sjk_ui::{DrawCommand, FontWeight, InputEvent, PointerButton, TextAlign, Vec2};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The two screens with a row of tabs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Screen {
    /// The character and the SJK profile.
    Profile,
    /// What the player collects: medals, achievements, shaders, toys, nameplates and
    /// holocrons.
    Collection,
}

impl Screen {
    /// The screen's tabs, in the row's order.
    pub(crate) const fn tabs(self) -> &'static [Tab] {
        match self {
            Self::Profile => &PROFILE_TABS,
            Self::Collection => &COLLECTION_TABS,
        }
    }

    /// The tabs' names, in the row's order.
    pub(crate) const fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Profile => &PROFILE_LABELS,
            Self::Collection => &COLLECTION_LABELS,
        }
    }
}

/// A tab of one of the screens; the Profile screen's come first.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Tab {
    /// The player screen's pages: name and model, saber, Force.
    #[default]
    Character,
    Saber,
    Force,
    /// The SJK profile: picture, bio and record.
    Profile,
    /// The medals the SJK team gave, and those it gives.
    Medals,
    /// The achievements, by category.
    Achievements,
    /// Looks for the saber (the blade skins) and, later, the body.
    Shaders,
    /// Things to use in a match: the Illuminate holocron.
    Toys,
    /// Ornaments for the nameplate over the player's head.
    Nameplates,
    /// The player's holocrons, the loot-box drops, a 3D holocron cycled by tier.
    Holocrons,
    /// The cards opening holocrons brought, and the credits.
    Cards,
}

const PROFILE_TABS: [Tab; 4] = [Tab::Character, Tab::Saber, Tab::Force, Tab::Profile];
const COLLECTION_TABS: [Tab; 7] = [
    Tab::Medals,
    Tab::Achievements,
    Tab::Shaders,
    Tab::Toys,
    Tab::Nameplates,
    Tab::Holocrons,
    Tab::Cards,
];
const PROFILE_LABELS: [&str; 4] = ["Character", "Saber", "Force", "SJK Profile"];
const COLLECTION_LABELS: [&str; 7] = [
    "Medals",
    "Achievements",
    "Shaders",
    "Toys",
    "Nameplates",
    HOLOCRONS,
    "Cards",
];

/// The Collection screen's title: one place to rename it.
pub(crate) const COLLECTION: &str = "Collection";

/// The name of the tab of the player's holocrons, the loot-box drops.
pub(crate) const HOLOCRONS: &str = "Holocrons";

impl Tab {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 11] = [
        Self::Character,
        Self::Saber,
        Self::Force,
        Self::Profile,
        Self::Medals,
        Self::Achievements,
        Self::Shaders,
        Self::Toys,
        Self::Nameplates,
        Self::Holocrons,
        Self::Cards,
    ];

    /// The screen the tab is on.
    pub(crate) const fn screen(self) -> Screen {
        match self {
            Self::Character | Self::Saber | Self::Force | Self::Profile => Screen::Profile,
            _ => Screen::Collection,
        }
    }

    /// Its place on its screen's row.
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Character | Self::Medals => 0,
            Self::Saber | Self::Achievements => 1,
            Self::Force | Self::Shaders => 2,
            Self::Profile | Self::Toys => 3,
            Self::Nameplates => 4,
            Self::Holocrons => 5,
            Self::Cards => 6,
        }
    }

    /// The tab's name on the row.
    pub(crate) const fn label(self) -> &'static str {
        self.screen().labels()[self.index()]
    }

    /// The tab after this one on its screen (`forward`) or before it, wrapping.
    pub(crate) fn next(self, forward: bool) -> Self {
        let tabs = self.screen().tabs();
        let count = tabs.len();
        let index = if forward {
            (self.index() + 1) % count
        } else {
            (self.index() + count - 1) % count
        };
        tabs[index]
    }

    /// The player screen's page this tab is (0 Character, 1 Saber, 2 Force), or
    /// `None` for a tab the console shows.
    pub(crate) const fn player_page(self) -> Option<usize> {
        match self {
            Self::Character | Self::Saber | Self::Force => Some(self.index()),
            _ => None,
        }
    }

    /// The tab of the player screen's page `index`.
    pub(crate) fn of_player_page(index: usize) -> Self {
        PROFILE_TABS[index.min(2)]
    }
}

/// The row's tokens on each screen's canvas, tab `i` answering to `TOKEN + i`; clear
/// of the player screen's (to 1700), the Profile page's (to 1030) and the Collection
/// page's (to 1400).
pub(crate) const TOKEN: u16 = 2_000;
/// Where the row starts and its labels' top, in frame pixels: under the title, as the
/// player screen's own tabs stood.
const ROW_X: f32 = 96.0;
const ROW_Y: f32 = 146.0;
/// The room between two tabs' names.
const GAP: f32 = 48.0;
/// How far down the console's pages move what they show under their title, to leave
/// the row its room.
pub(crate) const SHIFT: f32 = 44.0;

/// A tab's name's size, and its count's (the Collection's "2/4") and the room before
/// the count.
const LABEL_SIZE: f32 = 26.0;
const COUNT_SIZE: f32 = 17.0;
const COUNT_GAP: f32 = 9.0;

/// How wide a tab's name is drawn, the player's text size and letter spacing
/// (`ui_textScale`, `ui_letterSpacing`) included: the row makes room for the styled
/// names, which would otherwise be cut at their tab's end.
fn label_width(label: &str) -> f32 {
    styled_width(label, crate::text::style::current())
}

/// [`label_width`] in `style`.
fn styled_width(label: &str, style: crate::text::TextStyle) -> f32 {
    sized_width(label, LABEL_SIZE, style)
}

/// How wide `label` is drawn at `size` in `style`.
fn sized_width(label: &str, size: f32, style: crate::text::TextStyle) -> f32 {
    let size = size * style.scale;
    let tracking = style.tracking * size * label.chars().count() as f32;
    (crate::text::display_width(label, size) + tracking).max(0.0)
}

/// How wide a tab's count is drawn, with the room before it; nothing without one.
fn count_width(count: &str) -> f32 {
    if count.is_empty() {
        0.0
    } else {
        COUNT_GAP + sized_width(count, COUNT_SIZE, crate::text::style::current())
    }
}

/// Where tab `index` of `labels` starts, how wide its name is and how wide its count
/// (frame pixels); `counts` follow the names, missing or empty for none. `widen` makes
/// room for a face wider than the display family (Inter, where the families are not
/// loaded).
fn places(labels: &[&str], counts: &[String], index: usize, widen: f32) -> (f32, f32, f32) {
    let count = |at: usize| counts.get(at).map_or(0.0, |count| count_width(count)) * widen;
    let x = labels
        .iter()
        .take(index)
        .enumerate()
        .map(|(at, label)| label_width(label) * widen + count(at) + GAP)
        .sum::<f32>();
    (
        ROW_X + x,
        labels
            .get(index)
            .map_or(0.0, |label| label_width(label) * widen),
        count(index),
    )
}

/// The area that answers the pointer for tab `index` of `labels`, frame pixels.
fn hit_area(labels: &[&str], counts: &[String], index: usize, widen: f32) -> [f32; 4] {
    let (x, width, count) = places(labels, counts, index, widen);
    [x - 8.0, ROW_Y - 4.0, width + count + 16.0, 46.0]
}

/// A row of tabs under a screen's title: `labels` from the left edge of the form, the
/// one at `current` gold and underlined, one hovered brighter; tab `i` answers to
/// `token_base + i`. The player screen draws its own three this way, and the Profile
/// screen's four ([`row`]).
pub(crate) fn tabs(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    labels: &[&str],
    current: usize,
    token_base: u16,
) {
    counted_tabs(canvas, frame, labels, &[], current, token_base, 1.0);
}

/// [`tabs`] with a count after each name (`counts`, empty for none), quieter and
/// smaller: the Collection's "2/4".
fn counted_tabs(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    labels: &[&str],
    counts: &[String],
    current: usize,
    token_base: u16,
    widen: f32,
) {
    let s = frame.s;
    for (index, label) in labels.iter().enumerate() {
        let (x, width, count) = places(labels, counts, index, widen);
        let token = token_base + index as u16;
        let lit = index == current;
        let hovered = canvas.token_hovered(token);
        text(
            canvas,
            TextFamily::Display,
            format_args!("{label}"),
            frame.rect(x, ROW_Y, width + 20.0, 34.0),
            LABEL_SIZE * s,
            match (lit, hovered) {
                (true, _) => color::GOLD_BRIGHT,
                (false, true) => color::TEXT,
                (false, false) => color::MUTED,
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
        if let Some(value) = counts.get(index).filter(|count| !count.is_empty()) {
            text(
                canvas,
                TextFamily::Display,
                format_args!("{value}"),
                frame.rect(x + width + COUNT_GAP, ROW_Y - 6.0, count + 12.0, 24.0),
                COUNT_SIZE * s,
                if lit { color::GOLD } else { color::QUIET },
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if lit {
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x, ROW_Y + 38.0, width, 3.0),
                radius: 1.5 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        let [x, y, width, height] = hit_area(labels, counts, index, widen);
        canvas.hit_region(token, frame.rect(x, y, width, height));
    }
}

/// The Profile screen's row of tabs, `current` lit.
pub(crate) fn row(canvas: &mut MenuCanvas, frame: &Frame, current: Tab) {
    tabs(canvas, frame, &PROFILE_LABELS, current.index(), TOKEN);
}

/// What a console page shown as one of the tabs puts at its top: the way back and the
/// player's name, as the player screen does.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Header {
    /// Where the screen returns.
    pub(crate) back: ReturnTarget,
    /// The `name` cvar.
    pub(crate) name: String,
}

impl Header {
    /// Whether it says `back` and `name`.
    pub(crate) fn is(&self, back: ReturnTarget, name: &str) -> bool {
        self.back == back && self.name == name
    }
}

/// The way back's words to `target`.
pub(crate) fn back_words(target: ReturnTarget) -> &'static str {
    match target {
        ReturnTarget::MainMenu => "Main menu",
        ReturnTarget::InGame => "Game menu",
    }
}

/// The player's name as the screen's title: "Padawan" while it is empty, as retail's
/// default name.
pub(crate) fn title(name: &str) -> &str {
    if name.trim().is_empty() {
        "Padawan"
    } else {
        name
    }
}

/// The top of a console page shown as the Profile screen's tab `current`: the way back
/// (`back_token`), the player's name and the row of tabs.
pub(crate) fn header(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    header: &Header,
    back_token: u16,
    current: Tab,
) {
    top_bar(
        canvas,
        frame,
        back_words(header.back),
        back_token,
        title(&header.name),
        None,
    );
    row(canvas, frame, current);
}

/// The top of the Collection screen on tab `current`: the way back (`back`, its words,
/// answering to `back_token`), its title and the row of tabs with how many of each
/// tab's things the player holds (`counts`, in the row's order); `widen` as for
/// [`places`].
pub(crate) fn collection_header(
    canvas: &mut MenuCanvas,
    frame: &Frame,
    back: &str,
    back_token: u16,
    current: Tab,
    (counts, widen): (&[String], f32),
) {
    top_bar(canvas, frame, back, back_token, COLLECTION, None);
    counted_tabs(
        canvas,
        frame,
        &COLLECTION_LABELS,
        counts,
        current.index(),
        TOKEN,
        widen,
    );
}

/// The Collection screen's tab whose row token is `token`, if it is one.
pub(crate) fn collection_tab_of(token: u16) -> Option<Tab> {
    token
        .checked_sub(TOKEN)
        .and_then(|index| COLLECTION_TABS.get(usize::from(index)).copied())
}

/// The Profile screen's tab of the row under `point` (window pixels) in a window of
/// `viewport`. The Collection's row is its page's own (its counts are the page's).
pub(crate) fn tab_at(viewport: [f32; 2], point: Vec2) -> Option<Tab> {
    let frame = Frame::new(viewport);
    PROFILE_TABS.into_iter().find(|tab| {
        let [x, y, width, height] = hit_area(&PROFILE_LABELS, &[], tab.index(), 1.0);
        frame.rect(x, y, width, height).contains(point)
    })
}

/// What the switch remembers: the Profile screen's tab shown last (the game menu's
/// Profile opens on it; the Collection page keeps its own) and a tab pressed and not
/// yet released.
#[derive(Debug, Default)]
pub(crate) struct Hub {
    last: Tab,
    pressed: Option<Tab>,
}

impl GpuState {
    /// The tab of the Profile or Collection screen on show, when one shows.
    pub(crate) fn profile_hub_tab(&self) -> Option<Tab> {
        self.console
            .as_ref()
            .and_then(crate::console::ViewerConsole::profile_hub_tab)
            .or_else(|| {
                self.client_menu
                    .as_ref()
                    .and_then(crate::menu::ClientMenu::player_hub_page)
                    .map(Tab::of_player_page)
            })
    }

    /// Note the Profile screen's tab on show, for the game menu's Profile to open on it
    /// again: the console's pages change some tabs on their own. Each frame.
    pub(crate) fn remember_profile_hub_tab(&mut self) {
        if let Some(tab) = self
            .profile_hub_tab()
            .filter(|tab| tab.screen() == Screen::Profile)
        {
            self.profile_hub.last = tab;
        }
    }

    /// Open the Profile screen from the game menu on `tab`, else on the tab shown
    /// last (Character the first time); it returns to the menu's Profile entry.
    pub(crate) fn open_profile_hub_from_game(&mut self, tab: Option<Tab>) {
        let tab = tab.unwrap_or(self.profile_hub.last);
        self.show_profile_hub(tab, ReturnTarget::InGame);
    }

    /// Open the Collection screen from the game menu on the tab it showed last
    /// (Medals the first time); it returns to the menu's Collection entry.
    pub(crate) fn open_collection_from_game(&mut self) {
        let tab = self
            .console
            .as_ref()
            .map_or(Tab::Medals, crate::console::ViewerConsole::collection_tab);
        self.show_profile_hub(tab, ReturnTarget::InGame);
    }

    /// The `profile`, `collection`, `achievements` and `unlockables` commands in the SJK
    /// UI: show the screen on `tab`, or close it when it shows that tab. It returns to
    /// the main page from the menus, else to the game menu.
    pub(crate) fn toggle_profile_hub(&mut self, tab: Tab) {
        if self.profile_hub_tab() == Some(tab) {
            if let Some(console) = &mut self.console {
                console.close_profile_hub_page();
            }
            self.sync_cursor_policy();
            return;
        }
        self.open_profile_hub(tab);
    }

    /// Show `tab`'s screen on it unless it shows it already (a picture dropped on the
    /// window, `sjkavatar`): it returns to the main page from the menus, else to the
    /// game menu.
    pub(crate) fn open_profile_hub(&mut self, tab: Tab) {
        if self.profile_hub_tab() == Some(tab) {
            return;
        }
        let in_menus = self
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_visible);
        let target = if in_menus {
            ReturnTarget::MainMenu
        } else {
            ReturnTarget::InGame
        };
        self.show_profile_hub(tab, target);
    }

    /// Show `tab`, closing the screen on show if it is another; `target` is where the
    /// screen returns.
    fn show_profile_hub(&mut self, tab: Tab, target: ReturnTarget) {
        use crate::ingame_menu::{Page, sjk_focus::Focus, sjk_view::Entry};
        if tab.screen() == Screen::Profile {
            self.profile_hub.last = tab;
        }
        let in_game = target == ReturnTarget::InGame;
        if in_game {
            // Escape or the way back comes to the game menu's entry of the screen.
            let entry = match tab.screen() {
                Screen::Profile => Entry::Profile,
                Screen::Collection => Entry::Collection,
            };
            self.in_game_menu.remember_return(entry.index());
        }
        if let Some(page) = tab.player_page() {
            if let Some(console) = &mut self.console {
                console.close_profile_hub_page();
            }
            if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
                // Between the player screen's own pages it only turns the page.
                if menu.player_hub_page().is_none() {
                    menu.open_player_hub(console, target);
                }
                menu.show_player_hub_page(page);
                if in_game {
                    self.game_menu = false;
                }
            }
        } else {
            if let Some(menu) = self
                .client_menu
                .as_mut()
                .filter(|menu| menu.player_hub_shown())
            {
                menu.leave_player_hub();
            }
            if in_game {
                // The game menu stays under the console's page and shows again,
                // on the screen's entry, when the page closes.
                self.game_menu = true;
                self.game_menu_page = Page::Main;
                self.game_menu_row = self.in_game_menu.return_row();
                self.in_game_menu.focus = Focus::List;
            }
            if let Some(console) = &mut self.console {
                console.open_profile_hub_page(tab, target);
            }
        }
        self.sync_cursor_policy();
    }

    /// Switch the screen on show to its tab `tab`, keeping where it returns.
    fn switch_profile_hub(&mut self, tab: Tab) {
        let Some(current) = self.profile_hub_tab() else {
            return;
        };
        if tab == current || tab.screen() != current.screen() {
            return;
        }
        let target = if current.player_page().is_some() {
            self.client_menu
                .as_ref()
                .map_or(ReturnTarget::MainMenu, |menu| menu.player_return_target())
        } else {
            self.console
                .as_ref()
                .map_or(ReturnTarget::MainMenu, |console| {
                    console.profile_hub_return()
                })
        };
        self.show_profile_hub(tab, target);
    }

    /// The keys that change the screen's tab while one shows: Ctrl+Tab (Ctrl+Shift+Tab
    /// back) from any tab, and on the player screen's pages Tab (Shift+Tab back), `]`
    /// and `[`, unless a field there is typed in. Returns whether the key was taken.
    pub(crate) fn profile_hub_key(&mut self, event: &KeyEvent) -> bool {
        let PhysicalKey::Code(key) = event.physical_key else {
            return false;
        };
        if !matches!(
            key,
            KeyCode::Tab | KeyCode::BracketLeft | KeyCode::BracketRight
        ) {
            return false;
        }
        let Some(console) = self.console.as_ref() else {
            return false;
        };
        let Some(current) = self.profile_hub_tab() else {
            return false;
        };
        let (control, shift) = (console.control_held(), console.shift_held());
        let typing = self
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::player_hub_typing);
        let forward = match key {
            KeyCode::Tab if control => !shift,
            _ if control || current.player_page().is_none() || typing => return false,
            KeyCode::Tab => !shift,
            KeyCode::BracketRight => true,
            _ => false,
        };
        if event.state == ElementState::Pressed && !event.repeat {
            self.switch_profile_hub(current.next(forward));
        }
        true
    }

    /// A press and release on one of the Profile screen's tabs switch to it; the
    /// screen under the row sees neither. Returns whether the event was taken. The
    /// Collection page answers its own row.
    pub(crate) fn profile_hub_pointer(&mut self, event: InputEvent) -> bool {
        if self
            .profile_hub_tab()
            .is_none_or(|tab| tab.screen() != Screen::Profile)
        {
            self.profile_hub.pressed = None;
            return false;
        }
        let viewport = [
            self.configuration.width as f32,
            self.configuration.height as f32,
        ];
        match event {
            InputEvent::PointerPress {
                position,
                button: PointerButton::Primary,
            } => match tab_at(viewport, position) {
                Some(tab) => {
                    self.profile_hub.pressed = Some(tab);
                    true
                }
                None => false,
            },
            InputEvent::PointerRelease { position, .. } => {
                let Some(pressed) = self.profile_hub.pressed.take() else {
                    return false;
                };
                if tab_at(viewport, position) == Some(pressed) {
                    self.switch_profile_hub(pressed);
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
impl GpuState {
    /// Ctrl+Tab without a keyboard, for the world shots.
    pub(crate) fn profile_hub_next_for_shot(&mut self) {
        if let Some(current) = self.profile_hub_tab() {
            self.switch_profile_hub(current.next(true));
        }
    }

    /// A click on `tab`, for the world shots.
    pub(crate) fn profile_hub_show_for_shot(&mut self, tab: Tab) {
        self.switch_profile_hub(tab);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_screens_tabs_wrap_within_it() {
        assert_eq!(Tab::Character.next(true), Tab::Saber);
        assert_eq!(Tab::Force.next(true), Tab::Profile);
        assert_eq!(Tab::Profile.next(true), Tab::Character);
        assert_eq!(Tab::Character.next(false), Tab::Profile);
        assert_eq!(Tab::Medals.next(false), Tab::Cards);
        assert_eq!(Tab::Nameplates.next(true), Tab::Holocrons);
        assert_eq!(Tab::Holocrons.next(true), Tab::Cards);
        assert_eq!(Tab::Cards.next(true), Tab::Medals);
        assert_eq!(Tab::Achievements.next(true), Tab::Shaders);
        for screen in [Screen::Profile, Screen::Collection] {
            for (index, tab) in screen.tabs().iter().enumerate() {
                assert_eq!(tab.index(), index);
                assert_eq!(tab.screen(), screen);
                assert_eq!(tab.label(), screen.labels()[index]);
            }
        }
        for tab in Tab::ALL {
            let player = matches!(tab, Tab::Character | Tab::Saber | Tab::Force);
            assert_eq!(tab.player_page().is_some(), player, "{tab:?}");
        }
        assert_eq!(Tab::of_player_page(2), Tab::Force);
        assert_eq!(Tab::Profile.label(), "SJK Profile");
        assert_eq!(Tab::Nameplates.label(), "Nameplates");
        assert_eq!(collection_tab_of(TOKEN + 2), Some(Tab::Shaders));
        assert_eq!(collection_tab_of(TOKEN + 5), Some(Tab::Holocrons));
        assert_eq!(collection_tab_of(TOKEN + 6), Some(Tab::Cards));
        assert_eq!(collection_tab_of(TOKEN + 7), None);
        assert_eq!(collection_tab_of(TOKEN - 1), None);
    }

    /// A larger text style widens every tab's name by as much as it is drawn wider, so
    /// none is cut ("Achievements" was at `ui_textScale 1.2`), and even the largest
    /// style keeps each row within the frame, the Collection's with its counts.
    #[test]
    fn the_tabs_make_room_for_the_players_text_style() {
        let neutral = crate::text::TextStyle::NEUTRAL;
        let larger = crate::text::TextStyle {
            scale: 1.2,
            tracking: 0.0,
        };
        let widest = crate::text::TextStyle {
            scale: crate::text::style::SCALE_RANGE.1,
            tracking: crate::text::style::TRACKING_RANGE.1,
        };
        let plain = styled_width("Achievements", neutral);
        assert!(plain > 100.0);
        assert!((styled_width("Achievements", larger) - plain * 1.2).abs() < 0.01);
        for screen in [Screen::Profile, Screen::Collection] {
            let row: f32 = screen
                .labels()
                .iter()
                .map(|label| {
                    styled_width(label, widest)
                        + COUNT_GAP
                        + sized_width("21/21", COUNT_SIZE, widest)
                        + GAP
                })
                .sum();
            assert!(ROW_X + row < 1_824.0, "{screen:?} reaches {}", ROW_X + row);
        }
    }

    /// The row the screens draw and the areas the switch hit-tests are the same, in
    /// every window, the tabs side by side within the frame, under the title; the
    /// Collection's row, with its counts, answers its page's pointer.
    #[test]
    fn the_rows_answer_where_they_are_drawn() {
        let counts: Vec<String> = ["2/4", "11/21", "1/5", "1/1", "", "12"]
            .iter()
            .map(|count| (*count).to_owned())
            .collect();
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [2_560.0, 1_080.0],
        ] {
            for screen in [Screen::Profile, Screen::Collection] {
                let mut canvas = MenuCanvas::new();
                canvas.begin_transparent(viewport);
                let frame = Frame::new(viewport);
                match screen {
                    Screen::Profile => header(
                        &mut canvas,
                        &frame,
                        &Header::default(),
                        crate::menu_widgets::BACK_TOKEN,
                        Tab::Saber,
                    ),
                    Screen::Collection => collection_header(
                        &mut canvas,
                        &frame,
                        "Main menu",
                        crate::menu_widgets::BACK_TOKEN,
                        Tab::Shaders,
                        (&counts, 1.0),
                    ),
                }
                assert!(!canvas.overflowed());
                let mut previous: Option<sjk_ui::Rect> = None;
                for tab in screen.tabs() {
                    let token = TOKEN + tab.index() as u16;
                    let area = canvas.rect_for(token).expect("a tab's area");
                    let centre = Vec2::new(area.x + area.width * 0.5, area.y + area.height * 0.5);
                    match screen {
                        Screen::Profile => {
                            assert_eq!(tab_at(viewport, centre), Some(*tab), "{viewport:?}");
                        }
                        // The Collection's page answers its own row.
                        Screen::Collection => assert_eq!(collection_tab_of(token), Some(*tab)),
                    }
                    assert!(area.right() <= viewport[0] && area.y >= 0.0);
                    if let Some(previous) = previous {
                        assert!(previous.right() <= area.x, "{tab:?} overlaps");
                    }
                    previous = Some(area);
                }
                let back = canvas
                    .rect_for(crate::menu_widgets::BACK_TOKEN)
                    .expect("the way back");
                let first = canvas.rect_for(TOKEN).expect("the first tab");
                assert!(back.bottom() <= first.y, "the row is under the title");
            }
            assert_eq!(tab_at(viewport, Vec2::new(10.0, 10.0)), None);
        }
    }

    #[test]
    fn an_empty_name_is_titled_as_retail_names_a_new_player() {
        assert_eq!(title("  "), "Padawan");
        assert_eq!(title("^1Sol"), "^1Sol");
    }
}

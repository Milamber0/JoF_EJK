//! Classic in-game menu (`ui_menuStyle classic`), after the retail
//! `ui/jamp/ingame.menu`: a bar of nine buttons along the top of the screen
//! (About, Join, Profile, Add Bot, Controls, Setup, Vote, Call Vote, Exit),
//! after SJK's own button left of About (its pop-up is [`super::sjk`]),
//! each opening a small pop-up under it, laid out like the retail
//! `ingame_*.menu` files.
//!
//! The classic menu reuses the in-game [`Page`] states and their actions:
//! the bar is the main page, each pop-up a page. This module holds what
//! differs from the shared pages: the bar's buttons, the retail rows
//! of the join, vote and exit pop-ups (with the exit confirmations), which
//! entries SJK cannot offer yet, and the pop-up geometry on the 640x480
//! canvas. Drawing is in [`super::classic_view`], activation in
//! [`super::classic_actions`].

use super::{Page, View};

/// Pointer token of bar button 0; button `i` is `BAR_TOKEN + i`. Clear of
/// the row tokens (row indices) and the vote list's scrollbar.
pub(crate) const BAR_TOKEN: u16 = 200;
/// Height of the retail bar (`menu_top_mp`, `0 0 640 32`).
pub(crate) const BAR_HEIGHT: f32 = 32.0;
/// Width of the SJK button, and of each retail button beside it.
const SJK_WIDTH: f32 = 45.0;
const RETAIL_WIDTH: f32 = 70.0;
/// Top of every pop-up (`rect x 40 ...` in the retail pop-up menus).
pub(crate) const POPUP_TOP: f32 = 40.0;

/// One button of the bar, in retail order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tab {
    /// SJK: SJK's own screens.
    Sjk,
    About,
    /// Join a team (or, in Siege, choose a class).
    Join,
    /// The Player screen; Siege shows Objectives here instead.
    Profile,
    /// Add Bot; Siege shows voice chat here instead.
    AddBot,
    /// SJK: retail's Controls and Setup as one Settings pop-up, with tabs.
    Settings,
    Vote,
    CallVote,
    Exit,
}

impl Tab {
    /// Every button, left to right.
    pub(crate) const ALL: [Self; 9] = [
        Self::Sjk,
        Self::About,
        Self::Join,
        Self::Profile,
        Self::AddBot,
        Self::Settings,
        Self::Vote,
        Self::CallVote,
        Self::Exit,
    ];

    /// The button at bar position `index`.
    pub(crate) fn at(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// Bar position of this button.
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// Retail caption; Siege swaps two buttons as retail does.
    pub(crate) fn label(self, siege: bool) -> &'static str {
        match (self, siege) {
            (Self::Sjk, _) => "SJK",
            (Self::About, _) => "About",
            (Self::Join, _) => "Join",
            (Self::Profile, false) => "Profile",
            (Self::Profile, true) => "Objectives",
            (Self::AddBot, false) => "Add Bot",
            (Self::AddBot, true) => "V Chat",
            (Self::Settings, _) => "Settings",
            (Self::Vote, _) => "Vote",
            (Self::CallVote, _) => "Call Vote",
            (Self::Exit, _) => "Exit",
        }
    }

    /// Why the button does nothing in SJK yet, if it does nothing.
    pub(crate) fn unavailable(self, siege: bool) -> Option<&'static str> {
        match (self, siege) {
            (Self::Profile, true) => Some("Not in SJK yet: no Siege objectives page"),
            (Self::AddBot, false) => Some("Not in SJK yet: choose bots in Create game"),
            (Self::AddBot, true) => Some("Not in SJK yet: no voice chat menu"),
            _ => None,
        }
    }

    /// Canvas rectangle of the button. Retail's nine are `5 + 70 i, 0, 70, 32`;
    /// SJK's narrower button takes the left end, and the retail eight (Controls
    /// and Setup are one Settings) keep retail's width after it.
    pub(crate) fn rect(self) -> [f32; 4] {
        match self.index() {
            0 => [5.0, 0.0, SJK_WIDTH, BAR_HEIGHT],
            index => [
                5.0 + SJK_WIDTH + RETAIL_WIDTH * (index - 1) as f32,
                0.0,
                RETAIL_WIDTH,
                BAR_HEIGHT,
            ],
        }
    }

    /// The bar button whose pop-up `page` is, if it has one.
    pub(crate) fn of_page(page: Page) -> Option<Self> {
        match page {
            Page::Main | Page::Shot => None,
            Page::Sjk | Page::Players | Page::ReportPlayer => Some(Self::Sjk),
            Page::About => Some(Self::About),
            Page::Team | Page::Siege => Some(Self::Join),
            Page::Vote => Some(Self::Vote),
            Page::Leave | Page::ConfirmLeave | Page::ConfirmQuit => Some(Self::Exit),
            Page::CallVote
            | Page::VoteMap
            | Page::VoteGameType
            | Page::VoteKick
            | Page::VoteClientKick
            | Page::VoteWarmup
            | Page::VoteTimeLimit
            | Page::VoteFragLimit => Some(Self::CallVote),
        }
    }
}

/// The bar button a pointer token names, if it names one.
pub(crate) fn bar_tab(token: usize) -> Option<usize> {
    token
        .checked_sub(usize::from(BAR_TOKEN))
        .filter(|index| *index < Tab::ALL.len())
}

/// Rows of the retail exit pop-up (`ingame_leave.menu`).
pub(crate) mod leave {
    pub(crate) const MAIN_MENU: usize = 0;
    pub(crate) const RESTART: usize = 1;
    pub(crate) const QUIT: usize = 2;
}

/// Rows of the vote pop-up and the exit confirmations: Yes, then No.
pub(crate) const YES: usize = 0;
pub(crate) const NO: usize = 1;

/// Note under the exit pop-up's Restart Match, which only a listen server
/// (the retail client hosting the game itself) could do.
const RESTART_NOTE: &str = "Not in SJK yet: call a vote to restart (Call Vote)";

/// Row count of `page` where the classic menu has rows of its own.
pub(crate) fn row_count(page: Page, team_game: bool) -> Option<usize> {
    match page {
        Page::Main => Some(Tab::ALL.len()),
        // Retail pop-ups have no Back button.
        Page::Sjk => Some(super::sjk::ENTRIES.len()),
        Page::Leave => Some(3),
        Page::Vote | Page::ConfirmLeave | Page::ConfirmQuit => Some(2),
        // Retail's join pop-up has no Back button.
        Page::Team if team_game => Some(4),
        Page::Team => Some(2),
        _ => None,
    }
}

/// Fill `rows` and `enabled` for `view`'s page where the classic menu has
/// its own rows; `None` leaves the page to the shared rows.
pub(super) fn prepare(view: &View<'_>, rows: &mut [String], enabled: &mut [bool]) -> Option<usize> {
    let labels: &[&str] = match view.page {
        Page::Main => {
            for (index, tab) in Tab::ALL.iter().enumerate() {
                rows[index].push_str(tab.label(view.siege));
                enabled[index] = tab.unavailable(view.siege).is_none();
            }
            return Some(Tab::ALL.len());
        }
        Page::Sjk => {
            for (index, entry) in super::sjk::ENTRIES.iter().enumerate() {
                rows[index].push_str(entry.label);
            }
            return Some(super::sjk::ENTRIES.len());
        }
        Page::Leave => &["Main Menu", "Restart Match", "Quit Program"],
        Page::Vote | Page::ConfirmLeave | Page::ConfirmQuit => &["Yes", "No"],
        Page::Team if view.team_game => &["Auto Team", "Team Red", "Team Blue", "Spectate"],
        Page::Team => &["Join Game", "Spectate"],
        _ => return None,
    };
    for (index, label) in labels.iter().enumerate() {
        rows[index].push_str(label);
        enabled[index] = !(view.page == Page::Leave && index == leave::RESTART);
    }
    Some(labels.len())
}

/// The note shown for row `row` of `page`: why a dimmed entry does nothing.
pub(crate) fn note(page: Page, row: usize, siege: bool) -> Option<&'static str> {
    match page {
        Page::Main => Tab::at(row)?.unavailable(siege),
        Page::Leave if row == leave::RESTART => Some(RESTART_NOTE),
        Page::Team => Some("You are already here"),
        _ => None,
    }
}

/// Question above the Yes and No of a confirmation pop-up.
pub(crate) fn heading(page: Page) -> Option<&'static str> {
    match page {
        Page::ConfirmLeave => Some("Go to Main Menu?"),
        Page::ConfirmQuit => Some("Quit Program?"),
        _ => None,
    }
}

/// Height of one pop-up row: the retail 30, or a list row once a page has
/// more rows than fit (the call-vote lists).
pub(crate) fn row_height(rows: usize) -> f32 {
    if rows > 8 { 18.0 } else { 30.0 }
}

/// Height of one read-only info line of the about pop-up.
pub(crate) const INFO_LINE: f32 = 20.0;

/// Canvas rectangle of `page`'s pop-up holding `rows` rows under `info`
/// read-only lines, at the retail pop-up positions.
pub(crate) fn popup(page: Page, rows: usize, info: usize) -> [f32; 4] {
    let (x, width) = match page {
        Page::Sjk => (5.0, 150.0),
        // The players' names and numbers, and the report's who and why.
        Page::Players | Page::ReportPlayer => (5.0, 460.0),
        Page::About => (10.0, 380.0),
        Page::Team => (55.0, 128.0),
        Page::Siege => (55.0, 240.0),
        Page::Vote => (430.0, 80.0),
        Page::Leave | Page::ConfirmLeave | Page::ConfirmQuit => (474.0, 156.0),
        _ => (270.0, 360.0),
    };
    let header = if heading(page).is_some() { 30.0 } else { 0.0 };
    let height = 8.0 + header + info as f32 * INFO_LINE + rows as f32 * row_height(rows);
    [x, POPUP_TOP, width, height]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingame_menu::shared_row_count;

    fn view(page: Page, team_game: bool, siege: bool) -> View<'static> {
        View {
            page,
            selected_row: 0,
            team: 3,
            team_game,
            siege,
            red_players: 0,
            blue_players: 0,
            vote_active: false,
            staff: false,
            _frame: std::marker::PhantomData,
        }
    }

    #[test]
    fn bar_follows_the_retail_order_and_fits_the_canvas() {
        let labels: Vec<_> = Tab::ALL.iter().map(|tab| tab.label(false)).collect();
        assert_eq!(
            labels,
            [
                "SJK",
                "About",
                "Join",
                "Profile",
                "Add Bot",
                "Settings",
                "Vote",
                "Call Vote",
                "Exit"
            ]
        );
        let last = Tab::Exit.rect();
        assert!(last[0] + last[2] <= 640.0);
        for (index, tab) in Tab::ALL.iter().enumerate() {
            assert_eq!(Tab::at(index), Some(*tab));
            assert_eq!(bar_tab(usize::from(BAR_TOKEN) + index), Some(index));
        }
        assert_eq!(bar_tab(0), None);
        assert_eq!(bar_tab(usize::from(BAR_TOKEN) + Tab::ALL.len()), None);
        assert_eq!(bar_tab(usize::from(u16::MAX)), None);
    }

    #[test]
    fn siege_swaps_buttons_and_dims_what_sjk_lacks() {
        assert_eq!(Tab::Profile.label(true), "Objectives");
        assert_eq!(Tab::AddBot.label(true), "V Chat");
        for siege in [false, true] {
            let mut rows: [String; 24] = std::array::from_fn(|_| String::new());
            let mut enabled = [true; 24];
            let count = prepare(&view(Page::Main, false, siege), &mut rows, &mut enabled);
            assert_eq!(count, Some(Tab::ALL.len()));
            for tab in Tab::ALL {
                assert_eq!(
                    enabled[tab.index()],
                    tab.unavailable(siege).is_none(),
                    "{tab:?}"
                );
                assert_eq!(note(Page::Main, tab.index(), siege), tab.unavailable(siege));
            }
        }
        assert!(Tab::AddBot.unavailable(false).is_some());
        assert!(Tab::Profile.unavailable(false).is_none());
    }

    #[test]
    fn prepared_rows_match_row_counts() {
        for (page, team_game) in [
            (Page::Main, false),
            (Page::Sjk, false),
            (Page::Leave, false),
            (Page::Vote, false),
            (Page::ConfirmLeave, false),
            (Page::ConfirmQuit, false),
            (Page::Team, true),
            (Page::Team, false),
        ] {
            let mut rows: [String; 24] = std::array::from_fn(|_| String::new());
            let mut enabled = [true; 24];
            let count = prepare(&view(page, team_game, false), &mut rows, &mut enabled);
            assert_eq!(count, row_count(page, team_game), "{page:?}");
        }
        // Pages the classic menu shares keep the shared rows.
        assert_eq!(row_count(Page::About, false), None);
        assert_eq!(shared_row_count(Page::About), 1);
    }

    #[test]
    fn exit_rows_keep_the_retail_order() {
        let mut rows: [String; 24] = std::array::from_fn(|_| String::new());
        let mut enabled = [true; 24];
        prepare(&view(Page::Leave, false, false), &mut rows, &mut enabled);
        assert_eq!(rows[leave::MAIN_MENU], "Main Menu");
        assert_eq!(rows[leave::RESTART], "Restart Match");
        assert_eq!(rows[leave::QUIT], "Quit Program");
        assert!(enabled[leave::MAIN_MENU] && enabled[leave::QUIT]);
        assert!(!enabled[leave::RESTART]);
        assert!(note(Page::Leave, leave::RESTART, false).is_some());
        assert!(heading(Page::ConfirmLeave).is_some() && heading(Page::ConfirmQuit).is_some());
    }

    #[test]
    fn every_popup_sits_under_its_button_on_the_canvas() {
        for page in [
            Page::Sjk,
            Page::About,
            Page::Team,
            Page::Siege,
            Page::Vote,
            Page::Leave,
            Page::ConfirmLeave,
            Page::ConfirmQuit,
            Page::CallVote,
            Page::VoteMap,
            Page::Players,
            Page::ReportPlayer,
        ] {
            let tab = Tab::of_page(page).expect("pop-up page has a button");
            let [x, y, width, height] = popup(page, 18, 0);
            assert!(y >= BAR_HEIGHT && y + height <= 480.0, "{page:?}");
            assert!(x >= 0.0 && x + width <= 640.0, "{page:?}");
            let [bar_x, _, bar_width, _] = tab.rect();
            // The pop-up overlaps its button's column.
            assert!(x < bar_x + bar_width && bar_x < x + width, "{page:?}");
        }
        assert_eq!(Tab::of_page(Page::Main), None);
    }
}

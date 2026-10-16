//! The Staff page: for a player the hub's operator made staff, the SJK team's tools
//! in the game (`docs/identity.md`, "Staff"): find a player, verify them, give or take
//! back their medals, unlock or relock their unlockables, give or take back their
//! holocrons, clear their achievements, list their keys and unlink one, merge
//! another player into them (a player who reset their key), ask them for their game's
//! logs and read the logs players sent and their crash reports. Every action is a
//! request signed by the player's own key (`sjk_identity::StaffRequest`,
//! `sjk_identity::StaffQuery`); the hub refuses it from any other key, and the page
//! only opens for a key whose profile says staff.
//!
//! Opened by the Profile page's Staff tools button or the `staff` command. Like the
//! Profile page it lives in the console and has the SJK UI's look ([`view`]). Tab
//! and Shift+Tab walk every control; Enter or Space works the one focused; the
//! search pill, the note and the merge field take typing; Up and Down move through the
//! players found; Escape goes back (or lets go of a merge being confirmed or picked).

use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use sjk_identity::{Profile, StaffQuery, StaffRequest, StaffState};
use sjk_ui::{InputEvent, UiEventKind};
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "staff_panel_view.rs"]
mod view;

const SEARCH_TOKEN: u16 = 1_100;
const ME_TOKEN: u16 = 1_101;
const RECENT_TOKEN: u16 = 1_102;
const NOTE_TOKEN: u16 = 1_103;
const CLEAR_ALL_TOKEN: u16 = 1_104;
const PICTURE_DOWN_TOKEN: u16 = 1_105;
/// Players found, one token each.
const PLAYER_BASE: u16 = 1_110;
const PLAYERS_SHOWN: usize = 10;
/// Give and Take back, one token per medal of the catalogue.
const GIVE_BASE: u16 = 1_150;
const TAKE_BASE: u16 = 1_160;
/// Clear, one token per achievement of the catalogue.
const CLEAR_BASE: u16 = 1_170;
/// Unlock and Relock, one token per unlockable of the catalogue.
/// The unlockables' pager: previous page, next page (the list outgrew the page).
const UNLOCK_PREV_TOKEN: u16 = 7_590;
const UNLOCK_NEXT_TOKEN: u16 = 7_591;
const UNLOCK_BASE: u16 = 7_600;
const RELOCK_BASE: u16 = 7_900;
/// The holocron tier chips (one token per tier), Give holocron, and Remove (one token
/// per holocron row).
const HOLOCRON_TIER_BASE: u16 = 1_300;
const HOLOCRON_GIVE_TOKEN: u16 = 1_310;
const HOLOCRON_REMOVE_BASE: u16 = 1_320;
/// The chosen player's recent holocrons listed with Remove.
const HOLOCRONS_SHOWN: usize = 4;
/// Verify or Unverify the chosen player.
const VERIFY_TOKEN: u16 = 1_400;
/// The right column's two views: the achievements, or the keys and merging.
const ACHIEVEMENTS_TAB_TOKEN: u16 = 1_401;
const KEYS_TAB_TOKEN: u16 = 1_402;
/// The key id of the player to merge away, Pick in list, Merge, and the confirmation's
/// two buttons.
const MERGE_FIELD_TOKEN: u16 = 1_403;
const MERGE_PICK_TOKEN: u16 = 1_404;
const MERGE_TOKEN: u16 = 1_405;
const MERGE_CONFIRM_TOKEN: u16 = 1_406;
const MERGE_CANCEL_TOKEN: u16 = 1_407;
/// Unlink, one token per linked key listed.
const UNLINK_BASE: u16 = 1_410;
const LINKED_SHOWN: usize = 6;
/// The right column's Logs and Crashes views.
const LOGS_TAB_TOKEN: u16 = 1_420;
const CRASHES_TAB_TOKEN: u16 = 1_421;
/// Request logs of the chosen player, and the list read again.
const REQUEST_LOGS_TOKEN: u16 = 1_422;
const LOGS_REFRESH_TOKEN: u16 = 1_423;
/// Open, one token per log request listed (only sent ones have it).
const LOG_OPEN_BASE: u16 = 1_430;
const LOG_ROWS_SHOWN: usize = 12;
/// The crash groups read again, and back from one group's reports to the groups.
const CRASHES_REFRESH_TOKEN: u16 = 1_450;
const CRASHES_BACK_TOKEN: u16 = 1_451;
/// A crash group, one token each, and Open, one token per report of a group.
const CRASH_GROUP_BASE: u16 = 1_460;
const CRASH_GROUPS_SHOWN: usize = 9;
const CRASH_OPEN_BASE: u16 = 1_480;
const CRASH_ROWS_SHOWN: usize = 12;
/// Longest text the merge field takes (a pasted key id with spaces round it).
const MERGE_FIELD_MAX: usize = 40;
/// Longest search, as the hub takes it.
const QUERY_MAX: usize = 64;
/// Longest note with a medal or an unlock, as the hub takes it.
const NOTE_MAX: usize = 200;
/// How long Clear all and Unlink wait for their second press.
const CONFIRM_FOR: Duration = Duration::from_secs(3);

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Send this to the hub.
    Request(StaffRequest),
    /// And this, about logs and crash reports.
    Query(StaffQuery),
}

/// What the page knows this frame.
pub(crate) struct Inputs<'a> {
    /// The player's own profile, once the hub sent it.
    pub(crate) me: Option<&'a Profile>,
    pub(crate) staff: &'a StaffState,
}

/// What the right column shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RightView {
    #[default]
    Achievements,
    Keys,
    /// The log requests, with Request logs for the chosen player.
    Logs,
    /// The crash reports of every player: the groups, or one group's reports.
    Crashes,
}

/// A merge waiting for its confirmation: who is kept and who goes.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingMerge {
    kept: String,
    from: String,
}

/// Why the merge field's key cannot be merged into the chosen player yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MergeProblem {
    /// Nothing typed or picked.
    Empty,
    /// Not 16 hex digits.
    NotKey,
    /// The chosen player's own key (main or linked).
    Same,
    /// A staff player, which the hub never merges away.
    Staff,
}

impl MergeProblem {
    fn words(self) -> &'static str {
        match self {
            Self::Empty => "Type or paste the other player's key id, or pick them in the list",
            Self::NotKey => "A key id is 16 hex digits",
            Self::Same => "That key is already this player's",
            Self::Staff => "A staff player cannot be merged away",
        }
    }
}

/// The merge field's text as a key id: trimmed and lower case.
fn merge_key(text: &str) -> String {
    text.trim().to_ascii_lowercase()
}

/// What the last frame showed, for keys and clicks.
#[derive(Debug, Default)]
struct Shown {
    /// The players' keys in the list, in order.
    players: Vec<String>,
    me: Option<String>,
    /// The chosen player's key, how many of each medal they hold and which
    /// unlockables.
    target: Option<String>,
    medals: [u32; crate::medals::Medal::COUNT],
    /// The chosen player has a picture.
    picture: bool,
    /// Which unlockables of the catalogue they hold, in its order.
    unlocks: Vec<bool>,
    /// How many unlockables a page of the list shows.
    unlock_per_page: usize,
    /// The numbers of the holocrons listed with Remove, newest first, and how many.
    holocrons: [u64; HOLOCRONS_SHOWN],
    holocron_rows: usize,
    /// The chosen player is verified.
    verified: bool,
    /// The chosen player's linked keys listed with Unlink.
    linked: Vec<String>,
    /// Why the merge field's key cannot be merged in, or `None` when it can.
    merge_problem: Option<MergeProblem>,
    /// The log requests listed, by number, and whether each has logs to open.
    log_rows: Vec<(i64, bool)>,
    /// The crash groups listed, by signature, and one group's reports, by number.
    crash_groups: Vec<String>,
    crash_rows: Vec<i64>,
}

pub(crate) struct Panel {
    open: bool,
    owns_console: bool,
    ui: MenuCanvas,
    query: String,
    note: String,
    /// The chosen player's key; the player's own until another is chosen.
    selected: Option<String>,
    /// The holocron tier Give holocron gives (its index in the catalogue).
    holocron_tier: usize,
    /// The unlockables list's page, from 0.
    unlock_page: usize,
    /// The control the keyboard is on, by its token.
    focus: u16,
    /// The controls in the order Tab visits them, laid out by the last frame.
    order: Vec<u16>,
    /// The button pressed once (Clear all, an Unlink) waiting for its second press,
    /// and when.
    confirm: Option<(u16, Instant)>,
    /// What the right column shows.
    right: RightView,
    /// The merge field: the key id of the player to merge away.
    merge_from: String,
    /// The next player chosen in the list fills the merge field instead.
    picking: bool,
    /// A merge waiting for Merge for good or Cancel.
    pending_merge: Option<PendingMerge>,
    /// The crash group whose reports the Crashes view lists, or none for the groups.
    crash_group: Option<String>,
    shown: Shown,
    epoch: Instant,
    /// What a world shot shows in place of the live profile and answers.
    #[cfg(test)]
    pub(crate) preview: Option<(Profile, StaffState)>,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

/// Add `text` to `field` up to `limit` characters, control characters left out;
/// whether it changed.
fn type_into(field: &mut String, text: &str, limit: usize) -> bool {
    let mut changed = false;
    for c in text.chars().filter(|c| !c.is_control()) {
        if field.chars().count() >= limit {
            break;
        }
        field.push(c);
        changed = true;
    }
    changed
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            owns_console: false,
            ui: MenuCanvas::with_capacities(260, 160, 1_200),
            query: String::new(),
            note: String::new(),
            selected: None,
            holocron_tier: 0,
            unlock_page: 0,
            focus: SEARCH_TOKEN,
            order: Vec::with_capacity(64),
            confirm: None,
            right: RightView::Achievements,
            merge_from: String::new(),
            picking: false,
            pending_merge: None,
            crash_group: None,
            shown: Shown::default(),
            epoch: Instant::now(),
            #[cfg(test)]
            preview: None,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page; `owns_console` when the console was closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        self.focus = SEARCH_TOKEN;
        self.confirm = None;
        self.picking = false;
        self.pending_merge = None;
    }

    /// Closing the page closes the console too.
    pub(crate) fn own_console(&mut self) {
        self.owns_console = true;
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        owned
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// The chosen player's profile: from the players found, or the player's own.
    fn target<'a>(&self, inputs: &Inputs<'a>) -> Option<&'a Profile> {
        let me = inputs.me;
        let key = self
            .selected
            .as_deref()
            .or(me.map(|me| me.key_id.as_str()))?;
        inputs
            .staff
            .players
            .iter()
            .find(|player| player.key_id == key)
            .or(me.filter(|me| me.key_id == key))
    }

    /// Whether the button `token` is waiting for its second press.
    fn confirming(&self, token: u16) -> bool {
        self.confirm
            .is_some_and(|(pressed, at)| pressed == token && at.elapsed() < CONFIRM_FOR)
    }

    /// A press of a button that needs two: whether this one is the second.
    fn second_press(&mut self, token: u16) -> bool {
        if self.confirming(token) {
            self.confirm = None;
            true
        } else {
            self.confirm = Some((token, Instant::now()));
            false
        }
    }

    /// Why `from`, the merge field's key, cannot be merged into `target` yet, knowing
    /// the players found; `None` when it can.
    fn merge_problem(
        from: &str,
        target: Option<&Profile>,
        players: &[Profile],
    ) -> Option<MergeProblem> {
        if from.is_empty() {
            return Some(MergeProblem::Empty);
        }
        if from.len() != 16 || !from.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(MergeProblem::NotKey);
        }
        if target.is_some_and(|target| target.has_key(from)) {
            return Some(MergeProblem::Same);
        }
        if players
            .iter()
            .any(|player| player.has_key(from) && player.staff)
        {
            return Some(MergeProblem::Staff);
        }
        None
    }

    /// What Enter (or a click) on `token` does.
    fn activate(&mut self, token: u16) -> PanelAction {
        self.focus = token;
        if self.confirm.is_some_and(|(pressed, _)| pressed != token) {
            self.confirm = None;
        }
        if token != MERGE_PICK_TOKEN
            && !(PLAYER_BASE..PLAYER_BASE + PLAYERS_SHOWN as u16).contains(&token)
        {
            self.picking = false;
        }
        let target = self.shown.target.clone();
        match token {
            SEARCH_TOKEN => {
                PanelAction::Request(StaffRequest::Search(self.query.trim().to_owned()))
            }
            ME_TOKEN => {
                self.selected.clone_from(&self.shown.me);
                PanelAction::None
            }
            VERIFY_TOKEN => match target {
                Some(key_id) => PanelAction::Request(StaffRequest::Verify {
                    key_id,
                    verified: !self.shown.verified,
                }),
                None => PanelAction::None,
            },
            ACHIEVEMENTS_TAB_TOKEN => {
                self.right = RightView::Achievements;
                PanelAction::None
            }
            KEYS_TAB_TOKEN => {
                self.right = RightView::Keys;
                PanelAction::None
            }
            // The Logs and Crashes views read their lists from the hub as they open.
            LOGS_TAB_TOKEN | LOGS_REFRESH_TOKEN => {
                self.right = RightView::Logs;
                PanelAction::Query(StaffQuery::LogRequests)
            }
            CRASHES_TAB_TOKEN | CRASHES_REFRESH_TOKEN | CRASHES_BACK_TOKEN => {
                self.right = RightView::Crashes;
                self.crash_group = None;
                PanelAction::Query(StaffQuery::CrashGroups)
            }
            REQUEST_LOGS_TOKEN => match target {
                Some(key_id) => PanelAction::Query(StaffQuery::RequestLogs {
                    key_id,
                    note: self.note.trim().to_owned(),
                }),
                None => PanelAction::None,
            },
            token if (LOG_OPEN_BASE..LOG_OPEN_BASE + LOG_ROWS_SHOWN as u16).contains(&token) => {
                match self.shown.log_rows.get(usize::from(token - LOG_OPEN_BASE)) {
                    Some(&(id, true)) => PanelAction::Query(StaffQuery::Log(id)),
                    _ => PanelAction::None,
                }
            }
            token
                if (CRASH_GROUP_BASE..CRASH_GROUP_BASE + CRASH_GROUPS_SHOWN as u16)
                    .contains(&token) =>
            {
                match self
                    .shown
                    .crash_groups
                    .get(usize::from(token - CRASH_GROUP_BASE))
                {
                    Some(signature) => {
                        self.crash_group = Some(signature.clone());
                        self.focus = CRASHES_BACK_TOKEN;
                        PanelAction::Query(StaffQuery::Crashes(signature.clone()))
                    }
                    None => PanelAction::None,
                }
            }
            token
                if (CRASH_OPEN_BASE..CRASH_OPEN_BASE + CRASH_ROWS_SHOWN as u16)
                    .contains(&token) =>
            {
                match self
                    .shown
                    .crash_rows
                    .get(usize::from(token - CRASH_OPEN_BASE))
                {
                    Some(&id) => PanelAction::Query(StaffQuery::Crash(id)),
                    None => PanelAction::None,
                }
            }
            MERGE_FIELD_TOKEN => PanelAction::None,
            MERGE_PICK_TOKEN => {
                self.picking = !self.picking;
                PanelAction::None
            }
            MERGE_TOKEN => {
                if let Some(kept) = target
                    && self.shown.merge_problem.is_none()
                {
                    self.pending_merge = Some(PendingMerge {
                        kept,
                        from: merge_key(&self.merge_from),
                    });
                    // Cancel first, so a hurried Enter does not merge.
                    self.focus = MERGE_CANCEL_TOKEN;
                }
                PanelAction::None
            }
            MERGE_CANCEL_TOKEN => {
                self.pending_merge = None;
                self.focus = MERGE_TOKEN;
                PanelAction::None
            }
            MERGE_CONFIRM_TOKEN => match self.pending_merge.take() {
                // Only the merge confirmed for the player still chosen.
                Some(merge) if target.as_deref() == Some(merge.kept.as_str()) => {
                    self.merge_from.clear();
                    self.focus = MERGE_FIELD_TOKEN;
                    PanelAction::Request(StaffRequest::Merge {
                        key_id: merge.kept,
                        from: merge.from,
                    })
                }
                _ => PanelAction::None,
            },
            token if (UNLINK_BASE..UNLINK_BASE + LINKED_SHOWN as u16).contains(&token) => {
                let Some(key_id) = self
                    .shown
                    .linked
                    .get(usize::from(token - UNLINK_BASE))
                    .cloned()
                else {
                    return PanelAction::None;
                };
                if self.second_press(token) {
                    PanelAction::Request(StaffRequest::Unlink { key_id })
                } else {
                    PanelAction::None
                }
            }
            RECENT_TOKEN => {
                self.query.clear();
                PanelAction::Request(StaffRequest::Search(String::new()))
            }
            NOTE_TOKEN => PanelAction::None,
            PICTURE_DOWN_TOKEN => match target {
                Some(key_id) if self.shown.picture => {
                    PanelAction::Request(StaffRequest::AvatarRemove { key_id })
                }
                _ => PanelAction::None,
            },
            CLEAR_ALL_TOKEN => {
                let Some(key_id) = target else {
                    return PanelAction::None;
                };
                if self.second_press(CLEAR_ALL_TOKEN) {
                    PanelAction::Request(StaffRequest::ClearAchievements {
                        key_id,
                        id: String::new(),
                    })
                } else {
                    PanelAction::None
                }
            }
            token if (PLAYER_BASE..PLAYER_BASE + PLAYERS_SHOWN as u16).contains(&token) => {
                if let Some(key) = self.shown.players.get(usize::from(token - PLAYER_BASE)) {
                    if self.picking {
                        // Picking the player to merge away: the chosen one stays.
                        self.picking = false;
                        self.merge_from.clone_from(key);
                    } else {
                        self.selected = Some(key.clone());
                    }
                }
                PanelAction::None
            }
            token
                if (GIVE_BASE..GIVE_BASE + crate::medals::Medal::COUNT as u16).contains(&token) =>
            {
                let medal = crate::medals::Medal::ALL[usize::from(token - GIVE_BASE)];
                let held = self.shown.medals[medal.index()];
                match target {
                    Some(key_id) if held == 0 || medal.repeatable() => {
                        PanelAction::Request(StaffRequest::Award {
                            key_id,
                            medal: medal.id().to_owned(),
                            note: self.note.trim().to_owned(),
                        })
                    }
                    _ => PanelAction::None,
                }
            }
            token
                if (TAKE_BASE..TAKE_BASE + crate::medals::Medal::COUNT as u16).contains(&token) =>
            {
                let medal = crate::medals::Medal::ALL[usize::from(token - TAKE_BASE)];
                match target {
                    Some(key_id) if self.shown.medals[medal.index()] > 0 => {
                        PanelAction::Request(StaffRequest::Unaward {
                            key_id,
                            medal: medal.id().to_owned(),
                        })
                    }
                    _ => PanelAction::None,
                }
            }
            UNLOCK_PREV_TOKEN => {
                self.unlock_page = self.unlock_page.saturating_sub(1);
                PanelAction::None
            }
            UNLOCK_NEXT_TOKEN => {
                let per_page = self.shown.unlock_per_page.max(1);
                let last = crate::unlockables::ALL.len().div_ceil(per_page) - 1;
                self.unlock_page = (self.unlock_page + 1).min(last);
                PanelAction::None
            }
            token
                if (UNLOCK_BASE..UNLOCK_BASE + crate::unlockables::ALL.len() as u16)
                    .contains(&token) =>
            {
                let index = usize::from(token - UNLOCK_BASE);
                match target {
                    Some(key_id) if !self.shown.unlocks.get(index).copied().unwrap_or(false) => {
                        PanelAction::Request(StaffRequest::Unlock {
                            key_id,
                            unlock: crate::unlockables::ALL[index].id.to_owned(),
                            note: self.note.trim().to_owned(),
                        })
                    }
                    _ => PanelAction::None,
                }
            }
            token
                if (RELOCK_BASE..RELOCK_BASE + crate::unlockables::ALL.len() as u16)
                    .contains(&token) =>
            {
                let index = usize::from(token - RELOCK_BASE);
                match target {
                    Some(key_id) if self.shown.unlocks.get(index).copied().unwrap_or(false) => {
                        PanelAction::Request(StaffRequest::Relock {
                            key_id,
                            unlock: crate::unlockables::ALL[index].id.to_owned(),
                        })
                    }
                    _ => PanelAction::None,
                }
            }
            token
                if (HOLOCRON_TIER_BASE..HOLOCRON_TIER_BASE + crate::holocrons::COUNT as u16)
                    .contains(&token) =>
            {
                self.holocron_tier = usize::from(token - HOLOCRON_TIER_BASE);
                PanelAction::None
            }
            HOLOCRON_GIVE_TOKEN => match target {
                Some(key_id) => PanelAction::Request(StaffRequest::HolocronGive {
                    key_id,
                    tier: crate::holocrons::TIERS[self.holocron_tier].id.to_owned(),
                    note: self.note.trim().to_owned(),
                }),
                None => PanelAction::None,
            },
            token
                if (HOLOCRON_REMOVE_BASE..HOLOCRON_REMOVE_BASE + HOLOCRONS_SHOWN as u16)
                    .contains(&token) =>
            {
                let row = usize::from(token - HOLOCRON_REMOVE_BASE);
                match target {
                    Some(key_id) if row < self.shown.holocron_rows => {
                        PanelAction::Request(StaffRequest::HolocronRemove {
                            key_id,
                            id: self.shown.holocrons[row],
                        })
                    }
                    _ => PanelAction::None,
                }
            }
            token
                if (CLEAR_BASE..CLEAR_BASE + crate::achievements::ALL.len() as u16)
                    .contains(&token) =>
            {
                let kind = &crate::achievements::ALL[usize::from(token - CLEAR_BASE)];
                match target {
                    Some(key_id) => PanelAction::Request(StaffRequest::ClearAchievements {
                        key_id,
                        id: kind.id.to_owned(),
                    }),
                    None => PanelAction::None,
                }
            }
            _ => PanelAction::None,
        }
    }

    /// Choose the player with `key_id`, for a world shot.
    #[cfg(test)]
    pub(crate) fn choose_for_shot(&mut self, key_id: &str) {
        self.selected = Some(key_id.to_owned());
    }

    /// Show the Keys and merge view with `from` in the merge field, and its confirmation
    /// for the chosen player when `confirm`, for a world shot.
    #[cfg(test)]
    pub(crate) fn merge_for_shot(&mut self, from: &str, confirm: bool) {
        self.right = RightView::Keys;
        self.merge_from = from.to_owned();
        self.pending_merge = None;
        if confirm && let Some(kept) = self.shown.target.clone() {
            self.pending_merge = Some(PendingMerge {
                kept,
                from: merge_key(from),
            });
            self.focus = MERGE_CANCEL_TOKEN;
        }
    }

    /// Show the Logs view, or the Crashes view (one kind's reports with `group`), for a
    /// world shot.
    #[cfg(test)]
    pub(crate) fn view_for_shot(&mut self, logs: bool, group: Option<&str>) {
        self.right = if logs {
            RightView::Logs
        } else {
            RightView::Crashes
        };
        self.crash_group = group.map(str::to_owned);
    }

    /// Move the keyboard `by` controls along the order the last frame laid out.
    fn step(&mut self, forward: bool) {
        if self.order.is_empty() {
            return;
        }
        let at = self.order.iter().position(|token| *token == self.focus);
        let next = match (at, forward) {
            (None, _) => 0,
            (Some(at), true) => (at + 1) % self.order.len(),
            (Some(at), false) => (at + self.order.len() - 1) % self.order.len(),
        };
        self.focus = self.order[next];
    }

    /// A key, with Shift and Ctrl held or not.
    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        shift: bool,
        control: bool,
    ) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        let typing = matches!(self.focus, SEARCH_TOKEN | NOTE_TOKEN | MERGE_FIELD_TOKEN);
        let in_list = (PLAYER_BASE..PLAYER_BASE + PLAYERS_SHOWN as u16).contains(&self.focus);
        if typing && !event.repeat {
            let field = match self.focus {
                SEARCH_TOKEN => &mut self.query,
                MERGE_FIELD_TOKEN => &mut self.merge_from,
                _ => &mut self.note,
            };
            if crate::text_entry::before_edit(field, key, event.text.as_deref(), shift, control) {
                return PanelAction::None;
            }
        }
        match key {
            // A merge being confirmed or picked is let go first.
            KeyCode::Escape if self.pending_merge.is_some() => {
                return self.activate(MERGE_CANCEL_TOKEN);
            }
            KeyCode::Escape if self.picking => self.picking = false,
            KeyCode::Escape => return PanelAction::Close,
            KeyCode::Tab => self.step(!shift),
            KeyCode::ArrowDown | KeyCode::ArrowUp if in_list => {
                let count = self.shown.players.len().min(PLAYERS_SHOWN);
                if count > 0 {
                    let at = usize::from(self.focus - PLAYER_BASE);
                    let next = if key == KeyCode::ArrowDown {
                        (at + 1) % count
                    } else {
                        (at + count - 1) % count
                    };
                    return self.activate(PLAYER_BASE + next as u16);
                }
            }
            KeyCode::ArrowDown if !typing => self.step(true),
            KeyCode::ArrowUp if !typing => self.step(false),
            KeyCode::Enter | KeyCode::NumpadEnter => return self.activate(self.focus),
            KeyCode::Space if !typing => return self.activate(self.focus),
            KeyCode::Backspace if typing => {
                let field = match self.focus {
                    SEARCH_TOKEN => &mut self.query,
                    MERGE_FIELD_TOKEN => &mut self.merge_from,
                    _ => &mut self.note,
                };
                field.pop();
            }
            _ if typing && !event.repeat => {
                let pasted;
                let text = match event.text.as_deref() {
                    // Ctrl+V (Windows hands the letter without Ctrl) or Shift+Insert.
                    text if crate::sjk_chat_field::is_paste(key, text, shift, control) => {
                        pasted = crate::console::clipboard::paste().unwrap_or_default();
                        pasted.as_str()
                    }
                    Some(text) => text,
                    None => return PanelAction::None,
                };
                match self.focus {
                    SEARCH_TOKEN => type_into(&mut self.query, text, QUERY_MAX),
                    MERGE_FIELD_TOKEN => type_into(&mut self.merge_from, text, MERGE_FIELD_MAX),
                    _ => type_into(&mut self.note, text, NOTE_MAX),
                };
            }
            _ => {}
        }
        PanelAction::None
    }

    /// A pointer event.
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            // A click in a field puts the keyboard there; it searches on Enter.
            Some(token @ (SEARCH_TOKEN | NOTE_TOKEN | MERGE_FIELD_TOKEN)) => {
                self.focus = token;
                PanelAction::None
            }
            Some(token) => self.activate(token),
            None => PanelAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::{Achievement, Medal};

    pub(super) fn profile(key_id: &str, name: &str) -> Profile {
        Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: key_id.to_owned(),
            key: String::new(),
            name: name.to_owned(),
            bio: String::new(),
            verified: false,
            staff: false,
            created: 1_791_250_000,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: sjk_identity::HolocronCounts::default(),
            holocrons: Vec::new(),
        }
    }

    fn me() -> Profile {
        Profile {
            staff: true,
            medals: vec![Medal {
                id: "early_tester".into(),
                count: 1,
                awarded: 1,
                note: String::new(),
            }],
            achievements: vec![Achievement {
                id: "first_blood".into(),
                progress: 1,
                goal: 1,
                unlocked: 5,
            }],
            ..profile("aaaaaaaaaaaaaaaa", "^1Sol")
        }
    }

    /// The panel after one frame with `players` found, so its controls are known.
    fn drawn(players: Vec<Profile>) -> (Panel, Profile, StaffState) {
        let me = me();
        let staff = StaffState {
            players,
            ..StaffState::default()
        };
        let mut panel = Panel::new();
        panel.open(true);
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        panel.build(
            &Inputs {
                me: Some(&me),
                staff: &staff,
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
        (panel, me, staff)
    }

    #[test]
    fn logs_and_crashes_are_asked_for_and_opened() {
        use sjk_identity::logs::{CrashGroup, CrashSummary, LogRequestRow};
        let (mut panel, me, _) = drawn(Vec::new());
        // Opening the views reads their lists.
        assert_eq!(
            panel.activate(LOGS_TAB_TOKEN),
            PanelAction::Query(StaffQuery::LogRequests)
        );
        panel.note = " the freeze ".into();
        assert_eq!(
            panel.activate(REQUEST_LOGS_TOKEN),
            PanelAction::Query(StaffQuery::RequestLogs {
                key_id: me.key_id.clone(),
                note: "the freeze".into(),
            })
        );
        let row = |id, key_id: &str, state: &str| LogRequestRow {
            id,
            key_id: key_id.into(),
            state: state.into(),
            ..LogRequestRow::default()
        };
        let staff = StaffState {
            log_requests: vec![
                row(9, &me.key_id, "open"),
                row(8, "bbbbbbbbbbbbbbbb", "sent"),
            ],
            crash_groups: vec![CrashGroup {
                signature: "00ff00ff00ff00ff".into(),
                ..CrashGroup::default()
            }],
            crashes: vec![CrashSummary {
                id: 77,
                ..CrashSummary::default()
            }],
            crashes_of: "00ff00ff00ff00ff".into(),
            ..StaffState::default()
        };
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let inputs = Inputs {
            me: Some(&me),
            staff: &staff,
        };
        panel.build(&inputs, &fonts.font, [1920.0, 1080.0]);
        // One request open for this player: no second one; only sent logs open.
        assert!(!panel.order.contains(&REQUEST_LOGS_TOKEN));
        assert!(!panel.order.contains(&LOG_OPEN_BASE));
        assert_eq!(
            panel.activate(LOG_OPEN_BASE + 1),
            PanelAction::Query(StaffQuery::Log(8))
        );
        assert_eq!(panel.activate(LOG_OPEN_BASE), PanelAction::None);
        // A kind of crash lists its reports, whose Open reads one whole.
        assert_eq!(
            panel.activate(CRASHES_TAB_TOKEN),
            PanelAction::Query(StaffQuery::CrashGroups)
        );
        panel.build(&inputs, &fonts.font, [1920.0, 1080.0]);
        assert_eq!(
            panel.activate(CRASH_GROUP_BASE),
            PanelAction::Query(StaffQuery::Crashes("00ff00ff00ff00ff".into()))
        );
        panel.build(&inputs, &fonts.font, [1920.0, 1080.0]);
        assert!(panel.order.contains(&CRASHES_BACK_TOKEN));
        assert_eq!(
            panel.activate(CRASH_OPEN_BASE),
            PanelAction::Query(StaffQuery::Crash(77))
        );
        assert_eq!(
            panel.activate(CRASHES_BACK_TOKEN),
            PanelAction::Query(StaffQuery::CrashGroups)
        );
        assert_eq!(panel.crash_group, None);
    }

    #[test]
    fn the_player_is_their_own_target_until_another_is_chosen() {
        let (mut panel, me, staff) = drawn(vec![profile("bbbbbbbbbbbbbbbb", "Fox")]);
        let inputs = Inputs {
            me: Some(&me),
            staff: &staff,
        };
        assert_eq!(panel.target(&inputs).unwrap().key_id, me.key_id);
        assert_eq!(panel.activate(PLAYER_BASE), PanelAction::None);
        assert_eq!(panel.target(&inputs).unwrap().name, "Fox");
        let _ = panel.activate(ME_TOKEN);
        assert_eq!(panel.target(&inputs).unwrap().key_id, me.key_id);
    }

    #[test]
    fn a_picture_is_taken_down_only_when_there_is_one() {
        let (mut panel, me, _) = drawn(Vec::new());
        assert!(panel.order.contains(&PICTURE_DOWN_TOKEN));
        assert_eq!(panel.activate(PICTURE_DOWN_TOKEN), PanelAction::None);
        let pictured = Profile {
            avatar: "0123456789abcdef".into(),
            ..profile("bbbbbbbbbbbbbbbb", "Fox")
        };
        let (mut panel, _, _) = drawn(vec![pictured]);
        assert_eq!(panel.activate(PLAYER_BASE), PanelAction::None);
        // The next frame shows the chosen player.
        let staff = StaffState {
            players: vec![Profile {
                avatar: "0123456789abcdef".into(),
                ..profile("bbbbbbbbbbbbbbbb", "Fox")
            }],
            ..StaffState::default()
        };
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        panel.build(
            &Inputs {
                me: Some(&me),
                staff: &staff,
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
        assert_eq!(
            panel.activate(PICTURE_DOWN_TOKEN),
            PanelAction::Request(StaffRequest::AvatarRemove {
                key_id: "bbbbbbbbbbbbbbbb".into()
            })
        );
    }

    #[test]
    fn medals_are_given_and_taken_back_as_the_catalogue_allows() {
        let (mut panel, me, _) = drawn(Vec::new());
        panel.note = "  found the fog bug ".into();
        // Early Tester is held and not repeatable: no second one.
        assert_eq!(panel.activate(GIVE_BASE), PanelAction::None);
        assert_eq!(
            panel.activate(TAKE_BASE),
            PanelAction::Request(StaffRequest::Unaward {
                key_id: me.key_id.clone(),
                medal: "early_tester".into()
            })
        );
        assert_eq!(
            panel.activate(GIVE_BASE + 2),
            PanelAction::Request(StaffRequest::Award {
                key_id: me.key_id.clone(),
                medal: "bug_hunter".into(),
                note: "found the fog bug".into()
            })
        );
        // Not held: nothing to take back.
        assert_eq!(panel.activate(TAKE_BASE + 2), PanelAction::None);
    }

    #[test]
    fn unlockables_are_unlocked_and_relocked_as_held() {
        let (mut panel, me, _) = drawn(Vec::new());
        assert!(panel.order.contains(&UNLOCK_BASE), "Unlock is offered");
        assert!(!panel.order.contains(&RELOCK_BASE), "nothing to relock");
        panel.note = " for testing ".into();
        assert_eq!(panel.activate(RELOCK_BASE), PanelAction::None);
        assert_eq!(
            panel.activate(UNLOCK_BASE),
            PanelAction::Request(StaffRequest::Unlock {
                key_id: me.key_id.clone(),
                unlock: "saber_sun".into(),
                note: "for testing".into()
            })
        );
        // Once the profile lists it: Relock, and no second Unlock.
        let held = Profile {
            unlocks: vec![sjk_identity::Unlock {
                id: "saber_sun".into(),
                granted: 1,
                note: String::new(),
                count: 1,
                medal: None,
            }],
            ..me.clone()
        };
        let staff = StaffState::default();
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        panel.build(
            &Inputs {
                me: Some(&held),
                staff: &staff,
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
        assert!(panel.order.contains(&RELOCK_BASE));
        assert!(!panel.order.contains(&UNLOCK_BASE));
        assert_eq!(panel.activate(UNLOCK_BASE), PanelAction::None);
        assert_eq!(
            panel.activate(RELOCK_BASE),
            PanelAction::Request(StaffRequest::Relock {
                key_id: me.key_id,
                unlock: "saber_sun".into()
            })
        );
    }

    fn wire_holocron(id: u64, tier: &str) -> sjk_identity::Holocron {
        sjk_identity::Holocron {
            opened: None,
            id,
            tier: tier.to_owned(),
            dropped: 1_791_641_100,
            source: "play".to_owned(),
            note: String::new(),
        }
    }

    #[test]
    fn a_holocron_is_given_in_the_chosen_tier_with_the_note() {
        let (mut panel, me, _) = drawn(Vec::new());
        panel.note = "  for the fog bug ".into();
        // Uncommon is chosen to begin with.
        assert_eq!(
            panel.activate(HOLOCRON_GIVE_TOKEN),
            PanelAction::Request(StaffRequest::HolocronGive {
                key_id: me.key_id.clone(),
                tier: "uncommon".into(),
                note: "for the fog bug".into(),
            })
        );
        assert_eq!(
            panel.activate(HOLOCRON_TIER_BASE + 3),
            PanelAction::None,
            "choosing a tier sends nothing"
        );
        assert_eq!(
            panel.activate(HOLOCRON_GIVE_TOKEN),
            PanelAction::Request(StaffRequest::HolocronGive {
                key_id: me.key_id,
                tier: "mythical".into(),
                note: "for the fog bug".into(),
            })
        );
        assert!(panel.order.contains(&HOLOCRON_GIVE_TOKEN));
        for tier in 0..crate::holocrons::COUNT as u16 {
            assert!(panel.order.contains(&(HOLOCRON_TIER_BASE + tier)));
        }
    }

    #[test]
    fn a_holocron_the_player_holds_is_taken_back_by_its_number() {
        let (mut panel, _, staff) = drawn(Vec::new());
        // Nothing listed yet: nothing to remove.
        assert_eq!(panel.activate(HOLOCRON_REMOVE_BASE), PanelAction::None);
        assert!(!panel.order.contains(&HOLOCRON_REMOVE_BASE));
        let held = Profile {
            staff: true,
            holocrons: vec![
                wire_holocron(41, "legendary"),
                wire_holocron(40, "from_the_future"),
                wire_holocron(39, "rare"),
            ],
            ..profile("aaaaaaaaaaaaaaaa", "^1Sol")
        };
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        panel.build(
            &Inputs {
                me: Some(&held),
                staff: &staff,
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
        // Rows are the known tiers, newest first.
        assert_eq!(
            panel.activate(HOLOCRON_REMOVE_BASE),
            PanelAction::Request(StaffRequest::HolocronRemove {
                key_id: "aaaaaaaaaaaaaaaa".into(),
                id: 41
            })
        );
        assert_eq!(
            panel.activate(HOLOCRON_REMOVE_BASE + 1),
            PanelAction::Request(StaffRequest::HolocronRemove {
                key_id: "aaaaaaaaaaaaaaaa".into(),
                id: 39
            })
        );
        assert_eq!(panel.activate(HOLOCRON_REMOVE_BASE + 2), PanelAction::None);
        assert!(panel.order.contains(&(HOLOCRON_REMOVE_BASE + 1)));
        assert!(!panel.order.contains(&(HOLOCRON_REMOVE_BASE + 2)));
    }

    #[test]
    fn clear_all_needs_a_second_press() {
        let (mut panel, me, _) = drawn(Vec::new());
        assert_eq!(panel.activate(CLEAR_ALL_TOKEN), PanelAction::None);
        assert!(panel.confirming(CLEAR_ALL_TOKEN));
        assert_eq!(
            panel.activate(CLEAR_ALL_TOKEN),
            PanelAction::Request(StaffRequest::ClearAchievements {
                key_id: me.key_id.clone(),
                id: String::new()
            })
        );
        // Anything else in between starts again.
        let _ = panel.activate(CLEAR_ALL_TOKEN);
        let _ = panel.activate(ME_TOKEN);
        assert_eq!(panel.activate(CLEAR_ALL_TOKEN), PanelAction::None);
        assert_eq!(
            panel.activate(CLEAR_BASE),
            PanelAction::Request(StaffRequest::ClearAchievements {
                key_id: me.key_id,
                id: "first_blood".into()
            })
        );
    }

    /// `panel` laid out again with `players` found and `me` as the player's own.
    fn redraw(panel: &mut Panel, me: &Profile, players: Vec<Profile>) {
        let staff = StaffState {
            players,
            ..StaffState::default()
        };
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        panel.build(
            &Inputs {
                me: Some(me),
                staff: &staff,
            },
            &fonts.font,
            [1920.0, 1080.0],
        );
    }

    #[test]
    fn verify_shows_the_state_and_sends_the_other() {
        let (mut panel, me, _) = drawn(Vec::new());
        assert!(panel.order.contains(&VERIFY_TOKEN));
        assert_eq!(
            panel.activate(VERIFY_TOKEN),
            PanelAction::Request(StaffRequest::Verify {
                key_id: me.key_id.clone(),
                verified: true
            })
        );
        let verified = Profile {
            verified: true,
            ..me.clone()
        };
        redraw(&mut panel, &verified, Vec::new());
        assert_eq!(
            panel.activate(VERIFY_TOKEN),
            PanelAction::Request(StaffRequest::Verify {
                key_id: me.key_id,
                verified: false
            })
        );
    }

    #[test]
    fn a_linked_key_is_unlinked_on_a_second_press_and_the_main_key_never() {
        let (mut panel, me, _) = drawn(Vec::new());
        let _ = panel.activate(KEYS_TAB_TOKEN);
        let linked = Profile {
            keys: vec![me.key_id.clone(), "cccccccccccccccc".into()],
            ..me.clone()
        };
        redraw(&mut panel, &linked, Vec::new());
        assert!(panel.order.contains(&UNLINK_BASE));
        assert!(
            !panel.order.contains(&(UNLINK_BASE + 1)),
            "the main key has none"
        );
        assert_eq!(panel.activate(UNLINK_BASE), PanelAction::None);
        assert!(panel.confirming(UNLINK_BASE));
        assert_eq!(
            panel.activate(UNLINK_BASE),
            PanelAction::Request(StaffRequest::Unlink {
                key_id: "cccccccccccccccc".into()
            })
        );
        // Anything else in between starts again.
        let _ = panel.activate(UNLINK_BASE);
        let _ = panel.activate(KEYS_TAB_TOKEN);
        assert_eq!(panel.activate(UNLINK_BASE), PanelAction::None);
        assert_eq!(panel.activate(UNLINK_BASE + 1), PanelAction::None);
    }

    #[test]
    fn a_merge_keeps_the_chosen_player_and_asks_before_it_goes() {
        let reset = profile("bbbbbbbbbbbbbbbb", "Fox");
        let (mut panel, me, _) = drawn(vec![reset.clone()]);
        let _ = panel.activate(KEYS_TAB_TOKEN);
        let players = vec![reset.clone()];
        redraw(&mut panel, &me, players.clone());
        assert_eq!(panel.shown.merge_problem, Some(MergeProblem::Empty));
        assert!(!panel.order.contains(&MERGE_TOKEN), "nothing to merge yet");
        // Typed: checked as a key id, never the chosen player's own.
        for (typed, problem) in [
            ("bbbb", Some(MergeProblem::NotKey)),
            ("AAAAAAAAAAAAAAAA", Some(MergeProblem::Same)),
            (" BBBBBBBBBBBBBBBB ", None),
        ] {
            panel.merge_from = typed.into();
            redraw(&mut panel, &me, players.clone());
            assert_eq!(panel.shown.merge_problem, problem, "{typed}");
        }
        // Merge only asks; Merge for good sends, keeping the chosen player.
        assert_eq!(panel.activate(MERGE_TOKEN), PanelAction::None);
        assert_eq!(panel.focus, MERGE_CANCEL_TOKEN, "Cancel has the keyboard");
        redraw(&mut panel, &me, players.clone());
        assert!(panel.order.contains(&MERGE_CONFIRM_TOKEN));
        assert_eq!(
            panel.activate(MERGE_CONFIRM_TOKEN),
            PanelAction::Request(StaffRequest::Merge {
                key_id: me.key_id.clone(),
                from: "bbbbbbbbbbbbbbbb".into()
            })
        );
        assert!(panel.merge_from.is_empty());
        assert_eq!(panel.activate(MERGE_CONFIRM_TOKEN), PanelAction::None);
        // Picked in the list: the field fills, the chosen player stays.
        let _ = panel.activate(MERGE_PICK_TOKEN);
        assert!(panel.picking);
        assert_eq!(panel.activate(PLAYER_BASE), PanelAction::None);
        assert!(!panel.picking);
        assert_eq!(panel.merge_from, "bbbbbbbbbbbbbbbb");
        redraw(&mut panel, &me, players.clone());
        assert_eq!(panel.shown.target.as_deref(), Some(me.key_id.as_str()));
        // Choosing another player lets a merge waiting for confirmation go.
        let _ = panel.activate(MERGE_TOKEN);
        assert!(panel.pending_merge.is_some());
        panel.selected = Some("bbbbbbbbbbbbbbbb".into());
        redraw(&mut panel, &me, players.clone());
        assert!(panel.pending_merge.is_none());
        assert_eq!(panel.activate(MERGE_CONFIRM_TOKEN), PanelAction::None);
        // Escape cancels a waiting merge before it closes the page.
        panel.selected = None;
        redraw(&mut panel, &me, players);
        let _ = panel.activate(MERGE_TOKEN);
        assert!(panel.pending_merge.is_some());
        assert_eq!(panel.activate(MERGE_CANCEL_TOKEN), PanelAction::None);
        assert!(panel.pending_merge.is_none());
    }

    #[test]
    fn a_staff_player_is_never_offered_to_merge_away() {
        let other_staff = Profile {
            staff: true,
            keys: vec!["bbbbbbbbbbbbbbbb".into(), "cccccccccccccccc".into()],
            ..profile("bbbbbbbbbbbbbbbb", "Mod")
        };
        let (mut panel, me, _) = drawn(vec![other_staff.clone()]);
        let _ = panel.activate(KEYS_TAB_TOKEN);
        // By a linked key too.
        panel.merge_from = "cccccccccccccccc".into();
        redraw(&mut panel, &me, vec![other_staff]);
        assert_eq!(panel.shown.merge_problem, Some(MergeProblem::Staff));
        assert_eq!(panel.activate(MERGE_TOKEN), PanelAction::None);
        assert!(panel.pending_merge.is_none());
    }

    #[test]
    fn tab_walks_every_control_and_typing_goes_to_the_field() {
        let (mut panel, _, _) = drawn(vec![profile("bbbbbbbbbbbbbbbb", "Fox")]);
        assert!(panel.order.contains(&SEARCH_TOKEN));
        assert!(panel.order.contains(&PLAYER_BASE));
        assert!(panel.order.contains(&NOTE_TOKEN));
        assert!(panel.order.contains(&CLEAR_BASE), "first_blood is listed");
        let start = panel.focus;
        for _ in 0..panel.order.len() {
            panel.step(true);
        }
        assert_eq!(panel.focus, start, "Tab comes round");
        assert!(type_into(&mut panel.query, "So\u{7}l", QUERY_MAX));
        assert_eq!(panel.query, "Sol");
        panel.focus = SEARCH_TOKEN;
        assert_eq!(
            panel.activate(SEARCH_TOKEN),
            PanelAction::Request(StaffRequest::Search("Sol".into()))
        );
        let mut long = String::new();
        type_into(&mut long, &"x".repeat(300), NOTE_MAX);
        assert_eq!(long.len(), NOTE_MAX);
    }
}

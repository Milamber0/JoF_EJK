//! The Holocrons page (`docs/holocrons.md`, "The Holocrons tab"): the player's holocrons
//! on the SJK UI's Profile screen as its Holocrons tab ([`crate::profile_hub`]), and, with
//! the classic menus, opened by the Profile page's See holocrons or the `holocrons`
//! command. Like the Unlockables page it lives in the console and has the SJK UI's look
//! in every menu style ([`view`]).
//!
//! Left, the page leaves the world clear for one 3D holocron
//! ([`crate::holocrons::stage`]) wearing the look of the tier chosen; right, the four
//! tiers in a list (picture, name in the tier's colour, how many are held, the odds of a
//! drop), what the chosen tier is, how long to the next holocron, and the newest ten. A
//! tier held none of shows dimmed, never hidden. Under the holocron, Open opens the
//! oldest of the tier chosen and Open all up to twenty, in a ceremony over the page
//! ([`opening`]): each brings a sealed card and credits (`docs/cards.md`). Loot table (or
//! L) shows the odds of everything a holocron brings over the page ([`loot`]).
//!
//! Up and Down (or Left and Right, or the mouse wheel) choose a tier, Tab and Shift+Tab
//! walk them round, 1 to 4 and Home and End jump, Enter opens one, A opens all, L shows
//! the loot table, Escape goes back; a click on a tier chooses it. The hub's counts and list are read twice a
//! second ([`Data::read`]), never per frame; while the ceremony waits for the hub its
//! answer is looked for every frame.

use crate::holocrons::{self, COUNT, Tier};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use sjk_ui::{InputEvent, UiEventKind};
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "holocrons_panel_loot.rs"]
mod loot;
#[path = "holocrons_panel_opening.rs"]
pub(crate) mod opening;
#[path = "holocrons_panel_crate.rs"]
mod strip;
#[path = "holocrons_panel_view.rs"]
mod view;

/// The tiers' rows, one token each: clear of the Unlockables page's cards (to 1260), the
/// Profile page's (to 1030) and the Profile screen's row of tabs (from 2000).
pub(crate) const ROW_BASE: u16 = 1_300;
/// Where the holocron floats on the 16:9 frame: the middle of the clear left of the page,
/// in frame pixels (not moved with the page under the Profile screen's tabs).
pub(crate) const STAGE_AT: [f32; 2] = [540.0, 480.0];
/// How many recent holocrons the page lists.
pub(crate) const RECENT_SHOWN: usize = 10;
/// How often the hub's counts and list are read again while the page is open.
const REFRESH: Duration = Duration::from_millis(500);
/// The buttons under the holocron: Open (one of the tier chosen) and Open all.
pub(crate) const OPEN_TOKEN: u16 = 1_320;
pub(crate) const OPEN_ALL_TOKEN: u16 = 1_321;
/// Loot table, beside what the chosen tier holds, and the loot table's Back.
pub(crate) const LOOT_TOKEN: u16 = 1_323;
pub(crate) const LOOT_BACK_TOKEN: u16 = 1_324;

/// What the page knows of the player's holocrons: whether the hub could say, and what.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum State {
    /// The identity is off: holocrons need it.
    IdentityOff,
    /// No hub is set (`cl_hubUrl`).
    NoHub,
    /// The hub has not answered yet, or cannot be reached.
    Waiting,
    /// The own profile's counts and list are known.
    Known,
}

/// One of the recent holocrons as the page lists it, its words made once per read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Recent {
    /// Position in [`holocrons::TIERS`].
    pub(crate) tier: usize,
    /// `dd/mm/yyyy HH:MM` (UTC), empty when the hub did not say.
    pub(crate) when: String,
    /// Staff gave it rather than play.
    pub(crate) gift: bool,
    /// The team's note, plain text; empty without one.
    pub(crate) note: String,
    /// It was opened.
    pub(crate) opened: bool,
}

/// What the page shows of the identity and the hub, read by [`Data::read`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Data {
    pub(crate) state: State,
    /// How many of each tier the player holds, once known.
    pub(crate) counts: Option<[u32; COUNT]>,
    /// The newest holocrons first, at most [`RECENT_SHOWN`].
    pub(crate) recent: Vec<Recent>,
    /// "Next holocron in about 20 minutes of play", empty when the hub did not say.
    pub(crate) next: String,
    /// Holocrons found in the last day and the most a day brings.
    pub(crate) today: Option<(u32, u32)>,
    /// How far along the next one is, 0 to 1.
    pub(crate) fraction: Option<f32>,
    /// The ids of the holocrons of each tier the profile lists that can still be
    /// opened, the oldest first.
    pub(crate) sealed: [Vec<u64>; COUNT],
    /// The player's credits, once known.
    pub(crate) credits: Option<u64>,
    /// Seconds of play a holocron takes, once the hub said.
    pub(crate) every: Option<u64>,
    /// Whether the hub lets the player open holocrons now: only staff while opening is
    /// tested (`docs/holocrons.md`, "Opening"). False until the hub said.
    pub(crate) can_open: bool,
}

impl Data {
    /// Nothing known: the page of `state`.
    pub(crate) fn nothing(state: State) -> Self {
        Self {
            state,
            counts: None,
            recent: Vec::new(),
            next: String::new(),
            today: None,
            fraction: None,
            sealed: Default::default(),
            credits: None,
            every: None,
            can_open: false,
        }
    }

    /// From `cl_identity` (`enabled`) and the identity service's snapshot, by the rule the
    /// Collection's holdings follow.
    pub(crate) fn of(enabled: bool, snapshot: &sjk_identity::Snapshot) -> Self {
        use sjk_identity::Status;
        let state = match (enabled, &snapshot.status, &snapshot.me) {
            (false, _, _) | (_, Status::Disabled, _) => State::IdentityOff,
            (_, Status::NoHub, _) => State::NoHub,
            (_, _, Some(_)) => State::Known,
            (_, _, None) => State::Waiting,
        };
        let (Some(me), State::Known) = (&snapshot.me, state) else {
            return Self::nothing(state);
        };
        let known: Vec<(holocrons::Entry, bool)> = me
            .holocrons
            .iter()
            .filter_map(|wire| {
                holocrons::Entry::from_wire(wire).map(|entry| (entry, !wire.is_sealed()))
            })
            .collect();
        let mut sealed: [Vec<u64>; COUNT] = Default::default();
        // The list is the newest first: the oldest go first when opening.
        for (entry, opened) in known.iter().rev() {
            if !opened {
                sealed[entry.tier.index].push(entry.id);
            }
        }
        let recent = known
            .into_iter()
            .take(RECENT_SHOWN)
            .map(|(entry, opened)| Recent {
                tier: entry.tier.index,
                when: holocrons::when_text(entry.dropped),
                gift: entry.gift,
                note: entry.note,
                opened,
            })
            .collect();
        let progress = snapshot.holocrons;
        Self {
            state,
            counts: Some(holocrons::counts_of(&me.holocron_counts)),
            recent,
            next: progress
                .as_ref()
                .map(holocrons::next_text)
                .unwrap_or_default(),
            today: progress.map(|progress| (progress.today, progress.daily_cap)),
            fraction: progress
                .filter(|progress| progress.every_secs > 0)
                .map(|progress| {
                    (progress.progress_secs as f32 / progress.every_secs as f32).clamp(0.0, 1.0)
                }),
            sealed,
            credits: Some(me.credits),
            every: progress
                .map(|progress| progress.every_secs)
                .filter(|&every| every > 0),
            can_open: progress.is_some_and(|progress| progress.can_open),
        }
    }

    /// The live identity's: `enabled` is `cl_identity`.
    pub(crate) fn read(enabled: bool) -> Self {
        crate::player_identity::with_snapshot(|snapshot| Self::of(enabled, snapshot))
            .unwrap_or_else(|| Self::nothing(State::IdentityOff))
    }

    /// How many holocrons the player holds in all, once known.
    pub(crate) fn total(&self) -> Option<u32> {
        self.counts.map(|counts| counts.iter().sum())
    }

    /// How many of tier `index` they hold, once known.
    pub(crate) fn count(&self, index: usize) -> Option<u32> {
        self.counts.and_then(|counts| counts.get(index).copied())
    }

    /// How many holocrons the page can open now (those the profile lists, none while the
    /// hub keeps opening to staff).
    pub(crate) fn openable(&self) -> usize {
        if !self.can_open {
            return 0;
        }
        self.sealed.iter().map(Vec::len).sum()
    }

    /// Whether the hub said opening is not for the player yet (so the page says so in
    /// place of Open and Open all).
    pub(crate) fn opening_closed(&self) -> bool {
        self.state == State::Known && self.every.is_some() && !self.can_open
    }
}

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Show another of the Collection screen's tabs (a click on the row).
    Hub(crate::profile_hub::Tab),
    /// Ask the hub to open these holocrons ([`opening`]).
    Open(Vec<u64>),
}

pub(crate) struct Panel {
    /// The tier row the pointer rests on, for its hover card.
    hover: crate::menu::sjk::hover::Hover,
    open: bool,
    owns_console: bool,
    /// Shown as the Profile screen's Holocrons tab: its title and row of tabs at the top,
    /// the page moved down under them.
    hub: bool,
    /// What the Profile screen's tabs put at the top.
    hub_header: crate::profile_hub::Header,
    /// The Collection row's counts after each tab's name, in the row's order.
    row: Vec<String>,
    ui: MenuCanvas,
    /// The tier chosen, an index into [`holocrons::TIERS`].
    selected: usize,
    /// What the hub last said, and when it was read.
    data: Data,
    refreshed: Option<Instant>,
    /// The opening's ceremony, or the wait for the hub, while it shows.
    ceremony: Option<opening::Ceremony>,
    /// The loot table shows over the page.
    loot: bool,
    /// The serial of the identity's last opening the page saw, so a new one is its
    /// answer.
    opening_serial: u64,
    /// The page's clock, for the finishes' motion.
    epoch: Instant,
    /// The blade skins the hub's packs brought, the crate tiles' blades.
    skins: std::sync::Arc<crate::saber_skins::LoadedSkins>,
    /// The crate strip's tile under the marker last frame (one click a tile), and
    /// whether the entrance after the spin has played its cue.
    spin_tick: usize,
    entered: bool,
    /// The data is a world shot's own, never read again.
    #[cfg(test)]
    pinned: bool,
    /// The time a world shot draws the ceremony's finishes at.
    #[cfg(test)]
    pub(crate) shot_seconds: Option<f32>,
    /// How far into its entrance a world shot draws the ceremony's card.
    #[cfg(test)]
    pub(crate) shot_elapsed: Option<f32>,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            hover: Default::default(),
            open: false,
            owns_console: false,
            hub: false,
            hub_header: crate::profile_hub::Header::default(),
            row: Vec::new(),
            ui: MenuCanvas::with_capacities(160, 160, 1_100),
            selected: 0,
            data: Data::nothing(State::Waiting),
            refreshed: None,
            ceremony: None,
            loot: false,
            opening_serial: 0,
            epoch: Instant::now(),
            #[cfg(test)]
            pinned: false,
            skins: Default::default(),
            spin_tick: 0,
            entered: false,
            #[cfg(test)]
            shot_seconds: None,
            #[cfg(test)]
            shot_elapsed: None,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page on its own, on the first tier; `owns_console` when the console was
    /// closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        self.hub = false;
        self.selected = 0;
        self.refreshed = None;
        self.ceremony = None;
        self.loot = false;
    }

    /// Show the page as the Profile screen's Holocrons tab (`true`), or on its own;
    /// [`Self::open`] puts it back on its own.
    pub(crate) fn set_hub(&mut self, hub: bool) {
        self.hub = hub;
    }

    /// Whether the page is the Profile screen's Holocrons tab.
    pub(crate) fn is_hub(&self) -> bool {
        self.hub
    }

    /// What the Profile screen's tabs put at the top.
    /// Put `row`'s counts after the Collection's tabs' names.
    pub(crate) fn set_collection_row(&mut self, row: Vec<String>) {
        self.row = row;
    }

    pub(crate) fn hub_header(&self) -> &crate::profile_hub::Header {
        &self.hub_header
    }

    /// Put `header` at the top on the Profile screen.
    pub(crate) fn set_hub_header(&mut self, header: crate::profile_hub::Header) {
        self.hub_header = header;
    }

    /// Hide the page; returns whether it had opened the console.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.ceremony = None;
        self.loot = false;
        self.owns_console = false;
        owned
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Whether the last frame ran out of room on the canvas.
    #[cfg(test)]
    pub(crate) fn overflowed(&self) -> bool {
        self.ui.overflowed()
    }

    /// Read the hub's counts and list again when they are half a second old; `read`
    /// makes them (the live identity's, [`Data::read`]). A world shot's own are kept.
    pub(crate) fn refresh(&mut self, now: Instant, read: impl FnOnce() -> Data) {
        #[cfg(test)]
        if self.pinned {
            return;
        }
        if self
            .refreshed
            .is_none_or(|at| now.saturating_duration_since(at) >= REFRESH)
        {
            self.data = read();
            self.refreshed = Some(now);
        }
    }

    /// Show `data` in place of the live ones, for a world shot.
    #[cfg(test)]
    pub(crate) fn pin(&mut self, data: Data) {
        self.data = data;
        self.pinned = true;
    }

    /// What the page last showed.
    #[cfg(test)]
    pub(crate) fn data(&self) -> &Data {
        &self.data
    }

    /// The tier chosen.
    pub(crate) fn selected(&self) -> &'static Tier {
        &holocrons::TIERS[self.selected]
    }

    /// Show the hover card of tier row `index` (`None` for none), for a world shot.
    #[cfg(test)]
    pub(crate) fn hover_for_shot(&mut self, index: Option<usize>) {
        self.hover.pin(index.map(|index| ROW_BASE + index as u16));
    }

    /// Choose tier `index` (kept to the tiers there are), for a world shot and the keys.
    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(COUNT - 1);
    }

    /// What the 3D holocron shows: the chosen tier, in its own look while the player
    /// holds one of it, else the locked look.
    pub(crate) fn stage_request(&self) -> holocrons::stage::Request {
        holocrons::stage::Request {
            tier: self.selected,
            owned: self
                .data
                .count(self.selected)
                .is_some_and(|count| count > 0),
        }
    }

    /// Step the choice `by` tiers: round the list for Tab, stopping at its ends for the
    /// arrows and the wheel.
    fn step(&mut self, by: isize, wrap: bool) {
        let count = COUNT as isize;
        let next = self.selected as isize + by;
        self.selected = if wrap {
            next.rem_euclid(count)
        } else {
            next.clamp(0, count - 1)
        } as usize;
    }

    /// A key.
    pub(crate) fn handle_key(&mut self, event: &KeyEvent, shift: bool) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        self.hover.keyed();
        self.key(key, shift)
    }

    /// A pressed key, by its code.
    fn key(&mut self, key: KeyCode, shift: bool) -> PanelAction {
        let now = Instant::now();
        if self.ceremony.is_some() {
            return self.ceremony_key(key, now);
        }
        if self.loot {
            return self.loot_key(key);
        }
        match key {
            KeyCode::Escape => return PanelAction::Close,
            KeyCode::Tab => self.step(if shift { -1 } else { 1 }, true),
            KeyCode::ArrowUp | KeyCode::ArrowLeft => self.step(-1, false),
            KeyCode::ArrowDown | KeyCode::ArrowRight => self.step(1, false),
            KeyCode::Home => self.select(0),
            KeyCode::End => self.select(COUNT - 1),
            KeyCode::Digit1 | KeyCode::Numpad1 => self.select(0),
            KeyCode::Digit2 | KeyCode::Numpad2 => self.select(1),
            KeyCode::Digit3 | KeyCode::Numpad3 => self.select(2),
            KeyCode::Digit4 | KeyCode::Numpad4 => self.select(3),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(ids) = self.open_one() {
                    return self.ask(ids, now);
                }
            }
            KeyCode::KeyA => {
                if let Some(ids) = self.open_every() {
                    return self.ask(ids, now);
                }
            }
            KeyCode::KeyL => self.show_loot(true),
            _ => {}
        }
        PanelAction::None
    }

    /// A pointer event.
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let now = Instant::now();
        self.hover.moved();
        if self.ceremony.is_some() {
            // Any click moves the ceremony on, its button or not.
            if let InputEvent::PointerRelease { .. } = event {
                let _ = self.ui.pointer(event);
                self.advance(now);
            } else if !matches!(event, InputEvent::PointerWheel { .. }) {
                let _ = self.ui.pointer(event);
            }
            return PanelAction::None;
        }
        if self.loot {
            return self.loot_pointer(event);
        }
        if let InputEvent::PointerWheel { delta, .. } = event {
            // Up (positive) goes to the tier above.
            if delta.y > 0.0 {
                self.step(-1, false);
            } else if delta.y < 0.0 {
                self.step(1, false);
            }
            return PanelAction::None;
        }
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            Some(token) if (ROW_BASE..ROW_BASE + COUNT as u16).contains(&token) => {
                self.select(usize::from(token - ROW_BASE));
                PanelAction::None
            }
            Some(OPEN_TOKEN) => self
                .open_one()
                .map_or(PanelAction::None, |ids| self.ask(ids, now)),
            Some(OPEN_ALL_TOKEN) => self
                .open_every()
                .map_or(PanelAction::None, |ids| self.ask(ids, now)),
            Some(LOOT_TOKEN) => {
                self.show_loot(true);
                PanelAction::None
            }
            Some(token) if self.hub => match crate::profile_hub::collection_tab_of(token) {
                Some(tab) if tab != crate::profile_hub::Tab::Holocrons => PanelAction::Hub(tab),
                _ => PanelAction::None,
            },
            _ => PanelAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(counts: [u32; COUNT]) -> Data {
        Data {
            counts: Some(counts),
            ..Data::nothing(State::Known)
        }
    }

    #[test]
    fn the_keys_choose_a_tier_and_stop_at_the_ends_while_tab_comes_round() {
        let mut panel = Panel::new();
        panel.open(true);
        assert_eq!(
            panel.selected().id,
            "uncommon",
            "it opens on the first tier"
        );
        let _ = panel.key(KeyCode::ArrowUp, false);
        assert_eq!(panel.selected, 0, "the arrows stop at the ends");
        let _ = panel.key(KeyCode::ArrowDown, false);
        let _ = panel.key(KeyCode::ArrowRight, false);
        assert_eq!(panel.selected().id, "legendary");
        let _ = panel.key(KeyCode::ArrowLeft, false);
        assert_eq!(panel.selected().id, "rare");
        let _ = panel.key(KeyCode::End, false);
        assert_eq!(panel.selected().id, "mythical");
        let _ = panel.key(KeyCode::ArrowDown, false);
        assert_eq!(panel.selected, COUNT - 1, "no tier under the last");
        let _ = panel.key(KeyCode::Tab, false);
        assert_eq!(panel.selected, 0, "Tab comes round");
        let _ = panel.key(KeyCode::Tab, true);
        assert_eq!(panel.selected, COUNT - 1, "and Shift+Tab back");
        let _ = panel.key(KeyCode::Home, false);
        assert_eq!(panel.selected, 0);
        for (key, id) in [
            (KeyCode::Digit2, "rare"),
            (KeyCode::Digit3, "legendary"),
            (KeyCode::Numpad4, "mythical"),
            (KeyCode::Digit1, "uncommon"),
        ] {
            let _ = panel.key(key, false);
            assert_eq!(panel.selected().id, id);
        }
        // Nothing opens: Enter and Space change nothing.
        let _ = panel.key(KeyCode::Digit2, false);
        assert_eq!(panel.key(KeyCode::Enter, false), PanelAction::None);
        assert_eq!(panel.key(KeyCode::Space, false), PanelAction::None);
        assert_eq!(panel.selected().id, "rare");
        assert_eq!(panel.key(KeyCode::Escape, false), PanelAction::Close);
        // Opening again starts at the top.
        panel.open(true);
        assert_eq!(panel.selected, 0);
    }

    #[test]
    fn the_wheel_steps_the_choice() {
        let mut panel = Panel::new();
        panel.open(true);
        let wheel = |panel: &mut Panel, y: f32| {
            panel.handle_pointer(InputEvent::PointerWheel {
                position: sjk_ui::Vec2::new(1_400.0, 500.0),
                delta: sjk_ui::Vec2::new(0.0, y),
            })
        };
        assert_eq!(wheel(&mut panel, -1.0), PanelAction::None);
        assert_eq!(wheel(&mut panel, -1.0), PanelAction::None);
        assert_eq!(panel.selected, 2, "down goes to the next tier");
        let _ = wheel(&mut panel, 1.0);
        assert_eq!(panel.selected, 1);
        for _ in 0..9 {
            let _ = wheel(&mut panel, 1.0);
        }
        assert_eq!(panel.selected, 0, "it stops at the first");
    }

    #[test]
    fn the_stage_shows_the_tier_chosen_and_dims_one_held_none_of() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.data = held([3, 0, 1, 0]);
        let request = |panel: &Panel| panel.stage_request();
        assert_eq!(request(&panel).tier, 0);
        assert!(request(&panel).owned);
        panel.select(1);
        assert!(!request(&panel).owned, "none held: the locked look");
        assert_eq!(request(&panel).model(), holocrons::stage::LOCKED_MODEL);
        panel.select(2);
        assert_eq!(
            request(&panel).model(),
            "models/sjk/holocron_legendary.md3",
            "the chosen tier's own model"
        );
        panel.select(3);
        assert!(!request(&panel).owned);
        // Not known (no identity, no answer): shown dimmed, never hidden.
        panel.data = Data::nothing(State::IdentityOff);
        panel.select(0);
        assert!(!request(&panel).owned);
        panel.select(99);
        assert_eq!(panel.selected, COUNT - 1, "kept to the tiers there are");
    }

    #[test]
    fn the_hubs_counts_and_list_are_read_twice_a_second_not_every_frame() {
        let mut panel = Panel::new();
        panel.open(true);
        let start = Instant::now();
        let mut reads = 0;
        for frame in 0..30 {
            panel.refresh(start + Duration::from_millis(frame * 16), || {
                reads += 1;
                held([reads, 0, 0, 0])
            });
        }
        assert_eq!(reads, 1, "30 frames in 0.5 s read once");
        panel.refresh(start + Duration::from_millis(520), || {
            reads += 1;
            held([reads, 0, 0, 0])
        });
        assert_eq!(panel.data().count(0), Some(2));
        // A reopened page reads at once.
        panel.open(true);
        panel.refresh(start + Duration::from_millis(530), || {
            reads += 1;
            held([reads, 0, 0, 0])
        });
        assert_eq!(panel.data().count(0), Some(3));
    }

    #[test]
    fn nothing_is_known_before_the_identity_starts() {
        let data = Data::read(true);
        assert_eq!(data, Data::nothing(State::IdentityOff));
        assert_eq!((data.total(), data.count(0)), (None, None));
    }

    fn snapshot(
        status: sjk_identity::Status,
        me: Option<sjk_identity::Profile>,
    ) -> sjk_identity::Snapshot {
        sjk_identity::Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            local_key_id: "0123456789abcdef".into(),
            status,
            key_id: "0123456789abcdef".into(),
            me,
            server: None,
            players: Vec::new(),
            profiles: std::collections::HashMap::new(),
            model_checks: std::collections::HashMap::new(),
            profiles_revision: 0,
            notice: None,
            revision: 0,
            report: None,
            note: None,
            player_report: None,
            avatar: None,
            look_outcome: None,
            packs_revision: 0,
            assets_note: None,
            crashes_sent: 0,
            crash_note: None,
            holocrons: None,
            log_requests: Vec::new(),
            log_answer: None,
        }
    }

    fn profile(holocrons: Vec<sjk_identity::Holocron>) -> sjk_identity::Profile {
        sjk_identity::Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "0123456789abcdef".into(),
            key: String::new(),
            name: "Sol".into(),
            bio: String::new(),
            verified: false,
            staff: false,
            created: 0,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            unlocks: Vec::new(),
            avatar: String::new(),
            holocron_counts: sjk_identity::HolocronCounts {
                uncommon: 5,
                rare: 3,
                legendary: 1,
                mythical: 0,
            },
            holocrons,
        }
    }

    fn holocron(id: u64, tier: &str, source: &str, note: &str) -> sjk_identity::Holocron {
        sjk_identity::Holocron {
            opened: None,
            id,
            tier: tier.to_owned(),
            dropped: 1_791_641_100,
            source: source.to_owned(),
            note: note.to_owned(),
        }
    }

    /// The page follows the identity and the hub like the Collection does: off, no hub,
    /// waiting, then the own profile's counts, its newest ten (the known tiers) and the
    /// progress as the hub last said.
    #[test]
    fn the_data_follows_the_identity_and_the_own_profile() {
        use sjk_identity::Status;
        let online = snapshot(Status::Online, Some(profile(Vec::new())));
        assert_eq!(Data::of(false, &online).state, State::IdentityOff);
        assert_eq!(
            Data::of(true, &snapshot(Status::Disabled, None)).state,
            State::IdentityOff
        );
        assert_eq!(
            Data::of(true, &snapshot(Status::NoHub, None)).state,
            State::NoHub
        );
        assert_eq!(
            Data::of(true, &snapshot(Status::Registering, None)).state,
            State::Waiting
        );
        assert_eq!(
            Data::of(true, &snapshot(Status::Registering, None)).counts,
            None
        );
        let known = Data::of(true, &online);
        assert_eq!(known.state, State::Known);
        assert_eq!(known.counts, Some([5, 3, 1, 0]));
        assert_eq!(known.total(), Some(9));
        assert_eq!(known.count(1), Some(3));
        assert!(
            known.next.is_empty() && known.today.is_none(),
            "no progress yet"
        );
        // Twelve holocrons, one of an unknown tier, a gift: the newest ten known ones.
        let mut list: Vec<sjk_identity::Holocron> = (0..12)
            .map(|id| holocron(100 - id, "rare", "play", ""))
            .collect();
        list[1] = holocron(99, "from_the_future", "play", "");
        list[0] = holocron(100, "legendary", "staff", "For the ^1fog^7 bug");
        let mut snapshot = snapshot(Status::Online, Some(profile(list)));
        snapshot.holocrons = Some(sjk_identity::HolocronProgress {
            progress_secs: 600,
            every_secs: 1_800,
            today: 3,
            daily_cap: 8,
            server_time: 0,
            can_open: false,
            read_at: Instant::now(),
        });
        let data = Data::of(true, &snapshot);
        assert!(data.opening_closed(), "a player while opening is for staff");
        assert_eq!(data.openable(), 0);
        assert_eq!(data.recent.len(), RECENT_SHOWN);
        assert_eq!(data.recent[0].tier, 2);
        assert!(data.recent[0].gift);
        assert_eq!(data.recent[0].note, "For the fog bug");
        assert_eq!(data.recent[0].when, "10/10/2026 14:05");
        assert_eq!(data.recent[1].tier, 1, "the unknown tier is left out");
        assert!(!data.recent[1].gift);
        assert_eq!(data.next, "Next holocron in about 20 minutes of play");
        assert_eq!(data.today, Some((3, 8)));
        assert!((data.fraction.unwrap() - 1.0 / 3.0).abs() < 1e-6);
    }

    fn openable() -> Data {
        let mut data = held([2, 1, 0, 0]);
        data.sealed = [vec![3, 8], vec![5], Vec::new(), Vec::new()];
        data.credits = Some(120);
        data.every = Some(1_800);
        data.can_open = true;
        data
    }

    fn opened(tier: &str, finish: &str) -> sjk_identity::Opened {
        sjk_identity::Opened {
            holocron: 3,
            tier: tier.to_owned(),
            card: Some(sjk_identity::OpenedCard {
                id: 1,
                card: None,
                rarity: "rare".to_owned(),
                finish: finish.to_owned(),
                edition: "alpha".to_owned(),
            }),
            credits: 42,
            unlock: None,
        }
    }

    /// A shader newer than this client is still a prize: shown by the hub's name, ending
    /// the spin on the "new shader" tile; a known one keeps its catalogue entry.
    #[test]
    fn a_shader_this_client_does_not_know_still_shows() {
        let shader = |id: &str, name: &str| sjk_identity::Opened {
            card: None,
            credits: 0,
            unlock: Some(sjk_identity::OpenedUnlock {
                id: id.to_owned(),
                name: name.to_owned(),
                count: 1,
            }),
            ..opened("rare", "normal")
        };
        let Some(opening::Prize::Shader(found)) =
            opening::Prize::of(&shader("saber_from_the_future", "Comet blade"))
        else {
            panic!("a shader prize");
        };
        assert_eq!((found.skin, found.name.as_str()), (None, "Comet blade"));
        let Some(opening::Prize::Shader(nameless)) =
            opening::Prize::of(&shader("saber_from_the_future", " "))
        else {
            panic!("a shader prize");
        };
        assert_eq!(nameless.name, "A new saber shader");
        let Some(opening::Prize::Shader(tidal)) =
            opening::Prize::of(&shader("saber_tidal", "Tidal blade"))
        else {
            panic!("a shader prize");
        };
        assert_eq!(tidal.skin.map(|skin| skin.id), Some("saber_tidal"));
    }

    /// While the hub keeps opening to staff, neither Enter nor A asks it anything and the
    /// page says why.
    #[test]
    fn a_player_cannot_open_while_opening_is_for_staff() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.data = openable();
        panel.data.can_open = false;
        assert!(panel.data.opening_closed());
        assert_eq!(panel.data.openable(), 0);
        assert_eq!(panel.key(KeyCode::Enter, false), PanelAction::None);
        assert_eq!(panel.key(KeyCode::KeyA, false), PanelAction::None);
        assert_eq!(panel.ask(vec![3], Instant::now()), PanelAction::None);
        assert!(!panel.opening());
    }

    /// Enter opens the oldest of the tier chosen and A every one, the oldest first; the
    /// page then waits for the hub, whose new answer starts the ceremony, which Enter
    /// moves on card by card and ends.
    #[test]
    fn opening_asks_the_hub_and_plays_its_answer() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.data = openable();
        panel.select(2);
        assert_eq!(
            panel.key(KeyCode::Enter, false),
            PanelAction::None,
            "none held"
        );
        panel.select(0);
        assert_eq!(panel.open_every(), Some(vec![3, 5, 8]));
        assert_eq!(panel.key(KeyCode::Enter, false), PanelAction::Open(vec![3]));
        assert!(panel.opening());
        assert_eq!(
            panel.key(KeyCode::KeyA, false),
            PanelAction::None,
            "one at a time"
        );
        let now = Instant::now();
        // An old outcome (the serial seen before) is not this page's answer.
        panel.opening_serial = 4;
        let old = sjk_identity::OpenOutcome {
            serial: 4,
            opened: vec![opened("rare", "holo")],
            message: String::new(),
        };
        panel.follow_opening(now, Some(&old));
        assert!(matches!(
            panel.ceremony,
            Some(opening::Ceremony::Waiting { .. })
        ));
        let answer = sjk_identity::OpenOutcome {
            serial: 5,
            opened: vec![
                opened("uncommon", "holo"),
                opened("uncommon", "glitter"),
                opened("rare", "solar"),
            ],
            message: String::new(),
        };
        panel.follow_opening(now, Some(&answer));
        let Some(opening::Ceremony::Showing { shown, index, .. }) = &panel.ceremony else {
            panic!("the ceremony shows");
        };
        assert_eq!(
            shown.len(),
            2,
            "a finish the client does not know is left out"
        );
        let (opening::Prize::Card(first), opening::Prize::Card(second)) = (&shown[0], &shown[1])
        else {
            panic!("two cards");
        };
        assert_eq!((*index, second.finish.id, second.credits), (0, "solar", 42));
        assert_eq!(first.edition, "Alpha Edition");
        // The first press skips the crate spin, the next finishes the entrance, the next
        // goes on, the last ends it.
        panel.advance(now);
        panel.advance(now);
        assert!(matches!(
            panel.ceremony,
            Some(opening::Ceremony::Showing { index: 0, .. })
        ));
        panel.advance(now);
        assert!(matches!(
            panel.ceremony,
            Some(opening::Ceremony::Showing { index: 1, .. })
        ));
        panel.advance(now);
        panel.advance(now);
        panel.advance(now);
        assert!(!panel.opening(), "Done after the last");
        assert!(panel.refreshed.is_none(), "the page reads the hub again");
    }

    /// A refusal or no answer says why; Escape ends the ceremony at any card.
    #[test]
    fn a_refusal_or_no_answer_says_why() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.data = openable();
        let now = Instant::now();
        let _ = panel.ask(vec![3], now);
        let refused = sjk_identity::OpenOutcome {
            serial: 1,
            opened: Vec::new(),
            message: "That holocron was already opened.".to_owned(),
        };
        panel.follow_opening(now, Some(&refused));
        assert!(matches!(
            &panel.ceremony,
            Some(opening::Ceremony::Failed { message, .. }) if message == "That holocron was already opened."
        ));
        panel.advance(now);
        assert!(!panel.opening());
        let _ = panel.ask(vec![3], now);
        panel.follow_opening(now + Duration::from_secs(5), None);
        assert!(matches!(
            panel.ceremony,
            Some(opening::Ceremony::Waiting { .. })
        ));
        panel.follow_opening(now + Duration::from_secs(16), None);
        assert!(matches!(
            panel.ceremony,
            Some(opening::Ceremony::Failed { .. })
        ));
        panel.show(
            vec![opening::Prize::of(&opened("rare", "holo")).unwrap()],
            now,
        );
        assert_eq!(panel.key(KeyCode::Escape, false), PanelAction::None);
        assert!(!panel.opening(), "Escape ends it, the page stays");
        assert!(panel.is_open());
    }

    /// Every moment of the ceremony fits the canvas, in every finish and window.
    #[test]
    fn the_ceremony_fits_the_canvas() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.data = openable();
        let start = Instant::now();
        for viewport in [[1920.0, 1080.0], [3840.0, 2160.0], [1440.0, 1080.0]] {
            for finish in ["normal", "holo", "solar"] {
                let shown = vec![
                    opening::Prize::of(&opened("mythical", finish)).unwrap(),
                    opening::Prize::of(&opened("rare", finish)).unwrap(),
                ];
                panel.show(shown, start);
                for tenth in 0..30 {
                    panel.build_at(viewport, start + Duration::from_millis(tenth * 100));
                    assert!(!panel.overflowed(), "{finish} at {tenth} in {viewport:?}");
                }
            }
            let _ = panel.ask(vec![3], start);
            panel.ceremony = Some(opening::Ceremony::Waiting {
                tier: 3,
                since: start,
            });
            panel.build_at(viewport, start);
            panel.refused("No.", start);
            panel.build_at(viewport, start);
            assert!(!panel.overflowed());
            panel.end_ceremony();
        }
    }

    #[test]
    fn a_click_chooses_a_tier_and_the_way_back_closes() {
        let mut panel = Panel::new();
        panel.open(true);
        panel.build([1920.0, 1080.0]);
        let click = |panel: &mut Panel, token: u16| {
            let rect = panel.ui.rect_for(token).expect("a pointer area");
            let at = sjk_ui::Vec2::new(rect.x + 4.0, rect.y + 4.0);
            let mut action = PanelAction::None;
            for event in [
                InputEvent::PointerMove(at),
                InputEvent::PointerPress {
                    position: at,
                    button: sjk_ui::PointerButton::Primary,
                },
                InputEvent::PointerRelease {
                    position: at,
                    button: sjk_ui::PointerButton::Primary,
                },
            ] {
                action = panel.handle_pointer(event);
            }
            action
        };
        for (index, tier) in holocrons::TIERS.iter().enumerate() {
            assert_eq!(
                click(&mut panel, ROW_BASE + index as u16),
                PanelAction::None
            );
            assert_eq!(panel.selected().id, tier.id);
            panel.build([1920.0, 1080.0]);
        }
        assert_eq!(click(&mut panel, BACK_TOKEN), PanelAction::Close);
    }
}

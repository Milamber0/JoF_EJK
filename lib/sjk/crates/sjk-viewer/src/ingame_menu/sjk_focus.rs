//! What the SJK UI's in-game main page offers beside its list (`docs/sjk-ui.md`,
//! In-game menu): the row of small icon buttons under the emblem ([`Icon`]) and the
//! match card's controls ([`Control`]), and how the keyboard moves between the three
//! ([`Focus`], [`step`]). The drawing is `sjk_view.rs`'s, what each one does
//! `sjk_actions.rs`'s; this module holds only the model, so the tests can walk it.
//!
//! - Up and Down move along the list (as before); Right from the list enters the card.
//! - Tab moves list, then the bottom row, then the card's controls, then the docked
//!   SJK chat's field, then the list again; Shift+Tab goes the other way.
//! - In the bottom row Left and Right move between the icons (Right from the last
//!   reaches the chat), Up returns to the list.
//! - On the card Left and Right move along a line of controls (Left from a line's
//!   first returns to the list), Up and Down between its lines (Down from the last
//!   reaches the chat).
//! - On the chat's field Up returns to the list, Left reaches the row's last icon and
//!   Right the card's last control.
//! - Enter acts on what has the keyboard (on the chat it starts typing); Escape returns
//!   to the list from the row, the card or the chat, and from the list resumes the
//!   match (the caller's).

use sjk_client::LegacyTeamChoice;

/// A small icon button of the row under the emblem, left to right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Icon {
    /// Camera control ([`super::Page::Shot`]); F8 opens it too.
    Camera,
    WhatsNew,
    Credits,
    ReportBug,
    Chat,
    /// Staff tools, for staff keys only.
    Staff,
}

impl Icon {
    /// Every icon, Staff last.
    pub(crate) const ALL: [Self; 6] = [
        Self::Camera,
        Self::WhatsNew,
        Self::Credits,
        Self::ReportBug,
        Self::Chat,
        Self::Staff,
    ];

    /// The icons the row shows: Staff only for a staff key.
    pub(crate) fn shown(staff: bool) -> &'static [Self] {
        if staff {
            &Self::ALL
        } else {
            &Self::ALL[..Self::ALL.len() - 1]
        }
    }

    /// Its name, shown over the row while it is hovered or has the keyboard.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Camera => super::CAMERA_CONTROL,
            Self::WhatsNew => "What's new",
            Self::Credits => "Credits",
            Self::ReportBug => "Report a bug",
            Self::Chat => "SJK chat",
            Self::Staff => "Staff tools",
        }
    }

    /// What it opens, after its name.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::Camera => "Frame shots: the camera and the sun",
            Self::WhatsNew => "Every release and who made it",
            Self::Credits => "The people who make SJK",
            Self::ReportBug => "Tell the SJK team what went wrong",
            Self::Chat => "Talk with every SJK player",
            Self::Staff => "Medals, unlockables and players, for the SJK team",
        }
    }

    /// Its key, when one opens it from the match.
    pub(crate) fn key(self) -> Option<&'static str> {
        (self == Self::Camera).then_some("F8")
    }

    /// Its picture: a settings icon or a quick wheel icon (`settings_icons::texture`).
    pub(crate) fn picture(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::WhatsNew => "whats_new",
            Self::Credits => "credits",
            Self::ReportBug => "other_controls",
            Self::Chat => "text",
            Self::Staff => "identity",
        }
    }
}

/// A control of the match card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Control {
    /// The vote on: Yes and No.
    VoteYes,
    VoteNo,
    /// Join a side, or spectate.
    Team(LegacyTeamChoice),
    /// Siege: its page of classes and sides.
    SiegeClass,
    /// No vote on: the call-vote page.
    CallVote,
}

impl Control {
    /// Its label on the card.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::VoteYes => "Yes",
            Self::VoteNo => "No",
            Self::Team(LegacyTeamChoice::Free | LegacyTeamChoice::Auto) => "Join",
            Self::Team(LegacyTeamChoice::Red) => "Join red",
            Self::Team(LegacyTeamChoice::Blue) => "Join blue",
            Self::Team(LegacyTeamChoice::Spectator) => "Spectate",
            Self::SiegeClass => "Class and side",
            Self::CallVote => "Call a vote",
        }
    }

    /// The reliable command it sends the server, when it sends one rather than
    /// opening a page.
    pub(crate) fn command(self) -> Option<&'static [u8]> {
        match self {
            Self::VoteYes => Some(b"vote yes"),
            Self::VoteNo => Some(b"vote no"),
            Self::Team(choice) => Some(sjk_client::legacy_team_command(choice)),
            Self::SiegeClass | Self::CallVote => None,
        }
    }
}

/// A card control as laid out: the line of the card it stands on (0 the vote's,
/// 1 the side's, 2 the foot's) and whether it can be taken now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Placed {
    pub(crate) control: Control,
    pub(crate) line: u8,
    pub(crate) enabled: bool,
}

/// Lines of the card's controls.
pub(crate) const VOTE_LINE: u8 = 0;
pub(crate) const SIDE_LINE: u8 = 1;
pub(crate) const FOOT_LINE: u8 = 2;

/// The card's controls this frame, in reading order (fixed storage).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Controls {
    items: [Placed; 6],
    len: usize,
}

impl Default for Controls {
    fn default() -> Self {
        Self::none()
    }
}

/// What the card's controls depend on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Match {
    /// A match is on (the card shows).
    pub(crate) known: bool,
    pub(crate) team_game: bool,
    pub(crate) siege: bool,
    /// The player's team (`TEAM_FREE` 0, red 1, blue 2, spectator 3).
    pub(crate) team: u8,
    /// A vote is on.
    pub(crate) vote: bool,
}

impl Controls {
    /// No card, no controls.
    pub(crate) const fn none() -> Self {
        Self {
            items: [Placed {
                control: Control::CallVote,
                line: 0,
                enabled: false,
            }; 6],
            len: 0,
        }
    }

    /// The card's controls for `game`: Yes and No while a vote is on; the side's
    /// buttons (Join red, Join blue and Spectate in a team game, Join and Spectate
    /// otherwise, Class and side and Spectate in Siege), the side the player is on
    /// passed over; Call a vote at the foot while no vote is on.
    pub(crate) fn for_match(game: Match) -> Self {
        let mut controls = Self::none();
        if !game.known {
            return controls;
        }
        let watching = game.team == 3;
        if game.vote {
            controls.push(Control::VoteYes, VOTE_LINE, true);
            controls.push(Control::VoteNo, VOTE_LINE, true);
        }
        if game.siege {
            controls.push(Control::SiegeClass, SIDE_LINE, true);
        } else if game.team_game {
            controls.push(
                Control::Team(LegacyTeamChoice::Red),
                SIDE_LINE,
                game.team != 1,
            );
            controls.push(
                Control::Team(LegacyTeamChoice::Blue),
                SIDE_LINE,
                game.team != 2,
            );
        } else {
            controls.push(Control::Team(LegacyTeamChoice::Free), SIDE_LINE, watching);
        }
        controls.push(
            Control::Team(LegacyTeamChoice::Spectator),
            SIDE_LINE,
            !watching,
        );
        if !game.vote {
            controls.push(Control::CallVote, FOOT_LINE, true);
        }
        controls
    }

    fn push(&mut self, control: Control, line: u8, enabled: bool) {
        if let Some(slot) = self.items.get_mut(self.len) {
            *slot = Placed {
                control,
                line,
                enabled,
            };
            self.len += 1;
        }
    }

    pub(crate) fn as_slice(&self) -> &[Placed] {
        &self.items[..self.len]
    }

    /// Whether `control` is on the card and can be taken.
    pub(crate) fn takes(&self, control: Control) -> bool {
        self.as_slice()
            .iter()
            .any(|placed| placed.control == control && placed.enabled)
    }

    fn first(&self) -> Option<Control> {
        self.as_slice()
            .iter()
            .find(|placed| placed.enabled)
            .map(|placed| placed.control)
    }

    fn last(&self) -> Option<Control> {
        self.as_slice()
            .iter()
            .rev()
            .find(|placed| placed.enabled)
            .map(|placed| placed.control)
    }

    /// Where `control` is: its line and its place on the line.
    fn place(&self, control: Control) -> Option<(u8, usize)> {
        let placed = self
            .as_slice()
            .iter()
            .find(|placed| placed.control == control)?;
        let column = self
            .as_slice()
            .iter()
            .filter(|other| other.line == placed.line)
            .position(|other| other.control == control)?;
        Some((placed.line, column))
    }

    /// The control that can be taken on `line` nearest place `column`.
    fn on_line(&self, line: u8, column: usize) -> Option<Control> {
        self.as_slice()
            .iter()
            .filter(|placed| placed.line == line)
            .enumerate()
            .filter(|(_, placed)| placed.enabled)
            .min_by_key(|(index, _)| index.abs_diff(column))
            .map(|(_, placed)| placed.control)
    }

    /// The next control that can be taken along `control`'s line, `forward` or back.
    fn along(&self, control: Control, forward: bool) -> Option<Control> {
        let (line, column) = self.place(control)?;
        let line_items: Vec<Placed> = self
            .as_slice()
            .iter()
            .filter(|placed| placed.line == line)
            .copied()
            .collect();
        let found = if forward {
            line_items[column + 1..]
                .iter()
                .find(|placed| placed.enabled)
        } else {
            line_items[..column]
                .iter()
                .rev()
                .find(|placed| placed.enabled)
        };
        found.map(|placed| placed.control)
    }

    /// The nearest control that can be taken on a line above (`down` false) or
    /// below `control`'s.
    fn across(&self, control: Control, down: bool) -> Option<Control> {
        let (line, column) = self.place(control)?;
        let lines: [u8; 3] = [VOTE_LINE, SIDE_LINE, FOOT_LINE];
        let mut candidates: Vec<u8> = lines
            .into_iter()
            .filter(|other| if down { *other > line } else { *other < line })
            .collect();
        if !down {
            candidates.reverse();
        }
        candidates
            .into_iter()
            .find_map(|other| self.on_line(other, column))
    }
}

/// Where the keyboard is on the main page.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Focus {
    /// The list of entries (the chosen one is the menu's row).
    #[default]
    List,
    /// An icon of the bottom row.
    Row(Icon),
    /// A control of the match card.
    Card(Control),
    /// The field of the docked SJK chat, under the match.
    Chat,
}

impl Focus {
    /// The focus kept where it can still be: an icon no longer shown, the chat no longer
    /// docked (`chat`), or a card control gone or no longer to be taken, moves to the
    /// card's first control or the list.
    pub(crate) fn settle(self, icons: &[Icon], controls: &Controls, chat: bool) -> Self {
        match self {
            Self::Row(icon) if !icons.contains(&icon) => Self::List,
            Self::Card(control) if !controls.takes(control) => {
                controls.first().map_or(Self::List, Self::Card)
            }
            Self::Chat if !chat => Self::List,
            focus => focus,
        }
    }
}

/// A key the main page moves by.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Move {
    Up,
    Down,
    Left,
    Right,
    Tab,
    BackTab,
}

/// What a move does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Step {
    /// The keyboard goes to this.
    To(Focus),
    /// The list steps to its next entry (`true`) or previous one, as it always has.
    List(bool),
    /// Nothing there.
    Stay,
}

/// Where `movement` takes the keyboard from `focus`, with `icons` in the bottom row,
/// `controls` on the card and the SJK chat docked or not (`chat`).
pub(crate) fn step(
    focus: Focus,
    movement: Move,
    icons: &[Icon],
    controls: &Controls,
    chat: bool,
) -> Step {
    let to = |focus: Option<Focus>| focus.map_or(Step::Stay, Step::To);
    let first_icon = icons.first().copied().map(Focus::Row);
    let last_icon = icons.last().copied().map(Focus::Row);
    let card_first = controls.first().map(Focus::Card);
    let card_last = controls.last().map(Focus::Card);
    let field = chat.then_some(Focus::Chat);
    match (focus.settle(icons, controls, chat), movement) {
        (Focus::List, Move::Up) => Step::List(false),
        (Focus::List, Move::Down) => Step::List(true),
        (Focus::List, Move::Right) => to(card_first),
        (Focus::List, Move::Left) => Step::Stay,
        (Focus::List, Move::Tab) => to(first_icon.or(card_first).or(field)),
        (Focus::List, Move::BackTab) => to(field.or(card_last).or(first_icon)),
        (Focus::Row(icon), Move::Left | Move::Right) => {
            let at = icons.iter().position(|shown| *shown == icon).unwrap_or(0);
            let next = if movement == Move::Right {
                icons.get(at + 1).copied().map(Focus::Row).or(field)
            } else {
                at.checked_sub(1)
                    .and_then(|before| icons.get(before))
                    .copied()
                    .map(Focus::Row)
            };
            to(next)
        }
        (Focus::Row(_), Move::Up) => Step::To(Focus::List),
        (Focus::Row(_), Move::Down) => Step::Stay,
        (Focus::Row(_), Move::Tab) => Step::To(card_first.or(field).unwrap_or(Focus::List)),
        (Focus::Row(_), Move::BackTab) => Step::To(Focus::List),
        (Focus::Card(control), Move::Left) => Step::To(
            controls
                .along(control, false)
                .map_or(Focus::List, Focus::Card),
        ),
        (Focus::Card(control), Move::Right) => to(controls.along(control, true).map(Focus::Card)),
        (Focus::Card(control), Move::Up) => to(controls.across(control, false).map(Focus::Card)),
        (Focus::Card(control), Move::Down) => {
            to(controls.across(control, true).map(Focus::Card).or(field))
        }
        (Focus::Card(_), Move::Tab) => Step::To(field.unwrap_or(Focus::List)),
        (Focus::Card(_), Move::BackTab) => Step::To(first_icon.unwrap_or(Focus::List)),
        (Focus::Chat, Move::Up) => Step::To(Focus::List),
        (Focus::Chat, Move::Left) => to(last_icon),
        (Focus::Chat, Move::Right) => to(card_last),
        (Focus::Chat, Move::Down) => Step::Stay,
        (Focus::Chat, Move::Tab) => Step::To(Focus::List),
        (Focus::Chat, Move::BackTab) => Step::To(card_last.or(first_icon).unwrap_or(Focus::List)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(team_game: bool, team: u8, vote: bool) -> Match {
        Match {
            known: true,
            team_game,
            siege: false,
            team,
            vote,
        }
    }

    fn labels(controls: &Controls) -> Vec<(&'static str, u8, bool)> {
        controls
            .as_slice()
            .iter()
            .map(|placed| (placed.control.label(), placed.line, placed.enabled))
            .collect()
    }

    #[test]
    fn the_card_offers_the_sides_and_the_vote_the_match_has() {
        // A CTF on blue, no vote: both sides and Spectate, blue passed over, then
        // Call a vote at the foot.
        assert_eq!(
            labels(&Controls::for_match(game(true, 2, false))),
            [
                ("Join red", SIDE_LINE, true),
                ("Join blue", SIDE_LINE, false),
                ("Spectate", SIDE_LINE, true),
                ("Call a vote", FOOT_LINE, true),
            ]
        );
        // An FFA with a vote on: Yes and No first, no Call a vote.
        assert_eq!(
            labels(&Controls::for_match(game(false, 0, true))),
            [
                ("Yes", VOTE_LINE, true),
                ("No", VOTE_LINE, true),
                ("Join", SIDE_LINE, false),
                ("Spectate", SIDE_LINE, true),
            ]
        );
        // Watching an FFA: Join offered, Spectate passed over.
        assert_eq!(
            labels(&Controls::for_match(game(false, 3, false)))[..2],
            [("Join", SIDE_LINE, true), ("Spectate", SIDE_LINE, false)]
        );
        // Siege: its class and side page, and Spectate.
        let siege = Controls::for_match(Match {
            siege: true,
            ..game(true, 1, false)
        });
        assert_eq!(
            labels(&siege)[..2],
            [
                ("Class and side", SIDE_LINE, true),
                ("Spectate", SIDE_LINE, true)
            ]
        );
        // No match, no card.
        let none = Controls::for_match(Match {
            known: false,
            ..game(true, 1, true)
        });
        assert!(none.as_slice().is_empty());
    }

    #[test]
    fn the_cards_controls_send_the_servers_own_commands() {
        let sent = |control: Control| {
            control
                .command()
                .map(|bytes| std::str::from_utf8(bytes).unwrap())
        };
        assert_eq!(sent(Control::VoteYes), Some("vote yes"));
        assert_eq!(sent(Control::VoteNo), Some("vote no"));
        assert_eq!(sent(Control::Team(LegacyTeamChoice::Red)), Some("team red"));
        assert_eq!(
            sent(Control::Team(LegacyTeamChoice::Blue)),
            Some("team blue")
        );
        assert_eq!(
            sent(Control::Team(LegacyTeamChoice::Free)),
            Some("team free")
        );
        assert_eq!(
            sent(Control::Team(LegacyTeamChoice::Spectator)),
            Some("team s")
        );
        // These open pages instead.
        assert_eq!(sent(Control::CallVote), None);
        assert_eq!(sent(Control::SiegeClass), None);
    }

    #[test]
    fn the_staff_icon_shows_only_for_staff() {
        assert!(!Icon::shown(false).contains(&Icon::Staff));
        assert_eq!(Icon::shown(false).len(), 5);
        assert_eq!(Icon::shown(true).last(), Some(&Icon::Staff));
        // A staff icon focused when the key is no longer staff falls back to the list.
        let none = Controls::none();
        assert_eq!(
            Focus::Row(Icon::Staff).settle(Icon::shown(false), &none, false),
            Focus::List
        );
        assert_eq!(Icon::Camera.key(), Some("F8"));
        for icon in Icon::ALL {
            assert!(
                crate::settings_icons::ICONS
                    .iter()
                    .any(|(name, _)| *name == icon.picture())
                    || crate::quick_wheel::catalog::icon_index(icon.picture()).is_some(),
                "{icon:?}"
            );
        }
    }

    #[test]
    fn tab_moves_list_row_card_and_back() {
        let icons = Icon::shown(false);
        let card = Controls::for_match(game(true, 2, false));
        let mut focus = Focus::List;
        let mut seen = Vec::new();
        for _ in 0..3 {
            match step(focus, Move::Tab, icons, &card, false) {
                Step::To(next) => focus = next,
                other => panic!("{other:?}"),
            }
            seen.push(focus);
        }
        assert_eq!(
            seen,
            [
                Focus::Row(Icon::Camera),
                // Join blue is passed over: the player is on blue.
                Focus::Card(Control::Team(LegacyTeamChoice::Red)),
                Focus::List,
            ]
        );
        // Shift+Tab goes the other way.
        assert_eq!(
            step(Focus::List, Move::BackTab, icons, &card, false),
            Step::To(Focus::Card(Control::CallVote))
        );
        assert_eq!(
            step(
                Focus::Card(Control::CallVote),
                Move::BackTab,
                icons,
                &card,
                false
            ),
            Step::To(Focus::Row(Icon::Camera))
        );
        assert_eq!(
            step(Focus::Row(Icon::Chat), Move::BackTab, icons, &card, false),
            Step::To(Focus::List)
        );
        // Without a card (a map explored alone) Tab goes row, list.
        let none = Controls::none();
        assert_eq!(
            step(Focus::Row(Icon::Camera), Move::Tab, icons, &none, false),
            Step::To(Focus::List)
        );
        assert_eq!(
            step(Focus::List, Move::Right, icons, &none, false),
            Step::Stay
        );
    }

    #[test]
    fn arrows_move_within_the_list_the_row_and_the_card() {
        let icons = Icon::shown(true);
        let card = Controls::for_match(game(true, 1, true));
        // The list keeps its own steps; Right enters the card on its first control.
        assert_eq!(
            step(Focus::List, Move::Down, icons, &card, false),
            Step::List(true)
        );
        assert_eq!(
            step(Focus::List, Move::Up, icons, &card, false),
            Step::List(false)
        );
        assert_eq!(
            step(Focus::List, Move::Right, icons, &card, false),
            Step::To(Focus::Card(Control::VoteYes))
        );
        // The row: Left and Right along it, its ends stay, Up back to the list.
        assert_eq!(
            step(Focus::Row(Icon::Camera), Move::Right, icons, &card, false),
            Step::To(Focus::Row(Icon::WhatsNew))
        );
        assert_eq!(
            step(Focus::Row(Icon::Camera), Move::Left, icons, &card, false),
            Step::Stay
        );
        assert_eq!(
            step(Focus::Row(Icon::Staff), Move::Right, icons, &card, false),
            Step::Stay
        );
        assert_eq!(
            step(Focus::Row(Icon::Credits), Move::Up, icons, &card, false),
            Step::To(Focus::List)
        );
        // The card: along a line, Left from its first back to the list.
        assert_eq!(
            step(
                Focus::Card(Control::VoteYes),
                Move::Right,
                icons,
                &card,
                false
            ),
            Step::To(Focus::Card(Control::VoteNo))
        );
        assert_eq!(
            step(
                Focus::Card(Control::VoteYes),
                Move::Left,
                icons,
                &card,
                false
            ),
            Step::To(Focus::List)
        );
        // Down from No to the side's line, nearest its place: on red, Join red is
        // passed over, so Join blue under No.
        assert_eq!(
            step(
                Focus::Card(Control::VoteNo),
                Move::Down,
                icons,
                &card,
                false
            ),
            Step::To(Focus::Card(Control::Team(LegacyTeamChoice::Blue)))
        );
        assert_eq!(
            step(
                Focus::Card(Control::Team(LegacyTeamChoice::Spectator)),
                Move::Up,
                icons,
                &card,
                false
            ),
            Step::To(Focus::Card(Control::VoteNo))
        );
        assert_eq!(
            step(
                Focus::Card(Control::Team(LegacyTeamChoice::Spectator)),
                Move::Down,
                icons,
                &card,
                false
            ),
            Step::Stay
        );
        // A control that went away (the vote ended) settles on the card's first.
        let quiet = Controls::for_match(game(true, 1, false));
        assert_eq!(
            Focus::Card(Control::VoteNo).settle(icons, &quiet, false),
            Focus::Card(Control::Team(LegacyTeamChoice::Blue))
        );
    }

    #[test]
    fn the_docked_chat_joins_the_tab_cycle_and_the_arrows() {
        let icons = Icon::shown(false);
        let card = Controls::for_match(game(true, 2, false));
        let red = Focus::Card(Control::Team(LegacyTeamChoice::Red));
        let vote = Focus::Card(Control::CallVote);
        let mut focus = Focus::List;
        let mut seen = Vec::new();
        for _ in 0..4 {
            match step(focus, Move::Tab, icons, &card, true) {
                Step::To(next) => focus = next,
                other => panic!("{other:?}"),
            }
            seen.push(focus);
        }
        assert_eq!(
            seen,
            [Focus::Row(Icon::Camera), red, Focus::Chat, Focus::List]
        );
        assert_eq!(
            step(Focus::List, Move::BackTab, icons, &card, true),
            Step::To(Focus::Chat)
        );
        assert_eq!(
            step(Focus::Chat, Move::BackTab, icons, &card, true),
            Step::To(vote)
        );
        // The row's last icon and the card's foot reach it; from it Up is the list.
        assert_eq!(
            step(Focus::Row(Icon::Chat), Move::Right, icons, &card, true),
            Step::To(Focus::Chat)
        );
        assert_eq!(
            step(vote, Move::Down, icons, &card, true),
            Step::To(Focus::Chat)
        );
        assert_eq!(
            step(Focus::Chat, Move::Up, icons, &card, true),
            Step::To(Focus::List)
        );
        assert_eq!(
            step(Focus::Chat, Move::Left, icons, &card, true),
            Step::To(Focus::Row(Icon::Chat))
        );
        assert_eq!(
            step(Focus::Chat, Move::Right, icons, &card, true),
            Step::To(vote)
        );
        assert_eq!(
            step(Focus::Chat, Move::Down, icons, &card, true),
            Step::Stay
        );
        // Without the chat its field settles on the list and nothing reaches it.
        assert_eq!(Focus::Chat.settle(icons, &card, false), Focus::List);
        assert_eq!(
            step(vote, Move::Tab, icons, &card, false),
            Step::To(Focus::List)
        );
        // Without a match card (a map explored alone): the row, the chat, the list.
        assert_eq!(
            step(
                Focus::Row(Icon::Camera),
                Move::Tab,
                icons,
                &Controls::none(),
                true
            ),
            Step::To(Focus::Chat)
        );
    }
}

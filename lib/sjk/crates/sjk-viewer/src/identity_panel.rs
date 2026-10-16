//! The Identity page: switch the SJK identity on or off, write an optional bio, copy the key
//! id, see the medals the SJK team gave you (`identity_panel_medals.rs`) and who the hub
//! knows on the server (state and work in `player_identity.rs`; the same things can be
//! typed with `identity_command.rs`). There is no name to choose: the hub
//! takes the name the player plays under, so a new player has nothing to do here.
//!
//! Opened by Settings (Network's "SJK identity key" row), the classic in-game SJK menu or
//! the `identity` console command. Like the Update page it lives in the console and is
//! drawn in place of it. Tab, the arrow keys and the pointer move between the controls;
//! letters type into the focused field; Enter saves from a field.
//!
//! What tells the key apart (its id, the key file's path with the account's name in it)
//! is hidden behind dots until Show is pressed, every time the page opens, so the page
//! can be opened on a stream.

use crate::menu::art::ArtSet;
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use crate::text::{TextVertex, UiFont};
use sjk_identity::{Snapshot, Status};
use sjk_ui::{Color, FontWeight, InputEvent, Rect, TextAlign, UiEventKind};
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "identity_panel_classic.rs"]
mod classic;
#[path = "identity_panel_medals.rs"]
mod medals;
#[path = "identity_panel_sjk.rs"]
mod sjk;

/// Most known players the page lists.
const PLAYERS_SHOWN: usize = 5;
/// The hub's limit on a bio, in characters.
pub(crate) const BIO_LIMIT: usize = 500;
/// Earlier names the page lists after the current one.
const EARLIER_NAMES: usize = 3;
/// How long "Copied" stays after the key id went to the clipboard.
const COPIED_FOR: Duration = Duration::from_millis(1500);

const TOGGLE_TOKEN: u16 = 930;
const BIO_TOKEN: u16 = 932;
const SAVE_TOKEN: u16 = 933;
const COPY_TOKEN: u16 = 934;
const HUB_TOKEN: u16 = 935;
const REVEAL_TOKEN: u16 = 936;
/// What stands for the key's id and file while they are hidden: bullets, which every
/// family has (Windows-1252's 0x95).
pub(crate) const HIDDEN: &str =
    "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022} (hidden)";

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Switch the identity (`cl_identity`) on or off.
    SetEnabled(bool),
    /// Send the bio to the hub.
    Save {
        bio: String,
    },
    /// Put the key id on the clipboard.
    CopyKeyId,
    /// Point `cl_hubUrl` at SJK's own hub again.
    UseDefaultHub,
    /// Show or hide the key's id and file.
    Reveal,
}

/// The control the keyboard is on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    Toggle,
    Bio,
    Save,
    Copy,
    /// Back to SJK's own hub, offered while the address is another one.
    Hub,
    /// Show or hide the key's id and file.
    Reveal,
}

impl Focus {
    fn token(self) -> u16 {
        match self {
            Self::Toggle => TOGGLE_TOKEN,
            Self::Bio => BIO_TOKEN,
            Self::Save => SAVE_TOKEN,
            Self::Copy => COPY_TOKEN,
            Self::Hub => HUB_TOKEN,
            Self::Reveal => REVEAL_TOKEN,
        }
    }
}

/// The controls Tab visits, in order: with the identity off only the switch exists, and
/// the way back to SJK's own hub only while it is offered.
fn order(fields: bool, hub: bool) -> &'static [Focus] {
    const ALL: [Focus; 6] = [
        Focus::Toggle,
        Focus::Bio,
        Focus::Save,
        Focus::Copy,
        Focus::Reveal,
        Focus::Hub,
    ];
    match (fields, hub) {
        (false, _) => &ALL[..1],
        (true, false) => &ALL[..5],
        (true, true) => &ALL,
    }
}

/// Where Tab (`forward`) or Shift+Tab goes from `current`.
fn step(current: Focus, forward: bool, fields: bool, hub: bool) -> Focus {
    let order = order(fields, hub);
    let at = order
        .iter()
        .position(|focus| *focus == current)
        .unwrap_or(0);
    let next = if forward {
        (at + 1) % order.len()
    } else {
        (at + order.len() - 1) % order.len()
    };
    order[next]
}

/// Append `text` to `field` as far as a bio may hold it (`sjk_identity::bio`: no emoji,
/// symbols or invisible characters; this one-line field takes no line breaks), up to
/// `limit` characters; whether the field changed.
fn type_chars(field: &mut String, text: &str, limit: usize) -> bool {
    let mut changed = false;
    let allowed = |c: &char| sjk_identity::bio::allowed(*c) || *c == '^';
    for character in text.chars().filter(allowed) {
        if field.chars().count() >= limit {
            break;
        }
        field.push(character);
        changed = true;
    }
    changed
}

/// `text` cut to its last `max` characters, with `...` in front when it was longer, so the
/// end being typed stays in view.
fn fit_tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max || max < 4 {
        return text.to_owned();
    }
    let tail: String = text.chars().skip(count - (max - 3)).collect();
    format!("...{tail}")
}

pub(crate) struct Panel {
    open: bool,
    /// The page opened the console, so closing the page closes it too.
    owns_console: bool,
    /// The key's id and file show: Show was pressed since the page opened.
    revealed: bool,
    ui: MenuCanvas,
    focus: Focus,
    /// The bio being typed.
    bio: String,
    /// Something was typed since the hub's copy was last the same: the hub's copy then
    /// does not replace the draft.
    edited: bool,
    /// What the last frame showed, for keys and clicks.
    enabled: bool,
    fields: bool,
    /// The hub address is not SJK's own: the way back to it is on offer.
    offer_hub: bool,
    /// The retail menu art the classic+ look can use.
    art: ArtSet,
    /// The SJK UI's look (`identity_panel_sjk.rs`), else the classic+ one.
    sjk: bool,
    /// A problem found before anything was sent.
    message: String,
    copied_until: Option<Instant>,
    epoch: Instant,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

/// What the page knows about the settings and the service.
pub(crate) struct Inputs<'a> {
    pub(crate) enabled: bool,
    pub(crate) hub_url: &'a str,
    pub(crate) key_error: Option<&'a str>,
    pub(crate) snapshot: Option<&'a Snapshot>,
    /// Where `identity.key` is, to tell the player to back it up.
    pub(crate) key_file: &'a str,
}

/// One known player.
#[derive(Debug, Eq, PartialEq)]
struct PlayerLine {
    slot: u8,
    name: String,
    verified: bool,
}

/// What the page says besides its controls.
#[derive(Debug, Eq, PartialEq)]
struct View {
    headline: String,
    lines: Vec<String>,
    players: Vec<PlayerLine>,
    /// The player's own medals, once the hub sent their profile.
    medals: Option<Vec<crate::medals::Award>>,
}

fn plain(headline: &str, lines: &[&str]) -> View {
    View {
        headline: headline.to_owned(),
        lines: lines.iter().map(|line| (*line).to_owned()).collect(),
        players: Vec::new(),
        medals: None,
    }
}

/// What the page says; the key's id and file are [`HIDDEN`] unless `revealed`.
fn view(inputs: &Inputs<'_>, revealed: bool) -> View {
    if let Some(error) = inputs.key_error {
        return View {
            lines: vec![
                error.to_owned(),
                "Restore identity.key from a backup, or move it away to start a new identity, then restart SJK."
                    .to_owned(),
            ],
            ..plain("The identity key cannot be used", &[])
        };
    }
    if !inputs.enabled {
        return plain(
            "Identity is off",
            &[
                "No key is made and nothing is sent anywhere.",
                "Switch it on below to let other SJK players see your name.",
            ],
        );
    }
    let Some(snapshot) = inputs.snapshot else {
        return plain("Starting...", &["Preparing the identity key."]);
    };
    let (key_id, key_file) = if revealed {
        (
            snapshot.local_key_id.as_str(),
            fit_tail(inputs.key_file, 90),
        )
    } else {
        (HIDDEN, HIDDEN.to_owned())
    };
    let key = format!("Key id: {key_id}");
    // Two lines: the path can be long.
    let backup = [
        format!("Key file: {key_file}"),
        "Back it up: losing it loses this identity.".to_owned(),
    ];
    let mut view = match &snapshot.status {
        Status::Disabled => plain("Identity is off", &["The settings were just changed."]),
        Status::NoHub => View {
            lines: [key]
                .into_iter()
                .chain(backup)
                .chain([
                    "No hub is set (cl_hubUrl), so nothing is sent. The key stays on this PC."
                        .to_owned(),
                ])
                .collect(),
            ..plain("Your identity key is ready", &[])
        },
        Status::Registering => View {
            lines: vec![key, format!("Contacting {}", inputs.hub_url)],
            ..plain("Contacting the hub...", &[])
        },
        Status::Failed(error) => View {
            lines: vec![
                key,
                error.clone(),
                format!("Hub: {}", inputs.hub_url),
                "Retrying automatically.".to_owned(),
            ],
            ..plain("Cannot reach the hub", &[])
        },
        Status::Online => online(snapshot, key, backup),
    };
    if let Some(notice) = &snapshot.notice {
        view.lines.push(format!("Last change: {notice}"));
    }
    if !matches!(snapshot.status, Status::Disabled | Status::NoHub) {
        view.medals = snapshot
            .me
            .as_ref()
            .map(|me| crate::medals::awards(&me.medals));
    }
    view.players = snapshot
        .players
        .iter()
        .take(PLAYERS_SHOWN)
        .map(|player| PlayerLine {
            slot: player.slot,
            name: if player.name.is_empty() {
                player.claimed_name.clone()
            } else {
                player.name.clone()
            },
            verified: player.verified,
        })
        .collect();
    view
}

/// `name` without its Quake colour codes, as a line of the page shows it.
fn plain_name(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(|next| *next != '^') {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

fn online(snapshot: &Snapshot, key: String, backup: [String; 2]) -> View {
    let mut lines = vec![key];
    let headline = match &snapshot.me {
        Some(me) => {
            lines.push(if me.verified {
                "Verified: the hub's operator vouches for this key.".to_owned()
            } else {
                "Not verified yet. Nothing to do: the hub's operator verifies players.".to_owned()
            });
            let earlier: Vec<String> = me
                .names
                .iter()
                .map(|worn| plain_name(&worn.name))
                .filter(|name| *name != plain_name(&me.name))
                .take(EARLIER_NAMES)
                .collect();
            if !earlier.is_empty() {
                lines.push(format!("Also known as: {}", earlier.join(", ")));
            }
            if me.name.is_empty() {
                "Registered".to_owned()
            } else {
                plain_name(&me.name)
            }
        }
        None => "Registered".to_owned(),
    };
    lines.extend(backup);
    lines.push(
        "Your SJK name is the name you play under; the hub keeps the names you wear.".to_owned(),
    );
    lines
        .push("The hub gets your public key, your in-game name and the server and slot".to_owned());
    lines.push("you play in, while you play. Switching the identity off stops it.".to_owned());
    View {
        headline,
        lines,
        players: Vec::new(),
        medals: None,
    }
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            owns_console: false,
            revealed: false,
            ui: MenuCanvas::with_capacities(160, 640, 512),
            focus: Focus::Toggle,
            bio: String::new(),
            edited: false,
            enabled: false,
            fields: false,
            offer_hub: false,
            art: ArtSet::default(),
            sjk: false,
            message: String::new(),
            copied_until: None,
            epoch: Instant::now(),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page, the key's id and file hidden; `owns_console` when the console was
    /// closed before it.
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open = true;
        self.owns_console = owns_console;
        self.revealed = false;
        self.focus = Focus::Toggle;
        self.edited = false;
        self.message.clear();
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

    /// Whether the key's id and file show (Show pressed since the page opened).
    #[cfg(test)]
    pub(crate) fn revealed(&self) -> bool {
        self.revealed
    }

    /// Show the key's id and file, for the snapshots.
    #[cfg(test)]
    pub(crate) fn reveal_for_shot(&mut self) {
        self.revealed = true;
    }

    /// The retail `art` the classic+ look can draw.
    pub(crate) fn set_art(&mut self, art: ArtSet) {
        self.art = art;
    }

    /// Whether the classic+ look is drawn, so its text uses the retail font.
    pub(crate) fn is_classic(&self) -> bool {
        !self.sjk
    }

    /// Draw the SJK UI's look (`sjk`), in its families, or not.
    pub(crate) fn set_sjk(&mut self, sjk: bool) {
        self.sjk = sjk;
    }

    /// Whether the SJK UI's look is drawn.
    pub(crate) fn is_sjk(&self) -> bool {
        self.sjk
    }

    /// Show a problem found before anything was sent.
    pub(crate) fn set_message(&mut self, message: &str) {
        self.message = message.to_owned();
    }

    /// The key id reached the clipboard: say so for a moment.
    pub(crate) fn note_copied(&mut self) {
        self.copied_until = Some(Instant::now() + COPIED_FOR);
    }

    /// Send the bio that was typed.
    fn save(&mut self) -> PanelAction {
        self.message.clear();
        PanelAction::Save {
            bio: self.bio.trim().to_owned(),
        }
    }

    /// What Enter (or a click) does on the focused control.
    fn activate(&mut self) -> PanelAction {
        match self.focus {
            Focus::Toggle => PanelAction::SetEnabled(!self.enabled),
            Focus::Bio | Focus::Save => self.save(),
            Focus::Copy => PanelAction::CopyKeyId,
            Focus::Hub => PanelAction::UseDefaultHub,
            Focus::Reveal => self.reveal(),
        }
    }

    /// Show or hide the key's id and file.
    fn reveal(&mut self) -> PanelAction {
        self.revealed = !self.revealed;
        PanelAction::Reveal
    }

    fn field(&mut self) -> Option<(&mut String, usize)> {
        match self.focus {
            Focus::Bio if self.fields => Some((&mut self.bio, BIO_LIMIT)),
            _ => None,
        }
    }

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
        let typing = self.field().is_some();
        if typing
            && !event.repeat
            && self.field().is_some_and(|(field, _)| {
                crate::text_entry::before_edit(field, key, event.text.as_deref(), shift, control)
            })
        {
            self.edited = true;
            return PanelAction::None;
        }
        match key {
            KeyCode::Escape => return PanelAction::Close,
            KeyCode::Tab => self.focus = step(self.focus, !shift, self.fields, self.offer_hub),
            KeyCode::ArrowDown => self.focus = step(self.focus, true, self.fields, self.offer_hub),
            KeyCode::ArrowUp => self.focus = step(self.focus, false, self.fields, self.offer_hub),
            KeyCode::Enter | KeyCode::NumpadEnter => return self.activate(),
            KeyCode::Space if !typing => return self.activate(),
            KeyCode::Backspace if typing => {
                if let Some((field, _)) = self.field() {
                    field.pop();
                    self.edited = true;
                }
            }
            _ if typing && !event.repeat => {
                if crate::text_entry::is_copy(key, event.text.as_deref(), control) {
                    let _ = crate::console::clipboard::copy(&self.bio);
                    return PanelAction::None;
                }
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
                let typed = self
                    .field()
                    .is_some_and(|(field, limit)| type_chars(field, text, limit));
                self.edited |= typed;
            }
            _ => {}
        }
        PanelAction::None
    }

    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            Some(TOGGLE_TOKEN) => {
                self.focus = Focus::Toggle;
                PanelAction::SetEnabled(!self.enabled)
            }

            Some(BIO_TOKEN) if self.fields => {
                self.focus = Focus::Bio;
                PanelAction::None
            }
            Some(SAVE_TOKEN) if self.fields => {
                self.focus = Focus::Save;
                self.save()
            }
            Some(COPY_TOKEN) if self.fields => {
                self.focus = Focus::Copy;
                PanelAction::CopyKeyId
            }
            Some(HUB_TOKEN) if self.offer_hub => {
                self.focus = Focus::Hub;
                PanelAction::UseDefaultHub
            }
            Some(REVEAL_TOKEN) if self.fields => {
                self.focus = Focus::Reveal;
                self.reveal()
            }
            _ => PanelAction::None,
        }
    }

    /// Take the hub's copy of the bio into its field unless the player is typing.
    fn sync(&mut self, inputs: &Inputs<'_>) {
        self.enabled = inputs.enabled && inputs.key_error.is_none();
        self.fields = self.enabled && inputs.snapshot.is_some();
        self.offer_hub = self.fields && inputs.hub_url != crate::player_identity::DEFAULT_HUB_URL;
        if !self.fields && self.focus != Focus::Toggle {
            self.focus = Focus::Toggle;
        }
        if self.focus == Focus::Hub && !self.offer_hub {
            self.focus = Focus::Copy;
        }
        if let Some(me) = inputs.snapshot.and_then(|snapshot| snapshot.me.as_ref()) {
            // Only what a bio may hold reaches the screen, whatever the hub sent.
            let hub = sjk_identity::bio::for_display(&me.bio);
            if self.bio == hub {
                self.edited = false;
            } else if !self.edited {
                self.bio = hub;
            }
        }
    }

    /// Draw the page over the whole frame; text other overlays appended earlier this frame is
    /// dropped rather than shown through.
    pub(crate) fn append(
        &mut self,
        inputs: &Inputs<'_>,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        vertices.clear();
        self.sync(inputs);
        self.append_classic(inputs, vertices, font, viewport);
    }
}

#[cfg(test)]
impl Panel {
    /// The page as if the player had typed `bio` and the keyboard were on `focus` (`bio`,
    /// `save`, `copy`, anything else is the switch), for the off-screen snapshots
    /// (`menu_snapshot.rs`).
    pub(crate) fn preview(&mut self, bio: &str, focus: &str, message: &str) {
        self.bio = bio.to_owned();
        self.edited = true;
        self.message = message.to_owned();
        self.focus = match focus {
            "bio" => Focus::Bio,
            "save" => Focus::Save,
            "copy" => Focus::Copy,
            "hub" => Focus::Hub,
            "reveal" => Focus::Reveal,
            _ => Focus::Toggle,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::{Presence, Profile};
    use std::collections::HashMap;

    fn snapshot(status: Status) -> Snapshot {
        Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            local_key_id: "0123456789abcdef".to_owned(),
            status,
            key_id: "0123456789abcdef".to_owned(),
            me: None,
            server: None,
            players: Vec::new(),
            profiles: HashMap::new(),
            model_checks: HashMap::new(),
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

    fn inputs<'a>(snapshot: Option<&'a Snapshot>) -> Inputs<'a> {
        Inputs {
            enabled: true,
            hub_url: "https://hub.example",
            key_error: None,
            snapshot,
            key_file: "GameData/SJK/identity.key",
        }
    }

    fn me(name: &str, bio: &str) -> Profile {
        Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "0123456789abcdef".to_owned(),
            key: String::new(),
            name: name.to_owned(),
            bio: bio.to_owned(),
            verified: false,
            staff: false,
            created: 0,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        }
    }

    #[test]
    fn a_broken_key_beats_everything_else() {
        let shown = view(
            &Inputs {
                key_error: Some("damaged"),
                enabled: false,
                ..inputs(None)
            },
            true,
        );
        assert_eq!(shown.headline, "The identity key cannot be used");
        assert_eq!(shown.lines[0], "damaged");
    }

    #[test]
    fn off_says_nothing_is_sent_and_how_to_switch_on() {
        let shown = view(
            &Inputs {
                enabled: false,
                ..inputs(None)
            },
            true,
        );
        assert_eq!(shown.headline, "Identity is off");
        assert!(shown.lines[0].contains("nothing is sent"));
        assert!(shown.lines[1].contains("Switch it on"));
    }

    #[test]
    fn without_a_hub_the_key_stays_local_and_the_file_is_named() {
        let state = snapshot(Status::NoHub);
        let shown = view(&inputs(Some(&state)), true);
        assert_eq!(shown.headline, "Your identity key is ready");
        assert!(
            shown
                .lines
                .iter()
                .any(|line| line.contains("stays on this PC"))
        );
        assert!(
            shown
                .lines
                .iter()
                .any(|line| line.contains("GameData/SJK/identity.key"))
        );
        assert!(shown.lines[0].contains("0123456789abcdef"));
    }

    #[test]
    fn a_registered_player_sees_their_name_verification_and_what_is_sent() {
        let mut state = snapshot(Status::Online);
        state.me = Some(Profile {
            verified: true,
            ..me("Sol", "")
        });
        state.notice = Some("saved".to_owned());
        state.players = vec![Presence {
            slot: 7,
            claimed_name: "x".to_owned(),
            key_id: "aaaaaaaaaaaaaaaa".to_owned(),
            name: "Kit".to_owned(),
            verified: true,
            medals: Vec::new(),
            avatar: String::new(),
            look: None,
        }];
        let shown = view(&inputs(Some(&state)), true);
        assert_eq!(shown.headline, "Sol");
        assert!(shown.lines.iter().any(|line| line.starts_with("Verified")));
        assert!(
            shown
                .lines
                .iter()
                .any(|line| line.contains("The hub gets your public key"))
        );
        assert!(shown.lines.contains(&"Last change: saved".to_owned()));
        assert_eq!(
            shown.players,
            [PlayerLine {
                slot: 7,
                name: "Kit".to_owned(),
                verified: true
            }]
        );
    }

    #[test]
    fn the_headline_is_the_name_worn_in_game_and_earlier_names_follow() {
        let mut state = snapshot(Status::Online);
        state.me = Some(me("", ""));
        let shown = view(&inputs(Some(&state)), true);
        assert_eq!(shown.headline, "Registered");
        assert!(
            shown
                .lines
                .iter()
                .any(|line| line.contains("Nothing to do"))
        );
        let worn = |name: &str, seen| sjk_identity::WornName {
            name: name.to_owned(),
            first_seen: seen,
            last_seen: seen,
        };
        state.me = Some(Profile {
            names: vec![worn("^1Sol", 9), worn("^2Fox", 5), worn("Padawan", 1)],
            ..me("^1Sol", "")
        });
        let shown = view(&inputs(Some(&state)), true);
        assert_eq!(shown.headline, "Sol");
        assert!(
            shown
                .lines
                .contains(&"Also known as: Fox, Padawan".to_owned())
        );
    }

    #[test]
    fn the_page_lists_the_players_own_known_medals_once_the_hub_answered() {
        let mut state = snapshot(Status::Online);
        assert_eq!(
            view(&inputs(Some(&state)), true).medals,
            None,
            "no profile yet"
        );
        state.me = Some(me("Sol", ""));
        assert_eq!(view(&inputs(Some(&state)), true).medals, Some(Vec::new()));
        let medal = |id: &str, count| sjk_identity::Medal {
            id: id.to_owned(),
            count,
            awarded: 1_791_336_225,
            note: "^2Thanks".to_owned(),
        };
        state.me = Some(Profile {
            medals: vec![medal("bug_hunter", 2), medal("unknown", 1)],
            ..me("Sol", "")
        });
        let medals = view(&inputs(Some(&state)), true).medals.expect("medals");
        assert_eq!(medals.len(), 1);
        assert_eq!(medals[0].label(), "Bug Hunter x2");
        assert_eq!(medals[0].note, "Thanks");
        assert_eq!(view(&inputs(None), true).medals, None);
    }

    /// Until Show is pressed the key's id and file are dots in every state, so the page
    /// can be opened on a stream; opening the page hides them again.
    #[test]
    fn the_key_id_and_file_stay_hidden_until_shown() {
        let mut state = snapshot(Status::Online);
        state.me = Some(me("Sol", ""));
        for status in [
            Status::Online,
            Status::NoHub,
            Status::Registering,
            Status::Failed("x".to_owned()),
        ] {
            state.status = status;
            let shown = view(
                &Inputs {
                    key_file: "C:/Users/Streamer/GameData/SJK/identity.key",
                    ..inputs(Some(&state))
                },
                false,
            );
            for line in &shown.lines {
                assert!(!line.contains("0123456789abcdef"), "{line}");
                assert!(!line.contains("Streamer"), "{line}");
            }
            assert!(shown.lines.iter().any(|line| line.contains(HIDDEN)));
        }
        let mut panel = Panel::new();
        panel.open(true);
        panel.sync(&inputs(Some(&state)));
        assert!(!panel.revealed());
        panel.focus = Focus::Reveal;
        assert_eq!(panel.activate(), PanelAction::Reveal);
        assert!(panel.revealed());
        panel.close();
        panel.open(false);
        assert!(!panel.revealed(), "hidden again each time the page opens");
        // Show is a stop of Tab once the fields show.
        assert_eq!(step(Focus::Copy, true, true, false), Focus::Reveal);
    }

    #[test]
    fn a_failed_hub_says_why_and_that_it_retries() {
        let state = snapshot(Status::Failed("cannot reach the hub: timed out".to_owned()));
        let shown = view(&inputs(Some(&state)), true);
        assert_eq!(shown.headline, "Cannot reach the hub");
        assert!(shown.lines.iter().any(|line| line.contains("timed out")));
        assert!(shown.lines.contains(&"Retrying automatically.".to_owned()));
    }

    #[test]
    fn the_page_lists_a_bounded_number_of_players() {
        let mut state = snapshot(Status::Online);
        state.players = (0..20)
            .map(|slot| Presence {
                slot,
                claimed_name: format!("p{slot}"),
                key_id: format!("{slot:016x}"),
                name: format!("p{slot}"),
                verified: false,
                medals: Vec::new(),
                avatar: String::new(),
                look: None,
            })
            .collect();
        assert_eq!(
            view(&inputs(Some(&state)), true).players.len(),
            PLAYERS_SHOWN
        );
    }

    #[test]
    fn tab_walks_the_controls_and_wraps_and_the_switch_stands_alone_when_off() {
        assert_eq!(step(Focus::Toggle, true, true, false), Focus::Bio);
        assert_eq!(step(Focus::Reveal, true, true, false), Focus::Toggle);
        assert_eq!(step(Focus::Toggle, false, true, false), Focus::Reveal);
        assert_eq!(step(Focus::Bio, false, true, false), Focus::Toggle);
        assert_eq!(step(Focus::Bio, true, false, false), Focus::Toggle);
    }

    #[test]
    fn the_way_back_to_the_official_hub_is_a_stop_only_while_it_is_offered() {
        assert_eq!(step(Focus::Reveal, true, true, true), Focus::Hub);
        assert_eq!(step(Focus::Hub, true, true, true), Focus::Toggle);
        assert_eq!(step(Focus::Toggle, false, true, true), Focus::Hub);
        assert_eq!(step(Focus::Reveal, true, true, false), Focus::Toggle);
    }

    #[test]
    fn the_official_hub_is_offered_for_any_other_address() {
        let mut panel = Panel::new();
        let state = snapshot(Status::Online);
        panel.sync(&inputs(Some(&state)));
        assert!(panel.offer_hub, "hub.example is not the default");
        panel.sync(&Inputs {
            hub_url: crate::player_identity::DEFAULT_HUB_URL,
            ..inputs(Some(&state))
        });
        assert!(!panel.offer_hub);
        panel.offer_hub = true;
        panel.focus = Focus::Hub;
        panel.enabled = true;
        panel.fields = true;
        assert_eq!(panel.activate(), PanelAction::UseDefaultHub);
        // The focus leaves the button once it is no longer offered.
        panel.sync(&Inputs {
            hub_url: crate::player_identity::DEFAULT_HUB_URL,
            ..inputs(Some(&state))
        });
        assert_eq!(panel.focus, Focus::Copy);
    }

    #[test]
    fn a_failed_hub_names_its_address() {
        let state = snapshot(Status::Failed("refused".to_owned()));
        let shown = view(&inputs(Some(&state)), true);
        assert!(shown.lines.contains(&"Hub: https://hub.example".to_owned()));
    }

    #[test]
    fn typing_respects_the_limit_in_characters_and_drops_control_characters() {
        let mut field = String::new();
        assert!(type_chars(&mut field, "Sol\u{7}\n", 24));
        assert_eq!(field, "Sol");
        let mut full = "a".repeat(23);
        type_chars(&mut full, "éèx", 24);
        assert_eq!(
            full.chars().count(),
            24,
            "stops at 24 characters, not bytes"
        );
        assert!(full.ends_with('é'));
        assert!(!type_chars(&mut full, "more", 24));
    }

    #[test]
    fn a_long_field_shows_its_end() {
        assert_eq!(fit_tail("short", 20), "short");
        assert_eq!(fit_tail("abcdefghij", 8), "...fghij");
        assert_eq!(
            fit_tail("abcdefghij", 3),
            "abcdefghij",
            "no room to cut: left whole"
        );
        assert_eq!(fit_tail("abcdefghij", 8).chars().count(), 8);
    }

    #[test]
    fn saving_sends_the_trimmed_bio_even_an_empty_one() {
        let mut panel = Panel::new();
        assert_eq!(panel.save(), PanelAction::Save { bio: String::new() });
        panel.bio = " hi \n".to_owned();
        assert_eq!(
            panel.save(),
            PanelAction::Save {
                bio: "hi".to_owned()
            }
        );
        assert!(panel.message.is_empty());
    }

    #[test]
    fn the_hubs_copy_fills_the_bio_until_the_player_types() {
        let mut panel = Panel::new();
        let mut state = snapshot(Status::Online);
        state.me = Some(me("Sol", "about"));
        panel.sync(&inputs(Some(&state)));
        assert_eq!(panel.bio, "about");
        // The player edits: the hub's copy no longer replaces the draft.
        panel.bio = "about me".to_owned();
        panel.edited = true;
        panel.sync(&inputs(Some(&state)));
        assert_eq!(panel.bio, "about me");
        // Once the hub has the same text, the draft follows the hub again.
        state.me = Some(me("Sol", "about me"));
        panel.sync(&inputs(Some(&state)));
        assert!(!panel.edited);
        state.me = Some(me("Sol", "changed by the operator"));
        panel.sync(&inputs(Some(&state)));
        assert_eq!(panel.bio, "changed by the operator");
    }

    #[test]
    fn fields_exist_only_with_the_identity_on_and_a_running_service() {
        let mut panel = Panel::new();
        panel.focus = Focus::Bio;
        panel.sync(&Inputs {
            enabled: false,
            ..inputs(None)
        });
        assert!(!panel.fields);
        assert_eq!(
            panel.focus,
            Focus::Toggle,
            "the focus leaves a hidden field"
        );
        let state = snapshot(Status::Online);
        panel.sync(&inputs(Some(&state)));
        assert!(panel.fields && panel.enabled);
    }

    #[test]
    fn enter_on_the_switch_flips_it_and_on_copy_copies() {
        let mut panel = Panel::new();
        panel.enabled = true;
        panel.fields = true;
        assert_eq!(panel.activate(), PanelAction::SetEnabled(false));
        panel.focus = Focus::Copy;
        assert_eq!(panel.activate(), PanelAction::CopyKeyId);
    }
}

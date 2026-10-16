//! The Profile page: the player's SJK profile as others see it on the hub (picture,
//! name, verified flag, names worn, medals and bio), their own record from the
//! achievement counts, and the way to their achievements and collection
//! (`docs/identity.md`, "Profile", "Pictures", "Medals" and "Achievements"). The bio is written here, under
//! the hub's rules (`sjk_identity::bio`): what cannot be in a bio cannot be typed, and
//! what the hub sends back is shown only through those rules.
//!
//! The picture is changed here too: the player's picture on the left opens the picture
//! panel in the bio's place. A picture file chosen with Browse... (the system's file
//! dialog, opened on a worker thread), dropped on the window or named to the
//! `sjkavatar` command is read on a worker thread, checked, cropped and scaled
//! (`avatars::picture`) and shown as a preview; Use this picture sends it to the hub.
//!
//! In the SJK UI it is the Profile screen's SJK Profile tab ([`crate::profile_hub`],
//! [`Mode::Hub`]); its See the board and See the collection open the Collection screen
//! (`collection_panel`). In the classic menus the `profile` command opens it on its own,
//! the medals on the profile ([`Mode::Pages`]), and its buttons open the Collection page
//! on its own. Like the Identity page it lives in the console and is drawn in place of
//! it, always in the SJK UI's look ([`view`]). Tab moves between the picture, the bio
//! and their buttons; Enter saves the bio from the field (Shift+Enter starts a new
//! line); Escape goes back.

use crate::achievements::Standing;
use crate::avatars::picture::{PictureError, Prepared};
use crate::menu_widgets::{BACK_TOKEN, MenuCanvas};
use crate::profile_hub::Tab as HubTab;
use sjk_identity::bio;
use sjk_identity::{Snapshot, Status};
use sjk_ui::{InputEvent, UiEventKind};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[path = "profile_panel_view.rs"]
mod view;

const BIO_TOKEN: u16 = 1_010;
const SAVE_TOKEN: u16 = 1_011;
const REVERT_TOKEN: u16 = 1_012;
const BOARD_TOKEN: u16 = 1_013;
const STAFF_TOKEN: u16 = 1_015;
const UNLOCKABLES_TOKEN: u16 = 1_016;
const HOLOCRONS_TOKEN: u16 = 1_022;
const PICTURE_TOKEN: u16 = 1_017;
const USE_TOKEN: u16 = 1_018;
const REMOVE_TOKEN: u16 = 1_019;
const BIO_BACK_TOKEN: u16 = 1_020;
const BROWSE_TOKEN: u16 = 1_021;
/// How long Remove picture waits for its second press.
const REMOVE_CONFIRM: Duration = Duration::from_secs(3);

/// How the page was opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    /// On its own (the `profile` command in the classic menus): the medals on the
    /// profile.
    Pages,
    /// The Profile screen's SJK Profile tab ([`crate::profile_hub`]): the screen's row
    /// of tabs under the title; the medals are the Collection's.
    Hub,
}

/// The control the keyboard is on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    /// The player's picture, which opens the picture panel.
    Picture,
    Staff,
    Bio,
    Save,
    Revert,
    /// The picture panel's Browse..., Use this picture, Remove picture and Done.
    Browse,
    UsePicture,
    RemovePicture,
    BioBack,
    Board,
    Unlockables,
    Holocrons,
}

/// What the Profile tab's middle column shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Middle {
    Bio,
    Picture,
}

/// A picture file being made ready on a worker thread.
struct Reading {
    /// The file's name, as the panel says it.
    name: String,
    result: Receiver<Result<Prepared, PictureError>>,
}

/// A change of picture on its way to the hub.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Changing {
    /// The service's last picture outcome before it, to tell the hub's answer.
    serial_before: u64,
    /// Taking the picture down rather than sending one.
    removing: bool,
}

/// What the console does after the page handled an event.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PanelAction {
    None,
    Close,
    /// Send the bio to the hub.
    Save {
        bio: String,
    },
    /// Show a tab of the Collection screen (on its own in the classic menus).
    Hub(HubTab),
    /// Open the Staff page.
    Staff,
    /// Send this PNG to the hub as the player's picture.
    SetAvatar {
        png: Vec<u8>,
    },
    /// Take the player's picture down at the hub.
    RemoveAvatar,
}

/// A save on its way: the bio sent and the service's notice before it, to tell the
/// hub's answer from an older one.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Saving {
    bio: String,
    notice_before: Option<String>,
}

/// What the page knows about the identity, the hub and the counts.
pub(crate) struct Inputs<'a> {
    /// `cl_identity`.
    pub(crate) enabled: bool,
    pub(crate) snapshot: Option<&'a Snapshot>,
    /// Every achievement as the board shows it.
    pub(crate) standings: &'a [Standing],
    /// The player's record: label and value, in order.
    pub(crate) record: &'a [(&'static str, String)],
}

pub(crate) struct Panel {
    open: bool,
    owns_console: bool,
    ui: MenuCanvas,
    mode: Mode,
    focus: Focus,
    /// The bio being written.
    bio: String,
    /// Something was typed since the hub's copy was taken.
    edited: bool,
    /// The hub's copy the draft was last taken from.
    taken: Option<String>,
    saving: Option<Saving>,
    /// A problem found before anything was sent, or the hub's answer.
    message: String,
    /// The bio can be written: the identity is on and the hub answered.
    writable: bool,
    /// The hub made the player's key staff: Staff tools are offered.
    staff: bool,
    /// The Profile tab's middle column: the bio or the picture panel.
    middle: Middle,
    /// A picture file being read, the picture ready to send (its file's name and PNG)
    /// and a change on its way.
    reading: Option<Reading>,
    ready: Option<(String, Vec<u8>)>,
    changing: Option<Changing>,
    /// The service's last picture outcome, by serial.
    avatar_serial: u64,
    /// The hub holds a picture of the player.
    has_picture: bool,
    /// What the picture panel says last, and whether it is good news.
    picture_message: Option<(String, bool)>,
    /// When Remove picture was pressed once, waiting for the second press.
    remove_armed: Option<Instant>,
    /// The system's file dialog open on a worker thread: the file it gives, or
    /// nothing when it was closed without one.
    browsing: Option<Receiver<Option<PathBuf>>>,
    /// What opens the file dialog ([`pick_picture`]; tests put their own).
    picker: fn() -> Option<PathBuf>,
    /// What the Profile screen's tabs put at the top ([`Mode::Hub`]).
    hub_header: crate::profile_hub::Header,
    epoch: Instant,
    /// What a world shot shows in place of the live identity and counts.
    #[cfg(test)]
    pub(crate) preview: Option<Preview>,
}

/// A made-up identity and counts for a world shot.
#[cfg(test)]
pub(crate) struct Preview {
    pub(crate) snapshot: Snapshot,
    pub(crate) standings: Vec<Standing>,
    pub(crate) record: Vec<(&'static str, String)>,
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

/// Ask the player for a picture file with the system's file dialog, listing PNG, JPEG
/// and TGA files; it blocks until the dialog closes, so it runs on a worker thread.
fn pick_picture() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose your SJK picture")
        .add_filter("Pictures (PNG, JPEG, TGA)", &["png", "jpg", "jpeg", "tga"])
        .pick_file()
}

/// Add `text` to the bio `field` as far as the rules allow: characters a bio cannot
/// hold are left out, tabs become spaces, a line break only while lines are left, and
/// nothing past [`bio::BIO_MAX`] characters. Whether the field changed.
fn type_into(field: &mut String, text: &str) -> bool {
    let mut changed = false;
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for c in text.chars() {
        if field.chars().count() >= bio::BIO_MAX {
            break;
        }
        let c = if c == '\t' { ' ' } else { c };
        let fits = match c {
            '\n' => field.split('\n').count() < bio::LINES_MAX,
            '^' => true,
            c => bio::allowed(c),
        };
        if fits {
            field.push(c);
            changed = true;
        }
    }
    changed
}

impl Panel {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            owns_console: false,
            ui: MenuCanvas::with_capacities(240, 192, 1_000),
            mode: Mode::Pages,
            focus: Focus::Picture,
            bio: String::new(),
            edited: false,
            taken: None,
            saving: None,
            message: String::new(),
            writable: false,
            staff: false,
            middle: Middle::Bio,
            reading: None,
            ready: None,
            changing: None,
            avatar_serial: 0,
            has_picture: false,
            picture_message: None,
            remove_armed: None,
            browsing: None,
            picker: pick_picture,
            hub_header: crate::profile_hub::Header::default(),
            epoch: Instant::now(),
            #[cfg(test)]
            preview: None,
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Show the page on its own; `owns_console` when the console was closed before it.
    #[cfg(test)]
    pub(crate) fn open(&mut self, owns_console: bool) {
        self.open_as(owns_console, Mode::Pages);
    }

    /// Show the page as `mode` says ([`Mode`]).
    pub(crate) fn open_as(&mut self, owns_console: bool, mode: Mode) {
        self.open = true;
        self.owns_console = owns_console;
        self.mode = mode;
        self.focus = Focus::Picture;
        self.message.clear();
        if self.reading.is_none() && self.ready.is_none() && self.changing.is_none() {
            self.middle = Middle::Bio;
        }
    }

    /// What the Profile screen's tabs put at the top.
    pub(crate) fn hub_header(&self) -> &crate::profile_hub::Header {
        &self.hub_header
    }

    /// Put `header` at the top on the Profile screen.
    pub(crate) fn set_hub_header(&mut self, header: crate::profile_hub::Header) {
        self.hub_header = header;
    }

    /// Hide the page; returns whether it had opened the console. A picture read but
    /// not sent is dropped; a file dialog still open gives its file when the page
    /// shows again.
    pub(crate) fn close(&mut self) -> bool {
        let owned = self.open && self.owns_console;
        self.open = false;
        self.owns_console = false;
        if self.ready.take().is_some() {
            crate::avatars::set_preview(None);
        }
        self.reading = None;
        self.picture_message = None;
        owned
    }

    /// Show the picture panel in the bio's place.
    pub(crate) fn show_picture(&mut self) {
        self.middle = Middle::Picture;
        self.remove_armed = None;
    }

    /// Read the picture file at `path` on a worker thread and show it, once ready, as
    /// the picture about to be sent (a file chosen with Browse... or dropped on the
    /// window, `sjkavatar <file>`).
    pub(crate) fn load_picture(&mut self, path: std::path::PathBuf) {
        self.show_picture();
        self.focus = Focus::UsePicture;
        if self.ready.take().is_some() {
            crate::avatars::set_preview(None);
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (outbox, result) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("sjk-picture".to_owned())
            .spawn(move || {
                let _ = outbox.send(crate::avatars::picture::prepare_file(&path));
            });
        match spawned {
            Ok(_) => {
                self.picture_message = None;
                self.reading = Some(Reading { name, result });
            }
            Err(error) => {
                self.picture_message = Some((format!("Cannot read pictures now: {error}"), false));
            }
        }
    }

    /// Take the picture down at the hub (`sjkavatar clear`), without the second press.
    pub(crate) fn remove_picture_now(&mut self) -> PanelAction {
        self.show_picture();
        self.focus = Focus::RemovePicture;
        self.start_change(true)
    }

    /// Browse...: open the system's file dialog on a worker thread; the file chosen is
    /// read as a dropped one is. Nothing happens while one is open.
    fn browse(&mut self) {
        self.focus = Focus::Browse;
        if self.browsing.is_some() {
            return;
        }
        let picker = self.picker;
        let (outbox, result) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("sjk-file-dialog".to_owned())
            .spawn(move || {
                let _ = outbox.send(picker());
            });
        match spawned {
            Ok(_) => self.browsing = Some(result),
            Err(error) => {
                self.picture_message =
                    Some((format!("Cannot open the file dialog: {error}"), false));
            }
        }
    }

    /// Whether the file dialog is open.
    fn is_browsing(&self) -> bool {
        self.browsing.is_some()
    }

    /// Take in the file the dialog gave, once it has closed.
    fn sync_browse(&mut self) {
        let Some(browsing) = &self.browsing else {
            return;
        };
        match browsing.try_recv() {
            Ok(Some(path)) => {
                self.browsing = None;
                self.load_picture(path);
            }
            Ok(None) => self.browsing = None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.browsing = None;
                self.picture_message =
                    Some(("The file dialog closed without a file".to_owned(), false));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Say why a picture change could not be sent at all.
    pub(crate) fn picture_failed(&mut self, why: &str) {
        self.changing = None;
        self.picture_message = Some((why.to_owned(), false));
    }

    /// What the middle column shows.
    #[cfg(test)]
    pub(crate) fn middle(&self) -> Middle {
        self.middle
    }

    /// Start sending the ready picture (`removing` false) or taking the picture down.
    fn start_change(&mut self, removing: bool) -> PanelAction {
        self.remove_armed = None;
        if !self.writable {
            self.picture_message = Some((
                "Pictures need the SJK identity on and the SJK hub answering".to_owned(),
                false,
            ));
            return PanelAction::None;
        }
        if self.changing.is_some() {
            return PanelAction::None;
        }
        let action = if removing {
            PanelAction::RemoveAvatar
        } else {
            match &self.ready {
                Some((_, png)) => PanelAction::SetAvatar { png: png.clone() },
                None => return PanelAction::None,
            }
        };
        self.changing = Some(Changing {
            serial_before: self.avatar_serial,
            removing,
        });
        self.picture_message = None;
        action
    }

    /// Remove picture: the first press asks for a second within [`REMOVE_CONFIRM`].
    fn press_remove(&mut self) -> PanelAction {
        if !self.writable || !self.has_picture {
            return PanelAction::None;
        }
        match self.remove_armed {
            Some(at) if at.elapsed() < REMOVE_CONFIRM => self.start_change(true),
            _ => {
                self.remove_armed = Some(Instant::now());
                self.picture_message = Some((
                    "Press Remove picture again to take your picture down".to_owned(),
                    false,
                ));
                PanelAction::None
            }
        }
    }

    /// Escape: from the picture panel back to the bio first, else close the page.
    fn escape(&mut self) -> PanelAction {
        if self.middle == Middle::Picture {
            self.back_to_bio();
            PanelAction::None
        } else {
            PanelAction::Close
        }
    }

    /// How the page was opened.
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    /// Back from the picture panel to the bio, dropping a picture read but not sent.
    fn back_to_bio(&mut self) {
        self.middle = Middle::Bio;
        self.focus = Focus::Picture;
        self.remove_armed = None;
        if self.ready.take().is_some() {
            crate::avatars::set_preview(None);
        }
        self.reading = None;
        self.picture_message = None;
    }

    /// Take in a picture the worker finished reading, and the hub's answer to a change.
    fn sync_picture(&mut self, inputs: &Inputs<'_>) {
        self.sync_browse();
        if let Some(reading) = &self.reading
            && let Ok(result) = reading.result.try_recv()
        {
            let name = self
                .reading
                .take()
                .map(|reading| reading.name)
                .unwrap_or_default();
            match result {
                Ok(prepared) => {
                    crate::avatars::set_preview(Some(&prepared.rgba));
                    self.ready = Some((name, prepared.png));
                    self.picture_message = Some((
                        "This is how it will look. Use this picture to show it to everyone."
                            .to_owned(),
                        true,
                    ));
                }
                Err(error) => {
                    crate::avatars::set_preview(None);
                    self.picture_message = Some((error.to_string(), false));
                }
            }
        }
        let snapshot = inputs.snapshot;
        let me = snapshot.and_then(|snapshot| snapshot.me.as_ref());
        self.has_picture = me.is_some_and(|me| !me.avatar.is_empty());
        let outcome = snapshot.and_then(|snapshot| snapshot.avatar.as_ref());
        let serial = outcome.map_or(0, |outcome| outcome.serial);
        if let (Some(changing), Some(outcome)) = (self.changing, outcome)
            && serial != changing.serial_before
        {
            self.changing = None;
            if outcome.sent {
                if changing.removing {
                    self.picture_message = Some(("Your picture is gone".to_owned(), true));
                } else {
                    if let Some(me) = me {
                        crate::avatars::adopt_preview(&me.key_id, &me.avatar);
                    }
                    self.ready = None;
                    self.picture_message =
                        Some(("Saved. Everyone sees your new picture.".to_owned(), true));
                }
            } else {
                let why = if outcome.message.contains("not connected") {
                    "Not sent: the SJK hub is not answering (is the identity on?)".to_owned()
                } else {
                    format!("Not sent: {}", outcome.message)
                };
                self.picture_message = Some((why, false));
            }
        }
        self.avatar_serial = serial;
        if !self.writable {
            // A change cannot be answered without the hub.
            self.changing = None;
        }
    }

    pub(crate) fn draw_list(&self) -> &sjk_ui::DrawList {
        self.ui.draw_list()
    }

    /// Whether the last frame ran out of room on the canvas.
    #[cfg(test)]
    pub(crate) fn overflowed(&self) -> bool {
        self.ui.overflowed()
    }

    /// Take in what the service knows: the hub's bio replaces an untouched draft,
    /// and a save's answer becomes the message.
    fn sync(&mut self, inputs: &Inputs<'_>) {
        let me = inputs
            .snapshot
            .filter(|snapshot| snapshot.status == Status::Online)
            .and_then(|snapshot| snapshot.me.as_ref());
        self.writable = inputs.enabled && me.is_some();
        self.staff = self.writable && me.is_some_and(|me| me.staff);
        self.sync_picture(inputs);
        if !self.staff && self.focus == Focus::Staff {
            self.focus = Focus::Picture;
        }
        if !self.writable && matches!(self.focus, Focus::Bio | Focus::Save | Focus::Revert) {
            self.focus = Focus::Picture;
        }
        let Some(me) = me else {
            return;
        };
        let hub = bio::for_display(&me.bio);
        if self.taken.as_ref() != Some(&hub) {
            if !self.edited {
                self.bio.clone_from(&hub);
            }
            self.taken = Some(hub.clone());
        }
        if let Some(saving) = &self.saving {
            let notice = inputs.snapshot.and_then(|snapshot| snapshot.notice.clone());
            if hub == saving.bio {
                self.message = "Saved".to_owned();
                self.saving = None;
                self.edited = false;
            } else if notice != saving.notice_before
                && let Some(notice) = notice.filter(|notice| notice != "saved")
            {
                self.message = notice;
                self.saving = None;
            }
        }
    }

    /// Send the bio as written, once it passes the rules.
    fn save(&mut self, notice_before: Option<String>) -> PanelAction {
        if !self.writable {
            return PanelAction::None;
        }
        match bio::check(&self.bio) {
            Ok(tidied) => {
                self.message = "Saving...".to_owned();
                self.saving = Some(Saving {
                    bio: tidied.clone(),
                    notice_before,
                });
                PanelAction::Save { bio: tidied }
            }
            Err(error) => {
                self.message = error.to_string();
                PanelAction::None
            }
        }
    }

    /// Put back the hub's copy of the bio.
    fn revert(&mut self) {
        if let Some(hub) = &self.taken {
            self.bio.clone_from(hub);
        }
        self.edited = false;
        self.message.clear();
    }

    /// The controls Tab visits on this tab, in order.
    fn order(&self) -> Vec<Focus> {
        // On the Profile screen its tabs are the screen's (Ctrl+Tab), not a stop.
        let mut order = vec![Focus::Picture];
        if self.staff {
            order.push(Focus::Staff);
        }
        match (self.middle, self.writable) {
            (Middle::Bio, true) => order.extend([Focus::Bio, Focus::Save, Focus::Revert]),
            (Middle::Bio, false) => {}
            (Middle::Picture, true) => {
                order.extend([
                    Focus::Browse,
                    Focus::UsePicture,
                    Focus::RemovePicture,
                    Focus::BioBack,
                ]);
            }
            // A picture can still be chosen and looked at without the hub.
            (Middle::Picture, false) => order.extend([Focus::Browse, Focus::BioBack]),
        }
        order.extend([Focus::Board, Focus::Unlockables, Focus::Holocrons]);
        order
    }

    fn step(&mut self, forward: bool) {
        let order = self.order();
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if forward {
            (at + 1) % order.len()
        } else {
            (at + order.len() - 1) % order.len()
        };
        self.focus = order[next];
    }

    /// What Enter (or a click) does on the focused control.
    fn activate(&mut self, notice: Option<String>) -> PanelAction {
        match self.focus {
            Focus::Picture => {
                self.show_picture();
                PanelAction::None
            }
            Focus::Staff => PanelAction::Staff,
            Focus::Bio | Focus::Save => self.save(notice),
            Focus::Revert => {
                self.revert();
                PanelAction::None
            }
            Focus::Browse => {
                self.browse();
                PanelAction::None
            }
            Focus::UsePicture => self.start_change(false),
            Focus::RemovePicture => self.press_remove(),
            Focus::BioBack => {
                self.back_to_bio();
                PanelAction::None
            }
            Focus::Board => self.see_board(),
            Focus::Unlockables => self.see_unlockables(),
            Focus::Holocrons => self.see_holocrons(),
        }
    }

    /// See the board: the Collection's Achievements.
    fn see_board(&mut self) -> PanelAction {
        PanelAction::Hub(HubTab::Achievements)
    }

    /// See the collection: the Collection's Medals.
    fn see_unlockables(&self) -> PanelAction {
        PanelAction::Hub(HubTab::Medals)
    }

    /// See the holocrons: the Collection's Holocrons.
    fn see_holocrons(&self) -> PanelAction {
        PanelAction::Hub(HubTab::Holocrons)
    }

    /// A key; `notice` is the service's last notice, for a save.
    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        shift: bool,
        control: bool,
        notice: Option<String>,
    ) -> PanelAction {
        if event.state != ElementState::Pressed {
            return PanelAction::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PanelAction::None;
        };
        let typing = self.focus == Focus::Bio && self.writable;
        if typing
            && !event.repeat
            && crate::text_entry::before_edit(
                &mut self.bio,
                key,
                event.text.as_deref(),
                shift,
                control,
            )
        {
            self.edited = true;
            return PanelAction::None;
        }
        match key {
            KeyCode::Escape => return self.escape(),
            KeyCode::Tab => self.step(!shift),
            KeyCode::ArrowDown if !typing => self.step(true),
            KeyCode::ArrowUp if !typing => self.step(false),
            KeyCode::Enter | KeyCode::NumpadEnter if typing && shift => {
                if type_into(&mut self.bio, "\n") {
                    self.edited = true;
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter => return self.activate(notice),
            KeyCode::Space if !typing => return self.activate(notice),
            KeyCode::Backspace if typing => {
                if self.bio.pop().is_some() {
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
                if type_into(&mut self.bio, text) {
                    self.edited = true;
                }
            }
            _ => {}
        }
        PanelAction::None
    }

    /// A pointer event; `notice` as for [`Self::handle_key`].
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        notice: Option<String>,
    ) -> PanelAction {
        let Some(event) = self.ui.pointer(event) else {
            return PanelAction::None;
        };
        if event.kind != UiEventKind::Activate {
            return PanelAction::None;
        }
        match event.token {
            Some(BACK_TOKEN) => PanelAction::Close,
            Some(BIO_TOKEN) if self.writable => {
                self.focus = Focus::Bio;
                PanelAction::None
            }
            Some(SAVE_TOKEN) if self.writable => {
                self.focus = Focus::Save;
                self.save(notice)
            }
            Some(REVERT_TOKEN) if self.writable => {
                self.focus = Focus::Revert;
                self.revert();
                PanelAction::None
            }
            Some(STAFF_TOKEN) if self.staff => {
                self.focus = Focus::Staff;
                PanelAction::Staff
            }
            Some(BOARD_TOKEN) => {
                self.focus = Focus::Board;
                self.see_board()
            }
            Some(PICTURE_TOKEN) => {
                self.focus = Focus::Picture;
                self.show_picture();
                PanelAction::None
            }
            Some(BROWSE_TOKEN) if self.middle == Middle::Picture => {
                self.browse();
                PanelAction::None
            }
            Some(USE_TOKEN) if self.middle == Middle::Picture => {
                self.focus = Focus::UsePicture;
                self.start_change(false)
            }
            Some(REMOVE_TOKEN) if self.middle == Middle::Picture => {
                self.focus = Focus::RemovePicture;
                self.press_remove()
            }
            Some(BIO_BACK_TOKEN) if self.middle == Middle::Picture => {
                self.back_to_bio();
                PanelAction::None
            }
            Some(UNLOCKABLES_TOKEN) => {
                self.focus = Focus::Unlockables;
                self.see_unlockables()
            }
            Some(HOLOCRONS_TOKEN) => {
                self.focus = Focus::Holocrons;
                self.see_holocrons()
            }
            _ => PanelAction::None,
        }
    }

    /// Put the keyboard on the bio and type `text` there, for a world shot.
    #[cfg(test)]
    pub(crate) fn type_for_shot(&mut self, text: &str) {
        self.writable = true;
        self.focus = Focus::Bio;
        self.bio.clear();
        type_into(&mut self.bio, text);
        self.edited = true;
    }

    fn focus_token(&self) -> u16 {
        match self.focus {
            Focus::Picture => PICTURE_TOKEN,
            Focus::Browse => BROWSE_TOKEN,
            Focus::UsePicture => USE_TOKEN,
            Focus::RemovePicture => REMOVE_TOKEN,
            Focus::BioBack => BIO_BACK_TOKEN,
            Focus::Staff => STAFF_TOKEN,
            Focus::Bio => BIO_TOKEN,
            Focus::Save => SAVE_TOKEN,
            Focus::Revert => REVERT_TOKEN,
            Focus::Board => BOARD_TOKEN,
            Focus::Unlockables => UNLOCKABLES_TOKEN,
            Focus::Holocrons => HOLOCRONS_TOKEN,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sjk_identity::Profile;
    use std::collections::HashMap;

    pub(crate) fn me(bio: &str) -> Profile {
        Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "0123456789abcdef".to_owned(),
            key: String::new(),
            name: "^1Sol".to_owned(),
            bio: bio.to_owned(),
            verified: true,
            staff: false,
            created: 1_759_708_800,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        }
    }

    pub(crate) fn snapshot(me: Option<Profile>, notice: Option<&str>) -> Snapshot {
        Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            local_key_id: "0123456789abcdef".to_owned(),
            status: Status::Online,
            key_id: "0123456789abcdef".to_owned(),
            me,
            server: None,
            players: Vec::new(),
            profiles: HashMap::new(),
            model_checks: HashMap::new(),
            profiles_revision: 0,
            notice: notice.map(str::to_owned),
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
            snapshot,
            standings: &[],
            record: &[],
        }
    }

    fn opened(snapshot: &Snapshot) -> Panel {
        let mut panel = Panel::new();
        panel.open(true);
        panel.sync(&inputs(Some(snapshot)));
        panel
    }

    #[test]
    fn typing_keeps_to_what_a_bio_may_hold() {
        let mut field = String::new();
        assert!(type_into(&mut field, "Hi \u{221E}\u{200B}there\u{202E}!"));
        assert_eq!(field, "Hi there!");
        assert!(!type_into(&mut field, "\u{221E}"));
        type_into(&mut field, "\tok\r\nnext");
        assert_eq!(field, "Hi there! ok\nnext");
        // Six lines at most.
        let mut lines = String::new();
        type_into(&mut lines, "1\n2\n3\n4\n5\n6\n7");
        assert_eq!(lines, "1\n2\n3\n4\n5\n67");
        let mut long = String::new();
        type_into(&mut long, &"ab".repeat(400));
        assert_eq!(long.chars().count(), bio::BIO_MAX);
    }

    #[test]
    fn the_hubs_bio_fills_an_untouched_draft_shown_through_the_rules() {
        let shot = snapshot(Some(me("hello\u{202E} there")), None);
        let mut panel = opened(&shot);
        assert_eq!(panel.bio, "hello there");
        assert!(panel.writable);
        panel.bio.push('!');
        panel.edited = true;
        let changed = snapshot(Some(me("another")), None);
        panel.sync(&inputs(Some(&changed)));
        assert_eq!(panel.bio, "hello there!", "a draft being written stays");
        panel.revert();
        assert_eq!(panel.bio, "another");
    }

    #[test]
    fn saving_sends_the_tidied_bio_and_shows_the_answer() {
        let shot = snapshot(Some(me("")), Some("saved"));
        let mut panel = opened(&shot);
        panel.bio = "  two   spaces  ".to_owned();
        panel.edited = true;
        assert_eq!(
            panel.save(Some("saved".to_owned())),
            PanelAction::Save {
                bio: "two spaces".to_owned()
            }
        );
        assert_eq!(panel.message, "Saving...");
        // The same old notice is not the answer; the hub's copy is.
        panel.sync(&inputs(Some(&shot)));
        assert_eq!(panel.message, "Saving...");
        let saved = snapshot(Some(me("two spaces")), Some("saved"));
        panel.sync(&inputs(Some(&saved)));
        assert_eq!(panel.message, "Saved");
        assert!(!panel.edited);
        // A refusal shows the hub's words.
        panel.bio = "x".to_owned();
        let _ = panel.save(Some("saved".to_owned()));
        let refused = snapshot(Some(me("two spaces")), Some("the hub is busy"));
        panel.sync(&inputs(Some(&refused)));
        assert_eq!(panel.message, "the hub is busy");
        // A bio breaking a rule is not sent.
        panel.bio = "aaaaaaaaaaaa".to_owned();
        assert_eq!(panel.save(None), PanelAction::None);
        assert_eq!(panel.message, bio::BioError::Noise.message());
    }

    /// As the Profile screen's SJK Profile tab the page starts on the picture; See the
    /// board and See the collection show the Collection's tabs, and Escape closes it.
    #[test]
    fn on_the_profile_screen_the_buttons_show_its_tabs_and_escape_closes() {
        let shot = snapshot(Some(me("")), None);
        let mut panel = Panel::new();
        panel.open_as(true, Mode::Hub);
        panel.sync(&inputs(Some(&shot)));
        assert_eq!(panel.focus, Focus::Picture);
        panel.focus = Focus::Board;
        assert_eq!(panel.activate(None), PanelAction::Hub(HubTab::Achievements));
        panel.focus = Focus::Unlockables;
        assert_eq!(panel.activate(None), PanelAction::Hub(HubTab::Medals));
        panel.focus = Focus::Holocrons;
        assert_eq!(panel.activate(None), PanelAction::Hub(HubTab::Holocrons));
        assert_eq!(panel.escape(), PanelAction::Close);
        // On its own the page shows the Collection too.
        let mut alone = opened(&shot);
        alone.focus = Focus::Board;
        assert_eq!(alone.activate(None), PanelAction::Hub(HubTab::Achievements));
    }

    /// Browse... opens the file dialog on a worker thread, once while it is open; the
    /// file it gives is read as a dropped one, and closing it without a file changes
    /// nothing.
    #[test]
    fn browse_reads_the_file_the_dialog_gives() {
        static FILE: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);
        static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        fn picker() -> Option<std::path::PathBuf> {
            // Held open while the test holds the gate.
            let _open = GATE.lock().unwrap();
            FILE.lock().unwrap().clone()
        }
        let dir = tempfile::tempdir().unwrap();
        let shot = snapshot(Some(me("")), None);
        let mut panel = opened(&shot);
        panel.picker = picker;
        panel.show_picture();
        assert!(panel.order().contains(&Focus::Browse));
        // Closed without a file.
        panel.focus = Focus::Browse;
        assert_eq!(panel.activate(None), PanelAction::None);
        for _ in 0..500 {
            panel.sync(&inputs(Some(&shot)));
            if !panel.is_browsing() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!panel.is_browsing() && panel.reading.is_none() && panel.ready.is_none());
        // A file chosen: a second Browse while the dialog is open does nothing.
        *FILE.lock().unwrap() = Some(picture_file(dir.path(), 300, 200));
        let gate = GATE.lock().unwrap();
        let _ = panel.activate(None);
        assert!(panel.is_browsing());
        let first = panel.browsing.as_ref().map(|receiver| receiver as *const _);
        let _ = panel.activate(None);
        assert_eq!(
            panel.browsing.as_ref().map(|receiver| receiver as *const _),
            first,
            "one dialog at a time"
        );
        drop(gate);
        for _ in 0..500 {
            panel.sync(&inputs(Some(&shot)));
            if !panel.is_browsing() && panel.reading.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let (name, _) = panel.ready.clone().expect("the chosen file, ready to send");
        assert_eq!(name, "me.png");
        assert_eq!(panel.focus, Focus::UsePicture);
    }

    #[test]
    fn without_the_hub_nothing_can_be_written() {
        let mut panel = Panel::new();
        panel.open(false);
        panel.sync(&Inputs {
            enabled: false,
            ..inputs(None)
        });
        assert!(!panel.writable);
        assert_eq!(
            panel.order(),
            [
                Focus::Picture,
                Focus::Board,
                Focus::Unlockables,
                Focus::Holocrons
            ]
        );
        panel.bio = "hello there".to_owned();
        assert_eq!(panel.save(None), PanelAction::None);
    }

    #[test]
    fn tab_walks_the_controls_and_enter_opens_the_collection() {
        let shot = snapshot(Some(me("")), None);
        let mut panel = opened(&shot);
        let steps: Vec<Focus> = (0..7)
            .map(|_| {
                panel.step(true);
                panel.focus
            })
            .collect();
        assert_eq!(
            steps,
            [
                Focus::Bio,
                Focus::Save,
                Focus::Revert,
                Focus::Board,
                Focus::Unlockables,
                Focus::Holocrons,
                Focus::Picture
            ]
        );
        panel.focus = Focus::Unlockables;
        assert_eq!(panel.activate(None), PanelAction::Hub(HubTab::Medals));
        panel.focus = Focus::Holocrons;
        assert_eq!(panel.activate(None), PanelAction::Hub(HubTab::Holocrons));
        panel.focus = Focus::Board;
        assert_eq!(panel.activate(None), PanelAction::Hub(HubTab::Achievements));
    }

    /// A PNG file of `width` x `height` in `dir`.
    fn picture_file(dir: &std::path::Path, width: u32, height: u32) -> std::path::PathBuf {
        let path = dir.join("me.png");
        image::RgbaImage::from_pixel(width, height, image::Rgba([30, 60, 90, 255]))
            .save(&path)
            .unwrap();
        path
    }

    /// Sync until the worker has read the picture.
    fn wait_for_picture(panel: &mut Panel, snapshot: &Snapshot) {
        for _ in 0..500 {
            panel.sync(&inputs(Some(snapshot)));
            if panel.reading.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the picture was not read");
    }

    fn outcome(serial: u64, sent: bool, message: &str) -> Option<sjk_identity::ReportOutcome> {
        Some(sjk_identity::ReportOutcome {
            serial,
            sent,
            message: message.to_owned(),
        })
    }

    #[test]
    fn a_dropped_picture_is_shown_then_sent_and_the_hubs_answer_said() {
        let dir = tempfile::tempdir().unwrap();
        let shot = snapshot(Some(me("")), None);
        let mut panel = opened(&shot);
        panel.load_picture(picture_file(dir.path(), 300, 200));
        assert_eq!(panel.middle(), Middle::Picture);
        wait_for_picture(&mut panel, &shot);
        let (name, png) = panel.ready.clone().expect("ready to send");
        assert_eq!(name, "me.png");
        let sent = image::load_from_memory(&png).unwrap();
        assert_eq!(
            (sent.width(), sent.height()),
            (sjk_identity::avatar::SIZE, sjk_identity::avatar::SIZE)
        );
        assert!(
            panel
                .picture_message
                .as_ref()
                .is_some_and(|(_, good)| *good)
        );
        // Use this picture sends it, once.
        panel.focus = Focus::UsePicture;
        assert_eq!(panel.activate(None), PanelAction::SetAvatar { png });
        assert_eq!(
            panel.activate(None),
            PanelAction::None,
            "already on its way"
        );
        // The hub took it: the profile has its version.
        let mut saved = snapshot(
            Some(Profile {
                avatar: "0123456789abcdef".to_owned(),
                ..me("")
            }),
            None,
        );
        saved.avatar = outcome(1, true, "saved");
        panel.sync(&inputs(Some(&saved)));
        assert_eq!(
            panel.picture_message,
            Some(("Saved. Everyone sees your new picture.".to_owned(), true))
        );
        assert!(panel.ready.is_none() && panel.changing.is_none());
        assert!(panel.has_picture);
        // Remove picture asks for a second press, then takes it down.
        panel.focus = Focus::RemovePicture;
        assert_eq!(panel.activate(None), PanelAction::None);
        assert_eq!(panel.activate(None), PanelAction::RemoveAvatar);
        let mut refused = saved.clone();
        refused.avatar = outcome(2, false, "you changed your picture as often as you may");
        panel.sync(&inputs(Some(&refused)));
        let (message, good) = panel.picture_message.clone().unwrap();
        assert!(
            !good && message.contains("as often as you may"),
            "{message}"
        );
        // Escape leaves the picture panel for the bio before it leaves the page.
        assert_eq!(panel.escape(), PanelAction::None);
        assert_eq!(panel.middle(), Middle::Bio);
        assert_eq!(panel.escape(), PanelAction::Close);
    }

    #[test]
    fn a_file_that_is_no_picture_says_why_and_nothing_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let shot = snapshot(Some(me("")), None);
        let mut panel = opened(&shot);
        let text = dir.path().join("notes.png");
        std::fs::write(&text, "not a picture at all").unwrap();
        panel.load_picture(text);
        wait_for_picture(&mut panel, &shot);
        assert!(panel.ready.is_none());
        let (message, good) = panel.picture_message.clone().unwrap();
        assert!(!good && message.contains("not a picture"), "{message}");
        panel.focus = Focus::UsePicture;
        assert_eq!(panel.activate(None), PanelAction::None);
        // Each refusal shows its reason in the panel.
        for (width, height, reason) in [(20, 20, "too small"), (600, 100, "too narrow or too wide")]
        {
            panel.load_picture(picture_file(dir.path(), width, height));
            wait_for_picture(&mut panel, &shot);
            let (message, good) = panel.picture_message.clone().unwrap();
            assert!(!good && message.contains(reason), "{message}");
            assert!(panel.ready.is_none());
        }
        let empty = dir.path().join("empty.jpg");
        std::fs::write(&empty, []).unwrap();
        panel.load_picture(empty);
        wait_for_picture(&mut panel, &shot);
        assert!(panel.picture_message.clone().unwrap().0.contains("empty"));
    }

    #[test]
    fn without_the_hub_a_picture_cannot_be_sent() {
        let dir = tempfile::tempdir().unwrap();
        let mut panel = Panel::new();
        panel.open(true);
        let offline = Inputs {
            enabled: false,
            ..inputs(None)
        };
        panel.sync(&offline);
        panel.load_picture(picture_file(dir.path(), 128, 128));
        for _ in 0..500 {
            panel.sync(&offline);
            if panel.reading.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(panel.ready.is_some(), "it can still be looked at");
        assert_eq!(panel.start_change(false), PanelAction::None);
        assert!(
            panel
                .picture_message
                .clone()
                .unwrap()
                .0
                .contains("identity on")
        );
        assert_eq!(
            panel.order(),
            [
                Focus::Picture,
                Focus::Browse,
                Focus::BioBack,
                Focus::Board,
                Focus::Unlockables,
                Focus::Holocrons
            ]
        );
        // `sjkavatar clear` without the hub says so too.
        assert_eq!(panel.remove_picture_now(), PanelAction::None);
        // Closing the page drops the picture that was not sent.
        panel.close();
        assert!(panel.ready.is_none());
    }
}

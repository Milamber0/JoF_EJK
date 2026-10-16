//! The quick wheel's pages: what each holds and in what order, kept in
//! `wheel.json` in the profile folder (`GameData/SJK/`) once the player changes
//! them in Settings. Without the file the wheel has Force, General, Toys and Weather.
//! The file is JSON rather than cvars because a page is a list and a
//! custom choice a free console command (quotes, semicolons), which a `config.cfg`
//! line would have to escape and whose length a cvar would cap.
//!
//! ```json
//! { "version": 2, "pages": [
//!   { "id": "general", "name": "General", "choices": ["third_person", "hud"] },
//!   { "id": "force", "name": "Force", "kind": "force" },
//!   { "id": "duels", "name": "Duels", "choices": ["duel", {"name": "Ready", "command": "ready"}] }
//! ] }
//! ```
//!
//! A choice is an action of the catalogue by its id ([`super::catalog::ACTIONS`]),
//! or an object naming a console command. A page's `id` is what `+wheel <id>`
//! opens it with; it is made from the page's first name and kept when the page
//! is renamed, so a bind keeps working. The Force page (`"kind": "force"`, one at
//! most) holds no choices of its own: the wheel fills it with the player's Force
//! powers as it opens ([`super::force_page`]).
//!
//! Version 2 brought the Force page: a version 1 file gets it once, after General
//! (else last), unless the wheel already has eight pages ([`WheelPages::load`]).
//! Version 3 made the Force page the first one and added the Toys page (an
//! ordinary page of the catalogue's Toys, Illuminate for now): a file holding
//! exactly the version 2 defaults becomes the new defaults, any other file gets a
//! Toys page once, after its Force page (else last), unless it has eight pages.

use super::catalog::{self, ACTIONS, State};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The file, in the profile folder.
pub(crate) const FILE: &str = "wheel.json";
/// Most pages the wheel has: the dots under its middle stay a short row.
pub(crate) const MAX_PAGES: usize = 8;
/// Most choices a page holds: ten round icons fill the ring with a gap between
/// each, the highlighted one grown, at any window size.
pub(crate) const MAX_CHOICES: usize = 10;
/// Most powers the Force page shows on one ring, its icons drawn a little smaller
/// ([`super::ring`]); the rest go on a second page right after it.
pub(crate) const MAX_FORCE_CHOICES: usize = 12;
/// The file's version: 3 since the Toys page and the Force page first.
const VERSION: u64 = 3;
/// The Force page's id in the defaults.
pub(crate) const FORCE_ID: &str = "force";
/// The Toys page's id in the defaults.
pub(crate) const TOYS_ID: &str = "toys";
/// Longest page name, and longest name of a custom choice, in characters.
pub(crate) const NAME_CHARS: usize = 20;
pub(crate) const LABEL_CHARS: usize = 24;
/// Longest custom console command, in characters.
pub(crate) const COMMAND_CHARS: usize = 200;

/// One choice of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// An action of the catalogue, by its index in [`ACTIONS`].
    Action(usize),
    /// A console command the player named.
    Custom { label: String, command: String },
}

impl Slot {
    /// Its name on the ring.
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Action(index) => ACTIONS[*index].label,
            Self::Custom { label, .. } => label,
        }
    }

    /// The console command it runs.
    pub(crate) fn command(&self) -> &str {
        match self {
            Self::Action(index) => ACTIONS[*index].command,
            Self::Custom { command, .. } => command,
        }
    }

    /// Its picture's index in [`catalog::ICONS`]: a custom choice shows the
    /// custom command's ([`catalog::CUSTOM_ICON`]).
    pub(crate) fn icon(&self) -> Option<usize> {
        match self {
            Self::Action(index) => ACTIONS[*index].icon.and_then(catalog::icon_index),
            Self::Custom { .. } => catalog::icon_index(catalog::CUSTOM_ICON),
        }
    }

    /// Its picture in the UI atlas: the emote's own for `sjkemote <id>` when it has
    /// one (`emote_icons`), else [`Self::icon`]'s.
    pub(crate) fn texture(&self) -> Option<sjk_ui::TextureId> {
        crate::emote_icons::for_command(self.command())
            .or_else(|| self.icon().map(crate::ui_renderer::wheel_icon))
    }

    /// When it counts as in effect.
    pub(crate) fn state(&self) -> State {
        match self {
            Self::Action(index) => ACTIONS[*index].state,
            Self::Custom { .. } => State::None,
        }
    }
}

/// One page of the wheel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Page {
    /// What `+wheel <id>` opens it with: lower-case letters and digits.
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) choices: Vec<Slot>,
    /// The Force page: no choices of its own, the player's powers instead.
    pub(crate) force: bool,
}

impl Page {
    fn of(id: &str, name: &str, actions: &[&str]) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            choices: actions
                .iter()
                .filter_map(|id| catalog::action_index(id))
                .map(Slot::Action)
                .collect(),
            force: false,
        }
    }

    /// The Force page as the defaults have it.
    fn force() -> Self {
        Self {
            id: FORCE_ID.to_owned(),
            name: "Force".to_owned(),
            choices: Vec::new(),
            force: true,
        }
    }
}

/// The pages the wheel has with nothing saved: Force (the first page a new run
/// opens on), General, Toys and Weather.
pub(crate) fn defaults() -> Vec<Page> {
    vec![
        Page::force(),
        Page::of("general", "General", &catalog::GENERAL),
        Page::of(TOYS_ID, "Toys", &catalog::TOYS),
        Page::of("weather", "Weather", &catalog::WEATHER),
    ]
}

/// The pages version 2 had as its defaults: General, Force and Weather. A file
/// holding exactly these was never customised and takes the new defaults.
fn defaults_before_toys() -> Vec<Page> {
    vec![
        Page::of("general", "General", &catalog::GENERAL),
        Page::force(),
        Page::of("weather", "Weather", &catalog::WEATHER),
    ]
}

/// The pages and where they are kept.
#[derive(Debug)]
pub(crate) struct WheelPages {
    path: Option<PathBuf>,
    pages: Vec<Page>,
}

impl WheelPages {
    /// The pages kept in `directory` (the profile folder), or the defaults when
    /// it has none or the file cannot be read. A file from before the Force page
    /// (version 1) gets it once, after General (else last), and is saved so: a
    /// player who then removes it keeps it removed. A file from before the Toys
    /// page (version 2 or older) that holds exactly the old default pages becomes
    /// the new defaults (the file is removed); any other gets a Toys page once,
    /// after its Force page (else last), and is saved as the current version, so
    /// a removed Toys page stays removed. A wheel of eight pages is left as it is
    /// (Settings can add the pages later).
    pub(crate) fn load(directory: &Path) -> Self {
        let path = directory.join(FILE);
        let (pages, version) = match std::fs::read(&path) {
            Ok(bytes) => parse(&bytes).unwrap_or_else(|| {
                crate::log::progress(format_args!(
                    "warning: {} is not a quick wheel file; using the default pages",
                    path.display()
                ));
                (defaults(), VERSION)
            }),
            Err(_) => (defaults(), VERSION),
        };
        let mut pages = Self {
            path: Some(path),
            pages,
        };
        if version < 2 && pages.add_force_page().is_some() {
            crate::log::progress(format_args!("quick wheel: the Force page added"));
        }
        if version < VERSION {
            pages.migrate_to_toys();
        }
        pages
    }

    /// Version 3: the old default pages become the new ones; any other pages get
    /// a Toys page, after the Force page (else last).
    fn migrate_to_toys(&mut self) {
        if self.pages == defaults_before_toys() {
            self.restore_defaults();
            crate::log::progress(format_args!(
                "quick wheel: the default pages updated (Force first, Toys added)"
            ));
        } else if self.add_toys_page().is_some() {
            crate::log::progress(format_args!("quick wheel: the Toys page added"));
        }
    }

    /// Add the default Toys page, after the page of kind Force (else last); its
    /// index, or `None` when the wheel has [`MAX_PAGES`]. Its id is `toys`
    /// unless another page took it.
    fn add_toys_page(&mut self) -> Option<usize> {
        if self.pages.len() >= MAX_PAGES {
            return None;
        }
        let at = self
            .pages
            .iter()
            .position(|page| page.force)
            .map_or(self.pages.len(), |force| force + 1);
        let mut page = Page::of(TOYS_ID, "Toys", &catalog::TOYS);
        page.id = self.unique_id(TOYS_ID, None);
        self.pages.insert(at, page);
        self.save();
        Some(at)
    }

    /// The defaults, kept nowhere (tests).
    #[cfg(test)]
    pub(crate) fn unsaved() -> Self {
        Self {
            path: None,
            pages: defaults(),
        }
    }

    pub(crate) fn pages(&self) -> &[Page] {
        &self.pages
    }

    /// Whether the pages are the defaults.
    pub(crate) fn is_default(&self) -> bool {
        self.pages == defaults()
    }

    /// Whether the wheel has its Force page.
    pub(crate) fn has_force_page(&self) -> bool {
        self.pages.iter().any(|page| page.force)
    }

    /// Put the Force page back: after the page whose id is `general`, else at
    /// the end; its index, or `None` when the wheel has it already or has
    /// [`MAX_PAGES`]. Its id is `force` unless another page took it.
    pub(crate) fn add_force_page(&mut self) -> Option<usize> {
        if self.has_force_page() || self.pages.len() >= MAX_PAGES {
            return None;
        }
        let at = self
            .pages
            .iter()
            .position(|page| page.id == "general")
            .map_or(self.pages.len(), |general| general + 1);
        let mut page = Page::force();
        page.id = self.unique_id(FORCE_ID, None);
        self.pages.insert(at, page);
        self.save();
        Some(at)
    }

    /// The page `name` names: its id, else its name, ignoring case and spaces.
    pub(crate) fn find(&self, name: &str) -> Option<usize> {
        let wanted = squeeze(name);
        if wanted.is_empty() {
            return None;
        }
        self.pages
            .iter()
            .position(|page| page.id == wanted)
            .or_else(|| {
                self.pages
                    .iter()
                    .position(|page| squeeze(&page.name) == wanted)
            })
    }

    /// Add a page named `name` (a fresh "Page n" when blank) at the end; its
    /// index, or `None` when the wheel has [`MAX_PAGES`].
    pub(crate) fn add_page(&mut self, name: &str) -> Option<usize> {
        if self.pages.len() >= MAX_PAGES {
            return None;
        }
        let name = match clean(name, NAME_CHARS) {
            name if name.is_empty() => self.fresh_name(),
            name => name,
        };
        let id = self.unique_id(&name, None);
        self.pages.push(Page {
            id,
            name,
            choices: Vec::new(),
            force: false,
        });
        self.save();
        Some(self.pages.len() - 1)
    }

    /// The name the next page added gets unless it is given one.
    pub(crate) fn fresh_name(&self) -> String {
        (self.pages.len() + 1..)
            .map(|number| format!("Page {number}"))
            .find(|name| self.pages.iter().all(|page| page.name != *name))
            .unwrap_or_default()
    }

    /// Rename page `page` (a blank name keeps the old one). A page named for the
    /// first time (`first`, just added) takes its id from the name too.
    pub(crate) fn rename_page(&mut self, page: usize, name: &str, first: bool) {
        let name = clean(name, NAME_CHARS);
        if name.is_empty() || page >= self.pages.len() {
            return;
        }
        if first {
            self.pages[page].id = self.unique_id(&name, Some(page));
        }
        self.pages[page].name = name;
        self.save();
    }

    /// Remove page `page`, unless it is the last one left.
    pub(crate) fn remove_page(&mut self, page: usize) -> bool {
        if self.pages.len() <= 1 || page >= self.pages.len() {
            return false;
        }
        self.pages.remove(page);
        self.save();
        true
    }

    /// Move page `page` one place up (`direction` -1) or down (1); its new index.
    pub(crate) fn move_page(&mut self, page: usize, direction: i32) -> usize {
        let Some(to) = step(page, direction, self.pages.len()) else {
            return page;
        };
        self.pages.swap(page, to);
        self.save();
        to
    }

    /// Put `slot` on page `page`: in place of choice `at`, or after the last
    /// choice when `at` is `None` (unless the page is full). The choice's index.
    pub(crate) fn set_choice(
        &mut self,
        page: usize,
        at: Option<usize>,
        slot: Slot,
    ) -> Option<usize> {
        let slot = match slot {
            Slot::Custom { label, command } => {
                let (label, command) = (clean(&label, LABEL_CHARS), clean(&command, COMMAND_CHARS));
                if label.is_empty() || command.is_empty() {
                    return None;
                }
                Slot::Custom { label, command }
            }
            Slot::Action(index) if index < ACTIONS.len() => Slot::Action(index),
            Slot::Action(_) => return None,
        };
        // The Force page's choices are the player's powers, never set by hand.
        let page = self.pages.get_mut(page).filter(|page| !page.force)?;
        let choices = &mut page.choices;
        let index = match at {
            Some(at) if at < choices.len() => {
                choices[at] = slot;
                at
            }
            _ if choices.len() < MAX_CHOICES => {
                choices.push(slot);
                choices.len() - 1
            }
            _ => return None,
        };
        self.save();
        Some(index)
    }

    /// Remove choice `at` of page `page`.
    pub(crate) fn remove_choice(&mut self, page: usize, at: usize) {
        let Some(choices) = self.pages.get_mut(page).map(|page| &mut page.choices) else {
            return;
        };
        if at < choices.len() {
            choices.remove(at);
            self.save();
        }
    }

    /// Move choice `at` of page `page` one place earlier (-1) or later (1) round
    /// the ring; its new index.
    pub(crate) fn move_choice(&mut self, page: usize, at: usize, direction: i32) -> usize {
        let Some(choices) = self.pages.get_mut(page).map(|page| &mut page.choices) else {
            return at;
        };
        let Some(to) = step(at, direction, choices.len()) else {
            return at;
        };
        choices.swap(at, to);
        self.save();
        to
    }

    /// Back to the default pages: the file is removed, so later versions'
    /// defaults reach this profile too.
    pub(crate) fn restore_defaults(&mut self) {
        self.pages = defaults();
        if let Some(path) = &self.path
            && path.exists()
            && let Err(error) = std::fs::remove_file(path)
        {
            crate::log::progress(format_args!(
                "warning: {} not removed: {error}",
                path.display()
            ));
            self.save();
        }
    }

    /// `name` as an id no other page than `except` has: its letters and digits in
    /// lower case, starting with a letter, numbered when taken.
    fn unique_id(&self, name: &str, except: Option<usize>) -> String {
        let mut base = squeeze(name);
        if !base.starts_with(|c: char| c.is_ascii_lowercase()) {
            base.insert_str(0, "page");
        }
        let taken = |id: &str| {
            self.pages
                .iter()
                .enumerate()
                .any(|(index, page)| Some(index) != except && page.id == id)
        };
        if !taken(&base) {
            return base;
        }
        (2..)
            .map(|number| format!("{base}{number}"))
            .find(|id| !taken(id))
            .unwrap_or(base)
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Err(error) = write(path, &self.pages) {
            crate::log::progress(format_args!(
                "warning: quick wheel pages not saved to {}: {error}",
                path.display()
            ));
        }
    }
}

/// `text`'s ASCII letters and digits in lower case: an id, and how `+wheel`
/// compares a name.
fn squeeze(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// `text` trimmed, without control characters, at most `chars` long.
pub(crate) fn clean(text: &str, chars: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(chars)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The index `direction` places from `index` among `count`, if there is one.
fn step(index: usize, direction: i32, count: usize) -> Option<usize> {
    let to = index as i64 + i64::from(direction.signum());
    (index < count && (0..count as i64).contains(&to) && to != index as i64).then_some(to as usize)
}

/// The pages a `wheel.json` holds and its version (1 when it names none);
/// `None` when it is not one.
fn parse(bytes: &[u8]) -> Option<(Vec<Page>, u64)> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let items = value.get("pages")?.as_array()?;
    let version = value.get("version").and_then(Value::as_u64).unwrap_or(1);
    let mut pages = WheelPages {
        path: None,
        pages: Vec::new(),
    };
    for item in items.iter().take(MAX_PAGES) {
        let Some(name) = item
            .get("name")
            .and_then(Value::as_str)
            .map(|name| clean(name, NAME_CHARS))
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        let force = item.get("kind").and_then(Value::as_str) == Some("force");
        // One Force page: a second would show the same powers.
        if force && pages.has_force_page() {
            continue;
        }
        let choices = item
            .get("choices")
            .and_then(Value::as_array)
            .filter(|_| !force)
            .map(|choices| choices.iter().filter_map(slot).take(MAX_CHOICES).collect())
            .unwrap_or_default();
        let wanted = item.get("id").and_then(Value::as_str).unwrap_or(&name);
        let id = pages.unique_id(wanted, None);
        pages.pages.push(Page {
            id,
            name,
            choices,
            force,
        });
    }
    (!pages.pages.is_empty()).then_some((pages.pages, version))
}

/// A choice as `wheel.json` writes it; `None` for an unknown action or an
/// incomplete custom one.
fn slot(value: &Value) -> Option<Slot> {
    if let Some(id) = value.as_str() {
        return catalog::action_index(id).map(Slot::Action);
    }
    let text = |key: &str, chars: usize| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(|text| clean(text, chars))
            .filter(|text| !text.is_empty())
    };
    Some(Slot::Custom {
        label: text("name", LABEL_CHARS)?,
        command: text("command", COMMAND_CHARS)?,
    })
}

fn write(path: &Path, pages: &[Page]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pages: Vec<Value> = pages
        .iter()
        .map(|page| {
            let choices: Vec<Value> = page
                .choices
                .iter()
                .map(|choice| match choice {
                    Slot::Action(index) => json!(ACTIONS[*index].id),
                    Slot::Custom { label, command } => json!({"name": label, "command": command}),
                })
                .collect();
            if page.force {
                json!({"id": page.id, "name": page.name, "kind": "force"})
            } else {
                json!({"id": page.id, "name": page.name, "choices": choices})
            }
        })
        .collect();
    let bytes = serde_json::to_vec_pretty(&json!({"version": VERSION, "pages": pages}))
        .map_err(std::io::Error::other)?;
    // Written whole beside the file, then moved over it, so a crash never
    // leaves half a file.
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(label: &str, command: &str) -> Slot {
        Slot::Custom {
            label: label.to_owned(),
            command: command.to_owned(),
        }
    }

    /// The ids of `pages`, in order.
    fn ids(pages: &WheelPages) -> Vec<&str> {
        pages.pages().iter().map(|page| page.id.as_str()).collect()
    }

    #[test]
    fn without_a_file_the_wheel_has_force_general_toys_and_weather() {
        let directory = tempfile::tempdir().unwrap();
        let pages = WheelPages::load(directory.path());
        assert!(pages.is_default());
        let names: Vec<_> = pages
            .pages()
            .iter()
            .map(|page| page.name.as_str())
            .collect();
        assert_eq!(names, ["Force", "General", "Toys", "Weather"]);
        assert!(pages.pages()[0].force && pages.pages()[0].choices.is_empty());
        assert_eq!(pages.pages()[1].choices.len(), 8);
        assert_eq!(pages.pages()[3].choices[2].command(), "r_weatherForce 2");
        assert_eq!(pages.pages()[1].choices[0].label(), "Third person");
        // Toys is an ordinary page of the catalogue's Toys: Illuminate, which
        // toggles the holocron and is marked while it is lit.
        let toys = &pages.pages()[2];
        assert!(!toys.force);
        assert_eq!(toys.choices.len(), 1);
        assert_eq!(toys.choices[0].label(), "Illuminate");
        assert_eq!(toys.choices[0].command(), "toy_illuminate");
        assert_eq!(toys.choices[0].state(), State::Illuminate);
        assert!(toys.choices[0].icon().is_some());
        // Nothing is written until something changes.
        assert!(!directory.path().join(FILE).exists());
        // Binds name a page by id or name, whatever the case and spacing.
        assert_eq!(pages.find("force"), Some(0));
        assert_eq!(pages.find(" General "), Some(1));
        assert_eq!(pages.find("toys"), Some(2));
        assert_eq!(pages.find("weather"), Some(3));
        assert_eq!(pages.find("hail"), None);
        assert_eq!(pages.find(""), None);
    }

    #[test]
    fn edits_are_saved_and_read_back_as_made() {
        let directory = tempfile::tempdir().unwrap();
        let mut pages = WheelPages::load(directory.path());
        let duels = pages.add_page("  Duels ").unwrap();
        assert_eq!(duels, 4);
        assert_eq!(pages.pages()[duels].id, "duels");
        let duel = catalog::action_index("duel").unwrap();
        assert_eq!(pages.set_choice(duels, None, Slot::Action(duel)), Some(0));
        assert_eq!(
            pages.set_choice(duels, None, custom("Ready", "say \"ready\"; ready")),
            Some(1)
        );
        // A rename keeps the id a bind opens it with.
        pages.rename_page(duels, "1v1 duels", false);
        assert_eq!(pages.pages()[duels].id, "duels");
        assert_eq!(pages.find("duels"), Some(duels));
        assert_eq!(pages.find("1v1 Duels"), Some(duels));
        pages.move_page(duels, -1);
        assert_eq!(pages.pages()[3].name, "1v1 duels");
        pages.move_choice(3, 1, -1);
        assert_eq!(pages.pages()[3].choices[0].label(), "Ready");
        let saved = WheelPages::load(directory.path());
        assert_eq!(saved.pages(), pages.pages());
        assert!(!saved.is_default());
        // The Toys page is an ordinary page: a toy deleted from it comes back
        // from the catalogue.
        let toys = saved.find("toys").unwrap();
        let illuminate = catalog::action_index("illuminate").unwrap();
        pages.remove_choice(toys, 0);
        assert!(pages.pages()[toys].choices.is_empty());
        assert_eq!(
            pages.set_choice(toys, None, Slot::Action(illuminate)),
            Some(0)
        );
        assert_eq!(pages.pages()[toys].choices[0].label(), "Illuminate");
        // The Force page is kept as a page of its own kind, without choices.
        assert!(
            saved
                .pages()
                .iter()
                .any(|page| page.force && page.id == "force")
        );
        let force = saved.pages().iter().position(|page| page.force).unwrap();
        assert_eq!(pages.set_choice(force, None, Slot::Action(duel)), None);
        // Back to the defaults, in the new order: the file goes.
        pages.restore_defaults();
        assert!(pages.is_default());
        assert_eq!(ids(&pages), ["force", "general", "toys", "weather"]);
        assert!(!directory.path().join(FILE).exists());
        assert!(WheelPages::load(directory.path()).is_default());
    }

    #[test]
    fn limits_hold_and_the_last_page_stays() {
        let mut pages = WheelPages::unsaved();
        // Full pages take no more; a full wheel no more pages, nor the
        // Force page back.
        // The Force page takes none by hand; General is the page after it.
        assert_eq!(pages.set_choice(0, None, Slot::Action(0)), None);
        assert_eq!(pages.set_choice(1, None, Slot::Action(0)), Some(8));
        assert_eq!(pages.set_choice(1, None, Slot::Action(0)), Some(9));
        assert_eq!(pages.set_choice(1, None, Slot::Action(0)), None);
        // In place of a choice still works on a full page.
        assert_eq!(pages.set_choice(1, Some(9), Slot::Action(2)), Some(9));
        assert!(pages.remove_page(0));
        while pages.add_page("").is_some() {}
        assert_eq!(pages.pages().len(), MAX_PAGES);
        assert_eq!(pages.add_force_page(), None);
        assert_eq!(pages.pages()[3].name, "Page 4");
        assert_eq!(pages.pages()[3].id, "page4");
        // Blank or unfinished custom choices are refused, long ones cut.
        assert_eq!(pages.set_choice(3, None, custom(" ", "kill")), None);
        assert_eq!(pages.set_choice(3, None, custom("Kill", "")), None);
        let long = "x".repeat(40);
        let at = pages.set_choice(3, None, custom(&long, "kill")).unwrap();
        assert_eq!(pages.pages()[3].choices[at].label().len(), LABEL_CHARS);
        while pages.remove_page(0) {}
        assert_eq!(pages.pages().len(), 1);
        // Moves past the ends stay put.
        assert_eq!(pages.move_page(0, -1), 0);
        assert_eq!(pages.move_choice(0, 0, -1), 0);
    }

    #[test]
    fn a_first_name_gives_the_page_its_id_and_ids_stay_unique() {
        let mut pages = WheelPages::unsaved();
        let page = pages.add_page("").unwrap();
        pages.rename_page(page, "Weather", true);
        assert_eq!(pages.pages()[page].id, "weather2");
        let other = pages.add_page("42").unwrap();
        assert_eq!(pages.pages()[other].id, "page42");
        // A blank rename keeps the name.
        pages.rename_page(other, "   ", false);
        assert_eq!(pages.pages()[other].name, "42");
    }

    #[test]
    fn a_hand_written_or_broken_file_reads_what_it_can() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        std::fs::write(
            &path,
            br#"{"pages": [
                {"name": "Mine", "choices": ["rain", "hail", {"name": "Hi", "command": "say hi"},
                    {"name": "No command"}, 7]},
                {"choices": ["rain"]},
                {"id": "Mine", "name": "Copy", "choices": []}
            ]}"#,
        )
        .unwrap();
        let pages = WheelPages::load(directory.path());
        // The two pages read, and the Force and Toys pages a file without a
        // version gets, in that order, after the pages it has.
        assert_eq!(pages.pages().len(), 4);
        assert!(pages.pages()[2].force);
        assert_eq!(pages.pages()[3].id, "toys");
        let mine = &pages.pages()[0];
        assert_eq!((mine.id.as_str(), mine.name.as_str()), ("mine", "Mine"));
        assert_eq!(mine.choices.len(), 2);
        assert_eq!(mine.choices[1], custom("Hi", "say hi"));
        assert_eq!(pages.pages()[1].id, "mine2");
        std::fs::write(&path, b"not json").unwrap();
        assert!(WheelPages::load(directory.path()).is_default());
        std::fs::write(&path, br#"{"pages": []}"#).unwrap();
        assert!(WheelPages::load(directory.path()).is_default());
    }

    #[test]
    fn a_file_from_before_the_force_page_gets_it_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        // A version 1 file with its own pages: Force goes in after General.
        std::fs::write(
            &path,
            br#"{"version": 1, "pages": [
                {"id": "duels", "name": "Duels", "choices": ["duel"]},
                {"id": "general", "name": "Mine", "choices": ["hud"]},
                {"id": "weather", "name": "Weather", "choices": ["rain"]}
            ]}"#,
        )
        .unwrap();
        let mut pages = WheelPages::load(directory.path());
        // Force after General, then the Toys page after the Force page.
        assert_eq!(
            ids(&pages),
            ["duels", "general", "force", "toys", "weather"]
        );
        assert_eq!(pages.pages()[1].choices.len(), 1, "the player's pages stay");
        // Saved as the current version: removed, it stays removed.
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("\"version\": 3")
        );
        assert!(pages.remove_page(2));
        assert!(!WheelPages::load(directory.path()).has_force_page());
        // Settings can put it back.
        assert_eq!(pages.add_force_page(), Some(2));
        assert_eq!(pages.add_force_page(), None);
        // Without General it goes last; a page called force keeps its id.
        std::fs::write(
            &path,
            br#"{"pages": [{"id": "force", "name": "Pushes", "choices": ["afk"]}]}"#,
        )
        .unwrap();
        let pages = WheelPages::load(directory.path());
        assert_eq!(ids(&pages), ["force", "force2", "toys"]);
        assert!(pages.pages()[1].force);
        // A full version 1 wheel is left as it is; a second Force page is dropped.
        let full: Vec<String> = (0..MAX_PAGES)
            .map(|index| format!(r#"{{"name": "P{index}", "choices": []}}"#))
            .collect();
        std::fs::write(&path, format!(r#"{{"pages": [{}]}}"#, full.join(","))).unwrap();
        let pages = WheelPages::load(directory.path());
        assert!(!pages.has_force_page() && pages.find("toys").is_none());
        assert_eq!(pages.pages().len(), MAX_PAGES);
        std::fs::write(
            &path,
            br#"{"version": 2, "pages": [
                {"name": "Force", "kind": "force", "choices": ["rain"]},
                {"name": "Again", "kind": "force"}
            ]}"#,
        )
        .unwrap();
        let pages = WheelPages::load(directory.path());
        assert_eq!(ids(&pages), ["force", "toys"]);
        assert!(pages.pages()[0].force && pages.pages()[0].choices.is_empty());
    }

    /// A `wheel.json` of `version` with the pages version 2 had as its
    /// defaults, as the client wrote them: General, Force and Weather.
    fn old_defaults(version: u64) -> String {
        let list = |ids: &[&str]| format!("{ids:?}");
        format!(
            r#"{{"version": {version}, "pages": [
                {{"id": "general", "name": "General", "choices": {}}},
                {{"id": "force", "name": "Force", "kind": "force"}},
                {{"id": "weather", "name": "Weather", "choices": {}}}
            ]}}"#,
            list(&catalog::GENERAL),
            list(&catalog::WEATHER),
        )
    }

    #[test]
    fn the_previous_untouched_defaults_become_the_new_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        for version in [1, 2] {
            // Version 1 has no Force page of its own: the migrations add it,
            // and the result is the old defaults all the same.
            let old = old_defaults(version);
            let old = if version == 1 {
                old.replace(r#"{"id": "force", "name": "Force", "kind": "force"},"#, "")
            } else {
                old
            };
            std::fs::write(&path, old).unwrap();
            let pages = WheelPages::load(directory.path());
            assert!(pages.is_default(), "version {version}");
            assert_eq!(ids(&pages), ["force", "general", "toys", "weather"]);
            // The file is gone, so later defaults reach this profile too.
            assert!(!path.exists(), "version {version}");
        }
        // A file with no version at all is version 1.
        std::fs::write(&path, old_defaults(1).replace("\"version\": 1,", "")).unwrap();
        assert!(WheelPages::load(directory.path()).is_default());
    }

    #[test]
    fn any_other_old_wheel_gains_the_toys_page_once_after_its_force_page() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        let fixed = |expected: &[&str], json: String| {
            std::fs::write(&path, json).unwrap();
            let pages = WheelPages::load(directory.path());
            assert_eq!(ids(&pages), expected);
            assert!(!pages.is_default());
            // Rewritten as the current version, so it is not offered again.
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.contains("\"version\": 3"), "{text}");
            let toys = pages.find("toys").unwrap();
            assert_eq!(pages.pages()[toys].choices.len(), 1);
            pages
        };
        // A renamed page.
        let renamed = old_defaults(2).replace("\"name\": \"General\"", "\"name\": \"Main\"");
        let pages = fixed(&["general", "force", "toys", "weather"], renamed);
        assert_eq!(pages.pages()[0].name, "Main");
        // A custom choice on Weather.
        let custom = old_defaults(2).replace(
            r#""weather_toggle"]"#,
            r#""weather_toggle", {"name": "Hi", "command": "say hi"}]"#,
        );
        let pages = fixed(&["general", "force", "toys", "weather"], custom);
        assert_eq!(pages.pages()[3].choices.len(), 9);
        // A choice taken off General.
        let fewer = old_defaults(2).replace(r#""third_person", "#, "");
        fixed(&["general", "force", "toys", "weather"], fewer);
        // The same pages in another order: Toys still follows the Force page.
        let reordered = r#"{"version": 2, "pages": [
            {"id": "weather", "name": "Weather", "choices": ["rain"]},
            {"id": "force", "name": "Force", "kind": "force"},
            {"id": "general", "name": "General", "choices": ["hud"]}
        ]}"#;
        fixed(&["weather", "force", "toys", "general"], reordered.into());
        // A removed Force page stays removed; Toys goes last.
        let no_force = r#"{"version": 2, "pages": [
            {"id": "general", "name": "General", "choices": ["hud"]},
            {"id": "weather", "name": "Weather", "choices": ["rain"]}
        ]}"#;
        let pages = fixed(&["general", "weather", "toys"], no_force.into());
        assert!(!pages.has_force_page());
        // A page of the player's own already called Toys keeps its id.
        let own = r#"{"version": 2, "pages": [
            {"id": "toys", "name": "Toys", "choices": ["afk"]},
            {"id": "force", "name": "Force", "kind": "force"}
        ]}"#;
        let pages = fixed(&["toys", "force", "toys2"], own.into());
        assert_eq!(pages.pages()[0].choices.len(), 1);
        assert_eq!(pages.pages()[0].choices[0].label(), "AFK");
    }

    #[test]
    fn a_removed_toys_page_stays_removed_and_a_full_wheel_is_left_alone() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        let custom = old_defaults(2).replace("\"name\": \"General\"", "\"name\": \"Main\"");
        std::fs::write(&path, &custom).unwrap();
        let mut pages = WheelPages::load(directory.path());
        let toys = pages.find("toys").unwrap();
        assert!(pages.remove_page(toys));
        let mut again = WheelPages::load(directory.path());
        assert!(again.find("toys").is_none());
        assert_eq!(again.pages(), pages.pages());
        // A page called Toys added by hand is just a page; the catalogue
        // brings Illuminate back to it.
        let toys = again.add_page("Toys").unwrap();
        assert_eq!(again.pages()[toys].id, "toys");
        let illuminate = catalog::action_index("illuminate").unwrap();
        assert_eq!(
            again.set_choice(toys, None, Slot::Action(illuminate)),
            Some(0)
        );
        // Eight pages already: no Toys page, and the file is not rewritten.
        let full: Vec<String> = (0..MAX_PAGES)
            .map(|index| format!(r#"{{"name": "P{index}", "choices": []}}"#))
            .collect();
        let full = format!(r#"{{"version": 2, "pages": [{}]}}"#, full.join(","));
        std::fs::write(&path, &full).unwrap();
        let pages = WheelPages::load(directory.path());
        assert_eq!(pages.pages().len(), MAX_PAGES);
        assert!(pages.find("toys").is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), full);
    }
}

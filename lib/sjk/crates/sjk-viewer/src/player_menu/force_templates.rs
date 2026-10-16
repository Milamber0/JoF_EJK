//! Force templates, retail's `forcecfg` (`UI_LoadForceConfig_List`,
//! `UI_ForceConfigHandle`, `UI_SaveForceTemplate`): ready-made Force
//! profiles in `forcecfg/light/*.fcf` and `forcecfg/dark/*.fcf`, each file a
//! `forcepowers` string. The game data ships some (retail's Blademaster,
//! Healer, Knight and so on; community packs add theirs); the player saves
//! their own into the client's user folder, `forcecfg/<side>/<name>.fcf`
//! beside `config.cfg`, where retail wrote into its home path. Loading one
//! legalizes it under the server's rules into the Force draft, as retail did.

use sjk_client::ForceSide;
use sjk_vfs::VirtualFileSystem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Longest template name the field takes, in bytes (retail's `maxchars 12`,
/// with room for colour codes).
pub(super) const MAX_NAME: usize = 24;
/// Most templates listed per side (retail's `MAX_FORCE_CONFIGS` is 128).
const MAX_TEMPLATES: usize = 128;
/// Longest file read (`fcfBuffer`).
const MAX_FILE: usize = 8_192;

/// One template.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Template {
    /// The file name without `.fcf`, colour codes kept.
    pub(super) name: String,
    /// The `forcepowers` string it holds.
    pub(super) value: String,
    /// Saved by the player (in the user folder) rather than shipped.
    pub(super) own: bool,
}

/// The templates of both sides, listed once per file system and again
/// after a save.
#[derive(Default)]
pub(super) struct Templates {
    listed: Option<Arc<VirtualFileSystem>>,
    sides: [Vec<Template>; 2],
}

fn folder(side: ForceSide) -> &'static str {
    match side {
        ForceSide::Light => "forcecfg/light",
        ForceSide::Dark => "forcecfg/dark",
    }
}

fn slot(side: ForceSide) -> usize {
    match side {
        ForceSide::Light => 0,
        ForceSide::Dark => 1,
    }
}

impl Templates {
    /// List the templates of `vfs` and the user folder unless `vfs` is the
    /// file system listed last.
    pub(super) fn ensure(&mut self, vfs: Option<&Arc<VirtualFileSystem>>) {
        let Some(vfs) = vfs else {
            return;
        };
        if self
            .listed
            .as_ref()
            .is_some_and(|listed| Arc::ptr_eq(listed, vfs))
        {
            return;
        }
        self.rescan(vfs);
    }

    /// List both sides again.
    fn rescan(&mut self, vfs: &Arc<VirtualFileSystem>) {
        let user = user_root();
        for side in [ForceSide::Light, ForceSide::Dark] {
            self.sides[slot(side)] = list(vfs, user.as_deref(), side);
        }
        self.listed = Some(Arc::clone(vfs));
    }

    /// Replace `side`'s list with `count` of the player's own templates
    /// (tests: the most the page has to list).
    #[cfg(test)]
    pub(super) fn fill(&mut self, side: ForceSide, count: usize) {
        self.sides[slot(side)] = (0..count)
            .map(|index| Template {
                name: format!("My saved template {index:02}"),
                value: "7-2-031330310000030333".to_owned(),
                own: true,
            })
            .collect();
    }

    /// `side`'s templates: the player's own first, then the game data's,
    /// each by name.
    pub(super) fn of(&self, side: ForceSide) -> &[Template] {
        &self.sides[slot(side)]
    }

    /// Save `value` as `name` for `side` in the user folder and list again;
    /// returns the saved template's row.
    pub(super) fn save(
        &mut self,
        side: ForceSide,
        name: &str,
        value: &str,
    ) -> Result<usize, String> {
        let name = name.trim();
        if !valid_name(name) {
            return Err("Name the template first (letters, digits, spaces).".to_owned());
        }
        let root = user_root().ok_or("No user folder to save into.")?;
        let path = save_to(&root, side, name, value)?;
        crate::log::progress(format_args!("Force template saved: {}", path.display()));
        if let Some(vfs) = self.listed.clone() {
            self.rescan(&vfs);
        }
        self.of(side)
            .iter()
            .position(|template| template.own && template.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| "The template was saved but could not be listed.".to_owned())
    }
}

/// The Force page's template column: the list, its view, the name being
/// typed and how the last save went.
#[derive(Default)]
pub(super) struct TemplateState {
    pub(super) list: Templates,
    /// First visible row.
    pub(super) scroll: usize,
    /// The name the draft would be saved as.
    pub(super) name: String,
    /// The name field takes typing.
    pub(super) editing: bool,
    /// The name before editing began, for Escape.
    pub(super) name_before: String,
    /// The last save's result: the text, and whether it worked.
    pub(super) note: Option<(String, bool)>,
}

/// The client's user folder (where `config.cfg` lives).
fn user_root() -> Option<PathBuf> {
    crate::platform::user_config_file()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

/// Write `forcecfg/<side>/<name>.fcf` under `root` as retail writes it
/// (the string and a newline).
fn save_to(root: &Path, side: ForceSide, name: &str, value: &str) -> Result<PathBuf, String> {
    let directory = root.join(folder(side));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not make {}: {error}", directory.display()))?;
    let path = directory.join(format!("{name}.fcf"));
    std::fs::write(&path, format!("{value}\n"))
        .map_err(|error| format!("Could not write the template (read-only?): {error}"))?;
    Ok(path)
}

/// `side`'s templates in `vfs` and under `user`.
fn list(vfs: &VirtualFileSystem, user: Option<&Path>, side: ForceSide) -> Vec<Template> {
    let mut templates = Vec::new();
    if let Some(user) = user {
        let directory = user.join(folder(side));
        if let Ok(entries) = std::fs::read_dir(&directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file = path.file_name().and_then(|name| name.to_str());
                let Some(name) = file.and_then(fcf_name) else {
                    continue;
                };
                if let Some(value) = std::fs::read(&path).ok().and_then(|bytes| contents(&bytes)) {
                    push(&mut templates, name, value, true);
                }
            }
        }
    }
    for file in vfs.list_files(folder(side), ".fcf") {
        let path = format!("{}/{file}", folder(side));
        // Listed names are lowercase; show the file's own ("^2Side Mission").
        let original = vfs.original_name(&path);
        let shown = original
            .as_deref()
            .and_then(|original| original.rsplit('/').next())
            .unwrap_or(&file);
        let Some(name) = fcf_name(shown) else {
            continue;
        };
        if let Some(value) = vfs
            .read(&path)
            .ok()
            .flatten()
            .and_then(|asset| contents(&asset.bytes))
        {
            push(&mut templates, name, value, false);
        }
    }
    templates.sort_by_cached_key(|template| (!template.own, sort_key(&template.name)));
    templates.truncate(MAX_TEMPLATES);
    templates
}

/// Add a template unless one of the same name is listed (the player's own
/// win over the game data's, game data's earlier mounts over later ones).
fn push(templates: &mut Vec<Template>, name: &str, value: String, own: bool) {
    if templates
        .iter()
        .any(|template| template.name.eq_ignore_ascii_case(name))
    {
        return;
    }
    templates.push(Template {
        name: name.to_owned(),
        value,
        own,
    });
}

/// The name of a `.fcf` file directly in its folder.
fn fcf_name(file: &str) -> Option<&str> {
    if file.contains('/') || file.len() <= ".fcf".len() {
        return None;
    }
    let (name, extension) = file.split_at(file.len() - ".fcf".len());
    extension.eq_ignore_ascii_case(".fcf").then_some(name)
}

/// The `forcepowers` string of a template file: its first line, trimmed.
fn contents(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= MAX_FILE {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_owned())
}

/// Sorting key: the name without colour codes, in lowercase.
fn sort_key(name: &str) -> String {
    let mut key = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c == '^' {
            chars.next();
        } else {
            key.push(c.to_ascii_lowercase());
        }
    }
    key
}

/// Whether `name` can be a file name: 1 to [`MAX_NAME`] bytes of letters,
/// digits, spaces, `_`, `-` and `^` colour codes, with something printable.
pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.ends_with([' ', '.'])
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b" _-^".contains(&byte))
        && !sort_key(name).trim().is_empty()
        && !reserved(name)
}

/// Windows device names, which cannot be files (`con`, `nul`, `com1` ...).
fn reserved(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    matches!(name.as_str(), "con" | "prn" | "aux" | "nul")
        || (name.len() == 4
            && (name.starts_with("com") || name.starts_with("lpt"))
            && name.as_bytes()[3].is_ascii_digit())
}

/// Keep `text`'s characters a template name may hold, within [`MAX_NAME`].
pub(super) fn append_name(name: &mut String, text: &str) {
    for character in text.chars() {
        if name.len() + character.len_utf8() > MAX_NAME {
            break;
        }
        if character.is_ascii_alphanumeric() || " _-^".contains(character) {
            name.push(character);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vfs() -> VirtualFileSystem {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "assets1",
            [
                (
                    "forcecfg/light/Knight.fcf",
                    b"7-1-331322000200003322\n".to_vec(),
                ),
                (
                    "forcecfg/light/^2Side Mission.fcf",
                    b"7-1-330000000333003330".to_vec(),
                ),
                (
                    "forcecfg/dark/Destroyer.fcf",
                    b"7-2-012320333000030321\n".to_vec(),
                ),
                ("forcecfg/dark/empty.fcf", b"\n".to_vec()),
                ("forcecfg/template.png", b"x".to_vec()),
            ],
        )
        .unwrap();
        vfs
    }

    #[test]
    fn templates_list_by_side_with_the_players_own_first() {
        let directory = tempfile::tempdir().unwrap();
        let vfs = vfs();
        save_to(
            directory.path(),
            ForceSide::Light,
            "Mine",
            "7-1-000000000000000000",
        )
        .unwrap();
        // The player's own Knight replaces the shipped one.
        save_to(
            directory.path(),
            ForceSide::Light,
            "knight",
            "7-1-333333000000000000",
        )
        .unwrap();
        let light = list(&vfs, Some(directory.path()), ForceSide::Light);
        let names: Vec<&str> = light
            .iter()
            .map(|template| template.name.as_str())
            .collect();
        assert_eq!(names, ["knight", "Mine", "^2Side Mission"]);
        assert!(light[0].own && light[1].own && !light[2].own);
        assert_eq!(light[0].value, "7-1-333333000000000000");
        assert_eq!(light[2].value, "7-1-330000000333003330");
        let dark = list(&vfs, None, ForceSide::Dark);
        assert_eq!(dark.len(), 1);
        assert_eq!(dark[0].value, "7-2-012320333000030321");
        assert_eq!(
            std::fs::read_to_string(directory.path().join("forcecfg/light/Mine.fcf")).unwrap(),
            "7-1-000000000000000000\n"
        );
    }

    #[test]
    fn names_stay_file_names() {
        assert!(valid_name("Knight"));
        assert!(valid_name("^2My Build 2"));
        assert!(!valid_name(""));
        assert!(!valid_name("^2"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name("dots."));
        assert!(!valid_name("con:"));
        assert!(!valid_name("CON"));
        assert!(!valid_name("lpt1"));
        assert!(valid_name("console"));
        assert!(!valid_name(&"x".repeat(MAX_NAME + 1)));
        let mut name = String::new();
        append_name(&mut name, "My*Build/?");
        assert_eq!(name, "MyBuild");
        assert_eq!(fcf_name("Knight.FCF"), Some("Knight"));
        assert_eq!(fcf_name("sub/Knight.fcf"), None);
        assert_eq!(fcf_name("Knight.txt"), None);
    }
}

impl super::PlayerMenu {
    /// The list row of the template the draft holds, if it holds one.
    pub(super) fn template_row(&self) -> Option<usize> {
        let name = self.force.template()?;
        let side = self.force.allocation().side;
        self.force_templates
            .list
            .of(side)
            .iter()
            .position(|template| template.name == name)
    }

    /// Load the side's template at `row` into the draft.
    pub(super) fn load_template_row(&mut self, row: usize) {
        let side = self.force.allocation().side;
        let Some(template) = self.force_templates.list.of(side).get(row).cloned() else {
            return;
        };
        self.force.load_template(&template.name, &template.value);
    }

    /// Left and Right on the list: load the previous or next template.
    pub(super) fn step_template(&mut self, direction: isize) {
        let count = self
            .force_templates
            .list
            .of(self.force.allocation().side)
            .len();
        if count == 0 {
            return;
        }
        let row = match self.template_row() {
            Some(row) => super::controller::wrap(row, direction, count),
            None if direction < 0 => count - 1,
            None => 0,
        };
        self.load_template_row(row);
    }

    /// Save the draft as the typed name, for its side.
    pub(super) fn save_template(&mut self) {
        let side = self.force.allocation().side;
        let name = self.force_templates.name.trim().to_owned();
        let value = self.force.allocation().encode();
        let state = &mut self.force_templates;
        state.editing = false;
        state.note = Some(match state.list.save(side, &name, &value) {
            Ok(_) => {
                self.force.load_template(&name, &value);
                (format!("Saved as {name}^7."), true)
            }
            Err(reason) => (reason, false),
        });
    }

    /// Typing into the name field: Enter ends, Escape restores.
    pub(super) fn edit_template_name(
        &mut self,
        event: &winit::event::KeyEvent,
        key: winit::keyboard::KeyCode,
    ) -> super::PlayerMenuResult {
        use winit::keyboard::KeyCode;
        let state = &mut self.force_templates;
        match key {
            KeyCode::Escape => {
                state.name.clone_from(&state.name_before);
                state.editing = false;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => state.editing = false,
            KeyCode::Backspace => {
                state.name.pop();
            }
            _ => {
                if let Some(text) = event.text.as_deref() {
                    append_name(&mut state.name, text);
                }
            }
        }
        super::PlayerMenuResult::None
    }

    /// Start typing a template name.
    pub(super) fn begin_template_name(&mut self) {
        let state = &mut self.force_templates;
        state.name_before.clone_from(&state.name);
        state.editing = true;
        state.note = None;
    }
}
